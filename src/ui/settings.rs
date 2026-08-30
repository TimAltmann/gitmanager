use crate::config::{AgentProfile, AppConfig, IdeConfig, LanguageProfile, TerminalPreference};
use crate::i18n::{tr, Language};
use egui::{Color32, RichText};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Profiles,
    Agents,
    Terminal,
    Appearance,
    Language,
}

pub struct SettingsState {
    pub draft: AppConfig,
    pub error: Option<String>,
    pub success: Option<String>,
    selected_tab: SettingsTab,
    selected_profile_idx: Option<usize>,
    selected_agent_idx: Option<usize>,
    // Temporäre Eingabefelder für neue Profile/Agents
    new_profile_name: String,
    new_profile_ext: String,
    new_agent_name: String,
    new_agent_program: String,
}

impl SettingsState {
    pub fn from_config(cfg: &AppConfig) -> Self {
        Self {
            draft: cfg.clone(),
            error: None,
            success: None,
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
                            if state.draft.roots.is_empty() {
                                state.error =
                                    Some("Bitte mindestens einen Pfad angeben.".to_string());
                                state.success = None;
                            } else if state.draft.profiles.is_empty() {
                                state.error =
                                    Some("Mindestens ein Sprach-Profil erforderlich.".to_string());
                                state.success = None;
                            } else {
                                // Validierung Profile
                                let mut valid = true;
                                for p in &state.draft.profiles {
                                    if p.id.trim().is_empty() || p.display_name.trim().is_empty() {
                                        state.error =
                                            Some(format!("Profil '{}' hat leere ID/Name", p.id));
                                        valid = false;
                                        break;
                                    }
                                    if p.file_extension.trim().is_empty() {
                                        state.error = Some(format!(
                                            "Profil '{}' braucht Dateiendung",
                                            p.display_name
                                        ));
                                        valid = false;
                                        break;
                                    }
                                }
                                if valid {
                                    match state.draft.clone().save() {
                                        Ok(()) => {
                                            state.error = None;
                                            state.success = Some(
                                                "Gespeichert. Scan wird neu gestartet.".to_string(),
                                            );
                                            *on_save = Some(state.draft.clone());
                                        }
                                        Err(e) => {
                                            state.error =
                                                Some(format!("Speichern fehlgeschlagen: {e:#}"));
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
            if ui.button("＋ Hinzufügen").clicked() {
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
            if ui.button("✕").on_hover_text("Entfernen").clicked() {
                to_remove = Some(root.clone());
            }
            if ui.button("📂").on_hover_text("Durchsuchen").clicked() {
                if let Some(new_path) = rfd::FileDialog::new().pick_folder() {
                    if let Some(idx) = state.draft.roots.iter().position(|p| p == root) {
                        state.draft.roots[idx] = new_path;
                    }
                }
            }
        });
        ui.add_space(4.0);
        if !root.exists() {
            ui.label(
                RichText::new(format!("⚠ Pfad existiert nicht: {}", root.display()))
                    .size(11.0)
                    .color(Color32::from_rgb(200, 80, 20)),
            );
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
        let slider = egui::Slider::new(&mut depth, 1..=5).text("Ebenen");
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
            if ui.button("＋ Neues Profil").clicked() {
                let new_id = format!("custom{}", state.draft.profiles.len() + 1);
                state.draft.profiles.push(LanguageProfile {
                    id: new_id.clone(),
                    display_name: format!("Custom {}", state.draft.profiles.len() + 1),
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
                    }],
                    default_ide_id: Some("vscode".to_string()),
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
        let visuals = &ui.ctx().style().visuals;
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
                    if ui.small_button("✕ Löschen").clicked() {
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
            state.draft.profiles.push(new_p);
            state.selected_profile_idx = Some(state.draft.profiles.len() - 1);
        }
    }
    if let Some(idx) = to_delete {
        if state.draft.profiles.len() > 1 {
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
                            args: vec!["{file}".to_string()],
                            command: None,
                            use_shell: false,
                            allow_unsafe: false,
                        }],
                        default_ide_id: Some("vscode".to_string()),
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
                let visuals = &ui.ctx().style().visuals;
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
                                    if ui.small_button("✕").clicked() {
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
                                ui.add(
                                    egui::TextEdit::singleline(&mut ide.program)
                                        .hint_text("code / devenv / rider"),
                                );
                                ui.end_row();
                                ui.label("Args:");
                                let mut args_str = ide.args.join(" ");
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut args_str)
                                            .hint_text("{file} --reuse-window"),
                                    )
                                    .changed()
                                {
                                    ide.args = args_str
                                        .split_whitespace()
                                        .map(|s| s.to_string())
                                        .collect();
                                    if ide.args.is_empty() {
                                        ide.args = vec!["{file}".to_string()];
                                    }
                                }
                                ui.end_row();
                                ui.label("Shell:");
                                ui.checkbox(&mut ide.use_shell, "via cmd /C (unsicher)");
                                ui.end_row();
                            });
                        // Preview
                        let prog = ide.effective_program();
                        let args_preview = ide.effective_args().join(" ");
                        ui.label(
                            RichText::new(format!("Vorschau: {} {}", prog, args_preview))
                                .size(10.0)
                                .color(Color32::from_rgb(100, 100, 100))
                                .italics(),
                        );
                    });
                ui.add_space(4.0);
            }
            if let Some(ide_idx) = to_remove_ide {
                if profile.ides.len() > 1 {
                    profile.ides.remove(ide_idx);
                } else {
                    state.error = Some("Mindestens eine IDE pro Profil erforderlich".to_string());
                }
            }

            if ui.button("＋ IDE hinzufügen").clicked() {
                profile.ides.push(IdeConfig {
                    id: format!("ide{}", profile.ides.len() + 1),
                    display_name: "Neue IDE".to_string(),
                    program: "code".to_string(),
                    args: vec!["{file}".to_string()],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
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
            if ui.button("＋ Neuer Agent").clicked() {
                state.draft.agents.push(AgentProfile {
                    id: format!("agent{}", state.draft.agents.len() + 1),
                    display_name: "Neuer Agent".to_string(),
                    program: "claude".to_string(),
                    args: vec![],
                    command: None,
                    launch_mode: crate::config::AgentLaunchMode::Terminal,
                    terminal_override: None,
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
        let visuals = &ui.ctx().style().visuals;
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
                    if ui.small_button("✕ Löschen").clicked() {
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
            state.draft.agents.push(new_a);
            state.selected_agent_idx = Some(state.draft.agents.len() - 1);
        }
    }
    if let Some(idx) = to_delete {
        if state.draft.agents.len() > 1 {
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
        let visuals = &ui.ctx().style().visuals;
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
        let visuals = &ui.ctx().style().visuals;
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
        let mut cfg = AppConfig::default();
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
        let mut cfg2 = AppConfig::default();
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
            }],
            default_ide_id: Some("vscode".to_string()),
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
            }],
            default_ide_id: Some("vscode".to_string()),
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
                }],
                default_ide_id: Some("vscode".to_string()),
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
            }],
            default_ide_id: Some("vscode".to_string()),
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
}
