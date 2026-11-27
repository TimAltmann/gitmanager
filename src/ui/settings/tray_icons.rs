//! Tab „Tray Icons“ (Reihenfolge, Sichtbarkeit, Anzahl Repos).

use super::{
    hidden_info, icon_order_row, move_in_order, normalize_icon_order, toggle_hidden,
    tray_limit_control, IconRowAction, SettingsState,
};
use crate::config::{AppConfig, TrayIconTheme};
use crate::i18n::tr;
use crate::ui::theme::COLOR_MUTED;
use egui::{Color32, RichText};

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
    normalize_icon_order(&draft.tray_icons.icon_order, &all, true)
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
        RichText::new(tr(lang, "tray_max_display_desc"))
            .size(11.0)
            .color(COLOR_MUTED),
    );
    ui.add_space(6.0);
    tray_limit_control(
        ui,
        lang,
        &mut state.draft.tray_icons.max_display,
        "Repos",
        "tray_max_display_summary",
    );

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(
        RichText::new(tr(lang, "tray_icons_order_title"))
            .size(12.0)
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(lang, "tray_icons_order_desc"))
            .size(11.0)
            .color(COLOR_MUTED),
    );
    ui.add_space(8.0);

    let effective = effective_tray_order(&state.draft);
    let tray = &mut state.draft.tray_icons;
    let mut clicked = None;
    for (i, id) in effective.iter().enumerate() {
        let label = format!("{} ({id})", tray_icon_display_name(id));
        let hidden = tray.hidden_icon_ids.contains(id);
        if let Some(action) = icon_order_row(ui, lang, &label, hidden, i, effective.len()) {
            clicked = Some((i, action));
        }
    }
    match clicked {
        Some((i, IconRowAction::ToggleHidden)) => {
            toggle_hidden(&mut tray.hidden_icon_ids, &effective[i])
        }
        Some((i, IconRowAction::Move(delta))) => {
            move_in_order(&mut tray.icon_order, &effective, i, delta)
        }
        None => {}
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
    tray_limit_control(
        ui,
        lang,
        &mut state.draft.tray_branch_limit,
        "Branches",
        "tray_branch_limit_summary",
    );
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    hidden_info(ui, lang, &state.draft.tray_icons.hidden_icon_ids);
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
