use crate::config::AppConfig;
use crate::git::RepoInfo;
use crate::i18n::{tr, tr_fmt, Language};
use egui::{Color32, RichText, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;

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
        // #6: Existenz nur einmal pro Pfad per stat prüfen (statt pro Repo pro
        // Frame). Icons sind quasi-statisch; Dateiwechsel greift ab Neustart.
        if icon_exists_cached(path) {
            let uri = format!("file://{}", path.replace('\\', "/"));
            return egui::Image::new(uri).fit_to_exact_size(Vec2::splat(16.0));
        }
    }
    egui::Image::new(ide_icon_for(&ide.id)).fit_to_exact_size(Vec2::splat(16.0))
}

fn agent_image(agent: &crate::config::AgentProfile) -> egui::Image<'static> {
    if let Some(path) = &agent.icon {
        // #6: siehe ide_image — gecachter stat statt Syscall pro Frame.
        if icon_exists_cached(path) {
            let uri = format!("file://{}", path.replace('\\', "/"));
            return egui::Image::new(uri).fit_to_exact_size(Vec2::splat(16.0));
        }
    }
    egui::Image::new(agent_icon_for(&agent.id)).fit_to_exact_size(Vec2::splat(16.0))
}

/// #6: gecachter `Path.exists` für Icon-Dateien. Ein stat-Syscall pro Pfad
/// pro Prozess statt pro Repo-Zeile pro Frame (~3000 stats/s bei 30fps).
/// Der erste Aufruf prüft das FS, danach gilt der Cache (Icons ändern sich
/// praktisch nie zur Laufzeit; Wechsel greift ab Neustart).
pub fn icon_exists_cached(path: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(&hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(path) {
        return hit;
    }
    let exists = std::path::Path::new(path).exists();
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.to_string(), exists);
    exists
}

/// #9: ROOT-DPI auflösen. Nur endliche positive Werte übernehmen — sonst
/// neutral 1.0. Nie auf die DPI des off-screen Service-Viewports
/// zurückfallen (PerMonitorV2 meldet dort ggf. eine andere DPI).
pub fn resolve_root_ppp(root_ppp: f32) -> f32 {
    if root_ppp.is_finite() && root_ppp > 0.0 {
        root_ppp
    } else {
        1.0
    }
}

/// #3: Badge-Text fürs Tray-Popup, wenn im Hauptfenster etwas auf den Nutzer
/// wartet. Priorität: Branch-Dialog (mit Repo-Name) vor allgemeinem Fehler.
/// `None` = kein Badge.
pub fn tray_notice_text(
    lang: Language,
    dialog_repo_name: Option<&str>,
    has_error: bool,
) -> Option<String> {
    if let Some(name) = dialog_repo_name {
        Some(tr_fmt(lang, "tray_notice_branch_dialog", &[name]))
    } else if has_error {
        Some(tr(lang, "tray_notice_error"))
    } else {
        None
    }
}

#[derive(Default)]
pub struct TrayPopupActions {
    pub branch_switch: Option<(std::path::PathBuf, String)>,
    pub solution_select: Option<(std::path::PathBuf, std::path::PathBuf)>,
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

/// Task 6: Tray-Branch-Limit, geclampet auf 5..50 (wie Settings/Config).
pub fn tray_branch_limit(limit: usize) -> usize {
    limit.clamp(5, 50)
}

/// Task 6: Rest-Hinweis unter der Tray-Branch-Liste. `None` wenn alles
/// Sichtbare angezeigt wird, sonst sprachabhängig via i18n
/// (`tray_branch_more_filtered` / `tray_branch_more_main`).
/// De-Texte bleiben `+ N weitere (filtere)` / `+ N weitere (in Hauptfenster)`.
pub fn tray_branch_overflow_hint(
    lang: Language,
    total_matching: usize,
    shown: usize,
    filter_active: bool,
) -> Option<String> {
    let remaining = total_matching.saturating_sub(shown);
    if remaining == 0 {
        return None;
    }
    let key = if filter_active {
        "tray_branch_more_filtered"
    } else {
        "tray_branch_more_main"
    };
    Some(tr_fmt(lang, key, &[&remaining.to_string()]))
}

/// Task 6 fix (Finding F): totale Trefferzahl im selben Durchlauf wie die
/// angezeigte Liste — gleiche Semantik wie `filter_branches` (leerer Filter =
/// alles, case-insensitiv), aber ohne zweiten Volldurchlauf mit
/// String-Clones nur zum Zählen. `shown == Prefix(total)` gilt weiterhin.
pub fn tray_branch_match_counts(
    branches: &[String],
    filter: &str,
    limit: usize,
) -> (Vec<String>, usize) {
    let filter_lower = filter.to_lowercase();
    let mut shown = Vec::new();
    let mut total = 0usize;
    for b in branches {
        if !filter_lower.is_empty() && !b.to_lowercase().contains(&filter_lower) {
            continue;
        }
        total += 1;
        if shown.len() < limit {
            shown.push(b.clone());
        }
    }
    (shown, total)
}

/// Max. Zeichen für Labels im Tray-Popup (Branch/Solution).
/// Das Popup ist 360px breit (~320px nutzbar); längere Texte würden die
/// ComboBox-Buttons und damit die Viewport-Breite über das Fenster treiben.
pub const TRAY_LABEL_MAX_CHARS: usize = 40;

/// Kürzt `s` char-safe auf max. `max_chars` Zeichen (inkl. Ellipse).
/// Kurze Texte kommen unverändert zurück, der volle Text gehört in den Tooltip.
pub fn truncate_label(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Entscheidet, ob das Tray-Popup nach den Actions geschlossen wird.
/// Branch-/Solution-Wechsel und Refresh halten das Popup bewusst offen,
/// damit mehrere Wechsel hintereinander möglich sind; alles andere
/// (explizites Close, Main/Settings/Quit, externe Launches) schließt.
pub fn should_close_tray_popup(a: &TrayPopupActions) -> bool {
    a.close_popup
        || a.open_main
        || a.open_settings
        || a.quit
        || a.ide_open.is_some()
        || a.agent_open.is_some()
        || a.explorer_open.is_some()
        || a.shell_open.is_some()
}

/// Shows the tray popup UI inside the given viewport Ui.
/// Returns actions triggered by the user.
/// `notice` (#3) zeigt optional ein Badge (wartender Branch-Dialog/Fehler);
/// Klick öffnet das Hauptfenster.
pub fn show_tray_popup_ui(
    ui: &mut egui::Ui,
    repos: &[&RepoInfo],
    config: &AppConfig,
    actions: &mut TrayPopupActions,
    notice: Option<&str>,
    branch_filters: &mut HashMap<PathBuf, String>,
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
                    // Order in code is reverse of visual (right_to_left): first added = rightmost
                    // Desired visual: Refresh (left) | Settings (middle) | Close (right)
                    // So code order: Close, Settings, Refresh
                    if ui
                        .add(
                            egui::Button::image(
                                egui::Image::new(ICON_CROSS)
                                    .fit_to_exact_size(Vec2::splat(12.0))
                                    .tint(ui.visuals().text_color()),
                            )
                            .small(),
                        )
                        .on_hover_text("Schließen (Esc)")
                        .clicked()
                    {
                        actions.close_popup = true;
                    }
                    let settings_btn = egui::Button::image(
                        egui::Image::new(ICON_GEAR)
                            .fit_to_exact_size(Vec2::splat(12.0))
                            .tint(ui.visuals().text_color()),
                    )
                    .small();
                    if ui
                        .add(settings_btn)
                        .on_hover_text("Settings öffnen")
                        .clicked()
                    {
                        actions.open_settings = true;
                        actions.open_main = true;
                    }
                    let refresh_btn = egui::Button::image(
                        egui::Image::new(ICON_REFRESH)
                            .fit_to_exact_size(Vec2::splat(12.0))
                            .tint(ui.visuals().text_color()),
                    )
                    .small();
                    if ui.add(refresh_btn).on_hover_text("Refresh").clicked() {
                        actions.refresh = true;
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
            // #3: Badge, wenn im Hauptfenster eine Entscheidung/Fehler wartet
            // (z.B. Branch-Dialog nach Tray-Switch auf dirty Repo).
            if let Some(text) = notice {
                ui.add_space(2.0);
                let banner = egui::Button::new(
                    RichText::new(format!("⚠ {text}"))
                        .size(11.0)
                        .color(Color32::from_rgb(140, 90, 10)),
                )
                .fill(Color32::from_rgb(255, 243, 220));
                if ui
                    .add_sized([ui.available_width(), 26.0], banner)
                    .on_hover_text(text)
                    .clicked()
                {
                    actions.open_main = true;
                }
                ui.add_space(2.0);
            }
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
            // #8: Profil-/Agenten-Listen einmal pro Frame auflösen statt pro
            // Repo-Zeile (× Repos × 30fps). Profile-Overrides sind selten —
            // Default-Listen wiederverwenden, nur bei Override neu rechnen.
            let default_profile = config.get_active_profile();
            let default_ides = default_profile.visible_ides();
            let default_agents =
                default_profile.filtered_agents(&config.agents, &config.active_agent_ids);
            let active_agents = config.get_active_agents();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(2.0);
                    for repo in repos.iter().copied() {
                        let profile = config.get_effective_profile_for_repo(&repo.path);
                        let owned_ides;
                        let owned_agents;
                        let (ides, agents): (
                            &[&crate::config::IdeConfig],
                            &[&crate::config::AgentProfile],
                        ) = if std::ptr::eq(profile, default_profile) {
                            (&default_ides, &default_agents)
                        } else {
                            owned_ides = profile.visible_ides();
                            owned_agents =
                                profile.filtered_agents(&config.agents, &config.active_agent_ids);
                            (&owned_ides, &owned_agents)
                        };
                        show_tray_repo_row(
                            ui,
                            TrayRepoRowParams {
                                repo,
                                config,
                                actions,
                                profile_ides: ides,
                                profile_agents: agents,
                                active_agents: &active_agents,
                                branch_filters,
                            },
                        );
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

// Task 6 fix (Finding 4): Params als Struct statt 8 Einzel-Args —
// entfernt `#[allow(clippy::too_many_arguments)]`, kein Lint-Maskieren.
// `actions` + `branch_filters` bleiben `&mut` (kein Split nötig,
// #8-Call-Site mit vorberechneten IDE/Agent-Slices bleibt erhalten).
pub struct TrayRepoRowParams<'a> {
    pub repo: &'a RepoInfo,
    pub config: &'a AppConfig,
    pub actions: &'a mut TrayPopupActions,
    pub profile_ides: &'a [&'a crate::config::IdeConfig],
    pub profile_agents: &'a [&'a crate::config::AgentProfile],
    pub active_agents: &'a [&'a crate::config::AgentProfile],
    pub branch_filters: &'a mut HashMap<PathBuf, String>,
}

fn show_tray_repo_row(ui: &mut egui::Ui, p: TrayRepoRowParams<'_>) {
    let TrayRepoRowParams {
        repo,
        config,
        actions,
        profile_ides,
        profile_agents,
        active_agents,
        branch_filters,
    } = p;
    let lang = config.language;
    let visuals = ui.visuals().clone();
    let frame = egui::Frame::new()
        .fill(visuals.widgets.inactive.bg_fill)
        .stroke(egui::Stroke::new(
            1.0,
            visuals.widgets.inactive.fg_stroke.color,
        ))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 6));

    frame.show(ui, |ui| {
        ui.vertical(|ui| {
            // Row 1: Folder + name + dirty (tight, no branch)
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(ICON_FOLDER)
                        .fit_to_exact_size(Vec2::splat(14.0))
                        .tint(ui.visuals().text_color()),
                );
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
                let limit = tray_branch_limit(config.tray_branch_limit);
                if !branches.is_empty() {
                    // Task 6: pro-Repo Filter oben in der ComboBox (Filter-State
                    // pro Repo-Pfad, geleert bei Close).
                    // Finding G: kein PathBuf-Clone pro Frame — nur lesend
                    // nachschlagen; Key-Clone nur bei tatsächlicher Änderung.
                    let mut filter_text =
                        branch_filters.get(&repo.path).cloned().unwrap_or_default();
                    let current = truncate_label(&repo.branch, TRAY_LABEL_MAX_CHARS);
                    let combo_width = ui.available_width();
                    egui::ComboBox::from_id_salt(("tray_branch", repo.path.clone()))
                        .selected_text(&current)
                        .width(combo_width)
                        .show_ui(ui, |ui| {
                            // Task 6 fix (Finding 3): ComboBox+TextEdit-Härtung.
                            // egui 0.36 hält das ComboBox-Popup bei inneren
                            // Interaktionen offen; TextEdit konsumiert Tasten,
                            // sodass Tippen das Dropdown nicht schließt. Das
                            // Hauptfenster nutzt Window+TextEdit (repo_list.rs:
                            // Branch-Popup mit Fetch-Button + Outside-Click/Esc),
                            // hier reicht die leichte ComboBox (kein Fetch-Button,
                            // Außen-Tray-Popup bleibt ohnehin offen). Fokus sofort
                            // anfordern, damit Tippen ohne Extraklick filtert.
                            let edit_width = ui.available_width();
                            let edit_resp = ui.add(
                                egui::TextEdit::singleline(&mut filter_text)
                                    .hint_text(tr(lang, "branch_search"))
                                    .desired_width(edit_width),
                            );
                            if !edit_resp.has_focus() && ui.ctx().memory(|m| m.focused().is_none())
                            {
                                edit_resp.request_focus();
                            }
                            ui.separator();
                            // Finding 1: totale Treffer via dieselbe
                            // filter_branches-Prädikat (MAX-Limit) — shown ist
                            // dadurch garantiert Prefix von total.
                            let (shown, total_matching) =
                                tray_branch_match_counts(branches, filter_text.as_str(), limit);
                            let filter_active = !filter_text.is_empty();
                            for b in &shown {
                                let is_sel = *b == repo.branch;
                                if ui
                                    .selectable_label(
                                        is_sel,
                                        truncate_label(b, TRAY_LABEL_MAX_CHARS),
                                    )
                                    .on_hover_text(b.as_str())
                                    .clicked()
                                    && *b != repo.branch
                                {
                                    // Popup bleibt bewusst offen (mehrere Wechsel möglich).
                                    actions.branch_switch = Some((repo.path.clone(), b.clone()));
                                }
                            }
                            if shown.is_empty() {
                                ui.label(
                                    RichText::new(tr(lang, "no_matches"))
                                        .size(10.0)
                                        .color(Color32::from_rgb(120, 120, 120))
                                        .italics(),
                                );
                            } else if let Some(hint) = tray_branch_overflow_hint(
                                lang,
                                total_matching,
                                shown.len(),
                                filter_active,
                            ) {
                                ui.separator();
                                ui.label(
                                    RichText::new(hint)
                                        .size(10.0)
                                        .color(Color32::from_rgb(120, 120, 120))
                                        .italics(),
                                );
                            }
                        })
                        .response
                        .on_hover_text(&repo.branch);
                    // Finding G: State zurückschreiben nur bei Änderung —
                    // Key-Clone nur dann, leere Filter sofort entfernen.
                    let stored = branch_filters.get(&repo.path);
                    if stored.map(|s| s.as_str()) != Some(filter_text.as_str()) {
                        if filter_text.is_empty() {
                            branch_filters.remove(&repo.path);
                        } else {
                            branch_filters.insert(repo.path.clone(), filter_text);
                        }
                    }
                } else {
                    ui.label(
                        RichText::new(truncate_label(&repo.branch, TRAY_LABEL_MAX_CHARS))
                            .size(10.0)
                            .color(Color32::from_rgb(100, 100, 100))
                            .italics(),
                    )
                    .on_hover_text(&repo.branch);
                }
            }

            ui.add_space(4.0);

            // Row 3: Solution dropdown (only for .NET / when multiple solutions) - own line under branch
            if repo.solutions.len() > 1 {
                let selected_full = repo
                    .selected_solution
                    .as_ref()
                    .and_then(|p| {
                        repo.solutions
                            .iter()
                            .find(|s| &s.path == p)
                            .map(|s| s.relative.clone())
                    })
                    .unwrap_or_else(|| "–".to_string());
                let selected_text = truncate_label(&selected_full, TRAY_LABEL_MAX_CHARS);
                let combo_width = ui.available_width();
                egui::ComboBox::from_id_salt(("tray_solution", repo.path.clone()))
                    .selected_text(selected_text)
                    .width(combo_width)
                    .show_ui(ui, |ui| {
                        for sol in &repo.solutions {
                            let is_sel = Some(&sol.path) == repo.selected_solution.as_ref();
                            if ui
                                .selectable_label(
                                    is_sel,
                                    truncate_label(&sol.relative, TRAY_LABEL_MAX_CHARS),
                                )
                                .on_hover_text(&sol.relative)
                                .clicked()
                                && Some(&sol.path) != repo.selected_solution.as_ref()
                            {
                                // Popup bleibt bewusst offen (mehrere Wechsel möglich).
                                actions.solution_select =
                                    Some((repo.path.clone(), sol.path.clone()));
                            }
                        }
                    })
                    .response
                    .on_hover_text(&selected_full);
                ui.add_space(4.0);
            }

            // Row 4: Tools tight spacing – respects TrayIconConfig (hidden & order)
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let tray_hidden = &config.tray_icons.hidden_icon_ids;
                let tray_order = &config.tray_icons.icon_order;
                let order_idx = |id: &str| {
                    tray_order
                        .iter()
                        .position(|x| x == id)
                        .unwrap_or(usize::MAX)
                };
                // IDEs: filter by tray hidden, sort by tray order, limit 3
                let mut tray_ides: Vec<&crate::config::IdeConfig> = profile_ides
                    .iter()
                    .copied()
                    .filter(|ide| !tray_hidden.contains(&ide.id))
                    .collect();
                if !tray_order.is_empty() {
                    tray_ides.sort_by_key(|ide| order_idx(&ide.id));
                }
                for ide in tray_ides.iter().take(3) {
                    let file_path = repo
                        .selected_solution
                        .clone()
                        .unwrap_or_else(|| repo.path.clone());
                    let btn = egui::Button::image(ide_image(ide)).small();
                    let resp = ui
                        .add(btn)
                        .on_hover_text(format!("In {} öffnen", ide.display_name));
                    if resp.clicked() {
                        actions.ide_open = Some((repo.path.clone(), ide.id.clone(), file_path));
                        actions.close_popup = true;
                    }
                }
                if tray_ides.is_empty() {
                    // Only show placeholder if not all hidden? Keep "Keine IDE" only if original visible was empty?
                    // If filtered all hidden, don't show placeholder to reflect hidden state
                    let any_visible_original = !profile_ides.is_empty();
                    if any_visible_original
                        && tray_hidden
                            .iter()
                            .any(|h| ["vscode", "vs2022", "rider"].contains(&h.as_str()))
                    {
                        // all tray hidden case – show subtle hint instead of "Keine IDE"
                        ui.label(
                            RichText::new("—")
                                .size(9.0)
                                .color(Color32::from_rgb(180, 180, 180)),
                        );
                    } else if !any_visible_original {
                        ui.label(
                            RichText::new("Keine IDE")
                                .size(9.0)
                                .color(Color32::from_rgb(140, 140, 140)),
                        );
                    } else {
                        ui.label(
                            RichText::new("—")
                                .size(9.0)
                                .color(Color32::from_rgb(180, 180, 180)),
                        );
                    }
                }

                // Folder / Terminal – respect hidden
                let show_folder = !tray_hidden.contains(&"folder".to_string());
                let show_terminal = !tray_hidden.contains(&"terminal".to_string());
                if show_folder || show_terminal {
                    ui.separator();
                }
                if show_folder
                    && ui
                        .add(
                            egui::Button::image(
                                egui::Image::new(ICON_FOLDER)
                                    .fit_to_exact_size(Vec2::splat(12.0))
                                    .tint(ui.visuals().text_color()),
                            )
                            .small(),
                        )
                        .on_hover_text("Im Explorer öffnen")
                        .clicked()
                {
                    actions.explorer_open = Some(repo.path.clone());
                    actions.close_popup = true;
                }
                if show_terminal
                    && ui
                        .add(
                            egui::Button::image(
                                egui::Image::new(ICON_TERMINAL)
                                    .fit_to_exact_size(Vec2::splat(12.0))
                                    .tint(ui.visuals().text_color()),
                            )
                            .small(),
                        )
                        .on_hover_text("Terminal öffnen")
                        .clicked()
                {
                    actions.shell_open = Some(repo.path.clone());
                    actions.close_popup = true;
                }

                // Agents: filter by tray hidden + profile hidden/order, sort by tray order
                let filtered_raw = profile_agents;
                let mut filtered: Vec<&crate::config::AgentProfile> = filtered_raw
                    .iter()
                    .copied()
                    .filter(|a| !tray_hidden.contains(&a.id))
                    .collect();
                if !tray_order.is_empty() {
                    filtered.sort_by_key(|a| order_idx(&a.id));
                }
                // Only show separator if there will be agent buttons or fallback
                let filtered_empty = filtered.is_empty();
                let fallback_needed = filtered_empty
                    && !active_agents.is_empty()
                    && !tray_hidden.contains(&active_agents[0].id);
                if !filtered_empty || fallback_needed {
                    ui.separator();
                }
                for agent in filtered.iter().take(2) {
                    let btn = egui::Button::image(agent_image(agent)).small();
                    let resp = ui
                        .add(btn)
                        .on_hover_text(format!("Agent {} starten", agent.display_name));
                    if resp.clicked() {
                        actions.agent_open = Some((repo.path.clone(), agent.id.clone()));
                        actions.close_popup = true;
                    }
                }
                if filtered_empty && !active_agents.is_empty() {
                    let first = &active_agents[0];
                    if !tray_hidden.contains(&first.id)
                        && ui
                            .add(egui::Button::image(agent_image(first)).small())
                            .on_hover_text(format!("Agent {} starten", first.display_name))
                            .clicked()
                    {
                        actions.agent_open = Some((repo.path.clone(), first.id.clone()));
                        actions.close_popup = true;
                    }
                }
            });
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

    // Clamp to screen bounds with 4px margin.
    // M3: safe-clamp — wenn popup > screen (min>max), würde clamp() panicken.
    // Dann via min/max-Swap nie panicken (Popup bleibt ggf. leicht off-screen,
    // aber kein Crash; praktisch nie: 360px vs. 1920px).
    fn safe_clamp(v: f32, lo: f32, hi: f32) -> f32 {
        v.clamp(lo.min(hi), lo.max(hi))
    }
    let margin = 4.0;
    x = safe_clamp(
        x,
        screen_rect.min.x + margin,
        screen_rect.max.x - popup_size.x - margin,
    );
    y = safe_clamp(
        y,
        screen_rect.min.y + margin,
        screen_rect.max.y - popup_size.y - margin,
    );

    egui::pos2(x, y)
}

/// MRU timestamp for a repo (max of last_opened/branch_switch/config_change).
pub fn mru_timestamp(config: &AppConfig, repo_path: &std::path::Path) -> u64 {
    config
        .repo_usage
        .get(&AppConfig::repo_state_key(repo_path))
        .map(|u| {
            u.last_opened
                .unwrap_or(0)
                .max(u.last_branch_switch.unwrap_or(0))
                .max(u.last_config_change.unwrap_or(0))
        })
        .unwrap_or(0)
}

const ROW_H: f32 = 66.0;
const ROW_H_DROPDOWN: f32 = 94.0;
const POPUP_CHROME: f32 = 90.0;
const POPUP_MIN_H: f32 = 280.0;
const POPUP_MAX_H: f32 = 560.0;
/// Höhe des Notice-Banners (#3: wartender Branch-Dialog/Fehler).
const POPUP_NOTICE_H: f32 = 36.0;

/// Popup-Höhe aus den sichtbaren Repos summiert (66px normal, 94px pro Repo
/// mit Solution-Dropdown + 90px Chrome). Exakt auch bei gemischten Zeilen.
/// Mit Notice-Banner (#3) kommen 36px dazu.
/// Generisch über Borrow, damit Tray pro Frame mit Refs (ohne Clone) rechnen kann.
pub fn popup_height_for_visible<T: std::borrow::Borrow<RepoInfo>>(
    visible_repos: &[T],
    notice: Option<&str>,
) -> f32 {
    let rows: f32 = visible_repos
        .iter()
        .map(|r| {
            if r.borrow().solutions.len() > 1 {
                ROW_H_DROPDOWN
            } else {
                ROW_H
            }
        })
        .sum();
    let chrome = POPUP_CHROME + notice.map(|_| POPUP_NOTICE_H).unwrap_or(0.0);
    (rows + chrome).clamp(POPUP_MIN_H, POPUP_MAX_H)
}

/// #4: MRU-sortierte Indizes der sichtbaren Top-N Repos. Timestamps werden
/// einmalig O(n) vorberechnet (je ein HashMap-Lookup + String-Alloc), danach
/// sortiert nur noch der Vergleich gecachter u64 — statt O(n log n)
/// `repo_state_key`-Allocs pro Frame.
pub fn sorted_visible_indices(repos: &[RepoInfo], config: &AppConfig, limit: usize) -> Vec<usize> {
    let timestamps: Vec<u64> = repos
        .iter()
        .map(|r| mru_timestamp(config, &r.path))
        .collect();
    let mut idx: Vec<usize> = (0..repos.len()).collect();
    // Stabile Sortierung: Gleichstand behält Originalreihenfolge.
    idx.sort_by(|&a, &b| timestamps[b].cmp(&timestamps[a]));
    idx.truncate(limit);
    idx
}

/// P1: Referenzen statt Full-Clones pro Frame — der Tray-Hot-Path (30fps)
/// darf keine Vec<String>-Branches/Solutions klonen. Caller hält Arc<Vec>.
pub fn sorted_visible_refs<'a>(
    repos: &'a [RepoInfo],
    config: &AppConfig,
    limit: usize,
) -> Vec<&'a RepoInfo> {
    sorted_visible_indices(repos, config, limit)
        .into_iter()
        .map(|i| &repos[i])
        .collect()
}

/// Returns only the visible Top-N repos, MRU-sorted (N1: avoids full Vec deep-clone
/// per frame; clones only what is actually displayed).
/// Hinweis (P1): produktiv `sorted_visible_refs` nutzen (kein Clone);
/// diese Clone-Variante nur für Tests/Compat.
pub fn sorted_visible_repos(repos: &[RepoInfo], config: &AppConfig, limit: usize) -> Vec<RepoInfo> {
    sorted_visible_indices(repos, config, limit)
        .into_iter()
        .map(|i| repos[i].clone())
        .collect()
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
    fn popup_oversized_never_panics() {
        // M3: popup > screen (min>max) darf nicht panicken.
        let tray = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(20.0, 20.0));
        let popup = egui::vec2(3000.0, 2000.0);
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));
        let pos = calculate_popup_position(tray, popup, screen);
        assert!(pos.x.is_finite() && pos.y.is_finite());
    }

    #[test]
    fn popup_near_right_edge_clamped() {
        let tray = egui::Rect::from_min_size(egui::pos2(1910.0, 1050.0), egui::vec2(20.0, 20.0));
        let popup = egui::vec2(360.0, 480.0);
        let screen = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));
        let pos = calculate_popup_position(tray, popup, screen);
        assert!(pos.x + popup.x <= 1920.0 - 4.0);
    }

    #[test]
    fn popup_height_sums_per_repo_mixed_dropdowns() {
        // F1-1/F4-5: früheres globales Bool überschätzte bei gemischten Zeilen.
        // 2 normale Zeilen (66) + 1 Dropdown-Zeile (94) + 90 Chrome = 316.
        use crate::git::{RepoInfo, SolutionFile};
        let mk = |n_sln: usize| {
            let mut r = RepoInfo::new(
                std::path::PathBuf::from(format!("/tmp/m{n_sln}")),
                "main".to_string(),
                false,
                false,
            );
            r.solutions = (0..n_sln)
                .map(|i| SolutionFile {
                    path: std::path::PathBuf::from(format!("/tmp/m{n_sln}/{i}.sln")),
                    relative: format!("{i}.sln"),
                })
                .collect();
            r
        };
        let repos = vec![mk(0), mk(1), mk(2)];
        assert_eq!(
            popup_height_for_visible(&repos, None),
            2.0 * 66.0 + 94.0 + 90.0
        );
        assert_eq!(
            popup_height_for_visible::<RepoInfo>(&[], None),
            280.0
        );
        // Homogen (Clamp beachten: 2 Zeilen lägen unter MIN 280):
        assert_eq!(
            popup_height_for_visible(&[mk(0), mk(1), mk(0), mk(1)], None),
            4.0 * 66.0 + 90.0
        );
        assert_eq!(
            popup_height_for_visible(&[mk(2), mk(3), mk(2), mk(3)], None),
            4.0 * 94.0 + 90.0
        );
        // Clamp bleibt: 50 Dropdown-Zeilen deckeln auf 560.
        let many: Vec<RepoInfo> = (0..50).map(|_| mk(2)).collect();
        assert_eq!(popup_height_for_visible(&many, None), 560.0);
    }

    #[test]
    fn sorted_visible_clones_only_top_n_mru_first() {
        use crate::config::AppConfig;
        use std::path::PathBuf;
        let mut cfg = AppConfig::default();
        let mk = |name: &str| {
            crate::git::RepoInfo::new(
                PathBuf::from(format!("/tmp/{name}")),
                "main".into(),
                false,
                false,
            )
        };
        let repos = vec![mk("a"), mk("b"), mk("c")];
        // b als zuletzt benutzt markieren -> muss zuerst kommen
        let key_b = AppConfig::repo_state_key(&PathBuf::from("/tmp/b"));
        cfg.repo_usage.entry(key_b).or_default().last_opened = Some(9999);
        let vis = sorted_visible_repos(&repos, &cfg, 2);
        assert_eq!(vis.len(), 2);
        assert_eq!(vis[0].path, PathBuf::from("/tmp/b"));
        // Nur Top-N geklont, nicht alles
        let vis_all = sorted_visible_repos(&repos, &cfg, 10);
        assert_eq!(vis_all.len(), 3);
    }

    #[test]
    fn branch_and_solution_keep_popup_open() {
        // Gewünscht: Branch-/Solution-Wechsel und Refresh schließen NICHT,
        // alles andere (Close, Main/Settings/Quit, externe Launches) schon.
        let mut a = TrayPopupActions::default();
        assert!(!should_close_tray_popup(&a));

        a.branch_switch = Some((std::path::PathBuf::from("/tmp/r"), "main".into()));
        assert!(!should_close_tray_popup(&a));

        let mut a = TrayPopupActions::default();
        a.solution_select = Some((
            std::path::PathBuf::from("/tmp/r"),
            std::path::PathBuf::from("/tmp/r/a.sln"),
        ));
        assert!(!should_close_tray_popup(&a));

        let mut a = TrayPopupActions::default();
        a.refresh = true;
        assert!(!should_close_tray_popup(&a));

        let mut a = TrayPopupActions::default();
        a.close_popup = true;
        assert!(should_close_tray_popup(&a));

        let mut a = TrayPopupActions::default();
        a.open_main = true;
        assert!(should_close_tray_popup(&a));

        let mut a = TrayPopupActions::default();
        a.ide_open = Some((
            std::path::PathBuf::from("/tmp/r"),
            "vscode".into(),
            std::path::PathBuf::from("/tmp/r/a.sln"),
        ));
        assert!(should_close_tray_popup(&a));
    }

    #[test]
    fn truncate_label_keeps_short_and_cuts_long_char_safe() {
        assert_eq!(truncate_label("main", 40), "main");
        let s40: String = "x".repeat(40);
        assert_eq!(truncate_label(&s40, 40), s40);
        // Lang: max Zeichen inkl. Ellipse, Anfang bleibt erhalten
        let long = "feature/sehr-langer-branch-name-der-das-popup-sprengen-wuerde";
        let cut = truncate_label(long, 40);
        assert_eq!(cut.chars().count(), 40);
        assert!(cut.ends_with('…'));
        assert!(long.starts_with(&cut[..cut.len() - '…'.len_utf8()]));
        // Unicode/Emoji: kein Panic, keine zerschnittenen Zeichen
        let uni = "feature/äöü-🚀-sehr-langer-name-mit-umlauten-und-emoji";
        let cut_uni = truncate_label(uni, 20);
        assert_eq!(cut_uni.chars().count(), 20);
        assert!(cut_uni.ends_with('…'));
        assert_eq!(truncate_label("", 40), "");
        assert_eq!(truncate_label("main", 0), "");
    }

    #[test]
    fn sorted_visible_indices_match_mru_order_with_ties() {
        // #4: Indizes müssen MRU-Reihenfolge (stabile Sortierung bei Gleichstand)
        // liefern, ohne pro Vergleich String-Allocs zu brauchen.
        use crate::config::AppConfig;
        use std::path::PathBuf;
        let mut cfg = AppConfig::default();
        let repos: Vec<crate::git::RepoInfo> = (0..60)
            .map(|i| {
                crate::git::RepoInfo::new(
                    PathBuf::from(format!("/tmp/r{i:02}")),
                    "main".into(),
                    false,
                    false,
                )
            })
            .collect();
        // Verteilte Timestamps inkl. Gleichstand (r05 und r42 beide 500).
        for (name, ts) in [
            ("r05", 500),
            ("r42", 500),
            ("r07", 900),
            ("r59", 100),
            ("r00", 700),
        ] {
            let key = AppConfig::repo_state_key(&PathBuf::from(format!("/tmp/{name}")));
            cfg.repo_usage.entry(key).or_default().last_opened = Some(ts);
        }
        let idx = sorted_visible_indices(&repos, &cfg, 10);
        assert_eq!(idx.len(), 10);
        // Referenz: naive stabile Sortierung nach Timestamp desc.
        let mut expected: Vec<usize> = (0..repos.len()).collect();
        expected.sort_by(|&a, &b| {
            mru_timestamp(&cfg, &repos[b].path).cmp(&mru_timestamp(&cfg, &repos[a].path))
        });
        let expected: Vec<usize> = expected.into_iter().take(10).collect();
        assert_eq!(idx, expected);
        // Höchster zuerst, Gleichstand stabil in Originalreihenfolge.
        assert_eq!(repos[idx[0]].path, PathBuf::from("/tmp/r07"));
        assert_eq!(repos[idx[1]].path, PathBuf::from("/tmp/r00"));
        assert_eq!(repos[idx[2]].path, PathBuf::from("/tmp/r05"));
        assert_eq!(repos[idx[3]].path, PathBuf::from("/tmp/r42"));
    }

    #[test]
    fn icon_exists_cached_reports_fs_and_caches() {
        // #6: Einmal geprüfte Pfade dürfen keinen erneuten stat-Syscall brauchen.
        let dir = std::env::temp_dir().join(format!(
            "gm_icon_cache_test_{}_{}",
            std::process::id(),
            "traypopup"
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let present = dir.join("present_xyz123.png");
        std::fs::write(&present, b"x").unwrap();
        let missing = dir.join("missing_xyz123.png");
        assert!(icon_exists_cached(present.to_str().unwrap()));
        assert!(!icon_exists_cached(missing.to_str().unwrap()));
        // Gecacht: nach Löschen weiterhin true (kein erneuter stat).
        std::fs::remove_file(&present).unwrap();
        assert!(icon_exists_cached(present.to_str().unwrap()));
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn resolve_root_ppp_prefers_valid_root() {
        // #9: Nur endliche positive Root-DPI übernehmen, sonst neutral 1.0
        // (nie die DPI des off-screen Service-Viewports).
        assert_eq!(resolve_root_ppp(1.0), 1.0);
        assert_eq!(resolve_root_ppp(2.0), 2.0);
        assert_eq!(resolve_root_ppp(0.0), 1.0);
        assert_eq!(resolve_root_ppp(-1.5), 1.0);
        assert_eq!(resolve_root_ppp(f32::NAN), 1.0);
        assert_eq!(resolve_root_ppp(f32::INFINITY), 1.0);
    }

    #[test]
    fn tray_notice_text_prioritizes_dialog_over_error() {
        // #3: Badge-Text für wartenden Branch-Dialog (mit Repo-Name) bzw. Fehler.
        use crate::i18n::Language;
        assert!(tray_notice_text(Language::En, None, false).is_none());
        assert!(tray_notice_text(Language::De, None, false).is_none());
        let d = tray_notice_text(Language::En, Some("myrepo"), false).unwrap();
        assert!(d.contains("myrepo"), "dialog notice names repo: {d}");
        let e = tray_notice_text(Language::De, None, true).unwrap();
        assert!(!e.is_empty());
        // Dialog schlägt Fehler.
        let both = tray_notice_text(Language::En, Some("myrepo"), true).unwrap();
        assert!(both.contains("myrepo"), "dialog wins over error: {both}");
    }

    #[test]
    fn tray_branch_limit_clamps_to_five_fifty() {
        // Task 6: tray_branch_limit clamp 5..50 bleibt erhalten.
        assert_eq!(tray_branch_limit(20), 20);
        assert_eq!(tray_branch_limit(5), 5);
        assert_eq!(tray_branch_limit(50), 50);
        assert_eq!(tray_branch_limit(0), 5);
        assert_eq!(tray_branch_limit(4), 5);
        assert_eq!(tray_branch_limit(51), 50);
        assert_eq!(tray_branch_limit(500), 50);
    }

    #[test]
    fn tray_branch_overflow_hint_reports_remaining() {
        // Task 6 fix (Finding 2): sprachabhängig via i18n, De-Texte identisch.
        use crate::i18n::Language;
        assert_eq!(tray_branch_overflow_hint(Language::De, 10, 10, false), None);
        assert_eq!(tray_branch_overflow_hint(Language::En, 10, 10, false), None);
        assert_eq!(tray_branch_overflow_hint(Language::De, 3, 5, true), None);
        assert_eq!(
            tray_branch_overflow_hint(Language::De, 25, 20, false).as_deref(),
            Some("+ 5 weitere (in Hauptfenster)")
        );
        assert_eq!(
            tray_branch_overflow_hint(Language::De, 25, 20, true).as_deref(),
            Some("+ 5 weitere (filtere)")
        );
        assert_eq!(
            tray_branch_overflow_hint(Language::En, 25, 20, false).as_deref(),
            Some("+ 5 more (in main window)")
        );
        assert_eq!(
            tray_branch_overflow_hint(Language::En, 25, 20, true).as_deref(),
            Some("+ 5 more (filter)")
        );
        assert_eq!(tray_branch_overflow_hint(Language::De, 0, 0, false), None);
        assert_eq!(tray_branch_overflow_hint(Language::En, 0, 0, true), None);
    }

    #[test]
    fn tray_branch_filter_reuses_filter_branches_with_clamped_limit() {
        // Task 6: Tray nutzt filter_branches() aus repo_list mit geclampetem Limit.
        use crate::ui::repo_list::filter_branches;
        let branches: Vec<String> = (0..60).map(|i| format!("feature/branch-{i:02}")).collect();
        let limit = tray_branch_limit(200);
        assert_eq!(limit, 50);
        let shown = filter_branches(&branches, "", limit);
        assert_eq!(shown.len(), 50);
        let filtered = filter_branches(&branches, "branch-05", limit);
        assert_eq!(filtered, vec!["feature/branch-05".to_string()]);
        assert!(filter_branches(&branches, "zzz-no-match", limit).is_empty());
    }

    #[test]
    fn tray_branch_match_counts_consistent_with_filter_branches() {
        // Task 6 fix (Finding 1): total via filter_branches(..., MAX),
        // shown == Prefix(total); no_matches-Pfad (total==0, shown leer).
        use crate::ui::repo_list::filter_branches;
        let branches: Vec<String> = (0..60).map(|i| format!("feature/branch-{i:02}")).collect();
        let limit = tray_branch_limit(20);
        // Ungefiltert: total 60, shown 20 == erste 20 von total.
        let (shown, total) = tray_branch_match_counts(&branches, "", limit);
        let expected_total = filter_branches(&branches, "", usize::MAX);
        assert_eq!(total, expected_total.len());
        assert_eq!(total, 60);
        assert_eq!(shown.len(), 20);
        assert_eq!(shown, expected_total[..20].to_vec());
        // Gefiltert: total via MAX-Limit, shown Prefix.
        let (shown_f, total_f) = tray_branch_match_counts(&branches, "branch-0", limit);
        let expected_f = filter_branches(&branches, "branch-0", usize::MAX);
        assert_eq!(total_f, expected_f.len());
        assert_eq!(shown_f, expected_f[..shown_f.len()].to_vec());
        // Case-insensitiv wie filter_branches (kein eigener contains-Pfad).
        let (shown_ci, total_ci) = tray_branch_match_counts(&branches, "BRANCH-05", limit);
        assert_eq!(total_ci, 1);
        assert_eq!(shown_ci, vec!["feature/branch-05".to_string()]);
        // No-matches: leer + total 0 → UI zeigt no_matches statt Hint.
        let (shown_none, total_none) = tray_branch_match_counts(&branches, "zzz-no-match", limit);
        assert!(shown_none.is_empty());
        assert_eq!(total_none, 0);
        assert_eq!(
            total_none,
            filter_branches(&branches, "zzz-no-match", usize::MAX).len()
        );
        // Finding F: Single-Pass zählt auch bei limit==0 alle Treffer,
        // zeigt aber nichts (konsistent mit filter_branches-Guard).
        let (shown_zero, total_zero) = tray_branch_match_counts(&branches, "", 0);
        assert!(shown_zero.is_empty());
        assert_eq!(total_zero, 60);
        use crate::i18n::Language;
        assert!(tray_branch_overflow_hint(Language::En, total_none, 0, true).is_none());
    }

    #[test]
    fn popup_height_grows_with_notice() {
        // #3: Banner braucht Platz — Höhe mit Notice größer als ohne.
        use crate::git::{RepoInfo, SolutionFile};
        let mk = |n_sln: usize| {
            let mut r = RepoInfo::new(
                std::path::PathBuf::from(format!("/tmp/n{n_sln}")),
                "main".to_string(),
                false,
                false,
            );
            r.solutions = (0..n_sln)
                .map(|i| SolutionFile {
                    path: std::path::PathBuf::from(format!("/tmp/n{n_sln}/{i}.sln")),
                    relative: format!("{i}.sln"),
                })
                .collect();
            r
        };
        let repos = vec![mk(0), mk(1), mk(0), mk(1)];
        // 4×66 + 90 = 354 (über MIN 280, unter MAX 560).
        let plain = popup_height_for_visible(&repos, None);
        assert_eq!(plain, 354.0);
        let with_notice = popup_height_for_visible(&repos, Some("waiting"));
        assert_eq!(with_notice, 390.0);
        assert!(
            with_notice > plain,
            "notice must grow popup: {plain} -> {with_notice}"
        );
        assert_eq!(popup_height_for_visible::<RepoInfo>(&[], None), 280.0);
    }
}
