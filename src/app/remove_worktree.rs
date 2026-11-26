//! „Worktree entfernen“ aus dem `⋯`-Menü einer Repo-Karte: Dialog mit
//! Warnungen (nie stillschweigend Daten löschen), Worker und Ergebnis.
//! Gelöscht wird ohne ausdrückliches „Trotzdem entfernen“ nichts, was
//! ungesichert wäre; den Branch gibt es nur bei sicherem Zustand dazu.

use super::{repo_badges, GitBusy, MyApp};
use crate::git::{self, BranchSafety, RepoInfo};
use crate::i18n::{tr, tr_fmt};
use egui::{Color32, RichText};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

/// Offener Dialog und Kanal für die Ergebnisse der Worker.
pub(super) struct RemoveUi {
    pub dialog: Option<RemoveDialog>,
    tx: Sender<RemoveMsg>,
    rx: Receiver<RemoveMsg>,
}

impl Default for RemoveUi {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            dialog: None,
            tx,
            rx,
        }
    }
}

/// Ergebnisse der Worker des Dialogs.
enum RemoveMsg {
    /// Prüfung nach dem Öffnen (Branch-Sicherheit, erreichbare Commits)
    Checked {
        path: PathBuf,
        safety: Option<BranchSafety>,
        head_unreachable: bool,
    },
    /// Der Zustand hat sich seit dem Öffnen geändert: nichts entfernt, der
    /// Dialog zeigt den neuen Stand und will eine neue Bestätigung
    Changed(Box<RemoveDialog>),
    Done {
        path: PathBuf,
        result: anyhow::Result<RemoveOutcome>,
    },
}

/// Erfolgreiches Entfernen.
#[derive(Debug, PartialEq, Eq)]
enum RemoveOutcome {
    Removed,
    /// Worktree entfernt, der Branch blieb aber erhalten (Name, Grund)
    BranchKept {
        branch: String,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub(super) struct RemoveDialog {
    pub path: PathBuf,
    /// Haupt-Repo, in dem `git worktree remove` läuft
    pub main: PathBuf,
    pub branch: Option<String>,
    pub dirty_files: usize,
    pub ahead: usize,
    /// Detached HEAD mit nirgends erreichbaren Commits
    pub head_unreachable: bool,
    pub safety: Option<BranchSafety>,
    pub delete_branch: bool,
    pub claude_running: bool,
    /// `safety` und `head_unreachable` werden noch im Worker ermittelt
    /// (Graph-Läufe, bei vielen Refs oder langsamer Platte spürbar)
    pub checking: bool,
    pub running: bool,
}

impl RemoveDialog {
    /// Dialog aus dem Scan-Stand, ohne git-Zugriff; `safety` und
    /// `head_unreachable` liefert [`Self::inspect`].
    fn unchecked(repo: &RepoInfo, claude_running: bool) -> Self {
        Self {
            path: repo.path.clone(),
            main: git::main_repo_of(&repo.common_dir),
            branch: (!repo.is_detached && !repo.branch.is_empty()).then(|| repo.branch.clone()),
            dirty_files: repo.dirty_files,
            ahead: repo.ahead,
            head_unreachable: false,
            safety: None,
            delete_branch: false,
            claude_running,
            checking: true,
            running: false,
        }
    }

    /// Mit frisch gelesenem Zustand (Worker): öffnet Repos und läuft durch
    /// den Commit-Graphen.
    pub fn new(repo: &RepoInfo, claude_running: bool) -> Self {
        let mut dialog = Self::unchecked(repo, claude_running);
        let (safety, head_unreachable) = dialog.inspect();
        dialog.safety = safety;
        dialog.head_unreachable = head_unreachable;
        dialog.checking = false;
        dialog
    }

    /// Branch-Sicherheit und „HEAD nirgends erreichbar“. Der MR-Status
    /// kommt erst über „Aufräumen“: hier ohne gemergte MR-Köpfe.
    fn inspect(&self) -> (Option<BranchSafety>, bool) {
        let safety = self
            .branch
            .as_deref()
            .and_then(|b| git::branch_safety(&self.main, b, &[]));
        (safety, git::head_unreachable(&self.path))
    }

    /// Verlustgefahr: nur mit ausdrücklichem „Trotzdem entfernen“.
    pub fn needs_force(&self) -> bool {
        self.dirty_files > 0 || self.ahead > 0 || self.head_unreachable
    }

    /// Branch-Löschen ist nur bei sicherem Branch anwählbar.
    pub fn can_delete_branch(&self) -> bool {
        self.branch.is_some() && self.safety.is_some()
    }

    /// Branch und `-D`-Flag zum Löschen nach dem Entfernen, falls gewünscht.
    pub fn branch_to_delete(&self) -> Option<(&str, bool)> {
        let safety = self.safety?;
        if !self.delete_branch {
            return None;
        }
        Some((self.branch.as_deref()?, safety.needs_force_delete()))
    }

    /// Weicht der frisch gelesene Zustand von dem ab, den der Nutzer
    /// bestätigt hat?
    fn differs_from(&self, fresh: &RemoveDialog) -> bool {
        fresh.dirty_files != self.dirty_files
            || fresh.ahead != self.ahead
            || fresh.head_unreachable != self.head_unreachable
            || fresh.safety != self.safety
    }
}

/// Worker von „Entfernen“: Zustand neu lesen (zwischen Öffnen und Klick
/// kann sich der Ordner geändert haben, z. B. durch Claude), dann entfernen
/// und den Branch nur löschen, wenn er noch so sicher ist wie angezeigt.
fn run_remove(confirmed: &RemoveDialog) -> RemoveMsg {
    let path = confirmed.path.clone();
    if let Some(info) = git::get_repo_info(path.clone()) {
        let mut fresh = RemoveDialog::new(&info, confirmed.claude_running);
        if confirmed.differs_from(&fresh) {
            fresh.delete_branch = confirmed.delete_branch && fresh.can_delete_branch();
            return RemoveMsg::Changed(Box::new(fresh));
        }
    }
    let result = (|| {
        git::remove_worktree(&path, &confirmed.main, confirmed.needs_force(), None)?;
        let Some((branch, force_delete)) = confirmed.branch_to_delete() else {
            return Ok(RemoveOutcome::Removed);
        };
        let kept = |reason: String| RemoveOutcome::BranchKept {
            branch: branch.to_string(),
            reason,
        };
        if git::branch_safety(&confirmed.main, branch, &[]) != confirmed.safety {
            // Leerer Grund: „hat sich inzwischen geändert“ (übersetzt)
            return Ok(kept(String::new()));
        }
        Ok(
            match git::delete_local_branch(&confirmed.main, branch, force_delete) {
                Ok(()) => RemoveOutcome::Removed,
                Err(e) => kept(format!("{e:#}")),
            },
        )
    })();
    RemoveMsg::Done { path, result }
}

impl MyApp {
    pub(super) fn open_remove_worktree(&mut self, path: &Path) {
        // Läuft schon eine Entfernung, den laufenden Dialog nicht überschreiben
        if self.remove_ui.dialog.as_ref().is_some_and(|d| d.running) {
            self.set_status(tr(self.config.language, "rm_wt_running"));
            return;
        }
        let Some(repo) = self
            .repos
            .iter()
            .find(|r| r.path == path && r.is_linked_worktree)
        else {
            return;
        };
        let claude = repo_badges::claude_running_at(&self.claude.live_any_place(), path);
        // Sofort öffnen; die git-Prüfung läuft im Hintergrund
        let dialog = RemoveDialog::unchecked(repo, claude);
        let probe = dialog.clone();
        self.remove_ui.dialog = Some(dialog);
        let tx = self.remove_ui.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            // Nach einer Panic gilt der vorsichtige Stand: Branch nicht
            // löschbar, Entfernen nur mit „Trotzdem“
            let (safety, head_unreachable) =
                catch_unwind(AssertUnwindSafe(|| probe.inspect())).unwrap_or((None, true));
            let _ = tx.send(RemoveMsg::Checked {
                path: probe.path,
                safety,
                head_unreachable,
            });
            ctx.request_repaint();
        });
    }

    pub(super) fn show_remove_worktree_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.remove_ui.dialog.take() else {
            return;
        };
        let lang = self.config.language;
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        let busy = (self.git_busy.holds_refs() && !dialog.running)
            || self.auto_fetching
            || self.branch_switch.is_some();
        let red = crate::ui::theme::COLOR_ERROR;

        egui::Window::new(tr(lang, "rm_wt_title"))
            .id(egui::Id::new("remove_worktree_dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.add_enabled_ui(!dialog.running, |ui| {
                    ui.label(tr_fmt(
                        lang,
                        "rm_wt_question",
                        &[&dialog.path.display().to_string()],
                    ));
                    if dialog.dirty_files > 0 {
                        let n = dialog.dirty_files.to_string();
                        ui.label(RichText::new(tr_fmt(lang, "rm_wt_dirty", &[&n])).color(red));
                    }
                    if dialog.ahead > 0 {
                        let n = dialog.ahead.to_string();
                        ui.label(RichText::new(tr_fmt(lang, "rm_wt_ahead", &[&n])).color(red));
                    }
                    if dialog.head_unreachable {
                        ui.label(RichText::new(tr(lang, "rm_wt_detached")).color(red));
                    }
                    if dialog.claude_running {
                        ui.label(
                            RichText::new(tr(lang, "rm_wt_claude"))
                                .color(crate::ui::theme::COLOR_WARN),
                        );
                    }
                    if let Some(branch) = dialog.branch.clone() {
                        ui.add_space(6.0);
                        let enabled = dialog.can_delete_branch();
                        let resp = ui.add_enabled(
                            enabled,
                            egui::Checkbox::new(
                                &mut dialog.delete_branch,
                                tr_fmt(lang, "rm_wt_delete_branch", &[&branch]),
                            ),
                        );
                        if !enabled {
                            dialog.delete_branch = false;
                            if !dialog.checking {
                                resp.on_disabled_hover_text(tr(lang, "rm_wt_branch_unsafe"));
                            }
                        }
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let enabled = !dialog.running && !dialog.checking && !busy;
                    let button = if dialog.needs_force() {
                        egui::Button::new(
                            RichText::new(tr(lang, "rm_wt_remove_anyway")).color(Color32::WHITE),
                        )
                        .fill(red)
                    } else {
                        egui::Button::new(tr(lang, "rm_wt_remove"))
                    };
                    if ui.add_enabled(enabled, button).clicked() {
                        confirm = true;
                    }
                    if ui
                        .add_enabled(!dialog.running, egui::Button::new(tr(lang, "cancel")))
                        .clicked()
                    {
                        cancel = true;
                    }
                    // Warum „Entfernen“ gerade gesperrt ist
                    let waiting = if dialog.running {
                        Some("rm_wt_running")
                    } else if dialog.checking {
                        Some("rm_wt_checking")
                    } else if busy {
                        Some("wt_wait_busy")
                    } else {
                        None
                    };
                    if let Some(key) = waiting {
                        ui.spinner();
                        ui.label(RichText::new(tr(lang, key)).size(11.0));
                    }
                });
            });

        if confirm {
            self.remove_ui.dialog = Some(dialog);
            self.confirm_remove_worktree();
        } else if (open && !cancel) || dialog.running {
            self.remove_ui.dialog = Some(dialog);
        }
    }

    /// Startet den Worker für den offenen Dialog.
    pub(super) fn confirm_remove_worktree(&mut self) {
        if self.refuse_while_busy() {
            return;
        }
        let Some(dialog) = self.remove_ui.dialog.as_mut() else {
            return;
        };
        if dialog.running || dialog.checking {
            return;
        }
        dialog.running = true;
        let confirmed = dialog.clone();
        self.set_git_busy(GitBusy::Remove {
            path: confirmed.path.clone(),
        });
        let tx = self.remove_ui.tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            // Panic vor dem Senden darf `git_busy` nicht hängen lassen
            let msg =
                catch_unwind(AssertUnwindSafe(|| run_remove(&confirmed))).unwrap_or_else(|_| {
                    eprintln!("Worktree-Entfernen-Thread abgestürzt");
                    RemoveMsg::Done {
                        path: confirmed.path.clone(),
                        result: Err(anyhow::anyhow!("remove aborted")),
                    }
                });
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    /// Ergebnisse der Worker: Prüfung eintragen bzw. Dialog schließen,
    /// melden, neu scannen.
    pub(super) fn poll_remove_worktree(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.remove_ui.rx.try_recv() {
            let lang = self.config.language;
            match msg {
                RemoveMsg::Checked {
                    path,
                    safety,
                    head_unreachable,
                } => {
                    // Nur der noch offene, ungeprüfte Dialog dieses Ordners
                    if let Some(d) = self
                        .remove_ui
                        .dialog
                        .as_mut()
                        .filter(|d| d.path == path && d.checking)
                    {
                        d.safety = safety;
                        d.head_unreachable = head_unreachable;
                        d.checking = false;
                    }
                }
                RemoveMsg::Changed(fresh) => {
                    self.release_remove(&fresh.path);
                    if let Some(d) = self
                        .remove_ui
                        .dialog
                        .as_mut()
                        .filter(|d| d.path == fresh.path)
                    {
                        *d = *fresh;
                    }
                    self.set_status(tr(lang, "rm_wt_changed"));
                }
                RemoveMsg::Done { path, result } => {
                    self.release_remove(&path);
                    if self
                        .remove_ui
                        .dialog
                        .as_ref()
                        .is_some_and(|d| d.path == path)
                    {
                        self.remove_ui.dialog = None;
                    }
                    let shown = path.display().to_string();
                    // Auch nach Fehler: ein halb entfernter Worktree soll
                    // sichtbar werden. Scan zuerst — er leert `self.error`
                    // beim Start.
                    self.start_scan();
                    let problem = match result {
                        Ok(RemoveOutcome::Removed) => {
                            self.set_status(tr_fmt(lang, "rm_wt_done", &[&shown]));
                            None
                        }
                        // Der Worktree ist weg, nur der Branch nicht: kein
                        // „fehlgeschlagen“
                        Ok(RemoveOutcome::BranchKept { branch, reason }) => {
                            let reason = if reason.is_empty() {
                                tr(lang, "rm_wt_branch_changed")
                            } else {
                                reason
                            };
                            Some(tr_fmt(
                                lang,
                                "rm_wt_branch_kept",
                                &[&shown, &branch, &reason],
                            ))
                        }
                        Err(e) => {
                            eprintln!("Worktree entfernen fehlgeschlagen für {shown}: {e:#}");
                            Some(tr_fmt(lang, "rm_wt_failed", &[&shown, &format!("{e:#}")]))
                        }
                    };
                    if let Some(text) = problem {
                        // Banner bleibt über den Rescan hinweg stehen
                        self.error = Some(text.clone());
                        self.error_after_scan = Some(text);
                    }
                }
            }
            ctx.request_repaint();
        }
    }

    /// Ref-Sperre des Entfernens für diesen Ordner lösen.
    fn release_remove(&mut self, path: &Path) {
        if matches!(&self.git_busy, GitBusy::Remove { path: busy } if busy == path) {
            self.set_git_busy(GitBusy::Idle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{git, test_app};
    use crate::git::{BranchSafety, RepoInfo};
    use std::path::{Path, PathBuf};

    fn dialog(dirty: usize, ahead: usize, unreachable: bool) -> RemoveDialog {
        RemoveDialog {
            path: PathBuf::from("/x/wt"),
            main: PathBuf::from("/x/main"),
            branch: Some("feat".into()),
            dirty_files: dirty,
            ahead,
            head_unreachable: unreachable,
            safety: None,
            delete_branch: false,
            claude_running: false,
            checking: false,
            running: false,
        }
    }

    /// Übernimmt Ergebnisse, bis die Prüfung nach dem Öffnen da ist und
    /// kein Entfernen mehr läuft.
    fn settle(app: &mut MyApp) {
        let ctx = egui::Context::default();
        for _ in 0..2000 {
            app.poll_remove_worktree(&ctx);
            let waiting = app
                .remove_ui
                .dialog
                .as_ref()
                .is_some_and(|d| d.checking || d.running);
            if !waiting && !matches!(app.git_busy, GitBusy::Remove { .. }) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("Worker hängt");
    }

    #[test]
    fn dialog_needs_force_when_dirty_or_ahead() {
        assert!(!dialog(0, 0, false).needs_force());
        assert!(dialog(2, 0, false).needs_force());
        assert!(dialog(0, 1, false).needs_force());
        assert!(dialog(0, 0, true).needs_force());
    }

    #[test]
    fn delete_branch_disabled_without_safety() {
        let mut d = dialog(0, 0, false);
        d.delete_branch = true;
        assert!(!d.can_delete_branch());
        assert_eq!(d.branch_to_delete(), None);
        d.safety = Some(BranchSafety::Pushed);
        assert_eq!(d.branch_to_delete(), Some(("feat", false)));
        d.safety = Some(BranchSafety::MrMerged);
        assert_eq!(d.branch_to_delete(), Some(("feat", true)));
        d.delete_branch = false;
        assert_eq!(d.branch_to_delete(), None);
    }

    #[test]
    fn remove_runs_and_rescans() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q", "-b", "main"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let wt = tmp.path().join("wt");
        git(
            &main,
            &["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()],
        );

        let mut app = test_app();
        let mut info = RepoInfo::new(wt.clone(), "feat".into(), false, false);
        info.is_linked_worktree = true;
        info.common_dir = main.join(".git");
        app.repos = vec![info];
        app.open_remove_worktree(&wt);
        let d = app.remove_ui.dialog.as_ref().expect("dialog offen");
        assert_eq!(d.main, main);
        assert!(d.checking, "git-Prüfung läuft im Hintergrund");
        // Vor dem Ergebnis der Prüfung startet nichts
        app.confirm_remove_worktree();
        assert_eq!(app.git_busy, GitBusy::Idle);
        settle(&mut app);
        assert!(!app.remove_ui.dialog.as_ref().unwrap().needs_force());
        app.confirm_remove_worktree();
        assert!(matches!(app.git_busy, GitBusy::Remove { .. }));
        settle(&mut app);
        assert!(!wt.exists(), "Ordner entfernt");
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.remove_ui.dialog.is_none());
        assert_eq!(app.git_busy, GitBusy::Scan, "Rescan angestoßen");
    }

    #[test]
    fn remove_error_survives_the_rescan() {
        let mut app = test_app();
        let lang = app.config.language;
        let path = PathBuf::from("/x/wt");
        app.set_git_busy(GitBusy::Remove { path: path.clone() });
        app.remove_ui
            .tx
            .send(RemoveMsg::Done {
                path: path.clone(),
                result: Err(anyhow::anyhow!("kaputt")),
            })
            .unwrap();
        app.poll_remove_worktree(&egui::Context::default());
        let text = tr_fmt(
            lang,
            "rm_wt_failed",
            &[&path.display().to_string(), "kaputt"],
        );
        assert_eq!(app.error.as_deref(), Some(text.as_str()));
        assert_eq!(app.git_busy, GitBusy::Scan, "Rescan angestoßen");

        let ctx = egui::Context::default();
        let wait_scan = |app: &mut MyApp| {
            for _ in 0..400 {
                app.poll_scan(&ctx);
                if app.git_busy != GitBusy::Scan {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            panic!("Scan nicht fertig");
        };
        wait_scan(&mut app);
        assert_eq!(app.error.as_deref(), Some(text.as_str()), "nach Scan");
        // Ein späterer Scan löscht ihn (Fehler gilt für dieses Entfernen)
        app.start_scan();
        wait_scan(&mut app);
        assert_eq!(app.error, None);
    }

    fn setup(tmp: &Path) -> (MyApp, PathBuf, PathBuf) {
        let main = tmp.join("main");
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q", "-b", "main"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let wt = tmp.join("wt");
        git(
            &main,
            &["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()],
        );
        let mut app = test_app();
        app.repos = vec![crate::git::get_repo_info(wt.clone()).unwrap()];
        assert!(app.repos[0].is_linked_worktree);
        (app, main, wt)
    }

    #[test]
    fn confirm_rechecks_state_and_does_not_run_when_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut app, _main, wt) = setup(tmp.path());
        app.open_remove_worktree(&wt);
        settle(&mut app);
        assert!(!app.remove_ui.dialog.as_ref().unwrap().needs_force());
        std::fs::write(wt.join("neu.txt"), "x").unwrap();
        app.confirm_remove_worktree();
        settle(&mut app);
        assert_eq!(app.git_busy, GitBusy::Idle, "Sperre wieder gelöst");
        assert!(wt.exists(), "nichts entfernt");
        let d = app.remove_ui.dialog.as_ref().unwrap();
        assert!(d.needs_force() && !d.running);
        assert_eq!(
            app.status_message,
            Some(tr(app.config.language, "rm_wt_changed"))
        );
    }

    #[test]
    fn confirm_does_not_delete_branch_that_became_unsafe() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut app, main, wt) = setup(tmp.path());
        // Default-Branch des Remotes simulieren: feat ist darin enthalten
        git(&main, &["update-ref", "refs/remotes/origin/main", "main"]);
        app.open_remove_worktree(&wt);
        settle(&mut app);
        {
            let d = app.remove_ui.dialog.as_mut().unwrap();
            assert_eq!(d.safety, Some(BranchSafety::MergedIntoDefault));
            d.delete_branch = true;
        }
        git(&wt, &["commit", "-q", "--allow-empty", "-m", "neu"]);
        app.confirm_remove_worktree();
        settle(&mut app);
        assert_eq!(app.git_busy, GitBusy::Idle);
        assert!(wt.exists());
        let d = app.remove_ui.dialog.as_ref().unwrap();
        assert!(!d.delete_branch);
        git(&main, &["rev-parse", "--verify", "feat"]);
    }

    #[test]
    fn open_ignored_while_running_and_for_non_linked() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut app, main, wt) = setup(tmp.path());
        app.repos
            .push(crate::git::get_repo_info(main.clone()).unwrap());
        app.open_remove_worktree(&main);
        assert!(app.remove_ui.dialog.is_none(), "kein verknüpfter Worktree");
        app.open_remove_worktree(&wt);
        settle(&mut app);
        app.remove_ui.dialog.as_mut().unwrap().running = true;
        app.repos.push(RepoInfo::new(
            PathBuf::from("/x/other"),
            "b".into(),
            false,
            false,
        ));
        app.repos[2].is_linked_worktree = true;
        app.open_remove_worktree(Path::new("/x/other"));
        assert_eq!(app.remove_ui.dialog.as_ref().unwrap().path, wt);
        // Fremdes Ergebnis schließt den Dialog nicht
        app.remove_ui
            .tx
            .send(RemoveMsg::Done {
                path: PathBuf::from("/x/else"),
                result: Ok(RemoveOutcome::Removed),
            })
            .unwrap();
        app.poll_remove_worktree(&egui::Context::default());
        assert!(app.remove_ui.dialog.is_some());
    }
}
