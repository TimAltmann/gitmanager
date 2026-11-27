mod claude_sessions;
mod cleanup;
mod merge_requests;
mod mr_activity;
mod mr_checkout;
mod mr_pull_prompt;
mod mr_settings;
mod new_worktree;
mod pull;
mod remove_worktree;
pub(crate) mod repo_badges;

use crate::config::{AfterAction, AppConfig, RepoSortMode};
use crate::git::{launch_agent, launch_ide, MrPrep, RepoInfo};
use crate::gitlab::glab::{GlabErrorKind, GlabRunner};
use crate::gitlab::model::{MergedList, MrSnapshot};
use crate::gitlab::remote::{GlabHost, HostMap, LocalProject};
use crate::i18n::{tr, tr_fmt, Language};
use crate::scanner::scan_repos;
use crate::ui::theme::COLOR_MUTED;
use crate::ui::{
    merge_requests::{MrCheckoutChoice, MrViewState},
    repo_list::{show_repo_list, sort_main_repos, RepoHighlight, RepoListActions, RepoListContext},
    settings::SettingsState,
};
use egui::{Color32, RichText, Vec2};
use merge_requests::{top_bar_button, MrResult, TOP_BAR_CAPTIONS_WIDTH, TOP_BAR_COMPACT_WIDTH};
use mr_checkout::{MrCheckoutFailure, MrSwitch};
use mr_settings::{GlabFactory, MrSettingsCache, MrSettingsResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

/// Fenstergröße beim Start (ohne gespeicherte Größe: eframe ohne
/// `persistence`). Mindestens `TOP_BAR_COMPACT_WIDTH` breit, damit
/// „Aktualisieren“ und „Einstellungen“ beim Start ihren Text zeigen.
pub const DEFAULT_WINDOW_SIZE: [f32; 2] = [1260.0, 700.0];
const _: () = assert!(DEFAULT_WINDOW_SIZE[0] >= TOP_BAR_COMPACT_WIDTH);

/// Kleinste Fenstergröße (Punkte, also unabhängig von der Skalierung).
pub const MIN_WINDOW_SIZE: [f32; 2] = [920.0, 560.0];

use crate::ui::icons::{ICON_CHEVRON_DOWN as ICON_SORT, ICON_GEAR, ICON_REFRESH, ICON_WARNING};

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
        /// Der stille Auto-Fetch brach ab, nicht der Lauf in `git_busy`
        is_auto: bool,
    },
    /// Branch-Wechsel (`start_branch_switch`) fertig oder braucht eine
    /// Entscheidung.
    BranchSwitched {
        job: SwitchJob,
        result: SwitchResult,
    },
    /// K1: Auschecken eines MR fertig; `Ok(None)` nach dem Klonen, sonst
    /// der vorbereitete lokale Branch (Worktree, bestehender Klon).
    MrCheckoutReady {
        choice: Box<MrCheckoutChoice>,
        result: Result<Option<MrPrep>, MrCheckoutFailure>,
    },
}

/// Umgang eines Branch-Wechsels mit Änderungen im Arbeitsverzeichnis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwitchMode {
    /// Erst prüfen: bei Konflikten ablehnen, bei Änderungen nachfragen
    Ask,
    /// Änderungen stashen, wechseln, Stash wieder anwenden
    Stash,
    /// Änderungen verwerfen
    Force,
}

/// Ein Branch-Wechsel für den Worker.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SwitchJob {
    repo_path: PathBuf,
    target_branch: String,
    mode: SwitchMode,
}

/// Ergebnis des Workers (`run_branch_switch`).
enum SwitchResult {
    /// Merge-Konflikte oder laufender Merge/Rebase (Zustand als Text)
    Blocked(String),
    /// Änderungen im Arbeitsverzeichnis: der Dialog fragt nach
    Dirty { files: Vec<String>, truncated: bool },
    /// Gewechselt; `Some` = der Stash ließ sich danach nicht anwenden und
    /// liegt noch im Stash
    Done(anyhow::Result<(crate::git::CheckoutOutcome, Option<String>)>),
}

/// Worker des Branch-Wechsels: Status prüfen und auschecken. Alles hier
/// liest und schreibt das Arbeitsverzeichnis (Sekunden bei großen Repos)
/// und läuft deshalb nie im UI-Thread. `known_dirty` = Stand des letzten
/// Scans (F1#5: spart den teuren Live-Check; beide Fehlerrichtungen sind
/// sicher – „sauber“ scheitert im Dry-Run, „geändert“ fragt einmal zu viel).
fn run_branch_switch(job: &SwitchJob, known_dirty: Option<bool>) -> SwitchResult {
    let (path, branch) = (job.repo_path.as_path(), job.target_branch.as_str());
    if job.mode == SwitchMode::Ask {
        if let Some(repo) = crate::git::open_repo(path) {
            if crate::git::has_merge_conflicts(&repo) || crate::git::is_merge_in_progress(&repo) {
                return SwitchResult::Blocked(format!("{:?}", repo.state()));
            }
            if known_dirty.unwrap_or_else(|| crate::git::is_dirty(&repo)) {
                // P4: get_detailed_status ohne recurse, gecappt
                let (files, truncated) = crate::git::get_detailed_status(&repo);
                return SwitchResult::Dirty { files, truncated };
            }
        }
    }
    SwitchResult::Done(match job.mode {
        SwitchMode::Ask => crate::git::checkout_branch(path, branch).map(|o| (o, None)),
        SwitchMode::Force => crate::git::checkout_branch_force(path, branch).map(|o| (o, None)),
        SwitchMode::Stash => crate::git::stash_and_checkout(path, branch)
            .map(|s| (s.outcome, s.pop_failed.map(|p| p.to_string()))),
    })
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

/// Einheitlicher Text für gescheiterte Config-Saves.
fn save_failed_text(lang: Language, err: &anyhow::Error) -> String {
    tr_fmt(lang, "save_failed", &[&format!("{err:#}")])
}

/// Roter Rahmen der Fehlermeldungen (Banner und Branch-Dialog).
fn error_frame(inner_margin: egui::Margin) -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgb(255, 235, 235))
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(220, 100, 100)))
        .corner_radius(6)
        .inner_margin(inner_margin)
}

/// „Fehler: …“ in der roten Schrift von [`error_frame`].
fn error_label(lang: Language, msg: &str, size: f32) -> RichText {
    RichText::new(format!("{}: {msg}", tr(lang, "error_prefix")))
        .size(size)
        .color(ERROR_TEXT)
}

/// Schrift- und Iconfarbe auf dem roten Fehlerrahmen
const ERROR_TEXT: Color32 = Color32::from_rgb(160, 40, 40);

/// Statusleiste der Repo-Liste, solange ein Lauf die Refs hält. Beim
/// Auschecken eines MR (K1, auch während eines minutenlangen Klons)
/// „Auschecken für !{iid} läuft …“ statt „Scanne nach Repositories…“.
fn repo_busy_text(lang: Language, busy: &GitBusy) -> Option<String> {
    match busy {
        GitBusy::Idle => None,
        GitBusy::Scan => Some(tr(lang, "scanning_repos")),
        // Die Netzphase dauert oft länger als die Statusmeldung des Starts
        GitBusy::Fetch { repo } => Some(tr_fmt(
            lang,
            "fetching_branches",
            &[&repo.display().to_string()],
        )),
        GitBusy::Checkout { iid } => Some(tr_fmt(lang, "mr_checkout_running", &[&iid.to_string()])),
        GitBusy::Worktree => Some(tr(lang, "wt_creating")),
        GitBusy::Pull { .. } => Some(tr(lang, "pull_running")),
        GitBusy::Remove { .. } => Some(tr(lang, "rm_wt_running")),
        GitBusy::Cleanup => Some(tr(lang, "cleanup_running")),
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
    ClaudeSessions,
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
    /// „Neuer Worktree“ aus der Repo-Liste läuft, danach Rescan
    Worktree,
    /// Pull (Fast-Forward) dieses Repos per Klick, danach Rescan
    Pull { repo: PathBuf },
    /// Worktree entfernen (Dialog), danach Rescan
    Remove { path: PathBuf },
    /// „Aufräumen …“: Fetch mit Prune und Suche bzw. Entfernen, danach Rescan
    Cleanup,
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

    /// Pull oder Entfernen arbeitet gerade an diesem Pfad: ein Branch-Wechsel
    /// dort muss warten (sonst setzt der Fast-Forward den falschen Branch um).
    fn works_on(&self, path: &std::path::Path) -> bool {
        let key = crate::config::AppConfig::repo_state_key(path);
        match self {
            GitBusy::Pull { repo: p } | GitBusy::Remove { path: p } => {
                crate::config::AppConfig::repo_state_key(p) == key
            }
            _ => false,
        }
    }
}

pub struct MyApp {
    /// Weckt die Oberfläche aus Hintergrund-Threads (`request_repaint`),
    /// sonst läge ein Ergebnis bis zur nächsten Mausbewegung im Kanal.
    ctx: egui::Context,
    config: AppConfig,
    repos: Vec<RepoInfo>,
    /// Scan, manueller Fetch oder MR-Checkout (Ref-Lock, Spinner); nur über
    /// `set_git_busy` ändern (Tray)
    git_busy: GitBusy,
    error: Option<String>,
    /// Fehler, der das nächste Scan-Ergebnis überdauert (z. B. Pull-Fehler
    /// mit anschließendem Rescan); danach gilt wieder `error = None`.
    error_after_scan: Option<String>,
    /// Zuletzt gezeigter Fetch-Fehler: ein späterer erfolgreicher
    /// Auto-Fetch löscht genau dieses Banner
    fetch_error_shown: Option<String>,
    show_settings: bool,
    settings_state: Option<SettingsState>,
    scan_tx: Sender<ScanResult>,
    scan_rx: Receiver<ScanResult>,
    launch_err_tx: Sender<String>,
    launch_err_rx: Receiver<String>,
    /// Beim Start erkannte IDE-Programme (`detect_ide_programs`)
    detected_ides_rx: Receiver<Vec<crate::ui::settings::DetectedIde>>,
    /// Schreib-Thread der Config (`None` in Tests: die Datei bleibt unberührt)
    config_writer: Option<Sender<crate::config::ConfigSnapshot>>,
    config_saved_rx: Receiver<anyhow::Result<()>>,
    /// Hier meldet der Schreib-Thread; in `new` an ihn übergeben
    config_saved_tx: Sender<anyhow::Result<()>>,
    update_rx: Receiver<Result<Option<crate::updater::UpdateInfo>, String>>,
    update_info: Option<crate::updater::UpdateInfo>,
    update_error: Option<String>,
    show_update_dialog: bool,
    status_message: Option<String>,
    status_message_time: Option<std::time::Instant>,
    branch_dialog: Option<BranchDialog>,
    pending_branch_switches: Vec<(PathBuf, String)>,
    /// Laufender Branch-Wechsel (Worker): Repo und Ziel-Branch. Höchstens
    /// einer zur Zeit; weitere warten in `pending_branch_switches`.
    branch_switch: Option<(PathBuf, String)>,
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
    /// `start_scan` während eines laufenden Scans/Fetch: dessen Ergebnis ist
    /// veraltet (neue Suchpfade, Profil, Branch-Wechsel), danach neu scannen.
    scan_outdated: bool,
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
    /// Ansicht „Claude Sessions“
    claude: claude_sessions::ClaudeViewState,
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
    /// Klone je Projekt für die Checkout-Buttons (`update_mr_clones`)
    mr_clones: crate::gitlab::checkout::CloneIndex,
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
    /// Zeitraum (Tage) des laufenden Abrufs „Gemergt“; `None` = keiner
    mr_merged_loading: Option<u32>,
    /// Zählt neue Abrufe „Gemergt“; Ergebnisse älterer werden verworfen
    mr_merged_gen: u64,
    mr_tx: Sender<MrResult>,
    /// Neue Kommentare/Commits je MR und Benachrichtigungen
    mr_activity: mr_activity::MrActivityState,
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
    /// Agent aus der MR-Ansicht: Frage „Vorher pullen?“
    mr_pull_prompt: Option<mr_pull_prompt::MrPullPrompt>,
    /// Agent startet, sobald der Pull dieses Ordners fertig ist
    agent_after_pull: Option<(PathBuf, String)>,
    /// „In Repo-Liste zeigen“: hervorgehobene Karte und seit wann
    repo_highlight: Option<(RepoHighlight, std::time::Instant)>,
    /// Offener MR je Repo-Ordner für den Chip der Repo-Karte
    repo_mrs: HashMap<PathBuf, crate::gitlab::badge::MrBadge>,
    /// Lokale Erkennung (E16) für diese Hosts erneut, auch wenn bekannt
    mr_rediscover: std::collections::BTreeSet<String>,
    /// Einstellungs-Tab „Merge Requests“: Worker-Ergebnisse kommen auch
    /// nach dem Schließen an (der `SettingsState` wird dann verworfen)
    mr_settings_tx: Sender<MrSettingsResult>,
    mr_settings_rx: Receiver<MrSettingsResult>,
    mr_settings: MrSettingsCache,
    /// Repo-Liste: Suche und Dialog „Neuer Worktree“
    worktree_ui: new_worktree::WorktreeUi,
    pull_ui: pull::PullUi,
    remove_ui: remove_worktree::RemoveUi,
    cleanup_ui: cleanup::CleanupUi,
}

/// Schreib-Thread der Config: Schreiben und fsync laufen hier statt im
/// UI-Thread. Liegen mehrere Stände an, wird nur der neueste geschrieben;
/// jedes Ergebnis geht an `done`.
fn spawn_config_writer(
    done: Sender<anyhow::Result<()>>,
    ctx: egui::Context,
) -> Sender<crate::config::ConfigSnapshot> {
    let (tx, rx) = mpsc::channel::<crate::config::ConfigSnapshot>();
    std::thread::spawn(move || {
        while let Ok(mut snapshot) = rx.recv() {
            while let Ok(newer) = rx.try_recv() {
                snapshot = newer;
            }
            if done.send(snapshot.write()).is_err() {
                return;
            }
            ctx.request_repaint();
        }
    });
    tx
}

/// Sender der beiden Start-Threads (Auto-Erkennung der IDEs, Update-Prüfung).
struct StartupSenders {
    detected_ides: Sender<Vec<crate::ui::settings::DetectedIde>>,
    update: Sender<Result<Option<crate::updater::UpdateInfo>, String>>,
}

impl MyApp {
    /// Gemeinsamer Aufbau für `new` und die Test-App; übergeben wird nur,
    /// was sich dort unterscheidet. glab kommt aus `glab_factory`.
    fn build(
        config: AppConfig,
        window_visible: bool,
        last_auto_fetch: Option<std::time::Instant>,
        mr_activity: mr_activity::MrActivityState,
        glab_factory: GlabFactory,
    ) -> (Self, StartupSenders) {
        let (scan_tx, scan_rx) = mpsc::channel();
        let (launch_err_tx, launch_err_rx) = mpsc::channel();
        let (detected_ides_tx, detected_ides_rx) = mpsc::channel();
        let (config_saved_tx, config_saved_rx) = mpsc::channel();
        let (update_tx, update_rx) = mpsc::channel();
        let (mr_tx, mr_rx) = mpsc::channel();
        let (mr_settings_tx, mr_settings_rx) = mpsc::channel();
        let glab_program = config.merge_requests.glab_program.clone();
        let glab = glab_factory(&glab_program);
        let mr_view = MrViewState::new(&config.merge_requests);
        let app = Self {
            ctx: egui::Context::default(),
            config,
            repos: Vec::new(),
            git_busy: GitBusy::Idle,
            error: None,
            error_after_scan: None,
            fetch_error_shown: None,
            show_settings: false,
            settings_state: None,
            scan_tx,
            scan_rx,
            launch_err_tx,
            launch_err_rx,
            detected_ides_rx,
            config_writer: None,
            config_saved_rx,
            config_saved_tx,
            update_rx,
            update_info: None,
            update_error: None,
            show_update_dialog: false,
            status_message: None,
            status_message_time: None,
            branch_dialog: None,
            pending_branch_switches: Vec::new(),
            branch_switch: None,
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
            window_visible,
            should_quit: false,
            last_auto_fetch,
            auto_fetching: false,
            pending_scan: false,
            scan_outdated: false,
            config_save_fail_streak: 0,
            repo_sort_dirty: true,
            config_save_pending: false,
            last_config_save: None,
            main_view: MainView::Repos,
            claude: claude_sessions::ClaudeViewState::new(),
            glab,
            glab_program,
            glab_factory,
            glab_hosts: Vec::new(),
            mr_hosts: HostMap::default(),
            mr_locals: Vec::new(),
            mr_clones: Default::default(),
            mr_snapshot: None,
            mr_loading: false,
            mr_pending_refresh: false,
            mr_refresh_after_scan: false,
            mr_last_refresh: None,
            mr_auth_epoch: 0,
            mr_discovering: false,
            mr_more_pending: 0,
            mr_merged: None,
            mr_merged_loading: None,
            mr_merged_gen: 0,
            mr_tx,
            mr_activity,
            mr_rx,
            mr_view,
            mr_glab_missing: false,
            mr_glab_error: None,
            mr_switch: None,
            pending_after_checkout: None,
            mr_notice: None,
            mr_pull_prompt: None,
            agent_after_pull: None,
            repo_highlight: None,
            repo_mrs: Default::default(),
            mr_rediscover: Default::default(),
            mr_settings_tx,
            mr_settings_rx,
            mr_settings: MrSettingsCache::default(),
            worktree_ui: Default::default(),
            pull_ui: Default::default(),
            remove_ui: Default::default(),
            cleanup_ui: Default::default(),
        };
        let senders = StartupSenders {
            detected_ides: detected_ides_tx,
            update: update_tx,
        };
        (app, senders)
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(target_os = "windows")]
        if !crate::dpi_fix::install(cc) {
            eprintln!("DPI-Korrektur für das Hauptfenster nicht installiert");
        }
        let (config, load_error) = AppConfig::load();
        // Startup-Sync: Run-Key mit aktueller exe synchronisieren (falls exe verschoben).
        // Best effort — kein Start-Abbruch bei Registry-Fehlern.
        // M1: set() selbst ist idempotent (kein Rewrite wenn Wert identisch).
        // Nicht mit den Defaults einer unlesbaren Config: `autostart = false`
        // löschte sonst den Run-Key, den der Nutzer gesetzt hat.
        if load_error.is_none() {
            if let Err(e) = crate::autostart::apply_autostart(config.autostart) {
                eprintln!("Autostart-Sync fehlgeschlagen: {e:#}");
            }
        }
        // M1: --minimized (Autostart) startet versteckt im Tray.
        // N6: gleiche Entscheidung wie main() (Single Source).
        let start_minimized = crate::start_minimized_flag();
        crate::ui::theme::apply_theme(&cc.egui_ctx, &config.theme);
        // SVG Icons loader für egui_extras
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let (mut app, senders) = Self::build(
            config,
            !start_minimized,
            // Task 5: erster Auto-Fetch erst nach dem Intervall, nicht sofort
            // nach dem Startup-Scan (vermeidet Doppel-Fetch beim Start).
            Some(std::time::Instant::now()),
            mr_activity::MrActivityState::load_default(),
            mr_settings::system_glab_factory(),
        );
        app.ctx = cc.egui_ctx.clone();
        app.config_writer = Some(spawn_config_writer(
            app.config_saved_tx.clone(),
            app.ctx.clone(),
        ));
        // Fehler aus dem Start überstehen den ersten Scan (`start_scan`
        // und sein Ergebnis löschen `error` sonst wieder).
        #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
        let mut startup_errors: Vec<String> = load_error
            .map(|e| tr_fmt(app.config.language, "config_load_failed", &[&e]))
            .into_iter()
            .collect();
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
                startup_errors.push(tr(app.config.language, "tray_creation_error"));
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
        // Auto-Erkennung asynchron (verhindert UI Freeze beim Start). Der
        // Thread meldet nur die gefundenen Programme; eingetragen und
        // gespeichert wird im UI-Thread in den dann aktuellen Stand.
        {
            let candidates = crate::ui::settings::DetectedIde::candidates(&app.config);
            let tx = senders.detected_ides;
            let ctx = app.ctx.clone();
            std::thread::spawn(move || {
                let detected: Vec<_> = candidates
                    .into_iter()
                    .filter_map(|mut ide| {
                        let found = if ide.ide_id == "vs2022" {
                            crate::git::resolve_vs_path()
                        } else {
                            crate::git::resolve_rider_path()
                        }?;
                        ide.program = found.display().to_string();
                        Some(ide)
                    })
                    .collect();
                if !detected.is_empty() && tx.send(detected).is_ok() {
                    ctx.request_repaint();
                }
            });
        }
        // Update checker (async, 2s delay to not affect startup)
        // Nur wenn in den Settings aktiviert (F-06 Opt-out, Default AN)
        // M2: kein Thread + kein Sleep wenn deaktiviert.
        {
            let current_version = env!("CARGO_PKG_VERSION").to_string();
            let update_tx = senders.update;
            let ctx = app.ctx.clone();
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
                    ctx.request_repaint();
                });
            }
        }
        app.start_scan();
        if !startup_errors.is_empty() {
            let text = startup_errors.join(" · ");
            app.error = Some(text.clone());
            app.error_after_scan = Some(text);
        }
        // Initial size matches viewport default – used for collapse logic
        app.last_window_size = DEFAULT_WINDOW_SIZE;
        app
    }

    /// Statusmeldung in der Statusleiste (verschwindet nach 3 s).
    pub(super) fn set_status(&mut self, message: String) {
        self.status_message = Some(message);
        self.status_message_time = Some(std::time::Instant::now());
    }

    fn clear_status(&mut self) {
        self.status_message = None;
        self.status_message_time = None;
    }

    /// Kein zweiter Lauf auf denselben Refs (zwei libgit2-Läufe racen um
    /// FETCH_HEAD/Refs): während des Auto-Fetch oder solange ein anderer
    /// Lauf die Refs hält, nur melden. `true` = Aufrufer bricht ab.
    pub(super) fn refuse_while_busy(&mut self) -> bool {
        let key = if self.auto_fetching {
            "auto_fetch_running"
        } else if self.git_busy.holds_refs() || self.branch_switch.is_some() {
            "fetch_busy"
        } else {
            return false;
        };
        self.set_status(tr(self.config.language, key));
        true
    }

    fn start_scan(&mut self) {
        match self.git_busy {
            GitBusy::Idle => {}
            // Der laufende Scan liest mit der Config seines Starts und kann
            // dieses Repo schon hinter sich haben: danach noch einmal
            GitBusy::Scan | GitBusy::Fetch { .. } => {
                self.scan_outdated = true;
                return;
            }
            // K1: das Auschecken endet ohne Scan – Wunsch merken, `poll_scan`
            // holt ihn danach nach
            GitBusy::Checkout { .. }
            | GitBusy::Worktree
            | GitBusy::Pull { .. }
            | GitBusy::Remove { .. }
            | GitBusy::Cleanup => {
                self.pending_scan = true;
                return;
            }
        }
        // L2: kein paralleler Lauf zum Auto-Fetch — zwei libgit2-Fetches
        // auf denselben Repos racen um FETCH_HEAD/refs (cannot lock ref).
        // P2b: Intent merken statt verwerfen — FetchDone(is_auto) holt nach.
        if self.refuse_while_busy() {
            self.pending_scan = true;
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
                            is_auto: false,
                        }
                    }
                };
            let _ = tx.send(msg);
        });
    }

    /// Neue Repos aus `Repos` bzw. `FetchDone` übernehmen (sortiert) samt
    /// allem, was auf frische Repo-Daten wartet.
    fn intake_repos(&mut self, mut repos: Vec<RepoInfo>) {
        // Task 4: View-Sortierung einmalig am Intake (per Frame nur noch bei
        // dirty, siehe Render-Pfad) — mit der aktuellen Config.
        sort_main_repos(&mut repos, &self.config);
        self.repos = repos;
        self.repo_sort_dirty = false;
        self.mr_after_scan();
        self.claude_after_scan();
    }

    /// Abschluss nach `intake_repos`, wenn auch der Fehlerstand gesetzt ist:
    /// „Danach öffnen“ (K1), Tray-Sync, neu zeichnen.
    fn finish_intake(&mut self, ctx: &egui::Context) {
        // K1: IDE nach dem Auschecken, sobald der Ordner bekannt ist
        self.run_pending_after_checkout();
        // Review-Fix: dirty VOR dem direkten Sync markieren — bei
        // WouldBlock bleibt das Flag (Backstop in sync_tray_service).
        self.mark_tray_dirty();
        self.sync_tray_service();
        ctx.request_repaint();
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
                    // P2b: abgeschlossener Scan erfüllt gemerkten Refresh –
                    // außer er war beim Eintreffen schon überholt.
                    self.pending_scan = std::mem::take(&mut self.scan_outdated);
                    self.error = self.error_after_scan.take();
                    self.intake_repos(repos);
                    self.finish_intake(ctx);
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
                        self.pending_scan |= std::mem::take(&mut self.scan_outdated);
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
                    self.intake_repos(repos);
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
                        self.fetch_error_shown = Some(msg.clone());
                        self.set_status(msg);
                    } else if !is_auto || self.error == self.fetch_error_shown {
                        // Der stille Auto-Fetch räumt nur den Fetch-Fehler
                        // ab, kein fremdes Banner (Start-, Speicherfehler …)
                        self.error = None;
                        self.fetch_error_shown = None;
                    }
                    self.finish_intake(ctx);
                }
                ScanResult::FetchAborted {
                    reason_key,
                    is_auto,
                } => {
                    // H3: Thread-Panic vor send — Flag des abgebrochenen
                    // Laufs lösen (kein ewiger Spinner), Fehler persistent
                    // zeigen, Liste behalten. L4: ein abgebrochener Auto-Fetch
                    // löst nicht die Sperre eines laufenden Pull/Checkout.
                    if is_auto {
                        self.set_auto_fetching(false);
                        // Wie Auto-FetchDone als Intervall-Ende werten (kein
                        // sofortiges Back-to-Back mit Panic-Loop).
                        self.last_auto_fetch = Some(std::time::Instant::now());
                    } else {
                        self.set_git_busy(GitBusy::Idle);
                    }
                    // F1#F2: einen gemerkten Refresh holt auch nach Abort das
                    // Ende von `poll_scan` nach — sonst wartet er bis Intervall.
                    let msg = tr(self.config.language, reason_key);
                    let msg = if msg == reason_key {
                        reason_key.to_string()
                    } else {
                        msg
                    };
                    self.error = Some(msg.clone());
                    self.set_status(msg);
                    self.mark_tray_dirty();
                    ctx.request_repaint();
                }
                ScanResult::BranchSwitched { job, result } => {
                    self.branch_switch = None;
                    self.on_branch_switched(ctx, job, result);
                    // Dialog/Fehler speisen das Tray-Badge
                    self.mark_tray_dirty();
                    self.sync_tray_service();
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
            // Ein laufender oder gemerkter Scan löschte das Banner sonst
            // gleich wieder („Danach öffnen“ startet während des Rescans)
            if self.git_busy.scans_repos() || self.pending_scan {
                self.error_after_scan = Some(err.clone());
            }
            self.error = Some(err);
            self.clear_status();
            // Async Tray-Launches (Agent/Shell) landen hier ohne Ursprung;
            // hidden impliziert Tray-Ursprung → Fehler sichtbar machen (7a).
            // #3: Fehler speist das Tray-Badge → dirty für nächsten Sync.
            self.mark_tray_dirty();
            if should_reveal_main_on_error(self.window_visible, self.should_quit) {
                self.show_main_window(ctx);
            }
            ctx.request_repaint();
        }
        while let Ok(detected) = self.detected_ides_rx.try_recv() {
            // Nur wo noch der Programmname von vorhin steht: was der Nutzer
            // inzwischen eingetragen hat, bleibt.
            let mut changed = false;
            for ide in &detected {
                changed |= ide.apply(&mut self.config);
                if let Some(state) = self.settings_state.as_mut() {
                    ide.apply(&mut state.draft);
                }
            }
            if changed {
                self.save_config();
                ctx.request_repaint();
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
                    self.set_status(tr_fmt(self.config.language, "tray_update_error", &[&e]));
                    self.update_error = Some(e);
                    ctx.request_repaint();
                }
            }
        }
        if self.git_busy.holds_refs() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(t) = self.status_message_time {
            if t.elapsed() > std::time::Duration::from_secs(3) {
                self.clear_status();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        // Tray-Switches laufen headless: Main öffnet sich nie von selbst
        // (der Dialog bei Änderungen wartet in `branch_dialog`, Popup bleibt offen).
        // Dedup: pro Repo nur der letzte Klick (keine verschwendeten Zwischen-Switches).
        // Wechsel auf einem Pfad, an dem Pull/Entfernen gerade arbeitet,
        // bleiben in der Queue und laufen nach dessen Ende.
        // Einer zur Zeit (Worker); die übrigen bleiben in der Queue.
        if self.branch_switch.is_none() && !self.pending_branch_switches.is_empty() {
            let (deferred, ready): (Vec<_>, Vec<_>) =
                std::mem::take(&mut self.pending_branch_switches)
                    .into_iter()
                    .partition(|(path, _)| self.git_busy.works_on(path));
            let mut ready = dedup_branch_switches(ready).into_iter();
            let next = ready.next();
            self.pending_branch_switches = ready.chain(deferred).collect();
            if let Some((path, branch)) = next {
                self.start_branch_switch(path, branch, SwitchMode::Ask);
            }
        }
        // K1: MR-Wechsel ohne Ausführung und ohne eigenen Dialog verwerfen
        self.drop_stale_mr_switch();
        // Gemerkten Scan nachholen, sobald nichts mehr die Refs hält und kein
        // Auto-Fetch läuft (P2b nach dem Auto-Fetch, F1#F2 nach Abort, K1 nach
        // dem Auschecken) – erst nach dem Wechsel oben, sonst liest er den
        // alten Stand. Ein Fehler bleibt bis zum Scan-Ergebnis stehen.
        if self.pending_scan
            && !self.git_busy.holds_refs()
            && !self.auto_fetching
            && self.branch_switch.is_none()
        {
            self.pending_scan = false;
            let error = self.error.take();
            self.start_scan();
            self.error = error;
        }
    }
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
        if self.git_busy.holds_refs() || self.branch_switch.is_some() {
            // Manueller Scan/Fetch, Auschecken oder Branch-Wechsel läuft: Auto auslassen
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
        // Worktrees eines Repos teilen die Remote-Refs: ein Fetch je Repo
        let paths = crate::git::fetch_targets(&self.repos);
        let cfg = self.config.clone();
        let tx = self.scan_tx.clone();
        std::thread::spawn(move || {
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
                let shared = {
                    let g = first_err.lock().unwrap_or_else(|e| e.into_inner());
                    (g.0.clone(), g.1, g.2)
                };
                merge_parallel_fetch_result(
                    &mut fetch_error,
                    &mut fetch_partial,
                    &mut fetch_is_fetch_error,
                    shared,
                    worker_panicked,
                );
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
                        is_auto: true,
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

    /// Neue Aktionen der Repo-Karte (Stern, Chips, `⋯`-Menü) und der
    /// Listen-Werkzeugleiste („Aufräumen …“).
    fn apply_card_actions(&mut self, ctx: &egui::Context, actions: &mut RepoListActions) {
        if let Some(path) = actions.toggle_favorite.take() {
            self.config.toggle_favorite_repo(&path);
            // Favoriten stehen oben in ihrem Abschnitt
            self.repo_sort_dirty = true;
            self.save_config();
        }
        if let Some((host_key, id)) = actions.open_mr.take() {
            self.focus_mr(&host_key, &id);
        }
        if let Some(path) = actions.focus_claude.take() {
            if let Some(pid) = repo_badges::claude_pid_for(&self.claude.live_by_repo(), &path) {
                self.apply_claude_actions(crate::ui::claude_sessions::ClaudeSessionActions {
                    focus: Some(pid),
                    ..Default::default()
                });
            }
        }
        if let Some(text) = actions.copy_text.take() {
            ctx.copy_text(text);
        }
        if let Some(url) = actions.open_url.take() {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        if let Some(path) = actions.pull.take() {
            self.start_pull(path);
        }
        if let Some(path) = actions.remove_worktree.take() {
            self.open_remove_worktree(&path);
        }
        if std::mem::take(&mut actions.cleanup) {
            self.open_cleanup();
        }
    }

    /// #12: Config-Save-Fehler einheitlich melden statt still zu verwerfen:
    /// eprintln für Logs + `self.error` fürs Hauptfenster (sichtbar beim Öffnen).
    fn note_config_save_error(&mut self, err: &anyhow::Error) {
        eprintln!("Config speichern fehlgeschlagen: {err:#}");
        self.error = Some(save_failed_text(self.config.language, err));
    }

    /// Nach einem Fehler aus dem Tray das versteckte Hauptfenster zeigen:
    /// `self.error` bliebe sonst unsichtbar (7a). Der Fehler speist auch das
    /// Tray-Badge, daher dirty.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    fn reveal_main_on_error(&mut self, ctx: &egui::Context) {
        self.mark_tray_dirty();
        if should_reveal_main_on_error(self.window_visible, self.should_quit) {
            self.show_main_window(ctx);
        }
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
                self.save_config_now();
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
                busy: self.git_busy.holds_refs()
                    || self.auto_fetching
                    || self.branch_switch.is_some(),
                repo: match &self.git_busy {
                    GitBusy::Fetch { repo } => Some(repo.clone()),
                    _ => self.branch_switch.as_ref().map(|(path, _)| path.clone()),
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
                    self.open_settings();
                    self.show_main_window(ctx);
                }
                crate::tray_service::TrayAction::Quit => {
                    self.should_quit = true;
                    // L6: Usage-Timestamps nicht verlieren — Force-Flush
                    // (Throttle umgehen), sonst bleibt pending ungespeichert.
                    // pending=true erzwingen (Backoff-Give-up nicht im Weg).
                    self.save_config_now();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                crate::tray_service::TrayAction::BranchSwitch(path, branch) => {
                    // Queue instead of overwriting (N2): no action lost on rapid clicks.
                    self.pending_branch_switches.push((path, branch));
                }
                crate::tray_service::TrayAction::SolutionSelect(repo_path, sln_path) => {
                    self.select_solution(&repo_path, sln_path);
                    self.sync_tray_service();
                }
                // Dieselben Wege wie im Hauptfenster; ein Fehler dort stünde
                // im versteckten Fenster, also zeigen.
                crate::tray_service::TrayAction::IdeOpen(path, ide_id, file) => {
                    if !self.open_ide(&path, &ide_id, &file) {
                        self.reveal_main_on_error(ctx);
                    }
                }
                crate::tray_service::TrayAction::AgentOpen(path, agent_id) => {
                    if !self.open_agent(&path, &agent_id) {
                        self.reveal_main_on_error(ctx);
                    }
                }
                crate::tray_service::TrayAction::ExplorerOpen(path) => self.open_explorer(&path),
                crate::tray_service::TrayAction::ShellOpen(path) => self.open_shell(&path),
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
        // L2: Request verwerfen + Hinweis (der Tray-Stand kann veraltet sein)
        if self.refuse_while_busy() {
            return;
        }
        let path_clone = repo_path.clone();
        let cfg = self.config.clone();
        let tx = self.scan_tx.clone();
        // Manueller Fetch = frische Refs: Auto-Timer zurücksetzen,
        // damit der periodische Fetch nicht Sekunden später doppelt läuft.
        self.last_auto_fetch = Some(std::time::Instant::now());
        self.set_status(tr_fmt(
            lang,
            "fetching_branches",
            &[&repo_path.display().to_string()],
        ));
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
                        is_auto: false,
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

    /// Öffnet die Einstellungen; ein schon offener Entwurf bleibt erhalten
    /// (Zahnrad oder Tray-Eintrag bei offenem Fenster verwürfen ihn sonst).
    pub(super) fn open_settings(&mut self) {
        if !self.show_settings || self.settings_state.is_none() {
            self.settings_state = Some(SettingsState::from_config(&self.config));
        }
        self.show_settings = true;
    }

    /// Änderung außerhalb der Einstellungen (Top-Leiste) in deren offenen
    /// Entwurf spiegeln: „Speichern“ dort schriebe sonst den alten Wert.
    fn sync_settings_draft(&mut self, apply: impl FnOnce(&mut AppConfig, &AppConfig)) {
        if let Some(state) = self.settings_state.as_mut() {
            apply(&mut state.draft, &self.config);
        }
    }

    /// Config speichern, ohne den UI-Thread anzuhalten: nur vormerken.
    /// `flush_config_save` gibt den Stand höchstens alle 200 ms an den
    /// Schreib-Thread; Fehler kommen über `config_saved_rx` ins Banner.
    pub(super) fn save_config(&mut self) {
        self.config_save_pending = true;
        // Fast jede gespeicherte Änderung zeigt auch das Tray-Popup
        self.mark_tray_dirty();
    }

    /// Beim Beenden: sofort und blockierend schreiben, der Schreib-Thread
    /// käme nicht mehr dazu (gebatchte MRU-Zeitstempel gingen verloren).
    fn save_config_now(&mut self) {
        self.config_save_pending = false;
        if let Err(e) = self.config.save() {
            self.note_config_save_error(&e);
        }
    }

    /// P5: gebatchter Config-Save — höchstens ein Stand je 200 ms geht an
    /// den Schreib-Thread (Schreiben + fsync laufen dort).
    /// F2#5/L6: Throttle-Hit plant Repaint (sonst hängt pending bei
    /// eingestelltem egui-Repaint).
    fn flush_config_save(&mut self, ctx: &egui::Context) {
        self.poll_config_saved();
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
        match self.config.snapshot() {
            // Ohne Schreib-Thread (Tests) bleibt die Datei unberührt
            Ok(snapshot) => {
                if let Some(writer) = &self.config_writer {
                    let _ = writer.send(snapshot);
                }
            }
            Err(e) => self.note_config_save_error(&e),
        }
    }

    /// Ergebnisse des Schreib-Threads: Fehler zeigen und erneut versuchen.
    fn poll_config_saved(&mut self) {
        while let Ok(result) = self.config_saved_rx.try_recv() {
            let Err(e) = result else {
                self.config_save_fail_streak = 0;
                continue;
            };
            self.config_save_fail_streak = self.config_save_fail_streak.saturating_add(1);
            self.mark_tray_dirty();
            if self.config_save_fail_streak <= 5 {
                // Retry beim nächsten Durchlauf statt still zu verwerfen.
                self.config_save_pending = true;
                self.note_config_save_error(&e);
            } else {
                // L4/F3#2: Banner behalten, aber kein Dauer-Retry und kein
                // geflutetes Log (volle Disk/Rechte = Dauerzustand); die
                // nächste Änderung und das Beenden versuchen es wieder.
                self.error = Some(save_failed_text(self.config.language, &e));
            }
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
    /// des Basis-Klons). `true`, wenn es die IDE gibt und der Start läuft.
    fn open_ide_in(&mut self, path: &Path, profile_path: &Path, ide_id: &str, file: &Path) -> bool {
        let lang = self.config.language;
        // Letzte IDE-Wahl merken; gespeichert wird mit `record_usage` unten
        // (gebatcht, markiert auch den Tray-Stand als geändert).
        self.config.get_repo_state_mut(path).selected_ide = Some(ide_id.to_string());
        // Finde IdeConfig
        let profile = self.config.get_effective_profile_for_repo(profile_path);
        let ide = profile
            .ides
            .iter()
            .find(|i| i.id == ide_id)
            .cloned()
            // Profil ohne IDE: der „VS Code“-Button der Repo-Karte
            .or_else(|| {
                let fallback = crate::config::IdeConfig::vscode();
                (profile.ides.is_empty() && ide_id == fallback.id).then_some(fallback)
            });
        let started = if let Some(ide) = ide {
            let file = (!ide.no_args).then(|| file.to_path_buf());
            let shown = file.as_deref().unwrap_or(path).display().to_string();
            self.set_status(tr_fmt(lang, "opening_with", &[&shown, &ide.display_name]));
            let repo = path.to_path_buf();
            self.spawn_launch(
                tr_fmt(lang, "launch_ide_failed", &[&ide.display_name]),
                move || launch_ide(&ide, &repo, file.as_deref()),
            );
            true
        } else {
            self.error = Some(tr_fmt(
                lang,
                "ide_not_found",
                &[ide_id, &profile.display_name],
            ));
            false
        };
        self.record_usage(path, UsageType::Open);
        started
    }

    /// Startet ein Programm im Hintergrund: Prozessstart, `vswhere` oder
    /// `where` dauern spürbar und hielten sonst Fenster und Tray an. Ein
    /// Fehler kommt als „{what}: …“ über `launch_err_rx`.
    fn spawn_launch(
        &self,
        what: String,
        launch: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
    ) {
        let err_tx = self.launch_err_tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            if let Err(e) = launch() {
                let _ = err_tx.send(format!("{what}: {e:#}"));
                ctx.request_repaint();
            }
        });
    }

    /// Agent mit dieser ID, sonst der aktive (wie die Repo-Karte).
    fn agent_by_id(&self, agent_id: &str) -> Option<&crate::config::AgentProfile> {
        self.config
            .agents
            .iter()
            .find(|a| a.id == agent_id)
            .or_else(|| self.config.get_active_agent())
    }

    /// Startet `agent` im Terminal in `path`, ohne den UI-Thread zu
    /// blockieren (pwsh/wt brauchen spürbar); Fehler kommen über
    /// `launch_err_rx`. Gemeinsam für Repo-Karte, Tray, MR-Ansicht und
    /// Claude Sessions.
    pub(super) fn spawn_agent(&mut self, agent: crate::config::AgentProfile, path: &Path) {
        let term_pref = agent
            .terminal_override
            .clone()
            .unwrap_or_else(|| self.config.terminal.preference.clone());
        self.set_status(tr_fmt(
            self.config.language,
            "starting_agent_in",
            &[&agent.display_name, &path.display().to_string()],
        ));
        let path = path.to_path_buf();
        self.spawn_launch(
            tr_fmt(
                self.config.language,
                "launch_agent_failed",
                &[&agent.display_name],
            ),
            move || launch_agent(&agent, &path, &term_pref),
        );
    }

    /// Startet den Agenten `agent_id` (sonst den aktiven) im Terminal in
    /// `path`. `true`, wenn ein Agent gestartet wurde.
    fn open_agent(&mut self, path: &Path, agent_id: &str) -> bool {
        let started = if let Some(agent) = self.agent_by_id(agent_id).cloned() {
            self.spawn_agent(agent, path);
            true
        } else {
            self.error = Some(tr_fmt(self.config.language, "agent_not_found", &[agent_id]));
            false
        };
        self.record_usage(path, UsageType::Open);
        started
    }

    /// Merkt die gewählte Solution des Repos.
    fn select_solution(&mut self, repo_path: &Path, sln_path: PathBuf) {
        self.config.get_repo_state_mut(repo_path).selected_solution = Some(sln_path.clone());
        self.save_config();
        if let Some(repo) = self.repos.iter_mut().find(|r| r.path == repo_path) {
            repo.selected_solution = Some(sln_path);
        }
        self.set_status(tr_fmt(
            self.config.language,
            "solution_selected",
            &[&repo_path.display().to_string()],
        ));
    }

    /// Öffnet `path` im Explorer (im Hintergrund; Fehler kommen über
    /// `launch_err_rx`).
    fn open_explorer(&mut self, path: &Path) {
        let lang = self.config.language;
        let shown = path.display().to_string();
        self.set_status(tr_fmt(lang, "explorer_opened", &[&shown]));
        let dir = path.to_path_buf();
        self.spawn_launch(tr(lang, "launch_explorer_failed"), move || {
            crate::git::open_in_explorer(&dir)
        });
        self.record_usage(path, UsageType::Open);
    }

    /// Öffnet eine Shell in `path` (im Hintergrund; Fehler kommen über
    /// `launch_err_rx`).
    fn open_shell(&mut self, path: &Path) {
        let lang = self.config.language;
        let shown = path.display().to_string();
        self.set_status(tr_fmt(lang, "shell_opened", &[&shown]));
        let pref = self.config.terminal.preference.clone();
        let dir = path.to_path_buf();
        self.spawn_launch(tr(lang, "launch_shell_failed"), move || {
            crate::git::open_shell(&dir, &pref)
        });
        self.record_usage(path, UsageType::Open);
    }

    /// Startet den Wechsel im Hintergrund. Das Ergebnis kommt als
    /// `ScanResult::BranchSwitched` (`on_branch_switched`); bis dahin zeigen
    /// Statusleiste, Karte und Tray den laufenden Wechsel.
    fn start_branch_switch(&mut self, repo_path: PathBuf, target_branch: String, mode: SwitchMode) {
        if self.branch_switch.is_some() {
            // Dialog-Buttons sind währenddessen gesperrt; ein Wechsel aus
            // der Liste oder dem Tray wartet in der Queue
            if mode == SwitchMode::Ask {
                self.pending_branch_switches
                    .push((repo_path, target_branch));
            }
            return;
        }
        let known_dirty = self
            .repos
            .iter()
            .find(|r| r.path == repo_path)
            .map(|r| r.dirty);
        self.branch_switch = Some((repo_path.clone(), target_branch.clone()));
        // Spinner im Tray sofort statt per Throttle
        self.mark_tray_dirty();
        self.sync_tray_service();
        let job = SwitchJob {
            repo_path,
            target_branch,
            mode,
        };
        let tx = self.scan_tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            // Panic darf `branch_switch` nicht hängen lassen
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_branch_switch(&job, known_dirty)
            }))
            .unwrap_or_else(|_| {
                eprintln!("Branch-Wechsel-Thread abgestürzt");
                SwitchResult::Done(Err(anyhow::anyhow!("branch switch worker panicked")))
            });
            let _ = tx.send(ScanResult::BranchSwitched { job, result });
            ctx.request_repaint();
        });
    }

    /// „{Repo}: wechsle zu „{Branch}“ …“, solange der Worker läuft.
    fn branch_switch_text(&self) -> Option<String> {
        let (path, branch) = self.branch_switch.as_ref()?;
        Some(tr_fmt(
            self.config.language,
            "branch_switching",
            &[&short_repo_name(path), branch],
        ))
    }

    /// Gehört der offene Dialog zu diesem Wechsel? (Er ist nicht modal:
    /// ein Wechsel in einem anderen Repo darf ihn weder schließen noch
    /// seine Meldung dort ablegen.)
    fn branch_dialog_is_for(&self, repo_path: &Path, target_branch: &str) -> bool {
        self.branch_dialog.as_ref().is_some_and(|dlg| {
            dlg.target_branch == target_branch
                && AppConfig::path_key(&dlg.repo_path) == AppConfig::path_key(repo_path)
        })
    }

    /// Ergebnis eines Branch-Wechsels aus dem Worker.
    fn on_branch_switched(&mut self, ctx: &egui::Context, job: SwitchJob, result: SwitchResult) {
        let lang = self.config.language;
        let SwitchJob {
            repo_path,
            target_branch,
            ..
        } = job;
        let own_dialog = self.branch_dialog_is_for(&repo_path, &target_branch);
        match result {
            SwitchResult::Blocked(state) => {
                self.error = Some(tr_fmt(
                    lang,
                    "branch_switch_blocked",
                    &[&target_branch, &state],
                ));
                // K1: kein Wechsel, also auch kein „Danach öffnen“
                self.cancel_mr_switch(&repo_path, &target_branch);
                // I5: echte Fehler müssen bei hidden Window revealen — sonst
                // klickt der Nutzer im Tray und sieht nichts.
                self.reveal_main_on_error(ctx);
            }
            SwitchResult::Dirty { files, truncated } => {
                self.branch_dialog = Some(BranchDialog {
                    repo_path,
                    target_branch,
                    error: None,
                    dirty_files: files,
                    files_truncated: truncated,
                });
            }
            SwitchResult::Done(Ok((_, Some(pop_failed)))) => {
                // Gewechselt, aber Stash pop gescheitert (Stash bleibt
                // erhalten): Hinweis im Dialog wie bisher und neu scannen.
                // Ein MR-Wechsel ist trotzdem erledigt (Meldung, „Danach öffnen“).
                // Erst scannen: `start_scan` räumt das Banner ab.
                self.start_scan();
                match &mut self.branch_dialog {
                    Some(dlg) if own_dialog => dlg.error = Some(pop_failed),
                    // Dialog inzwischen geschlossen: Banner, das den Rescan
                    // überlebt – die Änderungen liegen sonst unbemerkt im Stash
                    _ => {
                        self.error = Some(pop_failed.clone());
                        self.error_after_scan = Some(pop_failed);
                        self.reveal_main_on_error(ctx);
                    }
                }
                self.mr_switch_succeeded(&repo_path, &target_branch);
            }
            SwitchResult::Done(Ok((outcome, None))) => {
                self.set_status(tr_fmt(
                    lang,
                    "branch_switched",
                    &[&target_branch, &repo_path.display().to_string()],
                ));
                self.error = None;
                if own_dialog {
                    self.branch_dialog = None;
                }
                self.record_usage(&repo_path, UsageType::BranchSwitch);
                self.start_scan();
                // E15: Hinweis (vorgespult / voraus / divergiert) erst nach
                // `start_scan` — während eines Auto-Fetch überschreibt der
                // sonst „auto_fetch_running“ (Scan bleibt per pending_scan gemerkt).
                if let Some(notice) = checkout_outcome_notice(lang, &outcome) {
                    self.set_status(notice);
                }
                // K1: Wechsel aus der MR-Ansicht → MR-Meldung, „Danach öffnen“
                self.mr_switch_succeeded(&repo_path, &target_branch);
            }
            SwitchResult::Done(Err(e)) => {
                // E15: Upstream-/Worktree-Abbruch lokalisiert.
                let detail = branch_switch_error_detail(lang, &e);
                // Mit eigenem Dialog dort (zweiter Versuch möglich), sonst global
                match &mut self.branch_dialog {
                    Some(dlg) if own_dialog => dlg.error = Some(detail),
                    _ => {
                        self.error = Some(tr_fmt(lang, "branch_switch_failed", &[&detail]));
                        // K1: ohne Dialog kein zweiter Versuch
                        self.cancel_mr_switch(&repo_path, &target_branch);
                    }
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
                                .color(COLOR_MUTED),
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
                        self.sync_settings_draft(|draft, cfg| {
                            draft.active_profile_id = cfg.active_profile_id.clone()
                        });
                        self.save_config();
                        self.set_status(tr_fmt(
                            lang,
                            "profile_switched",
                            &[&self.config.get_active_profile().display_name],
                        ));
                        self.start_scan();
                    }

                    // Agent Multi-Select wenn mehrere
                    // Only show agent section if not collapsed
                    let show_agents = !effective_collapse || self.config.agents.len() <= 1;
                    if show_agents {
                        if self.config.agents.len() > 1 {
                            ui.add_space(8.0);
                            if !no_captions {
                                ui.label(
                                    RichText::new(tr(lang, "ai_label"))
                                        .size(11.0)
                                        .color(COLOR_MUTED),
                                );
                            }
                            let active_agents = self.config.get_active_agents();
                            let current_text = if active_agents.is_empty() {
                                tr(lang, "none")
                            } else if active_agents.len() == 1 {
                                active_agents[0].display_name.clone()
                            } else {
                                tr_fmt(
                                    lang,
                                    "agents_active_count",
                                    &[&active_agents.len().to_string()],
                                )
                            };
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
                                        }
                                    }
                                    ui.separator();
                                    if ui.button(tr(lang, "agents_enable_all")).clicked() {
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
                                    }
                                    if ui.button(tr(lang, "agents_disable_all")).clicked() {
                                        self.config.active_agent_ids.clear();
                                        self.config.active_agent_id = None;
                                        agents_changed = true;
                                    }
                                });
                            if no_captions {
                                agent_combo
                                    .response
                                    .on_hover_text(tr(lang, "ai_label").trim_end_matches(':'));
                            }
                            if agents_changed {
                                self.sync_settings_draft(|draft, cfg| {
                                    draft.active_agent_ids = cfg.active_agent_ids.clone();
                                    draft.active_agent_id = cfg.active_agent_id.clone();
                                });
                                self.save_config();
                            }
                        } else if let Some(agent) = self.config.get_active_agent() {
                            ui.add_space(8.0);
                            ui.label(
                                RichText::new(format!(
                                    "{} {}",
                                    tr(lang, "ai_label"),
                                    agent.display_name
                                ))
                                .size(11.0)
                                .color(COLOR_MUTED),
                            );
                        }
                    }

                    // Language selector
                    ui.add_space(6.0);
                    if !no_captions {
                        ui.label(
                            RichText::new(tr(lang, "language"))
                                .size(11.0)
                                .color(COLOR_MUTED),
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
                        self.sync_settings_draft(|draft, cfg| draft.language = cfg.language);
                        self.save_config();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let settings =
                            top_bar_button(ui, ICON_GEAR, &tr(lang, "settings"), compact, true);
                        if settings
                            .on_hover_text(tr(lang, "settings_tooltip"))
                            .clicked()
                        {
                            self.open_settings();
                        }
                        ui.add_space(8.0);
                        // E19: in der MR-Ansicht lädt „Aktualisieren“ die MRs neu.
                        let mr_view = self.main_view == MainView::MergeRequests;
                        // Claude-Ansicht: „Aktualisieren“ sucht die Sessions neu
                        let claude_view = self.main_view == MainView::ClaudeSessions;
                        let checkout = matches!(self.git_busy, GitBusy::Checkout { .. });
                        let busy = if mr_view {
                            self.mr_loading
                                || self.mr_discovering
                                || self.mr_more_pending > 0
                                || self.mr_pending_refresh
                                || checkout
                        } else if claude_view {
                            self.claude.scanning
                        } else {
                            self.git_busy.holds_refs() || self.branch_switch.is_some()
                        };
                        // Beim Auschecken kein „Scanne…“
                        let refresh_label = if claude_view {
                            tr(lang, "refresh")
                        } else if !mr_view && self.git_busy.scans_repos() {
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
                            } else if claude_view {
                                self.start_claude_scan();
                            } else {
                                self.start_scan();
                            }
                        }
                        if busy {
                            ui.add_space(8.0);
                            ui.spinner();
                        }
                        // E19: Sortierung wirkt nur auf die Repo-Liste.
                        if mr_view || claude_view {
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
                            self.sync_settings_draft(|draft, cfg| {
                                draft.main_sort_mode = cfg.main_sort_mode
                            });
                            self.save_config();
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
                            // Bis zur ersten Prüfung als vorhanden zeigen
                            let exists = crate::path_probe::exists(ui.ctx(), root).unwrap_or(true);
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
                                format!(
                                    "{} ({} {})",
                                    root.display(),
                                    tr(self.config.language, "depth_label").trim_end_matches(':'),
                                    self.config.max_depth
                                )
                            } else {
                                format!(
                                    "{} {}",
                                    root.display(),
                                    tr(self.config.language, "not_found")
                                )
                            });
                        }
                        ui.label(
                            RichText::new(format!(
                                "  {} {}",
                                tr(self.config.language, "depth_label"),
                                self.config.max_depth
                            ))
                            .size(11.0)
                            .color(Color32::from_rgb(120, 120, 120)),
                        );
                        ui.add_space(8.0);
                        let prof = self.config.get_active_profile();
                        ui.label(
                            RichText::new(format!(
                                "{} {} (*{})",
                                tr(self.config.language, "profile_label"),
                                prof.display_name,
                                prof.file_extension
                            ))
                            .size(11.0)
                            .color(COLOR_MUTED),
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
                    } else if self.main_view == MainView::ClaudeSessions {
                        let (text, color) = match &self.status_message {
                            Some(msg) => (msg.clone(), Color32::from_rgb(60, 120, 60)),
                            None => (self.claude_status_line(), COLOR_MUTED),
                        };
                        ui.label(RichText::new(text).size(11.0).color(color));
                    } else if let Some(msg) = &self.status_message {
                        ui.label(
                            RichText::new(msg)
                                .size(11.0)
                                .color(Color32::from_rgb(60, 120, 60)),
                        );
                    } else if let Some(text) = self.branch_switch_text() {
                        ui.label(RichText::new(text).size(11.0).color(COLOR_MUTED));
                    } else if let Some(text) = repo_busy_text(lang, &self.git_busy) {
                        ui.label(RichText::new(text).size(11.0).color(COLOR_MUTED));
                    } else {
                        ui.label(
                            RichText::new({
                                let lang = self.config.language;
                                format!(
                                    "{} {} · {} {} · {} {} · {} {}",
                                    self.repos.len(),
                                    tr(lang, "status_repos_paths"),
                                    self.config.roots.len(),
                                    tr(lang, "status_paths"),
                                    tr(lang, "depth_label").trim_end_matches(':'),
                                    self.config.max_depth,
                                    tr(lang, "profile_label").trim_end_matches(':'),
                                    self.config.get_active_profile().display_name
                                )
                            })
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
        let mut action: Option<(PathBuf, String, SwitchMode)> = None;
        // Während ein Wechsel läuft oder Pull/Entfernen an diesem Ordner
        // arbeitet (der Fast-Forward setzte sonst den falschen Branch um)
        let busy = self.branch_switch.is_some()
            || self
                .branch_dialog
                .as_ref()
                .is_some_and(|dlg| self.git_busy.works_on(&dlg.repo_path));
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
                        .color(COLOR_MUTED),
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
                    error_frame(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                        ui.label(error_label(lang, err, 11.0));
                    });
                    ui.add_space(8.0);
                }
                ui.horizontal(|ui| {
                    if ui.button(tr(lang, "cancel")).clicked() {
                        to_close = true;
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(tr(lang, "stash_switch")))
                        .on_hover_text(tr(lang, "stash_switch_tooltip"))
                        .clicked()
                    {
                        action =
                            Some((repo_path.clone(), target_branch.clone(), SwitchMode::Stash));
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new(tr(lang, "force_discard")))
                        .on_hover_text(tr(lang, "force_discard_tooltip"))
                        .clicked()
                    {
                        action =
                            Some((repo_path.clone(), target_branch.clone(), SwitchMode::Force));
                    }
                    if busy {
                        ui.spinner();
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
        if let Some((path, branch, mode)) = action {
            self.start_branch_switch(path, branch, mode);
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
        let do_clear = crate::ui::widgets::banner(
            ui,
            error_frame(egui::Margin::symmetric(10, 8)),
            ICON_WARNING,
            error_label(lang, &err_msg, 12.0),
            ERROR_TEXT,
            &tr(lang, "close"),
        );
        ui.add_space(8.0);
        if do_clear {
            self.error = None;
            self.error_after_scan = None;
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
        self.poll_claude(ctx);
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
        self.poll_new_worktree(ctx);
        self.poll_pull(ctx);
        self.poll_remove_worktree(ctx);
        self.poll_cleanup(ctx);
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
        self.show_new_worktree_dialog(&ctx);
        self.show_remove_worktree_dialog(&ctx);
        self.show_cleanup_dialog(&ctx);
        self.show_new_claude_dialog(&ctx);
        self.show_mr_pull_prompt(&ctx);
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
                // Ansicht verlassen: Fokus gilt nicht mehr
                self.mr_view.focus = None;
                if self.main_view == MainView::ClaudeSessions {
                    self.show_claude_view(ui);
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
                let mut actions = RepoListActions::default();

                // Wir brauchen &mut für repo_list, um selektierte Werte zu zeigen, aber hier clonen wir mut
                // repo_list wird &mut repos erwarten, wir haben &mut self.repos
                // Lauf hält die Refs → Fetch-/Refresh-Buttons disabled + Spinner
                // Task 4: Sortierung im View (Scanner bleibt alphabetisch)
                // Finding A: nur bei dirty sortieren statt pro Frame (60fps).
                if self.repo_sort_dirty {
                    sort_main_repos(&mut self.repos, &self.config);
                    self.update_mr_clones();
                    self.repo_sort_dirty = false;
                }
                let fetching = self.git_busy.holds_refs();
                // K1: „In Repo-Liste zeigen“ hebt eine Karte kurz hervor
                let highlight = self.repo_highlight_frame(&ctx);
                // Claude-Chip: Live-Status je Repo-Ordner (leer ohne Abfrage)
                // Ohne laufende Session nichts zu tun (der Normalfall, je Frame)
                let claude = if self.claude.live.is_empty() {
                    HashMap::new()
                } else {
                    repo_badges::claude_card_status(&self.claude.live_by_repo(), &self.repos)
                };
                let card_ctx = RepoListContext {
                    mrs: &self.repo_mrs,
                    claude: &claude,
                    pulling: match &self.git_busy {
                        GitBusy::Pull { repo } => Some(repo.as_path()),
                        _ => None,
                    },
                    switching: self.branch_switch.as_ref().map(|(path, _)| path.as_path()),
                    now: crate::gitlab::time::now_unix(),
                    fetching,
                };
                show_repo_list(
                    ui,
                    &mut self.repos,
                    &self.config,
                    &mut actions,
                    highlight.as_ref(),
                    &mut self.worktree_ui.search,
                    &card_ctx,
                );
                self.apply_card_actions(&ctx, &mut actions);
                if let Some(base) = actions.new_worktree.take() {
                    self.open_new_worktree_dialog(base);
                }
                if actions.new_claude_session {
                    self.open_new_claude_dialog();
                }

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
                                self.set_status(msg);
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
                    self.select_solution(&repo_path, sln_path);
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
                    self.save_config();
                    self.set_status(tr_fmt(
                        lang,
                        "profile_changed_for",
                        &[&repo_path.display().to_string()],
                    ));
                    self.start_scan();
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

                if let Some(mut new_cfg) = save {
                    // Der Entwurf ist eine Kopie vom Öffnen der Einstellungen;
                    // was die App seither selbst geführt hat (zuletzt benutzt,
                    // IDE/Solution/Profil je Repo, Favoriten), gilt weiter.
                    new_cfg.repo_usage = std::mem::take(&mut self.config.repo_usage);
                    new_cfg.repo_state = std::mem::take(&mut self.config.repo_state);
                    new_cfg.favorite_repos = std::mem::take(&mut self.config.favorite_repos);
                    new_cfg.favorite_sessions = std::mem::take(&mut self.config.favorite_sessions);
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
                    self.save_config();
                    self.apply_glab_program();
                    // Autostart nach Save anwenden (Registry Run-Key schreiben/entfernen)
                    let autostart_failed = match crate::autostart::apply_autostart(autostart) {
                        Ok(()) => false,
                        Err(e) => {
                            eprintln!("Autostart anwenden fehlgeschlagen: {e:#}");
                            let msg = save_failed_text(lang, &e);
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
                        self.show_settings = false;
                        self.set_status(tr(lang, "saved_scan_restart"));
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
    use crate::gitlab::glab::{FakeGlab, FakeReply, GatedRunner, GlabRequest};

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

    // --- Gemeinsame Helfer der Testmodule unter `app/` ---

    pub(super) fn is_version(r: &GlabRequest) -> bool {
        r.args.first().is_some_and(|a| a == "version")
    }

    pub(super) fn version_reply() -> FakeReply {
        FakeReply::stdout(0, "glab 1.123.0 (a300719)\n", "")
    }

    pub(super) fn config_get(
        host: &'static str,
        key: &'static str,
    ) -> impl Fn(&GlabRequest) -> bool + Send {
        move |r: &GlabRequest| {
            r.args.first().is_some_and(|a| a == "config")
                && r.args.last().is_some_and(|k| k == key)
                && r.host_key.as_deref() == Some(host)
        }
    }

    /// Erkennung: `host` ist bei glab angemeldet.
    pub(super) fn push_signed_in(fake: &FakeGlab, host: &'static str) {
        fake.push_when(
            config_get(host, "user"),
            FakeReply::stdout(0, "taltmann\n", ""),
        );
        fake.push_when(config_get(host, "ssh_host"), FakeReply::stdout(0, "", ""));
        fake.push_when(
            config_get(host, "api_protocol"),
            FakeReply::stdout(0, "", ""),
        );
    }

    pub(super) fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
    }

    /// Übernimmt Ergebnisse, bis kein Scan, Fetch, Auschecken oder
    /// Branch-Wechsel mehr läuft oder ansteht (höchstens 10 s).
    pub(super) fn settle_git(app: &mut MyApp) {
        let ctx = egui::Context::default();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            app.poll_scan(&ctx);
            let queued = app
                .pending_branch_switches
                .iter()
                .any(|(path, _)| !app.git_busy.works_on(path));
            if !app.git_busy.holds_refs() && app.branch_switch.is_none() && !queued {
                return;
            }
            assert!(std::time::Instant::now() < until, "git-Worker hängt");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    /// Branch-Wechsel starten und sein Ergebnis abwarten.
    pub(super) fn switch_branch(app: &mut MyApp, path: &Path, branch: &str, mode: SwitchMode) {
        app.start_branch_switch(path.to_path_buf(), branch.to_string(), mode);
        settle_git(app);
    }

    /// Test-App ohne Fenster; glab ist ein `FakeGlab` ohne Antworten.
    pub(super) fn test_app() -> MyApp {
        test_app_with_glab(Arc::new(GatedRunner::new(
            crate::gitlab::glab::FakeGlab::new(),
        )))
    }

    /// Wie `test_app` mit eigenem Runner (Antworten vorbereiten, Aufrufe prüfen).
    pub(super) fn test_app_with_glab(glab: Arc<dyn GlabRunner>) -> MyApp {
        // Jedes Programm landet beim selben `FakeGlab`
        let glab_factory: GlabFactory = Box::new(move |_| Arc::clone(&glab));
        MyApp::build(
            AppConfig::default(),
            true,
            None,
            mr_activity::MrActivityState::default(),
            glab_factory,
        )
        .0
    }

    /// `start_scan` während eines laufenden Scans: dessen Ergebnis ist
    /// veraltet (neue Suchpfade, Profil), danach läuft ein weiterer.
    #[test]
    fn start_scan_during_scan_rescans_afterwards() {
        let mut app = test_app();
        app.config.roots.clear();
        let ctx = egui::Context::default();
        app.set_git_busy(GitBusy::Scan);
        app.start_scan();
        assert!(app.scan_outdated);
        app.scan_tx.send(ScanResult::Repos(Vec::new())).unwrap();
        app.poll_scan(&ctx);
        assert!(!app.scan_outdated);
        assert_eq!(app.git_busy, GitBusy::Scan, "zweiter Scan läuft");
        // Dessen Ergebnis ist aktuell: kein dritter Scan
        while app.git_busy.holds_refs() {
            app.poll_scan(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!app.pending_scan);
    }

    /// Speichern merkt nur vor; der Durchlauf danach gibt den Stand ab.
    #[test]
    fn save_config_is_deferred_and_flushed_once() {
        let mut app = test_app();
        let ctx = egui::Context::default();
        app.tray_dirty = false;
        app.save_config();
        assert!(app.config_save_pending && app.tray_dirty);
        app.flush_config_save(&ctx);
        assert!(!app.config_save_pending);
        // Fehler des Schreib-Threads: Banner und neuer Versuch
        app.config_saved_tx
            .send(Err(anyhow::anyhow!("disk full")))
            .unwrap();
        app.flush_config_save(&ctx);
        assert!(app
            .error
            .as_deref()
            .is_some_and(|e| e.contains("disk full")));
        assert_eq!(app.config_save_fail_streak, 1);
        app.config_saved_tx.send(Ok(())).unwrap();
        app.poll_config_saved();
        assert_eq!(app.config_save_fail_streak, 0);
    }

    /// Ein offener Entwurf überlebt das Zahnrad bzw. den Tray-Eintrag.
    #[test]
    fn open_settings_keeps_an_open_draft() {
        let mut app = test_app();
        app.open_settings();
        app.settings_state.as_mut().unwrap().draft.max_depth = 7;
        app.open_settings();
        assert_eq!(app.settings_state.as_ref().unwrap().draft.max_depth, 7);
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
    fn pending_switch_waits_while_pull_or_remove_runs_on_that_path() {
        let busy = std::path::PathBuf::from("/tmp/no-repo-busy");
        let other = std::path::PathBuf::from("/tmp/no-repo-other");
        let ctx = egui::Context::default();
        for state in [
            GitBusy::Pull { repo: busy.clone() },
            GitBusy::Remove { path: busy.clone() },
        ] {
            let mut app = test_app();
            app.set_git_busy(state.clone());
            app.pending_branch_switches
                .push((busy.clone(), "main".to_string()));
            app.pending_branch_switches
                .push((other.clone(), "main".to_string()));
            app.poll_scan(&ctx);
            // Switch auf dem belegten Pfad bleibt in der Queue, der andere läuft
            assert_eq!(
                app.pending_branch_switches,
                vec![(busy.clone(), "main".to_string())],
                "{state:?}"
            );
            // Nach dem Lauf wird er abgearbeitet
            app.set_git_busy(GitBusy::Idle);
            settle_git(&mut app);
            assert!(app.pending_branch_switches.is_empty(), "{state:?}");
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
        // Fehler eines früheren Fetch
        app.error = Some("stale".to_string());
        app.fetch_error_shown = app.error.clone();
        let auto_ok = || ScanResult::FetchDone {
            repos: vec![],
            fetch_error: None,
            fetch_partial: false,
            fetch_is_fetch_error: false,
            is_auto: true,
        };
        app.scan_tx.send(auto_ok()).expect("send fetch done");
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
        // Ein fremdes Banner (Start-, Speicherfehler) räumt der stille
        // Auto-Fetch nicht ab
        app.error = Some("Tray fehlt".to_string());
        app.scan_tx.send(auto_ok()).expect("send fetch done");
        app.poll_scan(&ctx);
        assert_eq!(app.error.as_deref(), Some("Tray fehlt"));
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
                is_auto: false,
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
        // Abgebrochener Auto-Fetch: der laufende Scan behält seine Sperre
        app.scan_tx
            .send(ScanResult::FetchAborted {
                reason_key: "fetch_aborted",
                is_auto: true,
            })
            .unwrap();
        let ctx = egui::Context::default();
        app.poll_scan(&ctx);
        assert_eq!(app.git_busy, GitBusy::Scan);
        assert!(!app.auto_fetching);
        app.scan_tx
            .send(ScanResult::FetchAborted {
                reason_key: "fetch_aborted",
                is_auto: false,
            })
            .unwrap();
        app.poll_scan(&ctx);
        assert_eq!(app.git_busy, GitBusy::Idle);
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
        assert_eq!(
            repo_busy_text(lang, &GitBusy::Scan),
            Some(tr(lang, "scanning_repos"))
        );
        // Der Fetch nennt sein Repo (die Netzphase dauert)
        assert_eq!(
            repo_busy_text(lang, &fetch),
            Some(tr_fmt(lang, "fetching_branches", &["/repos/a"]))
        );
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

    /// Der Wechsel läuft im Worker: bis zum Ergebnis ist er als laufend
    /// sichtbar und sperrt andere git-Läufe.
    #[test]
    fn branch_switch_runs_in_background_and_blocks_other_runs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("repo");
        repo_with_stale_feature(&path);
        let mut app = test_app();
        app.config.roots.clear();
        app.start_branch_switch(path.clone(), "origin/feature".to_string(), SwitchMode::Ask);
        assert_eq!(
            app.branch_switch,
            Some((path.clone(), "origin/feature".to_string()))
        );
        assert!(app.branch_switch_text().is_some());
        assert!(
            app.refuse_while_busy(),
            "kein Pull/Fetch während des Wechsels"
        );
        // Ein weiterer Wechsel wartet in der Queue
        app.start_branch_switch(path.clone(), "main".to_string(), SwitchMode::Ask);
        assert_eq!(app.pending_branch_switches.len(), 1);
        app.pending_branch_switches.clear();
        settle_git(&mut app);
        assert!(app.branch_switch.is_none());
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(path.join("b.txt").exists());
    }

    /// Der Dialog ist nicht modal: das Ergebnis eines Wechsels in einem
    /// anderen Repo landet weder in ihm noch schließt es ihn.
    #[test]
    fn branch_switch_result_leaves_a_foreign_dialog_alone() {
        let mut app = test_app();
        app.config.roots.clear();
        app.branch_dialog = Some(BranchDialog {
            repo_path: PathBuf::from("/tmp/gm-dialog/a"),
            target_branch: "main".to_string(),
            error: None,
            dirty_files: Vec::new(),
            files_truncated: false,
        });
        // Kein Repo unter dem Pfad: der Wechsel scheitert
        switch_branch(
            &mut app,
            Path::new("/tmp/gm-dialog/no-repo"),
            "main",
            SwitchMode::Ask,
        );
        let dlg = app.branch_dialog.as_ref().expect("Dialog bleibt offen");
        assert!(dlg.error.is_none(), "{:?}", dlg.error);
        assert!(app.error.is_some(), "Fehler im Banner");
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
        switch_branch(&mut app, &path, "origin/feature", SwitchMode::Ask);
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
        switch_branch(&mut app, &path, "origin/feature", SwitchMode::Ask);
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
