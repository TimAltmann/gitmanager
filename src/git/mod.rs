//! Git über libgit2 (`git2`): Repo-Zustand lesen, Branches, Vorsprung und
//! Rückstand. Die Untermodule decken Fetch, Branch-Wechsel, Worktrees,
//! Aufräumen sowie den Start von IDE, Agent und Terminal ab; alle
//! öffentlichen Namen sind hier wieder exportiert (`crate::git::…`).

use crate::gitlab::remote::RemoteInfo;
use git2::{BranchType, Repository, StatusOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod checkout;
mod cleanup;
mod fetch;
mod ide_discovery;
mod launch;
mod worktree;

pub use checkout::*;
pub use cleanup::*;
pub use fetch::*;
pub use ide_discovery::*;
pub use launch::*;
pub use worktree::*;

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
    /// Letzter Commit auf HEAD (Betreff, Autor, Zeit). Scan-only, Default `None`.
    pub last_commit: Option<LastCommit>,
    /// Zustand des Repos für den Status-Chip. Scan-only, Default `Dirty`
    /// bei `dirty`, sonst `Clean`.
    pub state: RepoState,
}

/// Letzter Commit auf HEAD (`RepoInfo.last_commit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastCommit {
    pub summary: String,
    pub author: String,
    /// Unix-Sekunden
    pub time: i64,
}

/// Art einer laufenden Git-Operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InProgressKind {
    #[default]
    Other,
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

/// Zustand eines Repos: Konflikte vor laufender Operation vor Dirty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepoState {
    #[default]
    Clean,
    Dirty,
    Conflict,
    InProgress(InProgressKind),
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
            last_commit: None,
            state: if dirty {
                RepoState::Dirty
            } else {
                RepoState::Clean
            },
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

/// `user.email` aus der globalen git-Konfiguration (kleingeschrieben).
pub fn global_user_email() -> Option<String> {
    let cfg = git2::Config::open_default().ok()?;
    let email = cfg.get_string("user.email").ok()?;
    let email = email.trim().to_ascii_lowercase();
    email.contains('@').then_some(email)
}

/// Ob HEAD in `path` den Commit `sha` schon enthält (gleich oder
/// Nachfahre). `Some(false)`, wenn der Commit lokal fehlt oder HEAD ihn
/// nicht enthält; `None`, wenn sich das nicht prüfen lässt (kein Repo,
/// ungültige SHA, kein HEAD).
pub fn head_contains(path: &Path, sha: &str) -> Option<bool> {
    let repo = Repository::open(path).ok()?;
    let oid = git2::Oid::from_str(sha).ok()?;
    let head = repo.head().ok()?.peel_to_commit().ok()?.id();
    if head == oid {
        return Some(true);
    }
    if repo.find_commit(oid).is_err() {
        return Some(false);
    }
    Some(repo.graph_descendant_of(head, oid).unwrap_or(false))
}

/// Detaillierter Status: staged, unstaged, untracked, Konflikte
pub fn has_merge_conflicts(repo: &Repository) -> bool {
    repo.index().map(|idx| idx.has_conflicts()).unwrap_or(false)
}

/// Zustand des Repos: Konflikte zuerst, dann laufende Operation, dann Dirty.
pub fn repo_state_of(repo: &Repository, dirty_files: usize) -> RepoState {
    use git2::RepositoryState as S;
    if has_merge_conflicts(repo) {
        return RepoState::Conflict;
    }
    match repo.state() {
        S::Clean => {
            if dirty_files > 0 {
                RepoState::Dirty
            } else {
                RepoState::Clean
            }
        }
        S::Merge => RepoState::InProgress(InProgressKind::Merge),
        S::Rebase
        | S::RebaseInteractive
        | S::RebaseMerge
        | S::ApplyMailbox
        | S::ApplyMailboxOrRebase => RepoState::InProgress(InProgressKind::Rebase),
        S::CherryPick | S::CherryPickSequence => RepoState::InProgress(InProgressKind::CherryPick),
        S::Revert | S::RevertSequence => RepoState::InProgress(InProgressKind::Revert),
        _ => RepoState::InProgress(InProgressKind::Other),
    }
}

/// Letzter Commit auf HEAD; `None` bei leerem Repo oder Lesefehler.
pub fn last_commit_of(repo: &Repository) -> Option<LastCommit> {
    let commit = repo.head().ok()?.peel_to_commit().ok()?;
    let summary = commit.summary().ok().flatten()?.to_string();
    let author = commit.author().name().ok()?.to_string();
    Some(LastCommit {
        summary,
        author,
        time: commit.time().seconds(),
    })
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
    // Zähler gegen `<remote>/<branch>`: erst der volle Ref-Name, dann die
    // Kurzform; `None`, wenn keiner auflösbar ist oder der Graph fehlschlägt.
    let against_remote = |remote: &str| -> Option<(usize, usize)> {
        [
            format!("refs/remotes/{remote}/{branch}"),
            format!("{remote}/{branch}"),
        ]
        .iter()
        .find_map(|candidate| {
            let obj = repo.revparse_single(candidate).ok()?;
            let up_oid = obj
                .peel_to_commit()
                .map(|c| c.id())
                .unwrap_or_else(|_| obj.id());
            repo.graph_ahead_behind(local_oid, up_oid).ok()
        })
    };
    // 2) Fallback: alle Remotes statt nur origin (I4) — sonst liefert
    // ein upstream-Remote oder Firmen-Remote ≠ origin immer (0,0).
    // Zuerst branch.<name>.remote lesen (explizites Tracking ohne upstream-Ref),
    // dann alle Remotes iterieren.
    if let Ok(cfg_branch_remote) = repo
        .config()
        .and_then(|c| c.get_string(&format!("branch.{branch}.remote")))
    {
        // F2#2: Graph-Fehler → nicht (0,0), sondern unten andere
        // Remotes versuchen (Upstream-bekannt-Regel oben bleibt).
        if let Some(counts) = against_remote(&cfg_branch_remote) {
            return counts;
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
            // F2#2: Graph-Fehler → nächsten Remote versuchen.
            if let Some(counts) = against_remote(remote_name) {
                return counts;
            }
        }
    }
    (0, 0)
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
    info.state = repo_state_of(&repo, dirty_files);
    info.last_commit = last_commit_of(&repo);
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
        strip_verbatim(&canon)
    }
    #[cfg(not(windows))]
    {
        canon
    }
}

/// Entfernt ein `\\?\`- bzw. `\\?\UNC\`-Präfix (nur Laufwerks- und UNC-Pfade;
/// `Volume{…}` bleibt verbatim). Reine Textlogik, daher plattformunabhängig.
fn strip_verbatim(path: &Path) -> PathBuf {
    if let Some(rest) = path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        if let Some(unc) = rest.strip_prefix(r"UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        if rest.as_bytes().get(1) == Some(&b':') {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
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

#[cfg(test)]
mod tests;
