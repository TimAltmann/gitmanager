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

/// Offener Dialog und Kanal für das Ergebnis des Workers.
pub(super) struct RemoveUi {
    pub dialog: Option<RemoveDialog>,
    tx: Sender<RemoveDone>,
    rx: Receiver<RemoveDone>,
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

struct RemoveDone {
    path: PathBuf,
    result: anyhow::Result<()>,
}

#[derive(Debug, Clone)]
pub(super) struct RemoveDialog {
    pub path: PathBuf,
    /// Haupt-Repo, in dem `git worktree remove` läuft
    pub main: PathBuf,
    pub branch: Option<String>,
    pub dirty_files: usize,
    pub ahead: usize,
    /// Detached HEAD mit nirgends erreichbaren Commits (einmal beim Öffnen
    /// berechnet, nicht pro Frame)
    pub head_unreachable: bool,
    pub safety: Option<BranchSafety>,
    pub delete_branch: bool,
    pub claude_running: bool,
    pub running: bool,
}

impl RemoveDialog {
    pub fn new(repo: &RepoInfo, claude_running: bool) -> Self {
        let main = git::main_repo_of(&repo.common_dir);
        let branch = (!repo.is_detached && !repo.branch.is_empty()).then(|| repo.branch.clone());
        // Der MR-Status kommt erst über „Aufräumen“: hier ohne gemergte MR-Köpfe
        let safety = branch
            .as_deref()
            .and_then(|b| git::branch_safety(&main, b, &[]));
        Self {
            path: repo.path.clone(),
            main,
            branch,
            dirty_files: repo.dirty_files,
            ahead: repo.ahead,
            head_unreachable: git::head_unreachable(&repo.path),
            safety,
            delete_branch: false,
            claude_running,
            running: false,
        }
    }

    /// Verlustgefahr: nur mit ausdrücklichem „Trotzdem entfernen“.
    pub fn needs_force(&self) -> bool {
        self.dirty_files > 0 || self.ahead > 0 || self.head_unreachable
    }

    /// Branch-Löschen ist nur bei sicherem Branch anwählbar.
    pub fn can_delete_branch(&self) -> bool {
        self.branch.is_some() && self.safety.is_some()
    }

    /// Branch und `-D`-Flag für `git::remove_worktree`, falls gewünscht.
    pub fn branch_to_delete(&self) -> Option<(&str, bool)> {
        let safety = self.safety?;
        if !self.delete_branch {
            return None;
        }
        Some((self.branch.as_deref()?, safety.needs_force_delete()))
    }
}

impl MyApp {
    pub(super) fn open_remove_worktree(&mut self, path: &Path) {
        // Läuft schon eine Entfernung, den laufenden Dialog nicht überschreiben
        if self.remove_ui.dialog.as_ref().is_some_and(|d| d.running) {
            return;
        }
        let Some(repo) = self
            .repos
            .iter()
            .find(|r| r.path == path && r.is_linked_worktree)
        else {
            return;
        };
        let claude = repo_badges::claude_running_at(&self.claude.live, path);
        self.remove_ui.dialog = Some(RemoveDialog::new(repo, claude));
    }

    pub(super) fn show_remove_worktree_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.remove_ui.dialog.take() else {
            return;
        };
        let lang = self.config.language;
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        let busy = (self.git_busy.holds_refs() && !dialog.running) || self.auto_fetching;
        let red = Color32::from_rgb(200, 60, 60);

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
                                .color(Color32::from_rgb(200, 120, 40)),
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
                            resp.on_disabled_hover_text(tr(lang, "rm_wt_branch_unsafe"));
                        }
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let enabled = !dialog.running && !busy;
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
                    if dialog.running {
                        ui.spinner();
                        ui.label(RichText::new(tr(lang, "rm_wt_running")).size(11.0));
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
        let lang = self.config.language;
        if self.auto_fetching {
            self.set_status(tr(lang, "auto_fetch_running"));
            return;
        }
        if self.git_busy.holds_refs() {
            self.set_status(tr(lang, "fetch_busy"));
            return;
        }
        let Some(dialog) = self.remove_ui.dialog.as_mut() else {
            return;
        };
        if dialog.running {
            return;
        }
        // Zustand zum Zeitpunkt der Bestätigung neu lesen: zwischen Öffnen
        // und Klick kann sich der Ordner (z. B. durch Claude) geändert haben
        if let Some(info) = git::get_repo_info(dialog.path.clone()) {
            let mut fresh = RemoveDialog::new(&info, dialog.claude_running);
            fresh.delete_branch = dialog.delete_branch && fresh.can_delete_branch();
            if fresh.dirty_files != dialog.dirty_files
                || fresh.ahead != dialog.ahead
                || fresh.head_unreachable != dialog.head_unreachable
                || fresh.safety != dialog.safety
                || fresh.needs_force() != dialog.needs_force()
            {
                *dialog = fresh;
                self.set_status(tr(lang, "rm_wt_changed"));
                return;
            }
        }
        dialog.running = true;
        let path = dialog.path.clone();
        let main = dialog.main.clone();
        let force = dialog.needs_force();
        let branch = dialog
            .branch_to_delete()
            .map(|(b, force_delete)| (b.to_string(), force_delete));
        let expected = dialog.safety;
        self.set_git_busy(GitBusy::Remove { path: path.clone() });
        let tx = self.remove_ui.tx.clone();
        std::thread::spawn(move || {
            // Panic vor dem Senden darf `git_busy` nicht hängen lassen
            let result = catch_unwind(AssertUnwindSafe(|| {
                git::remove_worktree(&path, &main, force, None)?;
                // Branch erst löschen, wenn er noch genauso sicher ist wie angezeigt
                if let Some((b, force_delete)) = &branch {
                    if git::branch_safety(&main, b, &[]) != expected {
                        anyhow::bail!(
                            "branch '{b}' changed meanwhile and was not deleted; \
                             the worktree itself was removed"
                        );
                    }
                    git::delete_local_branch(&main, b, *force_delete)?;
                }
                Ok(())
            }))
            .unwrap_or_else(|_| {
                eprintln!("Worktree-Entfernen-Thread abgestürzt");
                Err(anyhow::anyhow!("remove aborted"))
            });
            let _ = tx.send(RemoveDone { path, result });
        });
    }

    /// Ergebnis des Workers: Dialog schließen, melden, neu scannen.
    pub(super) fn poll_remove_worktree(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.remove_ui.rx.try_recv() {
            let lang = self.config.language;
            if matches!(&self.git_busy, GitBusy::Remove { path } if *path == done.path) {
                self.set_git_busy(GitBusy::Idle);
            }
            if self
                .remove_ui
                .dialog
                .as_ref()
                .is_some_and(|d| d.path == done.path)
            {
                self.remove_ui.dialog = None;
            }
            let shown = done.path.display().to_string();
            match &done.result {
                Ok(()) => self.set_status(tr_fmt(lang, "rm_wt_done", &[&shown])),
                Err(e) => {
                    eprintln!("Worktree entfernen fehlgeschlagen für {shown}: {e:#}");
                    self.error = Some(tr_fmt(lang, "rm_wt_failed", &[&shown, &format!("{e:#}")]));
                }
            }
            // Auch nach Fehler: ein halb entfernter Worktree soll sichtbar werden
            self.start_scan();
            ctx.request_repaint();
        }
        if matches!(self.git_busy, GitBusy::Remove { .. }) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;
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
            running: false,
        }
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

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
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
        assert!(!d.needs_force());
        app.confirm_remove_worktree();
        assert!(matches!(app.git_busy, GitBusy::Remove { .. }));

        let ctx = egui::Context::default();
        for _ in 0..200 {
            app.poll_remove_worktree(&ctx);
            if !matches!(app.git_busy, GitBusy::Remove { .. }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!wt.exists(), "Ordner entfernt");
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.remove_ui.dialog.is_none());
        assert_eq!(app.git_busy, GitBusy::Scan, "Rescan angestoßen");
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
        assert!(!app.remove_ui.dialog.as_ref().unwrap().needs_force());
        std::fs::write(wt.join("neu.txt"), "x").unwrap();
        app.confirm_remove_worktree();
        assert_eq!(app.git_busy, GitBusy::Idle, "nichts gestartet");
        assert!(wt.exists());
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
        {
            let d = app.remove_ui.dialog.as_mut().unwrap();
            assert_eq!(d.safety, Some(BranchSafety::MergedIntoDefault));
            d.delete_branch = true;
        }
        git(&wt, &["commit", "-q", "--allow-empty", "-m", "neu"]);
        app.confirm_remove_worktree();
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
            .send(RemoveDone {
                path: PathBuf::from("/x/else"),
                result: Ok(()),
            })
            .unwrap();
        app.poll_remove_worktree(&egui::Context::default());
        assert!(app.remove_ui.dialog.is_some());
    }
}
