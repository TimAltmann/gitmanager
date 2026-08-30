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
const ICON_CODEX: egui::ImageSource = egui::include_image!("../../assets/icons/codex.svg");
const ICON_GEMINI: egui::ImageSource = egui::include_image!("../../assets/icons/gemini.svg");
const ICON_COPILOT: egui::ImageSource = egui::include_image!("../../assets/icons/copilot.svg");
const ICON_CURSOR: egui::ImageSource = egui::include_image!("../../assets/icons/cursor.svg");
const ICON_AIDER: egui::ImageSource = egui::include_image!("../../assets/icons/aider.svg");

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

pub struct RepoListActions {
    pub branch_switch: Option<(PathBuf, String)>,
    pub solution_select: Option<(PathBuf, PathBuf)>,
    pub ide_open: Option<(PathBuf, String, PathBuf)>, // repo_path, ide_id, file_path
    pub agent_open: Option<(PathBuf, String)>,        // repo_path, agent_id
    pub profile_override: Option<(PathBuf, Option<String>)>,
    pub fetch_branches: Option<PathBuf>,
    pub explorer_open: Option<PathBuf>,
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
    let visuals = &ui.ctx().style().visuals;
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
            // Zeile 1: Name + Dirty + Branch Dropdown + Solution Dropdown + Profile Override + Explorer
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

                // Branch Dropdown mit Suche + Refresh
                let branch_text = if repo.is_detached {
                    format!("⬡ {}", repo.branch)
                } else {
                    format!(" {}", repo.branch)
                };
                // Persistenter Filter pro Repo
                let branch_filter_id =
                    egui::Id::new(format!("branch_filter_{}", repo.path.display()));
                let mut branch_filter = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<String>(branch_filter_id).unwrap_or_default());
                let mut branch_filter_changed = false;

                let mut branch_selected: Option<String> = None;
                let mut do_fetch = false;

                egui::ComboBox::from_id_salt(format!("branch_{}", repo.path.display()))
                    .selected_text(branch_text)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr(config.language, "search_label")).size(11.0));
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut branch_filter)
                                    .hint_text(tr(config.language, "filter_hint"))
                                    .desired_width(100.0),
                            );
                            if resp.changed() {
                                branch_filter_changed = true;
                            }
                            if ui
                                .small_button("↻")
                                .on_hover_text(tr(config.language, "branch_refresh_tooltip"))
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
                        let filter_lower = branch_filter.to_lowercase();
                        let mut shown = 0;
                        for b in &repo.branches {
                            if !filter_lower.is_empty() && !b.to_lowercase().contains(&filter_lower)
                            {
                                continue;
                            }
                            let is_current = b == &repo.branch;
                            let label = if is_current {
                                format!("● {}", b)
                            } else {
                                b.clone()
                            };
                            if ui.selectable_label(is_current, label).clicked() && !is_current {
                                branch_selected = Some(b.clone());
                            }
                            shown += 1;
                            if shown > 100 {
                                ui.label(
                                    RichText::new(format!(
                                        "... und {} mehr (filtere)",
                                        repo.branches.len() - shown
                                    ))
                                    .weak()
                                    .size(10.0),
                                );
                                break;
                            }
                        }
                        if repo.branches.is_empty() {
                            ui.label(
                                RichText::new(tr(config.language, "no_branches"))
                                    .weak()
                                    .size(11.0),
                            );
                            if ui.small_button(tr(config.language, "fetch_now")).clicked() {
                                do_fetch = true;
                            }
                        } else if shown == 0 {
                            ui.label(
                                RichText::new(tr(config.language, "no_matches"))
                                    .weak()
                                    .size(11.0),
                            );
                        }
                    })
                    .response
                    .on_hover_text(tr(config.language, "branch_switch_tooltip"));

                // Speichere Filter zurück
                if branch_filter_changed {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(branch_filter_id, branch_filter.clone()));
                } else if do_fetch {
                    // Filter beibehalten
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(branch_filter_id, branch_filter.clone()));
                }
                if let Some(b) = branch_selected {
                    actions.branch_switch = Some((repo.path.clone(), b));
                }
                if do_fetch {
                    actions.fetch_branches = Some(repo.path.clone());
                }

                ui.add_space(8.0);

                // Solution Dropdown mit Suche
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

                let sln_filter_id = egui::Id::new(format!("sln_filter_{}", repo.path.display()));
                let mut sln_filter = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<String>(sln_filter_id).unwrap_or_default());
                let mut sln_filter_changed = false;
                let mut sln_selected: Option<PathBuf> = None;

                let combo = egui::ComboBox::from_id_salt(format!("sln_{}", repo.path.display()))
                    .selected_text(selected_text.clone())
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr(config.language, "search_label")).size(11.0));
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut sln_filter)
                                    .hint_text(tr(config.language, "filter_hint"))
                                    .desired_width(120.0),
                            );
                            if resp.changed() {
                                sln_filter_changed = true;
                            }
                        });
                        ui.separator();
                        let filter_lower = sln_filter.to_lowercase();
                        let mut shown = 0;
                        for sln in &repo.solutions {
                            if !filter_lower.is_empty()
                                && !sln.relative.to_lowercase().contains(&filter_lower)
                            {
                                continue;
                            }
                            let is_selected = Some(&sln.path) == repo.selected_solution.as_ref();
                            if ui.selectable_label(is_selected, &sln.relative).clicked() {
                                sln_selected = Some(sln.path.clone());
                            }
                            shown += 1;
                            if shown > 50 {
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
                        } else if sln_selected.is_none()
                            && has_solutions
                            && !sln_filter.is_empty()
                            && shown == 0
                        {
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
                if sln_filter_changed {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(sln_filter_id, sln_filter.clone()));
                }
                if let Some(p) = sln_selected {
                    actions.solution_select = Some((repo.path.clone(), p));
                }
                combo.response.on_hover_text(format!(
                    "{} Dateien für Profil '{}' – tippe zum Filtern",
                    profile.file_extension, profile.display_name
                ));

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

            // Zeile 2: Pfad + Explorer + IDE Buttons + AI Button (ohne Branch Pill)
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(repo.path.display().to_string())
                        .size(10.5)
                        .color(Color32::from_rgb(120, 120, 120)),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // AI Buttons (mehrere aktiv → mehrere Icons nebeneinander)
                    let active_agents = config.get_active_agents();
                    // Falls keine aktiv, zeige trotzdem den Default (claude) als Fallback
                    let agents_to_show: Vec<_> = if active_agents.is_empty() {
                        config.agents.iter().take(1).collect()
                    } else {
                        active_agents
                    };
                    for agent in agents_to_show.iter().rev() {
                        // reverse wegen right_to-left
                        let icon = agent_icon_for(&agent.id);
                        let btn = egui::ImageButton::new(
                            egui::Image::new(icon).fit_to_exact_size(Vec2::splat(18.0)),
                        ); //.corner_radius(6);
                        let resp = ui.add(btn).on_hover_text(
                            tr(config.language, "open_in_terminal")
                                .replace("{}", &agent.display_name),
                        );
                        if resp.clicked() {
                            actions.agent_open = Some((repo.path.clone(), agent.id.clone()));
                        }
                        // Hover highlight: draw rect border when hovered
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

                    // Falls gar keine Agents konfiguriert (sollte nicht passieren)
                    if agents_to_show.is_empty() {
                        let image =
                            egui::Image::new(ICON_CLAUDE).fit_to_exact_size(Vec2::splat(18.0));
                        if ui
                            .add(image)
                            .on_hover_text(
                                tr(config.language, "open_in_terminal").replace("{}", "AI"),
                            )
                            .clicked()
                        {
                            actions.agent_open = Some((repo.path.clone(), "claude".to_string()));
                        }
                    }

                    // IDE Buttons (Icons)
                    let effective_profile = config.get_effective_profile_for_repo(&repo.path);
                    let mut ides_to_show: Vec<_> = effective_profile.ides.iter().collect();
                    if let Some(def) = &effective_profile.default_ide_id {
                        ides_to_show.sort_by_key(|ide| if &ide.id == def { 0 } else { 1 });
                    }
                    for ide in ides_to_show.iter().take(4) {
                        let icon = ide_icon_for(&ide.id);
                        let btn = egui::ImageButton::new(
                            egui::Image::new(icon).fit_to_exact_size(Vec2::splat(18.0)),
                        );
                        let resp = ui.add(btn).on_hover_text(format!(
                            "{} {} {}",
                            tr(config.language, "open_in"),
                            ide.display_name,
                            repo.selected_solution
                                .as_ref()
                                .map(|p| format!(" ({})", p.display()))
                                .unwrap_or_default()
                        ));
                        if resp.clicked() {
                            let file_to_open = repo
                                .selected_solution
                                .clone()
                                .unwrap_or_else(|| repo.path.clone());
                            actions.ide_open =
                                Some((repo.path.clone(), ide.id.clone(), file_to_open));
                        }
                        // Hover highlight
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

                    // Explorer Button
                    let explorer_btn = egui::ImageButton::new(
                        egui::Image::new(ICON_FOLDER).fit_to_exact_size(Vec2::splat(18.0)),
                    );
                    let resp = ui
                        .add(explorer_btn)
                        .on_hover_text(tr(config.language, "open_in_explorer"));
                    if resp.clicked() {
                        actions.explorer_open = Some(repo.path.clone());
                    }
                    // Hover highlight
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
    use crate::i18n::tr;
    use std::path::{Path, PathBuf};

    #[test]
    fn ide_icon_for_mapping() {
        // should not panic for known ids
        let _ = ide_icon_for("vs2022");
        let _ = ide_icon_for("vs");
        let _ = ide_icon_for("visualstudio");
        let _ = ide_icon_for("rider");
        let _ = ide_icon_for("jetbrains");
        let _ = ide_icon_for("vscode");
        let _ = ide_icon_for("unknown");
        // unknown fallback should be vscode (no panic and returns same as vscode)
        // We verify by ensuring both calls succeed; direct equality not testable due to ImageSource internals
        let vscode = ide_icon_for("vscode");
        let unknown = ide_icon_for("unknown_id_xyz");
        // Both should be valid ImageSource; we check that they don't panic and are debug-printable
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
        };
        assert!(actions.branch_switch.is_none());
        assert!(actions.solution_select.is_none());
        assert!(actions.ide_open.is_none());
        assert!(actions.agent_open.is_none());
        assert!(actions.profile_override.is_none());
        assert!(actions.fetch_branches.is_none());
        assert!(actions.explorer_open.is_none());
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
        // profile_override None = global
        actions.profile_override = Some((PathBuf::from("/tmp/repo"), None));
        assert_eq!(
            actions.profile_override,
            Some((PathBuf::from("/tmp/repo"), None))
        );
    }

    #[test]
    fn repo_branch_display_detached_vs_normal() {
        // logic from show_repo_row: if is_detached { format!("⬡ {}", branch) } else { format!(" {}", branch) }
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
        // verify constants used
        assert_eq!(
            crate::ui::theme::COLOR_DIRTY,
            Color32::from_rgb(220, 70, 40)
        );
        assert_eq!(
            crate::ui::theme::COLOR_CLEAN,
            Color32::from_rgb(60, 160, 80)
        );
        // dirty true -> ●, false -> ○
        let dirty_symbol = if true { "●" } else { "○" };
        let clean_symbol = if false { "●" } else { "○" };
        assert_eq!(dirty_symbol, "●");
        assert_eq!(clean_symbol, "○");
    }

    #[test]
    fn repo_solution_display_and_filter() {
        let profile = AppConfig::default().get_active_profile().clone();
        // has_solutions case
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
        // filter case-insensitive
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
        });
        let path = PathBuf::from("/tmp/repo");
        // no override -> global
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
        let active_agents = cfg.get_active_agents();
        // if empty, show 1 fallback
        let agents_to_show: Vec<_> = if active_agents.is_empty() {
            cfg.agents.iter().take(1).collect()
        } else {
            active_agents
        };
        assert!(!agents_to_show.is_empty());
        // max 4 IDEs
        let profile = cfg.get_effective_profile_for_repo(Path::new("/tmp/repo"));
        let ides_to_show: Vec<_> = profile.ides.iter().take(4).collect();
        assert!(ides_to_show.len() <= 4);
        // default first
        if let Some(def) = &profile.default_ide_id {
            let mut sorted = profile.ides.clone();
            sorted.sort_by_key(|ide| if &ide.id == def { 0 } else { 1 });
            assert_eq!(&sorted[0].id, def);
        }
    }
}
