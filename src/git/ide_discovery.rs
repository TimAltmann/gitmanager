//! Visual Studio, Rider und beliebige Programme auf dem System finden
//! (mit TTL-Cache für die teure Suche).

use crate::process::{set_creation_flags, CREATE_NO_WINDOW};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const VS_RIDER_CACHE_TTL: Duration = Duration::from_secs(300);

type CachedPath = Mutex<Option<(Option<PathBuf>, Instant)>>;

#[allow(clippy::type_complexity)]
static VS_PATH_CACHE: OnceLock<CachedPath> = OnceLock::new();

#[allow(clippy::type_complexity)]
static RIDER_PATH_CACHE: OnceLock<CachedPath> = OnceLock::new();

fn vs_cache() -> &'static CachedPath {
    VS_PATH_CACHE.get_or_init(|| Mutex::new(None))
}

fn rider_cache() -> &'static CachedPath {
    RIDER_PATH_CACHE.get_or_init(|| Mutex::new(None))
}

fn clear_vs_cache() {
    if let Some(m) = VS_PATH_CACHE.get() {
        *m.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn clear_rider_cache() {
    if let Some(m) = RIDER_PATH_CACHE.get() {
        *m.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn cached_resolve<F>(cache: &'static CachedPath, compute: F) -> Option<PathBuf>
where
    F: FnOnce() -> Option<PathBuf>,
{
    // Fast path: check TTL without holding lock during compute
    if let Some((cached, instant)) = cache.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        if instant.elapsed() < VS_RIDER_CACHE_TTL {
            return cached;
        }
    }
    let result = compute();
    *cache.lock().unwrap_or_else(|e| e.into_inner()) = Some((result.clone(), Instant::now()));
    result
}

const VS_PROGRAMS: &[&str] = &["devenv", "devenv.exe"];

const RIDER_PROGRAMS: &[&str] = &["rider", "rider.exe", "rider64.exe", "rider.cmd"];

pub fn is_vs_program(program: &str) -> bool {
    VS_PROGRAMS.contains(&program.to_lowercase().as_str())
}

pub fn is_rider_program(program: &str) -> bool {
    RIDER_PROGRAMS.contains(&program.to_lowercase().as_str())
}

pub fn is_program_available(program: &str) -> bool {
    if program.is_empty() {
        return false;
    }
    // Direkter Pfad? Prüfe Existenz ohne PATH-Suche – Shell-Zeichen in Pfaden erlauben (z.B. "Tom & Jerry")
    if program.contains('/') || program.contains('\\') {
        return Path::new(program).exists();
    }
    // Blocke Shell-Metazeichen nur für bare Programmnamen (verhindert Injection über Config)
    if program.chars().any(|c| "&|;><`$\"'".contains(c)) {
        return false;
    }
    // Manuelle PATH-Suche (vermeidet Hijacking von `where`/`which` selbst)
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            #[cfg(windows)]
            {
                let candidates = [
                    dir.join(program),
                    dir.join(format!("{}.exe", program)),
                    dir.join(format!("{}.cmd", program)),
                    dir.join(format!("{}.bat", program)),
                ];
                if candidates.iter().any(|p| p.exists()) {
                    return true;
                }
            }
            #[cfg(not(windows))]
            {
                let candidate = dir.join(program);
                if candidate.exists() {
                    // Auf Unix: prüfe ob ausführbar (existenz reicht für Desktop-App)
                    return true;
                }
            }
        }
    }
    // Fallback: versuche `where` (Windows) bzw. `which` (Unix) mit absoluten Pfaden
    #[cfg(windows)]
    {
        let system_where = PathBuf::from(r"C:\Windows\System32\where.exe");
        let where_cmd = if system_where.exists() {
            system_where
        } else {
            PathBuf::from("where")
        };
        // Ohne Konsolenfenster: die Release-EXE hat keine Konsole, `where`
        // ließe sonst bei jedem Agent-/Shell-Start kurz eine aufblitzen
        let mut cmd = std::process::Command::new(where_cmd);
        cmd.arg(program);
        set_creation_flags(&mut cmd, CREATE_NO_WINDOW);
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let which_path = PathBuf::from("/usr/bin/which");
        let which_cmd = if which_path.exists() {
            which_path
        } else {
            PathBuf::from("which")
        };
        std::process::Command::new(which_cmd)
            .arg(program)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// `devenv.exe`-Pfade aus der vswhere-Ausgabe (eine Instanz je Zeile, in
/// Ausgabereihenfolge); SSMS und andere Produkte fallen weg. `install_dirs`:
/// Zeilen sind Installationsordner statt Programmpfade.
pub(super) fn devenv_candidates(output: &str, install_dirs: bool) -> Vec<PathBuf> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| {
            let lower = l.to_lowercase();
            !lower.contains("sql server") && !lower.contains("ssms")
        })
        .map(|l| {
            if install_dirs {
                PathBuf::from(l)
                    .join("Common7")
                    .join("IDE")
                    .join("devenv.exe")
            } else {
                PathBuf::from(l)
            }
        })
        // Windows-Pfad auch unter Linux zerlegen (Tests): letzter Teil
        .filter(|p| {
            let s = p.to_string_lossy();
            let name = s.rsplit(['\\', '/']).next().unwrap_or("");
            name.eq_ignore_ascii_case("devenv.exe")
        })
        .collect()
}

fn resolve_vs_path_uncached() -> Option<PathBuf> {
    let vswhere =
        PathBuf::from(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    if vswhere.exists() {
        // Alle Instanzen, neueste zuerst: `-latest` kann SQL Server
        // Management Studio liefern (nutzt denselben Installer), dann
        // fände man Visual Studio nie
        for property in ["productPath", "installationPath"] {
            let mut cmd = std::process::Command::new(&vswhere);
            cmd.args(["-products", "*", "-sort", "-format", "value", "-property"])
                .arg(property);
            set_creation_flags(&mut cmd, CREATE_NO_WINDOW);
            let out = cmd.output();
            let Ok(out) = out else { continue };
            if !out.status.success() {
                continue;
            }
            let text = String::from_utf8_lossy(&out.stdout);
            let found = devenv_candidates(&text, property == "installationPath")
                .into_iter()
                .find(|p| p.exists());
            if found.is_some() {
                return found;
            }
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let lower = dir.to_string_lossy().to_lowercase();
            if lower.contains("sql server") || lower.contains("ssms") {
                continue;
            }
            let candidate = dir.join("devenv.exe");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn resolve_vs_path() -> Option<PathBuf> {
    // TTL-Cache: vermeidet erneute vswhere Aufrufe innerhalb 5min, aber nicht für immer
    cached_resolve(vs_cache(), resolve_vs_path_uncached)
}

pub fn resolve_vs_path_force() -> Option<PathBuf> {
    clear_vs_cache();
    resolve_vs_path()
}

fn resolve_rider_path_uncached() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let toolbox_base = PathBuf::from(local_app_data)
            .join("JetBrains")
            .join("Toolbox")
            .join("apps")
            .join("Rider");
        if toolbox_base.exists() {
            if let Ok(entries) = std::fs::read_dir(&toolbox_base) {
                for ch in entries.flatten() {
                    if let Ok(sub) = std::fs::read_dir(ch.path()) {
                        for ver in sub.flatten() {
                            let rider_exe = ver.path().join("bin").join("rider64.exe");
                            if rider_exe.exists() {
                                candidates.push(rider_exe);
                            }
                            let rider_exe2 = ver.path().join("bin").join("rider.exe");
                            if rider_exe2.exists() {
                                candidates.push(rider_exe2);
                            }
                        }
                    }
                }
            }
        }
    }
    for base in [
        r"C:\Program Files\JetBrains",
        r"C:\Program Files (x86)\JetBrains",
    ] {
        let base_path = PathBuf::from(base);
        if let Ok(entries) = std::fs::read_dir(&base_path) {
            for e in entries.flatten() {
                for exe in ["rider64.exe", "rider.exe"] {
                    let p = e.path().join("bin").join(exe);
                    if p.exists() {
                        candidates.push(p);
                    }
                }
            }
        }
    }
    candidates.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .map(std::cmp::Reverse)
            .unwrap_or(std::cmp::Reverse(std::time::SystemTime::UNIX_EPOCH))
    });
    if let Some(p) = candidates.into_iter().next() {
        return Some(p);
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for exe in ["rider64.exe", "rider.exe", "rider.cmd"] {
                let p = dir.join(exe);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    None
}

pub fn resolve_rider_path() -> Option<PathBuf> {
    cached_resolve(rider_cache(), resolve_rider_path_uncached)
}

pub fn resolve_rider_path_force() -> Option<PathBuf> {
    clear_rider_cache();
    resolve_rider_path()
}
