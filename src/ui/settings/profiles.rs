//! Tab „Profile“ (Sprachprofile, IDEs, Konfigurations-Selektoren).

use super::SettingsState;
use crate::config::{IdeConfig, LanguageProfile};
use crate::i18n::tr;
use crate::ui::icons::{
    ICON_CHEVRON_DOWN, ICON_CHEVRON_UP, ICON_FOLDER, ICON_PLUS, ICON_RIDER, ICON_TRASH, ICON_VS,
    ICON_VSCODE, ICON_WARNING,
};
use crate::ui::theme::COLOR_MUTED;
use egui::{Color32, RichText, Vec2};
use std::path::PathBuf;

pub(super) fn show_profiles_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(
        RichText::new("Sprach-Profile verwalten")
            .size(13.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Jedes Profil definiert eine Dateiendung (z.B. .sln) und die IDEs, mit denen diese geöffnet werden kann.")
            .size(11.0)
            .color(COLOR_MUTED),
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
                    egui::Image::new(ICON_PLUS)
                        .tint(ui.visuals().text_color())
                        .fit_to_exact_size(Vec2::splat(14.0)),
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
                            egui::Image::new(ICON_TRASH)
                                .tint(ui.visuals().text_color())
                                .fit_to_exact_size(Vec2::splat(12.0)),
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
                                                    .tint(ui.visuals().text_color())
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
                                                ICON_VS
                                            }
                                            "rider" | "jetbrains" => {
                                                ICON_RIDER
                                            }
                                            _ => {
                                                ICON_VSCODE
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
                                                    .tint(ui.visuals().text_color())
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
                                                    egui::Image::new(ICON_TRASH).tint(ui.visuals().text_color())
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
                            .color(COLOR_MUTED)
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
                    egui::Image::new(ICON_PLUS)
                        .tint(ui.visuals().text_color())
                        .fit_to_exact_size(Vec2::splat(14.0)),
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
                    .color(COLOR_MUTED),
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
                            egui::Image::new(ICON_PLUS)
                                .tint(ui.visuals().text_color())
                                .fit_to_exact_size(Vec2::splat(14.0)),
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
                                                .tint(ui.visuals().text_color())
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
                                                .tint(ui.visuals().text_color())
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
                                                .tint(ui.visuals().text_color())
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
                                            .tint(ui.visuals().text_color())
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
                                        .tint(ui.visuals().text_color())
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
                    egui::Image::new(ICON_PLUS)
                        .tint(ui.visuals().text_color())
                        .fit_to_exact_size(Vec2::splat(14.0)),
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
