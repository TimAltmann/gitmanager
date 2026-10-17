//! Auschecken eines MR-Branches nach K1 (E5/E6): Auswahlmenü (neuer
//! Worktree, neuer Klon, bestehender Klon, „Bereits ausgecheckt“) rein aus
//! den Scan-Daten und den Abrufdaten, dazu der Plan für die git-Schritte in
//! `git.rs` (`fetch_refspecs`, `prepare_mr_branch`, `add_worktree`,
//! `clone_repo`). Kein I/O: läuft bei jedem Frame auf zwischengespeicherten
//! Daten.

use super::model::{MergeRequest, MrSnapshot};
use super::remote::{HostMap, RemoteInfo};
use crate::config::MrConfig;
use crate::git::RepoInfo;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Was Menü und Plan über den MR wissen müssen (aus `MergeRequest`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MrRefInfo<'a> {
    /// glab-Host-Schlüssel
    pub host_key: &'a str,
    pub iid: u64,
    /// Zielprojekt
    pub project_path: &'a str,
    /// Quellprojekt; `None` bei unzugänglichem oder gelöschtem Fork
    pub source_project_path: Option<&'a str>,
    pub source_branch: &'a str,
    pub target_branch: &'a str,
    /// `None` = unbekannt (Schema-Fallback) → gilt als vorhanden
    pub source_branch_exists: Option<bool>,
    pub diff_head_sha: Option<&'a str>,
    /// `webUrl` des MR (Fallback für die Klon-URL)
    pub web_url: &'a str,
}

impl<'a> MrRefInfo<'a> {
    pub fn from_mr(mr: &'a MergeRequest) -> Self {
        Self {
            host_key: &mr.host_key,
            iid: mr.iid,
            project_path: &mr.project_path,
            source_project_path: mr.source_project_path.as_deref(),
            source_branch: &mr.source_branch,
            target_branch: &mr.target_branch,
            source_branch_exists: mr.source_branch_exists,
            diff_head_sha: mr.diff_head_sha.as_deref(),
            web_url: &mr.web_url,
        }
    }

    /// Fork-MR: Quellprojekt weicht vom Zielprojekt ab oder ist unbekannt.
    fn is_fork(&self) -> bool {
        self.source_project_path
            .is_none_or(|s| !s.eq_ignore_ascii_case(self.project_path))
    }

    /// Lokale Branch-Namen dieses MR: `sourceBranch` und das Ausweichen
    /// `mr-<iid>` (`git::prepare_mr_branch`).
    fn local_names(&self) -> [String; 2] {
        [self.source_branch.to_string(), fallback_branch(self.iid)]
    }
}

fn fallback_branch(iid: u64) -> String {
    format!("mr-{iid}")
}

/// GitLab-Projekt, auf das ein Remote zeigen muss (Host über `HostMap`, E16).
#[derive(Debug, Clone, Copy)]
pub struct ProjectRef<'a> {
    pub hosts: &'a HostMap,
    /// glab-Host-Schlüssel
    pub host_key: &'a str,
    pub full_path: &'a str,
}

impl ProjectRef<'_> {
    /// Remote von `repo` auf dieses Projekt, `origin` bevorzugt.
    pub fn remote_of<'r>(&self, repo: &'r RepoInfo) -> Option<&'r RemoteInfo> {
        repo.remotes
            .iter()
            .filter(|r| {
                r.parsed.as_ref().is_some_and(|p| {
                    p.full_path.eq_ignore_ascii_case(self.full_path)
                        && self
                            .hosts
                            .glab_key(p)
                            .is_some_and(|k| k.eq_ignore_ascii_case(self.host_key))
                })
            })
            .min_by_key(|r| r.name != "origin")
    }
}

/// MR-Branch ist schon in einem Klon oder Worktree ausgecheckt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlreadyCheckedOut {
    pub path: PathBuf,
    /// Commits hinter dem Upstream laut Scan; 0 bei Worktrees außerhalb der
    /// Suchpfade (dort ohne Scan-Daten)
    pub behind: usize,
}

/// „Neuer Worktree“ (E5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeProposal {
    /// `{review_root}/{repo}-mr-{iid}` bzw. `{eltern(basis)}/{repo}-mr-{iid}`
    pub path: PathBuf,
    /// Basis-Klon (zuletzt genutzt), mit dem der Worktree die Git-Daten teilt
    pub base: PathBuf,
    pub in_review_root: bool,
}

/// Warum ein bestehender Klon nicht umgeschaltet werden kann.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockReason {
    /// Der MR-Branch ist in einem anderen Worktree desselben Repos HEAD;
    /// git verweigert den Wechsel.
    CheckedOutElsewhere(PathBuf),
}

/// Zustand eines Klons im Abschnitt „In bestehendem Klon wechseln“.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneState {
    /// Sauber und auf dem Standard-Branch
    Free,
    /// „in Arbeit für !{iid}“ (Branch eines offenen MR) bzw. „in Arbeit“
    InUse {
        iid: Option<u64>,
    },
    /// Ungespeicherte Änderungen → Dialog „Stash & Switch / Force Discard“
    Dirty {
        files: usize,
    },
    Blocked(BlockReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOption {
    pub path: PathBuf,
    /// Aktueller Branch des Klons (bei Detached HEAD der Kurz-SHA)
    pub branch: String,
    pub state: CloneState,
    /// MRU-Zeitstempel (Unix-Sekunden); `None` = nie genutzt
    pub last_used: Option<u64>,
}

/// Nicht auscheckbar in v1 (E6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// Fork-MR, auch mit lokalem Fork-Remote (Auschecken erst in v2,
    /// Task 15)
    Fork,
    /// `sourceBranchExists == false`
    SourceBranchDeleted,
}

/// Inhalt des Checkout-Menüs einer MR-Zeile (K1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckoutMenu {
    /// MR-Branch ist schon ausgecheckt → die Zeile zeigt nur das
    /// Ordner-Pfeil-Menü
    pub already: Option<AlreadyCheckedOut>,
    /// „Neuer Worktree“; `None` ohne lokalen Klon
    pub new_worktree: Option<WorktreeProposal>,
    /// Für „Neuer Klon …“: Remote-URL ohne Userinfo
    pub clone_url: Option<String>,
    /// „In bestehendem Klon wechseln“, in Scan-Reihenfolge
    pub clones: Vec<CloneOption>,
    /// Nicht auscheckbar (E6); Worktree, Klon und Wechsel bleiben dann leer
    pub unavailable: Option<UnavailableReason>,
}

/// Quell-Branches offener MRs je Projekt, für „in Arbeit für !x“.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenBranches {
    /// (Host, Projekt klein, Branch) → iid
    by_branch: HashMap<(String, String, String), u64>,
}

impl OpenBranches {
    /// Quell-Branch im Quellprojekt (bei unbekanntem Quellprojekt keiner)
    /// und `mr-<iid>` im Zielprojekt; bei Doppelbelegung gewinnt der erste MR.
    pub fn from_mrs<'a>(mrs: impl IntoIterator<Item = &'a MergeRequest>) -> Self {
        let mut by_branch = HashMap::new();
        for mr in mrs {
            let host = mr.host_key.to_lowercase();
            if let Some(source) = &mr.source_project_path {
                let key = (
                    host.clone(),
                    source.to_lowercase(),
                    mr.source_branch.clone(),
                );
                by_branch.entry(key).or_insert(mr.iid);
            }
            let key = (
                host,
                mr.project_path.to_lowercase(),
                fallback_branch(mr.iid),
            );
            by_branch.entry(key).or_insert(mr.iid);
        }
        Self { by_branch }
    }

    /// Alle MRs des Stands: Meine, Von anderen und Nicht beteiligt.
    pub fn from_snapshot(snapshot: &MrSnapshot) -> Self {
        let uninvolved = snapshot.uninvolved.iter().flat_map(|u| &u.mrs);
        Self::from_mrs(
            snapshot
                .mine
                .iter()
                .chain(&snapshot.others)
                .chain(uninvolved),
        )
    }

    fn iid(&self, host_key: &str, project: &str, branch: &str) -> Option<u64> {
        let key = (
            host_key.to_lowercase(),
            project.to_lowercase(),
            branch.to_string(),
        );
        self.by_branch.get(&key).copied()
    }
}

/// git-Schritte für einen Klon (`git::fetch_refspecs`,
/// `git::prepare_mr_branch`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrCheckoutPlan {
    pub iid: u64,
    /// Remote des Klons auf das Quellprojekt (`origin` bevorzugt)
    pub remote: String,
    pub source_branch: String,
    /// `diffHeadSha` aus dem Abruf; weicht der geholte Stand ab, wird das
    /// nur geloggt
    pub expected_sha: Option<String>,
}

impl MrCheckoutPlan {
    /// Plan für `repo`; `None`, wenn kein Remote des Klons auf das
    /// Quellprojekt zeigt.
    pub fn for_repo(mr: &MrRefInfo, repo: &RepoInfo, hosts: &HostMap) -> Option<Self> {
        let project = ProjectRef {
            hosts,
            host_key: mr.host_key,
            full_path: mr.source_project_path?,
        };
        Some(Self {
            iid: mr.iid,
            remote: project.remote_of(repo)?.name.clone(),
            source_branch: mr.source_branch.to_string(),
            expected_sha: mr.diff_head_sha.map(str::to_string),
        })
    }

    /// `+refs/heads/<src>:refs/remotes/<remote>/<src>`: aktualisiert genau
    /// den Ref, den Dropdown und `ahead_behind_in` lesen.
    pub fn refspec(&self) -> String {
        format!("+refs/heads/{}:{}", self.source_branch, self.remote_ref())
    }

    /// Remote-Tracking-Ref nach dem Fetch (`refs/remotes/<remote>/<src>`).
    pub fn remote_ref(&self) -> String {
        format!("refs/remotes/{}", self.upstream())
    }

    /// Upstream als Kurzname (`<remote>/<src>`).
    pub fn upstream(&self) -> String {
        format!("{}/{}", self.remote, self.source_branch)
    }

    /// Ausweich-Branch bei Namens- oder Upstream-Konflikt.
    pub fn fallback_branch(&self) -> String {
        fallback_branch(self.iid)
    }
}

/// Was das Menü außerhalb der Scan-Daten nachschlägt, als Funktionen
/// übergeben: das Menü bleibt rein und testbar.
#[derive(Clone, Copy)]
pub struct MenuLookup<'a> {
    /// MRU-Zeitstempel eines Pfads in Unix-Sekunden, 0 = nie
    /// (`tray_popup::mru_timestamp`)
    pub mru: &'a dyn Fn(&Path) -> u64,
    /// Gibt es den Pfad schon? Nur für die Kandidaten des Worktree-Vorschlags
    /// (`{repo}-mr-{iid}`, `-2` …) gefragt. App: `Path::exists` beim Öffnen
    /// des Menüs, nicht je Frame (Task 11).
    pub exists: &'a dyn Fn(&Path) -> bool,
}

/// Baut das Checkout-Menü einer MR-Zeile (E5, K1). Kandidaten sind alle
/// gescannten Klone mit einem Remote auf das Quellprojekt, unabhängig von
/// der Repo-Auswahl (E1). `open` liefert die Branches offener MRs.
pub fn build_checkout_menu(
    mr: &MrRefInfo,
    repos: &[RepoInfo],
    hosts: &HostMap,
    cfg: &MrConfig,
    is_own: bool,
    lookup: &MenuLookup,
    open: &OpenBranches,
) -> CheckoutMenu {
    let mru = lookup.mru;
    let names = mr.local_names();
    let names = [names[0].as_str(), names[1].as_str()];
    let candidates = mr
        .source_project_path
        .map(|full_path| {
            let project = ProjectRef {
                hosts,
                host_key: mr.host_key,
                full_path,
            };
            candidates(repos, &project)
        })
        .unwrap_or_default();
    let mut menu = CheckoutMenu {
        already: checked_out_in(&candidates, &names),
        ..CheckoutMenu::default()
    };
    if mr.source_branch.is_empty() || mr.source_branch_exists == Some(false) {
        menu.unavailable = Some(UnavailableReason::SourceBranchDeleted);
        return menu;
    }
    // v1 (E6): Fork-MRs nie, auch nicht über einen lokalen Fork-Remote
    // (Task 15). `already` bleibt: ein lokaler Stand ist nur ein Ordner.
    if mr.is_fork() {
        menu.unavailable = Some(UnavailableReason::Fork);
        return menu;
    }
    let base = base_clone(&candidates, mru);
    menu.clone_url = match base {
        Some(c) => Some(c.remote.url_redacted.clone()),
        None => guess_clone_url(mr, repos, hosts),
    };
    menu.new_worktree = base.map(|c| {
        let taken = Taken {
            repos,
            candidates: &candidates,
            names: &names,
            exists: lookup.exists,
        };
        worktree_proposal(mr, &c.repo.path, &taken, cfg, is_own)
    });
    menu.clones = candidates
        .iter()
        .map(|c| CloneOption {
            path: c.repo.path.clone(),
            branch: c.repo.branch.clone(),
            state: clone_state(mr, c, &names, open),
            last_used: Some(mru(&c.repo.path)).filter(|t| *t > 0),
        })
        .collect();
    menu
}

/// Ordner, in dem `branch` für `project` ausgecheckt ist: gescannte Klone
/// mit Remote auf das Projekt und die übrigen Worktrees ihrer Repos (aus dem
/// Scan, `RepoInfo.worktrees`), auch außerhalb der Suchpfade.
pub fn find_checked_out(repos: &[RepoInfo], branch: &str, project: &ProjectRef) -> Option<PathBuf> {
    checked_out_in(&candidates(repos, project), &[branch]).map(|a| a.path)
}

/// Gescannter Klon mit seinem Remote auf das Quellprojekt.
struct Candidate<'r> {
    repo: &'r RepoInfo,
    remote: &'r RemoteInfo,
}

/// Klone mit Remote auf `project`, in Scan-Reihenfolge.
fn candidates<'r>(repos: &'r [RepoInfo], project: &ProjectRef) -> Vec<Candidate<'r>> {
    repos
        .iter()
        .filter_map(|repo| {
            let remote = project.remote_of(repo)?;
            Some(Candidate { repo, remote })
        })
        .collect()
}

/// Basis-Klon: der zuletzt genutzte Kandidat, bei Gleichstand der erste.
/// Verknüpfte Worktrees (z. B. ein Review-Ordner unter den Suchpfaden) nur,
/// wenn es keinen eigenen Klon gibt: sonst landeten eigene MRs neben einem
/// Wegwerf-Ordner.
fn base_clone<'c, 'r>(
    candidates: &'c [Candidate<'r>],
    mru: &dyn Fn(&Path) -> u64,
) -> Option<&'c Candidate<'r>> {
    let has_clone = candidates.iter().any(|c| !c.repo.is_linked_worktree);
    candidates
        .iter()
        .filter(|c| !has_clone || !c.repo.is_linked_worktree)
        .min_by_key(|c| Reverse(mru(&c.repo.path)))
}

/// Zuerst gescannte Klone (mit `behind` aus dem Scan), dann die übrigen
/// Worktrees ihrer Repos.
fn checked_out_in(candidates: &[Candidate], names: &[&str]) -> Option<AlreadyCheckedOut> {
    let scanned = candidates
        .iter()
        .find(|c| !c.repo.is_detached && names.contains(&c.repo.branch.as_str()))
        .map(|c| AlreadyCheckedOut {
            path: c.repo.path.clone(),
            behind: c.repo.behind,
        });
    scanned.or_else(|| {
        candidates
            .iter()
            .flat_map(|c| &c.repo.worktrees)
            .find(|w| w.branch.as_deref().is_some_and(|b| names.contains(&b)))
            .map(|w| AlreadyCheckedOut {
                path: w.path.clone(),
                behind: 0,
            })
    })
}

/// Reihenfolge: MR-Branch hier ausgecheckt → in einem anderen Worktree
/// desselben Repos (git verweigert den Wechsel) → Änderungen → Branch eines
/// offenen MR → Standard-Branch → sonst „in Arbeit“.
fn clone_state(
    mr: &MrRefInfo,
    candidate: &Candidate,
    names: &[&str],
    open: &OpenBranches,
) -> CloneState {
    let repo = candidate.repo;
    if !repo.is_detached && names.contains(&repo.branch.as_str()) {
        return CloneState::InUse { iid: Some(mr.iid) };
    }
    if let Some(wt) = repo
        .worktrees
        .iter()
        .find(|w| w.branch.as_deref().is_some_and(|b| names.contains(&b)))
    {
        return CloneState::Blocked(BlockReason::CheckedOutElsewhere(wt.path.clone()));
    }
    if repo.dirty {
        // `dirty` kann ohne Zähler gesetzt sein (App nach Config-Änderung)
        return CloneState::Dirty {
            files: repo.dirty_files.max(1),
        };
    }
    if !repo.is_detached {
        let project = candidate
            .remote
            .parsed
            .as_ref()
            .map_or("", |p| p.full_path.as_str());
        if let Some(iid) = open.iid(mr.host_key, project, &repo.branch) {
            return CloneState::InUse { iid: Some(iid) };
        }
        if is_default_branch(&repo.branch, mr.target_branch) {
            return CloneState::Free;
        }
    }
    CloneState::InUse { iid: None }
}

/// Standard-Branch des Projekts: GitLab liefert ihn in den MR-Abfragen
/// nicht, daher der Ziel-Branch des MR oder `main`/`master`.
fn is_default_branch(branch: &str, target_branch: &str) -> bool {
    branch == target_branch || matches!(branch, "main" | "master")
}

/// Ordner `{repo}-mr-{iid}` im Review-Ordner (MRs von anderen) bzw. neben
/// dem Basis-Klon (eigene MRs oder ohne Review-Ordner).
fn worktree_proposal(
    mr: &MrRefInfo,
    base: &Path,
    taken: &Taken,
    cfg: &MrConfig,
    is_own: bool,
) -> WorktreeProposal {
    let review_root = cfg
        .review_root
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty() && !is_own);
    let (parent, in_review_root) = match review_root {
        Some(root) => (PathBuf::from(root), true),
        None => (base.parent().unwrap_or(base).to_path_buf(), false),
    };
    let project = mr
        .project_path
        .rsplit('/')
        .next()
        .unwrap_or(mr.project_path);
    let name = format!("{project}-mr-{}", mr.iid);
    WorktreeProposal {
        path: taken.free_dir(&parent, &name),
        base: base.to_path_buf(),
        in_review_root,
    }
}

/// Belegte Ordner für den Worktree-Vorschlag.
struct Taken<'a> {
    repos: &'a [RepoInfo],
    candidates: &'a [Candidate<'a>],
    /// Lokale Branch-Namen dieses MR
    names: &'a [&'a str],
    exists: &'a dyn Fn(&Path) -> bool,
}

impl Taken<'_> {
    /// Erster freier Ordner `name`, `name-2`, `name-3` …: belegt ist ein
    /// Pfad, den es schon gibt (`exists`, z. B. Reste eines gelöschten
    /// Review-Ordners) oder unter dem ein gescanntes Repo oder ein Worktree
    /// eines gescannten Repos liegt – außer der MR-Branch ist dort schon
    /// ausgecheckt (Ordner dieses MR).
    fn free_dir(&self, parent: &Path, name: &str) -> PathBuf {
        let is_mr_branch = |b: Option<&str>| b.is_some_and(|b| self.names.contains(&b));
        let this_mr: HashSet<String> = self
            .candidates
            .iter()
            .flat_map(|c| {
                let own = (!c.repo.is_detached).then_some(c.repo.branch.as_str());
                std::iter::once((&c.repo.path, own)).chain(
                    c.repo
                        .worktrees
                        .iter()
                        .map(|w| (&w.path, w.branch.as_deref())),
                )
            })
            .filter(|(_, branch)| is_mr_branch(*branch))
            .map(|(path, _)| path_key(path))
            .collect();
        let scanned: HashSet<String> = self
            .repos
            .iter()
            .flat_map(|r| std::iter::once(&r.path).chain(r.worktrees.iter().map(|w| &w.path)))
            .map(|p| path_key(p))
            .filter(|k| !this_mr.contains(k))
            .collect();
        let is_free = |p: &PathBuf| {
            let key = path_key(p);
            this_mr.contains(&key) || (!scanned.contains(&key) && !(self.exists)(p))
        };
        // Laut Scan ist unter `scanned.len() + 1` Kandidaten immer einer
        // frei; für Ordner außerhalb des Scans höchstens 99 weitere.
        (1..=scanned.len() + 100)
            .map(|n| match n {
                1 => parent.join(name),
                n => parent.join(format!("{name}-{n}")),
            })
            .find(is_free)
            .unwrap_or_else(|| parent.join(name))
    }
}

/// Vergleichsschlüssel für Pfade: ohne abschließenden Separator, unter
/// Windows ohne Groß-/Kleinschreibung (wie `AppConfig::repo_state_key`).
fn path_key(path: &Path) -> String {
    let key = path
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .to_string();
    #[cfg(windows)]
    {
        key.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        key
    }
}

/// URL für „Neuer Klon …“ ohne lokalen Klon des Projekts: nach dem Muster
/// eines Remotes desselben GitLab-Hosts (Schema, Benutzer, Port wie bei den
/// übrigen Klonen), sonst HTTP(S) aus `webUrl` – die Projekt-URL vor
/// `/-/merge_requests/`, also mit einem Unterordner wie `/gitlab` (E7).
fn guess_clone_url(mr: &MrRefInfo, repos: &[RepoInfo], hosts: &HostMap) -> Option<String> {
    let same_host = repos.iter().flat_map(|r| &r.remotes).find_map(|remote| {
        let parsed = remote.parsed.as_ref()?;
        let key = hosts.glab_key(parsed)?;
        if !key.eq_ignore_ascii_case(mr.host_key) {
            return None;
        }
        // Pfad in der URL ersetzen; `parsed.full_path` ist kleingeschrieben
        // und ASCII, die Byte-Positionen stimmen also überein.
        let url = &remote.url_redacted;
        let start = url.to_ascii_lowercase().rfind(&parsed.full_path)?;
        let end = start + parsed.full_path.len();
        Some(format!(
            "{}{}{}",
            &url[..start],
            mr.project_path,
            &url[end..]
        ))
    });
    same_host.or_else(|| {
        // Ohne Userinfo, `\`, Leer- und Steuerzeichen; Host und Pfad nicht leer
        let (project_url, _) = mr.web_url.split_once("/-/merge_requests/")?;
        let (scheme, rest) = project_url.split_once("://")?;
        let bad = |c: char| c == '@' || c == '\\' || c.is_control() || c.is_whitespace();
        let has_path = rest
            .split_once('/')
            .is_some_and(|(host, path)| !host.is_empty() && !path.is_empty());
        (matches!(scheme, "http" | "https") && has_path && !project_url.contains(bad))
            .then(|| format!("{project_url}.git"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::WorktreeHead;
    use crate::gitlab::model::UninvolvedList;
    use crate::gitlab::remote::{resolve_hosts, GlabHost};

    const HOST: &str = "gitlab.example.com";
    const BACKEND: &str = "git@gitlab.example.com:kern/backend-api.git";

    fn repo(path: &str, branch: &str, remotes: &[(&str, &str)]) -> RepoInfo {
        let mut info = RepoInfo::new(PathBuf::from(path), branch.to_string(), false, false);
        info.remotes = remotes
            .iter()
            .map(|(name, url)| RemoteInfo::new(name, url))
            .collect();
        info
    }

    /// Klon von `kern/backend-api` mit `origin` per SSH
    fn backend(path: &str, branch: &str) -> RepoInfo {
        repo(path, branch, &[("origin", BACKEND)])
    }

    fn worktree(path: &str, branch: Option<&str>) -> WorktreeHead {
        WorktreeHead {
            path: PathBuf::from(path),
            branch: branch.map(str::to_string),
        }
    }

    /// Angemeldet ist nur `gitlab.example.com` (github.com bleibt „nicht GitLab“).
    fn hosts_for(repos: &[RepoInfo]) -> HostMap {
        let glab = [GlabHost {
            key: HOST.to_string(),
            user: "taltmann".to_string(),
            ..GlabHost::default()
        }];
        resolve_hosts(
            repos
                .iter()
                .flat_map(|r| r.remotes.iter().filter_map(|x| x.parsed.as_ref())),
            &glab,
            &MrConfig::default(),
        )
    }

    fn mr(iid: u64, branch: &str) -> MergeRequest {
        MergeRequest {
            host_key: HOST.to_string(),
            id: format!("gid://gitlab/MergeRequest/{iid}"),
            iid,
            // GitLab liefert Pfade in Originalschreibweise
            web_url: format!("https://{HOST}/Kern/Backend-API/-/merge_requests/{iid}"),
            project_path: "Kern/Backend-API".to_string(),
            source_project_path: Some("Kern/Backend-API".to_string()),
            source_branch: branch.to_string(),
            target_branch: "main".to_string(),
            source_branch_exists: Some(true),
            diff_head_sha: Some("0123abcd".to_string()),
            ..MergeRequest::default()
        }
    }

    fn review_cfg() -> MrConfig {
        MrConfig {
            review_root: Some("/dev/Reviews".to_string()),
            ..MrConfig::default()
        }
    }

    fn no_mru(_: &Path) -> u64 {
        0
    }

    /// Kein Ordner außerhalb der Scan-Daten
    fn nothing_exists(_: &Path) -> bool {
        false
    }

    fn menu_with(
        mr: &MergeRequest,
        repos: &[RepoInfo],
        cfg: &MrConfig,
        is_own: bool,
        lookup: &MenuLookup,
        open: &OpenBranches,
    ) -> CheckoutMenu {
        let hosts = hosts_for(repos);
        build_checkout_menu(
            &MrRefInfo::from_mr(mr),
            repos,
            &hosts,
            cfg,
            is_own,
            lookup,
            open,
        )
    }

    fn menu(
        mr: &MergeRequest,
        repos: &[RepoInfo],
        cfg: &MrConfig,
        is_own: bool,
        mru: &dyn Fn(&Path) -> u64,
    ) -> CheckoutMenu {
        let lookup = MenuLookup {
            mru,
            exists: &nothing_exists,
        };
        menu_with(mr, repos, cfg, is_own, &lookup, &OpenBranches::default())
    }

    fn proposal(menu: &CheckoutMenu) -> &WorktreeProposal {
        menu.new_worktree
            .as_ref()
            .unwrap_or_else(|| panic!("Worktree-Vorschlag fehlt: {menu:?}"))
    }

    /// MRU wie im Mockup: backend-api-2 zuletzt genutzt
    fn mockup_mru(p: &Path) -> u64 {
        match p.to_str().unwrap_or_default() {
            "/dev/backend-api" => 100,
            "/dev/backend-api-2" => 300,
            "/dev/backend-api-hotfix" => 200,
            _ => 0,
        }
    }

    // --- Worktree-Vorschlag ---

    #[test]
    fn worktree_for_others_goes_to_review_root() {
        let repos = [
            backend("/dev/backend-api", "main"),
            backend("/dev/backend-api-2", "feature/csv-export"),
        ];
        let m = menu(
            &mr(418, "feature/mandantenfilter"),
            &repos,
            &review_cfg(),
            false,
            &mockup_mru,
        );
        assert_eq!(
            m.new_worktree,
            Some(WorktreeProposal {
                path: PathBuf::from("/dev/Reviews/Backend-API-mr-418"),
                base: PathBuf::from("/dev/backend-api-2"),
                in_review_root: true,
            })
        );
        assert_eq!(m.clone_url.as_deref(), Some(BACKEND));
        assert_eq!(m.already, None);
        assert_eq!(m.unavailable, None);
        assert_eq!(m.clones.len(), 2);
    }

    #[test]
    fn worktree_for_own_mr_next_to_base_clone() {
        // Eigene MRs nie in den Review-Ordner, auch wenn er gesetzt ist
        let repos = [
            backend("/dev/backend-api", "main"),
            backend("/dev/backend-api-2", "develop"),
        ];
        let m = menu(
            &mr(412, "feature/csv-export"),
            &repos,
            &review_cfg(),
            true,
            &mockup_mru,
        );
        assert_eq!(
            m.new_worktree,
            Some(WorktreeProposal {
                path: PathBuf::from("/dev/Backend-API-mr-412"),
                base: PathBuf::from("/dev/backend-api-2"),
                in_review_root: false,
            })
        );
    }

    #[test]
    fn worktree_without_review_root_next_to_base_clone() {
        let repos = [backend("/dev/backend-api", "main")];
        for review_root in [None, Some("   ".to_string())] {
            let cfg = MrConfig {
                review_root,
                ..MrConfig::default()
            };
            let m = menu(&mr(418, "feature/x"), &repos, &cfg, false, &no_mru);
            assert_eq!(
                proposal(&m),
                &WorktreeProposal {
                    path: PathBuf::from("/dev/Backend-API-mr-418"),
                    base: PathBuf::from("/dev/backend-api"),
                    in_review_root: false,
                }
            );
        }
    }

    #[test]
    fn base_clone_is_most_recently_used() {
        let repos = [
            backend("/dev/backend-api", "main"),
            backend("/dev/backend-api-2", "main"),
            backend("/dev/backend-api-hotfix", "main"),
        ];
        let m = menu(
            &mr(418, "feature/x"),
            &repos,
            &review_cfg(),
            false,
            &mockup_mru,
        );
        assert_eq!(proposal(&m).base, PathBuf::from("/dev/backend-api-2"));
        // Gleichstand (nie genutzt) → erster Kandidat in Scan-Reihenfolge
        let m = menu(&mr(418, "feature/x"), &repos, &review_cfg(), false, &no_mru);
        assert_eq!(proposal(&m).base, PathBuf::from("/dev/backend-api"));
        let tie = |p: &Path| u64::from(p != Path::new("/dev/backend-api"));
        let m = menu(&mr(418, "feature/x"), &repos, &review_cfg(), false, &tie);
        assert_eq!(proposal(&m).base, PathBuf::from("/dev/backend-api-2"));
        // Die Klon-URL kommt vom Basis-Klon
        let mut repos = repos.to_vec();
        repos[1].remotes = vec![RemoteInfo::new(
            "origin",
            "https://oauth2:glpat-SECRET@gitlab.example.com/kern/backend-api.git",
        )];
        let m = menu(
            &mr(418, "feature/x"),
            &repos,
            &review_cfg(),
            false,
            &mockup_mru,
        );
        assert_eq!(
            m.clone_url.as_deref(),
            Some("https://gitlab.example.com/kern/backend-api.git")
        );
    }

    #[test]
    fn worktree_folder_collision_gets_suffix() {
        // Unter dem Vorschlag liegt schon ein gescanntes Repo eines anderen
        // Projekts, unter `-2` ein Worktree außerhalb der Suchpfade.
        let mut other = repo(
            "/dev/Reviews/Backend-API-mr-418",
            "main",
            &[("origin", "git@gitlab.example.com:web/kundenportal.git")],
        );
        let base = backend("/dev/backend-api", "main");
        let m = menu(
            &mr(418, "feature/x"),
            &[base.clone(), other.clone()],
            &review_cfg(),
            false,
            &no_mru,
        );
        assert_eq!(
            proposal(&m).path,
            PathBuf::from("/dev/Reviews/Backend-API-mr-418-2")
        );
        other.worktrees = vec![worktree(
            "/dev/Reviews/Backend-API-mr-418-2",
            Some("feature/y"),
        )];
        let m = menu(
            &mr(418, "feature/x"),
            &[base.clone(), other],
            &review_cfg(),
            false,
            &no_mru,
        );
        assert_eq!(
            proposal(&m).path,
            PathBuf::from("/dev/Reviews/Backend-API-mr-418-3")
        );
        // Der Ordner dieses MR (MR-Branch dort ausgecheckt) zählt nicht als
        // Kollision; die Zeile zeigt dann ohnehin „Bereits ausgecheckt“.
        let mine = backend("/dev/Reviews/Backend-API-mr-418", "feature/x");
        let m = menu(
            &mr(418, "feature/x"),
            &[base, mine],
            &review_cfg(),
            false,
            &no_mru,
        );
        assert_eq!(
            proposal(&m).path,
            PathBuf::from("/dev/Reviews/Backend-API-mr-418")
        );
        assert!(m.already.is_some());
    }

    #[test]
    fn worktree_folder_existing_outside_scan_gets_suffix() {
        // Reste eines im Explorer gelöschten Review-Ordners, ein Klon
        // außerhalb der Suchpfade o. Ä.: der Scan kennt sie nicht.
        let repos = [backend("/dev/backend-api", "main")];
        let exists = |p: &Path| {
            matches!(
                p.to_str().unwrap_or_default(),
                "/dev/Reviews/Backend-API-mr-418" | "/dev/Reviews/Backend-API-mr-418-2"
            )
        };
        let lookup = MenuLookup {
            mru: &no_mru,
            exists: &exists,
        };
        let open = OpenBranches::default();
        let m = menu_with(
            &mr(418, "feature/x"),
            &repos,
            &review_cfg(),
            false,
            &lookup,
            &open,
        );
        assert_eq!(
            proposal(&m).path,
            PathBuf::from("/dev/Reviews/Backend-API-mr-418-3")
        );
        // Der Ordner dieses MR zählt nicht als Kollision, auch wenn er existiert
        let mut base = backend("/dev/backend-api", "main");
        base.worktrees = vec![worktree(
            "/dev/Reviews/Backend-API-mr-418",
            Some("feature/x"),
        )];
        let m = menu_with(
            &mr(418, "feature/x"),
            &[base],
            &review_cfg(),
            false,
            &lookup,
            &open,
        );
        assert_eq!(
            proposal(&m).path,
            PathBuf::from("/dev/Reviews/Backend-API-mr-418")
        );
        // Existiert alles: Vorschlag ohne Suffix, `add_worktree` meldet es
        let all = MenuLookup {
            mru: &no_mru,
            exists: &|_: &Path| true,
        };
        let m = menu_with(
            &mr(418, "feature/x"),
            &repos,
            &review_cfg(),
            false,
            &all,
            &open,
        );
        assert_eq!(
            proposal(&m).path,
            PathBuf::from("/dev/Reviews/Backend-API-mr-418")
        );
    }

    #[test]
    fn base_clone_prefers_clone_over_linked_worktree() {
        // Zuletzt genutzt ist ein Review-Worktree unter den Suchpfaden; er
        // teilt die Remotes, ist aber kein Basis-Klon.
        let mut review = backend("/dev/Reviews/Backend-API-mr-412", "feature/csv-export");
        review.is_linked_worktree = true;
        let repos = [
            review,
            backend("/dev/backend-api", "main"),
            backend("/dev/backend-api-2", "develop"),
        ];
        let mru = |p: &Path| match p.to_str().unwrap_or_default() {
            "/dev/Reviews/Backend-API-mr-412" => 900,
            "/dev/backend-api-2" => 300,
            _ => 0,
        };
        let m = menu(&mr(420, "feature/y"), &repos, &review_cfg(), true, &mru);
        assert_eq!(
            m.new_worktree,
            Some(WorktreeProposal {
                path: PathBuf::from("/dev/Backend-API-mr-420"),
                base: PathBuf::from("/dev/backend-api-2"),
                in_review_root: false,
            })
        );
        // Umschalten bleibt in allen Kandidaten möglich
        assert_eq!(m.clones.len(), 3);
        // Nur verknüpfte Worktrees gescannt: dann eben der zuletzt genutzte
        let m = menu(
            &mr(420, "feature/y"),
            &repos[..1],
            &review_cfg(),
            true,
            &mru,
        );
        assert_eq!(
            proposal(&m).base,
            PathBuf::from("/dev/Reviews/Backend-API-mr-412")
        );
    }

    // --- Zustände der Klone ---

    #[test]
    fn clone_states_free_in_use_dirty() {
        let mut hotfix = backend("/dev/backend-api-hotfix", "hotfix/login-timeout");
        hotfix.dirty = true;
        hotfix.dirty_files = 3;
        let mut detached = backend("/dev/backend-api-detached", "1a2b3c4");
        detached.is_detached = true;
        let mut dirty_unknown = backend("/dev/backend-api-flag", "main");
        // `dirty` ohne Zähler (z. B. von der App nachträglich gesetzt)
        dirty_unknown.dirty = true;
        dirty_unknown.dirty_files = 0;
        let repos = [
            backend("/dev/backend-api", "main"),
            backend("/dev/backend-api-2", "feature/csv-export"),
            hotfix,
            backend("/dev/backend-api-old", "spike/cache"),
            backend("/dev/backend-api-develop", "develop"),
            detached,
            dirty_unknown,
        ];
        let mr412 = mr(412, "feature/csv-export");
        let mut mr418 = mr(418, "feature/mandantenfilter");
        // Ziel-Branch des MR zählt als Standard-Branch
        mr418.target_branch = "develop".to_string();
        let open = OpenBranches::from_mrs([&mr412, &mr418]);
        let lookup = MenuLookup {
            mru: &mockup_mru,
            exists: &nothing_exists,
        };
        let m = menu_with(&mr418, &repos, &review_cfg(), false, &lookup, &open);
        let states: Vec<(&str, &CloneState)> = m
            .clones
            .iter()
            .map(|c| (c.path.to_str().unwrap(), &c.state))
            .collect();
        assert_eq!(
            states,
            vec![
                ("/dev/backend-api", &CloneState::Free),
                ("/dev/backend-api-2", &CloneState::InUse { iid: Some(412) }),
                ("/dev/backend-api-hotfix", &CloneState::Dirty { files: 3 }),
                ("/dev/backend-api-old", &CloneState::InUse { iid: None }),
                ("/dev/backend-api-develop", &CloneState::Free),
                (
                    "/dev/backend-api-detached",
                    &CloneState::InUse { iid: None }
                ),
                ("/dev/backend-api-flag", &CloneState::Dirty { files: 1 }),
            ]
        );
        assert_eq!(m.clones[1].branch, "feature/csv-export");
        assert_eq!(m.clones[1].last_used, Some(300));
        assert_eq!(m.clones[3].last_used, None);
    }

    #[test]
    fn clone_on_mr_branch_or_blocked_by_sibling_worktree() {
        let mut main = backend("/dev/backend-api", "main");
        main.worktrees = vec![worktree("/mnt/review/wt", Some("feature/x"))];
        let on_branch = backend("/dev/backend-api-2", "feature/x");
        let m = menu(
            &mr(418, "feature/x"),
            &[main, on_branch],
            &review_cfg(),
            false,
            &no_mru,
        );
        assert_eq!(
            m.clones[0].state,
            CloneState::Blocked(BlockReason::CheckedOutElsewhere(PathBuf::from(
                "/mnt/review/wt"
            )))
        );
        assert_eq!(m.clones[1].state, CloneState::InUse { iid: Some(418) });
    }

    // --- Bereits ausgecheckt ---

    #[test]
    fn already_checked_out_in_scanned_clone() {
        let mut on_branch = backend("/dev/backend-api-2", "feature/x");
        on_branch.behind = 2;
        let repos = [backend("/dev/backend-api", "main"), on_branch];
        let m = menu(&mr(418, "feature/x"), &repos, &review_cfg(), false, &no_mru);
        assert_eq!(
            m.already,
            Some(AlreadyCheckedOut {
                path: PathBuf::from("/dev/backend-api-2"),
                behind: 2,
            })
        );
        // Ausweich-Branch `mr-<iid>` zählt ebenso
        let repos = [backend("/dev/backend-api", "mr-418")];
        let m = menu(&mr(418, "feature/x"), &repos, &review_cfg(), false, &no_mru);
        assert_eq!(
            m.already.map(|a| a.path),
            Some(PathBuf::from("/dev/backend-api"))
        );
        // Gleicher Branch-Name in einem fremden Projekt zählt nicht
        let repos = [
            backend("/dev/backend-api", "main"),
            repo(
                "/dev/kundenportal",
                "feature/x",
                &[("origin", "git@gitlab.example.com:web/kundenportal.git")],
            ),
        ];
        let m = menu(&mr(418, "feature/x"), &repos, &review_cfg(), false, &no_mru);
        assert_eq!(m.already, None);
    }

    #[test]
    fn already_checked_out_in_worktree_outside_search_paths() {
        let mut base = backend("/dev/backend-api", "main");
        base.worktrees = vec![
            worktree("/mnt/other/detached", None),
            worktree("/mnt/outside/Backend-API-mr-418", Some("feature/x")),
        ];
        let mut portal = repo(
            "/dev/kundenportal",
            "main",
            &[("origin", "git@gitlab.example.com:web/kundenportal.git")],
        );
        portal.worktrees = vec![worktree("/mnt/portal/wt", Some("feature/x"))];
        let repos = [portal, base];
        let m = menu(&mr(418, "feature/x"), &repos, &review_cfg(), false, &no_mru);
        assert_eq!(
            m.already,
            Some(AlreadyCheckedOut {
                path: PathBuf::from("/mnt/outside/Backend-API-mr-418"),
                behind: 0,
            })
        );
        let hosts = hosts_for(&repos);
        let project = ProjectRef {
            hosts: &hosts,
            host_key: HOST,
            full_path: "Kern/Backend-API",
        };
        assert_eq!(
            find_checked_out(&repos, "feature/x", &project),
            Some(PathBuf::from("/mnt/outside/Backend-API-mr-418"))
        );
        assert_eq!(
            find_checked_out(&repos, "main", &project),
            Some(PathBuf::from("/dev/backend-api"))
        );
        assert_eq!(find_checked_out(&repos, "feature/y", &project), None);
    }

    // --- Nicht auscheckbar (E6) ---

    #[test]
    fn fork_without_fork_remote_is_unavailable() {
        let repos = [backend("/dev/backend-api", "main")];
        let mut fork = mr(418, "feature/x");
        fork.source_project_path = Some("tim/backend-api".to_string());
        let m = menu(&fork, &repos, &review_cfg(), false, &no_mru);
        assert_eq!(
            m,
            CheckoutMenu {
                unavailable: Some(UnavailableReason::Fork),
                ..CheckoutMenu::default()
            }
        );
        // Quellprojekt unbekannt (`sourceProject: null`) → ebenso Fork
        fork.source_project_path = None;
        let m = menu(&fork, &repos, &review_cfg(), false, &no_mru);
        assert_eq!(m.unavailable, Some(UnavailableReason::Fork));
        assert!(m.clones.is_empty() && m.new_worktree.is_none() && m.clone_url.is_none());
        // ohne jeden Klon ebenso (kein Remote auf den Fork bekannt)
        let m = menu(&fork, &[], &review_cfg(), false, &no_mru);
        assert_eq!(m.unavailable, Some(UnavailableReason::Fork));
    }

    #[test]
    fn fork_with_fork_remote_is_unavailable_in_v1() {
        // E6: auch mit lokalem Fork-Remote nicht auscheckbar (v2, Task 15)
        let fork_clone = repo(
            "/dev/fork",
            "main",
            &[
                ("origin", "git@gitlab.example.com:tim/backend-api.git"),
                ("upstream", BACKEND),
            ],
        );
        let mut repos = [backend("/dev/backend-api", "main"), fork_clone];
        let mut fork = mr(418, "feature/x");
        fork.source_project_path = Some("tim/backend-api".to_string());
        let m = menu(&fork, &repos, &review_cfg(), false, &no_mru);
        assert_eq!(
            m,
            CheckoutMenu {
                unavailable: Some(UnavailableReason::Fork),
                ..CheckoutMenu::default()
            }
        );
        // Ein lokaler Stand im Fork-Klon bleibt über den Ordner-Pfeil erreichbar
        repos[1].branch = "feature/x".to_string();
        let m = menu(&fork, &repos, &review_cfg(), false, &no_mru);
        assert_eq!(m.unavailable, Some(UnavailableReason::Fork));
        assert_eq!(m.already.map(|a| a.path), Some(PathBuf::from("/dev/fork")));
        assert!(m.clones.is_empty() && m.new_worktree.is_none() && m.clone_url.is_none());
        // Der Plan löst weiter über das Quellprojekt auf (für Task 15)
        let hosts = hosts_for(&repos);
        let plan = MrCheckoutPlan::for_repo(&MrRefInfo::from_mr(&fork), &repos[1], &hosts);
        assert_eq!(plan.map(|p| p.remote), Some("origin".to_string()));
        assert_eq!(
            MrCheckoutPlan::for_repo(&MrRefInfo::from_mr(&fork), &repos[0], &hosts),
            None
        );
    }

    #[test]
    fn deleted_source_branch_is_unavailable() {
        let mut on_branch = backend("/dev/backend-api-2", "feature/x");
        on_branch.behind = 1;
        let repos = [backend("/dev/backend-api", "main"), on_branch];
        let mut deleted = mr(418, "feature/x");
        deleted.source_branch_exists = Some(false);
        let m = menu(&deleted, &repos, &review_cfg(), false, &no_mru);
        assert_eq!(m.unavailable, Some(UnavailableReason::SourceBranchDeleted));
        assert!(m.clones.is_empty() && m.new_worktree.is_none() && m.clone_url.is_none());
        // Ein lokaler Stand des Branches bleibt über den Ordner-Pfeil erreichbar
        assert_eq!(
            m.already.map(|a| a.path),
            Some(PathBuf::from("/dev/backend-api-2"))
        );
        // Unbekannt (altes GitLab) gilt als vorhanden
        deleted.source_branch_exists = None;
        let m = menu(&deleted, &repos, &review_cfg(), false, &no_mru);
        assert_eq!(m.unavailable, None);
    }

    // --- Kein lokaler Klon ---

    #[test]
    fn without_clone_only_new_clone_is_offered() {
        // Klon-URL nach dem Muster eines anderen Remotes desselben Hosts
        let repos = [
            repo(
                "/dev/rust",
                "main",
                &[("origin", "https://github.com/kern/backend-api.git")],
            ),
            repo(
                "/dev/kundenportal",
                "main",
                &[(
                    "origin",
                    "ssh://git@gitlab.example.com:2222/Web/Kundenportal.git",
                )],
            ),
        ];
        let m = menu(&mr(418, "feature/x"), &repos, &review_cfg(), false, &no_mru);
        assert_eq!(
            m,
            CheckoutMenu {
                clone_url: Some(
                    "ssh://git@gitlab.example.com:2222/Kern/Backend-API.git".to_string()
                ),
                ..CheckoutMenu::default()
            }
        );
        // Ohne Remote auf den Host: HTTPS aus `webUrl`
        let m = menu(
            &mr(418, "feature/x"),
            &repos[..1],
            &review_cfg(),
            false,
            &no_mru,
        );
        assert_eq!(
            m.clone_url.as_deref(),
            Some("https://gitlab.example.com/Kern/Backend-API.git")
        );
        assert!(m.clones.is_empty() && m.new_worktree.is_none());
        assert_eq!((m.already, m.unavailable), (None, None));
        // GitLab unter einem Unterordner (relative URL root, E7)
        let mut sub = mr(418, "feature/x");
        sub.web_url =
            "https://gitlab.example.com/gitlab/Kern/Backend-API/-/merge_requests/418".to_string();
        let m = menu(&sub, &repos[..1], &review_cfg(), false, &no_mru);
        assert_eq!(
            m.clone_url.as_deref(),
            Some("https://gitlab.example.com/gitlab/Kern/Backend-API.git")
        );
        // Ohne verwertbare `webUrl` gibt es keine URL
        for web_url in [
            "",
            "https://gitlab.example.com/Kern/Backend-API",
            "ftp://gitlab.example.com/Kern/Backend-API/-/merge_requests/418",
            "https://user@gitlab.example.com/Kern/Backend-API/-/merge_requests/418",
            "https://gitlab.example.com/-/merge_requests/418",
        ] {
            let mut no_web = mr(418, "feature/x");
            no_web.web_url = web_url.to_string();
            let m = menu(&no_web, &[], &review_cfg(), false, &no_mru);
            assert_eq!(m, CheckoutMenu::default(), "{web_url}");
        }
    }

    // --- Kandidaten und Plan ---

    #[test]
    fn candidates_need_gitlab_host_and_prefer_origin() {
        let both = repo(
            "/dev/both",
            "main",
            &[
                ("https", "https://gitlab.example.com/kern/backend-api.git"),
                ("origin", BACKEND),
            ],
        );
        let only_https = repo(
            "/dev/https",
            "main",
            &[("mirror", "https://GitLab.example.com/kern/backend-api.git")],
        );
        let github = repo(
            "/dev/github",
            "main",
            &[("origin", "https://github.com/kern/backend-api.git")],
        );
        let repos = [github, both, only_https];
        let m = menu(&mr(418, "feature/x"), &repos, &review_cfg(), false, &no_mru);
        let paths: Vec<&Path> = m.clones.iter().map(|c| c.path.as_path()).collect();
        assert_eq!(paths, [Path::new("/dev/both"), Path::new("/dev/https")]);
        let hosts = hosts_for(&repos);
        let m = mr(418, "feature/x");
        let info = MrRefInfo::from_mr(&m);
        let plan = |r: &RepoInfo| MrCheckoutPlan::for_repo(&info, r, &hosts).map(|p| p.remote);
        assert_eq!(plan(&repos[1]), Some("origin".to_string()));
        assert_eq!(plan(&repos[2]), Some("mirror".to_string()));
        assert_eq!(plan(&repos[0]), None);
    }

    #[test]
    fn checkout_plan_refs() {
        let m = mr(418, "feature/x");
        let repos = [backend("/dev/backend-api", "main")];
        let hosts = hosts_for(&repos);
        let plan = MrCheckoutPlan::for_repo(&MrRefInfo::from_mr(&m), &repos[0], &hosts)
            .expect("Plan für den Klon");
        assert_eq!(
            plan,
            MrCheckoutPlan {
                iid: 418,
                remote: "origin".to_string(),
                source_branch: "feature/x".to_string(),
                expected_sha: Some("0123abcd".to_string()),
            }
        );
        assert_eq!(
            plan.refspec(),
            "+refs/heads/feature/x:refs/remotes/origin/feature/x"
        );
        assert_eq!(plan.remote_ref(), "refs/remotes/origin/feature/x");
        assert_eq!(plan.upstream(), "origin/feature/x");
        assert_eq!(plan.fallback_branch(), "mr-418");
    }

    #[test]
    fn mr_ref_info_from_mr() {
        let m = mr(418, "feature/x");
        assert_eq!(
            MrRefInfo::from_mr(&m),
            MrRefInfo {
                host_key: HOST,
                iid: 418,
                project_path: "Kern/Backend-API",
                source_project_path: Some("Kern/Backend-API"),
                source_branch: "feature/x",
                target_branch: "main",
                source_branch_exists: Some(true),
                diff_head_sha: Some("0123abcd"),
                web_url: "https://gitlab.example.com/Kern/Backend-API/-/merge_requests/418",
            }
        );
    }

    #[test]
    fn open_branches_from_snapshot() {
        let mut fork = mr(77, "main");
        fork.source_project_path = Some("tim/backend-api".to_string());
        let mut unknown = mr(78, "feature/z");
        unknown.source_project_path = None;
        let snapshot = MrSnapshot {
            mine: vec![mr(412, "feature/csv-export")],
            others: vec![mr(418, "feature/mandantenfilter"), fork],
            uninvolved: Some(UninvolvedList {
                mrs: vec![mr(421, "feature/json-logging"), unknown],
                ..UninvolvedList::default()
            }),
            ..MrSnapshot::default()
        };
        let open = OpenBranches::from_snapshot(&snapshot);
        let lookup = MenuLookup {
            mru: &no_mru,
            exists: &nothing_exists,
        };
        let candidate = |branch: &str| {
            let r = backend("/dev/backend-api", branch);
            menu_with(&mr(1, "x"), &[r], &review_cfg(), false, &lookup, &open).clones[0]
                .state
                .clone()
        };
        assert_eq!(
            candidate("feature/csv-export"),
            CloneState::InUse { iid: Some(412) }
        );
        assert_eq!(
            candidate("feature/mandantenfilter"),
            CloneState::InUse { iid: Some(418) }
        );
        assert_eq!(
            candidate("feature/json-logging"),
            CloneState::InUse { iid: Some(421) }
        );
        // Ausweich-Branch eines früheren Checkouts
        assert_eq!(candidate("mr-418"), CloneState::InUse { iid: Some(418) });
        // `main` gehört zum Fork-MR !77 im Fork, nicht zu diesem Projekt
        assert_eq!(candidate("main"), CloneState::Free);
        // Quellprojekt unbekannt → kein Treffer
        assert_eq!(candidate("feature/z"), CloneState::InUse { iid: None });
    }
}
