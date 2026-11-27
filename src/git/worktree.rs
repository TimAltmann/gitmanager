//! Merge-Request-Checkout, Worktrees anlegen und Repos klonen.

use super::{
    apply_ref_update, branch_checked_out_elsewhere, canonical_key_path, credentials_callback,
    run_git_cli, RefUpdate, GIT_CLI_CLONE_TIMEOUT, GIT_CLI_WORKTREE_TIMEOUT,
};
use crate::gitlab::checkout::MrCheckoutPlan;
use git2::{BranchType, Repository};
use std::path::{Path, PathBuf};

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
/// die übrigen git-Fehler; die UI lokalisiert per Match bzw. Downcast.
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
    if let Some(path) = branch_checked_out_anywhere(&repo, local_branch) {
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

/// Ordner, in dem `branch` ausgecheckt ist – auch der Basis-Klon selbst,
/// den `branch_checked_out_elsewhere` als eigenen Worktree auslässt.
fn branch_checked_out_anywhere(repo: &Repository, branch: &str) -> Option<PathBuf> {
    let full = format!("refs/heads/{branch}");
    if repo.head().is_ok_and(|h| h.name().is_ok_and(|n| n == full)) {
        repo.workdir().map(|w| w.components().collect())
    } else {
        branch_checked_out_elsewhere(repo, branch)
    }
}

/// Vorhandenen lokalen Branch `local` für die Wahl des Remote-Branches
/// `remote_name` nutzen: liegt er zurück, auf den Remote-Stand vorspulen
/// (nur wenn nirgends ausgecheckt); enthält er den Remote-Stand schon,
/// bleibt er. Abgewichen → Fehler statt still den alten Stand auszuchecken.
fn sync_local_with_remote(
    repo: &Repository,
    local: &str,
    remote_name: &str,
    remote: &git2::Branch,
) -> Result<(), CheckoutError> {
    let err = |e: git2::Error| CheckoutError::Git(format!("Branch {local}: {e}"));
    let remote_oid = remote.get().peel_to_commit().map_err(err)?.id();
    let mut local_ref = repo
        .find_branch(local, BranchType::Local)
        .map_err(err)?
        .into_reference();
    let local_oid = local_ref.peel_to_commit().map_err(err)?.id();
    if local_oid == remote_oid
        || repo
            .graph_descendant_of(local_oid, remote_oid)
            .map_err(err)?
    {
        return Ok(());
    }
    if !repo
        .graph_descendant_of(remote_oid, local_oid)
        .map_err(err)?
    {
        return Err(CheckoutError::Git(format!(
            "Lokaler Branch {local} weicht von {remote_name} ab – lokalen Branch wählen oder zuerst abgleichen"
        )));
    }
    if let Some(path) = branch_checked_out_anywhere(repo, local) {
        return Err(CheckoutError::CheckedOutElsewhere {
            branch: local.to_string(),
            path,
        });
    }
    local_ref
        .set_target(
            remote_oid,
            &format!("worktree: fast-forward auf {remote_name}"),
        )
        .map_err(err)?;
    Ok(())
}

/// Branch-Quelle für „Neuer Worktree“ aus der Repo-Liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeBranch {
    /// Vorhandener Branch: lokal (`feature/x`) oder Remote-Branch
    /// (`origin/feature/x`, dann lokaler Branch `feature/x` mit Upstream)
    Existing(String),
    /// Neuer lokaler Branch `name` ab `start` (Branch oder Ref, `None` = HEAD
    /// des Basis-Repos)
    New { name: String, start: Option<String> },
}

/// Legt im Ordner `target_path` einen Worktree des Repos `base_path` an
/// (siehe [`add_worktree`]) und liefert den ausgecheckten lokalen Branch.
/// Ein Remote-Branch ohne lokalen Gegenpart wird als lokaler Branch mit
/// Upstream angelegt, ein neuer Branch am Startpunkt. Scheitert der
/// Worktree, wird ein hier angelegter Branch wieder entfernt.
pub fn create_worktree(
    base_path: &Path,
    target_path: &Path,
    branch: &WorktreeBranch,
) -> Result<String, CheckoutError> {
    let repo = Repository::open(base_path)
        .map_err(|e| CheckoutError::Git(format!("Repo öffnen fehlgeschlagen: {e}")))?;
    let has_local = |name: &str| repo.find_branch(name, BranchType::Local).is_ok();
    let (local, created) = match branch {
        WorktreeBranch::Existing(name) if has_local(name) => (name.clone(), false),
        WorktreeBranch::Existing(name) => {
            let remote_branch = repo
                .find_branch(name, BranchType::Remote)
                .map_err(|_| CheckoutError::Git(format!("Branch {name} nicht gefunden")))?;
            let local = remote_local_name(&repo, name)
                .ok_or_else(|| CheckoutError::Git(format!("Branch {name} nicht gefunden")))?;
            if has_local(&local) {
                sync_local_with_remote(&repo, &local, name, &remote_branch)?;
                (local, false)
            } else {
                let commit = remote_branch
                    .get()
                    .peel_to_commit()
                    .map_err(|e| CheckoutError::Git(format!("Branch {name}: {e}")))?;
                let mut created = repo.branch(&local, &commit, false).map_err(|e| {
                    CheckoutError::Git(format!("Branch {local} anlegen fehlgeschlagen: {e}"))
                })?;
                if let Err(e) = created.set_upstream(Some(name)) {
                    eprintln!("Upstream {name} für {local} setzen fehlgeschlagen: {e}");
                }
                (local, true)
            }
        }
        WorktreeBranch::New { name, start } => {
            let name = name.trim();
            if !git2::Branch::name_is_valid(name).unwrap_or(false) {
                return Err(CheckoutError::Git(format!(
                    "Ungültiger Branch-Name: {name}"
                )));
            }
            if has_local(name) {
                return Err(CheckoutError::Git(format!(
                    "Branch {name} existiert bereits"
                )));
            }
            let commit = match start.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(start) => repo.revparse_single(start).and_then(|o| o.peel_to_commit()),
                None => repo.head().and_then(|h| h.peel_to_commit()),
            }
            .map_err(|e| CheckoutError::Git(format!("Startpunkt nicht gefunden: {e}")))?;
            repo.branch(name, &commit, false).map_err(|e| {
                CheckoutError::Git(format!("Branch {name} anlegen fehlgeschlagen: {e}"))
            })?;
            (name.to_string(), true)
        }
    };
    drop(repo);
    match add_worktree(base_path, target_path, &local) {
        Ok(()) => Ok(local),
        Err(e) => {
            if created {
                if let Ok(repo) = Repository::open(base_path) {
                    if let Ok(mut b) = repo.find_branch(&local, BranchType::Local) {
                        let _ = b.delete();
                    }
                }
            }
            Err(e)
        }
    }
}

/// `origin/feature/x` → `feature/x`: längster passender Remote-Name als
/// Präfix (Remotes dürfen `/` enthalten). `None`, wenn kein Remote passt
/// oder nach dem Präfix nichts übrig bleibt (`origin/`).
pub fn strip_remote_prefix<'a, 'r>(
    remote_branch: &'a str,
    remotes: impl IntoIterator<Item = &'r str>,
) -> Option<&'a str> {
    remotes
        .into_iter()
        .filter_map(|r| remote_branch.strip_prefix(r)?.strip_prefix('/'))
        .filter(|rest| !rest.is_empty())
        .min_by_key(|rest| rest.len())
}

fn remote_local_name(repo: &Repository, remote_branch: &str) -> Option<String> {
    let remotes = repo.remotes().ok()?;
    strip_remote_prefix(remote_branch, remotes.iter().flatten().flatten()).map(str::to_string)
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
pub(super) fn missing_dir_key(path: &Path) -> String {
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
pub(super) fn worktree_add_with_git_cli(
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
pub(super) fn clone_with_git_cli(
    url: &str,
    target_path: &Path,
    branch: &str,
) -> anyhow::Result<()> {
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
