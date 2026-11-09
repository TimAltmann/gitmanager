//! Ansicht „Claude Sessions“: Suche, Abschnitte nach Repo/Worktree, Karten.

use crate::claude_sessions::{
    claude_projects_dir, session_matches_search, ClaudeSession, RepoSessions,
};
use crate::gitlab::time::{now_unix, relative_time};
use crate::i18n::{tr, tr_fmt, Language};
use egui::{RichText, Ui, Vec2};

const ICON_CLAUDE: egui::ImageSource = egui::include_image!("../../assets/icons/claude.svg");
const ICON_SEARCH: egui::ImageSource = egui::include_image!("../../assets/icons/search.svg");
const ICON_CROSS: egui::ImageSource = egui::include_image!("../../assets/icons/cross.svg");
const ICON_GIT_BRANCH: egui::ImageSource =
    egui::include_image!("../../assets/icons/git-branch.svg");

/// Was der Nutzer in der Ansicht ausgelöst hat.
#[derive(Debug, Default)]
pub struct ClaudeSessionActions {
    /// (Ordner, Session-ID) fortsetzen
    pub resume: Option<(String, String)>,
}

/// Eine sichtbare Gruppe mit ihren passenden Sessions.
pub type VisibleGroup<'a> = (&'a RepoSessions, Vec<&'a ClaudeSession>);

/// Wendet die Suche an und trennt in (Repositories, Worktrees); Gruppen ohne
/// Treffer fallen weg, Reihenfolge bleibt.
pub fn filter_groups<'a>(
    groups: &'a [RepoSessions],
    query: &str,
) -> (Vec<VisibleGroup<'a>>, Vec<VisibleGroup<'a>>) {
    let mut repos = Vec::new();
    let mut worktrees = Vec::new();
    for g in groups {
        let hits: Vec<&ClaudeSession> = g
            .sessions
            .iter()
            .filter(|s| session_matches_search(g, s, query))
            .collect();
        if hits.is_empty() {
            continue;
        }
        if g.is_worktree {
            worktrees.push((g, hits));
        } else {
            repos.push((g, hits));
        }
    }
    (repos, worktrees)
}

/// Zeichnet die Ansicht. Suche wie in der Repo-Liste (Strg+F, Esc leert).
pub fn show_claude_sessions(
    ui: &mut Ui,
    groups: &[RepoSessions],
    scanning: bool,
    lang: Language,
    search: &mut String,
    actions: &mut ClaudeSessionActions,
) {
    if groups.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            if scanning {
                ui.spinner();
                ui.add_space(8.0);
                ui.label(RichText::new(tr(lang, "claude_scanning")).size(13.0).weak());
                return;
            }
            ui.label(
                RichText::new(tr(lang, "claude_none_found"))
                    .size(16.0)
                    .color(egui::Color32::from_rgb(100, 100, 100)),
            );
            ui.add_space(8.0);
            let dir = claude_projects_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_else(|| "~/.claude/projects".to_string());
            ui.label(
                RichText::new(tr_fmt(lang, "claude_none_hint", &[&dir]))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(130, 130, 130)),
            );
        });
        return;
    }

    show_toolbar(ui, lang, search);
    ui.add_space(6.0);

    let (repos, worktrees) = filter_groups(groups, search);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(4.0);
            if repos.is_empty() && worktrees.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);
                    ui.label(
                        RichText::new(tr_fmt(lang, "claude_no_matches", &[search]))
                            .size(13.0)
                            .weak(),
                    );
                });
                return;
            }
            let now = now_unix();
            let sections = [
                ("repo_section_repos", "repos", &repos),
                ("repo_section_worktrees", "worktrees", &worktrees),
            ];
            for (title_key, salt, visible) in sections {
                if visible.is_empty() {
                    continue;
                }
                let count: usize = visible.iter().map(|(_, s)| s.len()).sum();
                let title = format!("{} ({})", tr(lang, title_key), count);
                egui::CollapsingHeader::new(RichText::new(title).size(13.0).strong())
                    .id_salt(("claude_section", salt))
                    .default_open(true)
                    .show_unindented(ui, |ui| {
                        ui.add_space(4.0);
                        for (group, sessions) in visible.iter() {
                            let header = format!("{} ({})", group.repo_name, sessions.len());
                            egui::CollapsingHeader::new(RichText::new(header).size(12.5))
                                .id_salt(("claude_group", &group.repo_path))
                                .default_open(true)
                                .show(ui, |ui| {
                                    for session in sessions {
                                        show_session_card(ui, session, lang, now, actions);
                                        ui.add_space(6.0);
                                    }
                                });
                        }
                    });
                ui.add_space(6.0);
            }
            ui.add_space(12.0);
        });
}

fn show_toolbar(ui: &mut Ui, lang: Language, search: &mut String) {
    ui.horizontal(|ui| {
        ui.add(
            egui::Image::new(ICON_SEARCH)
                .fit_to_exact_size(Vec2::splat(14.0))
                .tint(ui.visuals().weak_text_color()),
        );
        let edit = ui.add(
            egui::TextEdit::singleline(search)
                .id(egui::Id::new("claude_sessions_search"))
                .hint_text(tr(lang, "claude_search_hint"))
                .desired_width(340.0),
        );
        let focus_shortcut = ui.ctx().input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::F,
            ))
        });
        if focus_shortcut {
            edit.request_focus();
        }
        if edit.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            search.clear();
        }
        if !search.is_empty()
            && ui
                .add(
                    egui::Button::image(
                        egui::Image::new(ICON_CROSS)
                            .fit_to_exact_size(Vec2::splat(12.0))
                            .tint(ui.visuals().text_color()),
                    )
                    .small(),
                )
                .on_hover_text(tr(lang, "repo_search_clear"))
                .clicked()
        {
            search.clear();
        }
    });
}

/// Karte im Stil der Repo-Karte; Klick auf Karte oder „Fortsetzen“ setzt
/// `actions.resume`.
fn show_session_card(
    ui: &mut Ui,
    session: &ClaudeSession,
    lang: Language,
    now: i64,
    actions: &mut ClaudeSessionActions,
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

    let mut button_clicked = false;
    let card = frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn = egui::Button::image_and_text(
                    egui::Image::new(ICON_CLAUDE).fit_to_exact_size(Vec2::splat(14.0)),
                    tr(lang, "claude_resume"),
                );
                button_clicked = ui
                    .add(btn)
                    .on_hover_text(tr_fmt(lang, "claude_resume_tt", &[&session.cwd]))
                    .clicked();
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&session.title).size(13.0).strong())
                            .truncate(),
                    );
                    if let Some(prompt) = &session.last_prompt {
                        ui.add(
                            egui::Label::new(RichText::new(prompt).size(11.5).weak()).truncate(),
                        );
                    }
                    ui.horizontal(|ui| {
                        if let Some(branch) = &session.git_branch {
                            ui.add(
                                egui::Image::new(ICON_GIT_BRANCH)
                                    .fit_to_exact_size(Vec2::splat(12.0))
                                    .tint(ui.visuals().weak_text_color()),
                            );
                            ui.label(RichText::new(branch).size(11.0).weak());
                            ui.label(RichText::new("·").size(11.0).weak());
                        }
                        ui.label(
                            RichText::new(relative_time(session.last_activity, now, lang))
                                .size(11.0)
                                .weak(),
                        );
                    });
                });
            });
        });
    });
    let response = ui
        .interact(
            card.response.rect,
            egui::Id::new(("claude_card", &session.id)),
            egui::Sense::click(),
        )
        .on_hover_text(format!("{}\n{}", session.cwd, session.id));
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if button_clicked || response.clicked() {
        actions.resume = Some((session.cwd.clone(), session.id.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_sessions::{ClaudeSession, RepoSessions};
    use crate::i18n::{tr, Language};
    use std::path::PathBuf;

    fn session(id: &str, title: &str) -> ClaudeSession {
        ClaudeSession {
            id: id.to_string(),
            cwd: format!("C:\\{id}"),
            title: title.to_string(),
            last_prompt: None,
            git_branch: None,
            last_activity: 1,
        }
    }

    fn group(name: &str, wt: bool, sessions: Vec<ClaudeSession>) -> RepoSessions {
        RepoSessions {
            repo_path: PathBuf::from(format!("C:\\{name}")),
            repo_name: name.to_string(),
            is_worktree: wt,
            sessions,
        }
    }

    fn groups() -> Vec<RepoSessions> {
        vec![
            group(
                "a",
                false,
                vec![session("a1", "Login fixen"), session("a2", "Doku")],
            ),
            group("b", false, vec![session("b1", "Refactoring")]),
            group("w", true, vec![session("w1", "Login Review")]),
        ]
    }

    #[test]
    fn filter_groups_splits_and_drops_empty() {
        let g = groups();
        let (repos, worktrees) = filter_groups(&g, "login");
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].0.repo_name, "a");
        let ids: Vec<&str> = repos[0].1.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a1"]);
        assert_eq!(worktrees.len(), 1);
        assert_eq!(worktrees[0].0.repo_name, "w");
    }

    #[test]
    fn filter_groups_empty_query_keeps_all() {
        let g = groups();
        let (repos, worktrees) = filter_groups(&g, "");
        let names: Vec<&str> = repos.iter().map(|(g, _)| g.repo_name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(repos[0].1.len(), 2);
        assert_eq!(worktrees.len(), 1);
    }

    #[test]
    fn claude_keys_translated() {
        for key in [
            "view_claude",
            "view_claude_tt",
            "claude_search_hint",
            "claude_none_found",
            "claude_none_hint",
            "claude_no_matches",
            "claude_resume",
            "claude_resume_tt",
            "claude_scanning",
            "claude_status",
        ] {
            for lang in [Language::De, Language::En] {
                assert_ne!(tr(lang, key), key, "{key} fehlt für {lang:?}");
            }
        }
    }
}
