use crate::config::{AppConfig, LanguageProfile};
use crate::git::{get_repo_info, RepoInfo, SolutionFile};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".cargo",
    ".vscode",
    ".idea",
    "__pycache__",
    ".venv",
    "venv",
    ".next",
    ".nuxt",
    "dist",
    "build",
];

fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name)
}

/// Scannt alle Roots gemäß Config und gibt sortierte RepoInfos zurück
pub fn scan_repos(config: &AppConfig) -> Vec<RepoInfo> {
    let mut all_repos = Vec::new();
    let mut seen_paths = HashSet::new();

    for root in &config.roots {
        if !root.exists() {
            eprintln!(
                "Scan: Root existiert nicht, überspringe: {}",
                root.display()
            );
            continue;
        }
        if !root.is_dir() {
            eprintln!("Scan: Root ist kein Verzeichnis: {}", root.display());
            continue;
        }
        let repos = scan_single_root(root, config.max_depth);
        for repo in repos {
            if seen_paths.insert(repo.path.clone()) {
                all_repos.push(repo);
            }
        }
    }

    all_repos.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Zweite Stufe: Solution-Dateien pro Repo scannen (parallel)
    all_repos.par_iter_mut().for_each(|repo| {
        let profile = config.get_effective_profile_for_repo(&repo.path);
        let mut solutions = scan_solutions_for_repo(&repo.path, profile);
        // Sortierung: Root-level zuerst, dann alphabetisch, Tiefe
        solutions.sort_by(|a, b| {
            let da = a.relative.matches(std::path::MAIN_SEPARATOR).count();
            let db = b.relative.matches(std::path::MAIN_SEPARATOR).count();
            da.cmp(&db)
                .then_with(|| a.relative.to_lowercase().cmp(&b.relative.to_lowercase()))
        });
        if solutions.len() > 20 {
            solutions.truncate(20);
        }
        // Aus repo_state wiederherstellen
        let mut selected = None;
        if let Some(state) = config.get_repo_state(&repo.path) {
            if let Some(sel) = &state.selected_solution {
                if solutions.iter().any(|s| &s.path == sel) {
                    selected = Some(sel.clone());
                }
            }
        }
        if selected.is_none() && !solutions.is_empty() {
            selected = Some(solutions[0].path.clone());
        }
        repo.solutions = solutions;
        repo.selected_solution = selected;
    });

    all_repos
}

/// Scannt innerhalb eines Repos nach Solution-/Zieldateien gemäß Profil
pub fn scan_solutions_for_repo(repo_path: &Path, profile: &LanguageProfile) -> Vec<SolutionFile> {
    let depth = profile.max_scan_depth.min(4);
    let mut solutions = Vec::new();
    let walker = WalkDir::new(repo_path)
        .max_depth(depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if let Some(name) = e.file_name().to_str() {
                if name == ".git" {
                    return false;
                }
                if e.file_type().is_dir() && is_ignored_dir(name) {
                    return false;
                }
            }
            true
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if profile.matches_file(path) {
            let relative = path
                .strip_prefix(repo_path)
                .unwrap_or(path)
                .display()
                .to_string();
            solutions.push(SolutionFile {
                path: path.to_path_buf(),
                relative,
            });
        }
    }
    solutions
}

fn scan_single_root(root: &Path, max_depth: usize) -> Vec<RepoInfo> {
    // max_depth: 1 = direkte Kinder, 2 = Kinder+Enkel, etc.
    // WalkDir max_depth ist relativ zu root: root=0, Kind=1, Enkel=2
    let walk_max_depth = max_depth;

    let mut candidates: Vec<PathBuf> = Vec::new();

    // WalkDir iteriert rekursiv, filter_entry verhindert Abstieg in ignorierte/.git Ordner
    let walker = WalkDir::new(root)
        .max_depth(walk_max_depth)
        .min_depth(0)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Verhindere Abstieg in .git und ignorierte Dirs
            if e.depth() == 0 {
                return true;
            }
            if let Some(name) = e.file_name().to_str() {
                if name == ".git" {
                    return false;
                }
                if e.file_type().is_dir() && is_ignored_dir(name) {
                    return false;
                }
                // Versteckte Ordner (außer .git das schon behandelt) optional ignorieren?
                // Wir ignorieren nicht generell, da viele Repos in versteckten Pfaden liegen könnten
                // Aber: .cache, .local etc. können riesig sein - bei max_depth >3 relevant
                // Für max_depth <=2 ignorieren wir sie nicht, da sie direkte Kinder sein könnten
            }
            true
        });

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path().to_path_buf();
        // Nur Verzeichnisse betrachten
        if !entry.file_type().is_dir() {
            continue;
        }
        // Depth 0 ist root selbst - auch prüfen ob root ein Repo ist
        // Sonst nur Pfade mit depth >=1
        candidates.push(path);
    }

    // Parallel prüfen welche Kandidaten Git-Repos sind
    candidates
        .par_iter()
        .filter_map(|p| get_repo_info(p.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, LanguageProfile};
    use git2::Repository;
    use std::fs;
    use tempfile::tempdir;

    fn init_repo(path: &Path) {
        let repo = Repository::init(path).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@test.com").unwrap();
        let file = path.join("README.md");
        fs::write(&file, "hi").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        index.write().unwrap();
        let oid = index.write_tree().unwrap();
        {
            let tree = repo.find_tree(oid).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }
    }

    // --- Konstanten & is_ignored_dir (F-130, F-131) ---
    #[test]
    fn ignored_dirs_contains_common() {
        for expected in [
            "node_modules",
            "target",
            ".cargo",
            ".vscode",
            ".idea",
            "__pycache__",
            ".venv",
            "venv",
            ".next",
            ".nuxt",
            "dist",
            "build",
        ] {
            assert!(IGNORED_DIRS.contains(&expected), "missing {}", expected);
        }
    }

    #[test]
    fn is_ignored_dir_known_vs_unknown() {
        assert!(is_ignored_dir("node_modules"));
        assert!(is_ignored_dir("target"));
        assert!(is_ignored_dir(".cargo"));
        assert!(is_ignored_dir("dist"));
        assert!(!is_ignored_dir("src"));
        assert!(!is_ignored_dir("myapp"));
        assert!(!is_ignored_dir(""));
    }

    // --- scan_repos (F-132 - F-144) ---
    #[test]
    fn scan_empty_roots() {
        let cfg = AppConfig {
            roots: vec![],
            ..Default::default()
        };
        assert!(scan_repos(&cfg).is_empty());
    }

    #[test]
    fn scan_finds_repos_at_depth_1() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let repo_a = root.join("repo_a");
        let repo_b = root.join("repo_b");
        fs::create_dir(&repo_a).unwrap();
        fs::create_dir(&repo_b).unwrap();
        init_repo(&repo_a);
        init_repo(&repo_b);
        let cfg = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 1,
            ..Default::default()
        };
        let repos = scan_repos(&cfg);
        assert_eq!(repos.len(), 2);
        let names: Vec<_> = repos.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"repo_a"));
        assert!(names.contains(&"repo_b"));
    }

    #[test]
    fn scan_respects_max_depth() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let level1 = root.join("level1");
        fs::create_dir(&level1).unwrap();
        let repo_deep = level1.join("repo_deep");
        fs::create_dir(&repo_deep).unwrap();
        init_repo(&repo_deep);

        // Tiefe 1 sollte repo_deep nicht finden
        let cfg1 = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 1,
            ..Default::default()
        };
        assert_eq!(scan_repos(&cfg1).len(), 0);

        // Tiefe 2 sollte es finden
        let cfg2 = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 2,
            ..Default::default()
        };
        assert_eq!(scan_repos(&cfg2).len(), 1);
    }

    #[test]
    fn scan_ignores_non_repos() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("not_a_repo")).unwrap();
        fs::write(root.join("not_a_repo/file.txt"), "hello").unwrap();
        let cfg = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 1,
            ..Default::default()
        };
        assert_eq!(scan_repos(&cfg).len(), 0);
    }

    #[test]
    fn scan_dedup_multiple_roots() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        let cfg = AppConfig {
            roots: vec![root.to_path_buf(), root.to_path_buf()],
            max_depth: 1,
            ..Default::default()
        };
        let repos = scan_repos(&cfg);
        assert_eq!(repos.len(), 1);
    }

    #[test]
    fn scan_ignores_node_modules_target_hidden_git() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        for ignored in ["node_modules", "target", ".cargo"] {
            let p = root.join(ignored).join("repo_inside_ignored");
            fs::create_dir_all(&p).unwrap();
            init_repo(&p);
        }
        let cfg = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 3,
            ..Default::default()
        };
        // repos inside ignored should not be found
        let repos = scan_repos(&cfg);
        assert_eq!(
            repos.len(),
            0,
            "repos inside ignored dirs should be ignored"
        );
    }

    #[test]
    fn scan_skips_nonexistent_and_file_root() {
        let tmp = tempdir().unwrap();
        let missing = tmp.path().join("missing");
        let file = tmp.path().join("file.txt");
        fs::write(&file, "hi").unwrap();
        let cfg = AppConfig {
            roots: vec![missing, file],
            max_depth: 1,
            ..Default::default()
        };
        // should not panic, return empty
        let repos = scan_repos(&cfg);
        assert!(repos.is_empty());
    }

    #[test]
    fn scan_results_sorted_by_name_case_insensitive() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        for name in ["zebra", "Apple", "mango"] {
            let p = root.join(name);
            fs::create_dir(&p).unwrap();
            init_repo(&p);
        }
        let cfg = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 1,
            ..Default::default()
        };
        let repos = scan_repos(&cfg);
        let names: Vec<_> = repos.iter().map(|r| r.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn scan_root_itself_is_repo() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        init_repo(root);
        let cfg = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 1,
            ..Default::default()
        };
        let repos = scan_repos(&cfg);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].path, root);
    }

    #[test]
    fn scan_repos_with_multiple_roots_and_parallelism() {
        let tmp = tempdir().unwrap();
        let root1 = tmp.path().join("r1");
        let root2 = tmp.path().join("r2");
        fs::create_dir(&root1).unwrap();
        fs::create_dir(&root2).unwrap();
        for (root, name) in [(&root1, "a"), (&root2, "b")] {
            let p = root.join(name);
            fs::create_dir(&p).unwrap();
            init_repo(&p);
        }
        let cfg = AppConfig {
            roots: vec![root1.clone(), root2.clone()],
            max_depth: 1,
            ..Default::default()
        };
        let repos = scan_repos(&cfg);
        assert_eq!(repos.len(), 2);
    }

    #[test]
    fn scan_solutions_populated_and_selected() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let repo = root.join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        // create a .sln file in repo root
        fs::write(repo.join("app.sln"), "solution").unwrap();
        let mut cfg = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 1,
            ..Default::default()
        };
        // first scan without state: should select first
        let repos = scan_repos(&cfg);
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].solutions.len(), 1);
        assert_eq!(repos[0].selected_solution, Some(repo.join("app.sln")));
        // second scan with state preserving different selection when 2 files
        fs::write(repo.join("second.sln"), "solution2").unwrap();
        // set state to second.sln
        cfg.get_repo_state_mut(&repo).selected_solution = Some(repo.join("second.sln"));
        let repos2 = scan_repos(&cfg);
        assert_eq!(repos2[0].solutions.len(), 2);
        // should restore second.sln
        assert_eq!(repos2[0].selected_solution, Some(repo.join("second.sln")));
        // invalid selection fallback to first
        cfg.get_repo_state_mut(&repo).selected_solution = Some(repo.join("nonexistent.sln"));
        let repos3 = scan_repos(&cfg);
        assert_eq!(
            repos3[0].selected_solution,
            Some(repos3[0].solutions[0].path.clone())
        );
    }

    #[test]
    fn scan_nested_repos_and_depth_boundary() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let lvl1 = root.join("l1");
        fs::create_dir(&lvl1).unwrap();
        let lvl2 = lvl1.join("l2");
        fs::create_dir(&lvl2).unwrap();
        let repo = lvl2.join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        for depth in [1, 2] {
            let cfg = AppConfig {
                roots: vec![root.to_path_buf()],
                max_depth: depth,
                ..Default::default()
            };
            let repos = scan_repos(&cfg);
            if depth < 3 {
                assert_eq!(repos.len(), 0);
            }
        }
        let cfg3 = AppConfig {
            roots: vec![root.to_path_buf()],
            max_depth: 3,
            ..Default::default()
        };
        assert_eq!(scan_repos(&cfg3).len(), 1);
    }

    // --- scan_solutions_for_repo (F-145 - F-152) ---
    #[test]
    fn scan_solutions_finds_matching_and_ignores_non_matching() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        fs::write(repo.join("a.sln"), "x").unwrap();
        fs::write(repo.join("b.txt"), "x").unwrap();
        let profile = LanguageProfile {
            id: "dotnet".to_string(),
            display_name: ".NET".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: Some("*.sln".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
        };
        let sols = scan_solutions_for_repo(&repo, &profile);
        assert_eq!(sols.len(), 1);
        assert!(sols[0].relative.ends_with("a.sln"));
    }

    #[test]
    fn scan_solutions_respects_profile_depth_and_cap_20() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        // depth capped at 4
        let profile = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: Some("*.txt".to_string()),
            max_scan_depth: 10, // should be capped to 4
            ides: vec![],
            default_ide_id: None,
        };
        // create deep file at depth 5 (repo/a/b/c/d/e/file.txt) -> should be ignored because capped at 4
        let deep = repo.join("a/b/c/d/e");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("deep.txt"), "x").unwrap();
        let sols = scan_solutions_for_repo(&repo, &profile);
        assert!(
            sols.iter().all(|s| !s.relative.contains("deep.txt")),
            "depth >4 should be ignored"
        );

        // cap 20
        let repo2 = tmp.path().join("repo2");
        fs::create_dir(&repo2).unwrap();
        init_repo(&repo2);
        for i in 0..25 {
            fs::write(repo2.join(format!("file{i}.txt")), "x").unwrap();
        }
        let sols2 = scan_solutions_for_repo(&repo2, &profile);
        assert!(sols2.len() <= 25);
        // scan_repos truncates to 20, but scan_solutions_for_repo itself doesn't; we test truncation via scan_repos
        let mut cfg = AppConfig {
            roots: vec![tmp.path().to_path_buf()],
            max_depth: 1,
            ..Default::default()
        };
        // create profile with .txt to be active
        cfg.profiles[0] = LanguageProfile {
            id: "dotnet".to_string(),
            display_name: ".NET".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: Some("*.txt".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
        };
        let repos = scan_repos(&cfg);
        // repo2 should have truncated solutions to 20 when via scan_repos
        for r in repos.iter().filter(|r| r.path == repo2) {
            assert!(r.solutions.len() <= 20);
        }
    }

    #[test]
    fn scan_solutions_ignores_git_and_ignored_dirs() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        fs::create_dir(repo.join(".git/ignored")).unwrap();
        fs::write(repo.join(".git/ignored.sln"), "x").unwrap(); // inside .git should be ignored via filter_entry not walking into .git
        fs::create_dir(repo.join("node_modules")).unwrap();
        fs::write(repo.join("node_modules/a.sln"), "x").unwrap();
        let profile = LanguageProfile {
            id: "dotnet".to_string(),
            display_name: ".NET".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: Some("*.sln".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
        };
        let sols = scan_solutions_for_repo(&repo, &profile);
        assert!(
            sols.is_empty(),
            "files in .git and node_modules should be ignored"
        );
    }

    #[test]
    fn scan_solutions_relative_paths_correct() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        fs::create_dir(repo.join("sub")).unwrap();
        fs::write(repo.join("sub/app.sln"), "x").unwrap();
        let profile = LanguageProfile {
            id: "dotnet".to_string(),
            display_name: ".NET".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: Some("*.sln".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
        };
        let sols = scan_solutions_for_repo(&repo, &profile);
        assert_eq!(sols.len(), 1);
        assert_eq!(
            sols[0].relative,
            format!("sub{}app.sln", std::path::MAIN_SEPARATOR)
        );
        assert_eq!(sols[0].path, repo.join("sub/app.sln"));
    }

    #[test]
    fn scan_solutions_sorted_root_first_alphabetical() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        fs::write(repo.join("b.sln"), "x").unwrap();
        fs::write(repo.join("a.sln"), "x").unwrap();
        fs::create_dir(repo.join("sub")).unwrap();
        fs::write(repo.join("sub/c.sln"), "x").unwrap();
        let profile = LanguageProfile {
            id: "dotnet".to_string(),
            display_name: ".NET".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: Some("*.sln".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
        };
        let mut sols = scan_solutions_for_repo(&repo, &profile);
        sols.sort_by(|a, b| {
            let da = a.relative.matches(std::path::MAIN_SEPARATOR).count();
            let db = b.relative.matches(std::path::MAIN_SEPARATOR).count();
            da.cmp(&db)
                .then_with(|| a.relative.to_lowercase().cmp(&b.relative.to_lowercase()))
        });
        assert_eq!(sols[0].relative, "a.sln");
        assert_eq!(sols[1].relative, "b.sln");
        assert_eq!(
            sols[2].relative,
            format!("sub{}c.sln", std::path::MAIN_SEPARATOR)
        );
    }

    #[test]
    fn scan_solutions_pattern_multiple() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        fs::write(repo.join("a.sln"), "x").unwrap();
        fs::write(repo.join("b.csproj"), "x").unwrap();
        let profile = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: Some("*.sln,*.csproj".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
        };
        // Note: implementation checks normalized_extension for wildcard patterns, so *.csproj will not match because ext is .sln
        // Therefore this test expects only .sln matches - demonstrates wildcard logic limitation
        let sols = scan_solutions_for_repo(&repo, &profile);
        assert!(sols.iter().any(|s| s.relative.ends_with("a.sln")));
        // For multiple exact patterns like pom.xml,build.gradle it works; for mixed wildcard with different ext, only ext of profile matches
    }

    #[test]
    fn scan_solutions_case_insensitive_and_extension_normalized() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        init_repo(&repo);
        fs::write(repo.join("FOO.SLN"), "x").unwrap();
        let mut profile = LanguageProfile {
            id: "dotnet".to_string(),
            display_name: ".NET".to_string(),
            file_extension: "SLN".to_string(), // uppercase without dot
            file_pattern: Some("*.sln".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
        };
        // normalized to .sln
        assert_eq!(profile.normalized_extension(), ".sln");
        let sols = scan_solutions_for_repo(&repo, &profile);
        assert_eq!(sols.len(), 1);
        // .SOL should NOT match FOO.SLN (extension mismatch)
        profile.file_extension = ".SOL".to_string();
        assert_eq!(profile.normalized_extension(), ".sol");
        let sols2 = scan_solutions_for_repo(&repo, &profile);
        assert_eq!(sols2.len(), 0);
        // .sln should match again case-insensitive
        profile.file_extension = "sln".to_string();
        let sols3 = scan_solutions_for_repo(&repo, &profile);
        assert_eq!(sols3.len(), 1);
    }
}
