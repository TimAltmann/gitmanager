//! Tab „Tray Icons“ (Reihenfolge, Sichtbarkeit, Anzahl Repos).

use super::SettingsState;
use crate::config::{AppConfig, TrayIconTheme};
use crate::i18n::tr;
use crate::ui::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_UP, ICON_EYE, ICON_EYE_OFF};
use crate::ui::theme::COLOR_MUTED;
use egui::{Color32, RichText, Vec2};

const DEFAULT_TRAY_ICON_IDS: &[&str] = &[
    "vscode", "vs2022", "rider", "folder", "terminal", "claude", "codex", "gemini", "copilot",
    "cursor", "aider",
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
    let all: Vec<String> = DEFAULT_TRAY_ICON_IDS
        .iter()
        .map(|s| s.to_string())
        .collect();
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

pub(super) fn show_tray_icons_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    let lang = state.draft.language;
    ui.label(
        RichText::new(tr(lang, "tray_icons_title"))
            .size(13.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(lang, "tray_icons_desc"))
            .size(11.0)
            .color(COLOR_MUTED),
    );
    ui.add_space(12.0);

    // Tray-Icon Stil (Dark/Light) — konfigurierbar ob Dark- oder Light-Icon.
    ui.label(
        RichText::new(tr(lang, "tray_icon_theme_title"))
            .size(12.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(lang, "tray_icon_theme_desc"))
            .size(11.0)
            .color(COLOR_MUTED),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.radio_value(
            &mut state.draft.tray_icons.icon_theme,
            TrayIconTheme::Dark,
            tr(lang, "tray_icon_theme_dark"),
        );
        ui.radio_value(
            &mut state.draft.tray_icons.icon_theme,
            TrayIconTheme::Light,
            tr(lang, "tray_icon_theme_light"),
        );
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // Max display slider
    ui.label(
        RichText::new(tr(lang, "tray_max_display"))
            .size(12.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Wie viele Repos maximal im Tray-Popup angezeigt werden (5–50, Standard 10).",
        )
        .size(11.0)
        .color(COLOR_MUTED),
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

    ui.label(
        RichText::new("Icons (Reihenfolge & Sichtbarkeit)")
            .size(12.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Icons neu anordnen (↑/↓) und ausblenden (👁). Ausgeblendete Icons verschwinden ohne Lücke in der Tray-Zeile.")
            .size(11.0)
            .color(COLOR_MUTED),
    );
    ui.add_space(8.0);

    let effective = effective_tray_order(&state.draft);
    let mut tray_move: Option<(usize, isize)> = None;
    let mut tray_toggle: Option<String> = None;
    for (order_idx, icon_id) in effective.iter().enumerate() {
        let is_hidden = state.draft.tray_icons.hidden_icon_ids.contains(icon_id);
        ui.horizontal(|ui| {
            let eye_icon = if is_hidden { ICON_EYE_OFF } else { ICON_EYE };
            let eye_tint = if is_hidden {
                ui.visuals().weak_text_color()
            } else {
                ui.visuals().text_color()
            };
            if ui
                .add(egui::Button::image(
                    egui::Image::new(eye_icon)
                        .fit_to_exact_size(Vec2::splat(14.0))
                        .tint(eye_tint),
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
                        egui::Image::new(ICON_CHEVRON_DOWN)
                            .tint(ui.visuals().text_color())
                            .fit_to_exact_size(Vec2::splat(14.0)),
                    ))
                    .clicked()
                    && order_idx + 1 < effective.len()
                {
                    tray_move = Some((order_idx, 1));
                }
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(ICON_CHEVRON_UP)
                            .tint(ui.visuals().text_color())
                            .fit_to_exact_size(Vec2::splat(14.0)),
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

    // System Tray Verhalten (moved from General)
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        RichText::new(tr(lang, "tray_behavior_title"))
            .size(12.0)
            .strong(),
    );
    ui.add_space(4.0);
    {
        let mut minimize = state.draft.minimize_to_tray;
        if ui
            .checkbox(&mut minimize, tr(lang, "tray_minimize_label"))
            .changed()
        {
            state.draft.minimize_to_tray = minimize;
        }
        ui.label(
            RichText::new(if minimize {
                tr(lang, "tray_minimize_hint_on")
            } else {
                tr(lang, "tray_minimize_hint_off")
            })
            .size(10.0)
            .color(Color32::from_rgb(120, 120, 120))
            .italics(),
        );
    }
    ui.add_space(8.0);
    ui.label(
        RichText::new(tr(lang, "tray_branch_limit_title"))
            .size(12.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(lang, "tray_branch_limit_desc"))
            .size(11.0)
            .color(COLOR_MUTED),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(tr(lang, "tray_limit_label"));
        let mut limit = state.draft.tray_branch_limit;
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
        ui.label(tr(lang, "tray_direct_label"));
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
        RichText::new(crate::i18n::tr_fmt(
            lang,
            "tray_branch_limit_summary",
            &[&state.draft.tray_branch_limit.to_string()],
        ))
        .size(10.0)
        .color(Color32::from_rgb(120, 120, 120))
        .italics(),
    );
    ui.add_space(8.0);
    ui.separator();
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
        state.draft.tray_icons.icon_theme = TrayIconTheme::Dark;
    }
    if state.draft.tray_icons.hidden_icon_ids.len() == DEFAULT_TRAY_ICON_IDS.len() {
        ui.colored_label(
            Color32::from_rgb(200, 80, 20),
            tr(lang, "tray_all_hidden_warn"),
        );
    }
}
