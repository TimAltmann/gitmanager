//! „Neue Claude Session“: wählbare Ordner (gefundene Repos und Worktrees,
//! optional der Standard-Ordner aus den Einstellungen), Vorauswahl und
//! Suche.

use crate::claude_sessions::{matches_words, path_key};
use crate::config::{AppConfig, RepoUsage};
use crate::git::RepoInfo;
use crate::ui::repo_list::main_repo_name;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Ein Ordner, in dem eine neue Session starten kann.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSessionTarget {
    pub path: PathBuf,
    pub name: String,
    /// Branch (`None` beim Standard-Ordner außerhalb der Repos)
    pub branch: Option<String>,
    pub is_worktree: bool,
    /// Haupt-Repo eines verknüpften Worktrees
    pub main_name: Option<String>,
}

fn same_path(a: &Path, b: &Path) -> bool {
    path_key(&a.to_string_lossy()) == path_key(&b.to_string_lossy())
}

/// Alle Repos und Worktrees in Listenreihenfolge; liegt der Standard-Ordner
/// außerhalb, steht er als eigener Eintrag vorne.
pub fn new_session_targets(
    repos: &[RepoInfo],
    default_dir: Option<&Path>,
) -> Vec<NewSessionTarget> {
    let mut out = Vec::with_capacity(repos.len() + 1);
    if let Some(dir) = default_dir.filter(|d| !repos.iter().any(|r| same_path(&r.path, d))) {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.display().to_string());
        out.push(NewSessionTarget {
            path: dir.to_path_buf(),
            name,
            branch: None,
            is_worktree: false,
            main_name: None,
        });
    }
    out.extend(repos.iter().map(|r| NewSessionTarget {
        path: r.path.clone(),
        name: r.name.clone(),
        branch: Some(r.branch.clone()),
        is_worktree: r.is_linked_worktree,
        main_name: main_repo_name(r),
    }));
    out
}

/// Vorauswahl: Standard-Ordner, sonst das zuletzt geöffnete Repo, sonst
/// der erste Eintrag.
pub fn preselect(
    targets: &[NewSessionTarget],
    default_dir: Option<&Path>,
    usage: &HashMap<String, RepoUsage>,
) -> Option<usize> {
    if targets.is_empty() {
        return None;
    }
    if let Some(i) = default_dir.and_then(|d| targets.iter().position(|t| same_path(&t.path, d))) {
        return Some(i);
    }
    let last_opened = |t: &NewSessionTarget| {
        usage
            .get(&AppConfig::repo_state_key(&t.path))
            .and_then(|u| u.last_opened)
    };
    let recent = targets
        .iter()
        .enumerate()
        .filter_map(|(i, t)| last_opened(t).map(|ts| (ts, i)))
        // bei Gleichstand der frühere Eintrag
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|(_, i)| i);
    Some(recent.unwrap_or(0))
}

/// Suche: jedes Wort muss in Name, Branch, Pfad oder Haupt-Repo vorkommen.
pub fn target_matches_search(t: &NewSessionTarget, query: &str) -> bool {
    let path = t.path.display().to_string();
    let fields = [
        t.name.as_str(),
        t.branch.as_deref().unwrap_or(""),
        path.as_str(),
        t.main_name.as_deref().unwrap_or(""),
    ];
    matches_words(&fields, query)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(path: &str, branch: &str) -> RepoInfo {
        RepoInfo::new(PathBuf::from(path), branch.to_string(), false, false)
    }

    fn usage(ts: u64) -> RepoUsage {
        RepoUsage {
            last_opened: Some(ts),
            ..RepoUsage::default()
        }
    }

    #[test]
    fn targets_list_repos_and_worktrees_in_order() {
        let mut wt = repo("/dev/app-feature", "feature/x");
        wt.is_linked_worktree = true;
        wt.common_dir = PathBuf::from("/dev/app/.git");
        let t = new_session_targets(&[repo("/dev/app", "main"), wt], None);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].name, "app");
        assert_eq!(t[0].branch.as_deref(), Some("main"));
        assert!(!t[0].is_worktree);
        assert!(t[1].is_worktree);
        assert_eq!(t[1].main_name.as_deref(), Some("app"));
    }

    #[test]
    fn default_dir_outside_repos_comes_first() {
        let repos = [repo("/dev/app", "main")];
        let t = new_session_targets(&repos, Some(Path::new("/home/me/scratch")));
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].path, PathBuf::from("/home/me/scratch"));
        assert_eq!(t[0].name, "scratch");
        assert_eq!(t[0].branch, None);
        assert_eq!(
            preselect(&t, Some(Path::new("/home/me/scratch")), &HashMap::new()),
            Some(0)
        );
    }

    #[test]
    fn default_dir_inside_repos_is_not_duplicated_and_preselected() {
        let repos = [repo("/dev/a", "main"), repo("/dev/b", "main")];
        let t = new_session_targets(&repos, Some(Path::new("/dev/b/")));
        assert_eq!(t.len(), 2);
        assert_eq!(
            preselect(&t, Some(Path::new("/dev/b/")), &HashMap::new()),
            Some(1)
        );
    }

    #[test]
    fn preselect_falls_back_to_last_opened_then_first() {
        let repos = [
            repo("/dev/a", "main"),
            repo("/dev/b", "main"),
            repo("/dev/c", "main"),
        ];
        let t = new_session_targets(&repos, None);
        assert_eq!(preselect(&t, None, &HashMap::new()), Some(0));
        let mut u = HashMap::new();
        u.insert(AppConfig::repo_state_key(Path::new("/dev/a")), usage(10));
        u.insert(AppConfig::repo_state_key(Path::new("/dev/c")), usage(20));
        assert_eq!(preselect(&t, None, &u), Some(2));
        assert_eq!(preselect(&[], None, &u), None);
    }

    #[test]
    fn search_matches_name_branch_path_and_main_repo() {
        let mut wt = repo("/dev/app-feature", "feature/login");
        wt.is_linked_worktree = true;
        wt.common_dir = PathBuf::from("/dev/backend/.git");
        let t = &new_session_targets(&[wt], None)[0];
        assert!(target_matches_search(t, ""));
        assert!(target_matches_search(t, "LOGIN"));
        assert!(target_matches_search(t, "backend feature"));
        assert!(target_matches_search(t, "/dev/app"));
        assert!(!target_matches_search(t, "frontend"));
    }
}
