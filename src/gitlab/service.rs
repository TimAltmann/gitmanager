//! Service der Merge-Request-Ansicht ohne UI: lokale glab-Erkennung (E16),
//! Abruf je GitLab-Host (Dashboard, Erwähnungen, „Nicht beteiligt“) mit
//! Teilfehlern, Prüfung von Anmeldung, Token-Rechten und Projektzugriff (E4)
//! sowie Token prüfen und in glab speichern (E3). Alle Aufrufe gehen über
//! `GlabRunner`; das Token nur als `SecretString` in Kind-Env bzw. Kind-stdin.

use super::glab::{
    classify, glab_version, EnvValue, GlabErrorKind, GlabOutput, GlabRequest, GlabRunner,
    StdinBody, CI_ENV, DEFAULT_TIMEOUT, SECRET_ENV, TOKEN_ENV,
};
use super::graphql::{
    self, parse_access, parse_dashboard, parse_mentions, parse_merged, parse_uninvolved,
    parse_uninvolved_page, run_query, run_query_with, unique_paths, GqlData, HostSchema,
    MergedPage, ProjectMrs, ProjectNode, SchemaErrorCode, UserNode,
};
use super::model::{
    build_snapshot, sort_uninvolved, HostRaw, HostStatus, MergeRequest, MergedHostPage, MrPart,
    MrSnapshot, UninvolvedCursor,
};
use super::remote::{norm_host, strip_port, GlabHost, HostMap, LocalProject};
use super::secret::SecretString;
use super::time::{format_rfc3339, now_unix, parse_date};
use crate::config::MrConfig;
use crate::i18n::{tr, tr_fmt, Language};
use semver::Version;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsString;
use std::path::PathBuf;

/// Beim Prüfen eines neuen Tokens entfernt; `GITLAB_TOKEN` setzt die Prüfung
/// selbst (als `EnvValue::Secret`).
const VERIFY_ENV_REMOVE: [&str; 3] = ["GITLAB_ACCESS_TOKEN", "OAUTH_TOKEN", "GLAB_IS_OAUTH2"];
/// `glab auth status --output json` gibt es ab 1.122.
const AUTH_STATUS_JSON: Version = Version::new(1, 122, 0);
/// REST-Nachprüfung in ACCESS fehlender Projekte: höchstens so viele je Prüfung.
const REST_FALLBACK_MAX: usize = 10;
/// Scopes und Ablauf eines PAT; jeder Scope darf ihn lesen (GitLab ≥ 15.5).
const SELF_ENDPOINT: &str = "personal_access_tokens/self";

/// Ergebnis eines Abrufs (`fetch_snapshot`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MrFetchResult {
    /// Fertiger Stand für die Ansicht (E1, E9, E11)
    pub snapshot: MrSnapshot,
    /// Rohdaten je GitLab-Host, in der Reihenfolge von `HostMap::gitlab_hosts`
    pub raw: Vec<HostRaw>,
    /// glab-Host-Schlüssel → angemeldeter Benutzer (`currentUser`)
    pub me_per_host: BTreeMap<String, String>,
    /// glab fehlt (`NotInstalled`) oder ist zu alt (`TooOld`): dann kein
    /// Aufruf an einen Host. Ohne GitLab-Hosts wird glab nicht gestartet.
    pub glab_error: Option<GlabErrorKind>,
}

/// Weitere Seite „Nicht beteiligt“ eines Hosts (`fetch_uninvolved_more`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninvolvedPage {
    pub host_key: String,
    /// Neue MRs ohne eigene; Dubletten (per `id`) entfernt der Aufrufer
    pub mrs: Vec<MergeRequest>,
    /// MRs, bei denen ich Reviewer bin (Review-Anfrage jenseits der ersten
    /// Seite): gehören wie in `build_snapshot` zu „Von anderen“
    pub reviewing: Vec<MergeRequest>,
    /// Nächste Seiten, nur Projekte mit `hasNextPage`
    pub more: Vec<UninvolvedCursor>,
}

/// Art der Anmeldung eines Hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenKind {
    /// Persönliches Zugriffstoken
    Pat,
    /// OAuth-Anmeldung über glab (`personal_access_tokens/self` → 400)
    Oauth,
    /// Projekt- oder Gruppen-Token (`currentUser.bot`)
    ProjectBot,
    /// Feingranulares Token (`granular: true`)
    Granular,
    /// Umgebungsvariable im GitManager-Prozess überschreibt die Anmeldung
    Env(&'static str),
    #[default]
    Unknown,
}

/// Wo glab das Token hält.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenSource {
    /// Betriebssystem-Keyring (Windows-Anmeldeinformationsverwaltung)
    Keyring,
    /// glab-Konfigurationsdatei (Klartext)
    ConfigFile,
    Env(&'static str),
    #[default]
    Unknown,
}

/// Urteil „reicht für GitManager“ (E4): `read_api` oder `api`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GitManagerRights {
    Ok,
    Missing,
    #[default]
    Unknown,
}

/// Urteil „glab allgemein“ (E4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlabRights {
    /// `api` oder OAuth
    Full,
    /// `read_api`: glab mr approve/merge/note/create schlagen fehl
    ReadOnly,
    Insufficient,
    #[default]
    Unknown,
}

/// Zugriff des Tokens auf ein lokales Projekt.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AccessState {
    /// Mitglied; `level` = GitLab-Zugriffsstufe (10 Guest … 50 Owner, 0 =
    /// unbekannt)
    Ok {
        level: u32,
    },
    /// Öffentlich, ohne Mitgliedschaft; MRs lesbar
    Public,
    /// Umbenannt oder verschoben; `to` = neuer Pfad, leer = unbekannt
    Renamed {
        to: String,
    },
    /// Nicht vorhanden oder ohne Zugriff
    NoAccess,
    MrsDisabled,
    Archived,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectAccess {
    /// Wie angefragt (`LocalProject::full_path`)
    pub full_path: String,
    pub state: AccessState,
}

/// Ergebnis von `probe_host`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostProbe {
    pub host_key: String,
    /// `None`, wenn `glab version` nicht lesbar war
    pub glab_version: Option<Version>,
    /// Fehler, der die Prüfung beendet hat (glab fehlt oder zu alt, nicht
    /// angemeldet, Token abgelaufen/widerrufen/ungültig, Netz, TLS, …)
    pub error: Option<GlabErrorKind>,
    /// Benutzername
    pub logged_in_as: Option<String>,
    /// Anzeigename
    pub display_name: Option<String>,
    /// `metadata.version`
    pub gitlab_version: Option<String>,
    pub token_kind: TokenKind,
    pub token_source: TokenSource,
    pub scopes: Vec<String>,
    /// Ablaufdatum (Unix-Sekunden, 00:00 UTC); `None` = unbefristet/unbekannt
    pub expires_at: Option<i64>,
    pub rights_gitmanager: GitManagerRights,
    pub rights_glab: GlabRights,
    /// GitLab verbirgt die To-dos (feingranulares Token ohne „Todo: Read“):
    /// der Ansicht fehlen alle Erwähnungen
    pub todos_hidden: bool,
    /// In der Reihenfolge der angefragten Pfade
    pub projects: Vec<ProjectAccess>,
}

/// API-Protokoll eines Hosts (E17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Protocol {
    #[default]
    Https,
    Http,
}

impl Protocol {
    pub fn scheme(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Http => "http",
        }
    }
}

/// Ergebnis von `verify_token` (Vorschau vor dem Speichern).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TokenCheck {
    pub user: Option<String>,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub rights_gitmanager: GitManagerRights,
    pub rights_glab: GlabRights,
    /// Wie `HostProbe::todos_hidden`
    pub todos_hidden: bool,
    /// Zugriff auf die angefragten Projekte
    pub access_preview: Vec<ProjectAccess>,
    /// Die bisherige Anmeldung hat `api`, das neue Token nicht (E4)
    pub downgrade_warning: bool,
}

/// Wo glab das Token gespeichert hat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedWhere {
    Keyring,
    ConfigFile,
    Unknown,
}

/// Welche Anmeldung eine Prüfung sieht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeEnv {
    /// Wie der Abruf: ein Env-Token (Name) des eigenen Prozesses gilt
    Inherit(Option<&'static str>),
    /// Nur die gespeicherte Anmeldung (Env-Tokens entfernt)
    Stored,
}

/// Ergänzt einen Aufruf um Kind-Env (Token, Protokoll) bzw. entfernt sie.
type Prepare<'a> = &'a dyn Fn(GlabRequest) -> GlabRequest;

// --- Gelernte Schemata --------------------------------------------------

/// Gelernte Schema-Reduktionen je Host für die Sitzung (`HostSchema`).
/// Nur im aufrufenden Thread lesen und schreiben, nie in den Workern.
#[cfg(not(test))]
fn with_schemas<T>(f: impl FnOnce(&mut HashMap<String, HostSchema>) -> T) -> T {
    use std::sync::{Mutex, OnceLock, PoisonError};
    static SCHEMAS: OnceLock<Mutex<HashMap<String, HostSchema>>> = OnceLock::new();
    let mut schemas = SCHEMAS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    f(&mut schemas)
}

/// Unter Tests je Thread: parallele Tests teilen sich sonst Gelerntes.
#[cfg(test)]
fn with_schemas<T>(f: impl FnOnce(&mut HashMap<String, HostSchema>) -> T) -> T {
    use std::cell::RefCell;
    thread_local! {
        static SCHEMAS: RefCell<HashMap<String, HostSchema>> = RefCell::default();
    }
    SCHEMAS.with(|schemas| f(&mut schemas.borrow_mut()))
}

fn schema_for(host: &str) -> HostSchema {
    with_schemas(|s| s.get(&host.to_lowercase()).cloned().unwrap_or_default())
}

/// Führt Gelerntes zusammen statt zu überschreiben: Abruf und Prüfung laufen
/// in verschiedenen Threads und lernen je auf einer eigenen Kopie. Eine
/// andere Server-Version ersetzt den Stand (dort wird neu gelernt).
fn store_schema(host: &str, schema: HostSchema) {
    with_schemas(|s| {
        let known = s.entry(host.to_lowercase()).or_default();
        let newer =
            known.version.is_some() && schema.version.is_some() && known.version != schema.version;
        if newer {
            *known = schema;
        } else {
            if schema.version.is_some() {
                known.version = schema.version;
            }
            known.removed.extend(schema.removed);
        }
    });
}

// --- Fehlerarten ----------------------------------------------------------

/// Nach diesen Fehlern geht im selben Durchlauf kein Aufruf mehr an den Host.
fn ends_host(kind: &GlabErrorKind) -> bool {
    use GlabErrorKind::{
        Expired, HostKey, InvalidToken, MissingContentType, Network, NotAuthenticated,
        NotInstalled, Revoked, Timeout, Tls,
    };
    matches!(
        kind,
        NotInstalled
            | NotAuthenticated
            | InvalidToken
            | Expired
            | Revoked
            | Network
            | Tls
            | Timeout
            | HostKey
            | MissingContentType
    )
}

/// Anmeldefehler: das Token (bzw. keins) wird nicht angenommen.
pub fn is_auth_error(kind: &GlabErrorKind) -> bool {
    use GlabErrorKind::{Expired, InvalidToken, NotAuthenticated, Revoked};
    matches!(kind, NotAuthenticated | InvalidToken | Expired | Revoked)
}

/// Kurztext für Abruf- und Prüffehler. `None`, wenn die Ansicht einen eigenen
/// Text hat (glab fehlt oder zu alt, nicht angemeldet, Token-Zustände) oder
/// nur die Details (gekürzter stderr) weiterhelfen.
pub fn error_text(kind: &GlabErrorKind, host: &str, lang: Language) -> Option<String> {
    let text = match kind {
        GlabErrorKind::InsufficientScope => tr(lang, "mr_err_scope"),
        GlabErrorKind::Network => tr_fmt(lang, "mr_err_network", &[host]),
        GlabErrorKind::Tls => tr_fmt(lang, "mr_err_tls", &[host]),
        GlabErrorKind::Timeout => {
            let secs = DEFAULT_TIMEOUT.as_secs().to_string();
            tr_fmt(lang, "mr_err_timeout", &[host, &secs])
        }
        GlabErrorKind::MissingContentType => tr(lang, "mr_err_content_type"),
        _ => return None,
    };
    Some(text)
}

// --- Lokale Erkennung (E16) ----------------------------------------------

/// Lokale glab-Erkennung (E16): je Kandidat `glab config get --host H user`,
/// bei angemeldeten Hosts auch `ssh_host` und `api_protocol`. Liest nur die
/// glab-Config (kein Netz, nie `token`), Env-Tokens entfernt. Liefert jeden
/// Kandidaten (kleingeschrieben, sortiert); ohne `user` = nicht angemeldet.
/// Fehlt glab, bleibt es bei einem Aufruf.
pub fn discover_glab_hosts(runner: &dyn GlabRunner, candidates: &[String]) -> Vec<GlabHost> {
    let keys: BTreeSet<String> = candidates
        .iter()
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    let mut missing = false;
    keys.into_iter()
        .map(|key| {
            let mut host = GlabHost {
                key,
                ..GlabHost::default()
            };
            if missing {
                return host;
            }
            match glab_config_get(runner, &host.key, "user") {
                Ok(Some(user)) => {
                    host.ssh_host = glab_config_get(runner, &host.key, "ssh_host")
                        .ok()
                        .flatten();
                    host.api_protocol = glab_config_get(runner, &host.key, "api_protocol")
                        .ok()
                        .flatten();
                    host.user = user;
                }
                Err(GlabErrorKind::NotInstalled) => missing = true,
                _ => {}
            }
            host
        })
        .collect()
}

/// `glab config get --host H <key>` (nur nicht geheime Schlüssel, ohne
/// Env-Tokens); leere Ausgabe → `None`.
fn glab_config_get(
    runner: &dyn GlabRunner,
    host: &str,
    key: &str,
) -> Result<Option<String>, GlabErrorKind> {
    let req = GlabRequest::read(Some(host), ["config", "get", "--host", host, key])
        .without_env(&TOKEN_ENV);
    let out = runner.run(req).map_err(|e| e.kind())?;
    if out.exit != 0 {
        return Err(classify(&out).unwrap_or(GlabErrorKind::Other));
    }
    let value = out.body.trim();
    Ok((!value.is_empty()).then(|| value.to_string()))
}

/// Host-Schlüssel aus der glab-Config (`hosts:`), kleingeschrieben und
/// sortiert. Kandidaten der lokalen Erkennung (E16): der angemeldete Host
/// heißt oft anders als der Remote-Host (Web-Host `gitlab.firma.de`, Remotes
/// per SSH auf eine IP, verbunden über `ssh_host`). Liest nur die Namen der
/// Hosts, nie ihre Werte (dort kann ein Token im Klartext stehen). Pfad über
/// `glab config path` (ab 1.115), sonst die bekannten Orte; ohne Datei leer.
pub fn glab_config_hosts(
    runner: &dyn GlabRunner,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Vec<String> {
    let req = GlabRequest::read(None, ["config", "path"]).without_env(&TOKEN_ENV);
    let shown = match runner.run(req) {
        Ok(out) if out.exit == 0 => Some(PathBuf::from(out.body.trim())),
        Ok(_) => None,
        Err(_) => return Vec::new(),
    };
    let path = shown
        .filter(|p| !p.as_os_str().is_empty())
        .into_iter()
        .chain(fallback_config_paths(env))
        .find(|p| p.is_file());
    let Some(path) = path else {
        return Vec::new();
    };
    // Kein Riese: die glab-Config hat wenige KB
    let small = std::fs::metadata(&path).is_ok_and(|m| m.len() <= 1024 * 1024);
    match std::fs::read_to_string(&path) {
        Ok(text) if small => parse_config_host_keys(&text),
        _ => Vec::new(),
    }
}

/// Bekannte Orte der glab-Config für glab ohne `config path`, in glabs
/// Reihenfolge: `GLAB_CONFIG_DIR`, `~/.config/glab-cli` (alt), unter Windows
/// `%LOCALAPPDATA%\glab-cli`.
fn fallback_config_paths(env: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let file = |dir: PathBuf| dir.join("config.yml");
    let mut paths = Vec::new();
    if let Some(dir) = env("GLAB_CONFIG_DIR").filter(|d| !d.is_empty()) {
        paths.push(file(PathBuf::from(dir)));
    }
    if let Some(home) = env("USERPROFILE")
        .or_else(|| env("HOME"))
        .filter(|d| !d.is_empty())
    {
        paths.push(file(PathBuf::from(home).join(".config").join("glab-cli")));
    }
    if let Some(local) = env("LOCALAPPDATA").filter(|d| !d.is_empty()) {
        paths.push(file(PathBuf::from(local).join("glab-cli")));
    }
    paths
}

/// Schlüssel der ersten Ebene unter `hosts:` (z. B. `gitlab.com`,
/// `"host:8443"`). Werte und tiefere Ebenen werden übersprungen.
fn parse_config_host_keys(text: &str) -> Vec<String> {
    let mut keys = BTreeSet::new();
    let mut in_hosts = false;
    let mut level: Option<usize> = None;
    for line in text.lines() {
        let line = line.trim_end();
        let content = line.trim_start();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        let indent = line.len() - content.len();
        if indent == 0 {
            in_hosts = content == "hosts:";
            level = None;
            continue;
        }
        if !in_hosts || indent != *level.get_or_insert(indent) {
            continue;
        }
        let Some(key) = content.strip_suffix(':') else {
            continue;
        };
        let key = key.trim().trim_matches(|c| c == '"' || c == '\'');
        if !key.is_empty() && !key.contains(char::is_whitespace) {
            keys.insert(key.to_lowercase());
        }
    }
    keys.into_iter().collect()
}

// --- Abruf ----------------------------------------------------------------

/// Abruf aller GitLab-Hosts (E16, E20): je Host Dashboard und Erwähnungen
/// (höchstens drei Seiten), mit `include_uninvolved` auch „Nicht beteiligt“
/// für die einbezogenen lokalen Projekte. Hosts laufen parallel, je Host
/// nacheinander (Gate). Teilfehler stehen in `HostStatus`; nach Anmelde-,
/// Netz-, TLS- und Timeout-Fehlern geht kein weiterer Aufruf an den Host.
/// Ohne GitLab-Hosts startet kein glab.
pub fn fetch_snapshot(
    runner: &dyn GlabRunner,
    hosts: &HostMap,
    locals: &[LocalProject],
    cfg: &MrConfig,
    include_uninvolved: bool,
) -> MrFetchResult {
    let mut result = MrFetchResult::default();
    if !hosts.gitlab_hosts.is_empty() {
        match glab_version(runner) {
            Err(kind @ (GlabErrorKind::NotInstalled | GlabErrorKind::TooOld(_))) => {
                result.glab_error = Some(kind);
            }
            // Version unlesbar: die Abfragen zeigen selbst, ob glab geht
            _ => {
                result.raw = fetch_hosts(runner, &hosts.gitlab_hosts, locals, include_uninvolved);
            }
        }
    }
    result.me_per_host = result
        .raw
        .iter()
        .filter_map(|raw| Some((raw.host_key.clone(), raw.status.user.clone()?)))
        .collect();
    result.snapshot = build_snapshot(&result.raw, locals, cfg, &result.me_per_host);
    result
}

/// Alle Hosts parallel (ein Thread je Host); das gelernte Schema bleibt je
/// Host für die Sitzung.
fn fetch_hosts(
    runner: &dyn GlabRunner,
    hosts: &[String],
    locals: &[LocalProject],
    include_uninvolved: bool,
) -> Vec<HostRaw> {
    let fetched_at = now_unix();
    let jobs: Vec<(String, Vec<String>, HostSchema)> = hosts
        .iter()
        .map(|host| {
            // E1: „Nicht beteiligt“ nur aus einbezogenen lokalen Projekten
            let paths = locals
                .iter()
                .filter(|p| p.included && p.host_key.eq_ignore_ascii_case(host))
                .map(|p| p.full_path.clone())
                .collect();
            (host.clone(), paths, schema_for(host))
        })
        .collect();
    let done: Vec<(HostRaw, HostSchema)> = std::thread::scope(|scope| {
        let workers: Vec<_> = jobs
            .into_iter()
            .map(|(host, paths, mut schema)| {
                scope.spawn(move || {
                    let raw = fetch_host(
                        runner,
                        &host,
                        &paths,
                        include_uninvolved,
                        fetched_at,
                        &mut schema,
                    );
                    (raw, schema)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| {
                w.join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    });
    done.into_iter()
        .map(|(raw, schema)| {
            store_schema(&raw.host_key, schema);
            raw
        })
        .collect()
}

/// Ein Host: Dashboard, Erwähnungen, ggf. „Nicht beteiligt“.
fn fetch_host(
    runner: &dyn GlabRunner,
    host: &str,
    paths: &[String],
    include_uninvolved: bool,
    fetched_at: i64,
    schema: &mut HostSchema,
) -> HostRaw {
    let mut raw = HostRaw {
        host_key: host.to_string(),
        status: HostStatus {
            host_key: host.to_string(),
            ..HostStatus::default()
        },
        fetched_at,
        ..HostRaw::default()
    };
    // Eigene MRs und Review-Anfragen
    match run_query(runner, host, schema, graphql::dashboard()) {
        Ok(got) => {
            let dashboard = parse_dashboard(&got.data);
            raw.status.version.clone_from(&dashboard.version);
            let user = dashboard.me.as_ref().and_then(|me| me.username.clone());
            let Some(user) = user else {
                // `currentUser: null` ohne Fehler: glab hat ohne Token gefragt
                raw.status.error = Some(if got.errors.is_empty() {
                    GlabErrorKind::NotAuthenticated
                } else {
                    GlabErrorKind::GraphQl(got.errors)
                });
                return raw;
            };
            raw.status.user = Some(user);
            raw.dashboard = Some(dashboard);
            note_graphql(&mut raw.status.partial, MrPart::Dashboard, got.errors);
        }
        Err(kind) if ends_host(&kind) => {
            raw.status.error = Some(kind);
            return raw;
        }
        Err(kind) => note(&mut raw.status.partial, MrPart::Dashboard, kind),
    }
    // Erwähnungen, höchstens `MENTIONS_MAX_PAGES` Seiten
    let mut dead = None;
    let mut after: Option<String> = None;
    for page in 1..=graphql::MENTIONS_MAX_PAGES {
        let got = match run_query(runner, host, schema, graphql::mentions(after.as_deref())) {
            Ok(got) => got,
            Err(kind) => {
                if ends_host(&kind) {
                    dead = Some(kind.clone());
                }
                note(&mut raw.status.partial, MrPart::Mentions, kind);
                break;
            }
        };
        let mentions = parse_mentions(&got.data);
        note_graphql(&mut raw.status.partial, MrPart::Mentions, got.errors);
        if raw.status.user.is_none() {
            raw.status.user = mentions.me.as_ref().and_then(|me| me.username.clone());
        }
        // Verborgene To-dos: weitere Seiten wären ebenso leer
        raw.status.todos_hidden |= mentions.hidden;
        after = mentions
            .next_cursor(page)
            .filter(|_| !mentions.hidden)
            .map(str::to_string);
        raw.mentions.extend(mentions.items);
        if after.is_none() {
            break;
        }
    }
    if include_uninvolved {
        match (dead, raw.status.user.clone()) {
            (Some(kind), _) => note(&mut raw.status.partial, MrPart::Uninvolved, kind),
            (None, Some(me)) => {
                raw.uninvolved =
                    fetch_uninvolved(runner, host, &me, paths, schema, &mut raw.status.partial);
                raw.uninvolved_counts_own = not_filter_removed(schema);
            }
            // Benutzer unbekannt (Dashboard und Erwähnungen gescheitert):
            // eigene MRs ließen sich nicht ausschließen; Grund wie beim Dashboard
            (None, None) => {
                let kind = raw
                    .status
                    .partial
                    .iter()
                    .find(|(part, _)| *part == MrPart::Dashboard)
                    .map_or(GlabErrorKind::Other, |(_, kind)| kind.clone());
                note(&mut raw.status.partial, MrPart::Uninvolved, kind);
            }
        }
    }
    raw
}

/// „Nicht beteiligt“ in Abschnitten zu `UNINVOLVED_CHUNK` Projekten. `None`,
/// wenn kein Abschnitt geladen wurde; ohne Pfade abgefragt und leer.
fn fetch_uninvolved(
    runner: &dyn GlabRunner,
    host: &str,
    me: &str,
    paths: &[String],
    schema: &mut HostSchema,
    partial: &mut Vec<(MrPart, GlabErrorKind)>,
) -> Option<Vec<ProjectMrs>> {
    let mut projects = Vec::new();
    let mut loaded = paths.is_empty();
    for query in graphql::uninvolved(paths, me) {
        match run_query(runner, host, schema, query) {
            Ok(got) => {
                projects.extend(parse_uninvolved(&got.data, me));
                note_graphql(partial, MrPart::Uninvolved, got.errors);
                loaded = true;
            }
            Err(kind) => {
                let stop = ends_host(&kind);
                note(partial, MrPart::Uninvolved, kind);
                if stop {
                    break;
                }
            }
        }
    }
    loaded.then_some(projects)
}

/// Teilfehler eintragen, jede Kombination nur einmal.
fn note(partial: &mut Vec<(MrPart, GlabErrorKind)>, part: MrPart, kind: GlabErrorKind) {
    if !partial.iter().any(|(p, k)| *p == part && *k == kind) {
        partial.push((part, kind));
    }
}

/// Teilerfolg (`data` neben `errors`, z. B. ein Feld `null`): die Daten
/// bleiben, die Meldungen gehen als Teilfehler in den Host-Status.
fn note_graphql(partial: &mut Vec<(MrPart, GlabErrorKind)>, part: MrPart, mut errors: Vec<String>) {
    // Ein leer geliefertes Pflichtfeld trifft nur einzelne Knoten (GraphQL
    // setzt den nächsten nullbaren Elternknoten auf null); die Daten sind
    // da, also kein Banner „… konnten nicht geladen werden“.
    errors.retain(|m| !is_null_violation(m));
    if !errors.is_empty() {
        note(partial, part, GlabErrorKind::GraphQl(errors));
    }
}

/// „Cannot return null for non-nullable field Typ.feld“ (Serverdaten, kein
/// Abfrage- oder Rechteproblem).
fn is_null_violation(message: &str) -> bool {
    message.starts_with("Cannot return null for non-nullable field")
}

/// Kennt der Server `not: {authorUsername}` nicht (Schema-Fallback), zählt
/// `count` auch eigene MRs.
fn not_filter_removed(schema: &HostSchema) -> bool {
    schema.removed.iter().any(|e| {
        e.code != SchemaErrorCode::UndefinedField && (e.name == "not" || e.name == "authorUsername")
    })
}

/// Eigene gemergte MRs ab `since` (Unix-Sekunden): je Host die erste Seite
/// oder mit Cursor die nächste. Hosts parallel (ein Thread je Host), je Host
/// das gelernte Schema; Fehler stehen in der Seite des Hosts.
pub fn fetch_merged(
    runner: &dyn GlabRunner,
    jobs: &[(String, Option<String>, Option<String>)],
    since: i64,
) -> Vec<MergedHostPage> {
    let since = format_rfc3339(since);
    std::thread::scope(|scope| {
        let workers: Vec<_> = jobs
            .iter()
            .map(|(host, me, cursor)| {
                let since = &since;
                scope.spawn(move || {
                    let mut schema = schema_for(host);
                    let query = graphql::merged(since, cursor.as_deref());
                    let page = run_query(runner, host, &mut schema, query).and_then(merged_page);
                    store_schema(host, schema);
                    MergedHostPage {
                        host_key: host.clone(),
                        me: me.clone(),
                        page,
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| {
                w.join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    })
}

/// Seite aus der Antwort; fehlt die Liste, zählen die Meldungen als Fehler
/// (sonst sähe es aus wie „nichts gemergt“).
fn merged_page(got: GqlData) -> Result<MergedPage, GlabErrorKind> {
    let Some(user) = got.data.get("currentUser").filter(|u| u.is_object()) else {
        return Err(if got.errors.is_empty() {
            GlabErrorKind::NotAuthenticated
        } else {
            GlabErrorKind::GraphQl(got.errors)
        });
    };
    let missing = user.get("authoredMergeRequests").is_none_or(Value::is_null);
    if missing && !got.errors.is_empty() {
        return Err(GlabErrorKind::GraphQl(got.errors));
    }
    Ok(parse_merged(&got.data))
}

/// Nächste Seite „Nicht beteiligt“ für die Projekte eines Hosts (Cursor aus
/// `UninvolvedList::more`; Cursor anderer Hosts werden übergangen). Ein
/// Aufruf je `UNINVOLVED_PAGE_CHUNK` Projekte (Komplexität), bei bis zu drei
/// Projekten also genau einer. Der erste Fehler bricht ab. MRs, bei denen
/// ich Reviewer bin, kommen getrennt (`reviewing`).
pub fn fetch_uninvolved_more(
    runner: &dyn GlabRunner,
    host: &str,
    me: &str,
    cursors: &[UninvolvedCursor],
) -> Result<UninvolvedPage, GlabErrorKind> {
    let mut page = UninvolvedPage {
        host_key: host.to_string(),
        ..UninvolvedPage::default()
    };
    let mut seen = HashSet::new();
    let pages: Vec<(String, String)> = cursors
        .iter()
        .filter(|c| c.host_key.eq_ignore_ascii_case(host))
        .filter(|c| seen.insert(c.full_path.to_lowercase()))
        .map(|c| (c.full_path.clone(), c.cursor.clone()))
        .collect();
    if pages.is_empty() {
        return Ok(page);
    }
    let mut schema = schema_for(host);
    let loaded = load_uninvolved_pages(runner, host, me, &pages, &mut schema, &mut page);
    store_schema(host, schema);
    loaded?;
    sort_uninvolved(&mut page.mrs);
    Ok(page)
}

fn load_uninvolved_pages(
    runner: &dyn GlabRunner,
    host: &str,
    me: &str,
    pages: &[(String, String)],
    schema: &mut HostSchema,
    page: &mut UninvolvedPage,
) -> Result<(), GlabErrorKind> {
    for query in graphql::uninvolved_page(pages, me) {
        let got = run_query(runner, host, schema, query)?;
        for project in parse_uninvolved_page(&got.data, me) {
            let next = project.page.as_ref().filter(|p| p.has_next_page);
            if let (Some(path), Some(cursor)) = (
                project.full_path.as_ref(),
                next.and_then(|p| p.end_cursor.clone()),
            ) {
                page.more.push(UninvolvedCursor {
                    host_key: host.to_string(),
                    full_path: path.clone(),
                    cursor,
                });
            }
            for node in project.mrs {
                if let Some(mut mr) = MergeRequest::from_node(host, node, Some(me)) {
                    // Die Cursor stammen aus lokalen Projekten (`LocalProject`)
                    mr.local = true;
                    if mr.my_review.is_some() {
                        page.reviewing.push(mr);
                    } else {
                        page.mrs.push(mr);
                    }
                }
            }
        }
    }
    Ok(())
}

// --- Prüfung (E4) -----------------------------------------------------------

/// Prüft Anmeldung, Token-Rechte (E4) und Zugriff auf die Projekte eines
/// Hosts so, wie der Abruf ihn sieht (ein Env-Token gilt): `glab version`,
/// `auth status --hostname H` (JSON ab 1.122, unabhängig vom Exit-Code),
/// `personal_access_tokens/self`, ACCESS, REST `projects/<pfad>` nur für
/// fehlende Pfade (höchstens 10). Ohne gespeicherte Anmeldung (und ohne
/// Env-Token) kein Netz-Aufruf. Nur für angemeldete Hosts oder nach einer
/// ausdrücklichen Aktion des Users aufrufen.
pub fn probe_host(runner: &dyn GlabRunner, host: &str, project_paths: &[String]) -> HostProbe {
    probe_as(
        runner,
        host,
        project_paths,
        ProbeEnv::Inherit(env_token_override()),
    )
}

fn probe_as(runner: &dyn GlabRunner, host: &str, paths: &[String], env: ProbeEnv) -> HostProbe {
    probe_with(runner, host, paths, env, glab_version(runner))
}

/// Wie `probe_as` mit schon gelesener `glab version`.
fn probe_with(
    runner: &dyn GlabRunner,
    host: &str,
    paths: &[String],
    env: ProbeEnv,
    version: Result<Version, GlabErrorKind>,
) -> HostProbe {
    let mut probe = HostProbe {
        host_key: host.to_string(),
        projects: unknown_access(paths),
        ..HostProbe::default()
    };
    let env_var = match env {
        ProbeEnv::Inherit(var) => var,
        ProbeEnv::Stored => None,
    };
    let prepare = |req: GlabRequest| match env {
        ProbeEnv::Inherit(_) => req,
        ProbeEnv::Stored => req.without_env(&TOKEN_ENV),
    };
    match version {
        Ok(version) => probe.glab_version = Some(version),
        Err(kind @ (GlabErrorKind::NotInstalled | GlabErrorKind::TooOld(_))) => {
            probe.error = Some(kind);
            return probe;
        }
        Err(_) => {}
    }
    let json = probe
        .glab_version
        .as_ref()
        .is_some_and(|v| *v >= AUTH_STATUS_JSON);
    let status = match read_auth_status(runner, host, json, &prepare) {
        Ok(status) => status,
        Err(kind) => {
            probe.error = Some(kind);
            return probe;
        }
    };
    if status.signed_in == Some(false) && env_var.is_none() {
        // Kein Token für den Host: jeder API-Aufruf ginge ohne Anmeldung raus
        probe.error = Some(GlabErrorKind::NotAuthenticated);
        return probe;
    }
    probe.token_source = match (env_var, status.source) {
        (Some(var), _) => TokenSource::Env(var),
        (None, Some(source)) => source,
        (None, None) if !json => keyring_flag(runner, host, status.signed_in),
        (None, None) => TokenSource::Unknown,
    };
    probe.logged_in_as = status.user;
    let info = match token_self(runner, host, &prepare) {
        Ok(info) => info,
        Err(kind) => {
            probe.error = Some(kind);
            return probe;
        }
    };
    probe.token_kind = token_kind(&info, None, env_var);
    let access = match access_probe(runner, host, paths, &prepare) {
        Ok(access) => access,
        Err(kind) => {
            probe.error = Some(kind);
            return probe;
        }
    };
    if let Some(kind) = graphql_rejects(&info, &access) {
        // Ohne PAT-Angaben entscheidet GraphQL: die Anmeldung wirkt nicht
        probe.error = Some(kind);
        return probe;
    }
    let me = access.me.as_ref();
    if probe.logged_in_as.is_none() {
        probe.logged_in_as = me.and_then(|m| m.username.clone());
    }
    probe.display_name = me.and_then(|m| m.name.clone());
    probe.gitlab_version.clone_from(&access.version);
    probe.token_kind = token_kind(&info, me.and_then(|m| m.bot), env_var);
    if let SelfInfo::Pat(pat) = &info {
        probe.scopes.clone_from(&pat.scopes);
        probe.expires_at = pat.expires_at;
    }
    (probe.rights_gitmanager, probe.rights_glab) = rights_for(&info, access.graphql_ok());
    probe.todos_hidden = access.todos_hidden;
    (probe.projects, probe.error) = project_access(runner, host, paths, &access, &prepare);
    probe
}

/// Was `glab auth status` über die Anmeldung eines Hosts sagt.
#[derive(Debug, Default)]
struct AuthStatus {
    /// `Some(false)`: für den Host ist kein Token gespeichert
    signed_in: Option<bool>,
    user: Option<String>,
    source: Option<TokenSource>,
    /// Feld `error` des JSON-Status
    error: Option<String>,
}

/// `glab auth status --hostname H` (mit `json` als `--output json`, ab
/// 1.122). Die Ausgabe zählt unabhängig vom Exit-Code (Exit 1 bei jedem
/// Fehler); ältere Versionen schreiben Text auf stderr. `Err` bei Fehlern,
/// nach denen kein Aufruf mehr an den Host geht: Prozess (Timeout, …) oder
/// Netz, TLS, Timeout laut Status (glab fragt dafür `GET /user`).
fn read_auth_status(
    runner: &dyn GlabRunner,
    host: &str,
    json: bool,
    prepare: Prepare<'_>,
) -> Result<AuthStatus, GlabErrorKind> {
    let mut args = vec!["auth", "status", "--hostname", host];
    if json {
        args.extend(["--output", "json"]);
    }
    let out = match runner.run(prepare(GlabRequest::read(Some(host), args))) {
        Ok(out) => out,
        Err(e) if ends_host(&e.kind()) => return Err(e.kind()),
        // z. B. Ausgabe zu groß: ohne Status weiter
        Err(_) => return Ok(AuthStatus::default()),
    };
    // JSON: nur das Feld `error` (der Rest enthält Hosts und URLs)
    let (status, detail) = match out.json().and_then(|v| auth_status_entry(&v, host)) {
        Some(status) => {
            let detail = status.error.clone().unwrap_or_default();
            (status, detail)
        }
        None => {
            let text = format!("{}\n{}", out.stderr_tail, out.body);
            (auth_status_text(&text), text)
        }
    };
    match transport_error(&detail) {
        Some(kind) if status.signed_in != Some(true) => Err(kind),
        _ => Ok(status),
    }
}

/// Netz-, TLS- oder Timeout-Fehler in einer Meldung ohne HTTP-Antwort.
fn transport_error(detail: &str) -> Option<GlabErrorKind> {
    let out = GlabOutput {
        exit: 1,
        stderr_tail: detail.to_string(),
        ..GlabOutput::default()
    };
    classify(&out).filter(|kind| {
        matches!(
            kind,
            GlabErrorKind::Network | GlabErrorKind::Tls | GlabErrorKind::Timeout
        )
    })
}

/// Eintrag des Hosts aus `{"hosts":[{host,state,user,error,token_source,…}]}`.
/// `None` ohne `hosts`-Liste (dann zählt der Text). Fehlt der Host in der
/// Liste, ist der Status unbekannt: Einträge anderer Hosts (Schlüssel anders
/// geschrieben, glab ohne Filter) sagen nichts über diesen.
fn auth_status_entry(json: &Value, host: &str) -> Option<AuthStatus> {
    let hosts = json.get("hosts")?.as_array()?;
    let host = norm_host(host);
    let entry = hosts.iter().find(|h| {
        h.get("host")
            .and_then(Value::as_str)
            .is_some_and(|k| norm_host(k) == host)
    });
    let Some(entry) = entry else {
        return Some(AuthStatus::default());
    };
    let field = |key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let source = field("token_source");
    let error = field("error");
    let not_configured =
        error.is_some_and(|e| e.to_lowercase().contains("has not been authenticated"));
    let signed_in = if source == Some("none") || not_configured {
        Some(false)
    } else if field("state") == Some("success") {
        Some(true)
    } else {
        None
    };
    Some(AuthStatus {
        signed_in,
        user: field("user").map(str::to_string),
        source: source.and_then(|s| match s {
            "keyring" => Some(TokenSource::Keyring),
            "config_file" => Some(TokenSource::ConfigFile),
            "environment" => Some(TokenSource::Env("GITLAB_TOKEN")),
            _ => None,
        }),
        error: error.map(str::to_string),
    })
}

/// Textausgabe älterer Versionen: „H has not been authenticated with glab“
/// bzw. „✓ Logged in to H as USER (…)“.
fn auth_status_text(text: &str) -> AuthStatus {
    if text.contains("has not been authenticated") {
        return AuthStatus {
            signed_in: Some(false),
            ..AuthStatus::default()
        };
    }
    let user = text
        .split_once("Logged in to ")
        .and_then(|(_, rest)| rest.split_once(" as "))
        .map(|(_, rest)| {
            rest.chars()
                .take_while(|c| !c.is_whitespace() && *c != '(')
                .collect::<String>()
        })
        .filter(|u| !u.is_empty());
    AuthStatus {
        signed_in: user.is_some().then_some(true),
        user,
        ..AuthStatus::default()
    }
}

/// Speicherort ohne JSON-Status (glab < 1.122): `use_keyring` aus der glab-Config.
fn keyring_flag(runner: &dyn GlabRunner, host: &str, signed_in: Option<bool>) -> TokenSource {
    match glab_config_get(runner, host, "use_keyring") {
        Ok(Some(v)) if v.eq_ignore_ascii_case("true") => TokenSource::Keyring,
        Ok(_) if signed_in == Some(true) => TokenSource::ConfigFile,
        _ => TokenSource::Unknown,
    }
}

/// Was `personal_access_tokens/self` über das Token sagt.
enum SelfInfo {
    Pat(PatInfo),
    /// 400: kein PAT (OAuth-Anmeldung über glab)
    NotPat,
    /// 404 (GitLab < 15.5) oder anderer Fehler: GraphQL entscheidet
    Unavailable,
}

struct PatInfo {
    scopes: Vec<String>,
    expires_at: Option<i64>,
    granular: bool,
}

/// `glab api -i personal_access_tokens/self`. `Err` bei Anmelde-, Netz-,
/// TLS- und Timeout-Fehlern (dann keine weiteren Aufrufe).
fn token_self(
    runner: &dyn GlabRunner,
    host: &str,
    prepare: Prepare<'_>,
) -> Result<SelfInfo, GlabErrorKind> {
    let out = runner
        .run(prepare(GlabRequest::api(host, [SELF_ENDPOINT])))
        .map_err(|e| e.kind())?;
    match out.http.as_ref().map(|h| h.code) {
        Some(200) => Ok(out.json().map_or(SelfInfo::Unavailable, |json| {
            SelfInfo::Pat(parse_pat(&json))
        })),
        Some(400) => Ok(SelfInfo::NotPat),
        _ => match classify(&out) {
            Some(kind) if ends_host(&kind) => Err(kind),
            _ => Ok(SelfInfo::Unavailable),
        },
    }
}

fn parse_pat(json: &Value) -> PatInfo {
    let scopes = json
        .get("scopes")
        .and_then(Value::as_array)
        .map(|s| {
            s.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    PatInfo {
        scopes,
        expires_at: json
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(parse_date),
        granular: json
            .get("granular")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// Ergebnis der ACCESS-Abfrage(n).
#[derive(Default)]
struct AccessProbe {
    /// `currentUser`; `None` = GraphQL kennt keinen Benutzer
    me: Option<UserNode>,
    version: Option<String>,
    /// Knoten aller Abschnitte; `None` = nicht vollständig abgefragt
    nodes: Option<Vec<ProjectNode>>,
    /// Anmelde- oder anderer nicht beendender Fehler der Abfrage
    error: Option<GlabErrorKind>,
    /// GitLab verbirgt die To-dos vor dem Token
    todos_hidden: bool,
}

impl AccessProbe {
    /// Nimmt GraphQL das Token an (`currentUser != null`)? `None` = unklar.
    fn graphql_ok(&self) -> Option<bool> {
        match (&self.me, &self.error) {
            (Some(_), _) => Some(true),
            (None, None) => Some(false),
            (None, Some(kind)) if is_auth_error(kind) => Some(false),
            (None, Some(_)) => None,
        }
    }
}

/// ACCESS in Abschnitten zu je 50 Pfaden mit dem gelernten Schema des Hosts.
/// Netz-, TLS- und Timeout-Fehler beenden die Prüfung (`Err`).
fn access_probe(
    runner: &dyn GlabRunner,
    host: &str,
    paths: &[String],
    prepare: Prepare<'_>,
) -> Result<AccessProbe, GlabErrorKind> {
    let mut schema = schema_for(host);
    let result = run_access(runner, host, paths, prepare, &mut schema);
    store_schema(host, schema);
    result
}

fn run_access(
    runner: &dyn GlabRunner,
    host: &str,
    paths: &[String],
    prepare: Prepare<'_>,
    schema: &mut HostSchema,
) -> Result<AccessProbe, GlabErrorKind> {
    let mut probe = AccessProbe::default();
    let mut nodes = Vec::new();
    for query in graphql::access(paths) {
        let got = match run_query_with(runner, host, schema, query, prepare) {
            Ok(got) => got,
            Err(kind) if is_auth_error(&kind) || !ends_host(&kind) => {
                probe.error = Some(kind);
                return Ok(probe);
            }
            Err(kind) => return Err(kind),
        };
        let access = parse_access(&got.data);
        // `currentUser: null`: öffentliche Projekte kämen trotzdem, zählen
        // aber nicht (unbekannt)
        if access.me.is_none() {
            return Ok(probe);
        }
        if probe.me.is_none() {
            probe.me = access.me;
            probe.version = access.version;
        }
        probe.todos_hidden |= access.todos_hidden;
        nodes.extend(access.projects);
    }
    probe.nodes = Some(nodes);
    Ok(probe)
}

/// Anmeldefehler aus GraphQL, wenn `self` nichts über das Token sagt
/// (OAuth, altes GitLab); bei einem PAT entscheiden dessen Scopes.
fn graphql_rejects(info: &SelfInfo, access: &AccessProbe) -> Option<GlabErrorKind> {
    if matches!(info, SelfInfo::Pat(_)) {
        return None;
    }
    access.error.clone().filter(is_auth_error)
}

/// Urteile nach E4: Scopes eines klassischen PAT; sonst entscheidet die
/// GraphQL-Probe für GitManager, eine OAuth-Anmeldung hat immer `api`.
fn rights_for(info: &SelfInfo, graphql_ok: Option<bool>) -> (GitManagerRights, GlabRights) {
    let by_graphql = match graphql_ok {
        Some(true) => GitManagerRights::Ok,
        Some(false) => GitManagerRights::Missing,
        None => GitManagerRights::Unknown,
    };
    match info {
        SelfInfo::Pat(pat) if !pat.granular => scope_rights(&pat.scopes),
        SelfInfo::NotPat if graphql_ok == Some(true) => (GitManagerRights::Ok, GlabRights::Full),
        _ => (by_graphql, GlabRights::Unknown),
    }
}

fn scope_rights(scopes: &[String]) -> (GitManagerRights, GlabRights) {
    let has = |scope: &str| scopes.iter().any(|s| s == scope);
    if has("api") {
        (GitManagerRights::Ok, GlabRights::Full)
    } else if has("read_api") {
        (GitManagerRights::Ok, GlabRights::ReadOnly)
    } else {
        (GitManagerRights::Missing, GlabRights::Insufficient)
    }
}

fn token_kind(info: &SelfInfo, bot: Option<bool>, env_var: Option<&'static str>) -> TokenKind {
    if let Some(var) = env_var {
        return TokenKind::Env(var);
    }
    match info {
        SelfInfo::Pat(pat) if pat.granular => TokenKind::Granular,
        SelfInfo::NotPat => TokenKind::Oauth,
        _ if bot == Some(true) => TokenKind::ProjectBot,
        SelfInfo::Pat(_) => TokenKind::Pat,
        SelfInfo::Unavailable => TokenKind::Unknown,
    }
}

fn unknown_access(paths: &[String]) -> Vec<ProjectAccess> {
    unique_paths(paths)
        .into_iter()
        .map(|full_path| ProjectAccess {
            full_path,
            state: AccessState::Unknown,
        })
        .collect()
}

/// Zugriff je angefragtem Pfad: Knoten aus ACCESS, fehlende Pfade per REST
/// nachgeprüft (Umbenennung, höchstens `REST_FALLBACK_MAX`), sonst kein
/// Zugriff. Ohne GraphQL-Benutzer ist alles unbekannt. Dazu der Fehler, nach
/// dem kein REST-Aufruf mehr an den Host ging (übrige Pfade unbekannt).
fn project_access(
    runner: &dyn GlabRunner,
    host: &str,
    paths: &[String],
    access: &AccessProbe,
    prepare: Prepare<'_>,
) -> (Vec<ProjectAccess>, Option<GlabErrorKind>) {
    let Some(nodes) = access.nodes.as_ref().filter(|_| access.me.is_some()) else {
        return (unknown_access(paths), None);
    };
    // GitLab vergleicht ohne Groß-/Kleinschreibung und liefert den kanonischen Pfad
    let by_path: HashMap<String, &ProjectNode> = nodes
        .iter()
        .filter_map(|n| Some((n.full_path.as_deref()?.to_lowercase(), n)))
        .collect();
    let mut rest_left = REST_FALLBACK_MAX;
    let mut dead = None;
    let projects = unique_paths(paths)
        .into_iter()
        .map(|full_path| {
            let state = match by_path.get(&full_path.to_lowercase()) {
                Some(node) => node_state(node),
                None if dead.is_some() => AccessState::Unknown,
                None if rest_left > 0 => {
                    rest_left -= 1;
                    rest_project(runner, host, &full_path, prepare).unwrap_or_else(|kind| {
                        dead = Some(kind);
                        AccessState::Unknown
                    })
                }
                // GraphQL liefert fehlende und unsichtbare Projekte nicht
                None => AccessState::NoAccess,
            };
            ProjectAccess { full_path, state }
        })
        .collect();
    (projects, dead)
}

/// Zugriff laut ACCESS: zugreifbar = lesbare MRs und MRs aktiv; die Stufe
/// ist nur Anzeige (öffentlich ohne Mitgliedschaft = Stufe 0).
fn node_state(node: &ProjectNode) -> AccessState {
    if node.archived == Some(true) {
        AccessState::Archived
    } else if node.merge_requests_enabled == Some(false) {
        AccessState::MrsDisabled
    } else if node.read_merge_request == Some(false) {
        AccessState::NoAccess
    } else {
        match node.access_level {
            Some(0) => AccessState::Public,
            level => AccessState::Ok {
                level: level.unwrap_or(0),
            },
        }
    }
}

/// REST `projects/<pfad>` für einen in ACCESS fehlenden Pfad. Den 301 eines
/// umbenannten Projekts folgt glab (Go-Standard, nicht geprüft) und liefert
/// den neuen Pfad; kommt die Weiterleitung selbst an, ist das Ziel
/// (`/projects/<id>`) ohne Pfad. `Err` nur bei Fehlern, nach denen weitere
/// Aufrufe an den Host sinnlos sind.
fn rest_project(
    runner: &dyn GlabRunner,
    host: &str,
    path: &str,
    prepare: Prepare<'_>,
) -> Result<AccessState, GlabErrorKind> {
    let endpoint = format!("projects/{}", encode_path(path));
    let out = runner
        .run(prepare(GlabRequest::api(host, [endpoint])))
        .map_err(|e| e.kind())?;
    match out.http.as_ref().map(|h| h.code) {
        Some(200) => Ok(out
            .json()
            .map_or(AccessState::Unknown, |json| rest_state(&json, path))),
        Some(301 | 302 | 307 | 308) => Ok(AccessState::Renamed { to: String::new() }),
        Some(404) => Ok(AccessState::NoAccess),
        _ => match classify(&out) {
            Some(kind) if ends_host(&kind) => Err(kind),
            _ => Ok(AccessState::Unknown),
        },
    }
}

fn rest_state(json: &Value, requested: &str) -> AccessState {
    let path = json
        .get("path_with_namespace")
        .and_then(Value::as_str)
        .unwrap_or(requested);
    if !path.eq_ignore_ascii_case(requested) {
        return AccessState::Renamed {
            to: path.to_string(),
        };
    }
    if json.get("archived").and_then(Value::as_bool) == Some(true) {
        return AccessState::Archived;
    }
    if json.get("merge_requests_enabled").and_then(Value::as_bool) == Some(false) {
        return AccessState::MrsDisabled;
    }
    let level = ["project_access", "group_access"]
        .iter()
        .filter_map(|key| {
            json.get("permissions")?
                .get(key)?
                .get("access_level")?
                .as_u64()
        })
        .max()
        .unwrap_or(0);
    match u32::try_from(level) {
        Ok(level) if level > 0 => AccessState::Ok { level },
        _ => AccessState::Public,
    }
}

/// Projektpfad als ein URL-Segment (`gruppe/projekt` → `gruppe%2Fprojekt`).
fn encode_path(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// --- Token prüfen und speichern (E3) ---------------------------------------

/// Prüft ein neues Token, bevor es gespeichert wird (E3, E4): Token nur als
/// Kind-Env `GITLAB_TOKEN` (`EnvValue::Secret`), andere Env-Tokens entfernt,
/// das gewählte Protokoll immer als `GLAB_API_PROTOCOL` (E17: sonst gälte
/// das `api_protocol` der glab-Config, ggf. `http` trotz Wahl „https“).
/// `personal_access_tokens/self`, ACCESS für `project_paths` (Vorschau),
/// REST `/user`, wenn GraphQL das Token ablehnt. Fehlt dem neuen Token `api`,
/// fragt ein Aufruf mit der bisherigen Anmeldung unter `login_key` (die
/// Anmeldung, die `save_token` ersetzt), ob glab damit Schreibrechte
/// verlöre. Ungültig, abgelaufen oder widerrufen → `Err`.
pub fn verify_token(
    runner: &dyn GlabRunner,
    host: &str,
    token: SecretString,
    protocol: Protocol,
    project_paths: &[String],
) -> Result<TokenCheck, GlabErrorKind> {
    let mut check = check_token(runner, host, &token, protocol, project_paths)?;
    drop(token);
    if matches!(
        check.rights_glab,
        GlabRights::ReadOnly | GlabRights::Insufficient
    ) {
        check.downgrade_warning = stored_login_full(runner, login_key(host));
    }
    Ok(check)
}

/// Eigene Kopie des Tokens für die Kind-Env genau eines Aufrufs. Bewusste
/// Ausnahme von „nie klonen“: `EnvValue` (Task 4) besitzt seinen Wert und
/// jeder Prozess braucht einen; die Kopie ist wieder ein `SecretString`
/// (Drop überschreibt) und lebt nur bis zum Ende des Aufrufs.
fn token_env(token: &SecretString) -> EnvValue {
    EnvValue::Secret(SecretString::new(token.expose().to_string()))
}

fn check_token(
    runner: &dyn GlabRunner,
    host: &str,
    token: &SecretString,
    protocol: Protocol,
    paths: &[String],
) -> Result<TokenCheck, GlabErrorKind> {
    let prepare = |req: GlabRequest| {
        req.without_env(&VERIFY_ENV_REMOVE)
            .with_env("GITLAB_TOKEN", token_env(token))
            .with_env(
                "GLAB_API_PROTOCOL",
                EnvValue::Plain(protocol.scheme().into()),
            )
    };
    let info = token_self(runner, host, &prepare)?;
    let access = access_probe(runner, host, paths, &prepare)?;
    if let Some(kind) = graphql_rejects(&info, &access) {
        return Err(kind);
    }
    // Nur ein bestätigtes Token zählt als gültig: `auth login` löscht die
    // bisherige Anmeldung zuerst (E3). Bestätigt = PAT laut `self` oder
    // GraphQL kennt den Benutzer; sonst (z. B. 502, falscher Host) Fehler.
    let pat = matches!(info, SelfInfo::Pat(_));
    if !pat && access.me.is_none() {
        return Err(access
            .error
            .clone()
            .unwrap_or(GlabErrorKind::NotAuthenticated));
    }
    let mut user = access.me.as_ref().and_then(|m| m.username.clone());
    if user.is_none() && pat {
        user = rest_username(runner, host, &prepare)?;
    }
    let (rights_gitmanager, rights_glab) = rights_for(&info, access.graphql_ok());
    let (scopes, expires_at) = match &info {
        SelfInfo::Pat(pat) => (pat.scopes.clone(), pat.expires_at),
        _ => (Vec::new(), None),
    };
    let (access_preview, dead) = project_access(runner, host, paths, &access, &prepare);
    if let Some(kind) = dead {
        return Err(kind);
    }
    Ok(TokenCheck {
        user,
        scopes,
        expires_at,
        rights_gitmanager,
        rights_glab,
        todos_hidden: access.todos_hidden,
        access_preview,
        downgrade_warning: false,
    })
}

/// Benutzername über REST `/user` (geht auch mit `read_user`). `Err` nur bei
/// Fehlern, nach denen kein Aufruf mehr an den Host geht.
fn rest_username(
    runner: &dyn GlabRunner,
    host: &str,
    prepare: Prepare<'_>,
) -> Result<Option<String>, GlabErrorKind> {
    let out = runner
        .run(prepare(GlabRequest::api(host, ["user"])))
        .map_err(|e| e.kind())?;
    if out.http.as_ref().map(|h| h.code) == Some(200) {
        return Ok(out
            .json()
            .and_then(|json| json.get("username")?.as_str().map(str::to_string)));
    }
    match classify(&out) {
        Some(kind) if ends_host(&kind) => Err(kind),
        _ => Ok(None),
    }
}

/// Hat die bisher gespeicherte Anmeldung (ohne Env-Tokens) `api`? Eine
/// OAuth-Anmeldung über glab hat es immer.
fn stored_login_full(runner: &dyn GlabRunner, host: &str) -> bool {
    let stored = |req: GlabRequest| req.without_env(&TOKEN_ENV);
    match token_self(runner, host, &stored) {
        Ok(SelfInfo::Pat(pat)) => !pat.granular && pat.scopes.iter().any(|s| s == "api"),
        Ok(SelfInfo::NotPat) => true,
        _ => false,
    }
}

/// Speichert das Token in glab (E3): erst `glab version` (fehlt oder zu alt
/// → kein Login, glab < 1.111 speichert im Klartext), dann prüfen (ungültig
/// oder unbestätigt → kein Login, die bestehende Anmeldung bleibt), dann
/// `glab auth login --hostname H --stdin [--api-host H:P] --api-protocol
/// https|http` mit dem Token nur auf stdin, ohne Env-Tokens und
/// CI-Variablen. Das Protokoll steht immer da: gespeichert wird die Wahl des
/// Formulars, nicht ein altes `api_protocol` der glab-Config.
/// Ein Schlüssel mit Port meldet sich unter `login_key` an. Erfolg = Exit 0
/// und die anschließende Prüfung der gespeicherten Anmeldung (ohne
/// Env-Token) liefert denselben Benutzer; glabs Meldungen („Logged in as“,
/// „Could not look up the username“) zählen nicht, aus ihnen wird nur der
/// Speicherort gelesen.
pub fn save_token(
    runner: &dyn GlabRunner,
    host: &str,
    token: SecretString,
    protocol: Protocol,
) -> Result<SavedWhere, GlabErrorKind> {
    let version = glab_version(runner);
    if let Err(kind @ (GlabErrorKind::NotInstalled | GlabErrorKind::TooOld(_))) = &version {
        return Err(kind.clone());
    }
    let check = check_token(runner, host, &token, protocol, &[])?;
    let key = login_key(host);
    let mut args = vec!["auth", "login", "--hostname", key, "--stdin"];
    if key != host {
        args.extend(["--api-host", host]);
    }
    args.extend(["--api-protocol", protocol.scheme()]);
    let login = GlabRequest::write(Some(key), args)
        .without_env(&TOKEN_ENV)
        .without_env(&CI_ENV)
        .with_stdin(StdinBody::Secret(token));
    let out = runner.run(login).map_err(|e| e.kind())?;
    if out.exit != 0 {
        return Err(classify(&out).unwrap_or(GlabErrorKind::Other));
    }
    let probe = probe_with(runner, key, &[], ProbeEnv::Stored, version);
    if let Some(kind) = probe.error {
        return Err(kind);
    }
    let same_user = match (&check.user, &probe.logged_in_as) {
        (Some(want), Some(got)) => want.eq_ignore_ascii_case(got),
        (None, got) => got.is_some(),
        (Some(_), None) => false,
    };
    if !same_user {
        return Err(GlabErrorKind::Other);
    }
    // Speicherort aus glabs Erfolgszeile; die Datei zuerst, weil eine
    // Keyring-Warnung davor den Keyring erwähnen kann
    let stderr = out.stderr_tail.to_lowercase();
    let stored_in =
        |place: &str| stderr.contains(&format!("stored your credentials in the {place}"));
    let saved = if stored_in("configuration file") {
        SavedWhere::ConfigFile
    } else if stored_in("operating system keyring") {
        SavedWhere::Keyring
    } else {
        match probe.token_source {
            TokenSource::Keyring => SavedWhere::Keyring,
            TokenSource::ConfigFile => SavedWhere::ConfigFile,
            _ => SavedWhere::Unknown,
        }
    };
    Ok(saved)
}

/// glab-Schlüssel, unter dem `save_token` die Anmeldung ablegt: `auth login
/// --hostname` lehnt `:` ab, ein Schlüssel `host:port` meldet sich daher
/// unter `host` an und gibt `host:port` als `--api-host` mit. Der Remote-Host
/// `host:port` erreicht diese Anmeldung über „gehört zu …“ (E16).
pub fn login_key(host: &str) -> &str {
    strip_port(host)
}

/// Gesetzte Env-Variable mit Token im eigenen Prozess (`GITLAB_TOKEN`,
/// `GITLAB_ACCESS_TOKEN`, `OAUTH_TOKEN`, in glabs Reihenfolge): glab nimmt
/// sie für jeden Host statt der gespeicherten Anmeldung.
pub fn env_token_override() -> Option<&'static str> {
    env_token_in(|key| std::env::var_os(key))
}

fn env_token_in(lookup: impl Fn(&str) -> Option<OsString>) -> Option<&'static str> {
    SECRET_ENV
        .into_iter()
        .find(|key| lookup(key).is_some_and(|v| !v.is_empty()))
}

/// Link „Token in GitLab erstellen“, vorbelegt mit Name und Scopes:
/// `read_api` (Nur lesen) bzw. `api,write_repository` (Voll).
pub fn token_create_url(host: &str, protocol: Protocol, read_only: bool) -> String {
    let scopes = if read_only {
        "read_api"
    } else {
        "api,write_repository"
    };
    format!(
        "{}://{}/-/user_settings/personal_access_tokens?name=GitManager&scopes={scopes}",
        protocol.scheme(),
        host.trim().trim_end_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitlab::glab::fake::FakeCall;
    use crate::gitlab::glab::{
        CallKind, EnvValue, FakeGlab, FakeReply, GatedRunner, GlabError, GlabOutput, GlabRequest,
        HttpHead, StdinBody,
    };
    use crate::gitlab::graphql::SchemaError;
    use crate::gitlab::model::{MrPart, MyReview};
    use crate::gitlab::remote::HostTarget;
    use crate::gitlab::time::parse_date;
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    const HOST: &str = "gitlab.example.com";
    const HOST_B: &str = "git.intern.example";
    const ME: &str = "taltmann";
    const TOKEN: &str = "glpat-Neu0815Geheim";

    const DASHBOARD_EE: &str = include_str!("fixtures/dashboard_ee.json");
    const DASHBOARD_CE: &str = include_str!("fixtures/dashboard_ce.json");
    const DASHBOARD_CE_UNDEFINED: &str = include_str!("fixtures/dashboard_ce_undefined.json");
    const CURRENT_USER_NULL: &str = include_str!("fixtures/current_user_null.json");
    const MENTIONS: &str = include_str!("fixtures/mentions.json");
    const UNINVOLVED: &str = include_str!("fixtures/uninvolved.json");
    const ACCESS: &str = include_str!("fixtures/access.json");
    const ARGUMENT_NOT_ACCEPTED: &str = include_str!("fixtures/argument_not_accepted.json");
    const AUTH_STATUS: &str = include_str!("fixtures/auth_status.json");
    const PAT_SELF: &str = include_str!("fixtures/pat_self.json");
    const PROJECT_REST: &str = include_str!("fixtures/project_rest.json");

    const EMPTY_MENTIONS: &str = r#"{"data":{"currentUser":{"username":"taltmann","todos":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]}}}}"#;
    const INVALID_TOKEN_GQL: &str = r#"{"errors":[{"message":"Invalid token"}]}"#;
    const NOT_A_PAT: &str = r#"{"message":"400 Bad request - This endpoint requires token type to be a personal access token"}"#;
    const EXPIRED: &str = r#"{"error":"invalid_token","error_description":"Token is expired. You can either do re-authorization or token refresh."}"#;
    const REVOKED: &str = r#"{"error":"invalid_token","error_description":"Token was revoked. You have to re-authorize from the user."}"#;
    const BAD_TOKEN: &str = r#"{"error":"invalid_token","error_description":"Bad Access Token."}"#;
    const NOT_FOUND: &str = r#"{"message":"404 Project Not Found"}"#;

    fn gated() -> GatedRunner<FakeGlab> {
        GatedRunner::new(FakeGlab::new())
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn secret(s: &str) -> SecretString {
        SecretString::new(s.to_string())
    }

    fn version_reply(v: &str) -> FakeReply {
        FakeReply::stdout(0, &format!("glab {v} (a300719)\n"), "")
    }

    /// Antwort von `glab api graphql -i`: HTTP 200, bei `errors` Exit 1.
    fn gql(body: &str) -> FakeReply {
        let failed = body.contains("\"errors\"");
        FakeReply::output(GlabOutput {
            exit: i32::from(failed),
            http: Some(HttpHead {
                code: 200,
                reason: "OK".to_string(),
            }),
            body: body.to_string(),
            stderr_tail: if failed {
                "glab: GraphQL-Fehler".to_string()
            } else {
                String::new()
            },
        })
    }

    /// Erwähnungen aus der Fixture, ohne weitere Seite.
    fn mentions_last_page() -> String {
        let mut v: Value = serde_json::from_str(MENTIONS).expect("Fixture");
        v["data"]["currentUser"]["todos"]["pageInfo"]["hasNextPage"] = json!(false);
        v.to_string()
    }

    /// `personal_access_tokens/self` mit diesen Scopes.
    fn pat_self(scopes: &[&str]) -> String {
        let mut v: Value = serde_json::from_str(PAT_SELF).expect("Fixture");
        v["scopes"] = json!(scopes);
        v.to_string()
    }

    /// `glab auth status -F json` für `HOST`.
    fn auth_status(state: &str, user: &str, source: &str) -> FakeReply {
        auth_status_hosts(&[(HOST, state, user, source)])
    }

    /// `glab auth status -F json` mit diesen Hosts; Exit 1, wenn einer
    /// fehlschlägt.
    fn auth_status_hosts(hosts: &[(&str, &str, &str, &str)]) -> FakeReply {
        let exit = i32::from(hosts.iter().any(|(_, state, ..)| *state != "success"));
        FakeReply::stdout(exit, &auth_status_json(hosts).to_string(), "")
    }

    /// Status-JSON mit einem Eintrag der Fixture je `(host, state, user,
    /// token_source)`, in dieser Reihenfolge.
    fn auth_status_json(hosts: &[(&str, &str, &str, &str)]) -> Value {
        let fixture: Value = serde_json::from_str(AUTH_STATUS).expect("Fixture");
        let entries: Vec<Value> = hosts
            .iter()
            .map(|(host, state, user, source)| {
                let mut entry = fixture["hosts"][0].clone();
                entry["host"] = json!(host);
                entry["state"] = json!(state);
                entry["user"] = json!(user);
                entry["token_source"] = json!(source);
                entry
            })
            .collect();
        json!({ "hosts": entries })
    }

    /// ACCESS nur mit `currentUser` (ohne Projekte).
    fn access_user(username: &str) -> String {
        json!({ "data": {
            "currentUser": { "username": username, "name": "Tim Altmann", "bot": false },
            "metadata": { "version": "18.5.1-ee", "enterprise": true }
        } })
        .to_string()
    }

    fn is_version(r: &GlabRequest) -> bool {
        r.args.first().is_some_and(|a| a == "version")
    }

    fn on(host: &'static str) -> impl Fn(&GlabRequest) -> bool + Send {
        move |r: &GlabRequest| r.host_key.as_deref() == Some(host)
    }

    /// GraphQL-Operation `op` (z. B. `GmDashboard`) an `host`.
    fn query(host: &'static str, op: &'static str) -> impl Fn(&GlabRequest) -> bool + Send {
        move |r: &GlabRequest| {
            r.host_key.as_deref() == Some(host)
                && matches!(&r.stdin, Some(StdinBody::Json(b)) if b.contains(&format!("query {op}")))
        }
    }

    fn config_get(host: &'static str, key: &'static str) -> impl Fn(&GlabRequest) -> bool + Send {
        move |r: &GlabRequest| {
            r.args.starts_with(&strings(&["config", "get"]))
                && r.args.last().is_some_and(|k| k == key)
                && r.host_key.as_deref() == Some(host)
        }
    }

    /// Kurzbeschreibung eines Aufrufs: `version`, `config get user`,
    /// `auth status`, `auth login`, GraphQL-Operation oder REST-Pfad.
    fn op_of(call: &FakeCall) -> String {
        let args = &call.request.args;
        match args.first().map(String::as_str) {
            Some("version") => "version".to_string(),
            Some("config") => format!("config get {}", args.last().unwrap()),
            Some("auth") => format!("auth {}", args[1]),
            Some("api") => match &call.request.stdin {
                Some(StdinBody::Json(body)) => {
                    let body: Value = serde_json::from_str(body).expect("JSON");
                    let text = body["query"].as_str().expect("query");
                    // `query GmMentions($after: String) { … }`
                    let name = text.split_whitespace().nth(1).unwrap();
                    name.split('(').next().unwrap().to_string()
                }
                _ => args.last().unwrap().clone(),
            },
            other => format!("{other:?}"),
        }
    }

    fn ops(calls: &[FakeCall]) -> Vec<String> {
        calls.iter().map(op_of).collect()
    }

    fn calls_to(calls: &[FakeCall], host: &str) -> usize {
        calls
            .iter()
            .filter(|c| c.request.host_key.as_deref() == Some(host))
            .count()
    }

    /// Variablen eines GraphQL-Aufrufs.
    fn variables(call: &FakeCall) -> Value {
        let Some(StdinBody::Json(body)) = &call.request.stdin else {
            panic!("kein JSON auf stdin");
        };
        serde_json::from_str::<Value>(body).expect("JSON")["variables"].clone()
    }

    fn query_text(call: &FakeCall) -> String {
        let Some(StdinBody::Json(body)) = &call.request.stdin else {
            panic!("kein JSON auf stdin");
        };
        serde_json::from_str::<Value>(body).expect("JSON")["query"]
            .as_str()
            .expect("query")
            .to_string()
    }

    fn host_map(hosts: &[&str]) -> HostMap {
        HostMap {
            remotes: hosts
                .iter()
                .map(|h| (h.to_string(), HostTarget::GitLab(h.to_string())))
                .collect(),
            gitlab_hosts: hosts.iter().map(|h| h.to_string()).collect(),
            ..HostMap::default()
        }
    }

    fn local(host: &str, path: &str) -> LocalProject {
        LocalProject {
            host_key: host.to_string(),
            full_path: path.to_string(),
            repos: vec![PathBuf::from(format!("/repos/{path}"))],
            included: true,
        }
    }

    fn iids(mrs: &[MergeRequest]) -> Vec<u64> {
        let mut iids: Vec<u64> = mrs.iter().map(|m| m.iid).collect();
        iids.sort_unstable();
        iids
    }

    // --- Step 1: Abruf --------------------------------------------------

    #[test]
    fn fetch_snapshot_two_hosts() {
        let runner = gated();
        let fake = runner.inner();
        fake.push_when(is_version, version_reply("1.123.0"));
        fake.push_when(query(HOST, "GmDashboard"), gql(DASHBOARD_EE));
        fake.push_when(query(HOST, "GmMentions"), gql(&mentions_last_page()));
        fake.push_when(query(HOST_B, "GmDashboard"), gql(DASHBOARD_CE));
        fake.push_when(query(HOST_B, "GmMentions"), gql(EMPTY_MENTIONS));
        let locals = [
            local(HOST, "kern/backend-api"),
            local(HOST_B, "tools/gitmanager"),
        ];
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST_B, HOST]),
            &locals,
            &MrConfig::default(),
            false,
        );
        assert_eq!(fake.pending(), 0);
        assert_eq!(got.glab_error, None);
        assert_eq!(
            ops(&fake.take_calls())
                .iter()
                .filter(|o| *o == "version")
                .count(),
            1
        );
        // Meine aus beiden Hosts, Von anderen = Review-Anfragen + Erwähnungen
        assert_eq!(iids(&got.snapshot.mine), vec![19, 87, 305, 412]);
        assert_eq!(iids(&got.snapshot.others), vec![77, 92, 310, 421]);
        assert_eq!(got.snapshot.uninvolved, None);
        let hosts: Vec<(&str, Option<&str>)> = got
            .snapshot
            .per_host
            .iter()
            .map(|h| (h.host_key.as_str(), h.user.as_deref()))
            .collect();
        assert_eq!(hosts, vec![(HOST_B, Some(ME)), (HOST, Some(ME))]);
        assert!(got
            .snapshot
            .per_host
            .iter()
            .all(|h| h.error.is_none() && h.partial.is_empty()));
        assert_eq!(got.raw.len(), 2);
        assert_eq!(got.me_per_host.get(HOST).map(String::as_str), Some(ME));
        assert_eq!(got.me_per_host.get(HOST_B).map(String::as_str), Some(ME));
    }

    #[test]
    fn fetch_snapshot_mentions_up_to_three_pages() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(DASHBOARD_EE));
        // Fixture meldet immer `hasNextPage`: höchstens drei Seiten
        for _ in 0..3 {
            fake.push(gql(MENTIONS));
        }
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &[],
            &MrConfig::default(),
            false,
        );
        let calls = fake.take_calls();
        assert_eq!(
            ops(&calls),
            vec![
                "version",
                "GmDashboard",
                "GmMentions",
                "GmMentions",
                "GmMentions"
            ]
        );
        assert_eq!(variables(&calls[2]).get("after"), None);
        assert_eq!(variables(&calls[3])["after"], json!("MTAwMjQ"));
        assert_eq!(got.raw[0].mentions.len(), 9);
    }

    #[test]
    fn fetch_snapshot_hidden_todos_marked_without_more_pages() {
        // Feingranulares Token ohne „Todo: Read“: leere Seite mit `hasNextPage`
        let hidden = r#"{"data":{"currentUser":{"username":"taltmann","todos":{"pageInfo":{"hasNextPage":true,"endCursor":"MTA"},"nodes":[]}}}}"#;
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(DASHBOARD_EE));
        fake.push(gql(hidden));
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &[local(HOST, "web/kundenportal")],
            &MrConfig::default(),
            false,
        );
        assert_eq!(
            ops(&fake.take_calls()),
            vec!["version", "GmDashboard", "GmMentions"]
        );
        let status = &got.snapshot.per_host[0];
        assert!(status.todos_hidden);
        assert_eq!(status.partial, Vec::new());
        // Review-Anfragen bleiben
        assert_eq!(iids(&got.snapshot.others), vec![92, 421]);
    }

    #[test]
    fn fetch_merged_pages_per_host_with_errors() {
        let runner = gated();
        let fake = runner.inner();
        let page = json!({ "data": { "currentUser": { "username": ME, "authoredMergeRequests": {
            "count": 1,
            "pageInfo": { "hasNextPage": false, "endCursor": null },
            "nodes": [{ "id": "gid://gitlab/MergeRequest/1", "iid": "408",
                        "mergedAt": "2026-10-17T08:00:00Z" }]
        } } } })
        .to_string();
        fake.push_when(query(HOST, "GmMerged"), gql(&page));
        // Liste `null` mit Meldung: Fehler statt „nichts gemergt“
        fake.push_when(
            query(HOST_B, "GmMerged"),
            gql(r#"{"data":{"currentUser":{"username":"taltmann","authoredMergeRequests":null}},"errors":[{"message":"Gemergte nicht verfügbar"}]}"#),
        );
        let jobs = vec![
            (HOST.to_string(), Some(ME.to_string()), None),
            (
                HOST_B.to_string(),
                Some(ME.to_string()),
                Some("MTA".to_string()),
            ),
        ];
        let since = parse_date("2026-09-17").expect("Datum");
        let pages = fetch_merged(&runner, &jobs, since);
        assert_eq!(fake.pending(), 0);
        let calls = fake.take_calls();
        let to_b = calls
            .iter()
            .find(|c| c.request.host_key.as_deref() == Some(HOST_B))
            .expect("Aufruf an HOST_B");
        assert_eq!(
            variables(to_b),
            json!({ "since": "2026-09-17T00:00:00Z", "after": "MTA" })
        );
        let hosts: Vec<&str> = pages.iter().map(|p| p.host_key.as_str()).collect();
        assert_eq!(hosts, vec![HOST, HOST_B]);
        let ok = pages[0].page.as_ref().expect("Seite");
        assert_eq!(ok.items.len(), 1);
        assert_eq!(ok.count, Some(1));
        assert_eq!(
            pages[1].page,
            Err(GlabErrorKind::GraphQl(strings(&[
                "Gemergte nicht verfügbar"
            ])))
        );

        // Ohne Anmeldung (`currentUser: null`)
        let runner = gated();
        runner.inner().push(gql(r#"{"data":{"currentUser":null}}"#));
        let pages = fetch_merged(&runner, &jobs[..1], since);
        assert_eq!(pages[0].page, Err(GlabErrorKind::NotAuthenticated));
    }

    #[test]
    fn fetch_snapshot_host_401_stops_only_that_host() {
        let runner = gated();
        let fake = runner.inner();
        fake.push_when(is_version, version_reply("1.123.0"));
        fake.push_when(
            query(HOST, "GmDashboard"),
            FakeReply::http(401, INVALID_TOKEN_GQL),
        );
        fake.push_when(query(HOST_B, "GmDashboard"), gql(DASHBOARD_CE));
        fake.push_when(query(HOST_B, "GmMentions"), gql(EMPTY_MENTIONS));
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST, HOST_B]),
            &[local(HOST, "kern/backend-api")],
            &MrConfig::default(),
            true,
        );
        let calls = fake.take_calls();
        assert_eq!(calls_to(&calls, HOST), 1, "{:?}", ops(&calls));
        let status = |host: &str| {
            got.snapshot
                .per_host
                .iter()
                .find(|h| h.host_key == host)
                .cloned()
                .expect("Host-Status")
        };
        assert_eq!(status(HOST).error, Some(GlabErrorKind::InvalidToken));
        assert_eq!(status(HOST_B).error, None);
        assert_eq!(iids(&got.snapshot.mine), vec![19]);
        assert_eq!(got.me_per_host.get(HOST), None);
    }

    #[test]
    fn fetch_snapshot_current_user_null_is_not_authenticated() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(CURRENT_USER_NULL));
        let got = fetch_snapshot(&runner, &host_map(&[HOST]), &[], &MrConfig::default(), true);
        assert_eq!(fake.take_calls().len(), 2);
        let status = &got.snapshot.per_host[0];
        assert_eq!(status.error, Some(GlabErrorKind::NotAuthenticated));
        assert_eq!(status.version.as_deref(), Some("18.5.1-ee"));
    }

    #[test]
    fn fetch_snapshot_partial_mentions_403() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(DASHBOARD_EE));
        fake.push(FakeReply::http(
            403,
            r#"{"error":"insufficient_scope","scope":"api read_api"}"#,
        ));
        fake.push(gql(UNINVOLVED));
        let locals = [
            local(HOST, "kern/backend-api"),
            local(HOST, "web/kundenportal"),
        ];
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &locals,
            &MrConfig::default(),
            true,
        );
        assert_eq!(
            ops(&fake.take_calls()),
            vec!["version", "GmDashboard", "GmMentions", "GmUninvolved"]
        );
        let status = &got.snapshot.per_host[0];
        assert_eq!(status.error, None);
        assert_eq!(
            status.partial,
            vec![(MrPart::Mentions, GlabErrorKind::InsufficientScope)]
        );
        // Daten der übrigen Abfragen bleiben
        assert_eq!(iids(&got.snapshot.mine), vec![87, 305, 412]);
        assert_eq!(iids(&got.snapshot.others), vec![92, 421]);
        let uninvolved = got.snapshot.uninvolved.expect("Nicht beteiligt geladen");
        assert_eq!(iids(&uninvolved.mrs), vec![95, 423]);
        assert!(!got.raw[0].uninvolved_counts_own);
    }

    #[test]
    fn fetch_snapshot_timeout_stops_calls_to_host() {
        // Timeout bei den Erwähnungen: keine weiteren Aufrufe, Rest als Teilfehler
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(DASHBOARD_EE));
        fake.push(FakeReply::error(GlabError::Timeout(Duration::from_secs(
            45,
        ))));
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &[local(HOST, "kern/backend-api")],
            &MrConfig::default(),
            true,
        );
        assert_eq!(
            ops(&fake.take_calls()),
            vec!["version", "GmDashboard", "GmMentions"]
        );
        let status = &got.snapshot.per_host[0];
        assert_eq!(status.error, None);
        assert_eq!(
            status.partial,
            vec![
                (MrPart::Mentions, GlabErrorKind::Timeout),
                (MrPart::Uninvolved, GlabErrorKind::Timeout),
            ]
        );
        assert_eq!(got.snapshot.mine.len(), 3);
        assert_eq!(got.raw[0].uninvolved, None);

        // Timeout schon beim Dashboard: der ganze Host fällt aus
        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::error(GlabError::Timeout(Duration::from_secs(
            45,
        ))));
        let got = fetch_snapshot(&runner, &host_map(&[HOST]), &[], &MrConfig::default(), true);
        assert_eq!(ops(&fake.take_calls()), vec!["version", "GmDashboard"]);
        assert_eq!(got.snapshot.per_host[0].error, Some(GlabErrorKind::Timeout));
        assert!(got.snapshot.per_host[0].partial.is_empty());
    }

    #[test]
    fn fetch_snapshot_ignores_null_violations_with_data() {
        // Server liefert ein Pflichtfeld leer (z. B. `Pipeline.detailedStatus`
        // bei gitlab.firma.example): die Daten sind da, also kein Banner
        // „… konnten nicht geladen werden“. Echte Teilfehler bleiben.
        let mut dashboard: Value = serde_json::from_str(DASHBOARD_EE).expect("Fixture");
        dashboard["errors"] = json!([
            {
                "message": "Cannot return null for non-nullable field Pipeline.detailedStatus",
                "path": ["currentUser", "authored", "nodes", 0, "headPipeline", "detailedStatus"]
            },
            { "message": "Review-Anfragen nicht verfügbar", "path": ["currentUser"] }
        ]);
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(&dashboard.to_string()));
        let mut mentions: Value = serde_json::from_str(&mentions_last_page()).expect("Fixture");
        mentions["errors"] = json!([{
            "message": "Cannot return null for non-nullable field Pipeline.detailedStatus",
            "path": ["currentUser", "todos", "nodes", 1, "targetEntity", "headPipeline"]
        }]);
        fake.push(gql(&mentions.to_string()));
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &[],
            &MrConfig::default(),
            false,
        );
        assert_eq!(fake.pending(), 0);
        let status = &got.snapshot.per_host[0];
        assert_eq!(status.error, None);
        assert_eq!(
            status.partial,
            vec![(
                MrPart::Dashboard,
                GlabErrorKind::GraphQl(strings(&["Review-Anfragen nicht verfügbar"]))
            )]
        );
        assert_eq!(iids(&got.snapshot.mine), vec![87, 305, 412]);
    }

    #[test]
    fn fetch_snapshot_graphql_partial_errors_in_host_status() {
        // `data` neben `errors` (glab endet mit 1): Daten bleiben, die
        // Meldungen stehen je Abfrage im Host-Status
        let with_errors = |body: &str, path: &str, message: &str| {
            let mut v: Value = serde_json::from_str(body).expect("Fixture");
            v["errors"] = json!([{ "message": message, "path": [path] }]);
            v.to_string()
        };
        let mut dashboard: Value = serde_json::from_str(DASHBOARD_EE).expect("Fixture");
        dashboard["data"]["currentUser"]["reviewing"] = Value::Null;
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(&with_errors(
            &dashboard.to_string(),
            "currentUser",
            "Review-Anfragen nicht verfügbar",
        )));
        fake.push(gql(&with_errors(
            &mentions_last_page(),
            "currentUser",
            "Todos teilweise nicht verfügbar",
        )));
        fake.push(gql(&with_errors(
            UNINVOLVED,
            "projects",
            "Projekt nicht verfügbar",
        )));
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &[local(HOST, "kern/backend-api")],
            &MrConfig::default(),
            true,
        );
        assert_eq!(fake.pending(), 0);
        let status = &got.snapshot.per_host[0];
        assert_eq!(status.error, None);
        assert_eq!(
            status.partial,
            vec![
                (
                    MrPart::Dashboard,
                    GlabErrorKind::GraphQl(strings(&["Review-Anfragen nicht verfügbar"]))
                ),
                (
                    MrPart::Mentions,
                    GlabErrorKind::GraphQl(strings(&["Todos teilweise nicht verfügbar"]))
                ),
                (
                    MrPart::Uninvolved,
                    GlabErrorKind::GraphQl(strings(&["Projekt nicht verfügbar"]))
                ),
            ]
        );
        assert_eq!(iids(&got.snapshot.mine), vec![87, 305, 412]);
        assert!(got.snapshot.uninvolved.is_some());
    }

    #[test]
    fn fetch_snapshot_uninvolved_skipped_without_user_is_noted() {
        // Dashboard und Erwähnungen scheitern (500): Benutzer unbekannt,
        // „Nicht beteiligt“ bleibt aus, mit Grund
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::http(
            500,
            r#"{"message":"500 Internal Server Error"}"#,
        ));
        fake.push(FakeReply::http(
            500,
            r#"{"message":"500 Internal Server Error"}"#,
        ));
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &[local(HOST, "kern/backend-api")],
            &MrConfig::default(),
            true,
        );
        assert_eq!(
            ops(&fake.take_calls()),
            vec!["version", "GmDashboard", "GmMentions"]
        );
        assert_eq!(got.raw[0].uninvolved, None);
        assert_eq!(
            got.snapshot.per_host[0].partial,
            vec![
                (MrPart::Dashboard, GlabErrorKind::Other),
                (MrPart::Mentions, GlabErrorKind::Other),
                (MrPart::Uninvolved, GlabErrorKind::Other),
            ]
        );
    }

    #[test]
    fn store_schema_merges_learned_reductions() {
        let removed = |name: &str| SchemaError {
            code: SchemaErrorCode::UndefinedField,
            type_name: "MergeRequest".to_string(),
            name: name.to_string(),
            field: None,
        };
        let schema = |version: &str, names: &[&str]| HostSchema {
            version: Some(version.to_string()),
            removed: names.iter().map(|n| removed(n)).collect(),
        };
        // Abruf und Prüfung lernen je auf ihrer Kopie: beides bleibt
        store_schema(HOST, schema("17.0.0", &["approvalsLeft"]));
        store_schema(HOST, schema("17.0.0", &["detailedMergeStatus"]));
        assert_eq!(
            schema_for(HOST),
            schema("17.0.0", &["approvalsLeft", "detailedMergeStatus"])
        );
        // ohne gelesene Version: Gelerntes kommt dazu, die Version bleibt
        store_schema(
            HOST,
            HostSchema {
                version: None,
                removed: [removed("draft")].into_iter().collect(),
            },
        );
        assert_eq!(
            schema_for(HOST),
            schema("17.0.0", &["approvalsLeft", "detailedMergeStatus", "draft"])
        );
        // neue Server-Version ersetzt den Stand
        store_schema(HOST, schema("18.5.1-ee", &[]));
        assert_eq!(schema_for(HOST), schema("18.5.1-ee", &[]));
    }

    #[test]
    fn fetch_snapshot_schema_fallback_is_remembered() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(DASHBOARD_CE_UNDEFINED));
        fake.push(gql(DASHBOARD_CE));
        fake.push(gql(EMPTY_MENTIONS));
        fake.push(gql(ARGUMENT_NOT_ACCEPTED));
        fake.push(gql(UNINVOLVED));
        let locals = [
            local(HOST, "kern/backend-api"),
            local(HOST, "web/kundenportal"),
        ];
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &locals,
            &MrConfig::default(),
            true,
        );
        let calls = fake.take_calls();
        assert_eq!(
            ops(&calls),
            vec![
                "version",
                "GmDashboard",
                "GmDashboard",
                "GmMentions",
                "GmUninvolved",
                "GmUninvolved"
            ]
        );
        assert!(!query_text(&calls[2]).contains("approvalsLeft"));
        assert!(!query_text(&calls[5]).contains("not:"));
        assert_eq!(got.snapshot.per_host[0].partial, Vec::new());
        assert_eq!(iids(&got.snapshot.mine), vec![19]);
        // Ohne `not` zählt `count` auch eigene MRs mit
        assert!(got.raw[0].uninvolved_counts_own);

        // Zweiter Abruf: die reduzierte Abfrage geht gleich beim ersten Mal
        fake.push(version_reply("1.123.0"));
        fake.push(gql(DASHBOARD_CE));
        fake.push(gql(EMPTY_MENTIONS));
        fake.push(gql(UNINVOLVED));
        let got = fetch_snapshot(
            &runner,
            &host_map(&[HOST]),
            &locals,
            &MrConfig::default(),
            true,
        );
        let calls = fake.take_calls();
        assert_eq!(
            ops(&calls),
            vec!["version", "GmDashboard", "GmMentions", "GmUninvolved"]
        );
        assert!(!query_text(&calls[1]).contains("approvalsRequired"));
        assert!(!query_text(&calls[3]).contains("not:"));
        assert!(got.raw[0].uninvolved_counts_own);
    }

    #[test]
    fn fetch_snapshot_without_gitlab_hosts_calls_nothing() {
        // Nicht angemeldeter Remote-Host (github.com): kein Aufruf, auch nicht
        // `glab version`
        let runner = gated();
        let hosts = HostMap {
            remotes: [("github.com".to_string(), HostTarget::NotGitLab)]
                .into_iter()
                .collect(),
            ..HostMap::default()
        };
        let got = fetch_snapshot(&runner, &hosts, &[], &MrConfig::default(), true);
        assert!(runner.inner().take_calls().is_empty());
        assert_eq!(got, MrFetchResult::default());

        // Neben einem GitLab-Host: nur dieser bekommt Aufrufe
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(gql(DASHBOARD_CE));
        fake.push(gql(EMPTY_MENTIONS));
        let mut hosts = host_map(&[HOST]);
        hosts
            .remotes
            .insert("github.com".to_string(), HostTarget::NotGitLab);
        fetch_snapshot(&runner, &hosts, &[], &MrConfig::default(), false);
        let calls = fake.take_calls();
        assert_eq!(calls.len(), 3);
        assert!(calls
            .iter()
            .all(|c| c.request.host_key.as_deref() != Some("github.com")
                && !c.request.args.iter().any(|a| a.contains("github"))));
    }

    #[test]
    fn fetch_snapshot_stops_when_glab_missing_or_too_old() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(FakeReply::error(GlabError::NotFound));
        let got = fetch_snapshot(&runner, &host_map(&[HOST]), &[], &MrConfig::default(), true);
        assert_eq!(got.glab_error, Some(GlabErrorKind::NotInstalled));
        assert_eq!(fake.take_calls().len(), 1);
        assert!(got.raw.is_empty());

        fake.push(version_reply("1.98.0"));
        let got = fetch_snapshot(&runner, &host_map(&[HOST]), &[], &MrConfig::default(), true);
        assert_eq!(
            got.glab_error,
            Some(GlabErrorKind::TooOld(Version::new(1, 98, 0)))
        );
        assert_eq!(fake.take_calls().len(), 1);
    }

    #[test]
    fn fetch_snapshot_parallel_hosts_one_call_per_host() {
        let runner = gated();
        let fake = runner.inner();
        fake.push_when(is_version, version_reply("1.123.0"));
        fake.push_when(query(HOST, "GmDashboard"), gql(DASHBOARD_EE).rendezvous(2));
        fake.push_when(
            query(HOST_B, "GmDashboard"),
            gql(DASHBOARD_CE).rendezvous(2),
        );
        for host in [HOST, HOST_B] {
            fake.push_when(
                query(host, "GmMentions"),
                gql(EMPTY_MENTIONS).delay(Duration::from_millis(20)),
            );
        }
        fetch_snapshot(
            &runner,
            &host_map(&[HOST, HOST_B]),
            &[],
            &MrConfig::default(),
            false,
        );
        assert_eq!(fake.pending(), 0);
        // Hosts laufen parallel, je Host höchstens ein glab-Prozess
        assert_eq!(fake.max_total(), 2);
        assert_eq!(fake.max_parallel(HOST), 1);
        assert_eq!(fake.max_parallel(HOST_B), 1);
    }

    #[test]
    fn auth_login_never_runs_parallel_to_other_calls() {
        let runner = Arc::new(gated());
        let fake = runner.inner();
        // Abruf an HOST
        fake.push_when(is_version, version_reply("1.123.0"));
        fake.push_when(
            query(HOST, "GmDashboard"),
            gql(DASHBOARD_EE).delay(Duration::from_millis(30)),
        );
        fake.push_when(
            query(HOST, "GmMentions"),
            gql(EMPTY_MENTIONS).delay(Duration::from_millis(30)),
        );
        // Token für HOST_B speichern: prüfen, anmelden, erneut prüfen
        fake.push_when(on(HOST_B), FakeReply::http(200, &pat_self(&["api"])));
        fake.push_when(on(HOST_B), gql(&access_user(ME)));
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(0, "", "✓ Logged in as taltmann\n").delay(Duration::from_millis(20)),
        );
        fake.push_when(is_version, version_reply("1.123.0"));
        let mut status: Value = serde_json::from_str(AUTH_STATUS).expect("Fixture");
        status["hosts"][0]["host"] = json!(HOST_B);
        fake.push_when(on(HOST_B), FakeReply::stdout(0, &status.to_string(), ""));
        fake.push_when(on(HOST_B), FakeReply::http(200, &pat_self(&["api"])));
        fake.push_when(on(HOST_B), gql(&access_user(ME)));

        let fetch = {
            let runner = Arc::clone(&runner);
            thread::spawn(move || {
                fetch_snapshot(
                    &*runner,
                    &host_map(&[HOST]),
                    &[],
                    &MrConfig::default(),
                    false,
                )
            })
        };
        let save = {
            let runner = Arc::clone(&runner);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(10));
                save_token(&*runner, HOST_B, secret(TOKEN), Protocol::Https)
            })
        };
        let fetched = fetch.join().unwrap();
        let saved = save.join().unwrap();
        assert_eq!(fetched.snapshot.mine.len(), 3);
        assert_eq!(saved, Ok(SavedWhere::Keyring));
        assert_eq!(fake.pending(), 0);
        assert!(!fake.write_overlapped());
    }

    #[test]
    fn discover_glab_hosts_only_local_config_without_env_tokens() {
        let runner = GatedRunner::new(FakeGlab::new().with_parent_env(&[
            ("GITLAB_TOKEN", "glpat-AusDerUmgebung"),
            ("OAUTH_TOKEN", "oauth-geheim"),
            ("GLAB_IS_OAUTH2", "true"),
        ]));
        let fake = runner.inner();
        fake.push_when(
            config_get(HOST, "user"),
            FakeReply::stdout(0, "taltmann\n", ""),
        );
        fake.push_when(
            config_get(HOST, "ssh_host"),
            FakeReply::stdout(0, "ssh.example.com\n", ""),
        );
        fake.push_when(
            config_get(HOST, "api_protocol"),
            FakeReply::stdout(0, "http\n", ""),
        );
        fake.push_when(
            config_get("github.com", "user"),
            FakeReply::stdout(0, "\n", ""),
        );
        let hosts = discover_glab_hosts(
            &runner,
            &strings(&["GitLab.Example.com", "github.com", " gitlab.example.com "]),
        );
        assert_eq!(
            hosts,
            vec![
                GlabHost {
                    key: "github.com".to_string(),
                    ..GlabHost::default()
                },
                GlabHost {
                    key: HOST.to_string(),
                    user: ME.to_string(),
                    ssh_host: Some("ssh.example.com".to_string()),
                    api_protocol: Some("http".to_string()),
                },
            ]
        );
        let calls = fake.take_calls();
        assert_eq!(calls.len(), 4);
        for call in &calls {
            assert_eq!(
                &call.request.args[..3],
                &strings(&["config", "get", "--host"])[..]
            );
            assert_eq!(call.request.kind, CallKind::Read);
            for key in ["GITLAB_TOKEN", "OAUTH_TOKEN", "GLAB_IS_OAUTH2"] {
                assert!(!call.env.contains_key(key), "{key} in {:?}", call.env);
            }
        }
    }

    /// glab-Config wie unter Windows (CRLF, 4 Leerzeichen Einrückung,
    /// Token im Klartext beim zweiten Host).
    const GLAB_CONFIG: &str = "git_protocol: ssh\r\n\
        # What editor glab should run\r\n\
        editor:\r\n\
        host: gitlab.com\r\n\
        hosts:\r\n\
        \x20   gitlab.com:\r\n\
        \x20       api_protocol: https\r\n\
        \x20       user:\r\n\
        \x20   GitLab.Example.com:\r\n\
        \x20       token: glpat-NichtAuslesen\r\n\
        \x20       ssh_host: git@10.0.0.5:2222\r\n\
        \x20       user: taltmann\r\n\
        \x20   \"git.intern.example:8443\":\r\n\
        \x20       user: tim\r\n\
        aliases:\r\n\
        \x20   ci: pipeline ci\r\n";

    #[test]
    fn parse_config_host_keys_reads_only_host_names() {
        assert_eq!(
            parse_config_host_keys(GLAB_CONFIG),
            strings(&[
                "git.intern.example:8443",
                "gitlab.com",
                "gitlab.example.com"
            ])
        );
        assert!(parse_config_host_keys("hosts: {}\n").is_empty());
        assert!(parse_config_host_keys("git_protocol: ssh\n").is_empty());
        assert!(parse_config_host_keys("").is_empty());
        // Tabs/Kommentare/Leerzeilen innerhalb von hosts stören nicht
        let text = "hosts:\n\n  # Kommentar\n  a.example:\n    user: x\n  b.example:\n";
        assert_eq!(
            parse_config_host_keys(text),
            strings(&["a.example", "b.example"])
        );
    }

    #[test]
    fn glab_config_hosts_via_config_path_without_env_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yml");
        std::fs::write(&path, GLAB_CONFIG).unwrap();
        let runner = GatedRunner::new(
            FakeGlab::new().with_parent_env(&[("GITLAB_TOKEN", "glpat-AusDerUmgebung")]),
        );
        let fake = runner.inner();
        let shown = format!("{}\n", path.display());
        fake.push_when(
            |r: &GlabRequest| r.args == strings(&["config", "path"]),
            FakeReply::stdout(0, &shown, ""),
        );
        let hosts = glab_config_hosts(&runner, &|_| None);
        assert_eq!(
            hosts,
            strings(&[
                "git.intern.example:8443",
                "gitlab.com",
                "gitlab.example.com"
            ])
        );
        let calls = fake.take_calls();
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].env.contains_key("GITLAB_TOKEN"));
        assert_eq!(calls[0].request.kind, CallKind::Read);
    }

    #[test]
    fn glab_config_hosts_falls_back_for_old_glab() {
        // glab < 1.115 kennt `config path` nicht → bekannte Orte
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.yml"), GLAB_CONFIG).unwrap();
        let runner = gated();
        runner.inner().push(FakeReply::stdout(
            1,
            "",
            "Error: unknown command \"path\" for \"glab config\"",
        ));
        let dir_os = dir.path().as_os_str().to_owned();
        let env = move |key: &str| (key == "GLAB_CONFIG_DIR").then(|| dir_os.clone());
        assert_eq!(glab_config_hosts(&runner, &env).len(), 3);
    }

    #[test]
    fn glab_config_hosts_empty_when_missing_or_unreadable() {
        let runner = gated();
        runner
            .inner()
            .push(FakeReply::stdout(0, "/gibt/es/nicht/config.yml\n", ""));
        assert!(glab_config_hosts(&runner, &|_| None).is_empty());
        let runner = gated();
        runner.inner().push(FakeReply::error(GlabError::NotFound));
        assert!(glab_config_hosts(&runner, &|_| None).is_empty());
    }

    #[test]
    fn fallback_config_paths_order() {
        let env = |key: &str| match key {
            "GLAB_CONFIG_DIR" => Some(OsString::from("/cfg")),
            "USERPROFILE" | "HOME" => Some(OsString::from("/home/u")),
            "LOCALAPPDATA" => Some(OsString::from("/local")),
            _ => None,
        };
        let paths = fallback_config_paths(&env);
        assert_eq!(paths[0], PathBuf::from("/cfg").join("config.yml"));
        assert!(paths.contains(
            &PathBuf::from("/home/u")
                .join(".config")
                .join("glab-cli")
                .join("config.yml")
        ));
        assert!(paths.contains(&PathBuf::from("/local").join("glab-cli").join("config.yml")));
        assert!(fallback_config_paths(&|_| None).is_empty());
    }

    #[test]
    fn discover_glab_hosts_stops_when_glab_missing() {
        let runner = gated();
        runner.inner().push(FakeReply::error(GlabError::NotFound));
        let hosts = discover_glab_hosts(&runner, &strings(&[HOST, HOST_B]));
        assert_eq!(runner.inner().take_calls().len(), 1);
        assert_eq!(hosts.len(), 2);
        assert!(hosts.iter().all(|h| h.user.is_empty()));
    }

    /// Nächste Seite für zwei Projekte; nur das erste hat weitere. In !97 bin
    /// ich Reviewer (Review-Anfrage jenseits der ersten Seite).
    fn uninvolved_page_reply() -> FakeReply {
        let mr = |iid: u64, author: &str| {
            json!({
                "id": format!("gid://gitlab/MergeRequest/{iid}"),
                "iid": iid.to_string(),
                "title": format!("MR {iid}"),
                "updatedAt": "2026-10-16T08:00:00Z",
                "author": { "username": author, "name": author }
            })
        };
        let mut reviewing = mr(97, "alehmann");
        reviewing["reviewers"] = json!({ "nodes": [{
            "username": ME,
            "name": "Tim Altmann",
            "mergeRequestInteraction": { "reviewState": "UNREVIEWED" }
        }] });
        let body = json!({ "data": {
            "p0": { "fullPath": "web/kundenportal", "mergeRequests": {
                "count": 5,
                "pageInfo": { "hasNextPage": true, "endCursor": "portal-3" },
                "nodes": [mr(96, "alehmann"), reviewing]
            } },
            "p1": { "fullPath": "kern/backend-api", "mergeRequests": {
                "count": 3,
                "pageInfo": { "hasNextPage": false, "endCursor": "api-2" },
                "nodes": [mr(424, "fbraun")]
            } }
        } });
        gql(&body.to_string())
    }

    fn cursor(host: &str, path: &str, cursor: &str) -> UninvolvedCursor {
        UninvolvedCursor {
            host_key: host.to_string(),
            full_path: path.to_string(),
            cursor: cursor.to_string(),
        }
    }

    #[test]
    fn fetch_uninvolved_more_one_call_for_the_host() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(uninvolved_page_reply());
        let cursors = [
            cursor(HOST, "web/kundenportal", "portal-2"),
            cursor(HOST_B, "tools/gitmanager", "gm-2"),
            cursor(HOST, "kern/backend-api", "api-1"),
        ];
        let page = fetch_uninvolved_more(&runner, HOST, ME, &cursors).expect("Seite");
        let calls = fake.take_calls();
        assert_eq!(ops(&calls), vec!["GmUninvolvedPage"]);
        assert_eq!(calls[0].request.host_key.as_deref(), Some(HOST));
        let vars = variables(&calls[0]);
        assert_eq!(vars["p0"], json!("web/kundenportal"));
        assert_eq!(vars["c0"], json!("portal-2"));
        assert_eq!(vars["p1"], json!("kern/backend-api"));
        assert_eq!(vars["c1"], json!("api-1"));
        assert_eq!(vars["me"], json!(ME));
        assert_eq!(page.host_key, HOST);
        assert_eq!(iids(&page.mrs), vec![96, 424]);
        assert!(page.mrs.iter().all(|m| m.host_key == HOST && m.local));
        // Reviewer-MR gehört zu „Von anderen“, nicht zu „Nicht beteiligt“
        assert_eq!(iids(&page.reviewing), vec![97]);
        assert_eq!(page.reviewing[0].my_review, Some(MyReview::Pending));
        assert_eq!(
            page.mrs
                .iter()
                .find(|m| m.iid == 424)
                .map(|m| m.project_path.as_str()),
            Some("kern/backend-api")
        );
        // weitere Seiten nur für Projekte mit `hasNextPage`
        assert_eq!(
            page.more,
            vec![cursor(HOST, "web/kundenportal", "portal-3")]
        );

        // ohne Cursor für den Host: kein Aufruf
        let page = fetch_uninvolved_more(&runner, HOST_B, ME, &cursors[..1]).expect("leer");
        assert_eq!(
            page,
            UninvolvedPage {
                host_key: HOST_B.to_string(),
                ..UninvolvedPage::default()
            }
        );
        assert!(fake.take_calls().is_empty());
    }

    #[test]
    fn fetch_uninvolved_more_error_kind() {
        let runner = gated();
        runner.inner().push(FakeReply::http(401, INVALID_TOKEN_GQL));
        let got = fetch_uninvolved_more(
            &runner,
            HOST,
            ME,
            &[cursor(HOST, "web/kundenportal", "portal-2")],
        );
        assert_eq!(got, Err(GlabErrorKind::InvalidToken));
    }

    // --- Step 2: Prüfung ------------------------------------------------

    /// Standardablauf einer Prüfung mit glab 1.123: Version, `auth status`
    /// (JSON), `personal_access_tokens/self`, ACCESS.
    fn push_probe(fake: &FakeGlab, self_reply: FakeReply, access: FakeReply) {
        fake.push(version_reply("1.123.0"));
        fake.push(auth_status("success", ME, "keyring"));
        fake.push(self_reply);
        fake.push(access);
    }

    fn paths3() -> Vec<String> {
        strings(&["kern/backend-api", "oss/public-lib", "alt/legacy-tool"])
    }

    fn states(probe: &HostProbe) -> Vec<AccessState> {
        probe.projects.iter().map(|p| p.state.clone()).collect()
    }

    #[test]
    fn probe_pat_read_api() {
        let runner = gated();
        let fake = runner.inner();
        push_probe(
            fake,
            FakeReply::http(200, &pat_self(&["read_api"])),
            gql(ACCESS),
        );
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        let calls = fake.take_calls();
        assert_eq!(
            ops(&calls),
            vec![
                "version",
                "auth status",
                "personal_access_tokens/self",
                "GmAccess"
            ]
        );
        assert_eq!(
            calls[1].request.args,
            strings(&["auth", "status", "--hostname", HOST, "--output", "json"])
        );
        assert_eq!(probe.host_key, HOST);
        assert_eq!(probe.glab_version, Some(Version::new(1, 123, 0)));
        assert_eq!(probe.error, None);
        assert_eq!(probe.logged_in_as.as_deref(), Some(ME));
        assert_eq!(probe.display_name.as_deref(), Some("Tim Altmann"));
        assert_eq!(probe.gitlab_version.as_deref(), Some("18.5.1-ee"));
        assert_eq!(probe.token_kind, TokenKind::Pat);
        assert_eq!(probe.token_source, TokenSource::Keyring);
        assert_eq!(probe.scopes, strings(&["read_api"]));
        assert_eq!(probe.expires_at, parse_date("2027-03-01"));
        assert_eq!(probe.rights_gitmanager, GitManagerRights::Ok);
        assert_eq!(probe.rights_glab, GlabRights::ReadOnly);
        assert_eq!(
            probe.projects,
            vec![
                ProjectAccess {
                    full_path: "kern/backend-api".to_string(),
                    state: AccessState::Ok { level: 30 },
                },
                ProjectAccess {
                    full_path: "oss/public-lib".to_string(),
                    state: AccessState::Public,
                },
                ProjectAccess {
                    full_path: "alt/legacy-tool".to_string(),
                    state: AccessState::Archived,
                },
            ]
        );
    }

    #[test]
    fn probe_pat_api_is_full() {
        let runner = gated();
        push_probe(
            runner.inner(),
            FakeReply::http(200, &pat_self(&["api", "write_repository"])),
            gql(ACCESS),
        );
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(probe.rights_gitmanager, GitManagerRights::Ok);
        assert_eq!(probe.rights_glab, GlabRights::Full);
        assert_eq!(probe.scopes, strings(&["api", "write_repository"]));
    }

    #[test]
    fn probe_read_user_only_is_insufficient_despite_auth_status() {
        // `auth status` meldet „angemeldet“ (REST /user geht mit read_user),
        // GraphQL lehnt das Token aber ab.
        let runner = gated();
        push_probe(
            runner.inner(),
            FakeReply::http(200, &pat_self(&["read_user"])),
            FakeReply::http(401, INVALID_TOKEN_GQL),
        );
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(probe.error, None);
        assert_eq!(probe.logged_in_as.as_deref(), Some(ME));
        assert_eq!(probe.rights_gitmanager, GitManagerRights::Missing);
        assert_eq!(probe.rights_glab, GlabRights::Insufficient);
        assert_eq!(states(&probe), vec![AccessState::Unknown; 3]);
    }

    #[test]
    fn probe_oauth_full_after_graphql_probe() {
        let runner = gated();
        push_probe(runner.inner(), FakeReply::http(400, NOT_A_PAT), gql(ACCESS));
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(probe.error, None);
        assert_eq!(probe.token_kind, TokenKind::Oauth);
        assert!(probe.scopes.is_empty());
        assert_eq!(probe.rights_gitmanager, GitManagerRights::Ok);
        assert_eq!(probe.rights_glab, GlabRights::Full);

        // OAuth, aber GraphQL lehnt ab: abgelaufene Anmeldung
        let runner = gated();
        push_probe(
            runner.inner(),
            FakeReply::http(400, NOT_A_PAT),
            FakeReply::http(401, INVALID_TOKEN_GQL),
        );
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(probe.error, Some(GlabErrorKind::InvalidToken));
        assert_eq!(probe.rights_gitmanager, GitManagerRights::Unknown);
        assert_eq!(probe.rights_glab, GlabRights::Unknown);
    }

    #[test]
    fn probe_granular_and_project_bot() {
        let runner = gated();
        let mut granular: Value = serde_json::from_str(&pat_self(&[])).expect("JSON");
        granular["granular"] = json!(true);
        push_probe(
            runner.inner(),
            FakeReply::http(200, &granular.to_string()),
            gql(ACCESS),
        );
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(probe.token_kind, TokenKind::Granular);
        assert_eq!(probe.rights_gitmanager, GitManagerRights::Ok);
        assert_eq!(probe.rights_glab, GlabRights::Unknown);
        assert!(!probe.todos_hidden);

        // Ohne „Todo: Read“: leere To-do-Seite mit `hasNextPage`
        let runner = gated();
        let mut hidden: Value = serde_json::from_str(ACCESS).expect("Fixture");
        hidden["data"]["currentUser"]["todos"] =
            json!({ "pageInfo": { "hasNextPage": true }, "nodes": [] });
        push_probe(
            runner.inner(),
            FakeReply::http(200, &granular.to_string()),
            gql(&hidden.to_string()),
        );
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(probe.rights_gitmanager, GitManagerRights::Ok);
        assert!(probe.todos_hidden);

        let runner = gated();
        let mut bot: Value = serde_json::from_str(ACCESS).expect("Fixture");
        bot["data"]["currentUser"]["bot"] = json!(true);
        push_probe(
            runner.inner(),
            FakeReply::http(200, &pat_self(&["read_api"])),
            gql(&bot.to_string()),
        );
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(probe.token_kind, TokenKind::ProjectBot);
        assert_eq!(probe.rights_glab, GlabRights::ReadOnly);
    }

    #[test]
    fn probe_expired_and_revoked_stop() {
        for (body, want) in [
            (EXPIRED, GlabErrorKind::Expired),
            (REVOKED, GlabErrorKind::Revoked),
        ] {
            let runner = gated();
            let fake = runner.inner();
            fake.push(version_reply("1.123.0"));
            fake.push(auth_status("error", "", "keyring"));
            fake.push(FakeReply::http(401, body));
            let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
            assert_eq!(
                ops(&fake.take_calls()),
                vec!["version", "auth status", "personal_access_tokens/self"]
            );
            assert_eq!(probe.error, Some(want));
            assert_eq!(probe.token_source, TokenSource::Keyring);
            assert_eq!(probe.rights_gitmanager, GitManagerRights::Unknown);
            assert_eq!(probe.rights_glab, GlabRights::Unknown);
            assert_eq!(states(&probe), vec![AccessState::Unknown; 3]);
        }
    }

    #[test]
    fn probe_project_access_states_with_rest_fallback() {
        let runner = gated();
        let fake = runner.inner();
        let mut access: Value = serde_json::from_str(ACCESS).expect("Fixture");
        let nodes = access["data"]["projects"]["nodes"]
            .as_array_mut()
            .expect("nodes");
        nodes.push(json!({
            "fullPath": "web/mr-aus",
            "webUrl": "https://gitlab.example.com/web/mr-aus",
            "archived": false,
            "mergeRequestsEnabled": false,
            "maxAccessLevel": { "integerValue": 30, "stringValue": "DEVELOPER" },
            "userPermissions": { "readMergeRequest": true }
        }));
        push_probe(
            fake,
            FakeReply::http(200, &pat_self(&["read_api"])),
            gql(&access.to_string()),
        );
        fake.push(FakeReply::http(200, PROJECT_REST));
        fake.push(FakeReply::http(404, NOT_FOUND));
        let paths = strings(&[
            "kern/backend-api",
            "oss/public-lib",
            "alt/legacy-tool",
            "web/mr-aus",
            "alt/umbenannt",
            "geheim/verschwunden",
        ]);
        let probe = probe_as(&runner, HOST, &paths, ProbeEnv::Inherit(None));
        let calls = fake.take_calls();
        assert_eq!(
            ops(&calls)[4..],
            ["projects/alt%2Fumbenannt", "projects/geheim%2Fverschwunden"]
        );
        assert_eq!(
            states(&probe),
            vec![
                AccessState::Ok { level: 30 },
                AccessState::Public,
                AccessState::Archived,
                AccessState::MrsDisabled,
                // glab folgt dem 301 von GitLab
                AccessState::Renamed {
                    to: "neu/umbenannt".to_string()
                },
                AccessState::NoAccess,
            ]
        );
    }

    #[test]
    fn probe_rest_redirect_not_followed_is_renamed() {
        let runner = gated();
        let fake = runner.inner();
        let empty = json!({ "data": {
            "currentUser": { "username": ME, "name": "Tim Altmann", "bot": false },
            "projects": { "nodes": [] }
        } });
        push_probe(
            fake,
            FakeReply::http(200, &pat_self(&["read_api"])),
            gql(&empty.to_string()),
        );
        fake.push(FakeReply::http(
            301,
            r#"{"message":"301 Moved Permanently"}"#,
        ));
        let probe = probe_as(
            &runner,
            HOST,
            &strings(&["alt/umbenannt"]),
            ProbeEnv::Inherit(None),
        );
        assert_eq!(
            states(&probe),
            vec![AccessState::Renamed { to: String::new() }]
        );
        assert_eq!(probe.error, None);
    }

    #[test]
    fn probe_auth_status_transport_error_stops() {
        // `auth status` selbst läuft in den Timeout
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::error(GlabError::Timeout(Duration::from_secs(
            45,
        ))));
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(ops(&fake.take_calls()), vec!["version", "auth status"]);
        assert_eq!(probe.error, Some(GlabErrorKind::Timeout));
        assert_eq!(states(&probe), vec![AccessState::Unknown; 3]);

        // JSON-Status meldet den Netzfehler seines `GET /user`
        fake.push(version_reply("1.123.0"));
        let mut status: Value = serde_json::from_str(AUTH_STATUS).expect("Fixture");
        status["hosts"][0]["state"] = json!("error");
        status["hosts"][0]["user"] = json!("");
        status["hosts"][0]["error"] = json!(
            "Get \"https://gitlab.example.com/api/v4/user\": dial tcp: lookup gitlab.example.com: no such host"
        );
        fake.push(FakeReply::stdout(1, &status.to_string(), ""));
        let probe = probe_as(&runner, HOST, &[], ProbeEnv::Inherit(None));
        assert_eq!(ops(&fake.take_calls()), vec!["version", "auth status"]);
        assert_eq!(probe.error, Some(GlabErrorKind::Network));
        assert_eq!(probe.token_source, TokenSource::Unknown);

        // ältere glab-Version: TLS-Fehler im Text auf stderr
        fake.push(version_reply("1.115.0"));
        fake.push(FakeReply::stdout(
            1,
            "",
            "gitlab.example.com\n  x gitlab.example.com: API call failed: tls: failed to verify certificate: x509: certificate signed by unknown authority\n",
        ));
        let probe = probe_as(&runner, HOST, &[], ProbeEnv::Inherit(None));
        assert_eq!(ops(&fake.take_calls()), vec!["version", "auth status"]);
        assert_eq!(probe.error, Some(GlabErrorKind::Tls));
    }

    #[test]
    fn probe_rest_fallback_timeout_is_reported() {
        let runner = gated();
        let fake = runner.inner();
        let empty = json!({ "data": {
            "currentUser": { "username": ME, "name": "Tim Altmann", "bot": false },
            "projects": { "nodes": [] }
        } });
        push_probe(
            fake,
            FakeReply::http(200, &pat_self(&["read_api"])),
            gql(&empty.to_string()),
        );
        fake.push(FakeReply::error(GlabError::Timeout(Duration::from_secs(
            45,
        ))));
        let probe = probe_as(
            &runner,
            HOST,
            &strings(&["gruppe/a", "gruppe/b"]),
            ProbeEnv::Inherit(None),
        );
        let rest = ops(&fake.take_calls())
            .iter()
            .filter(|o| o.starts_with("projects/"))
            .count();
        assert_eq!(rest, 1);
        assert_eq!(probe.error, Some(GlabErrorKind::Timeout));
        assert_eq!(probe.rights_gitmanager, GitManagerRights::Ok);
        assert_eq!(states(&probe), vec![AccessState::Unknown; 2]);
    }

    #[test]
    fn probe_rest_fallback_at_most_ten() {
        let runner = gated();
        let fake = runner.inner();
        let empty = json!({ "data": {
            "currentUser": { "username": ME, "name": "Tim Altmann", "bot": false },
            "metadata": { "version": "18.5.1-ee", "enterprise": true },
            "projects": { "nodes": [] }
        } });
        push_probe(
            fake,
            FakeReply::http(200, &pat_self(&["read_api"])),
            gql(&empty.to_string()),
        );
        for _ in 0..10 {
            fake.push(FakeReply::http(404, NOT_FOUND));
        }
        let paths: Vec<String> = (0..12).map(|i| format!("gruppe/p{i}")).collect();
        let probe = probe_as(&runner, HOST, &paths, ProbeEnv::Inherit(None));
        let rest = ops(&fake.take_calls())
            .iter()
            .filter(|o| o.starts_with("projects/"))
            .count();
        assert_eq!(rest, 10);
        assert_eq!(states(&probe), vec![AccessState::NoAccess; 12]);
    }

    #[test]
    fn probe_not_signed_in_makes_no_network_call() {
        // glab 1.123: JSON mit `token_source: none`
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        let mut status: Value = serde_json::from_str(AUTH_STATUS).expect("Fixture");
        status["hosts"][0]["state"] = json!("error");
        status["hosts"][0]["user"] = json!("");
        status["hosts"][0]["token_source"] = json!("none");
        status["hosts"][0]["error"] = json!(
            "gitlab.example.com has not been authenticated with glab. Run glab auth login --hostname gitlab.example.com"
        );
        fake.push(FakeReply::stdout(1, &status.to_string(), ""));
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(ops(&fake.take_calls()), vec!["version", "auth status"]);
        assert_eq!(probe.error, Some(GlabErrorKind::NotAuthenticated));
        assert_eq!(states(&probe), vec![AccessState::Unknown; 3]);

        // ältere glab-Version: Text auf stderr, kein `--output json`
        fake.push(version_reply("1.115.0"));
        fake.push(FakeReply::stdout(
            1,
            "",
            "gitlab.example.com has not been authenticated with glab; run glab auth login --hostname gitlab.example.com\n",
        ));
        let probe = probe_as(&runner, HOST, &[], ProbeEnv::Inherit(None));
        let calls = fake.take_calls();
        assert_eq!(ops(&calls), vec!["version", "auth status"]);
        assert!(!calls[1].request.args.iter().any(|a| a == "--output"));
        assert_eq!(probe.error, Some(GlabErrorKind::NotAuthenticated));
    }

    #[test]
    fn auth_status_entry_only_for_requested_host() {
        // Mehrere Hosts, ein anderer zuerst: es zählt der Eintrag des Hosts,
        // der Schlüssel ohne Groß-/Kleinschreibung und Leerraum verglichen
        let json = auth_status_json(&[
            (HOST_B, "success", "jemand-anders", "keyring"),
            (" GitLab.Example.com ", "success", ME, "config_file"),
        ]);
        let status = auth_status_entry(&json, HOST).expect("JSON-Status");
        assert_eq!(status.signed_in, Some(true));
        assert_eq!(status.user.as_deref(), Some(ME));
        assert_eq!(status.source, Some(TokenSource::ConfigFile));

        // Host fehlt in der Liste: unbekannt statt des ersten Eintrags
        for json in [
            auth_status_json(&[(HOST_B, "success", "jemand-anders", "keyring")]),
            auth_status_json(&[(HOST_B, "error", "", "none")]),
            auth_status_json(&[]),
        ] {
            let status = auth_status_entry(&json, HOST).expect("JSON-Status");
            assert_eq!(status.signed_in, None, "{json}");
            assert_eq!(status.user, None, "{json}");
            assert_eq!(status.source, None, "{json}");
            assert_eq!(status.error, None, "{json}");
        }
        // ohne `hosts`-Liste kein JSON-Status: dann zählt der Text
        assert!(auth_status_entry(&json!({ "error": "kaputt" }), HOST).is_none());
    }

    #[test]
    fn probe_ignores_auth_status_of_other_host() {
        // glab listet nur einen anderen Host (z. B. ohne `--hostname`-Filter):
        // dessen Anmeldung, Benutzer und Speicherort gelten nicht für HOST,
        // die Prüfung fragt GitLab
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(auth_status_hosts(&[(
            HOST_B,
            "success",
            "jemand-anders",
            "keyring",
        )]));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        let probe = probe_as(&runner, HOST, &[], ProbeEnv::Inherit(None));
        assert_eq!(
            ops(&fake.take_calls()),
            vec![
                "version",
                "auth status",
                "personal_access_tokens/self",
                "GmAccess"
            ]
        );
        assert_eq!(probe.error, None);
        assert_eq!(probe.logged_in_as.as_deref(), Some(ME));
        assert_eq!(probe.token_source, TokenSource::Unknown);

        // „nicht angemeldet“ eines anderen Hosts beendet die Prüfung nicht
        fake.push(version_reply("1.123.0"));
        fake.push(auth_status_hosts(&[(HOST_B, "error", "", "none")]));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        let probe = probe_as(&runner, HOST, &[], ProbeEnv::Inherit(None));
        assert_eq!(probe.error, None);
        assert_eq!(probe.logged_in_as.as_deref(), Some(ME));
    }

    #[test]
    fn probe_old_glab_reads_text_status_and_keyring_flag() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.115.0"));
        fake.push(FakeReply::stdout(
            0,
            "",
            "gitlab.example.com\n  ✓ Logged in to gitlab.example.com as taltmann (C:\\Users\\tim\\AppData\\Local\\glab-cli\\config.yml)\n  ✓ API calls for gitlab.example.com are made over https protocol\n",
        ));
        fake.push_when(
            config_get(HOST, "use_keyring"),
            FakeReply::stdout(0, "true\n", ""),
        );
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        let probe = probe_as(&runner, HOST, &[], ProbeEnv::Inherit(None));
        assert_eq!(
            ops(&fake.take_calls()),
            vec![
                "version",
                "auth status",
                "config get use_keyring",
                "personal_access_tokens/self",
                "GmAccess"
            ]
        );
        assert_eq!(probe.error, None);
        assert_eq!(probe.logged_in_as.as_deref(), Some(ME));
        assert_eq!(probe.token_source, TokenSource::Keyring);
        assert_eq!(probe.rights_glab, GlabRights::Full);
    }

    #[test]
    fn probe_stops_when_glab_missing_or_too_old() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(FakeReply::error(GlabError::NotFound));
        let probe = probe_as(&runner, HOST, &paths3(), ProbeEnv::Inherit(None));
        assert_eq!(fake.take_calls().len(), 1);
        assert_eq!(probe.error, Some(GlabErrorKind::NotInstalled));
        assert_eq!(probe.glab_version, None);

        fake.push(version_reply("1.98.0"));
        let probe = probe_host(&runner, HOST, &[]);
        assert_eq!(fake.take_calls().len(), 1);
        assert_eq!(
            probe.error,
            Some(GlabErrorKind::TooOld(Version::new(1, 98, 0)))
        );
    }

    #[test]
    fn probe_env_token_inherited_or_stripped() {
        let runner = GatedRunner::new(
            FakeGlab::new().with_parent_env(&[("GITLAB_TOKEN", "glpat-AusDerUmgebung")]),
        );
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(auth_status("success", ME, "environment"));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        let probe = probe_as(&runner, HOST, &[], ProbeEnv::Inherit(Some("GITLAB_TOKEN")));
        assert_eq!(probe.token_kind, TokenKind::Env("GITLAB_TOKEN"));
        assert_eq!(probe.token_source, TokenSource::Env("GITLAB_TOKEN"));
        let calls = fake.take_calls();
        assert!(calls
            .iter()
            .filter(|c| c.request.host_key.is_some())
            .all(|c| c.env.contains_key("GITLAB_TOKEN")));

        // Gespeicherte Anmeldung: Env-Tokens fehlen in jedem Kind
        push_probe(
            fake,
            FakeReply::http(200, &pat_self(&["api"])),
            gql(&access_user(ME)),
        );
        let probe = probe_as(&runner, HOST, &[], ProbeEnv::Stored);
        assert_eq!(probe.token_kind, TokenKind::Pat);
        assert_eq!(probe.token_source, TokenSource::Keyring);
        let calls = fake.take_calls();
        assert!(calls
            .iter()
            .filter(|c| c.request.host_key.is_some())
            .all(|c| !c.env.contains_key("GITLAB_TOKEN")));
    }

    // --- Step 3: Token prüfen und speichern -----------------------------

    /// Prüfung eines neuen Tokens: Token nie in argv, nur als
    /// `EnvValue::Secret` in `GITLAB_TOKEN`.
    fn assert_secret_only_in_env(call: &FakeCall) {
        assert!(
            !call.request.args.iter().any(|a| a.contains(TOKEN)),
            "Token in argv"
        );
        assert!(call
            .request
            .env_set
            .iter()
            .any(|(k, v)| k == "GITLAB_TOKEN" && matches!(v, EnvValue::Secret(_))));
        assert!(!call.request.env_remove.iter().any(|k| k == "GITLAB_TOKEN"));
        for key in ["GITLAB_ACCESS_TOKEN", "OAUTH_TOKEN", "GLAB_IS_OAUTH2"] {
            assert!(call.request.env_remove.iter().any(|k| k == key), "{key}");
        }
        assert_eq!(
            call.env.get("GITLAB_TOKEN").map(String::as_str),
            Some("***")
        );
        if let Some(StdinBody::Json(body)) = &call.request.stdin {
            assert!(!body.contains(TOKEN));
        }
    }

    #[test]
    fn verify_token_only_in_child_env() {
        let runner = GatedRunner::new(
            FakeGlab::new().with_parent_env(&[("GITLAB_TOKEN", "glpat-AusDerUmgebung")]),
        );
        let fake = runner.inner();
        fake.push(FakeReply::http(200, &pat_self(&["read_api"])));
        fake.push(gql(ACCESS));
        // bisherige Anmeldung (ohne Env-Token): ebenfalls read_api
        fake.push(FakeReply::http(200, &pat_self(&["read_api"])));
        let check = verify_token(
            &runner,
            HOST,
            secret(TOKEN),
            Protocol::Https,
            &strings(&["kern/backend-api"]),
        )
        .expect("gültig");
        assert_eq!(
            check,
            TokenCheck {
                user: Some(ME.to_string()),
                scopes: strings(&["read_api"]),
                expires_at: parse_date("2027-03-01"),
                rights_gitmanager: GitManagerRights::Ok,
                rights_glab: GlabRights::ReadOnly,
                todos_hidden: false,
                access_preview: vec![ProjectAccess {
                    full_path: "kern/backend-api".to_string(),
                    state: AccessState::Ok { level: 30 },
                }],
                downgrade_warning: false,
            }
        );
        let calls = fake.take_calls();
        assert_eq!(
            ops(&calls),
            vec![
                "personal_access_tokens/self",
                "GmAccess",
                "personal_access_tokens/self"
            ]
        );
        assert_secret_only_in_env(&calls[0]);
        assert_secret_only_in_env(&calls[1]);
        // die bisherige Anmeldung ohne jedes Env-Token
        assert!(!calls[2].env.contains_key("GITLAB_TOKEN"));
        assert!(calls[2].request.env_set.is_empty());
        assert!(!format!("{calls:?}").contains(TOKEN));
        // Gewähltes https gilt ausdrücklich: ein `api_protocol: http` der
        // glab-Config schickte das neue Token sonst unverschlüsselt (E17)
        for call in &calls[..2] {
            assert_eq!(
                call.env.get("GLAB_API_PROTOCOL").map(String::as_str),
                Some("https")
            );
        }
    }

    #[test]
    fn invalid_token_never_reaches_auth_login() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(FakeReply::http(401, BAD_TOKEN));
        let got = verify_token(&runner, HOST, secret(TOKEN), Protocol::Https, &[]);
        assert_eq!(got, Err(GlabErrorKind::InvalidToken));
        assert_eq!(fake.take_calls().len(), 1);

        // Speichern prüft selbst zuerst: die bestehende Anmeldung bleibt
        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::http(401, EXPIRED));
        let got = save_token(&runner, HOST, secret(TOKEN), Protocol::Https);
        assert_eq!(got, Err(GlabErrorKind::Expired));
        let calls = fake.take_calls();
        assert_eq!(ops(&calls), vec!["version", "personal_access_tokens/self"]);
        assert!(calls.iter().all(|c| c.request.kind == CallKind::Read));
    }

    #[test]
    fn unconfirmed_token_never_reaches_auth_login() {
        // `self` und ACCESS antworten mit 502 (Wartung, falscher Host): das
        // Token ist nicht bestätigt, `auth login` löschte sonst die Anmeldung
        let runner = gated();
        let fake = runner.inner();
        fake.push(FakeReply::http(502, "<html>Bad Gateway</html>"));
        fake.push(FakeReply::http(502, "<html>Bad Gateway</html>"));
        let got = verify_token(&runner, HOST, secret(TOKEN), Protocol::Https, &[]);
        assert_eq!(got, Err(GlabErrorKind::Other));
        assert_eq!(
            ops(&fake.take_calls()),
            vec!["personal_access_tokens/self", "GmAccess"]
        );

        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::http(404, NOT_FOUND));
        fake.push(FakeReply::http(404, NOT_FOUND));
        let got = save_token(&runner, HOST, secret(TOKEN), Protocol::Https);
        assert_eq!(got, Err(GlabErrorKind::NotFound));
        let calls = fake.take_calls();
        assert_eq!(calls.len(), 3);
        assert!(calls.iter().all(|c| c.request.kind == CallKind::Read));
        assert_eq!(fake.pending(), 0);
    }

    #[test]
    fn save_token_stops_when_glab_missing_or_too_old() {
        // glab < 1.111 speicherte im Klartext: kein Login, keine Prüfung
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.98.0"));
        let got = save_token(&runner, HOST, secret(TOKEN), Protocol::Https);
        assert_eq!(got, Err(GlabErrorKind::TooOld(Version::new(1, 98, 0))));
        assert_eq!(ops(&fake.take_calls()), vec!["version"]);

        fake.push(FakeReply::error(GlabError::NotFound));
        let got = save_token(&runner, HOST, secret(TOKEN), Protocol::Https);
        assert_eq!(got, Err(GlabErrorKind::NotInstalled));
        assert_eq!(fake.take_calls().len(), 1);
    }

    #[test]
    fn verify_token_timeout_stops_calls_to_host() {
        // REST /user läuft in den Timeout: keine Abfrage der bisherigen Anmeldung
        let runner = gated();
        let fake = runner.inner();
        fake.push(FakeReply::http(200, &pat_self(&["read_user"])));
        fake.push(FakeReply::http(401, INVALID_TOKEN_GQL));
        fake.push(FakeReply::error(GlabError::Timeout(Duration::from_secs(
            45,
        ))));
        let got = verify_token(&runner, HOST, secret(TOKEN), Protocol::Https, &[]);
        assert_eq!(got, Err(GlabErrorKind::Timeout));
        assert_eq!(
            ops(&fake.take_calls()),
            vec!["personal_access_tokens/self", "GmAccess", "user"]
        );

        // REST-Nachprüfung eines Projekts läuft in den Timeout: ebenso
        let empty = json!({ "data": {
            "currentUser": { "username": ME, "name": "Tim Altmann", "bot": false },
            "projects": { "nodes": [] }
        } });
        fake.push(FakeReply::http(200, &pat_self(&["read_api"])));
        fake.push(gql(&empty.to_string()));
        fake.push(FakeReply::error(GlabError::Timeout(Duration::from_secs(
            45,
        ))));
        let got = verify_token(
            &runner,
            HOST,
            secret(TOKEN),
            Protocol::Https,
            &strings(&["gruppe/a", "gruppe/b"]),
        );
        assert_eq!(got, Err(GlabErrorKind::Timeout));
        assert_eq!(
            ops(&fake.take_calls()),
            vec![
                "personal_access_tokens/self",
                "GmAccess",
                "projects/gruppe%2Fa"
            ]
        );
    }

    #[test]
    fn verify_token_downgrade_warning() {
        // neues Token read_api, bisherige Anmeldung api → Warnung
        let runner = gated();
        let fake = runner.inner();
        fake.push(FakeReply::http(200, &pat_self(&["read_api"])));
        fake.push(gql(&access_user(ME)));
        fake.push(FakeReply::http(
            200,
            &pat_self(&["api", "write_repository"]),
        ));
        let check =
            verify_token(&runner, HOST, secret(TOKEN), Protocol::Https, &[]).expect("gültig");
        assert!(check.downgrade_warning);
        assert_eq!(check.rights_glab, GlabRights::ReadOnly);

        // bisherige Anmeldung per OAuth (immer api) → Warnung
        fake.push(FakeReply::http(200, &pat_self(&["read_api"])));
        fake.push(gql(&access_user(ME)));
        fake.push(FakeReply::http(400, NOT_A_PAT));
        let check =
            verify_token(&runner, HOST, secret(TOKEN), Protocol::Https, &[]).expect("gültig");
        assert!(check.downgrade_warning);

        // bisher nicht angemeldet → keine Warnung
        fake.push(FakeReply::http(200, &pat_self(&["read_api"])));
        fake.push(gql(&access_user(ME)));
        fake.push(FakeReply::http(401, r#"{"message":"401 Unauthorized"}"#));
        let check =
            verify_token(&runner, HOST, secret(TOKEN), Protocol::Https, &[]).expect("gültig");
        assert!(!check.downgrade_warning);

        // neues Token mit api: keine Abfrage der bisherigen Anmeldung
        fake.take_calls();
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        let check =
            verify_token(&runner, HOST, secret(TOKEN), Protocol::Https, &[]).expect("gültig");
        assert!(!check.downgrade_warning);
        assert_eq!(check.rights_glab, GlabRights::Full);
        assert_eq!(fake.take_calls().len(), 2);
    }

    /// Schlüssel mit Port: `save_token` ersetzt die Anmeldung unter dem
    /// Hostnamen (`login_key`), also fragt die Downgrade-Prüfung genau die.
    #[test]
    fn verify_token_downgrade_checks_login_key_of_port_host() {
        const PORT_HOST: &str = "git.firma.de:8443";
        let runner = gated();
        let fake = runner.inner();
        fake.push(FakeReply::http(200, &pat_self(&["read_api"])));
        fake.push(gql(&access_user(ME)));
        // bisherige Anmeldung unter `git.firma.de` (`--api-host` mit Port) hat api
        fake.push_when(
            |r| r.host_key.as_deref() == Some("git.firma.de"),
            FakeReply::http(200, &pat_self(&["api"])),
        );
        let check =
            verify_token(&runner, PORT_HOST, secret(TOKEN), Protocol::Https, &[]).expect("gültig");
        assert!(check.downgrade_warning);
        let calls = fake.take_calls();
        assert_eq!(calls.len(), 3);
        // Prüfen des neuen Tokens über `GITLAB_HOST=host:port`, die
        // bisherige Anmeldung über `--hostname host` ohne Env-Token
        assert_eq!(calls[0].request.host_key.as_deref(), Some(PORT_HOST));
        let stored = &calls[2];
        assert!(stored
            .request
            .args
            .windows(2)
            .any(|w| w == ["--hostname", "git.firma.de"]));
        assert!(!stored.env.contains_key("GITLAB_TOKEN"));
        assert!(!stored.env.contains_key("GITLAB_HOST"));
    }

    #[test]
    fn verify_token_read_user_only_needs_rest_user() {
        // GraphQL lehnt read_user ab; der Benutzer kommt aus REST /user
        let runner = gated();
        let fake = runner.inner();
        fake.push(FakeReply::http(200, &pat_self(&["read_user"])));
        fake.push(FakeReply::http(401, INVALID_TOKEN_GQL));
        fake.push(FakeReply::http(200, r#"{"id":42,"username":"taltmann"}"#));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        let check = verify_token(
            &runner,
            HOST,
            secret(TOKEN),
            Protocol::Https,
            &strings(&["kern/backend-api"]),
        )
        .expect("Token gültig, Rechte fehlen");
        assert_eq!(check.user.as_deref(), Some(ME));
        assert_eq!(check.rights_gitmanager, GitManagerRights::Missing);
        assert_eq!(check.rights_glab, GlabRights::Insufficient);
        assert_eq!(check.access_preview[0].state, AccessState::Unknown);
        assert!(check.downgrade_warning);
        let calls = fake.take_calls();
        assert_eq!(
            ops(&calls),
            vec![
                "personal_access_tokens/self",
                "GmAccess",
                "user",
                "personal_access_tokens/self"
            ]
        );
        assert_secret_only_in_env(&calls[2]);
    }

    /// Speichern mit glab 1.111: `auth login` ohne Ausgabe, danach Prüfung
    /// über die gespeicherte Anmeldung (Text-Status, `use_keyring`).
    #[test]
    fn save_token_glab_1_111_silent_login_and_http() {
        let runner = GatedRunner::new(FakeGlab::new().with_parent_env(&[
            ("GITLAB_TOKEN", "glpat-AusDerUmgebung"),
            ("CI", "true"),
            ("GITLAB_CI", "true"),
        ]));
        let fake = runner.inner();
        fake.push(version_reply("1.111.0"));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        fake.push_when(|r| r.kind == CallKind::Write, FakeReply::stdout(0, "", ""));
        fake.push(FakeReply::stdout(
            0,
            "",
            "  ✓ Logged in to gitlab.example.com as taltmann (C:\\Users\\tim\\AppData\\Local\\glab-cli\\config.yml)\n",
        ));
        fake.push_when(
            config_get(HOST, "use_keyring"),
            FakeReply::stdout(0, "true\n", ""),
        );
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        let got = save_token(&runner, HOST, secret(TOKEN), Protocol::Http);
        assert_eq!(got, Ok(SavedWhere::Keyring));
        let calls = fake.take_calls();
        // `glab version` nur einmal, vor dem Login
        assert_eq!(
            ops(&calls),
            vec![
                "version",
                "personal_access_tokens/self",
                "GmAccess",
                "auth login",
                "auth status",
                "config get use_keyring",
                "personal_access_tokens/self",
                "GmAccess"
            ]
        );
        // Prüfen über HTTP (E17)
        assert_secret_only_in_env(&calls[1]);
        assert_eq!(
            calls[1].env.get("GLAB_API_PROTOCOL").map(String::as_str),
            Some("http")
        );
        // Anmelden: Token nur auf stdin, ohne Env-Tokens und CI-Variablen
        let login = &calls[3];
        assert_eq!(login.request.kind, CallKind::Write);
        assert_eq!(
            login.request.args,
            strings(&[
                "auth",
                "login",
                "--hostname",
                HOST,
                "--stdin",
                "--api-protocol",
                "http"
            ])
        );
        assert!(matches!(login.request.stdin, Some(StdinBody::Secret(_))));
        for key in ["GITLAB_TOKEN", "CI", "GITLAB_CI", "GLAB_API_PROTOCOL"] {
            assert!(!login.env.contains_key(key), "{key}");
        }
        // danach die gespeicherte Anmeldung ohne Env-Token
        assert!(calls[4..]
            .iter()
            .all(|c| !c.env.contains_key("GITLAB_TOKEN")));
        assert!(!format!("{calls:?}").contains(TOKEN));
    }

    #[test]
    fn save_token_username_warning_needs_matching_probe() {
        let push_save = |fake: &FakeGlab, probe_self: FakeReply, probe_user: &str| {
            fake.push(version_reply("1.123.0"));
            fake.push(FakeReply::http(200, &pat_self(&["api"])));
            fake.push(gql(&access_user(ME)));
            fake.push_when(
                |r| r.kind == CallKind::Write,
                FakeReply::stdout(
                    0,
                    "",
                    "WARNING: Could not look up the username for this token: 401 Unauthorized\n✓ Stored your credentials in the configuration file.\n",
                ),
            );
            fake.push(auth_status("success", probe_user, "config_file"));
            fake.push(probe_self);
            fake.push(gql(&access_user(probe_user)));
        };
        // Prüfung passt → Erfolg, Speicherort aus der glab-Ausgabe
        let runner = gated();
        push_save(
            runner.inner(),
            FakeReply::http(200, &pat_self(&["api"])),
            ME,
        );
        assert_eq!(
            save_token(&runner, HOST, secret(TOKEN), Protocol::Https),
            Ok(SavedWhere::ConfigFile)
        );

        // anderer Benutzer nach dem Speichern → Fehler
        let runner = gated();
        push_save(
            runner.inner(),
            FakeReply::http(200, &pat_self(&["api"])),
            "jemand-anders",
        );
        assert_eq!(
            save_token(&runner, HOST, secret(TOKEN), Protocol::Https),
            Err(GlabErrorKind::Other)
        );

        // gespeichertes Token wirkt nicht → Fehlerart der Prüfung
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        fake.push_when(|r| r.kind == CallKind::Write, FakeReply::stdout(0, "", ""));
        fake.push(auth_status("error", "", "keyring"));
        fake.push(FakeReply::http(401, BAD_TOKEN));
        assert_eq!(
            save_token(&runner, HOST, secret(TOKEN), Protocol::Https),
            Err(GlabErrorKind::InvalidToken)
        );
    }

    #[test]
    fn save_token_keyring_warning_but_config_file() {
        // Keyring nicht verfügbar: glab warnt und speichert in der Datei
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(
                0,
                "",
                "WARNING: the operating system keyring is not available, falling back\n✓ Logged in as taltmann\n✓ Stored your credentials in the configuration file.\n",
            ),
        );
        fake.push(auth_status("success", ME, "keyring"));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        assert_eq!(
            save_token(&runner, HOST, secret(TOKEN), Protocol::Https),
            Ok(SavedWhere::ConfigFile)
        );
    }

    #[test]
    fn save_token_host_key_with_port_uses_api_host() {
        // `auth login --hostname` lehnt `:` ab: Anmeldung unter dem Hostnamen
        const PORT_HOST: &str = "git.firma.de:8443";
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(0, "", "✓ Logged in as taltmann\n"),
        );
        fake.push(auth_status_hosts(&[(
            "git.firma.de",
            "success",
            ME,
            "keyring",
        )]));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        assert_eq!(
            save_token(&runner, PORT_HOST, secret(TOKEN), Protocol::Https),
            Ok(SavedWhere::Keyring)
        );
        let calls = fake.take_calls();
        // Prüfen über `GITLAB_HOST=host:port`
        assert_eq!(calls[1].request.host_key.as_deref(), Some(PORT_HOST));
        assert_eq!(
            calls[1].env.get("GITLAB_HOST").map(String::as_str),
            Some(PORT_HOST)
        );
        let login = &calls[3];
        assert_eq!(login.request.host_key.as_deref(), Some("git.firma.de"));
        assert_eq!(
            login.request.args,
            strings(&[
                "auth",
                "login",
                "--hostname",
                "git.firma.de",
                "--stdin",
                "--api-host",
                PORT_HOST,
                "--api-protocol",
                "https"
            ])
        );
        // Gewähltes https auch beim Prüfen vor dem Login
        assert_eq!(
            calls[1].env.get("GLAB_API_PROTOCOL").map(String::as_str),
            Some("https")
        );
        // die Prüfung danach unter dem gespeicherten Schlüssel
        assert!(calls[4..]
            .iter()
            .all(|c| c.request.host_key.as_deref() == Some("git.firma.de")));
        assert_eq!(login_key(PORT_HOST), "git.firma.de");
        assert_eq!(login_key(HOST), HOST);
    }

    #[test]
    fn save_token_compares_user_of_own_host() {
        let push_save = |fake: &FakeGlab, status: FakeReply| {
            fake.push(version_reply("1.123.0"));
            fake.push(FakeReply::http(200, &pat_self(&["api"])));
            fake.push(gql(&access_user(ME)));
            fake.push_when(
                |r| r.kind == CallKind::Write,
                FakeReply::stdout(0, "", "✓ Logged in as taltmann\n"),
            );
            fake.push(status);
            fake.push(FakeReply::http(200, &pat_self(&["api"])));
            fake.push(gql(&access_user(ME)));
        };
        // anderer Host mit anderem Benutzer zuerst: Benutzer und Speicherort
        // aus dem Eintrag von HOST
        let runner = gated();
        push_save(
            runner.inner(),
            auth_status_hosts(&[
                (HOST_B, "success", "jemand-anders", "keyring"),
                (HOST, "success", ME, "config_file"),
            ]),
        );
        assert_eq!(
            save_token(&runner, HOST, secret(TOKEN), Protocol::Https),
            Ok(SavedWhere::ConfigFile)
        );

        // nur ein anderer Host: Benutzer aus GitLab, Speicherort unbekannt
        let runner = gated();
        push_save(
            runner.inner(),
            auth_status_hosts(&[(HOST_B, "success", "jemand-anders", "keyring")]),
        );
        assert_eq!(
            save_token(&runner, HOST, secret(TOKEN), Protocol::Https),
            Ok(SavedWhere::Unknown)
        );
    }

    #[test]
    fn save_token_login_failure() {
        let runner = gated();
        let fake = runner.inner();
        fake.push(version_reply("1.123.0"));
        fake.push(FakeReply::http(200, &pat_self(&["api"])));
        fake.push(gql(&access_user(ME)));
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(
                1,
                "",
                "Could not sign in!\nerror parsing '--hostname': invalid hostname\n",
            ),
        );
        assert_eq!(
            save_token(&runner, HOST, secret(TOKEN), Protocol::Https),
            Err(GlabErrorKind::HostKey)
        );
        assert_eq!(fake.pending(), 0);
    }

    // --- Hilfen -----------------------------------------------------------

    #[test]
    fn env_token_override_precedence() {
        let env = |vars: &'static [(&'static str, &'static str)]| {
            move |key: &str| {
                vars.iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| std::ffi::OsString::from(v))
            }
        };
        assert_eq!(env_token_in(env(&[])), None);
        assert_eq!(env_token_in(env(&[("GITLAB_TOKEN", "")])), None);
        assert_eq!(
            env_token_in(env(&[("OAUTH_TOKEN", "x"), ("GITLAB_ACCESS_TOKEN", "y")])),
            Some("GITLAB_ACCESS_TOKEN")
        );
        assert_eq!(
            env_token_in(env(&[("OAUTH_TOKEN", "x"), ("GITLAB_TOKEN", "y")])),
            Some("GITLAB_TOKEN")
        );
        assert_eq!(
            env_token_in(env(&[("OAUTH_TOKEN", "x")])),
            Some("OAUTH_TOKEN")
        );
        assert_eq!(env_token_in(env(&[("GLAB_IS_OAUTH2", "true")])), None);
    }

    #[test]
    fn token_create_url_prefilled() {
        assert_eq!(
            token_create_url(HOST, Protocol::Https, true),
            "https://gitlab.example.com/-/user_settings/personal_access_tokens?name=GitManager&scopes=read_api"
        );
        assert_eq!(
            token_create_url("gitlab.local:8080", Protocol::Http, false),
            "http://gitlab.local:8080/-/user_settings/personal_access_tokens?name=GitManager&scopes=api,write_repository"
        );
    }

    #[test]
    fn error_text_for_fetch_errors() {
        assert_eq!(
            error_text(&GlabErrorKind::Timeout, HOST, Language::De).as_deref(),
            Some("Zeitüberschreitung bei gitlab.example.com nach 45 s.")
        );
        assert_eq!(
            error_text(&GlabErrorKind::Network, HOST, Language::En).as_deref(),
            Some("gitlab.example.com is not reachable.")
        );
        assert_eq!(
            error_text(&GlabErrorKind::Tls, HOST, Language::De).as_deref(),
            Some("Zertifikat von gitlab.example.com wird nicht vertraut.")
        );
        assert_eq!(
            error_text(&GlabErrorKind::InsufficientScope, HOST, Language::De).as_deref(),
            Some("Das Token braucht read_api.")
        );
        assert!(
            error_text(&GlabErrorKind::MissingContentType, HOST, Language::En)
                .is_some_and(|t| t.contains("content type"))
        );
        // eigene Texte der Ansicht bzw. nur Details
        for kind in [
            GlabErrorKind::NotInstalled,
            GlabErrorKind::NotAuthenticated,
            GlabErrorKind::Other,
        ] {
            assert_eq!(error_text(&kind, HOST, Language::De), None, "{kind:?}");
        }
    }
}
