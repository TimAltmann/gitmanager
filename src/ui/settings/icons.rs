//! Tab „Icons“ (Reihenfolge und Sichtbarkeit je Profil).

use super::SettingsState;
use crate::i18n::tr;
use crate::ui::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_UP, ICON_EYE, ICON_EYE_OFF};
use crate::ui::theme::COLOR_MUTED;
use egui::{Color32, RichText, Vec2};

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
                                .tint(ui.visuals().text_color())
                                .fit_to_exact_size(Vec2::splat(14.0)),
                        ))
                        .clicked()
                        && order_idx + 1 < effective_ide_order.len()
                    {
                        ide_move = Some((order_idx, 1));
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
                                .tint(ui.visuals().text_color())
                                .fit_to_exact_size(Vec2::splat(14.0)),
                        ))
                        .clicked()
                        && order_idx + 1 < effective_agent_order.len()
                    {
                        agent_move = Some((order_idx, 1));
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
