//! Tab „Allgemein“ samt Ordner-Einstellungen.

use super::SettingsState;
use crate::config::{AppConfig, RepoSortMode};
use crate::i18n::{tr, Language};
use crate::ui::icons::{ICON_FOLDER, ICON_PLUS, ICON_TRASH, ICON_WARNING};
use crate::ui::theme::COLOR_MUTED;
use egui::{Color32, RichText, Vec2};
use std::path::{Path, PathBuf};

pub(super) fn show_general_tab(
    ui: &mut egui::Ui,
    state: &mut SettingsState,
    update_error: Option<&str>,
) {
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
                    egui::Image::new(ICON_PLUS)
                        .tint(ui.visuals().text_color())
                        .fit_to_exact_size(Vec2::splat(14.0)),
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

    let mut to_remove: Option<usize> = None;
    for idx in 0..state.draft.roots.len() {
        ui.horizontal(|ui| {
            // Der Text kommt je Frame aus dem Pfad: jede Änderung gleich
            // zurückschreiben, sonst verschwände das getippte Zeichen wieder
            let mut path_str = state.draft.roots[idx].display().to_string();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut path_str)
                    .hint_text("Pfad zum Suchordner")
                    .desired_width(ui.available_width() - 90.0),
            );
            if resp.changed() {
                state.draft.roots[idx] = PathBuf::from(path_str);
            }
            if ui
                .add(egui::Button::image(
                    egui::Image::new(ICON_TRASH)
                        .tint(ui.visuals().text_color())
                        .fit_to_exact_size(Vec2::splat(14.0)),
                ))
                .on_hover_text("Entfernen")
                .clicked()
            {
                to_remove = Some(idx);
            }
            if ui
                .add(egui::Button::image(
                    egui::Image::new(ICON_FOLDER)
                        .tint(ui.visuals().text_color())
                        .fit_to_exact_size(Vec2::splat(14.0)),
                ))
                .on_hover_text("Durchsuchen")
                .clicked()
            {
                if let Some(new_path) = rfd::FileDialog::new().pick_folder() {
                    state.draft.roots[idx] = new_path;
                }
            }
        });
        ui.add_space(4.0);
        let root = &state.draft.roots[idx];
        // Prüfung im Hintergrund: ein `stat` je Frame hielte das Fenster an,
        // sobald ein Suchpfad auf einem nicht erreichbaren Laufwerk liegt
        if crate::path_probe::exists(ui.ctx(), root) == Some(false) {
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
    if let Some(idx) = to_remove {
        let root = state.draft.roots[idx].clone();
        state.draft.remove_root(&root);
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
        .color(COLOR_MUTED),
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
        .color(COLOR_MUTED),
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

    // Update-Check (allgemeine Einstellung, kein Tray-Thema)
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    {
        let lang = state.draft.language;
        let mut check_updates = state.draft.check_for_updates;
        if ui
            .checkbox(&mut check_updates, tr(lang, "tray_update_label"))
            .changed()
        {
            state.draft.check_for_updates = check_updates;
        }
        ui.label(
            RichText::new(if check_updates {
                tr(lang, "tray_update_hint_on")
            } else {
                tr(lang, "tray_update_hint_off")
            })
            .size(10.0)
            .color(Color32::from_rgb(120, 120, 120))
            .italics(),
        );
        if let Some(err) = update_error {
            ui.add_space(4.0);
            ui.label(
                RichText::new(crate::i18n::tr_fmt(lang, "tray_update_error", &[err]))
                    .size(10.0)
                    .color(Color32::from_rgb(160, 40, 40))
                    .italics(),
            );
        }
    }
    // Autostart via Registry Run-Key (neben Update-Check im General-Tab)
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    {
        let lang = state.draft.language;
        let mut autostart = state.draft.autostart;
        if ui
            .checkbox(&mut autostart, tr(lang, "autostart_label"))
            .changed()
        {
            state.draft.autostart = autostart;
        }
        ui.label(
            RichText::new(tr(lang, "autostart_hint"))
                .size(10.0)
                .color(Color32::from_rgb(120, 120, 120))
                .italics(),
        );
    }
    // Task 4: Sortierung Hauptliste (Spiegel zum Top-Bar-Umschalter)
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    {
        let lang = state.draft.language;
        ui.label(RichText::new(tr(lang, "sort_title")).size(13.0).strong());
        ui.add_space(4.0);
        let selected = match state.draft.main_sort_mode {
            RepoSortMode::LastUsed => tr(lang, "sort_by_last_used"),
            RepoSortMode::Name => tr(lang, "sort_by_name"),
        };
        egui::ComboBox::from_id_salt("sort_mode_general")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for mode in RepoSortMode::all() {
                    let label = match mode {
                        RepoSortMode::LastUsed => tr(lang, "sort_by_last_used"),
                        RepoSortMode::Name => tr(lang, "sort_by_name"),
                    };
                    if ui
                        .selectable_label(state.draft.main_sort_mode == mode, label)
                        .clicked()
                    {
                        state.draft.main_sort_mode = mode;
                    }
                }
            })
            .response
            .on_hover_text(tr(lang, "sort_tooltip"));
    }
    // Task 5: Auto-Fetch Intervall (0 = aus, Default 10)
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    {
        let lang = state.draft.language;
        ui.label(
            RichText::new(tr(lang, "auto_fetch_label"))
                .size(13.0)
                .strong(),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(tr(lang, "auto_fetch_hint"))
                .size(11.0)
                .color(COLOR_MUTED),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let mut mins = state.draft.auto_fetch_interval_min;
            let off_txt = tr(lang, "auto_fetch_off");
            let unit = if mins == 0 {
                off_txt
            } else {
                "min".to_string()
            };
            let slider = egui::Slider::new(&mut mins, 0..=120).text(unit);
            if ui.add(slider).changed() {
                state.draft.auto_fetch_interval_min = mins;
            }
        });
        ui.horizontal(|ui| {
            let mut mins = state.draft.auto_fetch_interval_min;
            if ui
                .add(egui::DragValue::new(&mut mins).range(0..=120).speed(1.0))
                .changed()
            {
                state.draft.auto_fetch_interval_min = mins.min(120);
            }
        });
    }
    // Standard-Ordner für „Neuer Worktree“ (leer = neben dem Repo)
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    show_worktree_root_setting(ui, &mut state.draft);
    // Standard-Ordner für „Neue Claude Session“
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    show_claude_dir_setting(ui, &mut state.draft);
    // Vorlagedateien für neue Worktrees und Claude Sessions
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    show_folder_files_setting(ui, &mut state.draft);
    ui.add_space(10.0);
    let lang = state.draft.language;
    ui.checkbox(
        &mut state.draft.claude_notify,
        RichText::new(tr(lang, "claude_notify_label"))
            .size(13.0)
            .strong(),
    );
    ui.label(
        RichText::new(tr(lang, "claude_notify_hint"))
            .size(11.0)
            .color(COLOR_MUTED),
    );
}

fn show_worktree_root_setting(ui: &mut egui::Ui, draft: &mut AppConfig) {
    let lang = draft.language;
    let keys = ["wt_root_label", "wt_root_hint", "wt_root_placeholder"];
    show_dir_setting(ui, lang, keys, &mut draft.worktree_root);
}

/// Vorlagedateien (Pfad + Inhalt), die neue Worktrees und Ordner neuer
/// Claude Sessions bekommen, falls sie dort fehlen.
fn show_folder_files_setting(ui: &mut egui::Ui, draft: &mut AppConfig) {
    let lang = draft.language;
    let hint_color = COLOR_MUTED;
    ui.label(
        RichText::new(tr(lang, "folder_files_label"))
            .size(13.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(lang, "folder_files_hint"))
            .size(11.0)
            .color(hint_color),
    );
    ui.add_space(6.0);
    let mut remove = None;
    for (i, file) in draft.folder_files.iter_mut().enumerate() {
        ui.push_id(("folder_file", i), |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(tr(lang, "folder_files_path"));
                    ui.add(
                        egui::TextEdit::singleline(&mut file.path)
                            .hint_text(tr(lang, "folder_files_path_hint"))
                            .desired_width(260.0),
                    );
                    if ui.button(tr(lang, "folder_files_remove")).clicked() {
                        remove = Some(i);
                    }
                });
                if !file.path.trim().is_empty() && file.target(Path::new("x")).is_none() {
                    ui.label(
                        RichText::new(tr(lang, "folder_files_invalid"))
                            .size(11.0)
                            .color(Color32::from_rgb(200, 120, 40)),
                    );
                }
                ui.label(tr(lang, "folder_files_content"));
                ui.add(
                    egui::TextEdit::multiline(&mut file.content)
                        .code_editor()
                        .desired_rows(5)
                        .desired_width(f32::INFINITY),
                );
            });
        });
        ui.add_space(4.0);
    }
    if let Some(i) = remove {
        draft.folder_files.remove(i);
    }
    if ui.button(tr(lang, "folder_files_add")).clicked() {
        draft
            .folder_files
            .push(crate::config::FolderFile::default());
    }
}

/// Standard-Ordner für „Neue Claude Session“ (leer = zuletzt benutztes Repo)
fn show_claude_dir_setting(ui: &mut egui::Ui, draft: &mut AppConfig) {
    let lang = draft.language;
    let keys = [
        "claude_dir_label",
        "claude_dir_hint",
        "claude_dir_placeholder",
    ];
    show_dir_setting(ui, lang, keys, &mut draft.claude_default_dir);
}

/// Ordner-Einstellung: Textfeld, „Durchsuchen …“, „Zurücksetzen“, Hinweis bei
/// relativem Pfad. `keys` = Überschrift, Erklärung, Platzhalter.
fn show_dir_setting(
    ui: &mut egui::Ui,
    lang: Language,
    [label, hint, placeholder]: [&str; 3],
    value: &mut Option<String>,
) {
    let absolute = |v: &Option<String>| crate::config::absolute_dir(v.as_deref());
    ui.label(RichText::new(tr(lang, label)).size(13.0).strong());
    ui.add_space(4.0);
    ui.label(RichText::new(tr(lang, hint)).size(11.0).color(COLOR_MUTED));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let mut text = value.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text(tr(lang, placeholder))
                    .desired_width(320.0),
            )
            .changed()
        {
            *value = Some(text).filter(|r| !r.trim().is_empty());
        }
        if ui.button(format!("{} …", tr(lang, "browse"))).clicked() {
            let mut dialog = rfd::FileDialog::new();
            if let Some(dir) = absolute(value).filter(|d| d.is_dir()) {
                dialog = dialog.set_directory(dir);
            }
            if let Some(path) = dialog.pick_folder() {
                *value = Some(path.display().to_string());
            }
        }
        if value.is_some() && ui.button(tr(lang, "wt_root_reset")).clicked() {
            *value = None;
        }
    });
    if value.is_some() && absolute(value).is_none() {
        ui.label(
            RichText::new(tr(lang, "wt_root_relative"))
                .size(11.0)
                .color(Color32::from_rgb(200, 120, 40)),
        );
    }
}
