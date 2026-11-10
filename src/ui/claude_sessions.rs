//! Ansicht „Claude Sessions“: oben die offenen Sessions mit Live-Status,
//! darunter alle übrigen in einer Liste, neueste zuerst.

use crate::claude_live::{open_matches_search, LiveStatus, OpenSession};
use crate::claude_preview::{PreviewItem, PreviewRole};
use crate::claude_sessions::{claude_projects_dir, session_matches_search, SessionEntry};
use crate::config::AppConfig;
use crate::gitlab::time::relative_time;
use crate::i18n::{tr, tr_fmt, Language};
use crate::ui::claude_anim::{
    brighten, format_elapsed, paint_check, paint_pulse_dot, paint_star, seed_of, shimmer_job,
    status_color, verb,
};
use crate::ui::search_field::search_field;
use egui::{FontId, RichText, Ui, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

const ICON_CLAUDE: egui::ImageSource = egui::include_image!("../../assets/icons/claude.svg");
const ICON_GIT_BRANCH: egui::ImageSource =
    egui::include_image!("../../assets/icons/git-branch.svg");
const ICON_TERMINAL: egui::ImageSource = egui::include_image!("../../assets/icons/terminal.svg");
const ICON_FOLDER: egui::ImageSource = egui::include_image!("../../assets/icons/folder.svg");
const ICON_CHEVRON_DOWN: egui::ImageSource =
    egui::include_image!("../../assets/icons/chevron-down.svg");
const ICON_CHEVRON_UP: egui::ImageSource =
    egui::include_image!("../../assets/icons/chevron-up.svg");

/// Was der Nutzer in der Ansicht ausgelöst hat.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ClaudeSessionActions {
    /// (Ordner, Session-ID) fortsetzen
    pub resume: Option<(String, String)>,
    /// Terminal der laufenden Instanz (PID) nach vorne holen
    pub focus: Option<u32>,
    /// (Repo, IDE-ID, zu öffnender Pfad) wie in der Repo-Liste
    pub ide_open: Option<(PathBuf, String, PathBuf)>,
    /// Shell im Ordner öffnen
    pub shell_open: Option<PathBuf>,
    /// Ordner im Explorer öffnen
    pub explorer_open: Option<PathBuf>,
    /// Vorschau der Session (ID) auf- bzw. zuklappen
    pub toggle_preview: Option<String>,
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
    /// Sprache, IDEs und Profile (Buttons der offenen Sessions)
    pub config: &'a AppConfig,
    /// Unix-Millisekunden (Laufzeiten)
    pub now_ms: i64,
    /// Aufgeklappte Vorschauen (Session-IDs) und ihre Einträge
    pub expanded: &'a HashSet<String>,
    pub previews: &'a HashMap<String, Vec<PreviewItem>>,
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
        config,
        now_ms,
        expanded,
        previews,
    } = *data;
    let lang = config.language;
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
                            let preview = Preview::of(&o.live.session_id, expanded, previews);
                            show_open_card(ui, o, config, now_ms, preview, actions);
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
                            let preview = Preview::of(&e.session.id, expanded, previews);
                            show_session_card(ui, e, lang, now_secs, preview, actions);
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
        let hint = tr(lang, "claude_search_hint");
        search_field(ui, "claude_sessions_search", &hint, search, lang);
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
/// `header` ist klickbar, `below` (die Vorschau) nicht: dort soll ein Klick
/// zum Lesen oder Markieren keine Session starten.
fn clickable_card(
    ui: &mut Ui,
    id: egui::Id,
    stroke: egui::Stroke,
    tooltip: &str,
    header: impl FnOnce(&mut Ui),
    below: impl FnOnce(&mut Ui),
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
        // Alle Karten gleich breit: so breit wie das Fenster erlaubt
        ui.set_min_width(ui.available_width());
        // Markierbare Labels würden den Klick auf die Karte abfangen
        let selectable = ui.style().interaction.selectable_labels;
        ui.style_mut().interaction.selectable_labels = false;
        let header_bottom = ui.scope(header).response.rect.max.y;
        ui.style_mut().interaction.selectable_labels = selectable;
        let before = ui.min_rect().max.y;
        below(ui);
        (ui.min_rect().max.y > before).then_some(header_bottom)
    });
    // Mit Vorschau endet die Klickfläche unter dem Kopf der Karte
    let mut click_rect = card.response.rect;
    if let Some(bottom) = card.inner {
        click_rect.max.y = click_rect.max.y.min(bottom + 4.0);
    }
    ui.ctx().data_mut(|d| d.insert_temp(rect_key, click_rect));

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

/// Vorschau-Zustand einer Karte.
#[derive(Clone, Copy)]
enum Preview<'a> {
    Collapsed,
    /// Aufgeklappt; `None` = wird noch geladen
    Expanded(Option<&'a [PreviewItem]>),
}

impl<'a> Preview<'a> {
    fn of(
        id: &str,
        expanded: &HashSet<String>,
        previews: &'a HashMap<String, Vec<PreviewItem>>,
    ) -> Self {
        if expanded.contains(id) {
            Preview::Expanded(previews.get(id).map(Vec::as_slice))
        } else {
            Preview::Collapsed
        }
    }
}

/// Auf-/Zuklapp-Button der Vorschau; setzt `actions.toggle_preview`.
fn preview_toggle(
    ui: &mut Ui,
    id: &str,
    preview: Preview,
    lang: Language,
    actions: &mut ClaudeSessionActions,
) {
    let (icon, tip) = match preview {
        Preview::Collapsed => (ICON_CHEVRON_DOWN, "claude_preview_show"),
        Preview::Expanded(_) => (ICON_CHEVRON_UP, "claude_preview_hide"),
    };
    let image = egui::Image::new(icon)
        .fit_to_exact_size(Vec2::splat(16.0))
        .tint(ui.visuals().text_color());
    let tip = tr(lang, tip);
    let resp = ui.add(egui::Button::image(image)).on_hover_text(&tip);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &tip));
    hover_frame(ui, &resp);
    if resp.clicked() {
        actions.toggle_preview = Some(id.to_string());
    }
}

/// Letzte Nachrichten unter dem Karteninhalt.
fn show_preview(ui: &mut Ui, preview: Preview, lang: Language) {
    let Preview::Expanded(items) = preview else {
        return;
    };
    ui.add_space(6.0);
    ui.separator();
    let Some(items) = items else {
        ui.spinner();
        return;
    };
    if items.is_empty() {
        ui.label(
            RichText::new(tr(lang, "claude_preview_empty"))
                .size(11.5)
                .weak(),
        );
        return;
    }
    let you = tr(lang, "claude_preview_you");
    let claude_color = crate::ui::claude_anim::COLOR_BUSY;
    for item in items {
        ui.horizontal_top(|ui| {
            let (who, color) = match item.role {
                PreviewRole::User => (you.as_str(), ui.visuals().hyperlink_color),
                PreviewRole::Assistant => ("Claude", claude_color),
                PreviewRole::Tool => ("", ui.visuals().weak_text_color()),
            };
            let label_width = 48.0;
            ui.allocate_ui_with_layout(
                Vec2::new(label_width, 0.0),
                egui::Layout::left_to_right(egui::Align::Min),
                |ui| {
                    ui.set_min_width(label_width);
                    ui.label(RichText::new(who).size(11.5).strong().color(color));
                },
            );
            let text = match item.role {
                PreviewRole::Tool => RichText::new(format!("→ {}", item.text))
                    .size(11.0)
                    .monospace()
                    .weak(),
                _ => RichText::new(&item.text).size(11.5),
            };
            ui.add(egui::Label::new(text).wrap());
        });
        ui.add_space(2.0);
    }
}

/// Session-Karte; Klick auf Karte oder „Fortsetzen“ setzt `actions.resume`.
fn show_session_card(
    ui: &mut Ui,
    entry: &SessionEntry,
    lang: Language,
    now_secs: i64,
    preview: Preview,
    actions: &mut ClaudeSessionActions,
) {
    let session = &entry.session;
    let stroke = egui::Stroke::new(1.0_f32, ui.visuals().widgets.inactive.fg_stroke.color);
    let tooltip = format!("{}\n{}", session.cwd, session.id);
    let mut button_clicked = false;
    let card_clicked = clickable_card(
        ui,
        card_id(entry),
        stroke,
        &tooltip,
        |ui| {
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
                    preview_toggle(ui, &session.id, preview, lang, actions);
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&session.title).size(13.0).strong())
                                .truncate(),
                        );
                        if let Some(prompt) = &session.last_prompt {
                            ui.add(
                                egui::Label::new(RichText::new(prompt).size(11.5).weak())
                                    .truncate(),
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
        },
        |ui| show_preview(ui, preview, lang),
    );
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

/// Icon-Button wie in der Repo-Liste (Rahmen beim Überfahren).
fn icon_button(ui: &mut Ui, image: egui::Image<'static>, tooltip: &str) -> bool {
    let resp = ui.add(egui::Button::image(image)).on_hover_text(tooltip);
    hover_frame(ui, &resp);
    resp.clicked()
}

/// Rahmen um einen Icon-Button beim Überfahren.
fn hover_frame(ui: &Ui, resp: &egui::Response) {
    if resp.hovered() {
        ui.painter().rect_stroke(
            resp.rect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 100, 100)),
            egui::StrokeKind::Outside,
        );
    }
}

/// IDE-, Shell- und Explorer-Buttons (rechts, Reihenfolge wie in der
/// Repo-Liste) nach dem Profil des Repos bzw. Ordners.
fn open_buttons(
    ui: &mut Ui,
    o: &OpenSession,
    config: &AppConfig,
    actions: &mut ClaudeSessionActions,
) {
    let lang = config.language;
    let cwd = PathBuf::from(&o.live.cwd);
    let repo = o.repo_path.clone().unwrap_or_else(|| cwd.clone());
    let profile = config.get_effective_profile_for_repo(&repo);
    let tint = ui.visuals().text_color();
    let icon = |src: egui::ImageSource<'static>| {
        egui::Image::new(src)
            .fit_to_exact_size(Vec2::splat(18.0))
            .tint(tint)
    };
    if profile.show_explorer && icon_button(ui, icon(ICON_FOLDER), &tr(lang, "open_in_explorer")) {
        actions.explorer_open = Some(cwd.clone());
    }
    if profile.show_shell && icon_button(ui, icon(ICON_TERMINAL), &tr(lang, "open_in_shell")) {
        actions.shell_open = Some(cwd.clone());
    }
    for ide in profile.visible_ides().iter().take(4).rev() {
        let tooltip = format!("{} {}", tr(lang, "open_in"), ide.display_name);
        if icon_button(ui, crate::ui::repo_list::ide_image(ide), &tooltip) {
            actions.ide_open = Some((repo.clone(), ide.id.clone(), repo.clone()));
        }
    }
}

/// Offene Session: Rahmen in Statusfarbe, Klick holt das Terminal nach vorne;
/// rechts oben IDE, Shell und Explorer.
fn show_open_card(
    ui: &mut Ui,
    o: &OpenSession,
    config: &AppConfig,
    now_ms: i64,
    preview: Preview,
    actions: &mut ClaudeSessionActions,
) {
    let lang = config.language;
    let stroke = egui::Stroke::new(1.5_f32, status_color(o.live.status));
    let tooltip = format!("{}\n{}", tr(lang, "claude_focus_tt"), o.live.cwd);
    let clicked = clickable_card(
        ui,
        open_card_id(o),
        stroke,
        &tooltip,
        |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    open_buttons(ui, o, config, actions);
                    preview_toggle(ui, &o.live.session_id, preview, lang, actions);
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        status_line(ui, o, lang, now_ms);
                        ui.add(
                            egui::Label::new(RichText::new(&o.title).size(13.0).strong())
                                .truncate(),
                        );
                        if let Some(prompt) = &o.last_prompt {
                            ui.add(
                                egui::Label::new(RichText::new(prompt).size(11.5).weak())
                                    .truncate(),
                            );
                        }
                        footer(ui, &o.place, o.branch.as_deref(), None);
                    });
                });
            });
        },
        |ui| show_preview(ui, preview, lang),
    );
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
            repo_path: None,
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
        // Echte Icon-Größen (SVG), sonst sind Icon-Buttons zu schmal
        egui_extras::install_image_loaders(&ctx);
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
            show_session_card(ui, &e, Language::De, 10, Preview::Collapsed, a);
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
            show_session_card(ui, &e, Language::De, 10, Preview::Collapsed, a);
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
            show_session_card(ui, &e, Language::De, 10, Preview::Collapsed, a);
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
                show_open_card(
                    ui,
                    &o,
                    &crate::config::AppConfig::default(),
                    5_000,
                    Preview::Collapsed,
                    a,
                );
            });
            assert_eq!(actions.focus, Some(4016), "{status:?}");
            assert_eq!(actions.resume, None);
        }
    }

    #[test]
    fn open_cards_have_equal_full_width() {
        let short = open(1, "a", "Kurz", LiveStatus::Idle);
        let mut long = open(2, "b", "Lang", LiveStatus::Busy);
        long.last_prompt = Some("x ".repeat(30));
        let config = crate::config::AppConfig::default();
        let (ctx, _) = run_ui(open_card_id(&short), on_body, false, |ui, a| {
            show_open_card(ui, &short, &config, 0, Preview::Collapsed, a);
            show_open_card(ui, &long, &config, 0, Preview::Collapsed, a);
        });
        let w = |o: &OpenSession| ctx.read_response(open_card_id(o)).unwrap().rect.width();
        assert_eq!(w(&short), w(&long));
        assert!(w(&short) > 750.0, "volle Breite: {}", w(&short));
    }

    #[test]
    fn explorer_button_on_open_card_does_not_focus() {
        let o = open(7, "a1", "Titel", LiveStatus::Busy);
        let config = crate::config::AppConfig::default();
        // Explorer-Button: ganz rechts in der ersten Zeile
        let on_explorer = |r: egui::Rect| egui::pos2(r.right() - 25.0, r.top() + 23.0);
        let (_, actions) = run_ui(open_card_id(&o), on_explorer, true, |ui, a| {
            show_open_card(ui, &o, &config, 0, Preview::Collapsed, a);
        });
        assert_eq!(actions.explorer_open, Some(PathBuf::from("C:\\a1")));
        assert_eq!(actions.focus, None);
    }

    #[test]
    fn card_id_differs_for_same_session_in_two_folders() {
        let a = entry("x", "T");
        let mut b = a.clone();
        b.session.cwd = "C:\\anders".to_string();
        assert_ne!(card_id(&a), card_id(&b));
    }

    /// Knoten-Mitte per AccessKit-Label (Teilstring).
    fn find_label(output: &egui::FullOutput, needle: &str) -> Option<egui::Pos2> {
        let update = output.platform_output.accesskit_update.as_ref()?;
        update
            .nodes
            .iter()
            .find(|(_, n)| n.label().or(n.value()).is_some_and(|l| l.contains(needle)))
            .and_then(|(_, n)| n.bounds())
            .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
    }

    #[test]
    fn preview_toggle_click_toggles_without_resuming() {
        let e = entry("a1", "Titel");
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        egui_extras::install_image_loaders(&ctx);
        let mut target = None;
        let mut actions = ClaudeSessionActions::default();
        for frame in 0..4 {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 400.0),
                )),
                ..egui::RawInput::default()
            };
            if let (Some(pos), 3) = (target, frame) {
                raw.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true, false] {
                    raw.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            let output = ctx.run_ui(raw, |ui| {
                show_session_card(ui, &e, Language::De, 10, Preview::Collapsed, &mut actions);
            });
            target = find_label(&output, "Letzte Nachrichten zeigen");
            output.drop_without_applying_deltas();
        }
        assert_eq!(actions.toggle_preview.as_deref(), Some("a1"));
        assert_eq!(actions.resume, None);
    }

    #[test]
    fn expanded_preview_shows_messages_and_spinner_while_loading() {
        let o = open(3, "a1", "Titel", LiveStatus::Idle);
        let config = crate::config::AppConfig::default();
        let items = vec![
            PreviewItem {
                role: PreviewRole::User,
                text: "Fix den Login".into(),
            },
            PreviewItem {
                role: PreviewRole::Tool,
                text: "Edit src/login.rs".into(),
            },
        ];
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut a = ClaudeSessionActions::default();
        // (gefunden, Höhe der gezeichneten Karte, Höhe der Klickfläche)
        let mut height = |preview: Preview| {
            let mut card = 0.0;
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                card = ui
                    .scope(|ui| show_open_card(ui, &o, &config, 0, preview, &mut a))
                    .response
                    .rect
                    .height();
            });
            let found = (
                find_label(&output, "Fix den Login").is_some(),
                find_label(&output, "→ Edit src/login.rs").is_some(),
            );
            output.drop_without_applying_deltas();
            let click = ctx
                .data(|d| d.get_temp::<egui::Rect>(open_card_id(&o).with("rect")))
                .map_or(0.0, |r| r.height());
            (found, card, click)
        };
        height(Preview::Collapsed);
        let (found, collapsed, click_collapsed) = height(Preview::Collapsed);
        assert_eq!(found, (false, false));
        height(Preview::Expanded(Some(&items)));
        let (found, expanded, click_expanded) = height(Preview::Expanded(Some(&items)));
        assert_eq!(found, (true, true));
        assert!(expanded > collapsed + 20.0, "{collapsed} → {expanded}");
        assert!(
            click_expanded <= click_collapsed + 4.0,
            "Klick in die Vorschau zählt nicht als Kartenklick: \
             {click_collapsed} → {click_expanded}"
        );
        height(Preview::Expanded(None));
        let (_, loading, _) = height(Preview::Expanded(None));
        assert!(loading > collapsed, "Spinner braucht Platz");
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
