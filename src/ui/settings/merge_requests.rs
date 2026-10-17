//! Einstellungs-Tab „Merge Requests“ (Task 12): GitLab CLI, Anmeldung &
//! Token je GitLab-Host, Repositories für Merge Requests, Auschecken sowie
//! Anzeige & Aktualisierung. Zeichnet nur und fordert Hintergrundarbeit über
//! `MrSettingsOutput` an; die Worker startet `app.rs`, ihre Ergebnisse
//! liegen in `MrSettingsInput` bzw. kommen über `SettingsState::on_token_event`
//! zurück. Token-Aktionen wirken sofort, alles andere erst mit „Speichern“.
//! Das Token steht nur im Formularfeld (überschrieben beim Schließen, ohne
//! Undo-Verlauf) und geht als `SecretString` an die Worker (E3).

use super::SettingsState;
use crate::config::{
    AfterAction, AfterCheckoutPref, AppConfig, HostAlias, IdeConfig, MrConfig, Theme,
};
use crate::git::RepoInfo;
use crate::gitlab::checkout::review_root_dir;
use crate::gitlab::glab::GlabErrorKind;
use crate::gitlab::remote::{
    local_projects, project_key, resolve_hosts, GlabHost, HostMap, HostTarget, LocalProject,
    RemoteRef, RemoteScheme,
};
use crate::gitlab::secret::SecretString;
use crate::gitlab::service::{
    error_text, is_auth_error, login_key, token_create_url, AccessState, GitManagerRights,
    GlabRights, HostProbe, Protocol, SavedWhere, TokenCheck, TokenKind, TokenSource,
};
use crate::gitlab::time::format_date;
use crate::i18n::{tr, tr_fmt, Language};
use crate::ui::theme::{mr_palette, MrPalette};
use egui::text_edit::TextEditState;
use egui::{
    vec2, Align, Button, Color32, CursorIcon, FontId, Frame, Id, Image, ImageSource, Label, Layout,
    Margin, Response, RichText, Spinner, Stroke, TextEdit, Ui, WidgetInfo, WidgetType,
};
use semver::Version;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// Icons: Phosphor Regular wie in der MR-Ansicht (weiß, per `tint` gefärbt)
const ICON_CHECK_CIRCLE: ImageSource =
    egui::include_image!("../../../assets/icons/mr/check-circle.svg");
const ICON_X_CIRCLE: ImageSource = egui::include_image!("../../../assets/icons/mr/x-circle.svg");
const ICON_WARNING: ImageSource =
    egui::include_image!("../../../assets/icons/mr/warning-circle.svg");
const ICON_MINUS_CIRCLE: ImageSource =
    egui::include_image!("../../../assets/icons/mr/minus-circle.svg");
const ICON_LOCK: ImageSource = egui::include_image!("../../../assets/icons/mr/lock-simple.svg");
const ICON_COPY: ImageSource = egui::include_image!("../../../assets/icons/mr/copy.svg");
const ICON_OPEN_EXTERNAL: ImageSource =
    egui::include_image!("../../../assets/icons/mr/arrow-square-out.svg");
const ICON_TRASH: ImageSource = egui::include_image!("../../../assets/icons/trash.svg");

/// Warnung „Läuft in {n} Tagen ab“ ab so vielen Tagen vor dem Ablauf.
const EXPIRY_WARN_DAYS: i64 = 14;
const DAY: i64 = 86_400;
/// Ab dieser Höhe scrollt die Repo-Tabelle
const REPO_LIST_MAX_HEIGHT: f32 = 260.0;
/// Shell-Metazeichen, die ein bloßer Programmname nicht enthalten darf
/// (Regel wie `git::is_program_available`)
const SHELL_META: &str = "&|;><`$\"'";
/// Ordnername neuer Worktrees (K1, nur Anzeige)
const FOLDER_PATTERN: &str = "{repo}-mr-{iid}";
/// iid im Beispiel „z. B. backend-api-mr-418“
const EXAMPLE_IID: u64 = 418;
/// Zugriff ohne Prüfergebnis
static UNKNOWN_ACCESS: AccessState = AccessState::Unknown;
/// Zähler für die Feld-Id je Formularöffnung
static NEXT_TOKEN_FIELD: AtomicU64 = AtomicU64::new(0);

/// Was der Tab je Frame braucht; baut `app.rs` aus seinem Cache der Sitzung.
pub struct MrSettingsInput<'a> {
    /// Aktives Theme (Farben über `mr_palette`, E14)
    pub theme: &'a Theme,
    /// glab-Programm der Sitzung (Prüfungen, Token); ein anderes im Entwurf
    /// gilt erst nach „Speichern“
    pub glab_program: &'a str,
    /// Gescannte Repos: Remotes → Hosts und Projekte
    pub repos: &'a [RepoInfo],
    /// Lokale glab-Erkennung (E16)
    pub glab_hosts: &'a [GlabHost],
    /// Letzte Prüfung je Host (`probe_host`)
    pub probes: &'a BTreeMap<String, HostProbe>,
    /// Hosts, deren Prüfung läuft
    pub probing: &'a BTreeSet<String>,
    pub glab_check: Option<&'a GlabCheck>,
    pub glab_checking: bool,
    /// Ein Token wird gerade geprüft oder gespeichert (Host)
    pub token_busy: Option<&'a str>,
    /// Env-Token im eigenen Prozess (`env_token_override`)
    pub env_token: Option<&'static str>,
    /// Jetzt (Unix-Sekunden) für den Token-Ablauf
    pub now: i64,
}

/// Arbeit, die der Tab in diesem Frame anfordert; `app.rs` startet die
/// Worker. Das Token geht nur verschoben weiter, nie in die Config.
#[derive(Debug, Default)]
pub struct MrSettingsOutput {
    /// Prüfung je Host (Anmeldung, Rechte, Projektzugriff)
    pub start_probe: Vec<String>,
    pub start_verify: Option<(String, SecretString, Protocol)>,
    pub start_save: Option<(String, SecretString, Protocol)>,
    /// GitLab-Hosts oder „gehört zu …“ im Entwurf geändert: lokale Erkennung
    pub hosts_changed: bool,
    /// glab-Programm prüfen (Version, Pfad)
    pub start_glab_check: Option<String>,
}

/// Ergebnis der glab-Prüfung („Erneut prüfen“).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlabCheck {
    pub program: String,
    pub version: Result<Version, GlabErrorKind>,
    /// Gefundenes Programm (voller Pfad oder PATH-Suche)
    pub path: Option<PathBuf>,
}

/// Ergebnis eines Token-Workers für das offene Formular.
#[derive(Debug)]
pub enum TokenEvent {
    Verified(Result<TokenCheck, GlabErrorKind>),
    Saved(Result<SavedWhere, GlabErrorKind>),
}

/// Zeile der Repo-Auswahl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRow {
    /// Einbezogen = nicht abgewählt (E2), immer bedienbar
    pub checked: bool,
    /// Leer = noch nicht geprüft
    pub status_key: &'static str,
    pub args: Vec<String>,
    /// Einbezogen, aber ohne Zugriff: wird beim Laden übersprungen
    pub warn: bool,
}

/// Zugriff eines Projekts aus Sicht der Repo-Auswahl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowAccess<'a> {
    Checking,
    HostNotSignedIn,
    Known(&'a AccessState),
}

/// Nicht bei glab angemeldeter Remote-Host (E16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnrecognizedHost {
    /// Remote-Host-Schlüssel (`host` bzw. `host:port`)
    pub key: String,
    /// Repos mit einem Remote auf den Host
    pub repos: usize,
    /// Mindestens ein Remote über `http://` (E17)
    pub http: bool,
}

/// Zustand des Tabs im Entwurf (verworfen mit dem `SettingsState`).
#[derive(Default)]
pub struct MrSettingsState {
    /// glab-Prüfung beim Öffnen angefragt (einmal, nicht je Tastendruck im
    /// Programmfeld)
    glab_requested: bool,
    /// Automatisch angefragte Prüfungen angemeldeter Hosts (kleingeschrieben);
    /// später erkannte Hosts kommen dazu, keiner wird wiederholt
    requested: BTreeSet<String>,
    form: Option<TokenForm>,
    /// Eingabe „gehört zu …“ (höchstens eine offen)
    alias: Option<AliasEdit>,
    /// Nach dem Speichern eines Tokens im nächsten Frame neu prüfen
    probe_next: Vec<String>,
}

/// Eingabe „gehört zu …“ eines nicht angemeldeten Remote-Hosts.
struct AliasEdit {
    host: String,
    /// Freitext `host[:port]`
    text: String,
}

/// Phase des Token-Formulars.
enum FormPhase {
    Input,
    Checking,
    Preview(Box<TokenCheck>),
    Saving,
}

/// Formular „Token hinzufügen / ersetzen“ eines Hosts.
struct TokenForm {
    host: String,
    token: String,
    show: bool,
    /// Zweck „Voll“ (api, write_repository) statt „Nur lesen“ (read_api)
    full: bool,
    protocol: Protocol,
    /// Feld-Id je Formularöffnung neu (Undo-Verlauf des Tokens)
    field_id: Id,
    phase: FormPhase,
    error: Option<String>,
}

impl TokenForm {
    fn busy(&self) -> bool {
        matches!(self.phase, FormPhase::Checking | FormPhase::Saving)
    }

    /// Token leeren (Puffer überschrieben) und den `TextEditState` des
    /// Feldes samt Undo-Verlauf entfernen.
    fn reset(&mut self, ctx: &egui::Context) {
        wipe(&mut self.token);
        ctx.data_mut(|d| d.remove::<TextEditState>(self.field_id));
    }
}

impl Drop for TokenForm {
    /// Auch ein verworfener Entwurf hinterlässt das Token nicht im Speicher;
    /// den `TextEditState` räumt dann `forget_token_fields` weg.
    fn drop(&mut self) {
        wipe(&mut self.token);
    }
}

/// Überschreibt den Puffer nach bestem Bemühen (wie `SecretString`).
fn wipe(text: &mut String) {
    let mut bytes = std::mem::take(text).into_bytes();
    bytes.fill(0);
    std::hint::black_box(&bytes);
}

impl MrSettingsState {
    /// Öffnet das Formular für `host` (ein offenes wird vorher geleert).
    fn open_form(&mut self, ctx: &egui::Context, host: &str, protocol: Protocol, full: bool) {
        self.close_form(ctx);
        let field_id = Id::new((
            "mrs_token_field",
            NEXT_TOKEN_FIELD.fetch_add(1, Ordering::Relaxed),
        ));
        register_token_field(ctx, field_id);
        self.form = Some(TokenForm {
            host: host.to_string(),
            token: String::new(),
            show: false,
            full,
            protocol,
            field_id,
            phase: FormPhase::Input,
            error: None,
        });
    }

    /// Schließt das Formular (Speichern, Abbrechen): Token und Undo-Verlauf weg.
    fn close_form(&mut self, ctx: &egui::Context) {
        if let Some(mut form) = self.form.take() {
            form.reset(ctx);
        }
    }

    fn form_open_for(&self, host: &str) -> bool {
        self.form.as_ref().is_some_and(|f| f.host == host)
    }

    fn form_for(&mut self, host: &str) -> Option<&mut TokenForm> {
        self.form.as_mut().filter(|f| f.host == host)
    }

    /// Id des Token-Feldes, solange das Formular offen ist.
    pub(super) fn token_field_id(&self) -> Option<Id> {
        self.form.as_ref().map(|f| f.field_id)
    }

    /// Ergebnis eines Token-Workers; veraltete (Token inzwischen geändert,
    /// Formular geschlossen) verfallen.
    pub(super) fn on_token_event(
        &mut self,
        ctx: &egui::Context,
        lang: Language,
        host: &str,
        event: TokenEvent,
    ) {
        // Nur das Formular, das gespeichert hat, nicht ein nach „Abbrechen“
        // für denselben Host neu geöffnetes
        let saving = self
            .form_for(host)
            .is_some_and(|f| matches!(f.phase, FormPhase::Saving));
        match event {
            TokenEvent::Saved(Ok(_)) => {
                if saving {
                    self.close_form(ctx);
                }
                // Karte mit der neuen Anmeldung neu prüfen
                self.probe_next.push(host.to_string());
            }
            TokenEvent::Saved(Err(kind)) => {
                if let Some(form) = self.form_for(host).filter(|_| saving) {
                    form.phase = FormPhase::Input;
                    let reason = error_reason(lang, &kind, host);
                    form.error = Some(tr_fmt(lang, "mrs_token_failed", &[&reason]));
                }
            }
            TokenEvent::Verified(result) => {
                let Some(form) = self.form_for(host) else {
                    return;
                };
                if !matches!(form.phase, FormPhase::Checking) {
                    return;
                }
                match result {
                    Ok(check) => form.phase = FormPhase::Preview(Box::new(check)),
                    Err(kind) => {
                        form.phase = FormPhase::Input;
                        form.error = Some(verify_error(lang, &kind, host));
                    }
                }
            }
        }
    }
}

// --- Token-Felder im egui-Speicher ------------------------------------------

/// Schlüssel der Liste aller Token-Feld-Ids der Sitzung (temporär).
fn token_fields_key() -> Id {
    Id::new("mrs_token_fields")
}

fn register_token_field(ctx: &egui::Context, id: Id) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<Id>>(token_fields_key())
            .push(id);
    });
}

/// Entfernt den `TextEditState` (Undo-Verlauf mit dem Token) aller
/// Token-Felder außer `active` – auch wenn die App den Entwurf ohne
/// „Abbrechen“ verworfen hat (Schließen, „Speichern“). Aus `logic()`.
pub fn forget_token_fields(ctx: &egui::Context, active: Option<Id>) {
    ctx.data_mut(|d| {
        let ids: Vec<Id> = d.get_temp(token_fields_key()).unwrap_or_default();
        if ids.iter().all(|id| Some(*id) == active) {
            return;
        }
        let (keep, stale): (Vec<Id>, Vec<Id>) = ids.into_iter().partition(|id| Some(*id) == active);
        for id in stale {
            d.remove::<TextEditState>(id);
        }
        d.insert_temp(token_fields_key(), keep);
    });
}

// --- Reine Hilfen -------------------------------------------------------------

/// glab-Programm des Entwurfs wie nach `MrConfig::normalize`: getrimmt,
/// leer = „glab“.
pub fn effective_glab_program(program: &str) -> String {
    let program = program.trim();
    if program.is_empty() {
        "glab".to_string()
    } else {
        program.to_string()
    }
}

/// Programm der Sitzung, solange der Entwurf ein anderes hat: Prüfungen
/// der Karten und Token laufen bis „Speichern“ damit.
fn pending_glab_program(draft: &str, session: &str) -> Option<String> {
    let session = effective_glab_program(session);
    (effective_glab_program(draft) != session).then_some(session)
}

/// Ein bloßer Programmname ohne Shell-Metazeichen; ein Pfad (mit `/` oder
/// `\`) darf sie enthalten (Regel wie `git::is_program_available`).
pub fn valid_glab_program(program: &str) -> bool {
    let program = program.trim();
    program.contains(['/', '\\']) || !program.chars().any(|c| SHELL_META.contains(c))
}

/// Schließt `key` (`host/full/path`) aus bzw. bezieht es wieder ein (E2).
pub fn set_project_included(excluded: &mut Vec<String>, key: &str, included: bool) {
    set_projects_included(excluded, &[key.to_string()], included);
}

/// „Alle“ / „Keine“: nur die angezeigten Projekte; die Liste bleibt klein,
/// sortiert und ohne Dubletten (wie `MrConfig::normalize`).
pub fn set_projects_included(excluded: &mut Vec<String>, keys: &[String], included: bool) {
    let norm = |k: &String| k.trim().to_lowercase();
    let keys: BTreeSet<String> = keys.iter().map(norm).filter(|k| !k.is_empty()).collect();
    let mut list: BTreeSet<String> = excluded
        .iter()
        .map(norm)
        .filter(|k| !k.is_empty())
        .collect();
    if included {
        list.retain(|k| !keys.contains(k));
    } else {
        list.extend(keys);
    }
    *excluded = list.into_iter().collect();
}

/// Status-Chip einer Host-Karte: (i18n-Schlüssel, Argumente).
pub fn host_chip(probe: &HostProbe) -> (&'static str, Vec<String>) {
    match &probe.error {
        Some(kind) => error_chip(kind),
        None => match probe.rights_gitmanager {
            GitManagerRights::Ok => ("mrs_state_ready", Vec::new()),
            GitManagerRights::Missing => ("mrs_state_rights_missing", Vec::new()),
            GitManagerRights::Unknown => ("mr_err_unknown", Vec::new()),
        },
    }
}

/// Status zu einem Prüffehler.
fn error_chip(kind: &GlabErrorKind) -> (&'static str, Vec<String>) {
    let key = match kind {
        GlabErrorKind::NotAuthenticated | GlabErrorKind::InvalidToken => "mrs_state_not_signed",
        GlabErrorKind::Expired => "mrs_state_expired",
        GlabErrorKind::Revoked => "mrs_state_revoked",
        GlabErrorKind::InsufficientScope => "mrs_state_rights_missing",
        GlabErrorKind::Network | GlabErrorKind::Timeout => "mrs_state_unreachable",
        GlabErrorKind::Tls => "mrs_state_tls",
        GlabErrorKind::NotInstalled => "mrs_glab_not_found",
        GlabErrorKind::TooOld(version) => return ("mr_glab_too_old", vec![version.to_string()]),
        GlabErrorKind::MissingContentType => "mr_err_content_type",
        GlabErrorKind::HostKey
        | GlabErrorKind::NotFound
        | GlabErrorKind::GraphQl(_)
        | GlabErrorKind::Other => "mr_err_unknown",
    };
    (key, Vec::new())
}

/// Farbton eines Status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Ok,
    Warn,
    Fail,
    Neutral,
}

fn chip_tone(key: &str) -> Tone {
    match key {
        "mrs_state_ready" => Tone::Ok,
        "mrs_state_checking" => Tone::Neutral,
        "mrs_state_unreachable"
        | "mrs_state_tls"
        | "mr_glab_too_old"
        | "mr_err_content_type"
        | "mr_err_unknown" => Tone::Warn,
        _ => Tone::Fail,
    }
}

fn tone_color(p: &MrPalette, tone: Tone) -> Color32 {
    match tone {
        Tone::Ok => p.ok_text,
        Tone::Warn => p.warn_text,
        Tone::Fail => p.fail_text,
        Tone::Neutral => p.text2,
    }
}

/// Kurztext eines Fehlers beim Prüfen oder Speichern (Formular,
/// Statusmeldung).
pub fn error_reason(lang: Language, kind: &GlabErrorKind, host: &str) -> String {
    if let Some(text) = error_text(kind, host, lang) {
        return text;
    }
    let (key, args) = error_chip(kind);
    fmt(lang, key, &args)
}

/// Fehler der Prüfung eines neuen Tokens: „nicht angemeldet“ heißt hier,
/// dass GitLab das Token nicht annimmt.
pub fn verify_error(lang: Language, kind: &GlabErrorKind, host: &str) -> String {
    match kind {
        GlabErrorKind::NotAuthenticated | GlabErrorKind::InvalidToken => {
            tr(lang, "mrs_token_invalid")
        }
        _ => error_reason(lang, kind, host),
    }
}

/// Zeile der Repo-Auswahl aus Zugriff und Abwahl.
pub fn repo_row_state(access: RowAccess, excluded: bool) -> RepoRow {
    let checked = !excluded;
    let plain = |key| (key, Vec::new(), false);
    let (status_key, args, warn) = match access {
        RowAccess::Checking => plain("mrs_acc_checking"),
        RowAccess::HostNotSignedIn => plain("mrs_acc_host_auth"),
        RowAccess::Known(state) => match state {
            AccessState::Ok { level } => match role_name(*level) {
                Some(role) => ("mrs_acc_ok", vec![role.to_string()], false),
                None => plain("mrs_acc_ok_plain"),
            },
            AccessState::Public => plain("mrs_acc_public"),
            AccessState::Renamed { to } => ("mrs_acc_renamed", vec![to.clone()], false),
            AccessState::NoAccess if checked => ("mrs_acc_skipped", Vec::new(), true),
            AccessState::NoAccess => plain("mrs_acc_denied"),
            AccessState::MrsDisabled => plain("mrs_acc_mr_off"),
            AccessState::Archived => plain("mrs_acc_archived"),
            AccessState::Unknown => plain(""),
        },
    };
    RepoRow {
        checked,
        status_key,
        args,
        warn,
    }
}

/// GitLab-Rolle zur Zugriffsstufe (Begriffe wie in GitLab).
fn role_name(level: u32) -> Option<&'static str> {
    Some(match level {
        10 => "Guest",
        15 => "Planner",
        20 => "Reporter",
        30 => "Developer",
        40 => "Maintainer",
        50 => "Owner",
        _ => return None,
    })
}

/// Aus dem Projekt werden Merge Requests geladen („zugreifbar“): Mitglied,
/// öffentlich oder archiviert (lesbar). Ein umbenanntes Projekt findet der
/// Abruf unter dem alten Pfad nicht.
fn accessible(state: &AccessState) -> bool {
    matches!(
        state,
        AccessState::Ok { .. } | AccessState::Public | AccessState::Archived
    )
}

/// Alle geparsten Remotes der Repos.
fn remote_refs(repos: &[RepoInfo]) -> impl Iterator<Item = &RemoteRef> {
    repos
        .iter()
        .flat_map(|r| r.remotes.iter().filter_map(|x| x.parsed.as_ref()))
}

/// Zuordnung mit dem Entwurf (Hosts, „gehört zu …“, Auswahl).
fn draft_hosts(repos: &[RepoInfo], glab_hosts: &[GlabHost], cfg: &MrConfig) -> HostMap {
    resolve_hosts(remote_refs(repos), glab_hosts, cfg)
}

/// Projektpfade eines Hosts für Prüfung und Token-Vorschau: Remotes, die der
/// Entwurf dem Host zuordnet, und Remotes mit genau diesem Host (Token für
/// einen noch nicht angemeldeten Remote-Host). Sortiert, ohne Dubletten.
pub fn project_paths_for_host(
    repos: &[RepoInfo],
    glab_hosts: &[GlabHost],
    cfg: &MrConfig,
    host: &str,
) -> Vec<String> {
    let hosts = draft_hosts(repos, glab_hosts, cfg);
    let paths: BTreeSet<String> = remote_refs(repos)
        .filter(|r| serves(&hosts, r, host))
        .map(|r| r.full_path.clone())
        .collect();
    paths.into_iter().collect()
}

/// Gehört der Remote zu `host` (zugeordnet oder derselbe Remote-Host)?
fn serves(hosts: &HostMap, remote: &RemoteRef, host: &str) -> bool {
    remote.host_key().eq_ignore_ascii_case(host)
        || hosts
            .glab_key(remote)
            .is_some_and(|k| k.eq_ignore_ascii_case(host))
}

/// Hosts, deren lokale Erkennung nach einer Änderung im Entwurf („Als
/// GitLab-Host hinzufügen“, „gehört zu …“) neu läuft.
pub fn rediscover_candidates(cfg: &MrConfig) -> Vec<String> {
    let hosts: BTreeSet<String> = cfg
        .extra_hosts
        .iter()
        .chain(cfg.host_aliases.iter().map(|a| &a.to))
        .map(|h| h.trim().to_lowercase())
        .filter(|h| !h.is_empty())
        .collect();
    hosts.into_iter().collect()
}

/// Karten im Abschnitt „Anmeldung & Token“: GitLab-Hosts und Ziele von
/// „gehört zu …“, auch nicht angemeldete (dort lässt sich ein Token
/// hinzufügen; Netz-Aufrufe erst nach einer Aktion), sortiert.
fn card_hosts(hosts: &HostMap, cfg: &MrConfig) -> Vec<String> {
    let cards: BTreeSet<String> = hosts
        .gitlab_hosts
        .iter()
        .cloned()
        .chain(cfg.host_aliases.iter().map(|a| a.to.trim().to_lowercase()))
        .filter(|h| !h.is_empty())
        .collect();
    cards.into_iter().collect()
}

/// Ziel aus der Eingabe „gehört zu …“: `host[:port]` ohne Schema und `/`
/// am Ende, kleingeschrieben. `None` bei leer, Pfad, Userinfo, Leerzeichen
/// oder dem Host selbst.
pub fn alias_target(text: &str, from: &str) -> Option<String> {
    let text = text.trim().to_lowercase();
    let host = ["https://", "http://"]
        .iter()
        .find_map(|scheme| text.strip_prefix(scheme))
        .unwrap_or(&text)
        .trim_end_matches('/');
    let bad = |c: char| c.is_whitespace() || c.is_control() || "/\\@?#".contains(c);
    let valid = !host.is_empty()
        && !host.starts_with(':')
        && !host.ends_with(':')
        && !host.chars().any(bad)
        && host != from.trim().to_lowercase();
    valid.then(|| host.to_string())
}

/// Remote-Hosts ohne GitLab-Zuordnung (E16) mit Repo-Anzahl, sortiert.
/// Hosts mit „gehört zu …“ stehen auf der Karte ihres Ziels (dort wieder
/// entfernbar), auch solange das Ziel nicht angemeldet ist.
fn unrecognized_hosts(
    repos: &[RepoInfo],
    hosts: &HostMap,
    cfg: &MrConfig,
) -> Vec<UnrecognizedHost> {
    let aliased = |remote: &RemoteRef, key: &str| {
        cfg.host_aliases.iter().any(|a| {
            let from = a.from.trim();
            from.eq_ignore_ascii_case(key) || from.eq_ignore_ascii_case(&remote.host)
        })
    };
    let mut found: BTreeMap<String, (BTreeSet<&Path>, bool)> = BTreeMap::new();
    for repo in repos {
        for remote in repo.remotes.iter().filter_map(|r| r.parsed.as_ref()) {
            let key = remote.host_key();
            if hosts.remotes.get(&key) != Some(&HostTarget::NotGitLab) || aliased(remote, &key) {
                continue;
            }
            let entry = found.entry(key).or_default();
            entry.0.insert(&repo.path);
            entry.1 |= remote.scheme == RemoteScheme::Http;
        }
    }
    found
        .into_iter()
        .map(|(key, (repos, http))| UnrecognizedHost {
            key,
            repos: repos.len(),
            http,
        })
        .collect()
}

/// API-Protokoll eines Hosts (E17): Einstellung, glab-Config, sonst aus dem
/// Remote-Schema (`http://` → HTTP).
fn host_protocol(
    host: &str,
    repos: &[RepoInfo],
    hosts: &HostMap,
    glab_hosts: &[GlabHost],
    cfg: &MrConfig,
) -> Protocol {
    let parse = |p: &str| {
        if p.trim().eq_ignore_ascii_case("http") {
            Protocol::Http
        } else {
            Protocol::Https
        }
    };
    if let Some(p) = cfg
        .api_protocols
        .iter()
        .find(|p| p.host.trim().eq_ignore_ascii_case(host))
    {
        return parse(&p.protocol);
    }
    if let Some(p) = glab_hosts
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case(host))
        .and_then(|h| h.api_protocol.as_deref())
    {
        return parse(p);
    }
    let http = remote_refs(repos).any(|r| r.scheme == RemoteScheme::Http && serves(hosts, r, host));
    if http {
        Protocol::Http
    } else {
        Protocol::Https
    }
}

/// Vorbelegung „Zweck“ (E4): Voll, wenn die bisherige Anmeldung `api` hat
/// (OAuth über glab zählt als Voll), sonst Nur lesen.
fn full_purpose_default(probe: Option<&HostProbe>) -> bool {
    probe.is_some_and(|p| p.error.is_none() && p.rights_glab == GlabRights::Full)
}

/// Lokale Prüfung vor `verify_token`: i18n-Schlüssel des Problems.
pub fn token_problem(token: &str) -> Option<&'static str> {
    if token.is_empty() {
        Some("mrs_token_empty")
    } else if token.chars().any(char::is_whitespace) {
        Some("mrs_token_whitespace")
    } else {
        None
    }
}

/// Vorschau „@user · read_api · läuft ab … · {x}/{y} Repos zugreifbar“.
fn preview_text(lang: Language, check: &TokenCheck) -> String {
    let total = check.access_preview.len().to_string();
    let reachable = check
        .access_preview
        .iter()
        .filter(|a| accessible(&a.state))
        .count()
        .to_string();
    let user = check.user.as_deref().unwrap_or("?");
    let scopes = if check.scopes.is_empty() {
        "–".to_string()
    } else {
        check.scopes.join(", ")
    };
    match check.expires_at {
        Some(ts) => tr_fmt(
            lang,
            "mrs_preview",
            &[user, &scopes, &format_date(ts), &reachable, &total],
        ),
        None => tr_fmt(
            lang,
            "mrs_preview_no_expiry",
            &[user, &scopes, &reachable, &total],
        ),
    }
}

/// „Läuft in {n} Tagen ab“ ab 14 Tagen vor dem Ablauf; abgelaufen meldet
/// die Prüfung selbst.
fn expiry_warning(lang: Language, expires_at: Option<i64>, now: i64) -> Option<String> {
    let left = expires_at? - now;
    if left <= 0 {
        return None;
    }
    let days = (left + DAY - 1) / DAY;
    match days {
        1 => Some(tr(lang, "mrs_expires_soon_one")),
        d if d <= EXPIRY_WARN_DAYS => Some(tr_fmt(lang, "mrs_expires_soon", &[&d.to_string()])),
        _ => None,
    }
}

fn token_kind_text(lang: Language, kind: TokenKind) -> Option<String> {
    let key = match kind {
        TokenKind::Pat => "mrs_kind_pat",
        TokenKind::Oauth => "mrs_kind_oauth",
        TokenKind::ProjectBot => "mrs_kind_bot",
        TokenKind::Granular => "mrs_kind_granular",
        // steht schon als Speicherort da
        TokenKind::Env(_) | TokenKind::Unknown => return None,
    };
    Some(tr(lang, key))
}

fn token_source_text(lang: Language, source: TokenSource) -> Option<String> {
    Some(match source {
        TokenSource::Keyring => tr(lang, "mrs_src_keyring"),
        TokenSource::ConfigFile => tr(lang, "mrs_src_config"),
        TokenSource::Env(var) => tr_fmt(lang, "mrs_src_env", &[var]),
        TokenSource::Unknown => return None,
    })
}

/// Suchpfad, unter dem der Scan neue Worktrees `{review_root}\{repo}-mr-{iid}`
/// noch findet (Scan-Tiefe beachtet). Pfade wie unter Windows verglichen:
/// `/` = `\`, ohne Groß-/Kleinschreibung, ohne Trenner am Ende.
pub fn review_root_hit<'r>(
    review_root: &str,
    roots: &'r [PathBuf],
    max_depth: usize,
) -> Option<&'r PathBuf> {
    let dir = windows_key(review_root);
    if dir.is_empty() {
        return None;
    }
    roots.iter().find(|root| {
        let root = windows_key(&root.to_string_lossy());
        if root.is_empty() {
            return false;
        }
        let rest = if dir == root {
            Some("")
        } else {
            dir.strip_prefix(&root).and_then(|r| r.strip_prefix('\\'))
        };
        // Worktree-Ordner liegt eine Ebene unter dem Review-Ordner
        rest.is_some_and(|rest| rest.split('\\').filter(|s| !s.is_empty()).count() < max_depth)
    })
}

fn windows_key(path: &str) -> String {
    path.trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// „Als Suchpfad hinzufügen“: Review-Ordner als Suchpfad in den Entwurf.
pub fn add_review_root_as_search_path(draft: &mut AppConfig) {
    let root = draft
        .merge_requests
        .review_root
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(PathBuf::from);
    if let Some(root) = root {
        draft.add_root(root);
    }
}

/// „Als GitLab-Host hinzufügen“ (`extra_hosts`); `false`, wenn schon da.
fn add_extra_host(cfg: &mut MrConfig, host: &str) -> bool {
    let host = host.trim().to_lowercase();
    if host.is_empty()
        || cfg
            .extra_hosts
            .iter()
            .any(|h| h.trim().eq_ignore_ascii_case(&host))
    {
        return false;
    }
    cfg.extra_hosts.push(host);
    cfg.extra_hosts.sort();
    true
}

/// „gehört zu …“ (`host_aliases`): ersetzt eine Zuordnung desselben Hosts;
/// `false`, wenn sie schon so besteht.
pub fn set_alias(cfg: &mut MrConfig, from: &str, to: &str) -> bool {
    let from = from.trim().to_lowercase();
    let to = to.trim().to_lowercase();
    let same = |a: &HostAlias| a.from.trim().eq_ignore_ascii_case(&from);
    if cfg
        .host_aliases
        .iter()
        .any(|a| same(a) && a.to.trim().eq_ignore_ascii_case(&to))
    {
        return false;
    }
    cfg.host_aliases.retain(|a| !same(a));
    cfg.host_aliases.push(HostAlias { from, to });
    cfg.host_aliases.sort_by(|a, b| a.from.cmp(&b.from));
    true
}

/// Letzter Pfadteil eines Projekts („backend-api“).
fn project_name(full_path: &str) -> &str {
    full_path.rsplit('/').next().unwrap_or(full_path)
}

/// Erste sichtbare IDE des aktiven Profils (wie „Danach öffnen“).
fn profile_ide(cfg: &AppConfig) -> Option<&IdeConfig> {
    cfg.get_profile(&cfg.active_profile_id)
        .or_else(|| cfg.profiles.first())?
        .visible_ides()
        .into_iter()
        .next()
}

fn after_action_label(lang: Language, cfg: &AppConfig, action: &AfterAction) -> String {
    match action {
        AfterAction::Agent(id) => cfg
            .agents
            .iter()
            .find(|a| &a.id == id)
            .map_or_else(|| id.clone(), |a| a.display_name.clone()),
        AfterAction::Shell => tr(lang, "mr_after_shell"),
        AfterAction::Explorer => tr(lang, "mr_after_explorer"),
        AfterAction::Ide => {
            profile_ide(cfg).map_or_else(|| "IDE".to_string(), |i| i.display_name.clone())
        }
        AfterAction::Nothing => tr(lang, "mr_after_nothing"),
    }
}

/// „Nach dem Auschecken öffnen“: Zuletzt gewählt / aktive Agenten / Shell /
/// Explorer / IDE / Nichts.
fn after_options(lang: Language, cfg: &AppConfig) -> Vec<(AfterCheckoutPref, String)> {
    let mut actions: Vec<AfterAction> = cfg
        .get_active_agents()
        .iter()
        .map(|a| AfterAction::Agent(a.id.clone()))
        .collect();
    actions.extend([AfterAction::Shell, AfterAction::Explorer]);
    if profile_ide(cfg).is_some() {
        actions.push(AfterAction::Ide);
    }
    actions.push(AfterAction::Nothing);
    std::iter::once((AfterCheckoutPref::LastUsed, tr(lang, "mrs_after_last")))
        .chain(actions.into_iter().map(|a| {
            let label = after_action_label(lang, cfg, &a);
            (AfterCheckoutPref::Fixed(a), label)
        }))
        .collect()
}

fn fmt(lang: Language, key: &str, args: &[String]) -> String {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    tr_fmt(lang, key, &args)
}

fn count_text(lang: Language, n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        tr(lang, one)
    } else {
        tr_fmt(lang, many, &[&n.to_string()])
    }
}

// --- Tab ----------------------------------------------------------------------

/// Daten eines Frames; Zuordnung und Projekte mit dem Entwurf.
struct Tab<'t> {
    lang: Language,
    p: MrPalette,
    input: &'t MrSettingsInput<'t>,
    hosts: HostMap,
    locals: Vec<LocalProject>,
    /// Host-Karten (`card_hosts`)
    cards: Vec<String>,
    /// Nicht bei glab angemeldete Remote-Hosts
    unrecognized: Vec<UnrecognizedHost>,
}

impl Tab<'_> {
    fn signed_in(&self, host: &str) -> bool {
        self.input
            .glab_hosts
            .iter()
            .any(|h| h.key.eq_ignore_ascii_case(host) && !h.user.trim().is_empty())
    }

    fn probe(&self, host: &str) -> Option<&HostProbe> {
        self.input.probes.get(host).or_else(|| {
            self.input
                .probes
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(host))
                .map(|(_, p)| p)
        })
    }

    fn probing(&self, host: &str) -> bool {
        self.input
            .probing
            .iter()
            .any(|h| h.eq_ignore_ascii_case(host))
    }

    /// Status-Chip der Host-Karte; `None` = angemeldet, noch nicht geprüft.
    fn card_status(&self, host: &str) -> Option<(&'static str, Vec<String>)> {
        if self.probing(host) {
            return Some(("mrs_state_checking", Vec::new()));
        }
        if let Some(probe) = self.probe(host) {
            return Some(host_chip(probe));
        }
        (!self.signed_in(host)).then(|| ("mrs_state_not_signed", Vec::new()))
    }

    fn row_access(&self, project: &LocalProject) -> RowAccess<'_> {
        let host = &project.host_key;
        if self.probing(host) {
            return RowAccess::Checking;
        }
        match self.probe(host) {
            Some(probe) if probe.error.as_ref().is_some_and(is_auth_error) => {
                RowAccess::HostNotSignedIn
            }
            Some(probe) => probe
                .projects
                .iter()
                .find(|a| a.full_path.eq_ignore_ascii_case(&project.full_path))
                .map_or(RowAccess::Known(&UNKNOWN_ACCESS), |a| {
                    RowAccess::Known(&a.state)
                }),
            None if !self.signed_in(host) => RowAccess::HostNotSignedIn,
            None => RowAccess::Known(&UNKNOWN_ACCESS),
        }
    }

    /// Erster Frame: glab prüfen. Jeder Frame: angemeldete Hosts ohne
    /// Ergebnis der Sitzung, je Host einmal; so auch Hosts, die die
    /// Erkennung erst nach dem Öffnen findet (E16: nur angemeldete Hosts
    /// ohne ausdrückliche Aktion).
    fn auto_start(&self, mr: &mut MrSettingsState, cfg: &MrConfig, out: &mut MrSettingsOutput) {
        if !mr.glab_requested {
            mr.glab_requested = true;
            let program = effective_glab_program(&cfg.glab_program);
            let checked = self.input.glab_check.is_some_and(|c| c.program == program);
            if !checked && !self.input.glab_checking && valid_glab_program(&program) {
                out.start_glab_check = Some(program);
            }
        }
        for host in &self.cards {
            let pending = self.probe(host).is_none() && !self.probing(host);
            if self.signed_in(host)
                && pending
                && mr.requested.insert(host.to_lowercase())
                && !out.start_probe.contains(host)
            {
                out.start_probe.push(host.clone());
            }
        }
    }

    /// Angebotene Ziele für „gehört zu …“: Host-Karten und bei glab
    /// angemeldete Hosts, ohne den Host selbst.
    fn alias_choices(&self, from: &str) -> Vec<String> {
        let signed = self
            .input
            .glab_hosts
            .iter()
            .filter(|h| !h.user.trim().is_empty())
            .map(|h| h.key.trim().to_lowercase());
        let choices: BTreeSet<String> = self
            .cards
            .iter()
            .cloned()
            .chain(signed)
            .filter(|h| !h.is_empty() && !h.eq_ignore_ascii_case(from))
            .collect();
        choices.into_iter().collect()
    }

    /// Hat der Host in diesem Frame eine Karte oder eine Zeile?
    fn shows_host(&self, host: &str) -> bool {
        self.cards.iter().any(|h| h.eq_ignore_ascii_case(host))
            || self
                .unrecognized
                .iter()
                .any(|u| u.key.eq_ignore_ascii_case(host))
    }
}

/// Zeichnet den Tab (Abschnitte 1–5).
pub(super) fn show_merge_requests_tab(
    ui: &mut Ui,
    state: &mut SettingsState,
    input: &MrSettingsInput,
    out: &mut MrSettingsOutput,
) {
    let cfg = &state.draft.merge_requests;
    let hosts = draft_hosts(input.repos, input.glab_hosts, cfg);
    let locals = local_projects(input.repos, &hosts, cfg);
    let cards = card_hosts(&hosts, cfg);
    let unrecognized = unrecognized_hosts(input.repos, &hosts, cfg);
    let tab = Tab {
        lang: state.draft.language,
        p: mr_palette(input.theme, ui.visuals()),
        input,
        hosts,
        locals,
        cards,
        unrecognized,
    };
    out.start_probe.append(&mut state.mr.probe_next);
    tab.auto_start(&mut state.mr, &state.draft.merge_requests, out);
    cli_section(ui, &tab, &mut state.draft.merge_requests, out);
    section_gap(ui);
    auth_section(
        ui,
        &tab,
        &mut state.draft.merge_requests,
        &mut state.mr,
        out,
    );
    section_gap(ui);
    repos_section(ui, &tab, &mut state.draft.merge_requests, out);
    section_gap(ui);
    checkout_section(ui, &tab, &mut state.draft);
    section_gap(ui);
    display_section(ui, &tab, &mut state.draft.merge_requests);
}

// --- 1. GitLab CLI ------------------------------------------------------------

fn cli_section(ui: &mut Ui, t: &Tab, cfg: &mut MrConfig, out: &mut MrSettingsOutput) {
    let lang = t.lang;
    section_title(ui, &tr(lang, "mrs_sec_cli"));
    egui::Grid::new("mrs_cli_grid")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label(tr(lang, "mrs_glab_program"));
            ui.horizontal(|ui| {
                ui.add(
                    TextEdit::singleline(&mut cfg.glab_program)
                        .font(FontId::monospace(12.0))
                        .desired_width(320.0),
                );
                if ui.button(browse_label(lang)).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        cfg.glab_program = path.display().to_string();
                    }
                }
            });
            ui.end_row();
            ui.label("");
            glab_status(ui, t, &effective_glab_program(&cfg.glab_program), out);
            ui.end_row();
        });
}

/// „glab {v} gefunden · {Pfad}“ / „nicht gefunden“ (+ winget) / „zu alt“.
fn glab_status(ui: &mut Ui, t: &Tab, program: &str, out: &mut MrSettingsOutput) {
    let (lang, p) = (t.lang, &t.p);
    ui.vertical(|ui| {
        if !valid_glab_program(program) {
            status_line(
                ui,
                ICON_X_CIRCLE,
                p.fail_text,
                &tr(lang, "mrs_glab_invalid"),
            );
            return;
        }
        let check = t.input.glab_check.filter(|c| c.program == program);
        let checking = t.input.glab_checking;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if checking {
                ui.add(Spinner::new().size(14.0).color(p.run_icon));
                small(ui, &tr(lang, "mrs_state_checking"), p.text2);
            } else if let Some(check) = check {
                let (icon, color, text) = match &check.version {
                    Ok(version) => {
                        let path = check
                            .path
                            .as_ref()
                            .map_or_else(|| program.to_string(), |p| p.display().to_string());
                        let text = tr_fmt(lang, "mrs_glab_found", &[&version.to_string(), &path]);
                        (ICON_CHECK_CIRCLE, p.ok_text, text)
                    }
                    Err(kind @ GlabErrorKind::TooOld(_)) => {
                        let (key, args) = error_chip(kind);
                        (ICON_WARNING, p.warn_text, fmt(lang, key, &args))
                    }
                    Err(GlabErrorKind::NotInstalled) => {
                        (ICON_X_CIRCLE, p.fail_text, tr(lang, "mrs_glab_not_found"))
                    }
                    Err(kind) => (
                        ICON_X_CIRCLE,
                        p.fail_text,
                        error_reason(lang, kind, program),
                    ),
                };
                ui.add(icon_image(icon, 14.0, color));
                ui.add(Label::new(RichText::new(text).size(12.0).color(color)).wrap());
            }
            let label = if check.is_some() {
                "mrs_recheck"
            } else {
                "mrs_check"
            };
            if ui
                .add_enabled(!checking, Button::new(tr(lang, label)))
                .clicked()
            {
                out.start_glab_check = Some(program.to_string());
            }
        });
        if let Some(session) = pending_glab_program(program, t.input.glab_program) {
            let text = tr_fmt(lang, "mrs_glab_pending", &[&session]);
            status_line(ui, ICON_WARNING, p.warn_text, &text);
        }
        match check.map(|c| &c.version) {
            Some(Err(GlabErrorKind::NotInstalled)) => {
                command_line(ui, t, "winget install GLab.GLab");
                wrapped(ui, &tr(lang, "mr_glab_restart_hint"), 11.0, p.text2);
            }
            Some(Err(GlabErrorKind::TooOld(_))) => {
                command_line(ui, t, "winget upgrade GLab.GLab");
            }
            _ => {}
        }
    });
}

// --- 2. Anmeldung & Token -----------------------------------------------------

fn auth_section(
    ui: &mut Ui,
    t: &Tab,
    cfg: &mut MrConfig,
    mr: &mut MrSettingsState,
    out: &mut MrSettingsOutput,
) {
    let lang = t.lang;
    section_title(ui, &tr(lang, "mrs_sec_auth"));
    if let Some(var) = t.input.env_token {
        notice(ui, t, &tr_fmt(lang, "mrs_env_banner", &[var]));
        ui.add_space(8.0);
    }
    if t.hosts.gitlab_hosts.is_empty() {
        wrapped(ui, &tr(lang, "mr_no_gitlab_host"), 12.0, t.p.text2);
        ui.add_space(6.0);
    }
    // Formular eines nicht mehr gezeigten Hosts (nach „gehört zu …“, einem
    // Scan, …) schließen: Token und Undo-Verlauf weg (E3)
    if let Some(host) = mr.form.as_ref().map(|f| f.host.clone()) {
        if !t.shows_host(&host) {
            mr.close_form(ui.ctx());
        }
    }
    let listed = |host: &str| t.unrecognized.iter().any(|u| u.key == host);
    if mr.alias.as_ref().is_some_and(|a| !listed(&a.host)) {
        mr.alias = None;
    }
    let mut edit = None;
    for host in &t.cards {
        host_card(ui, t, cfg, mr, host, out, &mut edit);
        ui.add_space(8.0);
    }
    match edit {
        Some(CardEdit::RemoveHost(host)) => {
            cfg.extra_hosts
                .retain(|h| !h.trim().eq_ignore_ascii_case(&host));
        }
        Some(CardEdit::RemoveAlias(from)) => {
            cfg.host_aliases
                .retain(|a| !a.from.trim().eq_ignore_ascii_case(&from));
        }
        None => {}
    }
    unrecognized_section(ui, t, cfg, mr, out);
}

/// Änderung aus einer Host-Karte (nach dem Zeichnen angewandt).
enum CardEdit {
    /// Eingetragenen GitLab-Host entfernen
    RemoveHost(String),
    /// Zuordnung „gehört zu …“ dieses Hosts entfernen
    RemoveAlias(String),
}

/// Karte je GitLab-Host: Status, Anmeldung, Urteile (E4), Scopes, Formular.
fn host_card(
    ui: &mut Ui,
    t: &Tab,
    cfg: &MrConfig,
    mr: &mut MrSettingsState,
    host: &str,
    out: &mut MrSettingsOutput,
    edit: &mut Option<CardEdit>,
) {
    let lang = t.lang;
    let probe = t.probe(host);
    let protocol = host_protocol(host, t.input.repos, &t.hosts, t.input.glab_hosts, cfg);
    let extra = cfg
        .extra_hosts
        .iter()
        .any(|h| h.trim().eq_ignore_ascii_case(host));
    let signed = t.signed_in(host) || probe.is_some_and(|p| p.logged_in_as.is_some());
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(host)
                    .font(FontId::monospace(12.0))
                    .color(t.p.title),
            );
            if protocol == Protocol::Http {
                http_chip(ui, t);
            }
            if let Some((key, args)) = t.card_status(host) {
                status_chip(ui, t, key, &args);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if extra && icon_button(ui, ICON_TRASH, &tr(lang, "remove")).clicked() {
                    *edit = Some(CardEdit::RemoveHost(host.to_string()));
                }
                let open = mr.form_open_for(host);
                let key = if signed {
                    "mrs_replace_token"
                } else {
                    "mrs_add_token"
                };
                if ui.add(Button::new(tr(lang, key)).selected(open)).clicked() {
                    if open {
                        mr.close_form(ui.ctx());
                    } else {
                        mr.open_form(ui.ctx(), host, protocol, full_purpose_default(probe));
                    }
                }
                if ui
                    .add_enabled(!t.probing(host), Button::new(tr(lang, "mrs_check")))
                    .clicked()
                {
                    out.start_probe.push(host.to_string());
                }
            });
        });
        // Andere Namen dieses Hosts („gehört zu …“), einzeln entfernbar
        let aliases = cfg
            .host_aliases
            .iter()
            .filter(|a| a.to.trim().eq_ignore_ascii_case(host));
        for alias in aliases {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let text = tr_fmt(lang, "mrs_alias_of", &[&alias.from]);
                small(ui, &text, t.p.text2);
                let tooltip = format!("{}: {}", tr(lang, "mrs_alias_remove"), alias.from);
                if icon_button(ui, ICON_TRASH, &tooltip).clicked() {
                    *edit = Some(CardEdit::RemoveAlias(alias.from.clone()));
                }
            });
        }
        let mut close = false;
        if let Some(form) = mr.form_for(host) {
            token_form(ui, t, form, out, &mut close);
        } else if let Some(probe) = probe.filter(|p| p.error.is_none()) {
            probe_details(ui, t, probe);
        }
        if close {
            mr.close_form(ui.ctx());
        }
    });
}

/// „Angemeldet als …“, Ablauf, Urteile GitManager und glab (E4), Scopes.
fn probe_details(ui: &mut Ui, t: &Tab, probe: &HostProbe) {
    let (lang, p) = (t.lang, &t.p);
    ui.add_space(4.0);
    let mut parts = Vec::new();
    if let Some(user) = &probe.logged_in_as {
        let name = probe.display_name.as_deref().unwrap_or(user);
        parts.push(tr_fmt(lang, "mrs_signed_in_as", &[name, user]));
    }
    if let Some(kind) = token_kind_text(lang, probe.token_kind) {
        parts.push(tr_fmt(lang, "mrs_token_kind", &[&kind]));
    }
    if let Some(source) = token_source_text(lang, probe.token_source) {
        parts.push(tr_fmt(lang, "mrs_token_source", &[&source]));
    }
    if let Some(ts) = probe.expires_at {
        parts.push(tr_fmt(lang, "mrs_expires", &[&format_date(ts)]));
    }
    if !parts.is_empty() {
        wrapped(ui, &parts.join(" · "), 11.0, p.text2);
    }
    if let Some(warning) = expiry_warning(lang, probe.expires_at, t.input.now) {
        status_line(ui, ICON_WARNING, p.warn_text, &warning);
    }
    if probe.rights_gitmanager == GitManagerRights::Ok {
        status_line(
            ui,
            ICON_CHECK_CIRCLE,
            p.ok_text,
            &tr(lang, "mrs_rights_gitmanager_ok"),
        );
    }
    glab_rights(ui, t, probe.rights_glab);
    if !probe.scopes.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            small(ui, &tr(lang, "mrs_scopes"), p.text2);
            for scope in &probe.scopes {
                // Für GitManager nötig: read_api oder api
                let (fill, color) = if scope == "read_api" || scope == "api" {
                    (p.reviewer_bg, p.reviewer_text)
                } else {
                    (p.pill_bg, p.pill_text)
                };
                chip(ui, scope, fill, None, color);
            }
        });
    }
}

/// i18n-Schlüssel der glab-Zeile (E4). „Unbekannt“ (feingranulares Token,
/// `personal_access_tokens/self` fehlt) steht ausdrücklich da: sonst fehlte
/// das glab-Urteil samt Scope-Hinweis ganz.
fn glab_rights_key(rights: GlabRights) -> &'static str {
    match rights {
        GlabRights::Full => "mrs_glab_full",
        GlabRights::ReadOnly => "mrs_glab_readonly",
        GlabRights::Insufficient => "mrs_glab_insufficient",
        GlabRights::Unknown => "mrs_glab_unknown",
    }
}

/// Zeile „glab: Voll / Nur lesen / Unzureichend / Unbekannt“ mit dem
/// Hinweis aus E4.
fn glab_rights(ui: &mut Ui, t: &Tab, rights: GlabRights) {
    let p = &t.p;
    let (icon, color) = match rights {
        GlabRights::Full => (ICON_CHECK_CIRCLE, p.ok_text),
        GlabRights::ReadOnly => (ICON_WARNING, p.warn_text),
        GlabRights::Insufficient => (ICON_X_CIRCLE, p.fail_text),
        GlabRights::Unknown => (ICON_MINUS_CIRCLE, p.text2),
    };
    status_line(ui, icon, color, &tr(t.lang, glab_rights_key(rights)));
    if rights != GlabRights::Full {
        wrapped(ui, &tr(t.lang, "mrs_glab_scopes_hint"), 11.0, p.text2);
    }
}

/// Was der Hauptknopf des Formulars tut.
enum FormButton {
    Verify,
    /// `confirm`: „Trotzdem speichern“ nach der Downgrade-Warnung (E4)
    Save {
        confirm: bool,
    },
    /// Token ohne `read_api`/`api`: nicht speicherbar
    Blocked,
}

fn form_button(phase: &FormPhase) -> FormButton {
    match phase {
        FormPhase::Preview(check) if check.rights_gitmanager == GitManagerRights::Missing => {
            FormButton::Blocked
        }
        FormPhase::Preview(check) => FormButton::Save {
            confirm: check.downgrade_warning,
        },
        FormPhase::Saving => FormButton::Save { confirm: false },
        FormPhase::Input | FormPhase::Checking => FormButton::Verify,
    }
}

/// Formular „Token hinzufügen / ersetzen“: lokal prüfen → `verify_token` →
/// Vorschau → „Speichern“ → `save_token`.
fn token_form(
    ui: &mut Ui,
    t: &Tab,
    form: &mut TokenForm,
    out: &mut MrSettingsOutput,
    close: &mut bool,
) {
    let (lang, p) = (t.lang, &t.p);
    let busy = form.busy() || t.input.token_busy.is_some();
    ui.add_space(6.0);
    if form.protocol == Protocol::Http {
        let text = tr_fmt(lang, "mrs_http_warning", &[&form.host]);
        status_line(ui, ICON_WARNING, p.warn_text, &text);
    }
    // Ersetzt wird die Anmeldung unter `login_key` (`host:port` → `host`)
    wrapped(
        ui,
        &tr_fmt(lang, "mrs_token_note", &[login_key(&form.host)]),
        11.0,
        p.text2,
    );
    ui.add_space(4.0);
    egui::Grid::new(("mrs_token_grid", form.field_id))
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr(lang, "mrs_token_label"));
            ui.horizontal(|ui| {
                let edit = TextEdit::singleline(&mut form.token)
                    .id(form.field_id)
                    .password(!form.show)
                    .font(FontId::monospace(12.0))
                    .desired_width(300.0);
                if ui.add_enabled(!busy, edit).changed() {
                    // Geändertes Token: Vorschau gilt nicht mehr
                    form.error = None;
                    if matches!(form.phase, FormPhase::Preview(_)) {
                        form.phase = FormPhase::Input;
                    }
                }
                ui.checkbox(&mut form.show, tr(lang, "mrs_show_token"));
            });
            ui.end_row();
            ui.label(tr(lang, "mrs_purpose"));
            ui.vertical(|ui| {
                ui.radio_value(&mut form.full, false, tr(lang, "mrs_purpose_read"));
                ui.radio_value(&mut form.full, true, tr(lang, "mrs_purpose_full"));
                let url = token_create_url(&form.host, form.protocol, !form.full);
                if link_button(ui, &tr(lang, "mrs_create_token"))
                    .on_hover_text(&url)
                    .clicked()
                {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                }
            });
            ui.end_row();
            ui.label(tr(lang, "mrs_protocol"));
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    let mut changed = ui
                        .radio_value(&mut form.protocol, Protocol::Https, "https")
                        .changed();
                    changed |= ui
                        .radio_value(&mut form.protocol, Protocol::Http, "http")
                        .changed();
                    if changed && matches!(form.phase, FormPhase::Preview(_)) {
                        form.phase = FormPhase::Input;
                    }
                });
            });
            ui.end_row();
        });
    if let Some(error) = &form.error {
        status_line(ui, ICON_X_CIRCLE, p.fail_text, error);
    }
    if let FormPhase::Preview(check) = &form.phase {
        preview(ui, t, check);
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        match form_button(&form.phase) {
            FormButton::Verify => {
                if ui
                    .add_enabled(!busy, primary_button(ui, &tr(lang, "mrs_verify")))
                    .clicked()
                {
                    request_verify(form, lang, out);
                }
            }
            FormButton::Save { confirm } => {
                let key = if confirm {
                    "mrs_downgrade_confirm"
                } else {
                    "save"
                };
                if ui
                    .add_enabled(!busy, primary_button(ui, &tr(lang, key)))
                    .clicked()
                {
                    request_save(form, out);
                }
            }
            FormButton::Blocked => {
                ui.add_enabled(false, primary_button(ui, &tr(lang, "save")));
            }
        }
        if ui.button(tr(lang, "cancel")).clicked() {
            *close = true;
        }
        if busy {
            ui.add(Spinner::new().size(14.0).color(p.run_icon));
            small(ui, &tr(lang, "mrs_state_checking"), p.text2);
        }
    });
}

/// „Prüfen“: lokale Prüfung, dann `verify_token` (Kopie des Feldes als
/// `SecretString`; das Feld bleibt für „Speichern“).
fn request_verify(form: &mut TokenForm, lang: Language, out: &mut MrSettingsOutput) {
    if let Some(key) = token_problem(&form.token) {
        form.error = Some(tr(lang, key));
        return;
    }
    form.error = None;
    form.phase = FormPhase::Checking;
    let token = SecretString::new(form.token.clone());
    out.start_verify = Some((form.host.clone(), token, form.protocol));
}

/// „Speichern“: `save_token` (prüft selbst noch einmal).
fn request_save(form: &mut TokenForm, out: &mut MrSettingsOutput) {
    form.error = None;
    form.phase = FormPhase::Saving;
    let token = SecretString::new(form.token.clone());
    out.start_save = Some((form.host.clone(), token, form.protocol));
}

/// Vorschau nach `verify_token`, mit Downgrade-Warnung (E4).
fn preview(ui: &mut Ui, t: &Tab, check: &TokenCheck) {
    let (lang, p) = (t.lang, &t.p);
    ui.add_space(4.0);
    wrapped(ui, &preview_text(lang, check), 12.0, p.title);
    let (key, icon, color) = match check.rights_gitmanager {
        GitManagerRights::Ok => ("mrs_state_ready", ICON_CHECK_CIRCLE, p.ok_text),
        GitManagerRights::Missing => ("mrs_state_rights_missing", ICON_X_CIRCLE, p.fail_text),
        GitManagerRights::Unknown => ("mr_err_unknown", ICON_WARNING, p.warn_text),
    };
    status_line(ui, icon, color, &format!("GitManager: {}", tr(lang, key)));
    glab_rights(ui, t, check.rights_glab);
    if check.downgrade_warning {
        status_line(
            ui,
            ICON_WARNING,
            p.warn_text,
            &tr(lang, "mrs_downgrade_warning"),
        );
    }
}

/// „Nicht bei glab angemeldete Remote-Hosts ({n})“, eingeklappt, sobald ein
/// Host angemeldet ist. Netz-Aufrufe erst nach „Token hinzufügen“ (E16).
fn unrecognized_section(
    ui: &mut Ui,
    t: &Tab,
    cfg: &mut MrConfig,
    mr: &mut MrSettingsState,
    out: &mut MrSettingsOutput,
) {
    let list = &t.unrecognized;
    if list.is_empty() {
        return;
    }
    let lang = t.lang;
    let any_signed = t.hosts.gitlab_hosts.iter().any(|h| t.signed_in(h));
    let title = tr_fmt(lang, "mrs_unrecognized", &[&list.len().to_string()]);
    egui::CollapsingHeader::new(RichText::new(title).size(12.0))
        .id_salt("mrs_unrecognized")
        .default_open(!any_signed)
        .show(ui, |ui| {
            wrapped(ui, &tr(lang, "mrs_unrecognized_hint"), 11.0, t.p.text2);
            ui.add_space(6.0);
            for host in list {
                unrecognized_row(ui, t, cfg, mr, host, out);
                ui.add_space(6.0);
            }
        });
}

fn unrecognized_row(
    ui: &mut Ui,
    t: &Tab,
    cfg: &mut MrConfig,
    mr: &mut MrSettingsState,
    host: &UnrecognizedHost,
    out: &mut MrSettingsOutput,
) {
    let (lang, p) = (t.lang, &t.p);
    let protocol = if host.http {
        Protocol::Http
    } else {
        Protocol::Https
    };
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(&host.key)
                    .font(FontId::monospace(12.0))
                    .color(p.title),
            );
            if host.http {
                http_chip(ui, t);
            }
            let repos = count_text(lang, host.repos, "mrs_host_repos_one", "mrs_host_repos");
            small(ui, &repos, p.text2);
        });
        ui.horizontal_wrapped(|ui| {
            let open = mr.form_open_for(&host.key);
            if ui
                .add(Button::new(tr(lang, "mrs_add_token")).selected(open))
                .clicked()
            {
                if open {
                    mr.close_form(ui.ctx());
                } else {
                    mr.open_form(ui.ctx(), &host.key, protocol, false);
                }
            }
            if ui.button(tr(lang, "mr_add_gitlab_host")).clicked() && add_extra_host(cfg, &host.key)
            {
                out.hosts_changed = true;
            }
            let editing = mr.alias.as_ref().is_some_and(|a| a.host == host.key);
            if ui
                .add(Button::new(tr(lang, "mrs_belongs_to")).selected(editing))
                .on_hover_text(tr(lang, "mrs_belongs_to_tt"))
                .clicked()
            {
                mr.alias = (!editing).then(|| AliasEdit {
                    host: host.key.clone(),
                    text: String::new(),
                });
            }
        });
        let picked = mr
            .alias
            .as_mut()
            .filter(|a| a.host == host.key)
            .and_then(|edit| alias_editor(ui, t, edit));
        if let Some(to) = picked {
            set_alias(cfg, &host.key, &to);
            // Zeile wandert auf die Karte des Ziels; das Ziel lokal erkennen
            out.hosts_changed = true;
            mr.alias = None;
            // Token-Formular dieses Hosts gilt nicht mehr (E3: Feld leeren)
            if mr.form_open_for(&host.key) {
                mr.close_form(ui.ctx());
            }
        }
        let mut close = false;
        if let Some(form) = mr.form_for(&host.key) {
            token_form(ui, t, form, out, &mut close);
        }
        if close {
            mr.close_form(ui.ctx());
        }
    });
}

/// „gehört zu …“: bekannte GitLab-Hosts zum Anklicken, sonst `host[:port]`
/// frei eingeben (z. B. der Web-Host zu einem SSH-Host). Gibt das gewählte
/// Ziel zurück.
fn alias_editor(ui: &mut Ui, t: &Tab, edit: &mut AliasEdit) -> Option<String> {
    let (lang, p) = (t.lang, &t.p);
    let mut picked = None;
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        small(ui, &tr(lang, "mrs_belongs_to"), p.text2);
        for target in t.alias_choices(&edit.host) {
            let label = RichText::new(&target).font(FontId::monospace(12.0));
            if ui.button(label).clicked() {
                picked = Some(target);
            }
        }
    });
    ui.horizontal(|ui| {
        let resp = ui.add(
            TextEdit::singleline(&mut edit.text)
                .hint_text("host[:port]")
                .font(FontId::monospace(12.0))
                .desired_width(240.0),
        );
        let target = alias_target(&edit.text, &edit.host);
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let apply = ui
            .add_enabled(target.is_some(), Button::new(tr(lang, "mrs_alias_apply")))
            .clicked();
        if apply || enter {
            picked = picked.take().or(target);
        }
    });
    picked
}

// --- 3. Repositories ------------------------------------------------------------

fn repos_section(ui: &mut Ui, t: &Tab, cfg: &mut MrConfig, out: &mut MrSettingsOutput) {
    let (lang, p) = (t.lang, &t.p);
    let rows: Vec<(String, RowAccess, RepoRow)> = t
        .locals
        .iter()
        .map(|project| {
            let key = project_key(&project.host_key, &project.full_path);
            let excluded = cfg
                .excluded_projects
                .iter()
                .any(|k| k.trim().eq_ignore_ascii_case(&key));
            let access = t.row_access(project);
            let row = repo_row_state(access, excluded);
            (key, access, row)
        })
        .collect();
    let included = rows.iter().filter(|(_, _, r)| r.checked).count();
    // Nur einbezogene Projekte mit Zugriff: aus ihnen wird geladen
    let reachable = rows
        .iter()
        .filter(|(_, access, r)| {
            r.checked && matches!(access, RowAccess::Known(s) if accessible(s))
        })
        .count();
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr(lang, "mrs_sec_repos")).size(13.0).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let any_host = !t.hosts.gitlab_hosts.is_empty();
            if ui
                .add_enabled(any_host, Button::new(tr(lang, "mrs_check_access")))
                .clicked()
            {
                for host in &t.hosts.gitlab_hosts {
                    if !t.probing(host) && !out.start_probe.contains(host) {
                        out.start_probe.push(host.clone());
                    }
                }
            }
            let keys: Vec<String> = rows.iter().map(|(k, _, _)| k.clone()).collect();
            if ui.button(tr(lang, "mrs_none")).clicked() {
                set_projects_included(&mut cfg.excluded_projects, &keys, false);
            }
            if ui.button(tr(lang, "mrs_all")).clicked() {
                set_projects_included(&mut cfg.excluded_projects, &keys, true);
            }
            let counts = [included, rows.len(), reachable].map(|n| n.to_string());
            small(ui, &fmt(lang, "mrs_included", &counts), p.text2);
        });
    });
    ui.add_space(6.0);
    if !rows.is_empty() {
        // Breite Projektpfade scrollen in der Tabelle, nicht im Fenster
        egui::ScrollArea::both()
            .id_salt("mrs_repos")
            .max_height(REPO_LIST_MAX_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                egui::Grid::new("mrs_repo_grid")
                    .num_columns(4)
                    .spacing([12.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for key in [
                            "mrs_col_repo",
                            "mrs_col_clones",
                            "mrs_col_project",
                            "mrs_col_access",
                        ] {
                            small(ui, &tr(lang, key), p.text2);
                        }
                        ui.end_row();
                        for (project, (key, _, row)) in t.locals.iter().zip(&rows) {
                            repo_row(ui, t, cfg, project, key, row);
                            ui.end_row();
                        }
                    });
            });
        ui.add_space(6.0);
    }
    ui.checkbox(&mut cfg.include_nonlocal, tr(lang, "mrs_include_nonlocal"));
}

/// Checkbox + Name (Pfade im Tooltip), Klone, `host/full/path`, Zugriff.
fn repo_row(
    ui: &mut Ui,
    t: &Tab,
    cfg: &mut MrConfig,
    project: &LocalProject,
    key: &str,
    row: &RepoRow,
) {
    let (lang, p) = (t.lang, &t.p);
    let paths = project
        .repos
        .iter()
        .map(|r| r.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut checked = row.checked;
    let color = if checked { p.title } else { p.text2 };
    let name = RichText::new(project_name(&project.full_path))
        .size(12.0)
        .color(color);
    if ui
        .checkbox(&mut checked, name)
        .on_hover_text(&paths)
        .changed()
    {
        set_project_included(&mut cfg.excluded_projects, key, checked);
    }
    if project.repos.len() > 1 {
        let clones = tr_fmt(lang, "mrs_clones", &[&project.repos.len().to_string()]);
        small(ui, &clones, p.text2).on_hover_text(&paths);
    } else {
        ui.label("");
    }
    ui.label(
        RichText::new(format!("{}/{}", project.host_key, project.full_path))
            .font(FontId::monospace(11.0))
            .color(p.text2),
    );
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if row.status_key.is_empty() {
            small(ui, "–", p.text2);
            return;
        }
        let text = fmt(lang, row.status_key, &row.args);
        let (icon, icon_color, text_color) = match row.status_key {
            _ if row.warn => (Some(ICON_WARNING), p.warn_icon, p.warn_text),
            "mrs_acc_checking" => (None, p.text2, p.text2),
            "mrs_acc_ok" | "mrs_acc_ok_plain" | "mrs_acc_public" => {
                (Some(ICON_CHECK_CIRCLE), p.ok_icon, p.text2)
            }
            "mrs_acc_host_auth" => (Some(ICON_LOCK), p.neutral_icon, p.text2),
            _ => (Some(ICON_MINUS_CIRCLE), p.neutral_icon, p.text2),
        };
        match icon {
            Some(icon) => {
                ui.add(icon_image(icon, 12.0, icon_color));
            }
            None => {
                ui.add(Spinner::new().size(12.0).color(p.run_icon));
            }
        }
        small(ui, &text, text_color);
    });
}

// --- 4. Auschecken ----------------------------------------------------------------

fn checkout_section(ui: &mut Ui, t: &Tab, draft: &mut AppConfig) {
    let (lang, p) = (t.lang, &t.p);
    section_title(ui, &tr(lang, "mrs_sec_checkout"));
    egui::Grid::new("mrs_checkout_grid")
        .num_columns(2)
        .spacing([16.0, 10.0])
        .show(ui, |ui| {
            ui.label(tr(lang, "mrs_review_root"));
            ui.horizontal(|ui| {
                let mut text = draft.merge_requests.review_root.clone().unwrap_or_default();
                if ui
                    .add(TextEdit::singleline(&mut text).desired_width(320.0))
                    .changed()
                {
                    draft.merge_requests.review_root = Some(text).filter(|r| !r.trim().is_empty());
                }
                if ui.button(browse_label(lang)).clicked() {
                    let mut dialog = rfd::FileDialog::new();
                    if let Some(dir) = draft
                        .merge_requests
                        .review_root
                        .as_deref()
                        .map(PathBuf::from)
                        .filter(|d| d.is_dir())
                    {
                        dialog = dialog.set_directory(dir);
                    }
                    if let Some(path) = dialog.pick_folder() {
                        draft.merge_requests.review_root = Some(path.display().to_string());
                    }
                }
            });
            ui.end_row();
            let relative = draft.merge_requests.review_root.is_some()
                && review_root_dir(&draft.merge_requests).is_none();
            if relative {
                // Relativer Pfad: gilt als nicht gesetzt (siehe `review_root_dir`)
                ui.label("");
                let text = tr(lang, "mrs_review_root_relative");
                status_line(ui, ICON_WARNING, p.warn_text, &text);
                ui.end_row();
            } else if let Some(root) = draft.merge_requests.review_root.clone() {
                ui.label("");
                let hit = review_root_hit(&root, &draft.roots, draft.max_depth).cloned();
                ui.vertical(|ui| match hit {
                    Some(hit) => {
                        let text =
                            tr_fmt(lang, "mrs_review_root_ok", &[&hit.display().to_string()]);
                        status_line(ui, ICON_CHECK_CIRCLE, p.ok_text, &text);
                    }
                    None => {
                        let text = tr(lang, "mrs_review_root_warn");
                        status_line(ui, ICON_WARNING, p.warn_text, &text);
                        if ui.button(tr(lang, "mrs_add_search_path")).clicked() {
                            add_review_root_as_search_path(draft);
                        }
                    }
                });
                ui.end_row();
            }
            ui.label(tr(lang, "mrs_folder_name"));
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(FOLDER_PATTERN)
                        .font(FontId::monospace(12.0))
                        .color(p.title),
                );
                let repo = t
                    .locals
                    .first()
                    .map_or("repo", |l| project_name(&l.full_path));
                let example = format!("{repo}-mr-{EXAMPLE_IID}");
                small(
                    ui,
                    &tr_fmt(lang, "mrs_folder_name_example", &[&example]),
                    p.text2,
                );
            });
            ui.end_row();
            ui.label(tr(lang, "mrs_after_checkout"));
            after_checkout_combo(ui, t, draft);
            ui.end_row();
        });
}

/// „Nach dem Auschecken öffnen“ → `after_checkout_default`.
fn after_checkout_combo(ui: &mut Ui, t: &Tab, draft: &mut AppConfig) {
    let lang = t.lang;
    let options = after_options(lang, draft);
    let current = draft.merge_requests.after_checkout_default.clone();
    let selected = match &current {
        AfterCheckoutPref::LastUsed => tr(lang, "mrs_after_last"),
        AfterCheckoutPref::Fixed(action) => after_action_label(lang, draft, action),
    };
    let mut picked = None;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("mrs_after_checkout")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (pref, label) in &options {
                    if ui.selectable_label(*pref == current, label).clicked() {
                        picked = Some(pref.clone());
                    }
                }
            });
        if current == AfterCheckoutPref::LastUsed {
            let last = after_action_label(lang, draft, &draft.merge_requests.after_checkout_last);
            small(
                ui,
                &tr_fmt(lang, "mrs_after_last_hint", &[&last]),
                t.p.text2,
            );
        }
    });
    if let Some(pref) = picked {
        draft.merge_requests.after_checkout_default = pref;
    }
}

// --- 5. Anzeige & Aktualisierung --------------------------------------------------

fn display_section(ui: &mut Ui, t: &Tab, cfg: &mut MrConfig) {
    let lang = t.lang;
    section_title(ui, &tr(lang, "mrs_sec_display"));
    ui.horizontal(|ui| {
        ui.label(tr(lang, "mrs_refresh"));
        ui.add(
            egui::DragValue::new(&mut cfg.refresh_interval_min)
                .range(0..=60)
                .speed(1.0),
        );
        small(ui, &tr(lang, "mrs_refresh_unit"), t.p.text2);
    });
    ui.checkbox(
        &mut cfg.show_uninvolved_default,
        tr(lang, "mrs_show_all_default"),
    );
    ui.checkbox(&mut cfg.ignore_bot_approvals, tr(lang, "mrs_ignore_bots"));
}

// --- Bausteine ------------------------------------------------------------------

/// Abschnittstitel wie „Suchpfade“ (13 px).
fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(13.0).strong());
    ui.add_space(6.0);
}

fn section_gap(ui: &mut Ui) {
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
}

/// Unterkarte wie die Profil-Liste (Fläche, 1 px Rahmen).
fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let visuals = ui.visuals();
    Frame::new()
        .fill(visuals.widgets.inactive.bg_fill)
        .stroke(Stroke::new(1.0, visuals.widgets.inactive.fg_stroke.color))
        .corner_radius(8)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// Hinweisbanner (gesetztes Env-Token).
fn notice(ui: &mut Ui, t: &Tab, text: &str) {
    let p = &t.p;
    Frame::new()
        .fill(p.notice_bg)
        .stroke(Stroke::new(1.0, p.notice_stroke))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.add(icon_image(ICON_WARNING, 14.0, p.notice_text));
                ui.add(Label::new(RichText::new(text).size(12.0).color(p.notice_text)).wrap());
            });
        });
}

/// Icon + umbrechender Text in einer Farbe.
fn status_line(ui: &mut Ui, icon: ImageSource<'static>, color: Color32, text: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add(icon_image(icon, 14.0, color));
        ui.add(Label::new(RichText::new(text).size(12.0).color(color)).wrap());
    });
}

fn wrapped(ui: &mut Ui, text: &str, size: f32, color: Color32) {
    ui.add(Label::new(RichText::new(text).size(size).color(color)).wrap());
}

fn small(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.label(RichText::new(text).size(11.0).color(color))
}

fn icon_image(icon: ImageSource<'static>, size: f32, tint: Color32) -> Image<'static> {
    Image::new(icon)
        .fit_to_exact_size(vec2(size, size))
        .tint(tint)
}

/// Icon-Button 22 × 22 mit Tooltip und AccessKit-Namen.
fn icon_button(ui: &mut Ui, icon: ImageSource<'static>, tooltip: &str) -> Response {
    let tint = ui.visuals().widgets.inactive.fg_stroke.color;
    let resp = ui.add(
        Button::new(icon_image(icon, 14.0, tint))
            .min_size(vec2(22.0, 22.0))
            .corner_radius(6),
    );
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, tooltip));
    resp.on_hover_text(tooltip)
}

/// Hauptknopf in den Auswahlfarben des Themes.
fn primary_button(ui: &Ui, text: &str) -> Button<'static> {
    let visuals = ui.visuals();
    Button::new(
        RichText::new(text)
            .size(12.0)
            .color(visuals.selection.stroke.color),
    )
    .fill(visuals.selection.bg_fill)
}

/// Link mit Pfeil-Icon („Token in GitLab erstellen ↗“).
fn link_button(ui: &mut Ui, text: &str) -> Response {
    let color = ui.visuals().hyperlink_color;
    ui.add(
        Button::image_and_text(
            icon_image(ICON_OPEN_EXTERNAL, 12.0, color),
            RichText::new(text).size(12.0).color(color).underline(),
        )
        .frame(false),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

/// Pill (h 18, r 9, 11 px), gefüllt und/oder umrandet.
fn chip(
    ui: &mut Ui,
    text: &str,
    fill: Color32,
    stroke: Option<Color32>,
    color: Color32,
) -> Response {
    Frame::new()
        .fill(fill)
        .stroke(stroke.map_or(Stroke::NONE, |c| Stroke::new(1.0, c)))
        .corner_radius(9)
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.0).color(color))
        })
        .response
}

fn status_chip(ui: &mut Ui, t: &Tab, key: &str, args: &[String]) {
    let color = tone_color(&t.p, chip_tone(key));
    if key == "mrs_state_checking" {
        ui.add(Spinner::new().size(12.0).color(t.p.run_icon));
    }
    chip(
        ui,
        &fmt(t.lang, key, args),
        Color32::TRANSPARENT,
        Some(color),
        color,
    );
}

/// „http – unverschlüsselt“ (E17).
fn http_chip(ui: &mut Ui, t: &Tab) {
    let color = t.p.warn_text;
    chip(
        ui,
        &tr(t.lang, "mrs_http_chip"),
        Color32::TRANSPARENT,
        Some(color),
        color,
    );
}

/// Befehl in Monospace mit Kopier-Button.
fn command_line(ui: &mut Ui, t: &Tab, command: &str) {
    ui.horizontal(|ui| {
        Frame::new()
            .fill(t.p.track)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(command)
                        .font(FontId::monospace(12.0))
                        .color(t.p.title),
                )
            });
        if icon_button(ui, ICON_COPY, &tr(t.lang, "mr_copy_cmd")).clicked() {
            ui.ctx().copy_text(command.to_string());
        }
    });
}

fn browse_label(lang: Language) -> String {
    format!("{} …", tr(lang, "browse"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitlab::remote::RemoteInfo;
    use crate::gitlab::service::ProjectAccess;

    const HOST: &str = "gitlab.example.com";

    fn probe(error: Option<GlabErrorKind>, rights: GitManagerRights) -> HostProbe {
        HostProbe {
            host_key: HOST.to_string(),
            error,
            rights_gitmanager: rights,
            ..HostProbe::default()
        }
    }

    fn keys(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn repo(name: &str, urls: &[&str]) -> RepoInfo {
        let mut repo = RepoInfo::new(
            PathBuf::from(format!("C:\\Dev\\{name}")),
            "main".to_string(),
            false,
            false,
        );
        repo.remotes = urls
            .iter()
            .enumerate()
            .map(|(i, url)| RemoteInfo::new(if i == 0 { "origin" } else { "fork" }, url))
            .collect();
        repo
    }

    fn signed_in(key: &str) -> GlabHost {
        GlabHost {
            key: key.to_string(),
            user: "taltmann".to_string(),
            ..GlabHost::default()
        }
    }

    fn roots(items: &[&str]) -> Vec<PathBuf> {
        items.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn include_exclude_dedups() {
        let mut excluded = keys(&["gitlab.example.com/kern/api"]);
        // schon abgewählt (ohne Groß-/Kleinschreibung) → keine Dublette
        set_project_included(&mut excluded, "GitLab.example.com/Kern/API", false);
        assert_eq!(excluded, keys(&["gitlab.example.com/kern/api"]));
        set_project_included(&mut excluded, "gitlab.example.com/web/portal", false);
        assert_eq!(
            excluded,
            keys(&[
                "gitlab.example.com/kern/api",
                "gitlab.example.com/web/portal"
            ])
        );
        set_project_included(&mut excluded, "gitlab.example.com/kern/api", true);
        assert_eq!(excluded, keys(&["gitlab.example.com/web/portal"]));
        // Dubletten aus einem Handedit verschwinden beim Einbeziehen ganz
        let mut excluded = keys(&["h/x/y", " H/X/Y "]);
        set_project_included(&mut excluded, "h/x/y", true);
        assert!(excluded.is_empty(), "{excluded:?}");
        // Alle / Keine: nur die angezeigten Projekte, fremde Schlüssel bleiben
        let shown = keys(&["h/a/b", "h/c/d"]);
        let mut excluded = keys(&["other/z/z", "h/c/d"]);
        set_projects_included(&mut excluded, &shown, false);
        assert_eq!(excluded, keys(&["h/a/b", "h/c/d", "other/z/z"]));
        set_projects_included(&mut excluded, &shown, true);
        assert_eq!(excluded, keys(&["other/z/z"]));
    }

    #[test]
    fn host_chip_states() {
        use GitManagerRights::{Missing, Ok, Unknown};
        let cases = [
            (probe(None, Ok), "mrs_state_ready"),
            (probe(None, Missing), "mrs_state_rights_missing"),
            (
                probe(Some(GlabErrorKind::InsufficientScope), Unknown),
                "mrs_state_rights_missing",
            ),
            (
                probe(Some(GlabErrorKind::NotAuthenticated), Unknown),
                "mrs_state_not_signed",
            ),
            (
                probe(Some(GlabErrorKind::InvalidToken), Unknown),
                "mrs_state_not_signed",
            ),
            (
                probe(Some(GlabErrorKind::Expired), Unknown),
                "mrs_state_expired",
            ),
            (
                probe(Some(GlabErrorKind::Revoked), Unknown),
                "mrs_state_revoked",
            ),
            (
                probe(Some(GlabErrorKind::Network), Unknown),
                "mrs_state_unreachable",
            ),
            (
                probe(Some(GlabErrorKind::Timeout), Unknown),
                "mrs_state_unreachable",
            ),
            (probe(Some(GlabErrorKind::Tls), Unknown), "mrs_state_tls"),
            (
                probe(Some(GlabErrorKind::NotInstalled), Unknown),
                "mrs_glab_not_found",
            ),
        ];
        for (probe, key) in cases {
            assert_eq!(host_chip(&probe), (key, Vec::new()), "{probe:?}");
        }
        let old = probe(
            Some(GlabErrorKind::TooOld(Version::new(1, 100, 0))),
            Unknown,
        );
        assert_eq!(
            host_chip(&old),
            ("mr_glab_too_old", vec!["1.100.0".to_string()])
        );
    }

    #[test]
    fn repo_row_states() {
        let ok = AccessState::Ok { level: 30 };
        assert_eq!(
            repo_row_state(RowAccess::Known(&ok), false),
            RepoRow {
                checked: true,
                status_key: "mrs_acc_ok",
                args: vec!["Developer".to_string()],
                warn: false,
            }
        );
        let unknown_level = AccessState::Ok { level: 0 };
        assert_eq!(
            repo_row_state(RowAccess::Known(&unknown_level), true).status_key,
            "mrs_acc_ok_plain"
        );
        assert!(!repo_row_state(RowAccess::Known(&unknown_level), true).checked);
        let renamed = AccessState::Renamed {
            to: "kern/api-neu".to_string(),
        };
        let r = repo_row_state(RowAccess::Known(&renamed), false);
        assert_eq!(
            (r.status_key, r.args, r.warn),
            ("mrs_acc_renamed", vec!["kern/api-neu".to_string()], false)
        );
        for (state, key) in [
            (AccessState::Public, "mrs_acc_public"),
            (AccessState::MrsDisabled, "mrs_acc_mr_off"),
            (AccessState::Archived, "mrs_acc_archived"),
        ] {
            let r = repo_row_state(RowAccess::Known(&state), false);
            assert_eq!((r.status_key, r.warn), (key, false), "{state:?}");
        }
        // Kein Zugriff: einbezogen → wird übersprungen (Warnung)
        let denied = AccessState::NoAccess;
        let r = repo_row_state(RowAccess::Known(&denied), false);
        assert_eq!(
            (r.checked, r.status_key, r.warn),
            (true, "mrs_acc_skipped", true)
        );
        let r = repo_row_state(RowAccess::Known(&denied), true);
        assert_eq!(
            (r.checked, r.status_key, r.warn),
            (false, "mrs_acc_denied", false)
        );
        let r = repo_row_state(RowAccess::HostNotSignedIn, false);
        assert_eq!((r.status_key, r.warn), ("mrs_acc_host_auth", false));
        let r = repo_row_state(RowAccess::Checking, false);
        assert_eq!((r.status_key, r.warn), ("mrs_acc_checking", false));
        let r = repo_row_state(RowAccess::Known(&AccessState::Unknown), false);
        assert_eq!((r.status_key, r.warn), ("", false));
    }

    #[test]
    fn form_reset_clears_token_and_text_edit_state() {
        let ctx = egui::Context::default();
        let mut state = MrSettingsState::default();
        state.open_form(&ctx, HOST, Protocol::Https, false);
        let first = state.form.as_ref().expect("Formular offen").field_id;
        state.form.as_mut().expect("Formular").token = "glpat-Geheim0815".to_string();
        TextEditState::default().store(&ctx, first);
        assert!(TextEditState::load(&ctx, first).is_some());
        // Reset leert dieselbe Instanz: Token und Undo-Verlauf weg
        let form = state.form.as_mut().expect("Formular");
        form.reset(&ctx);
        assert!(form.token.is_empty(), "Token geleert");
        assert!(
            TextEditState::load(&ctx, first).is_none(),
            "Undo-Verlauf weg"
        );
        form.token = "glpat-Geheim0815".to_string();
        TextEditState::default().store(&ctx, first);
        state.close_form(&ctx);
        assert!(state.form.is_none());
        assert!(
            TextEditState::load(&ctx, first).is_none(),
            "Undo-Verlauf weg"
        );
        // Neue Öffnung → neue Feld-Id
        state.open_form(&ctx, HOST, Protocol::Https, false);
        let second = state.form.as_ref().expect("Formular offen").field_id;
        assert_ne!(first, second);
        // Erneutes Öffnen leert ein offenes Formular ebenso
        state.form.as_mut().expect("Formular").token = "glpat-Zwei".to_string();
        TextEditState::default().store(&ctx, second);
        state.open_form(&ctx, HOST, Protocol::Https, true);
        assert!(TextEditState::load(&ctx, second).is_none());
        assert!(state.form.as_ref().is_some_and(|f| f.token.is_empty()));
    }

    // --- Step 3: Auschecken -------------------------------------------------

    /// Review-Ordner innerhalb/außerhalb der Suchpfade (Scan-Tiefe beachtet,
    /// Pfade wie unter Windows verglichen).
    #[test]
    fn review_root_inside_or_outside_search_paths() {
        let dev = roots(&["C:\\Dev"]);
        // C:\Dev\Reviews\api-mr-1 liegt in Tiefe 2
        assert_eq!(
            review_root_hit("c:/dev/Reviews/", &dev, 2),
            Some(&dev[0]),
            "/ und \\, Groß-/Kleinschreibung, Trenner am Ende"
        );
        assert_eq!(
            review_root_hit("C:\\Dev\\Reviews", &dev, 1),
            None,
            "zu tief"
        );
        assert_eq!(review_root_hit("C:\\Dev", &dev, 1), Some(&dev[0]));
        assert_eq!(review_root_hit("D:\\Reviews", &dev, 5), None);
        assert_eq!(
            review_root_hit("C:\\Develop\\Reviews", &dev, 5),
            None,
            "nur ganze Ordnernamen"
        );
        assert_eq!(review_root_hit("  ", &dev, 5), None);
        let two = roots(&["D:\\Work\\", "C:\\Dev"]);
        assert_eq!(review_root_hit("C:\\Dev\\R\\x", &two, 3), Some(&two[1]));
    }

    /// „Als Suchpfad hinzufügen“ ergänzt die Suchpfade im Entwurf; danach
    /// liegt der Review-Ordner im Suchpfad.
    #[test]
    fn add_review_root_as_search_path_extends_draft() {
        let mut draft = AppConfig::default();
        draft.roots = roots(&["C:\\Dev"]);
        draft.max_depth = 1;
        draft.merge_requests.review_root = Some(" D:\\Reviews ".to_string());
        let review = draft.merge_requests.review_root.clone().unwrap_or_default();
        assert_eq!(
            review_root_hit(&review, &draft.roots, draft.max_depth),
            None
        );
        add_review_root_as_search_path(&mut draft);
        assert!(draft.roots.contains(&PathBuf::from("D:\\Reviews")));
        assert!(review_root_hit(&review, &draft.roots, draft.max_depth).is_some());
        // doppelt → einmal
        add_review_root_as_search_path(&mut draft);
        assert_eq!(draft.roots.len(), 2);
        // ohne Review-Ordner nichts
        draft.merge_requests.review_root = None;
        add_review_root_as_search_path(&mut draft);
        assert_eq!(draft.roots.len(), 2);
    }

    // --- Weitere reine Hilfen ---------------------------------------------------

    #[test]
    fn glab_program_validation() {
        assert!(valid_glab_program("glab"));
        assert!(valid_glab_program("  "));
        assert!(
            valid_glab_program("C:\\Tom & Jerry\\glab.exe"),
            "Pfade frei"
        );
        for bad in [
            "glab & calc",
            "glab|x",
            "glab;x",
            "gl$ab",
            "`glab`",
            "glab>x",
        ] {
            assert!(!valid_glab_program(bad), "{bad}");
        }
        assert_eq!(effective_glab_program("  "), "glab");
        assert_eq!(effective_glab_program(" glab.exe "), "glab.exe");
    }

    #[test]
    fn token_local_checks() {
        assert_eq!(token_problem(""), Some("mrs_token_empty"));
        assert_eq!(token_problem("glpat-abc def"), Some("mrs_token_whitespace"));
        assert_eq!(token_problem("glpat-abc\n"), Some("mrs_token_whitespace"));
        assert_eq!(token_problem("glpat-abc"), None);
    }

    /// Pfade je Host: zugeordnete Remotes und (für „Token hinzufügen“ an
    /// einen nicht angemeldeten Remote-Host) Remotes mit genau diesem Host.
    #[test]
    fn project_paths_and_unrecognized_hosts() {
        let repos = vec![
            repo("api", &["git@gitlab.example.com:kern/api.git"]),
            repo("api-2", &["https://gitlab.example.com/kern/api.git"]),
            repo("legacy", &["http://git.intern.example/alt/tool.git"]),
            repo(
                "lib",
                &[
                    "https://github.com/acme/lib.git",
                    "http://git.intern.example/alt/lib.git",
                ],
            ),
        ];
        let glab = vec![signed_in(HOST)];
        let cfg = MrConfig::default();
        assert_eq!(
            project_paths_for_host(&repos, &glab, &cfg, HOST),
            keys(&["kern/api"])
        );
        assert_eq!(
            project_paths_for_host(&repos, &glab, &cfg, "git.intern.example"),
            keys(&["alt/lib", "alt/tool"])
        );
        let hosts = draft_hosts(&repos, &glab, &cfg);
        assert_eq!(
            unrecognized_hosts(&repos, &hosts, &cfg),
            vec![
                UnrecognizedHost {
                    key: "git.intern.example".to_string(),
                    repos: 2,
                    http: true,
                },
                UnrecognizedHost {
                    key: "github.com".to_string(),
                    repos: 1,
                    http: false,
                },
            ]
        );
        assert_eq!(
            host_protocol("git.intern.example", &repos, &hosts, &glab, &cfg),
            Protocol::Http
        );
        assert_eq!(
            host_protocol(HOST, &repos, &hosts, &glab, &cfg),
            Protocol::Https
        );
        // „gehört zu …“ im Entwurf: Host verschwindet aus der Liste, seine
        // Projekte zählen zum Ziel
        let mut draft = cfg.clone();
        assert!(set_alias(&mut draft, "GIT.intern.example", HOST));
        assert!(
            !set_alias(&mut draft, "git.intern.example", HOST),
            "schon so"
        );
        assert_eq!(draft.host_aliases.len(), 1, "ersetzt statt doppelt");
        let hosts = draft_hosts(&repos, &glab, &draft);
        assert_eq!(unrecognized_hosts(&repos, &hosts, &draft).len(), 1);
        assert_eq!(
            project_paths_for_host(&repos, &glab, &draft, HOST),
            keys(&["alt/lib", "alt/tool", "kern/api"])
        );
        assert_eq!(rediscover_candidates(&draft), keys(&[HOST]));
        // „Als GitLab-Host hinzufügen“
        let mut draft = cfg;
        assert!(add_extra_host(&mut draft, " GitHub.com "));
        assert!(!add_extra_host(&mut draft, "github.com"));
        assert_eq!(draft.extra_hosts, keys(&["github.com"]));
        assert_eq!(rediscover_candidates(&draft), keys(&["github.com"]));
    }

    #[test]
    fn purpose_default_follows_current_sign_in() {
        assert!(!full_purpose_default(None));
        let mut probe = probe(None, GitManagerRights::Ok);
        probe.rights_glab = GlabRights::ReadOnly;
        assert!(!full_purpose_default(Some(&probe)));
        probe.rights_glab = GlabRights::Full;
        assert!(full_purpose_default(Some(&probe)));
        probe.error = Some(GlabErrorKind::Expired);
        assert!(!full_purpose_default(Some(&probe)));
    }

    #[test]
    fn preview_and_expiry_texts() {
        let lang = Language::De;
        let check = TokenCheck {
            user: Some("taltmann".to_string()),
            scopes: keys(&["read_api"]),
            expires_at: crate::gitlab::time::parse_date("2027-03-31"),
            access_preview: vec![
                ProjectAccess {
                    full_path: "kern/api".to_string(),
                    state: AccessState::Ok { level: 30 },
                },
                ProjectAccess {
                    full_path: "kern/geheim".to_string(),
                    state: AccessState::NoAccess,
                },
            ],
            ..TokenCheck::default()
        };
        assert_eq!(
            preview_text(lang, &check),
            "@taltmann · read_api · läuft ab 31.03.2027 · 1/2 Repos zugreifbar"
        );
        let unlimited = TokenCheck {
            expires_at: None,
            scopes: Vec::new(),
            ..check
        };
        assert_eq!(
            preview_text(Language::En, &unlimited),
            "@taltmann · – · 1/2 repos accessible"
        );
        let now = 1_000 * DAY;
        assert_eq!(expiry_warning(lang, Some(now + 30 * DAY), now), None);
        assert_eq!(
            expiry_warning(lang, Some(now + 14 * DAY), now).as_deref(),
            Some("Läuft in 14 Tagen ab")
        );
        assert_eq!(
            expiry_warning(lang, Some(now + 3600), now).as_deref(),
            Some("Läuft in 1 Tag ab")
        );
        assert_eq!(expiry_warning(lang, Some(now - 1), now), None);
        assert_eq!(expiry_warning(lang, None, now), None);
    }

    /// Ergebnisse der Token-Worker im Formular: Vorschau, Fehler, Schließen
    /// nach dem Speichern (Token und Undo-Verlauf weg), veraltete verfallen.
    #[test]
    fn token_events_update_form() {
        let ctx = egui::Context::default();
        let lang = Language::De;
        let mut state = MrSettingsState::default();
        state.open_form(&ctx, HOST, Protocol::Https, false);
        // Ohne laufende Prüfung (Token geändert): verfällt
        state.on_token_event(
            &ctx,
            lang,
            HOST,
            TokenEvent::Verified(Ok(TokenCheck::default())),
        );
        assert!(state
            .form
            .as_ref()
            .is_some_and(|f| matches!(f.phase, FormPhase::Input)));
        let mut out = MrSettingsOutput::default();
        if let Some(form) = state.form.as_mut() {
            form.token = "glpat-Geheim0815".to_string();
            request_verify(form, lang, &mut out);
        }
        let (host, token, protocol) = out.start_verify.take().expect("Prüfung angefragt");
        assert_eq!(
            (host.as_str(), token.expose(), protocol),
            (HOST, "glpat-Geheim0815", Protocol::Https)
        );
        state.on_token_event(
            &ctx,
            lang,
            HOST,
            TokenEvent::Verified(Err(GlabErrorKind::InvalidToken)),
        );
        let form = state.form.as_ref().expect("Formular");
        assert_eq!(form.error.as_deref(), Some("Das Token ist ungültig."));
        assert!(matches!(form.phase, FormPhase::Input));
        if let Some(form) = state.form.as_mut() {
            request_verify(form, lang, &mut out);
        }
        state.on_token_event(
            &ctx,
            lang,
            HOST,
            TokenEvent::Verified(Ok(TokenCheck {
                downgrade_warning: true,
                rights_gitmanager: GitManagerRights::Ok,
                ..TokenCheck::default()
            })),
        );
        let form = state.form.as_ref().expect("Formular");
        assert!(matches!(
            form_button(&form.phase),
            FormButton::Save { confirm: true }
        ));
        // Speichern scheitert → Fehler, erneut prüfen
        if let Some(form) = state.form.as_mut() {
            request_save(form, &mut out);
        }
        let (_, token, _) = out.start_save.take().expect("Speichern angefragt");
        assert_eq!(token.expose(), "glpat-Geheim0815");
        state.on_token_event(
            &ctx,
            lang,
            HOST,
            TokenEvent::Saved(Err(GlabErrorKind::NotInstalled)),
        );
        let form = state.form.as_ref().expect("Formular");
        assert_eq!(
            form.error.as_deref(),
            Some("glab konnte das Token nicht speichern: glab nicht gefunden")
        );
        assert!(matches!(form.phase, FormPhase::Input));
        // Erfolgreich gespeichert: Formular zu, Undo-Verlauf weg, Karte neu prüfen
        let field = form.field_id;
        TextEditState::default().store(&ctx, field);
        if let Some(form) = state.form.as_mut() {
            request_save(form, &mut out);
        }
        state.on_token_event(&ctx, lang, HOST, TokenEvent::Saved(Ok(SavedWhere::Keyring)));
        assert!(state.form.is_none());
        assert!(TextEditState::load(&ctx, field).is_none());
        assert_eq!(state.probe_next, keys(&[HOST]));
        // Fehlende Rechte: nicht speicherbar
        let missing = FormPhase::Preview(Box::new(TokenCheck {
            rights_gitmanager: GitManagerRights::Missing,
            ..TokenCheck::default()
        }));
        assert!(matches!(form_button(&missing), FormButton::Blocked));
    }

    /// Ergebnis eines Speicherns, das nach „Abbrechen“ und erneutem Öffnen
    /// für denselben Host ankommt, lässt das neue Formular in Ruhe.
    #[test]
    fn late_save_result_leaves_reopened_form_alone() {
        let ctx = egui::Context::default();
        let lang = Language::De;
        let mut state = MrSettingsState::default();
        state.open_form(&ctx, HOST, Protocol::Https, false);
        let mut out = MrSettingsOutput::default();
        if let Some(form) = state.form.as_mut() {
            form.token = "glpat-Alt".to_string();
            request_save(form, &mut out);
        }
        // Abbrechen, neu öffnen (Speichern läuft weiter)
        state.close_form(&ctx);
        state.open_form(&ctx, HOST, Protocol::Https, false);
        state.on_token_event(
            &ctx,
            lang,
            HOST,
            TokenEvent::Saved(Err(GlabErrorKind::Network)),
        );
        let form = state.form.as_ref().expect("neues Formular offen");
        assert!(form.error.is_none(), "kein Fehler des alten Tokens");
        assert!(matches!(form.phase, FormPhase::Input));
        state.on_token_event(&ctx, lang, HOST, TokenEvent::Saved(Ok(SavedWhere::Keyring)));
        assert!(state.form.is_some(), "neues Formular bleibt offen");
        assert_eq!(state.probe_next, keys(&[HOST]), "Karte trotzdem neu prüfen");
    }

    #[test]
    fn alias_target_from_free_text() {
        let from = "ssh.intern.example";
        for (text, target) in [
            ("gitlab.intern.example", Some("gitlab.intern.example")),
            (
                " HTTPS://GitLab.Intern.Example/ ",
                Some("gitlab.intern.example"),
            ),
            ("http://git.local:8443", Some("git.local:8443")),
            ("", None),
            ("  ", None),
            ("SSH.intern.example", None),
            ("gitlab.intern.example/group", None),
            ("user@gitlab.intern.example", None),
            ("gitlab intern", None),
            (":8443", None),
            ("gitlab.local:", None),
        ] {
            assert_eq!(alias_target(text, from).as_deref(), target, "{text:?}");
        }
    }

    /// „gehört zu …“ auf ein (noch) nicht angemeldetes Ziel: der Remote-Host
    /// verlässt die Liste, das Ziel bekommt eine Karte (Token hinzufügen,
    /// Zuordnung entfernen); angeboten werden Karten und angemeldete Hosts.
    #[test]
    fn alias_targets_get_cards_and_choices() {
        let repos = vec![
            repo("api", &["git@ssh.intern.example:kern/api.git"]),
            repo("lib", &["https://github.com/acme/lib.git"]),
        ];
        // glab kennt den Web-Host, aber kein Remote nennt ihn
        let glab = vec![signed_in("gitlab.intern.example"), signed_in(HOST)];
        let mut cfg = MrConfig::default();
        let hosts = draft_hosts(&repos, &glab, &cfg);
        assert!(card_hosts(&hosts, &cfg).is_empty());
        let unrecognized: Vec<String> = unrecognized_hosts(&repos, &hosts, &cfg)
            .into_iter()
            .map(|u| u.key)
            .collect();
        assert_eq!(unrecognized, keys(&["github.com", "ssh.intern.example"]));
        let theme = Theme::Dark;
        let (probes, probing) = (BTreeMap::new(), BTreeSet::new());
        let input = input(&theme, &repos, &glab, &probes, &probing);
        let tab = |cfg: &MrConfig| {
            let hosts = draft_hosts(&repos, &glab, cfg);
            Tab {
                lang: Language::De,
                p: mr_palette(&theme, &egui::Visuals::dark()),
                input: &input,
                cards: card_hosts(&hosts, cfg),
                unrecognized: unrecognized_hosts(&repos, &hosts, cfg),
                locals: Vec::new(),
                hosts,
            }
        };
        assert_eq!(
            tab(&cfg).alias_choices("ssh.intern.example"),
            keys(&[HOST, "gitlab.intern.example"]),
            "angemeldete Hosts ohne Remote"
        );
        // Ziel nicht angemeldet → Karte, Zeile weg
        assert!(set_alias(
            &mut cfg,
            "ssh.intern.example",
            "gitlab.neu.example"
        ));
        let t = tab(&cfg);
        assert_eq!(t.cards, keys(&["gitlab.neu.example"]));
        assert!(t.shows_host("gitlab.neu.example"));
        assert!(!t.shows_host("ssh.intern.example"));
        assert!(t.hosts.gitlab_hosts.is_empty(), "kein Netz-Ziel");
        // Ziel angemeldet → GitLab-Host mit dem Projekt
        assert!(set_alias(
            &mut cfg,
            "ssh.intern.example",
            "gitlab.intern.example"
        ));
        let t = tab(&cfg);
        assert_eq!(t.cards, keys(&["gitlab.intern.example"]));
        assert_eq!(t.hosts.gitlab_hosts, keys(&["gitlab.intern.example"]));
        assert_eq!(
            t.alias_choices("github.com"),
            keys(&[HOST, "gitlab.intern.example"])
        );
    }

    #[test]
    fn pending_glab_program_until_saved() {
        assert_eq!(pending_glab_program(" glab ", "glab"), None);
        assert_eq!(pending_glab_program("", "glab"), None);
        assert_eq!(
            pending_glab_program("C:\\Tools\\glab.exe", "glab").as_deref(),
            Some("glab")
        );
    }

    // --- Tab im Fenster -----------------------------------------------------

    fn input<'a>(
        theme: &'a crate::config::Theme,
        repos: &'a [RepoInfo],
        glab_hosts: &'a [GlabHost],
        probes: &'a BTreeMap<String, HostProbe>,
        probing: &'a BTreeSet<String>,
    ) -> MrSettingsInput<'a> {
        MrSettingsInput {
            theme,
            glab_program: "glab",
            repos,
            glab_hosts,
            probes,
            probing,
            glab_check: None,
            glab_checking: false,
            token_busy: None,
            env_token: Some("GITLAB_TOKEN"),
            now: 0,
        }
    }

    fn render(state: &mut SettingsState, input: &MrSettingsInput) -> MrSettingsOutput {
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx, input.theme);
        let mut out = MrSettingsOutput::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(900.0, 700.0),
            )),
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                show_merge_requests_tab(ui, state, input, &mut out);
            });
        });
        output.drop_without_applying_deltas();
        out
    }

    /// Erster Frame: glab prüfen und nur angemeldete Hosts ohne Ergebnis;
    /// danach nichts erneut. Zeichnet Karten, Formular und Liste ohne Panik.
    #[test]
    fn first_frame_requests_glab_check_and_signed_in_probes() {
        let theme = crate::config::Theme::Dark;
        let repos = vec![
            repo("api", &["git@gitlab.example.com:kern/api.git"]),
            repo("api-2", &["git@gitlab.example.com:kern/api.git"]),
            repo("legacy", &["http://git.intern.example/alt/tool.git"]),
        ];
        let glab = vec![signed_in(HOST)];
        let mut probes = BTreeMap::new();
        let probing = BTreeSet::new();
        let mut state = SettingsState::from_config(&AppConfig::default());
        state.draft.merge_requests.extra_hosts = keys(&["gitlab.local"]);
        let out = render(&mut state, &input(&theme, &repos, &glab, &probes, &probing));
        assert_eq!(out.start_glab_check.as_deref(), Some("glab"));
        assert_eq!(
            out.start_probe,
            keys(&[HOST]),
            "nicht gitlab.local (nicht angemeldet)"
        );
        assert!(out.start_verify.is_none() && out.start_save.is_none() && !out.hosts_changed);
        let out = render(&mut state, &input(&theme, &repos, &glab, &probes, &probing));
        assert!(out.start_glab_check.is_none() && out.start_probe.is_empty());
        // Mit Prüfergebnis und offenem Formular
        probes.insert(
            HOST.to_string(),
            HostProbe {
                host_key: HOST.to_string(),
                logged_in_as: Some("taltmann".to_string()),
                token_kind: TokenKind::Pat,
                token_source: TokenSource::Keyring,
                scopes: keys(&["read_api", "read_user"]),
                expires_at: Some(DAY),
                rights_gitmanager: GitManagerRights::Ok,
                rights_glab: GlabRights::ReadOnly,
                projects: vec![ProjectAccess {
                    full_path: "kern/api".to_string(),
                    state: AccessState::Ok { level: 40 },
                }],
                ..HostProbe::default()
            },
        );
        let ctx = egui::Context::default();
        state
            .mr
            .open_form(&ctx, "git.intern.example", Protocol::Http, false);
        let out = render(&mut state, &input(&theme, &repos, &glab, &probes, &probing));
        assert!(out.start_probe.is_empty());
        state.mr.close_form(&ctx);
    }

    /// Erst nach dem Öffnen erkannte angemeldete Hosts werden ebenfalls
    /// automatisch geprüft, jeder einmal; die glab-Prüfung bleibt einmalig.
    #[test]
    fn hosts_discovered_after_opening_are_probed_once() {
        let theme = crate::config::Theme::Dark;
        let repos = vec![
            repo("api", &["git@gitlab.example.com:kern/api.git"]),
            repo("tool", &["https://git.intern.example/alt/tool.git"]),
        ];
        let (probes, probing) = (BTreeMap::new(), BTreeSet::new());
        let mut state = SettingsState::from_config(&AppConfig::default());
        // Erkennung läuft noch: kein Host bekannt
        let out = render(&mut state, &input(&theme, &repos, &[], &probes, &probing));
        assert!(out.start_glab_check.is_some());
        assert!(out.start_probe.is_empty());
        let glab = vec![signed_in(HOST)];
        let out = render(&mut state, &input(&theme, &repos, &glab, &probes, &probing));
        assert_eq!(out.start_probe, keys(&[HOST]), "nachträglich erkannt");
        assert!(out.start_glab_check.is_none());
        let glab = vec![signed_in(HOST), signed_in("git.intern.example")];
        let out = render(&mut state, &input(&theme, &repos, &glab, &probes, &probing));
        assert_eq!(out.start_probe, keys(&["git.intern.example"]));
        // Ohne Ergebnis (z. B. Prüfung lief ins Leere) keine Wiederholung
        let out = render(&mut state, &input(&theme, &repos, &glab, &probes, &probing));
        assert!(out.start_probe.is_empty());
    }

    /// „gehört zu …“ für einen Host mit offenem Token-Formular: die Zeile
    /// verschwindet, Formular samt Token und Undo-Verlauf auch (E3).
    #[test]
    fn hidden_host_closes_its_token_form() {
        let theme = crate::config::Theme::Dark;
        let repos = vec![repo("api", &["git@ssh.intern.example:kern/api.git"])];
        let glab = vec![signed_in(HOST)];
        let (probes, probing) = (BTreeMap::new(), BTreeSet::new());
        let mut state = SettingsState::from_config(&AppConfig::default());
        let ctx = egui::Context::default();
        state
            .mr
            .open_form(&ctx, "ssh.intern.example", Protocol::Https, false);
        render(&mut state, &input(&theme, &repos, &glab, &probes, &probing));
        assert!(
            state.mr.form_open_for("ssh.intern.example"),
            "Zeile sichtbar"
        );
        state.mr.form.as_mut().expect("Formular").token = "glpat-Geheim".to_string();
        set_alias(&mut state.draft.merge_requests, "ssh.intern.example", HOST);
        render(&mut state, &input(&theme, &repos, &glab, &probes, &probing));
        assert!(state.mr.form.is_none(), "Formular geschlossen");
        assert!(state.token_field_id().is_none());
    }

    /// Verworfener Entwurf (Schließen, „Speichern“, neuer `SettingsState`):
    /// `forget_token_fields` aus `logic()` räumt den Undo-Verlauf weg.
    #[test]
    fn stale_token_fields_are_forgotten() {
        let ctx = egui::Context::default();
        let mut old = MrSettingsState::default();
        old.open_form(&ctx, HOST, Protocol::Https, false);
        let stale = old.form.as_ref().expect("Formular").field_id;
        TextEditState::default().store(&ctx, stale);
        drop(old);
        let mut current = MrSettingsState::default();
        current.open_form(&ctx, HOST, Protocol::Https, false);
        let active = current.form.as_ref().expect("Formular").field_id;
        TextEditState::default().store(&ctx, active);
        forget_token_fields(&ctx, Some(active));
        assert!(TextEditState::load(&ctx, stale).is_none());
        assert!(
            TextEditState::load(&ctx, active).is_some(),
            "aktives Feld bleibt"
        );
        forget_token_fields(&ctx, None);
        assert!(TextEditState::load(&ctx, active).is_none());
    }

    /// Jedes glab-Urteil hat eine Zeile, auch „unbekannt“ (feingranulares
    /// Token, GitLab ohne `personal_access_tokens/self`).
    #[test]
    fn glab_rights_line_for_every_verdict() {
        let keys: Vec<&str> = [
            GlabRights::Full,
            GlabRights::ReadOnly,
            GlabRights::Insufficient,
            GlabRights::Unknown,
        ]
        .into_iter()
        .map(glab_rights_key)
        .collect();
        assert_eq!(
            keys,
            [
                "mrs_glab_full",
                "mrs_glab_readonly",
                "mrs_glab_insufficient",
                "mrs_glab_unknown"
            ]
        );
        for lang in [Language::De, Language::En] {
            assert!(tr(lang, "mrs_glab_unknown").starts_with("glab: "));
        }
    }
}
