//! Merge-Request-Ansicht nach Design B (zweispaltiges Dashboard, E8):
//! „Meine Merge Requests“ und „Von anderen“ mit Filter-Chips (E13), darunter
//! „Nicht beteiligt“; Zeilen mit Approvals, klickbarer Pipeline (E7),
//! Threads und Checkout-Menü K1 (E5/E6) sowie die Zustandskarten. Zeichnet
//! nur und meldet Klicks über `MrViewActions`; Abruf, Auschecken und Öffnen
//! verdrahtet `app.rs` (Task 10/11). Farben nur aus `mr_palette` (E14).

use crate::config::{AfterAction, AfterCheckoutPref, AgentProfile, IdeConfig, MrConfig, Theme};
use crate::git::RepoInfo;
use crate::gitlab::checkout::{
    build_checkout_menu, checkout_brief, BlockReason, CheckoutBrief, CheckoutMenu, CloneOption,
    CloneState, MenuLookup, MrRefInfo, OpenBranches, UnavailableReason,
};
use crate::gitlab::glab::{GlabErrorKind, GlabOutput};
use crate::gitlab::model::{
    approvals, chip_counts, empty_state, filter_others, initials, mr_url, others_header_count,
    pipeline_url, ready, Approvals, ChipCounts, Chips, EmptyKind, HeaderCount, MergeRequest,
    MrPart, MrSnapshot, MyReview, PipelineState, Reason, UninvolvedList,
};
use crate::gitlab::remote::{HostMap, LocalProject};
use crate::gitlab::service::{error_text, is_auth_error};
use crate::gitlab::time::relative_time;
use crate::i18n::{tr, tr_fmt, Language};
use crate::ui::theme::{mr_palette, MrPalette};
use egui::{
    vec2, Align, Align2, Atoms, Button, Color32, CursorIcon, FontId, Frame, Id, Image, ImageSource,
    Label, Layout, Margin, Popup, PopupCloseBehavior, RectAlign, Response, RichText, ScrollArea,
    Sense, Spinner, Stroke, StrokeKind, Ui, UiBuilder, WidgetInfo, WidgetType,
};
use std::hash::Hash;
use std::path::{Path, PathBuf};

// Icons: Phosphor Regular (MIT), mit `color="#FFFFFF"`, damit `tint` sie
// einfärbt – resvg rendert `currentColor` sonst schwarz.
const ICON_CHECK_CIRCLE: ImageSource =
    egui::include_image!("../../assets/icons/mr/check-circle.svg");
const ICON_X_CIRCLE: ImageSource = egui::include_image!("../../assets/icons/mr/x-circle.svg");
const ICON_CIRCLE_NOTCH: ImageSource =
    egui::include_image!("../../assets/icons/mr/circle-notch.svg");
const ICON_CLOCK: ImageSource = egui::include_image!("../../assets/icons/mr/clock.svg");
const ICON_MINUS_CIRCLE: ImageSource =
    egui::include_image!("../../assets/icons/mr/minus-circle.svg");
const ICON_PROHIBIT: ImageSource = egui::include_image!("../../assets/icons/mr/prohibit.svg");
const ICON_SKIP_CIRCLE: ImageSource =
    egui::include_image!("../../assets/icons/mr/skip-forward-circle.svg");
const ICON_PLAY_CIRCLE: ImageSource = egui::include_image!("../../assets/icons/mr/play-circle.svg");
const ICON_CHAT: ImageSource = egui::include_image!("../../assets/icons/mr/chat-circle.svg");
const ICON_CHAT_DOTS: ImageSource =
    egui::include_image!("../../assets/icons/mr/chat-circle-dots.svg");
const ICON_CHAT_CHECK: ImageSource =
    egui::include_image!("../../assets/icons/mr/chat-circle-check.svg");
const ICON_CHECK: ImageSource = egui::include_image!("../../assets/icons/mr/check.svg");
const ICON_ARROW_U: ImageSource = egui::include_image!("../../assets/icons/mr/arrow-u-up-left.svg");
const ICON_USER: ImageSource = egui::include_image!("../../assets/icons/mr/user.svg");
const ICON_SORT: ImageSource = egui::include_image!("../../assets/icons/mr/sort-descending.svg");
const ICON_OPEN_EXTERNAL: ImageSource =
    egui::include_image!("../../assets/icons/mr/arrow-square-out.svg");
const ICON_PULL_REQUEST: ImageSource =
    egui::include_image!("../../assets/icons/mr/git-pull-request.svg");
const ICON_BRANCH: ImageSource = egui::include_image!("../../assets/icons/mr/git-branch.svg");
const ICON_TERMINAL: ImageSource =
    egui::include_image!("../../assets/icons/mr/terminal-window.svg");
const ICON_LOCK: ImageSource = egui::include_image!("../../assets/icons/mr/lock-simple.svg");
const ICON_COPY: ImageSource = egui::include_image!("../../assets/icons/mr/copy.svg");
const ICON_FOLDER_PLUS: ImageSource = egui::include_image!("../../assets/icons/mr/folder-plus.svg");
const ICON_DOWNLOAD: ImageSource =
    egui::include_image!("../../assets/icons/mr/download-simple.svg");
const ICON_DOTS: ImageSource = egui::include_image!("../../assets/icons/mr/dots-three.svg");
const ICON_FOLDER: ImageSource = egui::include_image!("../../assets/icons/mr/folder.svg");
const ICON_FOLDER_ARROW: ImageSource =
    egui::include_image!("../../assets/icons/mr/folder-arrow.svg");
const ICON_CODE: ImageSource = egui::include_image!("../../assets/icons/mr/code.svg");
const ICON_SPARKLE: ImageSource = egui::include_image!("../../assets/icons/mr/sparkle.svg");
const ICON_WARNING: ImageSource = egui::include_image!("../../assets/icons/mr/warning-circle.svg");

/// Mindestbreite einer Spalte: zwei Spalten ab 2 × 420 + 12 = 852 px
/// Inhaltsbreite (E8), damit B beim Mindestfenster zweispaltig bleibt.
const COLUMN_MIN_WIDTH: f32 = 420.0;
/// Spaltenabstand (`item_spacing.x`)
const COLUMN_GAP: f32 = 12.0;
/// Unter dieser Spaltenbreite entfallen die Mindestbreiten in Zeile 3.
const COMPACT_WIDTH: f32 = 460.0;
/// Zeilen „Nicht beteiligt“ je Schritt („{k} weitere laden“)
pub const UNINVOLVED_STEP: usize = 10;
/// Checkout-Menü (K1) bzw. Menü „Bereits ausgecheckt“
const MENU_WIDTH: f32 = 400.0;
const ALREADY_MENU_WIDTH: f32 = 320.0;
/// Zustandskarten
const CARD_WIDTH: f32 = 560.0;
/// Mehr Pips zeichnen wir nicht; die Zahl daneben reicht dann.
const MAX_PIPS: u32 = 10;

/// Spalten nebeneinander oder gestapelt in einer äußeren ScrollArea (E8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Columns,
    Stacked,
}

/// E8: zwei Spalten ab 852 px Inhaltsbreite.
pub fn layout_mode(width: f32) -> LayoutMode {
    if width >= 2.0 * COLUMN_MIN_WIDTH + COLUMN_GAP {
        LayoutMode::Columns
    } else {
        LayoutMode::Stacked
    }
}

/// Zustand der Ansicht für die Sitzung (nicht gespeichert, E13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrViewState {
    pub reviewer_on: bool,
    pub mention_on: bool,
    /// Chip „Alle offenen“
    pub show_uninvolved: bool,
    /// Angezeigte Zeilen „Nicht beteiligt“
    pub uninvolved_shown: usize,
}

impl MrViewState {
    /// Sitzungsstart: Reviewer und Erwähnt an, „Alle offenen“ wie eingestellt.
    pub fn new(cfg: &MrConfig) -> Self {
        let chips = Chips::session_start(cfg);
        Self {
            reviewer_on: chips.reviewer,
            mention_on: chips.mention,
            show_uninvolved: chips.all,
            uninvolved_shown: UNINVOLVED_STEP,
        }
    }

    pub fn chips(&self) -> Chips {
        Chips {
            reviewer: self.reviewer_on,
            mention: self.mention_on,
            all: self.show_uninvolved,
        }
    }
}

/// Wohin der MR-Branch ausgecheckt wird (K1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MrCheckoutKind {
    /// `git worktree add` im Review-Ordner bzw. neben dem Basis-Klon
    NewWorktree { path: PathBuf, base: PathBuf },
    /// `git clone` in einen neuen Ordner
    NewClone { path: PathBuf, url: String },
    /// Bestehenden Klon umschalten (Dialog bei Änderungen)
    Switch { clone: PathBuf },
}

/// Gewähltes Ziel im Checkout-Menü.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrCheckoutChoice {
    /// glab-Host-Schlüssel und globale ID: `iid` allein ist nur je Projekt
    /// eindeutig, `app.rs` sucht den MR damit im Stand
    pub host_key: String,
    pub mr_id: String,
    pub iid: u64,
    pub kind: MrCheckoutKind,
    /// „Danach öffnen“
    pub after: AfterAction,
}

/// Ziel im Menü „Bereits ausgecheckt in {Pfad}“.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTarget {
    ShowInRepoList,
    Agent(String),
    Shell,
    Explorer,
    Ide,
}

/// Klicks eines Frames; `app.rs` wertet sie danach aus (Task 10/11).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MrViewActions {
    /// MR, Pipeline (E7) im Browser
    pub open_url: Option<String>,
    pub checkout: Option<MrCheckoutChoice>,
    pub open_existing: Option<(PathBuf, OpenTarget)>,
    /// Neu laden (Karte „glab fehlt“)
    pub refresh: bool,
    /// „Alle offenen“ eingeschaltet, „Nicht beteiligt“ noch nicht geladen
    pub fetch_uninvolved: bool,
    /// Nächste Seite „Nicht beteiligt“
    pub load_more: bool,
    /// Einstellungen öffnen (Tab „Merge Requests“ ab Task 12)
    pub open_settings_mr_tab: bool,
}

/// Was die Ansicht je Frame braucht; baut `app.rs` (Task 10).
pub struct MrViewInput<'a> {
    pub lang: Language,
    pub theme: &'a Theme,
    pub cfg: &'a MrConfig,
    /// Letzter Stand; bleibt beim Neuladen sichtbar
    pub snapshot: Option<&'a MrSnapshot>,
    /// „Nicht beteiligt“ (erste oder weitere Seite) wird geladen
    pub uninvolved_loading: bool,
    /// glab fehlt (`NotInstalled`) oder ist zu alt (`TooOld`)
    pub glab_error: Option<&'a GlabErrorKind>,
    /// Scan oder lokale glab-Erkennung (E16) läuft noch: ohne GitLab-Host
    /// dann „Laden“ statt der Karten „Kein GitLab-Host“/„Keine Remotes“
    pub discovering: bool,
    /// Gescannte Repos; Klone für das Checkout-Menü, auch abgewählte (E1)
    pub repos: &'a [RepoInfo],
    pub hosts: &'a HostMap,
    pub locals: &'a [LocalProject],
    /// Aktive Agenten des Profils („Danach öffnen“, „{Agent} starten“)
    pub agents: &'a [&'a AgentProfile],
    /// IDE des Profils
    pub ide: Option<&'a IdeConfig>,
    /// MRU und Pfadprüfung; `exists` nur beim Öffnen des Menüs
    pub lookup: MenuLookup<'a>,
    /// Jetzt (Unix-Sekunden) für relative Zeiten
    pub now: i64,
}

/// Darstellung des Checkout-Buttons einer Zeile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ButtonKind {
    /// Menü K1 (neuer Worktree, neuer Klon, bestehender Klon)
    Menu,
    /// Ordner-Pfeil mit Menü „Bereits ausgecheckt in {Pfad}“
    AlreadyCheckedOut(PathBuf),
    /// Chip „nicht lokal“, Menü nur mit „Neuer Klon …“
    NotLocal,
    /// Fork-MR oder gelöschter Quell-Branch (E6)
    Disabled(UnavailableReason),
}

/// Button-Art einer Zeile: ein lokaler Stand gewinnt (auch bei Fork-MRs
/// nur ein Ordner), dann E6, dann „nicht lokal“.
pub fn checkout_button(brief: &CheckoutBrief) -> ButtonKind {
    if let Some(already) = &brief.already {
        return ButtonKind::AlreadyCheckedOut(already.path.clone());
    }
    if let Some(reason) = brief.unavailable {
        return ButtonKind::Disabled(reason);
    }
    if !brief.has_clone {
        return ButtonKind::NotLocal;
    }
    ButtonKind::Menu
}

pub fn checkout_tooltip(lang: Language, kind: &ButtonKind) -> String {
    match kind {
        ButtonKind::Menu => tr(lang, "mr_checkout_menu_tt"),
        ButtonKind::AlreadyCheckedOut(path) => {
            tr_fmt(lang, "mr_checked_out_tt", &[&path.display().to_string()])
        }
        ButtonKind::NotLocal => tr(lang, "mr_not_local_tt"),
        ButtonKind::Disabled(UnavailableReason::Fork) => tr(lang, "mr_fork_tt"),
        ButtonKind::Disabled(UnavailableReason::SourceBranchDeleted) => {
            tr(lang, "mr_branch_deleted_tt")
        }
    }
}

/// Chip unter „Danach öffnen“ (Einfachauswahl).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub action: AfterAction,
    pub label: String,
    pub selected: bool,
}

/// Chips „Danach öffnen“: aktive Agenten, Shell, Explorer, IDE des Profils,
/// Nichts. Vorausgewählt ist `last`; zeigt das auf einen nicht mehr aktiven
/// Agenten oder eine fehlende IDE, der erste Agent, sonst „Nichts“.
pub fn after_chips(
    lang: Language,
    active_agents: &[&AgentProfile],
    ide: Option<&IdeConfig>,
    last: &AfterAction,
) -> Vec<Chip> {
    let chip = |action, label| Chip {
        action,
        label,
        selected: false,
    };
    let mut chips: Vec<Chip> = active_agents
        .iter()
        .map(|a| chip(AfterAction::Agent(a.id.clone()), a.display_name.clone()))
        .collect();
    chips.push(chip(AfterAction::Shell, tr(lang, "mr_after_shell")));
    chips.push(chip(AfterAction::Explorer, tr(lang, "mr_after_explorer")));
    if let Some(ide) = ide {
        chips.push(chip(AfterAction::Ide, ide.display_name.clone()));
    }
    chips.push(chip(AfterAction::Nothing, tr(lang, "mr_after_nothing")));
    let pick = chips
        .iter()
        .position(|c| &c.action == last)
        .or_else(|| (!active_agents.is_empty()).then_some(0))
        .unwrap_or(chips.len() - 1);
    chips[pick].selected = true;
    chips
}

/// Vorbelegung „Danach öffnen“: fest eingestellt oder die letzte Wahl.
pub fn after_preselection(cfg: &MrConfig) -> &AfterAction {
    match &cfg.after_checkout_default {
        AfterCheckoutPref::Fixed(action) => action,
        AfterCheckoutPref::LastUsed => &cfg.after_checkout_last,
    }
}

/// Einzahl/Mehrzahl über getrennte Schlüssel (`…_one` ohne Platzhalter).
fn count_text(lang: Language, n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        tr(lang, one)
    } else {
        tr_fmt(lang, many, &[&n.to_string()])
    }
}

/// „{n} MRs aus abgewählten Repositories ausgeblendet“ (E1)
pub fn hidden_by_selection_text(lang: Language, n: usize) -> String {
    count_text(
        lang,
        n,
        "mr_hidden_by_selection_one",
        "mr_hidden_by_selection_many",
    )
}

/// Zustands-Chip eines Klons im Checkout-Menü.
pub fn clone_state_text(lang: Language, state: &CloneState) -> String {
    match state {
        CloneState::Free => tr(lang, "mr_state_free"),
        CloneState::InUse { iid: Some(iid) } => {
            tr_fmt(lang, "mr_state_in_use", &[&iid.to_string()])
        }
        CloneState::InUse { iid: None } => tr(lang, "mr_state_in_use_branch"),
        CloneState::Dirty { files } => {
            count_text(lang, *files, "mr_state_dirty_one", "mr_state_dirty_many")
        }
        CloneState::Blocked(BlockReason::CheckedOutElsewhere(path)) => {
            tr_fmt(lang, "mr_checked_out_tt", &[&path.display().to_string()])
        }
    }
}

/// „{Projekt} · {n} lokale Klone“
pub fn menu_header_text(lang: Language, project: &str, clones: usize) -> String {
    if clones == 1 {
        tr_fmt(lang, "mr_menu_header_clone_one", &[project])
    } else {
        tr_fmt(
            lang,
            "mr_menu_header_clones",
            &[project, &clones.to_string()],
        )
    }
}

/// „{Status} – Pipeline im Browser öffnen“, ohne sicheren Link nur der Status.
pub fn pipeline_tooltip(lang: Language, mr: &MergeRequest) -> String {
    let label = tr(lang, mr.pipeline.label_key());
    if pipeline_url(mr).is_some() {
        tr_fmt(lang, "mr_pipe_open_tt", &[&label])
    } else {
        label
    }
}

/// Kopf-Pill „Von anderen“ (E13).
pub fn header_count_text(lang: Language, count: HeaderCount) -> String {
    match count {
        HeaderCount::Total(total) => total.to_string(),
        HeaderCount::Of { visible, total } => tr_fmt(
            lang,
            "mr_count_of",
            &[&visible.to_string(), &total.to_string()],
        ),
    }
}

/// Tooltip „Alle offenen“, mit Zahlen erst nach dem Laden.
pub fn chip_all_tooltip(lang: Language, counts: &ChipCounts, projects: usize) -> String {
    match counts.uninvolved {
        Some(n) if projects == 1 => tr_fmt(lang, "mr_chip_all_tt_n_one", &[&n.to_string()]),
        Some(n) => tr_fmt(
            lang,
            "mr_chip_all_tt_n",
            &[&n.to_string(), &projects.to_string()],
        ),
        None => tr(lang, "mr_chip_all_tt"),
    }
}

/// Unterzeile „Nicht beteiligt“: offene MRs der {n} lokalen Projekte …
pub fn uninvolved_sub_text(lang: Language, projects: usize) -> String {
    count_text(lang, projects, "mr_uninvolved_sub_one", "mr_uninvolved_sub")
}

/// Fußzeile „Nicht beteiligt“: (angezeigt, gesamt, weitere). Ohne weitere
/// Seiten zählen nur geladene Zeilen als „weitere“: `total` (GitLab-`count`)
/// kann über dem Geladenen liegen (zwischendurch gemergt, Seite gescheitert).
pub fn uninvolved_counts(list: &UninvolvedList, limit: usize) -> (usize, usize, usize) {
    let shown = limit.min(list.mrs.len());
    let total = list.total.max(list.mrs.len());
    let more = if list.more.is_empty() {
        list.mrs.len() - shown
    } else {
        total - shown
    };
    (shown, total, more)
}

/// „{k} weitere laden“: neues Limit und ob eine Seite nachgeladen werden
/// muss (zuerst bereits geladene zeigen).
pub fn load_more_step(list: &UninvolvedList, limit: usize) -> (usize, bool) {
    let next = limit + UNINVOLVED_STEP;
    (next, list.mrs.len() < next && !list.more.is_empty())
}

/// Ordnerdialog-Ergebnis: ein leerer (oder neuer) Ordner ist selbst das
/// Ziel, sonst `{gewählt}/{name}`.
pub fn choose_target(picked: &Path, name: &str, picked_is_empty: bool) -> PathBuf {
    if picked_is_empty {
        picked.to_path_buf()
    } else {
        picked.join(name)
    }
}

/// Karte statt der Spalten (Schritt 7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewCard {
    GlabMissing,
    /// glab-Version
    GlabTooOld(String),
    NoRemotes,
    /// Remote-Hosts vorhanden, aber keiner bei glab angemeldet (erster Start)
    NoGitLabHost,
    AllDeselected,
    /// Noch kein Stand
    Loading,
    /// Kein Host liefert Daten, alle wegen der Anmeldung
    NotSignedIn(Vec<String>),
}

/// Welche Karte die Ansicht statt der Spalten zeigt; `None` = Spalten.
pub fn view_card(
    glab_error: Option<&GlabErrorKind>,
    discovering: bool,
    repos: &[RepoInfo],
    hosts: &HostMap,
    locals: &[LocalProject],
    snapshot: Option<&MrSnapshot>,
) -> Option<ViewCard> {
    match glab_error {
        Some(GlabErrorKind::NotInstalled) => return Some(ViewCard::GlabMissing),
        Some(GlabErrorKind::TooOld(version)) => {
            return Some(ViewCard::GlabTooOld(version.to_string()))
        }
        _ => {}
    }
    // Ein ausdrücklich eingetragener GitLab-Host liefert auch ohne Klon
    // eigene MRs. Erster Start: Hosts kennt erst die Erkennung nach dem Scan
    if hosts.gitlab_hosts.is_empty() {
        if discovering {
            return Some(ViewCard::Loading);
        }
        let any_remote = repos.iter().any(|r| !r.remotes.is_empty());
        return Some(if any_remote {
            ViewCard::NoGitLabHost
        } else {
            ViewCard::NoRemotes
        });
    }
    if !locals.is_empty() && locals.iter().all(|p| !p.included) {
        return Some(ViewCard::AllDeselected);
    }
    let Some(snap) = snapshot else {
        return Some(ViewCard::Loading);
    };
    // Nur wenn kein Host Daten liefert und alle wegen der Anmeldung: andere
    // Fehler (Netz, TLS, …) zeigen die Banner über den Spalten
    let signed_out = !snap.per_host.is_empty()
        && snap
            .per_host
            .iter()
            .all(|h| h.error.as_ref().is_some_and(is_auth_error));
    signed_out.then(|| {
        let hosts = snap.per_host.iter().map(|h| h.host_key.clone()).collect();
        ViewCard::NotSignedIn(hosts)
    })
}

/// Zeichnet die Ansicht. Klicks landen in `actions`, Chips und Zeilenzahl
/// „Nicht beteiligt“ in `state`.
pub fn show_merge_requests(
    ui: &mut Ui,
    input: &MrViewInput,
    state: &mut MrViewState,
    actions: &mut MrViewActions,
) {
    let p = mr_palette(input.theme, ui.visuals());
    let card = view_card(
        input.glab_error,
        input.discovering,
        input.repos,
        input.hosts,
        input.locals,
        input.snapshot,
    );
    ui.scope(|ui| {
        // Klicks gehören den Zeilen und Menüeinträgen, nicht der Textauswahl
        ui.style_mut().interaction.selectable_labels = false;
        if let Some(card) = card {
            show_card(ui, input, &p, &card, actions);
            return;
        }
        let Some(snap) = input.snapshot else {
            return;
        };
        let view = View {
            input,
            p,
            snap,
            open: OpenBranches::from_snapshot(snap),
            multi_host: snap.per_host.len() > 1,
            has_data: snap.per_host.is_empty() || snap.per_host.iter().any(|h| h.error.is_none()),
        };
        show_dashboard(ui, &view, state, actions);
    });
}

/// Gemeinsame Daten der Spalten und Zeilen eines Frames.
struct View<'v> {
    input: &'v MrViewInput<'v>,
    p: MrPalette,
    snap: &'v MrSnapshot,
    open: OpenBranches,
    /// Banner nennen den Host
    multi_host: bool,
    /// Mindestens ein Host lieferte Daten; sonst keine Leerzustände
    has_data: bool,
}

impl View<'_> {
    fn lang(&self) -> Language {
        self.input.lang
    }

    /// Checkout-Menü einer Zeile, nur beim Öffnen (`MenuCache`); je Frame
    /// reicht `checkout_brief`.
    fn menu(&self, mr: &MergeRequest, is_own: bool, lookup: &MenuLookup) -> CheckoutMenu {
        build_checkout_menu(
            &MrRefInfo::from_mr(mr),
            self.input.repos,
            self.input.hosts,
            self.input.cfg,
            is_own,
            lookup,
            &self.open,
        )
    }

    /// Einbezogene lokale Projekte (Unterzeile „Nicht beteiligt“, Tooltip)
    fn included_projects(&self) -> usize {
        self.input.locals.iter().filter(|p| p.included).count()
    }
}

// --- Layout ---------------------------------------------------------------

fn show_dashboard(ui: &mut Ui, v: &View, state: &mut MrViewState, actions: &mut MrViewActions) {
    host_banners(ui, v, actions);
    match layout_mode(ui.available_width()) {
        LayoutMode::Columns => {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = COLUMN_GAP;
                ui.columns(2, |cols| {
                    let [mine, others] = cols else {
                        return;
                    };
                    column_frame(mine, v, |ui| {
                        mine_header(ui, v);
                        ScrollArea::vertical()
                            .id_salt("mr_mine")
                            .auto_shrink([false, false])
                            .show(ui, |ui| mine_body(ui, v, actions));
                    });
                    column_frame(others, v, |ui| {
                        others_header(ui, v, state, actions);
                        ScrollArea::vertical()
                            .id_salt("mr_others")
                            .auto_shrink([false, false])
                            .show(ui, |ui| others_body(ui, v, state, actions));
                    });
                });
            });
        }
        // Eine äußere ScrollArea, Listen ohne eigene (kein verschachteltes
        // vertikales Scrollen)
        LayoutMode::Stacked => {
            ScrollArea::vertical()
                .id_salt("mr_stacked")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    column_frame(ui, v, |ui| {
                        mine_header(ui, v);
                        mine_body(ui, v, actions);
                    });
                    ui.add_space(COLUMN_GAP);
                    column_frame(ui, v, |ui| {
                        others_header(ui, v, state, actions);
                        others_body(ui, v, state, actions);
                    });
                });
        }
    }
}

/// Spaltenrahmen: Fläche `window_fill`, 1 px Linie, Radius 8.
fn column_frame(ui: &mut Ui, v: &View, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(v.p.column)
        .stroke(Stroke::new(1.0, v.p.line))
        .corner_radius(8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
}

/// Spaltenkopf: Titel und Zähler-Pill links, `right` rechtsbündig – oder
/// in einer zweiten Zeile, wenn `right_width` nicht mehr daneben passt.
fn column_header(
    ui: &mut Ui,
    v: &View,
    title: &str,
    count: &str,
    right_width: f32,
    right: impl FnOnce(&mut Ui, bool),
) {
    Frame::new()
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_min_height(24.0);
            let left_width = text_width(ui, title, FontId::proportional(13.0))
                + text_width(ui, count, FontId::proportional(11.0))
                + 30.0;
            let wide = ui.available_width() >= left_width + right_width + 12.0;
            let left = |ui: &mut Ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.label(RichText::new(title).size(13.0).color(v.p.title));
                count_pill(ui, v, count);
            };
            if wide {
                ui.horizontal(|ui| {
                    left(ui);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        right(ui, true);
                    });
                });
            } else {
                ui.horizontal(left);
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| right(ui, false));
            }
        });
}

fn mine_header(ui: &mut Ui, v: &View) {
    let lang = v.lang();
    let caption = tr(lang, "mr_sorted");
    let caption_width = text_width(ui, &caption, FontId::proportional(11.0)) + 26.0;
    let count = v.snap.mine.len().to_string();
    let tooltip = tr(lang, "mr_sorted_tt");
    column_header(
        ui,
        v,
        &tr(lang, "mr_mine_title"),
        &count,
        caption_width,
        |ui, rtl| {
            // Statische Beschriftung (E11), Regel im Tooltip
            let icon = |ui: &mut Ui| {
                ui.add(icon_image(ICON_SORT, 14.0, v.p.text2));
            };
            let resp = ui
                .scope(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if rtl {
                        icon(ui);
                        small(ui, &caption, v.p.text2);
                    } else {
                        small(ui, &caption, v.p.text2);
                        icon(ui);
                    }
                })
                .response;
            resp.on_hover_text(tooltip);
        },
    );
}

/// Filter-Chips im Kopf „Von anderen“ (E13), in Anzeige-Reihenfolge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderChip {
    Reviewer,
    Mention,
    Divider,
    All,
}

const HEADER_CHIPS: [HeaderChip; 4] = [
    HeaderChip::Reviewer,
    HeaderChip::Mention,
    HeaderChip::Divider,
    HeaderChip::All,
];

fn others_header(ui: &mut Ui, v: &View, state: &mut MrViewState, actions: &mut MrViewActions) {
    let lang = v.lang();
    let counts = chip_counts(v.snap);
    let count = header_count_text(lang, others_header_count(&v.snap.others, &state.chips()));
    let chip = |kind| -> (bool, String, Option<String>) {
        match kind {
            HeaderChip::Reviewer => (
                state.reviewer_on,
                tr(lang, "mr_chip_reviewer"),
                Some(counts.reviewer.to_string()),
            ),
            HeaderChip::Mention => (
                state.mention_on,
                tr(lang, "mr_chip_mention"),
                Some(counts.mention.to_string()),
            ),
            // „+N“ erst, wenn „Nicht beteiligt“ geladen ist
            HeaderChip::All => (
                state.show_uninvolved,
                tr(lang, "mr_chip_all"),
                counts.uninvolved.map(|n| format!("+{n}")),
            ),
            HeaderChip::Divider => (false, String::new(), None),
        }
    };
    let chips: Vec<(HeaderChip, bool, String, Option<String>)> = HEADER_CHIPS
        .iter()
        .map(|&kind| {
            let (on, label, count) = chip(kind);
            (kind, on, label, count)
        })
        .collect();
    let right_width: f32 = chips
        .iter()
        .map(|(kind, on, label, count)| match kind {
            HeaderChip::Divider => 1.0 + 6.0,
            _ => chip_width(ui, *on, label, count.as_deref()) + 6.0,
        })
        .sum();
    let projects = v.included_projects();
    column_header(
        ui,
        v,
        &tr(lang, "mr_others_title"),
        &count,
        right_width,
        |ui, rtl| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let ordered: Vec<_> = if rtl {
                chips.iter().rev().collect()
            } else {
                chips.iter().collect()
            };
            for (kind, on, label, count) in ordered {
                let (tooltip, toggle): (String, &mut bool) = match kind {
                    HeaderChip::Divider => {
                        let (rect, _) = ui.allocate_exact_size(vec2(1.0, 16.0), Sense::hover());
                        ui.painter().rect_filled(rect, 0.0, v.p.line);
                        continue;
                    }
                    HeaderChip::Reviewer => {
                        (tr(lang, "mr_chip_reviewer_tt"), &mut state.reviewer_on)
                    }
                    HeaderChip::Mention => (tr(lang, "mr_chip_mention_tt"), &mut state.mention_on),
                    HeaderChip::All => (
                        chip_all_tooltip(lang, &counts, projects),
                        &mut state.show_uninvolved,
                    ),
                };
                let resp = select_chip(ui, *on, label, count.as_deref()).on_hover_text(tooltip);
                if resp.clicked() {
                    *toggle = !*toggle;
                    // Ladeanzeige und Abruf nach dem Einschalten
                    if *kind == HeaderChip::All && *toggle && v.snap.uninvolved.is_none() {
                        actions.fetch_uninvolved = true;
                    }
                }
            }
        },
    );
}

fn mine_body(ui: &mut Ui, v: &View, actions: &mut MrViewActions) {
    let failed = partial_banners(ui, v, &[(MrPart::Dashboard, "mr_part_mine")]);
    for mr in &v.snap.mine {
        show_row(ui, v, mr, RowKind::Mine, actions);
    }
    if v.snap.mine.is_empty() && !failed && v.has_data {
        empty_block(ui, v, &tr(v.lang(), "mr_empty_mine"), None);
    }
}

fn others_body(ui: &mut Ui, v: &View, state: &mut MrViewState, actions: &mut MrViewActions) {
    let lang = v.lang();
    let chips = state.chips();
    let failed = partial_banners(
        ui,
        v,
        &[
            (MrPart::Dashboard, "mr_part_reviews"),
            (MrPart::Mentions, "mr_part_mentions"),
        ],
    );
    for mr in filter_others(&v.snap.others, &chips) {
        show_row(ui, v, mr, RowKind::Other, actions);
    }
    if !failed && v.has_data {
        match empty_state(&chips, &v.snap.others) {
            EmptyKind::NotEmpty => {}
            EmptyKind::NoData => empty_block(ui, v, &tr(lang, "mr_empty_others"), None),
            EmptyKind::NoFilter => empty_block(
                ui,
                v,
                &tr(lang, "mr_no_filter"),
                Some(&tr(lang, "mr_no_filter_hint")),
            ),
            EmptyKind::NoMatch => empty_block(ui, v, &tr(lang, "mr_filter_no_match"), None),
        }
    }
    if v.snap.hidden_by_selection > 0 {
        hidden_hint(ui, v, actions);
    }
    if state.show_uninvolved {
        uninvolved_section(ui, v, state, actions);
    }
}

/// Hinweiszeile „{n} MRs aus abgewählten Repositories ausgeblendet“ (E1);
/// Klick öffnet die Einstellungen.
fn hidden_hint(ui: &mut Ui, v: &View, actions: &mut MrViewActions) {
    let lang = v.lang();
    divider(ui, v);
    Frame::new()
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            let text = hidden_by_selection_text(lang, v.snap.hidden_by_selection);
            if link_button(ui, &text, 11.0, v.p.text2)
                .on_hover_text(tr(lang, "mr_open_settings"))
                .clicked()
            {
                actions.open_settings_mr_tab = true;
            }
        });
}

/// Abschnitt „Nicht beteiligt · {n}“ unter „Von anderen“.
fn uninvolved_section(ui: &mut Ui, v: &View, state: &mut MrViewState, actions: &mut MrViewActions) {
    let lang = v.lang();
    let p = &v.p;
    let list = v.snap.uninvolved.as_ref();
    let total = list.map_or_else(
        || "…".to_string(),
        |l| uninvolved_counts(l, state.uninvolved_shown).1.to_string(),
    );
    divider(ui, v);
    Frame::new()
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 14,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 2.0);
                ui.label(
                    RichText::new(tr_fmt(lang, "mr_uninvolved_h", &[&total]))
                        .size(12.0)
                        .color(p.title_quiet),
                );
                let sub = uninvolved_sub_text(lang, v.included_projects());
                small(ui, &sub, p.text2);
            });
        });
    let failed = partial_banners(ui, v, &[(MrPart::Uninvolved, "mr_part_uninvolved")]);
    let Some(list) = list else {
        // Spinner nur, solange wirklich geladen wird; sonst (Host-Fehler,
        // Abruf ohne „Nicht beteiligt“) ein Link zum Nachladen
        if v.input.uninvolved_loading {
            loading_line(ui, v, &tr(lang, "mr_loading_uninvolved"));
        } else if !failed {
            not_loaded_line(ui, v, actions);
        }
        return;
    };
    let (shown, total, more) = uninvolved_counts(list, state.uninvolved_shown);
    for mr in list.mrs.iter().take(shown) {
        show_row(ui, v, mr, RowKind::Uninvolved, actions);
    }
    divider(ui, v);
    Frame::new()
        .inner_margin(Margin {
            left: 12,
            right: 12,
            top: 8,
            bottom: 12,
        })
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if more > 0 {
                    if v.input.uninvolved_loading {
                        ui.add(Spinner::new().size(14.0).color(p.run_icon));
                    } else {
                        let label = tr_fmt(
                            lang,
                            "mr_load_more",
                            &[&more.min(UNINVOLVED_STEP).to_string()],
                        );
                        let button = Button::new(RichText::new(label).size(12.0))
                            .corner_radius(6)
                            .min_size(vec2(0.0, 22.0));
                        if ui.add(button).clicked() {
                            let (limit, fetch) = load_more_step(list, state.uninvolved_shown);
                            state.uninvolved_shown = limit;
                            actions.load_more |= fetch;
                        }
                    }
                }
                let text = tr_fmt(
                    lang,
                    "mr_shown_of",
                    &[&shown.to_string(), &total.to_string()],
                );
                small(ui, &text, p.text2);
            });
        });
}

// --- Zeilen ---------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowKind {
    Mine,
    Other,
    /// Ruhiger: Titel in Text-Farbe, zwei Zeilen
    Uninvolved,
}

fn show_row(ui: &mut Ui, v: &View, mr: &MergeRequest, kind: RowKind, actions: &mut MrViewActions) {
    let brief = checkout_brief(&MrRefInfo::from_mr(mr), v.input.repos, v.input.hosts);
    let button = checkout_button(&brief);
    ui.push_id((&mr.host_key, &mr.id), |ui| {
        divider(ui, v);
        let margin = if kind == RowKind::Uninvolved {
            Margin::symmetric(12, 8)
        } else {
            Margin {
                left: 12,
                right: 12,
                top: 9,
                bottom: 10,
            }
        };
        Frame::new().inner_margin(margin).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = if kind == RowKind::Uninvolved {
                4.0
            } else {
                5.0
            };
            let compact = ui.available_width() < COMPACT_WIDTH;
            let row = Row {
                v,
                mr,
                kind,
                button: &button,
                compact,
            };
            row.title_line(ui, actions);
            match kind {
                RowKind::Mine => {
                    row.mine_line2(ui);
                    row.line3(ui, actions, |ui, actions| {
                        row.approval_bar(ui);
                        row.pipeline(ui, actions, if compact { 0.0 } else { 130.0 });
                        row.threads(ui);
                    });
                }
                RowKind::Other => {
                    row.other_line2(ui);
                    row.line3(ui, actions, |ui, actions| {
                        if let Some(review) = mr.my_review {
                            row.my_review(ui, review);
                        }
                        let appr = approvals(mr, v.input.cfg.ignore_bot_approvals);
                        let tooltip = appr.tooltip(v.lang());
                        row.pips(ui, &appr, tooltip);
                        row.pipeline(ui, actions, if compact { 0.0 } else { 130.0 });
                        row.threads(ui);
                    });
                }
                RowKind::Uninvolved => {
                    row.line3(ui, actions, |ui, actions| {
                        let width = text_width(ui, &mr.project_path, FontId::proportional(11.0));
                        unit(ui, width, 22.0, |ui| {
                            small(ui, &mr.project_path, v.p.text2);
                        });
                        let appr = approvals(mr, v.input.cfg.ignore_bot_approvals);
                        let tooltip = appr.label(v.lang());
                        row.pips(ui, &appr, tooltip);
                        row.pipeline(ui, actions, 0.0);
                        row.threads(ui);
                    });
                }
            }
        });
    });
}

/// Eine MR-Zeile beim Zeichnen.
struct Row<'r> {
    v: &'r View<'r>,
    mr: &'r MergeRequest,
    kind: RowKind,
    button: &'r ButtonKind,
    /// Spalte schmaler als 460 px: Mindestbreiten entfallen
    compact: bool,
}

impl Row<'_> {
    fn lang(&self) -> Language {
        self.v.lang()
    }

    /// Zeile 1: Titel (klickbar, E12), `!iid`, Chips Entwurf / mergebereit
    /// (nur Meine) bzw. Autor (Nicht beteiligt).
    fn title_line(&self, ui: &mut Ui, actions: &mut MrViewActions) {
        let (lang, p, mr) = (self.lang(), &self.v.p, self.mr);
        let iid = format!("!{}", mr.iid);
        let draft = mr.draft.then(|| tr(lang, "mr_draft"));
        let ready_label = (self.kind == RowKind::Mine && ready(mr)).then(|| tr(lang, "mr_ready"));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            // Rest der Zeile zuerst messen: der Titel kürzt, iid und Chips bleiben
            let mut suffix = text_width(ui, &iid, FontId::monospace(11.0)) + 8.0;
            if let Some(d) = &draft {
                suffix += pill_width(ui, d, false) + 8.0;
            }
            if let Some(r) = &ready_label {
                suffix += pill_width(ui, r, true) + 8.0;
            }
            if self.kind == RowKind::Uninvolved {
                suffix += 26.0 + text_width(ui, &mr.author.name, FontId::proportional(11.0)) + 8.0;
            }
            let title_color = if self.kind == RowKind::Uninvolved {
                p.title_quiet
            } else {
                p.title
            };
            let width = (ui.available_width() - suffix).max(60.0);
            let resp = ui
                .allocate_ui(vec2(width, 18.0), |ui| {
                    let text = RichText::new(&mr.title).size(13.0).color(title_color);
                    ui.add(Button::new(text).frame(false).truncate())
                })
                .inner
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(tr(lang, "mr_open_title_tt"));
            if resp.clicked() {
                if let Some(url) = mr_url(mr) {
                    actions.open_url = Some(url.to_string());
                }
            }
            ui.label(
                RichText::new(&iid)
                    .font(FontId::monospace(11.0))
                    .color(p.text2),
            );
            if let Some(d) = &draft {
                pill(ui, d, PillStyle::outline(p.text2), None);
            }
            if let Some(r) = &ready_label {
                pill(
                    ui,
                    r,
                    PillStyle::outline(p.ok_text),
                    Some((ICON_CHECK, p.ok_icon)),
                )
                .on_hover_text(tr(lang, "mr_ready_tt"));
            }
            if self.kind == RowKind::Uninvolved {
                author(ui, self.v, &mr.author.name, p.text2);
            }
        });
    }

    /// Zeile 2 (Meine): Projekt · Branches · ausgecheckt in …
    fn mine_line2(&self, ui: &mut Ui) {
        let (p, mr) = (&self.v.p, self.mr);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 2.0);
            small(ui, &mr.project_path, p.text2);
            small(ui, "·", p.text2);
            let branches = format!("{} → {}", mr.source_branch, mr.target_branch);
            let width = 16.0 + text_width(ui, &branches, FontId::monospace(11.0));
            unit(ui, width, 16.0, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.add(icon_image(ICON_BRANCH, 12.0, p.text2));
                mono(ui, &branches, 11.0, p.text2);
            });
            self.checked_out_hint(ui);
        });
    }

    /// Zeile 2 (Von anderen): Initialen + Autor · Projekt · Grund-Chips
    fn other_line2(&self, ui: &mut Ui) {
        let (lang, p, mr) = (self.lang(), &self.v.p, self.mr);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 2.0);
            author(ui, self.v, &mr.author.name, p.title_quiet);
            small(ui, "·", p.text2);
            small(ui, &mr.project_path, p.text2);
            let prefix = tr(lang, "mr_reasons_prefix");
            for reason in &mr.reasons {
                let (key, style, icon) = match reason {
                    Reason::Reviewer => (
                        "mr_reason_reviewer",
                        PillStyle::filled(p.reviewer_bg, p.reviewer_text),
                        Some((ICON_USER, p.reviewer_text)),
                    ),
                    Reason::MentionDescription => (
                        "mr_reason_desc",
                        PillStyle::filled(p.pill_bg, p.pill_text),
                        None,
                    ),
                    Reason::MentionComment => (
                        "mr_reason_comment",
                        PillStyle::filled(p.pill_bg, p.pill_text),
                        None,
                    ),
                };
                let label = tr(lang, key);
                pill(ui, &label, style, icon).on_hover_text(format!("{prefix} {label}"));
            }
            self.checked_out_hint(ui);
        });
    }

    /// „· 📁 {Ordner}“, wenn der Branch schon ausgecheckt ist (K1).
    fn checked_out_hint(&self, ui: &mut Ui) {
        let ButtonKind::AlreadyCheckedOut(path) = self.button else {
            return;
        };
        let p = &self.v.p;
        small(ui, "·", p.text2);
        let name = folder_name(path);
        let width = 16.0 + text_width(ui, &name, FontId::monospace(11.0));
        unit(ui, width, 16.0, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add(icon_image(ICON_FOLDER, 12.0, p.text2));
            mono(ui, &name, 11.0, p.title_quiet);
        })
        .on_hover_text(checkout_tooltip(self.lang(), self.button));
    }

    /// Zeile 3: links die Gruppe (bricht um), rechts Zeit und Aktionen in
    /// der ersten Zeile.
    fn line3(
        &self,
        ui: &mut Ui,
        actions: &mut MrViewActions,
        left: impl FnOnce(&mut Ui, &mut MrViewActions),
    ) {
        let (lang, p, mr) = (self.lang(), &self.v.p, self.mr);
        ui.horizontal(|ui| {
            ui.set_min_height(22.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let url = mr_url(mr);
                let browser = icon_button(
                    ui,
                    ICON_OPEN_EXTERNAL,
                    &tr(lang, "mr_open_browser"),
                    false,
                    url.is_some(),
                );
                if browser.clicked() {
                    actions.open_url = url.map(str::to_string);
                }
                self.checkout_control(ui, actions);
                ui.add_space(4.0);
                let updated = mr
                    .updated_at
                    .map(|t| relative_time(t, self.v.input.now, lang))
                    .unwrap_or_default();
                small(ui, &updated, p.text2);
                ui.add_space(8.0);
                ui.with_layout(
                    Layout::left_to_right(Align::Center).with_main_wrap(true),
                    |ui| {
                        ui.spacing_mut().item_spacing = vec2(12.0, 4.0);
                        left(ui, actions);
                    },
                );
            });
        });
    }

    /// Approval-Balken mit dem Painter (`ProgressBar` zeichnet bei 0 einen Rest).
    fn approval_bar(&self, ui: &mut Ui) {
        let (lang, p) = (self.lang(), &self.v.p);
        let appr = approvals(self.mr, self.v.input.cfg.ignore_bot_approvals);
        let label = appr.label(lang);
        let label_width = text_width(ui, &label, FontId::proportional(11.0)).max(self.min(92.0));
        unit(ui, 126.0 + label_width, 22.0, |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let (rect, _) = ui.allocate_exact_size(vec2(120.0, 6.0), Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, 3.0, p.track);
            let fill = if appr.required > 0 && appr.given >= appr.required {
                Some((1.0, p.bar_done))
            } else if appr.given > 0 && appr.given < appr.required {
                Some((appr.given as f32 / appr.required as f32, p.bar_partial))
            } else {
                None
            };
            if let Some((fraction, color)) = fill {
                let width = (rect.width() * fraction).max(rect.height());
                let bar = egui::Rect::from_min_size(rect.min, vec2(width, rect.height()));
                painter.rect_filled(bar, 3.0, color);
            }
            small(ui, &label, p.title_quiet);
        })
        .on_hover_text(appr.tooltip(lang));
    }

    /// Pips je erforderlichem Approval + „1/2“.
    fn pips(&self, ui: &mut Ui, appr: &Approvals, tooltip: String) {
        let p = &self.v.p;
        let short = appr.short(self.lang());
        let n = appr.required.min(MAX_PIPS);
        let pips_width = if n > 0 {
            n as f32 * 8.0 + (n - 1) as f32 * 3.0
        } else {
            0.0
        };
        let short_width = text_width(ui, &short, FontId::proportional(11.0));
        let width = pips_width + 4.0 + short_width.max(self.min(44.0));
        unit(ui, width, 22.0, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if n > 0 {
                let (rect, _) = ui.allocate_exact_size(vec2(pips_width, 8.0), Sense::hover());
                let painter = ui.painter();
                for i in 0..n {
                    let center = rect.left_center() + vec2(4.0 + i as f32 * 11.0, 0.0);
                    if i < appr.given {
                        painter.circle_filled(center, 4.0, p.ok_icon);
                    } else {
                        painter.circle_stroke(center, 3.25, Stroke::new(1.5, p.text2));
                    }
                }
            }
            small(ui, &short, p.title_quiet);
        })
        .on_hover_text(tooltip);
    }

    /// Eigener Review-Status (12 px, farbig).
    fn my_review(&self, ui: &mut Ui, review: MyReview) {
        let p = &self.v.p;
        let (icon, icon_color, text_color) = match review {
            MyReview::Pending => (ICON_CLOCK, p.warn_icon, p.warn_text),
            MyReview::Approved => (ICON_CHECK, p.ok_icon, p.ok_text),
            MyReview::ChangesRequested => (ICON_ARROW_U, p.fail_icon, p.fail_text),
            MyReview::Commented => (ICON_CHAT, p.neutral_icon, p.text2),
        };
        let label = tr(self.lang(), review.label_key());
        let width =
            (19.0 + text_width(ui, &label, FontId::proportional(12.0))).max(self.min(132.0));
        unit(ui, width, 22.0, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.add(icon_image(icon, 14.0, icon_color));
            ui.label(RichText::new(label).size(12.0).color(text_color));
        });
    }

    /// Klickbarer Pipeline-Status (E7): Hand-Cursor, unterstrichen bei Hover.
    /// „Keine Pipeline“ bzw. ohne sicheren Link nur Text.
    fn pipeline(&self, ui: &mut Ui, actions: &mut MrViewActions, min_width: f32) {
        let (lang, p, mr) = (self.lang(), &self.v.p, self.mr);
        let (icon, color) = match mr.pipeline {
            PipelineState::Success => (ICON_CHECK_CIRCLE, p.ok_icon),
            PipelineState::Failed => (ICON_X_CIRCLE, p.fail_icon),
            PipelineState::Running => (ICON_CIRCLE_NOTCH, p.run_icon),
            PipelineState::Pending => (ICON_CLOCK, p.run_icon),
            PipelineState::Canceled => (ICON_PROHIBIT, p.neutral_icon),
            PipelineState::Skipped => (ICON_SKIP_CIRCLE, p.neutral_icon),
            PipelineState::Manual => (ICON_PLAY_CIRCLE, p.neutral_icon),
            PipelineState::NoPipeline => (ICON_MINUS_CIRCLE, p.neutral_icon),
        };
        let label = tr(lang, mr.pipeline.label_key());
        let tooltip = pipeline_tooltip(lang, mr);
        let width = (19.0 + text_width(ui, &label, FontId::proportional(11.0))).max(min_width);
        let Some(url) = pipeline_url(mr) else {
            unit(ui, width, 22.0, |ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.add(icon_image(icon, 14.0, color));
                small(ui, &label, p.title_quiet);
            })
            .on_hover_text(tooltip);
            return;
        };
        unit(ui, width, 22.0, |ui| {
            // Hover aus dem letzten Frame (wie `Button` selbst): unterstreichen
            let hovered = ui
                .ctx()
                .read_response(ui.next_auto_id())
                .is_some_and(|r| r.hovered());
            let mut text = RichText::new(label).size(11.0).color(p.title_quiet);
            if hovered {
                text = text.underline();
            }
            let button = Button::new((icon_image(icon, 14.0, color), text))
                .frame(false)
                .gap(5.0);
            let resp = ui
                .add(button)
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text(tooltip);
            if resp.clicked() {
                actions.open_url = Some(url);
            }
        });
    }

    fn threads(&self, ui: &mut Ui) {
        let (lang, p, mr) = (self.lang(), &self.v.p, self.mr);
        let (icon, color) = if mr.threads_open > 0 {
            (ICON_CHAT_DOTS, p.warn_icon)
        } else if mr.threads_resolved > 0 {
            (ICON_CHAT_CHECK, p.neutral_icon)
        } else {
            (ICON_CHAT, p.neutral_icon)
        };
        let label = mr.threads_label(lang);
        let width = 19.0 + text_width(ui, &label, FontId::proportional(11.0));
        unit(ui, width, 22.0, |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            ui.add(icon_image(icon, 14.0, color));
            small(ui, &label, p.title_quiet);
        })
        .on_hover_text(mr.threads_tooltip(lang));
    }

    /// Mindestbreite; entfällt in schmalen Spalten.
    fn min(&self, width: f32) -> f32 {
        if self.compact {
            0.0
        } else {
            width
        }
    }

    /// Checkout-Button mit Menü K1, Ordner-Pfeil, „nicht lokal“ oder
    /// deaktiviert (E6).
    fn checkout_control(&self, ui: &mut Ui, actions: &mut MrViewActions) {
        let (lang, p) = (self.lang(), &self.v.p);
        let tooltip = checkout_tooltip(lang, self.button);
        // Popup-ID wie `Popup::default_response_id` des nächsten Buttons
        let popup_id = ui.next_auto_id().with("popup");
        let open = Popup::is_id_open(ui.ctx(), popup_id);
        match self.button {
            ButtonKind::Disabled(_) => {
                icon_button(ui, ICON_BRANCH, &tooltip, false, false)
                    .on_disabled_hover_text(tooltip);
            }
            ButtonKind::AlreadyCheckedOut(path) => {
                let resp = icon_button(ui, ICON_FOLDER_ARROW, &tooltip, open, true);
                Popup::from_toggle_button_response(&resp)
                    .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
                    .align(RectAlign::BOTTOM_END)
                    .width(ALREADY_MENU_WIDTH)
                    .show(|ui| already_menu_ui(ui, self.v, path, actions));
            }
            ButtonKind::Menu | ButtonKind::NotLocal => {
                let resp = if *self.button == ButtonKind::NotLocal {
                    not_local_button(ui, p, &tr(lang, "mr_not_local"), &tooltip, open)
                } else {
                    icon_button(ui, ICON_BRANCH, &tooltip, open, true)
                };
                // Menü beim Öffnen einmal mit Dateisystem-Prüfung bauen
                if resp.clicked() && !open {
                    let cache = MenuCache::new(self.v, self.mr, self.kind == RowKind::Mine);
                    ui.ctx().data_mut(|d| d.insert_temp(popup_id, cache));
                }
                Popup::from_toggle_button_response(&resp)
                    .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
                    .align(RectAlign::BOTTOM_END)
                    .width(MENU_WIDTH)
                    .show(|ui| {
                        checkout_menu_ui(
                            ui,
                            self.v,
                            self.mr,
                            self.kind == RowKind::Mine,
                            popup_id,
                            actions,
                        )
                    });
            }
        }
    }
}

// --- Checkout-Menü (K1) ---------------------------------------------------

/// Menüinhalt und „Danach öffnen“ für die Dauer einer Menüöffnung.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MenuCache {
    menu: CheckoutMenu,
    after: AfterAction,
}

impl MenuCache {
    fn new(v: &View, mr: &MergeRequest, is_own: bool) -> Self {
        let input = v.input;
        let menu = v.menu(mr, is_own, &input.lookup);
        // Vorbelegung über die Chips auflösen (gelöschter Agent, fehlende IDE)
        let after = after_chips(
            input.lang,
            input.agents,
            input.ide,
            after_preselection(input.cfg),
        )
        .into_iter()
        .find(|c| c.selected)
        .map_or(AfterAction::Nothing, |c| c.action);
        Self { menu, after }
    }
}

/// Checkout-Menü K1 (Mockup `anhang/mockup-k1.dc.html`).
fn checkout_menu_ui(
    ui: &mut Ui,
    v: &View,
    mr: &MergeRequest,
    is_own: bool,
    cache_id: Id,
    actions: &mut MrViewActions,
) {
    let (lang, p) = (v.lang(), &v.p);
    let mut cache = ui
        .ctx()
        .data(|d| d.get_temp::<MenuCache>(cache_id))
        .unwrap_or_else(|| MenuCache::new(v, mr, is_own));
    let width = MENU_WIDTH
        .min(ui.ctx().content_rect().width() - 80.0)
        .max(240.0);
    ui.set_width(width);
    ui.style_mut().interaction.selectable_labels = false;
    ui.spacing_mut().item_spacing.y = 6.0;
    let mut pick: Option<MrCheckoutKind> = None;

    // Kopf: „!{iid} · {Branch}“ und „{Projekt} · {n} lokale Klone“
    Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 2,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                mono(ui, &format!("!{}", mr.iid), 12.0, p.title);
                small(ui, "·", p.text2);
                ui.add(icon_image(ICON_BRANCH, 12.0, p.text2));
                ui.add(
                    Label::new(
                        RichText::new(&mr.source_branch)
                            .font(FontId::monospace(12.0))
                            .color(p.title_quiet),
                    )
                    .truncate(),
                );
            });
            let header = menu_header_text(lang, &mr.project_path, cache.menu.clones.len());
            small(ui, &header, p.text2);
        });
    ui.separator();

    // In neuem Ordner
    section_label(ui, v, &tr(lang, "mr_menu_new_folder"));
    if let Some(wt) = &cache.menu.new_worktree {
        let key = if wt.in_review_root {
            "mr_menu_worktree_review"
        } else {
            "mr_menu_worktree"
        };
        let title = tr(lang, key);
        let path = wt.path.display().to_string();
        let sub = tr_fmt(
            lang,
            "mr_menu_worktree_sub",
            &[&path, &folder_name(&wt.base)],
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let width = ui.available_width() - 26.0;
            let resp = ui
                .allocate_ui(vec2(width, 0.0), |ui| {
                    menu_entry(ui, "worktree", &title, true, |ui| {
                        entry_lines(ui, v, ICON_FOLDER_PLUS, &title, Some(&sub));
                    })
                })
                .inner;
            if resp.clicked() {
                pick = Some(MrCheckoutKind::NewWorktree {
                    path: wt.path.clone(),
                    base: wt.base.clone(),
                });
            }
            let other = tr(lang, "mr_menu_other_folder");
            if icon_button(ui, ICON_DOTS, &other, false, true).clicked() {
                if let Some(path) = pick_folder(wt.path.parent(), &folder_name(&wt.path)) {
                    pick = Some(MrCheckoutKind::NewWorktree {
                        path,
                        base: wt.base.clone(),
                    });
                }
            }
        });
    }
    if let Some(url) = &cache.menu.clone_url {
        let title = tr(lang, "mr_menu_clone");
        let sub = tr(lang, "mr_menu_clone_sub");
        let resp = menu_entry(ui, "clone", &title, true, |ui| {
            entry_lines(ui, v, ICON_DOWNLOAD, &title, Some(&sub));
        });
        if resp.clicked() {
            let (start, name) = clone_target_hint(v, mr, &cache.menu);
            if let Some(path) = pick_folder(start.as_deref(), &name) {
                pick = Some(MrCheckoutKind::NewClone {
                    path,
                    url: url.clone(),
                });
            }
        }
    }

    // In bestehendem Klon wechseln
    if !cache.menu.clones.is_empty() {
        ui.separator();
        section_label(ui, v, &tr(lang, "mr_menu_switch"));
        for (i, clone) in cache.menu.clones.iter().enumerate() {
            if clone_entry(ui, v, i, clone).clicked() {
                pick = Some(MrCheckoutKind::Switch {
                    clone: clone.path.clone(),
                });
            }
        }
    }

    // Danach öffnen
    ui.separator();
    after_section(ui, v, &mut cache.after);

    if let Some(kind) = pick {
        actions.checkout = Some(MrCheckoutChoice {
            host_key: mr.host_key.clone(),
            mr_id: mr.id.clone(),
            iid: mr.iid,
            kind,
            after: cache.after.clone(),
        });
        ui.ctx().data_mut(|d| d.remove::<MenuCache>(cache_id));
        ui.close();
    } else {
        ui.ctx().data_mut(|d| d.insert_temp(cache_id, cache));
    }
}

/// Startordner und Ordnername für „Neuer Klon …“: wie der Worktree-Vorschlag,
/// sonst Review-Ordner und `{repo}-mr-{iid}`.
fn clone_target_hint(
    v: &View,
    mr: &MergeRequest,
    menu: &CheckoutMenu,
) -> (Option<PathBuf>, String) {
    if let Some(wt) = &menu.new_worktree {
        return (
            wt.path.parent().map(Path::to_path_buf),
            folder_name(&wt.path),
        );
    }
    let repo = mr.project_path.rsplit('/').next().unwrap_or("repo");
    let start = v.input.cfg.review_root.as_ref().map(PathBuf::from);
    (start, format!("{repo}-mr-{}", mr.iid))
}

/// Ordnerdialog (blockiert wie die Ordnerwahl in den Einstellungen).
fn pick_folder(start: Option<&Path>, name: &str) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    let picked = dialog.pick_folder()?;
    let empty = std::fs::read_dir(&picked).map_or(true, |mut e| e.next().is_none());
    Some(choose_target(&picked, name, empty))
}

/// Klon unter „In bestehendem Klon wechseln“: Ordner + Zustands-Chip,
/// Branch · zuletzt genutzt, ggf. Hinweis auf Stash/Verwerfen.
fn clone_entry(ui: &mut Ui, v: &View, index: usize, clone: &CloneOption) -> Response {
    let (lang, p) = (v.lang(), &v.p);
    let state = clone_state_text(lang, &clone.state);
    let blocked = matches!(clone.state, CloneState::Blocked(_));
    let path = clone.path.display().to_string();
    let a11y = format!("{path} – {} ({state})", clone.branch);
    let resp = menu_entry(ui, ("clone_entry", index), &a11y, !blocked, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add(icon_image(ICON_FOLDER, 16.0, p.text2));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.horizontal(|ui| {
                    let style = match clone.state {
                        CloneState::Free => PillStyle::outline(p.ok_text),
                        CloneState::InUse { .. } => PillStyle::filled(p.pill_bg, p.pill_text),
                        CloneState::Dirty { .. } => PillStyle::outline(p.warn_text),
                        CloneState::Blocked(_) => PillStyle::outline(p.text2),
                    };
                    let chip_w = pill_width(ui, &state, false);
                    let path_w = (ui.available_width() - chip_w - 8.0).max(40.0);
                    let used = ui
                        .allocate_ui(vec2(path_w, 18.0), |ui| {
                            ui.add(
                                Label::new(
                                    RichText::new(&path)
                                        .font(FontId::monospace(12.0))
                                        .color(p.title),
                                )
                                .truncate(),
                            )
                        })
                        .response
                        .rect
                        .width();
                    ui.add_space((path_w - used).max(0.0));
                    pill(ui, &state, style, None);
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
                    let width = 16.0 + text_width(ui, &clone.branch, FontId::monospace(11.0));
                    unit(ui, width, 16.0, |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.add(icon_image(ICON_BRANCH, 12.0, p.text2));
                        mono(ui, &clone.branch, 11.0, p.title_quiet);
                    });
                    if let Some(ts) = clone.last_used {
                        small(ui, "·", p.text2);
                        let ts = i64::try_from(ts).unwrap_or(i64::MAX);
                        let when = relative_time(ts, v.input.now, lang);
                        small(ui, &tr_fmt(lang, "mr_last_used", &[&when]), p.text2);
                    }
                });
                if matches!(clone.state, CloneState::Dirty { .. }) {
                    small(ui, &tr(lang, "mr_state_dirty_hint"), p.text2);
                }
            });
        });
    });
    if blocked {
        resp.on_disabled_hover_text(state)
    } else {
        resp
    }
}

/// „Danach öffnen“: Einfachauswahl über Chips, Vorbelegung aus dem Cache.
fn after_section(ui: &mut Ui, v: &View, after: &mut AfterAction) {
    let (lang, p) = (v.lang(), &v.p);
    Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 0,
            bottom: 2,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.horizontal(|ui| {
                small(ui, &tr(lang, "mr_menu_after"), p.text2);
                if v.input.cfg.after_checkout_default == AfterCheckoutPref::LastUsed {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        small(ui, &tr(lang, "mr_after_remembered"), p.text2);
                    });
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                for chip in after_chips(lang, v.input.agents, v.input.ide, after) {
                    if select_chip(ui, chip.selected, &chip.label, None).clicked() {
                        *after = chip.action;
                    }
                }
            });
        });
}

/// Menü „Bereits ausgecheckt in {Pfad}“: Repo-Liste, Agenten, Shell,
/// Explorer, IDE (→ `open_existing`).
fn already_menu_ui(ui: &mut Ui, v: &View, path: &Path, actions: &mut MrViewActions) {
    let (lang, p) = (v.lang(), &v.p);
    ui.set_width(
        ALREADY_MENU_WIDTH
            .min(ui.ctx().content_rect().width() - 80.0)
            .max(200.0),
    );
    ui.style_mut().interaction.selectable_labels = false;
    ui.spacing_mut().item_spacing.y = 2.0;
    Frame::new()
        .inner_margin(Margin {
            left: 8,
            right: 8,
            top: 2,
            bottom: 4,
        })
        .show(ui, |ui| {
            let header = tr_fmt(lang, "mr_already_header", &[&path.display().to_string()]);
            ui.label(RichText::new(header).size(12.0).color(p.title_quiet));
        });
    ui.separator();
    let mut entries: Vec<(ImageSource<'static>, String, OpenTarget)> = vec![(
        ICON_FOLDER_ARROW,
        tr(lang, "mr_open_show_in_list"),
        OpenTarget::ShowInRepoList,
    )];
    for agent in v.input.agents {
        entries.push((
            ICON_SPARKLE,
            tr_fmt(lang, "mr_open_agent", &[&agent.display_name]),
            OpenTarget::Agent(agent.id.clone()),
        ));
    }
    entries.push((ICON_TERMINAL, tr(lang, "mr_open_shell"), OpenTarget::Shell));
    entries.push((
        ICON_FOLDER,
        tr(lang, "mr_open_explorer"),
        OpenTarget::Explorer,
    ));
    if let Some(ide) = v.input.ide {
        entries.push((
            ICON_CODE,
            tr_fmt(lang, "mr_open_ide", &[&ide.display_name]),
            OpenTarget::Ide,
        ));
    }
    for (i, (icon, label, target)) in entries.into_iter().enumerate() {
        let resp = menu_entry(ui, ("open", i), &label, true, |ui| {
            entry_lines(ui, v, icon, &label, None);
        });
        if resp.clicked() {
            actions.open_existing = Some((path.to_path_buf(), target));
            ui.close();
        }
    }
}

/// Menüeintrag als echter Button (Tab, Enter/Leertaste, AccessKit-Name);
/// Fläche nur bei Hover oder Fokus.
fn menu_entry(
    ui: &mut Ui,
    id_salt: impl Hash + std::fmt::Debug,
    a11y: &str,
    enabled: bool,
    content: impl FnOnce(&mut Ui),
) -> Response {
    let resp = ui
        .add_enabled_ui(enabled, |ui| {
            ui.scope_builder(
                UiBuilder::new().id_salt(id_salt).sense(Sense::click()),
                |ui| {
                    let last = ui.response();
                    let fill = if last.hovered() || last.has_focus() {
                        ui.style().interact(&last).weak_bg_fill
                    } else {
                        Color32::TRANSPARENT
                    };
                    Frame::new()
                        .fill(fill)
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            content(ui);
                        });
                },
            )
            .response
        })
        .inner;
    if resp.hovered() && enabled {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, a11y));
    resp
}

/// Icon + Titel (12 px) + optionale Unterzeile (11 px).
fn entry_lines(ui: &mut Ui, v: &View, icon: ImageSource<'static>, title: &str, sub: Option<&str>) {
    let p = &v.p;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.add(icon_image(icon, 16.0, p.text2));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.add(Label::new(RichText::new(title).size(12.0).color(p.title)).wrap());
            if let Some(sub) = sub {
                ui.add(Label::new(RichText::new(sub).size(11.0).color(p.text2)).wrap());
            }
        });
    });
}

fn section_label(ui: &mut Ui, v: &View, text: &str) {
    Frame::new()
        .inner_margin(Margin::symmetric(8, 0))
        .show(ui, |ui| small(ui, text, v.p.text2));
}

// --- Banner und Karten ----------------------------------------------------

/// Host-Fehler oben über den Spalten: „Nicht bei {host} angemeldet –
/// [Token hinzufügen]“ bzw. Fehlerbanner mit Details (E20: nur hier).
fn host_banners(ui: &mut Ui, v: &View, actions: &mut MrViewActions) {
    let (lang, p) = (v.lang(), &v.p);
    for host in &v.snap.per_host {
        let Some(kind) = &host.error else {
            continue;
        };
        let key = &host.host_key;
        if is_auth_error(kind) {
            Frame::new()
                .fill(p.notice_bg)
                .stroke(Stroke::new(1.0, p.notice_stroke))
                .corner_radius(6)
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.add(icon_image(ICON_LOCK, 14.0, p.notice_text));
                        let text = tr_fmt(lang, "mr_not_logged_in", &[key]);
                        ui.label(RichText::new(text).size(12.0).color(p.notice_text));
                        if ui.button(tr(lang, "mr_add_token")).clicked() {
                            actions.open_settings_mr_tab = true;
                        }
                    });
                });
        } else {
            let text = error_text(kind, key, lang)
                .unwrap_or_else(|| tr_fmt(lang, "mr_host_failed", &[key]));
            error_banner(
                ui,
                v,
                Id::new(("mr_host_error", key)),
                &text,
                &error_details(kind),
            );
        }
        ui.add_space(8.0);
    }
}

/// Teilfehler einer Spalte als Banner oben in der Spalte; `true`, wenn einer
/// gezeichnet wurde (dann kein Leerzustand).
fn partial_banners(ui: &mut Ui, v: &View, parts: &[(MrPart, &str)]) -> bool {
    let lang = v.lang();
    let mut any = false;
    for host in v.snap.per_host.iter().filter(|h| h.error.is_none()) {
        for (part, kind) in &host.partial {
            let Some((_, name_key)) = parts.iter().find(|(p, _)| p == part) else {
                continue;
            };
            let reason = short_reason(kind, &host.host_key, lang);
            let mut text = tr_fmt(lang, "mr_partial", &[&tr(lang, name_key), &reason]);
            if v.multi_host {
                text = format!("{} · {text}", host.host_key);
            }
            let id = Id::new(("mr_partial", &host.host_key, *name_key, &reason));
            Frame::new()
                .inner_margin(Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    error_banner(ui, v, id, &text, &error_details(kind))
                });
            any = true;
        }
    }
    any
}

/// Grund in Klammern: Kurztext des Fehlers, sonst die erste GraphQL-Meldung.
fn short_reason(kind: &GlabErrorKind, host: &str, lang: Language) -> String {
    if let Some(text) = error_text(kind, host, lang) {
        return text.trim_end_matches('.').to_string();
    }
    match kind {
        GlabErrorKind::GraphQl(messages) if !messages.is_empty() => {
            let first: String = messages[0].chars().take(80).collect();
            first.trim_end_matches('.').to_string()
        }
        kind if is_auth_error(kind) => tr_fmt(lang, "mr_not_logged_in", &[host]),
        _ => tr(lang, "mr_err_unknown"),
    }
}

/// Details zum Fehler, gekürzt wie stderr (2 Zeilen, 300 Zeichen, ohne
/// Zeilen mit „token“). Nur GraphQL-Meldungen und die glab-Version sagen
/// mehr als der Bannertext; sonst leer (kein „Details“).
fn error_details(kind: &GlabErrorKind) -> String {
    let raw = match kind {
        GlabErrorKind::GraphQl(messages) => messages.join("\n"),
        GlabErrorKind::TooOld(version) => format!("glab {version}"),
        _ => return String::new(),
    };
    GlabOutput {
        stderr_tail: raw,
        ..GlabOutput::default()
    }
    .stderr_short()
}

/// Fehlerbanner (wie das App-Banner) mit aufklappbaren Details.
fn error_banner(ui: &mut Ui, v: &View, id: Id, text: &str, details: &str) {
    let (lang, p) = (v.lang(), &v.p);
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(id).unwrap_or(false));
    Frame::new()
        .fill(p.error_bg)
        .stroke(Stroke::new(1.0, p.error_stroke))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.add(icon_image(ICON_WARNING, 14.0, p.error_text));
                ui.label(RichText::new(text).size(12.0).color(p.error_text));
                if !details.is_empty() {
                    let label = RichText::new(tr(lang, "mr_details"))
                        .size(12.0)
                        .color(p.error_text)
                        .underline();
                    let resp = ui
                        .add(Button::new(label).frame(false).selected(open))
                        .on_hover_cursor(CursorIcon::PointingHand);
                    if resp.clicked() {
                        open = !open;
                    }
                }
            });
            if open && !details.is_empty() {
                ui.label(
                    RichText::new(details)
                        .font(FontId::monospace(11.0))
                        .color(p.error_text),
                );
            }
        });
    ui.ctx().data_mut(|d| d.insert_temp(id, open));
}

/// Zustandskarte mittig statt der Spalten.
fn show_card(
    ui: &mut Ui,
    input: &MrViewInput,
    p: &MrPalette,
    card: &ViewCard,
    actions: &mut MrViewActions,
) {
    let lang = input.lang;
    let avail = ui.available_width();
    let width = avail.min(CARD_WIDTH);
    ui.add_space(40.0);
    ui.horizontal(|ui| {
        ui.add_space(((avail - width) / 2.0).max(0.0));
        ui.vertical(|ui| {
            ui.set_width(width);
            Frame::new()
                .fill(p.column)
                .stroke(Stroke::new(1.0, p.line))
                .corner_radius(8)
                .inner_margin(Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 8.0;
                        card_content(ui, lang, p, card, actions);
                    });
                });
        });
    });
}

fn card_content(
    ui: &mut Ui,
    lang: Language,
    p: &MrPalette,
    card: &ViewCard,
    actions: &mut MrViewActions,
) {
    let title = |ui: &mut Ui, text: &str| {
        ui.add(Label::new(RichText::new(text).size(13.0).color(p.title)).wrap());
    };
    let desc = |ui: &mut Ui, text: &str| {
        ui.add(Label::new(RichText::new(text).size(12.0).color(p.text2)).wrap());
    };
    let settings = |ui: &mut Ui, actions: &mut MrViewActions| {
        if ui.button(tr(lang, "mr_open_settings")).clicked() {
            actions.open_settings_mr_tab = true;
        }
    };
    match card {
        ViewCard::Loading => {
            ui.add(Spinner::new().size(18.0).color(p.run_icon));
            desc(ui, &tr(lang, "mr_loading"));
        }
        ViewCard::GlabMissing | ViewCard::GlabTooOld(_) => {
            ui.add(icon_image(ICON_TERMINAL, 24.0, p.text2));
            let (heading, command) = match card {
                ViewCard::GlabTooOld(version) => (
                    tr_fmt(lang, "mr_glab_too_old", &[version]),
                    "winget upgrade GLab.GLab",
                ),
                _ => (tr(lang, "mr_glab_missing"), "winget install GLab.GLab"),
            };
            title(ui, &heading);
            desc(ui, &tr(lang, "mr_glab_missing_desc"));
            command_line(ui, lang, p, command);
            desc(ui, &tr(lang, "mr_glab_restart_hint"));
            centered_row(ui, Id::new("mr_card_glab_buttons"), |ui| {
                settings(ui, actions);
                if ui.button(tr(lang, "refresh")).clicked() {
                    actions.refresh = true;
                }
            });
        }
        ViewCard::NoRemotes => {
            ui.add(icon_image(ICON_PULL_REQUEST, 24.0, p.text2));
            desc(ui, &tr(lang, "mr_no_remotes"));
        }
        ViewCard::NoGitLabHost => {
            ui.add(icon_image(ICON_PULL_REQUEST, 24.0, p.text2));
            desc(ui, &tr(lang, "mr_no_gitlab_host"));
            centered_row(ui, Id::new("mr_card_host_buttons"), |ui| {
                for key in ["mr_add_token", "mr_add_gitlab_host"] {
                    if ui.button(tr(lang, key)).clicked() {
                        actions.open_settings_mr_tab = true;
                    }
                }
            });
        }
        ViewCard::AllDeselected => {
            ui.add(icon_image(ICON_PULL_REQUEST, 24.0, p.text2));
            desc(ui, &tr(lang, "mr_all_deselected"));
            settings(ui, actions);
        }
        ViewCard::NotSignedIn(hosts) => {
            ui.add(icon_image(ICON_LOCK, 24.0, p.text2));
            title(ui, &tr_fmt(lang, "mr_not_logged_in", &[&hosts.join(", ")]));
            if ui.button(tr(lang, "mr_add_token")).clicked() {
                actions.open_settings_mr_tab = true;
            }
        }
    }
}

/// Befehl in Monospace mit Kopier-Button.
fn command_line(ui: &mut Ui, lang: Language, p: &MrPalette, command: &str) {
    centered_row(ui, Id::new(("mr_command", command)), |ui| {
        Frame::new()
            .fill(p.track)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| mono(ui, command, 12.0, p.title));
        let tooltip = tr(lang, "mr_copy_cmd");
        if icon_button(ui, ICON_COPY, &tooltip, false, true).clicked() {
            ui.ctx().copy_text(command.to_string());
        }
    });
}

/// Zeile mittig; die Breite stammt aus dem letzten Frame (egui legt in
/// einem Durchlauf aus).
fn centered_row(ui: &mut Ui, id: Id, add: impl FnOnce(&mut Ui)) {
    let width: f32 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(0.0);
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
        let start = ui.cursor().min.x;
        add(ui);
        let used = ui.min_rect().max.x - start;
        if (used - width).abs() > 0.5 {
            ui.ctx().data_mut(|d| d.insert_temp(id, used));
            ui.ctx().request_repaint();
        }
    });
}

// --- Bausteine ------------------------------------------------------------

/// 1 px Trennlinie über die ganze Spaltenbreite (Zeilen, Abschnitte).
fn divider(ui: &mut Ui, v: &View) {
    let y = ui.cursor().top();
    ui.painter()
        .hline(ui.max_rect().x_range(), y, Stroke::new(1.0, v.p.line));
}

fn icon_image(icon: ImageSource<'static>, size: f32, tint: Color32) -> Image<'static> {
    Image::new(icon)
        .fit_to_exact_size(vec2(size, size))
        .tint(tint)
}

/// 11 px Text, nicht umbrechend.
fn small(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.label(RichText::new(text).size(11.0).color(color))
}

fn mono(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    ui.label(
        RichText::new(text)
            .font(FontId::monospace(size))
            .color(color),
    )
}

/// Element, das in umbrechenden Zeilen nur als Ganzes umbricht: Breite
/// vorab gemessen (`allocate_ui_with_layout` bricht anhand der gewünschten
/// Größe um), mindestens `width` breit.
fn unit(ui: &mut Ui, width: f32, height: f32, add: impl FnOnce(&mut Ui)) -> Response {
    ui.allocate_ui_with_layout(
        vec2(width, height),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(width);
            add(ui);
        },
    )
    .response
}

fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), font, Color32::WHITE)
        .size()
        .x
}

/// Fläche und Rahmen einer Pill.
#[derive(Clone, Copy)]
struct PillStyle {
    fill: Color32,
    stroke: Option<Color32>,
    text: Color32,
}

impl PillStyle {
    fn outline(color: Color32) -> Self {
        Self {
            fill: Color32::TRANSPARENT,
            stroke: Some(color),
            text: color,
        }
    }

    fn filled(fill: Color32, text: Color32) -> Self {
        Self {
            fill,
            stroke: None,
            text,
        }
    }
}

/// Nicht-interaktive Pill (h 18, r 9, 11 px): Entwurf, mergebereit,
/// Gründe, Klon-Zustand.
fn pill(
    ui: &mut Ui,
    text: &str,
    style: PillStyle,
    icon: Option<(ImageSource<'static>, Color32)>,
) -> Response {
    let width = pill_width(ui, text, icon.is_some());
    unit(ui, width, 18.0, |ui| {
        Frame::new()
            .fill(style.fill)
            .stroke(style.stroke.map_or(Stroke::NONE, |c| Stroke::new(1.0, c)))
            .corner_radius(9)
            .inner_margin(Margin::symmetric(7, 1))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if let Some((icon, color)) = icon {
                    ui.add(icon_image(icon, 10.0, color));
                }
                ui.label(RichText::new(text).size(11.0).color(style.text));
            });
    })
}

/// Breite einer Pill für das Messen vorab.
fn pill_width(ui: &Ui, text: &str, icon: bool) -> f32 {
    text_width(ui, text, FontId::proportional(11.0)) + 16.0 + if icon { 14.0 } else { 0.0 }
}

/// Zähler-Pill im Spaltenkopf.
fn count_pill(ui: &mut Ui, v: &View, count: &str) {
    pill(
        ui,
        count,
        PillStyle::filled(v.p.pill_bg, v.p.pill_text),
        None,
    );
}

/// Initialen-Kreis (Painter) mit dem Namen als Text daneben.
fn author(ui: &mut Ui, v: &View, name: &str, color: Color32) {
    let p = &v.p;
    let width = 26.0 + text_width(ui, name, FontId::proportional(11.0));
    unit(ui, width, 20.0, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
        let painter = ui.painter();
        painter.circle_filled(rect.center(), 10.0, p.pill_bg);
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            initials(name),
            FontId::proportional(10.0),
            p.pill_text,
        );
        small(ui, name, color);
    });
}

/// Umschalt-Chip (h 20, r 10) mit Häkchen, wenn aktiv.
fn select_chip(ui: &mut Ui, on: bool, label: &str, count: Option<&str>) -> Response {
    let fg = if on {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let mut atoms = Atoms::new(());
    if on {
        atoms.push_right(icon_image(ICON_CHECK, 10.0, fg));
    }
    atoms.push_right(RichText::new(label).size(11.0));
    if let Some(count) = count {
        atoms.push_right(RichText::new(count).size(10.0));
    }
    let button = Button::selectable(on, atoms)
        .frame_when_inactive(true)
        .corner_radius(10)
        .min_size(vec2(0.0, 20.0))
        .gap(5.0);
    ui.add(button)
}

/// Breite eines Umschalt-Chips für das Messen vorab.
fn chip_width(ui: &Ui, on: bool, label: &str, count: Option<&str>) -> f32 {
    let mut width =
        2.0 * ui.spacing().button_padding.x + text_width(ui, label, FontId::proportional(11.0));
    if on {
        width += 15.0;
    }
    if let Some(count) = count {
        width += 5.0 + text_width(ui, count, FontId::proportional(10.0));
    }
    width
}

/// Icon-Button 22 × 22 mit Tooltip und AccessKit-Namen.
fn icon_button(
    ui: &mut Ui,
    icon: ImageSource<'static>,
    tooltip: &str,
    selected: bool,
    enabled: bool,
) -> Response {
    let tint = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let button = Button::new(icon_image(icon, 14.0, tint))
        .min_size(vec2(22.0, 22.0))
        .corner_radius(6)
        .selected(selected);
    let resp = ui.add_enabled(enabled, button);
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, tooltip));
    if enabled {
        resp.on_hover_text(tooltip)
    } else {
        resp
    }
}

/// „nicht lokal“ als umrandeter Button (öffnet das Menü mit „Neuer Klon …“).
fn not_local_button(
    ui: &mut Ui,
    p: &MrPalette,
    label: &str,
    tooltip: &str,
    open: bool,
) -> Response {
    let button = Button::new(RichText::new(label).size(11.0).color(p.text2))
        .frame_when_inactive(false)
        .corner_radius(6)
        .min_size(vec2(0.0, 22.0))
        .selected(open);
    let resp = ui.add(button);
    ui.painter()
        .rect_stroke(resp.rect, 6.0, Stroke::new(1.0, p.line), StrokeKind::Inside);
    resp.on_hover_text(tooltip)
}

/// Link-artiger Button (unterstrichen, Hand-Cursor).
fn link_button(ui: &mut Ui, text: &str, size: f32, color: Color32) -> Response {
    let label = RichText::new(text).size(size).color(color).underline();
    ui.add(Button::new(label).frame(false))
        .on_hover_cursor(CursorIcon::PointingHand)
}

/// Leerzustand einer Spalte (E13).
fn empty_block(ui: &mut Ui, v: &View, title: &str, hint: Option<&str>) {
    divider(ui, v);
    Frame::new()
        .inner_margin(Margin::symmetric(12, 20))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.add(Label::new(RichText::new(title).size(13.0).color(v.p.title_quiet)).wrap());
            if let Some(hint) = hint {
                ui.add(Label::new(RichText::new(hint).size(11.0).color(v.p.text2)).wrap());
            }
        });
}

/// Spinner mit Text (Ladeanzeige „Nicht beteiligt“).
fn loading_line(ui: &mut Ui, v: &View, text: &str) {
    Frame::new()
        .inner_margin(Margin::symmetric(12, 12))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(Spinner::new().size(14.0).color(v.p.run_icon));
                small(ui, text, v.p.text2);
            });
        });
}

/// „Noch nicht geladen. Jetzt laden“ („Nicht beteiligt“ ohne Abruf).
fn not_loaded_line(ui: &mut Ui, v: &View, actions: &mut MrViewActions) {
    let lang = v.lang();
    Frame::new()
        .inner_margin(Margin::symmetric(12, 12))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                small(ui, &tr(lang, "mr_uninvolved_not_loaded"), v.p.text2);
                if link_button(ui, &tr(lang, "mr_load_now"), 11.0, v.p.text2).clicked() {
                    actions.fetch_uninvolved = true;
                }
            });
        });
}

/// Letzter Pfadteil für Anzeigen („teilt Git-Daten mit backend-api-2“).
fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentLaunchMode;
    use crate::gitlab::checkout::AlreadyCheckedOut;
    use crate::gitlab::model::{HostStatus, Person, UninvolvedCursor};
    use crate::gitlab::remote::{resolve_hosts, GlabHost, RemoteInfo};
    use semver::Version;

    fn agent(id: &str, name: &str) -> AgentProfile {
        AgentProfile {
            id: id.to_string(),
            display_name: name.to_string(),
            program: id.to_string(),
            args: Vec::new(),
            command: None,
            launch_mode: AgentLaunchMode::Terminal,
            terminal_override: None,
            icon: None,
        }
    }

    fn vscode() -> IdeConfig {
        IdeConfig {
            id: "vscode".to_string(),
            display_name: "VS Code".to_string(),
            program: "code".to_string(),
            args: vec![".".to_string()],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,
            icon: None,
        }
    }

    fn never_exists(_: &Path) -> bool {
        false
    }

    /// Lokaler Klon vorhanden, auscheckbar
    fn local_brief() -> CheckoutBrief {
        CheckoutBrief {
            has_clone: true,
            ..CheckoutBrief::default()
        }
    }

    #[test]
    fn layout_mode_breakpoint() {
        // E8: 2 × 420 + 12 = 852 px Inhaltsbreite
        assert_eq!(layout_mode(851.0), LayoutMode::Stacked);
        assert_eq!(layout_mode(851.9), LayoutMode::Stacked);
        assert_eq!(layout_mode(852.0), LayoutMode::Columns);
        assert_eq!(layout_mode(1400.0), LayoutMode::Columns);
        assert_eq!(layout_mode(0.0), LayoutMode::Stacked);
    }

    #[test]
    fn checkout_button_kinds() {
        assert_eq!(checkout_button(&local_brief()), ButtonKind::Menu);

        let wt = PathBuf::from("/elsewhere/backend-api-mr-418");
        let already = CheckoutBrief {
            already: Some(AlreadyCheckedOut {
                path: wt.clone(),
                behind: 0,
            }),
            ..local_brief()
        };
        assert_eq!(
            checkout_button(&already),
            ButtonKind::AlreadyCheckedOut(wt.clone())
        );

        // Kein lokaler Klon: nur „Neuer Klon …“
        assert_eq!(
            checkout_button(&CheckoutBrief::default()),
            ButtonKind::NotLocal
        );

        // E6 auch mit lokalem Klon
        let fork = CheckoutBrief {
            unavailable: Some(UnavailableReason::Fork),
            ..local_brief()
        };
        assert_eq!(
            checkout_button(&fork),
            ButtonKind::Disabled(UnavailableReason::Fork)
        );
        let deleted = CheckoutBrief {
            unavailable: Some(UnavailableReason::SourceBranchDeleted),
            ..CheckoutBrief::default()
        };
        assert_eq!(
            checkout_button(&deleted),
            ButtonKind::Disabled(UnavailableReason::SourceBranchDeleted)
        );

        // Lokaler Stand eines Fork-MR ist nur ein Ordner: Ordner-Pfeil gewinnt
        let fork_out = CheckoutBrief {
            already: Some(AlreadyCheckedOut {
                path: wt.clone(),
                behind: 2,
            }),
            unavailable: Some(UnavailableReason::Fork),
            ..CheckoutBrief::default()
        };
        assert_eq!(
            checkout_button(&fork_out),
            ButtonKind::AlreadyCheckedOut(wt)
        );
    }

    #[test]
    fn checkout_tooltips() {
        let de = Language::De;
        assert_eq!(
            checkout_tooltip(de, &ButtonKind::Menu),
            "Auschecken – Ziel wählen"
        );
        assert_eq!(
            checkout_tooltip(de, &ButtonKind::AlreadyCheckedOut(PathBuf::from("/dev/x"))),
            format!(
                "Branch ist in {} bereits ausgecheckt",
                Path::new("/dev/x").display()
            )
        );
        assert_eq!(
            checkout_tooltip(de, &ButtonKind::NotLocal),
            "Projekt ist nicht lokal geklont – Auschecken nicht möglich"
        );
        assert_eq!(
            checkout_tooltip(de, &ButtonKind::Disabled(UnavailableReason::Fork)),
            "Fork-MR – Auschecken folgt in einer späteren Version"
        );
        assert_eq!(
            checkout_tooltip(
                Language::En,
                &ButtonKind::Disabled(UnavailableReason::SourceBranchDeleted)
            ),
            "Source branch no longer exists"
        );
    }

    #[test]
    fn after_chips_active_agents_and_last_choice() {
        let claude = agent("claude", "Claude Code");
        let codex = agent("codex", "Codex");
        let ide = vscode();
        let chips = after_chips(
            Language::De,
            &[&claude, &codex],
            Some(&ide),
            &AfterAction::Agent("codex".to_string()),
        );
        let labels: Vec<&str> = chips.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Claude Code",
                "Codex",
                "Shell",
                "Explorer",
                "VS Code",
                "Nichts"
            ]
        );
        let selected: Vec<&AfterAction> = chips
            .iter()
            .filter(|c| c.selected)
            .map(|c| &c.action)
            .collect();
        assert_eq!(selected, [&AfterAction::Agent("codex".to_string())]);
        assert_eq!(chips[2].action, AfterAction::Shell);
        assert_eq!(chips[3].action, AfterAction::Explorer);
        assert_eq!(chips[4].action, AfterAction::Ide);
        assert_eq!(chips[5].action, AfterAction::Nothing);

        // „Nichts“ als letzte Wahl bleibt vorausgewählt
        let chips = after_chips(Language::En, &[&claude], None, &AfterAction::Nothing);
        let labels: Vec<&str> = chips.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Claude Code", "Shell", "Explorer", "Nothing"]);
        assert!(chips[3].selected && chips.iter().filter(|c| c.selected).count() == 1);
    }

    #[test]
    fn after_chips_fallback_when_last_unavailable() {
        let claude = agent("claude", "Claude Code");
        // Gelöschter oder inaktiver Agent → erster aktiver Agent
        let chips = after_chips(
            Language::De,
            &[&claude],
            None,
            &AfterAction::Agent("gone".to_string()),
        );
        assert!(chips[0].selected, "{chips:?}");
        assert_eq!(chips.iter().filter(|c| c.selected).count(), 1);
        // IDE gewählt, aber das Profil hat keine → ohne Agenten „Nichts“
        let chips = after_chips(Language::De, &[], None, &AfterAction::Ide);
        let selected: Vec<&AfterAction> = chips
            .iter()
            .filter(|c| c.selected)
            .map(|c| &c.action)
            .collect();
        assert_eq!(selected, [&AfterAction::Nothing]);
        // Shell bleibt Shell, auch ohne Agenten
        let chips = after_chips(Language::De, &[], None, &AfterAction::Shell);
        assert!(chips
            .iter()
            .any(|c| c.selected && c.action == AfterAction::Shell));
    }

    #[test]
    fn after_preselection_fixed_or_last() {
        let mut cfg = MrConfig {
            after_checkout_last: AfterAction::Explorer,
            ..MrConfig::default()
        };
        assert_eq!(after_preselection(&cfg), &AfterAction::Explorer);
        cfg.after_checkout_default = AfterCheckoutPref::Fixed(AfterAction::Shell);
        assert_eq!(after_preselection(&cfg), &AfterAction::Shell);
    }

    #[test]
    fn tooltip_texts_singular_plural() {
        let de = Language::De;
        let en = Language::En;
        assert_eq!(
            hidden_by_selection_text(de, 1),
            "1 MR aus abgewählten Repositories ausgeblendet"
        );
        assert_eq!(
            hidden_by_selection_text(de, 3),
            "3 MRs aus abgewählten Repositories ausgeblendet"
        );
        assert_eq!(
            hidden_by_selection_text(en, 2),
            "2 MRs from deselected repositories hidden"
        );
        assert_eq!(
            clone_state_text(de, &CloneState::Dirty { files: 1 }),
            "1 Änderung"
        );
        assert_eq!(
            clone_state_text(de, &CloneState::Dirty { files: 3 }),
            "3 Änderungen"
        );
        assert_eq!(
            clone_state_text(en, &CloneState::Dirty { files: 3 }),
            "3 changes"
        );
        assert_eq!(
            clone_state_text(de, &CloneState::InUse { iid: Some(412) }),
            "in Arbeit für !412"
        );
        assert_eq!(
            clone_state_text(de, &CloneState::InUse { iid: None }),
            "in Arbeit"
        );
        assert_eq!(clone_state_text(de, &CloneState::Free), "frei");
        let wt = PathBuf::from("/dev/wt");
        assert_eq!(
            clone_state_text(
                de,
                &CloneState::Blocked(BlockReason::CheckedOutElsewhere(wt.clone()))
            ),
            format!("Branch ist in {} bereits ausgecheckt", wt.display())
        );
        assert_eq!(
            menu_header_text(de, "kern/backend-api", 1),
            "kern/backend-api · 1 lokaler Klon"
        );
        assert_eq!(
            menu_header_text(de, "kern/backend-api", 3),
            "kern/backend-api · 3 lokale Klone"
        );
        assert_eq!(
            menu_header_text(en, "kern/backend-api", 1),
            "kern/backend-api · 1 local clone"
        );
        assert_eq!(header_count_text(de, HeaderCount::Total(6)), "6");
        assert_eq!(
            header_count_text(
                de,
                HeaderCount::Of {
                    visible: 4,
                    total: 6
                }
            ),
            "4 von 6"
        );
        let counts = ChipCounts {
            reviewer: 4,
            mention: 3,
            uninvolved: None,
        };
        assert_eq!(
            chip_all_tooltip(de, &counts, 5),
            "Auch MRs ohne meine Beteiligung anzeigen"
        );
        let counts = ChipCounts {
            uninvolved: Some(12),
            ..counts
        };
        assert_eq!(
            chip_all_tooltip(de, &counts, 5),
            "Auch MRs ohne meine Beteiligung anzeigen (12 offene in 5 lokalen Projekten)"
        );
        assert_eq!(
            chip_all_tooltip(de, &counts, 1),
            "Auch MRs ohne meine Beteiligung anzeigen (12 offene in 1 lokalen Projekt)"
        );
        assert_eq!(
            chip_all_tooltip(en, &counts, 1),
            "Also show MRs I'm not involved in (12 open in 1 local project)"
        );
        assert_eq!(
            uninvolved_sub_text(de, 1),
            "offene MRs des lokalen Projekts ohne deine Beteiligung"
        );
        assert_eq!(
            uninvolved_sub_text(de, 4),
            "offene MRs der 4 lokalen Projekte ohne deine Beteiligung"
        );
        assert_eq!(
            uninvolved_sub_text(en, 1),
            "open MRs in the local project without your involvement"
        );
    }

    #[test]
    fn pipeline_tooltip_names_status_and_link() {
        let mut mr = MergeRequest {
            web_url: "https://gitlab.example.com/kern/api/-/merge_requests/1".to_string(),
            web_path: Some("/kern/api/-/merge_requests/1".to_string()),
            pipeline: PipelineState::Success,
            pipeline_path: Some("/kern/api/-/pipelines/7".to_string()),
            ..MergeRequest::default()
        };
        assert_eq!(
            pipeline_tooltip(Language::De, &mr),
            "Pipeline grün – Pipeline im Browser öffnen"
        );
        assert_eq!(
            pipeline_tooltip(Language::En, &mr),
            "Pipeline passed – open pipeline in browser"
        );
        // Ohne sicheren Link nur der Status
        mr.pipeline_path = Some("//evil.example/x".to_string());
        assert_eq!(pipeline_tooltip(Language::De, &mr), "Pipeline grün");
        mr.pipeline = PipelineState::NoPipeline;
        mr.pipeline_path = None;
        assert_eq!(pipeline_tooltip(Language::De, &mr), "Keine Pipeline");
    }

    fn uninvolved(loaded: usize, total: usize, more: bool) -> UninvolvedList {
        UninvolvedList {
            mrs: (0..loaded)
                .map(|i| MergeRequest {
                    id: format!("gid://gitlab/MergeRequest/{i}"),
                    iid: i as u64,
                    ..MergeRequest::default()
                })
                .collect(),
            total,
            more: if more {
                vec![UninvolvedCursor {
                    host_key: "gitlab.example.com".to_string(),
                    full_path: "kern/api".to_string(),
                    cursor: "abc".to_string(),
                }]
            } else {
                Vec::new()
            },
        }
    }

    #[test]
    fn uninvolved_paging() {
        // (angezeigt, gesamt, weitere)
        assert_eq!(
            uninvolved_counts(&uninvolved(12, 30, true), 10),
            (10, 30, 20)
        );
        assert_eq!(uninvolved_counts(&uninvolved(4, 4, false), 10), (4, 4, 0));
        // Erst bereits geladene zeigen …
        assert_eq!(load_more_step(&uninvolved(25, 30, true), 10), (20, false));
        // … dann die nächste Seite holen
        assert_eq!(load_more_step(&uninvolved(12, 30, true), 10), (20, true));
        // Ohne weitere Seiten nichts holen
        assert_eq!(load_more_step(&uninvolved(12, 30, false), 10), (20, false));
        // … und nur Geladenes als „weitere“ anbieten: `count` 30, aber
        // keine Cursor mehr (zwischendurch gemergt, Seite gescheitert)
        assert_eq!(
            uninvolved_counts(&uninvolved(12, 30, false), 10),
            (10, 30, 2)
        );
        assert_eq!(
            uninvolved_counts(&uninvolved(12, 30, false), 20),
            (12, 30, 0)
        );
    }

    #[test]
    fn choose_target_empty_folder_or_subfolder() {
        let picked = Path::new("/dev/reviews/x");
        // Leerer (oder neuer) Ordner ist selbst das Ziel
        assert_eq!(
            choose_target(picked, "api-mr-1", true),
            PathBuf::from("/dev/reviews/x")
        );
        // Sonst ein Unterordner mit dem vorgeschlagenen Namen
        assert_eq!(
            choose_target(picked, "api-mr-1", false),
            PathBuf::from("/dev/reviews/x/api-mr-1")
        );
    }

    fn repo_with_remote(path: &str, url: Option<&str>) -> RepoInfo {
        let mut repo = RepoInfo::new(PathBuf::from(path), "main".to_string(), false, false);
        repo.remotes = url
            .map(|u| RemoteInfo::new("origin", u))
            .into_iter()
            .collect();
        repo
    }

    fn local(path: &str, included: bool) -> LocalProject {
        LocalProject {
            host_key: "gitlab.example.com".to_string(),
            full_path: path.to_string(),
            repos: vec![PathBuf::from("/dev").join(path)],
            included,
        }
    }

    fn gitlab_hosts() -> HostMap {
        HostMap {
            gitlab_hosts: vec!["gitlab.example.com".to_string()],
            ..HostMap::default()
        }
    }

    fn host(key: &str, error: Option<GlabErrorKind>) -> HostStatus {
        HostStatus {
            host_key: key.to_string(),
            error,
            ..HostStatus::default()
        }
    }

    #[test]
    fn view_card_order() {
        let gl = [repo_with_remote(
            "/dev/api",
            Some("git@gitlab.example.com:kern/api.git"),
        )];
        let hosts = gitlab_hosts();
        let locals = [local("kern/api", true)];
        let snap = MrSnapshot {
            per_host: vec![host("gitlab.example.com", None)],
            ..MrSnapshot::default()
        };
        // glab fehlt oder ist zu alt: vor allem anderen
        assert_eq!(
            view_card(
                Some(&GlabErrorKind::NotInstalled),
                true,
                &gl,
                &hosts,
                &locals,
                Some(&snap)
            ),
            Some(ViewCard::GlabMissing)
        );
        assert_eq!(
            view_card(
                Some(&GlabErrorKind::TooOld(Version::new(1, 90, 0))),
                false,
                &gl,
                &hosts,
                &locals,
                Some(&snap)
            ),
            Some(ViewCard::GlabTooOld("1.90.0".to_string()))
        );
        // Keine Remotes, Remotes ohne GitLab-Host
        let none = HostMap::default();
        let bare = [repo_with_remote("/dev/x", None)];
        assert_eq!(
            view_card(None, false, &bare, &none, &[], None),
            Some(ViewCard::NoRemotes)
        );
        let gh = [repo_with_remote(
            "/dev/y",
            Some("https://github.com/a/b.git"),
        )];
        assert_eq!(
            view_card(None, false, &gh, &none, &[], None),
            Some(ViewCard::NoGitLabHost)
        );
        // Erster Start: Scan bzw. glab-Erkennung läuft noch → Laden statt
        // „Kein GitLab-Host“ oder „Keine Remotes“
        assert_eq!(
            view_card(None, true, &gh, &none, &[], None),
            Some(ViewCard::Loading)
        );
        assert_eq!(
            view_card(None, true, &[], &none, &[], None),
            Some(ViewCard::Loading)
        );
        // Ausdrücklich eingetragener GitLab-Host ohne Klon: keine Karte
        assert_eq!(
            view_card(None, false, &bare, &hosts, &[], Some(&snap)),
            None
        );
        // Alle Repositories abgewählt
        assert_eq!(
            view_card(
                None,
                false,
                &gl,
                &hosts,
                &[local("kern/api", false)],
                Some(&snap)
            ),
            Some(ViewCard::AllDeselected)
        );
        // Noch kein Stand
        assert_eq!(
            view_card(None, false, &gl, &hosts, &locals, None),
            Some(ViewCard::Loading)
        );
        // Nicht angemeldet: Karte nur, wenn kein Host Daten liefert und
        // alle wegen der Anmeldung
        let signed_out = MrSnapshot {
            per_host: vec![
                host("gitlab.example.com", Some(GlabErrorKind::NotAuthenticated)),
                host("git.intern", Some(GlabErrorKind::Expired)),
            ],
            ..MrSnapshot::default()
        };
        assert_eq!(
            view_card(None, false, &gl, &hosts, &locals, Some(&signed_out)),
            Some(ViewCard::NotSignedIn(vec![
                "gitlab.example.com".to_string(),
                "git.intern".to_string()
            ]))
        );
        // Ein anderer Host ist nicht erreichbar: Spalten mit beiden Bannern
        let mixed = MrSnapshot {
            per_host: vec![
                host("gitlab.example.com", Some(GlabErrorKind::NotAuthenticated)),
                host("git.intern", Some(GlabErrorKind::Network)),
            ],
            ..MrSnapshot::default()
        };
        assert_eq!(
            view_card(None, false, &gl, &hosts, &locals, Some(&mixed)),
            None
        );
        let partly = MrSnapshot {
            per_host: vec![
                host("gitlab.example.com", Some(GlabErrorKind::Expired)),
                host("git.intern", None),
            ],
            ..MrSnapshot::default()
        };
        assert_eq!(
            view_card(None, false, &gl, &hosts, &locals, Some(&partly)),
            None
        );
        assert_eq!(
            view_card(None, false, &gl, &hosts, &locals, Some(&snap)),
            None
        );
        // Mit Stand und Hosts spielt die laufende Erkennung keine Rolle
        assert_eq!(
            view_card(None, true, &gl, &hosts, &locals, Some(&snap)),
            None
        );
    }

    // --- Zeichnen (ohne Fenster, ohne Netz) -------------------------------

    const HOST: &str = "gitlab.example.com";
    const NOW: i64 = 1_792_216_800; // 2026-10-17 10:00 UTC

    fn person(username: &str, name: &str, is_me: bool) -> Person {
        Person {
            username: username.to_string(),
            name: name.to_string(),
            bot: false,
            is_me,
        }
    }

    fn mr(iid: u64, project: &str, title: &str) -> MergeRequest {
        MergeRequest {
            host_key: HOST.to_string(),
            id: format!("gid://gitlab/MergeRequest/{iid}"),
            iid,
            title: title.to_string(),
            web_url: format!("https://{HOST}/{project}/-/merge_requests/{iid}"),
            web_path: Some(format!("/{project}/-/merge_requests/{iid}")),
            project_path: project.to_string(),
            source_project_path: Some(project.to_string()),
            source_branch: format!("feature/mr-{iid}"),
            target_branch: "main".to_string(),
            source_branch_exists: Some(true),
            author: person("dwagner", "David Wagner", false),
            updated_at: Some(NOW - 3600),
            local: true,
            ..MergeRequest::default()
        }
    }

    /// Stand mit allen Zeilenarten, Teilfehler und Hinweiszeile.
    fn sample_snapshot() -> MrSnapshot {
        let mut own = mr(
            412,
            "kern/api",
            "CSV-Export für Mandanten mit sehr langem Titel, der gekürzt wird",
        );
        own.author = person("taltmann", "Tim Altmann", true);
        own.approved_by = vec![person("jkrueger", "Jonas Krüger", false)];
        own.approvals_required = Some(2);
        own.pipeline = PipelineState::Success;
        own.pipeline_path = Some("/kern/api/-/pipelines/77".to_string());
        own.threads_open = 2;
        own.threads_resolved = 1;
        let mut own_ready = mr(87, "kern/api", "Login-Timeout");
        own_ready.author = own.author.clone();
        own_ready.detailed_merge_status = Some("MERGEABLE".to_string());
        own_ready.draft = true;
        let mut review = mr(421, "kern/api", "Mandantenfilter");
        review.reasons = vec![Reason::Reviewer, Reason::MentionComment];
        review.my_review = Some(MyReview::Pending);
        review.approvals_required = Some(1);
        review.pipeline = PipelineState::Running;
        let mut mention = mr(92, "web/portal", "Navigation");
        mention.reasons = vec![Reason::MentionDescription];
        mention.local = false;
        mention.pipeline = PipelineState::Failed;
        let mut fork = mr(240, "kern/api", "Fork-Beitrag");
        fork.reasons = vec![Reason::Reviewer];
        fork.my_review = Some(MyReview::ChangesRequested);
        fork.source_project_path = Some("extern/api".to_string());
        let mut quiet = mr(310, "kern/api", "Abhängigkeiten aktualisieren");
        quiet.pipeline = PipelineState::Manual;
        MrSnapshot {
            mine: vec![own, own_ready],
            others: vec![review, mention, fork],
            uninvolved: Some(UninvolvedList {
                mrs: vec![quiet],
                total: 5,
                more: vec![UninvolvedCursor {
                    host_key: HOST.to_string(),
                    full_path: "kern/api".to_string(),
                    cursor: "c1".to_string(),
                }],
            }),
            hidden_by_selection: 2,
            badge: 1,
            per_host: vec![HostStatus {
                host_key: HOST.to_string(),
                user: Some("taltmann".to_string()),
                partial: vec![(MrPart::Mentions, GlabErrorKind::InsufficientScope)],
                ..HostStatus::default()
            }],
            fetched_at: NOW,
        }
    }

    /// Klone von `kern/api`: sauber auf `main`, mit Änderungen, MR-Branch
    /// eines anderen MR.
    fn sample_repos() -> Vec<RepoInfo> {
        let url = "git@gitlab.example.com:kern/api.git";
        let mut free = repo_with_remote("/dev/api", Some(url));
        free.name = "api".to_string();
        let mut dirty = repo_with_remote("/dev/api-2", Some(url));
        dirty.dirty = true;
        dirty.dirty_files = 3;
        let mut busy = repo_with_remote("/dev/api-3", Some(url));
        busy.branch = "feature/mr-87".to_string();
        vec![free, dirty, busy]
    }

    fn sample_hosts(repos: &[RepoInfo]) -> HostMap {
        let glab = [GlabHost {
            key: HOST.to_string(),
            user: "taltmann".to_string(),
            ..GlabHost::default()
        }];
        let remotes: Vec<_> = repos
            .iter()
            .flat_map(|r| r.remotes.iter().filter_map(|x| x.parsed.as_ref()))
            .collect();
        resolve_hosts(remotes, &glab, &MrConfig::default())
    }

    fn mru(_: &Path) -> u64 {
        u64::try_from(NOW - 7200).unwrap()
    }

    fn render(input: &MrViewInput, state: &mut MrViewState, width: f32) -> MrViewActions {
        render_repaint(input, state, width).0
    }

    /// Wie `render`, dazu: will der letzte Durchlauf sofort wieder zeichnen
    /// (Spinner)?
    fn render_repaint(
        input: &MrViewInput,
        state: &mut MrViewState,
        width: f32,
    ) -> (MrViewActions, bool) {
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx, input.theme);
        let mut actions = MrViewActions::default();
        let mut repaint = false;
        // Mehrere Durchläufe: egui misst im ersten (Popups, gemerkte Breiten)
        for _ in 0..3 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(width, 800.0),
                )),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                show_merge_requests(ui, input, state, &mut actions)
            });
            repaint = output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|v| v.repaint_delay.is_zero());
            // Ohne Renderer: Textur-Updates (Schrift-Atlas) verwerfen
            output.drop_without_applying_deltas();
        }
        (actions, repaint)
    }

    #[test]
    fn show_merge_requests_renders_dashboard_without_actions() {
        let snap = sample_snapshot();
        let repos = sample_repos();
        let hosts = sample_hosts(&repos);
        let cfg = MrConfig::default();
        let locals = crate::gitlab::remote::local_projects(&repos, &hosts, &cfg);
        let claude = agent("claude", "Claude Code");
        let agents = [&claude];
        let ide = vscode();
        for theme in crate::config::Theme::all() {
            for width in [1200.0, 700.0] {
                let input = MrViewInput {
                    lang: Language::De,
                    theme: &theme,
                    cfg: &cfg,
                    snapshot: Some(&snap),
                    uninvolved_loading: false,
                    glab_error: None,
                    discovering: false,
                    repos: &repos,
                    hosts: &hosts,
                    locals: &locals,
                    agents: &agents,
                    ide: Some(&ide),
                    lookup: MenuLookup {
                        mru: &mru,
                        exists: &never_exists,
                    },
                    now: NOW,
                };
                let mut state = MrViewState::new(&cfg);
                state.show_uninvolved = true;
                let actions = render(&input, &mut state, width);
                assert_eq!(actions, MrViewActions::default(), "{theme:?} {width}");
                assert!(state.reviewer_on && state.mention_on);
            }
        }
    }

    #[test]
    fn show_merge_requests_renders_cards_and_empty_states() {
        let repos = sample_repos();
        let hosts = sample_hosts(&repos);
        let cfg = MrConfig::default();
        let locals = crate::gitlab::remote::local_projects(&repos, &hosts, &cfg);
        let theme = crate::config::Theme::Dark;
        let old = GlabErrorKind::TooOld(Version::new(1, 90, 0));
        let signed_out = MrSnapshot {
            per_host: vec![host(HOST, Some(GlabErrorKind::NotAuthenticated))],
            ..MrSnapshot::default()
        };
        let empty = MrSnapshot {
            per_host: vec![host(HOST, None)],
            ..MrSnapshot::default()
        };
        let no_hosts = HostMap::default();
        let cases: [(Option<&GlabErrorKind>, &HostMap, Option<&MrSnapshot>); 5] = [
            (Some(&GlabErrorKind::NotInstalled), &hosts, None),
            (Some(&old), &hosts, None),
            (None, &no_hosts, None),
            (None, &hosts, Some(&signed_out)),
            (None, &hosts, Some(&empty)),
        ];
        for (glab_error, hosts, snapshot) in cases {
            for lang in [Language::De, Language::En] {
                let input = MrViewInput {
                    lang,
                    theme: &theme,
                    cfg: &cfg,
                    snapshot,
                    uninvolved_loading: true,
                    glab_error,
                    discovering: false,
                    repos: &repos,
                    hosts,
                    locals: &locals,
                    agents: &[],
                    ide: None,
                    lookup: MenuLookup {
                        mru: &mru,
                        exists: &never_exists,
                    },
                    now: NOW,
                };
                let mut state = MrViewState::new(&cfg);
                state.reviewer_on = false;
                state.show_uninvolved = true;
                assert_eq!(render(&input, &mut state, 1000.0), MrViewActions::default());
            }
        }
    }

    #[test]
    fn uninvolved_spinner_only_while_loading() {
        // Host nicht erreichbar: der Abruf endet vor „Nicht beteiligt“, ohne
        // Teilfehler dafür; ebenso ein Abruf ohne „Nicht beteiligt“
        let snap = MrSnapshot {
            per_host: vec![host(HOST, Some(GlabErrorKind::Network))],
            ..MrSnapshot::default()
        };
        let repos = sample_repos();
        let hosts = sample_hosts(&repos);
        let cfg = MrConfig::default();
        let locals = crate::gitlab::remote::local_projects(&repos, &hosts, &cfg);
        let theme = crate::config::Theme::Dark;
        for loading in [false, true] {
            let input = MrViewInput {
                lang: Language::De,
                theme: &theme,
                cfg: &cfg,
                snapshot: Some(&snap),
                uninvolved_loading: loading,
                glab_error: None,
                discovering: false,
                repos: &repos,
                hosts: &hosts,
                locals: &locals,
                agents: &[],
                ide: None,
                lookup: MenuLookup {
                    mru: &mru,
                    exists: &never_exists,
                },
                now: NOW,
            };
            let mut state = MrViewState::new(&cfg);
            state.show_uninvolved = true;
            let (actions, repaint) = render_repaint(&input, &mut state, 1200.0);
            // Kein Abruf ohne Klick, kein Dauer-Neuzeichnen ohne Laden
            assert_eq!(actions, MrViewActions::default(), "loading {loading}");
            assert_eq!(repaint, loading, "Spinner nur beim Laden");
        }
    }

    #[test]
    fn checkout_menus_render_and_keep_choice_state() {
        let snap = sample_snapshot();
        let repos = sample_repos();
        let hosts = sample_hosts(&repos);
        let cfg = MrConfig {
            review_root: Some("/dev/reviews".to_string()),
            after_checkout_last: AfterAction::Agent("gone".to_string()),
            ..MrConfig::default()
        };
        let locals = crate::gitlab::remote::local_projects(&repos, &hosts, &cfg);
        let claude = agent("claude", "Claude Code");
        let agents = [&claude];
        let ide = vscode();
        let theme = crate::config::Theme::Light;
        let input = MrViewInput {
            lang: Language::De,
            theme: &theme,
            cfg: &cfg,
            snapshot: Some(&snap),
            uninvolved_loading: false,
            glab_error: None,
            discovering: false,
            repos: &repos,
            hosts: &hosts,
            locals: &locals,
            agents: &agents,
            ide: Some(&ide),
            lookup: MenuLookup {
                mru: &mru,
                exists: &never_exists,
            },
            now: NOW,
        };
        let view = View {
            input: &input,
            p: mr_palette(&theme, &egui::Visuals::light()),
            snap: &snap,
            open: OpenBranches::from_snapshot(&snap),
            multi_host: false,
            has_data: true,
        };
        let review = &snap.others[0];
        let cache_id = Id::new("menu_test");
        let ctx = egui::Context::default();
        let mut actions = MrViewActions::default();
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                checkout_menu_ui(ui, &view, review, false, cache_id, &mut actions);
                already_menu_ui(ui, &view, Path::new("/dev/api-3"), &mut actions);
            });
            output.drop_without_applying_deltas();
        }
        assert_eq!(actions, MrViewActions::default());
        let cache = ctx
            .data(|d| d.get_temp::<MenuCache>(cache_id))
            .expect("Menü bleibt bis zur Wahl gemerkt");
        // Gelöschter Agent als letzte Wahl → erster aktiver Agent
        assert_eq!(cache.after, AfterAction::Agent("claude".to_string()));
        // MR von anderen mit Review-Ordner → Worktree dort, drei Klone
        let wt = cache.menu.new_worktree.expect("Worktree-Vorschlag");
        assert!(wt.in_review_root, "{wt:?}");
        assert_eq!(cache.menu.clones.len(), 3);
        let states: Vec<String> = cache
            .menu
            .clones
            .iter()
            .map(|c| clone_state_text(Language::De, &c.state))
            .collect();
        assert_eq!(states, ["frei", "3 Änderungen", "in Arbeit für !87"]);
        // Fork-MR: deaktiviert (E6)
        let brief = |mr| checkout_brief(&MrRefInfo::from_mr(mr), &repos, &hosts);
        assert_eq!(
            checkout_button(&brief(&snap.others[2])),
            ButtonKind::Disabled(UnavailableReason::Fork)
        );
        // Projekt ohne Klon: „nicht lokal“, das Menü bietet „Neuer Klon …“
        // mit Klon-URL aus `webUrl`
        let portal = &snap.others[1];
        assert_eq!(checkout_button(&brief(portal)), ButtonKind::NotLocal);
        assert!(view.menu(portal, false, &input.lookup).clone_url.is_some());
    }

    #[test]
    fn view_state_session_start() {
        let cfg = MrConfig {
            show_uninvolved_default: true,
            ..MrConfig::default()
        };
        let state = MrViewState::new(&cfg);
        assert!(state.reviewer_on && state.mention_on && state.show_uninvolved);
        assert_eq!(state.uninvolved_shown, UNINVOLVED_STEP);
        assert_eq!(
            state.chips(),
            Chips {
                reviewer: true,
                mention: true,
                all: true
            }
        );
    }

    #[test]
    fn short_reason_and_details() {
        let de = Language::De;
        // Kurztext ohne Schlusspunkt (steht in Klammern)
        assert_eq!(
            short_reason(&GlabErrorKind::InsufficientScope, HOST, de),
            "Das Token braucht read_api"
        );
        assert_eq!(
            short_reason(
                &GlabErrorKind::GraphQl(vec!["Field 'x' doesn't exist.".to_string()]),
                HOST,
                de
            ),
            "Field 'x' doesn't exist"
        );
        assert_eq!(
            short_reason(&GlabErrorKind::Other, HOST, de),
            "unbekannter Fehler"
        );
        // Details wie stderr gekürzt: Zeilen mit „token“ fallen weg
        let kind = GlabErrorKind::GraphQl(vec![
            "first".to_string(),
            "invalid token abc".to_string(),
            "second".to_string(),
        ]);
        assert_eq!(error_details(&kind), "first\nsecond");
        // Sonst kein „Details“ (keine internen Namen wie „Network“)
        for kind in [
            GlabErrorKind::Network,
            GlabErrorKind::InsufficientScope,
            GlabErrorKind::HostKey,
            GlabErrorKind::Other,
        ] {
            assert_eq!(error_details(&kind), "", "{kind:?}");
        }
    }

    #[test]
    fn mr_icons_are_white_for_tint() {
        // `tint` multipliziert: nur weiß gerenderte Icons lassen sich färben
        for icon in [
            include_str!("../../assets/icons/mr/check-circle.svg"),
            include_str!("../../assets/icons/mr/chat-circle-check.svg"),
            include_str!("../../assets/icons/mr/folder-arrow.svg"),
            include_str!("../../assets/icons/mr/git-branch.svg"),
        ] {
            assert!(icon.contains(r##"color="#FFFFFF""##), "{icon}");
            assert!(icon.contains(r#"viewBox="0 0 256 256""#), "{icon}");
        }
    }
}
