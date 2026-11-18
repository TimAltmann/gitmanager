//! „Aufräumen …“: reine Kandidatenlogik ohne git2-Aufrufe und Netz. Der
//! Worker in `app::cleanup` liefert die Eingaben (verwaiste Branches nach
//! `fetch --prune`, gemergte MRs von GitLab) und ergänzt die Prüfungen, die
//! ein Repository brauchen (`apply_checks`).

use crate::git::{self, BranchSafety, RepoInfo};
use std::collections::HashMap;
use std::path::PathBuf;

/// Warum ein Eintrag aufgeräumt werden kann.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupReason {
    /// Merge Request dieses Quell-Branches ist auf GitLab gemergt
    MrMerged { iid: u64 },
    /// Upstream konfiguriert, Remote-Branch aber gelöscht
    UpstreamGone,
}

/// Was entfernt wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupTarget {
    /// Verknüpfter Worktree; `main` ist das Hauptrepo für `git worktree remove`
    Worktree {
        path: PathBuf,
        main: PathBuf,
        branch: Option<String>,
    },
    /// Lokaler Branch eines Haupt-Klons
    Branch { repo: PathBuf, branch: String },
}

/// Ein Eintrag im Dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupCandidate {
    pub target: CleanupTarget,
    pub reasons: Vec<CleanupReason>,
    pub dirty_files: usize,
    pub ahead: usize,
    /// Detached HEAD mit sonst nirgends erreichbaren Commits (nur Worktrees)
    pub head_unreachable: bool,
    /// Vom Nutzer angehakt
    pub selected: bool,
    /// Worktree: Branch mitlöschen (nur mit `branch_safety`)
    pub delete_branch: bool,
    /// Worktree: darf sein Branch gelöscht werden? Branch-Eintrag: `None` =
    /// nicht gemergt, Löschen verliert lokale Commits (`-D`)
    pub branch_safety: Option<BranchSafety>,
    /// Spitze des Branches bei der Suche; ändert sie sich bis zur
    /// Ausführung, wird der Branch nicht gelöscht
    pub tip: Option<git2::Oid>,
    /// Kopf-Commits der gemergten MRs dieses Branches (Worktrees)
    pub merged_heads: Vec<git2::Oid>,
    /// Worktree: offener MR auf diesem Ordner (iid), wird noch benutzt
    pub open_mr: Option<u64>,
    /// Worktree: in diesem Ordner läuft eine Claude Session
    pub claude_running: bool,
}

impl CleanupCandidate {
    /// Verlustgefahr: vorab abgewählt, orange markiert; ausgeführt nur mit
    /// ausdrücklichem Haken und dann mit `--force` bzw. `-D`.
    pub fn needs_force(&self) -> bool {
        match &self.target {
            CleanupTarget::Worktree { .. } => {
                self.dirty_files > 0 || self.ahead > 0 || self.head_unreachable
            }
            CleanupTarget::Branch { .. } => self.branch_safety.is_none(),
        }
    }

    /// Ist der MR des Worktree-Branches gemergt? (Prüfhilfe der Tests; die
    /// Branch-Sicherheit hängt an `merged_heads`, nicht am Grund.)
    #[cfg(test)]
    pub fn mr_merged(&self) -> bool {
        self.reasons
            .iter()
            .any(|r| matches!(r, CleanupReason::MrMerged { .. }))
    }

    /// „Branch mitlöschen“ ist nur bei Worktrees mit sicherem Branch wählbar.
    pub fn can_delete_branch(&self) -> bool {
        matches!(
            &self.target,
            CleanupTarget::Worktree {
                branch: Some(_),
                ..
            }
        ) && self.branch_safety.is_some()
    }

    /// Übernimmt die Prüfungen aus dem Repository und setzt die Vorauswahl
    /// neu: nur ohne Verlustgefahr angehakt; den Branch nur mitlöschen, wenn
    /// er sicher ist und der Eintrag selbst keine Warnung hat.
    pub fn apply_checks(&mut self, head_unreachable: bool, safety: Option<BranchSafety>) {
        self.head_unreachable = head_unreachable;
        self.branch_safety = safety;
        self.selected = !self.needs_force();
        self.delete_branch = self.can_delete_branch() && !self.needs_force();
    }

    /// Worktree noch in Benutzung (offener MR, laufende Claude Session):
    /// nie vorausgewählt, der Dialog warnt. Entfernen nur per Haken.
    pub fn mark_in_use(&mut self, open_mr: Option<u64>, claude_running: bool) {
        if !matches!(self.target, CleanupTarget::Worktree { .. }) {
            return;
        }
        self.open_mr = open_mr;
        self.claude_running = claude_running;
        if open_mr.is_some() || claude_running {
            self.selected = false;
            self.delete_branch = false;
        }
    }
}

/// Kandidaten aus den gescannten Repos.
///
/// `gone`: Repo-Pfad → lokale Branches mit verwaistem Upstream (bei
/// Worktrees nur ihr eigener Branch). `merged`: Repo-Pfad eines Worktrees →
/// gemergte MR-iid aus der GitLab-Abfrage. Worktrees kommen zuerst, dann
/// Branches, jeweils in Scan-Reihenfolge.
pub fn collect_candidates(
    repos: &[RepoInfo],
    gone: &HashMap<PathBuf, Vec<String>>,
    merged: &HashMap<PathBuf, u64>,
) -> Vec<CleanupCandidate> {
    let mut out = Vec::new();
    for repo in repos.iter().filter(|r| r.is_linked_worktree) {
        let branch = (!repo.is_detached && !repo.branch.is_empty()).then(|| repo.branch.clone());
        let mut reasons = Vec::new();
        if let Some(&iid) = merged.get(&repo.path) {
            reasons.push(CleanupReason::MrMerged { iid });
        }
        if branch
            .as_ref()
            .is_some_and(|b| gone.get(&repo.path).is_some_and(|g| g.contains(b)))
        {
            reasons.push(CleanupReason::UpstreamGone);
        }
        if reasons.is_empty() {
            continue;
        }
        out.push(CleanupCandidate {
            target: CleanupTarget::Worktree {
                path: repo.path.clone(),
                main: git::main_repo_of(&repo.common_dir),
                branch,
            },
            reasons,
            dirty_files: repo.dirty_files,
            ahead: repo.ahead,
            head_unreachable: false,
            selected: repo.dirty_files == 0 && repo.ahead == 0,
            delete_branch: false,
            branch_safety: None,
            tip: None,
            merged_heads: Vec::new(),
            open_mr: None,
            claude_running: false,
        });
    }
    for repo in repos.iter().filter(|r| !r.is_linked_worktree) {
        let Some(branches) = gone.get(&repo.path) else {
            continue;
        };
        for branch in branches {
            // Nie den ausgecheckten Branch, nie einen, der in einem Worktree hängt
            let checked_out = (!repo.is_detached && repo.branch == *branch)
                || repo
                    .worktrees
                    .iter()
                    .any(|w| w.branch.as_deref() == Some(branch.as_str()));
            if checked_out {
                continue;
            }
            out.push(CleanupCandidate {
                target: CleanupTarget::Branch {
                    repo: repo.path.clone(),
                    branch: branch.clone(),
                },
                reasons: vec![CleanupReason::UpstreamGone],
                dirty_files: 0,
                ahead: 0,
                head_unreachable: false,
                selected: true,
                delete_branch: false,
                branch_safety: None,
                tip: None,
                merged_heads: Vec::new(),
                open_mr: None,
                claude_running: false,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::WorktreeHead;

    fn worktree(path: &str, branch: &str) -> RepoInfo {
        let mut r = RepoInfo::new(PathBuf::from(path), branch.into(), false, false);
        r.is_linked_worktree = true;
        r.common_dir = PathBuf::from("/x/main/.git");
        r
    }

    fn main_clone(branch: &str) -> RepoInfo {
        let mut r = RepoInfo::new(PathBuf::from("/x/main"), branch.into(), false, false);
        r.common_dir = PathBuf::from("/x/main/.git");
        r
    }

    fn gone(entries: &[(&str, &[&str])]) -> HashMap<PathBuf, Vec<String>> {
        entries
            .iter()
            .map(|(p, b)| (PathBuf::from(p), b.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn worktree_with_merged_mr_is_candidate() {
        let repos = vec![main_clone("main"), worktree("/x/wt", "feat")];
        let merged = HashMap::from([(PathBuf::from("/x/wt"), 42)]);
        let got = collect_candidates(&repos, &HashMap::new(), &merged);
        assert_eq!(got.len(), 1);
        let c = &got[0];
        assert_eq!(
            c.target,
            CleanupTarget::Worktree {
                path: PathBuf::from("/x/wt"),
                main: PathBuf::from("/x/main"),
                branch: Some("feat".into()),
            }
        );
        assert_eq!(c.reasons, vec![CleanupReason::MrMerged { iid: 42 }]);
        assert!(c.selected && c.mr_merged());
    }

    #[test]
    fn worktree_with_gone_upstream_is_candidate_with_both_reasons_merged() {
        let repos = vec![worktree("/x/wt", "feat"), worktree("/x/wt2", "other")];
        let g = gone(&[("/x/wt", &["feat"])]);
        let got = collect_candidates(&repos, &g, &HashMap::new());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].reasons, vec![CleanupReason::UpstreamGone]);
        assert!(!got[0].mr_merged());

        let merged = HashMap::from([(PathBuf::from("/x/wt"), 7)]);
        let got = collect_candidates(&repos, &g, &merged);
        assert_eq!(got.len(), 1, "beide Gründe ergeben einen Eintrag");
        assert_eq!(
            got[0].reasons,
            vec![
                CleanupReason::MrMerged { iid: 7 },
                CleanupReason::UpstreamGone
            ]
        );
    }

    #[test]
    fn dirty_or_ahead_candidate_is_preselected_off() {
        let mut dirty = worktree("/x/a", "a");
        dirty.dirty_files = 2;
        let mut ahead = worktree("/x/b", "b");
        ahead.ahead = 1;
        let clean = worktree("/x/c", "c");
        let g = gone(&[("/x/a", &["a"]), ("/x/b", &["b"]), ("/x/c", &["c"])]);
        let got = collect_candidates(&[dirty, ahead, clean], &g, &HashMap::new());
        let selected: Vec<bool> = got.iter().map(|c| c.selected).collect();
        assert_eq!(selected, vec![false, false, true]);
        assert_eq!(got[0].dirty_files, 2);
        assert_eq!(got[1].ahead, 1);
    }

    #[test]
    fn gone_branch_checked_out_in_worktree_is_skipped() {
        let mut main = main_clone("main");
        main.worktrees = vec![WorktreeHead {
            path: PathBuf::from("/x/wt"),
            branch: Some("feat".into()),
        }];
        let g = gone(&[("/x/main", &["feat", "main", "old"])]);
        let got = collect_candidates(&[main], &g, &HashMap::new());
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].target,
            CleanupTarget::Branch {
                repo: PathBuf::from("/x/main"),
                branch: "old".into(),
            }
        );
        assert_eq!(got[0].reasons, vec![CleanupReason::UpstreamGone]);
        // Verwaiste Branches in Worktree-Einträgen zählen nicht als Branch-Kandidat
        let wt = worktree("/x/wt", "x");
        let g = gone(&[("/x/wt", &["y"])]);
        assert!(collect_candidates(&[wt], &g, &HashMap::new()).is_empty());
    }

    #[test]
    fn no_candidates_empty() {
        let repos = vec![main_clone("main"), worktree("/x/wt", "feat")];
        assert!(collect_candidates(&repos, &HashMap::new(), &HashMap::new()).is_empty());
        assert!(collect_candidates(&[], &HashMap::new(), &HashMap::new()).is_empty());
        // Detached Worktree: nie „gone“, ohne MR kein Kandidat
        let mut detached = worktree("/x/d", "");
        detached.is_detached = true;
        let g = gone(&[("/x/d", &[""])]);
        assert!(collect_candidates(&[detached], &g, &HashMap::new()).is_empty());
    }

    #[test]
    fn checks_update_preselection_and_branch_choice() {
        let mut c = collect_candidates(
            &[worktree("/x/wt", "feat")],
            &gone(&[("/x/wt", &["feat"])]),
            &HashMap::new(),
        )
        .remove(0);
        c.apply_checks(false, Some(BranchSafety::MergedIntoDefault));
        assert!(c.selected && c.delete_branch && c.can_delete_branch());
        c.apply_checks(true, Some(BranchSafety::MergedIntoDefault));
        assert!(!c.selected && !c.delete_branch, "unerreichbarer HEAD warnt");
        c.apply_checks(false, None);
        assert!(c.selected && !c.delete_branch && !c.can_delete_branch());

        let mut b = collect_candidates(
            &[main_clone("main")],
            &gone(&[("/x/main", &["old"])]),
            &HashMap::new(),
        )
        .remove(0);
        b.apply_checks(false, None);
        assert!(b.needs_force() && !b.selected, "nicht gemergt: abgewählt");
        b.apply_checks(false, Some(BranchSafety::MergedIntoDefault));
        assert!(!b.needs_force() && b.selected && !b.can_delete_branch());
    }
}
