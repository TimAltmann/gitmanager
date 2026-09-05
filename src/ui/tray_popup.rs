use crate::config::AppConfig;
use crate::git::RepoInfo;
use egui::{Color32, RichText, Vec2};

// Icons reused from repo_list
const ICON_VSCODE: egui::ImageSource = egui::include_image!("../../assets/icons/vscode.svg");
const ICON_VS: egui::ImageSource = egui::include_image!("../../assets/icons/visualstudio.svg");
const ICON_RIDER: egui::ImageSource = egui::include_image!("../../assets/icons/rider.svg");
const ICON_CLAUDE: egui::ImageSource = egui::include_image!("../../assets/icons/claude.svg");
const ICON_FOLDER: egui::ImageSource = egui::include_image!("../../assets/icons/folder.svg");
const ICON_TERMINAL: egui::ImageSource = egui::include_image!("../../assets/icons/terminal.svg");
const ICON_CODEX: egui::ImageSource = egui::include_image!("../../assets/icons/codex.svg");
const ICON_GEMINI: egui::ImageSource = egui::include_image!("../../assets/icons/gemini.svg");
const ICON_COPILOT: egui::ImageSource = egui::include_image!("../../assets/icons/copilot.svg");
const ICON_CURSOR: egui::ImageSource = egui::include_image!("../../assets/icons/cursor.svg");
const ICON_AIDER: egui::ImageSource = egui::include_image!("../../assets/icons/aider.svg");
const ICON_REFRESH: egui::ImageSource = egui::include_image!("../../assets/icons/refresh.svg");
const ICON_GEAR: egui::ImageSource = egui::include_image!("../../assets/icons/gear.svg");
const ICON_CROSS: egui::ImageSource = egui::include_image!("../../assets/icons/cross.svg");

fn ide_icon_for(ide_id: &str) -> egui::ImageSource<'static> {
    match ide_id {
        "vs2022" | "vs" | "visualstudio" => ICON_VS,
        "rider" | "jetbrains" => ICON_RIDER,
        "vscode" => ICON_VSCODE,
        _ => ICON_VSCODE,
    }
}

fn agent_icon_for(agent_id: &str) -> egui::ImageSource<'static> {
    match agent_id {
        "claude" => ICON_CLAUDE,
        "codex" => ICON_CODEX,
        "gemini" => ICON_GEMINI,
        "copilot" => ICON_COPILOT,
        "cursor" => ICON_CURSOR,
        "aider" => ICON_AIDER,
        _ => ICON_CLAUDE,
    }
}

fn ide_image(ide: &crate::config::IdeConfig) -> egui::Image<'static> {
    if let Some(path) = &ide.icon {
        let pb = std::path::PathBuf::from(path);
        if pb.exists() {
            let uri = format!("file://{}", pb.display().to_string().replace('\\', "/"));
            return egui::Image::new(uri).fit_to_exact_size(Vec2::splat(16.0));
        }
    }
    egui::Image::new(ide_icon_for(&ide.id)).fit_to_exact_size(Vec2::splat(16.0))
}

fn agent_image(agent: &crate::config::AgentProfile) -> egui::Image<'static> {
    if let Some(path) = &agent.icon {
        let pb = std::path::PathBuf::from(path);
        if pb.exists() {
            let uri = format!("file://{}", pb.display().to_string().replace('\\', "/"));
            return egui::Image::new(uri).fit_to_exact_size(Vec2::splat(16.0));
        }
    }
    egui::Image::new(agent_icon_for(&agent.id)).fit_to_exact_size(Vec2::splat(16.0))
}

pub struct TrayPopupActions {
    pub branch_switch: Option<(std::path::PathBuf, String)>,
    pub ide_open: Option<(std::path::PathBuf, String, std::path::PathBuf)>,
    pub agent_open: Option<(std::path::PathBuf, String)>,
    pub explorer_open: Option<std::path::PathBuf>,
    pub shell_open: Option<std::path::PathBuf>,
    pub refresh: bool,
    pub open_main: bool,
    pub open_settings: bool,
    pub quit: bool,
    pub close_popup: bool,
}

impl Default for TrayPopupActions {
    fn default() -> Self {
        Self {
            branch_switch: None,
            ide_open: None,
            agent_open: None,
            explorer_open: None,
            shell_open: None,
            refresh: false,
            open_main: false,
            open_settings: false,
            quit: false,
            close_popup: false,
        }
    }
}

/// Shows the tray popup UI inside the given viewport Ui.
/// Returns actions triggered by the user.
pub fn show_tray_popup_ui(
    ui: &mut egui::Ui,
    repos: &mut [RepoInfo],
    config: &AppConfig,
    actions: &mut TrayPopupActions,
) {
    let lang = config.language;

    // Header: Title + count + actions
    egui::Panel::top("tray_header")
        .frame(
            egui::Frame::new()
                .fill(ui.visuals().widgets.inactive.bg_fill)
                .inner_margin(egui::Margin::symmetric(10, 6)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("GitManager").size(13.0).strong());
                ui.label(
                    RichText::new(format!("{} Repos", repos.len()))
                        .size(10.0)
                        .color(Color32::from_rgb(120, 120, 120)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let refresh_btn = egui::Button::image(
                        egui::Image::new(ICON_REFRESH).fit_to_exact_size(Vec2::splat(12.0)),
                    )
                    .small();
                    if ui.add(refresh_btn).on_hover_text("Refresh").clicked() {
                        actions.refresh = true;
                    }
                    let settings_btn = egui::Button::image(
                        egui::Image::new(ICON_GEAR).fit_to_exact_size(Vec2::splat(12.0)),
                    )
                    .small();
                    if ui.add(settings_btn).on_hover_text("Settings öffnen").clicked() {
                        actions.open_settings = true;
                        actions.open_main = true;
                    }
                    if ui
                        .add(
                            egui::Button::image(
                                egui::Image::new(ICON_CROSS).fit_to_exact_size(Vec2::splat(12.0)),
                            )
                            .small(),
                        )
                        .on_hover_text("Schließen (Esc)")
                        .clicked()
                    {
                        actions.close_popup = true;
                    }
                });
            });
        });

    // Footer with main/quit
    egui::Panel::bottom("tray_footer")
        .frame(
            egui::Frame::new()
                .fill(ui.visuals().widgets.inactive.bg_fill)
                .inner_margin(egui::Margin::symmetric(10, 6)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        [ui.available_width() * 0.48, 22.0],
                        egui::Button::new(RichText::new(" Hauptfenster").size(11.0)),
                    )
                    .clicked()
                {
                    actions.open_main = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            [ui.available_width(), 22.0],
                            egui::Button::new(
                                RichText::new(" Beenden")
                                    .size(11.0)
                                    .color(Color32::from_rgb(160, 40, 40)),
                            ),
                        )
                        .clicked()
                    {
                        actions.quit = true;
                    }
                });
            });
        });

    // Main scroll area
    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6)))
        .show(ui, |ui| {
            if repos.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(
                        RichText::new(crate::i18n::tr(lang, "no_repos_found"))
                            .size(12.0)
                            .color(Color32::from_rgb(100, 100, 100)),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(crate::i18n::tr(lang, "no_repos_hint"))
                            .size(10.0)
                            .color(Color32::from_rgb(130, 130, 130)),
                    );
                    ui.add_space(12.0);
                    if ui.button("🔄 Refresh").clicked() {
                        actions.refresh = true;
                    }
                });
                return;
            }

            // Optional filter
            // Show repos
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(2.0);
                    for repo in repos.iter_mut() {
                        show_tray_repo_row(ui, repo, config, actions);
                        ui.add_space(4.0);
                    }
                });
        });

    // Close on Escape
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.close_popup = true;
    }
    // Close on clicking outside? Handled via viewport close_requested
}

fn show_tray_repo_row(
    ui: &mut egui::Ui,
    repo: &mut RepoInfo,
    config: &AppConfig,
    actions: &mut TrayPopupActions,
) {
    let visuals = ui.visuals().clone();
    let frame = egui::Frame::new()
        .fill(visuals.widgets.inactive.bg_fill)
        .stroke(egui::Stroke::new(1.0, visuals.widgets.inactive.fg_stroke.color))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 6));

    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            // Row 1: Folder + name + dirty (tight, no branch)
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(ICON_FOLDER).fit_to_exact_size(Vec2::splat(14.0)));
                ui.add_space(4.0);
                // Name uses remaining width minus dirty indicator (16) – no fixed 90 subtraction
                let name_available = ui.available_width() - 20.0;
                let name = repo.name.clone();
                ui.add_sized(
                    [name_available.max(80.0), 18.0],
                    egui::Label::new(RichText::new(name).size(11.0).strong())
                        .truncate()
                        .selectable(false),
                )
                .on_hover_text(&repo.name);
                let dirty_color = if repo.dirty {
                    crate::ui::theme::COLOR_DIRTY
                } else {
                    crate::ui::theme::COLOR_CLEAN
                };
                let dirty_char = if repo.dirty { "●" } else { "○" };
                ui.label(RichText::new(dirty_char).size(11.0).color(dirty_color));
            });

            ui.add_space(4.0);

            // Row 2: Branch dropdown full width on own line
            {
                let branches = &repo.branches;
                let limit = config.tray_branch_limit.clamp(5, 50);
                if !branches.is_empty() {
                    let display_branches: Vec<&String> = branches.iter().take(limit).collect();
                    let current = repo.branch.clone();
                    let combo_width = ui.available_width();
                    egui::ComboBox::from_id_salt(("tray_branch", repo.path.clone()))
                        .selected_text(&current)
                        .width(combo_width)
                        .show_ui(ui, |ui| {
                            for b in display_branches {
                                let is_sel = *b == repo.branch;
                                if ui.selectable_label(is_sel, b.as_str()).clicked() {
                                    if *b != repo.branch {
                                        actions.branch_switch =
                                            Some((repo.path.clone(), (*b).clone()));
                                    }
                                }
                            }
                            if branches.len() > limit {
                                ui.separator();
                                ui.label(
                                    RichText::new(format!(
                                        "+ {} weitere (in Hauptfenster)",
                                        branches.len() - limit
                                    ))
                                    .size(10.0)
                                    .color(Color32::from_rgb(120, 120, 120))
                                    .italics(),
                                );
                            }
                        });
                } else {
                    ui.label(
                        RichText::new(&repo.branch)
                            .size(10.0)
                            .color(Color32::from_rgb(100, 100, 100))
                            .italics(),
                    );
                }
            }

            ui.add_space(4.0);

            // Row 3: Tools tight spacing
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let profile = config.get_effective_profile_for_repo(&repo.path);
                let visible_ides = profile.visible_ides();
                // Show up to 3 IDE buttons
                for ide in visible_ides.iter().take(3) {
                    // Determine file path to open: selected solution or repo root
                    let file_path = repo
                        .selected_solution
                        .clone()
                        .unwrap_or_else(|| repo.path.clone());
                    let btn = egui::Button::image(ide_image(ide)).small();
                    let resp = ui
                        .add(btn)
                        .on_hover_text(format!("In {} öffnen", ide.display_name));
                    if resp.clicked() {
                        actions.ide_open =
                            Some((repo.path.clone(), ide.id.clone(), file_path));
                    }
                }
                if visible_ides.is_empty() {
                    ui.label(
                        RichText::new("Keine IDE")
                            .size(9.0)
                            .color(Color32::from_rgb(140, 140, 140)),
                    );
                }

                ui.separator();

                // Folder
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(ICON_FOLDER).fit_to_exact_size(Vec2::splat(12.0)),
                    ).small())
                    .on_hover_text("Im Explorer öffnen")
                    .clicked()
                {
                    actions.explorer_open = Some(repo.path.clone());
                }
                // Terminal
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(ICON_TERMINAL).fit_to_exact_size(Vec2::splat(12.0)),
                    ).small())
                    .on_hover_text("Terminal öffnen")
                    .clicked()
                {
                    actions.shell_open = Some(repo.path.clone());
                }

                ui.separator();

                // Agents: show up to 2
                let active_agents = config.get_active_agents();
                let filtered = profile.filtered_agents(&config.agents, &config.active_agent_ids);
                for agent in filtered.iter().take(2) {
                    let btn = egui::Button::image(agent_image(agent)).small();
                    let resp = ui
                        .add(btn)
                        .on_hover_text(format!("Agent {} starten", agent.display_name));
                    if resp.clicked() {
                        actions.agent_open = Some((repo.path.clone(), agent.id.clone()));
                    }
                }
                if filtered.is_empty() && !active_agents.is_empty() {
                    // fallback first active
                    if let Some(a) = active_agents.first() {
                        if ui
                            .add(
                                egui::Button::image(agent_image(a))
                                    .small(),
                            )
                            .on_hover_text(format!("Agent {} starten", a.display_name))
                            .clicked()
                        {
                            actions.agent_open = Some((repo.path.clone(), a.id.clone()));
                        }
                    }
                }
            });

            // Optional: show solution selector if more than 1
            if repo.solutions.len() > 1 {
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("📄")
                            .size(10.0)
                            .color(Color32::from_rgb(120, 120, 120)),
                    );
                    let selected_text = repo
                        .selected_solution
                        .as_ref()
                        .and_then(|p| {
                            repo.solutions
                                .iter()
                                .find(|s| &s.path == p)
                                .map(|s| s.relative.clone())
                        })
                        .unwrap_or_else(|| "–".to_string());
                    let trunc = if selected_text.len() > 28 {
                        format!("{}…", &selected_text[..27])
                    } else {
                        selected_text
                    };
                    ui.label(
                        RichText::new(trunc)
                            .size(9.0)
                            .color(Color32::from_rgb(100, 100, 100)),
                    )
                    .on_hover_text(
                        repo.selected_solution
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                    );
                });
            }
        });
    });
}

/// Calculates popup position directly above the tray icon rect.
/// `tray_rect` is in screen coordinates (physical pixels converted to points).
/// `popup_size` is the viewport inner size.
/// Returns egui::Pos2 in screen coordinates.
pub fn calculate_popup_position(
    tray_rect: egui::Rect,
    popup_size: egui::Vec2,
    screen_rect: egui::Rect,
) -> egui::Pos2 {
    // Center horizontally above tray icon, 4px gap
    let mut x = tray_rect.center().x - popup_size.x / 2.0;
    let mut y = tray_rect.min.y - popup_size.y - 4.0;

    // Clamp to screen bounds with 4px margin
    let margin = 4.0;
    x = x.clamp(
        screen_rect.min.x + margin,
        screen_rect.max.x - popup_size.x - margin,
    );
    y = y.clamp(
        screen_rect.min.y + margin,
        screen_rect.max.y - popup_size.y - margin,
    );

    egui::pos2(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_position_above_tray() {
        let tray = egui::Rect::from_min_size(egui::pos2(1800.0, 1050.0), egui::vec2(20.0, 20.0));
        let popup = egui::vec2(360.0, 480.0);
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));
        let pos = calculate_popup_position(tray, popup, screen);
        // Should be centered above tray
        assert!(pos.x > 1500.0 && pos.x < 1800.0);
        assert!(pos.y < tray.min.y);
        assert_eq!(pos.y + popup.y + 4.0, tray.min.y);
    }

    #[test]
    fn popup_clamped_to_screen() {
        let tray = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(20.0, 20.0));
        let popup = egui::vec2(400.0, 500.0);
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));
        let pos = calculate_popup_position(tray, popup, screen);
        // Should be clamped inside screen
        assert!(pos.x >= 4.0);
        assert!(pos.y >= 4.0);
        assert!(pos.x + popup.x <= 1920.0 - 4.0);
    }

    #[test]
    fn popup_near_right_edge_clamped() {
        let tray = egui::Rect::from_min_size(egui::pos2(1910.0, 1050.0), egui::vec2(20.0, 20.0));
        let popup = egui::vec2(360.0, 480.0);
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));
        let pos = calculate_popup_position(tray, popup, screen);
        assert!(pos.x + popup.x <= 1920.0 - 4.0);
    }
}
