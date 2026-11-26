//! Status-Chips, Commit-Zeile und `⋯`-Menü der Repo-Karte. Ausgelagert aus
//! `repo_list.rs`; Stil wie die Chips der MR-Ansicht (abgerundet, Icon +
//! kurzer Text, Tooltip).

use egui::containers::menu::MenuButton;
use egui::{
    vec2, Align, Atoms, Button, Color32, CursorIcon, FontId, Image, ImageSource, Label, Layout,
    Response, RichText, Sense, Stroke, Ui, Vec2, WidgetInfo, WidgetType,
};

use crate::claude_live::LiveStatus;
use crate::config::{AppConfig, HostAlias, IdeConfig, LanguageProfile};
use crate::git::{InProgressKind, RepoInfo, RepoState};
use crate::gitlab::badge::MrBadge;
use crate::gitlab::model::PipelineState;
use crate::gitlab::remote::RemoteScheme;
use crate::gitlab::time::relative_time;
use crate::i18n::{tr, tr_fmt, Language};
use crate::ui::merge_requests::pipeline_icon;
use crate::ui::repo_list::{agent_image, ide_image, RepoListActions, RepoListContext};
use crate::ui::theme::{mr_palette, MrPalette};

use crate::ui::icons::mr::{
    ICON_CHECK, ICON_COPY, ICON_DOWNLOAD as ICON_PULL, ICON_FOLDER_PLUS,
    ICON_OPEN_EXTERNAL as ICON_BROWSER, ICON_PULL_REQUEST as ICON_MR, ICON_SPARKLE, ICON_WARNING,
};
use crate::ui::icons::{
    ICON_ARROW_DOWN, ICON_ARROW_UP, ICON_DOTS, ICON_FOLDER, ICON_STAR, ICON_STAR_FILL,
    ICON_TERMINAL, ICON_TRASH,
};

/// Neue i18n-Schlüssel der Karte (Test: beide Sprachen vorhanden).
#[cfg(test)]
const CARD_KEYS: &[&str] = &[
    "repo_state_clean",
    "repo_state_dirty",
    "repo_state_conflict",
    "repo_state_merge",
    "repo_state_rebase",
    "repo_state_cherry_pick",
    "repo_state_revert",
    "repo_state_other",
    "repo_state_conflict_tt",
    "repo_state_in_progress_tt",
    "repo_ahead_tt",
    "repo_behind_tt",
    "repo_mr_chip_tt",
    "repo_mr_draft",
    "repo_claude_waiting",
    "repo_claude_working",
    "repo_claude_tt",
    "repo_fav_add",
    "repo_fav_remove",
    "repo_more",
    "repo_menu_open_in",
    "repo_menu_profile",
    "repo_menu_new_worktree",
    "repo_menu_copy_path",
    "repo_menu_copy_branch",
    "repo_menu_open_browser",
    "repo_menu_remove_worktree",
    "repo_cleanup_button",
    "repo_cleanup_tooltip",
    "repo_commit_subject",
];

// --- Reine Text-/Farb-Funktionen ------------------------------------------

/// Punkt im „geändert“-Chip; muss in Ubuntu-Light liegen (egui-Standard).
pub const DIRTY_DOT: &str = "•";

/// Text und Farbe des Status-Chips (immer sichtbar). Farben aus der
/// Theme-Palette (≥ 3:1 je Theme): sauber grün, geändert Bernstein,
/// Konflikt/laufende Operation rot – auch auf Dunkel klar unterscheidbar.
pub fn state_chip(
    lang: Language,
    state: RepoState,
    dirty_files: usize,
    p: &MrPalette,
) -> (String, Color32) {
    match state {
        RepoState::Clean => (tr(lang, "repo_state_clean"), p.ok_icon),
        RepoState::Dirty => (
            tr_fmt(lang, "repo_state_dirty", &[&dirty_files.max(1).to_string()]),
            p.warn_icon,
        ),
        RepoState::Conflict => (tr(lang, "repo_state_conflict"), p.fail_icon),
        RepoState::InProgress(kind) => {
            let key = match kind {
                InProgressKind::Merge => "repo_state_merge",
                InProgressKind::Rebase => "repo_state_rebase",
                InProgressKind::CherryPick => "repo_state_cherry_pick",
                InProgressKind::Revert => "repo_state_revert",
                InProgressKind::Other => "repo_state_other",
            };
            (tr(lang, key), p.fail_icon)
        }
    }
}

/// Tooltip des Status-Chips.
fn state_tooltip(lang: Language, state: RepoState) -> String {
    let key = match state {
        RepoState::Clean => "clean_tooltip",
        RepoState::Dirty => "dirty_tooltip",
        RepoState::Conflict => "repo_state_conflict_tt",
        RepoState::InProgress(_) => "repo_state_in_progress_tt",
    };
    tr(lang, key)
}

/// Tooltip (und AccessKit-Name) des ↓-Chips.
pub fn behind_tooltip(lang: Language, behind: usize) -> String {
    tr_fmt(lang, "repo_behind_tt", &[&behind.to_string()])
}

/// Sichtbare IDE der Karte: gemerkte Wahl, falls sichtbar, sonst die
/// Default-IDE des Profils, sonst die erste sichtbare.
pub fn card_ide<'a>(profile: &'a LanguageProfile, selected: Option<&str>) -> Option<&'a IdeConfig> {
    let visible = profile.visible_ides();
    let find = |id: Option<&str>| id.and_then(|id| visible.iter().copied().find(|i| i.id == id));
    find(selected)
        .or_else(|| find(profile.default_ide_id.as_deref()))
        .or_else(|| visible.first().copied())
}

/// IDEs als Icons auf der Karte, höchstens `max` (Platz): die der Karte
/// ([`card_ide`]) immer, dann die übrigen sichtbaren; in Profil-Reihenfolge.
pub fn card_ides<'a>(
    profile: &'a LanguageProfile,
    selected: Option<&str>,
    max: usize,
) -> Vec<&'a IdeConfig> {
    let visible = profile.visible_ides();
    let first = card_ide(profile, selected).map(|i| i.id.as_str());
    let mut picked: Vec<&str> = first.into_iter().collect();
    picked.extend(
        visible
            .iter()
            .map(|i| i.id.as_str())
            .filter(|id| Some(*id) != first),
    );
    picked.truncate(max.max(1));
    visible
        .into_iter()
        .filter(|i| picked.contains(&i.id.as_str()))
        .collect()
}

/// Mindestbreite für den letzten Commit, bevor IDEs ins `⋯`-Menü wandern.
const MIN_COMMIT_WIDTH: f32 = 200.0;

/// Web-Adresse des Remotes (`origin`, sonst des ersten) als
/// `https://host/pfad`; SSH-Hosts laufen über `host_aliases`, der SSH-Port
/// fällt weg. `None` bei lokalen Pfaden und unbekannten Schemata.
pub fn remote_web_url(repo: &RepoInfo, aliases: &[HostAlias]) -> Option<String> {
    let remote = repo
        .remotes
        .iter()
        .find(|r| r.name == "origin")
        .or_else(|| repo.remotes.first())?;
    let parsed = remote.parsed.as_ref()?;
    let host = aliases
        .iter()
        .find(|a| a.from.trim().eq_ignore_ascii_case(&parsed.host))
        .map_or(parsed.host.clone(), |a| a.to.trim().to_lowercase());
    let (scheme, port) = match parsed.scheme {
        RemoteScheme::Ssh => ("https", None),
        RemoteScheme::Https => ("https", parsed.port),
        RemoteScheme::Http => ("http", parsed.port),
    };
    // Alias-Ziel kann selbst schon einen Port tragen (`host:port`)
    let authority = match port {
        Some(port) if !host.contains(':') => format!("{host}:{port}"),
        _ => host,
    };
    Some(format!("{scheme}://{authority}/{}", parsed.full_path))
}

/// Kurztext des MR-Chips: `!iid`, Approvals `n/m` bzw. `n`.
pub fn mr_chip_text(badge: &MrBadge) -> String {
    let iid = format!("!{}", badge.iid);
    match badge.required {
        Some(required) => format!("{iid} · {}/{required}", badge.approved),
        None if badge.approved > 0 => format!("{iid} · {}", badge.approved),
        None => iid,
    }
}

/// Text des Claude-Chips; `None` = kein Chip (Session wartet auf Prompt).
pub fn claude_chip_text(lang: Language, status: LiveStatus) -> Option<String> {
    match status {
        LiveStatus::Waiting => Some(tr(lang, "repo_claude_waiting")),
        LiveStatus::Busy | LiveStatus::Shell => Some(tr(lang, "repo_claude_working")),
        LiveStatus::Idle => None,
    }
}

// --- Zeichnen ---------------------------------------------------------------

fn icon(source: ImageSource<'static>, size: f32, tint: Color32) -> Image<'static> {
    Image::new(source)
        .fit_to_exact_size(Vec2::splat(size))
        .tint(tint)
}

/// Chip (h 20, r 10, Text 11 px, Icon 12 px) mit getönter Fläche in der
/// Akzentfarbe; `label` ist der AccessKit-Name.
fn chip(
    ui: &mut Ui,
    atoms: Atoms<'_>,
    accent: Color32,
    label: &str,
    tooltip: &str,
    clickable: bool,
) -> Response {
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let button = Button::new(atoms)
        .fill(accent.gamma_multiply(0.14))
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.6)))
        .corner_radius(10)
        .min_size(vec2(0.0, 20.0))
        .gap(4.0)
        .sense(sense);
    let resp = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = vec2(6.0, 2.0);
            ui.add(button)
        })
        .inner;
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    let resp = if clickable {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    };
    resp.on_hover_text(tooltip)
}

fn chip_text(text: impl Into<String>, color: Color32) -> RichText {
    RichText::new(text.into()).size(11.0).color(color)
}

/// Zeile 1 ganz links: Stern (☆/★), Klick schaltet den Favoriten.
pub(crate) fn favorite_star(
    ui: &mut Ui,
    repo: &RepoInfo,
    config: &AppConfig,
    actions: &mut RepoListActions,
) {
    let lang = config.language;
    let fav = config.is_favorite_repo(&repo.path);
    let p = mr_palette(&config.theme, ui.visuals());
    let (source, tint, key) = if fav {
        (ICON_STAR_FILL, p.warn_icon, "repo_fav_remove")
    } else {
        (ICON_STAR, ui.visuals().weak_text_color(), "repo_fav_add")
    };
    let label = tr(lang, key);
    let resp = ui
        .add(Button::image(icon(source, 16.0, tint)).frame(false))
        .on_hover_cursor(CursorIcon::PointingHand);
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &label));
    if resp.on_hover_text(&label).clicked() {
        actions.toggle_favorite = Some(repo.path.clone());
    }
}

/// Zeile 2: Status, ↑/↓, MR und Claude als Chips.
pub(crate) fn show_chip_row(
    ui: &mut Ui,
    repo: &RepoInfo,
    config: &AppConfig,
    cx: &RepoListContext,
    actions: &mut RepoListActions,
) {
    let lang = config.language;
    let p = mr_palette(&config.theme, ui.visuals());
    let text = ui.visuals().text_color();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);

        // Status (immer sichtbar)
        let (label, color) = state_chip(lang, repo.state, repo.dirty_files, &p);
        let mut atoms = Atoms::new(());
        match repo.state {
            RepoState::Clean => atoms.push_right(icon(ICON_CHECK, 12.0, color)),
            // „•“ statt „●“: ● zeichnet die egui-Standardschrift als Quadrat
            RepoState::Dirty => atoms.push_right(RichText::new(DIRTY_DOT).size(13.0).color(color)),
            RepoState::Conflict | RepoState::InProgress(_) => {
                atoms.push_right(icon(ICON_WARNING, 12.0, color))
            }
        }
        atoms.push_right(chip_text(&label, text));
        chip(
            ui,
            atoms,
            color,
            &label,
            &state_tooltip(lang, repo.state),
            false,
        );

        // ↑ ungepushte Commits
        if repo.ahead > 0 {
            let n = repo.ahead.to_string();
            let tooltip = tr_fmt(lang, "repo_ahead_tt", &[&n]);
            let accent = ui.visuals().selection.stroke.color;
            let mut atoms = Atoms::new(());
            atoms.push_right(icon(ICON_ARROW_UP, 12.0, accent));
            atoms.push_right(chip_text(n, text));
            chip(ui, atoms, accent, &tooltip, &tooltip, false);
        }

        // ↓ Rückstand, Klick: Fast-Forward (während des Pulls Spinner)
        if repo.behind > 0 {
            let tooltip = behind_tooltip(lang, repo.behind);
            let pulling = cx.pulling.is_some_and(|path| path == repo.path);
            let mut atoms = Atoms::new(());
            atoms.push_right(icon(ICON_ARROW_DOWN, 12.0, p.warn_icon));
            atoms.push_right(chip_text(repo.behind.to_string(), text));
            if !pulling {
                atoms.push_right(icon(ICON_PULL, 12.0, p.warn_icon));
            }
            let resp = chip(ui, atoms, p.warn_icon, &tooltip, &tooltip, !pulling);
            if pulling {
                ui.spinner();
            } else if resp.clicked() {
                actions.pull = Some(repo.path.clone());
            }
        }

        // Offener MR auf diesem Ordner
        if let Some(badge) = cx.mrs.get(&repo.path) {
            mr_chip(ui, lang, &p, badge, actions);
        }

        // Laufende Claude Session in diesem Ordner
        if let Some(status) = cx.claude.get(&repo.path) {
            if let Some(label) = claude_chip_text(lang, *status) {
                let accent = if *status == LiveStatus::Waiting {
                    p.warn_icon
                } else {
                    p.run_icon
                };
                let mut atoms = Atoms::new(());
                atoms.push_right(icon(ICON_SPARKLE, 12.0, accent));
                atoms.push_right(chip_text(&label, text));
                let tooltip = tr(lang, "repo_claude_tt");
                if chip(ui, atoms, accent, &label, &tooltip, true).clicked() {
                    actions.focus_claude = Some(repo.path.clone());
                }
            }
        }
    });
}

/// MR-Chip: `!iid · n/m ✓ · Pipeline`; Entwurf grau, Klick springt in die
/// MR-Ansicht.
fn mr_chip(
    ui: &mut Ui,
    lang: Language,
    p: &MrPalette,
    badge: &MrBadge,
    actions: &mut RepoListActions,
) {
    let weak = ui.visuals().weak_text_color();
    let (accent, text) = if badge.draft {
        (weak, weak)
    } else {
        (
            ui.visuals().selection.stroke.color,
            ui.visuals().text_color(),
        )
    };
    let mut atoms = Atoms::new(());
    atoms.push_right(icon(ICON_MR, 12.0, accent));
    atoms.push_right(chip_text(format!("!{}", badge.iid), text));
    let approvals = match badge.required {
        Some(required) => Some(format!("{}/{required}", badge.approved)),
        None => (badge.approved > 0).then(|| badge.approved.to_string()),
    };
    if let Some(approvals) = approvals {
        atoms.push_right(chip_text(format!("· {approvals}"), text));
        let done = badge.required.is_none_or(|r| badge.approved >= r);
        let tint = if done { p.ok_icon } else { weak };
        atoms.push_right(icon(ICON_CHECK, 12.0, tint));
    }
    if badge.pipeline != PipelineState::NoPipeline {
        let (source, color) = pipeline_icon(badge.pipeline, p);
        atoms.push_right(chip_text("·", text));
        atoms.push_right(icon(source, 12.0, if badge.draft { weak } else { color }));
    }
    let mut title = badge.title.clone();
    if badge.draft {
        title = format!("{} – {title}", tr(lang, "repo_mr_draft"));
    }
    let tooltip = tr_fmt(
        lang,
        "repo_mr_chip_tt",
        &[&title, &tr(lang, badge.pipeline.label_key())],
    );
    if chip(ui, atoms, accent, &mr_chip_text(badge), &tooltip, true).clicked() {
        actions.open_mr = Some((badge.host_key.clone(), badge.id.clone()));
    }
}

/// Rahmen um Icon-Buttons beim Überfahren (Theme-Farbe).
fn hover_outline(ui: &Ui, resp: &Response) {
    if resp.hovered() {
        ui.painter().rect_stroke(
            resp.rect,
            egui::CornerRadius::same(4),
            Stroke::new(1.5, ui.visuals().widgets.hovered.fg_stroke.color),
            egui::StrokeKind::Outside,
        );
    }
}

/// Icon-Button 18 px mit Tooltip als AccessKit-Name.
fn tool_button(ui: &mut Ui, image: Image<'static>, tooltip: &str) -> bool {
    let resp = ui.add(Button::image(image));
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, tooltip));
    hover_outline(ui, &resp);
    resp.on_hover_text(tooltip).clicked()
}

/// Zeile 3: letzter Commit links; rechts Explorer, Shell, die IDEs (so
/// viele Platz haben), Agents und `⋯`.
pub(crate) fn show_bottom_row(
    ui: &mut Ui,
    repo: &RepoInfo,
    config: &AppConfig,
    cx: &RepoListContext,
    actions: &mut RepoListActions,
) {
    let lang = config.language;
    let profile = config.get_effective_profile_for_repo(&repo.path);
    let selected = config
        .get_repo_state(&repo.path)
        .and_then(|s| s.selected_ide.as_deref());
    let file = repo
        .selected_solution
        .clone()
        .unwrap_or_else(|| repo.path.clone());
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // AI-Agents (Profil: ausgeblendet/Reihenfolge); ohne aktive
            // Agents der erste nicht ausgeblendete
            let agents = {
                let filtered = profile.filtered_agents(&config.agents, &config.active_agent_ids);
                if filtered.is_empty() && config.active_agent_ids.is_empty() {
                    config
                        .agents
                        .iter()
                        .filter(|a| !profile.hidden_agent_ids.contains(&a.id))
                        .take(1)
                        .collect::<Vec<_>>()
                } else {
                    filtered
                }
            };
            // IDEs nebeneinander, soweit Platz bleibt; der Rest im `⋯`-Menü
            let slot = 18.0 + 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x;
            let fixed = 1
                + agents.len()
                + usize::from(profile.show_shell)
                + usize::from(profile.show_explorer);
            let free = ui.available_width() - fixed as f32 * slot - MIN_COMMIT_WIDTH;
            let max_ides = (free / slot).floor().max(1.0) as usize;
            let ides = card_ides(profile, selected, max_ides);
            let shown: Vec<&str> = ides.iter().map(|i| i.id.as_str()).collect();
            more_menu(ui, repo, config, &shown, actions);

            for agent in agents.iter().rev() {
                let tooltip = tr(lang, "open_in_terminal").replace("{}", &agent.display_name);
                if tool_button(ui, agent_image(agent), &tooltip) {
                    actions.agent_open = Some((repo.path.clone(), agent.id.clone()));
                }
            }

            // Ohne IDEs im Profil VS Code als Text-Button
            for ide in ides.iter().rev() {
                let tooltip = format!("{} {}", tr(lang, "open_in"), ide.display_name);
                let resp = ui.add(Button::image(ide_image(ide)));
                resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &tooltip));
                hover_outline(ui, &resp);
                if resp
                    .on_hover_text(format!(
                        "{} {}",
                        tr(lang, "open_in"),
                        ide_preview(ide, repo)
                    ))
                    .clicked()
                {
                    actions.ide_open = Some((repo.path.clone(), ide.id.clone(), file.clone()));
                }
            }
            if ides.is_empty() && profile.ides.is_empty() {
                let btn = Button::new(RichText::new("VS Code").size(11.0)).corner_radius(6);
                if ui.add(btn).clicked() {
                    actions.ide_open =
                        Some((repo.path.clone(), "vscode".to_string(), file.clone()));
                }
            }

            if profile.show_shell {
                let image = icon(ICON_TERMINAL, 18.0, ui.visuals().text_color());
                if tool_button(ui, image, &tr(lang, "open_in_shell")) {
                    actions.shell_open = Some(repo.path.clone());
                }
            }
            if profile.show_explorer {
                let image = icon(ICON_FOLDER, 18.0, ui.visuals().text_color());
                if tool_button(ui, image, &tr(lang, "open_in_explorer")) {
                    actions.explorer_open = Some(repo.path.clone());
                }
            }

            // Restbreite links: letzter Commit
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                last_commit_line(ui, repo, config, cx.now);
            });
        });
    });
}

/// „Betreff“ · Autor · vor 2 Std.; der Betreff wird auf die Restbreite
/// gekürzt (…), Autor und Zeit bleiben stehen.
fn last_commit_line(ui: &mut Ui, repo: &RepoInfo, config: &AppConfig, now: i64) {
    let Some(commit) = &repo.last_commit else {
        return;
    };
    let lang = config.language;
    let color = mr_palette(&config.theme, ui.visuals()).text2;
    let meta = format!(
        " · {} · {}",
        commit.author,
        relative_time(commit.time, now, lang)
    );
    let meta_width = ui
        .painter()
        .layout_no_wrap(meta.clone(), FontId::proportional(11.0), color)
        .size()
        .x;
    ui.spacing_mut().item_spacing.x = 0.0;
    let max = (ui.available_width() - meta_width - 4.0).max(24.0);
    let subject = tr_fmt(lang, "repo_commit_subject", &[&commit.summary]);
    ui.scope(|ui| {
        ui.set_max_width(max);
        ui.add(Label::new(RichText::new(subject).size(11.0).color(color)).truncate())
            .on_hover_text(&commit.summary);
    });
    ui.label(RichText::new(meta).size(11.0).color(color));
}

/// Aufruf-Vorschau der IDE mit eingesetzter Solution (Tooltip).
fn ide_preview(ide: &IdeConfig, repo: &RepoInfo) -> String {
    if ide.no_args {
        return format!("{} (cwd: {})", ide.display_name, repo.path.display());
    }
    let args = ide.effective_args();
    let sln = repo.selected_solution.as_ref().unwrap_or(&repo.path);
    let substituted: Vec<String> = args
        .iter()
        .map(|a| crate::git::substitute_placeholders(a, sln, &repo.path))
        .collect();
    let shown = if substituted.is_empty() {
        args.join(" ")
    } else {
        substituted.join(" ")
    };
    format!(
        "{} {} (cwd: {})",
        ide.display_name,
        shown,
        repo.path.display()
    )
}

/// Menüeintrag mit Icon; schließt das Menü beim Klick.
fn menu_item(ui: &mut Ui, source: ImageSource<'static>, text: &str) -> bool {
    let tint = ui.visuals().text_color();
    ui.add(Button::image_and_text(icon(source, 14.0, tint), text))
        .clicked()
}

/// `⋯`-Menü: IDEs, die nicht auf der Karte (`on_card`) passen, Profil, Worktree, Kopieren, Browser, Entfernen.
fn more_menu(
    ui: &mut Ui,
    repo: &RepoInfo,
    config: &AppConfig,
    on_card: &[&str],
    actions: &mut RepoListActions,
) {
    let lang = config.language;
    let label = tr(lang, "repo_more");
    let button = Button::image(icon(ICON_DOTS, 18.0, ui.visuals().text_color()));
    let (resp, _) = MenuButton::from_button(button).ui(ui, |ui| {
        ui.set_min_width(200.0);
        let profile = config.get_effective_profile_for_repo(&repo.path);
        let others: Vec<&IdeConfig> = profile
            .visible_ides()
            .into_iter()
            .filter(|i| !on_card.contains(&i.id.as_str()))
            .collect();
        if !others.is_empty() {
            ui.label(
                RichText::new(tr(lang, "repo_menu_open_in"))
                    .size(11.0)
                    .weak(),
            );
            for ide in others {
                let image = ide_image(ide).fit_to_exact_size(Vec2::splat(14.0));
                if ui
                    .add(Button::image_and_text(image, &ide.display_name))
                    .clicked()
                {
                    let file = repo
                        .selected_solution
                        .clone()
                        .unwrap_or_else(|| repo.path.clone());
                    // App merkt die Wahl (`selected_ide`) → Karte zeigt sie
                    actions.ide_open = Some((repo.path.clone(), ide.id.clone(), file));
                }
            }
            ui.separator();
        }

        // Profil-Override (vorher Dropdown in Zeile 1)
        let current = config
            .get_repo_state(&repo.path)
            .and_then(|s| s.profile_override.as_deref());
        ui.menu_button(tr(lang, "repo_menu_profile"), |ui| {
            if ui
                .selectable_label(current.is_none(), "— Global —")
                .clicked()
            {
                actions.profile_override = Some((repo.path.clone(), None));
            }
            for p in &config.profiles {
                let is_selected = current == Some(p.id.as_str());
                let text = if current.is_none() && p.id == config.active_profile_id {
                    format!("● {} (global)", p.display_name)
                } else {
                    p.display_name.clone()
                };
                if ui.selectable_label(is_selected, text).clicked() {
                    actions.profile_override = Some((repo.path.clone(), Some(p.id.clone())));
                }
            }
        })
        .response
        .on_hover_text(tr(lang, "profile_override_tooltip"));

        if !repo.is_linked_worktree
            && menu_item(ui, ICON_FOLDER_PLUS, &tr(lang, "repo_menu_new_worktree"))
        {
            actions.new_worktree = Some(Some(repo.path.clone()));
        }
        ui.separator();
        if menu_item(ui, ICON_COPY, &tr(lang, "repo_menu_copy_path")) {
            actions.copy_text = Some(repo.path.display().to_string());
        }
        if menu_item(ui, ICON_COPY, &tr(lang, "repo_menu_copy_branch")) {
            actions.copy_text = Some(repo.branch.clone());
        }
        if let Some(url) = remote_web_url(repo, &config.merge_requests.host_aliases) {
            if menu_item(ui, ICON_BROWSER, &tr(lang, "repo_menu_open_browser")) {
                actions.open_url = Some(url);
            }
        }
        if repo.is_linked_worktree {
            ui.separator();
            if menu_item(ui, ICON_TRASH, &tr(lang, "repo_menu_remove_worktree")) {
                actions.remove_worktree = Some(repo.path.clone());
            }
        }
    });
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &label));
    hover_outline(ui, &resp);
    resp.on_hover_text(label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HostAlias, IdeConfig, LanguageProfile};
    use crate::git::{InProgressKind, RepoInfo, RepoState};
    use crate::gitlab::remote::RemoteInfo;
    use crate::i18n::Language;
    use std::path::PathBuf;

    /// Dark-Palette wie auf der Karte im Dark-Theme.
    fn dark_palette() -> MrPalette {
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx, &crate::config::Theme::Dark);
        mr_palette(&crate::config::Theme::Dark, &ctx.global_style().visuals)
    }

    #[test]
    fn state_chip_texts_and_colors() {
        let en = Language::En;
        let de = Language::De;
        let p = dark_palette();
        assert_eq!(
            state_chip(en, RepoState::Clean, 0, &p),
            ("clean".to_string(), p.ok_icon)
        );
        assert_eq!(
            state_chip(de, RepoState::Clean, 0, &p),
            ("sauber".to_string(), p.ok_icon)
        );
        assert_eq!(
            state_chip(en, RepoState::Dirty, 3, &p),
            ("3 changed".to_string(), p.warn_icon)
        );
        assert_eq!(
            state_chip(de, RepoState::Dirty, 3, &p),
            ("3 geändert".to_string(), p.warn_icon)
        );
        assert_eq!(
            state_chip(en, RepoState::Conflict, 2, &p),
            ("Conflict".to_string(), p.fail_icon)
        );
        assert_eq!(
            state_chip(de, RepoState::Conflict, 2, &p),
            ("Konflikt".to_string(), p.fail_icon)
        );
        let rebase = RepoState::InProgress(InProgressKind::Rebase);
        assert_eq!(
            state_chip(en, rebase, 0, &p),
            ("Rebase in progress".to_string(), p.fail_icon)
        );
        assert_eq!(
            state_chip(de, rebase, 0, &p),
            ("Rebase läuft".to_string(), p.fail_icon)
        );
        let merge = RepoState::InProgress(InProgressKind::Merge);
        assert_eq!(state_chip(de, merge, 0, &p).0, "Merge läuft");
    }

    /// Farbton in Grad (0…360) für den Abstand zwischen Chip-Farben.
    fn hue(c: Color32) -> f32 {
        let (r, g, b) = (c.r() as f32, c.g() as f32, c.b() as f32);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        if max == min {
            return 0.0;
        }
        let h = if max == r {
            (g - b) / (max - min)
        } else if max == g {
            2.0 + (b - r) / (max - min)
        } else {
            4.0 + (r - g) / (max - min)
        };
        (h * 60.0).rem_euclid(360.0)
    }

    #[test]
    fn state_chip_conflict_and_changed_distinct_in_every_theme() {
        // „Geändert“ und „Konflikt“ lagen auf Dunkel fast im selben Rot
        // (220,70,40 vs. 210,45,45); Kontrast deckt `mr_palette_contrast` ab
        for theme in crate::config::Theme::all() {
            let ctx = egui::Context::default();
            crate::ui::theme::apply_theme(&ctx, &theme);
            let p = mr_palette(&theme, &ctx.global_style().visuals);
            let color = |s| state_chip(Language::En, s, 1, &p).1;
            let dirty = hue(color(RepoState::Dirty));
            let conflict = hue(color(RepoState::Conflict));
            let d = (dirty - conflict).abs();
            let d = d.min(360.0 - d);
            assert!(d >= 25.0, "{theme:?}: hue distance {d:.0}°");
            assert_ne!(color(RepoState::Clean), color(RepoState::Dirty));
        }
    }

    fn ide(id: &str) -> IdeConfig {
        IdeConfig {
            id: id.to_string(),
            display_name: id.to_uppercase(),
            program: id.to_string(),
            args: vec![],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,
            icon: None,
        }
    }

    fn profile(default: Option<&str>, hidden: &[&str]) -> LanguageProfile {
        LanguageProfile {
            id: "p".to_string(),
            display_name: "P".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![ide("vs"), ide("rider"), ide("vscode")],
            default_ide_id: default.map(str::to_string),
            ide_order: Vec::new(),
            hidden_ide_ids: hidden.iter().map(|s| s.to_string()).collect(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        }
    }

    #[test]
    fn card_ide_prefers_selected_then_default_then_first() {
        let p = profile(Some("rider"), &[]);
        let id = |i: Option<&IdeConfig>| i.map(|i| i.id.clone());
        assert_eq!(id(card_ide(&p, Some("vscode"))).as_deref(), Some("vscode"));
        assert_eq!(id(card_ide(&p, None)).as_deref(), Some("rider"));
        // Unbekannte Auswahl → Default
        assert_eq!(id(card_ide(&p, Some("weg"))).as_deref(), Some("rider"));
        // Ausgeblendete Auswahl → Default
        let hidden = profile(Some("rider"), &["vscode"]);
        assert_eq!(
            id(card_ide(&hidden, Some("vscode"))).as_deref(),
            Some("rider")
        );
        // Ausgeblendeter Default → erste sichtbare
        let no_default = profile(Some("vs"), &["vs"]);
        assert_eq!(id(card_ide(&no_default, None)).as_deref(), Some("rider"));
        // Keine sichtbare IDE
        let none = profile(None, &["vs", "rider", "vscode"]);
        assert!(card_ide(&none, Some("vs")).is_none());
    }

    #[test]
    fn card_ides_keep_card_ide_and_profile_order() {
        let p = profile(Some("rider"), &[]);
        let ids = |v: Vec<&IdeConfig>| v.into_iter().map(|i| i.id.clone()).collect::<Vec<_>>();
        let all = ids(p.visible_ides());
        assert_eq!(
            ids(card_ides(&p, None, 10)),
            all,
            "alle, Profil-Reihenfolge"
        );
        assert_eq!(ids(card_ides(&p, Some("vscode"), 1)), ["vscode"]);
        assert_eq!(ids(card_ides(&p, None, 0)), ["rider"], "mindestens eine");
        let two = ids(card_ides(&p, Some("vscode"), 2));
        assert_eq!(two.len(), 2);
        assert!(two.contains(&"vscode".to_string()));
        let hidden = profile(Some("rider"), &["vscode"]);
        assert!(!ids(card_ides(&hidden, None, 10)).contains(&"vscode".to_string()));
    }

    fn repo_with_remote(url: &str) -> RepoInfo {
        let mut repo = RepoInfo::new(PathBuf::from("/code/app"), "main".into(), false, false);
        repo.remotes = vec![RemoteInfo::new("origin", url)];
        repo
    }

    #[test]
    fn remote_web_url_ssh_and_https() {
        let ssh = repo_with_remote("git@gitlab.example.com:acme/app.git");
        assert_eq!(
            remote_web_url(&ssh, &[]).as_deref(),
            Some("https://gitlab.example.com/acme/app")
        );
        let https = repo_with_remote("https://gitlab.example.com/acme/sub/app.git");
        assert_eq!(
            remote_web_url(&https, &[]).as_deref(),
            Some("https://gitlab.example.com/acme/sub/app")
        );
        // SSH-Port fällt weg, HTTPS-Port bleibt
        let ssh_port = repo_with_remote("ssh://git@gitlab.example.com:2222/acme/app.git");
        assert_eq!(
            remote_web_url(&ssh_port, &[]).as_deref(),
            Some("https://gitlab.example.com/acme/app")
        );
        let https_port = repo_with_remote("https://gitlab.example.com:8443/acme/app.git");
        assert_eq!(
            remote_web_url(&https_port, &[]).as_deref(),
            Some("https://gitlab.example.com:8443/acme/app")
        );
        // Alias: SSH-Host gehört zum Web-Host
        let aliased = repo_with_remote("git@ssh.gitlab.example.com:acme/app.git");
        let aliases = [HostAlias {
            from: "ssh.gitlab.example.com".to_string(),
            to: "gitlab.example.com".to_string(),
        }];
        assert_eq!(
            remote_web_url(&aliased, &aliases).as_deref(),
            Some("https://gitlab.example.com/acme/app")
        );
        // Lokaler Pfad und kein Remote → nichts
        let local = repo_with_remote("/srv/git/app.git");
        assert_eq!(remote_web_url(&local, &aliases), None);
        let none = RepoInfo::new(PathBuf::from("/code/x"), "main".into(), false, false);
        assert_eq!(remote_web_url(&none, &[]), None);
    }

    #[test]
    fn remote_web_url_prefers_origin() {
        let mut repo = RepoInfo::new(PathBuf::from("/code/app"), "main".into(), false, false);
        repo.remotes = vec![
            RemoteInfo::new("fork", "https://github.com/me/app.git"),
            RemoteInfo::new("origin", "https://gitlab.example.com/acme/app.git"),
        ];
        assert_eq!(
            remote_web_url(&repo, &[]).as_deref(),
            Some("https://gitlab.example.com/acme/app")
        );
    }

    #[test]
    fn mr_chip_text_shows_iid_and_approvals() {
        use crate::gitlab::badge::MrBadge;
        use crate::gitlab::model::PipelineState;
        let mut badge = MrBadge {
            host_key: "h".into(),
            id: "gid://1".into(),
            iid: 128,
            title: "Fix".into(),
            draft: false,
            approved: 1,
            required: Some(2),
            pipeline: PipelineState::Failed,
            mine: true,
            updated_at: None,
        };
        assert_eq!(mr_chip_text(&badge), "!128 · 1/2");
        badge.required = None;
        assert_eq!(mr_chip_text(&badge), "!128 · 1");
        badge.approved = 0;
        assert_eq!(mr_chip_text(&badge), "!128");
    }

    #[test]
    fn claude_chip_texts() {
        use crate::claude_live::LiveStatus;
        let de = Language::De;
        let en = Language::En;
        assert_eq!(
            claude_chip_text(de, LiveStatus::Waiting).as_deref(),
            Some("Claude wartet")
        );
        assert_eq!(
            claude_chip_text(en, LiveStatus::Waiting).as_deref(),
            Some("Claude waiting")
        );
        for s in [LiveStatus::Busy, LiveStatus::Shell] {
            assert_eq!(claude_chip_text(de, s).as_deref(), Some("Claude arbeitet"));
            assert_eq!(claude_chip_text(en, s).as_deref(), Some("Claude working"));
        }
        assert_eq!(claude_chip_text(de, LiveStatus::Idle), None);
    }

    #[test]
    fn card_texts_exist_in_both_languages() {
        for key in CARD_KEYS {
            for lang in Language::all() {
                assert_ne!(crate::i18n::tr(lang, key), *key, "{key} ({lang:?})");
            }
        }
    }

    #[test]
    fn card_icons_are_white_phosphor() {
        for icon in [
            include_str!("../../assets/icons/star.svg"),
            include_str!("../../assets/icons/star-fill.svg"),
            include_str!("../../assets/icons/dots-three.svg"),
            include_str!("../../assets/icons/arrow-up.svg"),
            include_str!("../../assets/icons/arrow-down.svg"),
        ] {
            assert!(icon.contains(r##"color="#FFFFFF""##), "{icon}");
            assert!(icon.contains(r#"viewBox="0 0 256 256""#), "{icon}");
        }
    }
}
