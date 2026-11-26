use crate::config::{AgentProfile, AppConfig, LanguageProfile, TerminalPreference};
use crate::i18n::{tr, tr_fmt, Language};
use crate::ui::theme::COLOR_MUTED;
use egui::{Color32, RichText, Vec2};

mod agents;
mod general;
mod icons;
mod merge_requests;
mod profiles;
mod tray_icons;

use agents::show_agents_tab;
use general::show_general_tab;
use icons::show_icons_tab;
use profiles::show_profiles_tab;
use tray_icons::show_tray_icons_tab;

pub use merge_requests::{
    error_reason, forget_token_fields, project_paths_for_host, rediscover_candidates,
    review_root_hit, set_alias, verify_error, GlabCheck, MrSettingsInput, MrSettingsOutput,
    TokenEvent,
};

use crate::ui::icons::ICON_CROSS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    General,
    Profiles,
    Agents,
    Terminal,
    Appearance,
    Language,
    Icons,
    TrayIcons,
    MergeRequests,
}

/// Reiter des Einstellungsfensters in Anzeigereihenfolge.
fn settings_tabs(lang: Language) -> Vec<(SettingsTab, String)> {
    vec![
        (SettingsTab::General, tr(lang, "tabs_general")),
        (SettingsTab::Profiles, tr(lang, "tabs_profiles")),
        (SettingsTab::Agents, tr(lang, "tabs_agents")),
        (SettingsTab::Terminal, tr(lang, "tabs_terminal")),
        (SettingsTab::Appearance, tr(lang, "tabs_appearance")),
        (SettingsTab::Language, tr(lang, "tabs_language")),
        (SettingsTab::Icons, tr(lang, "tabs_icons")),
        (SettingsTab::TrayIcons, tr(lang, "tabs_tray_icons")),
        (SettingsTab::MergeRequests, tr(lang, "tabs_merge_requests")),
    ]
}

pub struct SettingsState {
    pub draft: AppConfig,
    pub error: Option<String>,
    pub success: Option<String>,
    pub auto_detect_toast: Option<(String, std::time::Instant, bool)>,
    /// Laufende Suche von „Auto erkennen“ (Tab „Profile“)
    ide_detect: Option<IdeDetect>,
    selected_tab: SettingsTab,
    selected_profile_idx: Option<usize>,
    selected_agent_idx: Option<usize>,
    // Temporäre Eingabefelder für neue Profile/Agents
    new_profile_name: String,
    new_profile_ext: String,
    new_agent_name: String,
    new_agent_program: String,
    prev_ide_args: std::collections::HashMap<String, Vec<String>>,
    /// Tab „Merge Requests“ (Token-Formular, erste Prüfung)
    mr: merge_requests::MrSettingsState,
}

/// „Auto erkennen“ für eine IDE: `vswhere` bzw. die Suche in der Toolbox
/// laufen in einem Thread (ohne Zeitlimit, im UI-Thread stünde das Fenster).
struct IdeDetect {
    profile_id: String,
    ide_id: String,
    is_vs: bool,
    rx: std::sync::mpsc::Receiver<Option<std::path::PathBuf>>,
}

impl IdeDetect {
    fn start(ctx: &egui::Context, profile_id: &str, ide_id: &str, is_vs: bool) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let found = if is_vs {
                crate::git::resolve_vs_path_force()
            } else {
                crate::git::resolve_rider_path_force()
            };
            let _ = tx.send(found);
            ctx.request_repaint();
        });
        Self {
            profile_id: profile_id.to_string(),
            ide_id: ide_id.to_string(),
            is_vs,
            rx,
        }
    }
}

impl SettingsState {
    /// Ergebnis von „Auto erkennen“ in den Entwurf übernehmen.
    fn poll_ide_detect(&mut self) {
        let Some(detect) = &self.ide_detect else {
            return;
        };
        let found = match detect.rx.try_recv() {
            Ok(found) => found,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            // Thread ohne Ergebnis beendet (Panic): wie „nicht gefunden“
            Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
        };
        let Some(detect) = self.ide_detect.take() else {
            return;
        };
        let ide = self
            .draft
            .profiles
            .iter_mut()
            .filter(|p| p.id == detect.profile_id)
            .flat_map(|p| &mut p.ides)
            .find(|ide| ide.id == detect.ide_id);
        let (text, is_error) = match (found, ide) {
            (Some(path), Some(ide)) => {
                ide.program = path.display().to_string();
                (format!("Erkannt: {}", ide.program), false)
            }
            (None, _) if detect.is_vs => (
                "Visual Studio nicht gefunden (vswhere fehlt oder keine Installation)".to_string(),
                true,
            ),
            (None, _) => (
                "Rider nicht gefunden (Toolbox / Program Files / PATH)".to_string(),
                true,
            ),
            // IDE inzwischen entfernt oder umbenannt
            (Some(_), None) => return,
        };
        self.auto_detect_toast = Some((text, std::time::Instant::now(), is_error));
        self.error = None;
        self.success = None;
    }
}

/// Beim Start erkanntes Programm einer IDE (Visual Studio, Rider), deren
/// Eintrag nur den bloßen Programmnamen trägt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedIde {
    pub profile_id: String,
    pub ide_id: String,
    /// Programm zum Zeitpunkt der Suche
    pub previous: String,
    /// Gefundener Pfad (anfangs gleich `previous`)
    pub program: String,
}

impl DetectedIde {
    /// IDE-Einträge, für die sich die Suche lohnt.
    pub fn candidates(config: &AppConfig) -> Vec<Self> {
        let mut out = Vec::new();
        for profile in &config.profiles {
            for ide in &profile.ides {
                let wanted = (ide.id == "vs2022" && crate::git::is_vs_program(&ide.program))
                    || (ide.id == "rider" && crate::git::is_rider_program(&ide.program));
                if wanted {
                    out.push(Self {
                        profile_id: profile.id.clone(),
                        ide_id: ide.id.clone(),
                        previous: ide.program.clone(),
                        program: ide.program.clone(),
                    });
                }
            }
        }
        out
    }

    /// Trägt den Pfad ein, wenn dort noch das Programm von vorhin steht
    /// (inzwischen Geändertes bleibt). `true`, wenn etwas geändert wurde.
    pub fn apply(&self, config: &mut AppConfig) -> bool {
        let ide = config
            .profiles
            .iter_mut()
            .filter(|p| p.id == self.profile_id)
            .flat_map(|p| &mut p.ides)
            .find(|ide| ide.id == self.ide_id && ide.program == self.previous);
        match ide {
            Some(ide) if ide.program != self.program => {
                ide.program = self.program.clone();
                true
            }
            _ => false,
        }
    }
}

/// Wirksame Reihenfolge: gespeicherte IDs, soweit es sie noch gibt, dann
/// die fehlenden aus `all`. Mit `keep_unknown` bleiben IDs außerhalb von
/// `all` am Ende erhalten (Tray-Icons neuerer Versionen).
fn normalize_icon_order(order: &[String], all: &[String], keep_unknown: bool) -> Vec<String> {
    let mut effective: Vec<String> = Vec::with_capacity(all.len());
    let known = order.iter().filter(|id| all.contains(id));
    let unknown = order.iter().filter(|id| keep_unknown && !all.contains(id));
    for id in known.chain(all).chain(unknown) {
        if !effective.contains(id) {
            effective.push(id.clone());
        }
    }
    effective
}

fn ensure_icon_orders(profile: &mut LanguageProfile, agents: &[AgentProfile]) {
    let ide_ids: Vec<String> = profile.ides.iter().map(|i| i.id.clone()).collect();
    profile.ide_order = normalize_icon_order(&profile.ide_order, &ide_ids, false);
    let agent_ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
    profile.agent_order = normalize_icon_order(&profile.agent_order, &agent_ids, false);
}

/// Klick in einer Zeile der Icon-Listen (Tabs „Icons“ und „Tray Icons“).
enum IconRowAction {
    ToggleHidden,
    /// Um so viele Plätze verschieben (-1 hoch, +1 runter)
    Move(isize),
}

/// Zeile einer Icon-Liste: Auge (ein-/ausblenden), Name, rechts ↑/↓.
fn icon_order_row(
    ui: &mut egui::Ui,
    lang: Language,
    label: &str,
    is_hidden: bool,
    idx: usize,
    len: usize,
) -> Option<IconRowAction> {
    use crate::ui::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_UP, ICON_EYE, ICON_EYE_OFF};
    let mut action = None;
    ui.horizontal(|ui| {
        let color = if is_hidden {
            ui.visuals().weak_text_color()
        } else {
            ui.visuals().text_color()
        };
        let icon = |source| {
            egui::Image::new(source)
                .fit_to_exact_size(Vec2::splat(14.0))
                .tint(color)
        };
        let eye = if is_hidden { ICON_EYE_OFF } else { ICON_EYE };
        if ui
            .add(egui::Button::image(icon(eye)))
            .on_hover_text(tr(lang, "icons_toggle_visibility"))
            .clicked()
        {
            action = Some(IconRowAction::ToggleHidden);
        }
        ui.label(RichText::new(label).size(11.0).color(color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let tint = ui.visuals().text_color();
            let arrow = |source| {
                egui::Button::image(
                    egui::Image::new(source)
                        .fit_to_exact_size(Vec2::splat(14.0))
                        .tint(tint),
                )
            };
            if ui
                .add_enabled(idx + 1 < len, arrow(ICON_CHEVRON_DOWN))
                .clicked()
            {
                action = Some(IconRowAction::Move(1));
            }
            if ui.add_enabled(idx > 0, arrow(ICON_CHEVRON_UP)).clicked() {
                action = Some(IconRowAction::Move(-1));
            }
        });
    });
    ui.add_space(2.0);
    action
}

/// Verschiebt den Eintrag `idx` der wirksamen Reihenfolge um `delta` und
/// speichert sie. Immer von `effective` aus: die gespeicherte Liste kann
/// kürzer oder anders sortiert sein als die gezeigte (neue IDE, entfernter
/// Agent), ein Tausch nach deren Index träfe falsche Einträge.
fn move_in_order(order: &mut Vec<String>, effective: &[String], idx: usize, delta: isize) {
    let Some(target) = idx.checked_add_signed(delta) else {
        return;
    };
    if idx < effective.len() && target < effective.len() {
        *order = effective.to_vec();
        order.swap(idx, target);
    }
}

/// ID in der Liste der ausgeblendeten umschalten (sortiert, ohne Dubletten).
fn toggle_hidden(hidden: &mut Vec<String>, id: &str) {
    if let Some(i) = hidden.iter().position(|h| h == id) {
        hidden.remove(i);
    } else {
        hidden.push(id.to_string());
        hidden.sort();
        hidden.dedup();
    }
}

/// „Ausgeblendet: a, b“ unter einer Icon-Liste.
fn hidden_info(ui: &mut egui::Ui, lang: Language, hidden: &[String]) {
    let ids = if hidden.is_empty() {
        "—".to_string()
    } else {
        hidden.join(", ")
    };
    ui.label(
        RichText::new(format!("{}: {ids}", tr(lang, "icons_hidden_info")))
            .size(10.0)
            .color(ui.visuals().weak_text_color()),
    );
}

/// Limit 5–50 als Slider und Zahlenfeld mit Zusammenfassung darunter
/// (Repos bzw. Branches im Tray-Popup).
fn tray_limit_control(
    ui: &mut egui::Ui,
    lang: Language,
    value: &mut usize,
    unit: &str,
    summary_key: &str,
) {
    const RANGE: std::ops::RangeInclusive<usize> = 5..=50;
    *value = (*value).clamp(*RANGE.start(), *RANGE.end());
    let weak = ui.visuals().weak_text_color();
    ui.horizontal(|ui| {
        ui.label(tr(lang, "tray_limit_label"));
        ui.add(egui::Slider::new(value, RANGE).text(unit).step_by(1.0));
    });
    ui.horizontal(|ui| {
        ui.label(tr(lang, "tray_direct_label"));
        ui.add(egui::DragValue::new(value).range(RANGE).speed(1.0));
        ui.label(
            RichText::new(tr_fmt(lang, "limit_current", &[&value.to_string()]))
                .size(11.0)
                .color(weak),
        );
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(tr_fmt(lang, summary_key, &[&value.to_string()]))
            .size(10.0)
            .color(weak)
            .italics(),
    );
}

impl SettingsState {
    pub fn from_config(cfg: &AppConfig) -> Self {
        let mut draft = cfg.clone();
        // Icon-Reihenfolgen vervollständigen ohne Draft als dirty zu markieren
        for profile in &mut draft.profiles {
            ensure_icon_orders(profile, &draft.agents);
        }
        Self {
            draft,
            error: None,
            success: None,
            auto_detect_toast: None,
            ide_detect: None,
            selected_tab: SettingsTab::General,
            selected_profile_idx: if cfg.profiles.is_empty() {
                None
            } else {
                Some(0)
            },
            selected_agent_idx: if cfg.agents.is_empty() { None } else { Some(0) },
            new_profile_name: String::new(),
            new_profile_ext: String::new(),
            new_agent_name: String::new(),
            new_agent_program: String::new(),
            prev_ide_args: Default::default(),
            mr: Default::default(),
        }
    }

    /// Deep-Link aus der MR-Ansicht: Tab „Merge Requests“ direkt öffnen.
    pub fn select_merge_requests_tab(&mut self) {
        self.selected_tab = SettingsTab::MergeRequests;
    }

    /// Ist der Tab „Merge Requests“ gewählt?
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn shows_merge_requests_tab(&self) -> bool {
        self.selected_tab == SettingsTab::MergeRequests
    }

    /// Id des offenen Token-Feldes (`forget_token_fields`).
    pub fn token_field_id(&self) -> Option<egui::Id> {
        self.mr.token_field_id()
    }

    /// Ergebnis eines Token-Workers für das offene Formular; nach dem
    /// Speichern zusätzlich die Erfolgszeile.
    pub fn on_token_event(&mut self, ctx: &egui::Context, host: &str, event: TokenEvent) {
        let lang = self.draft.language;
        if matches!(event, TokenEvent::Saved(Ok(_))) {
            self.error = None;
            self.success = Some(tr_fmt(lang, "mrs_token_saved", &[host]));
        }
        self.mr.on_token_event(ctx, lang, host, event);
    }
}

pub fn show_settings_window(
    ctx: &egui::Context,
    state: &mut SettingsState,
    open: &mut bool,
    on_save: &mut Option<AppConfig>,
    update_error: Option<&str>,
    mr_input: &MrSettingsInput,
    mr_out: &mut MrSettingsOutput,
) {
    let mut should_close = false;

    egui::Window::new(tr(state.draft.language, "settings"))
        .open(open)
        .resizable(true)
        .default_width(720.0)
        .default_height(560.0)
        .min_width(640.0)
        .show(ctx, |ui| {
            // Tabs (umbrechend, seit „Merge Requests“ neun Reiter)
            let lang = state.draft.language;
            ui.horizontal_wrapped(|ui| {
                for (tab, label) in settings_tabs(lang) {
                    let is_active = state.selected_tab == tab;
                    if ui
                        .selectable_label(is_active, RichText::new(label).size(12.0).strong())
                        .clicked()
                    {
                        state.selected_tab = tab;
                    }
                }
            });
            ui.separator();
            ui.add_space(6.0);

            // Fußzeile (Config-Pfad, Meldungen, Speichern/Schließen) fest unter
            // der ScrollArea, wie im Mockup des Tabs „Merge Requests“
            egui::Panel::bottom("settings_footer")
                .frame(egui::Frame::new().inner_margin(egui::Margin {
                    left: 0,
                    right: 0,
                    top: 8,
                    bottom: 0,
                }))
                .show(ui, |ui| {
                    if let Some(cfg_path) = AppConfig::config_path() {
                        ui.label(
                            RichText::new(format!(
                                "{} {}",
                                tr(state.draft.language, "config_path"),
                                cfg_path.display()
                            ))
                            .size(10.0)
                            .color(Color32::from_rgb(140, 140, 140)),
                        );
                        ui.add_space(8.0);
                    }

                    if let Some(err) = &state.error {
                        ui.colored_label(Color32::from_rgb(200, 40, 40), format!("✗ {}", err));
                        ui.add_space(4.0);
                    }
                    if let Some(msg) = &state.success {
                        ui.colored_label(Color32::from_rgb(40, 140, 40), format!("✓ {}", msg));
                        ui.add_space(4.0);
                    }

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(RichText::new(tr(state.draft.language, "close")).size(12.0))
                                .clicked()
                            {
                                should_close = true;
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(tr(state.draft.language, "save"))
                                            .size(12.0)
                                            .strong(),
                                    )
                                    .fill(Color32::from_rgb(60, 120, 220))
                                    .stroke(egui::Stroke::NONE),
                                )
                                .clicked()
                            {
                                let lang = state.draft.language;
                                if state.draft.roots.is_empty() {
                                    state.error = Some(tr(lang, "error_need_path"));
                                    state.success = None;
                                } else if state.draft.profiles.is_empty() {
                                    state.error = Some(tr(lang, "error_need_profile"));
                                    state.success = None;
                                } else {
                                    // Validierung Profile
                                    let mut valid = true;
                                    for p in &state.draft.profiles {
                                        if p.id.trim().is_empty() || p.display_name.trim().is_empty() {
                                            state.error =
                                                Some(tr_fmt(lang, "error_profile_empty", &[&p.id]));
                                            valid = false;
                                            break;
                                        }
                                        if p.file_extension.trim().is_empty() {
                                            state.error = Some(tr_fmt(
                                                lang,
                                                "error_profile_ext",
                                                &[&p.display_name],
                                            ));
                                            valid = false;
                                            break;
                                        }
                                        // Validierung Config-Selektoren (pro Profil)
                                        {
                                            use std::collections::HashSet;
                                            let mut seen: HashSet<String> = HashSet::new();
                                            for sel in &p.config_selectors {
                                                if sel.id.trim().is_empty() {
                                                    state.error = Some(format!(
                                                        "Selector in Profil '{}' hat leere ID",
                                                        p.display_name
                                                    ));
                                                    valid = false;
                                                    break;
                                                }
                                                if sel.display_name.trim().is_empty() {
                                                    state.error = Some(format!(
                                                        "Selector '{}' in Profil '{}' braucht Anzeigename",
                                                        sel.id, p.display_name
                                                    ));
                                                    valid = false;
                                                    break;
                                                }
                                                if sel.file_path.trim().is_empty() {
                                                    state.error = Some(format!(
                                                        "Selector '{}' in Profil '{}' braucht Dateipfad",
                                                        sel.id, p.display_name
                                                    ));
                                                    valid = false;
                                                    break;
                                                }
                                                if sel.key.trim().is_empty() {
                                                    state.error = Some(format!(
                                                        "Selector '{}' in Profil '{}' braucht Key",
                                                        sel.id, p.display_name
                                                    ));
                                                    valid = false;
                                                    break;
                                                }
                                                if sel.key_attribute.trim().is_empty() {
                                                    state.error = Some(format!(
                                                        "Selector '{}' in Profil '{}' braucht Key-Attribut",
                                                        sel.id, p.display_name
                                                    ));
                                                    valid = false;
                                                    break;
                                                }
                                                if sel.value_attribute.trim().is_empty() {
                                                    state.error = Some(format!(
                                                        "Selector '{}' in Profil '{}' braucht Value-Attribut",
                                                        sel.id, p.display_name
                                                    ));
                                                    valid = false;
                                                    break;
                                                }
                                                let nid = sel.id.trim().to_lowercase().replace(' ', "_");
                                                if !seen.insert(nid) {
                                                    state.error = Some(format!(
                                                        "Doppelte Selector-ID '{}' in Profil '{}'",
                                                        sel.id, p.display_name
                                                    ));
                                                    valid = false;
                                                    break;
                                                }
                                            }
                                            if !valid {
                                                break;
                                            }
                                        }
                                    }
                                    // Merge Requests: glab-Programm ohne Shell-Metazeichen
                                    if valid
                                        && !merge_requests::valid_glab_program(
                                            &state.draft.merge_requests.glab_program,
                                        )
                                    {
                                        state.error = Some(tr(lang, "mrs_glab_invalid"));
                                        valid = false;
                                    }
                                    if valid {
                                        // Wie beim Laden bereinigen (Listen, Intervall, Pfade)
                                        state.draft.merge_requests.normalize();
                                        // Geschrieben wird in der App (im
                                        // Hintergrund, mit den Laufzeitfeldern
                                        // der aktuellen Config)
                                        state.error = None;
                                        *on_save = Some(state.draft.clone());
                                    } else {
                                        state.success = None;
                                    }
                                }
                            }
                            if ui.button(tr(state.draft.language, "reset")).clicked() {
                                state.error = None;
                                state.success = None;
                            }
                        });
                    });
                });

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    match state.selected_tab {
                        SettingsTab::General => show_general_tab(ui, state, update_error),
                        SettingsTab::Profiles => show_profiles_tab(ui, state),
                        SettingsTab::Agents => show_agents_tab(ui, state),
                        SettingsTab::Terminal => show_terminal_tab(ui, state),
                        SettingsTab::Appearance => show_appearance_tab(ui, state),
                        SettingsTab::Language => show_language_tab(ui, state),
                        SettingsTab::Icons => show_icons_tab(ui, state),
                        SettingsTab::TrayIcons => show_tray_icons_tab(ui, state),
                        SettingsTab::MergeRequests => {
                            merge_requests::show_merge_requests_tab(ui, state, mr_input, mr_out)
                        }
                    }
                    ui.add_space(12.0);
                });
        });

    if should_close {
        *open = false;
    }

    // Non-modal toast für Auto-Erkennung (3s, nicht blockierend)
    if let Some((msg, t, is_err)) = state.auto_detect_toast.as_ref() {
        if t.elapsed() > std::time::Duration::from_secs(3) {
            state.auto_detect_toast = None;
        } else {
            let msg = msg.clone();
            let is_err = *is_err;
            let bg = if is_err {
                Color32::from_rgb(255, 235, 235)
            } else {
                Color32::from_rgb(235, 255, 235)
            };
            let stroke_col = if is_err {
                Color32::from_rgb(220, 100, 100)
            } else {
                Color32::from_rgb(100, 180, 100)
            };
            let text_col = if is_err {
                Color32::from_rgb(160, 40, 40)
            } else {
                Color32::from_rgb(40, 100, 40)
            };
            let mut close_toast = false;
            egui::Window::new("auto_detect_toast")
                .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .frame(
                    egui::Frame::new()
                        .fill(bg)
                        .stroke(egui::Stroke::new(1.0, stroke_col))
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(12, 8)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(msg.clone())
                                .size(12.0)
                                .color(text_col)
                                .strong(),
                        );
                        if ui
                            .add(egui::Button::image(
                                egui::Image::new(ICON_CROSS)
                                    .tint(ui.visuals().text_color())
                                    .fit_to_exact_size(Vec2::splat(12.0)),
                            ))
                            .clicked()
                        {
                            close_toast = true;
                        }
                    });
                });
            if close_toast || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                state.auto_detect_toast = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
    }
}

fn show_terminal_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(RichText::new("Terminal-Einstellungen").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Wähle das bevorzugte Terminal zum Starten von AI-Agents. 'Auto' probiert Windows Terminal → Powershell → cmd.")
            .size(11.0)
            .color(COLOR_MUTED),
    );
    ui.add_space(12.0);

    ui.label(RichText::new("Bevorzugtes Terminal:").size(12.0).strong());
    ui.add_space(4.0);
    let current = format!("{:?}", state.draft.terminal.preference);
    egui::ComboBox::from_id_salt("terminal_pref")
        .selected_text(current.clone())
        .show_ui(ui, |ui| {
            for (pref, label) in [
                (TerminalPreference::Auto, "Auto (empfohlen)"),
                (TerminalPreference::WindowsTerminal, "Windows Terminal (wt)"),
                (TerminalPreference::Powershell, "Powershell / pwsh"),
                (TerminalPreference::Cmd, "cmd"),
            ] {
                let is_sel = state.draft.terminal.preference == pref;
                if ui.selectable_label(is_sel, label).clicked() {
                    state.draft.terminal.preference = pref;
                }
            }
        });
    ui.add_space(8.0);
    // Custom Terminal
    ui.horizontal(|ui| {
        ui.label("Custom Terminal (optional):");
        if let TerminalPreference::Custom(custom) = &mut state.draft.terminal.preference {
            let mut c = custom.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut c).hint_text("z.B. alacritty"))
                .changed()
            {
                *custom = c;
            }
        } else {
            ui.label(RichText::new("Nur bei 'Custom' relevant").weak().size(11.0));
            if ui.small_button("Auf Custom wechseln").clicked() {
                state.draft.terminal.preference = TerminalPreference::Custom("".to_string());
            }
        }
    });
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(RichText::new("Hinweis:").size(11.0).strong());
    ui.label(
        RichText::new("• Windows Terminal (wt) wird bevorzugt – moderner, Tabs, 'wt -d <dir> -- cmd /k <agent>'\n• Powershell: 'powershell -NoExit -Command \"Set-Location ...; & claude\"'\n• cmd: 'cmd /C start \"\" /D \"<dir>\" cmd /K \"claude\"' (bleibt offen)\n• Alle Modi starten das Terminal im Repo-Verzeichnis und halten es nach Beenden offen.")
            .size(10.5)
            .color(Color32::from_rgb(80, 80, 80)),
    );
    ui.add_space(8.0);
    // Fallback
    ui.label(RichText::new("Fallback Terminal:").size(12.0).strong());
    let fallback_current = format!("{:?}", state.draft.terminal.fallback);
    egui::ComboBox::from_id_salt("terminal_fallback")
        .selected_text(fallback_current)
        .show_ui(ui, |ui| {
            for (pref, label) in [
                (TerminalPreference::Cmd, "cmd"),
                (TerminalPreference::Powershell, "Powershell"),
                (TerminalPreference::WindowsTerminal, "Windows Terminal"),
            ] {
                let is_sel = state.draft.terminal.fallback == pref;
                if ui.selectable_label(is_sel, label).clicked() {
                    state.draft.terminal.fallback = pref;
                }
            }
        });
}

fn show_language_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    let lang = state.draft.language;
    ui.label(RichText::new(tr(lang, "language")).size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new(if lang == Language::En {
            "Choose the application language. Changes apply after saving."
        } else {
            "Wähle die Sprache der Anwendung. Änderungen werden nach dem Speichern übernommen."
        })
        .size(11.0)
        .color(COLOR_MUTED),
    );
    ui.add_space(12.0);

    for l in Language::all() {
        let is_active = state.draft.language == l;
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
            .inner_margin(egui::Margin::symmetric(10, 8));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = format!("{} ({})", l.display_name(), l.code());
                if ui
                    .selectable_label(is_active, RichText::new(label).size(12.0))
                    .clicked()
                {
                    state.draft.language = l;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_active {
                        ui.label(
                            RichText::new(tr(lang, "active"))
                                .size(10.0)
                                .color(Color32::from_rgb(60, 120, 220))
                                .strong(),
                        );
                    } else if ui.small_button(tr(lang, "set_active")).clicked() {
                        state.draft.language = l;
                    }
                });
            });
        });
        ui.add_space(6.0);
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    ui.label(
        RichText::new(if lang == Language::En {
            "Current language:"
        } else {
            "Aktuelle Sprache:"
        })
        .size(11.0)
        .color(Color32::from_rgb(120, 120, 120)),
    );
    ui.label(
        RichText::new(format!(
            "{} - {}",
            state.draft.language.display_name(),
            state.draft.language.code()
        ))
        .size(12.0)
        .strong(),
    );
}

fn show_appearance_tab(ui: &mut egui::Ui, state: &mut SettingsState) {
    ui.label(RichText::new("Erscheinungsbild").size(13.0).strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("Wähle eines von 5 Themes — wirkt sofort nach Speichern.")
            .size(11.0)
            .color(COLOR_MUTED),
    );
    ui.add_space(12.0);

    let current = state.draft.theme.clone();
    ui.label(RichText::new("Theme:").size(12.0).strong());
    ui.add_space(4.0);

    for theme in crate::config::Theme::all() {
        let is_active = theme == current;
        let name = theme.display_name();
        let desc = match theme {
            crate::config::Theme::Light => "Hell, minimalistisch (Standard)",
            crate::config::Theme::Dark => "Dunkel, augenschonend",
            crate::config::Theme::Nord => "Nord — kühles Blau-Grau (dunkel)",
            crate::config::Theme::Dracula => "Dracula — dunkles Lila/Grün",
            crate::config::Theme::Solarized => "Solarized Light — warmes Beige",
        };
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
            .inner_margin(egui::Margin::symmetric(10, 8));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let label = if is_active {
                    format!("● {} — {}", name, desc)
                } else {
                    format!("{} — {}", name, desc)
                };
                if ui
                    .selectable_label(is_active, RichText::new(label).size(12.0))
                    .clicked()
                {
                    state.draft.theme = theme.clone();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_active {
                        ui.label(
                            RichText::new("Aktiv")
                                .size(10.0)
                                .color(Color32::from_rgb(60, 120, 220))
                                .strong(),
                        );
                    } else if ui.small_button("Aktivieren").clicked() {
                        state.draft.theme = theme.clone();
                    }
                });
            });
        });
        ui.add_space(6.0);
    }

    ui.add_space(8.0);
    ui.label(
        RichText::new("Hinweis: Theme wird beim nächsten Start automatisch angewendet und nach Speichern sofort.")
            .size(10.5)
            .color(Color32::from_rgb(120, 120, 120))
            .italics(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detected_ide_only_replaces_the_program_it_saw() {
        let mut config = AppConfig::default();
        let profile_id = config.profiles[0].id.clone();
        let ide = config.profiles[0]
            .ides
            .iter_mut()
            .find(|i| i.id == "vs2022")
            .expect("Default-Profil hat Visual Studio");
        ide.program = "devenv".to_string();
        let mut found = DetectedIde::candidates(&config)
            .into_iter()
            .find(|d| d.ide_id == "vs2022" && d.profile_id == profile_id)
            .expect("Kandidat");
        found.program = r"C:\VS\devenv.exe".to_string();
        // Inzwischen von Hand geändert: bleibt
        let mut edited = config.clone();
        edited.profiles[0]
            .ides
            .iter_mut()
            .find(|i| i.id == "vs2022")
            .unwrap()
            .program = r"D:\eigen\devenv.exe".to_string();
        assert!(!found.apply(&mut edited));
        assert!(found.apply(&mut config));
        assert!(!found.apply(&mut config), "zweites Mal ohne Änderung");
        let program = &config.profiles[0]
            .ides
            .iter()
            .find(|i| i.id == "vs2022")
            .unwrap()
            .program;
        assert_eq!(program, r"C:\VS\devenv.exe");
    }

    #[test]
    fn normalize_icon_order_keeps_order_and_completes() {
        let all = ids(&["a", "b", "c"]);
        assert_eq!(normalize_icon_order(&[], &all, false), all);
        // Entfernte raus, fehlende hinten an, Dubletten nur einmal
        let stored = ids(&["c", "weg", "a", "c"]);
        assert_eq!(
            normalize_icon_order(&stored, &all, false),
            ids(&["c", "a", "b"])
        );
        // Tray: unbekannte IDs bleiben am Ende erhalten
        assert_eq!(
            normalize_icon_order(&stored, &all, true),
            ids(&["c", "a", "b", "weg"])
        );
    }

    /// Die gespeicherte Reihenfolge ist kürzer als die gezeigte (neue IDE):
    /// der Tausch darf weder abstürzen noch falsche Einträge treffen.
    #[test]
    fn move_in_order_uses_the_shown_order() {
        let effective = ids(&["a", "b", "neu"]);
        let mut order = ids(&["a", "b"]);
        move_in_order(&mut order, &effective, 2, -1);
        assert_eq!(order, ids(&["a", "neu", "b"]));
        // Über den Rand hinaus: nichts ändert sich
        let mut order = ids(&["a", "b"]);
        move_in_order(&mut order, &effective, 0, -1);
        move_in_order(&mut order, &effective, 2, 1);
        assert_eq!(order, ids(&["a", "b"]));
        // Leere gespeicherte Reihenfolge
        let mut order = Vec::new();
        move_in_order(&mut order, &effective, 0, 1);
        assert_eq!(order, ids(&["b", "a", "neu"]));
    }

    #[test]
    fn toggle_hidden_adds_sorted_and_removes() {
        let mut hidden = ids(&["b"]);
        toggle_hidden(&mut hidden, "a");
        assert_eq!(hidden, ids(&["a", "b"]));
        toggle_hidden(&mut hidden, "b");
        assert_eq!(hidden, ids(&["a"]));
    }
    use crate::config::{AppConfig, IdeConfig, LanguageProfile, TerminalPreference};

    #[test]
    fn settings_state_from_config() {
        let cfg = AppConfig::default();
        let state = SettingsState::from_config(&cfg);
        assert_eq!(state.draft.active_profile_id, cfg.active_profile_id);
        assert!(state.error.is_none());
        assert!(state.success.is_none());
        assert_eq!(state.selected_tab, SettingsTab::General);
        assert_eq!(state.selected_profile_idx, Some(0));
        assert_eq!(state.selected_agent_idx, Some(0));
        assert_eq!(state.new_profile_name, "");
        assert_eq!(state.new_profile_ext, "");
        assert_eq!(state.new_agent_name, "");
        assert_eq!(state.new_agent_program, "");
        // Tab „Merge Requests“: nicht gewählt, kein offenes Token-Formular
        assert!(!state.shows_merge_requests_tab());
        assert!(state.token_field_id().is_none());
    }

    #[test]
    fn settings_state_empty_profiles_agents() {
        let cfg = AppConfig {
            profiles: vec![],
            agents: vec![],
            ..Default::default()
        };
        let state = SettingsState::from_config(&cfg);
        assert_eq!(state.selected_profile_idx, None);
        assert_eq!(state.selected_agent_idx, None);
    }

    #[test]
    fn settings_state_has_profile_idx() {
        let cfg = AppConfig::default();
        let state = SettingsState::from_config(&cfg);
        assert_eq!(state.selected_profile_idx, Some(0));
    }

    #[test]
    fn settings_state_has_agent_idx() {
        let cfg = AppConfig::default();
        let state = SettingsState::from_config(&cfg);
        assert_eq!(state.selected_agent_idx, Some(0));
    }

    #[test]
    fn settings_state_draft_is_independent_clone() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.active_profile_id = "changed".to_string();
        assert_ne!(cfg.active_profile_id, "changed");
    }

    #[test]
    fn settings_tabs_include_merge_requests() {
        let tabs = settings_tabs(Language::De);
        assert_eq!(tabs.len(), 9);
        assert_eq!(
            tabs.last(),
            Some(&(SettingsTab::MergeRequests, "Merge Requests".to_string()))
        );
        let mut state = SettingsState::from_config(&AppConfig::default());
        state.select_merge_requests_tab();
        assert_eq!(state.selected_tab, SettingsTab::MergeRequests);
    }

    /// Fenster mit dem Tab „Merge Requests“ und fester Fußzeile zeichnet
    /// ohne Panik; der erste Frame fragt die glab-Prüfung an.
    #[test]
    fn settings_window_renders_merge_requests_tab() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.select_merge_requests_tab();
        let probes = std::collections::BTreeMap::new();
        let probing = std::collections::BTreeSet::new();
        let input = MrSettingsInput {
            theme: &cfg.theme,
            glab_program: "glab",
            repos: &[],
            glab_hosts: &[],
            probes: &probes,
            probing: &probing,
            glab_check: None,
            glab_checking: false,
            token_busy: None,
            env_token: None,
            now: 0,
        };
        let ctx = egui::Context::default();
        let mut out = MrSettingsOutput::default();
        let mut open = true;
        let mut save = None;
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let ctx = ui.ctx().clone();
                show_settings_window(
                    &ctx, &mut state, &mut open, &mut save, None, &input, &mut out,
                );
            });
            output.drop_without_applying_deltas();
        }
        assert!(open && save.is_none());
        assert_eq!(out.start_glab_check.as_deref(), Some("glab"));
    }

    #[test]
    fn settings_tabs_all_five_and_switching() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        assert_eq!(state.selected_tab, SettingsTab::General);
        state.selected_tab = SettingsTab::Profiles;
        assert_eq!(state.selected_tab, SettingsTab::Profiles);
        state.selected_tab = SettingsTab::Agents;
        assert_eq!(state.selected_tab, SettingsTab::Agents);
        state.selected_tab = SettingsTab::Terminal;
        assert_eq!(state.selected_tab, SettingsTab::Terminal);
        state.selected_tab = SettingsTab::Appearance;
        assert_eq!(state.selected_tab, SettingsTab::Appearance);
    }

    #[test]
    fn settings_validation_empty_roots_error() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.roots = vec![];
        // simulate save validation
        if state.draft.roots.is_empty() {
            state.error = Some("Bitte mindestens einen Pfad angeben.".to_string());
        }
        assert_eq!(
            state.error,
            Some("Bitte mindestens einen Pfad angeben.".to_string())
        );
    }

    #[test]
    fn settings_validation_empty_profiles_error() {
        let cfg = AppConfig {
            profiles: vec![],
            ..Default::default()
        };
        let mut state = SettingsState::from_config(&cfg);
        state.draft.profiles = vec![];
        if state.draft.profiles.is_empty() {
            state.error = Some("Mindestens ein Sprach-Profil erforderlich.".to_string());
        }
        assert!(state.error.is_some());
    }

    #[test]
    fn settings_validation_profile_fields_empty_error() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.profiles[0].id = "".to_string();
        // validation: id trim empty
        let mut valid = true;
        for p in &state.draft.profiles {
            if p.id.trim().is_empty() || p.display_name.trim().is_empty() {
                state.error = Some(format!("Profil '{}' hat leere ID/Name", p.id));
                valid = false;
                break;
            }
        }
        assert!(!valid);
        assert!(state.error.is_some());
        // file_extension empty
        let cfg2 = AppConfig::default();
        let mut state2 = SettingsState::from_config(&cfg2);
        state2.draft.profiles[0].file_extension = "".to_string();
        valid = true;
        for p in &state2.draft.profiles {
            if p.file_extension.trim().is_empty() {
                state2.error = Some(format!("Profil '{}' braucht Dateiendung", p.display_name));
                valid = false;
                break;
            }
        }
        assert!(!valid);
    }

    #[test]
    fn settings_reset_clears_messages() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.error = Some("err".to_string());
        state.success = Some("ok".to_string());
        state.error = None;
        state.success = None;
        assert!(state.error.is_none());
        assert!(state.success.is_none());
    }

    #[test]
    fn settings_add_new_profile() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        let len_before = state.draft.profiles.len();
        let new_id = format!("custom{}", state.draft.profiles.len() + 1);
        state.draft.profiles.push(LanguageProfile {
            id: new_id.clone(),
            display_name: format!("Custom {}", len_before + 1),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![IdeConfig {
                id: "vscode".to_string(),
                display_name: "VS Code".to_string(),
                program: "code".to_string(),
                args: vec!["{file}".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,

                icon: None,
            }],
            default_ide_id: Some("vscode".to_string()),
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        });
        state.selected_profile_idx = Some(state.draft.profiles.len() - 1);
        assert_eq!(state.draft.profiles.len(), len_before + 1);
        assert_eq!(state.selected_profile_idx, Some(len_before));
    }

    #[test]
    fn settings_add_new_agent() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        let len_before = state.draft.agents.len();
        state.draft.agents.push(crate::config::AgentProfile {
            id: format!("agent{}", len_before + 1),
            display_name: "Neuer Agent".to_string(),
            program: "claude".to_string(),
            args: vec![],
            command: None,
            launch_mode: crate::config::AgentLaunchMode::Terminal,
            terminal_override: None,

            icon: None,
        });
        state.selected_agent_idx = Some(state.draft.agents.len() - 1);
        assert_eq!(state.draft.agents.len(), len_before + 1);
    }

    #[test]
    fn settings_delete_profile_requires_min_one() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        // try delete when only one profile left (default has 1)
        assert_eq!(state.draft.profiles.len(), 1);
        // simulate delete logic: if len>1 remove, else error
        let to_delete = 0;
        if state.draft.profiles.len() > 1 {
            state.draft.profiles.remove(to_delete);
        } else {
            state.error = Some("Mindestens ein Profil muss vorhanden sein.".to_string());
        }
        assert_eq!(
            state.error,
            Some("Mindestens ein Profil muss vorhanden sein.".to_string())
        );
        // add second then delete
        state.draft.profiles.push(LanguageProfile {
            id: "second".to_string(),
            display_name: "Second".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![IdeConfig {
                id: "vscode".to_string(),
                display_name: "VS Code".to_string(),
                program: "code".to_string(),
                args: vec!["{file}".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,

                icon: None,
            }],
            default_ide_id: Some("vscode".to_string()),
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        });
        state.draft.profiles.remove(0);
        assert_eq!(state.draft.profiles.len(), 1);
        assert_eq!(state.draft.profiles[0].id, "second");
    }

    #[test]
    fn settings_delete_agent_requires_min_one() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        assert!(state.draft.agents.len() > 1);
        let before = state.draft.agents.len();
        state.draft.agents.remove(0);
        assert_eq!(state.draft.agents.len(), before - 1);
        // delete until one left
        while state.draft.agents.len() > 1 {
            state.draft.agents.remove(0);
        }
        assert_eq!(state.draft.agents.len(), 1);
        if state.draft.agents.len() > 1 {
            state.draft.agents.remove(0);
        } else {
            state.error = Some("Mindestens ein Agent muss vorhanden sein.".to_string());
        }
        assert!(state.error.is_some());
    }

    #[test]
    fn settings_duplicate_profile_agent() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        let p = state.draft.profiles[0].clone();
        let mut new_p = p.clone();
        new_p.id = format!("{}_copy", p.id);
        new_p.display_name = format!("{} (Kopie)", p.display_name);
        state.draft.profiles.push(new_p);
        assert_eq!(state.draft.profiles.len(), 2);
        assert_eq!(state.draft.profiles[1].id, "dotnet_copy");

        let a = state.draft.agents[0].clone();
        let mut new_a = a.clone();
        new_a.id = format!("{}_copy", a.id);
        state.draft.agents.push(new_a);
        assert_eq!(state.draft.agents.last().unwrap().id, "claude_copy");
    }

    #[test]
    fn settings_set_active_profile_agent() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.active_profile_id = "dotnet".to_string();
        // add second profile
        state.draft.profiles.push(LanguageProfile {
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
        state.draft.active_profile_id = "rust".to_string();
        assert_eq!(state.draft.active_profile_id, "rust");
        state.draft.active_agent_id = Some("codex".to_string());
        assert_eq!(state.draft.active_agent_id, Some("codex".to_string()));
    }

    #[test]
    fn settings_new_profile_quick_add() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.new_profile_name = "Rust".to_string();
        state.new_profile_ext = "rs".to_string();
        if !state.new_profile_name.trim().is_empty() && !state.new_profile_ext.trim().is_empty() {
            let mut ext = state.new_profile_ext.trim().to_string();
            if !ext.starts_with('.') {
                ext = format!(".{}", ext);
            }
            let id = state
                .new_profile_name
                .trim()
                .to_lowercase()
                .replace(' ', "_");
            state.draft.profiles.push(LanguageProfile {
                id: id.clone(),
                display_name: state.new_profile_name.trim().to_string(),
                file_extension: ext.clone(),
                file_pattern: None,
                max_scan_depth: 3,
                ides: vec![IdeConfig {
                    id: "vscode".to_string(),
                    display_name: "VS Code".to_string(),
                    program: "code".to_string(),
                    args: vec!["{file}".to_string()],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,

                    icon: None,
                }],
                default_ide_id: Some("vscode".to_string()),
                ide_order: Vec::new(),
                hidden_ide_ids: Vec::new(),
                hidden_agent_ids: Vec::new(),
                agent_order: Vec::new(),
                show_shell: true,
                show_explorer: true,
                config_selectors: Vec::new(),
            });
            assert_eq!(ext, ".rs");
            assert_eq!(id, "rust");
        }
        assert!(state.draft.profiles.iter().any(|p| p.id == "rust"));
        // empty case
        let mut state2 = SettingsState::from_config(&cfg);
        state2.new_profile_name = "".to_string();
        state2.new_profile_ext = "".to_string();
        if state2.new_profile_name.trim().is_empty() || state2.new_profile_ext.trim().is_empty() {
            state2.error = Some("Name und Endung erforderlich".to_string());
        }
        assert!(state2.error.is_some());
    }

    #[test]
    fn settings_new_agent_quick_add() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.new_agent_name = "MyAgent".to_string();
        state.new_agent_program = "myprog".to_string();
        if !state.new_agent_name.trim().is_empty() && !state.new_agent_program.trim().is_empty() {
            let id = state.new_agent_name.trim().to_lowercase().replace(' ', "_");
            state.draft.agents.push(crate::config::AgentProfile {
                id: id.clone(),
                display_name: state.new_agent_name.trim().to_string(),
                program: state.new_agent_program.trim().to_string(),
                args: vec![],
                command: None,
                launch_mode: crate::config::AgentLaunchMode::Terminal,
                terminal_override: None,

                icon: None,
            });
            assert_eq!(id, "myagent");
        }
        assert!(state.draft.agents.iter().any(|a| a.id == "myagent"));
    }

    #[test]
    fn settings_ide_min_one_and_add() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        let profile = &mut state.draft.profiles[0];
        assert!(!profile.ides.is_empty());
        let before = profile.ides.len();
        profile.ides.push(IdeConfig {
            id: "newide".to_string(),
            display_name: "New IDE".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string()],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        });
        assert_eq!(profile.ides.len(), before + 1);
        // remove until one left should error
        while profile.ides.len() > 1 {
            profile.ides.remove(0);
        }
        assert_eq!(profile.ides.len(), 1);
        // simulate error when trying to remove last
        if profile.ides.len() > 1 {
            profile.ides.remove(0);
        } else {
            state.error = Some("Mindestens eine IDE pro Profil erforderlich".to_string());
        }
        assert!(state.error.is_some());
    }

    #[test]
    fn settings_ide_effective_preview() {
        let ide = IdeConfig {
            id: "vscode".to_string(),
            display_name: "VS Code".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string()],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_program(), "code");
        assert_eq!(ide.effective_args(), vec!["{file}"]);
    }

    #[test]
    fn settings_terminal_preference_switch_and_custom() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        state.draft.terminal.preference = TerminalPreference::WindowsTerminal;
        assert_eq!(
            state.draft.terminal.preference,
            TerminalPreference::WindowsTerminal
        );
        state.draft.terminal.preference = TerminalPreference::Custom("alacritty".to_string());
        if let TerminalPreference::Custom(c) = &state.draft.terminal.preference {
            assert_eq!(c, "alacritty");
        } else {
            panic!();
        }
        // switch back to Auto
        state.draft.terminal.preference = TerminalPreference::Auto;
        assert_eq!(state.draft.terminal.preference, TerminalPreference::Auto);
    }

    #[test]
    fn settings_theme_selected() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        for theme in crate::config::Theme::all() {
            state.draft.theme = theme.clone();
            assert_eq!(state.draft.theme, theme);
        }
        state.draft.theme = crate::config::Theme::Dark;
        assert_eq!(state.draft.theme.display_name(), "Dark");
    }

    #[test]
    fn settings_agent_toggle_and_empty_warning() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        // toggle off claude
        state.draft.toggle_agent_active("claude");
        assert!(!state.draft.is_agent_active("claude"));
        if state.draft.active_agent_ids.is_empty() {
            state.error =
                Some("Warnung: Kein Agent aktiv — es wird kein AI-Button angezeigt.".to_string());
        }
        assert!(state.error.is_some());
        // toggle on again
        state.draft.toggle_agent_active("claude");
        assert!(state.draft.is_agent_active("claude"));
        // all activate
        for a in state.draft.agents.clone() {
            if !state.draft.is_agent_active(&a.id) {
                state.draft.toggle_agent_active(&a.id);
            }
        }
        assert_eq!(state.draft.active_agent_ids.len(), state.draft.agents.len());
        // all deactivate
        state.draft.active_agent_ids.clear();
        state.draft.active_agent_id = None;
        assert!(state.draft.active_agent_ids.is_empty());
    }

    #[test]
    fn settings_delete_adjusts_selection() {
        let cfg = AppConfig::default();
        let mut state = SettingsState::from_config(&cfg);
        // add second profile
        state.draft.profiles.push(LanguageProfile {
            id: "second".to_string(),
            display_name: "Second".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![IdeConfig {
                id: "vscode".to_string(),
                display_name: "VS Code".to_string(),
                program: "code".to_string(),
                args: vec!["{file}".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,

                icon: None,
            }],
            default_ide_id: Some("vscode".to_string()),
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        });
        state.selected_profile_idx = Some(1);
        // delete idx 0, selection should adjust -1
        let idx = 0;
        state.draft.profiles.remove(idx);
        if let Some(sel) = state.selected_profile_idx {
            if sel > idx {
                state.selected_profile_idx = Some(sel - 1);
            }
        }
        assert_eq!(state.selected_profile_idx, Some(0));
    }

    #[test]
    fn settings_add_config_selector_to_profile() {
        let mut cfg = AppConfig::default();
        let p = &mut cfg.profiles[0];
        assert!(p.config_selectors.is_empty());
        p.config_selectors.push(crate::config::ConfigSelector {
            id: "db".into(),
            display_name: "Datenbank".into(),
            file_path: "App.config".into(),
            key: "Database".into(),
            key_attribute: "key".into(),
            value_attribute: "value".into(),
            kind: crate::config::XmlSelectorKind::AddKeyValue,
            options: vec![crate::config::ConfigOption {
                value: "dev".into(),
                label: "Dev".into(),
            }],
            allow_custom: false,
        });
        assert_eq!(p.config_selectors.len(), 1);
    }
}
