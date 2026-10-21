//! glab als externer Prozess: Anfrage- und Antworttypen, `SystemGlab`
//! (Kind-Umgebung, Timeout, Ausgabegrenze, ohne Konsolenfenster), das
//! prozessweite Gate `GatedRunner`, die Fehlerklassifizierung und – nur für
//! Tests – `FakeGlab`. Tokens nur als `SecretString` in Kind-stdin bzw.
//! Kind-Env (E3), nie in argv.

use super::secret::SecretString;
use crate::process::{self, Keep, Limits, RunError, CREATE_NO_WINDOW};
use semver::Version;
use std::collections::HashMap;
use std::ffi::OsString;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

/// Mindestversion (E18): Keyring als Standard ab 1.111.
pub const MIN_GLAB_VERSION: Version = Version::new(1, 111, 0);
/// Timeout je Aufruf; GitLab bricht GraphQL nach 30 s selbst ab.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(45);
/// Obergrenze für stdout; darüber wird der Aufruf abgebrochen.
pub const MAX_OUTPUT: usize = 16 * 1024 * 1024;
/// Behaltenes Ende von stderr (für `classify`, Anzeige nur gekürzt).
const STDERR_KEEP: usize = 8 * 1024;

/// Env-Tokens: glab schickt sie an jeden Host (E16). Entfernen bei lokaler
/// Erkennung, Speichern und Prüfen mit der gespeicherten Anmeldung.
pub const TOKEN_ENV: [&str; 4] = [
    "GITLAB_TOKEN",
    "GITLAB_ACCESS_TOKEN",
    "OAUTH_TOKEN",
    "GLAB_IS_OAUTH2",
];
/// Bei `auth login` zusätzlich: CI-Variablen erzwingen Klartext-Speicherung.
pub const CI_ENV: [&str; 2] = ["CI", "GITLAB_CI"];
/// Env-Variablen mit einem Token als Wert: nur als `EnvValue::Secret`.
const SECRET_ENV: [&str; 3] = ["GITLAB_TOKEN", "GITLAB_ACCESS_TOKEN", "OAUTH_TOKEN"];
/// Immer gesetzt: keine Rückfragen, kein Update-Check, keine Telemetrie,
/// keine Hinweise, keine Farben.
const ENV_SET: [(&str, &str); 6] = [
    ("GLAB_NO_PROMPT", "1"),
    ("GLAB_CHECK_UPDATE", "false"),
    ("GLAB_SEND_TELEMETRY", "0"),
    ("GLAB_SHOW_WHATS_NEW", "false"),
    ("GLAB_NOTIFY_SKILL_UPDATES", "false"),
    ("NO_COLOR", "1"),
];
/// Immer entfernt: HTTP-Debug-Ausgaben enthalten Header samt Token.
const ENV_REMOVE: [&str; 2] = ["GLAB_DEBUG_HTTP", "DEBUG"];
/// Entfernt, außer der Request setzt sie selbst (Prüfen mit „Erweitert“,
/// E17): ein geerbter Wert gälte für jeden Host und schickte dessen Token an
/// einen fremden API-Host bzw. unverschlüsselt.
const ENV_REMOVE_UNLESS_SET: [&str; 2] = ["GITLAB_API_HOST", "GLAB_API_PROTOCOL"];
/// glab-Config-Schlüssel mit Geheimnissen: nie per `config get` lesen.
const SECRET_CONFIG_KEYS: [&str; 3] = ["token", "job_token", "oauth2_refresh_token"];

/// Art des Aufrufs für das Gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    /// `api`, `version`, `auth status`, `config get`: parallel, je Host einzeln
    Read,
    /// `auth login`, `config set`: exklusiv
    Write,
}

/// Inhalt für Kind-stdin; stdin wird danach geschlossen (EOF).
#[derive(Debug)]
pub enum StdinBody {
    /// GraphQL-Body für `--input -`
    Json(String),
    /// Token für `auth login --stdin`
    Secret(SecretString),
}

/// Für den stdin-Thread von `process::finish`.
impl AsRef<[u8]> for StdinBody {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Json(s) => s.as_bytes(),
            Self::Secret(s) => s.expose().as_bytes(),
        }
    }
}

/// Wert einer Kind-Umgebungsvariable.
#[derive(Debug)]
pub enum EnvValue {
    Plain(String),
    /// Token für die Prüfung (`GITLAB_TOKEN`)
    Secret(SecretString),
}

impl EnvValue {
    fn expose(&self) -> &str {
        match self {
            Self::Plain(s) => s,
            Self::Secret(s) => s.expose(),
        }
    }
}

/// Ein glab-Aufruf. `Debug` zeigt keine Secrets.
#[derive(Debug)]
pub struct GlabRequest {
    /// glab-Host-Schlüssel für das Gate (ein Prozess je Host). Der Host
    /// selbst steht in `args` (`--hostname`/`--host`) bzw. in `GITLAB_HOST`
    /// (E16); `check_request` prüft, dass beides passt.
    pub host_key: Option<String>,
    pub args: Vec<String>,
    pub stdin: Option<StdinBody>,
    /// Wird nach `env_remove` angewandt
    pub env_set: Vec<(String, EnvValue)>,
    pub env_remove: Vec<String>,
    pub kind: CallKind,
    pub timeout: Duration,
}

impl GlabRequest {
    fn new(
        kind: CallKind,
        host_key: Option<&str>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            host_key: host_key.map(str::to_string),
            args: args.into_iter().map(Into::into).collect(),
            stdin: None,
            env_set: Vec::new(),
            env_remove: Vec::new(),
            kind,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Lesender Aufruf (`config get`, `auth status`, …); Host-Flag in `args`.
    pub fn read(host_key: Option<&str>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::new(CallKind::Read, host_key, args)
    }

    /// Schreibender Aufruf (`auth login`, `config set`), exklusiv im Gate.
    pub fn write(
        host_key: Option<&str>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::new(CallKind::Write, host_key, args)
    }

    /// `glab api -i …` an einen GitLab-Host: `--hostname H`; ein Schlüssel
    /// mit Port geht über `GITLAB_HOST`, weil `glab api` `:` in
    /// `--hostname` ablehnt (E16).
    pub fn api(host_key: &str, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut req = Self::read(Some(host_key), ["api"]);
        if host_key.contains(':') {
            req.env_set.push((
                "GITLAB_HOST".to_string(),
                EnvValue::Plain(host_key.to_string()),
            ));
        } else {
            req.args
                .extend(["--hostname".to_string(), host_key.to_string()]);
        }
        req.args.push("-i".to_string());
        req.args.extend(args.into_iter().map(Into::into));
        req
    }

    /// `glab version`
    pub fn version() -> Self {
        Self::read(None, ["version"])
    }

    pub fn with_stdin(mut self, body: StdinBody) -> Self {
        self.stdin = Some(body);
        self
    }

    pub fn with_env(mut self, key: &str, value: EnvValue) -> Self {
        self.env_set.push((key.to_string(), value));
        self
    }

    pub fn without_env(mut self, keys: &[&str]) -> Self {
        self.env_remove.extend(keys.iter().map(|k| k.to_string()));
        self
    }

    // Nur in Tests genutzt.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// `glab api … -i`: stdout beginnt mit dem HTTP-Kopf.
    fn includes_head(&self) -> bool {
        self.args.first().is_some_and(|a| a == "api")
            && self.args.iter().any(|a| a == "-i" || a == "--include")
    }
}

/// Statuszeile aus `glab api -i`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpHead {
    pub code: u16,
    pub reason: String,
}

/// Ergebnis eines beendeten glab-Prozesses. Der Body wird immer geparst,
/// unabhängig vom Exit-Code (glab endet bei jedem GraphQL-`errors` mit 1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlabOutput {
    pub exit: i32,
    /// Nur bei `glab api -i`
    pub http: Option<HttpHead>,
    /// stdout ohne HTTP-Kopf
    pub body: String,
    /// Ende von stderr (höchstens 8 KiB) für `classify`. Nie loggen;
    /// angezeigt wird nur `stderr_short()`.
    pub stderr_tail: String,
}

impl GlabOutput {
    /// Baut die Ausgabe aus rohem stdout/stderr; `include` = Aufruf mit `-i`.
    pub fn from_raw(exit: i32, stdout: &[u8], stderr: &[u8], include: bool) -> Self {
        let stdout = String::from_utf8_lossy(stdout);
        let (http, body) = if include {
            parse_include(&stdout)
        } else {
            (None, stdout.as_ref())
        };
        let tail = &stderr[stderr.len().saturating_sub(STDERR_KEEP)..];
        Self {
            exit,
            http,
            body: body.to_string(),
            stderr_tail: String::from_utf8_lossy(tail).into_owned(),
        }
    }

    /// Body als JSON, `None` bei leerem oder ungültigem Body.
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(self.body.trim()).ok()
    }

    /// stderr für die Anzeige: letzte 2 Zeilen, höchstens 300 Zeichen,
    /// Zeilen mit „token“ entfernt.
    pub fn stderr_short(&self) -> String {
        const MAX_CHARS: usize = 300;
        let lines: Vec<&str> = self
            .stderr_tail
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.to_lowercase().contains("token"))
            .collect();
        let short = lines[lines.len().saturating_sub(2)..].join("\n");
        if short.chars().count() <= MAX_CHARS {
            return short;
        }
        let mut cut: String = short.chars().take(MAX_CHARS - 1).collect();
        cut.push('…');
        cut
    }
}

/// Teilt `glab api -i`-Ausgabe in Statuszeile und Body: erste Zeile
/// `HTTP/<ver> <code> <grund>`, Header bis zur Leerzeile (`\r\n` oder `\n`),
/// Rest ist der Body. Ohne Kopf bleibt die Ausgabe unverändert.
pub fn parse_include(raw: &str) -> (Option<HttpHead>, &str) {
    let mut head = None;
    let mut rest = raw;
    while let Some(after) = rest.strip_prefix("HTTP/") {
        let (status, mut tail) = split_line(after);
        // `<ver> <code> <grund>`; der Grund darf fehlen
        let mut parts = status.splitn(3, ' ');
        let _version = parts.next();
        let Some(code) = parts.next().and_then(|c| c.parse::<u16>().ok()) else {
            break;
        };
        let reason = parts.next().unwrap_or_default().trim().to_string();
        // Header bis zur Leerzeile überspringen
        while !tail.is_empty() {
            let (line, next) = split_line(tail);
            tail = next;
            if line.is_empty() {
                break;
            }
        }
        head = Some(HttpHead { code, reason });
        rest = tail;
        // 1xx-Zwischenantworten: der echte Kopf folgt
        if !(100..200).contains(&code) {
            break;
        }
    }
    (head, rest)
}

/// Erste Zeile ohne `\r\n` bzw. `\n` und der Rest danach.
fn split_line(s: &str) -> (&str, &str) {
    match s.split_once('\n') {
        Some((line, rest)) => (line.strip_suffix('\r').unwrap_or(line), rest),
        None => (s.strip_suffix('\r').unwrap_or(s), ""),
    }
}

/// Prozessfehler (kein Ergebnis von glab). `Display` nur für Logs und
/// Tests; die UI übersetzt über `kind()` (i18n).
#[derive(Debug)]
pub enum GlabError {
    /// Programm nicht gefunden (auch nicht mit dem PATH aus der Registry)
    NotFound,
    /// Zeitüberschreitung; der Prozess ist beendet und geerntet
    Timeout(Duration),
    /// stdout über `MAX_OUTPUT`
    OutputTooLarge,
    /// Anfrage verletzt eine Regel (Programmierfehler, nichts gestartet)
    Rejected(&'static str),
    Io(std::io::Error),
}

impl std::fmt::Display for GlabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("glab nicht gefunden"),
            Self::Timeout(t) => write!(f, "glab: Zeitüberschreitung nach {} s", t.as_secs()),
            Self::OutputTooLarge => write!(f, "glab: Ausgabe über {} MB", MAX_OUTPUT >> 20),
            Self::Rejected(why) => write!(f, "glab-Aufruf abgelehnt: {why}"),
            Self::Io(e) => write!(f, "glab: {e}"),
        }
    }
}

impl std::error::Error for GlabError {}

impl GlabError {
    pub fn kind(&self) -> GlabErrorKind {
        match self {
            Self::NotFound => GlabErrorKind::NotInstalled,
            Self::Timeout(_) => GlabErrorKind::Timeout,
            Self::OutputTooLarge | Self::Rejected(_) | Self::Io(_) => GlabErrorKind::Other,
        }
    }
}

/// Fehlerart für Service und UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlabErrorKind {
    NotInstalled,
    TooOld(Version),
    NotAuthenticated,
    InvalidToken,
    Expired,
    Revoked,
    InsufficientScope,
    NotFound,
    Network,
    Tls,
    Timeout,
    /// glab lehnt den Host-Schlüssel ab (`error parsing '--hostname'`)
    HostKey,
    /// glab < 1.123 ohne Content-Type bei `--input` (E18)
    MissingContentType,
    /// GraphQL-`errors`; bei vorhandenem `data` ein Teilerfolg
    GraphQl(Vec<String>),
    Other,
}

/// Ordnet eine glab-Ausgabe ein; `None` = Erfolg. Wertet HTTP-Code und Body
/// aus, stderr nur für Transport- und Flag-Fehler (dort fehlt der Kopf).
pub fn classify(out: &GlabOutput) -> Option<GlabErrorKind> {
    const NO_CONTENT_TYPE: &str = "unexpected end of document";
    let body = out.body.to_lowercase();
    let stderr = out.stderr_tail.to_lowercase();
    let messages = graphql_errors(out);
    // E18: glab < 1.123 schickt `--input` ohne Content-Type. Nur in den
    // Fehlermeldungen suchen, `data` enthält beliebige Titel.
    if stderr.contains(NO_CONTENT_TYPE)
        || messages
            .iter()
            .any(|m| m.to_lowercase().contains(NO_CONTENT_TYPE))
    {
        return Some(GlabErrorKind::MissingContentType);
    }
    let code = out.http.as_ref().map(|h| h.code);
    match code {
        Some(401) => return Some(classify_401(&body)),
        Some(403) if body.contains("insufficient_scope") => {
            return Some(GlabErrorKind::InsufficientScope)
        }
        Some(404) => return Some(GlabErrorKind::NotFound),
        _ => {}
    }
    // GraphQL-`errors`, auch neben `data` (Teilerfolg)
    if !messages.is_empty() {
        return Some(GlabErrorKind::GraphQl(messages));
    }
    if code.is_some_and(|c| c >= 400) {
        return Some(GlabErrorKind::Other);
    }
    if out.exit == 0 {
        return None;
    }
    Some(classify_stderr(&stderr))
}

/// 401-Body (kleingeschrieben): Grund aus `error_description` bzw. GraphQL.
fn classify_401(body: &str) -> GlabErrorKind {
    if body.contains("token is expired") {
        GlabErrorKind::Expired
    } else if body.contains("token was revoked") {
        GlabErrorKind::Revoked
    } else if body.contains("invalid_token")
        || body.contains("invalid token")
        || body.contains("bad access token")
    {
        GlabErrorKind::InvalidToken
    } else {
        // `{"message":"401 Unauthorized"}`: ohne Token angefragt
        GlabErrorKind::NotAuthenticated
    }
}

/// Fehler ohne HTTP-Antwort (stderr kleingeschrieben): Flags, fehlende
/// Anmeldung, Transport. Go-Meldungen, unter Windows nicht geprüft.
fn classify_stderr(stderr: &str) -> GlabErrorKind {
    let has = |needles: &[&str]| needles.iter().any(|n| stderr.contains(n));
    if has(&["error parsing '--hostname'", "error parsing '--api-host'"]) {
        GlabErrorKind::HostKey
    } else if has(&[
        "has not been authenticated",
        "missing from the operating system keyring",
    ]) {
        GlabErrorKind::NotAuthenticated
    } else if has(&["invalid_grant"]) {
        // OAuth-Refresh gescheitert: neu anmelden
        GlabErrorKind::Expired
    } else if has(&["timeout", "deadline exceeded", "timed out"]) {
        GlabErrorKind::Timeout
    } else if has(&["x509", "certificate", "tls:"]) {
        GlabErrorKind::Tls
    } else if has(&[
        "no such host",
        "dial tcp",
        "connection refused",
        "connectex",
        "network is unreachable",
        "proxyconnect",
        "connection reset",
    ]) {
        GlabErrorKind::Network
    } else {
        GlabErrorKind::Other
    }
}

/// `errors[].message` aus einem GraphQL-Body.
fn graphql_errors(out: &GlabOutput) -> Vec<String> {
    out.json()
        .map(|json| graphql_messages(&json))
        .unwrap_or_default()
}

/// `errors[].message` aus einem schon geparsten GraphQL-Body.
pub(super) fn graphql_messages(json: &serde_json::Value) -> Vec<String> {
    let Some(errors) = json.get("errors").and_then(|e| e.as_array()) else {
        return Vec::new();
    };
    errors
        .iter()
        .filter_map(|e| match e {
            serde_json::Value::String(s) => Some(s.clone()),
            other => other.get("message")?.as_str().map(str::to_string),
        })
        .collect()
}

/// Führt glab-Aufrufe aus. App, Einstellungen und Tests teilen sich eine
/// Instanz (`Arc<dyn GlabRunner>`, immer in `GatedRunner`).
pub trait GlabRunner: Send + Sync {
    fn run(&self, req: GlabRequest) -> Result<GlabOutput, GlabError>;
}

/// Liest die glab-Version (`glab 1.123.0 (…)`); älter als 1.111 → `TooOld`.
pub fn glab_version(runner: &dyn GlabRunner) -> Result<Version, GlabErrorKind> {
    let out = runner.run(GlabRequest::version()).map_err(|e| e.kind())?;
    if out.exit != 0 {
        return Err(classify(&out).unwrap_or(GlabErrorKind::Other));
    }
    let version = parse_glab_version(&out.body).ok_or(GlabErrorKind::Other)?;
    if version < MIN_GLAB_VERSION {
        return Err(GlabErrorKind::TooOld(version));
    }
    Ok(version)
}

/// Erste Versionsnummer in der Ausgabe von `glab version`
/// (`glab 1.123.0 (abc)` bzw. älter `glab version 1.22.0 (2022-03-24)`).
fn parse_glab_version(text: &str) -> Option<Version> {
    text.split_whitespace()
        .find_map(|word| Version::parse(word.trim_start_matches('v')).ok())
}

/// Prüft die Regeln aus den Global Constraints, bevor ein Prozess startet:
/// kein Token in argv oder Klartext-Env, kein `config get`/`config set` für
/// Geheimnisse, schreibende Aufrufe nur als `CallKind::Write` (Gate),
/// Netz-Aufrufe nur mit ausdrücklichem Host (sonst nähme glab gitlab.com),
/// `api` immer mit `-i`, `--input` nur mit JSON-Content-Type (E18).
fn check_request(req: &GlabRequest) -> Result<(), GlabError> {
    let args: Vec<&str> = req.args.iter().map(String::as_str).collect();
    let Some(&command) = args.first() else {
        return Err(GlabError::Rejected("leerer glab-Aufruf"));
    };
    // `--show-token` (bei `auth status` auch `-t`) gibt das Token aus.
    let token_flag = |a: &&str| {
        a.starts_with("--token")
            || a.starts_with("--job-token")
            || a.starts_with("--show-token")
            || (command == "auth" && *a == "-t")
    };
    if args.iter().any(token_flag) {
        return Err(GlabError::Rejected("Token in argv"));
    }
    let secret_key = |a: &&str| SECRET_CONFIG_KEYS.contains(&a.to_ascii_lowercase().as_str());
    let config = args.starts_with(&["config", "get"]) || args.starts_with(&["config", "set"]);
    if config && args.iter().any(secret_key) {
        return Err(GlabError::Rejected(
            "config get/set für einen geheimen Schlüssel",
        ));
    }
    let plain_token = req.env_set.iter().any(|(k, v)| {
        matches!(v, EnvValue::Plain(_)) && SECRET_ENV.iter().any(|s| s.eq_ignore_ascii_case(k))
    });
    if plain_token {
        return Err(GlabError::Rejected("Token als EnvValue::Plain"));
    }
    // Nur `CallKind::Write` läuft im Gate exklusiv.
    let writes: [&[&str]; 3] = [&["auth", "login"], &["auth", "logout"], &["config", "set"]];
    if req.kind != CallKind::Write && writes.iter().any(|w| args.starts_with(w)) {
        return Err(GlabError::Rejected(
            "schreibender Aufruf ohne CallKind::Write",
        ));
    }
    let host_flag = args.iter().any(|a| {
        matches!(*a, "--hostname" | "--host")
            || a.starts_with("--hostname=")
            || a.starts_with("--host=")
    });
    let host_env = req.env_set.iter().any(|(k, _)| k == "GITLAB_HOST");
    match req.host_key.as_deref() {
        None if command == "api" || host_flag || host_env => {
            return Err(GlabError::Rejected("Host-Schlüssel fehlt"));
        }
        Some(host) if !selects_host(req, &args, host) => {
            return Err(GlabError::Rejected("Host nicht in argv oder GITLAB_HOST"));
        }
        _ => {}
    }
    if command == "api" && !args.iter().any(|a| matches!(*a, "-i" | "--include")) {
        return Err(GlabError::Rejected("glab api ohne -i"));
    }
    let json_header = args.windows(2).any(|w| {
        matches!(w[0], "-H" | "--header")
            && w[1]
                .replace(' ', "")
                .eq_ignore_ascii_case("content-type:application/json")
    });
    if args.contains(&"--input") && !json_header {
        return Err(GlabError::Rejected(
            "--input ohne Content-Type: application/json",
        ));
    }
    Ok(())
}

/// Wählt der Aufruf genau den Host des Gate-Schlüssels?
fn selects_host(req: &GlabRequest, args: &[&str], host: &str) -> bool {
    let flag = args
        .windows(2)
        .any(|w| matches!(w[0], "--hostname" | "--host") && w[1].eq_ignore_ascii_case(host))
        || args.iter().any(|a| {
            a.strip_prefix("--hostname=")
                .or_else(|| a.strip_prefix("--host="))
                .is_some_and(|v| v.eq_ignore_ascii_case(host))
        });
    let env = req.env_set.iter().any(|(k, v)| {
        k == "GITLAB_HOST" && matches!(v, EnvValue::Plain(p) if p.eq_ignore_ascii_case(host))
    });
    flag || env
}

/// Eine Variable der Kind-Umgebung.
struct EnvVar<'a> {
    key: &'a str,
    value: &'a str,
    /// Für das Protokoll von `FakeGlab`
    #[cfg_attr(not(test), allow(dead_code))]
    secret: bool,
}

/// Kind-Umgebung: erst `remove`, dann `set`.
struct EnvPlan<'a> {
    remove: Vec<&'a str>,
    set: Vec<EnvVar<'a>>,
}

/// Feste Regeln + Request. Ein Schlüssel in beiden Listen ist ein
/// Programmierfehler.
fn env_plan(req: &GlabRequest) -> EnvPlan<'_> {
    let sets = |key: &str| req.env_set.iter().any(|(k, _)| k.eq_ignore_ascii_case(key));
    let remove: Vec<&str> = ENV_REMOVE
        .into_iter()
        .chain(ENV_REMOVE_UNLESS_SET.into_iter().filter(|k| !sets(k)))
        .chain(req.env_remove.iter().map(String::as_str))
        .collect();
    let set: Vec<EnvVar<'_>> = ENV_SET
        .into_iter()
        .map(|(key, value)| EnvVar {
            key,
            value,
            secret: false,
        })
        .chain(req.env_set.iter().map(|(key, value)| EnvVar {
            key,
            value: value.expose(),
            secret: matches!(value, EnvValue::Secret(_)),
        }))
        .collect();
    // Windows vergleicht Env-Schlüssel ohne Groß-/Kleinschreibung.
    debug_assert!(
        !set.iter()
            .any(|v| remove.iter().any(|r| r.eq_ignore_ascii_case(v.key))),
        "Env-Schlüssel steht in env_remove und env_set"
    );
    debug_assert!(
        set.iter()
            .enumerate()
            .all(|(i, v)| set[..i].iter().all(|w| !w.key.eq_ignore_ascii_case(v.key))),
        "Env-Schlüssel doppelt in env_set"
    );
    EnvPlan { remove, set }
}

/// Startet das echte glab (bzw. `program`) ohne Konsolenfenster in einem
/// neutralen Arbeitsverzeichnis. Das Gate liegt in `GatedRunner`.
pub struct SystemGlab {
    pub program: String,
}

impl SystemGlab {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// Startet den Prozess; `path` ersetzt den PATH des Kindes. Den
    /// `Command` nie mit `{:?}` ausgeben (enthält die Kind-Env).
    fn spawn(
        &self,
        req: &GlabRequest,
        with_stdin: bool,
        path: Option<&OsString>,
    ) -> std::io::Result<Child> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&req.args)
            .stdin(if with_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Neutral: in einem Repo läse glab `.git/glab-cli/config.yml` und
        // nähme den Host des Remotes.
        let tmp = std::env::temp_dir();
        if tmp.is_dir() {
            cmd.current_dir(tmp);
        }
        let plan = env_plan(req);
        for key in plan.remove {
            cmd.env_remove(key);
        }
        for var in plan.set {
            cmd.env(var.key, var.value);
        }
        if let Some(path) = path {
            cmd.env("PATH", path);
        }
        crate::git::set_creation_flags(&mut cmd, CREATE_NO_WINDOW);
        cmd.spawn()
    }
}

impl GlabRunner for SystemGlab {
    fn run(&self, mut req: GlabRequest) -> Result<GlabOutput, GlabError> {
        #[cfg(test)]
        assert!(
            !is_real_glab(&self.program),
            "Tests dürfen glab nicht starten (FakeGlab verwenden): {}",
            self.program
        );
        check_request(&req)?;
        let stdin = req.stdin.take();
        let spawned = match self.spawn(&req, stdin.is_some(), None) {
            // Nach `winget install` kennt ein laufender GUI-Prozess glab
            // noch nicht: einmal mit dem PATH aus der Registry versuchen.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match fresh_path() {
                Some(path) => self.spawn(&req, stdin.is_some(), Some(&path)),
                None => Err(e),
            },
            other => other,
        };
        let child = spawned.map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => GlabError::NotFound,
            _ => GlabError::Io(e),
        })?;
        // stdout ganz (höchstens `MAX_OUTPUT`), von stderr nur das Ende
        let limits = Limits {
            timeout: req.timeout,
            stdout: Keep::Head(MAX_OUTPUT),
            stderr: Keep::Tail(STDERR_KEEP),
        };
        let out = process::finish(child, stdin, limits).map_err(|e| match e {
            RunError::Timeout(timeout) => GlabError::Timeout(timeout),
            RunError::OutputTooLarge => GlabError::OutputTooLarge,
            RunError::Spawn(e) | RunError::Wait(e) => GlabError::Io(e),
        })?;
        Ok(GlabOutput::from_raw(
            out.status.code().unwrap_or(-1),
            &out.stdout,
            &out.stderr,
            req.includes_head(),
        ))
    }
}

/// Schutz für Tests: das echte glab nie starten.
#[cfg(test)]
fn is_real_glab(program: &str) -> bool {
    std::path::Path::new(program)
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case("glab"))
}

/// Aktueller PATH aus der Registry (Windows): Ein GUI- oder Autostart-
/// Prozess kennt nur den PATH vom Login, `winget install` ändert ihn erst in
/// der Registry.
#[cfg(windows)]
fn fresh_path() -> Option<OsString> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let read = |root, subkey: &str| {
        RegKey::predef(root)
            .open_subkey(subkey)
            .ok()?
            .get_value::<String, _>("Path")
            .ok()
    };
    let system = read(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
    );
    let user = read(HKEY_CURRENT_USER, "Environment");
    let joined = join_path_lists(system.as_deref(), user.as_deref())?;
    Some(expand_env_vars(&joined, |name| std::env::var(name).ok()).into())
}

#[cfg(not(windows))]
fn fresh_path() -> Option<OsString> {
    None
}

/// Windows-PATH aus der Registry: System vor Benutzer, wie beim Login.
#[cfg(any(windows, test))]
fn join_path_lists(system: Option<&str>, user: Option<&str>) -> Option<String> {
    let parts: Vec<&str> = [system, user]
        .into_iter()
        .flatten()
        .map(|p| p.trim_matches(';'))
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(";"))
}

/// Expandiert `%NAME%` (REG_EXPAND_SZ); Unbekanntes bleibt stehen.
#[cfg(any(windows, test))]
fn expand_env_vars(text: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            // `%%` oder `%` ohne Gegenstück bleibt stehen
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Locks des prozessweiten Gates. Alle Runner einer Sitzung teilen sich
/// eins (`GatedRunner::with_gate`), auch nach einem Wechsel des
/// glab-Programms und beim Prüfen eines noch nicht gespeicherten Programms.
#[derive(Default)]
pub struct GlabGate {
    rw: RwLock<()>,
    hosts: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl GlabGate {
    fn host_lock(&self, host: &str) -> Arc<Mutex<()>> {
        let mut hosts = self.hosts.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(hosts.entry(host.to_ascii_lowercase()).or_default())
    }
}

/// Prozessweites Gate: lesende Aufrufe teilen sich den Lese-Lock und laufen
/// je Host einzeln (höchstens ein glab-Prozess je Host); `auth login` und
/// `config set` nehmen den Schreib-Lock und laufen exklusiv.
pub struct GatedRunner<R: GlabRunner> {
    inner: R,
    gate: Arc<GlabGate>,
}

impl<R: GlabRunner> GatedRunner<R> {
    /// Runner mit eigenem Gate (Tests); die App baut ihre Runner mit
    /// `with_gate` an einem gemeinsamen Gate.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(inner: R) -> Self {
        Self::with_gate(inner, Arc::default())
    }

    /// Runner hinter einem geteilten Gate.
    pub fn with_gate(inner: R, gate: Arc<GlabGate>) -> Self {
        Self { inner, gate }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn inner(&self) -> &R {
        &self.inner
    }
}

impl<R: GlabRunner> GlabRunner for GatedRunner<R> {
    fn run(&self, req: GlabRequest) -> Result<GlabOutput, GlabError> {
        // Reihenfolge immer Gate → Host, damit sich nichts verklemmt.
        // Vergiftete Locks (Panic in einem Aufruf) sperren weiter normal.
        match req.kind {
            CallKind::Write => {
                let _gate = self.gate.rw.write().unwrap_or_else(PoisonError::into_inner);
                self.inner.run(req)
            }
            CallKind::Read => {
                let _gate = self.gate.rw.read().unwrap_or_else(PoisonError::into_inner);
                let host = req.host_key.as_deref().map(|h| self.gate.host_lock(h));
                let _host = host
                    .as_ref()
                    .map(|m| m.lock().unwrap_or_else(PoisonError::into_inner));
                self.inner.run(req)
            }
        }
    }
}

#[cfg(test)]
pub use fake::{FakeGlab, FakeReply};

/// Test-Runner: Antworten-Queue, Protokoll der Requests samt Kind-Env,
/// Messung der Parallelität. In Tests immer in `GatedRunner` gewickelt.
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::MutexGuard;
    use std::time::Instant;

    /// Platzhalter für Secrets im protokollierten Env.
    pub const SECRET_SHOWN: &str = "***";

    /// Vorbereitete Antwort.
    pub struct FakeReply {
        result: Result<GlabOutput, GlabError>,
        delay: Duration,
        /// Antwort erst, wenn einmal so viele Aufrufe gleichzeitig liefen
        /// (höchstens 5 s)
        rendezvous: usize,
    }

    impl FakeReply {
        pub fn output(out: GlabOutput) -> Self {
            Self {
                result: Ok(out),
                delay: Duration::ZERO,
                rendezvous: 0,
            }
        }

        /// Antwort von `glab api -i`: ab HTTP 400 Exit 1 und Fehlerzeile auf
        /// stderr, wie glab.
        pub fn http(code: u16, body: &str) -> Self {
            let reason = match code {
                200 => "OK",
                400 => "Bad Request",
                401 => "Unauthorized",
                403 => "Forbidden",
                404 => "Not Found",
                _ => "",
            };
            let failed = code >= 400;
            Self::output(GlabOutput {
                exit: i32::from(failed),
                http: Some(HttpHead {
                    code,
                    reason: reason.to_string(),
                }),
                body: body.to_string(),
                stderr_tail: if failed {
                    format!("glab: {code} {reason} (HTTP {code})")
                } else {
                    String::new()
                },
            })
        }

        /// Ausgabe ohne HTTP-Kopf (`version`, `config get`, `auth status`).
        pub fn stdout(exit: i32, stdout: &str, stderr: &str) -> Self {
            Self::output(GlabOutput {
                exit,
                http: None,
                body: stdout.to_string(),
                stderr_tail: stderr.to_string(),
            })
        }

        pub fn error(err: GlabError) -> Self {
            Self {
                result: Err(err),
                delay: Duration::ZERO,
                rendezvous: 0,
            }
        }

        pub fn delay(mut self, delay: Duration) -> Self {
            self.delay = delay;
            self
        }

        pub fn rendezvous(mut self, parallel: usize) -> Self {
            self.rendezvous = parallel;
            self
        }
    }

    /// Protokollierter Aufruf.
    #[derive(Debug)]
    pub struct FakeCall {
        pub request: GlabRequest,
        /// Kind-Env nach `env_remove` → `env_set`; Secrets als `***`
        pub env: BTreeMap<String, String>,
    }

    type Matcher = Box<dyn Fn(&GlabRequest) -> bool + Send>;

    type ReplyFactory = Box<dyn Fn() -> FakeReply + Send>;

    #[derive(Default)]
    struct FakeState {
        replies: VecDeque<(Matcher, FakeReply)>,
        /// Antworten ohne Verbrauch (nach der Queue geprüft, nicht in `pending`)
        sticky: Vec<(Matcher, ReplyFactory)>,
        calls: Vec<FakeCall>,
        in_flight: HashMap<String, usize>,
        max_parallel: HashMap<String, usize>,
        total: usize,
        max_total: usize,
        write_running: bool,
        write_overlap: bool,
    }

    #[derive(Default)]
    pub struct FakeGlab {
        parent_env: BTreeMap<String, String>,
        state: Mutex<FakeState>,
    }

    impl FakeGlab {
        pub fn new() -> Self {
            Self::default()
        }

        /// Umgebung des GitManager-Prozesses (z. B. ein gesetztes `GITLAB_TOKEN`).
        pub fn with_parent_env(mut self, vars: &[(&str, &str)]) -> Self {
            self.parent_env
                .extend(vars.iter().map(|(k, v)| (k.to_string(), v.to_string())));
            self
        }

        /// Antwort für den nächsten beliebigen Aufruf.
        pub fn push(&self, reply: FakeReply) {
            self.push_when(|_| true, reply);
        }

        /// Antwort für den ersten Aufruf, auf den `matches` passt.
        pub fn push_when(
            &self,
            matches: impl Fn(&GlabRequest) -> bool + Send + 'static,
            reply: FakeReply,
        ) {
            self.lock().replies.push_back((Box::new(matches), reply));
        }

        /// Dauerhafte Antwort für jeden passenden Aufruf, auf den keine
        /// Antwort aus der Queue passt (z. B. `config path` in App-Tests).
        pub fn always_when(
            &self,
            matches: impl Fn(&GlabRequest) -> bool + Send + 'static,
            reply: impl Fn() -> FakeReply + Send + 'static,
        ) {
            self.lock()
                .sticky
                .push((Box::new(matches), Box::new(reply)));
        }

        /// Protokoll in Aufrufreihenfolge (wird geleert).
        pub fn take_calls(&self) -> Vec<FakeCall> {
            std::mem::take(&mut self.lock().calls)
        }

        /// Größte Zahl gleichzeitiger Aufrufe für einen Host („“ = ohne Host).
        pub fn max_parallel(&self, host: &str) -> usize {
            self.lock().max_parallel.get(host).copied().unwrap_or(0)
        }

        /// Größte Zahl gleichzeitiger Aufrufe insgesamt.
        pub fn max_total(&self) -> usize {
            self.lock().max_total
        }

        /// Lief ein Schreib-Aufruf gleichzeitig mit einem anderen Aufruf?
        pub fn write_overlapped(&self) -> bool {
            self.lock().write_overlap
        }

        /// Noch nicht abgerufene Antworten.
        pub fn pending(&self) -> usize {
            self.lock().replies.len()
        }

        fn lock(&self) -> MutexGuard<'_, FakeState> {
            self.state.lock().unwrap_or_else(PoisonError::into_inner)
        }

        fn child_env(&self, req: &GlabRequest) -> BTreeMap<String, String> {
            let plan = env_plan(req);
            let mut env = self.parent_env.clone();
            for key in plan.remove {
                env.remove(key);
            }
            for var in plan.set {
                let value = if var.secret { SECRET_SHOWN } else { var.value };
                env.insert(var.key.to_string(), value.to_string());
            }
            env
        }
    }

    impl GlabRunner for FakeGlab {
        fn run(&self, req: GlabRequest) -> Result<GlabOutput, GlabError> {
            if let Err(err) = check_request(&req) {
                // Nur Befehl und Unterbefehl: argv kann das Token enthalten.
                let command: Vec<&String> = req
                    .args
                    .iter()
                    .take_while(|a| !a.starts_with('-'))
                    .take(2)
                    .collect();
                panic!("FakeGlab: Regelverstoß {err:?} bei {command:?}");
            }
            let env = self.child_env(&req);
            let host = req.host_key.clone().unwrap_or_default();
            let kind = req.kind;
            let reply = {
                let mut st = self.lock();
                let queued = st.replies.iter().position(|(m, _)| m(&req));
                let reply = match queued {
                    Some(pos) => st.replies.remove(pos).expect("Position aus der Queue").1,
                    None => match st.sticky.iter().find(|(m, _)| m(&req)) {
                        Some((_, make)) => make(),
                        None => panic!("FakeGlab: keine Antwort für {:?}", req.args),
                    },
                };
                if st.write_running || (kind == CallKind::Write && st.total > 0) {
                    st.write_overlap = true;
                }
                st.write_running |= kind == CallKind::Write;
                st.total += 1;
                st.max_total = st.max_total.max(st.total);
                let running = {
                    let n = st.in_flight.entry(host.clone()).or_default();
                    *n += 1;
                    *n
                };
                let max = st.max_parallel.entry(host.clone()).or_default();
                *max = (*max).max(running);
                st.calls.push(FakeCall { request: req, env });
                reply
            };
            if reply.rendezvous > 1 {
                let until = Instant::now() + Duration::from_secs(5);
                while self.lock().max_total < reply.rendezvous && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            std::thread::sleep(reply.delay);
            {
                let mut st = self.lock();
                st.total -= 1;
                if let Some(n) = st.in_flight.get_mut(&host) {
                    *n -= 1;
                }
                if kind == CallKind::Write {
                    st.write_running = false;
                }
            }
            reply.result
        }
    }

    /// Ein Fake hinter mehreren `GatedRunner` (geteiltes Gate).
    impl GlabRunner for Arc<FakeGlab> {
        fn run(&self, req: GlabRequest) -> Result<GlabOutput, GlabError> {
            FakeGlab::run(self, req)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    const TOKEN: &str = "glpat-Geheim0815xyz";

    fn http_out(code: u16, body: &str, exit: i32) -> GlabOutput {
        GlabOutput {
            exit,
            http: Some(HttpHead {
                code,
                reason: String::new(),
            }),
            body: body.to_string(),
            stderr_tail: String::new(),
        }
    }

    fn stderr_out(exit: i32, stderr: &str) -> GlabOutput {
        GlabOutput {
            exit,
            http: None,
            body: String::new(),
            stderr_tail: stderr.to_string(),
        }
    }

    fn gated() -> GatedRunner<FakeGlab> {
        GatedRunner::new(FakeGlab::new())
    }

    fn secret(s: &str) -> SecretString {
        SecretString::new(s.to_string())
    }

    // --- Step 1: classify und `-i`-Parser -------------------------------

    #[test]
    fn classify_graphql_401_invalid_token() {
        let out = http_out(401, r#"{"errors":[{"message":"Invalid token"}]}"#, 1);
        assert_eq!(classify(&out), Some(GlabErrorKind::InvalidToken));
    }

    #[test]
    fn classify_rest_401_reasons() {
        let cases = [
            (
                r#"{"error":"invalid_token","error_description":"Bad Access Token."}"#,
                GlabErrorKind::InvalidToken,
            ),
            (
                r#"{"error":"invalid_token","error_description":"Token is expired. You can either do re-authorization or token refresh."}"#,
                GlabErrorKind::Expired,
            ),
            (
                r#"{"error":"invalid_token","error_description":"Token was revoked. You have to re-authorize from the user."}"#,
                GlabErrorKind::Revoked,
            ),
            // ohne Token (keine Anmeldung gespeichert)
            (
                r#"{"message":"401 Unauthorized"}"#,
                GlabErrorKind::NotAuthenticated,
            ),
        ];
        for (body, want) in cases {
            assert_eq!(classify(&http_out(401, body, 1)), Some(want), "{body}");
        }
    }

    #[test]
    fn classify_403_and_404() {
        let scope = r#"{"error":"insufficient_scope","error_description":"The request requires higher privileges than provided by the access token.","scope":"api read_api"}"#;
        assert_eq!(
            classify(&http_out(403, scope, 1)),
            Some(GlabErrorKind::InsufficientScope)
        );
        assert_eq!(
            classify(&http_out(403, r#"{"message":"403 Forbidden"}"#, 1)),
            Some(GlabErrorKind::Other)
        );
        assert_eq!(
            classify(&http_out(404, r#"{"message":"404 Project Not Found"}"#, 1)),
            Some(GlabErrorKind::NotFound)
        );
        assert_eq!(
            classify(&http_out(502, "<html>Bad Gateway</html>", 1)),
            Some(GlabErrorKind::Other)
        );
    }

    #[test]
    fn classify_graphql_partial_success_keeps_data() {
        // glab endet mit 1, sobald `errors` vorkommt – auch bei Teilerfolg.
        let body = r#"{"data":{"currentUser":{"username":"tim"}},"errors":[{"message":"Field 'approvalsLeft' doesn't exist on type 'MergeRequest'","extensions":{"code":"undefinedField","typeName":"MergeRequest","fieldName":"approvalsLeft"}}]}"#;
        let out = http_out(200, body, 1);
        assert_eq!(
            classify(&out),
            Some(GlabErrorKind::GraphQl(vec![
                "Field 'approvalsLeft' doesn't exist on type 'MergeRequest'".to_string()
            ]))
        );
        let json = out.json().expect("Body bleibt lesbar");
        assert_eq!(json["data"]["currentUser"]["username"], "tim");
    }

    #[test]
    fn classify_success_is_none() {
        let out = http_out(200, r#"{"data":{"currentUser":null}}"#, 0);
        assert_eq!(classify(&out), None);
        // `config get` ohne HTTP-Kopf
        let out = GlabOutput {
            exit: 0,
            body: "tim\n".to_string(),
            ..GlabOutput::default()
        };
        assert_eq!(classify(&out), None);
    }

    #[test]
    fn classify_missing_content_type() {
        let body = r#"{"errors":[{"message":"Unexpected end of document","locations":[]}]}"#;
        assert_eq!(
            classify(&http_out(200, body, 1)),
            Some(GlabErrorKind::MissingContentType)
        );
        assert_eq!(
            classify(&stderr_out(1, "glab: Unexpected end of document")),
            Some(GlabErrorKind::MissingContentType)
        );
        // derselbe Text in den Daten (MR-Titel) ist kein Fehler
        let body = r#"{"data":{"mr":{"title":"Unexpected end of document beheben"}}}"#;
        assert_eq!(classify(&http_out(200, body, 0)), None);
    }

    #[test]
    fn classify_stderr_transport_and_flag_errors() {
        let cases = [
            (
                "Error: error parsing '--hostname': invalid hostname \"git.firma.de:8443\"",
                GlabErrorKind::HostKey,
            ),
            (
                "glab: Post \"https://gitlab.firma.de/api/graphql\": tls: failed to verify certificate: x509: certificate signed by unknown authority",
                GlabErrorKind::Tls,
            ),
            (
                "glab: Post \"https://gitlab.local/api/graphql\": dial tcp: lookup gitlab.local: no such host",
                GlabErrorKind::Network,
            ),
            (
                "glab: Post \"https://10.0.0.5/api/graphql\": dial tcp 10.0.0.5:443: connectex: No connection could be made because the target machine actively refused it.",
                GlabErrorKind::Network,
            ),
            (
                "glab: Post \"https://gitlab.firma.de/api/graphql\": net/http: request canceled while waiting for connection (Client.Timeout exceeded while awaiting headers)",
                GlabErrorKind::Timeout,
            ),
            (
                "gitlab.firma.de has not been authenticated with glab. Run `glab auth login --hostname gitlab.firma.de` to authenticate.",
                GlabErrorKind::NotAuthenticated,
            ),
            (
                "glab: the token for \"gitlab.firma.de\" is missing from the operating system keyring; run glab auth login --hostname gitlab.firma.de",
                GlabErrorKind::NotAuthenticated,
            ),
            (
                "glab: oauth2: \"invalid_grant\" \"The provided authorization grant is invalid, expired, revoked\"",
                GlabErrorKind::Expired,
            ),
            ("glab: something else went wrong", GlabErrorKind::Other),
        ];
        for (stderr, want) in cases {
            assert_eq!(classify(&stderr_out(1, stderr)), Some(want), "{stderr}");
        }
    }

    #[test]
    fn parse_include_crlf_head() {
        let raw = "HTTP/2.0 401 Unauthorized\r\nContent-Type: application/json\r\nDate: Sat, 17 Oct 2026 10:00:00 GMT\r\n\r\n{\"errors\":[{\"message\":\"Invalid token\"}]}";
        let (head, body) = parse_include(raw);
        assert_eq!(
            head,
            Some(HttpHead {
                code: 401,
                reason: "Unauthorized".to_string()
            })
        );
        assert_eq!(body, "{\"errors\":[{\"message\":\"Invalid token\"}]}");
    }

    #[test]
    fn parse_include_variants() {
        // nur `\n`, Body mit Leerzeilen bleibt ganz
        let (head, body) = parse_include("HTTP/1.1 200 OK\nX-A: 1\n\n{\n\n}");
        assert_eq!(head.map(|h| h.code), Some(200));
        assert_eq!(body, "{\n\n}");
        // 1xx-Zwischenantwort überspringen
        let (head, body) =
            parse_include("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 404 Not Found\r\n\r\n{}");
        assert_eq!(
            head,
            Some(HttpHead {
                code: 404,
                reason: "Not Found".to_string()
            })
        );
        assert_eq!(body, "{}");
        // ohne Kopf unverändert, auch kaputte Statuszeile
        assert_eq!(parse_include("{\"a\":1}"), (None, "{\"a\":1}"));
        assert_eq!(parse_include("HTTP/x kaputt"), (None, "HTTP/x kaputt"));
        // leerer Body, Statuszeile ohne Grund
        let (head, body) = parse_include("HTTP/2.0 204\r\n\r\n");
        assert_eq!(
            head,
            Some(HttpHead {
                code: 204,
                reason: String::new()
            })
        );
        assert_eq!(body, "");
    }

    #[test]
    fn from_raw_parses_head_only_with_include() {
        let raw = b"HTTP/2.0 200 OK\r\n\r\n{}";
        let out = GlabOutput::from_raw(0, raw, b"warn\n", true);
        assert_eq!(out.http.as_ref().map(|h| h.code), Some(200));
        assert_eq!(out.body, "{}");
        assert_eq!(out.stderr_tail, "warn\n");
        let out = GlabOutput::from_raw(0, raw, b"", false);
        assert_eq!(out.http, None);
        assert!(out.body.starts_with("HTTP/2.0"));
    }

    #[test]
    fn stderr_short_rules() {
        let out = stderr_out(
            1,
            "erste Zeile\nzweite Zeile\nglab: the token for \"h\" is missing\n\nletzte Zeile\n",
        );
        assert_eq!(out.stderr_short(), "zweite Zeile\nletzte Zeile");
        let long = "x".repeat(500);
        let short = stderr_out(1, &long).stderr_short();
        assert_eq!(short.chars().count(), 300);
        assert!(short.ends_with('…'));
        assert_eq!(stderr_out(1, "Token: abc").stderr_short(), "");
    }

    #[test]
    fn parse_glab_version_formats() {
        assert_eq!(
            parse_glab_version("glab 1.123.0 (a300719)\n"),
            Some(Version::new(1, 123, 0))
        );
        assert_eq!(
            parse_glab_version("glab version 1.22.0 (2022-03-24)\n"),
            Some(Version::new(1, 22, 0))
        );
        assert_eq!(
            parse_glab_version("glab v1.111.2"),
            Some(Version::new(1, 111, 2))
        );
        assert_eq!(parse_glab_version("glab DEV"), None);
        assert_eq!(parse_glab_version(""), None);
    }

    #[test]
    fn glab_version_via_runner() {
        let runner = gated();
        runner
            .inner()
            .push(FakeReply::stdout(0, "glab 1.123.0 (a300719)\n", ""));
        runner
            .inner()
            .push(FakeReply::stdout(0, "glab 1.110.0 (abc)\n", ""));
        runner.inner().push(FakeReply::error(GlabError::NotFound));
        runner.inner().push(FakeReply::stdout(0, "kaputt", ""));
        runner
            .inner()
            .push(FakeReply::error(GlabError::Timeout(DEFAULT_TIMEOUT)));
        assert_eq!(glab_version(&runner), Ok(Version::new(1, 123, 0)));
        assert_eq!(
            glab_version(&runner),
            Err(GlabErrorKind::TooOld(Version::new(1, 110, 0)))
        );
        assert_eq!(glab_version(&runner), Err(GlabErrorKind::NotInstalled));
        assert_eq!(glab_version(&runner), Err(GlabErrorKind::Other));
        assert_eq!(glab_version(&runner), Err(GlabErrorKind::Timeout));
        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 5);
        assert!(calls.iter().all(|c| c.request.args == ["version"]
            && c.request.host_key.is_none()
            && c.request.kind == CallKind::Read));
    }

    #[test]
    fn error_kinds_of_process_errors() {
        assert_eq!(GlabError::NotFound.kind(), GlabErrorKind::NotInstalled);
        assert_eq!(
            GlabError::Timeout(DEFAULT_TIMEOUT).kind(),
            GlabErrorKind::Timeout
        );
        assert_eq!(GlabError::OutputTooLarge.kind(), GlabErrorKind::Other);
        assert_eq!(GlabError::Rejected("x").kind(), GlabErrorKind::Other);
        assert_eq!(
            GlabError::Timeout(DEFAULT_TIMEOUT).to_string(),
            "glab: Zeitüberschreitung nach 45 s"
        );
        assert_eq!(
            GlabError::OutputTooLarge.to_string(),
            "glab: Ausgabe über 16 MB"
        );
        assert_eq!(
            GlabError::Rejected("Token in argv").to_string(),
            "glab-Aufruf abgelehnt: Token in argv"
        );
    }

    // --- Step 2: Secrets, Request-Regeln, Kind-Env ----------------------

    #[test]
    fn request_debug_hides_token() {
        let req = GlabRequest::api("gitlab.example.com", ["personal_access_tokens/self"])
            .without_env(&TOKEN_ENV[1..])
            .with_env("GITLAB_TOKEN", EnvValue::Secret(secret(TOKEN)));
        let shown = format!("{req:?} {req:#?}");
        assert!(!shown.contains(TOKEN), "{shown}");
        assert!(shown.contains("SecretString(***)"), "{shown}");
        let login = GlabRequest::write(
            Some("gitlab.example.com"),
            [
                "auth",
                "login",
                "--hostname",
                "gitlab.example.com",
                "--stdin",
            ],
        )
        .with_stdin(StdinBody::Secret(secret(TOKEN)));
        let shown = format!("{login:?}");
        assert!(!shown.contains(TOKEN), "{shown}");
    }

    #[test]
    fn token_never_in_logged_argv() {
        let runner =
            GatedRunner::new(FakeGlab::new().with_parent_env(&[("GITLAB_TOKEN", "env-token")]));
        runner
            .inner()
            .push(FakeReply::http(200, r#"{"scopes":["read_api"]}"#));
        runner
            .inner()
            .push(FakeReply::stdout(0, "", "✓ Logged in as tim"));
        let verify = GlabRequest::api("gitlab.example.com", ["personal_access_tokens/self"])
            .without_env(&TOKEN_ENV[1..])
            .with_env("GITLAB_TOKEN", EnvValue::Secret(secret(TOKEN)));
        runner.run(verify).unwrap();
        let login = GlabRequest::write(
            Some("gitlab.example.com"),
            [
                "auth",
                "login",
                "--hostname",
                "gitlab.example.com",
                "--stdin",
            ],
        )
        .without_env(&TOKEN_ENV)
        .without_env(&CI_ENV)
        .with_stdin(StdinBody::Secret(secret(TOKEN)));
        runner.run(login).unwrap();

        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 2);
        for call in &calls {
            assert!(
                call.request.args.iter().all(|a| !a.contains(TOKEN)),
                "Token in argv: {:?}",
                call.request.args
            );
            assert!(call.env.values().all(|v| !v.contains(TOKEN)));
            let shown = format!("{call:?}");
            assert!(!shown.contains(TOKEN), "{shown}");
        }
        // Prüfung: Token nur als Secret in env_set, nicht in env_remove
        let verify = &calls[0];
        assert_eq!(
            verify.env.get("GITLAB_TOKEN").map(String::as_str),
            Some("***")
        );
        assert!(!verify
            .request
            .env_remove
            .iter()
            .any(|k| k == "GITLAB_TOKEN"));
        assert!(matches!(
            verify.request.env_set.as_slice(),
            [(k, EnvValue::Secret(s))] if k == "GITLAB_TOKEN" && s.expose() == TOKEN
        ));
        // Login: Env-Token des Prozesses entfernt, Token nur auf stdin
        let login = &calls[1];
        assert!(!login.env.contains_key("GITLAB_TOKEN"));
        assert!(matches!(
            &login.request.stdin,
            Some(StdinBody::Secret(s)) if s.expose() == TOKEN
        ));
        assert_eq!(login.request.kind, CallKind::Write);
    }

    #[test]
    fn api_request_selects_host() {
        let req = GlabRequest::api("gitlab.example.com", ["graphql", "--input", "-"]);
        assert_eq!(
            req.args,
            [
                "api",
                "--hostname",
                "gitlab.example.com",
                "-i",
                "graphql",
                "--input",
                "-"
            ]
        );
        assert!(req.env_set.is_empty());
        assert_eq!(req.host_key.as_deref(), Some("gitlab.example.com"));
        assert_eq!(req.kind, CallKind::Read);
        assert_eq!(req.timeout, DEFAULT_TIMEOUT);
        assert!(req.includes_head());
        // mit Port: kein `--hostname`, sondern `GITLAB_HOST` (E16)
        let req = GlabRequest::api("git.firma.de:8443", ["user"]);
        assert_eq!(req.args, ["api", "-i", "user"]);
        assert!(matches!(
            req.env_set.as_slice(),
            [(k, EnvValue::Plain(v))] if k == "GITLAB_HOST" && v == "git.firma.de:8443"
        ));
        assert!(!GlabRequest::version().includes_head());
    }

    #[test]
    fn check_request_rules() {
        let ok = |req: GlabRequest| check_request(&req).is_ok();
        let h = Some("gitlab.example.com");
        assert!(ok(GlabRequest::version()));
        assert!(ok(GlabRequest::api("gitlab.example.com", ["user"])));
        assert!(ok(GlabRequest::api("git.firma.de:8443", ["user"])));
        assert!(ok(GlabRequest::read(
            h,
            ["config", "get", "--host", "gitlab.example.com", "user"]
        )));
        assert!(ok(GlabRequest::read(
            h,
            ["auth", "status", "--hostname=gitlab.example.com"]
        )));
        assert!(ok(GlabRequest::api(
            "gitlab.example.com",
            [
                "graphql",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "--input",
                "-"
            ]
        )));
        // Token in argv
        assert!(!ok(GlabRequest::write(
            h,
            [
                "auth",
                "login",
                "--hostname",
                "gitlab.example.com",
                "--token",
                "x"
            ]
        )));
        assert!(!ok(GlabRequest::write(
            h,
            [
                "auth",
                "login",
                "--hostname",
                "gitlab.example.com",
                "-t",
                "x"
            ]
        )));
        // `auth status` gibt das Token aus
        for flag in ["--show-token", "-t"] {
            assert!(!ok(GlabRequest::read(
                h,
                ["auth", "status", "--hostname", "gitlab.example.com", flag]
            )));
        }
        // Geheime Config-Schlüssel, lesen wie schreiben
        for key in ["token", "job_token", "oauth2_refresh_token", "Token"] {
            assert!(!ok(GlabRequest::read(
                h,
                ["config", "get", "--host", "gitlab.example.com", key]
            )));
            assert!(!ok(GlabRequest::write(
                h,
                ["config", "set", "--host", "gitlab.example.com", key, "x"]
            )));
        }
        // Token in der Kind-Env nur als Secret
        for key in ["GITLAB_TOKEN", "gitlab_access_token", "OAUTH_TOKEN"] {
            assert!(!ok(GlabRequest::api("gitlab.example.com", ["user"])
                .with_env(key, EnvValue::Plain("x".into()))));
        }
        // Schreibende Aufrufe nur als `CallKind::Write` (Gate exklusiv)
        let login = [
            "auth",
            "login",
            "--hostname",
            "gitlab.example.com",
            "--stdin",
        ];
        let logout = ["auth", "logout", "--hostname", "gitlab.example.com"];
        let set = [
            "config",
            "set",
            "--host",
            "gitlab.example.com",
            "api_protocol",
            "http",
        ];
        assert!(!ok(GlabRequest::read(h, login)));
        assert!(!ok(GlabRequest::read(h, logout)));
        assert!(!ok(GlabRequest::read(h, set)));
        assert!(ok(GlabRequest::write(h, login)));
        assert!(ok(GlabRequest::write(h, set)));
        // Host fehlt bzw. passt nicht zum Gate-Schlüssel
        let mut no_host = GlabRequest::api("gitlab.example.com", ["user"]);
        no_host.host_key = None;
        assert!(!ok(no_host));
        assert!(!ok(GlabRequest::read(
            None,
            ["auth", "status", "--hostname", "gitlab.example.com"]
        )));
        assert!(!ok(GlabRequest::read(h, ["auth", "status"])));
        assert!(!ok(GlabRequest::read(
            h,
            ["auth", "status", "--hostname", "anderer.example.com"]
        )));
        // `api` ohne `-i`
        assert!(!ok(GlabRequest::read(
            h,
            ["api", "--hostname", "gitlab.example.com", "user"]
        )));
        // `--input` ohne JSON-Content-Type (E18)
        assert!(!ok(GlabRequest::api(
            "gitlab.example.com",
            ["graphql", "-X", "POST", "--input", "-"]
        )));
        assert!(!ok(GlabRequest::read(None, Vec::<String>::new())));
    }

    #[test]
    fn child_env_remove_then_set() {
        let fake = FakeGlab::new().with_parent_env(&[
            ("GITLAB_TOKEN", "env-token"),
            ("GITLAB_ACCESS_TOKEN", "x"),
            ("OAUTH_TOKEN", "y"),
            ("GLAB_DEBUG_HTTP", "1"),
            ("DEBUG", "1"),
            ("GLAB_NO_PROMPT", "0"),
            ("HOME", "/home/tim"),
        ]);
        let runner = GatedRunner::new(fake);
        runner.inner().push(FakeReply::http(200, "{}"));
        runner.inner().push(FakeReply::stdout(0, "tim", ""));
        let verify = GlabRequest::api("gitlab.example.com", ["user"])
            .without_env(&TOKEN_ENV[1..])
            .with_env("GITLAB_TOKEN", EnvValue::Secret(secret(TOKEN)));
        runner.run(verify).unwrap();
        let discover = GlabRequest::read(
            Some("gitlab.example.com"),
            ["config", "get", "--host", "gitlab.example.com", "user"],
        )
        .without_env(&TOKEN_ENV);
        runner.run(discover).unwrap();

        let calls = runner.inner().take_calls();
        let env = &calls[0].env;
        assert_eq!(env.get("GITLAB_TOKEN").map(String::as_str), Some("***"));
        for gone in [
            "GITLAB_ACCESS_TOKEN",
            "OAUTH_TOKEN",
            "GLAB_DEBUG_HTTP",
            "DEBUG",
        ] {
            assert!(!env.contains_key(gone), "{gone} muss fehlen");
        }
        for (key, value) in ENV_SET {
            assert_eq!(env.get(key).map(String::as_str), Some(value), "{key}");
        }
        assert_eq!(env.get("HOME").map(String::as_str), Some("/home/tim"));
        // lokale Erkennung: alle Env-Tokens entfernt
        let env = &calls[1].env;
        for key in TOKEN_ENV {
            assert!(!env.contains_key(key), "{key} muss fehlen");
        }
    }

    #[test]
    fn inherited_api_host_and_protocol_removed() {
        // Global gesetzt gälten sie für jeden Host (Token an fremden
        // API-Host bzw. über http); nur der Request selbst darf sie setzen.
        let fake = FakeGlab::new().with_parent_env(&[
            ("GITLAB_API_HOST", "api.fremd.example"),
            ("GLAB_API_PROTOCOL", "http"),
        ]);
        let runner = GatedRunner::new(fake);
        runner.inner().push(FakeReply::http(200, "{}"));
        runner.inner().push(FakeReply::http(200, "{}"));
        runner
            .run(GlabRequest::api("gitlab.example.com", ["user"]))
            .unwrap();
        let check = GlabRequest::api("gitlab.example.com", ["user"])
            .with_env("GLAB_API_PROTOCOL", EnvValue::Plain("http".into()));
        runner.run(check).unwrap();

        let calls = runner.inner().take_calls();
        let env = &calls[0].env;
        assert!(!env.contains_key("GITLAB_API_HOST"));
        assert!(!env.contains_key("GLAB_API_PROTOCOL"));
        let env = &calls[1].env;
        assert!(!env.contains_key("GITLAB_API_HOST"));
        assert_eq!(
            env.get("GLAB_API_PROTOCOL").map(String::as_str),
            Some("http")
        );
    }

    #[test]
    fn fake_rule_violation_hides_argv() {
        let fake = FakeGlab::new();
        let req = GlabRequest::write(
            Some("gitlab.example.com"),
            [
                "auth",
                "login",
                "--hostname",
                "gitlab.example.com",
                "--token",
                TOKEN,
            ],
        );
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fake.run(req)))
            .expect_err("Regelverstoß muss panicen");
        let msg = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_default();
        assert!(msg.contains("Regelverstoß"), "{msg}");
        assert!(msg.contains("[\"auth\", \"login\"]"), "{msg}");
        assert!(!msg.contains(TOKEN), "{msg}");
    }

    #[test]
    #[should_panic(expected = "env_remove")]
    fn env_key_in_both_lists_is_a_bug() {
        let req = GlabRequest::api("gitlab.example.com", ["user"])
            .without_env(&["GITLAB_TOKEN"])
            .with_env("GITLAB_TOKEN", EnvValue::Secret(secret(TOKEN)));
        let _ = env_plan(&req);
    }

    #[test]
    #[should_panic(expected = "env_remove")]
    fn setting_debug_http_is_a_bug() {
        let req = GlabRequest::version().with_env("GLAB_DEBUG_HTTP", EnvValue::Plain("1".into()));
        let _ = env_plan(&req);
    }

    // --- Step 3: GatedRunner --------------------------------------------

    fn is_host(host: &'static str) -> impl Fn(&GlabRequest) -> bool + Send {
        move |r: &GlabRequest| r.host_key.as_deref() == Some(host)
    }

    #[test]
    fn gated_reads_for_same_host_run_one_after_another() {
        let runner = Arc::new(gated());
        for _ in 0..3 {
            runner
                .inner()
                .push(FakeReply::http(200, "{}").delay(Duration::from_millis(40)));
        }
        let handles: Vec<_> = (0..3)
            .map(|_| {
                let runner = Arc::clone(&runner);
                thread::spawn(move || runner.run(GlabRequest::api("gitlab.example.com", ["user"])))
            })
            .collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }
        assert_eq!(runner.inner().max_parallel("gitlab.example.com"), 1);
        assert_eq!(runner.inner().take_calls().len(), 3);
    }

    #[test]
    fn gated_reads_for_different_hosts_run_in_parallel() {
        let runner = Arc::new(gated());
        for host in ["a.example.com", "b.example.com"] {
            runner
                .inner()
                .push_when(is_host(host), FakeReply::http(200, "{}").rendezvous(2));
        }
        let handles: Vec<_> = ["a.example.com", "b.example.com"]
            .into_iter()
            .map(|host| {
                let runner = Arc::clone(&runner);
                thread::spawn(move || runner.run(GlabRequest::api(host, ["user"])))
            })
            .collect();
        for h in handles {
            h.join().unwrap().unwrap();
        }
        assert_eq!(runner.inner().max_total(), 2);
        assert_eq!(runner.inner().max_parallel("a.example.com"), 1);
    }

    #[test]
    fn gated_write_runs_exclusively() {
        let runner = Arc::new(gated());
        let fake = runner.inner();
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(0, "", "").delay(Duration::from_millis(30)),
        );
        fake.push_when(
            is_host("a.example.com"),
            FakeReply::http(200, "{}").delay(Duration::from_millis(60)),
        );
        fake.push_when(
            is_host("b.example.com"),
            FakeReply::http(200, "{}").delay(Duration::from_millis(30)),
        );
        let start = |delay_ms: u64, req: fn() -> GlabRequest| {
            let runner = Arc::clone(&runner);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(delay_ms));
                runner.run(req())
            })
        };
        let handles = [
            start(0, || GlabRequest::api("a.example.com", ["user"])),
            start(10, || {
                GlabRequest::write(
                    Some("w.example.com"),
                    ["auth", "login", "--hostname", "w.example.com", "--stdin"],
                )
                .with_stdin(StdinBody::Secret(SecretString::new(TOKEN.to_string())))
            }),
            start(20, || GlabRequest::api("b.example.com", ["user"])),
        ];
        for h in handles {
            h.join().unwrap().unwrap();
        }
        assert!(!runner.inner().write_overlapped());
        assert_eq!(runner.inner().take_calls().len(), 3);
        assert_eq!(runner.inner().pending(), 0);
    }

    #[test]
    fn fake_measures_overlap_without_gate() {
        // Gegenprobe: ohne Gate sieht der Fake die Überlappung.
        let fake = Arc::new(FakeGlab::new());
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(0, "", "").rendezvous(2),
        );
        fake.push_when(
            |r| r.kind == CallKind::Read,
            FakeReply::http(200, "{}").rendezvous(2),
        );
        let write = {
            let fake = Arc::clone(&fake);
            thread::spawn(move || {
                fake.run(GlabRequest::write(
                    Some("gitlab.example.com"),
                    [
                        "config",
                        "set",
                        "-g",
                        "--host",
                        "gitlab.example.com",
                        "ca_cert",
                        "c.pem",
                    ],
                ))
            })
        };
        let read = {
            let fake = Arc::clone(&fake);
            thread::spawn(move || fake.run(GlabRequest::api("gitlab.example.com", ["user"])))
        };
        write.join().unwrap().unwrap();
        read.join().unwrap().unwrap();
        assert!(fake.write_overlapped());
        assert_eq!(fake.max_parallel("gitlab.example.com"), 2);
    }

    /// Zwei Runner mit demselben Gate (neues glab-Programm nach
    /// „Speichern“, laufender Worker mit dem alten): `auth login` des einen
    /// läuft exklusiv gegenüber Lesern des anderen, Leser je Host einzeln.
    #[test]
    fn runners_sharing_a_gate_exclude_each_other() {
        let fake = Arc::new(FakeGlab::new());
        let gate = Arc::new(GlabGate::default());
        let old = Arc::new(GatedRunner::with_gate(Arc::clone(&fake), Arc::clone(&gate)));
        let new = Arc::new(GatedRunner::with_gate(Arc::clone(&fake), gate));
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(0, "", "").delay(Duration::from_millis(40)),
        );
        for _ in 0..2 {
            fake.push_when(
                is_host("a.example.com"),
                FakeReply::http(200, "{}").delay(Duration::from_millis(20)),
            );
        }
        let write = {
            let old = Arc::clone(&old);
            thread::spawn(move || {
                old.run(
                    GlabRequest::write(
                        Some("a.example.com"),
                        ["auth", "login", "--hostname", "a.example.com", "--stdin"],
                    )
                    .with_stdin(StdinBody::Secret(SecretString::new(TOKEN.to_string()))),
                )
            })
        };
        let reads: Vec<_> = [Arc::clone(&old), new]
            .into_iter()
            .map(|runner| {
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(10));
                    runner.run(GlabRequest::api("a.example.com", ["user"]))
                })
            })
            .collect();
        write.join().unwrap().unwrap();
        for read in reads {
            read.join().unwrap().unwrap();
        }
        assert!(!fake.write_overlapped());
        assert_eq!(fake.max_parallel("a.example.com"), 1);
        assert_eq!(fake.take_calls().len(), 3);
    }

    // --- Step 3: SystemGlab gegen ein Stub-Skript ------------------------

    #[cfg(unix)]
    mod system {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::path::{Path, PathBuf};
        use std::time::Instant;

        const STUB: &str = r#"#!/bin/sh
printf 'ARGV:%s\n' "$*" >&2
case "$STUB_MODE" in
  noop) ;;
  big)
    head -c 1048576 /dev/zero | tr '\0' e >&2
    head -c 1048576 /dev/zero | tr '\0' o
    ;;
  cat) cat ;;
  sleep) echo $$ > "$STUB_PIDFILE"; exec sleep 60 ;;
  env)
    printf 'token_len=%s\n' "${#GITLAB_TOKEN}"
    printf 'removed=%s\n' "$(printenv "$STUB_VAR" || echo unset)"
    printf 'fixed=%s,%s,%s,%s,%s,%s,%s\n' "$GLAB_NO_PROMPT" "$GLAB_CHECK_UPDATE" "$GLAB_SEND_TELEMETRY" "$GLAB_SHOW_WHATS_NEW" "$GLAB_NOTIFY_SKILL_UPDATES" "$NO_COLOR" "${GLAB_DEBUG_HTTP-unset}"
    printf 'cwd=%s\n' "$(pwd -P)"
    ;;
  early) echo "glab: kaputt" >&2; exit 3 ;;
  huge) head -c 16777217 /dev/zero ;;
  http)
    printf 'HTTP/2.0 401 Unauthorized\r\nContent-Type: application/json\r\n\r\n{"errors":[{"message":"Invalid token"}]}'
    exit 1
    ;;
esac
"#;

        /// Stub-Skript im tempdir. Der Probelauf wartet ein `ETXTBSY` ab
        /// (Datei noch in einem parallel geforkten Kind offen).
        fn stub(dir: &Path) -> SystemGlab {
            let path = dir.join("glab-stub.sh");
            std::fs::write(&path, STUB).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            for _ in 0..200 {
                match Command::new(&path).env("STUB_MODE", "noop").output() {
                    Err(e) if e.raw_os_error() == Some(26) => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    other => {
                        other.unwrap();
                        break;
                    }
                }
            }
            SystemGlab::new(path.to_string_lossy())
        }

        fn mode(mode: &str) -> GlabRequest {
            GlabRequest::read(None, ["stub"]).with_env("STUB_MODE", EnvValue::Plain(mode.into()))
        }

        #[test]
        fn large_stdout_and_stderr_without_deadlock() {
            let dir = tempfile::tempdir().unwrap();
            let out = stub(dir.path()).run(mode("big")).unwrap();
            assert_eq!(out.exit, 0);
            assert_eq!(out.body.len(), 1024 * 1024);
            assert!(out.body.bytes().all(|b| b == b'o'));
            assert!(out.stderr_tail.len() <= 8 * 1024);
            assert!(out.stderr_tail.ends_with('e'));
        }

        #[test]
        fn stdin_until_eof_and_token_not_in_argv() {
            let dir = tempfile::tempdir().unwrap();
            let glab = stub(dir.path());
            let req = mode("cat").with_stdin(StdinBody::Secret(secret(TOKEN)));
            let out = glab.run(req).unwrap();
            assert_eq!(out.exit, 0);
            assert_eq!(out.body, TOKEN);
            assert!(out.stderr_tail.contains("ARGV:stub"));
            assert!(!out.stderr_tail.contains(TOKEN));
            // JSON-Body größer als ein Pipe-Puffer
            let json = format!("{{\"query\":\"{}\"}}", "q".repeat(200_000));
            let out = glab
                .run(mode("cat").with_stdin(StdinBody::Json(json.clone())))
                .unwrap();
            assert_eq!(out.body, json);
        }

        #[test]
        fn timeout_kills_and_reaps() {
            let dir = tempfile::tempdir().unwrap();
            let pidfile = dir.path().join("pid");
            let req = mode("sleep")
                .with_env(
                    "STUB_PIDFILE",
                    EnvValue::Plain(pidfile.to_string_lossy().into()),
                )
                .with_timeout(Duration::from_secs(2));
            let started = Instant::now();
            let err = stub(dir.path()).run(req).unwrap_err();
            assert!(matches!(err, GlabError::Timeout(_)), "{err:?}");
            assert_eq!(err.kind(), GlabErrorKind::Timeout);
            assert!(started.elapsed() < Duration::from_secs(10));
            // Unter Last kann der Kill vor dem Schreiben der PID-Datei
            // kommen; dann bleibt nur die Prüfung auf `Timeout`.
            let pid = std::fs::read_to_string(&pidfile).unwrap_or_default();
            let pid = pid.trim();
            if !pid.is_empty() {
                // geerntet: kein Zombie-Eintrag mehr
                let proc_dir = PathBuf::from(format!("/proc/{pid}"));
                assert!(!proc_dir.exists(), "Prozess {pid} lebt noch");
            }
        }

        #[test]
        fn env_rules_and_neutral_cwd() {
            let dir = tempfile::tempdir().unwrap();
            // eine vorhandene Variable des Testprozesses, die entfernt wird
            // (keine der festen Regeln, sonst greift die Doppelprüfung)
            let fixed = |k: &str| {
                ENV_SET.iter().any(|(s, _)| *s == k)
                    || ENV_REMOVE.contains(&k)
                    || TOKEN_ENV.contains(&k)
            };
            let (var, _) = std::env::vars()
                .find(|(k, _)| {
                    k != "PATH"
                        && !k.starts_with("STUB_")
                        && !fixed(k)
                        && k.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
                })
                .expect("Testprozess ohne Umgebungsvariablen");
            let req = mode("env")
                .with_env("STUB_VAR", EnvValue::Plain(var.clone()))
                .without_env(&[var.as_str()])
                .without_env(&TOKEN_ENV[1..])
                .with_env("GITLAB_TOKEN", EnvValue::Secret(secret(TOKEN)));
            let out = stub(dir.path()).run(req).unwrap();
            let lines: Vec<&str> = out.body.lines().collect();
            assert_eq!(lines[0], format!("token_len={}", TOKEN.len()));
            assert_eq!(lines[1], "removed=unset", "{var} muss entfernt sein");
            assert_eq!(lines[2], "fixed=1,false,0,false,false,1,unset");
            let tmp = std::env::temp_dir().canonicalize().unwrap();
            assert_eq!(lines[3], format!("cwd={}", tmp.display()));
            assert!(!out.body.contains(TOKEN));
        }

        #[test]
        fn early_exit_ignores_broken_pipe() {
            let dir = tempfile::tempdir().unwrap();
            let big = "x".repeat(1024 * 1024);
            let out = stub(dir.path())
                .run(mode("early").with_stdin(StdinBody::Json(big)))
                .unwrap();
            assert_eq!(out.exit, 3);
            assert!(out.stderr_tail.contains("glab: kaputt"));
            assert_eq!(classify(&out), Some(GlabErrorKind::Other));
        }

        #[test]
        fn output_limited_to_16_mb() {
            let dir = tempfile::tempdir().unwrap();
            let err = stub(dir.path()).run(mode("huge")).unwrap_err();
            assert!(matches!(err, GlabError::OutputTooLarge), "{err:?}");
        }

        #[test]
        fn api_head_parsed_and_classified() {
            let dir = tempfile::tempdir().unwrap();
            let req = GlabRequest::api("stub.example.com", ["graphql"])
                .with_env("STUB_MODE", EnvValue::Plain("http".into()));
            let out = stub(dir.path()).run(req).unwrap();
            assert_eq!(out.exit, 1);
            assert_eq!(out.http.as_ref().map(|h| h.code), Some(401));
            assert_eq!(out.body, r#"{"errors":[{"message":"Invalid token"}]}"#);
            assert_eq!(classify(&out), Some(GlabErrorKind::InvalidToken));
            assert!(out
                .stderr_tail
                .contains("ARGV:api --hostname stub.example.com -i graphql"));
        }

        #[test]
        fn missing_program_is_not_found() {
            let dir = tempfile::tempdir().unwrap();
            let runner = SystemGlab::new(dir.path().join("fehlt").to_string_lossy());
            let err = runner.run(GlabRequest::version()).unwrap_err();
            assert!(matches!(err, GlabError::NotFound), "{err:?}");
            assert_eq!(glab_version(&runner), Err(GlabErrorKind::NotInstalled));
        }

        #[test]
        fn rejected_request_starts_nothing() {
            let dir = tempfile::tempdir().unwrap();
            let req = GlabRequest::read(None, ["api", "user"]);
            let err = stub(dir.path()).run(req).unwrap_err();
            assert!(matches!(err, GlabError::Rejected(_)), "{err:?}");
        }
    }

    #[test]
    #[should_panic(expected = "glab")]
    fn system_glab_refuses_real_glab_in_tests() {
        let _ = SystemGlab::new("glab").run(GlabRequest::version());
    }

    #[test]
    fn registry_path_helpers() {
        assert_eq!(
            join_path_lists(Some(r"C:\Windows;C:\Tools;"), Some(r"%LOCALAPPDATA%\glab")),
            Some(r"C:\Windows;C:\Tools;%LOCALAPPDATA%\glab".to_string())
        );
        assert_eq!(join_path_lists(None, Some("a")), Some("a".to_string()));
        assert_eq!(join_path_lists(None, Some("")), None);
        let lookup = |name: &str| {
            (name == "LOCALAPPDATA").then(|| r"C:\Users\tim\AppData\Local".to_string())
        };
        assert_eq!(
            expand_env_vars(r"%LOCALAPPDATA%\glab;%UNBEKANNT%\x;50%;a%%b", lookup),
            r"C:\Users\tim\AppData\Local\glab;%UNBEKANNT%\x;50%;a%%b"
        );
    }
}
