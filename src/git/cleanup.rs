//! Aufräumen: Sicherheit beim Löschen von Branches, verwaiste Branches,
//! Worktrees und lokale Branches entfernen.

use super::{
    branch_upstream, canonical_key_path, run_git_cli, strip_verbatim, GIT_CLI_WORKTREE_TIMEOUT,
};
use git2::{BranchType, Repository};
use std::path::{Path, PathBuf};

/// Warum ein lokaler Branch gefahrlos gelöscht werden darf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchSafety {
    /// Upstream existiert und enthält alle lokalen Commits. Vertraut dem
    /// zuletzt gefetchten Tracking-Ref: ohne frischen Fetch kann der Remote
    /// inzwischen anders aussehen.
    Pushed,
    /// Branch-Spitze ist schon im Default-Branch des Remotes enthalten
    MergedIntoDefault,
    /// Der Merge Request ist laut Hoster gemergt
    MrMerged,
}

impl BranchSafety {
    /// `git branch -d` prüft die Merge-Eigenschaft gegen den Upstream bzw.
    /// den HEAD des Hauptrepos, nicht gegen den Default-Branch des Remotes.
    /// Bei `MergedIntoDefault` und `MrMerged` würde `-d` daher trotz sicherer
    /// Lage scheitern; dort muss `-D` genommen werden. Nur `Pushed` kommt
    /// mit `-d` aus.
    pub fn needs_force_delete(self) -> bool {
        !matches!(self, BranchSafety::Pushed)
    }
}

/// Prüft, ob ein lokaler Branch ohne Datenverlust gelöscht werden darf.
/// `None` = nicht sicher (Aufrufer muss ausdrücklich bestätigen lassen).
///
/// `merged_heads`: Kopf-Commits gemergter MRs dieses Branches. `MrMerged`
/// gilt nur, wenn die lokale Spitze einer davon ist oder darin enthalten
/// ist; spätere lokale Commits (Branch weiterbenutzt, Name wiederverwendet)
/// machen den Branch unsicher. Ein lokal unbekannter Kopf zählt nicht.
pub fn branch_safety(
    repo_path: &Path,
    branch: &str,
    merged_heads: &[git2::Oid],
) -> Option<BranchSafety> {
    let repo = Repository::open(repo_path).ok()?;
    let local = repo.find_branch(branch, BranchType::Local).ok()?;
    let oid = local.get().target()?;
    if let Some(up_oid) = local.upstream().ok().and_then(|u| u.get().target()) {
        if let Ok((ahead, _)) = repo.graph_ahead_behind(oid, up_oid) {
            if ahead == 0 {
                return Some(BranchSafety::Pushed);
            }
        }
    }
    // Default-Branch des Remotes: origin/HEAD, sonst origin/main, origin/master
    let mut defaults = Vec::new();
    if let Ok(head) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let Ok(resolved) = head.resolve() {
            defaults.extend(resolved.target());
        }
    }
    for name in ["refs/remotes/origin/main", "refs/remotes/origin/master"] {
        defaults.extend(repo.refname_to_id(name).ok());
    }
    let merged = defaults
        .into_iter()
        .any(|d| d == oid || repo.graph_descendant_of(d, oid).unwrap_or(false));
    if merged {
        return Some(BranchSafety::MergedIntoDefault);
    }
    tip_in_merged_heads(&repo, oid, merged_heads).then_some(BranchSafety::MrMerged)
}

/// Ist `tip` einer der gemergten MR-Köpfe oder darin enthalten? Ein lokal
/// unbekannter Kopf zählt nicht. Gemeinsame Regel für die Branch-Sicherheit
/// und den Aufräum-Grund „MR gemergt“.
pub fn tip_in_merged_heads(repo: &Repository, tip: git2::Oid, merged_heads: &[git2::Oid]) -> bool {
    merged_heads.iter().any(|&head| {
        head == tip
            || (repo.find_commit(head).is_ok()
                && repo.graph_descendant_of(head, tip).unwrap_or(false))
    })
}

/// Hat der Branch einen konfigurierten Upstream, dessen Remote-Branch aber
/// nicht mehr existiert (nach `fetch --prune` gelöscht)?
pub fn upstream_gone(repo: &Repository, branch: &str) -> bool {
    // Ein lokaler Upstream (".") hat kein Remote-Tracking-Ref: nie „gone“
    let Ok(Some((remote, name))) = branch_upstream(repo, branch) else {
        return false;
    };
    repo.find_reference(&format!("refs/remotes/{remote}/{name}"))
        .is_err()
}

/// Lokale Branches mit verwaistem Upstream, ohne den aktuell ausgecheckten
/// und ohne die in anderen Worktrees ausgecheckten.
pub fn gone_local_branches(repo_path: &Path) -> Vec<String> {
    let Ok(repo) = Repository::open(repo_path) else {
        return Vec::new();
    };
    let mut checked_out: Vec<String> = Vec::new();
    let mut note_head = |r: &Repository| {
        if let Ok(head) = r.find_reference("HEAD") {
            if let Some(name) = head
                .symbolic_target()
                .ok()
                .flatten()
                .and_then(|t| t.strip_prefix("refs/heads/"))
            {
                checked_out.push(name.to_string());
            }
        }
    };
    note_head(&repo);
    // Aus einem Worktree heraus: auch das Hauptrepo und alle Geschwister
    let main = Repository::open(repo.commondir()).ok();
    let base = main.as_ref().unwrap_or(&repo);
    if !base.is_bare() {
        note_head(base);
    }
    if let Ok(names) = base.worktrees() {
        for name in names.iter().flatten().flatten() {
            if let Ok(wt) = base.find_worktree(name) {
                if let Ok(wt_repo) = Repository::open_from_worktree(&wt) {
                    note_head(&wt_repo);
                }
            }
        }
    }
    let Ok(branches) = repo.branches(Some(BranchType::Local)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = branches
        .flatten()
        .filter_map(|(b, _)| b.name().ok().flatten().map(str::to_string))
        .filter(|n| !checked_out.contains(n) && upstream_gone(&repo, n))
        .collect();
    out.sort();
    out
}

/// Hauptrepo eines Worktrees: Elternordner von `common_dir` (heißt er
/// `.git`), sonst `common_dir` selbst (Bare-Repo).
pub fn main_repo_of(common_dir: &Path) -> PathBuf {
    if common_dir.file_name().is_some_and(|n| n == ".git") {
        if let Some(parent) = common_dir.parent() {
            return parent.to_path_buf();
        }
    }
    common_dir.to_path_buf()
}

/// Ist HEAD des Worktrees detached und sein Commit von keinem lokalen oder
/// Remote-Tracking-Branch erreichbar? Dann gingen beim Entfernen Commits
/// verloren. Im Zweifel (Fehler beim Öffnen/Lesen) `true`.
pub fn head_unreachable(worktree_path: &Path) -> bool {
    let Ok(repo) = Repository::open(worktree_path) else {
        return true;
    };
    match repo.head_detached() {
        Ok(false) => return false,
        Ok(true) => {}
        Err(_) => return true,
    }
    let Some(head) = repo.head().ok().and_then(|h| h.target()) else {
        return true;
    };
    let Ok(refs) = repo.references() else {
        return true;
    };
    for r in refs {
        let Ok(r) = r else {
            return true;
        };
        let Ok(name) = r.name() else {
            continue;
        };
        if !(name.starts_with("refs/heads/") || name.starts_with("refs/remotes/")) {
            continue;
        }
        let Ok(r) = r.resolve() else {
            continue;
        };
        if let Some(tip) = r.target() {
            match repo.graph_descendant_of(tip, head) {
                Ok(true) => return false,
                Ok(false) if tip == head => return false,
                Ok(false) => {}
                Err(_) => return true,
            }
        }
    }
    true
}

/// Entfernt einen Worktree über System-`git`. Ohne `force` verweigert git
/// bei uncommitteten Änderungen (und gesperrten Worktrees); zusätzlich
/// verweigern wir bei detached HEAD mit sonst nirgends erreichbaren Commits
/// ([`head_unreachable`]), bevor etwas angefasst wird. Existiert der Ordner
/// schon nicht mehr, wird nur der verwaiste Eintrag geprunt; `worktree
/// prune` wirkt dabei auf das ganze Repo, nicht nur auf diesen Worktree. Am
/// Ende muss der Pfad aus der Worktree-Liste verschwunden sein, sonst `Err`.
/// Der Branch wird nur gelöscht, wenn `delete_branch` gesetzt ist; sein
/// `force_delete` entscheidet zwischen `-d` und `-D`.
pub fn remove_worktree(
    worktree_path: &Path,
    main_repo: &Path,
    force: bool,
    delete_branch: Option<(&str, bool)>,
) -> anyhow::Result<()> {
    let worktree_path = strip_verbatim(worktree_path);
    let main_repo = strip_verbatim(main_repo);
    let main_str = main_repo.display().to_string();
    let path_str = worktree_path.display().to_string();
    let exists = worktree_path.exists();
    if exists && !force && head_unreachable(&worktree_path) {
        anyhow::bail!(
            "worktree '{path_str}' has a detached HEAD with commits not reachable from any \
             branch; removing it would lose them (force required)"
        );
    }
    if exists {
        let mut args = vec!["-C", &main_str, "worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push("--");
        args.push(&path_str);
        run_git_cli(&args, "worktree remove", GIT_CLI_WORKTREE_TIMEOUT)?;
    }
    // Immer: räumt auch Einträge, deren Ordner von Hand gelöscht wurde
    run_git_cli(
        &["-C", &main_str, "worktree", "prune"],
        "worktree prune",
        GIT_CLI_WORKTREE_TIMEOUT,
    )?;
    // Nichts als erledigt melden, was git unter anderem Pfad führt
    let repo = Repository::open(&main_repo)?;
    let wanted = canonical_key_path(&worktree_path);
    for name in repo.worktrees()?.iter().flatten().flatten() {
        if let Ok(wt) = repo.find_worktree(name) {
            if canonical_key_path(&strip_verbatim(wt.path())) == wanted {
                anyhow::bail!("worktree '{path_str}' is still registered after removal");
            }
        }
    }
    if let Some((branch, force_delete)) = delete_branch {
        delete_local_branch(&main_repo, branch, force_delete)?;
    }
    Ok(())
}

/// Löscht einen lokalen Branch (`git branch -d`, mit `force` `-D`). Ohne
/// `force` verweigert git bei ungemergten Commits.
pub fn delete_local_branch(repo_path: &Path, branch: &str, force: bool) -> anyhow::Result<()> {
    let repo_str = repo_path.display().to_string();
    let flag = if force { "-D" } else { "-d" };
    run_git_cli(
        &["-C", &repo_str, "branch", flag, "--", branch],
        &format!("branch {flag} '{branch}'"),
        GIT_CLI_WORKTREE_TIMEOUT,
    )
}
