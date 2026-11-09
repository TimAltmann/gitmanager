//! Ansicht „Claude Sessions“: oben die offenen Sessions mit Live-Status,
//! darunter alle übrigen in einer Liste, neueste zuerst.

use crate::claude_live::{open_matches_search, LiveStatus, OpenSession};
use crate::claude_sessions::{claude_projects_dir, session_matches_search, SessionEntry};
use crate::gitlab::time::relative_time;
use crate::i18n::{tr, tr_fmt, Language};
use crate::ui::claude_anim::{
    brighten, format_elapsed, paint_check, paint_pulse_dot, paint_star, seed_of, shimmer_job,
    status_color, verb,
};
use egui::{FontId, RichText, Ui, Vec2};
use std::collections::HashSet;

const ICON_CLAUDE: egui::ImageSource = egui::include_image!("../../assets/icons/claude.svg");
const ICON_SEARCH: egui::ImageSource = egui::include_image!("../../assets/icons/search.svg");
const ICON_CROSS: egui::ImageSource = egui::include_image!("../../assets/icons/cross.svg");
const ICON_GIT_BRANCH: egui::ImageSource =
    egui::include_image!("../../assets/icons/git-branch.svg");

/// Was der Nutzer in der Ansicht ausgelöst hat.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ClaudeSessionActions {
    /// (Ordner, Session-ID) fortsetzen
    pub resume: Option<(String, String)>,
    /// Terminal der laufenden Instanz (PID) nach vorne holen
    pub focus: Option<u32>,
}

/// Sichtbare offene Sessions und übrige Sessions (ohne die offenen), jeweils
/// nach Suche gefiltert; Reihenfolge bleibt.
pub fn visible_lists<'a>(
    open: &'a [OpenSession],
    entries: &'a [SessionEntry],
    query: &str,
) -> (Vec<&'a OpenSession>, Vec<&'a SessionEntry>) {
    let open_ids: HashSet<&str> = open.iter().map(|o| o.live.session_id.as_str()).collect();
    let open_hits = open
        .iter()
        .filter(|o| open_matches_search(o, query))
        .collect();
    let rest = entries
        .iter()
        .filter(|e| !open_ids.contains(e.session.id.as_str()))
        .filter(|e| session_matches_search(e, query))
        .collect();
    (open_hits, rest)
}

/// Eingaben der Ansicht.
pub struct ViewData<'a> {
    pub open: &'a [OpenSession],
    pub entries: &'a [SessionEntry],
    pub scanning: bool,
    pub lang: Language,
    /// Unix-Millisekunden (Laufzeiten)
    pub now_ms: i64,
}

/// Zeichnet die Ansicht.
pub fn show_claude_sessions(
    ui: &mut Ui,
    data: &ViewData,
    search: &mut String,
    actions: &mut ClaudeSessionActions,
) {
    let ViewData {
        open,
        entries,
        scanning,
        lang,
        now_ms,
    } = *data;
    if open.is_empty() && entries.is_empty() {
        show_empty(ui, scanning, lang);
        return;
    }

    show_toolbar(ui, lang, search);
    ui.add_space(6.0);

    let (open_hits, rest) = visible_lists(open, entries, search);
    if open_hits.iter().any(|o| o.live.status.animated()) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(4.0);
            if open_hits.is_empty() && rest.is_empty() {
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
            if !open_hits.is_empty() {
                let title = tr_fmt(lang, "claude_open_section", &[&open_hits.len().to_string()]);
                egui::CollapsingHeader::new(RichText::new(title).size(13.0).strong())
                    .id_salt("claude_open_section")
                    .default_open(true)
                    .show_unindented(ui, |ui| {
                        ui.add_space(4.0);
                        for o in &open_hits {
                            show_open_card(ui, o, lang, now_ms, actions);
                            ui.add_space(6.0);
                        }
                    });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
            }
            if !rest.is_empty() {
                let title = tr_fmt(lang, "claude_all_section", &[&rest.len().to_string()]);
                let now_secs = now_ms / 1000;
                egui::CollapsingHeader::new(RichText::new(title).size(13.0).strong())
                    .id_salt("claude_all_section")
                    .default_open(true)
                    .show_unindented(ui, |ui| {
                        ui.add_space(4.0);
                        for e in &rest {
                            show_session_card(ui, e, lang, now_secs, actions);
                            ui.add_space(6.0);
                        }
                    });
            }
            ui.add_space(12.0);
        });
}

fn show_empty(ui: &mut Ui, scanning: bool, lang: Language) {
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

/// Id der Klickfläche einer Session-Karte.
pub(crate) fn card_id(entry: &SessionEntry) -> egui::Id {
    egui::Id::new(("claude_card", &entry.session.cwd, &entry.session.id))
}

/// Id der Klickfläche einer offenen Karte.
pub(crate) fn open_card_id(o: &OpenSession) -> egui::Id {
    egui::Id::new(("claude_open_card", o.live.pid, &o.live.session_id))
}

/// Karte im Stil der Repo-Karte. Die Klickfläche wird VOR dem Inhalt
/// registriert (Rechteck des vorigen Frames), damit Buttons im Inhalt darüber
/// liegen und Hover/Tooltip behalten. Liefert, ob der Kartenkörper geklickt
/// wurde.
fn clickable_card(
    ui: &mut Ui,
    id: egui::Id,
    stroke: egui::Stroke,
    tooltip: &str,
    add_contents: impl FnOnce(&mut Ui),
) -> bool {
    let rect_key = id.with("rect");
    let response = ui
        .ctx()
        .data(|d| d.get_temp::<egui::Rect>(rect_key))
        .map(|rect| ui.interact(rect, id, egui::Sense::click()));

    let frame = egui::Frame::new()
        .fill(ui.visuals().widgets.inactive.bg_fill)
        .stroke(stroke)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(12, 10));
    let card = frame.show(ui, |ui| {
        // Markierbare Labels würden den Klick auf die Karte abfangen
        ui.style_mut().interaction.selectable_labels = false;
        add_contents(ui);
    });
    ui.ctx()
        .data_mut(|d| d.insert_temp(rect_key, card.response.rect));

    let Some(response) = response else {
        // Erster Frame: Rechteck erst jetzt bekannt, nächster Frame übernimmt
        ui.ctx().request_repaint();
        return false;
    };
    let response = response.on_hover_text(tooltip);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.clicked()
}

/// Fußzeile: Ort · Branch · Zeit (leere Teile entfallen).
fn footer(ui: &mut Ui, place: &str, branch: Option<&str>, time: Option<String>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(place).size(11.0).weak());
        if let Some(branch) = branch {
            ui.label(RichText::new("·").size(11.0).weak());
            ui.add(
                egui::Image::new(ICON_GIT_BRANCH)
                    .fit_to_exact_size(Vec2::splat(12.0))
                    .tint(ui.visuals().weak_text_color()),
            );
            ui.label(RichText::new(branch).size(11.0).weak());
        }
        if let Some(time) = time {
            ui.label(RichText::new("·").size(11.0).weak());
            ui.label(RichText::new(time).size(11.0).weak());
        }
    });
}

/// Session-Karte; Klick auf Karte oder „Fortsetzen“ setzt `actions.resume`.
fn show_session_card(
    ui: &mut Ui,
    entry: &SessionEntry,
    lang: Language,
    now_secs: i64,
    actions: &mut ClaudeSessionActions,
) {
    let session = &entry.session;
    let stroke = egui::Stroke::new(1.0_f32, ui.visuals().widgets.inactive.fg_stroke.color);
    let tooltip = format!("{}\n{}", session.cwd, session.id);
    let mut button_clicked = false;
    let card_clicked = clickable_card(ui, card_id(entry), stroke, &tooltip, |ui| {
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
                    footer(
                        ui,
                        &entry.repo_name,
                        session.git_branch.as_deref(),
                        Some(relative_time(session.last_activity, now_secs, lang)),
                    );
                });
            });
        });
    });
    if button_clicked || card_clicked {
        actions.resume = Some((session.cwd.clone(), session.id.clone()));
    }
}

/// Statuszeile einer offenen Session mit Animation.
fn status_line(ui: &mut Ui, o: &OpenSession, lang: Language, now_ms: i64) {
    let t = ui.input(|i| i.time);
    let status = o.live.status;
    let color = status_color(status);
    let elapsed = format!("({})", format_elapsed(now_ms - o.live.status_since_ms));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        match status {
            LiveStatus::Busy | LiveStatus::Shell => {
                paint_star(ui, 14.0, t, color);
                let text = if status == LiveStatus::Shell {
                    tr(lang, "claude_st_shell")
                } else {
                    format!("{}…", verb(lang, seed_of(&o.live.session_id), t))
                };
                let job = shimmer_job(&text, t, FontId::proportional(12.5), color, brighten(color));
                ui.label(job);
                ui.label(RichText::new(elapsed).size(11.0).weak());
            }
            LiveStatus::Waiting => {
                paint_pulse_dot(ui, 14.0, t, color);
                ui.label(
                    RichText::new(tr(lang, "claude_st_waiting"))
                        .size(12.5)
                        .strong()
                        .color(color),
                );
                ui.label(RichText::new(elapsed).size(11.0).weak());
            }
            LiveStatus::Idle => {
                paint_check(ui, 14.0, color);
                ui.label(
                    RichText::new(tr(lang, "claude_st_idle"))
                        .size(12.5)
                        .color(color),
                );
            }
        }
    });
}

/// Offene Session: Rahmen in Statusfarbe, Klick holt das Terminal nach vorne.
fn show_open_card(
    ui: &mut Ui,
    o: &OpenSession,
    lang: Language,
    now_ms: i64,
    actions: &mut ClaudeSessionActions,
) {
    let stroke = egui::Stroke::new(1.5_f32, status_color(o.live.status));
    let tooltip = format!("{}\n{}", tr(lang, "claude_focus_tt"), o.live.cwd);
    let clicked = clickable_card(ui, open_card_id(o), stroke, &tooltip, |ui| {
        ui.vertical(|ui| {
            status_line(ui, o, lang, now_ms);
            ui.add(egui::Label::new(RichText::new(&o.title).size(13.0).strong()).truncate());
            if let Some(prompt) = &o.last_prompt {
                ui.add(egui::Label::new(RichText::new(prompt).size(11.5).weak()).truncate());
            }
            footer(ui, &o.place, o.branch.as_deref(), None);
        });
    });
    if clicked {
        actions.focus = Some(o.live.pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_live::LiveSession;
    use crate::claude_sessions::ClaudeSession;
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

    fn entry(id: &str, title: &str) -> SessionEntry {
        SessionEntry {
            session: session(id, title),
            repo_path: PathBuf::from("C:\\repo"),
            repo_name: "repo".to_string(),
            is_worktree: false,
        }
    }

    fn open(pid: u32, id: &str, title: &str, status: LiveStatus) -> OpenSession {
        OpenSession {
            live: LiveSession {
                pid,
                session_id: id.to_string(),
                cwd: format!("C:\\{id}"),
                status,
                status_since_ms: 0,
                name: None,
                proc_start: None,
            },
            title: title.to_string(),
            place: "repo".to_string(),
            branch: None,
            last_prompt: None,
        }
    }

    #[test]
    fn visible_lists_excludes_open_and_applies_search() {
        let open_list = [
            open(1, "a1", "Login fixen", LiveStatus::Busy),
            open(2, "x", "Doku", LiveStatus::Idle),
        ];
        let entries = [
            entry("a1", "Login fixen"),
            entry("b1", "Login Review"),
            entry("c1", "Refactoring"),
        ];

        let (o, rest) = visible_lists(&open_list, &entries, "");
        assert_eq!(o.len(), 2);
        let ids: Vec<&str> = rest.iter().map(|e| e.session.id.as_str()).collect();
        assert_eq!(ids, ["b1", "c1"], "offene Session nicht doppelt");

        let (o, rest) = visible_lists(&open_list, &entries, "login");
        assert_eq!(o.len(), 1);
        let ids: Vec<&str> = rest.iter().map(|e| e.session.id.as_str()).collect();
        assert_eq!(ids, ["b1"]);
    }

    /// Rendert `draw` mehrmals; `pointer` = Mauszeiger relativ zur Karte `id`.
    fn run_ui(
        id: egui::Id,
        pointer: impl Fn(egui::Rect) -> egui::Pos2,
        click: bool,
        mut draw: impl FnMut(&mut Ui, &mut ClaudeSessionActions),
    ) -> (egui::Context, ClaudeSessionActions) {
        let ctx = egui::Context::default();
        let mut actions = ClaudeSessionActions::default();
        let mut rect: Option<egui::Rect> = None;
        for frame in 0..4 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 400.0),
                )),
                ..egui::RawInput::default()
            };
            if let Some(pos) = rect.map(&pointer) {
                raw.events.push(egui::Event::PointerMoved(pos));
                if click && frame == 3 {
                    for pressed in [true, false] {
                        raw.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
            }
            let output = ctx.run_ui(raw, |ui| draw(ui, &mut actions));
            output.drop_without_applying_deltas();
            rect = ctx.read_response(id).map(|r| r.rect);
        }
        (ctx, actions)
    }

    fn on_button(r: egui::Rect) -> egui::Pos2 {
        egui::pos2(r.right() - 40.0, r.center().y)
    }

    fn on_body(r: egui::Rect) -> egui::Pos2 {
        egui::pos2(r.left() + 20.0, r.center().y)
    }

    #[test]
    fn resume_button_sits_above_card_click_area() {
        let e = entry("a1", "Titel");
        let draw = |ui: &mut Ui, a: &mut ClaudeSessionActions| {
            show_session_card(ui, &e, Language::De, 10, a);
        };
        let (ctx, _) = run_ui(card_id(&e), on_button, false, draw);
        let card = ctx.read_response(card_id(&e)).expect("Karte");
        assert!(!card.hovered(), "Karte verdeckt den Button");

        let (_, actions) = run_ui(card_id(&e), on_button, true, draw);
        assert_eq!(
            actions.resume,
            Some((e.session.cwd.clone(), e.session.id.clone()))
        );
    }

    #[test]
    fn click_on_card_body_resumes() {
        let e = entry("a1", "Titel");
        let (_, actions) = run_ui(card_id(&e), on_body, true, |ui, a| {
            show_session_card(ui, &e, Language::De, 10, a);
        });
        assert_eq!(
            actions.resume,
            Some((e.session.cwd.clone(), e.session.id.clone()))
        );
        assert_eq!(actions.focus, None);
    }

    #[test]
    fn click_on_card_title_resumes() {
        let e = entry("a1", "Ein recht langer Titel");
        // Titelzeile: oben links in der Karte
        let on_title = |r: egui::Rect| egui::pos2(r.left() + 30.0, r.top() + 18.0);
        let (_, actions) = run_ui(card_id(&e), on_title, true, |ui, a| {
            show_session_card(ui, &e, Language::De, 10, a);
        });
        assert_eq!(
            actions.resume,
            Some((e.session.cwd.clone(), e.session.id.clone()))
        );
    }

    #[test]
    fn click_on_open_card_focuses_terminal() {
        for status in [
            LiveStatus::Busy,
            LiveStatus::Shell,
            LiveStatus::Waiting,
            LiveStatus::Idle,
        ] {
            let o = open(4016, "a1", "Titel", status);
            let (_, actions) = run_ui(open_card_id(&o), on_body, true, |ui, a| {
                show_open_card(ui, &o, Language::De, 5_000, a);
            });
            assert_eq!(actions.focus, Some(4016), "{status:?}");
            assert_eq!(actions.resume, None);
        }
    }

    #[test]
    fn card_id_differs_for_same_session_in_two_folders() {
        let a = entry("x", "T");
        let mut b = a.clone();
        b.session.cwd = "C:\\anders".to_string();
        assert_ne!(card_id(&a), card_id(&b));
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
            "claude_open_section",
            "claude_all_section",
            "claude_st_waiting",
            "claude_st_idle",
            "claude_st_shell",
            "claude_focus_tt",
            "claude_focus_failed",
            "claude_waiting_tt",
        ] {
            for lang in [Language::De, Language::En] {
                assert_ne!(tr(lang, key), key, "{key} fehlt für {lang:?}");
            }
        }
    }
}
