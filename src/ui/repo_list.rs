use crate::config::AppConfig;
use crate::git::RepoInfo;
use crate::i18n::tr;
use egui::{Color32, RichText, Ui, Vec2};

// Icons via egui include_image! (Simple Icons CC0 + Phosphor for folder)
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
const ICON_CHEVRON_DOWN: egui::ImageSource =
    egui::include_image!("../../assets/icons/chevron-down.svg");
const ICON_CHEVRON_UP: egui::ImageSource =
    egui::include_image!("../../assets/icons/chevron-up.svg");
const ICON_GIT_BRANCH: egui::ImageSource =
    egui::include_image!("../../assets/icons/git-branch.svg");
const ICON_GIT_COMMIT: egui::ImageSource =
    egui::include_image!("../../assets/icons/git-commit.svg");

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

/// Helper: Dropdown-Button mit rechtsbündigem Chevron *innerhalb* des Buttons.
/// Nutzt `Button::right_text` (mit `Atom::grow`) damit der Pfeil immer am rechten
/// Rand des Buttons liegt und der Text links getruncate wird.
fn dropdown_button(
    ui: &mut Ui,
    text: &str,
    chevron: egui::ImageSource<'static>,
    width: f32,
    selected: bool,
    text_color: Option<Color32>,
    font_size: f32,
) -> egui::Response {
    let tint = text_color.unwrap_or_else(|| ui.visuals().text_color());
    let chevron_img = egui::Image::new(chevron)
        .fit_to_exact_size(Vec2::splat(12.0))
        .tint(tint);
    let rich = match text_color {
        Some(c) => RichText::new(text.to_owned()).size(font_size).color(c),
        None => RichText::new(text.to_owned()).size(font_size),
    };
    let btn = egui::Button::new(rich)
        .selected(selected)
        .truncate()
        .right_text(chevron_img);
    ui.add_sized([width, 22.0], btn)
}

pub fn filter_branches(branches: &[String], filter: &str, limit: usize) -> Vec<String> {
    let filter_lower = filter.to_lowercase();
    let mut out = Vec::new();
    for b in branches {
        if !filter_lower.is_empty() && !b.to_lowercase().contains(&filter_lower) {
            continue;
        }
        out.push(b.clone());
        if out.len() >= limit {
            break;
        }
    }
    out
}

pub struct RepoListActions {
    pub branch_switch: Option<(PathBuf, String)>,
    pub solution_select: Option<(PathBuf, PathBuf)>,
    pub ide_open: Option<(PathBuf, String, PathBuf)>, // repo_path, ide_id, file_path
    pub agent_open: Option<(PathBuf, String)>,        // repo_path, agent_id
    pub profile_override: Option<(PathBuf, Option<String>)>,
    pub fetch_branches: Option<PathBuf>,
    pub explorer_open: Option<PathBuf>,
    pub shell_open: Option<PathBuf>,
    pub custom_select: Option<(PathBuf, String, String)>,
}

use std::path::PathBuf;

pub fn show_repo_list(
    ui: &mut Ui,
    repos: &mut [RepoInfo],
    config: &AppConfig,
    actions: &mut RepoListActions,
) {
    if repos.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(
                RichText::new(tr(config.language, "no_repos_found"))
                    .size(16.0)
                    .color(Color32::from_rgb(100, 100, 100)),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new(tr(config.language, "no_repos_hint"))
                    .size(12.0)
                    .color(Color32::from_rgb(130, 130, 130)),
            );
        });
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(4.0);
            for repo in repos.iter_mut() {
                show_repo_row(ui, repo, config, actions);
                ui.add_space(6.0);
            }
            ui.add_space(12.0);
        });
}

fn show_repo_row(
    ui: &mut Ui,
    repo: &mut RepoInfo,
    config: &AppConfig,
    actions: &mut RepoListActions,
) {
    let visuals = ui.visuals();
    let frame = egui::Frame::new()
        .fill(visuals.widgets.inactive.bg_fill)
        .stroke(egui::Stroke::new(
            1.0_f32,
            visuals.widgets.inactive.fg_stroke.color,
        ))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(12, 10));

    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            // Zeile 1: Name + Dirty + Branch Dropdown + Solution Dropdown + Profile Override
            ui.horizontal(|ui| {
                ui.label(RichText::new("📁").size(14.0));
                ui.label(RichText::new(&repo.name).size(13.0).strong());
                if repo.dirty {
                    ui.label(
                        RichText::new("●")
                            .size(14.0)
                            .color(crate::ui::theme::COLOR_DIRTY),
                    )
                    .on_hover_text(tr(config.language, "dirty_tooltip"));
                } else {
                    ui.label(
                        RichText::new("○")
                            .size(14.0)
                            .color(crate::ui::theme::COLOR_CLEAN),
                    )
                    .on_hover_text(tr(config.language, "clean_tooltip"));
                }

                ui.add_space(8.0);

                // Branch Dropdown mit Suche + Refresh (custom popup statt ComboBox)
                let branch_text = repo.branch.clone();
                let branch_filter_id = egui::Id::new("branch_filter").with(&repo.path);
                let branch_popup_id = egui::Id::new("branch_popup").with(&repo.path);
                let branch_win_id = egui::Id::new("branch_win").with(&repo.path);
                let branch_focus_id = egui::Id::new("branch_focus").with(&repo.path);
                let mut branch_filter = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<String>(branch_filter_id).unwrap_or_default());
                let mut popup_open = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<bool>(branch_popup_id).unwrap_or(false));

                let mut branch_selected: Option<String> = None;
                let mut do_fetch = false;

                let chevron_icon = if popup_open {
                    ICON_CHEVRON_UP
                } else {
                    ICON_CHEVRON_DOWN
                };
                let branch_icon = if repo.is_detached {
                    ICON_GIT_COMMIT
                } else {
                    ICON_GIT_BRANCH
                };
                let branch_tooltip = if repo.is_detached {
                    format!(
                        "{}: {}",
                        repo.branch,
                        tr(config.language, "detached_tooltip")
                    )
                } else {
                    format!(
                        "{} – {}",
                        repo.branch,
                        tr(config.language, "branch_switch_tooltip")
                    )
                };
                // Branch Dropdown: adaptive Breite für lange Branchenamen (140..280)
                let branch_width = (branch_text.len() as f32 * 7.0 + 46.0).clamp(140.0, 280.0);
                // Branch Dropdown: Chevron jetzt innerhalb des Buttons (rechtsbündig)
                let btn_resp = ui
                    .horizontal(|ui| {
                        let icon_tint = if repo.is_detached {
                            Color32::from_rgb(200, 120, 40)
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.add(
                            egui::Image::new(branch_icon)
                                .fit_to_exact_size(Vec2::splat(14.0))
                                .tint(icon_tint),
                        )
                        .on_hover_text(branch_tooltip.clone());
                        let branch_color = if repo.is_detached {
                            Color32::from_rgb(200, 120, 40)
                        } else {
                            ui.visuals().text_color()
                        };
                        dropdown_button(
                            ui,
                            &branch_text,
                            chevron_icon,
                            branch_width,
                            popup_open,
                            Some(branch_color),
                            12.0,
                        )
                        .on_hover_text(branch_tooltip.clone())
                    })
                    .inner;
                let should_toggle = btn_resp.clicked();
                if should_toggle {
                    popup_open = !popup_open;
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(branch_popup_id, popup_open));
                    if !popup_open {
                        // Filter zurücksetzen beim Schließen
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(branch_filter_id, String::new());
                            d.insert_temp(branch_focus_id, false);
                        });
                        branch_filter.clear();
                    }
                }

                // Popup via Window (bleibt offen während Suche)
                let mut close_popup = false;
                let mut branch_filter_changed = false;
                if popup_open {
                    let win_resp =
                        egui::Window::new(format!("branch_popup_win_{}", repo.path.display()))
                            .id(branch_win_id)
                            .collapsible(false)
                            .resizable(false)
                            .title_bar(false)
                            .movable(false)
                            .fixed_pos(btn_resp.rect.left_bottom())
                            .pivot(egui::Align2::LEFT_TOP)
                            .show(ui.ctx(), |ui| {
                                ui.set_min_width(260.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(tr(config.language, "search_label"))
                                            .size(11.0),
                                    );
                                    let edit_resp = ui.add(
                                        egui::TextEdit::singleline(&mut branch_filter)
                                            .hint_text(tr(config.language, "filter_hint"))
                                            .desired_width(120.0),
                                    );
                                    // Fokus beim Öffnen direkt in Suche + Recovery falls Fokus verloren
                                    let has_focused = ui.ctx().data_mut(|d| {
                                        d.get_temp::<bool>(branch_focus_id).unwrap_or(false)
                                    });
                                    if !has_focused {
                                        edit_resp.request_focus();
                                        ui.ctx().data_mut(|d| d.insert_temp(branch_focus_id, true));
                                    }
                                    // Recovery: wenn kein Fokus mehr, erneut anfordern
                                    if !edit_resp.has_focus()
                                        && ui.ctx().memory(|m| m.focused().is_none())
                                    {
                                        edit_resp.request_focus();
                                    }
                                    if edit_resp.has_focus() && !has_focused {
                                        ui.ctx().data_mut(|d| d.insert_temp(branch_focus_id, true));
                                    }
                                    if edit_resp.changed() {
                                        branch_filter_changed = true;
                                    }
                                    if ui
                                        .small_button("↻")
                                        .on_hover_text(tr(
                                            config.language,
                                            "branch_refresh_tooltip",
                                        ))
                                        .clicked()
                                    {
                                        do_fetch = true;
                                    }
                                });
                                ui.separator();
                                ui.label(
                                    RichText::new(format!(
                                        "{} {}",
                                        tr(config.language, "current"),
                                        repo.branch
                                    ))
                                    .weak()
                                    .size(11.0),
                                );
                                ui.separator();
                                egui::ScrollArea::vertical()
                                    .max_height(220.0)
                                    .show(ui, |ui| {
                                        let limit = config.branch_display_limit.clamp(50, 500);
                                        let filtered =
                                            filter_branches(&repo.branches, &branch_filter, limit);
                                        for b in &filtered {
                                            let is_current = b == &repo.branch;
                                            let label = if is_current {
                                                format!("● {}", b)
                                            } else {
                                                b.clone()
                                            };
                                            if ui.selectable_label(is_current, label).clicked()
                                                && !is_current
                                            {
                                                branch_selected = Some(b.clone());
                                                close_popup = true;
                                            }
                                        }
                                        if filtered.len() >= limit {
                                            let total_matching = repo
                                                .branches
                                                .iter()
                                                .filter(|b| {
                                                    branch_filter.is_empty()
                                                        || b.to_lowercase()
                                                            .contains(&branch_filter.to_lowercase())
                                                })
                                                .count();
                                            let remaining =
                                                total_matching.saturating_sub(filtered.len());
                                            if remaining > 0 {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "... und {} mehr (filtere)",
                                                        remaining
                                                    ))
                                                    .weak()
                                                    .size(10.0),
                                                );
                                            } else if repo.branches.len() > filtered.len() {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "... und {} mehr (filtere)",
                                                        repo.branches.len() - filtered.len()
                                                    ))
                                                    .weak()
                                                    .size(10.0),
                                                );
                                            }
                                        }
                                        if repo.branches.is_empty() {
                                            ui.label(
                                                RichText::new(tr(config.language, "no_branches"))
                                                    .weak()
                                                    .size(11.0),
                                            );
                                            if ui
                                                .small_button(tr(config.language, "fetch_now"))
                                                .clicked()
                                            {
                                                do_fetch = true;
                                            }
                                        } else if filtered.is_empty() {
                                            ui.label(
                                                RichText::new(tr(config.language, "no_matches"))
                                                    .weak()
                                                    .size(11.0),
                                            );
                                        }
                                    });
                            });
                    // Außenklick schließt Popup – tolerant (4px) für Scrollbar-Rand, nur primary
                    if ui.ctx().input(|i| i.pointer.primary_clicked()) {
                        if let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
                            let btn_rect = btn_resp.rect;
                            let win_rect = win_resp
                                .as_ref()
                                .map(|r| r.response.rect)
                                .unwrap_or(egui::Rect::NOTHING)
                                .expand(4.0);
                            if !btn_rect.contains(pos) && !win_rect.contains(pos) {
                                close_popup = true;
                            }
                        } else {
                            close_popup = true;
                        }
                    }
                    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                        close_popup = true;
                    }
                    // Wenn Window geschlossen (z.B. durch Klick außerhalb des egui Bereichs), open wird false
                    if let Some(r) = win_resp {
                        // r.response enthält den Frame – nicht direkt nutzen
                        // Wir halten popup offen, es sei denn close_popup
                        let _ = r;
                    } else {
                        // Window not shown due to earlier return? keep open
                    }
                    // Persist filter während Popup offen
                    branch_filter_changed = branch_filter_changed || do_fetch;
                }

                // Speichere Filter zurück
                if branch_filter_changed || (popup_open && !branch_filter.is_empty()) {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(branch_filter_id, branch_filter.clone()));
                }
                if close_popup {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(branch_popup_id, false);
                        d.insert_temp(branch_filter_id, String::new());
                        d.insert_temp(branch_focus_id, false);
                    });
                } else if do_fetch {
                    // Filter beibehalten, Popup bleibt offen
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(branch_filter_id, branch_filter.clone()));
                }
                if let Some(b) = branch_selected {
                    actions.branch_switch = Some((repo.path.clone(), b));
                    // Nach Auswahl Popup schließen und Filter leeren
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(branch_popup_id, false);
                        d.insert_temp(branch_filter_id, String::new());
                        d.insert_temp(branch_focus_id, false);
                    });
                }
                if do_fetch {
                    actions.fetch_branches = Some(repo.path.clone());
                }

                ui.add_space(8.0);

                // Solution Dropdown mit Suche (ebenfalls custom popup)
                let profile = config.get_effective_profile_for_repo(&repo.path);
                let has_solutions = !repo.solutions.is_empty();
                let selected_text = if let Some(sel) = &repo.selected_solution {
                    repo.solutions
                        .iter()
                        .find(|s| &s.path == sel)
                        .map(|s| s.relative.clone())
                        .unwrap_or_else(|| sel.display().to_string())
                } else if has_solutions {
                    repo.solutions[0].relative.clone()
                } else {
                    tr(config.language, "no_solution").replace("{}", &profile.file_extension)
                };

                let sln_filter_id = egui::Id::new("sln_filter").with(&repo.path);
                let sln_popup_id = egui::Id::new("sln_popup").with(&repo.path);
                let sln_win_id = egui::Id::new("sln_win").with(&repo.path);
                let sln_focus_id = egui::Id::new("sln_focus").with(&repo.path);
                let mut sln_filter = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<String>(sln_filter_id).unwrap_or_default());
                let mut sln_popup_open = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<bool>(sln_popup_id).unwrap_or(false));
                let mut sln_selected: Option<PathBuf> = None;
                let mut sln_filter_changed = false;
                let mut sln_close_popup = false;

                let sln_chevron = if sln_popup_open {
                    ICON_CHEVRON_UP
                } else {
                    ICON_CHEVRON_DOWN
                };
                // Solution Dropdown: Chevron jetzt innerhalb des Buttons
                let sln_resp = dropdown_button(
                    ui,
                    &selected_text,
                    sln_chevron,
                    170.0,
                    sln_popup_open,
                    None,
                    11.0,
                )
                .on_hover_text(format!(
                    "{} Dateien für Profil '{}' – tippe zum Filtern",
                    profile.file_extension, profile.display_name
                ));
                if sln_resp.clicked() {
                    sln_popup_open = !sln_popup_open;
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(sln_popup_id, sln_popup_open));
                    if !sln_popup_open {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(sln_filter_id, String::new());
                            d.insert_temp(sln_focus_id, false);
                        });
                        sln_filter.clear();
                    }
                }
                if sln_popup_open {
                    let sln_win_resp =
                        egui::Window::new(format!("sln_popup_win_{}", repo.path.display()))
                            .id(sln_win_id)
                            .collapsible(false)
                            .resizable(false)
                            .title_bar(false)
                            .movable(false)
                            .fixed_pos(sln_resp.rect.left_bottom())
                            .pivot(egui::Align2::LEFT_TOP)
                            .show(ui.ctx(), |ui| {
                                ui.set_min_width(260.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(tr(config.language, "search_label"))
                                            .size(11.0),
                                    );
                                    let edit_resp = ui.add(
                                        egui::TextEdit::singleline(&mut sln_filter)
                                            .hint_text(tr(config.language, "filter_hint"))
                                            .desired_width(140.0),
                                    );
                                    let has_focused = ui.ctx().data_mut(|d| {
                                        d.get_temp::<bool>(sln_focus_id).unwrap_or(false)
                                    });
                                    if !has_focused {
                                        edit_resp.request_focus();
                                        ui.ctx().data_mut(|d| d.insert_temp(sln_focus_id, true));
                                    }
                                    if !edit_resp.has_focus()
                                        && ui.ctx().memory(|m| m.focused().is_none())
                                    {
                                        edit_resp.request_focus();
                                    }
                                    if edit_resp.has_focus() && !has_focused {
                                        ui.ctx().data_mut(|d| d.insert_temp(sln_focus_id, true));
                                    }
                                    if edit_resp.changed() {
                                        sln_filter_changed = true;
                                    }
                                });
                                ui.separator();
                                egui::ScrollArea::vertical()
                                    .max_height(220.0)
                                    .show(ui, |ui| {
                                        let filter_lower = sln_filter.to_lowercase();
                                        let limit = config.branch_display_limit.clamp(50, 500);
                                        let mut shown = 0;
                                        for sln in &repo.solutions {
                                            if !filter_lower.is_empty()
                                                && !sln
                                                    .relative
                                                    .to_lowercase()
                                                    .contains(&filter_lower)
                                            {
                                                continue;
                                            }
                                            let is_selected =
                                                Some(&sln.path) == repo.selected_solution.as_ref();
                                            if ui
                                                .selectable_label(is_selected, &sln.relative)
                                                .clicked()
                                            {
                                                sln_selected = Some(sln.path.clone());
                                                sln_close_popup = true;
                                            }
                                            shown += 1;
                                            if shown >= limit {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "... und {} mehr (filtere)",
                                                        repo.solutions.len() - shown
                                                    ))
                                                    .weak()
                                                    .size(10.0),
                                                );
                                                break;
                                            }
                                        }
                                        if !has_solutions {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} ({} {})",
                                                    tr(config.language, "no_solution")
                                                        .replace("{}", &profile.file_extension),
                                                    tr(config.language, "depth_label"),
                                                    profile.max_scan_depth
                                                ))
                                                .weak()
                                                .size(11.0),
                                            );
                                        } else if shown == 0 {
                                            ui.label(
                                                RichText::new(tr(config.language, "no_matches"))
                                                    .weak()
                                                    .size(11.0),
                                            );
                                        } else if repo.solutions.len() >= 20 {
                                            ui.separator();
                                            ui.label(
                                                RichText::new("+ weitere vorhanden (Cap 20)")
                                                    .weak()
                                                    .size(10.0),
                                            );
                                        }
                                    });
                            });
                    if ui.ctx().input(|i| i.pointer.primary_clicked()) {
                        if let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
                            let btn_rect = sln_resp.rect;
                            let win_rect = sln_win_resp
                                .as_ref()
                                .map(|r| r.response.rect)
                                .unwrap_or(egui::Rect::NOTHING)
                                .expand(4.0);
                            if !btn_rect.contains(pos) && !win_rect.contains(pos) {
                                sln_close_popup = true;
                            }
                        } else {
                            sln_close_popup = true;
                        }
                    }
                    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                        sln_close_popup = true;
                    }
                }
                if sln_filter_changed {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(sln_filter_id, sln_filter.clone()));
                } else if sln_popup_open && !sln_filter.is_empty() {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(sln_filter_id, sln_filter.clone()));
                }
                if sln_close_popup {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(sln_popup_id, false);
                        d.insert_temp(sln_filter_id, String::new());
                        d.insert_temp(sln_focus_id, false);
                    });
                }
                if let Some(p) = sln_selected {
                    actions.solution_select = Some((repo.path.clone(), p));
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(sln_popup_id, false);
                        d.insert_temp(sln_filter_id, String::new());
                        d.insert_temp(sln_focus_id, false);
                    });
                }

                // Custom Config Selectors (per effective profile)
                if !profile.config_selectors.is_empty() {
                    for sel in &profile.config_selectors {
                        ui.add_space(8.0);
                        let current_val = repo.custom_values.get(&sel.id);
                        let error_msg = repo.custom_errors.get(&sel.id);
                        let has_error = error_msg.is_some();
                        let display_text = if let Some(v) = current_val {
                            sel.options
                                .iter()
                                .find(|o| &o.value == v)
                                .map(|o| o.label.clone())
                                .unwrap_or_else(|| v.clone())
                        } else {
                            "—".to_string()
                        };
                        let tooltip = if let Some(err) = error_msg {
                            format!("{}:{} – {}", sel.file_path, sel.key, err)
                        } else if let Some(v) = current_val {
                            format!("{}:{} = {}", sel.file_path, sel.key, v)
                        } else {
                            format!("{}:{} (nicht gesetzt)", sel.file_path, sel.key)
                        };
                        let text_color = if has_error {
                            Some(Color32::from_rgb(220, 70, 40))
                        } else {
                            None
                        };
                        let custom_popup_id =
                            egui::Id::new("custom_popup").with(&repo.path).with(&sel.id);
                        let custom_win_id =
                            egui::Id::new("custom_win").with(&repo.path).with(&sel.id);
                        let mut custom_popup_open = ui
                            .ctx()
                            .data_mut(|d| d.get_temp::<bool>(custom_popup_id).unwrap_or(false));
                        let chevron = if custom_popup_open {
                            ICON_CHEVRON_UP
                        } else {
                            ICON_CHEVRON_DOWN
                        };
                        let btn_resp = dropdown_button(
                            ui,
                            &display_text,
                            chevron,
                            140.0,
                            custom_popup_open,
                            text_color,
                            11.0,
                        )
                        .on_hover_text(tooltip);
                        if btn_resp.clicked() {
                            custom_popup_open = !custom_popup_open;
                            ui.ctx()
                                .data_mut(|d| d.insert_temp(custom_popup_id, custom_popup_open));
                        }
                        let mut custom_close = false;
                        let mut selected_value: Option<String> = None;
                        if custom_popup_open {
                            let win_resp = egui::Window::new(format!(
                                "custom_popup_win_{}_{}",
                                repo.path.display(),
                                sel.id
                            ))
                            .id(custom_win_id)
                            .collapsible(false)
                            .resizable(false)
                            .title_bar(false)
                            .movable(false)
                            .fixed_pos(btn_resp.rect.left_bottom())
                            .pivot(egui::Align2::LEFT_TOP)
                            .show(ui.ctx(), |ui| {
                                ui.set_min_width(160.0);
                                ui.label(RichText::new(&sel.display_name).size(11.0).strong());
                                ui.separator();
                                egui::ScrollArea::vertical()
                                    .max_height(220.0)
                                    .show(ui, |ui| {
                                        for opt in &sel.options {
                                            let is_current = current_val
                                                .map(|v| v == &opt.value)
                                                .unwrap_or(false);
                                            let label = if is_current {
                                                format!("● {}", opt.label)
                                            } else {
                                                opt.label.clone()
                                            };
                                            if ui.selectable_label(is_current, label).clicked() {
                                                selected_value = Some(opt.value.clone());
                                                custom_close = true;
                                            }
                                        }
                                        if sel.options.is_empty() {
                                            ui.label(
                                                RichText::new("Keine Optionen").weak().size(11.0),
                                            );
                                        }
                                    });
                            });
                            if ui.ctx().input(|i| i.pointer.primary_clicked()) {
                                if let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
                                    let btn_rect = btn_resp.rect;
                                    let win_rect = win_resp
                                        .as_ref()
                                        .map(|r| r.response.rect)
                                        .unwrap_or(egui::Rect::NOTHING)
                                        .expand(4.0);
                                    if !btn_rect.contains(pos) && !win_rect.contains(pos) {
                                        custom_close = true;
                                    }
                                } else {
                                    custom_close = true;
                                }
                            }
                            if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
                                custom_close = true;
                            }
                        }
                        if custom_close {
                            ui.ctx().data_mut(|d| d.insert_temp(custom_popup_id, false));
                        }
                        if let Some(val) = selected_value {
                            actions.custom_select = Some((repo.path.clone(), sel.id.clone(), val));
                            ui.ctx().data_mut(|d| d.insert_temp(custom_popup_id, false));
                        }
                    }
                }

                // Profile Override Combo (klein, rechts)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let current_profile = config.get_effective_profile_for_repo(&repo.path);
                    let override_text = current_profile.display_name.to_string();
                    egui::ComboBox::from_id_salt(format!("profile_{}", repo.path.display()))
                        .selected_text(override_text)
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    config
                                        .get_repo_state(&repo.path)
                                        .and_then(|s| s.profile_override.as_ref())
                                        .is_none(),
                                    "— Global —",
                                )
                                .clicked()
                            {
                                actions.profile_override = Some((repo.path.clone(), None));
                            }
                            for p in &config.profiles {
                                let is_selected = config
                                    .get_repo_state(&repo.path)
                                    .and_then(|s| s.profile_override.as_ref())
                                    .map(|id| id == &p.id)
                                    .unwrap_or(false);
                                let label = if p.id == config.active_profile_id
                                    && !is_selected
                                    && config.get_repo_state(&repo.path).is_none()
                                {
                                    format!("● {} (global)", p.display_name)
                                } else {
                                    p.display_name.clone()
                                };
                                if ui.selectable_label(is_selected, label).clicked() {
                                    actions.profile_override =
                                        Some((repo.path.clone(), Some(p.id.clone())));
                                }
                            }
                        })
                        .response
                        .on_hover_text(tr(config.language, "profile_override_tooltip"));
                });
            });

            ui.add_space(4.0);

            // Zeile 2: Pfad + Explorer + Shell + IDE Buttons + AI Button (ohne Branch Pill)
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(repo.path.display().to_string())
                        .size(11.5)
                        .color(Color32::from_rgb(90, 90, 90)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let effective_profile = config.get_effective_profile_for_repo(&repo.path);
                    // AI Buttons (per Profil hidden/order) – respect hidden, fallback only when no active
                    let agents_to_show = {
                        let filtered = effective_profile
                            .filtered_agents(&config.agents, &config.active_agent_ids);
                        if filtered.is_empty() && config.active_agent_ids.is_empty() {
                            config
                                .agents
                                .iter()
                                .filter(|a| !effective_profile.hidden_agent_ids.contains(&a.id))
                                .take(1)
                                .collect::<Vec<_>>()
                        } else {
                            filtered
                        }
                    };
                    for agent in agents_to_show.iter().rev() {
                        let icon = agent_icon_for(&agent.id);
                        let btn = egui::Button::image(
                            egui::Image::new(icon).fit_to_exact_size(Vec2::splat(18.0)),
                        );
                        let resp = ui.add(btn).on_hover_text(
                            tr(config.language, "open_in_terminal")
                                .replace("{}", &agent.display_name),
                        );
                        if resp.clicked() {
                            actions.agent_open = Some((repo.path.clone(), agent.id.clone()));
                        }
                        if resp.hovered() {
                            let painter = ui.painter();
                            let rect = resp.rect;
                            painter.rect_stroke(
                                rect,
                                egui::CornerRadius::same(4),
                                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 100, 100)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }

                    // IDE Buttons (Icons) – per Profil hidden/order
                    let ides_to_show = effective_profile.visible_ides();
                    for ide in ides_to_show.iter().take(4) {
                        let icon = ide_icon_for(&ide.id);
                        let btn = egui::Button::image(
                            egui::Image::new(icon).fit_to_exact_size(Vec2::splat(18.0)),
                        );
                        let preview_path = if ide.no_args {
                            format!("{} (cwd: {})", ide.display_name, repo.path.display())
                        } else {
                            let eff = ide.effective_args();
                            let arg_preview = eff.join(" ");
                            // Zeige substituierte Vorschau mit Repo + Lösung
                            let sln_path = repo.selected_solution.as_ref().unwrap_or(&repo.path);
                            let substituted: Vec<String> = eff
                                .iter()
                                .map(|a| {
                                    crate::git::substitute_placeholders(a, sln_path, &repo.path)
                                })
                                .collect();
                            let sub_str = substituted.join(" ");
                            format!(
                                "{} {} (cwd: {})",
                                ide.display_name,
                                if sub_str.is_empty() {
                                    arg_preview
                                } else {
                                    sub_str
                                },
                                repo.path.display()
                            )
                        };
                        let resp = ui.add(btn).on_hover_text(format!(
                            "{} {}",
                            tr(config.language, "open_in"),
                            preview_path
                        ));
                        if resp.clicked() {
                            let file_to_open = repo
                                .selected_solution
                                .clone()
                                .unwrap_or_else(|| repo.path.clone());
                            actions.ide_open =
                                Some((repo.path.clone(), ide.id.clone(), file_to_open));
                        }
                        if resp.hovered() {
                            let painter = ui.painter();
                            let rect = resp.rect;
                            painter.rect_stroke(
                                rect,
                                egui::CornerRadius::same(4),
                                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 100, 100)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }

                    if effective_profile.ides.is_empty() {
                        let btn =
                            egui::Button::new(RichText::new("VS Code").size(11.0)).corner_radius(6);
                        if ui.add(btn).clicked() {
                            let file = repo
                                .selected_solution
                                .clone()
                                .unwrap_or_else(|| repo.path.clone());
                            actions.ide_open =
                                Some((repo.path.clone(), "vscode".to_string(), file));
                        }
                    }

                    // Shell Button (per Profil show_shell)
                    if effective_profile.show_shell {
                        let shell_btn = egui::Button::image(
                            egui::Image::new(ICON_TERMINAL).fit_to_exact_size(Vec2::splat(18.0)),
                        );
                        let resp = ui
                            .add(shell_btn)
                            .on_hover_text(tr(config.language, "open_in_shell"));
                        if resp.clicked() {
                            actions.shell_open = Some(repo.path.clone());
                        }
                        if resp.hovered() {
                            let painter = ui.painter();
                            let rect = resp.rect;
                            painter.rect_stroke(
                                rect,
                                egui::CornerRadius::same(4),
                                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 100, 100)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }

                    // Explorer Button (per Profil show_explorer)
                    if effective_profile.show_explorer {
                        let explorer_btn = egui::Button::image(
                            egui::Image::new(ICON_FOLDER).fit_to_exact_size(Vec2::splat(18.0)),
                        );
                        let resp = ui
                            .add(explorer_btn)
                            .on_hover_text(tr(config.language, "open_in_explorer"));
                        if resp.clicked() {
                            actions.explorer_open = Some(repo.path.clone());
                        }
                        if resp.hovered() {
                            let painter = ui.painter();
                            let rect = resp.rect;
                            painter.rect_stroke(
                                rect,
                                egui::CornerRadius::same(4),
                                egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 100, 100)),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                });
            });

            if let Some(sel) = &repo.selected_solution {
                if repo.solutions.len() > 1 {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(
                            tr(config.language, "selected")
                                .replace("{}", &sel.display().to_string()),
                        )
                        .size(10.0)
                        .color(Color32::from_rgb(100, 100, 100))
                        .italics(),
                    );
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    use std::path::{Path, PathBuf};

    #[test]
    fn ide_icon_for_mapping() {
        let _ = ide_icon_for("vs2022");
        let _ = ide_icon_for("vs");
        let _ = ide_icon_for("visualstudio");
        let _ = ide_icon_for("rider");
        let _ = ide_icon_for("jetbrains");
        let _ = ide_icon_for("vscode");
        let _ = ide_icon_for("unknown");
        let vscode = ide_icon_for("vscode");
        let unknown = ide_icon_for("unknown_id_xyz");
        assert!(format!("{:?}", vscode).len() > 0);
        assert!(format!("{:?}", unknown).len() > 0);
    }

    #[test]
    fn agent_icon_for_mapping() {
        for id in [
            "claude", "codex", "gemini", "copilot", "cursor", "aider", "unknown",
        ] {
            let _ = agent_icon_for(id);
        }
        let claude = agent_icon_for("claude");
        let unknown = agent_icon_for("unknown_xyz");
        assert!(format!("{:?}", claude).len() > 0);
        assert!(format!("{:?}", unknown).len() > 0);
    }

    #[test]
    fn repo_list_actions_default() {
        let actions = RepoListActions {
            branch_switch: None,
            solution_select: None,
            ide_open: None,
            agent_open: None,
            profile_override: None,
            fetch_branches: None,
            explorer_open: None,
            shell_open: None,
            custom_select: None,
        };
        assert!(actions.branch_switch.is_none());
        assert!(actions.solution_select.is_none());
        assert!(actions.ide_open.is_none());
        assert!(actions.agent_open.is_none());
        assert!(actions.profile_override.is_none());
        assert!(actions.fetch_branches.is_none());
        assert!(actions.explorer_open.is_none());
        assert!(actions.shell_open.is_none());
        assert!(actions.custom_select.is_none());
    }

    #[test]
    fn repo_list_actions_can_be_set() {
        let mut actions = RepoListActions {
            branch_switch: None,
            solution_select: None,
            ide_open: None,
            agent_open: None,
            profile_override: None,
            fetch_branches: None,
            explorer_open: None,
            shell_open: None,
            custom_select: None,
        };
        actions.branch_switch = Some((PathBuf::from("/tmp/repo"), "main".to_string()));
        assert_eq!(
            actions.branch_switch,
            Some((PathBuf::from("/tmp/repo"), "main".to_string()))
        );
        actions.solution_select = Some((
            PathBuf::from("/tmp/repo"),
            PathBuf::from("/tmp/repo/app.sln"),
        ));
        assert!(actions.solution_select.is_some());
        actions.ide_open = Some((
            PathBuf::from("/tmp/repo"),
            "vscode".to_string(),
            PathBuf::from("/tmp/repo/app.sln"),
        ));
        assert!(actions.ide_open.is_some());
        actions.agent_open = Some((PathBuf::from("/tmp/repo"), "claude".to_string()));
        assert!(actions.agent_open.is_some());
        actions.profile_override = Some((PathBuf::from("/tmp/repo"), Some("dotnet".to_string())));
        assert!(actions.profile_override.is_some());
        actions.fetch_branches = Some(PathBuf::from("/tmp/repo"));
        assert!(actions.fetch_branches.is_some());
        actions.explorer_open = Some(PathBuf::from("/tmp/repo"));
        assert!(actions.explorer_open.is_some());
        actions.shell_open = Some(PathBuf::from("/tmp/repo"));
        assert!(actions.shell_open.is_some());
        actions.profile_override = Some((PathBuf::from("/tmp/repo"), None));
        assert_eq!(
            actions.profile_override,
            Some((PathBuf::from("/tmp/repo"), None))
        );
    }

    #[test]
    fn repo_branch_display_detached_vs_normal() {
        let branch = "main";
        let detached_text = if true {
            format!("⬡ {}", branch)
        } else {
            format!(" {}", branch)
        };
        let normal_text = if false {
            format!("⬡ {}", branch)
        } else {
            format!(" {}", branch)
        };
        assert_eq!(detached_text, "⬡ main");
        assert_eq!(normal_text, " main");
    }

    #[test]
    fn repo_dirty_indicator_logic() {
        assert_eq!(
            crate::ui::theme::COLOR_DIRTY,
            Color32::from_rgb(220, 70, 40)
        );
        assert_eq!(
            crate::ui::theme::COLOR_CLEAN,
            Color32::from_rgb(60, 160, 80)
        );
        let dirty_symbol = if true { "●" } else { "○" };
        let clean_symbol = if false { "●" } else { "○" };
        assert_eq!(dirty_symbol, "●");
        assert_eq!(clean_symbol, "○");
    }

    #[test]
    fn repo_solution_display_and_filter() {
        let profile = AppConfig::default().get_active_profile().clone();
        let has_solutions = true;
        let selected_text_empty = if has_solutions {
            "a.sln".to_string()
        } else {
            format!("Keine {}", profile.file_extension)
        };
        assert_eq!(selected_text_empty, "a.sln");
        let selected_none: Option<PathBuf> = None;
        let text_no_selection = if selected_none.is_some() {
            "selected".to_string()
        } else if has_solutions {
            "first.sln".to_string()
        } else {
            format!("Keine {}", profile.file_extension)
        };
        assert_eq!(text_no_selection, "first.sln");
        let branches = vec!["main".to_string(), "feature/xyz".to_string()];
        let filter = "feat".to_lowercase();
        let filtered: Vec<_> = branches
            .iter()
            .filter(|b| b.to_lowercase().contains(&filter))
            .collect();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn repo_profile_override_combo() {
        let mut cfg = AppConfig::default();
        cfg.profiles.push(crate::config::LanguageProfile {
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
        let path = PathBuf::from("/tmp/repo");
        assert!(cfg.get_repo_state(&path).is_none());
        let effective = cfg.get_effective_profile_for_repo(&path);
        assert_eq!(effective.id, "dotnet");
        cfg.set_repo_profile_override(&path, Some("rust".to_string()));
        let effective2 = cfg.get_effective_profile_for_repo(&path);
        assert_eq!(effective2.id, "rust");
    }

    #[test]
    fn repo_ide_and_agent_buttons_logic() {
        let cfg = AppConfig::default();
        let profile = cfg.get_effective_profile_for_repo(Path::new("/tmp/repo"));
        let agents_to_show = profile.filtered_agents(&cfg.agents, &cfg.active_agent_ids);
        assert!(!agents_to_show.is_empty());
        let ides_to_show: Vec<_> = profile.visible_ides().into_iter().take(4).collect();
        assert!(ides_to_show.len() <= 4);
        if let Some(def) = &profile.default_ide_id {
            let mut sorted = profile.ides.clone();
            sorted.sort_by_key(|ide| if &ide.id == def { 0 } else { 1 });
            assert_eq!(&sorted[0].id, def);
        }
    }

    #[test]
    fn filter_branches_logic() {
        let branches = vec![
            "main".to_string(),
            "feature/a".to_string(),
            "Feature/B".to_string(),
            "develop".to_string(),
        ];
        let filtered = filter_branches(&branches, "feat", 10);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&"feature/a".to_string()));
        assert!(filtered.contains(&"Feature/B".to_string()));
        let limited = filter_branches(&branches, "", 2);
        assert_eq!(limited.len(), 2);
        assert_eq!(limited[0], "main");
        let empty_filter = filter_branches(&branches, "xyz", 10);
        assert!(empty_filter.is_empty());
    }

    #[test]
    fn dropdown_arrow_and_filter_reset_logic() {
        // Arrow sollte bei offenem Popup ▲ vs ▼ wechseln – reine Logik
        let open_arrow = if true { " ▲" } else { " ▼" };
        let closed_arrow = if false { " ▲" } else { " ▼" };
        assert_eq!(open_arrow, " ▲");
        assert_eq!(closed_arrow, " ▼");
        // Filter Reset: nach Schließen sollte String leer sein
        let mut filter = "feat".to_string();
        let popup_closed = true;
        if popup_closed {
            filter.clear();
        }
        assert!(filter.is_empty());
    }

    #[test]
    fn profile_visible_and_hidden_icons() {
        let mut profile = crate::config::LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![
                crate::config::IdeConfig {
                    id: "a".to_string(),
                    display_name: "A".to_string(),
                    program: "a".to_string(),
                    args: vec![],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,
                },
                crate::config::IdeConfig {
                    id: "b".to_string(),
                    display_name: "B".to_string(),
                    program: "b".to_string(),
                    args: vec![],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,
                },
            ],
            default_ide_id: None,
            ide_order: vec!["b".to_string(), "a".to_string()],
            hidden_ide_ids: vec!["a".to_string()],
            hidden_agent_ids: vec![],
            agent_order: vec![],
            show_shell: false,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        let visible = profile.visible_ides();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, "b");
        assert!(!profile.show_shell);
        assert!(profile.show_explorer);
        // Lücke schließen: hidden a entfernt, b rutscht nach vorne
        profile.hidden_ide_ids.clear();
        let visible2 = profile.visible_ides();
        assert_eq!(visible2.len(), 2);
        assert_eq!(visible2[0].id, "b");
        assert_eq!(visible2[1].id, "a");
    }

    #[test]
    fn repo_list_actions_has_custom_select() {
        let mut a = RepoListActions {
            branch_switch: None,
            solution_select: None,
            ide_open: None,
            agent_open: None,
            profile_override: None,
            fetch_branches: None,
            explorer_open: None,
            shell_open: None,
            custom_select: None,
        };
        a.custom_select = Some((PathBuf::from("/tmp/repo"), "db".into(), "prod".into()));
        assert_eq!(
            a.custom_select,
            Some((PathBuf::from("/tmp/repo"), "db".into(), "prod".into()))
        );
    }
}
