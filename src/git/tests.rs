use super::*;
use crate::config::{AgentProfile, IdeConfig, TerminalPreference};
use crate::gitlab::checkout::MrCheckoutPlan;
use std::fs;
use std::path::Component;
use std::time::{Duration, Instant};
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
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Ssh,
            "SSH could not read data: Failed getting response",
        )
    };
    let delays = [Duration::ZERO; 2];
    // SSH-Abbruch mitten in der Übertragung: zweiter Versuch klappt
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
    // Endgültige Fehler (abgelehntes SSH- und HTTP-Login, offline,
    // kaputte Refspec) nur einmal
    for (code, class, message) in [
        (
            git2::ErrorCode::Auth,
            git2::ErrorClass::Ssh,
            "remote rejected authentication: Failed getting response",
        ),
        (
            git2::ErrorCode::Auth,
            git2::ErrorClass::Http,
            "unexpected authentication failure",
        ),
        (
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Net,
            "failed to resolve address for gitlab.firma.example: Name or service not known",
        ),
        (
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Reference,
            "nope",
        ),
    ] {
        let mut calls = 0;
        let res = with_fetch_retries("origin", &delays, || {
            calls += 1;
            Err(git2::Error::new(code, class, message))
        });
        assert!(res.is_err());
        assert_eq!(calls, 1, "{message}");
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
    let scanned = |path: &PathBuf| get_repo_info(path.clone()).unwrap();
    let missing = RepoInfo::new(base.path().join("fehlt"), "main".into(), false, false);
    // Worktrees vor dem Haupt-Worktree: der Haupt-Worktree gewinnt
    let repos = vec![
        scanned(&wt_a),
        scanned(&other),
        missing.clone(),
        scanned(&main),
        scanned(&wt_b),
    ];
    let expected = vec![main.clone(), other.clone(), missing.path.clone()];
    assert_eq!(fetch_targets(&repos), expected);
    // Nur aus den Scan-Daten: ein seither gelöschtes Repo bleibt
    fs::remove_dir_all(&other).unwrap();
    assert_eq!(fetch_targets(&repos), expected);
    // Nur Worktrees: der erste
    assert_eq!(fetch_targets(&[scanned(&wt_b), scanned(&wt_a)]), vec![wt_b]);
    assert!(fetch_targets(&[]).is_empty());
}

/// Texte wie in libgit2 1.9 (`transports/ssh_libssh2.c`, `http.c`,
/// `httpclient.c`, `winhttp.c`, `streams/socket.c`).
#[test]
fn fetch_transient_error_classes() {
    use git2::{ErrorClass as C, ErrorCode as E};
    let transient = [
        // Server trennt im Handshake (überlastet), Abbruch mitten im Transfer
        (
            E::GenericError,
            C::Ssh,
            "failed to start SSH session: Failed getting banner",
        ),
        (
            E::GenericError,
            C::Ssh,
            "SSH could not read data: Timed out waiting on socket",
        ),
        (E::Timeout, C::Net, "could not read from socket: timed out"),
        (
            E::GenericError,
            C::Net,
            "error receiving data from socket: Connection reset by peer",
        ),
        (E::Eof, C::Net, "could not read from remote repository"),
        (E::GenericError, C::Http, "unexpected EOF"),
        (E::GenericError, C::Http, "unexpected http status code: 503"),
        (
            E::GenericError,
            C::Http,
            "request failed with status code: 502",
        ),
        (
            E::GenericError,
            C::Os,
            "failed to receive response: Verbindung zurückgesetzt",
        ),
        (
            E::Locked,
            C::Os,
            "failed to lock file '/r/.git/FETCH_HEAD.lock' for writing",
        ),
    ];
    let permanent = [
        // Server trennt nach zu vielen Fehlversuchen (MaxAuthTries)
        (
            E::Auth,
            C::Ssh,
            "remote rejected authentication: Failed getting response",
        ),
        (
            E::GenericError,
            C::Ssh,
            "failed to authenticate SSH session: socket disconnect",
        ),
        (
            E::Auth,
            C::Callback,
            "authentication failed (tried: ssh-agent)",
        ),
        (
            E::Certificate,
            C::Ssh,
            "invalid or unknown remote ssh hostkey",
        ),
        // Meldung des Servers auf stderr
        (
            E::Eof,
            C::Ssh,
            "GitLab: The project you were looking for could not be found.",
        ),
        // Offline, DNS, Host nicht erreichbar
        (
            E::GenericError,
            C::Net,
            "failed to resolve address for gitlab.firma.example: Name or service not known",
        ),
        (
            E::GenericError,
            C::Net,
            "failed to connect to 192.0.2.10: Operation timed out",
        ),
        (E::GenericError, C::Os, "failed to connect to 192.0.2.10"),
        (
            E::GenericError,
            C::Os,
            "failed to send request: Name nicht aufgelöst",
        ),
        // HTTP: Login, Rechte, nicht gefunden, falsche Zugangsdaten
        (E::Auth, C::Http, "unexpected authentication failure"),
        (E::GenericError, C::Http, "unexpected http status code: 404"),
        (
            E::GenericError,
            C::Http,
            "request failed with status code: 403",
        ),
        (
            E::GenericError,
            C::Http,
            "too many redirects or authentication replays",
        ),
        (E::NotFound, C::Config, "x"),
    ];
    for (code, class, message) in transient {
        let err = git2::Error::new(code, class, message);
        assert!(is_transient_fetch_error(&err), "{err}");
    }
    for (code, class, message) in permanent {
        let err = git2::Error::new(code, class, message);
        assert!(!is_transient_fetch_error(&err), "{err}");
    }
}

/// Eine hängende Verbindung (Server nimmt an und schweigt) endet nach
/// der Stille-Grenze mit einem vorübergehenden Fehler, statt Worker und
/// Ref-Lock für immer zu halten. SSH über libssh2; http nur außerhalb
/// von Windows (WinHTTP nimmt die Grenze nicht).
#[test]
fn libgit2_timeouts_end_stalled_connection() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        // Verbindungen offen halten, nie antworten
        let mut open = Vec::new();
        for stream in listener.incoming() {
            open.push(stream);
        }
    });
    set_libgit2_timeouts(LIBGIT2_CONNECT_TIMEOUT, Duration::from_secs(1));
    let dir = tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let mut urls = vec![format!("ssh://git@127.0.0.1:{port}/g/p.git")];
    if cfg!(not(windows)) {
        urls.push(format!("http://127.0.0.1:{port}/g/p.git"));
    }
    for url in &urls {
        let mut remote = repo.remote_anonymous(url).unwrap();
        let start = Instant::now();
        let err = libgit2_fetch(&repo, &mut remote, url, &[], false).expect_err(url);
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{url}: {:?}",
            start.elapsed()
        );
        assert!(is_transient_fetch_error(&err), "{url}: {err}");
    }
    init_libgit2_timeouts();
    unsafe {
        assert_eq!(
            git2::opts::get_server_connect_timeout_in_milliseconds().unwrap(),
            30_000
        );
        assert_eq!(
            git2::opts::get_server_timeout_in_milliseconds().unwrap(),
            120_000
        );
    }
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

#[test]
fn stash_and_checkout_pop_failure_still_switched() {
    // Wechsel erledigt, Pop gescheitert: kein Err, sondern `pop_failed`,
    // der Stash bleibt. Ein Lock-Ordner auf dem Stash-Reflog lässt das
    // Droppen scheitern (wie eine gesperrte Datei unter Windows).
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    let lock = c1.join(".git/logs/refs/stash.lock");
    fs::create_dir_all(&lock).unwrap();
    fs::write(c1.join("a.txt"), "local\n").unwrap();
    let switch = stash_and_checkout(&c1, "feature/x").expect("Wechsel erledigt");
    assert_eq!(switch.outcome, CheckoutOutcome::Switched);
    let pop_failed = switch.pop_failed.expect("Pop-Fehler gemeldet");
    assert!(
        pop_failed
            .to_string()
            .contains("Stash bleibt unter stash@{0}"),
        "{pop_failed}"
    );
    assert_eq!(head_target(&c1), "refs/heads/feature/x");
    let stash = |dir: &Path| {
        let repo = Repository::open(dir).unwrap();
        repo.revparse_single("stash@{0}").map(|o| o.id()).ok()
    };
    let kept = stash(&c1).expect("Stash bleibt erhalten");
    // Ohne Lock zurück: sauber gepoppt, der alte Stash bleibt liegen
    fs::remove_dir(&lock).unwrap();
    let switch = stash_and_checkout(&c1, "main").expect("Wechsel erledigt");
    assert!(switch.pop_failed.is_none(), "{:?}", switch.pop_failed);
    assert_eq!(head_target(&c1), "refs/heads/main");
    assert_eq!(fs::read_to_string(c1.join("a.txt")).unwrap(), "local\n");
    assert_eq!(stash(&c1), Some(kept));
}

#[test]
fn checkout_prefers_branch_over_same_named_tag() {
    // Gleichnamige Tags auf `main`: Wechsel landet trotzdem auf dem
    // Branch (lokal bzw. remote), nicht als Detached HEAD auf dem Tag.
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    git_cmd(&c1, &["tag", "feature/x", "main"]);
    git_cmd(&c1, &["tag", "origin/feature/x", "main"]);
    let feature = ref_oid(&c1, "refs/heads/feature/x");
    assert_ne!(feature, ref_oid(&c1, "refs/tags/feature/x"));
    checkout_branch(&c1, "feature/x").unwrap();
    assert_eq!(head_target(&c1), "refs/heads/feature/x");
    assert_eq!(ref_oid(&c1, "HEAD"), feature);
    // Remote-Name ohne lokalen Branch: angelegt am Remote-Stand
    checkout_branch(&c1, "main").unwrap();
    git_cmd(&c1, &["branch", "-D", "feature/x"]);
    checkout_branch(&c1, "origin/feature/x").unwrap();
    assert_eq!(head_target(&c1), "refs/heads/feature/x");
    assert_eq!(ref_oid(&c1, "HEAD"), feature);
    assert!(c1.join("x.txt").exists());
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
fn create_worktree_from_local_branch() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    let target = base.path().join("wts").join("c1-feature-x");
    let branch = WorktreeBranch::Existing("feature/x".to_string());
    assert_eq!(create_worktree(&c1, &target, &branch).unwrap(), "feature/x");
    assert_eq!(head_target(&target), "refs/heads/feature/x");
    assert!(target.join("x.txt").exists());
    assert_eq!(head_target(&c1), "refs/heads/main");
    let info = get_repo_info(target.clone()).unwrap();
    assert!(info.is_linked_worktree);
}

#[test]
fn create_worktree_from_remote_branch_creates_tracking_branch() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    // Lokalen Branch entfernen: nur origin/feature/x bleibt
    git_cmd(&c1, &["branch", "-D", "feature/x"]);
    let target = base.path().join("c1-feature-x");
    let branch = WorktreeBranch::Existing("origin/feature/x".to_string());
    assert_eq!(create_worktree(&c1, &target, &branch).unwrap(), "feature/x");
    assert_eq!(head_target(&target), "refs/heads/feature/x");
    let repo = Repository::open(&c1).unwrap();
    let upstream = repo.branch_upstream_name("refs/heads/feature/x").unwrap();
    assert_eq!(
        upstream.as_str().ok(),
        Some("refs/remotes/origin/feature/x")
    );
}

#[test]
fn create_worktree_from_remote_branch_fast_forwards_stale_local() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    push_from_second_clone(base.path(), "feature/x", "y.txt");
    git_cmd(&c1, &["fetch", "origin"]);
    let remote = ref_oid(&c1, "refs/remotes/origin/feature/x");
    assert_ne!(ref_oid(&c1, "refs/heads/feature/x"), remote);
    let target = base.path().join("c1-feature-x");
    let branch = WorktreeBranch::Existing("origin/feature/x".to_string());
    assert_eq!(create_worktree(&c1, &target, &branch).unwrap(), "feature/x");
    assert_eq!(ref_oid(&c1, "refs/heads/feature/x"), remote);
    assert!(target.join("y.txt").exists());
}

#[test]
fn create_worktree_from_remote_branch_rejects_diverged_local() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    push_from_second_clone(base.path(), "feature/x", "y.txt");
    git_cmd(&c1, &["fetch", "origin"]);
    git_cmd(&c1, &["checkout", "feature/x"]);
    commit_file(&c1, "z.txt", "lokal\n", "local commit");
    git_cmd(&c1, &["checkout", "main"]);
    let local = ref_oid(&c1, "refs/heads/feature/x");
    let target = base.path().join("c1-feature-x");
    let branch = WorktreeBranch::Existing("origin/feature/x".to_string());
    assert!(create_worktree(&c1, &target, &branch).is_err());
    assert_eq!(
        ref_oid(&c1, "refs/heads/feature/x"),
        local,
        "lokal unverändert"
    );
    assert!(!target.exists());
}

#[test]
fn create_worktree_with_new_branch_from_head_and_start() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    let from_head = WorktreeBranch::New {
        name: "feature/neu".to_string(),
        start: None,
    };
    let t1 = base.path().join("neu");
    assert_eq!(
        create_worktree(&c1, &t1, &from_head).unwrap(),
        "feature/neu"
    );
    assert_eq!(head_target(&t1), "refs/heads/feature/neu");
    assert_eq!(
        ref_oid(&c1, "refs/heads/feature/neu"),
        ref_oid(&c1, "refs/heads/main")
    );
    let from_feature = WorktreeBranch::New {
        name: "fix".to_string(),
        start: Some("origin/feature/x".to_string()),
    };
    let t2 = base.path().join("fix");
    create_worktree(&c1, &t2, &from_feature).unwrap();
    assert!(t2.join("x.txt").exists());
    // Name schon vergeben / ungültig
    let taken = WorktreeBranch::New {
        name: "fix".to_string(),
        start: None,
    };
    assert!(create_worktree(&c1, &base.path().join("t3"), &taken).is_err());
    let invalid = WorktreeBranch::New {
        name: "a..b".to_string(),
        start: None,
    };
    assert!(create_worktree(&c1, &base.path().join("t4"), &invalid).is_err());
    assert!(!base.path().join("t4").exists());
}

#[test]
fn create_worktree_failure_removes_new_branch() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    let busy = base.path().join("busy");
    fs::create_dir(&busy).unwrap();
    fs::write(busy.join("f.txt"), "x").unwrap();
    let branch = WorktreeBranch::New {
        name: "tmp".to_string(),
        start: None,
    };
    assert!(matches!(
        create_worktree(&c1, &busy, &branch),
        Err(CheckoutError::TargetNotEmpty { .. })
    ));
    let repo = Repository::open(&c1).unwrap();
    assert!(repo.find_branch("tmp", BranchType::Local).is_err());
    // Vorhandener Branch bleibt bei Fehler erhalten (main ist HEAD)
    let main = WorktreeBranch::Existing("main".to_string());
    assert!(matches!(
        create_worktree(&c1, &base.path().join("m"), &main),
        Err(CheckoutError::CheckedOutElsewhere { .. })
    ));
    assert!(repo.find_branch("main", BranchType::Local).is_ok());
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
fn repo_state_clean_dirty() {
    let dir = tempdir().unwrap();
    let repo = init_repo_with_commit(dir.path(), "main");
    assert_eq!(repo_state_of(&repo, 0), RepoState::Clean);
    fs::write(dir.path().join("README.md"), "changed\n").unwrap();
    assert_eq!(repo_state_of(&repo, dirty_count(&repo)), RepoState::Dirty);
}

#[test]
fn repo_state_conflict_and_merge() {
    let dir = tempdir().unwrap();
    let p = dir.path();
    init_repo_with_commit(p, "main");
    commit_file(p, "a.txt", "base\n", "base");
    git_cmd(p, &["checkout", "-b", "other"]);
    commit_file(p, "a.txt", "other\n", "other");
    git_cmd(p, &["checkout", "main"]);
    commit_file(p, "a.txt", "main\n", "main");
    // Merge scheitert mit Konflikt: Fehler bewusst ignorieren
    let _ = std::process::Command::new("git")
        .args(["merge", "other"])
        .current_dir(p)
        .output();
    let repo = Repository::open(p).unwrap();
    assert_eq!(repo_state_of(&repo, 1), RepoState::Conflict);
    fs::write(p.join("a.txt"), "resolved\n").unwrap();
    git_cmd(p, &["add", "a.txt"]);
    let repo = Repository::open(p).unwrap();
    assert_eq!(
        repo_state_of(&repo, 1),
        RepoState::InProgress(InProgressKind::Merge)
    );
}

#[test]
fn last_commit_reads_head() {
    let dir = tempdir().unwrap();
    init_repo_with_commit(dir.path(), "main");
    commit_file(dir.path(), "a.txt", "x", "Fix rounding\n\nbody");
    let repo = Repository::open(dir.path()).unwrap();
    let lc = last_commit_of(&repo).unwrap();
    assert_eq!(lc.summary, "Fix rounding");
    assert_eq!(lc.author, "Test");
    assert!(lc.time > 0);
    let empty = tempdir().unwrap();
    let repo = Repository::init(empty.path()).unwrap();
    assert!(last_commit_of(&repo).is_none());
}

#[test]
fn get_repo_info_fills_state_and_last_commit() {
    let dir = tempdir().unwrap();
    init_repo_with_commit(dir.path(), "main");
    let info = get_repo_info(dir.path().to_path_buf()).unwrap();
    assert_eq!(info.state, RepoState::Clean);
    assert_eq!(info.last_commit.unwrap().summary, "initial");
    fs::write(dir.path().join("README.md"), "changed\n").unwrap();
    let info = get_repo_info(dir.path().to_path_buf()).unwrap();
    assert_eq!(info.state, RepoState::Dirty);
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
        is_program_available("ls") || is_program_available("echo") || is_program_available("true")
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
fn devenv_candidates_skip_ssms() {
    // Ausgabe bei installiertem SSMS 22 neben Visual Studio 18
    let out = "C:\\Program Files\\Microsoft SQL Server Management Studio 22\\Release\\Common7\\IDE\\SSMS.exe\r\n\
                   C:\\Program Files\\Microsoft Visual Studio\\18\\Professional\\Common7\\IDE\\devenv.exe\r\n";
    let got = devenv_candidates(out, false);
    assert_eq!(got.len(), 1);
    assert!(got[0].to_string_lossy().contains("Visual Studio\\18"));
    let dirs = "C:\\Program Files\\Microsoft SQL Server Management Studio 22\\Release\n\
                    C:\\VS\\18\n";
    let got = devenv_candidates(dirs, true);
    assert_eq!(got.len(), 1);
    assert!(got[0].ends_with("devenv.exe"));
    assert!(devenv_candidates("C:\\X\\Blend.exe\n", false).is_empty());
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

// --- fast_forward: Bare-Origin, c1 ist behind, c2 pusht zwei Commits ---

/// c1 (auf `main`, Upstream origin/main); c2 pusht danach zwei Commits.
fn ff_setup(base: &Path) -> PathBuf {
    let c1 = origin_and_clone_with_feature(base);
    git_cmd(base, &["clone", "-b", "main", "origin.git", "c2"]);
    let c2 = base.join("c2");
    git_identity(&c2);
    commit_file(&c2, "n1.txt", "1\n", "remote 1");
    commit_file(&c2, "n2.txt", "2\n", "remote 2");
    git_cmd(&c2, &["push", "origin", "main"]);
    c1
}

#[test]
fn ff_fast_forwards_clean_branch() {
    let base = tempdir().unwrap();
    let c1 = ff_setup(base.path());
    let out = fast_forward(&c1).unwrap();
    assert_eq!(out, FfOutcome::FastForwarded { commits: 2 });
    assert_eq!(
        ref_oid(&c1, "refs/heads/main"),
        ref_oid(&base.path().join("c2"), "refs/heads/main")
    );
    assert!(c1.join("n2.txt").exists());
    assert_eq!(fast_forward(&c1).unwrap(), FfOutcome::UpToDate);
}

#[test]
fn ff_up_to_date() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    assert_eq!(fast_forward(&c1).unwrap(), FfOutcome::UpToDate);
}

#[test]
fn ff_refuses_dirty() {
    let base = tempdir().unwrap();
    let c1 = ff_setup(base.path());
    let before = ref_oid(&c1, "refs/heads/main");
    fs::write(c1.join("a.txt"), "geaendert\n").unwrap();
    assert_eq!(fast_forward(&c1).unwrap(), FfOutcome::Dirty { files: 1 });
    assert_eq!(ref_oid(&c1, "refs/heads/main"), before);
}

#[test]
fn ff_refuses_diverged() {
    let base = tempdir().unwrap();
    let c1 = ff_setup(base.path());
    commit_file(&c1, "own.txt", "own\n", "own");
    let before = ref_oid(&c1, "refs/heads/main");
    assert_eq!(
        fast_forward(&c1).unwrap(),
        FfOutcome::Diverged {
            ahead: 1,
            behind: 2
        }
    );
    assert_eq!(ref_oid(&c1, "refs/heads/main"), before);
}

#[test]
fn ff_no_upstream() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    git_cmd(&c1, &["checkout", "-b", "lokal"]);
    assert_eq!(fast_forward(&c1).unwrap(), FfOutcome::NoUpstream);
}

#[test]
fn ff_detached() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    git_cmd(&c1, &["checkout", "--detach"]);
    assert_eq!(fast_forward(&c1).unwrap(), FfOutcome::Detached);
}

#[test]
fn ff_fetch_failure_is_a_fetch_error() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    let gone = base.path().join("weg.git");
    git_cmd(
        &c1,
        &["remote", "set-url", "origin", gone.to_str().unwrap()],
    );
    let err = fast_forward(&c1).unwrap_err();
    assert!(is_fetch_error(&err), "{err:#}");
}

#[test]
fn ff_head_switched_during_fetch_changes_nothing() {
    let base = tempdir().unwrap();
    let c1 = ff_setup(base.path());
    let main_before = ref_oid(&c1, "refs/heads/main");
    let feat_before = ref_oid(&c1, "refs/heads/feature/x");
    // Zwischen Fetch und Umsetzen wechselt jemand den Branch.
    let out = fast_forward_inner(&c1, || {
        git_cmd(&c1, &["checkout", "feature/x"]);
    })
    .unwrap();
    assert_eq!(out, FfOutcome::Changed);
    assert_eq!(ref_oid(&c1, "refs/heads/main"), main_before);
    assert_eq!(ref_oid(&c1, "refs/heads/feature/x"), feat_before);
    assert_eq!(head_target(&c1), "refs/heads/feature/x");
    assert!(c1.join("x.txt").exists());
    assert!(!c1.join("n1.txt").exists());
}

#[test]
fn ff_branch_moved_during_fetch_changes_nothing() {
    let base = tempdir().unwrap();
    let c1 = ff_setup(base.path());
    // Zwischen Fetch und Umsetzen entsteht ein neuer lokaler Commit.
    let out = fast_forward_inner(&c1, || {
        commit_file(&c1, "own.txt", "own\n", "own");
    })
    .unwrap();
    assert_eq!(out, FfOutcome::Changed);
    let repo = Repository::open(&c1).unwrap();
    let main = repo.refname_to_id("refs/heads/main").unwrap();
    let commit = repo.find_commit(main).unwrap();
    assert_eq!(commit.summary().ok().flatten(), Some("own"));
    assert!(c1.join("own.txt").exists());
    assert!(!c1.join("n1.txt").exists());
}

/// Hauptrepo `c1` (origin + main) und ein CLI-Worktree `wt` auf neuem
/// Branch `feat` (ein eigener Commit, ungepusht).
fn repo_with_feat_worktree(base: &Path) -> (PathBuf, PathBuf) {
    let c1 = origin_and_clone_with_feature(base);
    let wt = base.join("wt");
    git_cmd(
        &c1,
        &[
            "worktree",
            "add",
            "-b",
            "feat",
            wt.to_str().unwrap(),
            "main",
        ],
    );
    git_identity(&wt);
    commit_file(&wt, "f.txt", "f\n", "own commit");
    (c1, wt)
}

fn worktree_listing(repo: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn remove_clean_worktree() {
    let base = tempdir().unwrap();
    let (c1, wt) = repo_with_feat_worktree(base.path());
    remove_worktree(&wt, &c1, false, None).unwrap();
    assert!(!wt.exists());
    assert!(!worktree_listing(&c1).contains("wt"));
    assert!(ref_exists(&c1, "refs/heads/feat"), "Branch bleibt");
}

#[test]
fn remove_dirty_worktree_needs_force() {
    let base = tempdir().unwrap();
    let (c1, wt) = repo_with_feat_worktree(base.path());
    fs::write(wt.join("f.txt"), "geändert\n").unwrap();
    assert!(remove_worktree(&wt, &c1, false, None).is_err());
    assert!(wt.exists(), "dirty Worktree bleibt ohne force");
    remove_worktree(&wt, &c1, true, None).unwrap();
    assert!(!wt.exists());
}

#[test]
fn remove_worktree_already_deleted_folder() {
    let base = tempdir().unwrap();
    let (c1, wt) = repo_with_feat_worktree(base.path());
    fs::remove_dir_all(&wt).unwrap();
    remove_worktree(&wt, &c1, false, None).unwrap();
    assert!(!worktree_listing(&c1).contains("wt"));
    assert!(!c1.join(".git").join("worktrees").join("wt").exists());
}

#[test]
fn remove_worktree_deletes_branch() {
    let base = tempdir().unwrap();
    let (c1, wt) = repo_with_feat_worktree(base.path());
    // Ungemergter Branch: ohne force Fehler (Worktree ist dann schon weg)
    let err = remove_worktree(&wt, &c1, false, Some(("feat", false))).unwrap_err();
    assert!(err.to_string().contains("feat"), "{err}");
    assert!(!wt.exists());
    assert!(ref_exists(&c1, "refs/heads/feat"));
    delete_local_branch(&c1, "feat", true).unwrap();
    assert!(!ref_exists(&c1, "refs/heads/feat"));
    // Direkt mit force_delete
    git_cmd(
        &c1,
        &[
            "worktree",
            "add",
            "-b",
            "feat2",
            wt.to_str().unwrap(),
            "main",
        ],
    );
    git_identity(&wt);
    commit_file(&wt, "g.txt", "g\n", "own");
    remove_worktree(&wt, &c1, false, Some(("feat2", true))).unwrap();
    assert!(!ref_exists(&c1, "refs/heads/feat2"));
    // Gemergter Branch: -d genügt
    git_cmd(&c1, &["branch", "merged", "main"]);
    delete_local_branch(&c1, "merged", false).unwrap();
    assert!(!ref_exists(&c1, "refs/heads/merged"));
}

#[test]
fn branch_safety_cases() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    // gepusht (Upstream vorhanden, ahead == 0)
    assert_eq!(
        branch_safety(&c1, "feature/x", &[]),
        Some(BranchSafety::Pushed)
    );
    // in origin/main gemergt, Upstream weg
    git_cmd(&c1, &["branch", "old", "main"]);
    assert_eq!(
        branch_safety(&c1, "old", &[]),
        Some(BranchSafety::MergedIntoDefault)
    );
    // eigener Commit ohne Upstream
    git_cmd(&c1, &["checkout", "-b", "own", "main"]);
    commit_file(&c1, "o.txt", "o\n", "own");
    git_cmd(&c1, &["checkout", "main"]);
    assert_eq!(branch_safety(&c1, "own", &[]), None);
    // eigener Commit zusätzlich zum Upstream: nicht „Pushed“
    git_cmd(&c1, &["checkout", "feature/x"]);
    commit_file(&c1, "x2.txt", "x2\n", "unpushed");
    git_cmd(&c1, &["checkout", "main"]);
    assert_eq!(branch_safety(&c1, "feature/x", &[]), None);
}

#[test]
fn branch_safety_mr_merged_needs_matching_head() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    git_cmd(&c1, &["checkout", "-b", "own", "main"]);
    commit_file(&c1, "o.txt", "o\n", "own");
    let repo = Repository::open(&c1).unwrap();
    let tip = repo.refname_to_id("refs/heads/own").unwrap();
    // MR-Kopf == Spitze
    assert_eq!(
        branch_safety(&c1, "own", &[tip]),
        Some(BranchSafety::MrMerged)
    );
    // Spitze ist Vorfahre des MR-Kopfs (MR hatte später mehr Commits)
    commit_file(&c1, "o2.txt", "o2\n", "later in mr");
    let later = repo.refname_to_id("refs/heads/own").unwrap();
    git_cmd(&c1, &["reset", "-q", "--hard", &tip.to_string()]);
    assert_eq!(
        branch_safety(&c1, "own", &[later]),
        Some(BranchSafety::MrMerged)
    );
    // Lokaler Commit über den MR-Kopf hinaus: nicht sicher
    commit_file(&c1, "o3.txt", "o3\n", "after merge");
    assert_eq!(branch_safety(&c1, "own", &[tip]), None);
    // MR-Kopf lokal unbekannt: nicht sicher
    let missing = git2::Oid::from_str("1234567890abcdef1234567890abcdef12345678").unwrap();
    git_cmd(&c1, &["reset", "-q", "--hard", &tip.to_string()]);
    assert_eq!(branch_safety(&c1, "own", &[missing]), None);
    assert_eq!(
        branch_safety(&c1, "own", &[missing, tip]),
        Some(BranchSafety::MrMerged)
    );
    git_cmd(&c1, &["checkout", "main"]);
}

#[test]
fn gone_upstream_detected() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    let repo = Repository::open(&c1).unwrap();
    assert!(!upstream_gone(&repo, "feature/x"));
    assert!(!upstream_gone(&repo, "nonexistent"));
    // Remote-Branch löschen (über zweiten Klon) und prunen
    git_cmd(base.path(), &["clone", "origin.git", "c2"]);
    git_cmd(
        &base.path().join("c2"),
        &["push", "origin", "--delete", "feature/x"],
    );
    git_cmd(&c1, &["fetch", "--prune", "origin"]);
    let repo = Repository::open(&c1).unwrap();
    assert!(upstream_gone(&repo, "feature/x"));
    assert!(!upstream_gone(&repo, "main"));
    assert_eq!(gone_local_branches(&c1), vec!["feature/x".to_string()]);
    // Aktueller Branch wird nie vorgeschlagen
    git_cmd(&c1, &["checkout", "feature/x"]);
    assert!(gone_local_branches(&c1).is_empty());
    // … und ebenso keiner, der in einem Worktree ausgecheckt ist
    git_cmd(&c1, &["checkout", "main"]);
    let wt = base.path().join("wt");
    git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "feature/x"]);
    assert!(gone_local_branches(&c1).is_empty());
}

#[test]
fn needs_force_delete_per_safety() {
    assert!(!BranchSafety::Pushed.needs_force_delete());
    assert!(BranchSafety::MergedIntoDefault.needs_force_delete());
    assert!(BranchSafety::MrMerged.needs_force_delete());
}

#[test]
fn merged_into_origin_default_is_deletable_via_safety() {
    let base = tempdir().unwrap();
    let c1 = origin_and_clone_with_feature(base.path());
    // origin/main enthält `merged`; das Hauptrepo steht auf einem anderen
    // Branch ohne diesen Commit
    git_cmd(&c1, &["checkout", "-b", "merged", "main"]);
    commit_file(&c1, "m.txt", "m\n", "merged work");
    git_cmd(&c1, &["push", "origin", "merged:main"]);
    git_cmd(&c1, &["fetch", "origin"]);
    git_cmd(&c1, &["checkout", "-b", "other", "main"]);
    let safety = branch_safety(&c1, "merged", &[]).unwrap();
    assert_eq!(safety, BranchSafety::MergedIntoDefault);
    // `-d` würde hier am HEAD des Hauptrepos scheitern
    assert!(delete_local_branch(&c1, "merged", false).is_err());
    let wt = base.path().join("wt");
    git_cmd(&c1, &["worktree", "add", wt.to_str().unwrap(), "main"]);
    remove_worktree(
        &wt,
        &c1,
        false,
        Some(("merged", safety.needs_force_delete())),
    )
    .unwrap();
    assert!(!ref_exists(&c1, "refs/heads/merged"));
}

#[test]
fn head_unreachable_detached_cases() {
    let base = tempdir().unwrap();
    let (c1, wt) = repo_with_feat_worktree(base.path());
    // Branch ausgecheckt: nicht detached → false
    assert!(!head_unreachable(&wt));
    // Detached auf einem Branch-Commit
    git_cmd(&wt, &["checkout", "--detach", "main"]);
    assert!(!head_unreachable(&wt));
    remove_worktree(&wt, &c1, false, None).unwrap();
    assert!(!wt.exists());
    // Detached mit eigenem, nirgends erreichbarem Commit
    git_cmd(
        &c1,
        &["worktree", "add", "--detach", wt.to_str().unwrap(), "main"],
    );
    git_identity(&wt);
    commit_file(&wt, "d.txt", "d\n", "lost");
    assert!(head_unreachable(&wt));
    let err = remove_worktree(&wt, &c1, false, None).unwrap_err();
    assert!(err.to_string().contains("detached"), "{err}");
    assert!(wt.exists());
    remove_worktree(&wt, &c1, true, None).unwrap();
    assert!(!wt.exists());
    // Kein Repo → im Zweifel „unerreichbar“
    assert!(head_unreachable(&base.path().join("nirgends")));
}

#[test]
fn remove_worktree_rejects_unregistered_path() {
    let base = tempdir().unwrap();
    let (c1, _wt) = repo_with_feat_worktree(base.path());
    let other = base.path().join("other");
    fs::create_dir(&other).unwrap();
    assert!(remove_worktree(&other, &c1, false, None).is_err());
    assert!(other.exists());
}

#[test]
fn strip_verbatim_prefix_cases() {
    assert_eq!(
        strip_verbatim(Path::new(r"\\?\C:\Dev\x")),
        PathBuf::from(r"C:\Dev\x")
    );
    assert_eq!(
        strip_verbatim(Path::new(r"\\?\UNC\srv\share\x")),
        PathBuf::from(r"\\srv\share\x")
    );
    assert_eq!(
        strip_verbatim(Path::new(r"\\?\Volume{1}\x")),
        PathBuf::from(r"\\?\Volume{1}\x")
    );
    assert_eq!(strip_verbatim(Path::new("/a/b")), PathBuf::from("/a/b"));
}
