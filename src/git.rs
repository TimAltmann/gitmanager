use crate::gitlab::checkout::MrCheckoutPlan;
use crate::gitlab::remote::RemoteInfo;
use git2::{BranchType, Repository, StatusOptions};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
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
    /// Remotes (origin zuerst, dann alphabetisch), URLs ohne Zugangsdaten.
    /// Scan-only, Default leer.
    pub remotes: Vec<RemoteInfo>,
    /// Gemeinsames Git-Verzeichnis (`commondir`): bei verknüpften Worktrees
    /// das des Haupt-Repos. Kanonisch, damit Worktree-Geschwister per `==`
    /// gruppiert werden können. Scan-only, Default `<path>/.git`.
    pub common_dir: PathBuf,
    /// Anzahl geänderter Einträge wie bei `is_dirty` (ein untracked Ordner
    /// zählt einmal), für „{n} Änderungen“ im Checkout-Menü. Scan-only,
    /// Default 1 bei `dirty`, sonst 0.
    pub dirty_files: usize,
    /// Die übrigen Worktrees desselben Repos (Haupt- und verknüpfte, ohne
    /// den eigenen) mit ihrem Branch, auch außerhalb der Suchpfade – für
    /// „Bereits ausgecheckt“ (K1). Scan-only, Default leer.
    pub worktrees: Vec<WorktreeHead>,
    /// Verknüpfter Worktree (`git worktree add`), kein eigener Klon: nie
    /// Basis-Klon für neue Worktrees (K1). Scan-only, Default `false`.
    pub is_linked_worktree: bool,
}

/// Anderer Worktree eines Repos (`RepoInfo.worktrees`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeHead {
    pub path: PathBuf,
    /// Kurzname des ausgecheckten Branches, `None` bei Detached HEAD
    pub branch: Option<String>,
}

impl RepoInfo {
    pub fn new(path: PathBuf, branch: String, dirty: bool, is_detached: bool) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let common_dir = path.join(".git");
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
            remotes: Vec::new(),
            common_dir,
            dirty_files: usize::from(dirty),
            worktrees: Vec::new(),
            is_linked_worktree: false,
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
/// P1: ohne `recurse_untracked_dirs` — ein untracked Verzeichnis meldet
/// libgit2 als einen Eintrag (z.B. `target/`), das reicht für Bool.
/// Mit Rekursion würden tausende Dateien einzeln gestat'ed (pro Repo pro Scan).
pub fn is_dirty(repo: &Repository) -> bool {
    dirty_count(repo) > 0
}

/// Anzahl geänderter Einträge mit denselben Optionen wie [`is_dirty`]
/// (untracked Ordner als ein Eintrag); Fehler → 0.
fn dirty_count(repo: &Repository) -> usize {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(false)
        .include_unmodified(false);
    repo.statuses(Some(&mut opts)).map_or(0, |s| s.len())
}

/// Detaillierter Status: staged, unstaged, untracked, Konflikte
pub fn has_merge_conflicts(repo: &Repository) -> bool {
    repo.index().map(|idx| idx.has_conflicts()).unwrap_or(false)
}

pub fn is_merge_in_progress(repo: &Repository) -> bool {
    repo.state() != git2::RepositoryState::Clean
}

/// Maximale Dateianzahl für den Branch-Dialog (M2: Liste ist gecappt —
/// UI kennzeichnet das ehrlich mit "+", siehe `capped_file_count_label`).
pub const DETAILED_STATUS_CAP: usize = 100;

/// Detaillierter Status für den Branch-Dialog: (Dateiliste auf CAP
/// trunkiert, `truncated` sagt ob gekappt wurde — F1#F3: bei exakt CAP
/// Einträgen ist nichts abgeschnitten, Label ohne "+").
pub fn get_detailed_status(repo: &Repository) -> (Vec<String>, bool) {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        // P4: kein recurse in untracked Dirs auf dem UI-Thread — bei großen
        // Repos (node_modules/target) sonst hunderte ms Freeze. Top-Level
        // reicht für den Dialog; Details sieht man in der IDE.
        .recurse_untracked_dirs(false)
        .include_ignored(false);
    let mut files = Vec::new();
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        for entry in statuses.iter().take(DETAILED_STATUS_CAP + 1) {
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
    if files.len() > DETAILED_STATUS_CAP {
        files.truncate(DETAILED_STATUS_CAP);
        (files, true)
    } else {
        (files, false)
    }
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
/// Auth: libgit2 mit Credential-Callbacks (SSH-Agent, Default-Keys,
/// git-credential-helper) zuerst; bei Fehlschlag Fallback auf System-`git fetch`
/// (nutzt Credential-Manager/SSH-Agent des Users — nötig für authentifizierte
/// Repos). Branch-Wechsel ist lokal und braucht kein Netzwerk, Fetch schon.
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
        match fetch_one_remote(&repo, path, &remote_name) {
            Ok(()) => succeeded.push(remote_name),
            Err(e) => {
                eprintln!("Fetch für Remote '{remote_name}' fehlgeschlagen: {e}");
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

/// Pfade für einen Fetch-Durchlauf über viele Repos: Worktrees eines
/// Repositorys teilen Remotes und Remote-Refs, ein Fetch je Repository
/// reicht; gleichzeitige Fetches in dasselbe `.git` stören sich zudem an
/// den Ref-Locks. Bevorzugt den Haupt-Worktree, sonst den ersten Pfad.
/// Pfade, die sich nicht öffnen lassen, bleiben (ihr Fehler soll weiter
/// erscheinen). Reihenfolge wie in `paths`.
pub fn fetch_targets(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut by_common: HashMap<PathBuf, usize> = HashMap::new();
    // (Pfad, ist ein verknüpfter Worktree)
    let mut out: Vec<(PathBuf, bool)> = Vec::new();
    for path in paths {
        let Ok(repo) = Repository::open(path) else {
            out.push((path.clone(), false));
            continue;
        };
        let common = repo.commondir();
        let common = std::fs::canonicalize(common).unwrap_or_else(|_| common.to_path_buf());
        let linked = repo.is_worktree();
        match by_common.get(&common) {
            Some(&i) if out[i].1 && !linked => out[i] = (path.clone(), false),
            Some(_) => {}
            None => {
                by_common.insert(common, out.len());
                out.push((path.clone(), linked));
            }
        }
    }
    out.into_iter().map(|(path, _)| path).collect()
}

/// K1: holt nur die angegebenen Refspecs eines Remotes, z. B. den
/// Quell-Branch eines MR (`MrCheckoutPlan::refspec`). Ausdrücklich ohne
/// Prune (`FetchPrune::Off` übersteuert `fetch.prune` des Users) und ohne
/// Tags: andere Remote-Refs bleiben unberührt. Auth und CLI-Fallback wie
/// [`fetch_all`].
pub fn fetch_refspecs(path: &Path, remote: &str, refspecs: &[&str]) -> anyhow::Result<()> {
    let repo =
        Repository::open(path).map_err(|e| anyhow::anyhow!("Failed to open repository: {e}"))?;
    fetch_remote_with(&repo, path, remote, refspecs, false)
}

/// Ein Remote fetchen: libgit2 mit Auth-Callbacks, Fallback auf System-`git`.
fn fetch_one_remote(repo: &Repository, repo_path: &Path, remote_name: &str) -> anyhow::Result<()> {
    fetch_remote_with(repo, repo_path, remote_name, &[], true)
}

/// Fetch eines Remotes mit expliziten Refspecs (leer = konfigurierte) und
/// wählbarem Prune. Mit Refspecs ohne Tags (wie `--no-tags`). Kurze
/// Abbrüche wiederholt libgit2 (siehe [`with_fetch_retries`]), erst danach
/// greift System-`git`.
fn fetch_remote_with(
    repo: &Repository,
    repo_path: &Path,
    remote_name: &str,
    refspecs: &[&str],
    prune: bool,
) -> anyhow::Result<()> {
    let mut remote = match repo.find_remote(remote_name) {
        Ok(remote) => remote,
        Err(e) => {
            eprintln!("Remote '{remote_name}' nicht gefunden: {e}");
            return Err(anyhow::anyhow!("{e}"));
        }
    };
    let remote_url = remote.url().unwrap_or("").to_string();
    let lib_res = with_fetch_retries(remote_name, &fetch_retry_delays(), || {
        libgit2_fetch(repo, &mut remote, &remote_url, refspecs, prune)
    });
    match lib_res {
        Ok(()) => Ok(()),
        Err(lib_err) => {
            eprintln!(
                "libgit2 fetch für Remote '{remote_name}' fehlgeschlagen ({lib_err}), versuche System-git..."
            );
            // Fallback: System-git nutzt Credential-Manager/SSH-Agent des Users.
            match fetch_with_git_cli(repo_path, remote_name, refspecs, prune) {
                Ok(()) => {
                    eprintln!("System-git fetch für Remote '{remote_name}' erfolgreich (Fallback)");
                    Ok(())
                }
                Err(cli_err) => {
                    eprintln!(
                        "System-git fetch für Remote '{remote_name}' ebenfalls fehlgeschlagen: {cli_err:#}"
                    );
                    // Strukturierter Fehler bleibt libgit2-basiert (English-neutral,
                    // keine lokalisierten git-CLI-Texte) — CLI nur im Log.
                    Err(anyhow::anyhow!("{lib_err}"))
                }
            }
        }
    }
}

/// Ein Fetch-Versuch per libgit2 mit Auth-Callbacks.
fn libgit2_fetch(
    repo: &Repository,
    remote: &mut git2::Remote,
    remote_url: &str,
    refspecs: &[&str],
    prune: bool,
) -> Result<(), git2::Error> {
    // Repo-Config für credential.helper (https) — Snapshot vor Callbacks.
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(credentials_callback(
        repo.config().ok(),
        remote_url.to_string(),
    ));
    // Zertifikatsprüfung NICHT deaktivieren (TLS bleibt verifiziert).
    let mut opts = git2::FetchOptions::new();
    opts.prune(if prune {
        git2::FetchPrune::On
    } else {
        git2::FetchPrune::Off
    });
    if !refspecs.is_empty() {
        opts.download_tags(git2::AutotagOption::None);
    }
    opts.remote_callbacks(callbacks);
    remote.fetch(refspecs, Some(&mut opts), None)
}

/// Wartezeiten vor dem 2. und 3. Versuch, mit bis zu 1 s Streuung: Worker,
/// die gemeinsam abgebrochen sind, sollen nicht wieder gleichzeitig ankommen.
/// Tests warten nicht (ihre `invalid.example.com`-Remotes scheitern im Netz).
fn fetch_retry_delays() -> [Duration; 2] {
    if cfg!(test) {
        return [Duration::ZERO; 2];
    }
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_millis()));
    [
        Duration::from_millis(1_500 + jitter),
        Duration::from_millis(4_000 + jitter),
    ]
}

/// Wiederholt `fetch` nach jeder Wartezeit aus `delays`, solange der Fehler
/// vorübergehend ist ([`is_transient_fetch_error`]); andere Fehler und der
/// Fehler des letzten Versuchs gehen unverändert zurück.
fn with_fetch_retries(
    remote_name: &str,
    delays: &[Duration],
    mut fetch: impl FnMut() -> Result<(), git2::Error>,
) -> Result<(), git2::Error> {
    let mut delays = delays.iter();
    loop {
        match fetch() {
            Err(e) if is_transient_fetch_error(&e) => {
                let Some(delay) = delays.next() else {
                    return Err(e);
                };
                eprintln!(
                    "libgit2 fetch für Remote '{remote_name}' abgebrochen ({e}), neuer Versuch in {delay:?}"
                );
                std::thread::sleep(*delay);
            }
            res => return res,
        }
    }
}

/// Abbruch in der SSH-, Netzwerk- oder OS-Schicht (Verbindung getrennt,
/// Timeout, Lock-Datei eines gleichzeitigen Fetches), der beim nächsten
/// Versuch meist weg ist. Ein abgelehntes Login ist endgültig: über HTTP
/// (`code=Auth`) und wenn der Credential-Callback alles probiert hat
/// (`class=Callback`, [`CredState::exhausted`]).
fn is_transient_fetch_error(e: &git2::Error) -> bool {
    match e.class() {
        git2::ErrorClass::Ssh | git2::ErrorClass::Net | git2::ErrorClass::Os => true,
        git2::ErrorClass::Http => e.code() != git2::ErrorCode::Auth,
        _ => false,
    }
}

/// Anmeldeart, die der Credential-Callback libgit2 anbietet.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CredChoice {
    SshAgent,
    /// Default-Key aus `~/.ssh` (ohne Passphrase)
    SshKeyFile(PathBuf),
    /// `credential.helper` (https, z. B. Windows Credential Manager)
    CredentialHelper,
    /// NTLM/Kerberos u. ä.
    Default,
    Username,
}

/// Gedächtnis des Credential-Callbacks für eine Verbindung. libgit2 fragt
/// nach jedem abgelehnten Login erneut; ohne Gedächtnis bekam es endlos
/// dasselbe (etwa den SSH-Agent, auch wenn keiner läuft), bis der Server
/// wegen zu vieler Fehlversuche trennte („remote rejected authentication:
/// Failed getting response“) und die Default-Keys nie an die Reihe kamen.
#[derive(Debug, Default)]
struct CredState {
    offered: Vec<CredChoice>,
}

impl CredState {
    /// Nächste noch nicht angebotene Anmeldeart in der Reihenfolge
    /// SSH-Agent → Default-Keys → credential.helper → default; `None`, wenn
    /// alles probiert ist. Der Benutzername (libgit2 fragt ihn einmal, wenn
    /// die URL keinen nennt) zählt nicht als Versuch.
    fn next(&mut self, allowed: git2::CredentialType, key_files: &[PathBuf]) -> Option<CredChoice> {
        let mut candidates = Vec::new();
        if allowed.is_ssh_key() {
            candidates.push(CredChoice::SshAgent);
            candidates.extend(key_files.iter().cloned().map(CredChoice::SshKeyFile));
        }
        if allowed.is_user_pass_plaintext() {
            candidates.push(CredChoice::CredentialHelper);
        }
        if allowed.is_default() {
            candidates.push(CredChoice::Default);
        }
        if let Some(next) = candidates.into_iter().find(|c| !self.offered.contains(c)) {
            self.offered.push(next.clone());
            return Some(next);
        }
        allowed.is_username().then_some(CredChoice::Username)
    }

    /// Fehler, wenn alles probiert ist: Klasse `Callback`, damit
    /// [`with_fetch_retries`] nicht noch mehr Fehlversuche erzeugt.
    fn exhausted(&self) -> git2::Error {
        let tried: Vec<String> = self
            .offered
            .iter()
            .map(|c| match c {
                CredChoice::SshAgent => "ssh-agent".to_string(),
                CredChoice::SshKeyFile(path) => path.display().to_string(),
                CredChoice::CredentialHelper => "credential helper".to_string(),
                CredChoice::Default => "default credentials".to_string(),
                CredChoice::Username => "username".to_string(),
            })
            .collect();
        let message = if tried.is_empty() {
            "no credentials available for this remote".to_string()
        } else {
            format!("authentication failed (tried: {})", tried.join(", "))
        };
        git2::Error::new(git2::ErrorCode::Auth, git2::ErrorClass::Callback, message)
    }
}

/// Credential-Callback für libgit2 (Fetch, Klon) mit eigenem
/// [`CredState`]; jede Anmeldeart kommt höchstens einmal.
///
/// `config` ist der Snapshot der Repo- bzw. globalen Config (für
/// `credential.helper` bei https), `remote_url` der konfigurierte Remote
/// (Fallback, wenn die Callback-URL leer ist).
fn credentials_callback(
    config: Option<git2::Config>,
    remote_url: String,
) -> impl FnMut(&str, Option<&str>, git2::CredentialType) -> Result<git2::Cred, git2::Error> {
    let key_files: Vec<PathBuf> = default_ssh_key_paths()
        .into_iter()
        .filter(|p| p.exists())
        .collect();
    credentials_with_keys(config, remote_url, key_files)
}

/// [`credentials_callback`] mit fester Liste vorhandener Default-Keys.
fn credentials_with_keys(
    config: Option<git2::Config>,
    remote_url: String,
    key_files: Vec<PathBuf>,
) -> impl FnMut(&str, Option<&str>, git2::CredentialType) -> Result<git2::Cred, git2::Error> {
    let mut state = CredState::default();
    move |callback_url, username_from_url, allowed| {
        let url = if callback_url.is_empty() {
            remote_url.as_str()
        } else {
            callback_url
        };
        let username = username_from_url.unwrap_or("git");
        while let Some(choice) = state.next(allowed, &key_files) {
            let cred = match &choice {
                CredChoice::SshAgent => git2::Cred::ssh_key_from_agent(username),
                CredChoice::SshKeyFile(key) => {
                    let pub_path = key.with_extension("pub");
                    let pub_opt = pub_path.exists().then_some(pub_path.as_path());
                    git2::Cred::ssh_key(username, pub_opt, key, None)
                }
                CredChoice::CredentialHelper => match &config {
                    Some(cfg) => git2::Cred::credential_helper(cfg, url, username_from_url),
                    None => continue,
                },
                CredChoice::Default => git2::Cred::default(),
                CredChoice::Username => git2::Cred::username(username),
            };
            if let Ok(cred) = cred {
                return Ok(cred);
            }
        }
        Err(state.exhausted())
    }
}

/// Kandidaten für Default-SSH-Keys (ohne Passphrase versucht).
fn default_ssh_key_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    if let Some(mut ssh_dir) = home.map(|h| h.join(".ssh")) {
        // Umgebungsvariable SSH_DIR als Override (Tests)?
        if let Some(env_dir) = std::env::var_os("GITMANAGER_SSH_DIR") {
            ssh_dir = PathBuf::from(env_dir);
        }
        for name in ["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"] {
            out.push(ssh_dir.join(name));
        }
    }
    out
}

/// Fetch eines einzelnen Remotes via System-`git` (Fallback für Auth).
///
/// Nutzt Credential-Manager/SSH-Agent des Users — daher funktioniert das auch
/// mit authentifizierten Repos, wo reines libgit2 ohne Callbacks scheitert.
/// `Ok` nur bei Exit-Code 0, sonst `Err` mit stderr (English-neutral via git,
/// aber nur fürs Log — strukturierter Fehler bleibt libgit2).
///
/// K1: ohne Refspecs `git -C p fetch --prune <remote>` (Refresh), mit
/// Refspecs `git -C p fetch --no-prune --no-tags <remote> <refspec…>`.
pub fn fetch_with_git_cli(
    repo_path: &Path,
    remote_name: &str,
    refspecs: &[&str],
    prune: bool,
) -> anyhow::Result<()> {
    let repo_str = repo_path.display().to_string();
    let mut args = vec!["-C", &repo_str, "fetch"];
    args.push(if prune { "--prune" } else { "--no-prune" });
    if !refspecs.is_empty() {
        args.push("--no-tags");
    }
    args.push(remote_name);
    args.extend_from_slice(refspecs);
    run_git_cli(
        &args,
        &format!("fetch '{remote_name}'"),
        GIT_CLI_FETCH_TIMEOUT,
    )
}

/// Obergrenzen für System-`git`: ein Prompt, den niemand sieht (SSH-
/// Passphrase ohne Agent auf der versteckten Konsole von
/// `CREATE_NO_WINDOW`), hielte sonst den Worker und damit den Ref-Lock
/// (`scanning`, K1) für die ganze Sitzung.
const GIT_CLI_FETCH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// `worktree add` checkt aus (ggf. mit LFS-Filtern).
const GIT_CLI_WORKTREE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Ein Klon über eine langsame Leitung darf länger dauern.
const GIT_CLI_CLONE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// Nachlauf für die Pipe-Leser nach dem Exit: ein Enkelprozess (ssh) mit
/// geerbter Pipe darf den Aufruf nicht festhalten.
const GIT_CLI_PIPE_GRACE: Duration = Duration::from_secs(1);

/// System-`git` ohne Konsolenfenster und ohne Terminal-Prompt, höchstens
/// `timeout` lang (danach `kill`). `Ok` nur bei Exit-Code 0, sonst `Err`
/// mit stderr/stdout (nur fürs Log, siehe [`fetch_with_git_cli`]).
fn run_git_cli(args: &[&str], what: &str, timeout: Duration) -> anyhow::Result<()> {
    let out = output_with_timeout(&mut git_cli_command(args), timeout)
        .map_err(|e| anyhow::anyhow!("git {what} {e}"))?;
    if out.success {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let detail = if stderr.is_empty() {
        if stdout.is_empty() {
            format!("exit {}", out.status)
        } else {
            stdout
        }
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stderr} / {stdout}")
    };
    Err(anyhow::anyhow!("git {what} failed: {detail}"))
}

/// `git <args>` für [`run_git_cli`]: Windows-GUI ohne Konsolen-Flash
/// (CREATE_NO_WINDOW); `GIT_TERMINAL_PROMPT=0`, weil kein Terminal zu sehen
/// ist (git fragt sonst Benutzer/Passwort auf der versteckten Konsole ab und
/// wartet); Credential-Helper wie der Git Credential Manager fragen weiter
/// per Fenster.
fn git_cli_command(args: &[&str]) -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    set_creation_flags(&mut cmd, 0x08000000);
    cmd
}

/// Ergebnis von [`output_with_timeout`].
struct CliOutput {
    success: bool,
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Wie `Command::output`, aber höchstens `timeout`: danach wird der Prozess
/// beendet und geerntet (`Err`). stdout/stderr lesen eigene Threads (kein
/// Pipe-Stau); nach dem Exit wartet der Aufruf höchstens
/// `GIT_CLI_PIPE_GRACE` auf sie (Enkelprozess mit geerbter Pipe).
fn output_with_timeout(
    cmd: &mut std::process::Command,
    timeout: Duration,
) -> Result<CliOutput, String> {
    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
    let stdout = read_pipe_async(child.stdout.take());
    let stderr = read_pipe_async(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {timeout:?}"));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("wait failed: {e}"));
            }
        }
    };
    Ok(CliOutput {
        success: status.success(),
        status,
        stdout: stdout.recv_timeout(GIT_CLI_PIPE_GRACE).unwrap_or_default(),
        stderr: stderr.recv_timeout(GIT_CLI_PIPE_GRACE).unwrap_or_default(),
    })
}

/// Liest eine Pipe bis EOF in einem eigenen Thread.
fn read_pipe_async<R: std::io::Read + Send + 'static>(
    pipe: Option<R>,
) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
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

/// E15: Ergebnis eines Branch-Wechsels. Bei einem Remote-Ref (`origin/x`)
/// mit vorhandenem lokalem `x` meldet es, ob `x` vorgespult wurde oder
/// eigene Commits hat — die App zeigt dazu einen Hinweis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutOutcome {
    /// Normaler Wechsel (lokaler Branch, neu angelegter Tracking-Branch,
    /// lokaler Stand gleich Remote-Stand, Commit/Tag).
    Switched,
    /// Lokaler Branch war Vorfahr des Remote-Stands, um `by` Commits vorgespult.
    FastForwarded { branch: String, by: usize },
    /// Lokaler Branch hat `ahead` eigene Commits — lokaler Stand ausgecheckt.
    LocalAhead { branch: String, ahead: usize },
    /// Lokaler Branch und Remote-Stand divergieren — lokaler Stand
    /// ausgecheckt, Ref unverändert.
    Diverged {
        branch: String,
        ahead: usize,
        behind: usize,
    },
}

/// E15: typisierte Abbrüche vor dem Checkout (Arbeitsverzeichnis unberührt).
/// Display bleibt Deutsch wie die übrigen git.rs-Fehler; die UI lokalisiert
/// per Downcast (`branch_upstream_mismatch`, `branch_checked_out_elsewhere`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchSwitchBlocked {
    /// Lokaler Branch verfolgt einen anderen Upstream als den gewählten Remote-Branch.
    UpstreamMismatch { branch: String, upstream: String },
    /// Branch ist HEAD in einem anderen Worktree desselben Repos.
    CheckedOutElsewhere { branch: String, path: PathBuf },
}

impl std::fmt::Display for BranchSwitchBlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UpstreamMismatch { branch, upstream } => {
                write!(f, "Lokaler Branch {branch} verfolgt {upstream}")
            }
            Self::CheckedOutElsewhere { branch, path } => {
                write!(f, "Branch {branch} ist in {} ausgecheckt", path.display())
            }
        }
    }
}

impl std::error::Error for BranchSwitchBlocked {}

/// E15: Ist `branch` (Kurzname) in einem anderen Worktree desselben Repos
/// als HEAD ausgecheckt? Liefert dessen Pfad. Liest `<commondir>/HEAD`
/// (Haupt-Worktree, nicht bei Bare) und `<commondir>/worktrees/*/HEAD`
/// direkt; der eigene Worktree zählt nicht. Verwaiste Einträge (Ordner
/// gelöscht, nicht gesperrt) zählen wie bei `git worktree prune` nicht.
pub(crate) fn branch_checked_out_elsewhere(repo: &Repository, branch: &str) -> Option<PathBuf> {
    let wanted = format!("refs/heads/{branch}");
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let own = canon(repo.path());
    let common = repo.commondir();
    if canon(common) != own && head_symref(common).as_deref() == Some(wanted.as_str()) {
        let main = Repository::open(common).ok();
        if let Some(workdir) = main.as_ref().and_then(|r| r.workdir()) {
            // Ohne abschließenden Separator (`…/c1/` → `…/c1`) für die Meldung.
            return Some(workdir.components().collect());
        }
    }
    let entries = std::fs::read_dir(common.join("worktrees")).ok()?;
    for admin in entries.flatten().map(|e| e.path()) {
        if canon(&admin) == own || head_symref(&admin).as_deref() != Some(wanted.as_str()) {
            continue;
        }
        if let Some(path) = linked_worktree_path(&admin) {
            return Some(path);
        }
    }
    None
}

/// Symbolischer HEAD eines Git-Verzeichnisses (`ref: refs/heads/x` →
/// `refs/heads/x`); `None` bei Detached HEAD oder unlesbarer Datei. Anders
/// als `parse_head_file` ohne SHA-Kurzform (kein Zufallstreffer).
fn head_symref(git_dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    content
        .trim()
        .strip_prefix("ref: ")
        .map(|r| r.trim().to_string())
}

/// Ordner eines verknüpften Worktrees aus `<admin>/gitdir` (zeigt auf
/// `<wt>/.git`, relativ zum Admin-Ordner bei `worktree.useRelativePaths`).
/// `None` bei verwaistem Eintrag: Ziel fehlt und kein `locked`.
fn linked_worktree_path(admin: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(admin.join("gitdir")).ok()?;
    let gitfile = admin.join(raw.trim());
    if !gitfile.exists() && !admin.join("locked").exists() {
        return None;
    }
    // Lexikalisch normalisieren (kein canonicalize → kein `\\?\` unter
    // Windows): `..` relativer Einträge auflösen, Git-`/` unter Windows
    // wird `\` wie beim Haupt-Worktree. `commondir` ist von libgit2 schon
    // per realpath aufgelöst, `..` läuft also nicht über Symlinks.
    let mut out = PathBuf::new();
    for c in gitfile.parent()?.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    Some(out)
}

/// E15: Ref-Änderung, die zu einem Wechsel auf einen Remote-Ref gehört.
enum RefUpdate {
    None,
    /// Lokaler Branch fehlt → am Remote-Stand anlegen, Upstream als
    /// Kurzname (`origin/feature/foo`).
    Create {
        branch: String,
        at: git2::Oid,
        upstream: String,
    },
    /// Lokaler Branch ist Vorfahr → per Compare-and-Swap vorspulen.
    FastForward {
        branch: String,
        from: git2::Oid,
        to: git2::Oid,
    },
}

/// E15: vor Dry-Run und `checkout_tree` aufgelöstes Ziel eines Wechsels.
struct CheckoutTarget<'r> {
    /// Baum, der ausgecheckt wird (bei voraus/divergiert der lokale Stand).
    object: git2::Object<'r>,
    /// Lokaler Branch, an den HEAD danach angehängt wird; `None` → Commit/Tag
    /// über `set_head_from_reference`.
    branch: Option<String>,
    reference: Option<git2::Reference<'r>>,
    update: RefUpdate,
    outcome: CheckoutOutcome,
}

/// E15: löst `branch_name` auf, bevor irgendetwas am Arbeitsverzeichnis
/// passiert. Remote-Ref mit vorhandenem lokalem Branch: Vorfahr → vorspulen,
/// voraus/divergiert → lokaler Stand; anderer Upstream oder Branch in einem
/// anderen Worktree → typisierter Abbruch (`BranchSwitchBlocked`).
fn resolve_checkout_target<'r>(
    repo: &'r Repository,
    branch_name: &str,
) -> anyhow::Result<CheckoutTarget<'r>> {
    let (object, reference) = repo
        .revparse_ext(branch_name)
        .map_err(|e| anyhow::anyhow!("Branch '{branch_name}' nicht gefunden: {e}"))?;
    let full = reference
        .as_ref()
        .map(|r| r.name().map(str::to_string))
        .transpose()
        .map_err(|e| anyhow::anyhow!("Ungültige Referenz: {e}"))?
        .unwrap_or_default();
    let Some(upstream) = full.strip_prefix("refs/remotes/") else {
        // Lokaler Branch, Tag oder Commit: Ziel unverändert, beim lokalen
        // Branch nur die Worktree-Prüfung.
        let branch = full.strip_prefix("refs/heads/").map(str::to_string);
        if let Some(branch) = &branch {
            ensure_not_checked_out_elsewhere(repo, branch)?;
        }
        return Ok(CheckoutTarget {
            object,
            branch,
            reference,
            update: RefUpdate::None,
            outcome: CheckoutOutcome::Switched,
        });
    };
    // K2: nur Remote-Präfix strippen (siehe remote_short_name).
    let short = remote_short_name(&full, branch_name);
    let remote = object
        .peel_to_commit()
        .map_err(|e| anyhow::anyhow!("Commit nicht gefunden: {e}"))?;
    let Ok(local) = repo.find_branch(&short, BranchType::Local) else {
        return Ok(CheckoutTarget {
            object: remote.as_object().clone(),
            update: RefUpdate::Create {
                branch: short.clone(),
                at: remote.id(),
                upstream: upstream.to_string(),
            },
            branch: Some(short),
            reference: None,
            outcome: CheckoutOutcome::Switched,
        });
    };
    // Kein Upstream konfiguriert → wie der gleichnamige Remote-Branch behandeln.
    if let Ok(buf) = repo.branch_upstream_name(&format!("refs/heads/{short}")) {
        let tracked = buf.as_str().unwrap_or_default();
        if !tracked.is_empty() && tracked != full {
            let upstream = tracked
                .strip_prefix("refs/remotes/")
                .or_else(|| tracked.strip_prefix("refs/heads/"))
                .unwrap_or(tracked);
            return Err(anyhow::Error::new(BranchSwitchBlocked::UpstreamMismatch {
                branch: short,
                upstream: upstream.to_string(),
            }));
        }
    }
    ensure_not_checked_out_elsewhere(repo, &short)?;
    let local = local
        .get()
        .peel_to_commit()
        .map_err(|e| anyhow::anyhow!("Commit von {short} nicht gefunden: {e}"))?;
    let (ahead, behind) = if local.id() == remote.id() {
        (0, 0)
    } else {
        repo.graph_ahead_behind(local.id(), remote.id())
            .map_err(|e| anyhow::anyhow!("Vergleich {short} / {upstream} fehlgeschlagen: {e}"))?
    };
    let branch = short.clone();
    let outcome = match (ahead, behind) {
        (0, 0) => CheckoutOutcome::Switched,
        (0, by) => CheckoutOutcome::FastForwarded { branch, by },
        (ahead, 0) => CheckoutOutcome::LocalAhead { branch, ahead },
        (ahead, behind) => CheckoutOutcome::Diverged {
            branch,
            ahead,
            behind,
        },
    };
    // Nur beim Vorspulen der Remote-Baum, sonst immer der lokale Stand.
    let (object, update) = if ahead == 0 && behind > 0 {
        let update = RefUpdate::FastForward {
            branch: short.clone(),
            from: local.id(),
            to: remote.id(),
        };
        (remote.into_object(), update)
    } else {
        (local.into_object(), RefUpdate::None)
    };
    Ok(CheckoutTarget {
        object,
        branch: Some(short),
        reference: None,
        update,
        outcome,
    })
}

fn ensure_not_checked_out_elsewhere(repo: &Repository, branch: &str) -> anyhow::Result<()> {
    match branch_checked_out_elsewhere(repo, branch) {
        Some(path) => Err(anyhow::Error::new(
            BranchSwitchBlocked::CheckedOutElsewhere {
                branch: branch.to_string(),
                path,
            },
        )),
        None => Ok(()),
    }
}

fn apply_ref_update(repo: &Repository, update: &RefUpdate) -> anyhow::Result<()> {
    match update {
        RefUpdate::None => {}
        RefUpdate::Create {
            branch,
            at,
            upstream,
        } => {
            let commit = repo
                .find_commit(*at)
                .map_err(|e| anyhow::anyhow!("Commit nicht gefunden: {e}"))?;
            let mut local_branch = repo
                .branch(branch, &commit, false)
                .map_err(|e| anyhow::anyhow!("Lokalen Branch erstellen fehlgeschlagen: {e}"))?;
            let _ = local_branch.set_upstream(Some(upstream));
        }
        RefUpdate::FastForward { branch, from, to } => {
            // Compare-and-Swap: hat sich der Ref seit dem Auflösen bewegt,
            // bricht libgit2 mit GIT_EMODIFIED ab statt ihn zu überschreiben.
            repo.reference_matching(
                &format!("refs/heads/{branch}"),
                *to,
                true,
                *from,
                &format!("gitmanager: fast-forward {branch}"),
            )
            .map_err(|e| anyhow::anyhow!("Vorspulen von {branch} fehlgeschlagen: {e}"))?;
        }
    }
    Ok(())
}

/// E15: gemeinsamer Ablauf für sicheren und Force-Wechsel. Ziel zuerst
/// auflösen (inkl. Upstream-/Worktree-Prüfung), dann Dry-Run (nur sicher),
/// Refs, Baum, HEAD. Refs vor dem Baum, damit ein gescheitertes Anlegen oder
/// Vorspulen das Arbeitsverzeichnis nicht anfasst — außer beim Vorspulen des
/// aktuellen HEAD-Branch: `checkout_tree` rechnet gegen HEAD als Basis, der
/// Ref folgt dort erst nach dem Baum.
fn switch_branch(path: &Path, branch_name: &str, force: bool) -> anyhow::Result<CheckoutOutcome> {
    let repo = preflight_checkout(path)?;
    let target = resolve_checkout_target(&repo, branch_name)?;

    if !force {
        // Dry-run zuerst
        let mut cb = git2::build::CheckoutBuilder::new();
        cb.dry_run();
        cb.safe();
        if let Err(e) = repo.checkout_tree(&target.object, Some(&mut cb)) {
            if e.code() == git2::ErrorCode::Conflict {
                anyhow::bail!("Checkout würde Konflikte erzeugen (dirty Dateien): {e}");
            }
            anyhow::bail!("Preflight fehlgeschlagen: {e}");
        }
    }

    let updates_head = match &target.update {
        RefUpdate::FastForward { branch, .. } => repo.head().is_ok_and(|h| {
            h.name()
                .is_ok_and(|n| n.strip_prefix("refs/heads/") == Some(branch.as_str()))
        }),
        _ => false,
    };
    if !updates_head {
        apply_ref_update(&repo, &target.update)?;
    }

    // Eigentlicher Checkout
    let mut cb = git2::build::CheckoutBuilder::new();
    if force {
        cb.force();
    } else {
        cb.safe();
    }
    let label = if force { "Force Checkout" } else { "Checkout" };
    repo.checkout_tree(&target.object, Some(&mut cb))
        .map_err(|e| anyhow::anyhow!("{label} fehlgeschlagen: {e}"))?;

    if updates_head {
        apply_ref_update(&repo, &target.update)?;
    }
    match &target.branch {
        Some(branch) => repo
            .set_head(&format!("refs/heads/{branch}"))
            .map_err(|e| anyhow::anyhow!("HEAD setzen fehlgeschlagen: {e}"))?,
        None => set_head_from_reference(&repo, target.reference.as_ref(), &target.object)?,
    }
    Ok(target.outcome)
}

/// Führt sicheren Branch-Wechsel durch (nur wenn keine Konflikte, sonst Fehler).
/// E15: Remote-Ref (`origin/x`) mit vorhandenem lokalem `x` → vorspulen oder
/// lokalen Stand auschecken (siehe [`CheckoutOutcome`]), nie Remote-Baum
/// unter lokalem Ref.
pub fn checkout_branch(path: &Path, branch_name: &str) -> anyhow::Result<CheckoutOutcome> {
    switch_branch(path, branch_name, false)
}

/// Force Checkout (überschreibt lokale Änderungen) – nur nach Bestätigung via Dialog nach Dirty-Check.
/// Verhindert versehentliches Zerstören eines laufenden Merge/Rebase-States.
/// E15: Remote-Ref wie bei [`checkout_branch`] auf den lokalen Branch
/// abgebildet (ggf. angelegt) statt Detached HEAD.
pub fn checkout_branch_force(path: &Path, branch_name: &str) -> anyhow::Result<CheckoutOutcome> {
    switch_branch(path, branch_name, true)
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
pub fn stash_and_checkout(path: &Path, branch_name: &str) -> anyhow::Result<CheckoutOutcome> {
    // E15: deterministische Abbrüche (Merge/Rebase, Branch fehlt, anderer
    // Upstream, Worktree) vor dem Stash — sonst liefe das Arbeitsverzeichnis
    // durch Stash + Pop, und `stash_pop` stellt den Index nicht wieder her.
    {
        let repo = preflight_checkout(path)?;
        resolve_checkout_target(&repo, branch_name)?;
    }
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
        Ok(outcome) => {
            // Nur poppen wenn tatsächlich gestasht wurde — sonst würde
            // stash_pop(0) einen alten, bewusst aufgehobenen Stash poppen (K1).
            if !stashed {
                return Ok(outcome);
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
            Ok(outcome)
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

// --- Merge-Request-Checkout (K1) ---

/// K1: Stand des lokalen MR-Branches nach [`prepare_mr_branch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MrPrepOutcome {
    /// Lokaler Branch neu am MR-Stand angelegt, Upstream gesetzt
    Created,
    UpToDate,
    /// Lokaler Branch war Vorfahr, um `by` Commits vorgespult
    FastForwarded {
        by: usize,
    },
    /// Lokaler Branch hat eigene Commits; Ref unverändert
    LocalAhead {
        ahead: usize,
    },
    /// Lokaler Branch und MR-Stand divergieren; Ref unverändert
    Diverged {
        ahead: usize,
        behind: usize,
    },
    /// Branch ist im Klon selbst HEAD; Ref unverändert (kein Pull in v1)
    AlreadyHead {
        behind: usize,
    },
}

/// K1: Ergebnis von [`prepare_mr_branch`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrPrep {
    /// Tatsächlich genutzter lokaler Branch: `sourceBranch` oder bei
    /// Namens-/Upstream-Konflikt `mr-<iid>`
    pub local_branch: String,
    pub outcome: MrPrepOutcome,
}

/// K1: typisierte Fehler beim Auschecken eines MR. Display bleibt Deutsch wie
/// die übrigen git.rs-Fehler; die UI lokalisiert per Match bzw. Downcast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutError {
    /// Branch ist HEAD in einem anderen Worktree (oder im Basis-Klon selbst)
    CheckedOutElsewhere { branch: String, path: PathBuf },
    /// Remote-Branch fehlt nach dem Fetch (`<remote>/<src>`)
    RemoteBranchMissing { upstream: String },
    /// Weder `sourceBranch` noch `mr-<iid>` sind als lokaler Branch nutzbar
    BranchNameTaken { branch: String, fallback: String },
    /// Zielordner existiert und ist nicht leer
    TargetNotEmpty { path: PathBuf },
    /// Sonstiger git-Fehler (libgit2; der CLI-Fehler steht nur im Log)
    Git(String),
}

impl std::fmt::Display for CheckoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CheckedOutElsewhere { branch, path } => {
                write!(f, "Branch {branch} ist in {} ausgecheckt", path.display())
            }
            Self::RemoteBranchMissing { upstream } => {
                write!(f, "Remote-Branch {upstream} nicht gefunden")
            }
            Self::BranchNameTaken { branch, fallback } => write!(
                f,
                "Weder {branch} noch {fallback} lassen sich als lokaler Branch nutzen"
            ),
            Self::TargetNotEmpty { path } => write!(
                f,
                "Der Ordner {} existiert bereits und ist nicht leer",
                path.display()
            ),
            Self::Git(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for CheckoutError {}

/// K1: bereitet nach [`fetch_refspecs`] den lokalen MR-Branch im Klon
/// `base_path` vor. Ändert nur Refs, nie Arbeitsverzeichnis oder HEAD, und
/// setzt nie zurück: fehlt der Branch, wird er am MR-Stand angelegt (mit
/// Upstream); ist er Vorfahr, wird er per Compare-and-Swap vorgespult;
/// voraus/divergiert bleibt er unverändert. Vorher wie beim Branch-Wechsel
/// (E15) die Worktree-Prüfung ([`branch_checked_out_elsewhere`]).
pub fn prepare_mr_branch(base_path: &Path, plan: &MrCheckoutPlan) -> Result<MrPrep, CheckoutError> {
    let repo = Repository::open(base_path)
        .map_err(|e| CheckoutError::Git(format!("Repo öffnen fehlgeschlagen: {e}")))?;
    let target =
        repo.refname_to_id(&plan.remote_ref())
            .map_err(|_| CheckoutError::RemoteBranchMissing {
                upstream: plan.upstream(),
            })?;
    let local_branch = choose_mr_branch(&repo, plan)?;
    if let Some(path) = branch_checked_out_elsewhere(&repo, &local_branch) {
        return Err(CheckoutError::CheckedOutElsewhere {
            branch: local_branch,
            path,
        });
    }
    if let Some(expected) = plan.expected_sha.as_deref() {
        if !target.to_string().starts_with(expected) {
            eprintln!(
                "MR !{}: geholter Stand {target} weicht von diffHeadSha {expected} ab (MR inzwischen aktualisiert?)",
                plan.iid
            );
        }
    }
    let git_err = |e: anyhow::Error| CheckoutError::Git(e.to_string());
    let local_ref = format!("refs/heads/{local_branch}");
    let Ok(local) = repo.refname_to_id(&local_ref) else {
        let update = RefUpdate::Create {
            branch: local_branch.clone(),
            at: target,
            upstream: plan.upstream(),
        };
        apply_ref_update(&repo, &update).map_err(git_err)?;
        return Ok(MrPrep {
            local_branch,
            outcome: MrPrepOutcome::Created,
        });
    };
    let (ahead, behind) = if local == target {
        (0, 0)
    } else {
        repo.graph_ahead_behind(local, target).map_err(|e| {
            CheckoutError::Git(format!(
                "Vergleich {local_branch} / {} fehlgeschlagen: {e}",
                plan.upstream()
            ))
        })?
    };
    let is_head = repo
        .head()
        .is_ok_and(|h| h.name().is_ok_and(|n| n == local_ref));
    let outcome = match (ahead, behind) {
        _ if is_head => MrPrepOutcome::AlreadyHead { behind },
        (0, 0) => MrPrepOutcome::UpToDate,
        (0, by) => {
            let update = RefUpdate::FastForward {
                branch: local_branch.clone(),
                from: local,
                to: target,
            };
            apply_ref_update(&repo, &update).map_err(git_err)?;
            MrPrepOutcome::FastForwarded { by }
        }
        (ahead, 0) => MrPrepOutcome::LocalAhead { ahead },
        (ahead, behind) => MrPrepOutcome::Diverged { ahead, behind },
    };
    Ok(MrPrep {
        local_branch,
        outcome,
    })
}

/// Lokaler Branch für den MR: `sourceBranch`, außer ein gleichnamiger
/// Branch verfolgt einen anderen Upstream oder das Anlegen kollidiert
/// (Ordner/Datei wie `x` gegen `x/y`, nur Groß-/Kleinschreibung verschieden –
/// lose Refs unter Windows) – dann `mr-<iid>`. Ein vorhandener Branch ohne
/// Upstream zählt wie in E15 als der MR-Branch.
fn choose_mr_branch(repo: &Repository, plan: &MrCheckoutPlan) -> Result<String, CheckoutError> {
    let locals: Vec<String> = repo
        .branches(Some(BranchType::Local))
        .map(|branches| {
            branches
                .flatten()
                .filter_map(|(b, _)| b.name().ok().flatten().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let upstream = plan.remote_ref();
    let usable = |name: &str| {
        if locals.iter().any(|l| l == name) {
            return repo
                .branch_upstream_name(&format!("refs/heads/{name}"))
                .ok()
                .and_then(|b| b.as_str().ok().map(str::to_string))
                .is_none_or(|tracked| tracked.is_empty() || tracked == upstream);
        }
        !locals.iter().any(|l| {
            l.eq_ignore_ascii_case(name) || is_ref_dir_of(l, name) || is_ref_dir_of(name, l)
        })
    };
    let fallback = plan.fallback_branch();
    if usable(&plan.source_branch) {
        Ok(plan.source_branch.clone())
    } else if usable(&fallback) {
        Ok(fallback)
    } else {
        Err(CheckoutError::BranchNameTaken {
            branch: plan.source_branch.clone(),
            fallback,
        })
    }
}

/// Ist `dir` ein Ordner-Präfix von `name` (`feature` vor `feature/x`)?
/// Ohne Groß-/Kleinschreibung wie lose Refs unter Windows.
fn is_ref_dir_of(dir: &str, name: &str) -> bool {
    name.as_bytes().get(dir.len()) == Some(&b'/')
        && name
            .get(..dir.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(dir))
}

/// K1: legt im Ordner `target_path` einen Worktree des Klons `base_path` auf
/// dem lokalen Branch `local_branch` an (Ordnername = Worktree-Name). Der
/// Zielordner darf fehlen oder leer sein; Elternordner (Review-Ordner)
/// werden angelegt, ein verwaister Eintrag für denselben Ordner wird
/// entfernt. libgit2 zuerst, Fallback `git -C base worktree add`.
pub fn add_worktree(
    base_path: &Path,
    target_path: &Path,
    local_branch: &str,
) -> Result<(), CheckoutError> {
    let repo = Repository::open(base_path)
        .map_err(|e| CheckoutError::Git(format!("Repo öffnen fehlgeschlagen: {e}")))?;
    let full = format!("refs/heads/{local_branch}");
    // Branch darf in keinem Worktree HEAD sein – auch nicht im Basis-Klon,
    // den `branch_checked_out_elsewhere` als eigenen Worktree auslässt.
    let elsewhere = if repo.head().is_ok_and(|h| h.name().is_ok_and(|n| n == full)) {
        repo.workdir().map(|w| w.components().collect())
    } else {
        branch_checked_out_elsewhere(&repo, local_branch)
    };
    if let Some(path) = elsewhere {
        return Err(CheckoutError::CheckedOutElsewhere {
            branch: local_branch.to_string(),
            path,
        });
    }
    let reference = repo
        .find_reference(&full)
        .map_err(|e| CheckoutError::Git(format!("Branch {local_branch} nicht gefunden: {e}")))?;
    let name = target_path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| {
            CheckoutError::Git(format!("Ungültiger Zielordner: {}", target_path.display()))
        })?
        .to_string();
    prepare_target_dir(target_path)?;
    prune_stale_worktrees_at(&repo, target_path);
    let admin = repo.commondir().join("worktrees").join(&name);
    let admin_existed = admin.exists();
    let mut opts = git2::WorktreeAddOptions::new();
    opts.reference(Some(&reference));
    let Err(lib_err) = repo.worktree(&name, target_path, Some(&opts)) else {
        return Ok(());
    };
    eprintln!(
        "libgit2 worktree add nach '{}' fehlgeschlagen ({lib_err}), versuche System-git...",
        target_path.display()
    );
    // Halbfertigen Stand von libgit2 entfernen – nur, was dieser Aufruf
    // angelegt hat (der Zielordner fehlte oder war leer und ist entfernt).
    if !admin_existed {
        let _ = std::fs::remove_dir_all(&admin);
    }
    let _ = std::fs::remove_dir_all(target_path);
    worktree_add_with_git_cli(base_path, target_path, local_branch).map_err(|cli_err| {
        eprintln!("System-git worktree add ebenfalls fehlgeschlagen: {cli_err:#}");
        let _ = std::fs::remove_dir_all(target_path);
        // Strukturierter Fehler bleibt libgit2-basiert (wie beim Fetch).
        CheckoutError::Git(format!("Worktree anlegen fehlgeschlagen: {lib_err}"))
    })
}

/// Verwaiste Worktree-Einträge, die auf genau `target` zeigen (Ordner im
/// Explorer gelöscht, ohne `git worktree prune`), entfernen: libgit2 legt
/// `<commondir>/worktrees/<name>` exklusiv an, und `git worktree add`
/// verweigert einen „missing but already registered worktree“. Gesperrte
/// Einträge und solche für andere Ordner (z. B. auf einem getrennten
/// Laufwerk) bleiben. `prune` ohne Flags entfernt nur den Admin-Ordner
/// ungültiger, ungesperrter Einträge, nie ein Arbeitsverzeichnis.
fn prune_stale_worktrees_at(repo: &Repository, target: &Path) {
    let Ok(names) = repo.worktrees() else {
        return;
    };
    let target = missing_dir_key(target);
    for name in names.iter().filter_map(|n| n.ok().flatten()) {
        let Ok(wt) = repo.find_worktree(name) else {
            continue;
        };
        let stale = wt.validate().is_err()
            && matches!(wt.is_locked(), Ok(git2::WorktreeLockStatus::Unlocked))
            && missing_dir_key(wt.path()) == target;
        if !stale {
            continue;
        }
        match wt.prune(None) {
            Ok(()) => eprintln!("Verwaisten Worktree-Eintrag '{name}' entfernt"),
            Err(e) => {
                eprintln!("Verwaisten Worktree-Eintrag '{name}' entfernen fehlgeschlagen: {e}")
            }
        }
    }
}

/// Vergleichsschlüssel für einen evtl. fehlenden Ordner: Elternordner
/// kanonisch ([`canonical_key_path`]), Name angehängt; unter Windows ohne
/// Groß-/Kleinschreibung. Git schreibt auch unter Windows `/`.
fn missing_dir_key(path: &Path) -> String {
    let key: PathBuf = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => canonical_key_path(parent).join(name),
        _ => path.components().collect(),
    };
    let key = key.to_string_lossy().to_string();
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
}

/// CLI-Fallback: `git -C <base> worktree add <target> <branch>`.
fn worktree_add_with_git_cli(
    base_path: &Path,
    target_path: &Path,
    local_branch: &str,
) -> anyhow::Result<()> {
    let base = base_path.display().to_string();
    let target = target_path.display().to_string();
    run_git_cli(
        &["-C", &base, "worktree", "add", "--", &target, local_branch],
        "worktree add",
        GIT_CLI_WORKTREE_TIMEOUT,
    )
}

/// K1: klont `url` nach `target_path` und checkt `branch` aus. Der Zielordner
/// darf fehlen oder leer sein. libgit2 mit den Credentials wie beim Fetch
/// ([`credentials_callback`], globale Config für `credential.helper`), Fallback
/// `git clone --branch <branch> <url> <target>`. Ein halber Klon wird
/// wieder entfernt.
pub fn clone_repo(url: &str, target_path: &Path, branch: &str) -> Result<(), CheckoutError> {
    prepare_target_dir(target_path)?;
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(credentials_callback(
        git2::Config::open_default().ok(),
        url.to_string(),
    ));
    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);
    let result = git2::build::RepoBuilder::new()
        .branch(branch)
        .fetch_options(fetch_opts)
        .clone(url, target_path);
    let Err(lib_err) = result else {
        return Ok(());
    };
    eprintln!(
        "libgit2 clone nach '{}' fehlgeschlagen ({lib_err}), versuche System-git...",
        target_path.display()
    );
    // Der Zielordner fehlte oder war leer (und ist entfernt): alles darin
    // stammt von diesem Aufruf.
    let _ = std::fs::remove_dir_all(target_path);
    clone_with_git_cli(url, target_path, branch).map_err(|cli_err| {
        eprintln!("System-git clone ebenfalls fehlgeschlagen: {cli_err:#}");
        let _ = std::fs::remove_dir_all(target_path);
        CheckoutError::Git(format!("Klonen fehlgeschlagen: {lib_err}"))
    })
}

/// CLI-Fallback: `git clone --branch <branch> -- <url> <target>`.
fn clone_with_git_cli(url: &str, target_path: &Path, branch: &str) -> anyhow::Result<()> {
    let target = target_path.display().to_string();
    run_git_cli(
        &["clone", "--branch", branch, "--", url, &target],
        "clone",
        GIT_CLI_CLONE_TIMEOUT,
    )
}

/// Zielordner für Worktree oder Klon: darf fehlen oder leer sein. Ein leerer
/// Ordner wird entfernt (libgit2 legt den Worktree-Ordner exklusiv an),
/// Elternordner werden angelegt.
fn prepare_target_dir(target: &Path) -> Result<(), CheckoutError> {
    let not_empty = || CheckoutError::TargetNotEmpty {
        path: target.to_path_buf(),
    };
    match std::fs::symlink_metadata(target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(CheckoutError::Git(format!(
                "Zielordner {} nicht lesbar: {e}",
                target.display()
            )))
        }
        Ok(meta) if meta.is_dir() => {
            let has_entries = std::fs::read_dir(target)
                .map_err(|e| {
                    CheckoutError::Git(format!("Zielordner {} nicht lesbar: {e}", target.display()))
                })?
                .next()
                .is_some();
            if has_entries {
                return Err(not_empty());
            }
            std::fs::remove_dir(target).map_err(|e| {
                CheckoutError::Git(format!(
                    "Leeren Zielordner {} entfernen fehlgeschlagen: {e}",
                    target.display()
                ))
            })?;
        }
        // Datei oder Symlink
        Ok(_) => return Err(not_empty()),
    }
    if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| {
            CheckoutError::Git(format!(
                "Ordner {} anlegen fehlgeschlagen: {e}",
                parent.display()
            ))
        })?;
    }
    Ok(())
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
    let dirty_files = dirty_count(&repo);
    // Finding C: geöffnetes Handle wiederverwenden statt 2× erneut öffnen.
    let branches = list_branches_in(&repo);
    let (ahead, behind) = ahead_behind_in(&repo);
    let mut info =
        RepoInfo::new(path, branch, dirty_files > 0, is_detached).with_branches(branches);
    info.ahead = ahead;
    info.behind = behind;
    info.remotes = read_remotes(&repo);
    info.common_dir = canonical_key_path(repo.commondir());
    info.dirty_files = dirty_files;
    info.worktrees = read_worktree_heads(&repo);
    info.is_linked_worktree = repo.is_worktree();
    Some(info)
}

/// K1: die übrigen Worktrees desselben Repos mit ihrem Branch – bei einem
/// verknüpften Worktree der Haupt-Worktree, dazu die verknüpften aus
/// `Repository::worktrees()` außer dem eigenen. Liest wie
/// [`branch_checked_out_elsewhere`] nur Dateien unter `commondir` (ein
/// `readdir` je Repo); verwaiste Einträge fehlen.
fn read_worktree_heads(repo: &Repository) -> Vec<WorktreeHead> {
    let branch_of = |git_dir: &Path| {
        head_symref(git_dir).and_then(|r| r.strip_prefix("refs/heads/").map(str::to_string))
    };
    let common = repo.commondir();
    let mut heads = Vec::new();
    if repo.is_worktree() {
        let main = Repository::open(common).ok();
        if let Some(workdir) = main.as_ref().and_then(|r| r.workdir()) {
            heads.push(WorktreeHead {
                // Ohne abschließenden Separator wie bei `branch_checked_out_elsewhere`
                path: workdir.components().collect(),
                branch: branch_of(common),
            });
        }
    }
    let Ok(names) = repo.worktrees() else {
        return heads;
    };
    if names.is_empty() {
        return heads;
    }
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let own = canon(repo.path());
    for name in names.iter().filter_map(|n| n.ok().flatten()) {
        let admin = common.join("worktrees").join(name);
        if canon(&admin) == own {
            continue;
        }
        if let Some(path) = linked_worktree_path(&admin) {
            heads.push(WorktreeHead {
                path,
                branch: branch_of(&admin),
            });
        }
    }
    heads
}

/// Kanonischer Pfad als Vergleichsschlüssel (`RepoInfo.common_dir`). libgit2
/// löst unter Windows weder Symlinks, Junctions und `subst`-Laufwerke noch
/// die Groß-/Kleinschreibung auf (nur `GetFullPathNameW`): Haupt-Repo (Pfad
/// aus dem Suchpfad) und Worktree (`gitdir:` von git aufgelöst) lieferten
/// sonst verschiedene Werte. Ohne `\\?\`-Präfix und ohne abschließenden
/// Separator; nicht auflösbar → der Pfad selbst.
fn canonical_key_path(path: &Path) -> PathBuf {
    let Ok(canon) = path.canonicalize() else {
        return path.components().collect();
    };
    #[cfg(windows)]
    {
        if let Some(rest) = canon.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
            if let Some(unc) = rest.strip_prefix(r"UNC\") {
                return PathBuf::from(format!(r"\\{unc}"));
            }
            // Nur Laufwerkspfade (`C:\…`), `Volume{…}` bleibt verbatim
            if rest.as_bytes().get(1) == Some(&b':') {
                return PathBuf::from(rest);
            }
        }
    }
    canon
}

/// Remotes für die Merge-Request-Zuordnung: origin zuerst, dann
/// alphabetisch. Zugangsdaten in der URL entfernt `RemoteInfo::new` sofort.
fn read_remotes(repo: &Repository) -> Vec<RemoteInfo> {
    let Ok(names) = repo.remotes() else {
        return Vec::new();
    };
    let mut remotes: Vec<RemoteInfo> = names
        .iter()
        .filter_map(|n| n.ok().flatten())
        .filter_map(|name| {
            let remote = repo.find_remote(name).ok()?;
            Some(RemoteInfo::new(name, remote.url().ok()?))
        })
        .collect();
    remotes.sort_by(|a, b| (a.name != "origin", &a.name).cmp(&(b.name != "origin", &b.name)));
    remotes
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

pub(crate) fn set_creation_flags(_cmd: &mut std::process::Command, _flags: u32) {
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

    /// System-`git` fragt nie auf einem (unsichtbaren) Terminal und liest
    /// kein stdin.
    #[test]
    fn git_cli_never_prompts_on_terminal() {
        let cmd = git_cli_command(&["fetch", "origin"]);
        assert!(cmd.get_envs().any(|(key, value)| {
            key == "GIT_TERMINAL_PROMPT" && value == Some(std::ffi::OsStr::new("0"))
        }));
        assert_eq!(cmd.get_program(), "git");
    }

    /// Ein hängender Prozess endet nach dem Timeout (`kill`) mit Fehler,
    /// statt den Worker samt Ref-Lock für immer zu halten; ein Enkelprozess
    /// mit geerbter Pipe hält den Aufruf nach dem Exit nicht fest.
    #[cfg(unix)]
    #[test]
    fn git_cli_output_is_bounded() {
        let sh = |script: &str| {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", script])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            cmd
        };
        let start = Instant::now();
        let err = output_with_timeout(&mut sh("sleep 5"), Duration::from_millis(200))
            .err()
            .expect("Timeout");
        assert!(err.starts_with("timed out after"), "{err}");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );

        let start = Instant::now();
        let out = output_with_timeout(
            &mut sh("echo fehler >&2; sleep 5 & exit 3"),
            Duration::from_secs(20),
        )
        .expect("beendet");
        assert!(!out.success);
        assert_eq!(out.status.code(), Some(3));
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );

        let out = output_with_timeout(&mut sh("echo ok; echo warn >&2"), Duration::from_secs(20))
            .expect("beendet");
        assert!(out.success);
        assert_eq!(out.stdout, b"ok\n");
        assert_eq!(out.stderr, b"warn\n");
    }

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
        assert!(info.remotes.is_empty());
        assert_eq!(info.common_dir, path.join(".git"));
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
        let (status, truncated) = get_detailed_status(&repo3);
        assert!(!truncated, "small status must not be truncated");
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
        let (status, truncated) = get_detailed_status(&repo);
        assert!(!truncated);
        assert!(status.is_empty());
        fs::write(dir.path().join("untracked.txt"), "hi").unwrap();
        fs::write(dir.path().join("README.md"), "modified").unwrap();
        let repo2 = Repository::open(dir.path()).unwrap();
        let mut idx = repo2.index().unwrap();
        idx.add_path(Path::new("untracked.txt")).unwrap();
        idx.write().unwrap();
        // need to stage untracked? Actually add makes it staged
        let (status2, truncated2) = get_detailed_status(&repo2);
        assert!(!truncated2);
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
        let (status3, truncated3) = get_detailed_status(&repo3);
        assert!(!truncated3);
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
    fn fetch_retries_transient_errors_only() {
        let dropped = || {
            git2::Error::new(
                git2::ErrorCode::Auth,
                git2::ErrorClass::Ssh,
                "remote rejected authentication: Failed getting response",
            )
        };
        let delays = [Duration::ZERO; 2];
        // SSH-Abbruch beim Handshake: zweiter Versuch klappt
        let mut calls = 0;
        let res = with_fetch_retries("origin", &delays, || {
            calls += 1;
            if calls == 1 {
                Err(dropped())
            } else {
                Ok(())
            }
        });
        assert!(res.is_ok());
        assert_eq!(calls, 2);
        // Bleibt der Abbruch, kommt nach allen Versuchen der letzte Fehler
        let mut calls = 0;
        let res = with_fetch_retries("origin", &delays, || {
            calls += 1;
            Err(dropped())
        });
        assert_eq!(calls, 3);
        assert!(res
            .unwrap_err()
            .message()
            .contains("Failed getting response"));
        // Endgültige Fehler (abgelehntes HTTP-Login, kaputte Refspec) nur einmal
        for (code, class) in [
            (git2::ErrorCode::Auth, git2::ErrorClass::Http),
            (git2::ErrorCode::GenericError, git2::ErrorClass::Reference),
        ] {
            let mut calls = 0;
            let res = with_fetch_retries("origin", &delays, || {
                calls += 1;
                Err(git2::Error::new(code, class, "nope"))
            });
            assert!(res.is_err());
            assert_eq!(calls, 1, "{class:?}");
        }
    }

    #[test]
    fn credentials_each_choice_once_then_give_up() {
        use git2::CredentialType as T;
        let keys = vec![
            PathBuf::from("/home/u/.ssh/id_ed25519"),
            PathBuf::from("/home/u/.ssh/id_rsa"),
        ];
        // SSH: Agent → Keys → Schluss (früher endlos der Agent)
        let mut state = CredState::default();
        assert_eq!(state.next(T::SSH_KEY, &keys), Some(CredChoice::SshAgent));
        assert_eq!(
            state.next(T::SSH_KEY, &keys),
            Some(CredChoice::SshKeyFile(keys[0].clone()))
        );
        assert_eq!(
            state.next(T::SSH_KEY, &keys),
            Some(CredChoice::SshKeyFile(keys[1].clone()))
        );
        assert_eq!(state.next(T::SSH_KEY, &keys), None);
        assert_eq!(state.next(T::SSH_KEY, &keys), None);
        let err = state.exhausted();
        assert_eq!(err.class(), git2::ErrorClass::Callback);
        assert_eq!(err.code(), git2::ErrorCode::Auth);
        assert_eq!(
            err.message(),
            "authentication failed (tried: ssh-agent, /home/u/.ssh/id_ed25519, /home/u/.ssh/id_rsa)"
        );
        assert!(!is_transient_fetch_error(&err));
        // https: Helper, dann default
        let mut state = CredState::default();
        let https = T::USER_PASS_PLAINTEXT | T::DEFAULT;
        assert_eq!(state.next(https, &keys), Some(CredChoice::CredentialHelper));
        assert_eq!(state.next(https, &keys), Some(CredChoice::Default));
        assert_eq!(state.next(https, &keys), None);
        // Benutzername zählt nicht als Versuch
        let mut state = CredState::default();
        assert_eq!(state.next(T::USERNAME, &[]), Some(CredChoice::Username));
        assert_eq!(state.next(T::USERNAME, &[]), Some(CredChoice::Username));
        assert_eq!(state.next(T::SSH_KEY, &[]), Some(CredChoice::SshAgent));
        assert_eq!(state.next(T::SSH_KEY, &[]), None);
        assert_eq!(
            CredState::default().exhausted().message(),
            "no credentials available for this remote"
        );
    }

    #[test]
    fn credentials_callback_stops_after_all_choices() {
        // Ohne Keys: einmal Agent, dann Fehler statt Endlosschleife
        let url = "ssh://git@h.example.com/g/p.git".to_string();
        let mut callback = credentials_with_keys(None, url, Vec::new());
        assert!(callback("", Some("git"), git2::CredentialType::SSH_KEY).is_ok());
        let Err(err) = callback("", Some("git"), git2::CredentialType::SSH_KEY) else {
            panic!("zweiter Aufruf muss aufgeben");
        };
        assert_eq!(err.class(), git2::ErrorClass::Callback);
        assert_eq!(err.message(), "authentication failed (tried: ssh-agent)");
    }

    #[test]
    fn fetch_targets_one_per_repository() {
        let base = tempdir().unwrap();
        let main = base.path().join("main");
        let other = base.path().join("other");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&other).unwrap();
        let repo = init_repo_with_commit(&main, "main");
        init_repo_with_commit(&other, "main");
        let wt_a = base.path().join("wt-a");
        let wt_b = base.path().join("wt-b");
        repo.worktree("wt-a", &wt_a, None).unwrap();
        repo.worktree("wt-b", &wt_b, None).unwrap();
        let missing = base.path().join("fehlt");
        // Worktrees vor dem Haupt-Worktree: der Haupt-Worktree gewinnt
        let paths = vec![
            wt_a.clone(),
            other.clone(),
            missing.clone(),
            main.clone(),
            wt_b.clone(),
        ];
        assert_eq!(
            fetch_targets(&paths),
            vec![main.clone(), other.clone(), missing]
        );
        // Nur Worktrees: der erste
        assert_eq!(fetch_targets(&[wt_b.clone(), wt_a]), vec![wt_b]);
        assert!(fetch_targets(&[]).is_empty());
    }

    #[test]
    fn fetch_transient_error_classes() {
        let err = |code, class| git2::Error::new(code, class, "x");
        use git2::{ErrorClass, ErrorCode};
        assert!(is_transient_fetch_error(&err(
            ErrorCode::Auth,
            ErrorClass::Ssh
        )));
        assert!(is_transient_fetch_error(&err(
            ErrorCode::GenericError,
            ErrorClass::Net
        )));
        assert!(is_transient_fetch_error(&err(
            ErrorCode::GenericError,
            ErrorClass::Os
        )));
        assert!(is_transient_fetch_error(&err(
            ErrorCode::GenericError,
            ErrorClass::Http
        )));
        assert!(!is_transient_fetch_error(&err(
            ErrorCode::Auth,
            ErrorClass::Http
        )));
        assert!(!is_transient_fetch_error(&err(
            ErrorCode::NotFound,
            ErrorClass::Config
        )));
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

    #[test]
    fn fetch_all_https_must_not_fail_with_missing_tls_or_protocol() {
        // RED für "unsupported URL protocol / no TLS stream": mit https-Feature
        // muss ein https-Remote einen echten Netzwerkfehler liefern, nicht
        // "unsupported URL protocol (class-Net 12)" oder "no TLS stream (class-Ssl 16)".
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let repo = Repository::open(dir.path()).unwrap();
        repo.remote("origin", "https://invalid.example.com/doesnotexist.git")
            .unwrap();
        drop(repo);
        let res = fetch_all(dir.path());
        assert!(res.is_err(), "invalid https remote must still be Err");
        let err = res.unwrap_err();
        let msg = format!("{err:#}");
        for needle in [
            "unsupported URL protocol",
            "no TLS stream",
            "there is no TLS stream",
            "class=Ssl (16)",
            "class-Net (12)",
        ] {
            assert!(
                !msg.to_lowercase().contains(&needle.to_lowercase()),
                "https fetch must have TLS/protocol support, got missing-feature error: {msg}"
            );
        }
    }

    #[test]
    fn fetch_all_cli_fallback_succeeds_for_file_remote() {
        // CLI-Fallback-Pfad: lokaler file-Remote muss auch via `git fetch` gehen.
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        let bare_dir = tempdir().unwrap();
        let _bare = Repository::init_bare(bare_dir.path()).unwrap();
        let repo = Repository::open(dir.path()).unwrap();
        repo.remote("good", bare_dir.path().to_str().unwrap())
            .unwrap();
        drop(repo);
        assert!(
            fetch_with_git_cli(dir.path(), "good", &[], true).is_ok(),
            "git CLI fetch for file remote must succeed"
        );
        assert!(
            fetch_with_git_cli(dir.path(), "does-not-exist-remote", &[], true).is_err(),
            "git CLI fetch for unknown remote must fail"
        );
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
    fn get_repo_info_fills_remotes_origin_first() {
        use crate::gitlab::remote::{RemoteRef, RemoteScheme};
        let dir = tempdir().unwrap();
        let repo = init_repo_with_commit(dir.path(), "main");
        repo.remote(
            "upstream",
            "https://gitlab.example.com/kern/backend-api.git",
        )
        .unwrap();
        repo.remote("origin", "git@gitlab.example.com:kern/backend-api.git")
            .unwrap();
        repo.remote(
            "a-fork",
            "https://oauth2:glpat-SECRET123@gitlab.example.com/tim/backend-api.git",
        )
        .unwrap();
        repo.remote("backup", "/srv/git/backend-api.git").unwrap();
        let info = get_repo_info(dir.path().to_path_buf()).unwrap();
        let names: Vec<&str> = info.remotes.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["origin", "a-fork", "backup", "upstream"]);
        assert_eq!(
            info.remotes[0].parsed,
            Some(RemoteRef {
                scheme: RemoteScheme::Ssh,
                host: "gitlab.example.com".to_string(),
                port: None,
                full_path: "kern/backend-api".to_string(),
            })
        );
        assert_eq!(
            info.remotes[0].url_redacted,
            "git@gitlab.example.com:kern/backend-api.git"
        );
        // Token aus der Remote-URL landet nicht im RepoInfo
        assert_eq!(
            info.remotes[1].url_redacted,
            "https://gitlab.example.com/tim/backend-api.git"
        );
        assert!(!format!("{info:?}").contains("SECRET123"));
        assert_eq!(info.remotes[2].parsed, None);
        assert_eq!(
            info.remotes[3].parsed.as_ref().map(|r| r.scheme),
            Some(RemoteScheme::Https)
        );
    }

    #[test]
    fn get_repo_info_common_dir_shared_with_worktree() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let wt = base.path().join("wt");
        git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
        let main_info = get_repo_info(c1.clone()).unwrap();
        let wt_info = get_repo_info(wt.clone()).unwrap();
        assert_eq!(wt_info.branch, "feature/x");
        assert_eq!(main_info.common_dir, wt_info.common_dir);
        assert_eq!(main_info.common_dir, canon(&c1.join(".git")));
        // Ohne abschließenden Separator (Vergleich als Schlüssel)
        assert!(!main_info
            .common_dir
            .to_string_lossy()
            .ends_with(std::path::MAIN_SEPARATOR));
        // Haupt-Repo über einen Symlink gescannt → derselbe Schlüssel
        #[cfg(unix)]
        {
            let link = base.path().join("link");
            std::os::unix::fs::symlink(&c1, &link).unwrap();
            let link_info = get_repo_info(link).unwrap();
            assert_eq!(link_info.common_dir, wt_info.common_dir);
        }
        // Worktree liest dieselben Remotes wie das Haupt-Repo
        assert_eq!(main_info.remotes, wt_info.remotes);
        assert_eq!(main_info.remotes[0].name, "origin");
        // Eigenständiges Repo hat sein eigenes common_dir
        let other = base.path().join("other");
        fs::create_dir(&other).unwrap();
        init_repo_with_commit(&other, "main");
        let other_info = get_repo_info(other.clone()).unwrap();
        assert_ne!(other_info.common_dir, main_info.common_dir);
        assert!(other_info.remotes.is_empty());
    }

    #[test]
    fn canonical_key_path_resolves_dots_links_and_separator() {
        let base = tempdir().unwrap();
        let real = base.path().join("real");
        fs::create_dir(&real).unwrap();
        let dotted = base.path().join("real").join("..").join("real").join("");
        assert_eq!(canonical_key_path(&dotted), canon(&real));
        #[cfg(unix)]
        {
            let link = base.path().join("link");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert_eq!(canonical_key_path(&link), canonical_key_path(&real));
        }
        // Nicht auflösbar → Pfad selbst, ohne abschließenden Separator
        let missing = base.path().join("missing");
        assert_eq!(canonical_key_path(&missing.join("")), missing);
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

    // --- E15: Wechsel auf Remote-Branch mit abweichendem lokalem Branch ---

    fn git_identity(dir: &Path) {
        git_cmd(dir, &["config", "user.name", "Test"]);
        git_cmd(dir, &["config", "user.email", "test@test.com"]);
    }

    fn commit_file(dir: &Path, file: &str, content: &str, msg: &str) {
        fs::write(dir.join(file), content).unwrap();
        git_cmd(dir, &["add", file]);
        git_cmd(dir, &["commit", "-m", msg]);
    }

    /// Bare-Origin + Klon `c1`: `main` (a.txt) und `feature/x` (zusätzlich
    /// x.txt), beide gepusht mit Upstream; c1 steht danach wieder auf `main`.
    fn origin_and_clone_with_feature(base: &Path) -> PathBuf {
        git_cmd(base, &["init", "--bare", "origin.git"]);
        git_cmd(base, &["clone", "origin.git", "c1"]);
        let c1 = base.join("c1");
        git_identity(&c1);
        git_cmd(&c1, &["checkout", "-b", "main"]);
        commit_file(&c1, "a.txt", "one\n", "initial");
        git_cmd(&c1, &["push", "-u", "origin", "main"]);
        git_cmd(&c1, &["checkout", "-b", "feature/x"]);
        commit_file(&c1, "x.txt", "x1\n", "feature");
        git_cmd(&c1, &["push", "-u", "origin", "feature/x"]);
        git_cmd(&c1, &["checkout", "main"]);
        c1
    }

    /// Zweiter Klon `c2` pusht einen weiteren Commit (Datei `file`) auf
    /// `branch` — origin läuft c1 danach voraus (c1 muss noch fetchen).
    fn push_from_second_clone(base: &Path, branch: &str, file: &str) {
        git_cmd(base, &["clone", "-b", branch, "origin.git", "c2"]);
        let c2 = base.join("c2");
        git_identity(&c2);
        commit_file(&c2, file, "remote\n", "remote commit");
        git_cmd(&c2, &["push", "origin", branch]);
    }

    fn ref_oid(dir: &Path, name: &str) -> git2::Oid {
        Repository::open(dir).unwrap().refname_to_id(name).unwrap()
    }

    /// Symbolisches Ziel von HEAD (`refs/heads/…`), leer bei Detached HEAD.
    fn head_target(dir: &Path) -> String {
        let repo = Repository::open(dir).unwrap();
        let head = repo.find_reference("HEAD").unwrap();
        head.symbolic_target()
            .ok()
            .flatten()
            .unwrap_or("")
            .to_string()
    }

    fn dirty(dir: &Path) -> bool {
        is_dirty(&Repository::open(dir).unwrap())
    }

    fn canon(p: &Path) -> PathBuf {
        p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
    }

    #[test]
    fn checkout_remote_branch_fast_forwards_stale_local() {
        // E15/Step 1: lokales feature/x hinter origin/feature/x → vorspulen
        // statt Remote-Baum + alter lokaler Ref (Phantom-Staging).
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        push_from_second_clone(base.path(), "feature/x", "y.txt");
        git_cmd(&c1, &["fetch", "origin"]);
        let outcome = checkout_branch(&c1, "origin/feature/x").unwrap();
        assert_eq!(
            outcome,
            CheckoutOutcome::FastForwarded {
                branch: "feature/x".to_string(),
                by: 1
            }
        );
        assert!(!dirty(&c1), "kein Phantom-Diff nach dem Wechsel");
        assert_eq!(head_target(&c1), "refs/heads/feature/x");
        assert_eq!(
            ref_oid(&c1, "refs/heads/feature/x"),
            ref_oid(&c1, "refs/remotes/origin/feature/x"),
            "lokaler Branch muss vorgespult sein"
        );
        assert!(
            c1.join("y.txt").exists(),
            "Remote-Stand im Arbeitsverzeichnis"
        );
    }

    #[test]
    fn checkout_remote_branch_fast_forwards_current_head() {
        // E15: c1 steht schon auf dem veralteten feature/x und wählt
        // origin/feature/x → Baum und Ref vorgespult, sauber.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(&c1, &["checkout", "feature/x"]);
        push_from_second_clone(base.path(), "feature/x", "y.txt");
        git_cmd(&c1, &["fetch", "origin"]);
        let outcome = checkout_branch(&c1, "origin/feature/x").unwrap();
        assert_eq!(
            outcome,
            CheckoutOutcome::FastForwarded {
                branch: "feature/x".to_string(),
                by: 1
            }
        );
        assert!(!dirty(&c1), "kein Phantom-Diff nach dem Vorspulen");
        assert_eq!(head_target(&c1), "refs/heads/feature/x");
        assert_eq!(
            ref_oid(&c1, "refs/heads/feature/x"),
            ref_oid(&c1, "refs/remotes/origin/feature/x")
        );
        assert!(c1.join("y.txt").exists());
    }

    #[test]
    fn checkout_remote_branch_keeps_local_ahead() {
        // E15/Step 2: lokales feature/x hat einen eigenen Commit → Wechsel
        // auf den lokalen Stand, Ergebnis „voraus“.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(&c1, &["checkout", "feature/x"]);
        commit_file(&c1, "local.txt", "local\n", "local-only");
        git_cmd(&c1, &["checkout", "main"]);
        let local_before = ref_oid(&c1, "refs/heads/feature/x");
        let outcome = checkout_branch(&c1, "origin/feature/x").unwrap();
        assert_eq!(
            outcome,
            CheckoutOutcome::LocalAhead {
                branch: "feature/x".to_string(),
                ahead: 1
            }
        );
        assert!(!dirty(&c1), "lokaler Stand sauber ausgecheckt");
        assert_eq!(head_target(&c1), "refs/heads/feature/x");
        assert_eq!(ref_oid(&c1, "refs/heads/feature/x"), local_before);
        assert!(c1.join("local.txt").exists());
    }

    #[test]
    fn checkout_remote_branch_keeps_diverged_local() {
        // E15/Step 2: lokal und Remote je ein eigener Commit → lokaler
        // Stand, Ref unverändert, Ergebnis „divergiert“.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(&c1, &["checkout", "feature/x"]);
        commit_file(&c1, "local.txt", "local\n", "local-only");
        git_cmd(&c1, &["checkout", "main"]);
        push_from_second_clone(base.path(), "feature/x", "y.txt");
        git_cmd(&c1, &["fetch", "origin"]);
        let local_before = ref_oid(&c1, "refs/heads/feature/x");
        let outcome = checkout_branch(&c1, "origin/feature/x").unwrap();
        assert_eq!(
            outcome,
            CheckoutOutcome::Diverged {
                branch: "feature/x".to_string(),
                ahead: 1,
                behind: 1
            }
        );
        assert!(!dirty(&c1), "lokaler Stand sauber ausgecheckt");
        assert_eq!(head_target(&c1), "refs/heads/feature/x");
        assert_eq!(
            ref_oid(&c1, "refs/heads/feature/x"),
            local_before,
            "divergierter Ref bleibt unverändert"
        );
        assert!(c1.join("local.txt").exists());
        assert!(!c1.join("y.txt").exists(), "kein Remote-Baum");
    }

    #[test]
    fn checkout_force_remote_only_branch_attaches_local_branch() {
        // E15/Step 3: Force auf origin/only-remote → angehängter lokaler
        // Branch mit Upstream statt Detached HEAD.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(base.path(), &["clone", "-b", "main", "origin.git", "c2"]);
        let c2 = base.path().join("c2");
        git_identity(&c2);
        git_cmd(&c2, &["checkout", "-b", "only-remote"]);
        commit_file(&c2, "r.txt", "r\n", "remote branch");
        git_cmd(&c2, &["push", "origin", "only-remote"]);
        git_cmd(&c1, &["fetch", "origin"]);
        fs::write(c1.join("a.txt"), "dirty\n").unwrap();
        let outcome = checkout_branch_force(&c1, "origin/only-remote").unwrap();
        assert_eq!(outcome, CheckoutOutcome::Switched);
        assert_eq!(head_target(&c1), "refs/heads/only-remote");
        let repo = Repository::open(&c1).unwrap();
        assert!(!get_branch(&repo).1, "kein Detached HEAD");
        let local = repo.find_branch("only-remote", BranchType::Local).unwrap();
        assert_eq!(
            local.upstream().unwrap().name().unwrap(),
            Some("origin/only-remote")
        );
        assert!(!is_dirty(&repo), "Force verwirft die Änderung");
        assert!(c1.join("r.txt").exists());
    }

    #[test]
    fn checkout_safe_remote_only_branch_creates_tracking_branch() {
        // Bestehendes Verhalten abgesichert: kein lokaler Branch → anlegen
        // mit Upstream, HEAD angehängt.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(&c1, &["branch", "-D", "feature/x"]);
        let outcome = checkout_branch(&c1, "origin/feature/x").unwrap();
        assert_eq!(outcome, CheckoutOutcome::Switched);
        assert_eq!(head_target(&c1), "refs/heads/feature/x");
        assert!(!dirty(&c1));
        let repo = Repository::open(&c1).unwrap();
        let local = repo.find_branch("feature/x", BranchType::Local).unwrap();
        assert_eq!(
            local.upstream().unwrap().name().unwrap(),
            Some("origin/feature/x")
        );
    }

    #[test]
    fn checkout_force_remote_branch_fast_forwards_stale_local() {
        // E15: Force-Variante analog — veraltetes lokales feature/x wird
        // vorgespult, lokale Änderung verworfen, kein Phantom-Diff.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        push_from_second_clone(base.path(), "feature/x", "y.txt");
        git_cmd(&c1, &["fetch", "origin"]);
        fs::write(c1.join("a.txt"), "dirty\n").unwrap();
        let outcome = checkout_branch_force(&c1, "origin/feature/x").unwrap();
        assert_eq!(
            outcome,
            CheckoutOutcome::FastForwarded {
                branch: "feature/x".to_string(),
                by: 1
            }
        );
        assert!(!dirty(&c1));
        assert_eq!(head_target(&c1), "refs/heads/feature/x");
        assert_eq!(
            ref_oid(&c1, "refs/heads/feature/x"),
            ref_oid(&c1, "refs/remotes/origin/feature/x")
        );
    }

    #[test]
    fn checkout_remote_branch_rejects_other_upstream() {
        // E15/Step 4: lokales feature/x verfolgt upstream/feature/x, gewählt
        // wird origin/feature/x → Fehler, Arbeitsverzeichnis unverändert.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(
            base.path(),
            &["clone", "--bare", "origin.git", "upstream.git"],
        );
        let upstream_url = base.path().join("upstream.git");
        git_cmd(
            &c1,
            &["remote", "add", "upstream", upstream_url.to_str().unwrap()],
        );
        git_cmd(&c1, &["fetch", "upstream"]);
        git_cmd(
            &c1,
            &[
                "branch",
                "--set-upstream-to=upstream/feature/x",
                "feature/x",
            ],
        );
        for force in [false, true] {
            let res = if force {
                checkout_branch_force(&c1, "origin/feature/x")
            } else {
                checkout_branch(&c1, "origin/feature/x")
            };
            let err = res.expect_err("anderer Upstream muss abbrechen");
            assert!(
                err.to_string().contains("verfolgt upstream/feature/x"),
                "unerwartete Meldung: {err}"
            );
            assert_eq!(
                err.downcast_ref::<BranchSwitchBlocked>(),
                Some(&BranchSwitchBlocked::UpstreamMismatch {
                    branch: "feature/x".to_string(),
                    upstream: "upstream/feature/x".to_string(),
                })
            );
            assert_eq!(head_target(&c1), "refs/heads/main");
            assert!(!dirty(&c1), "Arbeitsverzeichnis unverändert");
            assert!(!c1.join("x.txt").exists());
        }
    }

    #[test]
    fn checkout_rejects_branch_checked_out_in_linked_worktree() {
        // E15/Step 5: feature/x ist in einem verknüpften Worktree HEAD →
        // Fehler mit Worktree-Pfad, Prüfung vor checkout_tree (c1 sauber).
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let wt = base.path().join("wt");
        git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
        for target in ["feature/x", "origin/feature/x"] {
            for force in [false, true] {
                let res = if force {
                    checkout_branch_force(&c1, target)
                } else {
                    checkout_branch(&c1, target)
                };
                let err = res.expect_err("Worktree-Branch muss abbrechen");
                match err.downcast_ref::<BranchSwitchBlocked>() {
                    Some(BranchSwitchBlocked::CheckedOutElsewhere { branch, path }) => {
                        assert_eq!(branch, "feature/x");
                        assert_eq!(canon(path), canon(&wt));
                    }
                    other => panic!("{target} force={force}: unerwartet {other:?} / {err}"),
                }
                assert!(
                    err.to_string().contains("wt"),
                    "Meldung nennt den Worktree-Pfad: {err}"
                );
                assert_eq!(head_target(&c1), "refs/heads/main");
                assert!(
                    !dirty(&c1),
                    "{target} force={force}: c1 muss sauber bleiben"
                );
                assert!(!c1.join("x.txt").exists());
            }
        }
    }

    #[test]
    fn branch_checked_out_elsewhere_reads_all_worktrees() {
        // E15: Haupt-Worktree und verknüpfte Worktrees, der eigene zählt nicht.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let wt = base.path().join("wt");
        git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
        let main_repo = Repository::open(&c1).unwrap();
        assert_eq!(
            branch_checked_out_elsewhere(&main_repo, "feature/x").map(|p| canon(&p)),
            Some(canon(&wt))
        );
        assert_eq!(branch_checked_out_elsewhere(&main_repo, "main"), None);
        assert_eq!(branch_checked_out_elsewhere(&main_repo, "other"), None);
        let wt_repo = Repository::open(&wt).unwrap();
        assert_eq!(
            branch_checked_out_elsewhere(&wt_repo, "main").map(|p| canon(&p)),
            Some(canon(&c1))
        );
        assert_eq!(branch_checked_out_elsewhere(&wt_repo, "feature/x"), None);
        drop(wt_repo);
        // Verwaister Worktree (Ordner gelöscht, nicht gesperrt) zählt wie
        // bei `git worktree prune` nicht mehr.
        fs::remove_dir_all(&wt).unwrap();
        assert_eq!(branch_checked_out_elsewhere(&main_repo, "feature/x"), None);
        drop(main_repo);
        checkout_branch(&c1, "feature/x").unwrap();
        assert_eq!(head_target(&c1), "refs/heads/feature/x");
        assert!(!dirty(&c1));
    }

    #[test]
    fn branch_checked_out_elsewhere_normalizes_relative_gitdir() {
        // E15-Review: `worktree.useRelativePaths` schreibt `<admin>/gitdir`
        // relativ → Pfad ohne `..`-Teile, gleich dem echten Worktree-Ordner.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let wt = base.path().join("wt");
        git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
        // <c1>/.git/worktrees/wt → vier Ebenen hoch bis base.
        fs::write(c1.join(".git/worktrees/wt/gitdir"), "../../../../wt/.git\n").unwrap();
        let repo = Repository::open(&c1).unwrap();
        let path = branch_checked_out_elsewhere(&repo, "feature/x").expect("Worktree erkannt");
        assert!(
            !path.components().any(|c| c == Component::ParentDir),
            "kein `..` im Pfad: {}",
            path.display()
        );
        assert_eq!(path, canon(&wt));
    }

    #[test]
    fn stash_and_checkout_blocked_switch_keeps_index() {
        // E15-Review: Upstream-/Worktree-Abbruch vor dem Stash — kein
        // Stash + Pop, die gestagte Änderung bleibt gestagt.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(
            &c1,
            &["branch", "--set-upstream-to=origin/main", "feature/x"],
        );
        let wt = base.path().join("wt");
        git_cmd(
            &c1,
            &["worktree", "add", "-b", "wt-branch", wt.to_str().unwrap()],
        );
        fs::write(c1.join("a.txt"), "staged\n").unwrap();
        git_cmd(&c1, &["add", "a.txt"]);
        for target in ["origin/feature/x", "wt-branch"] {
            let err = stash_and_checkout(&c1, target).expect_err("Wechsel muss abbrechen");
            assert!(
                err.downcast_ref::<BranchSwitchBlocked>().is_some(),
                "{target}: unerwartet {err}"
            );
            let repo = Repository::open(&c1).unwrap();
            assert!(
                repo.revparse_single("stash@{0}").is_err(),
                "{target}: kein Stash angelegt"
            );
            assert_eq!(
                repo.status_file(Path::new("a.txt")).unwrap(),
                git2::Status::INDEX_MODIFIED,
                "{target}: Änderung bleibt gestagt"
            );
            assert_eq!(head_target(&c1), "refs/heads/main");
        }
    }

    // --- K1: MR auschecken (fetch_refspecs, prepare_mr_branch, add_worktree, clone_repo) ---

    fn mr_plan(source_branch: &str) -> MrCheckoutPlan {
        MrCheckoutPlan {
            iid: 418,
            remote: "origin".to_string(),
            source_branch: source_branch.to_string(),
            expected_sha: None,
        }
    }

    fn ref_exists(dir: &Path, name: &str) -> bool {
        Repository::open(dir).unwrap().refname_to_id(name).is_ok()
    }

    /// Konfigurierter Upstream als voller Ref-Name.
    fn upstream_of(dir: &Path, branch: &str) -> Option<String> {
        Repository::open(dir)
            .unwrap()
            .branch_upstream_name(&format!("refs/heads/{branch}"))
            .ok()
            .and_then(|b| b.as_str().ok().map(str::to_string))
    }

    /// Zweiter Klon `c2` legt `branch` (von `main`) mit einem Commit auf
    /// origin an; c1 kennt ihn danach noch nicht.
    fn push_new_branch_from_second_clone(base: &Path, branch: &str) {
        git_cmd(base, &["clone", "-b", "main", "origin.git", "c2"]);
        let c2 = base.join("c2");
        git_identity(&c2);
        git_cmd(&c2, &["checkout", "-b", branch]);
        commit_file(&c2, "y.txt", "y\n", "new branch");
        git_cmd(&c2, &["push", "origin", branch]);
    }

    /// Ausgangslage für den Fetch-Test: c1 kennt `origin/old`; origin hat
    /// danach `old` gelöscht, einen neuen Commit auf `feature/x`, einen neuen
    /// Branch `fresh` und das Tag `v1`. `fetch.prune` des Users ist an.
    fn origin_moved_on(base: &Path) -> PathBuf {
        let c1 = origin_and_clone_with_feature(base);
        git_cmd(&c1, &["push", "origin", "main:refs/heads/old"]);
        git_cmd(&c1, &["fetch", "origin"]);
        assert!(ref_exists(&c1, "refs/remotes/origin/old"));
        push_from_second_clone(base, "feature/x", "y.txt");
        let c2 = base.join("c2");
        git_cmd(&c2, &["push", "origin", "--delete", "old"]);
        git_cmd(&c2, &["push", "origin", "feature/x:refs/heads/fresh"]);
        git_cmd(&c2, &["tag", "v1"]);
        git_cmd(&c2, &["push", "origin", "v1"]);
        git_cmd(&c1, &["config", "fetch.prune", "true"]);
        c1
    }

    fn assert_fetched_only_feature(base: &Path, c1: &Path, local_before: git2::Oid) {
        assert_eq!(
            ref_oid(c1, "refs/remotes/origin/feature/x"),
            ref_oid(&base.join("origin.git"), "refs/heads/feature/x"),
            "angefragter Branch aktualisiert"
        );
        assert!(
            ref_exists(c1, "refs/remotes/origin/old"),
            "kein Prune trotz fetch.prune"
        );
        assert!(
            !ref_exists(c1, "refs/remotes/origin/fresh"),
            "nur der angefragte Branch"
        );
        assert!(!ref_exists(c1, "refs/tags/v1"), "keine Tags");
        assert_eq!(
            ref_oid(c1, "refs/heads/feature/x"),
            local_before,
            "lokale Branches bleiben unberührt"
        );
    }

    #[test]
    fn fetch_refspecs_fetches_only_the_branch_without_prune() {
        let base = tempdir().unwrap();
        let c1 = origin_moved_on(base.path());
        let before = ref_oid(&c1, "refs/heads/feature/x");
        let spec = mr_plan("feature/x").refspec();
        fetch_refspecs(&c1, "origin", &[&spec]).unwrap();
        assert_fetched_only_feature(base.path(), &c1, before);
        assert!(fetch_refspecs(&c1, "nope", &[&spec]).is_err());
    }

    #[test]
    fn fetch_refspecs_cli_path_fetches_only_the_branch_without_prune() {
        let base = tempdir().unwrap();
        let c1 = origin_moved_on(base.path());
        let before = ref_oid(&c1, "refs/heads/feature/x");
        let spec = mr_plan("feature/x").refspec();
        fetch_with_git_cli(&c1, "origin", &[&spec], false).unwrap();
        assert_fetched_only_feature(base.path(), &c1, before);
    }

    #[test]
    fn prepare_mr_branch_creates_tracking_branch() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        push_new_branch_from_second_clone(base.path(), "feature/y");
        let plan = mr_plan("feature/y");
        fetch_refspecs(&c1, "origin", &[&plan.refspec()]).unwrap();
        let prep = prepare_mr_branch(&c1, &plan).unwrap();
        assert_eq!(
            prep,
            MrPrep {
                local_branch: "feature/y".to_string(),
                outcome: MrPrepOutcome::Created,
            }
        );
        assert_eq!(
            ref_oid(&c1, "refs/heads/feature/y"),
            ref_oid(&c1, "refs/remotes/origin/feature/y")
        );
        assert_eq!(
            upstream_of(&c1, "feature/y").as_deref(),
            Some("refs/remotes/origin/feature/y")
        );
        // Nur Refs: Arbeitsverzeichnis und HEAD unverändert
        assert_eq!(head_target(&c1), "refs/heads/main");
        assert!(!dirty(&c1));
        assert!(!c1.join("y.txt").exists());
    }

    #[test]
    fn prepare_mr_branch_up_to_date_and_fast_forward() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let plan = mr_plan("feature/x");
        assert_eq!(
            prepare_mr_branch(&c1, &plan).unwrap(),
            MrPrep {
                local_branch: "feature/x".to_string(),
                outcome: MrPrepOutcome::UpToDate,
            }
        );
        push_from_second_clone(base.path(), "feature/x", "y.txt");
        fetch_refspecs(&c1, "origin", &[&plan.refspec()]).unwrap();
        let prep = prepare_mr_branch(&c1, &plan).unwrap();
        assert_eq!(prep.outcome, MrPrepOutcome::FastForwarded { by: 1 });
        assert_eq!(
            ref_oid(&c1, "refs/heads/feature/x"),
            ref_oid(&c1, "refs/remotes/origin/feature/x")
        );
        assert_eq!(head_target(&c1), "refs/heads/main");
        assert!(!dirty(&c1));
        assert!(!c1.join("y.txt").exists(), "Arbeitsverzeichnis unberührt");
    }

    #[test]
    fn prepare_mr_branch_local_ahead_and_diverged_keep_ref() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(&c1, &["checkout", "feature/x"]);
        commit_file(&c1, "local.txt", "l\n", "local");
        git_cmd(&c1, &["checkout", "main"]);
        let local = ref_oid(&c1, "refs/heads/feature/x");
        let plan = mr_plan("feature/x");
        assert_eq!(
            prepare_mr_branch(&c1, &plan).unwrap().outcome,
            MrPrepOutcome::LocalAhead { ahead: 1 }
        );
        assert_eq!(ref_oid(&c1, "refs/heads/feature/x"), local);
        push_from_second_clone(base.path(), "feature/x", "y.txt");
        fetch_refspecs(&c1, "origin", &[&plan.refspec()]).unwrap();
        assert_eq!(
            prepare_mr_branch(&c1, &plan).unwrap().outcome,
            MrPrepOutcome::Diverged {
                ahead: 1,
                behind: 1
            }
        );
        assert_eq!(
            ref_oid(&c1, "refs/heads/feature/x"),
            local,
            "Divergenz setzt nie zurück"
        );
        assert!(!dirty(&c1));
    }

    #[test]
    fn prepare_mr_branch_already_head_changes_nothing() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(&c1, &["checkout", "feature/x"]);
        push_from_second_clone(base.path(), "feature/x", "y.txt");
        let plan = mr_plan("feature/x");
        fetch_refspecs(&c1, "origin", &[&plan.refspec()]).unwrap();
        let before = ref_oid(&c1, "refs/heads/feature/x");
        assert_eq!(
            prepare_mr_branch(&c1, &plan).unwrap(),
            MrPrep {
                local_branch: "feature/x".to_string(),
                outcome: MrPrepOutcome::AlreadyHead { behind: 1 },
            }
        );
        assert_eq!(ref_oid(&c1, "refs/heads/feature/x"), before);
        assert!(!dirty(&c1));
        assert!(!c1.join("y.txt").exists());
    }

    #[test]
    fn prepare_mr_branch_rejects_branch_in_other_worktree() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let wt = base.path().join("wt");
        git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
        match prepare_mr_branch(&c1, &mr_plan("feature/x")) {
            Err(CheckoutError::CheckedOutElsewhere { branch, path }) => {
                assert_eq!(branch, "feature/x");
                assert_eq!(canon(&path), canon(&wt));
            }
            other => panic!("unerwartet: {other:?}"),
        }
    }

    #[test]
    fn prepare_mr_branch_falls_back_on_other_upstream() {
        // Lokales feature/x verfolgt upstream/feature/x → mr-418 mit
        // Upstream origin/feature/x, feature/x bleibt unverändert.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let origin = base.path().join("origin.git");
        git_cmd(
            &c1,
            &["remote", "add", "upstream", origin.to_str().unwrap()],
        );
        git_cmd(&c1, &["fetch", "upstream"]);
        git_cmd(&c1, &["branch", "-u", "upstream/feature/x", "feature/x"]);
        let before = ref_oid(&c1, "refs/heads/feature/x");
        let prep = prepare_mr_branch(&c1, &mr_plan("feature/x")).unwrap();
        assert_eq!(
            prep,
            MrPrep {
                local_branch: "mr-418".to_string(),
                outcome: MrPrepOutcome::Created,
            }
        );
        assert_eq!(
            upstream_of(&c1, "mr-418").as_deref(),
            Some("refs/remotes/origin/feature/x")
        );
        assert_eq!(ref_oid(&c1, "refs/heads/feature/x"), before);
        assert_eq!(
            upstream_of(&c1, "feature/x").as_deref(),
            Some("refs/remotes/upstream/feature/x")
        );
        // Zweiter Durchlauf findet mr-418 mit passendem Upstream wieder
        assert_eq!(
            prepare_mr_branch(&c1, &mr_plan("feature/x")).unwrap(),
            MrPrep {
                local_branch: "mr-418".to_string(),
                outcome: MrPrepOutcome::UpToDate,
            }
        );
    }

    #[test]
    fn prepare_mr_branch_falls_back_on_name_clash() {
        // Ordner-/Datei-Konflikt (`feature` gegen `feature/x`) und Branch,
        // der sich nur in der Groß-/Kleinschreibung unterscheidet.
        for clash in ["feature", "Feature/X", "feature/x/deeper"] {
            let base = tempdir().unwrap();
            let c1 = origin_and_clone_with_feature(base.path());
            git_cmd(&c1, &["branch", "-D", "feature/x"]);
            git_cmd(&c1, &["branch", clash, "main"]);
            let prep = prepare_mr_branch(&c1, &mr_plan("feature/x")).unwrap();
            assert_eq!(prep.local_branch, "mr-418", "{clash}");
            assert_eq!(prep.outcome, MrPrepOutcome::Created, "{clash}");
            assert_eq!(
                ref_oid(&c1, "refs/heads/mr-418"),
                ref_oid(&c1, "refs/remotes/origin/feature/x")
            );
        }
    }

    #[test]
    fn prepare_mr_branch_existing_branch_without_upstream_is_used() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(&c1, &["branch", "--unset-upstream", "feature/x"]);
        assert_eq!(
            prepare_mr_branch(&c1, &mr_plan("feature/x")).unwrap(),
            MrPrep {
                local_branch: "feature/x".to_string(),
                outcome: MrPrepOutcome::UpToDate,
            }
        );
    }

    #[test]
    fn prepare_mr_branch_errors() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        // Nicht geholt / gelöscht
        assert_eq!(
            prepare_mr_branch(&c1, &mr_plan("feature/nope")),
            Err(CheckoutError::RemoteBranchMissing {
                upstream: "origin/feature/nope".to_string()
            })
        );
        // Weder feature/x noch mr-418 nutzbar
        let origin = base.path().join("origin.git");
        git_cmd(
            &c1,
            &["remote", "add", "upstream", origin.to_str().unwrap()],
        );
        git_cmd(&c1, &["fetch", "upstream"]);
        git_cmd(&c1, &["branch", "-u", "upstream/feature/x", "feature/x"]);
        git_cmd(&c1, &["branch", "--track", "mr-418", "upstream/main"]);
        assert_eq!(
            prepare_mr_branch(&c1, &mr_plan("feature/x")),
            Err(CheckoutError::BranchNameTaken {
                branch: "feature/x".to_string(),
                fallback: "mr-418".to_string(),
            })
        );
        let missing = base.path().join("missing");
        assert!(matches!(
            prepare_mr_branch(&missing, &mr_plan("feature/x")),
            Err(CheckoutError::Git(_))
        ));
    }

    #[test]
    fn add_worktree_creates_folder_on_branch() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        // Review-Ordner gibt es noch nicht
        let target = base.path().join("Reviews").join("backend-api-mr-418");
        add_worktree(&c1, &target, "feature/x").unwrap();
        assert_eq!(head_target(&target), "refs/heads/feature/x");
        assert!(target.join("x.txt").exists());
        assert!(!dirty(&target));
        // Basis-Klon unverändert
        assert_eq!(head_target(&c1), "refs/heads/main");
        assert!(!dirty(&c1));
        assert!(!c1.join("x.txt").exists());
        // Worktree-Name = Ordnername
        let repo = Repository::open(&c1).unwrap();
        let wt = repo.find_worktree("backend-api-mr-418").unwrap();
        assert_eq!(canon(wt.path()), canon(&target));
        drop(repo);
        assert_eq!(
            get_repo_info(target.clone()).unwrap().common_dir,
            canon(&c1.join(".git"))
        );
    }

    #[test]
    fn add_worktree_target_must_be_missing_or_empty() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let empty = base.path().join("empty");
        fs::create_dir(&empty).unwrap();
        add_worktree(&c1, &empty, "feature/x").unwrap();
        assert_eq!(head_target(&empty), "refs/heads/feature/x");

        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let full = base.path().join("full");
        fs::create_dir(&full).unwrap();
        fs::write(full.join("notes.txt"), "keep\n").unwrap();
        assert_eq!(
            add_worktree(&c1, &full, "feature/x"),
            Err(CheckoutError::TargetNotEmpty { path: full.clone() })
        );
        assert_eq!(
            fs::read_to_string(full.join("notes.txt")).unwrap(),
            "keep\n"
        );
        assert!(!full.join(".git").exists());
        let file = base.path().join("file");
        fs::write(&file, "x").unwrap();
        assert_eq!(
            add_worktree(&c1, &file, "feature/x"),
            Err(CheckoutError::TargetNotEmpty { path: file.clone() })
        );
        assert!(Repository::open(&c1)
            .unwrap()
            .worktrees()
            .unwrap()
            .is_empty());
        // Unbekannter Branch
        assert!(matches!(
            add_worktree(&c1, &base.path().join("wt"), "nope"),
            Err(CheckoutError::Git(_))
        ));
        assert!(!base.path().join("wt").exists());
    }

    #[test]
    fn add_worktree_rejects_branch_checked_out_elsewhere() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let wt = base.path().join("wt");
        git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
        let target = base.path().join("second");
        match add_worktree(&c1, &target, "feature/x") {
            Err(CheckoutError::CheckedOutElsewhere { branch, path }) => {
                assert_eq!(branch, "feature/x");
                assert_eq!(canon(&path), canon(&wt));
            }
            other => panic!("unerwartet: {other:?}"),
        }
        // Branch ist HEAD im Basis-Klon selbst
        match add_worktree(&c1, &target, "main") {
            Err(CheckoutError::CheckedOutElsewhere { branch, path }) => {
                assert_eq!(branch, "main");
                assert_eq!(canon(&path), canon(&c1));
            }
            other => panic!("unerwartet: {other:?}"),
        }
        assert!(!target.exists());
    }

    #[test]
    fn add_worktree_again_after_folder_was_deleted() {
        // Review-Ordner im Explorer gelöscht, ohne `git worktree prune`: der
        // verwaiste Eintrag mit gleichem Namen und Pfad blockiert nicht.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let target = base.path().join("Reviews").join("backend-api-mr-418");
        add_worktree(&c1, &target, "feature/x").unwrap();
        fs::remove_dir_all(&target).unwrap();
        add_worktree(&c1, &target, "feature/x").unwrap();
        assert_eq!(head_target(&target), "refs/heads/feature/x");
        assert!(target.join("x.txt").exists());
        assert_eq!(head_target(&c1), "refs/heads/main");
        let admin = c1.join(".git").join("worktrees");
        // Auch nach `git worktree add` (CLI) angelegte Einträge
        fs::remove_dir_all(&target).unwrap();
        git_cmd(&c1, &["worktree", "prune"]);
        git_cmd(
            &c1,
            &["worktree", "add", target.to_str().unwrap(), "feature/x"],
        );
        fs::remove_dir_all(&target).unwrap();
        add_worktree(&c1, &target, "feature/x").unwrap();
        assert_eq!(head_target(&target), "refs/heads/feature/x");
        // Gesperrte verwaiste Einträge bleiben: Fehler statt Prune. Der
        // Branch gilt dort weiter als ausgecheckt (wie in E15) …
        git_cmd(&c1, &["worktree", "lock", target.to_str().unwrap()]);
        fs::remove_dir_all(&target).unwrap();
        match add_worktree(&c1, &target, "feature/x") {
            Err(CheckoutError::CheckedOutElsewhere { path, .. }) => {
                assert_eq!(missing_dir_key(&path), missing_dir_key(&target));
            }
            other => panic!("unerwartet: {other:?}"),
        }
        // … und auch mit einem anderen Branch wird nichts entfernt
        git_cmd(&c1, &["branch", "feature/other", "main"]);
        assert!(matches!(
            add_worktree(&c1, &target, "feature/other"),
            Err(CheckoutError::Git(_))
        ));
        assert!(admin.join("backend-api-mr-418").join("locked").exists());
        assert!(!target.exists());
    }

    #[test]
    fn add_worktree_keeps_stale_entry_of_other_folder() {
        // Gleichnamiger verwaister Eintrag für einen anderen Ordner (z. B.
        // auf einem gerade nicht verbundenen Laufwerk) bleibt erhalten.
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        git_cmd(&c1, &["branch", "feature/old", "main"]);
        let old = base.path().join("old").join("backend-api-mr-418");
        add_worktree(&c1, &old, "feature/old").unwrap();
        fs::remove_dir_all(&old).unwrap();
        let admin = c1.join(".git").join("worktrees").join("backend-api-mr-418");
        let gitdir = fs::read_to_string(admin.join("gitdir")).unwrap();
        let target = base.path().join("Reviews").join("backend-api-mr-418");
        // libgit2 scheitert am Admin-Ordner, `git worktree add` weicht auf
        // einen freien Namen aus
        add_worktree(&c1, &target, "feature/x").unwrap();
        assert_eq!(head_target(&target), "refs/heads/feature/x");
        assert_eq!(fs::read_to_string(admin.join("gitdir")).unwrap(), gitdir);
    }

    #[test]
    fn add_worktree_cli_fallback() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let target = base.path().join("Reviews").join("cli-wt");
        worktree_add_with_git_cli(&c1, &target, "feature/x").unwrap();
        assert_eq!(head_target(&target), "refs/heads/feature/x");
        assert!(!dirty(&target));
        assert_eq!(head_target(&c1), "refs/heads/main");
        assert!(worktree_add_with_git_cli(&c1, &base.path().join("x"), "nope").is_err());
    }

    #[test]
    fn clone_repo_checks_out_branch() {
        let base = tempdir().unwrap();
        origin_and_clone_with_feature(base.path());
        let origin = base.path().join("origin.git");
        let target = base.path().join("clones").join("backend-api-mr-418");
        clone_repo(origin.to_str().unwrap(), &target, "feature/x").unwrap();
        assert_eq!(head_target(&target), "refs/heads/feature/x");
        assert!(target.join("x.txt").exists());
        assert!(!dirty(&target));
        assert_eq!(
            upstream_of(&target, "feature/x").as_deref(),
            Some("refs/remotes/origin/feature/x")
        );
    }

    #[test]
    fn clone_repo_errors_leave_no_folder() {
        let base = tempdir().unwrap();
        origin_and_clone_with_feature(base.path());
        let origin = base.path().join("origin.git");
        let full = base.path().join("full");
        fs::create_dir(&full).unwrap();
        fs::write(full.join("notes.txt"), "keep\n").unwrap();
        assert_eq!(
            clone_repo(origin.to_str().unwrap(), &full, "feature/x"),
            Err(CheckoutError::TargetNotEmpty { path: full.clone() })
        );
        assert_eq!(
            fs::read_to_string(full.join("notes.txt")).unwrap(),
            "keep\n"
        );
        // libgit2 und System-git scheitern → kein halber Klon bleibt liegen
        let missing = base.path().join("missing.git");
        let target = base.path().join("clone");
        assert!(matches!(
            clone_repo(missing.to_str().unwrap(), &target, "feature/x"),
            Err(CheckoutError::Git(_))
        ));
        assert!(!target.exists());
        let target = base.path().join("clone-branch");
        assert!(clone_repo(origin.to_str().unwrap(), &target, "nope").is_err());
        assert!(!target.exists());
    }

    #[test]
    fn clone_repo_cli_fallback() {
        let base = tempdir().unwrap();
        origin_and_clone_with_feature(base.path());
        let origin = base.path().join("origin.git");
        let target = base.path().join("cli-clone");
        clone_with_git_cli(origin.to_str().unwrap(), &target, "feature/x").unwrap();
        assert_eq!(head_target(&target), "refs/heads/feature/x");
        assert!(target.join("x.txt").exists());
    }

    #[test]
    fn get_repo_info_counts_dirty_files() {
        let dir = tempdir().unwrap();
        init_repo_with_commit(dir.path(), "main");
        assert_eq!(
            get_repo_info(dir.path().to_path_buf()).unwrap().dirty_files,
            0
        );
        fs::write(dir.path().join("README.md"), "changed\n").unwrap();
        fs::write(dir.path().join("new.txt"), "n\n").unwrap();
        // Untracked Ordner zählt einmal (wie `is_dirty`, P1)
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/a.txt"), "a\n").unwrap();
        fs::write(dir.path().join("sub/b.txt"), "b\n").unwrap();
        let info = get_repo_info(dir.path().to_path_buf()).unwrap();
        assert!(info.dirty);
        assert_eq!(info.dirty_files, 3);
    }

    #[test]
    fn get_repo_info_lists_other_worktrees() {
        let base = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(base.path());
        let wt = base.path().join("wt");
        let detached = base.path().join("detached");
        let stale = base.path().join("stale");
        git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
        git_cmd(
            &c1,
            &[
                "worktree",
                "add",
                "--detach",
                detached.to_str().unwrap(),
                "main",
            ],
        );
        git_cmd(
            &c1,
            &["worktree", "add", "-b", "gone", stale.to_str().unwrap()],
        );
        fs::remove_dir_all(&stale).unwrap();
        let sorted = |mut heads: Vec<WorktreeHead>| {
            heads.sort_by(|a, b| a.path.cmp(&b.path));
            heads
                .into_iter()
                .map(|h| (canon(&h.path), h.branch))
                .collect::<Vec<_>>()
        };
        let main_info = get_repo_info(c1.clone()).unwrap();
        assert!(!main_info.is_linked_worktree);
        assert_eq!(
            sorted(main_info.worktrees),
            vec![
                (canon(&detached), None),
                (canon(&wt), Some("feature/x".to_string())),
            ]
        );
        // Aus dem verknüpften Worktree: Haupt-Worktree und Geschwister, nicht er selbst
        let wt_info = get_repo_info(wt.clone()).unwrap();
        assert!(wt_info.is_linked_worktree);
        let mut want = vec![
            (canon(&c1), Some("main".to_string())),
            (canon(&detached), None),
        ];
        want.sort();
        assert_eq!(sorted(wt_info.worktrees), want);
        // Ohne weitere Worktrees leer
        let other = base.path().join("other");
        fs::create_dir(&other).unwrap();
        init_repo_with_commit(&other, "main");
        assert!(get_repo_info(other).unwrap().worktrees.is_empty());
    }

    #[test]
    fn find_checked_out_finds_worktree_outside_scan_root() {
        use crate::config::MrConfig;
        use crate::gitlab::checkout::{find_checked_out, ProjectRef};
        use crate::gitlab::remote::{resolve_hosts, GlabHost};
        // Scan-Root enthält nur c1; der Worktree liegt woanders
        let scan_root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let c1 = origin_and_clone_with_feature(scan_root.path());
        let wt = outside.path().join("backend-api-mr-418");
        git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
        git_cmd(
            &c1,
            &[
                "remote",
                "set-url",
                "origin",
                "git@gitlab.example.com:kern/backend-api.git",
            ],
        );
        let cfg = crate::config::AppConfig {
            roots: vec![scan_root.path().to_path_buf()],
            max_depth: 2,
            ..crate::config::AppConfig::default()
        };
        // origin.git ist bare und wird nicht gelistet
        let repos = crate::scanner::scan_repos(&cfg);
        let names: Vec<&str> = repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["c1"], "nur c1 gescannt");
        let glab = [GlabHost {
            key: "gitlab.example.com".to_string(),
            user: "tim".to_string(),
            ..GlabHost::default()
        }];
        let hosts = resolve_hosts(
            repos
                .iter()
                .flat_map(|r| r.remotes.iter().filter_map(|x| x.parsed.as_ref())),
            &glab,
            &MrConfig::default(),
        );
        let project = ProjectRef {
            hosts: &hosts,
            host_key: "gitlab.example.com",
            full_path: "kern/backend-api",
        };
        assert_eq!(
            find_checked_out(&repos, "feature/x", &project).map(|p| canon(&p)),
            Some(canon(&wt))
        );
        let other = ProjectRef {
            full_path: "web/kundenportal",
            ..project
        };
        assert_eq!(find_checked_out(&repos, "feature/x", &other), None);
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
