//! Tab „Icons“ (Reihenfolge und Sichtbarkeit je Profil).

use super::{
    hidden_info, icon_order_row, move_in_order, normalize_icon_order, toggle_hidden, IconRowAction,
    SettingsState,
};
use crate::i18n::tr;
use crate::ui::theme::COLOR_MUTED;
use egui::{Color32, RichText};

pub(super) fn show_icons_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    let lang = state.draft.language;
    ui.label(RichText::new(tr(lang, "icons_title")).size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr(lang, "icons_desc"))
            .size(11.0)
            .color(COLOR_MUTED),
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
        ui.label(RichText::new(tr(lang, "icons_no_profile")).weak());
        return;
    };
    let agents = &state.draft.agents;
    let active_agent_ids = &state.draft.active_agent_ids;
    let Some(profile) = state.draft.profiles.get_mut(idx) else {
        ui.label(RichText::new(tr(lang, "icons_profile_missing")).weak());
        return;
    };
    // Wirksame Reihenfolgen nur fürs Zeichnen; der Draft ändert sich erst
    // bei einem Klick.
    let ide_ids: Vec<String> = profile.ides.iter().map(|i| i.id.clone()).collect();
    let agent_ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
    let ide_order = normalize_icon_order(&profile.ide_order, &ide_ids, false);
    let agent_order = normalize_icon_order(&profile.agent_order, &agent_ids, false);

    ui.label(
        RichText::new(format!(
            "{} {}",
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
    let mut clicked = None;
    for (i, id) in ide_order.iter().enumerate() {
        let Some(ide) = profile.ides.iter().find(|ide| &ide.id == id) else {
            continue;
        };
        let label = format!("{} ({})", ide.display_name, ide.id);
        let hidden = profile.hidden_ide_ids.contains(id);
        if let Some(action) = icon_order_row(ui, lang, &label, hidden, i, ide_order.len()) {
            clicked = Some((i, action));
        }
    }
    match clicked {
        Some((i, IconRowAction::ToggleHidden)) => {
            toggle_hidden(&mut profile.hidden_ide_ids, &ide_order[i])
        }
        Some((i, IconRowAction::Move(delta))) => {
            move_in_order(&mut profile.ide_order, &ide_order, i, delta)
        }
        None => {}
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
    ui.checkbox(&mut profile.show_shell, tr(lang, "icons_show_shell"));
    ui.checkbox(&mut profile.show_explorer, tr(lang, "icons_show_explorer"));
    ui.label(
        RichText::new(tr(lang, "icons_terminal_hint"))
            .size(10.0)
            .color(ui.visuals().weak_text_color())
            .italics(),
    );
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // AI Agents per Profil
    ui.label(RichText::new(tr(lang, "icons_agents")).size(12.0).strong());
    ui.add_space(4.0);
    let mut clicked = None;
    for (i, id) in agent_order.iter().enumerate() {
        let Some(agent) = agents.iter().find(|a| &a.id == id) else {
            continue;
        };
        let mut label = format!("{} ({})", agent.display_name, agent.id);
        if !active_agent_ids.contains(id) {
            label.push_str(&format!(" [{}]", tr(lang, "icons_inactive")));
        }
        let hidden = profile.hidden_agent_ids.contains(id);
        if let Some(action) = icon_order_row(ui, lang, &label, hidden, i, agent_order.len()) {
            clicked = Some((i, action));
        }
    }
    match clicked {
        Some((i, IconRowAction::ToggleHidden)) => {
            toggle_hidden(&mut profile.hidden_agent_ids, &agent_order[i])
        }
        Some((i, IconRowAction::Move(delta))) => {
            move_in_order(&mut profile.agent_order, &agent_order, i, delta)
        }
        None => {}
    }
    ui.add_space(4.0);
    hidden_info(ui, lang, &profile.hidden_agent_ids);
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
}
