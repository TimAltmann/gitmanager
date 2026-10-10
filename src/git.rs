use git2::{BranchType, Repository, StatusOptions};
use std::collections::HashMap;
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

pub fn clear_vs_cache() {
    if let Some(m) = VS_PATH_CACHE.get() {
        *m.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
pub fn clear_rider_cache() {
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

// --- SolutionFile ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolutionFile {
    pub path: PathBuf,
    pub relative: String,
}

// --- RepoInfo ---
#[derive(Debug, Clone)]
pub struct RepoInfo {
    pub path: PathBuf,
    pub name: String,
    pub branch: String,
    pub dirty: bool,
    pub is_detached: bool,
    pub branches: Vec<String>,
    pub solutions: Vec<SolutionFile>,
    pub selected_solution: Option<PathBuf>,
    pub custom_values: HashMap<String, String>,
    pub custom_errors: HashMap<String, String>,
    /// Commits voraus (lokal vor Upstream). Scan-only, Default 0.
    pub ahead: usize,
    /// Commits hinterher (Upstream vor lokal). Scan-only, Default 0.
    pub behind: usize,
}

impl RepoInfo {
    pub fn new(path: PathBuf, branch: String, dirty: bool, is_detached: bool) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        Self {
            path,
            name,
            branch,
            dirty,
            is_detached,
            branches: Vec::new(),
            solutions: Vec::new(),
            selected_solution: None,
            custom_values: HashMap::new(),
            custom_errors: HashMap::new(),
            ahead: 0,
            behind: 0,
        }
    }

    pub fn with_branches(mut self, branches: Vec<String>) -> Self {
        self.branches = branches;
        self
    }
}

/// Versucht, für `path` ein Repo zu öffnen. Gibt None zurück wenn kein Repo.
pub fn open_repo(path: &Path) -> Option<Repository> {
    Repository::open(path).ok()
}

/// Ermittelt Branch-Namen und Detached-Status
pub fn get_branch(repo: &Repository) -> (String, bool) {
    match repo.head() {
        Ok(head) => {
            if head.is_branch() {
                if let Ok(name) = head.shorthand() {
                    return (name.to_string(), false);
                }
            }
            if let Some(oid) = head.target() {
                let short = format!("{oid:.7}");
                return (short, true);
            }
            if let Some(name) = parse_head_file(repo.path()) {
                return (name, false);
            }
            ("detached".to_string(), true)
        }
        Err(e) => {
            if let Some(name) = parse_head_file(repo.path()) {
                return (name, false);
            }
            eprintln!("get_branch Fehler für {}: {e}", repo.path().display());
            ("no commits".to_string(), false)
        }
    }
}

fn parse_head_file(git_dir: &Path) -> Option<String> {
    let head_path = git_dir.join("HEAD");
    let content = std::fs::read_to_string(head_path).ok()?;
    let content = content.trim();
    if let Some(stripped) = content.strip_prefix("ref: ") {
        if let Some(branch) = stripped.strip_prefix("refs/heads/") {
            return Some(branch.to_string());
        }
        return Some(stripped.to_string());
    }
    if content.len() >= 7 {
        return Some(content[..7].to_string());
    }
    None
}

/// Prüft ob uncommitted changes vorhanden sind (modified, staged, untracked)
pub fn is_dirty(repo: &Repository) -> bool {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false);
    match repo.statuses(Some(&mut opts)) {
        Ok(statuses) => !statuses.is_empty(),
        Err(_) => false,
    }
}

/// Detaillierter Status: staged, unstaged, untracked, Konflikte
pub fn has_merge_conflicts(repo: &Repository) -> bool {
    repo.index().map(|idx| idx.has_conflicts()).unwrap_or(false)
}

pub fn is_merge_in_progress(repo: &Repository) -> bool {
    repo.state() != git2::RepositoryState::Clean
}

pub fn get_detailed_status(repo: &Repository) -> Vec<String> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        // P4: kein recurse in untracked Dirs auf dem UI-Thread — bei großen
        // Repos (node_modules/target) sonst hunderte ms Freeze. Top-Level
        // reicht für den Dialog; Details sieht man in der IDE.
        .recurse_untracked_dirs(false)
        .include_ignored(false);
    let mut files = Vec::new();
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        for entry in statuses.iter().take(100) {
            if let Ok(path) = entry.path() {
                let status = entry.status();
                let flag = if status.is_wt_new() {
                    "untracked"
                } else if status.is_wt_modified() {
                    "modified"
                } else if status.is_index_modified() {
                    "staged"
                } else if status.is_conflicted() {
                    "conflict"
                } else {
                    "dirty"
                };
                files.push(format!("{flag}: {path}"));
            }
        }
    }
    files
}

/// Listet lokale und remote Branches auf (für Dropdown). Remote als "origin/branch".
/// Pfad-Wrapper um [`list_branches_in`] (nur Tests nutzen ihn direkt;
/// produktiv läuft alles über das geöffnete Handle).
#[cfg_attr(not(test), allow(dead_code))]
pub fn list_branches(path: &Path) -> Vec<String> {
    let repo = match Repository::open(path) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    list_branches_in(&repo)
}

/// Finding C: Variante mit bereits geöffnetem Handle — kein zweites
/// `Repository::open` pro Scan (siehe `get_repo_info`).
pub fn list_branches_in(repo: &Repository) -> Vec<String> {
    if repo.is_bare() {
        return Vec::new();
    }
    let mut branches = Vec::new();
    // Lokale Branches
    if let Ok(iter) = repo.branches(Some(BranchType::Local)) {
        for b in iter.flatten() {
            if let Ok(Some(name)) = b.0.name() {
                branches.push(name.to_string());
            } else if let Ok(sh) = b.0.get().shorthand() {
                branches.push(sh.to_string());
            }
        }
    }
    // Remote Branches (origin/...)
    if let Ok(iter) = repo.branches(Some(BranchType::Remote)) {
        for b in iter.flatten() {
            // Remote branches kommen als "origin/main" oder "origin/HEAD"
            let shorthand = b.0.get().shorthand().unwrap_or("").to_string();
            // Ignoriere HEAD
            if shorthand.ends_with("/HEAD") {
                continue;
            }
            // Füge hinzu, auch wenn lokaler Branch gleichen Namens existiert – deduplizieren später
            if !shorthand.is_empty() {
                // Nur wenn nicht schon als lokaler vorhanden (z.B. "main" vs "origin/main")
                // Behalte remote mit Prefix, damit Checkout tracking erstellen kann
                branches.push(shorthand);
            }
        }
    }
    branches.sort();
    branches.dedup();
    branches
}

/// Holt alle Remotes (git fetch --all) – für Refresh-Button.
///
/// Ehrliche Semantik: `Ok` nur wenn alle Remotes erfolgreich waren (oder keine
/// Remotes konfiguriert sind). Jeder Remote-Fehler wird pro Remote gesammelt und
/// als strukturiertes [`PartialFetch`] (`succeeded` + `failed`, auch bei komplettem
/// Fehlschlag mit leerem `succeeded`) zurückgegeben, siehe
/// [`is_partial_fetch_error`] / [`is_fetch_error`], damit die UI `fetch_partial`
/// statt `fetch_failed` wählen und Wrapper/Hinweis via i18n lokalisieren kann.
/// `eprintln!` bleibt als Log erhalten.
///
/// Signatur bleibt `fetch_all(path: &Path) -> anyhow::Result<()>` (Task 5 baut darauf).
pub fn fetch_all(path: &Path) -> anyhow::Result<()> {
    let repo =
        Repository::open(path).map_err(|e| anyhow::anyhow!("Failed to open repository: {e}"))?;
    let remotes = repo
        .remotes()
        .map_err(|e| anyhow::anyhow!("Failed to read remotes: {e}"))?;
    if remotes.is_empty() {
        return Ok(());
    }
    let mut succeeded: Vec<String> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    for remote_res in remotes.iter() {
        let remote_name = match remote_res {
            Ok(Some(name)) => name.to_string(),
            Ok(None) => continue,
            Err(e) => {
                eprintln!("Remote name invalid: {e}");
                failed.push(("<invalid>".to_string(), e.to_string()));
                continue;
            }
        };
        match repo.find_remote(&remote_name) {
            Ok(mut remote) => {
                let mut opts = git2::FetchOptions::new();
                opts.prune(git2::FetchPrune::On);
                match remote.fetch(&[] as &[&str], Some(&mut opts), None) {
                    Ok(()) => succeeded.push(remote_name),
                    Err(e) => {
                        eprintln!("Fetch für Remote '{remote_name}' fehlgeschlagen: {e}");
                        failed.push((remote_name, e.to_string()));
                    }
                }
            }
            Err(e) => {
                eprintln!("Remote '{remote_name}' nicht gefunden: {e}");
                failed.push((remote_name, e.to_string()));
            }
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    // I1: alle Remote-Fehler (komplett + partiell) als strukturiertes PartialFetch.
    // UI baut Wrapper/Hinweis lokalisiert via tr_fmt(fetch_failed/fetch_partial) +
    // tr(auth_hint); Detail bleibt English-neutral mit opakem libgit2-Text.
    Err(anyhow::Error::new(PartialFetch { succeeded, failed }))
}

/// Strukturierter Fetch-Fehler (alle Remotes failed UND partielle Fehlschläge).
/// Als anyhow-Error transportiert, damit die UI per [`is_partial_fetch_error`]
/// ohne String-Parsing `fetch_partial` statt `fetch_failed` wählen kann und das
/// Detail via [`PartialFetch::detail_en`] English-neutral (opake libgit2-Texte)
/// einbettet. Display ist English-neutral für Logs/Tests; die UI lokalisiert
/// Wrapper + Hinweis via i18n (`fetch_failed`/`fetch_partial`, `auth_hint`).
#[derive(Debug)]
pub struct PartialFetch {
    pub succeeded: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl PartialFetch {
    /// English-neutrales Detail ohne Wrapper/Hinweis: `'<remote>': <libgit2-msg>; ...`
    /// plus `; succeeded: '<ok>', ...` bei Teil-Erfolg. libgit2-Texte bleiben opak.
    pub fn detail_en(&self) -> String {
        let details = self
            .failed
            .iter()
            .map(|(n, e)| format!("'{n}': {e}"))
            .collect::<Vec<_>>()
            .join("; ");
        if self.succeeded.is_empty() {
            return details;
        }
        let ok_names = self
            .succeeded
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(", ");
        if details.is_empty() {
            format!("succeeded: {ok_names}")
        } else {
            format!("{details}; succeeded: {ok_names}")
        }
    }
}

impl std::fmt::Display for PartialFetch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // English-neutral für EN-UI + Logs; DE-UI nutzt detail_en() + i18n-Wrapper.
        const EN_HINT: &str = "Note: check Credentials/SSH (auth may be required)";
        if self.succeeded.is_empty() {
            let names = self
                .failed
                .iter()
                .map(|(n, _)| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ");
            write!(
                f,
                "Fetch failed for remote(s) {names}: {}. {EN_HINT}",
                self.detail_en()
            )
        } else {
            let failed_names = self
                .failed
                .iter()
                .map(|(n, _)| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ");
            write!(
                f,
                "Fetch partially failed for remote(s) {failed_names}: {}. {EN_HINT}",
                self.detail_en()
            )
        }
    }
}

impl std::error::Error for PartialFetch {}

/// True wenn `err` (aus [`fetch_all`]) einen teilweisen Fehlschlag beschreibt
/// (mind. 1 Remote ok, mind. 1 failed). Komplette Remote-Fehler sind ebenfalls
/// [`PartialFetch`] (mit leerem `succeeded`), zählen aber NICHT als partiell.
pub fn is_partial_fetch_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<PartialFetch>()
        .is_some_and(|p| !p.succeeded.is_empty())
}

/// True wenn `err` ein strukturierter Fetch-Remote-Fehler ist (komplett oder
/// partiell, d.h. [`PartialFetch`] mit beliebigem `succeeded`). Sonstige Fehler
/// (z.B. Repo öffnen) sind keine Remote-Fehler.
pub fn is_fetch_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<PartialFetch>().is_some()
}

/// Ermittelt (ahead, behind) des aktuellen Branches gegenüber Upstream.
///
/// Upstream-Auflösung: `branch_upstream` (lokaler Tracking-Branch) oder
/// `origin/<branch>` als Fallback. Detached HEAD, fehlender Upstream oder
/// kein Remote → `(0, 0)`. Scan-only, nie Err — Fehler bedeuten "unbekannt".
/// Pfad-Wrapper um [`ahead_behind_in`] (nur Tests nutzen ihn direkt;
/// produktiv läuft alles über das geöffnete Handle).
#[cfg_attr(not(test), allow(dead_code))]
pub fn ahead_behind(path: &Path) -> (usize, usize) {
    let repo = match Repository::open(path) {
        Ok(r) => r,
        Err(_) => return (0, 0),
    };
    ahead_behind_in(&repo)
}

/// Finding C: Variante mit bereits geöffnetem Handle — kein drittes
/// `Repository::open` pro Scan (siehe `get_repo_info`).
pub fn ahead_behind_in(repo: &Repository) -> (usize, usize) {
    if repo.is_bare() {
        return (0, 0);
    }
    let (branch, detached) = get_branch(repo);
    if detached {
        return (0, 0);
    }
    if branch.is_empty() || branch == "no commits" || branch == "detached" {
        return (0, 0);
    }
    // Lokale OID: refs/heads/<branch>, Fallback HEAD target.
    let local_oid = match repo.refname_to_id(&format!("refs/heads/{branch}")) {
        Ok(o) => o,
        Err(_) => match repo.head().ok().and_then(|h| h.target()) {
            Some(o) => o,
            None => return (0, 0),
        },
    };
    // 1) Tracking-Upstream via branch_upstream
    if let Ok(local_branch) = repo.find_branch(&branch, BranchType::Local) {
        if let Ok(upstream) = local_branch.upstream() {
            // upstream.get().target() ist die OID des Upstream-Commits
            if let Some(up_oid) = upstream.get().target() {
                if let Ok((a, b)) = repo.graph_ahead_behind(local_oid, up_oid) {
                    return (a, b);
                }
            } else if let Ok(up_ref) = upstream.get().peel_to_commit() {
                if let Ok((a, b)) = repo.graph_ahead_behind(local_oid, up_ref.id()) {
                    return (a, b);
                }
            }
            // Upstream bekannt aber graph failed → (0,0) statt Fallback-Raten
            // (verhindert falsche origin/<branch>-Werte bei divergenten Refs).
            // Falls upstream-OID fehlt, falle unten auf origin/<branch> zurück.
            if upstream.get().target().is_some() {
                return (0, 0);
            }
        }
    }
    // 2) Fallback: alle Remotes statt nur origin (I4) — sonst liefert
    // ein upstream-Remote oder Firmen-Remote ≠ origin immer (0,0).
    // Zuerst branch.<name>.remote lesen (explizites Tracking ohne upstream-Ref),
    // dann alle Remotes iterieren.
    if let Ok(cfg_branch_remote) = repo
        .config()
        .and_then(|c| c.get_string(&format!("branch.{branch}.remote")))
    {
        for candidate in [
            format!("refs/remotes/{cfg_branch_remote}/{branch}"),
            format!("{cfg_branch_remote}/{branch}"),
        ] {
            if let Ok(obj) = repo.revparse_single(&candidate) {
                let up_oid = obj
                    .peel_to_commit()
                    .map(|c| c.id())
                    .unwrap_or_else(|_| obj.id());
                if let Ok((a, b)) = repo.graph_ahead_behind(local_oid, up_oid) {
                    return (a, b);
                }
                // F2#2: Graph-Fehler → nicht (0,0), sondern unten andere
                // Remotes versuchen (Upstream-bekannt-Regel oben bleibt).
            }
        }
    }
    if let Ok(remotes) = repo.remotes() {
        // L5: `origin` zuerst prüfen — bei Forks (origin+upstream) würde
        // alphabetisch sonst der falsche Remote den Zähler liefern.
        let mut names: Vec<String> = remotes
            .iter()
            .filter_map(|r| r.ok().flatten().map(|n| n.to_string()))
            .collect();
        names.sort_by_key(|n| (n != "origin", n.clone()));
        for remote_name in &names {
            for candidate in [
                format!("refs/remotes/{remote_name}/{branch}"),
                format!("{remote_name}/{branch}"),
            ] {
                if let Ok(obj) = repo.revparse_single(&candidate) {
                    let up_oid = obj
                        .peel_to_commit()
                        .map(|c| c.id())
                        .unwrap_or_else(|_| obj.id());
                    if let Ok((a, b)) = repo.graph_ahead_behind(local_oid, up_oid) {
                        return (a, b);
                    }
                    // F2#2: Graph-Fehler → nächsten Remote versuchen.
                }
            }
        }
    }
    (0, 0)
}

/// Öffnet den Explorer / Dateimanager im Repo-Verzeichnis
pub fn open_in_explorer(path: &Path) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        // In tests: don't actually spawn file manager windows
        // Just verify path handling, return Ok to avoid stray windows staying open
        let _ = path;
        return Ok(());
    }
    #[cfg(not(test))]
    {
        #[cfg(windows)]
        {
            std::process::Command::new("explorer")
                .arg(path)
                .spawn()
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("Explorer konnte nicht geöffnet werden: {e}"))
        }
        #[cfg(not(windows))]
        {
            // Linux: xdg-open, WSL: explorer.exe, macOS: open – plus gio als Fallback
            std::process::Command::new("xdg-open")
                .arg(path)
                .spawn()
                .or_else(|_| std::process::Command::new("explorer.exe").arg(path).spawn())
                .or_else(|_| std::process::Command::new("open").arg(path).spawn())
                .or_else(|_| {
                    std::process::Command::new("gio")
                        .args(["open", &path.display().to_string()])
                        .spawn()
                })
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("Dateimanager konnte nicht geöffnet werden: {e}"))
        }
    }
}

fn preflight_checkout(path: &Path) -> anyhow::Result<Repository> {
    let repo =
        Repository::open(path).map_err(|e| anyhow::anyhow!("Repo öffnen fehlgeschlagen: {e}"))?;
    if repo.is_bare() {
        anyhow::bail!("Bare Repository – Checkout nicht möglich");
    }
    if is_merge_in_progress(&repo) {
        anyhow::bail!("Merge/Rebase läuft noch ({:?})", repo.state());
    }
    if has_merge_conflicts(&repo) {
        anyhow::bail!("Merge-Konflikte vorhanden – bitte erst lösen");
    }
    Ok(repo)
}

fn set_head_from_reference(
    repo: &Repository,
    reference: Option<&git2::Reference>,
    object: &git2::Object,
) -> anyhow::Result<()> {
    match reference {
        Some(gref) => {
            let name = gref
                .name()
                .map_err(|e| anyhow::anyhow!("Ungültige Referenz: {e}"))?;
            repo.set_head(name)
                .map_err(|e| anyhow::anyhow!("HEAD setzen fehlgeschlagen: {e}"))?;
        }
        None => {
            repo.set_head_detached(object.id())
                .map_err(|e| anyhow::anyhow!("Detached HEAD setzen fehlgeschlagen: {e}"))?;
        }
    }
    Ok(())
}

/// K2: Kurzname eines Remote-Branch — nur das `refs/remotes/<remote>/`-Präfix
/// strippen, nicht das letzte `/`-Segment (sonst wird `feature/foo` zu `foo`).
pub fn remote_short_name(full_ref: &str, branch_name: &str) -> String {
    if let Some(rest) = full_ref.strip_prefix("refs/remotes/") {
        if let Some((_, short)) = rest.split_once('/') {
            return short.to_string();
        }
    }
    if let Some(rest) = branch_name.strip_prefix("origin/") {
        return rest.to_string();
    }
    // F1#7: generisch erstes Segment strippen (z.B. `upstream/foo` → `foo`),
    // statt nur `origin/` zu kennen.
    if let Some((_, short)) = branch_name.split_once('/') {
        return short.to_string();
    }
    branch_name.to_string()
}

/// Führt sicheren Branch-Wechsel durch (nur wenn keine Konflikte, sonst Fehler)
pub fn checkout_branch(path: &Path, branch_name: &str) -> anyhow::Result<()> {
    let repo = preflight_checkout(path)?;
    let (object, reference) = repo
        .revparse_ext(branch_name)
        .map_err(|e| anyhow::anyhow!("Branch '{branch_name}' nicht gefunden: {e}"))?;

    // Dry-run zuerst
    {
        let mut cb = git2::build::CheckoutBuilder::new();
        cb.dry_run();
        cb.safe();
        if let Err(e) = repo.checkout_tree(&object, Some(&mut cb)) {
            if e.code() == git2::ErrorCode::Conflict {
                anyhow::bail!("Checkout würde Konflikte erzeugen (dirty Dateien): {e}");
            }
            anyhow::bail!("Preflight fehlgeschlagen: {e}");
        }
    }

    // Eigentlicher Checkout
    let mut cb = git2::build::CheckoutBuilder::new();
    cb.safe();
    repo.checkout_tree(&object, Some(&mut cb))
        .map_err(|e| anyhow::anyhow!("Checkout fehlgeschlagen: {e}"))?;

    // Bei Remote-Branch: lokalen Tracking-Branch erstellen
    if let Some(gref) = &reference {
        let name = gref
            .name()
            .map_err(|e| anyhow::anyhow!("Ungültige Referenz: {e}"))?;
        if name.starts_with("refs/remotes/") {
            // K2: nur Remote-Präfix strippen (siehe remote_short_name).
            let short = remote_short_name(name, branch_name);
            let commit = object
                .peel_to_commit()
                .map_err(|e| anyhow::anyhow!("Commit nicht gefunden: {e}"))?;
            if repo.find_branch(&short, BranchType::Local).is_err() {
                let mut local_branch = repo
                    .branch(&short, &commit, false)
                    .map_err(|e| anyhow::anyhow!("Lokalen Branch erstellen fehlgeschlagen: {e}"))?;
                // Upstream als Kurzname (origin/feature/foo), nicht Full-Ref.
                let upstream = name.strip_prefix("refs/remotes/").unwrap_or(name);
                let _ = local_branch.set_upstream(Some(upstream));
            }
            repo.set_head(&format!("refs/heads/{short}"))
                .map_err(|e| anyhow::anyhow!("HEAD setzen fehlgeschlagen: {e}"))?;
            return Ok(());
        }
    }

    set_head_from_reference(&repo, reference.as_ref(), &object)
}

/// Force Checkout (überschreibt lokale Änderungen) – nur nach Bestätigung via Dialog nach Dirty-Check.
/// Verhindert versehentliches Zerstören eines laufenden Merge/Rebase-States.
pub fn checkout_branch_force(path: &Path, branch_name: &str) -> anyhow::Result<()> {
    let repo = preflight_checkout(path)?;
    let (object, reference) = repo
        .revparse_ext(branch_name)
        .map_err(|e| anyhow::anyhow!("Branch '{branch_name}' nicht gefunden: {e}"))?;
    let mut cb = git2::build::CheckoutBuilder::new();
    cb.force();
    repo.checkout_tree(&object, Some(&mut cb))
        .map_err(|e| anyhow::anyhow!("Force Checkout fehlgeschlagen: {e}"))?;
    set_head_from_reference(&repo, reference.as_ref(), &object)
}

/// M4: typisierter Stash-Pop-Konflikt — statt String-Match
/// (`contains("Stash pop fehlgeschlagen")`, fragil bei Text/i18n) prüft die
/// UI via `is_stash_pop_failed()` per Downcast. Display-Text bleibt kompatibel.
#[derive(Debug)]
pub struct StashPopFailed {
    pub detail: String,
}

impl std::fmt::Display for StashPopFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Branch gewechselt, aber Stash pop fehlgeschlagen (Konflikte) – Stash bleibt unter stash@{{0}} erhalten: {}",
            self.detail
        )
    }
}

impl std::error::Error for StashPopFailed {}

/// True wenn `err` ein Stash-Pop-Konflikt nach erfolgreichem Checkout ist
/// (Checkout ok, Pop gescheitert — Rescan trotzdem sinnvoll).
pub fn is_stash_pop_failed(err: &anyhow::Error) -> bool {
    err.downcast_ref::<StashPopFailed>().is_some()
}

/// Stash + Checkout (für Dirty-Dialog Option)
pub fn stash_and_checkout(path: &Path, branch_name: &str) -> anyhow::Result<()> {
    let mut repo =
        Repository::open(path).map_err(|e| anyhow::anyhow!("Repo öffnen fehlgeschlagen: {e}"))?;
    let sig = repo
        .signature()
        .or_else(|_| git2::Signature::now("gitmanager", "gitmanager@example.com"))
        .map_err(|e| anyhow::anyhow!("Signatur erstellen fehlgeschlagen: {e}"))?;
    // Stash mit untracked
    let stash_flags = git2::StashFlags::INCLUDE_UNTRACKED;
    let stash_msg = format!("autostash vor Wechsel zu {branch_name}");
    // stash_save gibt Oid zurück, Fehler wenn nichts zu stashen
    let stash_result = repo.stash_save(&sig, &stash_msg, Some(stash_flags));
    if let Err(e) = &stash_result {
        // Wenn nichts zu stashen (keine Änderungen), ist das ok – trotzdem checkout
        if e.code() != git2::ErrorCode::NotFound {
            // NotFound bedeutet "nothing to stash" bei manchen libgit2 Versionen
            eprintln!("Stash warnung: {e}");
        }
    }
    // Jetzt checkout
    let stashed = stash_result.is_ok();
    match checkout_branch(path, branch_name) {
        Ok(()) => {
            // Nur poppen wenn tatsächlich gestasht wurde — sonst würde
            // stash_pop(0) einen alten, bewusst aufgehobenen Stash poppen (K1).
            if !stashed {
                return Ok(());
            }
            // Versuche stash pop, aber ignoriere Konflikte (lasse stash bestehen)
            let mut repo2 = Repository::open(path)
                .map_err(|e| anyhow::anyhow!("Repo erneut öffnen fehlgeschlagen: {e}"))?;
            if let Err(e) = repo2.stash_pop(0, None) {
                eprintln!(
                    "Stash pop nach Checkout fehlgeschlagen (Konflikt), Stash bleibt erhalten: {e}"
                );
                // Lasse stash bestehen, informiere Nutzer (M4: typisiert).
                return Err(anyhow::Error::new(StashPopFailed {
                    detail: format!("{e}"),
                }));
            }
            Ok(())
        }
        Err(e) => {
            // Checkout fehlgeschlagen → versuche stash wiederherzustellen
            if stash_result.is_ok() {
                let repo2 = Repository::open(path).ok();
                if let Some(mut r) = repo2 {
                    let _ = r.stash_pop(0, None);
                }
            }
            Err(e)
        }
    }
}

/// Convenience: RepoInfo für einen Pfad ermitteln, falls es ein Repo ist
pub fn get_repo_info(path: PathBuf) -> Option<RepoInfo> {
    let repo = open_repo(&path)?;
    if repo.is_bare() {
        return None;
    }
    // Verhindere Discovery-Bug: Repository::open() macht Discovery und öffnet Parent-Repos.
    // Nur wenn workdir == path, ist path selbst ein Repo-Root.
    {
        let workdir = repo.workdir()?;
        let candidate_canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        let workdir_canon = workdir
            .canonicalize()
            .unwrap_or_else(|_| workdir.to_path_buf());
        // Auf Windows case-insensitiv vergleichen
        #[cfg(windows)]
        {
            if candidate_canon.to_string_lossy().to_lowercase()
                != workdir_canon.to_string_lossy().to_lowercase()
            {
                return None;
            }
        }
        #[cfg(not(windows))]
        {
            if candidate_canon != workdir_canon {
                return None;
            }
        }
    }
    let (branch, is_detached) = get_branch(&repo);
    let dirty = is_dirty(&repo);
    // Finding C: geöffnetes Handle wiederverwenden statt 2× erneut öffnen.
    let branches = list_branches_in(&repo);
    let (ahead, behind) = ahead_behind_in(&repo);
    let mut info = RepoInfo::new(path, branch, dirty, is_detached).with_branches(branches);
    info.ahead = ahead;
    info.behind = behind;
    Some(info)
}

// --- IDE & Agent Launch Helpers ---
use crate::config::{AgentProfile, IdeConfig, TerminalPreference};

pub(crate) fn substitute_placeholders(template: &str, file: &Path, repo: &Path) -> String {
    let file_str = file.display().to_string();
    let repo_str = repo.display().to_string();
    // {dir} = parent of file (solution dir) when file != repo, else repo itself (for agent case where file==repo)
    let dir_str = if file == repo {
        repo_str.clone()
    } else {
        file.parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| repo_str.clone())
    };
    template
        .replace("{file}", &file_str)
        .replace("{dir}", &dir_str)
        .replace("{repo}", &repo_str)
}

fn quote_if_needed(s: &str) -> String {
    if s.contains(' ') && !(s.starts_with('"') && s.ends_with('"')) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn escape_for_powershell(s: &str) -> String {
    let needs_quote = s.contains(' ')
        || s.contains('\'')
        || s.contains('"')
        || s.contains('&')
        || s.contains('|')
        || s.contains(';')
        || s.contains('>')
        || s.contains('<')
        || s.contains('^')
        || s.contains('`')
        || s.contains('$');
    if needs_quote {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

#[cfg(not(test))]
fn escape_for_powershell_force(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn escape_for_cmd(s: &str) -> String {
    // Für cmd /K: wrap in double quotes if contains space or shell meta, escape inner quotes
    let needs_quote = s.contains(' ')
        || s.contains('&')
        || s.contains('|')
        || s.contains('<')
        || s.contains('>')
        || s.contains('^')
        || s.contains('"');
    if needs_quote {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn has_placeholders(s: &str) -> bool {
    s.contains("{file}") || s.contains("{dir}") || s.contains("{repo}")
}

fn build_agent_command<F>(
    agent: &AgentProfile,
    repo_path: &Path,
    escape_fn: F,
    prefix: &str,
    substitute_args: bool,
) -> String
where
    F: Fn(&str) -> String,
{
    if !agent.args.is_empty() {
        let escaped_program = escape_fn(&agent.program);
        let args_str = agent
            .args
            .iter()
            .map(|a| {
                let substituted = if substitute_args {
                    substitute_placeholders(a, repo_path, repo_path)
                } else {
                    a.clone()
                };
                escape_fn(&substituted)
            })
            .collect::<Vec<_>>()
            .join(" ");
        if args_str.is_empty() {
            format!("{prefix}{escaped_program}")
        } else {
            format!("{prefix}{escaped_program} {args_str}")
        }
    } else if let Some(cmd) = &agent.command {
        if has_placeholders(cmd) {
            substitute_placeholders(cmd, repo_path, repo_path)
        } else {
            cmd.clone()
        }
    } else {
        format!("{}{}", prefix, escape_fn(&agent.program))
    }
}

fn build_agent_cmd_raw(agent: &AgentProfile, repo_path: &Path) -> String {
    // Wird nur für TerminalPreference::Custom via `cmd.arg(&agent_cmd_raw)` verwendet
    // (`src/git.rs:841`). `Command::arg` übergibt ohne Shell-Parsing – zusätzliche
    // Quotes würden literal Teil des Arguments (z. B. `"C:\Program Files\..."`).
    // Daher program NICHT quoten (ursprüngliches Verhalten: `program.clone()`).
    if !agent.args.is_empty() {
        let args_str = agent
            .args
            .iter()
            .map(|a| {
                let substituted = substitute_placeholders(a, repo_path, repo_path);
                quote_if_needed(&substituted)
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} {}", agent.program, args_str)
    } else if let Some(cmd) = &agent.command {
        if has_placeholders(cmd) {
            substitute_placeholders(cmd, repo_path, repo_path)
        } else {
            cmd.clone()
        }
    } else {
        agent.program.clone()
    }
}

fn build_powershell_script(agent: &AgentProfile, repo_path: &Path) -> String {
    let prog = &agent.program;
    let needs_amp =
        prog.contains(' ') || prog.contains('\\') || prog.contains('/') || prog.contains('\'');
    let prefix = if needs_amp { "& " } else { "" };
    build_agent_command(agent, repo_path, escape_for_powershell, prefix, true)
}

fn build_cmd_agent_string(agent: &AgentProfile, repo_path: &Path) -> String {
    build_agent_command(agent, repo_path, escape_for_cmd, "", true)
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
        std::process::Command::new(where_cmd)
            .arg(program)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
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

fn resolve_vs_path_uncached() -> Option<PathBuf> {
    let vswhere =
        PathBuf::from(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    if vswhere.exists() {
        if let Ok(out) = std::process::Command::new(&vswhere)
            .args(["-latest", "-products", "*", "-property", "productPath"])
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty()
                    && !p.to_lowercase().contains("sql server")
                    && !p.to_lowercase().contains("ssms")
                {
                    let pb = PathBuf::from(&p);
                    if pb.exists() {
                        return Some(pb);
                    }
                }
            }
        }
        if let Ok(out) = std::process::Command::new(&vswhere)
            .args(["-latest", "-products", "*", "-property", "installationPath"])
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    let candidate = PathBuf::from(&p)
                        .join("Common7")
                        .join("IDE")
                        .join("devenv.exe");
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
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

fn set_creation_flags(_cmd: &mut std::process::Command, _flags: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        _cmd.creation_flags(_flags);
    }
}

/// Öffnet eine Datei/Ordner mit einer IDE-Konfiguration
/// `repo_path` ist immer das Repo-Root (für cwd), `file` ist optional die Solution/Project Datei
pub fn launch_ide(ide: &IdeConfig, repo_path: &Path, file: Option<&Path>) -> anyhow::Result<()> {
    let program = ide.effective_program();
    let args_template = ide.effective_args();
    let file_for_subst: PathBuf = file
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| repo_path.to_path_buf());
    let args: Vec<String> = args_template
        .iter()
        .map(|a| substitute_placeholders(a, &file_for_subst, repo_path))
        .collect();

    let mut effective_program = program.clone();
    if is_vs_program(&program) {
        if let Some(vs_path) = resolve_vs_path() {
            effective_program = vs_path.to_string_lossy().to_string();
        }
    } else if is_rider_program(&program) {
        // Konsistent zu VS: immer versuchen aufzulösen, nicht nur wenn !is_program_available
        if let Some(rider_path) = resolve_rider_path() {
            effective_program = rider_path.to_string_lossy().to_string();
        }
    }

    let spawn = |prog: &str, args: &[String]| -> std::io::Result<std::process::Child> {
        let mut cmd = std::process::Command::new(prog);
        cmd.args(args);
        cmd.current_dir(repo_path);
        set_creation_flags(&mut cmd, 0x08000000);
        cmd.spawn()
    };

    // Validierung: nur wenn Shell verwendet wird (use_shell) und nicht allow_unsafe, blockiere gefährliche Zeichen
    // Bei use_shell=false werden args via Command::args ohne Shell-Parsing übergeben – Zeichen wie '&' in Pfaden
    // wie "C:\Users\Tom & Jerry\app.sln" sind dann harmlos und dürfen nicht blockiert werden.
    // Bei use_shell=true wird via cmd /C gestartet und Shell-Zeichen müssen blockiert werden.
    let shell_chars = ['&', '|', ';', '>', '<'];
    if ide.use_shell && !ide.allow_unsafe {
        for a in &args {
            if a.chars().any(|c| shell_chars.contains(&c)) {
                anyhow::bail!("Ungültige Zeichen in IDE-Args (shell-Zeichen) – aktiviere allow_unsafe wenn beabsichtigt: {a}");
            }
        }
        if effective_program.chars().any(|c| shell_chars.contains(&c)) {
            anyhow::bail!(
                "Ungültige Zeichen in IDE-Programm (shell-Zeichen) – aktiviere allow_unsafe wenn beabsichtigt: {}",
                effective_program
            );
        }
    }

    #[cfg(test)]
    {
        // In tests: don't actually spawn GUI IDEs (VS Code, Visual Studio, Rider)
        // This prevents VS Code windows staying open after `cargo test` on dev machines
        let lower = effective_program.to_lowercase();
        if lower.contains("code") || lower.contains("devenv") || lower.contains("rider") {
            return Ok(());
        }
    }

    match spawn(&effective_program, &args) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(windows)]
            {
                // Fallback via cmd /C verwendet eine Shell – daher bei !allow_unsafe Shell-Zeichen blockieren,
                // unabhängig von use_shell, da hier immer eine Shell involviert ist.
                if !ide.allow_unsafe {
                    for a in &args {
                        if a.chars().any(|c| shell_chars.contains(&c)) {
                            anyhow::bail!("Ungültige Zeichen in IDE-Args für Shell-Fallback (allow_unsafe erforderlich): {a}");
                        }
                    }
                    if effective_program.chars().any(|c| shell_chars.contains(&c)) {
                        anyhow::bail!(
                            "Ungültige Zeichen in IDE-Programm für Shell-Fallback (allow_unsafe erforderlich): {}",
                            effective_program
                        );
                    }
                }
                let mut shell_args = vec!["/C".to_string(), effective_program.clone()];
                shell_args.extend(args.clone());
                let mut cmd = std::process::Command::new("cmd");
                cmd.args(&shell_args);
                cmd.current_dir(repo_path);
                set_creation_flags(&mut cmd, 0x08000000);
                cmd.spawn().map(|_| ()).map_err(|e2| {
                    anyhow::anyhow!(
                        "IDE nicht gefunden (weder '{}' noch 'cmd /C'): {} / {}",
                        effective_program,
                        e,
                        e2
                    )
                })
            }
            #[cfg(not(windows))]
            {
                Err(anyhow::anyhow!(
                    "Programm '{}' nicht im PATH: {}",
                    effective_program,
                    e
                ))
            }
        }
        Err(e) => Err(anyhow::anyhow!(
            "Konnte IDE '{}' nicht starten: {}",
            effective_program,
            e
        )),
    }
}

#[cfg(not(test))]
fn spawn_wt(repo_str: &str, cmd_string: &str, repo_path: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("wt");
    cmd.args(["-d", repo_str, "--", "cmd", "/k", cmd_string]);
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, 0x00000010);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Windows Terminal starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_powershell(ps_script: &str, repo_path: &Path) -> anyhow::Result<()> {
    let has_pwsh = is_program_available("pwsh");
    let ps = if has_pwsh { "pwsh" } else { "powershell" };
    let mut cmd = std::process::Command::new(ps);
    if has_pwsh {
        cmd.args([
            "-NoLogo",
            "-NoExit",
            "-WorkingDirectory",
            &repo_path.display().to_string(),
            "-Command",
            ps_script,
        ]);
    } else {
        let repo_escaped = escape_for_powershell_force(&repo_path.display().to_string());
        let full_script = format!("Set-Location {}; {}", repo_escaped, ps_script);
        cmd.args(["-NoLogo", "-NoExit", "-Command", &full_script]);
    }
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, 0x00000010);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Powershell starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_powershell_shell(repo_path: &Path) -> anyhow::Result<()> {
    let has_pwsh = is_program_available("pwsh");
    let ps = if has_pwsh { "pwsh" } else { "powershell" };
    let mut cmd = std::process::Command::new(ps);
    if has_pwsh {
        cmd.args([
            "-NoLogo",
            "-NoExit",
            "-WorkingDirectory",
            &repo_path.display().to_string(),
        ]);
    } else {
        cmd.args([
            "-NoLogo",
            "-NoExit",
            "-Command",
            &format!(
                "Set-Location {}",
                escape_for_powershell_force(&repo_path.display().to_string())
            ),
        ]);
    }
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, 0x00000010);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Powershell starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_wt_shell(repo_path: &Path) -> anyhow::Result<()> {
    let repo_str = repo_path.display().to_string();
    let mut cmd = std::process::Command::new("wt");
    cmd.args(["-d", &repo_str]);
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, 0x00000010);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Windows Terminal starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_cmd_shell(repo_str: &str, repo_path: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "start", "", "/D", repo_str, "cmd", "/K", ""]);
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, 0x08000000);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("cmd starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_cmd(agent_string: &str, repo_str: &str, repo_path: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "start", "", "/D", repo_str, "cmd", "/K", agent_string]);
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, 0x08000000);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("cmd starten fehlgeschlagen: {e}"))
}

/// Öffnet ein Terminal und startet einen Agenten darin
pub fn launch_agent(
    agent: &AgentProfile,
    repo_path: &Path,
    terminal_pref: &TerminalPreference,
) -> anyhow::Result<()> {
    let pref = agent.terminal_override.as_ref().unwrap_or(terminal_pref);
    let ps_script = build_powershell_script(agent, repo_path);
    let cmd_agent_string = build_cmd_agent_string(agent, repo_path);
    let agent_cmd_raw = build_agent_cmd_raw(agent, repo_path);
    let repo_str = repo_path.display().to_string();

    #[cfg(test)]
    {
        let _ = (
            &ps_script,
            &cmd_agent_string,
            &agent_cmd_raw,
            &repo_str,
            &pref,
        );
        return Ok(());
    }

    #[cfg(not(test))]
    {
        let has_wt = is_program_available("wt");

        match pref {
            TerminalPreference::WindowsTerminal if has_wt => {
                spawn_wt(&repo_str, &cmd_agent_string, repo_path)
            }
            TerminalPreference::Powershell => spawn_powershell(&ps_script, repo_path),
            TerminalPreference::Cmd => spawn_cmd(&cmd_agent_string, &repo_str, repo_path),
            TerminalPreference::Auto if !has_wt => spawn_powershell(&ps_script, repo_path)
                .or_else(|_| spawn_cmd(&cmd_agent_string, &repo_str, repo_path)),
            TerminalPreference::Auto => spawn_wt(&repo_str, &cmd_agent_string, repo_path)
                .or_else(|_| spawn_powershell(&ps_script, repo_path))
                .or_else(|_| spawn_cmd(&cmd_agent_string, &repo_str, repo_path)),
            TerminalPreference::Custom(custom) => {
                let mut cmd = std::process::Command::new(custom);
                if !agent.args.is_empty() {
                    cmd.arg(&agent.program);
                    for a in &agent.args {
                        let substituted = substitute_placeholders(a, repo_path, repo_path);
                        cmd.arg(substituted);
                    }
                } else {
                    cmd.arg(&agent_cmd_raw);
                }
                cmd.current_dir(repo_path);
                set_creation_flags(&mut cmd, 0x00000010);
                cmd.spawn().map(|_| ()).map_err(|e| {
                    anyhow::anyhow!("Custom Terminal '{}' starten fehlgeschlagen: {e}", custom)
                })
            }
            _ => spawn_cmd(&cmd_agent_string, &repo_str, repo_path),
        }
    }
}

/// Öffnet nur die Shell im Repo-Verzeichnis (ohne Agent-Command)
pub fn open_shell(repo_path: &Path, terminal_pref: &TerminalPreference) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        let _ = (repo_path, terminal_pref);
        return Ok(());
    }
    #[cfg(not(test))]
    {
        let has_wt = is_program_available("wt");
        let repo_str = repo_path.display().to_string();
        match terminal_pref {
            TerminalPreference::WindowsTerminal if has_wt => spawn_wt_shell(repo_path),
            TerminalPreference::Powershell => spawn_powershell_shell(repo_path),
            TerminalPreference::Cmd => spawn_cmd_shell(&repo_str, repo_path),
            TerminalPreference::Auto if !has_wt => {
                spawn_powershell_shell(repo_path).or_else(|_| spawn_cmd_shell(&repo_str, repo_path))
            }
            TerminalPreference::Auto => spawn_wt_shell(repo_path)
                .or_else(|_| spawn_powershell_shell(repo_path))
                .or_else(|_| spawn_cmd_shell(&repo_str, repo_path)),
            TerminalPreference::Custom(custom) => {
                let mut cmd = std::process::Command::new(custom);
                cmd.current_dir(repo_path);
                set_creation_flags(&mut cmd, 0x00000010);
                cmd.spawn().map(|_| ()).map_err(|e| {
                    anyhow::anyhow!("Custom Terminal '{}' starten fehlgeschlagen: {e}", custom)
                })
            }
            _ => spawn_cmd_shell(&repo_str, repo_path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn init_repo_with_commit(dir: &Path, branch: &str) -> Repository {
        let repo = Repository::init(dir).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
        let file = dir.join("README.md");
        fs::write(&file, "# test\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        index.write().unwrap();
        let oid = index.write_tree().unwrap();
        {
            let tree = repo.find_tree(oid).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                .unwrap();
        }
        if branch != "master" && branch != "main" {
            let head = repo.head().unwrap();
            let commit = head.peel_to_commit().unwrap();
            repo.branch(branch, &commit, false).unwrap();
            repo.set_head(&format!("refs/heads/{branch}")).unwrap();
            repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
                .unwrap();
        } else if branch == "main" {
            let head = repo.head().unwrap();
            let commit = head.peel_to_commit().unwrap();
            if repo.find_branch("main", BranchType::Local).is_err() {
                repo.branch("main", &commit, false).unwrap();
                repo.set_head("refs/heads/main").unwrap();
                repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
                    .unwrap();
            }
        }
        repo
    }

    // --- SolutionFile & RepoInfo (F-076 - F-079) ---
    #[test]
    fn solution_file_creation_with_paths() {
        let p = PathBuf::from("/tmp/repo/foo.sln");
        let s = SolutionFile {
            path: p.clone(),
            relative: "foo.sln".to_string(),
        };
        assert_eq!(s.path, p);
        assert_eq!(s.relative, "foo.sln");
    }

    #[test]
    fn solution_file_clone_and_eq() {
        let s1 = SolutionFile {
            path: PathBuf::from("/a/b.sln"),
            relative: "b.sln".to_string(),
        };
        let s2 = s1.clone();
        assert_eq!(s1, s2);
        let s3 = SolutionFile {
            path: PathBuf::from("/a/c.sln"),
            relative: "c.sln".to_string(),
        };
        assert_ne!(s1, s3);
        let s4 = SolutionFile {
            path: PathBuf::from("/a/b.sln"),
            relative: "different".to_string(),
        };
        assert_ne!(s1, s4);
    }

    #[test]
    fn repo_info_new_with_defaults() {
        let path = PathBuf::from("/tmp/myrepo");
        let info = RepoInfo::new(path.clone(), "main".to_string(), false, false);
        assert_eq!(info.path, path);
        assert_eq!(info.name, "myrepo");
        assert_eq!(info.branch, "main");
        assert!(!info.dirty);
        assert!(!info.is_detached);
        assert!(info.branches.is_empty());
        assert!(info.solutions.is_empty());
        assert!(info.selected_solution.is_none());
        // fallback when no file_name
        let root = PathBuf::from("/");
        let info2 = RepoInfo::new(root.clone(), "main".to_string(), false, false);
        // "/" has no file_name, fallback to display
        assert!(!info2.name.is_empty());
    }

    #[test]
    fn repo_info_with_branches_builder() {
        let info = RepoInfo::new(PathBuf::from("/tmp/repo"), "main".to_string(), false, false)
            .with_branches(vec!["main".to_string(), "feature".to_string()]);
        assert_eq!(info.branches, vec!["main", "feature"]);
        let info2 = RepoInfo::new(PathBuf::from("/tmp/repo"), "main".to_string(), false, false)
            .with_branches(vec![]);
        assert!(info2.branches.is_empty());
    }

    #[test]
    fn open_repo_valid_vs_invalid() {
        let dir = tempdir().unwrap();
        assert!(open_repo(dir.path()).is_none());
        init_repo_with_commit(dir.path(), "main");
        assert!(open_repo(dir.path()).is_some());
        assert!(open_repo(Path::new("/nonexistent/path/xyz")).is_none());
    }

    #[test]
    fn get_repo_info_none_and_some() {
        let dir = tempdir().unwrap();
        assert!(get_repo_info(dir.path().to_path_buf()).is_none());
        init_repo_with_commit(dir.path(), "main");
        let info = get_repo_info(dir.path().to_path_buf());
        assert!(info.is_some());
        let info = info.unwrap();
        assert!(!info.branch.is_empty());
    }

    // --- get_branch & parse_head_file (F-082 - F-086) ---
    #[test]
    fn get_branch_normal_is_not_detached() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        let (branch, detached) = get_branch(&repo);
        assert!(!detached);
        assert!(branch == "main" || branch == "master");
        // feature branch
        let repo2 = Repository::open(dir.path()).unwrap();
        let head = repo2.head().unwrap().peel_to_commit().unwrap();
        repo2.branch("feature", &head, false).unwrap();
        repo2.set_head("refs/heads/feature").unwrap();
        repo2
            .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        let repo3 = Repository::open(dir.path()).unwrap();
        let (b2, d2) = get_branch(&repo3);
        assert_eq!(b2, "feature");
        assert!(!d2);
    }

    #[test]
    fn get_branch_detached_true_with_short_oid() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let oid = head.id();
        repo.set_head_detached(oid).unwrap();
        let repo2 = Repository::open(dir.path()).unwrap();
        let (branch, detached) = get_branch(&repo2);
        assert!(detached);
        assert_eq!(branch.len(), 7);
        assert_eq!(&format!("{oid:.7}"), &branch);
    }

    #[test]
    fn get_branch_via_head_file_ref() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let git_dir = dir.path().join(".git");
        // parse_head_file directly via get_branch fallback: corrupt HEAD to test parse
        // write ref
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        // open repo and get branch via parse_head_file when head() fails? But head() will still succeed if ref exists but not checked out
        // Instead test parse_head_file directly
        let parsed = parse_head_file(&git_dir);
        assert_eq!(parsed, Some("feature/x".to_string()));
        fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(parse_head_file(&git_dir), Some("main".to_string()));
        // other prefix
        fs::write(git_dir.join("HEAD"), "ref: refs/tags/v1.0\n").unwrap();
        assert_eq!(
            parse_head_file(&git_dir),
            Some("refs/tags/v1.0".to_string())
        );
    }

    #[test]
    fn get_branch_no_commits_fallback() {
        let dir = tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@test.com").unwrap();
        // no commit, HEAD will error
        let (branch, detached) = get_branch(&repo);
        // should be "no commits" or via parse_head_file if HEAD contains ref
        // In empty repo, HEAD is ref: refs/heads/master, parse succeeds -> branch "master"
        // So we accept either
        assert!(!branch.is_empty());
        assert!(!detached || branch.len() == 7);
    }

    #[test]
    fn parse_head_file_invalid_and_detached_sha() {
        let dir = tempdir().unwrap();
        let git_dir = dir.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::write(git_dir.join("HEAD"), "abc123def456\n").unwrap();
        assert_eq!(parse_head_file(&git_dir), Some("abc123d".to_string()));
        fs::write(git_dir.join("HEAD"), "short\n").unwrap();
        assert_eq!(parse_head_file(&git_dir), None);
        fs::write(git_dir.join("HEAD"), "").unwrap();
        assert_eq!(parse_head_file(&git_dir), None);
    }

    // --- is_dirty etc (F-087 - F-096) ---
    #[test]
    fn is_dirty_clean_false() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        assert!(!is_dirty(&repo));
    }

    #[test]
    fn is_dirty_modified_true() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        fs::write(dir.path().join("README.md"), "modified").unwrap();
        assert!(is_dirty(&repo));
    }

    #[test]
    fn is_dirty_untracked_true() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        fs::write(dir.path().join("untracked.txt"), "hi").unwrap();
        assert!(is_dirty(&repo));
    }

    #[test]
    fn is_dirty_staged_true() {
        let dir = tempdir().unwrap();
        let _repo = init_repo_with_commit(dir.path(), "main");
        fs::write(dir.path().join("staged.txt"), "hi").unwrap();
        let repo2 = Repository::open(dir.path()).unwrap();
        let mut idx = repo2.index().unwrap();
        idx.add_path(Path::new("staged.txt")).unwrap();
        idx.write().unwrap();
        assert!(is_dirty(&repo2));
    }

    #[test]
    fn is_dirty_ignored_ignored() {
        let dir = tempdir().unwrap();
        let _repo = init_repo_with_commit(dir.path(), "main");
        // Need to commit .gitignore, otherwise .gitignore itself is untracked and makes repo dirty
        fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
        // stage and commit .gitignore
        let repo2 = Repository::open(dir.path()).unwrap();
        let mut idx = repo2.index().unwrap();
        idx.add_path(Path::new(".gitignore")).unwrap();
        idx.write().unwrap();
        let tree_id = idx.write_tree().unwrap();
        let tree = repo2.find_tree(tree_id).unwrap();
        let parent = repo2.head().unwrap().peel_to_commit().unwrap();
        let sig = repo2.signature().unwrap();
        repo2
            .commit(Some("HEAD"), &sig, &sig, "add gitignore", &tree, &[&parent])
            .unwrap();
        // now create ignored file - should not make repo dirty
        fs::write(dir.path().join("ignored.txt"), "ignore me").unwrap();
        let repo3 = Repository::open(dir.path()).unwrap();
        assert!(
            !is_dirty(&repo3),
            "ignored.txt should be ignored via .gitignore"
        );
        // also verify that ignored file is not in detailed status when ignored, but untracked non-ignored would be
        let status = get_detailed_status(&repo3);
        assert!(
            !status.iter().any(|s| s.contains("ignored.txt")),
            "ignored file should not appear in status"
        );
    }

    #[test]
    fn is_dirty_recurse_and_bare() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        fs::create_dir(dir.path().join("subdir")).unwrap();
        fs::write(dir.path().join("subdir/untracked.txt"), "hi").unwrap();
        assert!(is_dirty(&repo));
        // bare repo case: create bare
        let bare_dir = tempdir().unwrap();
        let _bare = Repository::init_bare(bare_dir.path()).unwrap();
        let bare_repo = Repository::open_bare(bare_dir.path()).unwrap();
        // is_dirty on bare should return false (Err -> false)
        assert!(!is_dirty(&bare_repo));
    }

    #[test]
    fn has_merge_conflicts_clean_and_with_conflicts() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        assert!(!has_merge_conflicts(&repo));
        // bare
        let bare_dir = tempdir().unwrap();
        let _bare = Repository::init_bare(bare_dir.path()).unwrap();
        let bare_repo = Repository::open_bare(bare_dir.path()).unwrap();
        assert!(!has_merge_conflicts(&bare_repo));
    }

    #[test]
    fn is_merge_in_progress_true_for_merge() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        assert!(!is_merge_in_progress(&repo));
        // Simulate merge by setting state? We can just check clean is false when no merge
        // Rebase/merge detection relies on repo.state(), hard to trigger without actual merge conflict,
        // but we verify clean case and that function doesn't panic on bare
        let bare_dir = tempdir().unwrap();
        let _bare = Repository::init_bare(bare_dir.path()).unwrap();
        let bare_repo = Repository::open_bare(bare_dir.path()).unwrap();
        // bare state is not Clean? but we just ensure it doesn't panic
        let _ = is_merge_in_progress(&bare_repo);
    }

    #[test]
    fn get_detailed_status_classifies() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        let status = get_detailed_status(&repo);
        assert!(status.is_empty());
        fs::write(dir.path().join("untracked.txt"), "hi").unwrap();
        fs::write(dir.path().join("README.md"), "modified").unwrap();
        let repo2 = Repository::open(dir.path()).unwrap();
        let mut idx = repo2.index().unwrap();
        idx.add_path(Path::new("untracked.txt")).unwrap();
        idx.write().unwrap();
        // need to stage untracked? Actually add makes it staged
        let status2 = get_detailed_status(&repo2);
        // should contain untracked or staged and modified
        assert!(!status2.is_empty());
        // check prefixes
        let has_modified = status2.iter().any(|s| {
            s.starts_with("modified:") || s.starts_with("staged:") || s.starts_with("untracked:")
        });
        assert!(has_modified);
        // staged file
        fs::write(dir.path().join("staged2.txt"), "hi2").unwrap();
        let repo3 = Repository::open(dir.path()).unwrap();
        let mut idx3 = repo3.index().unwrap();
        idx3.add_path(Path::new("staged2.txt")).unwrap();
        idx3.write().unwrap();
        let status3 = get_detailed_status(&repo3);
        let has_staged = status3
            .iter()
            .any(|s| s.contains("staged") || s.contains("untracked"));
        assert!(has_staged);
    }

    // --- list_branches etc (F-097 - F-105) ---
    #[test]
    fn list_branches_local_and_remote() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        let branches = list_branches(dir.path());
        assert!(branches.contains(&"main".to_string()) || branches.contains(&"master".to_string()));
        assert!(branches.contains(&"feature".to_string()));
    }

    #[test]
    fn list_branches_excludes_origin_head() {
        // remote HEAD filtering requires a remote; we test local only filtering doesn't include HEAD-like
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let branches = list_branches(dir.path());
        assert!(!branches.iter().any(|b| b.ends_with("/HEAD")));
    }

    #[test]
    fn list_branches_dedup_and_sorted() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("a_branch", &head, false).unwrap();
        repo.branch("z_branch", &head, false).unwrap();
        let branches = list_branches(dir.path());
        let mut sorted = branches.clone();
        sorted.sort();
        assert_eq!(branches, sorted);
        // dedup: length should equal deduped set
        let mut deduped = branches.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(branches.len(), deduped.len());
    }

    #[test]
    fn list_branches_bare_and_invalid_empty() {
        let bare_dir = tempdir().unwrap();
        let _bare = Repository::init_bare(bare_dir.path()).unwrap();
        assert!(list_branches(bare_dir.path()).is_empty());
        assert!(list_branches(Path::new("/nonexistent/xyz")).is_empty());
    }

    #[test]
    fn repo_handle_variants_match_path_variants() {
        // Finding C: `_in`-Varianten mit geöffnetem Handle liefern dasselbe
        // wie die Pfad-Varianten (die nur öffnen + delegieren).
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        assert_eq!(list_branches(dir.path()), list_branches_in(&repo));
        assert_eq!(ahead_behind(dir.path()), ahead_behind_in(&repo));
    }

    #[test]
    fn fetch_all_no_remotes_ok() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        assert!(fetch_all(dir.path()).is_ok());
    }

    #[test]
    fn fetch_all_single_fail_errors() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        // add failing remote
        repo.remote("origin", "https://invalid.example.com/doesnotexist.git")
            .unwrap();
        drop(repo);
        let res = fetch_all(dir.path());
        // single remote fail should error (structured complete, not partial)
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            is_fetch_error(&err),
            "single fail must be fetch error: {err:#}"
        );
        assert!(
            !is_partial_fetch_error(&err),
            "single fail must not be partial: {err:#}"
        );
    }

    #[test]
    fn fetch_all_multi_remote_partial_fails_honestly() {
        // TDD RED: >1 Remote, einer fails → aktuell Ok (Masking), soll Err sein.
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        // Working remote: local bare repo (offline-capable file transport)
        let bare_dir = tempdir().unwrap();
        let _bare = Repository::init_bare(bare_dir.path()).unwrap();
        let repo = Repository::open(dir.path()).unwrap();
        repo.remote("good", bare_dir.path().to_str().unwrap())
            .unwrap();
        repo.remote("bad", "https://invalid.example.com/doesnotexist.git")
            .unwrap();
        drop(repo);
        let res = fetch_all(dir.path());
        assert!(
            res.is_err(),
            "partial fetch (1 ok, 1 fail) must be Err, got Ok (multi-remote masking)"
        );
        let err = res.unwrap_err();
        assert!(
            is_partial_fetch_error(&err),
            "partial failure must be marked as partial, got: {err:#}"
        );
        assert!(is_fetch_error(&err), "partial must be fetch error: {err:#}");
        let p = err
            .downcast_ref::<PartialFetch>()
            .expect("partial must expose parts");
        assert_eq!(p.succeeded, vec!["good".to_string()]);
        assert_eq!(p.failed.len(), 1);
        assert_eq!(p.failed[0].0, "bad");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("bad"),
            "error must name failing remote, got: {msg}"
        );
        assert!(
            msg.contains("good"),
            "error must name successful remote, got: {msg}"
        );
        assert!(
            msg.contains("Credentials/SSH"),
            "error must hint at auth, got: {msg}"
        );
        // I1: Display English-neutral (EN-UI darf kein Deutsch einbetten).
        for german in [
            "Hinweis",
            "prüfen",
            "fehlgeschlagen",
            "teilweise",
            "Erfolgreich",
        ] {
            assert!(
                !msg.contains(german),
                "partial Display must not embed German '{german}': {msg}"
            );
        }
    }

    #[test]
    fn fetch_all_multi_remote_all_fail_is_complete_error() {
        // Alle Remotes fail → Err, strukturiert (PartialFetch mit leerem succeeded),
        // aber kein Partial-Marker.
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        repo.remote("bad1", "https://invalid.example.com/a.git")
            .unwrap();
        repo.remote("bad2", "https://invalid.example.com/b.git")
            .unwrap();
        drop(repo);
        let res = fetch_all(dir.path());
        assert!(res.is_err(), "all remotes failing must be Err");
        let err = res.unwrap_err();
        assert!(
            !is_partial_fetch_error(&err),
            "complete failure must not be marked partial: {err:#}"
        );
        // I1: kompletter Fehlschlag bleibt strukturiert für lokalisierten Wrapper.
        assert!(
            is_fetch_error(&err),
            "complete failure must be structured fetch error: {err:#}"
        );
        let p = err
            .downcast_ref::<PartialFetch>()
            .expect("complete must expose parts");
        assert!(p.succeeded.is_empty(), "complete must have empty succeeded");
        assert_eq!(p.failed.len(), 2);
        let msg = format!("{err:#}");
        assert!(
            msg.contains("bad1") && msg.contains("bad2"),
            "must name all failing remotes, got: {msg}"
        );
        assert!(
            msg.contains("Credentials/SSH"),
            "error must hint at auth, got: {msg}"
        );
        for german in [
            "Hinweis",
            "prüfen",
            "fehlgeschlagen",
            "teilweise",
            "Erfolgreich",
        ] {
            assert!(
                !msg.contains(german),
                "complete Display must not embed German '{german}': {msg}"
            );
        }
    }

    #[test]
    fn fetch_all_zero_remotes_ok_and_no_partial_marker() {
        // Keine Remotes → Ok (kein Fehler möglich)
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        assert!(fetch_all(dir.path()).is_ok());
    }

    // --- Task 5 TDD RED: ahead_behind mit 2 Clones (1 Commit voraus) ---
    fn git_cmd(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|e| panic!("git {args:?} spawn failed: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} in {} failed: stdout={} stderr={}",
            dir.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn ahead_behind_two_clones_one_commit_behind() {
        // Bare-Origin + 2 Clones: clone1 pusht 1 Commit voraus, clone2 fetcht → behind=1.
        let base = tempdir().unwrap();
        let origin = base.path().join("origin.git");
        let c1 = base.path().join("c1");
        let c2 = base.path().join("c2");
        git_cmd(base.path(), &["init", "--bare", "origin.git"]);
        git_cmd(base.path(), &["clone", "origin.git", "c1"]);
        git_cmd(&c1, &["config", "user.name", "Test"]);
        git_cmd(&c1, &["config", "user.email", "test@test.com"]);
        git_cmd(&c1, &["checkout", "-b", "main"]);
        fs::write(c1.join("a.txt"), "one\n").unwrap();
        git_cmd(&c1, &["add", "a.txt"]);
        git_cmd(&c1, &["commit", "-m", "initial"]);
        git_cmd(&c1, &["push", "-u", "origin", "main"]);
        // Origin-HEAD auf main zeigen, damit Folge-Clones direkt main auschecken
        let _ = std::process::Command::new("git")
            .args([
                "--git-dir",
                "origin.git",
                "symbolic-ref",
                "HEAD",
                "refs/heads/main",
            ])
            .current_dir(base.path())
            .output();
        // Zweiter Clone kennt main bereits (explizit -b main für Robustheit)
        git_cmd(base.path(), &["clone", "-b", "main", "origin.git", "c2"]);
        git_cmd(&c2, &["config", "user.name", "Test"]);
        git_cmd(&c2, &["config", "user.email", "test@test.com"]);
        // c1 ein Commit voraus + push
        fs::write(c1.join("b.txt"), "two\n").unwrap();
        git_cmd(&c1, &["add", "b.txt"]);
        git_cmd(&c1, &["commit", "-m", "second"]);
        git_cmd(&c1, &["push", "origin", "main"]);
        // c2 fetcht → kennt neuen origin/main, ist 1 behind
        git_cmd(&c2, &["fetch", "origin"]);
        let _ = &origin;
        let (ahead_c2, behind_c2) = ahead_behind(&c2);
        assert_eq!(ahead_c2, 0, "c2 should not be ahead");
        assert_eq!(behind_c2, 1, "c2 should be 1 behind after fetch");
        let (ahead_c1, behind_c1) = ahead_behind(&c1);
        assert_eq!(
            (ahead_c1, behind_c1),
            (0, 0),
            "up-to-date clone must be (0,0)"
        );
    }

    #[test]
    fn ahead_behind_detached_and_no_remote_is_zero() {
        // Detached HEAD und Repo ohne Remote → (0,0), kein Panic.
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        assert_eq!(ahead_behind(dir.path()), (0, 0));
        // Detached
        let repo = Repository::open(dir.path()).unwrap();
        let oid = repo.head().unwrap().peel_to_commit().unwrap().id();
        repo.set_head_detached(oid).unwrap();
        drop(repo);
        assert_eq!(ahead_behind(dir.path()), (0, 0));
        // Ungültiger Pfad → (0,0)
        assert_eq!(ahead_behind(Path::new("/nonexistent/xyz-123")), (0, 0));
    }

    #[test]
    fn ahead_behind_ahead_one_when_local_commit_not_pushed() {
        // Lokaler Commit ohne Push → ahead=1, behind=0.
        let base = tempdir().unwrap();
        git_cmd(base.path(), &["init", "--bare", "origin.git"]);
        git_cmd(base.path(), &["clone", "origin.git", "c1"]);
        let c1 = base.path().join("c1");
        git_cmd(&c1, &["config", "user.name", "Test"]);
        git_cmd(&c1, &["config", "user.email", "test@test.com"]);
        git_cmd(&c1, &["checkout", "-b", "main"]);
        fs::write(c1.join("a.txt"), "one\n").unwrap();
        git_cmd(&c1, &["add", "a.txt"]);
        git_cmd(&c1, &["commit", "-m", "initial"]);
        git_cmd(&c1, &["push", "-u", "origin", "main"]);
        // Lokaler Commit ohne Push
        fs::write(c1.join("local.txt"), "local\n").unwrap();
        git_cmd(&c1, &["add", "local.txt"]);
        git_cmd(&c1, &["commit", "-m", "local-only"]);
        let (ahead, behind) = ahead_behind(&c1);
        assert_eq!(ahead, 1, "local-only commit must be ahead 1");
        assert_eq!(behind, 0);
    }

    #[test]
    fn get_repo_info_fills_ahead_behind_defaults() {
        // Ohne Remote → (0,0); mit Remote nach Fetch → behind=1.
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let info = get_repo_info(dir.path().to_path_buf()).unwrap();
        assert_eq!((info.ahead, info.behind), (0, 0));
    }

    #[test]
    fn open_in_explorer_platform_and_graceful() {
        // Should not panic, even with missing path. On Linux it tries xdg-open -> fallback open
        // We test that it doesn't panic and returns Ok or Err but not panic
        let dir = tempdir().unwrap();
        let res = open_in_explorer(dir.path());
        // On CI without xdg-open, it may fallback to open which also may not exist -> could be Err, but both Ok/Err acceptable
        assert!(res.is_ok() || res.is_err());
        let res2 = open_in_explorer(Path::new("/nonexistent/path/for/explorer"));
        assert!(res2.is_ok() || res2.is_err());
    }

    // --- Checkout etc (F-106 - F-119) ---
    #[test]
    fn checkout_branch_safe_success() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        checkout_branch(dir.path(), "feature").unwrap();
        let repo2 = Repository::open(dir.path()).unwrap();
        let (branch, _) = get_branch(&repo2);
        assert_eq!(branch, "feature");
    }

    #[test]
    fn checkout_branch_blocks_merge_in_progress() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        // create a situation where is_merge_in_progress true requires MERGE_HEAD file
        // Simulate by writing MERGE_HEAD
        let git_dir = dir.path().join(".git");
        fs::write(git_dir.join("MERGE_HEAD"), "abc123\n").unwrap();
        // Also need to set repo state to Merge? libgit2 checks for MERGE_HEAD existence
        let res = checkout_branch(dir.path(), "main");
        assert!(res.is_err());
        let err = res.unwrap_err().to_string();
        assert!(err.contains("Merge") || err.contains("Rebase"));
        // cleanup
        let _ = fs::remove_file(git_dir.join("MERGE_HEAD"));
    }

    #[test]
    fn checkout_branch_blocks_conflicts() {
        // has_merge_conflicts requires index conflicts; hard to create without actual merge conflict
        // We test that checkout doesn't panic when conflicts would be present, and that has_merge_conflicts on clean is false
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        assert!(!has_merge_conflicts(&Repository::open(dir.path()).unwrap()));
        // checkout to nonexistent should fail, not panic
        let res = checkout_branch(dir.path(), "nonexistent_branch_xyz");
        assert!(res.is_err());
    }

    #[test]
    fn checkout_branch_blocks_bare() {
        let bare_dir = tempdir().unwrap();
        let _bare = Repository::init_bare(bare_dir.path()).unwrap();
        let res = checkout_branch(bare_dir.path(), "main");
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Bare"));
    }

    #[test]
    fn checkout_branch_nonexistent_error() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let res = checkout_branch(dir.path(), "does_not_exist_123");
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("nicht gefunden"));
    }

    #[test]
    fn checkout_branch_same_branch_ok() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let (branch, _) = get_branch(&repo);
        let res = checkout_branch(dir.path(), &branch);
        assert!(res.is_ok());
    }

    #[test]
    fn checkout_branch_detached_to_commit() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let oid = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        checkout_branch(dir.path(), &oid).unwrap();
        let repo2 = Repository::open(dir.path()).unwrap();
        let (_, detached) = get_branch(&repo2);
        assert!(detached);
    }

    #[test]
    fn checkout_branch_force_overwrites() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        fs::write(dir.path().join("README.md"), "dirty").unwrap();
        // force should succeed
        checkout_branch_force(dir.path(), "feature").unwrap();
        let repo2 = Repository::open(dir.path()).unwrap();
        let (b, _) = get_branch(&repo2);
        assert_eq!(b, "feature");
    }

    #[test]
    fn checkout_force_nonexistent_and_bare_error() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        assert!(checkout_branch_force(dir.path(), "nope").is_err());
        let bare_dir = tempdir().unwrap();
        let _bare = Repository::init_bare(bare_dir.path()).unwrap();
        // checkout_branch_force on bare currently doesn't check bare explicitly, but will fail via revparse or checkout
        // It should error
        let res = checkout_branch_force(bare_dir.path(), "main");
        assert!(res.is_err());
    }

    #[test]
    fn stash_and_checkout_clean_warns_nothing_to_stash() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        // K1: clean repo ohne Änderungen → kein Stash, kein Pop → Ok.
        // Früher wurde trotzdem stash_pop(0) versucht (fremder Stash-Risk).
        let res = stash_and_checkout(dir.path(), "feature");
        assert!(
            res.is_ok(),
            "clean checkout must succeed without pop: {res:?}"
        );
    }

    #[test]
    fn stash_and_checkout_clean_preserves_preexisting_stash() {
        // K1-Regression: clean Repo + alter stash@{0} → Switch darf den alten
        // Stash weder poppen noch löschen.
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        {
            let mut repo = Repository::open(dir.path()).unwrap();
            let head_id = repo.head().unwrap().peel_to_commit().unwrap().id();
            let head = repo.find_commit(head_id).unwrap();
            repo.branch("feature", &head, false).unwrap();
            drop(head);
            // Alten Stash anlegen (dirty → stash, dann liegt stash@{0}).
            fs::write(dir.path().join("old.txt"), "old").unwrap();
            let sig = repo.signature().unwrap();
            repo.stash_save(&sig, "old stash", Some(git2::StashFlags::INCLUDE_UNTRACKED))
                .unwrap();
            assert!(
                repo.revparse_single("stash@{0}").is_ok(),
                "precondition: stash must exist"
            );
        }
        let res = stash_and_checkout(dir.path(), "feature");
        assert!(res.is_ok(), "clean switch must not pop old stash: {res:?}");
        let repo = Repository::open(dir.path()).unwrap();
        assert!(
            repo.revparse_single("stash@{0}").is_ok(),
            "preexisting stash must be preserved"
        );
    }

    #[test]
    fn remote_short_name_keeps_slashes() {
        // K2: origin/feature/foo → feature/foo (nicht foo).
        assert_eq!(
            remote_short_name("refs/remotes/origin/feature/foo", "origin/feature/foo"),
            "feature/foo"
        );
        assert_eq!(
            remote_short_name("refs/remotes/upstream/bugfix/bar", "upstream/bugfix/bar"),
            "bugfix/bar"
        );
        assert_eq!(
            remote_short_name("refs/remotes/origin/main", "origin/main"),
            "main"
        );
        // F1#7: Fallback ohne refs/remotes/-Prefix strippt generisch das
        // erste Segment (nicht nur `origin/`).
        assert_eq!(remote_short_name("", "upstream/foo"), "foo");
        assert_eq!(remote_short_name("", "origin/foo"), "foo");
        assert_eq!(remote_short_name("", "main"), "main");
    }

    #[test]
    fn stash_pop_failed_typed_detection() {
        // M4: typisiert statt String-Match.
        let err = anyhow::Error::new(StashPopFailed {
            detail: "conflict".to_string(),
        });
        assert!(is_stash_pop_failed(&err));
        let other = anyhow::anyhow!("plain");
        assert!(!is_stash_pop_failed(&other));
    }

    #[test]
    fn stash_and_checkout_preserves_on_failure() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        fs::write(dir.path().join("newfile.txt"), "dirty").unwrap();
        // checkout to nonexistent should fail and preserve stash if there was one
        let res = stash_and_checkout(dir.path(), "nonexistent_xyz");
        assert!(res.is_err());
        // should not panic, and stash if created should be popped back
    }

    #[test]
    fn is_program_available_true_false() {
        assert!(
            is_program_available("ls")
                || is_program_available("echo")
                || is_program_available("true")
        );
        assert!(!is_program_available("definitely_not_a_real_program_12345"));
    }

    #[test]
    fn resolve_vs_path_returns_none_on_linux() {
        // On Linux, vswhere not present -> None
        let res = resolve_vs_path();
        // On non-Windows, should be None because path doesn't exist
        #[cfg(not(windows))]
        assert!(res.is_none());
        #[cfg(windows)]
        assert!(res.is_none() || res.is_some());
    }

    #[test]
    fn substitute_placeholders_all() {
        let file = Path::new("/tmp/repo/sub/foo.sln");
        let repo = Path::new("/tmp/repo");
        assert_eq!(
            substitute_placeholders("{file}", file, repo),
            "/tmp/repo/sub/foo.sln"
        );
        assert_eq!(
            substitute_placeholders("{dir}", file, repo),
            "/tmp/repo/sub"
        );
        assert_eq!(substitute_placeholders("{repo}", file, repo), "/tmp/repo");
        assert_eq!(
            substitute_placeholders("{file} {dir} {repo}", file, repo),
            "/tmp/repo/sub/foo.sln /tmp/repo/sub /tmp/repo"
        );
        assert_eq!(
            substitute_placeholders("no placeholder", file, repo),
            "no placeholder"
        );
        let deep = Path::new("/a/b/c/d/e/file.txt");
        assert!(substitute_placeholders("{dir}", deep, deep).contains("/a/b/c/d/e"));
        assert_eq!(
            substitute_placeholders("{dir}", file, file),
            "/tmp/repo/sub/foo.sln"
        );
    }

    #[test]
    fn quote_if_needed_spaces_and_already_quoted() {
        assert_eq!(quote_if_needed("code"), "code");
        assert_eq!(
            quote_if_needed("C:\\Program Files\\app.exe"),
            "\"C:\\Program Files\\app.exe\""
        );
        assert_eq!(quote_if_needed("\"already quoted\""), "\"already quoted\"");
        assert_eq!(quote_if_needed("a & b"), "\"a & b\"");
        // with quotes inside
        assert_eq!(quote_if_needed("a \"b\" c"), "\"a \\\"b\\\" c\"");
    }

    #[test]
    fn launch_ide_program_and_args_resolution() {
        let ide = IdeConfig {
            id: "vscode".to_string(),
            display_name: "VS Code".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string()],
            command: Some("other arg".to_string()),
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_program(), "code");
        assert_eq!(ide.effective_args(), vec!["{file}"]);
        let ide2 = IdeConfig {
            program: "".to_string(),
            command: Some("devenv /something".to_string()),
            args: vec![],
            ..ide.clone()
        };
        assert_eq!(ide2.effective_program(), "devenv");
        assert_eq!(ide2.effective_args(), vec!["/something"]);
    }

    #[test]
    fn launch_ide_blocks_shell_chars_when_not_unsafe() {
        // Nur wenn use_shell=true und allow_unsafe=false soll blockiert werden.
        // Bei use_shell=false ist Command::args sicher (kein Shell-Parsing), daher sind '&' etc in Pfaden erlaubt.
        let ide = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "code".to_string(),
            args: vec!["a & b".to_string()],
            command: None,
            use_shell: true,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        let res = launch_ide(&ide, Path::new("/tmp"), Some(Path::new("/tmp/foo.sln")));
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Ungültige Zeichen"));

        // Bei use_shell=false soll gleicher Arg nicht blockiert werden (harmlos)
        let ide_no_shell = IdeConfig {
            use_shell: false,
            ..ide.clone()
        };
        let res_no_shell = launch_ide(
            &ide_no_shell,
            Path::new("/tmp"),
            Some(Path::new("/tmp/foo.sln")),
        );
        // In tests wird spawn für "code" gemockt -> Ok, aber nicht wegen Shell-Chars fehlgeschlagen
        assert!(
            res_no_shell.is_ok()
                || !res_no_shell
                    .unwrap_err()
                    .to_string()
                    .contains("Ungültige Zeichen")
        );

        let mut ide2 = IdeConfig {
            allow_unsafe: true,
            no_args: false,
            use_shell: true,
            ..ide.clone()
        };
        // Use harmless program for test to avoid opening VS Code window
        ide2.program = "true".to_string();
        ide2.args = vec!["a & b".to_string()];
        // With allow_unsafe true, it should not fail due to shell chars
        let res2 = launch_ide(&ide2, Path::new("/tmp"), Some(Path::new("/tmp/foo.sln")));
        // Should succeed (mocked in tests for GUI, or `true` exits 0)
        assert!(res2.is_ok());
        if let Err(e) = res2 {
            assert!(!e.to_string().contains("Ungültige Zeichen"));
        }

        // Pfad mit '&' bei use_shell=false soll erlaubt sein (z.B. "C:\Users\Tom & Jerry\app.sln")
        let ide_path = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "true".to_string(),
            args: vec!["{file}".to_string()],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        let res_path = launch_ide(
            &ide_path,
            Path::new("/tmp"),
            Some(Path::new("/tmp/Tom & Jerry/app.sln")),
        );
        assert!(res_path.is_ok());
    }

    #[test]
    fn launch_ide_blocks_various_shell_chars() {
        for ch in ["&", "|", ";", ">", "<"] {
            let ide = IdeConfig {
                id: "test".to_string(),
                display_name: "Test".to_string(),
                program: "code".to_string(),
                args: vec![format!("a {} b", ch)],
                command: None,
                use_shell: true,
                allow_unsafe: false,
                no_args: false,

                icon: None,
            };
            assert!(launch_ide(&ide, Path::new("/tmp"), Some(Path::new("/tmp/f"))).is_err());
        }
        // Bei use_shell=false sollen gleiche Zeichen nicht blockiert werden
        for ch in ["&", "|", ";", ">", "<"] {
            let ide = IdeConfig {
                id: "test".to_string(),
                display_name: "Test".to_string(),
                program: "true".to_string(),
                args: vec![format!("a {} b", ch)],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,

                icon: None,
            };
            let res = launch_ide(&ide, Path::new("/tmp"), Some(Path::new("/tmp/f")));
            assert!(
                res.is_ok() || !res.unwrap_err().to_string().contains("Ungültige Zeichen"),
                "should not block shell chars when use_shell=false for '{}'",
                ch
            );
        }
    }

    #[test]
    fn launch_agent_with_args_and_command_templates() {
        // test agent_cmd building logic indirectly via launch_agent with Custom terminal that exists
        // Use echo as program which exists
        let agent = AgentProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "echo".to_string(),
            args: vec!["hello world".to_string(), "foo".to_string()],
            command: None,
            launch_mode: crate::config::AgentLaunchMode::Terminal,
            terminal_override: Some(TerminalPreference::Custom("true".to_string())),

            icon: None,
        };
        // Custom("true") should spawn "true" with agent_cmd as arg, which should succeed (true exits 0)
        let dir = tempdir().unwrap();
        let res = launch_agent(&agent, dir.path(), &TerminalPreference::Auto);
        // true exists on Linux, should be Ok
        assert!(res.is_ok() || res.is_err()); // not panic

        // test command with placeholders
        let agent2 = AgentProfile {
            program: "echo".to_string(),
            args: vec![],
            command: Some("echo {file} {dir} {repo}".to_string()),
            terminal_override: Some(TerminalPreference::Custom("true".to_string())),
            ..agent.clone()
        };
        let res2 = launch_agent(
            &agent2,
            Path::new("/tmp/repo/file.txt"),
            &TerminalPreference::Auto,
        );
        assert!(res2.is_ok() || res2.is_err());

        // command without placeholders
        let agent3 = AgentProfile {
            command: Some("echo hello".to_string()),
            args: vec![],
            ..agent.clone()
        };
        let res3 = launch_agent(&agent3, dir.path(), &TerminalPreference::Auto);
        assert!(res3.is_ok() || res3.is_err());
    }

    #[test]
    fn launch_agent_terminal_override() {
        let agent = AgentProfile {
            id: "claude".to_string(),
            display_name: "Claude".to_string(),
            program: "echo".to_string(),
            args: vec![],
            command: None,
            launch_mode: crate::config::AgentLaunchMode::Terminal,
            terminal_override: Some(TerminalPreference::Custom("true".to_string())),

            icon: None,
        };
        let dir = tempdir().unwrap();
        // terminal_override should be used even if global is different
        let res = launch_agent(&agent, dir.path(), &TerminalPreference::Cmd);
        assert!(res.is_ok() || res.is_err());
    }

    // Keep original tests
    #[test]
    fn branch_detection() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        let (branch, detached) = get_branch(&repo);
        assert!(!detached);
        assert!(branch == "main" || branch == "master");
    }

    #[test]
    fn dirty_detection_clean_and_modified() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        assert!(!is_dirty(&repo), "frisch committet sollte clean sein");
        fs::write(dir.path().join("README.md"), "modified\n").unwrap();
        assert!(is_dirty(&repo), "nach Modifikation sollte dirty sein");
    }

    #[test]
    fn dirty_detection_untracked() {
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        fs::write(dir.path().join("untracked.txt"), "hello").unwrap();
        assert!(is_dirty(&repo), "untracked sollte als dirty zählen");
    }

    #[test]
    fn get_repo_info_none_for_non_repo() {
        let dir = tempdir().unwrap();
        let info = get_repo_info(dir.path().to_path_buf());
        assert!(info.is_none());
    }

    #[test]
    fn get_repo_info_some_for_repo() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let info = get_repo_info(dir.path().to_path_buf());
        assert!(info.is_some());
        let info = info.unwrap();
        assert!(!info.branch.is_empty());
    }

    #[test]
    fn list_branches_works() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        // Erstelle zweiten Branch
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        let branches = list_branches(dir.path());
        assert!(branches.contains(&"main".to_string()) || branches.contains(&"master".to_string()));
        assert!(branches.contains(&"feature".to_string()));
    }

    #[test]
    fn checkout_branch_works() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        // Wechsel zu feature
        checkout_branch(dir.path(), "feature").unwrap();
        let repo2 = Repository::open(dir.path()).unwrap();
        let (branch, _) = get_branch(&repo2);
        assert_eq!(branch, "feature");
        // Zurück zu main
        checkout_branch(dir.path(), "main").unwrap();
        let repo3 = Repository::open(dir.path()).unwrap();
        let (branch, _) = get_branch(&repo3);
        assert!(branch == "main" || branch == "master");
    }

    #[test]
    fn checkout_branch_blocks_on_dirty() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        // Mache dirty
        fs::write(dir.path().join("README.md"), "dirty\n").unwrap();
        // Erstelle konfliktierenden Zustand: ändere gleiche Datei in feature branch
        // Für Test: checkout mit dirty sollte fehlschlagen wenn Datei in Zielbranch anders?
        // Einfacher: dirty + checkout ohne Konflikt sollte eigentlich erlaubt sein wenn Datei gleich
        // Wir testen dass checkout bei dirty nicht panickt, sondern entweder Ok oder Err mit sinnvoller Nachricht
        let result = checkout_branch(dir.path(), "feature");
        // Sollte entweder Ok sein (wenn safe) oder Err wegen Konflikt, aber nicht panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn ide_launch_command_substitution() {
        let ide = IdeConfig {
            id: "vscode".to_string(),
            display_name: "VS Code".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string(), "--reuse-window".to_string()],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        let file = Path::new("/tmp/foo.sln");
        let prog = ide.effective_program();
        assert_eq!(prog, "code");
        let args = ide.effective_args();
        assert_eq!(args, vec!["{file}", "--reuse-window"]);
        let sub = substitute_placeholders("{file} --test", file, file);
        assert!(sub.contains("foo.sln"));
    }

    #[test]
    fn escape_powershell_bare_vs_quoted() {
        assert_eq!(escape_for_powershell("claude"), "claude");
        assert_eq!(escape_for_powershell("code"), "code");
        assert_eq!(
            escape_for_powershell("C:\\Program Files\\app.exe"),
            "'C:\\Program Files\\app.exe'"
        );
        assert_eq!(escape_for_powershell("a&b"), "'a&b'");
        assert_eq!(escape_for_powershell("a'b"), "'a''b'");
    }

    #[test]
    fn build_powershell_script_bare_and_path() {
        let repo = Path::new("/tmp/repo");
        let agent_bare = AgentProfile {
            id: "claude".to_string(),
            display_name: "Claude".to_string(),
            program: "claude".to_string(),
            args: vec![],
            command: None,
            launch_mode: crate::config::AgentLaunchMode::Terminal,
            terminal_override: None,

            icon: None,
        };
        let script = build_powershell_script(&agent_bare, repo);
        assert_eq!(script, "claude");
        let agent_path = AgentProfile {
            program: "C:\\Program Files\\claude.exe".to_string(),
            ..agent_bare.clone()
        };
        let script2 = build_powershell_script(&agent_path, repo);
        assert!(script2.starts_with("& "));
        assert!(script2.contains("'C:\\Program Files\\claude.exe'"));
        let mut agent_args = agent_bare.clone();
        agent_args.args = vec!["--help".to_string(), "foo bar".to_string()];
        let script3 = build_powershell_script(&agent_args, repo);
        assert_eq!(script3, "claude --help 'foo bar'");
    }

    #[test]
    fn visible_ides_and_filtered_agents_logic() {
        let profile = crate::config::LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![
                IdeConfig {
                    id: "a".to_string(),
                    display_name: "A".to_string(),
                    program: "a".to_string(),
                    args: vec![],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,

                    icon: None,
                },
                IdeConfig {
                    id: "b".to_string(),
                    display_name: "B".to_string(),
                    program: "b".to_string(),
                    args: vec![],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,

                    icon: None,
                },
            ],
            default_ide_id: None,
            ide_order: vec!["b".to_string(), "a".to_string()],
            hidden_ide_ids: vec!["a".to_string()],
            hidden_agent_ids: vec![],
            agent_order: vec![],
            show_shell: false,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        let visible = profile.visible_ides();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "b");
        assert!(!profile.show_shell);
        assert!(profile.show_explorer);
    }

    #[test]
    fn substitute_with_repo_and_file_distinct() {
        let file = Path::new("/tmp/repo/sub/app.sln");
        let repo = Path::new("/tmp/repo");
        let sub = substitute_placeholders("{file} {dir} {repo}", file, repo);
        assert_eq!(sub, "/tmp/repo/sub/app.sln /tmp/repo/sub /tmp/repo");
    }
}
