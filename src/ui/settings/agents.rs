//! Tab „Agents“.

use super::SettingsState;
use crate::config::{AgentProfile, TerminalPreference};
use crate::ui::icons::{
    ICON_AIDER, ICON_CLAUDE, ICON_CODEX, ICON_COPILOT, ICON_CURSOR, ICON_FOLDER, ICON_GEMINI,
    ICON_PLUS, ICON_TRASH, ICON_WARNING,
};
use crate::ui::theme::COLOR_MUTED;
use egui::{Color32, RichText, Vec2};
use std::path::PathBuf;

pub(super) fn show_agents_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(RichText::new("AI Agents verwalten").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Agenten starten ein Terminal im Repo-Verzeichnis und führen den konfigurierten Befehl aus. Claude ist vordefiniert.")
            .size(11.0)
            .color(COLOR_MUTED),
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
                    egui::Image::new(ICON_PLUS)
                        .tint(ui.visuals().text_color())
                        .fit_to_exact_size(Vec2::splat(14.0)),
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
                            egui::Image::new(ICON_TRASH)
                                .tint(ui.visuals().text_color())
                                .fit_to_exact_size(Vec2::splat(12.0)),
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
                                    "claude" => ICON_CLAUDE,
                                    "codex" => ICON_CODEX,
                                    "gemini" => ICON_GEMINI,
                                    "copilot" => ICON_COPILOT,
                                    "cursor" => ICON_CURSOR,
                                    "aider" => ICON_AIDER,
                                    _ => ICON_CLAUDE,
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
                                egui::Image::new(ICON_FOLDER)
                                    .tint(ui.visuals().text_color())
                                    .fit_to_exact_size(Vec2::splat(14.0)),
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
                                        .tint(ui.visuals().text_color())
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
                    .color(COLOR_MUTED)
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
        .color(COLOR_MUTED)
        .italics(),
    );
}
