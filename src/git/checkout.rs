//! Branch-Wechsel im Arbeitsverzeichnis (auch mit Stash).

use super::{has_merge_conflicts, is_merge_in_progress, other_worktrees};
use git2::{BranchType, Repository};
use std::path::{Component, Path, PathBuf};

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
/// Display bleibt Deutsch wie die übrigen git-Fehler; die UI lokalisiert
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
/// als HEAD ausgecheckt? Liefert dessen Pfad; der eigene Worktree zählt
/// nicht (siehe [`other_worktrees`]).
pub(crate) fn branch_checked_out_elsewhere(repo: &Repository, branch: &str) -> Option<PathBuf> {
    other_worktrees(repo)
        .into_iter()
        .find(|w| w.branch.as_deref() == Some(branch))
        .map(|w| w.path)
}

/// Symbolischer HEAD eines Git-Verzeichnisses (`ref: refs/heads/x` →
/// `refs/heads/x`); `None` bei Detached HEAD oder unlesbarer Datei. Anders
/// als `parse_head_file` ohne SHA-Kurzform (kein Zufallstreffer).
pub(super) fn head_symref(git_dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    content
        .trim()
        .strip_prefix("ref: ")
        .map(|r| r.trim().to_string())
}

/// Ordner eines verknüpften Worktrees aus `<admin>/gitdir` (zeigt auf
/// `<wt>/.git`, relativ zum Admin-Ordner bei `worktree.useRelativePaths`).
/// `None` bei verwaistem Eintrag: Ziel fehlt und kein `locked`.
pub(super) fn linked_worktree_path(admin: &Path) -> Option<PathBuf> {
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
pub(super) enum RefUpdate {
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
    let (object, reference) = revparse_branch_first(repo, branch_name)
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

/// `revparse_ext`, aber Branches zuerst: `revparse_ext` prüft
/// `refs/tags/<name>` vor `refs/heads/<name>` und `refs/remotes/<name>`,
/// ein gleichnamiger Tag (`release-1.2`) landete so als Detached HEAD statt
/// auf dem Branch. Übergeben werden Namen aus der Branch-Liste (`x`,
/// `origin/x`) und der MR-Branch; Tags, Commits und Ausdrücke laufen weiter
/// über `revparse_ext`.
fn revparse_branch_first<'r>(
    repo: &'r Repository,
    name: &str,
) -> Result<(git2::Object<'r>, Option<git2::Reference<'r>>), git2::Error> {
    for prefix in ["refs/heads/", "refs/remotes/"] {
        // Wie `revparse_ext` aufgelöst (`origin/HEAD` → `origin/main`)
        let found = repo
            .find_reference(&format!("{prefix}{name}"))
            .and_then(|r| r.resolve());
        if let Ok(reference) = found {
            let object = reference.peel(git2::ObjectType::Any)?;
            return Ok((object, Some(reference)));
        }
    }
    repo.revparse_ext(name)
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

pub(super) fn apply_ref_update(repo: &Repository, update: &RefUpdate) -> anyhow::Result<()> {
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

/// M4: typisiert: Stash pop nach erfolgreichem Wechsel gescheitert (Teil von
/// [`StashSwitch`]); der Display-Text ist der Hinweis im Dialog.
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

/// Ergebnis von [`stash_and_checkout`]: der Branch ist gewechselt.
/// `pop_failed`, wenn sich der Stash danach nicht poppen ließ und erhalten
/// bleibt – kein `Err`, sonst gälte ein erledigter Wechsel (MR) als
/// gescheitert.
#[derive(Debug)]
pub struct StashSwitch {
    pub outcome: CheckoutOutcome,
    pub pop_failed: Option<StashPopFailed>,
}

/// Anzahl der Stashes (0, wenn sie sich nicht lesen lassen).
fn stash_count(repo: &mut Repository) -> usize {
    let mut count = 0;
    let _ = repo.stash_foreach(|_, _, _| {
        count += 1;
        true
    });
    count
}

/// Stash + Checkout (für Dirty-Dialog Option)
pub fn stash_and_checkout(path: &Path, branch_name: &str) -> anyhow::Result<StashSwitch> {
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
    // Vorhandene Stashes zählen: scheitert `stash_save` nach dem Anlegen
    // des Stashes (libgit2 schreibt `refs/stash`, bevor es das
    // Arbeitsverzeichnis zurücksetzt – eine von IDE oder Virenscanner
    // gesperrte Datei genügt), liegen die Änderungen im Stash, obwohl ein
    // Fehler zurückkommt.
    let stashes_before = stash_count(&mut repo);
    let stashed = match repo.stash_save(&sig, &stash_msg, Some(stash_flags)) {
        Ok(_) => true,
        // „Nichts zu stashen“ (keine Änderungen): trotzdem wechseln.
        // Repo ohne Commit: libgit2 kann nicht stashen, der Checkout
        // selbst überschreibt keine fremden Dateien
        Err(e)
            if matches!(
                e.code(),
                git2::ErrorCode::NotFound | git2::ErrorCode::UnbornBranch
            ) =>
        {
            false
        }
        Err(e) => {
            let hint = if stash_count(&mut repo) > stashes_before {
                " – ein Teil der Änderungen liegt bereits unter stash@{0}"
            } else {
                ""
            };
            anyhow::bail!("Stash fehlgeschlagen: {e}{hint}");
        }
    };
    // Jetzt checkout
    match checkout_branch(path, branch_name) {
        Ok(outcome) => {
            // Nur poppen wenn tatsächlich gestasht wurde — sonst würde
            // stash_pop(0) einen alten, bewusst aufgehobenen Stash poppen (K1).
            if !stashed {
                return Ok(StashSwitch {
                    outcome,
                    pop_failed: None,
                });
            }
            // Versuche stash pop, aber ignoriere Konflikte (lasse stash bestehen)
            let mut repo2 = Repository::open(path)
                .map_err(|e| anyhow::anyhow!("Repo erneut öffnen fehlgeschlagen: {e}"))?;
            let pop_failed = repo2.stash_pop(0, None).err().map(|e| {
                eprintln!(
                    "Stash pop nach Checkout fehlgeschlagen (Konflikt), Stash bleibt erhalten: {e}"
                );
                // Lasse stash bestehen, informiere Nutzer (M4: typisiert).
                StashPopFailed {
                    detail: format!("{e}"),
                }
            });
            Ok(StashSwitch {
                outcome,
                pop_failed,
            })
        }
        Err(e) => {
            // Checkout fehlgeschlagen → Stash wiederherstellen; gelingt
            // das nicht, muss der Nutzer wissen, wo seine Änderungen liegen
            if stashed {
                let restored = Repository::open(path)
                    .and_then(|mut r| r.stash_pop(0, None))
                    .map_err(|pe| format!("{pe}"));
                if let Err(pe) = restored {
                    return Err(e.context(format!(
                        "Änderungen liegen weiter unter stash@{{0}} (Wiederherstellen fehlgeschlagen: {pe})"
                    )));
                }
            }
            Err(e)
        }
    }
}
