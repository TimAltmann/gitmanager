//! „Neuer Worktree“ aus der Repo-Liste: Dialog (Repo, Branch, Zielordner),
//! Worker und Ergebnis. Der Zielordner wird aus dem Standard-Ordner der
//! Einstellungen (`AppConfig.worktree_root`, sonst neben dem Repo) und dem
//! Branch vorgeschlagen und lässt sich ändern.

use super::{GitBusy, MyApp};
use crate::git::{self, CheckoutError, RepoInfo, WorktreeBranch};
use crate::gitlab::checkout::safe_folder_name;
use crate::gitlab::remote::RemoteInfo;
use crate::i18n::{tr, tr_fmt};
use crate::ui::settings::review_root_hit;
use egui::{Color32, RichText};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

/// Zustand der Repo-Liste rund um Worktrees: Suchtext, offener Dialog und
/// Kanal für das Ergebnis des Workers.
pub(super) struct WorktreeUi {
    pub search: String,
    pub dialog: Option<NewWorktreeDialog>,
    tx: Sender<WorktreeDone>,
    rx: Receiver<WorktreeDone>,
}

impl Default for WorktreeUi {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            search: String::new(),
            dialog: None,
            tx,
            rx,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BranchMode {
    Existing,
    New,
}

#[derive(Debug, Clone)]
pub(super) struct NewWorktreeDialog {
    pub base: PathBuf,
    pub mode: BranchMode,
    /// Gewählter vorhandener Branch (lokal oder `remote/…`)
    pub existing: String,
    pub filter: String,
    pub new_name: String,
    /// Startpunkt des neuen Branches (leer = HEAD)
    pub start: String,
    pub target: String,
    /// Zielordner von Hand geändert: Vorschlag nicht mehr nachführen
    pub target_edited: bool,
    pub error: Option<String>,
    pub running: bool,
}

impl NewWorktreeDialog {
    fn new(base: &RepoInfo) -> Self {
        Self {
            base: base.path.clone(),
            mode: BranchMode::Existing,
            existing: String::new(),
            filter: String::new(),
            new_name: String::new(),
            start: if base.is_detached {
                String::new()
            } else {
                base.branch.clone()
            },
            target: String::new(),
            target_edited: false,
            error: None,
            running: false,
        }
    }

    /// Lokaler Name des gewählten bzw. neuen Branches (Ordnervorschlag).
    fn local_branch(&self, remotes: &[RemoteInfo]) -> String {
        match self.mode {
            BranchMode::Existing => local_branch_name(&self.existing, remotes),
            BranchMode::New => self.new_name.trim().to_string(),
        }
    }

    fn branch_spec(&self) -> Option<WorktreeBranch> {
        match self.mode {
            BranchMode::Existing => {
                let name = self.existing.trim();
                (!name.is_empty()).then(|| WorktreeBranch::Existing(name.to_string()))
            }
            BranchMode::New => {
                let name = self.new_name.trim();
                (!name.is_empty()).then(|| WorktreeBranch::New {
                    name: name.to_string(),
                    start: Some(self.start.trim().to_string()).filter(|s| !s.is_empty()),
                })
            }
        }
    }
}

struct WorktreeDone {
    target: PathBuf,
    result: Result<String, CheckoutError>,
}

/// `origin/feature/x` → `feature/x`, wenn ein Remote so heißt (längster
/// Remote-Name gewinnt); lokale Branches bleiben unverändert.
/// Gleiche Regel wie beim Anlegen ([`crate::git::strip_remote_prefix`]).
pub(super) fn local_branch_name(branch: &str, remotes: &[RemoteInfo]) -> String {
    crate::git::strip_remote_prefix(branch, remotes.iter().map(|r| r.name.as_str()))
        .unwrap_or(branch)
        .to_string()
}

/// Ordnername `{repo}-{branch}` mit erlaubten Zeichen (`/` → `-`).
pub(super) fn worktree_folder_name(base: &Path, branch: &str) -> String {
    let repo = base
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let repo = safe_folder_name(&repo);
    let branch = safe_folder_name(&branch.replace(['/', '\\'], "-"));
    if branch == "repo" {
        format!("{repo}-worktree")
    } else {
        format!("{repo}-{branch}")
    }
}

/// Vorschlag für den Zielordner: im Standard-Ordner (`root`) bzw. neben dem
/// Basis-Repo; ist der Ordner belegt, `-2`, `-3`, …
pub(super) fn suggest_worktree_path(
    base: &Path,
    branch: &str,
    root: Option<&Path>,
    exists: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    let parent = root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| base.parent().unwrap_or(base).to_path_buf());
    let name = worktree_folder_name(base, branch);
    let first = parent.join(&name);
    if !exists(&first) {
        return first;
    }
    (2..1000)
        .map(|n| parent.join(format!("{name}-{n}")))
        .find(|p| !exists(p))
        .unwrap_or(first)
}

/// Branch ist schon in einem Worktree des Repos ausgecheckt (dort HEAD):
/// Pfad dieses Worktrees.
pub(super) fn checked_out_at<'a>(base: &'a RepoInfo, local: &str) -> Option<&'a Path> {
    if !base.is_detached && base.branch == local {
        return Some(&base.path);
    }
    base.worktrees
        .iter()
        .find(|w| w.branch.as_deref() == Some(local))
        .map(|w| w.path.as_path())
}

impl MyApp {
    /// Dialog öffnen, für `base` oder das erste Repo (kein Worktree).
    pub(super) fn open_new_worktree_dialog(&mut self, base: Option<PathBuf>) {
        let repo = match base {
            Some(base) => self.repos.iter().find(|r| r.path == base),
            None => self.repos.iter().find(|r| !r.is_linked_worktree),
        };
        let Some(repo) = repo else {
            self.set_status(tr(self.config.language, "wt_no_base_repo"));
            return;
        };
        let mut dialog = NewWorktreeDialog::new(repo);
        self.refresh_worktree_suggestion(&mut dialog);
        self.worktree_ui.dialog = Some(dialog);
    }

    fn refresh_worktree_suggestion(&self, dialog: &mut NewWorktreeDialog) {
        if dialog.target_edited {
            return;
        }
        let Some(repo) = self.repos.iter().find(|r| r.path == dialog.base) else {
            return;
        };
        let branch = dialog.local_branch(&repo.remotes);
        let root = self.config.worktree_root_dir();
        let taken: Vec<&Path> = self.repos.iter().map(|r| r.path.as_path()).collect();
        let exists = |p: &Path| p.exists() || taken.contains(&p);
        dialog.target = suggest_worktree_path(&dialog.base, &branch, root.as_deref(), &exists)
            .display()
            .to_string();
    }

    pub(super) fn show_new_worktree_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.worktree_ui.dialog.take() else {
            return;
        };
        let lang = self.config.language;
        let mut open = true;
        let mut create = false;
        let mut cancel = false;
        let mut suggestion_dirty = false;
        let busy = self.git_busy.holds_refs() || self.auto_fetching;
        let bases: Vec<&RepoInfo> = self
            .repos
            .iter()
            .filter(|r| !r.is_linked_worktree)
            .collect();
        let Some(repo) = self.repos.iter().find(|r| r.path == dialog.base) else {
            // Repo nach Rescan verschwunden
            return;
        };

        egui::Window::new(tr(lang, "wt_dialog_title"))
            .id(egui::Id::new("new_worktree_dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.add_enabled_ui(!dialog.running, |ui| {
                    egui::Grid::new("new_worktree_grid")
                        .num_columns(2)
                        .spacing([12.0, 10.0])
                        .show(ui, |ui| {
                            // Repository
                            ui.label(tr(lang, "wt_repo_label"));
                            egui::ComboBox::from_id_salt("new_worktree_base")
                                .selected_text(&repo.name)
                                .width(380.0)
                                .height(320.0)
                                .show_ui(ui, |ui| {
                                    for b in &bases {
                                        let selected = b.path == dialog.base;
                                        let resp = ui
                                            .selectable_label(selected, &b.name)
                                            .on_hover_text(b.path.display().to_string());
                                        if resp.clicked() && !selected {
                                            dialog = NewWorktreeDialog {
                                                target: dialog.target.clone(),
                                                target_edited: dialog.target_edited,
                                                ..NewWorktreeDialog::new(b)
                                            };
                                            suggestion_dirty = true;
                                        }
                                    }
                                });
                            ui.end_row();

                            // Branch-Quelle
                            ui.label(tr(lang, "wt_branch_label"));
                            ui.horizontal(|ui| {
                                if ui
                                    .radio_value(
                                        &mut dialog.mode,
                                        BranchMode::Existing,
                                        tr(lang, "wt_mode_existing"),
                                    )
                                    .changed()
                                    | ui.radio_value(
                                        &mut dialog.mode,
                                        BranchMode::New,
                                        tr(lang, "wt_mode_new"),
                                    )
                                    .changed()
                                {
                                    suggestion_dirty = true;
                                }
                            });
                            ui.end_row();
                        });

                    // Grid-Zeilen wechseln je Modus: Repo kann in der ersten
                    // Zeile gewechselt haben, daher Branches über `dialog.base`
                    let repo = self
                        .repos
                        .iter()
                        .find(|r| r.path == dialog.base)
                        .unwrap_or(repo);
                    ui.add_space(4.0);
                    match dialog.mode {
                        BranchMode::Existing => {
                            if branch_picker(ui, lang, repo, &mut dialog) {
                                suggestion_dirty = true;
                            }
                        }
                        BranchMode::New => {
                            if new_branch_fields(ui, lang, repo, &mut dialog) {
                                suggestion_dirty = true;
                            }
                        }
                    }

                    ui.add_space(8.0);
                    ui.label(tr(lang, "wt_target_label"));
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut dialog.target).desired_width(360.0),
                            )
                            .changed()
                        {
                            dialog.target_edited = true;
                        }
                        if ui.button(format!("{} …", tr(lang, "browse"))).clicked() {
                            let current = PathBuf::from(dialog.target.trim());
                            let mut picker = rfd::FileDialog::new();
                            if let Some(dir) = current.parent().filter(|d| d.is_dir()) {
                                picker = picker.set_directory(dir);
                            }
                            if let Some(parent) = picker.pick_folder() {
                                let branch = dialog.local_branch(&repo.remotes);
                                let name = current
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_else(|| worktree_folder_name(&dialog.base, &branch));
                                dialog.target = parent.join(name).display().to_string();
                                dialog.target_edited = true;
                            }
                        }
                        if dialog.target_edited
                            && ui
                                .button(tr(lang, "wt_target_suggest"))
                                .on_hover_text(tr(lang, "wt_target_suggest_tooltip"))
                                .clicked()
                        {
                            dialog.target_edited = false;
                            suggestion_dirty = true;
                        }
                    });
                    let default_hint = match self.config.worktree_root_dir() {
                        Some(root) => {
                            tr_fmt(lang, "wt_default_root_hint", &[&root.display().to_string()])
                        }
                        None => tr(lang, "wt_default_next_to_repo_hint"),
                    };
                    ui.label(RichText::new(default_hint).size(11.0).weak());
                    let target = PathBuf::from(dialog.target.trim());
                    let parent = target.parent().map(|p| p.display().to_string());
                    let visible = parent.as_deref().is_some_and(|p| {
                        review_root_hit(p, &self.config.roots, self.config.max_depth).is_some()
                    });
                    if !dialog.target.trim().is_empty() && !visible {
                        ui.label(
                            RichText::new(tr(lang, "wt_outside_search_paths"))
                                .size(11.0)
                                .color(Color32::from_rgb(200, 120, 40)),
                        );
                    }
                });

                if let Some(err) = &dialog.error {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(err)
                            .size(11.5)
                            .color(Color32::from_rgb(200, 60, 60)),
                    );
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let ready = dialog.branch_spec().is_some()
                        && Path::new(dialog.target.trim()).is_absolute()
                        && !dialog.running
                        && !busy;
                    if ui
                        .add_enabled(ready, egui::Button::new(tr(lang, "wt_create")))
                        .clicked()
                    {
                        create = true;
                    }
                    if ui.button(tr(lang, "cancel")).clicked() {
                        cancel = true;
                    }
                    if dialog.running {
                        ui.spinner();
                        ui.label(RichText::new(tr(lang, "wt_creating")).size(11.0));
                    } else if busy {
                        ui.label(RichText::new(tr(lang, "wt_wait_busy")).size(11.0).weak());
                    }
                });
            });

        if suggestion_dirty {
            self.refresh_worktree_suggestion(&mut dialog);
        }
        if create {
            self.start_new_worktree(&mut dialog);
        }
        // Während des Laufs bleibt der Dialog offen (Ergebnis schließt ihn)
        if (open && !cancel) || dialog.running {
            self.worktree_ui.dialog = Some(dialog);
        }
    }

    fn start_new_worktree(&mut self, dialog: &mut NewWorktreeDialog) {
        let lang = self.config.language;
        let Some(spec) = dialog.branch_spec() else {
            return;
        };
        let target = PathBuf::from(dialog.target.trim());
        if !target.is_absolute() {
            dialog.error = Some(tr(lang, "wt_target_not_absolute"));
            return;
        }
        if let WorktreeBranch::New { name, .. } = &spec {
            if !git2::Branch::name_is_valid(name).unwrap_or(false) {
                dialog.error = Some(tr_fmt(lang, "wt_invalid_branch", &[name]));
                return;
            }
        }
        if let Some(repo) = self.repos.iter().find(|r| r.path == dialog.base) {
            let local = dialog.local_branch(&repo.remotes);
            if let WorktreeBranch::New { name, .. } = &spec {
                if repo.branches.iter().any(|b| b == name) {
                    dialog.error = Some(tr_fmt(lang, "wt_branch_exists", &[name]));
                    return;
                }
            }
            if let Some(at) = checked_out_at(repo, &local) {
                dialog.error = Some(tr_fmt(
                    lang,
                    "branch_checked_out_elsewhere",
                    &[&local, &at.display().to_string()],
                ));
                return;
            }
        }
        dialog.error = None;
        dialog.running = true;
        self.set_git_busy(GitBusy::Worktree);
        let base = dialog.base.clone();
        let tx = self.worktree_ui.tx.clone();
        std::thread::spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                git::create_worktree(&base, &target, &spec)
            }))
            .unwrap_or_else(|_| {
                eprintln!("Worktree-Thread abgestürzt");
                Err(CheckoutError::Git(
                    "Worktree anlegen abgebrochen".to_string(),
                ))
            });
            let _ = tx.send(WorktreeDone { target, result });
        });
    }

    /// Ergebnis des Workers: Erfolg schließt den Dialog und scannt neu,
    /// ein Fehler bleibt im Dialog stehen.
    pub(super) fn poll_new_worktree(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.worktree_ui.rx.try_recv() {
            let lang = self.config.language;
            if self.git_busy == GitBusy::Worktree {
                self.set_git_busy(GitBusy::Idle);
            }
            match done.result {
                Ok(branch) => {
                    self.worktree_ui.dialog = None;
                    self.set_status(tr_fmt(
                        lang,
                        "wt_created",
                        &[&branch, &done.target.display().to_string()],
                    ));
                    self.start_scan();
                }
                Err(e) => {
                    let text = super::mr_checkout::checkout_error_text(lang, &e);
                    match self.worktree_ui.dialog.as_mut() {
                        Some(dialog) => {
                            dialog.running = false;
                            dialog.error = Some(text);
                        }
                        None => self.error = Some(text),
                    }
                }
            }
            ctx.request_repaint();
        }
        if self.worktree_ui.dialog.as_ref().is_some_and(|d| d.running) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

/// Liste der vorhandenen Branches mit Filter; `true` bei neuer Auswahl.
fn branch_picker(
    ui: &mut egui::Ui,
    lang: crate::i18n::Language,
    repo: &RepoInfo,
    dialog: &mut NewWorktreeDialog,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(tr(lang, "search_label"));
        ui.add(
            egui::TextEdit::singleline(&mut dialog.filter)
                .hint_text(tr(lang, "filter_hint"))
                .desired_width(240.0),
        );
    });
    egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let limit = 500;
                let shown =
                    crate::ui::repo_list::filter_branches(&repo.branches, &dialog.filter, limit);
                if shown.is_empty() {
                    ui.label(RichText::new(tr(lang, "no_matches")).weak().size(11.0));
                }
                for b in shown {
                    let local = local_branch_name(&b, &repo.remotes);
                    let at = checked_out_at(repo, &local);
                    let selected = dialog.existing == b;
                    let resp = ui
                        .add_enabled(at.is_none(), egui::Button::selectable(selected, b.as_str()));
                    let resp = match at {
                        Some(path) => resp.on_disabled_hover_text(tr_fmt(
                            lang,
                            "wt_branch_in_use",
                            &[&path.display().to_string()],
                        )),
                        None => resp,
                    };
                    if resp.clicked() && !selected {
                        dialog.existing = b.clone();
                        changed = true;
                    }
                }
            });
    });
    changed
}

/// Name und Startpunkt eines neuen Branches; `true` bei Namensänderung.
fn new_branch_fields(
    ui: &mut egui::Ui,
    lang: crate::i18n::Language,
    repo: &RepoInfo,
    dialog: &mut NewWorktreeDialog,
) -> bool {
    let mut changed = false;
    egui::Grid::new("new_worktree_new_branch")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr(lang, "wt_new_branch_name"));
            changed = ui
                .add(
                    egui::TextEdit::singleline(&mut dialog.new_name)
                        .hint_text("feature/…")
                        .desired_width(300.0),
                )
                .changed();
            ui.end_row();
            ui.label(tr(lang, "wt_start_point"));
            let head = tr(lang, "wt_start_head");
            let selected = if dialog.start.is_empty() {
                head.clone()
            } else {
                dialog.start.clone()
            };
            egui::ComboBox::from_id_salt("new_worktree_start")
                .selected_text(selected)
                .width(300.0)
                .height(300.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut dialog.start, String::new(), head);
                    for b in &repo.branches {
                        ui.selectable_value(&mut dialog.start, b.clone(), b.as_str());
                    }
                });
            ui.end_row();
        });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::WorktreeHead;

    fn remote(name: &str) -> RemoteInfo {
        RemoteInfo {
            name: name.to_string(),
            url_redacted: String::new(),
            parsed: None,
        }
    }

    #[test]
    fn local_branch_name_strips_remote_prefix() {
        let remotes = [remote("origin"), remote("up/stream")];
        assert_eq!(local_branch_name("origin/feature/x", &remotes), "feature/x");
        assert_eq!(local_branch_name("up/stream/main", &remotes), "main");
        assert_eq!(local_branch_name("feature/x", &remotes), "feature/x");
        // Kein Remote „feature“: bleibt lokal
        assert_eq!(
            local_branch_name("feature/origin/x", &remotes),
            "feature/origin/x"
        );
        assert_eq!(local_branch_name("origin/", &remotes), "origin/");
    }

    #[test]
    fn folder_name_is_safe() {
        let base = Path::new("/code/backend-api");
        assert_eq!(
            worktree_folder_name(base, "feature/login"),
            "backend-api-feature-login"
        );
        assert_eq!(worktree_folder_name(base, "fix/ä&;x"), "backend-api-fix-x");
        assert_eq!(worktree_folder_name(base, ""), "backend-api-worktree");
        assert_eq!(worktree_folder_name(base, ".."), "backend-api-worktree");
    }

    #[test]
    fn suggestion_uses_root_or_parent_and_skips_taken() {
        let base = Path::new("/code/app");
        let none = |_: &Path| false;
        assert_eq!(
            suggest_worktree_path(base, "feature/a", None, &none),
            PathBuf::from("/code/app-feature-a")
        );
        assert_eq!(
            suggest_worktree_path(base, "feature/a", Some(Path::new("/wt")), &none),
            PathBuf::from("/wt/app-feature-a")
        );
        let taken =
            |p: &Path| p == Path::new("/wt/app-feature-a") || p == Path::new("/wt/app-feature-a-2");
        assert_eq!(
            suggest_worktree_path(base, "feature/a", Some(Path::new("/wt")), &taken),
            PathBuf::from("/wt/app-feature-a-3")
        );
    }

    fn repo_on(branch: &str) -> RepoInfo {
        let mut repo = RepoInfo::new(PathBuf::from("/code/app"), branch.to_string(), false, false);
        repo.worktrees = vec![WorktreeHead {
            path: PathBuf::from("/code/app-feature-a"),
            branch: Some("feature/a".to_string()),
        }];
        repo
    }

    #[test]
    fn checked_out_branches_are_detected() {
        let repo = repo_on("main");
        assert_eq!(checked_out_at(&repo, "main"), Some(Path::new("/code/app")));
        assert_eq!(
            checked_out_at(&repo, "feature/a"),
            Some(Path::new("/code/app-feature-a"))
        );
        assert_eq!(checked_out_at(&repo, "feature/b"), None);
        let mut detached = repo_on("abc1234");
        detached.is_detached = true;
        assert_eq!(checked_out_at(&detached, "abc1234"), None);
    }

    #[test]
    fn dialog_branch_spec_per_mode() {
        let repo = repo_on("main");
        let mut d = NewWorktreeDialog::new(&repo);
        assert_eq!(d.start, "main");
        assert_eq!(d.branch_spec(), None);
        d.existing = "origin/feature/b".to_string();
        assert_eq!(
            d.branch_spec(),
            Some(WorktreeBranch::Existing("origin/feature/b".to_string()))
        );
        assert_eq!(d.local_branch(&[remote("origin")]), "feature/b");
        d.mode = BranchMode::New;
        assert_eq!(d.branch_spec(), None);
        d.new_name = " feature/c ".to_string();
        d.start = String::new();
        assert_eq!(
            d.branch_spec(),
            Some(WorktreeBranch::New {
                name: "feature/c".to_string(),
                start: None
            })
        );
    }
}
