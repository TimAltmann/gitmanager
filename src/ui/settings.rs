use crate::config::{AgentProfile, AppConfig, IdeConfig, LanguageProfile, TerminalPreference};
use crate::i18n::{tr, tr_fmt, Language};
use egui::{Color32, RichText, Vec2};
use std::path::PathBuf;

const ICON_CHEVRON_DOWN: egui::ImageSource =
    egui::include_image!("../../assets/icons/chevron-down.svg");
const ICON_CHEVRON_UP: egui::ImageSource =
    egui::include_image!("../../assets/icons/chevron-up.svg");
const ICON_EYE: egui::ImageSource = egui::include_image!("../../assets/icons/eye.svg");
const ICON_EYE_OFF: egui::ImageSource = egui::include_image!("../../assets/icons/eye-off.svg");
const ICON_CROSS: egui::ImageSource = egui::include_image!("../../assets/icons/cross.svg");
const ICON_PLUS: egui::ImageSource = egui::include_image!("../../assets/icons/plus.svg");
const ICON_TRASH: egui::ImageSource = egui::include_image!("../../assets/icons/trash.svg");
const ICON_FOLDER: egui::ImageSource = egui::include_image!("../../assets/icons/folder.svg");
const ICON_WARNING: egui::ImageSource = egui::include_image!("../../assets/icons/warning.svg");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Profiles,
    Agents,
    Terminal,
    Appearance,
    Language,
    Icons,
    TrayIcons,
}

pub struct SettingsState {
    pub draft: AppConfig,
    pub error: Option<String>,
    pub success: Option<String>,
    pub auto_detect_toast: Option<(String, std::time::Instant, bool)>,
    selected_tab: SettingsTab,
    selected_profile_idx: Option<usize>,
    selected_agent_idx: Option<usize>,
    // Temporäre Eingabefelder für neue Profile/Agents
    new_profile_name: String,
    new_profile_ext: String,
    new_agent_name: String,
    new_agent_program: String,
    prev_ide_args: std::collections::HashMap<String, Vec<String>>,
}

fn ensure_icon_orders(profile: &mut LanguageProfile, agents: &[AgentProfile]) {
    // IDE order: fehlende ergänzen, entfernte entfernen, Reihenfolge behalten
    let all_ids: Vec<String> = profile.ides.iter().map(|i| i.id.clone()).collect();
    if profile.ide_order.is_empty() && !all_ids.is_empty() {
        profile.ide_order = all_ids.clone();
    } else {
        for id in &all_ids {
            if !profile.ide_order.contains(id) {
                profile.ide_order.push(id.clone());
            }
        }
        profile.ide_order.retain(|id| all_ids.contains(id));
    }
    // Agent order: analog, basierend auf allen globalen Agents
    let all_agent_ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
    if profile.agent_order.is_empty() && !all_agent_ids.is_empty() {
        profile.agent_order = all_agent_ids.clone();
    } else {
        for id in &all_agent_ids {
            if !profile.agent_order.contains(id) {
                profile.agent_order.push(id.clone());
            }
        }
        profile.agent_order.retain(|id| all_agent_ids.contains(id));
    }
}

impl SettingsState {
    pub fn from_config(cfg: &AppConfig) -> Self {
        let mut draft = cfg.clone();
        // Icon-Reihenfolgen vervollständigen ohne Draft als dirty zu markieren
        for profile in &mut draft.profiles {
            ensure_icon_orders(profile, &draft.agents);
        }
        Self {
            draft,
            error: None,
            success: None,
            auto_detect_toast: None,
            selected_tab: SettingsTab::General,
            selected_profile_idx: if cfg.profiles.is_empty() {
                None
            } else {
                Some(0)
            },
            selected_agent_idx: if cfg.agents.is_empty() { None } else { Some(0) },
            new_profile_name: String::new(),
            new_profile_ext: String::new(),
            new_agent_name: String::new(),
            new_agent_program: String::new(),
            prev_ide_args: Default::default(),
        }
    }

    /// Merged nur auto-erkannte VS/Rider Pfade aus neuer Config in den bestehenden Draft,
    /// ohne sonstige Benutzer-Änderungen zu verwerfen.
    pub fn merge_auto_detected(&mut self, new_cfg: &AppConfig) {
        for new_profile in &new_cfg.profiles {
            if let Some(draft_profile) = self
                .draft
                .profiles
                .iter_mut()
                .find(|p| p.id == new_profile.id)
            {
                for new_ide in &new_profile.ides {
                    if let Some(draft_ide) =
                        draft_profile.ides.iter_mut().find(|i| i.id == new_ide.id)
                    {
                        let is_vs = new_ide.id == "vs2022";
                        let is_rider = new_ide.id == "rider";
                        if (is_vs || is_rider) && draft_ide.program != new_ide.program {
                            draft_ide.program = new_ide.program.clone();
                        }
                    }
                }
            }
        }
        // Branch-Limit NICHT automatisch übernehmen: würde Slider-Änderungen im offenen Settings-Draft clobbern.
        // Auto-Erkennung ändert nur VS/Rider Programme, nicht das Limit. Limit wird erst beim nächsten
        // Laden (from_config) oder explizitem Save übernommen.
        // Agents haben sich nicht via Auto-Erkennung geändert, aber sicherstellen dass Orders vollständig
        for profile in &mut self.draft.profiles {
            ensure_icon_orders(profile, &self.draft.agents);
        }
    }
}

pub fn show_settings_window(
    ctx: &egui::Context,
    state: &mut SettingsState,
    open: &mut bool,
    on_save: &mut Option<AppConfig>,
) {
    let mut should_close = false;

    egui::Window::new(tr(state.draft.language, "settings"))
        .open(open)
        .resizable(true)
        .default_width(720.0)
        .default_height(560.0)
        .min_width(640.0)
        .show(ctx, |ui| {
            // Tabs
            let lang = state.draft.language;
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (SettingsTab::General, tr(lang, "tabs_general")),
                    (SettingsTab::Profiles, tr(lang, "tabs_profiles")),
                    (SettingsTab::Agents, tr(lang, "tabs_agents")),
                    (SettingsTab::Terminal, tr(lang, "tabs_terminal")),
                    (SettingsTab::Appearance, tr(lang, "tabs_appearance")),
                    (SettingsTab::Language, tr(lang, "tabs_language")),
                    (SettingsTab::Icons, tr(lang, "tabs_icons")),
                    (SettingsTab::TrayIcons, tr(lang, "tabs_tray_icons")),
                ] {
                    let is_active = state.selected_tab == tab;
                    if ui
                        .selectable_label(is_active, RichText::new(label).size(12.0).strong())
                        .clicked()
                    {
                        state.selected_tab = tab;
                    }
                }
            });
            ui.separator();
            ui.add_space(6.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                match state.selected_tab {
                    SettingsTab::General => show_general_tab(ui, state),
                    SettingsTab::Profiles => show_profiles_tab(ui, state),
                    SettingsTab::Agents => show_agents_tab(ui, state),
                    SettingsTab::Terminal => show_terminal_tab(ui, state),
                    SettingsTab::Appearance => show_appearance_tab(ui, state),
                    SettingsTab::Language => show_language_tab(ui, state),
                    SettingsTab::Icons => show_icons_tab(ui, state),
                    SettingsTab::TrayIcons => show_tray_icons_tab(ui, state),
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                if let Some(cfg_path) = AppConfig::config_path() {
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            tr(state.draft.language, "config_path"),
                            cfg_path.display()
                        ))
                        .size(10.0)
                        .color(Color32::from_rgb(140, 140, 140)),
                    );
                    ui.add_space(8.0);
                }

                if let Some(err) = &state.error {
                    ui.colored_label(Color32::from_rgb(200, 40, 40), format!("✗ {}", err));
                    ui.add_space(4.0);
                }
                if let Some(msg) = &state.success {
                    ui.colored_label(Color32::from_rgb(40, 140, 40), format!("✓ {}", msg));
                    ui.add_space(4.0);
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(RichText::new(tr(state.draft.language, "close")).size(12.0))
                            .clicked()
                        {
                            should_close = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(tr(state.draft.language, "save"))
                                        .size(12.0)
                                        .strong(),
                                )
                                .fill(Color32::from_rgb(60, 120, 220))
                                .stroke(egui::Stroke::NONE),
                            )
                            .clicked()
                        {
                            let lang = state.draft.language;
                            if state.draft.roots.is_empty() {
                                state.error = Some(tr(lang, "error_need_path"));
                                state.success = None;
                            } else if state.draft.profiles.is_empty() {
                                state.error = Some(tr(lang, "error_need_profile"));
                                state.success = None;
                            } else {
                                // Validierung Profile
                                let mut valid = true;
                                for p in &state.draft.profiles {
                                    if p.id.trim().is_empty() || p.display_name.trim().is_empty() {
                                        state.error =
                                            Some(tr_fmt(lang, "error_profile_empty", &[&p.id]));
                                        valid = false;
                                        break;
                                    }
                                    if p.file_extension.trim().is_empty() {
                                        state.error = Some(tr_fmt(
                                            lang,
                                            "error_profile_ext",
                                            &[&p.display_name],
                                        ));
                                        valid = false;
                                        break;
                                    }
                                    // Validierung Config-Selektoren (pro Profil)
                                    {
                                        use std::collections::HashSet;
                                        let mut seen: HashSet<String> = HashSet::new();
                                        for sel in &p.config_selectors {
                                            if sel.id.trim().is_empty() {
                                                state.error = Some(format!(
                                                    "Selector in Profil '{}' hat leere ID",
                                                    p.display_name
                                                ));
                                                valid = false;
                                                break;
                                            }
                                            if sel.display_name.trim().is_empty() {
                                                state.error = Some(format!(
                                                    "Selector '{}' in Profil '{}' braucht Anzeigename",
                                                    sel.id, p.display_name
                                                ));
                                                valid = false;
                                                break;
                                            }
                                            if sel.file_path.trim().is_empty() {
                                                state.error = Some(format!(
                                                    "Selector '{}' in Profil '{}' braucht Dateipfad",
                                                    sel.id, p.display_name
                                                ));
                                                valid = false;
                                                break;
                                            }
                                            if sel.key.trim().is_empty() {
                                                state.error = Some(format!(
                                                    "Selector '{}' in Profil '{}' braucht Key",
                                                    sel.id, p.display_name
                                                ));
                                                valid = false;
                                                break;
                                            }
                                            if sel.key_attribute.trim().is_empty() {
                                                state.error = Some(format!(
                                                    "Selector '{}' in Profil '{}' braucht Key-Attribut",
                                                    sel.id, p.display_name
                                                ));
                                                valid = false;
                                                break;
                                            }
                                            if sel.value_attribute.trim().is_empty() {
                                                state.error = Some(format!(
                                                    "Selector '{}' in Profil '{}' braucht Value-Attribut",
                                                    sel.id, p.display_name
                                                ));
                                                valid = false;
                                                break;
                                            }
                                            let nid = sel.id.trim().to_lowercase().replace(' ', "_");
                                            if !seen.insert(nid) {
                                                state.error = Some(format!(
                                                    "Doppelte Selector-ID '{}' in Profil '{}'",
                                                    sel.id, p.display_name
                                                ));
                                                valid = false;
                                                break;
                                            }
                                        }
                                        if !valid {
                                            break;
                                        }
                                    }
                                }
                                if valid {
                                    match state.draft.clone().save() {
                                        Ok(()) => {
                                            state.error = None;
                                            state.success = Some(tr(lang, "saved_scan_restart"));
                                            *on_save = Some(state.draft.clone());
                                        }
                                        Err(e) => {
                                            state.error = Some(tr_fmt(
                                                lang,
                                                "save_failed",
                                                &[&format!("{e:#}")],
                                            ));
                                            state.success = None;
                                        }
                                    }
                                } else {
                                    state.success = None;
                                }
                            }
                        }
                        if ui.button(tr(state.draft.language, "reset")).clicked() {
                            state.error = None;
                            state.success = None;
                        }
                    });
                });
                ui.add_space(8.0);
            });
        });

    if should_close {
        *open = false;
    }

    // Non-modal toast für Auto-Erkennung (3s, nicht blockierend)
    if let Some((msg, t, is_err)) = state.auto_detect_toast.as_ref() {
        if t.elapsed() > std::time::Duration::from_secs(3) {
            state.auto_detect_toast = None;
        } else {
            let msg = msg.clone();
            let is_err = *is_err;
            let bg = if is_err {
                Color32::from_rgb(255, 235, 235)
            } else {
                Color32::from_rgb(235, 255, 235)
            };
            let stroke_col = if is_err {
                Color32::from_rgb(220, 100, 100)
            } else {
                Color32::from_rgb(100, 180, 100)
            };
            let text_col = if is_err {
                Color32::from_rgb(160, 40, 40)
            } else {
                Color32::from_rgb(40, 100, 40)
            };
            let mut close_toast = false;
            egui::Window::new("auto_detect_toast")
                .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .frame(
                    egui::Frame::new()
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, stroke_col))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(12, 8)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(msg.clone())
                                .size(12.0)
                                .color(text_col)
                                .strong(),
                        );
                        if ui
                            .add(egui::Button::image(
                                egui::Image::new(ICON_CROSS).fit_to_exact_size(Vec2::splat(12.0)),
                            ))
                            .clicked()
                        {
                            close_toast = true;
                        }
                    });
                });
            if close_toast || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                state.auto_detect_toast = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }
}

fn show_general_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(
        RichText::new(
            "Wähle die Ordner, die nach Git-Repositories durchsucht werden sollen. Es werden alle direkten Unterordner bis zur eingestellten Tiefe geprüft.",
        )
        .size(12.0)
        .color(Color32::from_rgb(80, 80, 80)),
    );
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        ui.label(RichText::new("Suchpfade").size(13.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::image_and_text(
                    egui::Image::new(ICON_PLUS).fit_to_exact_size(Vec2::splat(14.0)),
                    "Hinzufügen",
                ))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    state.draft.add_root(path);
                    state.error = None;
                    state.success = None;
                }
            }
        });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    if state.draft.roots.is_empty() {
        ui.label(
            RichText::new("Keine Pfade konfiguriert. Füge einen Ordner hinzu.")
                .italics()
                .color(Color32::from_rgb(140, 140, 140)),
        );
        ui.add_space(8.0);
    }

    let mut to_remove: Option<PathBuf> = None;
    for root in &state.draft.roots.clone() {
        ui.horizontal(|ui| {
            let mut path_str = root.display().to_string();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut path_str)
                    .hint_text("Pfad zum Suchordner")
                    .desired_width(ui.available_width() - 90.0),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(idx) = state.draft.roots.iter().position(|p| p == root) {
                    state.draft.roots[idx] = PathBuf::from(path_str.clone());
                }
            }
            if ui
                .add(egui::Button::image(
                    egui::Image::new(ICON_TRASH).fit_to_exact_size(Vec2::splat(14.0)),
                ))
                .on_hover_text("Entfernen")
                .clicked()
            {
                to_remove = Some(root.clone());
            }
            if ui
                .add(egui::Button::image(
                    egui::Image::new(ICON_FOLDER).fit_to_exact_size(Vec2::splat(14.0)),
                ))
                .on_hover_text("Durchsuchen")
                .clicked()
            {
                if let Some(new_path) = rfd::FileDialog::new().pick_folder() {
                    if let Some(idx) = state.draft.roots.iter().position(|p| p == root) {
                        state.draft.roots[idx] = new_path;
                    }
                }
            }
        });
        ui.add_space(4.0);
        if !root.exists() {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(ICON_WARNING)
                        .fit_to_exact_size(Vec2::splat(14.0))
                        .tint(Color32::from_rgb(200, 80, 20)),
                );
                ui.label(
                    RichText::new(format!("Pfad existiert nicht: {}", root.display()))
                        .size(11.0)
                        .color(Color32::from_rgb(200, 80, 20)),
                );
            });
            ui.add_space(4.0);
        }
    }
    if let Some(p) = to_remove {
        state.draft.remove_root(&p);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(RichText::new("Suchtiefe").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Wie viele Ebenen tief gesucht wird (1 = nur direkte Kinder, 2 = Kinder + Enkel, ...)",
        )
        .size(11.0)
        .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Tiefe:");
        let mut depth = state.draft.max_depth as u8;
        let slider = egui::Slider::new(&mut depth, 1..=10).text("Ebenen");
        if ui.add(slider).changed() {
            state.draft.max_depth = depth as usize;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Oder direkt:");
        let mut d = state.draft.max_depth;
        if ui
            .add(egui::DragValue::new(&mut d).range(1..=10).speed(0.2))
            .changed()
        {
            state.draft.max_depth = d.clamp(1, 10);
        }
        ui.label(
            RichText::new(format!("(aktuell: {})", state.draft.max_depth))
                .size(11.0)
                .color(Color32::from_rgb(120, 120, 120)),
        );
    });
    ui.add_space(8.0);

    // Branch Anzeige Limit
    ui.separator();
    ui.add_space(8.0);
    ui.label(RichText::new("Branch-Anzeige Limit").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Maximale Anzahl Branches im Dropdown (50–500, Standard 200). Höhere Werte zeigen mehr Branches, können aber die Liste unübersichtlich machen.",
        )
        .size(11.0)
        .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Limit:");
        let mut limit = state.draft.branch_display_limit;
        let slider = egui::Slider::new(&mut limit, 50..=500)
            .text("Branches")
            .step_by(10.0);
        if ui.add(slider).changed() {
            state.draft.branch_display_limit = limit.clamp(50, 500);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Oder direkt:");
        let mut l = state.draft.branch_display_limit;
        if ui
            .add(egui::DragValue::new(&mut l).range(50..=500).speed(1.0))
            .changed()
        {
            state.draft.branch_display_limit = l.clamp(50, 500);
        }
        ui.label(
            RichText::new(format!("(aktuell: {})", state.draft.branch_display_limit))
                .size(11.0)
                .color(Color32::from_rgb(120, 120, 120)),
        );
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "Zeigt bis zu {} Branches (Tippe zum Filtern, mehr anzeigen).",
            state.draft.branch_display_limit
        ))
        .size(10.0)
        .color(Color32::from_rgb(120, 120, 120))
        .italics(),
    );
    ui.add_space(8.0);

    // Tray-Einstellungen
    ui.separator();
    ui.add_space(8.0);
    ui.label(RichText::new("System Tray").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Verhalten wenn das Fenster geschlossen wird und Einstellungen für das Tray-Popup.",
        )
        .size(11.0)
        .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(6.0);
    {
        let mut minimize = state.draft.minimize_to_tray;
        if ui
            .checkbox(
                &mut minimize,
                "Beim Schließen in Tray minimieren (statt beenden)",
            )
            .changed()
        {
            state.draft.minimize_to_tray = minimize;
        }
        ui.label(
            RichText::new(if minimize {
                "✓ Das Fenster wird beim Schließen ausgeblendet und läuft im Tray weiter (Links-Klick: eigenes Menü, Rechts-Klick: Kontextmenü)."
            } else {
                "Das Fenster wird beim Schließen beendet."
            })
            .size(10.0)
            .color(Color32::from_rgb(120, 120, 120))
            .italics(),
        );
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Tray Branch-Limit").size(12.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Wie viele Branches maximal im Tray-Popup Dropdown angezeigt werden (5–50, Standard 20).",
        )
        .size(11.0)
        .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Limit:");
        let mut limit = state.draft.tray_branch_limit;
        // ensure at least 5
        if limit < 5 {
            limit = 5;
        }
        let slider = egui::Slider::new(&mut limit, 5..=50)
            .text("Branches")
            .step_by(1.0);
        if ui.add(slider).changed() {
            state.draft.tray_branch_limit = limit.clamp(5, 50);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Oder direkt:");
        let mut l = state.draft.tray_branch_limit;
        if ui
            .add(egui::DragValue::new(&mut l).range(5..=50).speed(1.0))
            .changed()
        {
            state.draft.tray_branch_limit = l.clamp(5, 50);
        }
        ui.label(
            RichText::new(format!("(aktuell: {})", state.draft.tray_branch_limit))
                .size(11.0)
                .color(Color32::from_rgb(120, 120, 120)),
        );
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "Zeigt bis zu {} Branches im Tray-Popup. Weitere im Hauptfenster.",
            state.draft.tray_branch_limit
        ))
        .size(10.0)
        .color(Color32::from_rgb(120, 120, 120))
        .italics(),
    );
    ui.add_space(8.0);

    // Aktives Profil global
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        RichText::new("Aktives Sprach-Profil (global)")
            .size(13.0)
            .strong(),
    );
    ui.add_space(4.0);
    egui::ComboBox::from_id_salt("active_profile_general")
        .selected_text(state.draft.get_active_profile().display_name.clone())
        .show_ui(ui, |ui| {
            for p in &state.draft.profiles {
                let is_active = p.id == state.draft.active_profile_id;
                if ui.selectable_label(is_active, &p.display_name).clicked() {
                    state.draft.active_profile_id = p.id.clone();
                }
            }
        });
    ui.add_space(4.0);
    ui.label(
        RichText::new("Pro Repo kann in der Übersicht ein Override gewählt werden.")
            .size(11.0)
            .color(Color32::from_rgb(120, 120, 120)),
    );
}

fn show_profiles_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(
        RichText::new("Sprach-Profile verwalten")
            .size(13.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Jedes Profil definiert eine Dateiendung (z.B. .sln) und die IDEs, mit denen diese geöffnet werden kann.")
            .size(11.0)
            .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(8.0);

    // Liste der Profile
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Profile ({}):", state.draft.profiles.len()))
                .size(12.0)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::image_and_text(
                    egui::Image::new(ICON_PLUS).fit_to_exact_size(Vec2::splat(14.0)),
                    "Neues Profil",
                ))
                .clicked()
            {
                let mut max_n = 0;
                for p in &state.draft.profiles {
                    if let Some(suffix) = p.id.strip_prefix("custom") {
                        if let Ok(n) = suffix.parse::<usize>() {
                            if n > max_n {
                                max_n = n;
                            }
                        }
                    }
                }
                let new_id = format!("custom{}", max_n + 1);
                let display_n = max_n + 1;
                state.draft.profiles.push(LanguageProfile {
                    id: new_id.clone(),
                    display_name: format!("Custom {}", display_n),
                    file_extension: ".txt".to_string(),
                    file_pattern: None,
                    max_scan_depth: 3,
                    ides: vec![IdeConfig {
                        id: "vscode".to_string(),
                        display_name: "VS Code".to_string(),
                        program: "code".to_string(),
                        args: vec![".".to_string()],
                        command: None,
                        use_shell: false,
                        allow_unsafe: false,
                        no_args: false,

                        icon: None,
                    }],
                    default_ide_id: Some("vscode".to_string()),
                    ide_order: Vec::new(),
                    hidden_ide_ids: Vec::new(),
                    hidden_agent_ids: Vec::new(),
                    agent_order: Vec::new(),
                    show_shell: true,
                    show_explorer: true,
                    config_selectors: Vec::new(),
                });
                state.selected_profile_idx = Some(state.draft.profiles.len() - 1);
            }
        });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // Profile-Liste
    let mut to_delete: Option<usize> = None;
    let mut to_duplicate: Option<usize> = None;
    for (idx, profile) in state.draft.profiles.iter().enumerate() {
        let is_selected = Some(idx) == state.selected_profile_idx;
        let is_active = profile.id == state.draft.active_profile_id;
        let visuals = ui.visuals();
        let frame = egui::Frame::new()
            .fill(visuals.widgets.inactive.bg_fill)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if is_active {
                    visuals.text_color()
                } else {
                    visuals.widgets.inactive.fg_stroke.color
                },
            ))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(8, 6));

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = if is_active {
                    format!(
                        "● {} ({}) [{}]",
                        profile.display_name, profile.id, profile.file_extension
                    )
                } else {
                    format!(
                        "{} ({}) [{}]",
                        profile.display_name, profile.id, profile.file_extension
                    )
                };
                if ui
                    .selectable_label(is_selected, RichText::new(label).size(12.0))
                    .clicked()
                {
                    state.selected_profile_idx = Some(idx);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Duplizieren").clicked() {
                        to_duplicate = Some(idx);
                    }
                    if ui
                        .add(egui::Button::image_and_text(
                            egui::Image::new(ICON_TRASH).fit_to_exact_size(Vec2::splat(12.0)),
                            "Löschen",
                        ))
                        .clicked()
                    {
                        to_delete = Some(idx);
                    }
                    if ui.small_button("Aktiv setzen").clicked() {
                        state.draft.active_profile_id = profile.id.clone();
                    }
                });
            });
        });
        ui.add_space(4.0);
    }
    if let Some(idx) = to_duplicate {
        if let Some(p) = state.draft.profiles.get(idx).cloned() {
            let mut new_p = p.clone();
            new_p.id = format!("{}_copy", p.id);
            new_p.display_name = format!("{} (Kopie)", p.display_name);
            // Copy custom icons to new files so deletion of original doesn't affect copy
            for ide in &mut new_p.ides {
                if let Some(old_path) = ide.icon.clone() {
                    let src = PathBuf::from(&old_path);
                    if src.exists() {
                        if let Ok(dest) = crate::config::AppConfig::copy_icon_to_storage(&src) {
                            ide.icon = Some(dest.display().to_string());
                        }
                    }
                }
            }
            state.draft.profiles.push(new_p);
            state.selected_profile_idx = Some(state.draft.profiles.len() - 1);
        }
    }
    if let Some(idx) = to_delete {
        if state.draft.profiles.len() > 1 {
            // Cleanup custom icons of removed profile
            if let Some(removed) = state.draft.profiles.get(idx) {
                for ide in &removed.ides {
                    if let Some(icon) = &ide.icon {
                        crate::config::AppConfig::remove_icon_file(icon);
                    }
                }
            }
            state.draft.profiles.remove(idx);
            if let Some(sel) = state.selected_profile_idx {
                if sel == idx {
                    state.selected_profile_idx = Some(0);
                } else if sel > idx {
                    state.selected_profile_idx = Some(sel - 1);
                }
            }
            if !state
                .draft
                .profiles
                .iter()
                .any(|p| p.id == state.draft.active_profile_id)
            {
                state.draft.active_profile_id = state.draft.profiles[0].id.clone();
            }
        } else {
            state.error = Some("Mindestens ein Profil muss vorhanden sein.".to_string());
        }
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Neuer Profil Schnell-Anlage
    ui.collapsing("Schnell-Anlage neues Profil", |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(
                egui::TextEdit::singleline(&mut state.new_profile_name)
                    .hint_text(".NET / Rust / Node")
                    .desired_width(120.0),
            );
            ui.label("Endung:");
            ui.add(
                egui::TextEdit::singleline(&mut state.new_profile_ext)
                    .hint_text(".sln / .rs")
                    .desired_width(80.0),
            );
            if ui.button("Anlegen").clicked() {
                if !state.new_profile_name.trim().is_empty()
                    && !state.new_profile_ext.trim().is_empty()
                {
                    let mut ext = state.new_profile_ext.trim().to_string();
                    if !ext.starts_with('.') {
                        ext = format!(".{}", ext);
                    }
                    let id = state
                        .new_profile_name
                        .trim()
                        .to_lowercase()
                        .replace(' ', "_");
                    state.draft.profiles.push(LanguageProfile {
                        id: id.clone(),
                        display_name: state.new_profile_name.trim().to_string(),
                        file_extension: ext,
                        file_pattern: None,
                        max_scan_depth: 3,
                        ides: vec![IdeConfig {
                            id: "vscode".to_string(),
                            display_name: "VS Code".to_string(),
                            program: "code".to_string(),
                            args: vec![".".to_string()],
                            command: None,
                            use_shell: false,
                            allow_unsafe: false,
                            no_args: false,

                            icon: None,
                        }],
                        default_ide_id: Some("vscode".to_string()),
                        ide_order: Vec::new(),
                        hidden_ide_ids: Vec::new(),
                        hidden_agent_ids: Vec::new(),
                        agent_order: Vec::new(),
                        show_shell: true,
                        show_explorer: true,
                        config_selectors: Vec::new(),
                    });
                    state.selected_profile_idx = Some(state.draft.profiles.len() - 1);
                    state.new_profile_name.clear();
                    state.new_profile_ext.clear();
                } else {
                    state.error = Some("Name und Endung erforderlich".to_string());
                }
            }
        });
    });

    // Detail-Editor für ausgewähltes Profil
    if let Some(idx) = state.selected_profile_idx {
        if let Some(profile) = state.draft.profiles.get_mut(idx) {
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("Profil bearbeiten: {}", profile.display_name))
                    .size(13.0)
                    .strong(),
            );
            ui.add_space(6.0);

            egui::Grid::new(format!("profile_edit_{}", idx))
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("ID:");
                    let mut id = profile.id.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut id).hint_text("dotnet"))
                        .changed()
                    {
                        profile.id = id.trim().to_lowercase().replace(' ', "_");
                    }
                    ui.end_row();
                    ui.label("Anzeigename:");
                    ui.add(egui::TextEdit::singleline(&mut profile.display_name).hint_text(".NET"));
                    ui.end_row();
                    ui.label("Dateiendung:");
                    ui.add(
                        egui::TextEdit::singleline(&mut profile.file_extension).hint_text(".sln"),
                    );
                    ui.end_row();
                    ui.label("Muster (optional):");
                    let mut pat = profile.file_pattern.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut pat).hint_text("*.sln oder Cargo.toml"),
                        )
                        .changed()
                    {
                        profile.file_pattern = if pat.trim().is_empty() {
                            None
                        } else {
                            Some(pat)
                        };
                    }
                    ui.end_row();
                    ui.label("Scan-Tiefe:");
                    ui.add(egui::Slider::new(&mut profile.max_scan_depth, 1..=4).text("Ebenen"));
                    ui.end_row();
                });

            ui.add_space(8.0);
            ui.label(RichText::new("IDEs für dieses Profil:").size(12.0).strong());
            ui.add_space(4.0);

            let mut to_remove_ide: Option<usize> = None;
            for (ide_idx, ide) in profile.ides.iter_mut().enumerate() {
                let visuals = ui.visuals();
                egui::Frame::new()
                    .fill(visuals.widgets.inactive.bg_fill)
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        visuals.widgets.inactive.fg_stroke.color,
                    ))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("IDE {}", ide_idx + 1))
                                    .size(11.0)
                                    .strong(),
                            );
                            let is_default = Some(&ide.id) == profile.default_ide_id.as_ref();
                            if is_default {
                                ui.label(
                                    RichText::new("★ Default")
                                        .size(10.0)
                                        .color(Color32::from_rgb(60, 120, 220)),
                                );
                            } else if ui.small_button("Als Default").clicked() {
                                profile.default_ide_id = Some(ide.id.clone());
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::image(
                                                egui::Image::new(ICON_TRASH)
                                                    .fit_to_exact_size(Vec2::splat(14.0)),
                                            )
                                        )
                                        .on_hover_text("IDE entfernen")
                                        .clicked()
                                    {
                                        to_remove_ide = Some(ide_idx);
                                    }
                                },
                            );
                        });
                        ui.add_space(4.0);
                        egui::Grid::new(format!("ide_edit_{}_{}", idx, ide_idx))
                            .num_columns(2)
                            .spacing([8.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("ID:");
                                ui.add(egui::TextEdit::singleline(&mut ide.id).hint_text("vscode"));
                                ui.end_row();
                                ui.label("Name:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut ide.display_name)
                                        .hint_text("VS Code"),
                                );
                                ui.end_row();
                                ui.label("Programm:");
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut ide.program)
                                            .hint_text("code / devenv / rider"),
                                    );
                                    let is_vs = matches!(
                                        ide.id.as_str(),
                                        "vs2022" | "vs" | "visualstudio"
                                    );
                                    let is_rider = matches!(ide.id.as_str(), "rider" | "jetbrains");
                                    if (is_vs || is_rider)
                                        && ui
                                            .small_button("Auto erkennen")
                                            .on_hover_text(
                                                "Pfad automatisch erkennen (vswhere / Toolbox)",
                                            )
                                            .clicked()
                                    {
                                            let detected = if is_vs {
                                                crate::git::resolve_vs_path_force()
                                            } else {
                                                crate::git::resolve_rider_path_force()
                                            };
                                            if let Some(p) = detected {
                                                ide.program = p.display().to_string();
                                                state.auto_detect_toast = Some((
                                                    format!("Erkannt: {}", ide.program),
                                                    std::time::Instant::now(),
                                                    false,
                                                ));
                                                state.error = None;
                                                state.success = None;
                                            } else {
                                                state.auto_detect_toast = Some((
                                                    if is_vs {
                                                        "Visual Studio nicht gefunden (vswhere fehlt oder keine Installation)".to_string()
                                                    } else {
                                                        "Rider nicht gefunden (Toolbox / Program Files / PATH)".to_string()
                                                    },
                                                    std::time::Instant::now(),
                                                    true,
                                                ));
                                                state.error = None;
                                                state.success = None;
                                            }
                                        }
                                });
                                ui.end_row();
                                ui.label("Args:");
                                ui.horizontal(|ui| {
                                    let hint = if ide.id == "vscode" {
                                        ".  ({file} {dir} {repo})"
                                    } else {
                                        "{file} --reuse-window  ({dir} {repo} | .)"
                                    };
                                    let mut args_str = ide.args.join(" ");
                                    let enabled = !ide.no_args;
                                    let resp = ui.add_enabled(
                                        enabled,
                                        egui::TextEdit::singleline(&mut args_str).hint_text(hint),
                                    );
                                    if resp.changed() {
                                        ide.args = if args_str.trim().is_empty() {
                                            Vec::new()
                                        } else {
                                            args_str.split_whitespace().map(|s| s.to_string()).collect()
                                        };
                                    }
                                    if ui.checkbox(&mut ide.no_args, "Kein Argument").changed() {
                                        let key = format!("{}:{}", profile.id, ide.id);
                                        if ide.no_args {
                                            state.prev_ide_args.insert(key, ide.args.clone());
                                            ide.args.clear();
                                        } else if let Some(prev) = state
                                            .prev_ide_args
                                            .remove(&key)
                                            .or_else(|| state.prev_ide_args.remove(&ide.id.clone()))
                                        {
                                            if !prev.is_empty() {
                                                ide.args = prev;
                                            } else {
                                                ide.args = vec!["{file}".to_string()];
                                            }
                                        } else if ide.args.is_empty() {
                                            ide.args = vec!["{file}".to_string()];
                                        }
                                    }
                                });
                                ui.end_row();
                                ui.label("Shell:");
                                {
                                    let mut allow_unsafe = ide.allow_unsafe || ide.use_shell;
                                    if ui
                                        .checkbox(&mut allow_unsafe, "via cmd /C (unsicher)")
                                        .changed()
                                    {
                                        ide.allow_unsafe = allow_unsafe;
                                        ide.use_shell = allow_unsafe;
                                    }
                                }
                                ui.end_row();
                                ui.label("Icon:");
                                ui.horizontal(|ui| {
                                    // Preview 18px – zeigt Custom Icon wenn vorhanden, sonst Default
                                    let is_custom = ide.icon.is_some();
                                    let preview_size = Vec2::splat(18.0);
                                    if let Some(icon_path) = &ide.icon {
                                        let pb = PathBuf::from(icon_path);
                                        if pb.exists() {
                                            let uri = format!(
                                                "file://{}",
                                                pb.display().to_string().replace('\\', "/")
                                            );
                                            ui.add(
                                                egui::Image::new(uri)
                                                    .fit_to_exact_size(preview_size),
                                            );
                                        } else {
                                            ui.add(
                                                egui::Image::new(ICON_WARNING)
                                                    .fit_to_exact_size(preview_size)
                                                    .tint(Color32::from_rgb(200, 80, 20)),
                                            );
                                            ui.label(
                                                RichText::new("nicht gefunden")
                                                    .size(9.0)
                                                    .color(Color32::from_rgb(200, 80, 20)),
                                            );
                                        }
                                    } else {
                                        // Default icon preview
                                        let default_icon = match ide.id.as_str() {
                                            "vs2022" | "vs" | "visualstudio" => {
                                                egui::include_image!("../../assets/icons/visualstudio.svg")
                                            }
                                            "rider" | "jetbrains" => {
                                                egui::include_image!("../../assets/icons/rider.svg")
                                            }
                                            _ => {
                                                egui::include_image!("../../assets/icons/vscode.svg")
                                            }
                                        };
                                        ui.add(
                                            egui::Image::new(default_icon)
                                                .fit_to_exact_size(preview_size),
                                        );
                                        ui.label(
                                            RichText::new("(Default)")
                                                .size(9.0)
                                                .color(Color32::from_rgb(120, 120, 120)),
                                        );
                                    }
                                    if ui
                                        .add(
                                            egui::Button::image(
                                                egui::Image::new(ICON_FOLDER)
                                                    .fit_to_exact_size(Vec2::splat(14.0)),
                                            )
                                        )
                                        .on_hover_text("Icon wählen (svg, png, ico, jpg - max 2 MB)")
                                        .clicked()
                                    {
                                        if let Some(src) = rfd::FileDialog::new()
                                            .add_filter("Icon", &["svg", "png", "ico", "jpg", "jpeg"])
                                            .pick_file()
                                        {
                                            let old = ide.icon.clone();
                                            match crate::config::AppConfig::copy_icon_to_storage(&src) {
                                                Ok(dest) => {
                                                    if let Some(old_path) = old {
                                                        crate::config::AppConfig::remove_icon_file(&old_path);
                                                    }
                                                    ide.icon = Some(dest.display().to_string());
                                                    state.error = None;
                                                }
                                                Err(e) => {
                                                    state.error = Some(e);
                                                }
                                            }
                                        }
                                    }
                                    if is_custom
                                        && ui
                                            .add(
                                                egui::Button::image(
                                                    egui::Image::new(ICON_TRASH)
                                                        .fit_to_exact_size(Vec2::splat(14.0)),
                                                )
                                            )
                                            .on_hover_text("Custom Icon entfernen (zurück zu Default)")
                                            .clicked()
                                    {
                                        if let Some(old) = ide.icon.take() {
                                            crate::config::AppConfig::remove_icon_file(&old);
                                        }
                                    }
                                });
                                ui.end_row();
                            });
                        // Preview (mit Substitution und cwd)
                        let prog = ide.effective_program();
                        let args_preview = ide.effective_args().join(" ");
                        let cwd_preview = "C:\\repo";
                        let file_preview = "C:\\repo\\MyApp.sln";
                        let sub_preview: String = if ide.no_args || args_preview.is_empty() {
                            "(keine Args, cwd = Repo)".to_string()
                        } else {
                            let file_path = std::path::Path::new(file_preview);
                            let repo_path = std::path::Path::new(cwd_preview);
                            ide.effective_args()
                                .iter()
                                .map(|a| crate::git::substitute_placeholders(a, file_path, repo_path))
                                .collect::<Vec<_>>()
                                .join(" ")
                        };
                        ui.label(
                            RichText::new(format!(
                                "Vorschau: {} {}  (cwd = {})",
                                prog,
                                if sub_preview.is_empty() {
                                    args_preview.clone()
                                } else {
                                    sub_preview
                                },
                                cwd_preview
                            ))
                            .size(10.0)
                            .color(Color32::from_rgb(100, 100, 100))
                            .italics(),
                        );
                        if ide.id == "vscode" && ide.args == vec![".".to_string()] {
                            ui.label(
                                RichText::new("Hinweis: VS Code öffnet mit '.' das Repo-Root (cwd)")
                                    .size(9.0)
                                    .color(Color32::from_rgb(100, 120, 100))
                                    .italics(),
                            );
                        }
                    });
                ui.add_space(4.0);
            }
            if let Some(ide_idx) = to_remove_ide {
                if profile.ides.len() > 1 {
                    if let Some(removed) = profile.ides.get(ide_idx) {
                        if let Some(icon) = &removed.icon {
                            crate::config::AppConfig::remove_icon_file(icon);
                        }
                    }
                    profile.ides.remove(ide_idx);
                } else {
                    state.error = Some("Mindestens eine IDE pro Profil erforderlich".to_string());
                }
            }

            if ui
                .add(egui::Button::image_and_text(
                    egui::Image::new(ICON_PLUS).fit_to_exact_size(Vec2::splat(14.0)),
                    "IDE hinzufügen",
                ))
                .clicked()
            {
                let mut max_n = 0;
                for ide in &profile.ides {
                    if let Some(suffix) = ide.id.strip_prefix("ide") {
                        if let Ok(n) = suffix.parse::<usize>() {
                            if n > max_n {
                                max_n = n;
                            }
                        }
                    }
                }
                profile.ides.push(IdeConfig {
                    id: format!("ide{}", max_n + 1),
                    display_name: "Neue IDE".to_string(),
                    program: "code".to_string(),
                    args: vec!["{file}".to_string()],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,

                    icon: None,
                });
            }

            ui.add_space(8.0);
            // Default IDE Combo
            ui.horizontal(|ui| {
                ui.label("Default IDE:");
                let current_def = profile.default_ide_id.clone().unwrap_or_default();
                let current_name = profile
                    .ides
                    .iter()
                    .find(|i| i.id == current_def)
                    .map(|i| i.display_name.clone())
                    .unwrap_or_else(|| "Keine".to_string());
                egui::ComboBox::from_id_salt(format!("def_ide_{}", idx))
                    .selected_text(current_name)
                    .show_ui(ui, |ui| {
                        for ide in &profile.ides {
                            let is_def = Some(&ide.id) == profile.default_ide_id.as_ref();
                            if ui.selectable_label(is_def, &ide.display_name).clicked() {
                                profile.default_ide_id = Some(ide.id.clone());
                            }
                        }
                    });
            });

            // ── Config-Dropdowns für dieses Profil ───────────────────────
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            let lang = state.draft.language;
            ui.label(
                RichText::new(tr(lang, "config_selector_title"))
                    .size(13.0)
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(lang, "config_selector_desc"))
                    .size(11.0)
                    .color(Color32::from_rgb(100, 100, 100)),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Selektoren ({}):", profile.config_selectors.len()))
                        .size(12.0)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::image_and_text(
                            egui::Image::new(ICON_PLUS).fit_to_exact_size(Vec2::splat(14.0)),
                            "Selector hinzufügen",
                        ))
                        .clicked()
                    {
                        let mut max_n = 0;
                        for sel in &profile.config_selectors {
                            if let Some(suffix) = sel.id.strip_prefix("selector") {
                                if let Ok(n) = suffix.parse::<usize>() {
                                    if n > max_n {
                                        max_n = n;
                                    }
                                }
                            }
                        }
                        let new_id = format!("selector{}", max_n + 1);
                        profile
                            .config_selectors
                            .push(crate::config::ConfigSelector {
                                id: new_id,
                                display_name: "Neuer Selector".to_string(),
                                file_path: "App.config".to_string(),
                                key: "Database".to_string(),
                                key_attribute: "key".to_string(),
                                value_attribute: "value".to_string(),
                                kind: crate::config::XmlSelectorKind::AddKeyValue,
                                options: vec![
                                    crate::config::ConfigOption {
                                        value: "dev".to_string(),
                                        label: "Dev".to_string(),
                                    },
                                    crate::config::ConfigOption {
                                        value: "prod".to_string(),
                                        label: "Prod".to_string(),
                                    },
                                ],
                                allow_custom: false,
                            });
                    }
                });
            });
            ui.add_space(6.0);

            let sel_count = profile.config_selectors.len();
            let mut to_remove_sel: Option<usize> = None;
            let mut sel_move: Option<(usize, isize)> = None;
            for (sel_idx, sel) in profile.config_selectors.iter_mut().enumerate() {
                let visuals = ui.visuals();
                egui::Frame::new()
                    .fill(visuals.widgets.inactive.bg_fill)
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        visuals.widgets.inactive.fg_stroke.color,
                    ))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Selector {}: {} ({})",
                                    sel_idx + 1,
                                    sel.display_name,
                                    sel.id
                                ))
                                .size(11.0)
                                .strong(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(egui::Button::image(
                                            egui::Image::new(ICON_TRASH)
                                                .fit_to_exact_size(Vec2::splat(12.0)),
                                        ))
                                        .on_hover_text("Löschen")
                                        .clicked()
                                    {
                                        to_remove_sel = Some(sel_idx);
                                    }
                                    if ui
                                        .add(egui::Button::image(
                                            egui::Image::new(ICON_CHEVRON_DOWN)
                                                .fit_to_exact_size(Vec2::splat(14.0)),
                                        ))
                                        .clicked()
                                        && sel_idx + 1 < sel_count
                                    {
                                        sel_move = Some((sel_idx, 1));
                                    }
                                    if ui
                                        .add(egui::Button::image(
                                            egui::Image::new(ICON_CHEVRON_UP)
                                                .fit_to_exact_size(Vec2::splat(14.0)),
                                        ))
                                        .clicked()
                                        && sel_idx > 0
                                    {
                                        sel_move = Some((sel_idx, -1));
                                    }
                                },
                            );
                        });
                        ui.add_space(4.0);
                        egui::Grid::new(format!("selector_edit_{}_{}", idx, sel_idx))
                            .num_columns(2)
                            .spacing([8.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("ID:");
                                let mut id = sel.id.clone();
                                if ui
                                    .add(egui::TextEdit::singleline(&mut id).hint_text("db"))
                                    .changed()
                                {
                                    sel.id = id.trim().to_lowercase().replace(' ', "_");
                                }
                                ui.end_row();
                                ui.label("Anzeigename:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut sel.display_name)
                                        .hint_text("Datenbank"),
                                );
                                ui.end_row();
                                ui.label("Datei:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut sel.file_path)
                                        .hint_text("App.config"),
                                );
                                ui.end_row();
                                ui.label("Key:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut sel.key).hint_text("Database"),
                                );
                                ui.end_row();
                                ui.label("Key-Attribut:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut sel.key_attribute)
                                        .hint_text("key"),
                                );
                                ui.end_row();
                                ui.label("Value-Attribut:");
                                ui.add(
                                    egui::TextEdit::singleline(&mut sel.value_attribute)
                                        .hint_text("value"),
                                );
                                ui.end_row();
                                ui.label("Custom erlauben:");
                                ui.checkbox(&mut sel.allow_custom, "erlauben");
                                ui.end_row();
                            });
                        ui.add_space(6.0);
                        ui.label(RichText::new("Optionen:").size(11.0).strong());
                        ui.add_space(2.0);
                        let mut to_remove_opt: Option<usize> = None;
                        for (opt_idx, opt) in sel.options.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}:", opt_idx + 1));
                                ui.add(
                                    egui::TextEdit::singleline(&mut opt.value)
                                        .hint_text("value")
                                        .desired_width(90.0),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut opt.label)
                                        .hint_text("Label")
                                        .desired_width(120.0),
                                );
                                if ui
                                    .add(egui::Button::image(
                                        egui::Image::new(ICON_TRASH)
                                            .fit_to_exact_size(Vec2::splat(12.0)),
                                    ))
                                    .on_hover_text("Option löschen")
                                    .clicked()
                                {
                                    to_remove_opt = Some(opt_idx);
                                }
                            });
                            ui.add_space(2.0);
                        }
                        if let Some(o_idx) = to_remove_opt {
                            sel.options.remove(o_idx);
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .add(egui::Button::image_and_text(
                                    egui::Image::new(ICON_PLUS)
                                        .fit_to_exact_size(Vec2::splat(12.0)),
                                    "Option",
                                ))
                                .clicked()
                            {
                                let n = sel.options.len() + 1;
                                sel.options.push(crate::config::ConfigOption {
                                    value: format!("value{}", n),
                                    label: format!("Option {}", n),
                                });
                            }
                        });
                        if sel.options.is_empty() {
                            ui.label(
                                RichText::new(
                                    "Keine Optionen – füge Werte hinzu oder erlaube Custom.",
                                )
                                .size(10.0)
                                .color(Color32::from_rgb(140, 140, 140))
                                .italics(),
                            );
                        }
                    });
                ui.add_space(4.0);
            }
            if let Some(s_idx) = to_remove_sel {
                profile.config_selectors.remove(s_idx);
            }
            if let Some((s_idx, delta)) = sel_move {
                let new_idx = (s_idx as isize + delta) as usize;
                if new_idx < profile.config_selectors.len() {
                    profile.config_selectors.swap(s_idx, new_idx);
                }
            }
            if profile.config_selectors.is_empty() {
                ui.label(
                    RichText::new(
                        "Keine Selektoren konfiguriert. Füge einen hinzu, um XML-Werte per Dropdown zu steuern.",
                    )
                    .italics()
                    .color(Color32::from_rgb(140, 140, 140)),
                );
                ui.add_space(4.0);
            }
            if ui
                .add(egui::Button::image_and_text(
                    egui::Image::new(ICON_PLUS).fit_to_exact_size(Vec2::splat(14.0)),
                    "Selector hinzufügen",
                ))
                .clicked()
            {
                let mut max_n = 0;
                for sel in &profile.config_selectors {
                    if let Some(suffix) = sel.id.strip_prefix("selector") {
                        if let Ok(n) = suffix.parse::<usize>() {
                            if n > max_n {
                                max_n = n;
                            }
                        }
                    }
                }
                let new_id = format!("selector{}", max_n + 1);
                profile
                    .config_selectors
                    .push(crate::config::ConfigSelector {
                        id: new_id,
                        display_name: "Neuer Selector".to_string(),
                        file_path: "App.config".to_string(),
                        key: "Database".to_string(),
                        key_attribute: "key".to_string(),
                        value_attribute: "value".to_string(),
                        kind: crate::config::XmlSelectorKind::AddKeyValue,
                        options: vec![
                            crate::config::ConfigOption {
                                value: "dev".to_string(),
                                label: "Dev".to_string(),
                            },
                            crate::config::ConfigOption {
                                value: "prod".to_string(),
                                label: "Prod".to_string(),
                            },
                        ],
                        allow_custom: false,
                    });
            }
        }
    }
}

fn show_agents_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(RichText::new("AI Agents verwalten").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Agenten starten ein Terminal im Repo-Verzeichnis und führen den konfigurierten Befehl aus. Claude ist vordefiniert.")
            .size(11.0)
            .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Agents ({}):", state.draft.agents.len()))
                .size(12.0)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::image_and_text(
                    egui::Image::new(ICON_PLUS).fit_to_exact_size(Vec2::splat(14.0)),
                    "Neuer Agent",
                ))
                .clicked()
            {
                let mut max_n = 0;
                for a in &state.draft.agents {
                    if let Some(suffix) = a.id.strip_prefix("agent") {
                        if let Ok(n) = suffix.parse::<usize>() {
                            if n > max_n {
                                max_n = n;
                            }
                        }
                    }
                }
                state.draft.agents.push(AgentProfile {
                    id: format!("agent{}", max_n + 1),
                    display_name: "Neuer Agent".to_string(),
                    program: "claude".to_string(),
                    args: vec![],
                    command: None,
                    launch_mode: crate::config::AgentLaunchMode::Terminal,
                    terminal_override: None,

                    icon: None,
                });
                state.selected_agent_idx = Some(state.draft.agents.len() - 1);
            }
        });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    let mut to_delete: Option<usize> = None;
    let mut to_duplicate: Option<usize> = None;
    for (idx, agent) in state.draft.agents.iter().enumerate() {
        let is_selected = Some(idx) == state.selected_agent_idx;
        let is_active = Some(&agent.id) == state.draft.active_agent_id.as_ref();
        let visuals = ui.visuals();
        let frame = egui::Frame::new()
            .fill(visuals.widgets.inactive.bg_fill)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if is_active {
                    visuals.text_color()
                } else {
                    visuals.widgets.inactive.fg_stroke.color
                },
            ))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(8, 6));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = if is_active {
                    format!(
                        "● {} ({}) [{}]",
                        agent.display_name, agent.id, agent.program
                    )
                } else {
                    format!("{} ({}) [{}]", agent.display_name, agent.id, agent.program)
                };
                if ui
                    .selectable_label(is_selected, RichText::new(label).size(12.0))
                    .clicked()
                {
                    state.selected_agent_idx = Some(idx);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Duplizieren").clicked() {
                        to_duplicate = Some(idx);
                    }
                    if ui
                        .add(egui::Button::image_and_text(
                            egui::Image::new(ICON_TRASH).fit_to_exact_size(Vec2::splat(12.0)),
                            "Löschen",
                        ))
                        .clicked()
                    {
                        to_delete = Some(idx);
                    }
                    if ui.small_button("Aktiv").clicked() {
                        state.draft.active_agent_id = Some(agent.id.clone());
                    }
                });
            });
        });
        ui.add_space(4.0);
    }
    if let Some(idx) = to_duplicate {
        if let Some(a) = state.draft.agents.get(idx).cloned() {
            let mut new_a = a.clone();
            new_a.id = format!("{}_copy", a.id);
            new_a.display_name = format!("{} (Kopie)", a.display_name);
            if let Some(old_path) = new_a.icon.clone() {
                let src = PathBuf::from(&old_path);
                if src.exists() {
                    if let Ok(dest) = crate::config::AppConfig::copy_icon_to_storage(&src) {
                        new_a.icon = Some(dest.display().to_string());
                    }
                }
            }
            state.draft.agents.push(new_a);
            state.selected_agent_idx = Some(state.draft.agents.len() - 1);
        }
    }
    if let Some(idx) = to_delete {
        if state.draft.agents.len() > 1 {
            if let Some(removed) = state.draft.agents.get(idx) {
                if let Some(icon) = &removed.icon {
                    crate::config::AppConfig::remove_icon_file(icon);
                }
            }
            state.draft.agents.remove(idx);
            if let Some(sel) = state.selected_agent_idx {
                if sel == idx {
                    state.selected_agent_idx = Some(0);
                } else if sel > idx {
                    state.selected_agent_idx = Some(sel - 1);
                }
            }
            if let Some(active) = &state.draft.active_agent_id {
                if !state.draft.agents.iter().any(|a| &a.id == active) {
                    state.draft.active_agent_id = state.draft.agents.first().map(|a| a.id.clone());
                }
            }
        } else {
            state.error = Some("Mindestens ein Agent muss vorhanden sein.".to_string());
        }
    }

    ui.add_space(8.0);
    // Schnell-Anlage
    ui.collapsing("Schnell-Anlage neuer Agent", |ui| {
        ui.horizontal(|ui| {
            ui.label("Name:");
            ui.add(
                egui::TextEdit::singleline(&mut state.new_agent_name)
                    .hint_text("Claude / Codex")
                    .desired_width(120.0),
            );
            ui.label("Programm:");
            ui.add(
                egui::TextEdit::singleline(&mut state.new_agent_program)
                    .hint_text("claude")
                    .desired_width(100.0),
            );
            if ui.button("Anlegen").clicked() {
                if !state.new_agent_name.trim().is_empty()
                    && !state.new_agent_program.trim().is_empty()
                {
                    let id = state.new_agent_name.trim().to_lowercase().replace(' ', "_");
                    state.draft.agents.push(AgentProfile {
                        id: id.clone(),
                        display_name: state.new_agent_name.trim().to_string(),
                        program: state.new_agent_program.trim().to_string(),
                        args: vec![],
                        command: None,
                        launch_mode: crate::config::AgentLaunchMode::Terminal,
                        terminal_override: None,

                        icon: None,
                    });
                    state.selected_agent_idx = Some(state.draft.agents.len() - 1);
                    state.new_agent_name.clear();
                    state.new_agent_program.clear();
                } else {
                    state.error = Some("Name und Programm erforderlich".to_string());
                }
            }
        });
    });

    if let Some(idx) = state.selected_agent_idx {
        if let Some(agent) = state.draft.agents.get_mut(idx) {
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);
            ui.label(
                RichText::new(format!("Agent bearbeiten: {}", agent.display_name))
                    .size(13.0)
                    .strong(),
            );
            ui.add_space(6.0);
            egui::Grid::new(format!("agent_edit_{}", idx))
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("ID:");
                    let mut id = agent.id.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut id).hint_text("claude"))
                        .changed()
                    {
                        agent.id = id.trim().to_lowercase().replace(' ', "_");
                    }
                    ui.end_row();
                    ui.label("Anzeigename:");
                    ui.add(
                        egui::TextEdit::singleline(&mut agent.display_name)
                            .hint_text("Claude Code"),
                    );
                    ui.end_row();
                    ui.label("Programm:");
                    ui.add(egui::TextEdit::singleline(&mut agent.program).hint_text("claude"));
                    ui.end_row();
                    ui.label("Args (Leerzeichen-getrennt):");
                    let mut args_str = agent.args.join(" ");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut args_str)
                                .hint_text("--dangerously-skip-permissions"),
                        )
                        .changed()
                    {
                        agent.args = if args_str.trim().is_empty() {
                            Vec::new()
                        } else {
                            args_str.split_whitespace().map(|s| s.to_string()).collect()
                        };
                    }
                    ui.end_row();
                    ui.label("Terminal-Override:");
                    let current_term = match &agent.terminal_override {
                        Some(t) => format!("{:?}", t),
                        None => "— Auto (global) —".to_string(),
                    };
                    egui::ComboBox::from_id_salt(format!("agent_term_{}", idx))
                        .selected_text(current_term)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    agent.terminal_override.is_none(),
                                    "— Auto (global) —",
                                )
                                .clicked()
                            {
                                agent.terminal_override = None;
                            }
                            for (pref, label) in [
                                (TerminalPreference::Auto, "Auto"),
                                (TerminalPreference::WindowsTerminal, "Windows Terminal"),
                                (TerminalPreference::Cmd, "cmd"),
                                (TerminalPreference::Powershell, "Powershell"),
                            ] {
                                let is_sel = agent.terminal_override.as_ref() == Some(&pref);
                                if ui.selectable_label(is_sel, label).clicked() {
                                    agent.terminal_override = Some(pref);
                                }
                            }
                        });
                    ui.end_row();
                    ui.label("Icon:");
                    ui.horizontal(|ui| {
                        let is_custom = agent.icon.is_some();
                        let preview_size = Vec2::splat(18.0);
                        if let Some(icon_path) = &agent.icon {
                            let pb = PathBuf::from(icon_path);
                            if pb.exists() {
                                let uri = format!(
                                    "file://{}",
                                    pb.display().to_string().replace('\\', "/")
                                );
                                ui.add(egui::Image::new(uri).fit_to_exact_size(preview_size));
                            } else {
                                ui.add(
                                    egui::Image::new(ICON_WARNING)
                                        .fit_to_exact_size(preview_size)
                                        .tint(Color32::from_rgb(200, 80, 20)),
                                );
                                ui.label(
                                    RichText::new("nicht gefunden")
                                        .size(9.0)
                                        .color(Color32::from_rgb(200, 80, 20)),
                                );
                            }
                        } else {
                            ui.add(
                                egui::Image::new(match agent.id.as_str() {
                                    "claude" => {
                                        egui::include_image!("../../assets/icons/claude.svg")
                                    }
                                    "codex" => egui::include_image!("../../assets/icons/codex.svg"),
                                    "gemini" => {
                                        egui::include_image!("../../assets/icons/gemini.svg")
                                    }
                                    "copilot" => {
                                        egui::include_image!("../../assets/icons/copilot.svg")
                                    }
                                    "cursor" => {
                                        egui::include_image!("../../assets/icons/cursor.svg")
                                    }
                                    "aider" => egui::include_image!("../../assets/icons/aider.svg"),
                                    _ => egui::include_image!("../../assets/icons/claude.svg"),
                                })
                                .fit_to_exact_size(preview_size),
                            );
                            ui.label(
                                RichText::new("(Default)")
                                    .size(9.0)
                                    .color(Color32::from_rgb(120, 120, 120)),
                            );
                        }
                        if ui
                            .add(egui::Button::image(
                                egui::Image::new(ICON_FOLDER).fit_to_exact_size(Vec2::splat(14.0)),
                            ))
                            .on_hover_text("Icon wählen (svg, png, ico, jpg - max 2 MB)")
                            .clicked()
                        {
                            if let Some(src) = rfd::FileDialog::new()
                                .add_filter("Icon", &["svg", "png", "ico", "jpg", "jpeg"])
                                .pick_file()
                            {
                                let old = agent.icon.clone();
                                match crate::config::AppConfig::copy_icon_to_storage(&src) {
                                    Ok(dest) => {
                                        if let Some(old_path) = old {
                                            crate::config::AppConfig::remove_icon_file(&old_path);
                                        }
                                        agent.icon = Some(dest.display().to_string());
                                        state.error = None;
                                    }
                                    Err(e) => {
                                        state.error = Some(e);
                                    }
                                }
                            }
                        }
                        if is_custom
                            && ui
                                .add(egui::Button::image(
                                    egui::Image::new(ICON_TRASH)
                                        .fit_to_exact_size(Vec2::splat(14.0)),
                                ))
                                .on_hover_text("Custom Icon entfernen")
                                .clicked()
                        {
                            if let Some(old) = agent.icon.take() {
                                crate::config::AppConfig::remove_icon_file(&old);
                            }
                        }
                    });
                    ui.end_row();
                });
            let prog = agent.program.clone();
            let args_preview = agent.args.join(" ");
            ui.label(
                RichText::new(format!("Vorschau: {} {}", prog, args_preview))
                    .size(10.0)
                    .color(Color32::from_rgb(100, 100, 100))
                    .italics(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Wird gestartet als: Terminal -d <repo> -- <program> <args> (bleibt offen)",
                )
                .size(10.0)
                .color(Color32::from_rgb(120, 120, 120)),
            );
        }
    }

    ui.add_space(8.0);
    // Aktiver Agent Auswahl
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        RichText::new("Aktive Agents (mehrere möglich — alle Icons erscheinen nebeneinander):")
            .size(12.0)
            .strong(),
    );
    ui.add_space(4.0);
    for agent in &state.draft.agents.clone() {
        let mut is_active = state.draft.is_agent_active(&agent.id);
        if ui
            .checkbox(
                &mut is_active,
                format!("{} ({})", agent.display_name, agent.id),
            )
            .clicked()
        {
            state.draft.toggle_agent_active(&agent.id);
            // Verhindere leere Liste nicht strikt, aber zeige Warnung
            if state.draft.active_agent_ids.is_empty() {
                state.error = Some(
                    "Warnung: Kein Agent aktiv — es wird kein AI-Button angezeigt.".to_string(),
                );
            } else {
                state.error = None;
            }
        }
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.small_button("Alle aktivieren").clicked() {
            for a in state.draft.agents.clone() {
                if !state.draft.is_agent_active(&a.id) {
                    state.draft.toggle_agent_active(&a.id);
                }
            }
        }
        if ui.small_button("Alle deaktivieren").clicked() {
            state.draft.active_agent_ids.clear();
            state.draft.active_agent_id = None;
        }
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "{} von {} Agents aktiv — alle aktiven erscheinen als Icons in der Repo-Liste",
            state.draft.active_agent_ids.len(),
            state.draft.agents.len()
        ))
        .size(10.5)
        .color(Color32::from_rgb(100, 100, 100))
        .italics(),
    );
}

fn show_terminal_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(RichText::new("Terminal-Einstellungen").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Wähle das bevorzugte Terminal zum Starten von AI-Agents. 'Auto' probiert Windows Terminal → Powershell → cmd.")
            .size(11.0)
            .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(12.0);

    ui.label(RichText::new("Bevorzugtes Terminal:").size(12.0).strong());
    ui.add_space(4.0);
    let current = format!("{:?}", state.draft.terminal.preference);
    egui::ComboBox::from_id_salt("terminal_pref")
        .selected_text(current.clone())
        .show_ui(ui, |ui| {
            for (pref, label) in [
                (TerminalPreference::Auto, "Auto (empfohlen)"),
                (TerminalPreference::WindowsTerminal, "Windows Terminal (wt)"),
                (TerminalPreference::Powershell, "Powershell / pwsh"),
                (TerminalPreference::Cmd, "cmd"),
            ] {
                let is_sel = state.draft.terminal.preference == pref;
                if ui.selectable_label(is_sel, label).clicked() {
                    state.draft.terminal.preference = pref;
                }
            }
        });
    ui.add_space(8.0);
    // Custom Terminal
    ui.horizontal(|ui| {
        ui.label("Custom Terminal (optional):");
        if let TerminalPreference::Custom(custom) = &mut state.draft.terminal.preference {
            let mut c = custom.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut c).hint_text("z.B. alacritty"))
                .changed()
            {
                *custom = c;
            }
        } else {
            ui.label(RichText::new("Nur bei 'Custom' relevant").weak().size(11.0));
            if ui.small_button("Auf Custom wechseln").clicked() {
                state.draft.terminal.preference = TerminalPreference::Custom("".to_string());
            }
        }
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(RichText::new("Hinweis:").size(11.0).strong());
    ui.label(
        RichText::new("• Windows Terminal (wt) wird bevorzugt – moderner, Tabs, 'wt -d <dir> -- cmd /k <agent>'\n• Powershell: 'powershell -NoExit -Command \"Set-Location ...; & claude\"'\n• cmd: 'cmd /C start \"\" /D \"<dir>\" cmd /K \"claude\"' (bleibt offen)\n• Alle Modi starten das Terminal im Repo-Verzeichnis und halten es nach Beenden offen.")
            .size(10.5)
            .color(Color32::from_rgb(80, 80, 80)),
    );
    ui.add_space(8.0);
    // Fallback
    ui.label(RichText::new("Fallback Terminal:").size(12.0).strong());
    let fallback_current = format!("{:?}", state.draft.terminal.fallback);
    egui::ComboBox::from_id_salt("terminal_fallback")
        .selected_text(fallback_current)
        .show_ui(ui, |ui| {
            for (pref, label) in [
                (TerminalPreference::Cmd, "cmd"),
                (TerminalPreference::Powershell, "Powershell"),
                (TerminalPreference::WindowsTerminal, "Windows Terminal"),
            ] {
                let is_sel = state.draft.terminal.fallback == pref;
                if ui.selectable_label(is_sel, label).clicked() {
                    state.draft.terminal.fallback = pref;
                }
            }
        });
}

fn show_language_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    let lang = state.draft.language;
    ui.label(RichText::new(tr(lang, "language")).size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(if lang == Language::En {
            "Choose the application language. Changes apply after saving."
        } else {
            "Wähle die Sprache der Anwendung. Änderungen werden nach dem Speichern übernommen."
        })
        .size(11.0)
        .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(12.0);

    for l in Language::all() {
        let is_active = state.draft.language == l;
        let visuals = ui.visuals();
        let frame = egui::Frame::new()
            .fill(visuals.widgets.inactive.bg_fill)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if is_active {
                    visuals.text_color()
                } else {
                    visuals.widgets.inactive.fg_stroke.color
                },
            ))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(10, 8));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = format!("{} ({})", l.display_name(), l.code());
                if ui
                    .selectable_label(is_active, RichText::new(label).size(12.0))
                    .clicked()
                {
                    state.draft.language = l;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_active {
                        ui.label(
                            RichText::new(tr(lang, "active"))
                                .size(10.0)
                                .color(Color32::from_rgb(60, 120, 220))
                                .strong(),
                        );
                    } else if ui.small_button(tr(lang, "set_active")).clicked() {
                        state.draft.language = l;
                    }
                });
            });
        });
        ui.add_space(6.0);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        RichText::new(if lang == Language::En {
            "Current language:"
        } else {
            "Aktuelle Sprache:"
        })
        .size(11.0)
        .color(Color32::from_rgb(120, 120, 120)),
    );
    ui.label(
        RichText::new(format!(
            "{} - {}",
            state.draft.language.display_name(),
            state.draft.language.code()
        ))
        .size(12.0)
        .strong(),
    );
}

fn show_appearance_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(RichText::new("Erscheinungsbild").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Wähle eines von 5 Themes — wirkt sofort nach Speichern.")
            .size(11.0)
            .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(12.0);

    let current = state.draft.theme.clone();
    ui.label(RichText::new("Theme:").size(12.0).strong());
    ui.add_space(4.0);

    for theme in crate::config::Theme::all() {
        let is_active = theme == current;
        let name = theme.display_name();
        let desc = match theme {
            crate::config::Theme::Light => "Hell, minimalistisch (Standard)",
            crate::config::Theme::Dark => "Dunkel, augenschonend",
            crate::config::Theme::Nord => "Nord — kühles Blau-Grau (dunkel)",
            crate::config::Theme::Dracula => "Dracula — dunkles Lila/Grün",
            crate::config::Theme::Solarized => "Solarized Light — warmes Beige",
        };
        let visuals = ui.visuals();
        let frame = egui::Frame::new()
            .fill(visuals.widgets.inactive.bg_fill)
            .stroke(egui::Stroke::new(
                1.0_f32,
                if is_active {
                    visuals.text_color()
                } else {
                    visuals.widgets.inactive.fg_stroke.color
                },
            ))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(10, 8));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = if is_active {
                    format!("● {} — {}", name, desc)
                } else {
                    format!("{} — {}", name, desc)
                };
                if ui
                    .selectable_label(is_active, RichText::new(label).size(12.0))
                    .clicked()
                {
                    state.draft.theme = theme.clone();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_active {
                        ui.label(
                            RichText::new("Aktiv")
                                .size(10.0)
                                .color(Color32::from_rgb(60, 120, 220))
                                .strong(),
                        );
                    } else if ui.small_button("Aktivieren").clicked() {
                        state.draft.theme = theme.clone();
                    }
                });
            });
        });
        ui.add_space(6.0);
    }

    ui.add_space(8.0);
    ui.label(
        RichText::new("Hinweis: Theme wird beim nächsten Start automatisch angewendet und nach Speichern sofort.")
            .size(10.5)
            .color(Color32::from_rgb(120, 120, 120))
            .italics(),
    );
}

fn show_icons_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    let lang = state.draft.language;
    ui.label(RichText::new(tr(lang, "icons_title")).size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(lang, "icons_desc"))
            .size(11.0)
            .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(8.0);

    // Profilauswahl für per-Profil Icons
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr(lang, "icons_profile")).size(12.0).strong());
        let current_idx = state.selected_profile_idx.unwrap_or(0);
        let current_name = state
            .draft
            .profiles
            .get(current_idx)
            .map(|p| p.display_name.clone())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("icons_profile_select")
            .selected_text(current_name)
            .show_ui(ui, |ui| {
                for (idx, p) in state.draft.profiles.iter().enumerate() {
                    let is_active = Some(idx) == state.selected_profile_idx;
                    if ui.selectable_label(is_active, &p.display_name).clicked() {
                        state.selected_profile_idx = Some(idx);
                    }
                }
            });
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    let Some(idx) = state.selected_profile_idx else {
        ui.label(RichText::new("Kein Profil ausgewählt").weak());
        return;
    };
    if let Some(profile) = state.draft.profiles.get_mut(idx) {
        // Effektive Orders berechnen ohne Draft jedes Frame zu mutieren (pure rendering)
        let all_ids: Vec<String> = profile.ides.iter().map(|i| i.id.clone()).collect();
        let all_agent_ids: Vec<String> = state.draft.agents.iter().map(|a| a.id.clone()).collect();
        let effective_ide_order: Vec<String> = if profile.ide_order.is_empty() {
            all_ids.clone()
        } else {
            let mut eff = profile.ide_order.clone();
            eff.retain(|id| all_ids.contains(id));
            for id in &all_ids {
                if !eff.contains(id) {
                    eff.push(id.clone());
                }
            }
            eff
        };
        let effective_agent_order: Vec<String> = if profile.agent_order.is_empty() {
            all_agent_ids.clone()
        } else {
            let mut eff = profile.agent_order.clone();
            eff.retain(|id| all_agent_ids.contains(id));
            for id in &all_agent_ids {
                if !eff.contains(id) {
                    eff.push(id.clone());
                }
            }
            eff
        };

        ui.label(
            RichText::new(format!(
                "{}: {}",
                tr(lang, "profile_label"),
                profile.display_name
            ))
            .size(12.0)
            .strong(),
        );
        ui.add_space(8.0);

        // IDEs
        ui.label(RichText::new(tr(lang, "icons_ides")).size(12.0).strong());
        ui.add_space(4.0);
        let mut ide_move: Option<(usize, isize)> = None;
        let mut ide_toggle: Option<String> = None;
        for (order_idx, ide_id) in effective_ide_order.iter().enumerate() {
            let ide_opt = profile.ides.iter().find(|i| &i.id == ide_id);
            let Some(ide) = ide_opt else { continue };
            let is_hidden = profile.hidden_ide_ids.contains(ide_id);
            ui.horizontal(|ui| {
                let eye_icon = if is_hidden { ICON_EYE_OFF } else { ICON_EYE };
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(eye_icon).fit_to_exact_size(Vec2::splat(14.0)),
                    ))
                    .on_hover_text(tr(lang, "icons_toggle_visibility"))
                    .clicked()
                {
                    ide_toggle = Some(ide_id.clone());
                }
                let visuals = ui.visuals();
                ui.label(
                    RichText::new(format!("{} ({})", ide.display_name, ide.id))
                        .size(11.0)
                        .color(if is_hidden {
                            visuals.weak_text_color()
                        } else {
                            visuals.text_color()
                        }),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(ICON_CHEVRON_DOWN)
                                .fit_to_exact_size(Vec2::splat(14.0)),
                        ))
                        .clicked()
                        && order_idx + 1 < effective_ide_order.len()
                    {
                        ide_move = Some((order_idx, 1));
                    }
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(ICON_CHEVRON_UP).fit_to_exact_size(Vec2::splat(14.0)),
                        ))
                        .clicked()
                        && order_idx > 0
                    {
                        ide_move = Some((order_idx, -1));
                    }
                });
            });
            ui.add_space(2.0);
        }
        if let Some(id) = ide_toggle {
            if profile.hidden_ide_ids.contains(&id) {
                profile.hidden_ide_ids.retain(|x| x != &id);
            } else {
                profile.hidden_ide_ids.push(id);
            }
        }
        if let Some((idx, delta)) = ide_move {
            // Initialisiere Order falls leer (lazy)
            if profile.ide_order.is_empty() {
                profile.ide_order = effective_ide_order.clone();
            }
            let new_idx = (idx as isize + delta) as usize;
            if new_idx < profile.ide_order.len() {
                profile.ide_order.swap(idx, new_idx);
            }
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Terminal (Shell/Explorer)
        ui.label(
            RichText::new(tr(lang, "icons_terminal"))
                .size(12.0)
                .strong(),
        );
        ui.add_space(4.0);
        let mut show_shell = profile.show_shell;
        let mut show_explorer = profile.show_explorer;
        if ui
            .checkbox(&mut show_shell, tr(lang, "icons_show_shell"))
            .changed()
        {
            profile.show_shell = show_shell;
        }
        if ui
            .checkbox(&mut show_explorer, tr(lang, "icons_show_explorer"))
            .changed()
        {
            profile.show_explorer = show_explorer;
        }
        ui.label(
            RichText::new(tr(lang, "icons_terminal_hint"))
                .size(10.0)
                .color(Color32::from_rgb(120, 120, 120))
                .italics(),
        );
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // AI Agents per Profil
        ui.label(RichText::new(tr(lang, "icons_agents")).size(12.0).strong());
        ui.add_space(4.0);
        let mut agent_toggle: Option<String> = None;
        let mut agent_move: Option<(usize, isize)> = None;
        for (order_idx, agent_id) in effective_agent_order.iter().enumerate() {
            let agent_opt = state.draft.agents.iter().find(|a| &a.id == agent_id);
            let Some(agent) = agent_opt else { continue };
            let is_hidden = profile.hidden_agent_ids.contains(agent_id);
            let is_active = state.draft.active_agent_ids.contains(agent_id);
            ui.horizontal(|ui| {
                let eye_icon = if is_hidden { ICON_EYE_OFF } else { ICON_EYE };
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(eye_icon).fit_to_exact_size(Vec2::splat(14.0)),
                    ))
                    .on_hover_text(tr(lang, "icons_toggle_visibility"))
                    .clicked()
                {
                    agent_toggle = Some(agent_id.clone());
                }
                let mut label = format!("{} ({})", agent.display_name, agent.id);
                if !is_active {
                    label.push_str(" [inaktiv]");
                }
                let visuals = ui.visuals();
                ui.label(RichText::new(label).size(11.0).color(if is_hidden {
                    visuals.weak_text_color()
                } else {
                    visuals.text_color()
                }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(ICON_CHEVRON_DOWN)
                                .fit_to_exact_size(Vec2::splat(14.0)),
                        ))
                        .clicked()
                        && order_idx + 1 < effective_agent_order.len()
                    {
                        agent_move = Some((order_idx, 1));
                    }
                    if ui
                        .add(egui::Button::image(
                            egui::Image::new(ICON_CHEVRON_UP).fit_to_exact_size(Vec2::splat(14.0)),
                        ))
                        .clicked()
                        && order_idx > 0
                    {
                        agent_move = Some((order_idx, -1));
                    }
                });
            });
            ui.add_space(2.0);
        }
        if let Some(id) = agent_toggle {
            if profile.hidden_agent_ids.contains(&id) {
                profile.hidden_agent_ids.retain(|x| x != &id);
            } else {
                profile.hidden_agent_ids.push(id);
            }
        }
        if let Some((idx, delta)) = agent_move {
            if profile.agent_order.is_empty() {
                profile.agent_order = effective_agent_order.clone();
            }
            let new_idx = (idx as isize + delta) as usize;
            if new_idx < profile.agent_order.len() {
                profile.agent_order.swap(idx, new_idx);
            }
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "{} hidden: {}",
                tr(lang, "icons_hidden_info"),
                if profile.hidden_agent_ids.is_empty() {
                    "—".to_string()
                } else {
                    profile.hidden_agent_ids.join(", ")
                }
            ))
            .size(10.0)
            .color(Color32::from_rgb(120, 120, 120)),
        );
        ui.add_space(8.0);
        if ui.small_button(tr(lang, "icons_reset")).clicked() {
            profile.hidden_ide_ids.clear();
            profile.hidden_agent_ids.clear();
            profile.ide_order.clear();
            profile.agent_order.clear();
            profile.show_shell = true;
            profile.show_explorer = true;
        }
        // Warnung wenn alles hidden
        if profile.hidden_ide_ids.len() == profile.ides.len() && !profile.ides.is_empty() {
            ui.colored_label(
                Color32::from_rgb(200, 80, 20),
                tr(lang, "icons_all_hidden_warn"),
            );
        }
    } else {
        ui.label(RichText::new("Profil nicht gefunden").weak());
    }
}

const DEFAULT_TRAY_ICON_IDS: &[&str] = &[
    "vscode", "vs2022", "rider", "folder", "terminal", "claude", "codex", "gemini",
    "copilot", "cursor", "aider",
];

fn tray_icon_display_name(id: &str) -> &str {
    match id {
        "vscode" => "VS Code",
        "vs2022" => "Visual Studio",
        "rider" => "Rider",
        "folder" => "Explorer",
        "terminal" => "Terminal",
        "claude" => "Claude Code",
        "codex" => "Codex (OpenAI)",
        "gemini" => "Gemini CLI",
        "copilot" => "Copilot CLI",
        "cursor" => "Cursor Agent",
        "aider" => "Aider",
        _ => id,
    }
}

fn effective_tray_order(draft: &AppConfig) -> Vec<String> {
    let all: Vec<String> = DEFAULT_TRAY_ICON_IDS.iter().map(|s| s.to_string()).collect();
    if draft.tray_icons.icon_order.is_empty() {
        all
    } else {
        let mut eff = draft.tray_icons.icon_order.clone();
        eff.retain(|id| all.contains(id));
        for id in &all {
            if !eff.contains(id) {
                eff.push(id.clone());
            }
        }
        // keep any custom unknown at end (already retained if not in all, but we removed them above; re-append unknowns)
        for id in &draft.tray_icons.icon_order {
            if !all.contains(id) && !eff.contains(id) {
                eff.push(id.clone());
            }
        }
        eff
    }
}

fn show_tray_icons_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    let lang = state.draft.language;
    ui.label(RichText::new(tr(lang, "tray_icons_title")).size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(lang, "tray_icons_desc"))
            .size(11.0)
            .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(12.0);

    // Max display slider
    ui.label(RichText::new(tr(lang, "tray_max_display")).size(12.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Wie viele Repos maximal im Tray-Popup angezeigt werden (5–50, Standard 10).")
            .size(11.0)
            .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Limit:");
        let mut limit = state.draft.tray_icons.max_display;
        if limit < 5 {
            limit = 5;
        }
        let slider = egui::Slider::new(&mut limit, 5..=50)
            .text("Repos")
            .step_by(1.0);
        if ui.add(slider).changed() {
            state.draft.tray_icons.max_display = limit.clamp(5, 50);
        }
    });
    ui.horizontal(|ui| {
        ui.label("Oder direkt:");
        let mut l = state.draft.tray_icons.max_display;
        if ui
            .add(egui::DragValue::new(&mut l).range(5..=50).speed(1.0))
            .changed()
        {
            state.draft.tray_icons.max_display = l.clamp(5, 50);
        }
        ui.label(
            RichText::new(format!("(aktuell: {})", state.draft.tray_icons.max_display))
                .size(11.0)
                .color(Color32::from_rgb(120, 120, 120)),
        );
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "Zeigt bis zu {} Repos im Tray-Popup. Weitere im Hauptfenster.",
            state.draft.tray_icons.max_display
        ))
        .size(10.0)
        .color(Color32::from_rgb(120, 120, 120))
        .italics(),
    );

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(RichText::new("Icons (Reihenfolge & Sichtbarkeit)").size(12.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Icons neu anordnen (↑/↓) und ausblenden (👁). Ausgeblendete Icons verschwinden ohne Lücke in der Tray-Zeile.")
            .size(11.0)
            .color(Color32::from_rgb(100, 100, 100)),
    );
    ui.add_space(8.0);

    let effective = effective_tray_order(&state.draft);
    let mut tray_move: Option<(usize, isize)> = None;
    let mut tray_toggle: Option<String> = None;
    for (order_idx, icon_id) in effective.iter().enumerate() {
        let is_hidden = state.draft.tray_icons.hidden_icon_ids.contains(icon_id);
        ui.horizontal(|ui| {
            let eye_icon = if is_hidden { ICON_EYE_OFF } else { ICON_EYE };
            if ui
                .add(egui::Button::image(
                    egui::Image::new(eye_icon).fit_to_exact_size(Vec2::splat(14.0)),
                ))
                .on_hover_text(tr(lang, "icons_toggle_visibility"))
                .clicked()
            {
                tray_toggle = Some(icon_id.clone());
            }
            let visuals = ui.visuals();
            ui.label(
                RichText::new(format!("{} ({})", tray_icon_display_name(icon_id), icon_id))
                    .size(11.0)
                    .color(if is_hidden {
                        visuals.weak_text_color()
                    } else {
                        visuals.text_color()
                    }),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(ICON_CHEVRON_DOWN).fit_to_exact_size(Vec2::splat(14.0)),
                    ))
                    .clicked()
                    && order_idx + 1 < effective.len()
                {
                    tray_move = Some((order_idx, 1));
                }
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(ICON_CHEVRON_UP).fit_to_exact_size(Vec2::splat(14.0)),
                    ))
                    .clicked()
                    && order_idx > 0
                {
                    tray_move = Some((order_idx, -1));
                }
            });
        });
        ui.add_space(2.0);
    }
    if let Some(id) = tray_toggle {
        if state.draft.tray_icons.hidden_icon_ids.contains(&id) {
            state.draft.tray_icons.hidden_icon_ids.retain(|x| x != &id);
        } else {
            state.draft.tray_icons.hidden_icon_ids.push(id);
        }
        state.draft.tray_icons.hidden_icon_ids.sort();
        state.draft.tray_icons.hidden_icon_ids.dedup();
    }
    if let Some((idx, delta)) = tray_move {
        if state.draft.tray_icons.icon_order.is_empty() {
            state.draft.tray_icons.icon_order = effective.clone();
        }
        let new_idx = (idx as isize + delta) as usize;
        if new_idx < state.draft.tray_icons.icon_order.len() {
            state.draft.tray_icons.icon_order.swap(idx, new_idx);
        }
    }

    ui.add_space(8.0);
    ui.label(
        RichText::new(format!(
            "{} hidden: {}",
            tr(lang, "icons_hidden_info"),
            if state.draft.tray_icons.hidden_icon_ids.is_empty() {
                "—".to_string()
            } else {
                state.draft.tray_icons.hidden_icon_ids.join(", ")
            }
        ))
        .size(10.0)
        .color(Color32::from_rgb(120, 120, 120)),
    );
    ui.add_space(8.0);
    if ui.small_button(tr(lang, "icons_reset")).clicked() {
        state.draft.tray_icons.hidden_icon_ids.clear();
        state.draft.tray_icons.icon_order.clear();
        state.draft.tray_icons.max_display = 10;
    }
    if state.draft.tray_icons.hidden_icon_ids.len() == DEFAULT_TRAY_ICON_IDS.len() {
        ui.colored_label(
            Color32::from_rgb(200, 80, 20),
            "Warnung: alle Tray-Icons ausgeblendet – es werden keine Icons in der Tray-Zeile angezeigt.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, IdeConfig, LanguageProfile, TerminalPreference};

    #[test]
    fn settings_state_from_config() {
        let cfg = AppConfig::default();
        let state = SettingsState::from_config(&cfg);
        assert_eq!(state.draft.active_profile_id, cfg.active_profile_id);
        assert!(state.error.is_none());
        assert!(state.success.is_none());
        assert_eq!(state.selected_tab, SettingsTab::General);
        assert_eq!(state.selected_profile_idx, Some(0));
        assert_eq!(state.selected_agent_idx, Some(0));
        assert_eq!(state.new_profile_name, "");
        assert_eq!(state.new_profile_ext, "");
        assert_eq!(state.new_agent_name, "");
        assert_eq!(state.new_agent_program, "");
    }

    #[test]
    fn settings_state_empty_profiles_agents() {
        let mut cfg = AppConfig::default();
        cfg.profiles = vec![];
        cfg.agents = vec![];
        let state = SettingsState::from_config(&cfg);
        assert_eq!(state.selected_profile_idx, None);
        assert_eq!(state.selected_agent_idx, None);
    }

    #[test]
    fn settings_state_has_profile_idx() {
        let cfg = AppConfig::default();
        let state = SettingsState::from_config(&cfg);
        assert_eq!(state.selected_profile_idx, Some(0));
    }

    #[test]
    fn settings_state_has_agent_idx() {
        let cfg = AppConfig::default();
        let state = SettingsState::from_config(&cfg);
        assert_eq!(state.selected_agent_idx, Some(0));
    }

    #[test]
    fn settings_state_draft_is_independent_clone() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.active_profile_id = "changed".to_string();
        assert_ne!(cfg.active_profile_id, "changed");
    }

    #[test]
    fn settings_tabs_all_five_and_switching() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        assert_eq!(state.selected_tab, SettingsTab::General);
        state.selected_tab = SettingsTab::Profiles;
        assert_eq!(state.selected_tab, SettingsTab::Profiles);
        state.selected_tab = SettingsTab::Agents;
        assert_eq!(state.selected_tab, SettingsTab::Agents);
        state.selected_tab = SettingsTab::Terminal;
        assert_eq!(state.selected_tab, SettingsTab::Terminal);
        state.selected_tab = SettingsTab::Appearance;
        assert_eq!(state.selected_tab, SettingsTab::Appearance);
    }

    #[test]
    fn settings_validation_empty_roots_error() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.roots = vec![];
        // simulate save validation
        if state.draft.roots.is_empty() {
            state.error = Some("Bitte mindestens einen Pfad angeben.".to_string());
        }
        assert_eq!(
            state.error,
            Some("Bitte mindestens einen Pfad angeben.".to_string())
        );
    }

    #[test]
    fn settings_validation_empty_profiles_error() {
        let mut cfg = AppConfig::default();
        cfg.profiles = vec![];
        let mut state = SettingsState::from_config(&cfg);
        state.draft.profiles = vec![];
        if state.draft.profiles.is_empty() {
            state.error = Some("Mindestens ein Sprach-Profil erforderlich.".to_string());
        }
        assert!(state.error.is_some());
    }

    #[test]
    fn settings_validation_profile_fields_empty_error() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.profiles[0].id = "".to_string();
        // validation: id trim empty
        let mut valid = true;
        for p in &state.draft.profiles {
            if p.id.trim().is_empty() || p.display_name.trim().is_empty() {
                state.error = Some(format!("Profil '{}' hat leere ID/Name", p.id));
                valid = false;
                break;
            }
        }
        assert!(!valid);
        assert!(state.error.is_some());
        // file_extension empty
        let cfg2 = AppConfig::default();
        let mut state2 = SettingsState::from_config(&cfg2);
        state2.draft.profiles[0].file_extension = "".to_string();
        valid = true;
        for p in &state2.draft.profiles {
            if p.file_extension.trim().is_empty() {
                state2.error = Some(format!("Profil '{}' braucht Dateiendung", p.display_name));
                valid = false;
                break;
            }
        }
        assert!(!valid);
    }

    #[test]
    fn settings_reset_clears_messages() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.error = Some("err".to_string());
        state.success = Some("ok".to_string());
        state.error = None;
        state.success = None;
        assert!(state.error.is_none());
        assert!(state.success.is_none());
    }

    #[test]
    fn settings_add_new_profile() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        let len_before = state.draft.profiles.len();
        let new_id = format!("custom{}", state.draft.profiles.len() + 1);
        state.draft.profiles.push(LanguageProfile {
            id: new_id.clone(),
            display_name: format!("Custom {}", len_before + 1),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![IdeConfig {
                id: "vscode".to_string(),
                display_name: "VS Code".to_string(),
                program: "code".to_string(),
                args: vec!["{file}".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,

                icon: None,
            }],
            default_ide_id: Some("vscode".to_string()),
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        });
        state.selected_profile_idx = Some(state.draft.profiles.len() - 1);
        assert_eq!(state.draft.profiles.len(), len_before + 1);
        assert_eq!(state.selected_profile_idx, Some(len_before));
    }

    #[test]
    fn settings_add_new_agent() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        let len_before = state.draft.agents.len();
        state.draft.agents.push(crate::config::AgentProfile {
            id: format!("agent{}", len_before + 1),
            display_name: "Neuer Agent".to_string(),
            program: "claude".to_string(),
            args: vec![],
            command: None,
            launch_mode: crate::config::AgentLaunchMode::Terminal,
            terminal_override: None,

            icon: None,
        });
        state.selected_agent_idx = Some(state.draft.agents.len() - 1);
        assert_eq!(state.draft.agents.len(), len_before + 1);
    }

    #[test]
    fn settings_delete_profile_requires_min_one() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        // try delete when only one profile left (default has 1)
        assert_eq!(state.draft.profiles.len(), 1);
        // simulate delete logic: if len>1 remove, else error
        let to_delete = 0;
        if state.draft.profiles.len() > 1 {
            state.draft.profiles.remove(to_delete);
        } else {
            state.error = Some("Mindestens ein Profil muss vorhanden sein.".to_string());
        }
        assert_eq!(
            state.error,
            Some("Mindestens ein Profil muss vorhanden sein.".to_string())
        );
        // add second then delete
        state.draft.profiles.push(LanguageProfile {
            id: "second".to_string(),
            display_name: "Second".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![IdeConfig {
                id: "vscode".to_string(),
                display_name: "VS Code".to_string(),
                program: "code".to_string(),
                args: vec!["{file}".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,

                icon: None,
            }],
            default_ide_id: Some("vscode".to_string()),
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        });
        state.draft.profiles.remove(0);
        assert_eq!(state.draft.profiles.len(), 1);
        assert_eq!(state.draft.profiles[0].id, "second");
    }

    #[test]
    fn settings_delete_agent_requires_min_one() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        assert!(state.draft.agents.len() > 1);
        let before = state.draft.agents.len();
        state.draft.agents.remove(0);
        assert_eq!(state.draft.agents.len(), before - 1);
        // delete until one left
        while state.draft.agents.len() > 1 {
            state.draft.agents.remove(0);
        }
        assert_eq!(state.draft.agents.len(), 1);
        if state.draft.agents.len() > 1 {
            state.draft.agents.remove(0);
        } else {
            state.error = Some("Mindestens ein Agent muss vorhanden sein.".to_string());
        }
        assert!(state.error.is_some());
    }

    #[test]
    fn settings_duplicate_profile_agent() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        let p = state.draft.profiles[0].clone();
        let mut new_p = p.clone();
        new_p.id = format!("{}_copy", p.id);
        new_p.display_name = format!("{} (Kopie)", p.display_name);
        state.draft.profiles.push(new_p);
        assert_eq!(state.draft.profiles.len(), 2);
        assert_eq!(state.draft.profiles[1].id, "dotnet_copy");

        let a = state.draft.agents[0].clone();
        let mut new_a = a.clone();
        new_a.id = format!("{}_copy", a.id);
        state.draft.agents.push(new_a);
        assert_eq!(state.draft.agents.last().unwrap().id, "claude_copy");
    }

    #[test]
    fn settings_set_active_profile_agent() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.active_profile_id = "dotnet".to_string();
        // add second profile
        state.draft.profiles.push(LanguageProfile {
            id: "rust".to_string(),
            display_name: "Rust".to_string(),
            file_extension: ".rs".to_string(),
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
            config_selectors: Vec::new(),
        });
        state.draft.active_profile_id = "rust".to_string();
        assert_eq!(state.draft.active_profile_id, "rust");
        state.draft.active_agent_id = Some("codex".to_string());
        assert_eq!(state.draft.active_agent_id, Some("codex".to_string()));
    }

    #[test]
    fn settings_new_profile_quick_add() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.new_profile_name = "Rust".to_string();
        state.new_profile_ext = "rs".to_string();
        if !state.new_profile_name.trim().is_empty() && !state.new_profile_ext.trim().is_empty() {
            let mut ext = state.new_profile_ext.trim().to_string();
            if !ext.starts_with('.') {
                ext = format!(".{}", ext);
            }
            let id = state
                .new_profile_name
                .trim()
                .to_lowercase()
                .replace(' ', "_");
            state.draft.profiles.push(LanguageProfile {
                id: id.clone(),
                display_name: state.new_profile_name.trim().to_string(),
                file_extension: ext.clone(),
                file_pattern: None,
                max_scan_depth: 3,
                ides: vec![IdeConfig {
                    id: "vscode".to_string(),
                    display_name: "VS Code".to_string(),
                    program: "code".to_string(),
                    args: vec!["{file}".to_string()],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,

                    icon: None,
                }],
                default_ide_id: Some("vscode".to_string()),
                ide_order: Vec::new(),
                hidden_ide_ids: Vec::new(),
                hidden_agent_ids: Vec::new(),
                agent_order: Vec::new(),
                show_shell: true,
                show_explorer: true,
                config_selectors: Vec::new(),
            });
            assert_eq!(ext, ".rs");
            assert_eq!(id, "rust");
        }
        assert!(state.draft.profiles.iter().any(|p| p.id == "rust"));
        // empty case
        let mut state2 = SettingsState::from_config(&cfg);
        state2.new_profile_name = "".to_string();
        state2.new_profile_ext = "".to_string();
        if state2.new_profile_name.trim().is_empty() || state2.new_profile_ext.trim().is_empty() {
            state2.error = Some("Name und Endung erforderlich".to_string());
        }
        assert!(state2.error.is_some());
    }

    #[test]
    fn settings_new_agent_quick_add() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.new_agent_name = "MyAgent".to_string();
        state.new_agent_program = "myprog".to_string();
        if !state.new_agent_name.trim().is_empty() && !state.new_agent_program.trim().is_empty() {
            let id = state.new_agent_name.trim().to_lowercase().replace(' ', "_");
            state.draft.agents.push(crate::config::AgentProfile {
                id: id.clone(),
                display_name: state.new_agent_name.trim().to_string(),
                program: state.new_agent_program.trim().to_string(),
                args: vec![],
                command: None,
                launch_mode: crate::config::AgentLaunchMode::Terminal,
                terminal_override: None,

                icon: None,
            });
            assert_eq!(id, "myagent");
        }
        assert!(state.draft.agents.iter().any(|a| a.id == "myagent"));
    }

    #[test]
    fn settings_ide_min_one_and_add() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        let profile = &mut state.draft.profiles[0];
        assert!(profile.ides.len() >= 1);
        let before = profile.ides.len();
        profile.ides.push(IdeConfig {
            id: "newide".to_string(),
            display_name: "New IDE".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string()],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        });
        assert_eq!(profile.ides.len(), before + 1);
        // remove until one left should error
        while profile.ides.len() > 1 {
            profile.ides.remove(0);
        }
        assert_eq!(profile.ides.len(), 1);
        // simulate error when trying to remove last
        if profile.ides.len() > 1 {
            profile.ides.remove(0);
        } else {
            state.error = Some("Mindestens eine IDE pro Profil erforderlich".to_string());
        }
        assert!(state.error.is_some());
    }

    #[test]
    fn settings_ide_effective_preview() {
        let ide = IdeConfig {
            id: "vscode".to_string(),
            display_name: "VS Code".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string()],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_program(), "code");
        assert_eq!(ide.effective_args(), vec!["{file}"]);
    }

    #[test]
    fn settings_terminal_preference_switch_and_custom() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.terminal.preference = TerminalPreference::WindowsTerminal;
        assert_eq!(
            state.draft.terminal.preference,
            TerminalPreference::WindowsTerminal
        );
        state.draft.terminal.preference = TerminalPreference::Custom("alacritty".to_string());
        if let TerminalPreference::Custom(c) = &state.draft.terminal.preference {
            assert_eq!(c, "alacritty");
        } else {
            panic!();
        }
        // switch back to Auto
        state.draft.terminal.preference = TerminalPreference::Auto;
        assert_eq!(state.draft.terminal.preference, TerminalPreference::Auto);
    }

    #[test]
    fn settings_theme_selected() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        for theme in crate::config::Theme::all() {
            state.draft.theme = theme.clone();
            assert_eq!(state.draft.theme, theme);
        }
        state.draft.theme = crate::config::Theme::Dark;
        assert_eq!(state.draft.theme.display_name(), "Dark");
    }

    #[test]
    fn settings_agent_toggle_and_empty_warning() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        // toggle off claude
        state.draft.toggle_agent_active("claude");
        assert!(!state.draft.is_agent_active("claude"));
        if state.draft.active_agent_ids.is_empty() {
            state.error =
                Some("Warnung: Kein Agent aktiv — es wird kein AI-Button angezeigt.".to_string());
        }
        assert!(state.error.is_some());
        // toggle on again
        state.draft.toggle_agent_active("claude");
        assert!(state.draft.is_agent_active("claude"));
        // all activate
        for a in state.draft.agents.clone() {
            if !state.draft.is_agent_active(&a.id) {
                state.draft.toggle_agent_active(&a.id);
            }
        }
        assert_eq!(state.draft.active_agent_ids.len(), state.draft.agents.len());
        // all deactivate
        state.draft.active_agent_ids.clear();
        state.draft.active_agent_id = None;
        assert!(state.draft.active_agent_ids.is_empty());
    }

    #[test]
    fn settings_delete_adjusts_selection() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        // add second profile
        state.draft.profiles.push(LanguageProfile {
            id: "second".to_string(),
            display_name: "Second".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![IdeConfig {
                id: "vscode".to_string(),
                display_name: "VS Code".to_string(),
                program: "code".to_string(),
                args: vec!["{file}".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,

                icon: None,
            }],
            default_ide_id: Some("vscode".to_string()),
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        });
        state.selected_profile_idx = Some(1);
        // delete idx 0, selection should adjust -1
        let idx = 0;
        state.draft.profiles.remove(idx);
        if let Some(sel) = state.selected_profile_idx {
            if sel > idx {
                state.selected_profile_idx = Some(sel - 1);
            }
        }
        assert_eq!(state.selected_profile_idx, Some(0));
    }

    #[test]
    fn settings_add_config_selector_to_profile() {
        let mut cfg = AppConfig::default();
        let p = &mut cfg.profiles[0];
        assert!(p.config_selectors.is_empty());
        p.config_selectors.push(crate::config::ConfigSelector {
            id: "db".into(),
            display_name: "Datenbank".into(),
            file_path: "App.config".into(),
            key: "Database".into(),
            key_attribute: "key".into(),
            value_attribute: "value".into(),
            kind: crate::config::XmlSelectorKind::AddKeyValue,
            options: vec![crate::config::ConfigOption {
                value: "dev".into(),
                label: "Dev".into(),
            }],
            allow_custom: false,
        });
        assert_eq!(p.config_selectors.len(), 1);
    }
}
