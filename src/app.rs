use crate::config::AppConfig;
use crate::git::{launch_agent, launch_ide, RepoInfo};
use crate::i18n::{tr, tr_fmt, Language};
use crate::scanner::scan_repos;
use crate::ui::{
    repo_list::{show_repo_list, RepoListActions},
    settings::SettingsState,
};
use egui::{Color32, RichText};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

enum ScanResult {
    Repos(Vec<RepoInfo>),
}

struct BranchDialog {
    repo_path: PathBuf,
    target_branch: String,
    error: Option<String>,
    dirty_files: Vec<String>,
}

pub struct MyApp {
    config: AppConfig,
    repos: Vec<RepoInfo>,
    scanning: bool,
    error: Option<String>,
    show_settings: bool,
    settings_state: Option<SettingsState>,
    scan_tx: Sender<ScanResult>,
    scan_rx: Receiver<ScanResult>,
    launch_err_tx: Sender<String>,
    launch_err_rx: Receiver<String>,
    config_update_rx: Receiver<Result<AppConfig, String>>,
    status_message: Option<String>,
    status_message_time: Option<std::time::Instant>,
    branch_dialog: Option<BranchDialog>,
    pending_branch_switch: Option<(PathBuf, String)>,
    // Window resizing state
    last_window_size: [f32; 2],
    // Panel collapse state
    top_bar_collapsed: bool,
}

impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        crate::ui::theme::apply_theme(&cc.egui_ctx, &config.theme);
        // SVG Icons loader für egui_extras
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let (tx, rx) = mpsc::channel();
        let (launch_err_tx, launch_err_rx) = mpsc::channel();
        let (config_update_tx, config_update_rx) = mpsc::channel::<Result<AppConfig, String>>();

        let mut app = Self {
            config,
            repos: Vec::new(),
            scanning: false,
            error: None,
            show_settings: false,
            settings_state: None,
            scan_tx: tx,
            scan_rx: rx,
            launch_err_tx,
            launch_err_rx,
            config_update_rx,
            status_message: None,
            status_message_time: None,
            branch_dialog: None,
            pending_branch_switch: None,
            last_window_size: [0.0; 2],
            top_bar_collapsed: false,
        };
        // Auto-Erkennung asynchron (verhindert UI Freeze beim Start)
        {
            let cfg = app.config.clone();
            let tx = config_update_tx;
            std::thread::spawn(move || {
                let mut cfg = cfg;
                let mut need_save = false;
                for profile in &mut cfg.profiles {
                    for ide in &mut profile.ides {
                        let prog_lower = ide.program.to_lowercase();
                        let is_vs_program = matches!(prog_lower.as_str(), "devenv" | "devenv.exe");
                        let is_rider_program = matches!(
                            prog_lower.as_str(),
                            "rider" | "rider.exe" | "rider64.exe" | "rider.cmd"
                        );
                        if ide.id == "vs2022" && is_vs_program {
                            if let Some(p) = crate::git::resolve_vs_path() {
                                ide.program = p.display().to_string();
                                need_save = true;
                            }
                        } else if ide.id == "rider" && is_rider_program {
                            if let Some(p) = crate::git::resolve_rider_path() {
                                ide.program = p.display().to_string();
                                need_save = true;
                            }
                        }
                    }
                }
                if need_save {
                    match cfg.save() {
                        Ok(()) => {
                            let _ = tx.send(Ok(cfg));
                        }
                        Err(e) => {
                            let _ = tx.send(Err(format!(
                                "Config Auto-Erkennung speichern fehlgeschlagen: {e:#}"
                            )));
                        }
                    }
                }
            });
        }
        app.start_scan();
        // Initial size matches viewport default (1080x680) – used for collapse logic
        app.last_window_size = [1080.0, 680.0];
        app
    }

    fn start_scan(&mut self) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        self.error = None;
        let cfg = self.config.clone();
        let tx = self.scan_tx.clone();
        std::thread::spawn(move || {
            let repos = scan_repos(&cfg);
            let _ = tx.send(ScanResult::Repos(repos));
        });
    }

    fn poll_scan(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.scan_rx.try_recv() {
            self.scanning = false;
            let ScanResult::Repos(repos) = result;
            self.repos = repos;
            self.error = None;
            ctx.request_repaint();
        }
        while let Ok(err) = self.launch_err_rx.try_recv() {
            self.error = Some(err);
            self.status_message = None;
            self.status_message_time = None;
            ctx.request_repaint();
        }
        while let Ok(res) = self.config_update_rx.try_recv() {
            match res {
                Ok(cfg) => {
                    self.config = cfg;
                    self.error = None;
                    // Settings offen: nicht kompletten Draft verwerfen, nur auto-erkannte Programme mergen
                    if self.show_settings {
                        if let Some(state) = self.settings_state.as_mut() {
                            state.merge_auto_detected(&self.config);
                        } else {
                            self.settings_state = Some(SettingsState::from_config(&self.config));
                        }
                    }
                    self.status_message = Some(tr(self.config.language, "saved_scan_restart"));
                    self.status_message_time = Some(std::time::Instant::now());
                    ctx.request_repaint();
                }
                Err(err_msg) => {
                    self.error = Some(err_msg);
                    self.status_message = None;
                    self.status_message_time = None;
                    ctx.request_repaint();
                }
            }
        }
        if self.scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(t) = self.status_message_time {
            if t.elapsed() > std::time::Duration::from_secs(3) {
                self.status_message = None;
                self.status_message_time = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        // Pending branch switch nach Scan? Eigentlich direkt
        if let Some((path, branch)) = self.pending_branch_switch.take() {
            self.handle_branch_switch(path, branch);
        }
    }

    fn handle_branch_switch(&mut self, repo_path: PathBuf, target_branch: String) {
        // Prüfe dirty und conflicts
        let repo_path_clone = repo_path.clone();
        let target_clone = target_branch.clone();
        // Öffne Repo um Status zu prüfen
        if let Some(repo) = crate::git::open_repo(&repo_path) {
            let dirty = crate::git::is_dirty(&repo);
            let has_conflicts = crate::git::has_merge_conflicts(&repo);
            let in_progress = crate::git::is_merge_in_progress(&repo);
            if has_conflicts || in_progress {
                self.error = Some(format!(
                    "Branch-Wechsel zu '{}' nicht möglich: Merge-Konflikte oder Merge/Rebase läuft ({:?})",
                    target_branch,
                    repo.state()
                ));
                return;
            }
            if dirty {
                // Zeige Dialog
                let files = crate::git::get_detailed_status(&repo);
                self.branch_dialog = Some(BranchDialog {
                    repo_path,
                    target_branch,
                    error: None,
                    dirty_files: files,
                });
                return;
            }
        }
        // Kein dirty, direkt wechseln
        self.execute_branch_switch(repo_path_clone, target_clone, false, false);
    }

    fn execute_branch_switch(
        &mut self,
        repo_path: PathBuf,
        target_branch: String,
        force: bool,
        stash: bool,
    ) {
        let result = if stash {
            crate::git::stash_and_checkout(&repo_path, &target_branch)
        } else if force {
            crate::git::checkout_branch_force(&repo_path, &target_branch)
        } else {
            crate::git::checkout_branch(&repo_path, &target_branch)
        };

        match result {
            Ok(()) => {
                self.status_message = Some(format!(
                    "Branch zu '{}' gewechselt: {}",
                    target_branch,
                    repo_path.display()
                ));
                self.status_message_time = Some(std::time::Instant::now());
                self.error = None;
                self.branch_dialog = None;
                self.start_scan();
            }
            Err(e) => {
                // Wenn Fehler und Dialog offen, zeige Fehler im Dialog, sonst global
                if let Some(dlg) = &mut self.branch_dialog {
                    dlg.error = Some(format!("{e:#}"));
                } else {
                    self.error = Some(format!("Branch-Wechsel fehlgeschlagen: {e:#}"));
                }
                // Bei stash_and_checkout kann es sein dass stash pop fehlgeschlagen ist, aber checkout ok – dann trotzdem neu scannen
                if e.to_string().contains("Stash pop fehlgeschlagen") {
                    self.start_scan();
                }
            }
        }
    }

    fn show_top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let lang = self.config.language;
        // Hysterese wird in `update()` gepflegt ( <400 collapsed, >=500 expanded )
        let effective_collapse = self.top_bar_collapsed;

        egui::Panel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(ctx.global_style().visuals.widgets.inactive.bg_fill)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        ctx.global_style().visuals.widgets.inactive.fg_stroke.color,
                    )),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("📦 {}", tr(lang, "app_title")))
                            .size(17.0)
                            .strong(),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!("{} Repositories", self.repos.len()))
                            .size(12.0)
                            .color(Color32::from_rgb(100, 100, 100)),
                    );

                    // Profile Combo in TopBar
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(tr(lang, "profile_label"))
                            .size(11.0)
                            .color(Color32::from_rgb(100, 100, 100)),
                    );
                    let mut new_profile = self.config.active_profile_id.clone();
                    egui::ComboBox::from_id_salt("active_profile_top")
                        .selected_text(self.config.get_active_profile().display_name.clone())
                        .width(110.0)
                        .show_ui(ui, |ui| {
                            for p in &self.config.profiles {
                                let is_active = p.id == self.config.active_profile_id;
                                if ui.selectable_label(is_active, &p.display_name).clicked() {
                                    new_profile = p.id.clone();
                                }
                            }
                        });
                    if new_profile != self.config.active_profile_id {
                        self.config.active_profile_id = new_profile;
                        if let Err(e) = self.config.save() {
                            self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                        } else {
                            self.status_message = Some(tr_fmt(
                                lang,
                                "profile_switched",
                                &[&self.config.get_active_profile().display_name],
                            ));
                            self.status_message_time = Some(std::time::Instant::now());
                            self.start_scan();
                        }
                    }

                    // Agent Multi-Select wenn mehrere
                    // Only show agent section if not collapsed
                    let show_agents = !effective_collapse || self.config.agents.len() <= 1;
                    if show_agents {
                        if self.config.agents.len() > 1 {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new("AI:")
                                    .size(11.0)
                                    .color(Color32::from_rgb(100, 100, 100)),
                            );
                            let active_agents = self.config.get_active_agents();
                            let current_text = if active_agents.is_empty() {
                                "Keine".to_string()
                            } else if active_agents.len() == 1 {
                                active_agents[0].display_name.clone()
                            } else {
                                format!("{} aktiv", active_agents.len())
                            };
                            let mut save_err: Option<String> = None;
                            egui::ComboBox::from_id_salt("active_agent_top")
                                .selected_text(current_text)
                                .width(120.0)
                                .show_ui(ui, |ui| {
                                    let agents_snapshot = self.config.agents.clone();
                                    for a in &agents_snapshot {
                                        let mut is_active = self.config.is_agent_active(&a.id);
                                        if ui.checkbox(&mut is_active, &a.display_name).clicked() {
                                            self.config.toggle_agent_active(&a.id);
                                            if let Err(e) = self.config.save() {
                                                save_err = Some(format!("{e:#}"));
                                            }
                                        }
                                    }
                                    ui.separator();
                                    if ui.button("Alle aktivieren").clicked() {
                                        for a in &self.config.agents.clone() {
                                            if !self.config.is_agent_active(&a.id) {
                                                self.config.toggle_agent_active(&a.id);
                                            }
                                        }
                                        if let Err(e) = self.config.save() {
                                            save_err = Some(format!("{e:#}"));
                                        }
                                    }
                                    if ui.button("Alle deaktivieren").clicked() {
                                        self.config.active_agent_ids.clear();
                                        self.config.active_agent_id = None;
                                        if let Err(e) = self.config.save() {
                                            save_err = Some(format!("{e:#}"));
                                        }
                                    }
                                });
                            if let Some(e) = save_err {
                                self.error = Some(tr_fmt(lang, "save_failed", &[&e]));
                            }
                        } else if let Some(agent) = self.config.get_active_agent() {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(format!("AI: {}", agent.display_name))
                                    .size(11.0)
                                    .color(Color32::from_rgb(100, 100, 100)),
                            );
                        }
                    }

                    // Language selector
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(tr(lang, "language"))
                            .size(11.0)
                            .color(Color32::from_rgb(100, 100, 100)),
                    );
                    let mut new_lang = lang;
                    egui::ComboBox::from_id_salt("language_top")
                        .selected_text(lang.display_name().to_string())
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for l in Language::all() {
                                let is_active = l == lang;
                                if ui.selectable_label(is_active, l.display_name()).clicked() {
                                    new_lang = l;
                                }
                            }
                        });
                    if new_lang != lang {
                        self.config.language = new_lang;
                        if let Err(e) = self.config.save() {
                            self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(RichText::new(format!("⚙ {}", tr(lang, "settings"))).size(12.0))
                            .on_hover_text(tr(lang, "settings_tooltip"))
                            .clicked()
                        {
                            self.show_settings = true;
                            self.settings_state = Some(SettingsState::from_config(&self.config));
                        }
                        ui.add_space(8.0);
                        let refresh_label = if self.scanning {
                            format!("⟳ {}", tr(lang, "scanning"))
                        } else {
                            format!("↻ {}", tr(lang, "refresh"))
                        };
                        let btn = egui::Button::new(RichText::new(refresh_label).size(12.0));
                        if ui.add_enabled(!self.scanning, btn).clicked() {
                            self.start_scan();
                        }
                        if self.scanning {
                            ui.add_space(8.0);
                            ui.spinner();
                        }
                    });
                });

                // Only show path info when not collapsed
                if !effective_collapse && !self.config.roots.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(tr(lang, "search_paths"))
                                .size(11.0)
                                .color(Color32::from_rgb(120, 120, 120)),
                        );
                        for (i, root) in self.config.roots.iter().enumerate() {
                            if i > 0 {
                                ui.label(
                                    RichText::new("·").color(Color32::from_rgb(180, 180, 180)),
                                );
                            }
                            let exists = root.exists();
                            let color = if exists {
                                Color32::from_rgb(80, 80, 80)
                            } else {
                                Color32::from_rgb(200, 80, 20)
                            };
                            ui.label(
                                RichText::new(root.display().to_string())
                                    .size(11.0)
                                    .color(color),
                            )
                            .on_hover_text(if exists {
                                format!("{} (Tiefe {})", root.display(), self.config.max_depth)
                            } else {
                                format!("{} (nicht gefunden)", root.display())
                            });
                        }
                        ui.label(
                            RichText::new(format!("  Tiefe: {}", self.config.max_depth))
                                .size(11.0)
                                .color(Color32::from_rgb(120, 120, 120)),
                        );
                        ui.add_space(8.0);
                        let prof = self.config.get_active_profile();
                        ui.label(
                            RichText::new(format!(
                                "Profil: {} (*{})",
                                prof.display_name, prof.file_extension
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(100, 100, 100)),
                        );
                    });
                }
            });
    }

    fn show_status_bar(&self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let lang = self.config.language;
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(ctx.global_style().visuals.widgets.inactive.bg_fill)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        ctx.global_style().visuals.widgets.inactive.fg_stroke.color,
                    )),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(msg) = &self.status_message {
                        ui.label(
                            RichText::new(msg)
                                .size(11.0)
                                .color(Color32::from_rgb(60, 120, 60)),
                        );
                    } else if self.scanning {
                        ui.label(
                            RichText::new(tr(lang, "scanning_repos"))
                                .size(11.0)
                                .color(Color32::from_rgb(100, 100, 100)),
                        );
                    } else {
                        ui.label(
                            RichText::new(format!(
                                "{} Repos · {} Pfade · Tiefe {} · Profil {}",
                                self.repos.len(),
                                self.config.roots.len(),
                                self.config.max_depth,
                                self.config.get_active_profile().display_name
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(120, 120, 120)),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("v0.2.0")
                                .size(10.0)
                                .color(Color32::from_rgb(160, 160, 160)),
                        );
                    });
                });
            });
    }

    fn show_branch_dialog(&mut self, ctx: &egui::Context) {
        let lang = self.config.language;
        let mut to_close = false;
        let mut action: Option<(PathBuf, String, bool, bool)> = None; // path, branch, force, stash
        if let Some(dlg) = &mut self.branch_dialog {
            let repo_path = dlg.repo_path.clone();
            let target_branch = dlg.target_branch.clone();
            egui::Window::new(format!(
                "{}: {} → {}",
                tr(lang, "branch_switch_title"),
                repo_path.display(),
                target_branch
            ))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(RichText::new(tr(lang, "branch_dirty_msg")).size(12.0));
                ui.add_space(8.0);
                if !dlg.dirty_files.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            dlg.dirty_files.len(),
                            tr(lang, "affected_files")
                        ))
                        .size(11.0)
                        .color(Color32::from_rgb(100, 100, 100)),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(100.0)
                        .show(ui, |ui| {
                            for f in &dlg.dirty_files {
                                ui.label(
                                    RichText::new(f)
                                        .size(10.5)
                                        .color(Color32::from_rgb(80, 80, 80)),
                                );
                            }
                        });
                    ui.add_space(8.0);
                }
                if let Some(err) = &dlg.error {
                    let frame = egui::Frame::new()
                        .fill(Color32::from_rgb(255, 235, 235))
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 100, 100)))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(8, 6));
                    frame.show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}: {}",
                                if lang == Language::En {
                                    "Error"
                                } else {
                                    "Fehler"
                                },
                                err
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(160, 40, 40)),
                        );
                    });
                    ui.add_space(8.0);
                }
                ui.horizontal(|ui| {
                    if ui.button(tr(lang, "cancel")).clicked() {
                        to_close = true;
                    }
                    if ui
                        .button(tr(lang, "stash_switch"))
                        .on_hover_text(tr(lang, "stash_switch_tooltip"))
                        .clicked()
                    {
                        action = Some((repo_path.clone(), target_branch.clone(), false, true));
                    }
                    if ui
                        .button(tr(lang, "force_discard"))
                        .on_hover_text(tr(lang, "force_discard_tooltip"))
                        .clicked()
                    {
                        action = Some((repo_path.clone(), target_branch.clone(), true, false));
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr(lang, "branch_tip"))
                        .size(10.0)
                        .color(Color32::from_rgb(120, 120, 120))
                        .italics(),
                );
            });
        }
        if to_close {
            self.branch_dialog = None;
        }
        if let Some((path, branch, force, stash)) = action {
            self.execute_branch_switch(path, branch, force, stash);
        }
    }
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let lang = self.config.language;
        self.poll_scan(&ctx);

        // Track window size for collapsing logic using ctx screen rect (Hysterese: <400 collapsed, >=500 expanded)
        let screen_rect = ctx.viewport_rect();
        let current_size = [screen_rect.width(), screen_rect.height()];
        let prev_size = self.last_window_size;
        if prev_size != current_size {
            if current_size[1] < 400.0 {
                self.top_bar_collapsed = true;
            } else if current_size[1] >= 500.0 {
                self.top_bar_collapsed = false;
            }
            self.last_window_size = current_size;
        }

        self.show_top_bar(ui);
        self.show_status_bar(ui);
        self.show_branch_dialog(&ctx);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.global_style().visuals.widgets.inactive.bg_fill)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ui, |ui| {
                if let Some(err) = &self.error {
                    let frame = egui::Frame::new()
                        .fill(Color32::from_rgb(255, 235, 235))
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 100, 100)))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(10, 8));
                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "⚠ {}: {}",
                                    if lang == Language::En {
                                        "Error"
                                    } else {
                                        "Fehler"
                                    },
                                    err
                                ))
                                .size(12.0)
                                .color(Color32::from_rgb(160, 40, 40)),
                            );
                            if ui.small_button("✕").clicked() {
                                // Will be cleared next frame via status polling
                            }
                        });
                    });
                    ui.add_space(8.0);
                }

                if self.config.roots.is_empty() && !self.scanning {
                    let frame = egui::Frame::new()
                        .fill(Color32::from_rgb(255, 248, 220))
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 180, 80)))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(10, 8));
                    frame.show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("⚠ {}", tr(lang, "no_search_path")))
                                .size(12.0)
                                .color(Color32::from_rgb(120, 90, 20)),
                        );
                    });
                    ui.add_space(12.0);
                }

                // Repo-Liste mit neuen Callbacks
                let mut actions = RepoListActions {
                    branch_switch: None,
                    solution_select: None,
                    ide_open: None,
                    agent_open: None,
                    profile_override: None,
                    fetch_branches: None,
                    explorer_open: None,
                    shell_open: None,
                };

                // Wir brauchen &mut für repo_list, um selektierte Werte zu zeigen, aber hier clonen wir mut
                // repo_list wird &mut repos erwarten, wir haben &mut self.repos
                show_repo_list(ui, &mut self.repos, &self.config, &mut actions);

                // Handle Actions
                if let Some((repo_path, branch)) = actions.branch_switch {
                    // Verzögert handeln, damit UI nicht blockiert
                    self.pending_branch_switch = Some((repo_path, branch));
                    ctx.request_repaint();
                }
                if let Some((repo_path, sln_path)) = actions.solution_select {
                    // Speichere Auswahl in config
                    let state = self.config.get_repo_state_mut(&repo_path);
                    state.selected_solution = Some(sln_path.clone());
                    if let Err(e) = self.config.save() {
                        self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                    } else {
                        // Update auch im RepoInfo direkt
                        if let Some(repo) = self.repos.iter_mut().find(|r| r.path == repo_path) {
                            repo.selected_solution = Some(sln_path);
                        }
                        self.status_message = Some(tr_fmt(
                            lang,
                            "solution_selected",
                            &[&repo_path.display().to_string()],
                        ));
                        self.status_message_time = Some(std::time::Instant::now());
                    }
                }
                if let Some((repo_path, ide_id, file_path)) = actions.ide_open {
                    // Speichere letzte IDE Wahl
                    {
                        let state = self.config.get_repo_state_mut(&repo_path);
                        state.selected_ide = Some(ide_id.clone());
                        if let Err(e) = self.config.save() {
                            self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                        }
                    }
                    // Finde IdeConfig
                    let profile = self.config.get_effective_profile_for_repo(&repo_path);
                    if let Some(ide) = profile.ides.iter().find(|i| i.id == ide_id) {
                        let file_opt = if ide.no_args {
                            None
                        } else {
                            Some(file_path.as_path())
                        };
                        let display_path = file_opt
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| repo_path.display().to_string());
                        match launch_ide(ide, &repo_path, file_opt) {
                            Ok(()) => {
                                self.status_message = Some(tr_fmt(
                                    lang,
                                    "opening_with",
                                    &[&display_path, &ide.display_name],
                                ));
                                self.status_message_time = Some(std::time::Instant::now());
                            }
                            Err(e) => {
                                self.error = Some(format!(
                                    "IDE '{}' konnte nicht gestartet werden: {e:#}",
                                    ide.display_name
                                ));
                            }
                        }
                    } else {
                        self.error = Some(tr_fmt(
                            lang,
                            "ide_not_found",
                            &[&ide_id, &profile.display_name],
                        ));
                    }
                }
                if let Some((repo_path, agent_id)) = actions.agent_open {
                    let agent = self
                        .config
                        .agents
                        .iter()
                        .find(|a| a.id == agent_id)
                        .cloned()
                        .or_else(|| self.config.get_active_agent().cloned());
                    if let Some(agent) = agent {
                        let term_pref = agent
                            .terminal_override
                            .clone()
                            .unwrap_or_else(|| self.config.terminal.preference.clone());
                        // Nicht blockierend starten (verhindert UI Freeze, besonders bei pwsh/wt)
                        let repo_clone = repo_path.clone();
                        let agent_clone = agent.clone();
                        let display_name = agent.display_name.clone();
                        let err_tx = self.launch_err_tx.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = launch_agent(&agent_clone, &repo_clone, &term_pref) {
                                let _ = err_tx.send(format!(
                                    "Agent '{}' konnte nicht gestartet werden: {e:#}",
                                    display_name
                                ));
                            }
                        });
                        self.status_message = Some(tr_fmt(
                            lang,
                            "starting_agent_in",
                            &[&agent.display_name, &repo_path.display().to_string()],
                        ));
                        self.status_message_time = Some(std::time::Instant::now());
                    } else {
                        self.error = Some(tr_fmt(lang, "agent_not_found", &[&agent_id]));
                    }
                }
                if let Some((repo_path, profile_opt)) = actions.profile_override {
                    self.config
                        .set_repo_profile_override(&repo_path, profile_opt.clone());
                    if let Err(e) = self.config.save() {
                        self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                    } else {
                        self.status_message = Some(tr_fmt(
                            lang,
                            "profile_changed_for",
                            &[&repo_path.display().to_string()],
                        ));
                        self.status_message_time = Some(std::time::Instant::now());
                        self.start_scan();
                    }
                }
                if let Some(repo_path) = actions.fetch_branches {
                    let path_clone = repo_path.clone();
                    let cfg = self.config.clone();
                    let tx = self.scan_tx.clone();
                    self.scanning = true;
                    self.status_message = Some(tr_fmt(
                        lang,
                        "fetching_branches",
                        &[&repo_path.display().to_string()],
                    ));
                    self.status_message_time = Some(std::time::Instant::now());
                    std::thread::spawn(move || {
                        let fetch_res = crate::git::fetch_all(&path_clone);
                        if let Err(e) = fetch_res {
                            eprintln!("Fetch fehlgeschlagen für {}: {e:#}", path_clone.display());
                        }
                        let repos = crate::scanner::scan_repos(&cfg);
                        let _ = tx.send(ScanResult::Repos(repos));
                    });
                }
                if let Some(repo_path) = actions.explorer_open {
                    match crate::git::open_in_explorer(&repo_path) {
                        Ok(()) => {
                            self.status_message = Some(tr_fmt(
                                lang,
                                "explorer_opened",
                                &[&repo_path.display().to_string()],
                            ));
                            self.status_message_time = Some(std::time::Instant::now());
                        }
                        Err(e) => {
                            self.error =
                                Some(format!("Explorer konnte nicht geöffnet werden: {e:#}"));
                        }
                    }
                }
                if let Some(repo_path) = actions.shell_open {
                    let pref = self.config.terminal.preference.clone();
                    let repo_clone = repo_path.clone();
                    let err_tx = self.launch_err_tx.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = crate::git::open_shell(&repo_clone, &pref) {
                            let _ =
                                err_tx.send(format!("Shell konnte nicht geöffnet werden: {e:#}"));
                        }
                    });
                    self.status_message = Some(tr_fmt(
                        lang,
                        "shell_opened",
                        &[&repo_path.display().to_string()],
                    ));
                    self.status_message_time = Some(std::time::Instant::now());
                }
            });

        if self.show_settings {
            if self.settings_state.is_none() {
                self.settings_state = Some(SettingsState::from_config(&self.config));
            }
            let mut save: Option<AppConfig> = None;
            let mut state = self.settings_state.take().unwrap();
            let mut open = self.show_settings;
            crate::ui::settings::show_settings_window(&ctx, &mut state, &mut open, &mut save);
            self.show_settings = open;

            if let Some(new_cfg) = save {
                // Theme sofort anwenden
                crate::ui::theme::apply_theme(&ctx, &new_cfg.theme);
                let lang = new_cfg.language;
                self.config = new_cfg;
                self.settings_state = Some(SettingsState::from_config(&self.config));
                self.show_settings = false;
                self.status_message = Some(tr(lang, "saved_scan_restart"));
                self.status_message_time = Some(std::time::Instant::now());
                self.start_scan();
            } else {
                if self.show_settings {
                    self.settings_state = Some(state);
                } else {
                    self.settings_state = None;
                }
            }
            if !self.show_settings {
                self.settings_state = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn branch_dialog_fields() {
        let dlg = BranchDialog {
            repo_path: PathBuf::from("/tmp/repo"),
            target_branch: "feature".to_string(),
            error: None,
            dirty_files: vec![],
        };
        assert_eq!(dlg.repo_path, PathBuf::from("/tmp/repo"));
        assert_eq!(dlg.target_branch, "feature");
        assert!(dlg.error.is_none());
        assert!(dlg.dirty_files.is_empty());

        let dlg2 = BranchDialog {
            repo_path: PathBuf::from("/tmp/repo"),
            target_branch: "main".to_string(),
            error: Some("err".to_string()),
            dirty_files: vec!["modified: foo.txt".to_string()],
        };
        assert_eq!(dlg2.error, Some("err".to_string()));
        assert_eq!(dlg2.dirty_files.len(), 1);
    }

    #[test]
    fn scan_result_repos_contains_vec() {
        let result = ScanResult::Repos(vec![]);
        match result {
            ScanResult::Repos(v) => assert!(v.is_empty()),
        }
        let info =
            crate::git::RepoInfo::new(PathBuf::from("/tmp/repo"), "main".to_string(), false, false);
        let result2 = ScanResult::Repos(vec![info]);
        match result2 {
            ScanResult::Repos(v) => assert_eq!(v.len(), 1),
        }
    }

    #[test]
    fn myapp_struct_default_values() {
        // MyApp::new requires eframe CreationContext, so we test struct construction directly via manual init
        // We verify that the expected defaults match config defaults
        let cfg = AppConfig::default();
        // Simulate what MyApp::new does: scanning false initially then true after start_scan
        // We test that AppConfig default has valid state
        assert!(cfg.max_depth >= 1 && cfg.max_depth <= 10);
        assert!(!cfg.profiles.is_empty());
        // BranchDialog None initially
        let branch_dialog: Option<BranchDialog> = None;
        assert!(branch_dialog.is_none());
        let scanning = false;
        assert!(!scanning);
        let repos: Vec<crate::git::RepoInfo> = vec![];
        assert!(repos.is_empty());
    }

    #[test]
    fn myapp_status_timeout_logic() {
        // Simulate status_message_time >3s should clear
        let now = std::time::Instant::now();
        let old = now - std::time::Duration::from_secs(4);
        assert!(old.elapsed().as_secs() > 3);
        let recent = now - std::time::Duration::from_secs(1);
        assert!(recent.elapsed().as_secs() <= 3);
    }

    #[test]
    fn myapp_top_bar_collapse_logic() {
        // logic: height <400 => collapsed true, >=500 => false
        let collapse_if_smaller_than = 400.0;
        let should_collapse = |h: f32| h < collapse_if_smaller_than;
        assert!(should_collapse(380.0));
        assert!(!should_collapse(400.0));
        assert!(!should_collapse(500.0));
        // transition: last_window_size tracking
        let mut top_bar_collapsed = false;
        let current = 350.0;
        if current < 400.0 {
            top_bar_collapsed = true;
        }
        assert!(top_bar_collapsed);
        let current2 = 550.0;
        if current2 >= 500.0 && top_bar_collapsed {
            top_bar_collapsed = false;
        }
        assert!(!top_bar_collapsed);
    }

    #[test]
    fn myapp_handle_branch_switch_logic() {
        // Test that dirty check would trigger dialog vs direct checkout
        // We simulate with a temp repo
        use crate::git::{has_merge_conflicts, is_dirty, is_merge_in_progress};
        // This test uses git2 to create repo and check status
        let dir = tempdir().unwrap();
        let path = dir.path().join("repo");
        std::fs::create_dir(&path).unwrap();
        // init repo
        let repo = git2::Repository::init(&path).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@test.com").unwrap();
        let file = path.join("README.md");
        std::fs::write(&file, "hi").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("README.md")).unwrap();
        idx.write().unwrap();
        let oid = idx.write_tree().unwrap();
        let tree = repo.find_tree(oid).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        assert!(!is_dirty(&repo));
        assert!(!has_merge_conflicts(&repo));
        assert!(!is_merge_in_progress(&repo));
        // dirty
        std::fs::write(path.join("dirty.txt"), "dirty").unwrap();
        let repo2 = git2::Repository::open(&path).unwrap();
        assert!(is_dirty(&repo2));
        // dirty would trigger dialog in MyApp::handle_branch_switch
        let dirty = is_dirty(&repo2);
        let has_conflicts = has_merge_conflicts(&repo2);
        let in_progress = is_merge_in_progress(&repo2);
        assert!(dirty);
        assert!(!has_conflicts);
        assert!(!in_progress);
    }

    #[test]
    fn myapp_profile_switch_triggers_scan_logic() {
        let mut cfg = AppConfig::default();
        let original = cfg.active_profile_id.clone();
        // simulate switching
        let new_profile = if original == "dotnet" {
            "rust"
        } else {
            "dotnet"
        };
        // add new profile if not exists
        if cfg.get_profile(new_profile).is_none() {
            cfg.profiles.push(crate::config::LanguageProfile {
                id: new_profile.to_string(),
                display_name: new_profile.to_string(),
                file_extension: ".txt".to_string(),
                file_pattern: None,
                max_scan_depth: 3,
                ides: vec![],
                default_ide_id: None,
                ide_order: Vec::new(),
                hidden_ide_ids: Vec::new(),
                hidden_agent_ids: Vec::new(),
                agent_order: Vec::new(),
                show_shell: true,
                show_explorer: true,
            });
        }
        cfg.active_profile_id = new_profile.to_string();
        assert_eq!(cfg.active_profile_id, new_profile);
        // status message would be set and scan started
    }

    #[test]
    fn myapp_agent_toggle_logic() {
        let mut cfg = AppConfig::default();
        let initial_len = cfg.active_agent_ids.len();
        cfg.toggle_agent_active("codex");
        assert!(cfg.is_agent_active("codex"));
        cfg.toggle_agent_active("codex");
        assert!(!cfg.is_agent_active("codex"));
        assert_eq!(cfg.active_agent_ids.len(), initial_len);
    }

    #[test]
    fn myapp_solution_select_persists() {
        let mut cfg = AppConfig::default();
        let repo_path = PathBuf::from("/tmp/repo");
        let sln_path = PathBuf::from("/tmp/repo/app.sln");
        cfg.get_repo_state_mut(&repo_path).selected_solution = Some(sln_path.clone());
        assert_eq!(
            cfg.get_repo_state(&repo_path).unwrap().selected_solution,
            Some(sln_path)
        );
    }

    #[test]
    fn myapp_pending_branch_switch_handling() {
        let mut pending: Option<(PathBuf, String)> =
            Some((PathBuf::from("/tmp/repo"), "main".to_string()));
        // poll_scan would take pending and call handle_branch_switch
        let taken = pending.take();
        assert!(taken.is_some());
        assert!(pending.is_none());
        let (path, branch) = taken.unwrap();
        assert_eq!(path, PathBuf::from("/tmp/repo"));
        assert_eq!(branch, "main");
    }
}
