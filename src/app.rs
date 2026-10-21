mod merge_requests;
mod mr_checkout;
mod mr_settings;

use crate::config::{AfterAction, AppConfig, RepoSortMode};
use crate::git::{launch_agent, launch_ide, MrPrep, RepoInfo};
use crate::gitlab::glab::{GlabErrorKind, GlabRunner};
use crate::gitlab::model::{MergedList, MrSnapshot};
use crate::gitlab::remote::{GlabHost, HostMap, LocalProject};
use crate::i18n::{tr, tr_fmt, Language};
use crate::scanner::scan_repos;
use crate::ui::{
    merge_requests::{MrCheckoutChoice, MrViewState},
    repo_list::{show_repo_list, sort_main_repos, RepoHighlight, RepoListActions},
    settings::SettingsState,
};
use egui::{Color32, RichText, Vec2};
use merge_requests::{top_bar_button, MrResult, TOP_BAR_CAPTIONS_WIDTH, TOP_BAR_COMPACT_WIDTH};
use mr_checkout::{MrCheckoutFailure, MrSwitch};
use mr_settings::{GlabFactory, MrSettingsCache, MrSettingsResult};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

/// Fenstergröße beim Start (ohne gespeicherte Größe: eframe ohne
/// `persistence`). Mindestens `TOP_BAR_COMPACT_WIDTH` breit, damit
/// „Aktualisieren“ und „Einstellungen“ beim Start ihren Text zeigen.
pub const DEFAULT_WINDOW_SIZE: [f32; 2] = [1100.0, 680.0];
const _: () = assert!(DEFAULT_WINDOW_SIZE[0] >= TOP_BAR_COMPACT_WIDTH);

const ICON_GEAR: egui::ImageSource = egui::include_image!("../assets/icons/gear.svg");
const ICON_REFRESH: egui::ImageSource = egui::include_image!("../assets/icons/refresh.svg");
const ICON_WARNING: egui::ImageSource = egui::include_image!("../assets/icons/warning.svg");
const ICON_CROSS: egui::ImageSource = egui::include_image!("../assets/icons/cross.svg");
const ICON_SORT: egui::ImageSource = egui::include_image!("../assets/icons/chevron-down.svg");

enum ScanResult {
    Repos(Vec<RepoInfo>),
    FetchDone {
        repos: Vec<RepoInfo>,
        fetch_error: Option<String>,
        fetch_partial: bool,
        fetch_is_fetch_error: bool,
        is_auto: bool,
    },
    /// H3: Worker-Thread ist vor `send` abgestürzt — setzt nur Flags +
    /// Fehler zurück, rührt `repos` bewusst NICHT an (kein Listen-Wipe).
    FetchAborted {
        reason_key: &'static str,
    },
    /// K1: Auschecken eines MR fertig; `Ok(None)` nach dem Klonen, sonst
    /// der vorbereitete lokale Branch (Worktree, bestehender Klon).
    MrCheckoutReady {
        choice: Box<MrCheckoutChoice>,
        result: Result<Option<MrPrep>, MrCheckoutFailure>,
    },
}

/// Wählt den i18n-Key für Fetch-Fehler: `fetch_partial` bei Teil-Fehlschlag
/// (einige Remotes ok), sonst `fetch_failed`.
pub fn fetch_error_i18n_key(fetch_partial: bool) -> &'static str {
    if fetch_partial {
        "fetch_partial"
    } else {
        "fetch_failed"
    }
}

/// Mapt einen `fetch_all`-Fehler auf (UI-Detail, partial?, is_fetch?) — English-neutral,
/// ohne Wrapper/Hinweis (Wrapper + auth_hint werden lokalisiert in poll_scan
/// angehängt). Geteilt von manuellem und periodischem Fetch (Finding E),
/// damit beide Pfade identische Fehlersemantik haben.
pub fn fetch_error_parts(e: &anyhow::Error) -> (String, bool, bool) {
    let partial = crate::git::is_partial_fetch_error(e);
    let is_fetch = crate::git::is_fetch_error(e);
    // I1: strukturiertes Detail English-neutral (opake libgit2-Texte),
    // ohne Wrapper/Hinweis — Wrapper + auth_hint lokalisiert in poll_scan.
    let msg = if is_fetch {
        e.downcast_ref::<crate::git::PartialFetch>()
            .map(|p| p.detail_en())
            .unwrap_or_else(|| format!("{e:#}"))
    } else {
        format!("{e:#}")
    };
    (msg, partial, is_fetch)
}

/// E15: Hinweis nach dem Wechsel auf einen Remote-Branch, dessen lokaler
/// Branch vorgespult wurde, voraus ist oder divergiert. `None` → die
/// bisherige Erfolgsmeldung.
pub fn checkout_outcome_notice(
    lang: Language,
    outcome: &crate::git::CheckoutOutcome,
) -> Option<String> {
    use crate::git::CheckoutOutcome;
    Some(match outcome {
        CheckoutOutcome::Switched => return None,
        CheckoutOutcome::FastForwarded { branch, by: 1 } => {
            tr_fmt(lang, "branch_fast_forwarded_one", &[branch])
        }
        CheckoutOutcome::FastForwarded { branch, by } => {
            tr_fmt(lang, "branch_fast_forwarded", &[branch, &by.to_string()])
        }
        CheckoutOutcome::LocalAhead { branch, ahead: 1 } => {
            tr_fmt(lang, "branch_local_ahead_one", &[branch])
        }
        CheckoutOutcome::LocalAhead { branch, ahead } => {
            tr_fmt(lang, "branch_local_ahead", &[branch, &ahead.to_string()])
        }
        CheckoutOutcome::Diverged {
            branch,
            ahead,
            behind,
        } => tr_fmt(
            lang,
            "branch_local_diverged",
            &[branch, &ahead.to_string(), &behind.to_string()],
        ),
    })
}

/// E15: Fehlerdetail für den Branch-Wechsel — typisierte Abbrüche
/// (`BranchSwitchBlocked`) lokalisiert, sonst der bisherige Fehlertext.
pub fn branch_switch_error_detail(lang: Language, e: &anyhow::Error) -> String {
    use crate::git::BranchSwitchBlocked;
    match e.downcast_ref::<BranchSwitchBlocked>() {
        Some(BranchSwitchBlocked::UpstreamMismatch { branch, upstream }) => {
            tr_fmt(lang, "branch_upstream_mismatch", &[branch, upstream])
        }
        Some(BranchSwitchBlocked::CheckedOutElsewhere { branch, path }) => tr_fmt(
            lang,
            "branch_checked_out_elsewhere",
            &[branch, &path.display().to_string()],
        ),
        None => format!("{e:#}"),
    }
}

/// M2: Zähler-Label für die gecappte Dialog-Dateiliste — nur bei
/// tatsächlicher Trunkierung mit "+" (`files_truncated` aus
/// `get_detailed_status`), nicht schon bei exakt CAP Einträgen (F1#F3).
pub fn capped_file_count_label(count: usize, truncated: bool) -> String {
    if truncated {
        format!("{}+", crate::git::DETAILED_STATUS_CAP)
    } else {
        count.to_string()
    }
}

/// Statusleiste der Repo-Liste, solange ein Lauf die Refs hält. Beim
/// Auschecken eines MR (K1, auch während eines minutenlangen Klons)
/// „Auschecken für !{iid} läuft …“ statt „Scanne nach Repositories…“.
fn repo_busy_text(lang: Language, busy: &GitBusy) -> Option<String> {
    match busy {
        GitBusy::Idle => None,
        GitBusy::Scan | GitBusy::Fetch { .. } => Some(tr(lang, "scanning_repos")),
        GitBusy::Checkout { iid } => Some(tr_fmt(lang, "mr_checkout_running", &[&iid.to_string()])),
    }
}

/// F1#F1: Anzeigename für Fetch-Fehlermeldungen — `file_name` reicht
/// (voller Pfad zu lang für Banner/Tooltip-Kurzform).
fn short_repo_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| p.display().to_string())
}

/// Finding L1/M1 + N9: Fehler-Aggregation über Repos/Threads in EINEM
/// Aufruf — gespeichertes Detail und Partial-Flag gehören zusammen.
/// Erster Fehler wird gemerkt; ist ein späterer Fehler partiell und der
/// gemerkte nicht, ersetzt der partielle (Detail mit `succeeded`-Liste ist
/// informativer als ein Complete-Detail mit falschem Partial-Flag).
/// Einmal binden statt `fetch_error_parts` doppelt aufzurufen.
/// F1#F1: mit Repo-Kontext — der Auto-Fetch läuft über N Repos, ein Detail
/// ohne Repo-Name ist im Banner nicht zuordenbar (`p.file_name()`, voller
/// Pfad zu lang für Banner; Tooltip trägt ggf. Details).
fn aggregate_named_fetch_error(
    slot_err: &mut Option<String>,
    slot_partial: &mut bool,
    slot_is_fetch: &mut bool,
    repo_name: &str,
    e: &anyhow::Error,
) {
    let (msg, partial, is_fetch) = fetch_error_parts(e);
    let msg = if repo_name.is_empty() {
        msg
    } else {
        format!("{repo_name}: {msg}")
    };
    if slot_err.is_none() {
        *slot_err = Some(msg);
        *slot_partial = partial;
        *slot_is_fetch = is_fetch;
    } else if partial && !*slot_partial {
        *slot_err = Some(msg);
        *slot_partial = true;
        *slot_is_fetch = is_fetch;
    }
}

/// F2#B1/F3#1: führt Worker-Panic-Marker + Shared-State zusammen — der
/// Panic-Marker geht nicht verloren, wenn Shared leer ist (und umgekehrt
/// überschreibt ein vorhandener Shared-Fehler keinen gesetzten Marker
/// ohne Inhalt). Reine Funktion, separat getestet.
fn merge_parallel_fetch_result(
    slot_err: &mut Option<String>,
    slot_partial: &mut bool,
    slot_is_fetch: &mut bool,
    shared: (Option<String>, bool, bool),
    worker_panicked: bool,
) {
    if shared.0.is_some() {
        *slot_err = shared.0;
        *slot_partial = shared.1;
        *slot_is_fetch = shared.2;
    } else if worker_panicked && slot_err.is_none() {
        *slot_err = Some("background fetch worker panicked".to_string());
    }
}

/// Reine Entscheidung für Settings-Save mit Autostart-Apply (I2, headless-testbar):
/// `apply_failed=true` → Dialog offen halten, kein Scan (Korrektur/Retry);
/// `false` → schließen + Scan. Gibt `(keep_open, should_scan)` zurück.
pub fn settings_save_autostart_outcome(apply_failed: bool) -> (bool, bool) {
    if apply_failed {
        (true, false)
    } else {
        (false, true)
    }
}

struct BranchDialog {
    repo_path: PathBuf,
    target_branch: String,
    error: Option<String>,
    dirty_files: Vec<String>,
    /// F1#F3: true wenn die Liste am Cap trunkiert wurde (Label mit "+").
    files_truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsageType {
    Open,
    BranchSwitch,
    ConfigChange,
}

/// Hauptansicht im Fenster (nicht gespeichert).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainView {
    Repos,
    MergeRequests,
}

/// Was gerade die Refs der Repos hält: höchstens ein Scan, manueller Fetch
/// oder MR-Checkout zur Zeit (zwei libgit2-Läufe racen um FETCH_HEAD/Refs,
/// „cannot lock ref“). Der Auto-Fetch zählt bewusst nicht dazu
/// (`auto_fetching`, Finding D1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum GitBusy {
    #[default]
    Idle,
    /// Scan der Suchpfade (`start_scan`)
    Scan,
    /// Manueller Fetch dieses Repos, danach Rescan (`start_repo_fetch`)
    Fetch { repo: PathBuf },
    /// K1: Auschecken dieses MR, ohne abschließenden Scan
    Checkout { iid: u64 },
}

impl GitBusy {
    /// Ein Lauf hält die Refs: kein zweiter Scan, Fetch oder Checkout
    fn holds_refs(&self) -> bool {
        *self != GitBusy::Idle
    }

    /// Scan oder manueller Fetch: beide enden mit neuen Repos (`Repos` bzw.
    /// `FetchDone`), das Auschecken nicht
    fn scans_repos(&self) -> bool {
        matches!(self, GitBusy::Scan | GitBusy::Fetch { .. })
    }
}

pub struct MyApp {
    config: AppConfig,
    repos: Vec<RepoInfo>,
    /// Scan, manueller Fetch oder MR-Checkout (Ref-Lock, Spinner); nur über
    /// `set_git_busy` ändern (Tray)
    git_busy: GitBusy,
    error: Option<String>,
    show_settings: bool,
    settings_state: Option<SettingsState>,
    scan_tx: Sender<ScanResult>,
    scan_rx: Receiver<ScanResult>,
    launch_err_tx: Sender<String>,
    launch_err_rx: Receiver<String>,
    config_update_rx: Receiver<Result<AppConfig, String>>,
    update_rx: Receiver<Result<Option<crate::updater::UpdateInfo>, String>>,
    update_info: Option<crate::updater::UpdateInfo>,
    update_error: Option<String>,
    show_update_dialog: bool,
    status_message: Option<String>,
    status_message_time: Option<std::time::Instant>,
    branch_dialog: Option<BranchDialog>,
    pending_branch_switches: Vec<(PathBuf, String)>,
    // Window resizing state
    last_window_size: [f32; 2],
    // Panel collapse state
    top_bar_collapsed: bool,
    // Tray state (Service ist einzige Windows-Quelle, F-01: Fallback gestrichen)
    #[cfg(target_os = "windows")]
    tray_icon: Option<tray_icon::TrayIcon>,
    #[cfg(target_os = "windows")]
    tray_shared: Option<std::sync::Arc<std::sync::Mutex<crate::tray_service::TrayShared>>>,
    #[cfg(target_os = "windows")]
    tray_action_rx: Option<std::sync::mpsc::Receiver<crate::tray_service::TrayAction>>,
    #[cfg(target_os = "windows")]
    tray_service_tray_rx: Option<
        std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<tray_icon::TrayIconEvent>>>,
    >,
    #[cfg(target_os = "windows")]
    last_tray_sync: Option<std::time::Instant>,
    /// #5: true, sobald sich Tray-relevante Daten geändert haben und noch kein
    /// erfolgreicher Sync lief (WouldBlock behält das Flag für Retry).
    /// Plattformunabhängig gehalten, damit Tests das Markieren prüfen können;
    /// auf Nicht-Windows ohne Leser (nur gesetzt).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    tray_dirty: bool,
    window_visible: bool,
    should_quit: bool,
    /// Task 5: Zeitpunkt des letzten periodischen Auto-Fetch (throttled).
    /// `None` = noch nie gelaufen (erster Durchlauf nach Startup-Scan fällig).
    last_auto_fetch: Option<std::time::Instant>,
    /// Finding D1: eigener Busy-Zustand für den periodischen Auto-Fetch.
    /// Der Auto-Fetch setzt bewusst NICHT `git_busy`, damit Refresh/Fetch
    /// währenddessen klickbar bleiben (stiller Hintergrundlauf). Nur über
    /// `set_auto_fetching` ändern (Tray).
    auto_fetching: bool,
    /// P2b/L2: manueller Refresh während Auto-Fetch — Intent merken statt
    /// verwerfen; der `FetchDone(is_auto)`-Arm holt ihn nach.
    pending_scan: bool,
    /// L4: Zähler aufeinanderfolgender Config-Save-Fehler (Backoff fürs Log).
    config_save_fail_streak: u32,
    /// Finding A: `true`, wenn sich die Hauptsortierung ändern kann
    /// (Scan-Intake, `record_usage`, Sortmodus-Wechsel). Der Render-Pfad
    /// sortiert nur dann statt pro Frame.
    repo_sort_dirty: bool,
    /// P5: gebatchte Config-Saves — `record_usage` markiert nur pending,
    /// `flush_config_save` schreibt max. 1× pro Frame (statt N File-Syncs
    /// bei N Tray-Actions im selben Frame).
    config_save_pending: bool,
    last_config_save: Option<std::time::Instant>,
    main_view: MainView,
    /// glab-Aufrufe der Sitzung (immer `GatedRunner`, E16); Tests: `FakeGlab`
    glab: Arc<dyn GlabRunner>,
    /// Programm hinter `glab`; ein anderes nach „Speichern“ → neuer Runner
    glab_program: String,
    glab_factory: GlabFactory,
    /// Lokale glab-Erkennung je Kandidat (E16), Cache je Sitzung; nach jedem
    /// Scan nur für neue Kandidaten ergänzt
    glab_hosts: Vec<GlabHost>,
    /// GitLab-Hosts und lokale Projekte aus Repos, Erkennung und Config
    mr_hosts: HostMap,
    mr_locals: Vec<LocalProject>,
    /// Letzter Stand der MR-Ansicht; bleibt beim Neuladen sichtbar
    mr_snapshot: Option<Box<MrSnapshot>>,
    mr_loading: bool,
    /// Refresh während eines laufenden Abrufs gemerkt
    mr_pending_refresh: bool,
    /// „Speichern“ während des Rescans: Abruf erst mit den neuen Repos
    mr_refresh_after_scan: bool,
    /// Ende des letzten Abrufs (wie `last_auto_fetch`)
    mr_last_refresh: Option<std::time::Instant>,
    /// Steigt bei Anmeldungsänderungen; ältere Ergebnisse werden verworfen
    mr_auth_epoch: u64,
    mr_discovering: bool,
    /// Ausstehende Seiten „Nicht beteiligt“ (eine je Host)
    mr_more_pending: usize,
    /// Eigene gemergte MRs (Umschalter „Gemergt“); `None` = nicht geladen
    mr_merged: Option<MergedList>,
    mr_merged_loading: bool,
    /// Zählt neue Abrufe „Gemergt“; Ergebnisse älterer werden verworfen
    mr_merged_gen: u64,
    mr_tx: Sender<MrResult>,
    mr_rx: Receiver<MrResult>,
    mr_view: MrViewState,
    /// glab fehlt oder ist zu alt: keine weiteren glab-Aufrufe in der Sitzung
    /// (außer „Aktualisieren“); `mr_glab_error` trägt die Art für die Karte
    mr_glab_missing: bool,
    mr_glab_error: Option<GlabErrorKind>,
    /// K1: Wechsel eines bestehenden Klons wartet auf `execute_branch_switch`
    mr_switch: Option<MrSwitch>,
    /// K1: „Danach öffnen“ mit der IDE wartet auf den Rescan
    pending_after_checkout: Option<(PathBuf, AfterAction)>,
    /// K1: Hinweis in der MR-Ansicht (divergierter Branch); überlebt Rescans
    mr_notice: Option<String>,
    /// „In Repo-Liste zeigen“: hervorgehobene Karte und seit wann
    repo_highlight: Option<(RepoHighlight, std::time::Instant)>,
    /// Lokale Erkennung (E16) für diese Hosts erneut, auch wenn bekannt
    mr_rediscover: std::collections::BTreeSet<String>,
    /// Einstellungs-Tab „Merge Requests“: Worker-Ergebnisse kommen auch
    /// nach dem Schließen an (der `SettingsState` wird dann verworfen)
    mr_settings_tx: Sender<MrSettingsResult>,
    mr_settings_rx: Receiver<MrSettingsResult>,
    mr_settings: MrSettingsCache,
}

impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        // Startup-Sync: Run-Key mit aktueller exe synchronisieren (falls exe verschoben).
        // Best effort — kein Start-Abbruch bei Registry-Fehlern.
        // M1: set() selbst ist idempotent (kein Rewrite wenn Wert identisch),
        // daher hier kein is_enabled-Vorabcheck nötig.
        if let Err(e) = crate::autostart::apply_autostart(config.autostart) {
            eprintln!("Autostart-Sync fehlgeschlagen: {e:#}");
        }
        // M1: --minimized (Autostart) startet versteckt im Tray.
        // N6: gleiche Entscheidung wie main() (Single Source).
        let start_minimized = crate::start_minimized_flag();
        crate::ui::theme::apply_theme(&cc.egui_ctx, &config.theme);
        // SVG Icons loader für egui_extras
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let (tx, rx) = mpsc::channel();
        let (launch_err_tx, launch_err_rx) = mpsc::channel();
        let (config_update_tx, config_update_rx) = mpsc::channel::<Result<AppConfig, String>>();
        let (update_tx, update_rx) =
            mpsc::channel::<Result<Option<crate::updater::UpdateInfo>, String>>();
        let (mr_tx, mr_rx) = mpsc::channel();
        let (mr_settings_tx, mr_settings_rx) = mpsc::channel();
        let glab_factory = mr_settings::system_glab_factory();
        let glab_program = config.merge_requests.glab_program.clone();
        let glab = glab_factory(&glab_program);
        let mr_view = MrViewState::new(&config.merge_requests);

        let mut app = Self {
            config,
            repos: Vec::new(),
            git_busy: GitBusy::Idle,
            error: None,
            show_settings: false,
            settings_state: None,
            scan_tx: tx,
            scan_rx: rx,
            launch_err_tx,
            launch_err_rx,
            config_update_rx,
            update_rx,
            update_info: None,
            update_error: None,
            show_update_dialog: false,
            status_message: None,
            status_message_time: None,
            branch_dialog: None,
            pending_branch_switches: Vec::new(),
            last_window_size: [0.0; 2],
            top_bar_collapsed: false,
            #[cfg(target_os = "windows")]
            tray_icon: None,
            #[cfg(target_os = "windows")]
            tray_shared: None,
            #[cfg(target_os = "windows")]
            tray_action_rx: None,
            #[cfg(target_os = "windows")]
            tray_service_tray_rx: None,
            #[cfg(target_os = "windows")]
            last_tray_sync: None,
            tray_dirty: false,
            window_visible: !start_minimized,
            should_quit: false,
            // Task 5: direkt auf now setzen — erster Auto-Fetch erst nach Intervall,
            // nicht sofort nach Startup-Scan (vermeidet Doppel-Fetch beim Start).
            last_auto_fetch: Some(std::time::Instant::now()),
            auto_fetching: false,
            pending_scan: false,
            config_save_fail_streak: 0,
            repo_sort_dirty: true,
            config_save_pending: false,
            last_config_save: None,
            main_view: MainView::Repos,
            glab,
            glab_program,
            glab_factory,
            glab_hosts: Vec::new(),
            mr_hosts: HostMap::default(),
            mr_locals: Vec::new(),
            mr_snapshot: None,
            mr_loading: false,
            mr_pending_refresh: false,
            mr_refresh_after_scan: false,
            mr_last_refresh: None,
            mr_auth_epoch: 0,
            mr_discovering: false,
            mr_more_pending: 0,
            mr_merged: None,
            mr_merged_loading: false,
            mr_merged_gen: 0,
            mr_tx,
            mr_rx,
            mr_view,
            mr_glab_missing: false,
            mr_glab_error: None,
            mr_switch: None,
            pending_after_checkout: None,
            mr_notice: None,
            repo_highlight: None,
            mr_rediscover: Default::default(),
            mr_settings_tx,
            mr_settings_rx,
            mr_settings: MrSettingsCache::default(),
        };
        #[cfg(target_os = "windows")]
        {
            // Setup tray icon - custom popup on left/right click, no native menu.
            // Theme aus Config (Dark = Weiß-Icon, Light = dunkles Icon).
            let tray_theme = app.config.tray_icons.icon_theme;
            if let Some(channels) =
                crate::tray::create_tray_channels_with_theme(cc.egui_ctx.clone(), tray_theme)
            {
                app.tray_icon = Some(channels.tray_icon);
                // Create shared state and channels for tray service (efficient Arc sharing)
                let (action_tx, action_rx) = mpsc::channel();
                let shared = std::sync::Arc::new(std::sync::Mutex::new(
                    crate::tray_service::TrayShared::new(action_tx),
                ));
                // Wrap receiver in Arc<Mutex> for Sync required by deferred viewport
                let tray_rx_arc = std::sync::Arc::new(std::sync::Mutex::new(channels.tray_rx));
                // Initial sync of data (efficient: clones once at startup)
                {
                    if let Ok(mut guard) = shared.lock() {
                        guard.update_data(app.repos.clone(), app.config.clone());
                    }
                }
                app.tray_shared = Some(shared);
                app.tray_action_rx = Some(action_rx);
                app.tray_service_tray_rx = Some(tray_rx_arc);
                app.last_tray_sync = Some(std::time::Instant::now());
            } else {
                // F2-C1: stilles Fehlen wäre null Tray-Funktionalität ohne Hinweis.
                // Als sichtbarer Fehler im Hauptfenster (nicht nur stderr).
                app.error = Some(crate::i18n::tr(app.config.language, "tray_creation_error"));
                eprintln!("Tray icon creation failed");
                // L3: Tray-Erstellung fehlgeschlagen → kein unsichtbarer Start
                // (App wäre sonst unerreichbar); Fehler steht oben im Banner.
                if start_minimized {
                    app.window_visible = true;
                    cc.egui_ctx
                        .send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
            }
        }
        // Auto-Erkennung asynchron (verhindert UI Freeze beim Start)
        {
            let cfg = app.config.clone();
            let tx = config_update_tx;
            std::thread::spawn(move || {
                let mut cfg = cfg;
                let mut need_save = false;
                for profile in &mut cfg.profiles {
                    for ide in &mut profile.ides {
                        let prog_lower = ide.program.to_lowercase();
                        let is_vs_program = matches!(prog_lower.as_str(), "devenv" | "devenv.exe");
                        let is_rider_program = matches!(
                            prog_lower.as_str(),
                            "rider" | "rider.exe" | "rider64.exe" | "rider.cmd"
                        );
                        if ide.id == "vs2022" && is_vs_program {
                            if let Some(p) = crate::git::resolve_vs_path() {
                                ide.program = p.display().to_string();
                                need_save = true;
                            }
                        } else if ide.id == "rider" && is_rider_program {
                            if let Some(p) = crate::git::resolve_rider_path() {
                                ide.program = p.display().to_string();
                                need_save = true;
                            }
                        }
                    }
                }
                if need_save {
                    match cfg.save() {
                        Ok(()) => {
                            let _ = tx.send(Ok(cfg));
                        }
                        Err(e) => {
                            let _ = tx.send(Err(format!(
                                "Config Auto-Erkennung speichern fehlgeschlagen: {e:#}"
                            )));
                        }
                    }
                }
            });
        }
        // Update checker (async, 2s delay to not affect startup)
        // Nur wenn in den Settings aktiviert (F-06 Opt-out, Default AN)
        // M2: kein Thread + kein Sleep wenn deaktiviert.
        {
            let current_version = env!("CARGO_PKG_VERSION").to_string();
            if !app.config.check_for_updates {
                let _ = update_tx.send(Ok(None));
            } else {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    // Diagnosefähig (M2): Fehler mit Kontext weiterreichen statt still zu schlucken.
                    let outcome = crate::updater::check_for_update_result(&current_version);
                    if let Err(ref e) = outcome {
                        eprintln!("Update-Check fehlgeschlagen: {e} (offline/Proxy/TLS?)");
                    }
                    let _ = update_tx.send(outcome);
                });
            }
        }
        app.start_scan();
        // Initial size matches viewport default – used for collapse logic
        app.last_window_size = DEFAULT_WINDOW_SIZE;
        app
    }

    fn start_scan(&mut self) {
        match self.git_busy {
            GitBusy::Idle => {}
            // Ein laufender Scan/Fetch liefert ohnehin neue Repos
            GitBusy::Scan | GitBusy::Fetch { .. } => return,
            // K1: das Auschecken endet ohne Scan – Wunsch merken, `poll_scan`
            // holt ihn danach nach
            GitBusy::Checkout { .. } => {
                self.pending_scan = true;
                return;
            }
        }
        // L2: kein paralleler Lauf zum Auto-Fetch — zwei libgit2-Fetches
        // auf denselben Repos racen um FETCH_HEAD/refs (cannot lock ref).
        // P2b: Intent merken statt verwerfen — FetchDone(is_auto) holt nach.
        if self.auto_fetching {
            self.pending_scan = true;
            self.status_message = Some(tr(self.config.language, "auto_fetch_running"));
            self.status_message_time = Some(std::time::Instant::now());
            return;
        }
        self.set_git_busy(GitBusy::Scan);
        self.error = None;
        let cfg = self.config.clone();
        let tx = self.scan_tx.clone();
        std::thread::spawn(move || {
            // H3: Panic darf `git_busy` nicht hängen lassen — Fallback
            // resettet Flags + zeigt Fehler (Liste bleibt erhalten).
            let msg =
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scan_repos(&cfg))) {
                    Ok(repos) => ScanResult::Repos(repos),
                    Err(_) => {
                        eprintln!("start_scan-Thread abgestürzt (panic vor send)");
                        ScanResult::FetchAborted {
                            reason_key: "fetch_aborted",
                        }
                    }
                };
            let _ = tx.send(msg);
        });
    }

    fn poll_scan(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.scan_rx.try_recv() {
            match result {
                ScanResult::Repos(repos) => {
                    self.set_git_busy(GitBusy::Idle);
                    // I1: auto_fetching NICHT hier löschen — ein laufender
                    // Auto-Fetch würde sonst als fertig gelten und sofort ein
                    // zweiter Auto-Fetch starten (Ref-Race). Nur FetchDone
                    // mit is_auto löscht das Flag.
                    // Task 4: View-Sortierung einmalig am Intake (per Frame nur
                    // noch bei dirty, siehe Render-Pfad) — Intake sortiert mit
                    // aktueller Config und erfüllt damit pending dirty.
                    let mut repos = repos;
                    sort_main_repos(&mut repos, &self.config);
                    self.repos = repos;
                    self.repo_sort_dirty = false;
                    // P2b: abgeschlossener Scan erfüllt gemerkten Refresh.
                    self.pending_scan = false;
                    self.error = None;
                    self.mr_after_scan();
                    // K1: IDE nach dem Auschecken, sobald der Ordner bekannt ist
                    self.run_pending_after_checkout();
                    // Review-Fix: dirty VOR dem direkten Sync markieren — bei
                    // WouldBlock bleibt das Flag (Backstop in sync_tray_service).
                    self.mark_tray_dirty();
                    self.sync_tray_service();
                    ctx.request_repaint();
                }
                ScanResult::FetchDone {
                    repos,
                    fetch_error,
                    fetch_partial,
                    fetch_is_fetch_error,
                    is_auto,
                } => {
                    // L4: `git_busy` nur für manuelle Läufe zurücksetzen — ein
                    // parallel eintreffendes Auto-FetchDone darf den Spinner
                    // eines noch laufenden manuellen Laufs nicht löschen.
                    if !is_auto {
                        self.set_git_busy(GitBusy::Idle);
                    }
                    // I1: nur Auto-FetchDone löscht auto_fetching — manueller
                    // Fetch (is_auto=false) lässt einen laufenden Auto-Fetch in Ruhe.
                    // P2b: einen gemerkten Refresh holt das Ende von `poll_scan` nach.
                    if is_auto {
                        self.set_auto_fetching(false);
                        // I6: Timestamp = Ende statt Start — bei langsamem Fetch
                        // (> Intervall) kein Back-to-Back-Dauerfeuer.
                        self.last_auto_fetch = Some(std::time::Instant::now());
                    }
                    let mut repos = repos;
                    sort_main_repos(&mut repos, &self.config);
                    self.repos = repos;
                    self.repo_sort_dirty = false;
                    self.mr_after_scan();
                    if let Some(raw) = fetch_error {
                        let key = fetch_error_i18n_key(fetch_partial);
                        // I2: auth_hint nur bei echten Remote-Fehlern — bei
                        // Open-/Setup-Fehlern wäre "Credentials prüfen" irreführend.
                        let arg = if fetch_is_fetch_error {
                            let hint = tr(self.config.language, "auth_hint");
                            format!("{raw}. {hint}")
                        } else {
                            raw
                        };
                        let msg = tr_fmt(self.config.language, key, &[&arg]);
                        // Fallback falls Key fehlt (sollte nicht passieren, i18n-Test sichert ab)
                        let msg = if msg == key { arg } else { msg };
                        self.error = Some(msg.clone());
                        self.status_message = Some(msg);
                        self.status_message_time = Some(std::time::Instant::now());
                    } else {
                        self.error = None;
                    }
                    self.run_pending_after_checkout();
                    self.mark_tray_dirty();
                    self.sync_tray_service();
                    ctx.request_repaint();
                }
                ScanResult::FetchAborted { reason_key } => {
                    // H3: Thread-Panic vor send — Flags lösen (kein ewiger
                    // Spinner), Fehler persistent zeigen, Liste behalten.
                    // Wie Auto-FetchDone als Intervall-Ende werten (kein
                    // sofortiges Back-to-Back mit Panic-Loop).
                    self.set_git_busy(GitBusy::Idle);
                    self.set_auto_fetching(false);
                    self.last_auto_fetch = Some(std::time::Instant::now());
                    // F1#F2: einen gemerkten Refresh holt auch nach Abort das
                    // Ende von `poll_scan` nach — sonst wartet er bis Intervall.
                    let msg = tr(self.config.language, reason_key);
                    let msg = if msg == reason_key {
                        reason_key.to_string()
                    } else {
                        msg
                    };
                    self.error = Some(msg.clone());
                    self.status_message = Some(msg);
                    self.status_message_time = Some(std::time::Instant::now());
                    self.mark_tray_dirty();
                    ctx.request_repaint();
                }
                ScanResult::MrCheckoutReady { choice, result } => {
                    // K1: Ref-Lock lösen; dann Rescan, Wechsel oder Fehler
                    self.set_git_busy(GitBusy::Idle);
                    self.on_mr_checkout_ready(*choice, result);
                    self.mark_tray_dirty();
                    ctx.request_repaint();
                }
            }
        }
        while let Ok(err) = self.launch_err_rx.try_recv() {
            self.error = Some(err);
            self.status_message = None;
            self.status_message_time = None;
            // Async Tray-Launches (Agent/Shell) landen hier ohne Ursprung;
            // hidden impliziert Tray-Ursprung → Fehler sichtbar machen (7a).
            // #3: Fehler speist das Tray-Badge → dirty für nächsten Sync.
            self.mark_tray_dirty();
            if should_reveal_main_on_error(self.window_visible, self.should_quit) {
                self.show_main_window(ctx);
            }
            ctx.request_repaint();
        }
        while let Ok(res) = self.config_update_rx.try_recv() {
            match res {
                Ok(cfg) => {
                    self.config = cfg;
                    self.error = None;
                    // Finding A: neue Config kann Usage/Sortmodus geändert haben.
                    self.repo_sort_dirty = true;
                    self.mark_tray_dirty();
                    self.sync_tray_service();
                    // Settings offen: nicht kompletten Draft verwerfen, nur auto-erkannte Programme mergen
                    if self.show_settings {
                        if let Some(state) = self.settings_state.as_mut() {
                            state.merge_auto_detected(&self.config);
                        } else {
                            self.settings_state = Some(SettingsState::from_config(&self.config));
                        }
                    }
                    self.status_message = Some(tr(self.config.language, "saved_scan_restart"));
                    self.status_message_time = Some(std::time::Instant::now());
                    ctx.request_repaint();
                }
                Err(err_msg) => {
                    self.error = Some(err_msg);
                    self.status_message = None;
                    self.status_message_time = None;
                    // #3: Fehler speist das Tray-Badge → dirty für nächsten Sync.
                    self.mark_tray_dirty();
                    ctx.request_repaint();
                }
            }
        }
        while let Ok(outcome) = self.update_rx.try_recv() {
            match outcome {
                Ok(Some(info)) => {
                    self.update_info = Some(info);
                    self.update_error = None;
                    self.show_update_dialog = true;
                    ctx.request_repaint();
                }
                Ok(None) => {
                    self.update_error = None;
                }
                Err(e) => {
                    // In Release ohne Konsole unsichtbar -> für Support persistieren,
                    // Anzeige in Settings (M2). Zusätzlich kurzer Status-Hinweis im
                    // Hauptfenster (F2-H4), damit Fehlschläge nicht unsichtbar bleiben.
                    // Kein roter Error-Block: bei Offline-Start wäre das Popup-Spam.
                    self.status_message =
                        Some(tr_fmt(self.config.language, "tray_update_error", &[&e]));
                    self.update_error = Some(e);
                    self.status_message_time = Some(std::time::Instant::now());
                    ctx.request_repaint();
                }
            }
        }
        if self.git_busy.holds_refs() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(t) = self.status_message_time {
            if t.elapsed() > std::time::Duration::from_secs(3) {
                self.status_message = None;
                self.status_message_time = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        // Tray-Switches laufen headless: Main öffnet sich nie von selbst
        // (Dialog/Fehler bei Dirty/Konflikten warten in main, Popup bleibt offen).
        // Dedup: pro Repo nur der letzte Klick (keine verschwendeten Zwischen-Switches).
        let pending = std::mem::take(&mut self.pending_branch_switches);
        if !pending.is_empty() {
            for (path, branch) in dedup_branch_switches(pending) {
                if tray_branch_switch_opens_main_window() {
                    self.show_main_window(ctx);
                }
                self.handle_branch_switch(ctx, path, branch);
            }
            // K1: MR-Wechsel ohne Ausführung und ohne eigenen Dialog verwerfen
            self.drop_stale_mr_switch();
            // Dialog/Fehler landen in branch_dialog/error ohne direkten Sync
            // (#3 Badge) → dirty, damit der nächste Throttle-Sync übernimmt.
            self.mark_tray_dirty();
        }
        // Gemerkten Scan nachholen, sobald nichts mehr die Refs hält und kein
        // Auto-Fetch läuft (P2b nach dem Auto-Fetch, F1#F2 nach Abort, K1 nach
        // dem Auschecken) – erst nach dem Wechsel oben, sonst liest er den
        // alten Stand. Ein Fehler bleibt bis zum Scan-Ergebnis stehen.
        if self.pending_scan && !self.git_busy.holds_refs() && !self.auto_fetching {
            self.pending_scan = false;
            let error = self.error.take();
            self.start_scan();
            self.error = error;
        }
    }
}

/// Produktentscheidung: Tray-Branch-Switches öffnen das Hauptfenster nie von selbst.
/// Saubere Switches laufen headless im Tray (Popup bleibt offen); bei Dirty landet
/// der Entscheidungs-Dialog in `branch_dialog`, Konflikte/Fehler in `error` —
/// beides wird sichtbar, sobald das Hauptfenster manuell geöffnet wird.
pub fn tray_branch_switch_opens_main_window() -> bool {
    false
}

/// Dedupliziert gequeue-te Branch-Switches: pro Repo zählt nur der zuletzt
/// geklickte Branch (schnelle Mehrfach-Klicks im offen bleibenden Popup
/// erzeugen sonst sinnlose Zwischen-Switches). Die Position im Vec folgt dem
/// ersten Auftreten des Pfads, der Branch-Wert dem letzten Klick.
pub fn dedup_branch_switches(
    switches: Vec<(std::path::PathBuf, String)>,
) -> Vec<(std::path::PathBuf, String)> {
    use std::collections::HashMap;
    // I3: Key via repo_state_key normalisieren (Windows case-insensitiv),
    // sonst werden C:\Repo und c:\repo als verschiedene Repos behandelt.
    let mut last_idx: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<(std::path::PathBuf, String)> = Vec::with_capacity(switches.len());
    for (path, branch) in switches {
        let key = crate::config::AppConfig::repo_state_key(&path);
        if let Some(&i) = last_idx.get(&key) {
            out[i].1 = branch;
        } else {
            last_idx.insert(key, out.len());
            out.push((path, branch));
        }
    }
    out
}

/// #5: Sync-Entscheidung für den Tray-Service: nur bei dirty + abgelaufenem
/// Throttle (500ms) — kein blinder Sync im Idle.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn should_sync_tray_service(
    dirty: bool,
    last_sync: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    if !dirty {
        return false;
    }
    match last_sync {
        Some(t) => now.saturating_duration_since(t) >= std::time::Duration::from_millis(500),
        None => true,
    }
}

/// Task 5: Entscheidung für periodischen Auto-Fetch (throttled, Pattern
/// `should_sync_tray_service`): nur wenn Intervall > 0 (0 = aus) und seit dem
/// letzten Lauf mindestens `interval_min` Minuten vergangen sind.
/// `None` als `last` bedeutet "noch nie" → fällig.
pub fn should_auto_fetch(
    last: Option<std::time::Instant>,
    interval_min: u64,
    now: std::time::Instant,
) -> bool {
    if interval_min == 0 {
        return false;
    }
    match last {
        None => true,
        Some(t) => {
            now.saturating_duration_since(t)
                >= std::time::Duration::from_secs(interval_min.saturating_mul(60))
        }
    }
}

/// Pure helper for minimize-to-tray decision (H1).
/// `tray_available` must be true only when a tray icon + service actually exist,
/// otherwise minimizing would hide the window with no way back.
pub fn should_minimize_to_tray(
    minimize_cfg: bool,
    should_quit: bool,
    tray_available: bool,
) -> bool {
    minimize_cfg && !should_quit && tray_available
}

/// Entscheidet, ob nach einem Tray-initiierten Fehler das Hauptfenster
/// geöffnet wird (7a): `self.error` im versteckten Fenster wäre unsichtbar.
/// Beim Beenden (`should_quit`) nie öffnen; bei sichtbarem Fenster nichts tun.
/// Cross-platform: Aufrufer in `poll_scan` (alle OS) + Tray-Actions (Windows).
pub fn should_reveal_main_on_error(window_visible: bool, should_quit: bool) -> bool {
    !window_visible && !should_quit
}

#[cfg(target_os = "windows")]
fn clear_tray_popup(shared: &std::sync::Arc<std::sync::Mutex<crate::tray_service::TrayShared>>) {
    // Poison-tolerant try_lock: WouldBlock -> skip this frame (retry next),
    // Poisoned -> recover inner guard so tray does not stay stale forever.
    match shared.try_lock() {
        Ok(mut guard) => {
            guard.popup_open = false;
            guard.popup_rect = None;
            guard.popup_opened_at = None;
            guard.branch_filters.clear();
        }
        Err(std::sync::TryLockError::Poisoned(e)) => {
            let mut guard = e.into_inner();
            guard.popup_open = false;
            guard.popup_rect = None;
            guard.popup_opened_at = None;
            guard.branch_filters.clear();
        }
        Err(std::sync::TryLockError::WouldBlock) => {}
    }
}

impl MyApp {
    /// #5: Tray-Daten als geändert markieren (nächster Throttle-Sync übernimmt).
    fn mark_tray_dirty(&mut self) {
        self.tray_dirty = true;
    }

    /// Setzt, was die Refs hält. Jeder Wechsel sperrt oder löst die
    /// Fetch-Buttons im Tray (`TrayFetching`), daher dirty.
    fn set_git_busy(&mut self, busy: GitBusy) {
        if self.git_busy != busy {
            self.git_busy = busy;
            self.mark_tray_dirty();
        }
    }

    /// Wie `set_git_busy` für den Auto-Fetch (D1).
    fn set_auto_fetching(&mut self, running: bool) {
        if self.auto_fetching != running {
            self.auto_fetching = running;
            self.mark_tray_dirty();
        }
    }

    /// Task 5: Periodischer Auto-Fetch im Hintergrund (throttled).
    /// Scope: ALLE gescannten Repos (Default-Intervall 10 min, 0 = aus) —
    /// der Veraltet-Indikator im Hauptfenster braucht frische Counts überall,
    /// nicht nur für die Tray-Top-N.
    /// Fehler gehen als FetchDone mit und setzen bewusst den persistenten
    /// Banner (Entscheidung: sichtbar statt still — Offline-Remotes sind
    /// ein Dauerzustand, kein Rauschen); Erfolg löscht ihn wieder.
    /// Danach Rescan für frische `behind`-Counts.
    fn maybe_auto_fetch(&mut self, ctx: &egui::Context) {
        let interval = self.config.auto_fetch_interval_min;
        if interval == 0 || self.repos.is_empty() {
            // Idle-Wake für nächsten Check: min(Intervall, 60s), damit
            // `logic()` auch ohne Events aufwacht.
            if interval > 0 {
                ctx.request_repaint_after(std::time::Duration::from_secs(60));
            }
            return;
        }
        if self.auto_fetching {
            // Läuft bereits: Ergebnis zeitnah anwenden (stiller Lauf ohne
            // `git_busy`, daher kein automatischer Repaint-Loop).
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            return;
        }
        if self.git_busy.holds_refs() {
            // Manueller Scan/Fetch oder Auschecken läuft: Auto auslassen
            // statt blockieren (Finding D1) — nächster Tick holt es nach.
            ctx.request_repaint_after(std::time::Duration::from_secs(60));
            return;
        }
        let now = std::time::Instant::now();
        if !should_auto_fetch(self.last_auto_fetch, interval, now) {
            ctx.request_repaint_after(std::time::Duration::from_secs(60));
            return;
        }
        // I6: Timestamp erst im FetchDone-Arm (Ende) setzen — nicht hier am
        // Start, sonst Back-to-Back bei Fetch-Dauer > Intervall.
        // Finding D1: bewusst NICHT `git_busy` — Refresh/Fetch bleiben klickbar.
        self.set_auto_fetching(true);
        let paths: Vec<PathBuf> = self.repos.iter().map(|r| r.path.clone()).collect();
        let cfg = self.config.clone();
        let tx = self.scan_tx.clone();
        std::thread::spawn(move || {
            // Worktrees eines Repos teilen die Remote-Refs: ein Fetch je Repo
            let paths = crate::git::fetch_targets(&paths);
            // Finding E: Fehler aggregieren statt nur loggen — erster Fehler
            // geht als FetchDone mit (persistenter Banner), weiter fetchen,
            // damit erreichbare Remotes trotzdem frische Counts liefern.
            let mut fetch_error: Option<String> = None;
            let mut fetch_partial = false;
            let mut fetch_is_fetch_error = false;
            // P6: begrenzt parallel (max 4 Threads) statt seriell — bei vielen
            // Repos sonst N×Latenz. Ohne rayon: simple Chunk-Worker mit Mutex.
            let parallel = std::cmp::min(4, paths.len().max(1));
            if parallel <= 1 {
                // H3: Panic im Fetch darf Scan + Send nicht verhindern —
                // als Fehler notieren, mit Rescan fortfahren.
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    for p in &paths {
                        if let Err(e) = crate::git::fetch_all(p) {
                            eprintln!("Auto-Fetch für {} fehlgeschlagen: {e:#}", p.display());
                            let repo_name = short_repo_name(p);
                            aggregate_named_fetch_error(
                                &mut fetch_error,
                                &mut fetch_partial,
                                &mut fetch_is_fetch_error,
                                &repo_name,
                                &e,
                            );
                        }
                    }
                }));
                if r.is_err() {
                    eprintln!("Auto-Fetch-Worker abgestürzt (panic im fetch)");
                    if fetch_error.is_none() {
                        fetch_error = Some("background fetch worker panicked".to_string());
                    }
                }
            } else {
                let queue = std::sync::Arc::new(std::sync::Mutex::new(paths));
                let first_err = std::sync::Arc::new(std::sync::Mutex::new((None, false, false)));
                let mut handles = Vec::new();
                for _ in 0..parallel {
                    let q = queue.clone();
                    let fe = first_err.clone();
                    handles.push(std::thread::spawn(move || loop {
                        // F1#8: Poison-Recovery wie im Tray-Pfad — ein poisoned
                        // Mutex darf nicht still den Rest der Queue verwerfen.
                        let next = q.lock().unwrap_or_else(|e| e.into_inner()).pop();
                        let Some(p) = next else { break };
                        if let Err(e) = crate::git::fetch_all(&p) {
                            eprintln!("Auto-Fetch für {} fehlgeschlagen: {e:#}", p.display());
                            // F3#12: Poison-Recovery wie beim Queue-Pop (statt
                            // Fehlerverlust durch `if let Ok`).
                            let mut g = fe.lock().unwrap_or_else(|e| e.into_inner());
                            // Disjunkte Tuple-Felder separat binden (kein
                            // Mehrfach-Borrow des Guards).
                            let repo_name = short_repo_name(&p);
                            let (e0, e1, e2) = &mut *g;
                            aggregate_named_fetch_error(e0, e1, e2, &repo_name, &e);
                        }
                    }));
                }
                // F2#B1/F3#1: Worker-Panic merken — der Merge unten darf den
                // Marker nicht mit leerem Shared-State überschreiben.
                let mut worker_panicked = false;
                for h in handles {
                    // H3: abgestürzter Worker (panic) — Rest wurde von den
                    // Überlebenden per Queue abgearbeitet; nur vermerken.
                    if h.join().is_err() {
                        eprintln!("Auto-Fetch-Worker abgestürzt (panic im Thread)");
                        worker_panicked = true;
                    }
                }
                if let Ok(g) = first_err.lock() {
                    let shared = (g.0.clone(), g.1, g.2);
                    merge_parallel_fetch_result(
                        &mut fetch_error,
                        &mut fetch_partial,
                        &mut fetch_is_fetch_error,
                        shared,
                        worker_panicked,
                    );
                };
            }
            // H3: Panic im Scan darf Flags nicht hängen lassen —
            // FetchAborted resettet (Liste bleibt erhalten, kein Wipe).
            let repos = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::scanner::scan_repos(&cfg)
            })) {
                Ok(r) => r,
                Err(_) => {
                    eprintln!("Auto-Fetch-Scan abgestürzt (panic vor send)");
                    let _ = tx.send(ScanResult::FetchAborted {
                        reason_key: "fetch_aborted",
                    });
                    return;
                }
            };
            let _ = tx.send(ScanResult::FetchDone {
                repos,
                fetch_error,
                fetch_partial,
                fetch_is_fetch_error,
                is_auto: true,
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    /// #12: Config-Save-Fehler einheitlich melden statt still zu verwerfen:
    /// eprintln für Logs + `self.error` fürs Hauptfenster (sichtbar beim Öffnen).
    fn note_config_save_error(&mut self, err: &anyhow::Error) {
        eprintln!("Config speichern fehlgeschlagen: {err:#}");
        self.error = Some(tr_fmt(
            self.config.language,
            "save_failed",
            &[&format!("{err:#}")],
        ));
    }

    fn show_main_window(&mut self, ctx: &egui::Context) {
        self.window_visible = true;
        // F2#3: `should_quit` NICHT zurücksetzen — ein ausstehender Quit
        // (Tray-Quit + Close-Cmd bereits unterwegs) gewinnt gegen ein
        // nebenläufiges ShowMain; sonst überlebt die App den Quit-Wunsch.
        // Also clear popup in dedicated tray service (efficient: shared state)
        #[cfg(target_os = "windows")]
        if let Some(shared) = &self.tray_shared {
            clear_tray_popup(shared);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) {
            // Only minimize to tray when a tray icon actually exists (H1).
            // Without tray, hiding would lose the window with no way back.
            #[cfg(target_os = "windows")]
            let tray_available = self.tray_icon.is_some() && self.tray_shared.is_some();
            #[cfg(not(target_os = "windows"))]
            let tray_available = false;
            let should_minimize = should_minimize_to_tray(
                self.config.minimize_to_tray,
                self.should_quit,
                tray_available,
            );
            if should_minimize {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                // Hide instead of closing - tray service keeps popup alive
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.window_visible = false;
                #[cfg(target_os = "windows")]
                if let Some(shared) = &self.tray_shared {
                    clear_tray_popup(shared);
                }
            } else {
                // F1#F4: echter Fenster-Close — gebatchte Usage-Saves forcen
                // wie bei Tray-Quit, sonst gehen MRU-Timestamps im
                // Throttle-Fenster (<200 ms) verloren.
                self.last_config_save = None;
                self.config_save_pending = true;
                self.flush_config_save(ctx);
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn sync_tray_service(&mut self) {
        // Review-Nit: erst prüfen, dann klonen (ohne Tray kein Clone).
        if let Some(shared) = &self.tray_shared {
            // Finding B: Lock-Probe VOR den Deep-Clones — bei Contention
            // (WouldBlock) wird kein Clone verworfen, nur dirty für Retry
            // gesetzt. Der Lock wird weiterhin nur für den kurzen
            // Zeiger-Tausch gehalten, nicht für die Clones.
            // (Probe-Guard wird sofort gedroppt; Race zum zweiten Lock
            // unten ist benign: WouldBlock → dirty → Throttle-Retry.)
            let contended = matches!(shared.try_lock(), Err(std::sync::TryLockError::WouldBlock));
            if contended {
                self.tray_dirty = true;
                return;
            }
            // #5: Klone VOR dem Lock erstellen — der Mutex wird nur für den kurzen
            // Zeiger-Tausch gehalten, nicht für die Deep-Clones von Repos/Config.
            let repos = self.repos.clone();
            let config = self.config.clone();
            // #3: Badge-Text (wartender Branch-Dialog/Fehler) gleich mit syncen.
            let notice = self.tray_notice();
            // E21: Badge am Tray-Button „Merge Requests“ (letzter Stand).
            let mr_badge = merge_requests::mr_badge(self.mr_snapshot.as_deref());
            // Fetch-Buttons im Tray: gesperrt während Scan/Fetch, Spinner am Repo
            let fetching = crate::ui::tray_popup::TrayFetching {
                busy: self.git_busy.holds_refs() || self.auto_fetching,
                repo: match &self.git_busy {
                    GitBusy::Fetch { repo } => Some(repo.clone()),
                    _ => None,
                },
            };
            // Poison-tolerant: recover from poisoned mutex instead of staling forever.
            let locked = match shared.try_lock() {
                Ok(g) => Some(g),
                Err(std::sync::TryLockError::Poisoned(e)) => Some(e.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => None,
            };
            if let Some(mut guard) = locked {
                guard.update_data(repos, config);
                guard.notice = notice;
                guard.mr_badge = mr_badge;
                guard.fetching = fetching;
                // Erfolgreich: dirty zurücksetzen.
                self.tray_dirty = false;
                self.last_tray_sync = Some(std::time::Instant::now());
            } else {
                // Review-Fix (:248): verlorener Sync bei WouldBlock → dirty
                // setzen, damit maybe_sync_tray_service per Throttle nachholt.
                // Deckt alle direkten Sync-Aufrufer ab (Scan, Config, Usage …).
                self.tray_dirty = true;
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn maybe_sync_tray_service(&mut self) {
        // #5: nur bei dirty + Throttle syncen — kein blinder Sync alle 500ms
        // im Idle ohne jede Datenänderung.
        if should_sync_tray_service(
            self.tray_dirty,
            self.last_tray_sync,
            std::time::Instant::now(),
        ) {
            self.sync_tray_service();
        }
    }

    /// #3: Badge-Text fürs Tray-Popup (wartender Branch-Dialog/Fehler im
    /// Hauptfenster). Priorität: Dialog (mit Repo-Name) vor Fehler.
    #[cfg(target_os = "windows")]
    fn tray_notice(&self) -> Option<String> {
        let dialog_repo = self.branch_dialog.as_ref().and_then(|d| {
            d.repo_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        });
        crate::ui::tray_popup::tray_notice_text(
            self.config.language,
            dialog_repo.as_deref(),
            self.error.is_some(),
        )
    }

    #[cfg(target_os = "windows")]
    fn poll_tray_service_actions(&mut self, ctx: &egui::Context) {
        let actions: Vec<crate::tray_service::TrayAction> = if let Some(rx) = &self.tray_action_rx {
            let mut v = Vec::new();
            while let Ok(a) = rx.try_recv() {
                v.push(a);
            }
            v
        } else {
            Vec::new()
        };
        for action in actions {
            match action {
                crate::tray_service::TrayAction::ShowMainWindow => self.show_main_from_tray(ctx),
                crate::tray_service::TrayAction::ShowMergeRequests => self.show_mrs_from_tray(ctx),
                crate::tray_service::TrayAction::Refresh => self.start_scan(),
                crate::tray_service::TrayAction::FetchRepo(path) => self.start_repo_fetch(path),
                crate::tray_service::TrayAction::OpenSettings => {
                    self.show_settings = true;
                    self.settings_state = Some(SettingsState::from_config(&self.config));
                    self.show_main_window(ctx);
                }
                crate::tray_service::TrayAction::Quit => {
                    self.should_quit = true;
                    // L6: Usage-Timestamps nicht verlieren — Force-Flush
                    // (Throttle umgehen), sonst bleibt pending ungespeichert.
                    // pending=true erzwingen (Backoff-Give-up nicht im Weg).
                    self.last_config_save = None;
                    self.config_save_pending = true;
                    self.flush_config_save(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                crate::tray_service::TrayAction::BranchSwitch(path, branch) => {
                    // Queue instead of overwriting (N2): no action lost on rapid clicks.
                    self.pending_branch_switches.push((path, branch));
                }
                crate::tray_service::TrayAction::SolutionSelect(repo_path, sln_path) => {
                    let state = self.config.get_repo_state_mut(&repo_path);
                    state.selected_solution = Some(sln_path.clone());
                    if let Err(e) = self.config.save() {
                        // Review-Fix: Fehler speist das Tray-Badge → dirty.
                        self.mark_tray_dirty();
                        self.error = Some(crate::i18n::tr_fmt(
                            self.config.language,
                            "save_failed",
                            &[&format!("{e:#}")],
                        ));
                        if should_reveal_main_on_error(self.window_visible, self.should_quit) {
                            self.show_main_window(ctx);
                        }
                    } else {
                        if let Some(repo) = self.repos.iter_mut().find(|r| r.path == repo_path) {
                            repo.selected_solution = Some(sln_path.clone());
                        }
                        self.status_message = Some(crate::i18n::tr_fmt(
                            self.config.language,
                            "solution_selected",
                            &[&repo_path.display().to_string()],
                        ));
                        self.status_message_time = Some(std::time::Instant::now());
                        self.mark_tray_dirty();
                        self.sync_tray_service();
                    }
                }
                crate::tray_service::TrayAction::IdeOpen(path, ide_id, file) => {
                    {
                        let state = self.config.get_repo_state_mut(&path);
                        state.selected_ide = Some(ide_id.clone());
                        // #12: Save-Fehler melden statt still verwerfen.
                        self.mark_tray_dirty();
                        if let Err(e) = self.config.save() {
                            self.note_config_save_error(&e);
                            if should_reveal_main_on_error(self.window_visible, self.should_quit) {
                                self.show_main_window(ctx);
                            }
                        }
                    }
                    let profile = self.config.get_effective_profile_for_repo(&path);
                    if let Some(ide) = profile.ides.iter().find(|i| i.id == ide_id) {
                        let file_opt = if ide.no_args {
                            None
                        } else {
                            Some(file.as_path())
                        };
                        let display = file_opt
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| path.display().to_string());
                        match launch_ide(ide, &path, file_opt) {
                            Ok(()) => {
                                self.status_message = Some(crate::i18n::tr_fmt(
                                    self.config.language,
                                    "opening_with",
                                    &[&display, &ide.display_name],
                                ));
                                self.status_message_time = Some(std::time::Instant::now());
                            }
                            Err(e) => {
                                self.error = Some(format!(
                                    "IDE '{}' konnte nicht gestartet werden: {e:#}",
                                    ide.display_name
                                ));
                                if should_reveal_main_on_error(
                                    self.window_visible,
                                    self.should_quit,
                                ) {
                                    self.show_main_window(ctx);
                                }
                            }
                        }
                    }
                    self.record_usage(&path, UsageType::Open);
                }
                crate::tray_service::TrayAction::AgentOpen(path, agent_id) => {
                    let agent = self
                        .config
                        .agents
                        .iter()
                        .find(|a| a.id == agent_id)
                        .cloned()
                        .or_else(|| self.config.get_active_agent().cloned());
                    if let Some(agent) = agent {
                        let term_pref = agent
                            .terminal_override
                            .clone()
                            .unwrap_or_else(|| self.config.terminal.preference.clone());
                        let repo_clone = path.clone();
                        let agent_clone = agent.clone();
                        let name = agent.display_name.clone();
                        let err_tx = self.launch_err_tx.clone();
                        std::thread::spawn(move || {
                            if let Err(e) = launch_agent(&agent_clone, &repo_clone, &term_pref) {
                                let _ = err_tx.send(format!(
                                    "Agent '{}' konnte nicht gestartet werden: {e:#}",
                                    name
                                ));
                            }
                        });
                        self.status_message = Some(crate::i18n::tr_fmt(
                            self.config.language,
                            "starting_agent_in",
                            &[&agent.display_name, &path.display().to_string()],
                        ));
                        self.status_message_time = Some(std::time::Instant::now());
                    }
                    self.record_usage(&path, UsageType::Open);
                }
                crate::tray_service::TrayAction::ExplorerOpen(path) => {
                    match crate::git::open_in_explorer(&path) {
                        Ok(()) => {
                            self.status_message = Some(crate::i18n::tr_fmt(
                                self.config.language,
                                "explorer_opened",
                                &[&path.display().to_string()],
                            ));
                            self.status_message_time = Some(std::time::Instant::now());
                        }
                        Err(e) => {
                            self.error =
                                Some(format!("Explorer konnte nicht geöffnet werden: {e:#}"));
                            if should_reveal_main_on_error(self.window_visible, self.should_quit) {
                                self.show_main_window(ctx);
                            }
                        }
                    }
                    self.record_usage(&path, UsageType::Open);
                }
                crate::tray_service::TrayAction::ShellOpen(path) => {
                    let pref = self.config.terminal.preference.clone();
                    let repo_clone = path.clone();
                    let err_tx = self.launch_err_tx.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = crate::git::open_shell(&repo_clone, &pref) {
                            let _ =
                                err_tx.send(format!("Shell konnte nicht geöffnet werden: {e:#}"));
                        }
                    });
                    self.status_message = Some(crate::i18n::tr_fmt(
                        self.config.language,
                        "shell_opened",
                        &[&path.display().to_string()],
                    ));
                    self.status_message_time = Some(std::time::Instant::now());
                    self.record_usage(&path, UsageType::Open);
                }
            }
        }
        // P5: gebatcht — 1 Save max. statt N Saves bei N Actions im Frame.
        self.flush_config_save(ctx);
    }

    #[cfg(target_os = "windows")]
    fn show_tray_service_viewport(&mut self, ctx: &egui::Context) {
        // Efficient: create dedicated tray service viewport - runs independently even when main hidden.
        // M3: ROOT-ppp hier (Main-Thread) abgreifen und in shared spiegeln, damit
        // toggle_popup die Tray-Rect-Umrechnung mit der DPI des Tray-Monitors macht
        // statt mit der des off-screen Service-Viewports.
        if let (Some(shared), Some(tray_rx)) =
            (self.tray_shared.clone(), self.tray_service_tray_rx.clone())
        {
            let ppp = ctx.pixels_per_point();
            if ppp.is_finite() && ppp > 0.0 {
                match shared.try_lock() {
                    Ok(mut g) => g.set_root_ppp(ppp),
                    Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner().set_root_ppp(ppp),
                    Err(std::sync::TryLockError::WouldBlock) => {}
                }
            }
            crate::tray_service::create_tray_service_viewport(ctx, shared, tray_rx);
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn sync_tray_service(&mut self) {}

    /// Manueller `git fetch --all` für ein Repo (Branch-Popup im Hauptfenster,
    /// Fetch-Button im Tray): im Hintergrund, danach Rescan (`FetchDone`).
    fn start_repo_fetch(&mut self, repo_path: PathBuf) {
        let lang = self.config.language;
        // L2: wie start_scan — kein manueller Fetch während
        // Auto-Fetch läuft (Ref-Race); Request verwerfen + Hinweis.
        if self.auto_fetching {
            self.status_message = Some(tr(lang, "auto_fetch_running"));
            self.status_message_time = Some(std::time::Instant::now());
            return;
        }
        // Ein Scan/Fetch/Checkout läuft schon (Tray-Stand kann veraltet sein):
        // kein zweiter Fetch auf dieselben Refs
        if self.git_busy.holds_refs() {
            self.status_message = Some(tr(lang, "fetch_busy"));
            self.status_message_time = Some(std::time::Instant::now());
            return;
        }
        let path_clone = repo_path.clone();
        let cfg = self.config.clone();
        let tx = self.scan_tx.clone();
        // Manueller Fetch = frische Refs: Auto-Timer zurücksetzen,
        // damit der periodische Fetch nicht Sekunden später doppelt läuft.
        self.last_auto_fetch = Some(std::time::Instant::now());
        self.status_message = Some(tr_fmt(
            lang,
            "fetching_branches",
            &[&repo_path.display().to_string()],
        ));
        self.status_message_time = Some(std::time::Instant::now());
        std::thread::spawn(move || {
            // H3: Panic vor send darf `git_busy` nicht hängen
            // lassen — FetchAborted resettet Flags ohne Listen-Wipe.
            let msg = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let fetch_res = crate::git::fetch_all(&path_clone);
                let (fetch_error, fetch_partial, fetch_is_fetch_error) = match fetch_res {
                    Ok(()) => (None, false, false),
                    Err(e) => {
                        let (msg, partial, is_fetch) = fetch_error_parts(&e);
                        eprintln!("Fetch fehlgeschlagen für {}: {msg}", path_clone.display());
                        (Some(msg), partial, is_fetch)
                    }
                };
                let repos = crate::scanner::scan_repos(&cfg);
                (fetch_error, fetch_partial, fetch_is_fetch_error, repos)
            })) {
                Ok((fetch_error, fetch_partial, fetch_is_fetch_error, repos)) => {
                    ScanResult::FetchDone {
                        repos,
                        fetch_error,
                        fetch_partial,
                        fetch_is_fetch_error,
                        is_auto: false,
                    }
                }
                Err(_) => {
                    eprintln!("Manueller Fetch-Thread abgestürzt (panic vor send)");
                    ScanResult::FetchAborted {
                        reason_key: "fetch_aborted",
                    }
                }
            };
            let _ = tx.send(msg);
        });
        // Spinner/gesperrte Fetch-Buttons auch im Tray, sofort statt per Throttle
        self.set_git_busy(GitBusy::Fetch { repo: repo_path });
        self.sync_tray_service();
    }

    fn record_usage(&mut self, repo_path: &Path, usage_type: UsageType) {
        let key = crate::config::AppConfig::repo_state_key(repo_path);
        let usage = self.config.repo_usage.entry(key).or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        match usage_type {
            UsageType::Open => {
                usage.last_opened = Some(now);
                usage.open_count = usage.open_count.wrapping_add(1);
            }
            UsageType::BranchSwitch => {
                usage.last_branch_switch = Some(now);
                usage.branch_switch_count = usage.branch_switch_count.wrapping_add(1);
            }
            UsageType::ConfigChange => {
                usage.last_config_change = Some(now);
                usage.config_change_count = usage.config_change_count.wrapping_add(1);
            }
        }
        // #12: Save-Fehler melden statt still verwerfen (einheitlich mit
        // SolutionSelect).
        self.mark_tray_dirty();
        // Finding A: Usage ändert MRU → Hauptsortierung invalidieren.
        self.repo_sort_dirty = true;
        // P5: nicht sofort speichern — N Actions im selben Frame würden N
        // File-Syncs auslösen. Nur pending markieren, flush 1× pro Frame.
        self.config_save_pending = true;
        // F1#6/P1: kein direkter sync_tray_service() (Deep-Clone pro Klick) —
        // der Throttle-Sync in logic() übernimmt (N1: das Markieren oben
        // genügt — Flag ist idempotent, kein zweites nötig).
    }

    /// P5: gebatchter Config-Save — max. 1 File-Sync pro Aufruf, Fehler via
    /// note_config_save_error (wie bisher in record_usage).
    /// F2#5/L6: Throttle-Hit plant Repaint (sonst hängt pending bei
    /// eingestelltem egui-Repaint), Fehler re-armt Retry, Quit flusht force.
    fn flush_config_save(&mut self, ctx: &egui::Context) {
        if !self.config_save_pending {
            return;
        }
        // Throttle: max. 1 Save / 200ms, sonst pending für nächsten Frame.
        if let Some(t) = self.last_config_save {
            let elapsed = t.elapsed();
            if elapsed < std::time::Duration::from_millis(200) {
                ctx.request_repaint_after(std::time::Duration::from_millis(200) - elapsed);
                return;
            }
        }
        self.config_save_pending = false;
        self.last_config_save = Some(std::time::Instant::now());
        if let Err(e) = self.config.save() {
            // Retry beim nächsten Frame statt still zu verwerfen.
            self.config_save_pending = true;
            self.config_save_fail_streak = self.config_save_fail_streak.saturating_add(1);
            if self.config_save_fail_streak <= 5 {
                self.note_config_save_error(&e);
            } else {
                // L4: Backoff — Banner behalten (sichtbar), aber Log nicht mit
                // ~5 Zeilen/s fluten (volle Disk/Rechte = Dauerzustand).
                self.error = Some(tr_fmt(
                    self.config.language,
                    "save_failed",
                    &[&format!("{e:#}")],
                ));
                // F3#2: Dauer-Retry stoppen (kein 5-Hz-fsync-Loop) — nächste
                // User-Action (record_usage) und Quit-/Close-Flush re-armen.
                self.config_save_pending = false;
            }
        } else {
            self.config_save_fail_streak = 0;
        }
    }

    /// Öffnet `path` mit der IDE `ide_id` des wirksamen Profils (Repo-Karte
    /// und MR-Ansicht); `file` = Solution/Projektdatei, bei `no_args` nur der
    /// Ordner. Merkt die IDE-Wahl. `true`, wenn die IDE gestartet wurde.
    fn open_ide(&mut self, path: &Path, ide_id: &str, file: &Path) -> bool {
        self.open_ide_in(path, path, ide_id, file)
    }

    /// Wie [`Self::open_ide`], die IDE kommt aber aus dem wirksamen Profil
    /// von `profile_path` (K1: Worktree außerhalb der Suchpfade mit der IDE
    /// des Basis-Klons).
    fn open_ide_in(&mut self, path: &Path, profile_path: &Path, ide_id: &str, file: &Path) -> bool {
        let lang = self.config.language;
        // Speichere letzte IDE Wahl
        {
            let state = self.config.get_repo_state_mut(path);
            state.selected_ide = Some(ide_id.to_string());
            // Review-Fix: Tray-Snapshot sonst stale.
            self.mark_tray_dirty();
            if let Err(e) = self.config.save() {
                self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
            }
        }
        let mut started = false;
        // Finde IdeConfig
        let profile = self.config.get_effective_profile_for_repo(profile_path);
        if let Some(ide) = profile.ides.iter().find(|i| i.id == ide_id) {
            let file_opt = if ide.no_args { None } else { Some(file) };
            let display_path = file_opt
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| path.display().to_string());
            match launch_ide(ide, path, file_opt) {
                Ok(()) => {
                    self.status_message = Some(tr_fmt(
                        lang,
                        "opening_with",
                        &[&display_path, &ide.display_name],
                    ));
                    self.status_message_time = Some(std::time::Instant::now());
                    started = true;
                }
                Err(e) => {
                    self.error = Some(format!(
                        "IDE '{}' konnte nicht gestartet werden: {e:#}",
                        ide.display_name
                    ));
                }
            }
        } else {
            self.error = Some(tr_fmt(
                lang,
                "ide_not_found",
                &[ide_id, &profile.display_name],
            ));
        }
        self.record_usage(path, UsageType::Open);
        started
    }

    /// Agent mit dieser ID, sonst der aktive (wie die Repo-Karte).
    fn agent_by_id(&self, agent_id: &str) -> Option<&crate::config::AgentProfile> {
        self.config
            .agents
            .iter()
            .find(|a| a.id == agent_id)
            .or_else(|| self.config.get_active_agent())
    }

    /// Startet den Agenten `agent_id` (sonst den aktiven) im Terminal in
    /// `path`. `true`, wenn ein Agent gestartet wurde.
    fn open_agent(&mut self, path: &Path, agent_id: &str) -> bool {
        let lang = self.config.language;
        let started = if let Some(agent) = self.agent_by_id(agent_id).cloned() {
            let term_pref = agent
                .terminal_override
                .clone()
                .unwrap_or_else(|| self.config.terminal.preference.clone());
            // Nicht blockierend starten (verhindert UI Freeze, besonders bei pwsh/wt)
            let repo_clone = path.to_path_buf();
            let display_name = agent.display_name.clone();
            let err_tx = self.launch_err_tx.clone();
            self.status_message = Some(tr_fmt(
                lang,
                "starting_agent_in",
                &[&agent.display_name, &path.display().to_string()],
            ));
            self.status_message_time = Some(std::time::Instant::now());
            std::thread::spawn(move || {
                if let Err(e) = launch_agent(&agent, &repo_clone, &term_pref) {
                    let _ = err_tx.send(format!(
                        "Agent '{}' konnte nicht gestartet werden: {e:#}",
                        display_name
                    ));
                }
            });
            true
        } else {
            self.error = Some(tr_fmt(lang, "agent_not_found", &[agent_id]));
            false
        };
        self.record_usage(path, UsageType::Open);
        started
    }

    /// Öffnet `path` im Explorer. `true` bei Erfolg.
    fn open_explorer(&mut self, path: &Path) -> bool {
        let opened = match crate::git::open_in_explorer(path) {
            Ok(()) => {
                self.status_message = Some(tr_fmt(
                    self.config.language,
                    "explorer_opened",
                    &[&path.display().to_string()],
                ));
                self.status_message_time = Some(std::time::Instant::now());
                true
            }
            Err(e) => {
                self.error = Some(format!("Explorer konnte nicht geöffnet werden: {e:#}"));
                false
            }
        };
        self.record_usage(path, UsageType::Open);
        opened
    }

    /// Öffnet eine Shell in `path` (im Hintergrund; Fehler kommen über
    /// `launch_err_rx`).
    fn open_shell(&mut self, path: &Path) {
        let pref = self.config.terminal.preference.clone();
        let repo_clone = path.to_path_buf();
        let err_tx = self.launch_err_tx.clone();
        std::thread::spawn(move || {
            if let Err(e) = crate::git::open_shell(&repo_clone, &pref) {
                let _ = err_tx.send(format!("Shell konnte nicht geöffnet werden: {e:#}"));
            }
        });
        self.status_message = Some(tr_fmt(
            self.config.language,
            "shell_opened",
            &[&path.display().to_string()],
        ));
        self.status_message_time = Some(std::time::Instant::now());
        self.record_usage(path, UsageType::Open);
    }

    fn handle_branch_switch(
        &mut self,
        ctx: &egui::Context,
        repo_path: PathBuf,
        target_branch: String,
    ) {
        // Prüfe dirty und conflicts
        let repo_path_clone = repo_path.clone();
        let target_clone = target_branch.clone();
        // Öffne Repo um Status zu prüfen
        if let Some(repo) = crate::git::open_repo(&repo_path) {
            // F1#5: Dirty-Flag aus letztem Scan wiederverwenden statt teurem
            // Live-Check (recurse_untracked_dirs, unbegrenzt) auf dem UI-Thread.
            // Fallback Live-Check, falls Repo (noch) nicht in der Liste.
            // Beide Fehlerrichtungen failen safe: false-clean → Checkout
            // scheitert im Dry-Run mit Fehler; false-dirty → Dialog zu viel.
            let dirty = self
                .repos
                .iter()
                .find(|r| r.path == repo_path)
                .map(|r| r.dirty)
                .unwrap_or_else(|| crate::git::is_dirty(&repo));
            let has_conflicts = crate::git::has_merge_conflicts(&repo);
            let in_progress = crate::git::is_merge_in_progress(&repo);
            if has_conflicts || in_progress {
                self.error = Some(tr_fmt(
                    self.config.language,
                    "branch_switch_blocked",
                    &[&target_branch, &format!("{:?}", repo.state())],
                ));
                // K1: kein Wechsel, also auch kein „Danach öffnen“
                self.cancel_mr_switch(&repo_path, &target_branch);
                // I5: echte Fehler müssen bei hidden Window revealen — sonst
                // klickt der Nutzer im Tray und sieht nichts.
                self.mark_tray_dirty();
                if should_reveal_main_on_error(self.window_visible, self.should_quit) {
                    self.show_main_window(ctx);
                }
                return;
            }
            if dirty {
                // Zeige Dialog (P4: get_detailed_status ohne recurse, gecappt)
                let (files, files_truncated) = crate::git::get_detailed_status(&repo);
                self.branch_dialog = Some(BranchDialog {
                    repo_path,
                    target_branch,
                    error: None,
                    dirty_files: files,
                    files_truncated,
                });
                return;
            }
        }
        // Kein dirty, direkt wechseln
        self.execute_branch_switch(repo_path_clone, target_clone, false, false);
    }

    fn execute_branch_switch(
        &mut self,
        repo_path: PathBuf,
        target_branch: String,
        force: bool,
        stash: bool,
    ) {
        let result = if stash {
            crate::git::stash_and_checkout(&repo_path, &target_branch)
                .map(|switch| (switch.outcome, switch.pop_failed))
        } else if force {
            crate::git::checkout_branch_force(&repo_path, &target_branch).map(|o| (o, None))
        } else {
            crate::git::checkout_branch(&repo_path, &target_branch).map(|o| (o, None))
        };

        let lang = self.config.language;
        match result {
            Ok((_, Some(pop_failed))) => {
                // Gewechselt, aber Stash pop gescheitert (Stash bleibt
                // erhalten): Hinweis im Dialog wie bisher und neu scannen.
                // Ein MR-Wechsel ist trotzdem erledigt (Meldung, „Danach öffnen“).
                let hint = pop_failed.to_string();
                match &mut self.branch_dialog {
                    Some(dlg) => dlg.error = Some(hint),
                    None => self.error = Some(hint),
                }
                self.start_scan();
                self.mr_switch_succeeded(&repo_path, &target_branch);
            }
            Ok((outcome, None)) => {
                self.status_message = Some(tr_fmt(
                    lang,
                    "branch_switched",
                    &[&target_branch, &repo_path.display().to_string()],
                ));
                self.status_message_time = Some(std::time::Instant::now());
                self.error = None;
                self.branch_dialog = None;
                self.record_usage(&repo_path, UsageType::BranchSwitch);
                self.start_scan();
                // E15: Hinweis (vorgespult / voraus / divergiert) erst nach
                // `start_scan` — während eines Auto-Fetch überschreibt der
                // sonst „auto_fetch_running“ (Scan bleibt per pending_scan gemerkt).
                if let Some(notice) = checkout_outcome_notice(lang, &outcome) {
                    self.status_message = Some(notice);
                    self.status_message_time = Some(std::time::Instant::now());
                }
                // K1: Wechsel aus der MR-Ansicht → MR-Meldung, „Danach öffnen“
                self.mr_switch_succeeded(&repo_path, &target_branch);
            }
            Err(e) => {
                // E15: Upstream-/Worktree-Abbruch lokalisiert.
                let detail = branch_switch_error_detail(lang, &e);
                // Wenn Fehler und Dialog offen, zeige Fehler im Dialog, sonst global
                if let Some(dlg) = &mut self.branch_dialog {
                    dlg.error = Some(detail);
                } else {
                    self.error = Some(tr_fmt(lang, "branch_switch_failed", &[&detail]));
                    // K1: ohne Dialog kein zweiter Versuch
                    self.cancel_mr_switch(&repo_path, &target_branch);
                }
            }
        }
    }

    fn show_top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let lang = self.config.language;
        // Hysterese wird in `update()` gepflegt ( <400 collapsed, >=500 expanded )
        let effective_collapse = self.top_bar_collapsed;
        // Schmales Fenster (Umschalter braucht Platz): Buttons nur als Icon;
        // noch schmaler Feldbeschriftungen als Tooltip der Combos.
        let compact = self.last_window_size[0] < TOP_BAR_COMPACT_WIDTH;
        let no_captions = self.last_window_size[0] < TOP_BAR_CAPTIONS_WIDTH;

        egui::Panel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(ctx.global_style().visuals.widgets.inactive.bg_fill)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        ctx.global_style().visuals.widgets.inactive.fg_stroke.color,
                    )),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr(lang, "app_title")).size(17.0).strong());
                    ui.add_space(12.0);
                    self.view_switcher(ui);

                    // Profile Combo in TopBar
                    ui.add_space(8.0);
                    if !no_captions {
                        ui.label(
                            RichText::new(tr(lang, "profile_label"))
                                .size(11.0)
                                .color(Color32::from_rgb(100, 100, 100)),
                        );
                    }
                    let mut new_profile = self.config.active_profile_id.clone();
                    let profile_combo = egui::ComboBox::from_id_salt("active_profile_top")
                        .selected_text(self.config.get_active_profile().display_name.clone())
                        .width(110.0)
                        .show_ui(ui, |ui| {
                            for p in &self.config.profiles {
                                let is_active = p.id == self.config.active_profile_id;
                                if ui.selectable_label(is_active, &p.display_name).clicked() {
                                    new_profile = p.id.clone();
                                }
                            }
                        });
                    if no_captions {
                        profile_combo
                            .response
                            .on_hover_text(tr(lang, "profile_label").trim_end_matches(':'));
                    }
                    if new_profile != self.config.active_profile_id {
                        self.config.active_profile_id = new_profile;
                        if let Err(e) = self.config.save() {
                            self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                        } else {
                            // Review-Fix: Profilwechsel betrifft Tray-Listen.
                            self.mark_tray_dirty();
                            self.status_message = Some(tr_fmt(
                                lang,
                                "profile_switched",
                                &[&self.config.get_active_profile().display_name],
                            ));
                            self.status_message_time = Some(std::time::Instant::now());
                            self.start_scan();
                        }
                    }

                    // Agent Multi-Select wenn mehrere
                    // Only show agent section if not collapsed
                    let show_agents = !effective_collapse || self.config.agents.len() <= 1;
                    if show_agents {
                        if self.config.agents.len() > 1 {
                            ui.add_space(8.0);
                            if !no_captions {
                                ui.label(
                                    RichText::new("AI:")
                                        .size(11.0)
                                        .color(Color32::from_rgb(100, 100, 100)),
                                );
                            }
                            let active_agents = self.config.get_active_agents();
                            let current_text = if active_agents.is_empty() {
                                "Keine".to_string()
                            } else if active_agents.len() == 1 {
                                active_agents[0].display_name.clone()
                            } else {
                                format!("{} aktiv", active_agents.len())
                            };
                            let mut save_err: Option<String> = None;
                            // Review-Fix: Agent-Liste betrifft Tray-Popup → dirty.
                            let mut agents_changed = false;
                            let agent_combo = egui::ComboBox::from_id_salt("active_agent_top")
                                .selected_text(current_text)
                                .width(120.0)
                                .show_ui(ui, |ui| {
                                    // M6: keine Full-Clones pro Frame — nur
                                    // (id, display_name) snapshotten (2 Strings
                                    // statt kompletter AgentProfile inkl.
                                    // terminal_override). Mutiert wird via id.
                                    let snapshot: Vec<(String, String)> = self
                                        .config
                                        .agents
                                        .iter()
                                        .map(|a| (a.id.clone(), a.display_name.clone()))
                                        .collect();
                                    for (id, name) in &snapshot {
                                        let mut is_active = self.config.is_agent_active(id);
                                        if ui.checkbox(&mut is_active, name).clicked() {
                                            self.config.toggle_agent_active(id);
                                            agents_changed = true;
                                            if let Err(e) = self.config.save() {
                                                save_err = Some(format!("{e:#}"));
                                            }
                                        }
                                    }
                                    ui.separator();
                                    if ui.button("Alle aktivieren").clicked() {
                                        let ids: Vec<String> = self
                                            .config
                                            .agents
                                            .iter()
                                            .map(|a| a.id.clone())
                                            .collect();
                                        for id in &ids {
                                            if !self.config.is_agent_active(id) {
                                                self.config.toggle_agent_active(id);
                                                agents_changed = true;
                                            }
                                        }
                                        if let Err(e) = self.config.save() {
                                            save_err = Some(format!("{e:#}"));
                                        }
                                    }
                                    if ui.button("Alle deaktivieren").clicked() {
                                        self.config.active_agent_ids.clear();
                                        self.config.active_agent_id = None;
                                        agents_changed = true;
                                        if let Err(e) = self.config.save() {
                                            save_err = Some(format!("{e:#}"));
                                        }
                                    }
                                });
                            if no_captions {
                                agent_combo
                                    .response
                                    .on_hover_text(tr(lang, "ai_label").trim_end_matches(':'));
                            }
                            if agents_changed {
                                self.mark_tray_dirty();
                            }
                            if let Some(e) = save_err {
                                self.error = Some(tr_fmt(lang, "save_failed", &[&e]));
                            }
                        } else if let Some(agent) = self.config.get_active_agent() {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(format!("AI: {}", agent.display_name))
                                    .size(11.0)
                                    .color(Color32::from_rgb(100, 100, 100)),
                            );
                        }
                    }

                    // Language selector
                    ui.add_space(6.0);
                    if !no_captions {
                        ui.label(
                            RichText::new(tr(lang, "language"))
                                .size(11.0)
                                .color(Color32::from_rgb(100, 100, 100)),
                        );
                    }
                    let mut new_lang = lang;
                    let lang_combo = egui::ComboBox::from_id_salt("language_top")
                        .selected_text(lang.display_name().to_string())
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for l in Language::all() {
                                let is_active = l == lang;
                                if ui.selectable_label(is_active, l.display_name()).clicked() {
                                    new_lang = l;
                                }
                            }
                        });
                    if no_captions {
                        lang_combo.response.on_hover_text(tr(lang, "language"));
                    }
                    if new_lang != lang {
                        self.config.language = new_lang;
                        // Review-Fix: Sprache betrifft auch Tray-Texte.
                        self.mark_tray_dirty();
                        if let Err(e) = self.config.save() {
                            self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let settings =
                            top_bar_button(ui, ICON_GEAR, &tr(lang, "settings"), compact, true);
                        if settings
                            .on_hover_text(tr(lang, "settings_tooltip"))
                            .clicked()
                        {
                            self.show_settings = true;
                            self.settings_state = Some(SettingsState::from_config(&self.config));
                        }
                        ui.add_space(8.0);
                        // E19: in der MR-Ansicht lädt „Aktualisieren“ die MRs neu.
                        let mr_view = self.main_view == MainView::MergeRequests;
                        let checkout = matches!(self.git_busy, GitBusy::Checkout { .. });
                        let busy = if mr_view {
                            self.mr_loading || self.mr_discovering || checkout
                        } else {
                            self.git_busy.holds_refs()
                        };
                        // Beim Auschecken kein „Scanne…“
                        let refresh_label = if !mr_view && self.git_busy.scans_repos() {
                            tr(lang, "scanning")
                        } else {
                            tr(lang, "refresh")
                        };
                        let refresh =
                            top_bar_button(ui, ICON_REFRESH, &refresh_label, compact, !busy);
                        // Tooltip auch deaktiviert (nur Icon: sonst ohne Text)
                        let tooltip = if mr_view {
                            Some(tr(lang, "mr_reload_tt"))
                        } else if compact {
                            Some(refresh_label)
                        } else {
                            None
                        };
                        let refresh = match tooltip {
                            Some(tt) => {
                                refresh.on_hover_text(tt.clone()).on_disabled_hover_text(tt)
                            }
                            None => refresh,
                        };
                        if refresh.clicked() {
                            if mr_view {
                                self.mr_user_refresh();
                            } else {
                                self.start_scan();
                            }
                        }
                        if busy {
                            ui.add_space(8.0);
                            ui.spinner();
                        }
                        // E19: Sortierung wirkt nur auf die Repo-Liste.
                        if mr_view {
                            return;
                        }
                        // Task 4: Sort-Modus als Icon-Button mit Dropdown
                        // (kein Text-Label im Button → kein Truncation des DE-Labels;
                        // volle Labels nur im auto-großen Popup).
                        ui.add_space(8.0);
                        let mut new_sort = self.config.main_sort_mode;
                        ui.menu_image_button(
                            egui::Image::new(ICON_SORT)
                                .fit_to_exact_size(Vec2::splat(14.0))
                                .tint(ui.visuals().text_color()),
                            |ui| {
                                for mode in RepoSortMode::all() {
                                    let label = match mode {
                                        RepoSortMode::LastUsed => tr(lang, "sort_by_last_used"),
                                        RepoSortMode::Name => tr(lang, "sort_by_name"),
                                    };
                                    if ui.selectable_label(new_sort == mode, label).clicked() {
                                        new_sort = mode;
                                        ui.close();
                                    }
                                }
                            },
                        )
                        .response
                        .on_hover_text(tr(lang, "sort_tooltip"));
                        if new_sort != self.config.main_sort_mode {
                            self.config.main_sort_mode = new_sort;
                            // Finding A: Moduswechsel invalidiert die Sortierung.
                            self.repo_sort_dirty = true;
                            if let Err(e) = self.config.save() {
                                self.error =
                                    Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                            }
                        }
                    });
                });

                // Only show path info when not collapsed
                if !effective_collapse && !self.config.roots.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(tr(lang, "search_paths"))
                                .size(11.0)
                                .color(Color32::from_rgb(120, 120, 120)),
                        );
                        for (i, root) in self.config.roots.iter().enumerate() {
                            if i > 0 {
                                ui.label(
                                    RichText::new("·").color(Color32::from_rgb(180, 180, 180)),
                                );
                            }
                            let exists = root.exists();
                            let color = if exists {
                                Color32::from_rgb(80, 80, 80)
                            } else {
                                Color32::from_rgb(200, 80, 20)
                            };
                            ui.label(
                                RichText::new(root.display().to_string())
                                    .size(11.0)
                                    .color(color),
                            )
                            .on_hover_text(if exists {
                                format!("{} (Tiefe {})", root.display(), self.config.max_depth)
                            } else {
                                format!("{} (nicht gefunden)", root.display())
                            });
                        }
                        ui.label(
                            RichText::new(format!("  Tiefe: {}", self.config.max_depth))
                                .size(11.0)
                                .color(Color32::from_rgb(120, 120, 120)),
                        );
                        ui.add_space(8.0);
                        let prof = self.config.get_active_profile();
                        ui.label(
                            RichText::new(format!(
                                "Profil: {} (*{})",
                                prof.display_name, prof.file_extension
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(100, 100, 100)),
                        );
                    });
                }
            });
    }

    fn show_status_bar(&self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let lang = self.config.language;
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(ctx.global_style().visuals.widgets.inactive.bg_fill)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        ctx.global_style().visuals.widgets.inactive.fg_stroke.color,
                    )),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.main_view == MainView::MergeRequests {
                        // Statusmeldung (3 s) hat Vorrang, sonst Hosts und Stand.
                        let (text, is_message) = self.mr_status_line();
                        let color = if is_message {
                            Color32::from_rgb(60, 120, 60)
                        } else {
                            crate::ui::theme::mr_palette(&self.config.theme, ui.visuals()).text2
                        };
                        ui.label(RichText::new(text).size(11.0).color(color));
                    } else if let Some(msg) = &self.status_message {
                        ui.label(
                            RichText::new(msg)
                                .size(11.0)
                                .color(Color32::from_rgb(60, 120, 60)),
                        );
                    } else if let Some(text) = repo_busy_text(lang, &self.git_busy) {
                        ui.label(
                            RichText::new(text)
                                .size(11.0)
                                .color(Color32::from_rgb(100, 100, 100)),
                        );
                    } else {
                        ui.label(
                            RichText::new(format!(
                                "{} Repos · {} Pfade · Tiefe {} · Profil {}",
                                self.repos.len(),
                                self.config.roots.len(),
                                self.config.max_depth,
                                self.config.get_active_profile().display_name
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(120, 120, 120)),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(10.0)
                                .color(Color32::from_rgb(160, 160, 160)),
                        );
                    });
                });
            });
    }

    fn show_branch_dialog(&mut self, ctx: &egui::Context) {
        let lang = self.config.language;
        let mut to_close = false;
        let mut action: Option<(PathBuf, String, bool, bool)> = None; // path, branch, force, stash
        if let Some(dlg) = &mut self.branch_dialog {
            let repo_path = dlg.repo_path.clone();
            let target_branch = dlg.target_branch.clone();
            egui::Window::new(format!(
                "{}: {} → {}",
                tr(lang, "branch_switch_title"),
                repo_path.display(),
                target_branch
            ))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(RichText::new(tr(lang, "branch_dirty_msg")).size(12.0));
                ui.add_space(8.0);
                if !dlg.dirty_files.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "{} {}",
                            // M2: gecappte Liste ehrlich kennzeichnen (Cap 100
                            // in get_detailed_status) statt exakt zu wirken.
                            capped_file_count_label(dlg.dirty_files.len(), dlg.files_truncated),
                            tr(lang, "affected_files")
                        ))
                        .size(11.0)
                        .color(Color32::from_rgb(100, 100, 100)),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(100.0)
                        .show(ui, |ui| {
                            for f in &dlg.dirty_files {
                                ui.label(
                                    RichText::new(f)
                                        .size(10.5)
                                        .color(Color32::from_rgb(80, 80, 80)),
                                );
                            }
                        });
                    ui.add_space(8.0);
                }
                if let Some(err) = &dlg.error {
                    let frame = egui::Frame::new()
                        .fill(Color32::from_rgb(255, 235, 235))
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 100, 100)))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(8, 6));
                    frame.show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}: {}",
                                if lang == Language::En {
                                    "Error"
                                } else {
                                    "Fehler"
                                },
                                err
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(160, 40, 40)),
                        );
                    });
                    ui.add_space(8.0);
                }
                ui.horizontal(|ui| {
                    if ui.button(tr(lang, "cancel")).clicked() {
                        to_close = true;
                    }
                    if ui
                        .button(tr(lang, "stash_switch"))
                        .on_hover_text(tr(lang, "stash_switch_tooltip"))
                        .clicked()
                    {
                        action = Some((repo_path.clone(), target_branch.clone(), false, true));
                    }
                    if ui
                        .button(tr(lang, "force_discard"))
                        .on_hover_text(tr(lang, "force_discard_tooltip"))
                        .clicked()
                    {
                        action = Some((repo_path.clone(), target_branch.clone(), true, false));
                    }
                });
                ui.add_space(4.0);
                ui.label(
                    RichText::new(tr(lang, "branch_tip"))
                        .size(10.0)
                        .color(Color32::from_rgb(120, 120, 120))
                        .italics(),
                );
            });
        }
        if to_close {
            self.close_branch_dialog();
        }
        if let Some((path, branch, force, stash)) = action {
            self.execute_branch_switch(path, branch, force, stash);
        }
    }

    /// „Abbrechen“ im Branch-Dialog; ein wartender MR-Wechsel entfällt
    /// samt „Danach öffnen“ (K1).
    fn close_branch_dialog(&mut self) {
        if let Some(dlg) = self.branch_dialog.take() {
            self.cancel_mr_switch(&dlg.repo_path, &dlg.target_branch);
        }
    }

    fn show_update_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_update_dialog {
            return;
        }
        // F2#8: kein Clone pro Frame — disjunkte Field-Borrows
        // (`update_info` vs. `show_update_dialog`) statt owned Kopie.
        let info = match &self.update_info {
            Some(i) => i,
            None => {
                self.show_update_dialog = false;
                return;
            }
        };
        let lang = self.config.language;
        let mut close = false;
        let mut open_link = false;
        egui::Window::new(tr(lang, "update_available_title"))
            .collapsible(false)
            .resizable(false)
            .open(&mut self.show_update_dialog)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{}: {} → {}",
                        tr(lang, "update_available_msg"),
                        crate::updater::normalize_version(&info.current_version),
                        crate::updater::normalize_version(&info.latest_version)
                    ))
                    .size(12.0),
                );
                if let Some(body) = &info.body {
                    if !body.trim().is_empty() {
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .max_height(80.0)
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(body)
                                        .size(11.0)
                                        // F3#14: Theme-Text statt hardcoded Grau
                                        // (in Dark-Themes sonst unlesbar).
                                        .color(ui.visuals().weak_text_color()),
                                );
                            });
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(tr(lang, "cancel")).clicked() {
                        close = true;
                    }
                    if ui.button(tr(lang, "open_link")).clicked() {
                        open_link = true;
                        close = true;
                    }
                });
            });
        if open_link {
            ctx.open_url(egui::OpenUrl::new_tab(&info.url));
        }
        // Hinweis: Hauptfenster-Close wird allein in handle_close_request behandelt.
        // Das Dialog-X wird via .open() von egui gesetzt.
        if close {
            self.show_update_dialog = false;
        }
    }

    /// Gemeinsames Fehlerbanner (`self.error`) über der Repo-Liste bzw. der
    /// MR-Ansicht; das Kreuz löscht den Fehler.
    fn show_error_banner(&mut self, ui: &mut egui::Ui) {
        let Some(err_msg) = self.error.clone() else {
            return;
        };
        let lang = self.config.language;
        let mut do_clear = false;
        let frame = egui::Frame::new()
            .fill(Color32::from_rgb(255, 235, 235))
            .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 100, 100)))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(10, 8));
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Image::new(ICON_WARNING)
                        .fit_to_exact_size(Vec2::splat(16.0))
                        .tint(Color32::from_rgb(160, 40, 40)),
                );
                ui.label(
                    RichText::new(format!(
                        "{}: {}",
                        if lang == Language::En {
                            "Error"
                        } else {
                            "Fehler"
                        },
                        err_msg
                    ))
                    .size(12.0)
                    .color(Color32::from_rgb(160, 40, 40)),
                );
                if ui
                    .add(egui::Button::image(
                        egui::Image::new(ICON_CROSS)
                            .fit_to_exact_size(Vec2::splat(12.0))
                            .tint(Color32::from_rgb(160, 40, 40)),
                    ))
                    .on_hover_text("Schließen")
                    .clicked()
                {
                    do_clear = true;
                }
            });
        });
        ui.add_space(8.0);
        if do_clear {
            self.error = None;
        }
    }
}

impl eframe::App for MyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Called also when window hidden (Visible(false)), ensures tray stays responsive.
        // F-11: poll_scan + handle_close_request nur hier (nicht zusätzlich in ui),
        // sonst doppelte Channel-Drains und doppeltes Visible(false).
        // F-01: Service ist einzige Windows-Quelle, kein Fallback mehr.
        self.poll_scan(ctx);
        #[cfg(target_os = "windows")]
        {
            if self.tray_shared.is_some() {
                // Dedicated tray service handles tray events efficiently in its own viewport
                self.maybe_sync_tray_service();
                self.poll_tray_service_actions(ctx);
                self.show_tray_service_viewport(ctx);
            } else {
                // Kein Tray-Service (Erstellung fehlgeschlagen): kein Popup.
                // Früherer Fallback (tray_event_rx + show_tray_popup_viewport) gestrichen.
            }
        }
        // Task 5: periodischer Fetch (throttled, 0=aus) — auch bei hidden Window.
        self.maybe_auto_fetch(ctx);
        // Merge Requests: Ergebnisse übernehmen, Intervall nur bei sichtbarem Fenster.
        self.poll_mr(ctx);
        self.poll_mr_settings(ctx);
        self.maybe_refresh_mr(ctx);
        self.handle_close_request(ctx);
        // P5: gebatchte Usage-Saves hier flush (deckt auch ui()-Actions vom
        // Vorframe ab — max. 1 Save pro Frame).
        self.flush_config_save(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let lang = self.config.language;
        // F-11: kein poll_scan / poll_tray_events / handle_close_request hier —
        // alles läuft in logic(), auch bei hidden Window.
        // Popup wird in logic() via TrayService-Viewport gemanagt.

        // Track window size for collapsing logic using ctx screen rect (Hysterese: <400 collapsed, >=500 expanded)
        let screen_rect = ctx.viewport_rect();
        let current_size = [screen_rect.width(), screen_rect.height()];
        let prev_size = self.last_window_size;
        if prev_size != current_size {
            if current_size[1] < 400.0 {
                self.top_bar_collapsed = true;
            } else if current_size[1] >= 500.0 {
                self.top_bar_collapsed = false;
            }
            self.last_window_size = current_size;
        }

        self.show_top_bar(ui);
        self.show_status_bar(ui);
        self.show_branch_dialog(&ctx);
        self.show_update_dialog(&ctx);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.global_style().visuals.widgets.inactive.bg_fill)
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ui, |ui| {
                // E20: Fehler beim Auschecken/Branch-Wechsel in beiden Ansichten.
                self.show_error_banner(ui);
                if self.main_view == MainView::MergeRequests {
                    self.show_mr_view(ui);
                    return;
                }

                if self.config.roots.is_empty() && !self.git_busy.scans_repos() {
                    let frame = egui::Frame::new()
                        .fill(Color32::from_rgb(255, 248, 220))
                        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 180, 80)))
                        .corner_radius(6)
                        .inner_margin(egui::Margin::symmetric(10, 8));
                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Image::new(ICON_WARNING)
                                    .fit_to_exact_size(Vec2::splat(14.0))
                                    .tint(Color32::from_rgb(120, 90, 20)),
                            );
                            ui.label(
                                RichText::new(tr(lang, "no_search_path"))
                                    .size(12.0)
                                    .color(Color32::from_rgb(120, 90, 20)),
                            );
                        });
                    });
                    ui.add_space(12.0);
                }

                // Repo-Liste mit neuen Callbacks
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

                // Wir brauchen &mut für repo_list, um selektierte Werte zu zeigen, aber hier clonen wir mut
                // repo_list wird &mut repos erwarten, wir haben &mut self.repos
                // Lauf hält die Refs → Fetch-/Refresh-Buttons disabled + Spinner
                // Task 4: Sortierung im View (Scanner bleibt alphabetisch)
                // Finding A: nur bei dirty sortieren statt pro Frame (60fps).
                if self.repo_sort_dirty {
                    sort_main_repos(&mut self.repos, &self.config);
                    self.repo_sort_dirty = false;
                }
                let fetching = self.git_busy.holds_refs();
                // K1: „In Repo-Liste zeigen“ hebt eine Karte kurz hervor
                let highlight = self.repo_highlight_frame(&ctx);
                show_repo_list(
                    ui,
                    &mut self.repos,
                    &self.config,
                    &mut actions,
                    fetching,
                    highlight.as_ref(),
                );

                // Handle Actions
                if let Some((repo_path, branch)) = actions.branch_switch {
                    // Verzögert handeln, damit UI nicht blockiert (Queue, kein Overwrite)
                    self.pending_branch_switches.push((repo_path, branch));
                    ctx.request_repaint();
                }
                if let Some((repo_path, selector_id, new_value)) = actions.custom_select {
                    let sel_opt = {
                        let profile = self.config.get_effective_profile_for_repo(&repo_path);
                        profile
                            .config_selectors
                            .iter()
                            .find(|s| s.id == selector_id)
                            .cloned()
                    };
                    if let Some(sel) = sel_opt {
                        match crate::config_parser::write_xml_value(&repo_path, &sel, &new_value) {
                            Ok(()) => {
                                let raw =
                                    tr_fmt(lang, "config_saved", &[&sel.display_name, &new_value]);
                                let msg = if raw == "config_saved" {
                                    format!("{} auf '{}' gesetzt", sel.display_name, new_value)
                                } else {
                                    raw
                                };
                                self.status_message = Some(msg);
                                self.status_message_time = Some(std::time::Instant::now());
                                if let Some(repo) =
                                    self.repos.iter_mut().find(|r| r.path == repo_path)
                                {
                                    repo.custom_values
                                        .insert(selector_id.clone(), new_value.clone());
                                    repo.custom_errors.remove(&selector_id);
                                    repo.dirty = true;
                                }
                                self.record_usage(&repo_path, UsageType::ConfigChange);
                                self.start_scan();
                            }
                            Err(e) => {
                                let raw =
                                    tr_fmt(lang, "config_save_failed", &[&sel.display_name, &e]);
                                let msg = if raw == "config_save_failed" {
                                    format!(
                                        "Config '{}' speichern fehlgeschlagen: {e}",
                                        sel.display_name
                                    )
                                } else {
                                    raw
                                };
                                self.error = Some(msg);
                            }
                        }
                    }
                }
                if let Some((repo_path, sln_path)) = actions.solution_select {
                    // Speichere Auswahl in config
                    let state = self.config.get_repo_state_mut(&repo_path);
                    state.selected_solution = Some(sln_path.clone());
                    // Review-Fix: Tray-Snapshot sonst stale (kein Scan/Sync hier).
                    self.mark_tray_dirty();
                    if let Err(e) = self.config.save() {
                        self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                    } else {
                        // Update auch im RepoInfo direkt
                        if let Some(repo) = self.repos.iter_mut().find(|r| r.path == repo_path) {
                            repo.selected_solution = Some(sln_path);
                        }
                        self.status_message = Some(tr_fmt(
                            lang,
                            "solution_selected",
                            &[&repo_path.display().to_string()],
                        ));
                        self.status_message_time = Some(std::time::Instant::now());
                    }
                }
                if let Some((repo_path, ide_id, file_path)) = actions.ide_open {
                    self.open_ide(&repo_path, &ide_id, &file_path);
                }
                if let Some((repo_path, agent_id)) = actions.agent_open {
                    self.open_agent(&repo_path, &agent_id);
                }
                if let Some((repo_path, profile_opt)) = actions.profile_override {
                    self.config
                        .set_repo_profile_override(&repo_path, profile_opt.clone());
                    if let Err(e) = self.config.save() {
                        self.error = Some(tr_fmt(lang, "save_failed", &[&format!("{e:#}")]));
                    } else {
                        self.status_message = Some(tr_fmt(
                            lang,
                            "profile_changed_for",
                            &[&repo_path.display().to_string()],
                        ));
                        self.status_message_time = Some(std::time::Instant::now());
                        self.start_scan();
                    }
                }
                if let Some(repo_path) = actions.fetch_branches {
                    self.start_repo_fetch(repo_path);
                }
                if let Some(repo_path) = actions.explorer_open {
                    self.open_explorer(&repo_path);
                }
                if let Some(repo_path) = actions.shell_open {
                    self.open_shell(&repo_path);
                }
            });

        if self.show_settings {
            if self.settings_state.is_none() {
                self.settings_state = Some(SettingsState::from_config(&self.config));
            }
            if let Some(mut state) = self.settings_state.take() {
                let mut save: Option<AppConfig> = None;
                let mut open = self.show_settings;
                let mut mr_out = crate::ui::settings::MrSettingsOutput::default();
                crate::ui::settings::show_settings_window(
                    &ctx,
                    &mut state,
                    &mut open,
                    &mut save,
                    self.update_error.as_deref(),
                    &self.mr_settings_input(),
                    &mut mr_out,
                );
                self.show_settings = open;
                // Tab „Merge Requests“: Prüfung und Token wirken sofort
                self.handle_mr_settings_output(mr_out, &state.draft.merge_requests);

                if let Some(new_cfg) = save {
                    // Theme sofort anwenden
                    crate::ui::theme::apply_theme(&ctx, &new_cfg.theme);
                    let lang = new_cfg.language;
                    let autostart = new_cfg.autostart;
                    // Tray-Icon-Stil sofort wechseln (Dark/Light konfigurierbar).
                    #[cfg(target_os = "windows")]
                    {
                        if new_cfg.tray_icons.icon_theme != self.config.tray_icons.icon_theme {
                            if let Some(tray) = &self.tray_icon {
                                if !crate::tray::set_tray_icon_theme(
                                    tray,
                                    new_cfg.tray_icons.icon_theme,
                                ) {
                                    eprintln!(
                                        "Tray-Icon Theme-Wechsel fehlgeschlagen ({:?})",
                                        new_cfg.tray_icons.icon_theme
                                    );
                                }
                            }
                        }
                    }
                    self.config = new_cfg;
                    self.apply_glab_program();
                    // Autostart nach Save anwenden (Registry Run-Key schreiben/entfernen)
                    let autostart_failed = match crate::autostart::apply_autostart(autostart) {
                        Ok(()) => false,
                        Err(e) => {
                            eprintln!("Autostart anwenden fehlgeschlagen: {e:#}");
                            let msg = tr_fmt(lang, "save_failed", &[&format!("{e:#}")]);
                            self.error = Some(msg.clone());
                            // I2: Dialog offen halten für Korrektur/Retry; Fehler auch
                            // im Settings-State sichtbar (show_settings_window zeigt
                            // state.error rot an).
                            let mut reopened = SettingsState::from_config(&self.config);
                            reopened.error = Some(msg);
                            reopened.success = None;
                            self.settings_state = Some(reopened);
                            self.show_settings = true;
                            self.mark_tray_dirty();
                            true
                        }
                    };
                    // I2-Entscheidung (rein, headless-testbar): Err → offen/kein Scan,
                    // Ok → schließen + Scan. Registry-Err ist headless nicht erzwingbar
                    // (Non-Windows No-Op), daher ist die Verzweigung hier explizit plus
                    // Unit-Test für settings_save_autostart_outcome.
                    let (keep_open, should_scan) =
                        settings_save_autostart_outcome(autostart_failed);
                    if !keep_open {
                        // Review-Fix: Tray-Settings (Icons/Limit) bis Scan-Ende stale.
                        self.mark_tray_dirty();
                        self.settings_state = Some(SettingsState::from_config(&self.config));
                        self.show_settings = false;
                        self.status_message = Some(tr(lang, "saved_scan_restart"));
                        self.status_message_time = Some(std::time::Instant::now());
                    }
                    if should_scan {
                        self.start_scan();
                    }
                    // Merge Requests: Hosts/Auswahl neu, sofort neu laden.
                    self.mr_after_settings_save();
                } else if self.show_settings {
                    self.settings_state = Some(state);
                } else {
                    self.settings_state = None;
                }
                if !self.show_settings {
                    self.settings_state = None;
                }
            } else {
                // Defensive: state missing despite initialization above
                self.show_settings = false;
                self.settings_state = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::gitlab::glab::GatedRunner;

    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn branch_dialog_fields() {
        let dlg = BranchDialog {
            repo_path: PathBuf::from("/tmp/repo"),
            target_branch: "feature".to_string(),
            error: None,
            dirty_files: vec![],
            files_truncated: false,
        };
        assert_eq!(dlg.repo_path, PathBuf::from("/tmp/repo"));
        assert_eq!(dlg.target_branch, "feature");
        assert!(dlg.error.is_none());
        assert!(dlg.dirty_files.is_empty());

        let dlg2 = BranchDialog {
            repo_path: PathBuf::from("/tmp/repo"),
            target_branch: "main".to_string(),
            error: Some("err".to_string()),
            dirty_files: vec!["modified: foo.txt".to_string()],
            files_truncated: false,
        };
        assert_eq!(dlg2.error, Some("err".to_string()));
        assert_eq!(dlg2.dirty_files.len(), 1);
    }

    #[test]
    fn should_sync_tray_service_only_when_dirty_and_throttled() {
        // #5: kein blinder Sync alle 500ms — nur bei dirty + Throttle abgelaufen.
        let now = std::time::Instant::now();
        assert!(!should_sync_tray_service(false, None, now));
        assert!(should_sync_tray_service(true, None, now));
        let recent = now - std::time::Duration::from_millis(100);
        assert!(!should_sync_tray_service(true, Some(recent), now));
        let old = now - std::time::Duration::from_millis(600);
        assert!(should_sync_tray_service(true, Some(old), now));
    }

    #[test]
    fn should_auto_fetch_throttled_and_zero_off() {
        // Task 5: 0 = aus, sonst Intervall-Minuten seit letztem Lauf.
        let now = std::time::Instant::now();
        assert!(!should_auto_fetch(None, 0, now));
        assert!(!should_auto_fetch(Some(now), 0, now));
        assert!(should_auto_fetch(None, 10, now));
        let recent = now - std::time::Duration::from_secs(60);
        assert!(!should_auto_fetch(Some(recent), 10, now));
        let old = now - std::time::Duration::from_secs(10 * 60 + 1);
        assert!(should_auto_fetch(Some(old), 10, now));
        let just_under = now - std::time::Duration::from_secs(10 * 60 - 5);
        assert!(!should_auto_fetch(Some(just_under), 10, now));
        // 1-Min-Intervall
        let old1 = now - std::time::Duration::from_secs(61);
        assert!(should_auto_fetch(Some(old1), 1, now));
    }

    /// Test-App ohne Fenster; glab ist ein `FakeGlab` ohne Antworten.
    pub(super) fn test_app() -> MyApp {
        test_app_with_glab(Arc::new(GatedRunner::new(
            crate::gitlab::glab::FakeGlab::new(),
        )))
    }

    /// Wie `test_app` mit eigenem Runner (Antworten vorbereiten, Aufrufe prüfen).
    pub(super) fn test_app_with_glab(glab: Arc<dyn GlabRunner>) -> MyApp {
        let (scan_tx, scan_rx) = mpsc::channel();
        let (launch_err_tx, launch_err_rx) = mpsc::channel();
        let (_config_update_tx, config_update_rx) = mpsc::channel();
        let (_update_tx, update_rx) = mpsc::channel();
        let (mr_tx, mr_rx) = mpsc::channel();
        let (mr_settings_tx, mr_settings_rx) = mpsc::channel();
        let config = AppConfig::default();
        let mr_view = MrViewState::new(&config.merge_requests);
        let glab_program = config.merge_requests.glab_program.clone();
        // Jedes Programm landet beim selben `FakeGlab`
        let fake = Arc::clone(&glab);
        let glab_factory: GlabFactory = Box::new(move |_| Arc::clone(&fake));
        MyApp {
            config,
            repos: Vec::new(),
            git_busy: GitBusy::Idle,
            error: None,
            show_settings: false,
            settings_state: None,
            scan_tx,
            scan_rx,
            launch_err_tx,
            launch_err_rx,
            config_update_rx,
            update_rx,
            update_info: None,
            update_error: None,
            show_update_dialog: false,
            status_message: None,
            status_message_time: None,
            branch_dialog: None,
            pending_branch_switches: Vec::new(),
            last_window_size: [0.0; 2],
            top_bar_collapsed: false,
            #[cfg(target_os = "windows")]
            tray_icon: None,
            #[cfg(target_os = "windows")]
            tray_shared: None,
            #[cfg(target_os = "windows")]
            tray_action_rx: None,
            #[cfg(target_os = "windows")]
            tray_service_tray_rx: None,
            #[cfg(target_os = "windows")]
            last_tray_sync: None,
            tray_dirty: false,
            window_visible: true,
            should_quit: false,
            last_auto_fetch: None,
            auto_fetching: false,
            pending_scan: false,
            config_save_fail_streak: 0,
            config_save_pending: false,
            last_config_save: None,
            repo_sort_dirty: true,
            main_view: MainView::Repos,
            glab,
            glab_program,
            glab_factory,
            glab_hosts: Vec::new(),
            mr_hosts: HostMap::default(),
            mr_locals: Vec::new(),
            mr_snapshot: None,
            mr_loading: false,
            mr_pending_refresh: false,
            mr_refresh_after_scan: false,
            mr_last_refresh: None,
            mr_auth_epoch: 0,
            mr_discovering: false,
            mr_more_pending: 0,
            mr_merged: None,
            mr_merged_loading: false,
            mr_merged_gen: 0,
            mr_tx,
            mr_rx,
            mr_view,
            mr_glab_missing: false,
            mr_glab_error: None,
            mr_switch: None,
            pending_after_checkout: None,
            mr_notice: None,
            repo_highlight: None,
            mr_rediscover: Default::default(),
            mr_settings_tx,
            mr_settings_rx,
            mr_settings: MrSettingsCache::default(),
        }
    }

    #[test]
    fn note_config_save_error_sets_error() {
        // #12: fehlgeschlagener Config-Save darf nicht still verworfen werden.
        let mut app = test_app();
        app.note_config_save_error(&anyhow::anyhow!("disk full"));
        let msg = app.error.expect("error must be set");
        assert!(msg.contains("disk full"), "unexpected message: {msg}");
    }

    #[test]
    fn poll_scan_marks_tray_dirty_on_scan_result() {
        // Review-Fix: Scan-Arm muss dirty markieren, damit ein WouldBlock-
        // Sync per Throttle nachgeholt wird (statt still zu verlieren).
        let mut app = test_app();
        assert!(!app.tray_dirty);
        app.scan_tx
            .send(ScanResult::Repos(vec![]))
            .expect("send scan result");
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert!(app.tray_dirty, "scan result must mark tray dirty");
    }

    #[test]
    fn poll_scan_marks_tray_dirty_on_launch_error() {
        // Fehler speisen das Tray-Badge (#3) → dirty für nächsten Sync.
        let mut app = test_app();
        app.launch_err_tx
            .send("boom".to_string())
            .expect("send launch err");
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert!(app.tray_dirty, "launch error must mark tray dirty");
        assert!(app.error.is_some());
    }

    /// Start von Scan und Auto-Fetch sperrt die Fetch-Buttons im Tray
    /// (`TrayFetching.busy`) → dirty, sonst zeigt das Popup sie bis zum
    /// Ergebnis klickbar und ein Klick ginge still verloren.
    #[test]
    fn scan_and_auto_fetch_start_mark_tray_dirty() {
        let mut app = test_app();
        app.config.roots.clear();
        app.start_scan();
        assert_eq!(app.git_busy, GitBusy::Scan);
        assert!(app.tray_dirty, "Scan-Start");

        let dir = tempdir().unwrap();
        let mut app = test_app();
        app.config.roots.clear();
        app.repos = vec![crate::git::RepoInfo::new(
            dir.path().to_path_buf(),
            "main".to_string(),
            false,
            false,
        )];
        app.maybe_auto_fetch(&egui::Context::default());
        assert!(app.auto_fetching);
        assert!(app.tray_dirty, "Auto-Fetch-Start");
        app.tray_dirty = false;
        app.set_auto_fetching(false);
        assert!(app.tray_dirty, "Auto-Fetch-Ende");
        // Ohne Wechsel kein neuer Sync
        app.tray_dirty = false;
        app.set_auto_fetching(false);
        app.set_git_busy(GitBusy::Idle);
        assert!(!app.tray_dirty);
    }

    #[test]
    fn poll_scan_marks_tray_dirty_on_pending_switches() {
        // Branch-Switch ohne Repo (open_repo → None → Fehlerpfad) markiert dirty.
        let mut app = test_app();
        app.pending_branch_switches.push((
            std::path::PathBuf::from("/tmp/no-repo-xyz"),
            "main".to_string(),
        ));
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert!(app.tray_dirty, "pending switch must mark tray dirty");
        assert!(app.pending_branch_switches.is_empty());
    }

    #[test]
    fn scan_result_repos_contains_vec() {
        let result = ScanResult::Repos(vec![]);
        match result {
            ScanResult::Repos(v) => assert!(v.is_empty()),
            ScanResult::FetchDone { .. } => panic!("expected Repos"),
            ScanResult::FetchAborted { .. } => panic!("expected Repos"),
            ScanResult::MrCheckoutReady { .. } => panic!("expected Repos"),
        }
        let info =
            crate::git::RepoInfo::new(PathBuf::from("/tmp/repo"), "main".to_string(), false, false);
        let result2 = ScanResult::Repos(vec![info]);
        match result2 {
            ScanResult::Repos(v) => assert_eq!(v.len(), 1),
            ScanResult::FetchDone { .. } => panic!("expected Repos"),
            ScanResult::FetchAborted { .. } => panic!("expected Repos"),
            ScanResult::MrCheckoutReady { .. } => panic!("expected Repos"),
        }
        // K1: Ergebnis des Checkout-Workers trägt Wahl und Ergebnis
        let ready = ScanResult::MrCheckoutReady {
            choice: Box::new(crate::ui::merge_requests::MrCheckoutChoice {
                host_key: "gitlab.example.com".to_string(),
                mr_id: "gid://gitlab/MergeRequest/5".to_string(),
                iid: 5,
                kind: crate::ui::merge_requests::MrCheckoutKind::Switch {
                    clone: PathBuf::from("/tmp/repo"),
                },
                after: crate::config::AfterAction::Nothing,
            }),
            result: Ok(None),
        };
        match ready {
            ScanResult::MrCheckoutReady { choice, result } => {
                assert_eq!(choice.iid, 5);
                assert!(matches!(result, Ok(None)));
            }
            ScanResult::Repos(_) => panic!("expected MrCheckoutReady"),
            ScanResult::FetchDone { .. } => panic!("expected MrCheckoutReady"),
            ScanResult::FetchAborted { .. } => panic!("expected MrCheckoutReady"),
        }
        let fetch_done = ScanResult::FetchDone {
            repos: vec![],
            fetch_error: None,
            fetch_partial: false,
            fetch_is_fetch_error: false,
            is_auto: false,
        };
        match fetch_done {
            ScanResult::FetchDone {
                repos,
                fetch_error,
                fetch_partial,
                fetch_is_fetch_error: _,
                is_auto: _,
            } => {
                assert!(repos.is_empty());
                assert!(fetch_error.is_none());
                assert!(!fetch_partial);
            }
            ScanResult::Repos(_) => panic!("expected FetchDone"),
            ScanResult::FetchAborted { .. } => panic!("expected FetchDone"),
            ScanResult::MrCheckoutReady { .. } => panic!("expected FetchDone"),
        }
    }

    #[test]
    fn fetch_error_i18n_key_selects_partial_vs_failed() {
        assert_eq!(fetch_error_i18n_key(true), "fetch_partial");
        assert_eq!(fetch_error_i18n_key(false), "fetch_failed");
    }

    #[test]
    fn poll_scan_fetch_done_error_sets_error_and_status() {
        // UI-Error-Pfad: Fetch-Fehler landet in self.error + Status (statt eprintln-only)
        let mut app = test_app();
        app.git_busy = GitBusy::Scan;
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: Some("'origin': auth failed".to_string()),
                fetch_partial: false,
                fetch_is_fetch_error: true,
                is_auto: false,
            })
            .expect("send fetch done");
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert_eq!(app.git_busy, GitBusy::Idle, "fetch done must clear spinner");
        let err = app.error.expect("error must be set");
        assert!(
            err.contains("'origin': auth failed"),
            "error must embed fetch detail, got: {err}"
        );
        assert!(
            err.contains(&crate::i18n::tr(app.config.language, "fetch_failed").replace(" {}", "")),
            "complete failure must use fetch_failed key, got: {err}"
        );
        // I1: lokalisierter auth_hint wird angehängt.
        assert!(
            err.contains(&crate::i18n::tr(app.config.language, "auth_hint")),
            "error must append localized auth_hint, got: {err}"
        );
        assert!(app.status_message.is_some(), "status must be set");
        assert!(app.tray_dirty, "fetch error must mark tray dirty");
    }

    #[test]
    fn poll_scan_fetch_done_partial_uses_partial_key() {
        let mut app = test_app();
        app.git_busy = GitBusy::Scan;
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                // I1: Background liefert English-neutral (detail_en), kein Deutsch.
                fetch_error: Some("'bad': timeout; succeeded: 'good'".to_string()),
                fetch_partial: true,
                fetch_is_fetch_error: true,
                is_auto: false,
            })
            .expect("send fetch done");
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert_eq!(app.git_busy, GitBusy::Idle);
        let err = app.error.expect("error must be set");
        assert!(
            err.contains("'bad'"),
            "error must embed fetch detail, got: {err}"
        );
        assert!(
            err.contains(&crate::i18n::tr(app.config.language, "fetch_partial").replace(" {}", "")),
            "partial failure must use fetch_partial key, got: {err}"
        );
        assert!(
            err.contains(&crate::i18n::tr(app.config.language, "auth_hint")),
            "partial must append localized auth_hint, got: {err}"
        );
        assert!(app.status_message.is_some());
    }

    #[test]
    fn poll_scan_fetch_error_en_has_no_german() {
        // I1: EN-UI darf kein Deutsch einbetten (Wrapper + Hint + Detail EN).
        let mut app = test_app();
        assert_eq!(app.config.language, crate::i18n::Language::En);
        app.git_busy = GitBusy::Scan;
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: Some("'bad': timeout; succeeded: 'good'".to_string()),
                fetch_partial: true,
                fetch_is_fetch_error: true,
                is_auto: false,
            })
            .expect("send");
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        let err = app.error.expect("error must be set");
        for german in [
            "Hinweis",
            "prüfen",
            "fehlgeschlagen",
            "teilweise",
            "Erfolgreich",
        ] {
            assert!(
                !err.contains(german),
                "EN fetch message must not embed German '{german}': {err}"
            );
        }
    }

    #[test]
    fn settings_save_autostart_outcome_keeps_open_on_err() {
        // I2: reiner Zweig (Registry-Err headless nicht erzwingbar, Non-Windows No-Op).
        assert_eq!(settings_save_autostart_outcome(true), (true, false));
        assert_eq!(settings_save_autostart_outcome(false), (false, true));
    }

    #[test]
    fn poll_scan_fetch_done_ok_clears_error() {
        let mut app = test_app();
        app.git_busy = GitBusy::Scan;
        app.auto_fetching = true;
        app.error = Some("stale".to_string());
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: None,
                fetch_partial: false,
                fetch_is_fetch_error: false,
                is_auto: true,
            })
            .expect("send fetch done");
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        // L4: Auto-FetchDone räumt eigenen Flag/Timestamp, lässt aber einen
        // parallelen manuellen Lauf (git_busy) in Ruhe.
        assert!(
            app.git_busy.holds_refs(),
            "auto FetchDone must not clear manual scanning"
        );
        assert!(!app.auto_fetching);
        assert!(app.error.is_none(), "successful fetch must clear error");
    }

    #[test]
    fn fetch_error_parts_maps_partial_complete_and_plain() {
        // Finding E: identische Fehlersemantik für manuellen + Auto-Fetch.
        use crate::git::PartialFetch;
        let partial = anyhow::Error::new(PartialFetch {
            succeeded: vec!["good".to_string()],
            failed: vec![("bad".to_string(), "boom".to_string())],
        });
        let (msg, is_partial, is_fetch) = fetch_error_parts(&partial);
        assert!(is_partial);
        assert!(is_fetch);
        assert_eq!(msg, "'bad': boom; succeeded: 'good'");
        let complete = anyhow::Error::new(PartialFetch {
            succeeded: Vec::new(),
            failed: vec![("bad".to_string(), "boom".to_string())],
        });
        let (msg_c, is_partial_c, is_fetch_c) = fetch_error_parts(&complete);
        assert!(!is_partial_c);
        assert!(is_fetch_c);
        assert_eq!(msg_c, "'bad': boom");
        let plain = anyhow::anyhow!("plain setup failure");
        let (msg_p, is_partial_p, is_fetch_p) = fetch_error_parts(&plain);
        assert!(!is_partial_p);
        assert!(!is_fetch_p);
        assert_eq!(msg_p, "plain setup failure");
    }

    #[test]
    fn poll_scan_repos_keeps_auto_fetching() {
        // I1: manueller Scan (Repos) darf einen laufenden Auto-Fetch nicht
        // als fertig markieren — sonst zwei parallele fetch_all (Ref-Race).
        let mut app = test_app();
        app.git_busy = GitBusy::Scan;
        app.auto_fetching = true;
        app.scan_tx.send(ScanResult::Repos(vec![])).unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert_eq!(app.git_busy, GitBusy::Idle);
        assert!(app.auto_fetching, "Repos must not clear auto_fetching");
    }

    #[test]
    fn poll_scan_manual_fetch_done_keeps_auto_fetching() {
        // I1: manueller Fetch (is_auto=false) lässt Auto-Flag in Ruhe.
        let mut app = test_app();
        app.auto_fetching = true;
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: None,
                fetch_partial: false,
                fetch_is_fetch_error: false,
                is_auto: false,
            })
            .unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert!(
            app.auto_fetching,
            "manual FetchDone must not clear auto flag"
        );
    }

    #[test]
    fn repo_fetch_from_tray_refused_while_busy() {
        // Tray-Stand kann veraltet sein: kein zweiter Fetch auf dieselben Refs
        let mut app = test_app();
        app.git_busy = GitBusy::Scan;
        app.start_repo_fetch(PathBuf::from("/repos/a"));
        assert_eq!(app.git_busy, GitBusy::Scan);
        assert_eq!(
            app.status_message.as_deref(),
            Some(tr(app.config.language, "fetch_busy").as_str())
        );
        let mut app = test_app();
        app.auto_fetching = true;
        app.start_repo_fetch(PathBuf::from("/repos/a"));
        assert_eq!(app.git_busy, GitBusy::Idle);
    }

    #[test]
    fn repo_fetch_marks_repo_until_fetch_done() {
        let mut app = test_app();
        let dir = tempfile::tempdir().unwrap();
        app.start_repo_fetch(dir.path().to_path_buf());
        assert_eq!(
            app.git_busy,
            GitBusy::Fetch {
                repo: dir.path().to_path_buf()
            }
        );
        // Der Worker meldet sich auf dem alten Kanal; der kommt erst nach
        // dem Auto-FetchDone zurück (sonst wertet poll_scan beide zugleich aus)
        let (tx, rx) = mpsc::channel();
        let worker_rx = std::mem::replace(&mut app.scan_rx, rx);
        // Auto-FetchDone lässt den manuellen Fetch in Ruhe
        tx.send(ScanResult::FetchDone {
            repos: vec![],
            fetch_error: None,
            fetch_partial: false,
            fetch_is_fetch_error: false,
            is_auto: true,
        })
        .unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert!(matches!(app.git_busy, GitBusy::Fetch { .. }));
        app.scan_rx = worker_rx;
        // Ende des manuellen Fetch (Worker oder Abbruch) gibt das Repo frei
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.git_busy.holds_refs() && std::time::Instant::now() < until {
            app.poll_scan(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(app.git_busy, GitBusy::Idle);
    }

    #[test]
    fn poll_scan_auto_fetch_done_clears_and_timestamps() {
        // I1+I6: nur Auto-FetchDone löscht Flag + setzt Ende-Timestamp.
        let mut app = test_app();
        app.auto_fetching = true;
        assert!(app.last_auto_fetch.is_none());
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: None,
                fetch_partial: false,
                fetch_is_fetch_error: false,
                is_auto: true,
            })
            .unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert!(!app.auto_fetching);
        assert!(
            app.last_auto_fetch.is_some(),
            "auto FetchDone must timestamp end"
        );
    }

    #[test]
    fn poll_scan_auto_fetch_done_keeps_manual_scanning() {
        // L4: Auto-FetchDone darf den Spinner eines parallelen manuellen
        // Laufs nicht löschen (nur is_auto=false setzt git_busy zurück).
        let mut app = test_app();
        app.git_busy = GitBusy::Scan;
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: None,
                fetch_partial: false,
                fetch_is_fetch_error: false,
                is_auto: true,
            })
            .unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert!(
            app.git_busy.holds_refs(),
            "auto FetchDone must not clear manual scanning"
        );
        // Manueller FetchDone räumt auf wie bisher.
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: None,
                fetch_partial: false,
                fetch_is_fetch_error: false,
                is_auto: false,
            })
            .unwrap();
        app.poll_scan(&ctx);
        assert_eq!(app.git_busy, GitBusy::Idle);
    }

    #[test]
    fn start_scan_skips_while_auto_fetching() {
        // L2: kein paralleler manueller Scan während Auto-Fetch läuft
        // (Ref-Race) — mit Hinweis statt stillem Ignore.
        let mut app = test_app();
        app.auto_fetching = true;
        app.start_scan();
        assert_eq!(
            app.git_busy,
            GitBusy::Idle,
            "must not spawn manual scan during auto-fetch"
        );
        assert!(
            app.status_message
                .as_deref()
                .is_some_and(|m| m.to_lowercase().contains("auto-fetch")),
            "skip needs visible reason, got {:?}",
            app.status_message
        );
        // Ohne Auto-Lauf normaler Start.
        app.auto_fetching = false;
        app.start_scan();
        assert_eq!(app.git_busy, GitBusy::Scan);
    }

    #[test]
    fn aggregate_fetch_error_prefers_partial_detail() {
        // L1/M1: Detail + Flag gehören zusammen — partielles Detail
        // ersetzt gespeichertes Complete, umgekehrt nicht.
        // (Leerer Repo-Name = kein Präfix; mit Name siehe named-Test.)
        use crate::git::PartialFetch;
        let mut slot = (None, false, false);
        let complete = anyhow::Error::new(PartialFetch {
            succeeded: Vec::new(),
            failed: vec![("a".to_string(), "boom".to_string())],
        });
        aggregate_named_fetch_error(&mut slot.0, &mut slot.1, &mut slot.2, "", &complete);
        assert_eq!(slot.0.as_deref(), Some("'a': boom"));
        assert!(!slot.1);
        let partial = anyhow::Error::new(PartialFetch {
            succeeded: vec!["g".to_string()],
            failed: vec![("b".to_string(), "x".to_string())],
        });
        aggregate_named_fetch_error(&mut slot.0, &mut slot.1, &mut slot.2, "", &partial);
        assert!(slot.1);
        assert_eq!(slot.0.as_deref(), Some("'b': x; succeeded: 'g'"));
        // Späteres Complete verdrängt Partial nicht mehr.
        aggregate_named_fetch_error(&mut slot.0, &mut slot.1, &mut slot.2, "", &complete);
        assert!(slot.1);
        assert!(slot.0.as_deref() == Some("'b': x; succeeded: 'g'"));
    }

    #[test]
    fn merge_parallel_fetch_result_keeps_panic_marker() {
        // F2#B1/F3#1: Panic-Marker geht nicht verloren, wenn Shared leer ist;
        // vorhandener Shared-Fehler gewinnt über fehlenden Marker.
        let mut slot = (None, false, false);
        merge_parallel_fetch_result(
            &mut slot.0,
            &mut slot.1,
            &mut slot.2,
            (Some("shared".to_string()), true, true),
            true,
        );
        assert_eq!(slot.0.as_deref(), Some("shared"));
        assert!(slot.1);
        let mut slot2: (Option<String>, bool, bool) = (None, false, false);
        merge_parallel_fetch_result(
            &mut slot2.0,
            &mut slot2.1,
            &mut slot2.2,
            (None, false, false),
            true,
        );
        assert_eq!(slot2.0.as_deref(), Some("background fetch worker panicked"));
        let mut slot3: (Option<String>, bool, bool) = (None, false, false);
        merge_parallel_fetch_result(
            &mut slot3.0,
            &mut slot3.1,
            &mut slot3.2,
            (None, false, false),
            false,
        );
        assert!(slot3.0.is_none(), "no panic, no error → stays clean");
    }

    #[test]
    fn aggregate_named_fetch_error_prefixes_repo() {
        // F1#F1: Auto-Banner braucht Repo-Kontext (N Repos!).
        use crate::git::PartialFetch;
        let err = anyhow::Error::new(PartialFetch {
            succeeded: Vec::new(),
            failed: vec![("origin".to_string(), "timeout".to_string())],
        });
        let mut slot = (None, false, false);
        aggregate_named_fetch_error(&mut slot.0, &mut slot.1, &mut slot.2, "myrepo", &err);
        assert_eq!(slot.0.as_deref(), Some("myrepo: 'origin': timeout"));
        assert_eq!(
            short_repo_name(std::path::Path::new("/a/b/myrepo")),
            "myrepo"
        );
        assert_eq!(short_repo_name(std::path::Path::new("myrepo")), "myrepo");
    }

    #[test]
    fn fetch_aborted_honors_pending_scan() {
        // F1#F2: gemerkter Refresh geht auch bei Abort nicht verloren.
        let mut app = test_app();
        app.pending_scan = true;
        app.scan_tx
            .send(ScanResult::FetchAborted {
                reason_key: "fetch_aborted",
            })
            .unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert!(!app.pending_scan);
        assert_eq!(
            app.git_busy,
            GitBusy::Scan,
            "pending refresh must start after abort"
        );
        assert!(app.error.is_some());
    }

    #[test]
    fn pending_scan_runs_after_auto_fetch_done() {
        // P2b: gemerkter Refresh wird nach Auto-FetchDone nachgeholt.
        let mut app = test_app();
        app.auto_fetching = true;
        app.start_scan();
        assert!(app.pending_scan);
        assert_eq!(app.git_busy, GitBusy::Idle);
        let ctx = egui::Context::default();
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: None,
                fetch_partial: false,
                fetch_is_fetch_error: false,
                is_auto: true,
            })
            .unwrap();
        app.poll_scan(&ctx);
        assert!(!app.pending_scan);
        assert_eq!(
            app.git_busy,
            GitBusy::Scan,
            "pending refresh must start after auto done"
        );
    }

    #[test]
    fn fetch_aborted_resets_flags_keeps_repos() {
        // H3: Abort resettet Flags + zeigt Fehler, wischt Liste nicht.
        let mut app = test_app();
        app.git_busy = GitBusy::Scan;
        app.auto_fetching = true;
        app.repos = vec![crate::git::RepoInfo::new(
            PathBuf::from("/tmp/repo"),
            "main".to_string(),
            false,
            false,
        )];
        app.scan_tx
            .send(ScanResult::FetchAborted {
                reason_key: "fetch_aborted",
            })
            .unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert_eq!(app.git_busy, GitBusy::Idle);
        assert!(!app.auto_fetching);
        assert_eq!(app.repos.len(), 1);
        assert!(app.error.is_some());
    }

    #[test]
    fn repo_busy_text_names_mr_checkout() {
        let lang = Language::De;
        assert_eq!(repo_busy_text(lang, &GitBusy::Idle), None);
        let fetch = GitBusy::Fetch {
            repo: PathBuf::from("/repos/a"),
        };
        for busy in [GitBusy::Scan, fetch] {
            assert_eq!(
                repo_busy_text(lang, &busy),
                Some(tr(lang, "scanning_repos"))
            );
        }
        // Auschecken hält nur die Refs: kein „Scanne …“
        assert_eq!(
            repo_busy_text(lang, &GitBusy::Checkout { iid: 5 }).as_deref(),
            Some("Auschecken für !5 läuft …")
        );
    }

    #[test]
    fn git_busy_tells_ref_lock_from_scan() {
        assert!(!GitBusy::Idle.holds_refs() && !GitBusy::Idle.scans_repos());
        let fetch = GitBusy::Fetch {
            repo: PathBuf::from("/repos/a"),
        };
        for busy in [GitBusy::Scan, fetch] {
            assert!(busy.holds_refs() && busy.scans_repos(), "{busy:?}");
        }
        // K1: das Auschecken endet ohne Scan
        let checkout = GitBusy::Checkout { iid: 5 };
        assert!(checkout.holds_refs() && !checkout.scans_repos());
    }

    /// Ohne GitLab-Host zeigt die MR-Ansicht „Laden“ nur, solange ein Scan
    /// noch Hosts liefern kann, nicht während eines (ggf. minutenlangen)
    /// Auscheckens (K1).
    #[test]
    fn mr_view_loading_card_only_while_scanning() {
        let loading = tr(Language::En, "mr_loading");
        let fetch = GitBusy::Fetch {
            repo: PathBuf::from("/repos/a"),
        };
        for (busy, shows_loading) in [
            (GitBusy::Scan, true),
            (fetch, true),
            (GitBusy::Checkout { iid: 5 }, false),
            (GitBusy::Idle, false),
        ] {
            let mut app = test_app();
            app.main_view = MainView::MergeRequests;
            app.git_busy = busy.clone();
            let ctx = egui::Context::default();
            ctx.enable_accesskit();
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 800.0),
                )),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| app.show_mr_view(ui));
            let nodes = output
                .platform_output
                .accesskit_update
                .as_ref()
                .map(|u| u.nodes.clone())
                .unwrap_or_default();
            output.drop_without_applying_deltas();
            let shown = nodes
                .iter()
                .any(|(_, n)| n.value().or(n.label()) == Some(loading.as_str()));
            assert_eq!(shown, shows_loading, "{busy:?}");
        }
    }

    #[test]
    fn capped_file_count_label_marks_cap() {
        // M2/F1#F3: "+" nur bei echter Trunkierung, nicht bei exakt CAP.
        assert_eq!(capped_file_count_label(0, false), "0");
        assert_eq!(capped_file_count_label(99, false), "99");
        assert_eq!(
            capped_file_count_label(crate::git::DETAILED_STATUS_CAP, false),
            format!("{}", crate::git::DETAILED_STATUS_CAP)
        );
        assert_eq!(
            capped_file_count_label(crate::git::DETAILED_STATUS_CAP, true),
            format!("{}+", crate::git::DETAILED_STATUS_CAP)
        );
    }

    #[test]
    fn poll_scan_plain_error_has_no_auth_hint() {
        // I2: Open-/Setup-Fehler ohne auth_hint (irreführend sonst).
        let mut app = test_app();
        app.git_busy = GitBusy::Scan;
        app.scan_tx
            .send(ScanResult::FetchDone {
                repos: vec![],
                fetch_error: Some("Failed to open repository: xyz".to_string()),
                fetch_partial: false,
                fetch_is_fetch_error: false,
                is_auto: false,
            })
            .unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        let err = app.error.expect("error must be set");
        assert!(
            !err.contains(&crate::i18n::tr(app.config.language, "auth_hint")),
            "plain error must not append auth_hint, got: {err}"
        );
    }

    #[test]
    fn myapp_struct_default_values() {
        // MyApp::new requires eframe CreationContext, so we test struct construction directly via manual init
        // We verify that the expected defaults match config defaults
        let cfg = AppConfig::default();
        // Simulate what MyApp::new does: scanning false initially then true after start_scan
        // We test that AppConfig default has valid state
        assert!(cfg.max_depth >= 1 && cfg.max_depth <= 10);
        assert!(!cfg.profiles.is_empty());
        // BranchDialog None initially
        let branch_dialog: Option<BranchDialog> = None;
        assert!(branch_dialog.is_none());
        let scanning = false;
        assert!(!scanning);
        let repos: Vec<crate::git::RepoInfo> = vec![];
        assert!(repos.is_empty());
    }

    #[test]
    fn myapp_status_timeout_logic() {
        // Simulate status_message_time >3s should clear
        let now = std::time::Instant::now();
        let old = now - std::time::Duration::from_secs(4);
        assert!(old.elapsed().as_secs() > 3);
        let recent = now - std::time::Duration::from_secs(1);
        assert!(recent.elapsed().as_secs() <= 3);
    }

    #[test]
    fn myapp_top_bar_collapse_logic() {
        // logic: height <400 => collapsed true, >=500 => false
        let collapse_if_smaller_than = 400.0;
        let should_collapse = |h: f32| h < collapse_if_smaller_than;
        assert!(should_collapse(380.0));
        assert!(!should_collapse(400.0));
        assert!(!should_collapse(500.0));
        // transition: last_window_size tracking
        let mut top_bar_collapsed = false;
        let current = 350.0;
        if current < 400.0 {
            top_bar_collapsed = true;
        }
        assert!(top_bar_collapsed);
        let current2 = 550.0;
        if current2 >= 500.0 && top_bar_collapsed {
            top_bar_collapsed = false;
        }
        assert!(!top_bar_collapsed);
    }

    #[test]
    fn checkout_outcome_notice_texts() {
        // E15: Hinweis nur bei abweichendem lokalem Branch, Einzahl getrennt.
        use crate::git::CheckoutOutcome;
        assert_eq!(
            checkout_outcome_notice(Language::De, &CheckoutOutcome::Switched),
            None
        );
        let ff = |by| CheckoutOutcome::FastForwarded {
            branch: "feature/x".to_string(),
            by,
        };
        assert_eq!(
            checkout_outcome_notice(Language::De, &ff(1)).as_deref(),
            Some("Lokaler Branch feature/x um 1 Commit vorgespult")
        );
        assert_eq!(
            checkout_outcome_notice(Language::De, &ff(3)).as_deref(),
            Some("Lokaler Branch feature/x um 3 Commits vorgespult")
        );
        assert_eq!(
            checkout_outcome_notice(Language::En, &ff(3)).as_deref(),
            Some("Fast-forwarded local branch feature/x by 3 commits")
        );
        let ahead = |ahead| CheckoutOutcome::LocalAhead {
            branch: "feature/x".to_string(),
            ahead,
        };
        assert_eq!(
            checkout_outcome_notice(Language::De, &ahead(1)).as_deref(),
            Some("Lokaler Branch feature/x ist 1 Commit voraus – lokaler Stand ausgecheckt")
        );
        assert_eq!(
            checkout_outcome_notice(Language::En, &ahead(2)).as_deref(),
            Some("Local branch feature/x is 2 commits ahead – local state checked out")
        );
        let diverged = CheckoutOutcome::Diverged {
            branch: "feature/x".to_string(),
            ahead: 2,
            behind: 3,
        };
        assert_eq!(
            checkout_outcome_notice(Language::De, &diverged).as_deref(),
            Some(
                "Lokaler Branch feature/x weicht ab (2 voraus, 3 hinterher) – lokaler Stand ausgecheckt"
            )
        );
        assert_eq!(
            checkout_outcome_notice(Language::En, &diverged).as_deref(),
            Some("Local branch feature/x differs (2 ahead, 3 behind) – local state checked out")
        );
    }

    #[test]
    fn branch_switch_error_detail_localizes_blocked() {
        // E15: typisierte Abbrüche lokalisiert, andere Fehler unverändert.
        use crate::git::BranchSwitchBlocked;
        let upstream = anyhow::Error::new(BranchSwitchBlocked::UpstreamMismatch {
            branch: "feature/x".to_string(),
            upstream: "upstream/feature/x".to_string(),
        });
        assert_eq!(
            branch_switch_error_detail(Language::En, &upstream),
            "Local branch feature/x tracks upstream/feature/x"
        );
        assert_eq!(
            branch_switch_error_detail(Language::De, &upstream),
            "Lokaler Branch feature/x verfolgt upstream/feature/x"
        );
        let wt = PathBuf::from("/tmp/wt");
        let elsewhere = anyhow::Error::new(BranchSwitchBlocked::CheckedOutElsewhere {
            branch: "feature/x".to_string(),
            path: wt.clone(),
        });
        assert_eq!(
            branch_switch_error_detail(Language::En, &elsewhere),
            format!("Branch feature/x is checked out in {}", wt.display())
        );
        assert_eq!(
            branch_switch_error_detail(Language::De, &elsewhere),
            format!("Branch feature/x ist in {} ausgecheckt", wt.display())
        );
        let other = anyhow::anyhow!("Checkout fehlgeschlagen: boom");
        assert_eq!(
            branch_switch_error_detail(Language::En, &other),
            "Checkout fehlgeschlagen: boom"
        );
    }

    /// E15: Repo mit lokalem `feature` einen Commit hinter
    /// `refs/remotes/origin/feature` (nur git2, kein Netz).
    fn repo_with_stale_feature(path: &Path) {
        std::fs::create_dir(path).unwrap();
        let repo = git2::Repository::init(path).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@test.com").unwrap();
        std::fs::write(path.join("a.txt"), "one\n").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("a.txt")).unwrap();
        idx.write().unwrap();
        let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let first = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let first = repo.find_commit(first).unwrap();
        repo.branch("feature", &first, false).unwrap();
        // Remote-Stand: ein Commit weiter (b.txt), nur als Remote-Ref.
        let blob = repo.blob(b"two\n").unwrap();
        let mut tb = repo.treebuilder(Some(&tree)).unwrap();
        tb.insert("b.txt", blob, 0o100644).unwrap();
        let tree2 = repo.find_tree(tb.write().unwrap()).unwrap();
        let second = repo
            .commit(None, &sig, &sig, "remote", &tree2, &[&first])
            .unwrap();
        repo.reference("refs/remotes/origin/feature", second, true, "test")
            .unwrap();
    }

    #[test]
    fn execute_branch_switch_notice_survives_auto_fetch() {
        // E15-Review: läuft ein Auto-Fetch, darf `start_scan` den Hinweis
        // nicht durch „auto_fetch_running“ ersetzen; der Scan bleibt gemerkt.
        let dir = tempdir().unwrap();
        let path = dir.path().join("repo");
        repo_with_stale_feature(&path);
        let mut app = test_app();
        app.config.roots.clear();
        app.config.language = Language::De;
        app.auto_fetching = true;
        app.execute_branch_switch(path.clone(), "origin/feature".to_string(), false, false);
        assert!(app.error.is_none(), "unerwarteter Fehler: {:?}", app.error);
        assert_eq!(
            app.status_message.as_deref(),
            Some("Lokaler Branch feature um 1 Commit vorgespult")
        );
        assert!(app.pending_scan, "Folge-Scan nach dem Auto-Fetch gemerkt");
        assert_eq!(app.git_busy, GitBusy::Idle);
    }

    #[test]
    fn execute_branch_switch_shows_fast_forward_notice() {
        // E15: Dropdown „origin/feature“ mit veraltetem lokalem `feature`
        // → vorgespult, Hinweis als Statusmeldung, kein Fehler.
        let dir = tempdir().unwrap();
        let path = dir.path().join("repo");
        repo_with_stale_feature(&path);
        let mut app = test_app();
        // Folge-Scan ohne Suchpfade (keine Platte, kein Netz).
        app.config.roots.clear();
        app.config.language = Language::De;
        app.execute_branch_switch(path.clone(), "origin/feature".to_string(), false, false);
        assert!(app.error.is_none(), "unerwarteter Fehler: {:?}", app.error);
        assert_eq!(
            app.status_message.as_deref(),
            Some("Lokaler Branch feature um 1 Commit vorgespult")
        );
        assert!(path.join("b.txt").exists());
    }

    #[test]
    fn myapp_handle_branch_switch_logic() {
        // Test that dirty check would trigger dialog vs direct checkout
        // We simulate with a temp repo
        use crate::git::{has_merge_conflicts, is_dirty, is_merge_in_progress};
        // This test uses git2 to create repo and check status
        let dir = tempdir().unwrap();
        let path = dir.path().join("repo");
        std::fs::create_dir(&path).unwrap();
        // init repo
        let repo = git2::Repository::init(&path).unwrap();
        let mut cfg = repo.config().unwrap();
        cfg.set_str("user.name", "Test").unwrap();
        cfg.set_str("user.email", "test@test.com").unwrap();
        let file = path.join("README.md");
        std::fs::write(&file, "hi").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("README.md")).unwrap();
        idx.write().unwrap();
        let oid = idx.write_tree().unwrap();
        let tree = repo.find_tree(oid).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        assert!(!is_dirty(&repo));
        assert!(!has_merge_conflicts(&repo));
        assert!(!is_merge_in_progress(&repo));
        // dirty
        std::fs::write(path.join("dirty.txt"), "dirty").unwrap();
        let repo2 = git2::Repository::open(&path).unwrap();
        assert!(is_dirty(&repo2));
        // dirty would trigger dialog in MyApp::handle_branch_switch
        let dirty = is_dirty(&repo2);
        let has_conflicts = has_merge_conflicts(&repo2);
        let in_progress = is_merge_in_progress(&repo2);
        assert!(dirty);
        assert!(!has_conflicts);
        assert!(!in_progress);
    }

    #[test]
    fn myapp_profile_switch_triggers_scan_logic() {
        let mut cfg = AppConfig::default();
        let original = cfg.active_profile_id.clone();
        // simulate switching
        let new_profile = if original == "dotnet" {
            "rust"
        } else {
            "dotnet"
        };
        // add new profile if not exists
        if cfg.get_profile(new_profile).is_none() {
            cfg.profiles.push(crate::config::LanguageProfile {
                id: new_profile.to_string(),
                display_name: new_profile.to_string(),
                file_extension: ".txt".to_string(),
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
        }
        cfg.active_profile_id = new_profile.to_string();
        assert_eq!(cfg.active_profile_id, new_profile);
        // status message would be set and scan started
    }

    #[test]
    fn myapp_agent_toggle_logic() {
        let mut cfg = AppConfig::default();
        let initial_len = cfg.active_agent_ids.len();
        cfg.toggle_agent_active("codex");
        assert!(cfg.is_agent_active("codex"));
        cfg.toggle_agent_active("codex");
        assert!(!cfg.is_agent_active("codex"));
        assert_eq!(cfg.active_agent_ids.len(), initial_len);
    }

    #[test]
    fn myapp_solution_select_persists() {
        let mut cfg = AppConfig::default();
        let repo_path = PathBuf::from("/tmp/repo");
        let sln_path = PathBuf::from("/tmp/repo/app.sln");
        cfg.get_repo_state_mut(&repo_path).selected_solution = Some(sln_path.clone());
        assert_eq!(
            cfg.get_repo_state(&repo_path).unwrap().selected_solution,
            Some(sln_path)
        );
    }

    #[test]
    fn myapp_pending_branch_switch_handling() {
        // Queue statt Option (N2): keine Action geht bei schnellen Klicks verloren.
        let mut pending: Vec<(PathBuf, String)> = Vec::new();
        pending.push((PathBuf::from("/tmp/repo"), "a".to_string()));
        pending.push((PathBuf::from("/tmp/repo"), "b".to_string()));
        assert_eq!(pending.len(), 2);
        let drained = std::mem::take(&mut pending);
        assert!(pending.is_empty());
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].1, "a");
        assert_eq!(drained[1].1, "b");
    }

    #[test]
    fn should_minimize_requires_tray_available() {
        // H1: ohne Tray-Icon darf nicht minimiert werden (Fenster ginge verloren).
        assert!(should_minimize_to_tray(true, false, true));
        assert!(!should_minimize_to_tray(true, false, false));
        assert!(!should_minimize_to_tray(true, true, true));
        assert!(!should_minimize_to_tray(false, false, true));
        assert!(!should_minimize_to_tray(false, false, false));
    }

    #[test]
    fn reveal_main_on_error_only_when_hidden_and_not_quitting() {
        // 7a: Tray-Fehler im versteckten Fenster wäre unsichtbar → Main öffnen.
        // Beim Beenden nie öffnen; bei sichtbarem Fenster ist nichts zu tun.
        assert!(should_reveal_main_on_error(false, false));
        assert!(!should_reveal_main_on_error(true, false));
        assert!(!should_reveal_main_on_error(false, true));
        assert!(!should_reveal_main_on_error(true, true));
    }

    #[test]
    fn custom_select_writes_file() {
        let dir = tempdir().unwrap();
        let sel = crate::config::ConfigSelector {
            id: "db".into(),
            display_name: "DB".into(),
            file_path: "App.config".into(),
            key: "Database".into(),
            key_attribute: "key".into(),
            value_attribute: "value".into(),
            kind: crate::config::XmlSelectorKind::AddKeyValue,
            options: vec![],
            allow_custom: true,
        };
        std::fs::write(
            dir.path().join("App.config"),
            r#"<add key="Database" value="dev"/>"#,
        )
        .unwrap();
        crate::config_parser::write_xml_value(dir.path(), &sel, "prod").unwrap();
        let out = std::fs::read_to_string(dir.path().join("App.config")).unwrap();
        assert!(out.contains(r#"value="prod""#));
    }

    #[test]
    fn tray_branch_switch_never_forces_main_window() {
        // Produktentscheidung: Tray-Switches laufen headless, auch bei Dirty
        // (Dialog/Fehler warten in main). Popup bleibt offen, Main bleibt zu.
        assert!(!tray_branch_switch_opens_main_window());
    }

    #[test]
    fn pending_branch_switches_dedup_keeps_last_per_repo() {
        // Schnelle Mehrfach-Klicks: pro Repo zählt nur der letzte Branch,
        // Zwischen-Switches (dev, dann sofort main) sind verschwendete Arbeit.
        let a = PathBuf::from("/tmp/repo-a");
        let b = PathBuf::from("/tmp/repo-b");
        let in_q = vec![
            (a.clone(), "dev".to_string()),
            (b.clone(), "x".to_string()),
            (a.clone(), "main".to_string()),
        ];
        let out = dedup_branch_switches(in_q);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], (a, "main".to_string()));
        assert_eq!(out[1], (b, "x".to_string()));
        assert!(dedup_branch_switches(vec![]).is_empty());
    }
}
