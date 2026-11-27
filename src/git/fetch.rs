//! Fetch und Fast-Forward: libgit2 mit Zugangsdaten und Wiederholungen,
//! Rückfall auf System-`git`.

use super::{dirty_count, get_branch, RepoInfo};
use crate::process;
use crate::process::{set_creation_flags, Keep, Limits, CREATE_NO_WINDOW};
use git2::{BranchType, Repository};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Ergebnis von [`fast_forward`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FfOutcome {
    UpToDate,
    FastForwarded {
        commits: usize,
    },
    Dirty {
        files: usize,
    },
    Diverged {
        ahead: usize,
        behind: usize,
    },
    NoUpstream,
    Detached,
    /// HEAD oder Branch hat sich während des Abrufs geändert – nichts getan
    Changed,
}

/// Zieht den aktuellen Branch per Fast-Forward auf seinen Upstream. Setzt nie
/// einen Branch zurück: bei lokalen Commits (`Diverged`) oder uncommitteten
/// Änderungen (`Dirty`) passiert nichts. Fetch-Fehler werden als `Err`
/// durchgereicht.
pub fn fast_forward(path: &Path) -> anyhow::Result<FfOutcome> {
    fast_forward_inner(path, || {})
}

/// Upstream eines Branches als (Remote, Branch dort) aus
/// `branch.<name>.remote` und `.merge`. `None` ohne Upstream, bei lokalem
/// Upstream (`.`, kein Remote-Tracking-Ref) und bei einem Merge-Ref
/// außerhalb von `refs/heads/`.
pub(super) fn branch_upstream(
    repo: &Repository,
    branch: &str,
) -> anyhow::Result<Option<(String, String)>> {
    let cfg = repo.config()?;
    let remote = cfg.get_string(&format!("branch.{branch}.remote")).ok();
    let merge = cfg.get_string(&format!("branch.{branch}.merge")).ok();
    let (Some(remote), Some(merge)) = (remote, merge) else {
        return Ok(None);
    };
    if remote == "." {
        return Ok(None);
    }
    Ok(merge
        .strip_prefix("refs/heads/")
        .map(|name| (remote, name.to_string())))
}

/// Wie [`fast_forward`]; `after_fetch` läuft direkt nach dem Fetch (Tests
/// simulieren damit Änderungen während des Abrufs).
pub(super) fn fast_forward_inner(
    path: &Path,
    after_fetch: impl FnOnce(),
) -> anyhow::Result<FfOutcome> {
    let repo =
        Repository::open(path).map_err(|e| anyhow::anyhow!("Failed to open repository: {e}"))?;
    let (branch, detached) = get_branch(&repo);
    if detached {
        return Ok(FfOutcome::Detached);
    }
    let Some((remote, merge_branch)) = branch_upstream(&repo, &branch)? else {
        return Ok(FfOutcome::NoUpstream);
    };
    let refname = format!("refs/heads/{branch}");
    // Stand vor dem Fetch merken: danach wird nur umgesetzt, wenn HEAD und
    // Branch noch genau so stehen (Compare-and-Swap).
    let Ok(local_oid) = repo.refname_to_id(&refname) else {
        return Ok(FfOutcome::NoUpstream);
    };
    drop(repo);
    let spec = format!("+refs/heads/{merge_branch}:refs/remotes/{remote}/{merge_branch}");
    // Als Fetch-Fehler kennzeichnen (UI: Fetch-Text samt Auth-Hinweis)
    fetch_refspecs(path, &remote, &[&spec]).map_err(|e| {
        anyhow::Error::new(PartialFetch {
            succeeded: Vec::new(),
            failed: vec![(remote.clone(), format!("{e:#}"))],
        })
    })?;
    after_fetch();

    // Nach dem Fetch neu öffnen, damit der Handle die neuen Refs sicher sieht.
    let repo = Repository::open(path)?;
    // HEAD muss noch auf denselben Branch zeigen, der Branch auf denselben Commit
    let head_same = repo
        .find_reference("HEAD")
        .ok()
        .is_some_and(|h| h.symbolic_target().ok().flatten() == Some(refname.as_str()));
    if !head_same || repo.refname_to_id(&refname).ok() != Some(local_oid) {
        return Ok(FfOutcome::Changed);
    }
    let local = repo.find_branch(&branch, BranchType::Local)?;
    let Some(up_oid) = local.upstream().ok().and_then(|u| u.get().target()) else {
        return Ok(FfOutcome::NoUpstream);
    };
    let (ahead, behind) = repo.graph_ahead_behind(local_oid, up_oid)?;
    if behind == 0 {
        return Ok(FfOutcome::UpToDate);
    }
    if ahead > 0 {
        return Ok(FfOutcome::Diverged { ahead, behind });
    }
    let files = dirty_count(&repo);
    if files > 0 {
        return Ok(FfOutcome::Dirty { files });
    }
    let target = repo.find_commit(up_oid)?;
    // Erst Arbeitsverzeichnis (safe: überschreibt nichts Lokales), dann Ref.
    repo.checkout_tree(
        target.as_object(),
        Some(git2::build::CheckoutBuilder::new().safe()),
    )?;
    // Ref nur umsetzen, wenn er noch auf `local_oid` steht.
    if let Err(e) = repo.reference_matching(
        &refname,
        up_oid,
        true,
        local_oid,
        "gitmanager: fast-forward",
    ) {
        // Arbeitsverzeichnis bestmöglich auf den alten Stand zurückholen
        if let Ok(old) = repo.find_commit(local_oid) {
            let _ = repo.checkout_tree(
                old.as_object(),
                Some(git2::build::CheckoutBuilder::new().safe()),
            );
        }
        return Err(anyhow::anyhow!("Branch-Ref nicht aktualisiert: {e}"));
    }
    Ok(FfOutcome::FastForwarded { commits: behind })
}

/// Holt alle Remotes (git fetch --all) – für Refresh-Button.
///
/// Ehrliche Semantik: `Ok` nur wenn alle Remotes erfolgreich waren (oder keine
/// Remotes konfiguriert sind). Jeder Remote-Fehler wird pro Remote gesammelt und
/// als strukturiertes [`PartialFetch`] (`succeeded` + `failed`, auch bei komplettem
/// Fehlschlag mit leerem `succeeded`) zurückgegeben, siehe
/// [`is_partial_fetch_error`] / [`is_fetch_error`], damit die UI `fetch_partial`
/// statt `fetch_failed` wählen und Wrapper/Hinweis via i18n lokalisieren kann.
/// `eprintln!` bleibt als Log erhalten.
///
/// Auth: libgit2 mit Credential-Callbacks (SSH-Agent, Default-Keys,
/// git-credential-helper) zuerst; bei Fehlschlag Fallback auf System-`git fetch`
/// (nutzt Credential-Manager/SSH-Agent des Users — nötig für authentifizierte
/// Repos). Branch-Wechsel ist lokal und braucht kein Netzwerk, Fetch schon.
/// Signatur bleibt `fetch_all(path: &Path) -> anyhow::Result<()>` (Task 5 baut darauf).
pub fn fetch_all(path: &Path) -> anyhow::Result<()> {
    let repo =
        Repository::open(path).map_err(|e| anyhow::anyhow!("Failed to open repository: {e}"))?;
    let remotes = repo
        .remotes()
        .map_err(|e| anyhow::anyhow!("Failed to read remotes: {e}"))?;
    if remotes.is_empty() {
        return Ok(());
    }
    let mut succeeded: Vec<String> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    for remote_res in remotes.iter() {
        let remote_name = match remote_res {
            Ok(Some(name)) => name.to_string(),
            Ok(None) => continue,
            Err(e) => {
                eprintln!("Remote name invalid: {e}");
                failed.push(("<invalid>".to_string(), e.to_string()));
                continue;
            }
        };
        match fetch_one_remote(&repo, path, &remote_name) {
            Ok(()) => succeeded.push(remote_name),
            Err(e) => {
                eprintln!("Fetch für Remote '{remote_name}' fehlgeschlagen: {e}");
                failed.push((remote_name, e.to_string()));
            }
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    // I1: alle Remote-Fehler (komplett + partiell) als strukturiertes PartialFetch.
    // UI baut Wrapper/Hinweis lokalisiert via tr_fmt(fetch_failed/fetch_partial) +
    // tr(auth_hint); Detail bleibt English-neutral mit opakem libgit2-Text.
    Err(anyhow::Error::new(PartialFetch { succeeded, failed }))
}

/// Pfade für einen Fetch-Durchlauf über die gescannten Repos: Worktrees
/// eines Repositorys teilen Remotes und Remote-Refs, ein Fetch je
/// Repository reicht; gleichzeitige Fetches in dasselbe `.git` stören sich
/// zudem an den Ref-Locks. Gruppiert nach `common_dir` aus dem Scan, ohne
/// die Repos erneut zu öffnen; bevorzugt den Haupt-Worktree, sonst den
/// ersten Pfad. Ein Pfad, der sich seit dem Scan nicht mehr öffnen lässt,
/// bleibt (sein Fehler soll beim Fetch erscheinen). Reihenfolge wie in
/// `repos`.
pub fn fetch_targets(repos: &[RepoInfo]) -> Vec<PathBuf> {
    let mut by_common: HashMap<&Path, usize> = HashMap::new();
    let mut out: Vec<&RepoInfo> = Vec::new();
    for repo in repos {
        match by_common.get(repo.common_dir.as_path()) {
            Some(&i) if out[i].is_linked_worktree && !repo.is_linked_worktree => out[i] = repo,
            Some(_) => {}
            None => {
                by_common.insert(&repo.common_dir, out.len());
                out.push(repo);
            }
        }
    }
    out.into_iter().map(|repo| repo.path.clone()).collect()
}

/// K1: holt nur die angegebenen Refspecs eines Remotes, z. B. den
/// Quell-Branch eines MR (`MrCheckoutPlan::refspec`). Ausdrücklich ohne
/// Prune (`FetchPrune::Off` übersteuert `fetch.prune` des Users) und ohne
/// Tags: andere Remote-Refs bleiben unberührt. Auth und CLI-Fallback wie
/// [`fetch_all`].
pub fn fetch_refspecs(path: &Path, remote: &str, refspecs: &[&str]) -> anyhow::Result<()> {
    let repo =
        Repository::open(path).map_err(|e| anyhow::anyhow!("Failed to open repository: {e}"))?;
    fetch_remote_with(&repo, path, remote, refspecs, false)
}

/// Ein Remote fetchen: libgit2 mit Auth-Callbacks, Fallback auf System-`git`.
fn fetch_one_remote(repo: &Repository, repo_path: &Path, remote_name: &str) -> anyhow::Result<()> {
    fetch_remote_with(repo, repo_path, remote_name, &[], true)
}

/// Fetch eines Remotes mit expliziten Refspecs (leer = konfigurierte) und
/// wählbarem Prune. Mit Refspecs ohne Tags (wie `--no-tags`). Kurze
/// Abbrüche wiederholt libgit2 (siehe [`with_fetch_retries`]), erst danach
/// greift System-`git`.
fn fetch_remote_with(
    repo: &Repository,
    repo_path: &Path,
    remote_name: &str,
    refspecs: &[&str],
    prune: bool,
) -> anyhow::Result<()> {
    let mut remote = match repo.find_remote(remote_name) {
        Ok(remote) => remote,
        Err(e) => {
            eprintln!("Remote '{remote_name}' nicht gefunden: {e}");
            return Err(anyhow::anyhow!("{e}"));
        }
    };
    let remote_url = remote.url().unwrap_or("").to_string();
    let lib_res = with_fetch_retries(remote_name, &fetch_retry_delays(), || {
        libgit2_fetch(repo, &mut remote, &remote_url, refspecs, prune)
    });
    match lib_res {
        Ok(()) => Ok(()),
        Err(lib_err) => {
            eprintln!(
                "libgit2 fetch für Remote '{remote_name}' fehlgeschlagen ({lib_err}), versuche System-git..."
            );
            // Fallback: System-git nutzt Credential-Manager/SSH-Agent des Users.
            match fetch_with_git_cli(repo_path, remote_name, refspecs, prune) {
                Ok(()) => {
                    eprintln!("System-git fetch für Remote '{remote_name}' erfolgreich (Fallback)");
                    Ok(())
                }
                Err(cli_err) => {
                    eprintln!(
                        "System-git fetch für Remote '{remote_name}' ebenfalls fehlgeschlagen: {cli_err:#}"
                    );
                    // Strukturierter Fehler bleibt libgit2-basiert (English-neutral,
                    // keine lokalisierten git-CLI-Texte) — CLI nur im Log.
                    Err(anyhow::anyhow!("{lib_err}"))
                }
            }
        }
    }
}

/// Ein Fetch-Versuch per libgit2 mit Auth-Callbacks.
pub(super) fn libgit2_fetch(
    repo: &Repository,
    remote: &mut git2::Remote,
    remote_url: &str,
    refspecs: &[&str],
    prune: bool,
) -> Result<(), git2::Error> {
    // Repo-Config für credential.helper (https) — Snapshot vor Callbacks.
    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(credentials_callback(
        repo.config().ok(),
        remote_url.to_string(),
    ));
    // Zertifikatsprüfung NICHT deaktivieren (TLS bleibt verifiziert).
    let mut opts = git2::FetchOptions::new();
    opts.prune(if prune {
        git2::FetchPrune::On
    } else {
        git2::FetchPrune::Off
    });
    if !refspecs.is_empty() {
        opts.download_tags(git2::AutotagOption::None);
    }
    opts.remote_callbacks(callbacks);
    remote.fetch(refspecs, Some(&mut opts), None)
}

/// Obergrenzen für Verbindungen von libgit2 (Fetch, Klon): ohne sie wartet
/// eine hängende Verbindung (VPN weg, halb offene TCP-Verbindung) für immer
/// und hält den Worker und damit den Ref-Lock (`git_busy`, K1) für die
/// ganze Sitzung, wie bei System-`git` (`GIT_CLI_FETCH_TIMEOUT`). Die
/// Stille gilt je Lese- oder Schreibaufruf, nicht für den ganzen Transfer:
/// Ein langer Klon, der Daten bekommt, läuft weiter (der Server meldet auch
/// beim Packen Fortschritt oder Keepalives).
///
/// Wirkt für SSH (libssh2, auch Handshake und Anmeldung), für http(s)
/// außer unter Windows und für git://. WinHTTP (https unter Windows)
/// nimmt feste Werte von libgit2: Verbindung 60 s, Empfang ohne Grenze.
pub(super) const LIBGIT2_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

const LIBGIT2_IDLE_TIMEOUT: Duration = Duration::from_secs(2 * 60);

/// Setzt [`LIBGIT2_CONNECT_TIMEOUT`] und [`LIBGIT2_IDLE_TIMEOUT`]
/// prozessweit. Nur beim Start aufrufen, bevor Threads laufen: libgit2
/// liest die Werte ohne Synchronisierung.
pub fn init_libgit2_timeouts() {
    set_libgit2_timeouts(LIBGIT2_CONNECT_TIMEOUT, LIBGIT2_IDLE_TIMEOUT);
}

/// Beide Werte gemeinsam: Mit Connect-Timeout allein lässt libgit2 den
/// Socket nicht blockierend, wartet beim Lesen aber nicht, und http bricht
/// sofort mit „Resource temporarily unavailable“ ab.
pub(super) fn set_libgit2_timeouts(connect: Duration, idle: Duration) {
    let ms = |d: Duration| i32::try_from(d.as_millis()).unwrap_or(i32::MAX);
    // Kann nicht fehlschlagen (siehe git2::opts); unsafe nur wegen der
    // fehlenden Synchronisierung
    unsafe {
        let _ = git2::opts::set_server_connect_timeout_in_milliseconds(ms(connect));
        let _ = git2::opts::set_server_timeout_in_milliseconds(ms(idle));
    }
}

/// Wartezeiten vor dem 2. und 3. Versuch, mit bis zu 1 s Streuung: Worker,
/// die gemeinsam abgebrochen sind, sollen nicht wieder gleichzeitig ankommen.
/// Tests warten nicht (ihre `invalid.example.com`-Remotes scheitern im Netz).
fn fetch_retry_delays() -> [Duration; 2] {
    if cfg!(test) {
        return [Duration::ZERO; 2];
    }
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_millis()));
    [
        Duration::from_millis(1_500 + jitter),
        Duration::from_millis(4_000 + jitter),
    ]
}

/// Wiederholt `fetch` nach jeder Wartezeit aus `delays`, solange der Fehler
/// vorübergehend ist ([`is_transient_fetch_error`]); andere Fehler und der
/// Fehler des letzten Versuchs gehen unverändert zurück.
pub(super) fn with_fetch_retries(
    remote_name: &str,
    delays: &[Duration],
    mut fetch: impl FnMut() -> Result<(), git2::Error>,
) -> Result<(), git2::Error> {
    let mut delays = delays.iter();
    loop {
        match fetch() {
            Err(e) if is_transient_fetch_error(&e) => {
                let Some(delay) = delays.next() else {
                    return Err(e);
                };
                eprintln!(
                    "libgit2 fetch für Remote '{remote_name}' abgebrochen ({e}), neuer Versuch in {delay:?}"
                );
                std::thread::sleep(*delay);
            }
            res => return res,
        }
    }
}

/// Abbruch, der beim nächsten Versuch meist weg ist: Die Verbindung steht
/// und reißt dann ab (Reset, Timeout, vorzeitiges Ende; bei SSH auch schon
/// im Handshake), ein Gateway meldet 502/503/504, oder ein gleichzeitiger
/// Fetch hält eine Lock-Datei.
///
/// Endgültig, also ohne weitere Versuche (die bei SSH nur weitere
/// Fehlversuche beim Server erzeugen): jedes abgelehnte Login (`code=Auth`,
/// auch „remote rejected authentication: Failed getting response“, wenn der
/// Server nach zu vielen Fehlversuchen trennt, und
/// [`CredState::exhausted`]) und ein Abbruch während der Anmeldung,
/// Host-Key und Zertifikat, DNS-Fehler, Connect-Timeout und abgewiesene
/// Verbindung (offline) sowie Antworten des Servers (401/403/404, „project
/// not found“). Geprüft werden nur die Texte von libgit2 selbst; was
/// libssh2 oder Windows dahinter anhängen, ist nicht stabil (lokalisiert).
pub(super) fn is_transient_fetch_error(e: &git2::Error) -> bool {
    use git2::{ErrorClass, ErrorCode};
    match e.code() {
        ErrorCode::Auth | ErrorCode::Certificate | ErrorCode::User => return false,
        ErrorCode::Timeout | ErrorCode::Locked => return true,
        _ => {}
    }
    let message = e.message();
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| message.starts_with(p));
    match e.class() {
        // Handshake oder Kanal nach dem Login; Login und Host-Key-Prüfung
        // liegen dazwischen und haben eigene Texte
        ErrorClass::Ssh => starts(&[
            "failed to start SSH session",
            "Failed to open SSH channel",
            "SSH could not",
        ]),
        // Socket nach dem Connect, Ende ohne Meldung des Servers
        ErrorClass::Net => {
            e.code() == ErrorCode::Eof || starts(&["error receiving data from socket"])
        }
        // WinHTTP nach dem Senden („failed to send request“: offline, DNS)
        ErrorClass::Os => starts(&[
            "failed to receive response",
            "failed to read data",
            "failed to write",
            "failed to lock",
        ]),
        // „unexpected http status code: 503“, WinHTTP „request failed with
        // status code: 503“
        ErrorClass::Http => {
            message == "unexpected EOF"
                || message
                    .rsplit_once(": ")
                    .and_then(|(_, status)| status.parse::<u16>().ok())
                    .is_some_and(|status| matches!(status, 502..=504))
        }
        _ => false,
    }
}

/// Anmeldeart, die der Credential-Callback libgit2 anbietet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CredChoice {
    SshAgent,
    /// Default-Key aus `~/.ssh` (ohne Passphrase)
    SshKeyFile(PathBuf),
    /// `credential.helper` (https, z. B. Windows Credential Manager)
    CredentialHelper,
    /// NTLM/Kerberos u. ä.
    Default,
    Username,
}

/// Gedächtnis des Credential-Callbacks für eine Verbindung. libgit2 fragt
/// nach jedem abgelehnten Login erneut; ohne Gedächtnis bekam es endlos
/// dasselbe (etwa den SSH-Agent, auch wenn keiner läuft), bis der Server
/// wegen zu vieler Fehlversuche trennte („remote rejected authentication:
/// Failed getting response“) und die Default-Keys nie an die Reihe kamen.
#[derive(Debug, Default)]
pub(super) struct CredState {
    offered: Vec<CredChoice>,
}

impl CredState {
    /// Nächste noch nicht angebotene Anmeldeart in der Reihenfolge
    /// SSH-Agent → Default-Keys → credential.helper → default; `None`, wenn
    /// alles probiert ist. Der Benutzername (libgit2 fragt ihn einmal, wenn
    /// die URL keinen nennt) zählt nicht als Versuch.
    pub(super) fn next(
        &mut self,
        allowed: git2::CredentialType,
        key_files: &[PathBuf],
    ) -> Option<CredChoice> {
        let mut candidates = Vec::new();
        if allowed.is_ssh_key() {
            candidates.push(CredChoice::SshAgent);
            candidates.extend(key_files.iter().cloned().map(CredChoice::SshKeyFile));
        }
        if allowed.is_user_pass_plaintext() {
            candidates.push(CredChoice::CredentialHelper);
        }
        if allowed.is_default() {
            candidates.push(CredChoice::Default);
        }
        if let Some(next) = candidates.into_iter().find(|c| !self.offered.contains(c)) {
            self.offered.push(next.clone());
            return Some(next);
        }
        allowed.is_username().then_some(CredChoice::Username)
    }

    /// Fehler, wenn alles probiert ist: Klasse `Callback`, damit
    /// [`with_fetch_retries`] nicht noch mehr Fehlversuche erzeugt.
    pub(super) fn exhausted(&self) -> git2::Error {
        let tried: Vec<String> = self
            .offered
            .iter()
            .map(|c| match c {
                CredChoice::SshAgent => "ssh-agent".to_string(),
                CredChoice::SshKeyFile(path) => path.display().to_string(),
                CredChoice::CredentialHelper => "credential helper".to_string(),
                CredChoice::Default => "default credentials".to_string(),
                CredChoice::Username => "username".to_string(),
            })
            .collect();
        let message = if tried.is_empty() {
            "no credentials available for this remote".to_string()
        } else {
            format!("authentication failed (tried: {})", tried.join(", "))
        };
        git2::Error::new(git2::ErrorCode::Auth, git2::ErrorClass::Callback, message)
    }
}

/// Credential-Callback für libgit2 (Fetch, Klon) mit eigenem
/// [`CredState`]; jede Anmeldeart kommt höchstens einmal.
///
/// `config` ist der Snapshot der Repo- bzw. globalen Config (für
/// `credential.helper` bei https), `remote_url` der konfigurierte Remote
/// (Fallback, wenn die Callback-URL leer ist).
pub(super) fn credentials_callback(
    config: Option<git2::Config>,
    remote_url: String,
) -> impl FnMut(&str, Option<&str>, git2::CredentialType) -> Result<git2::Cred, git2::Error> {
    let key_files: Vec<PathBuf> = default_ssh_key_paths()
        .into_iter()
        .filter(|p| p.exists())
        .collect();
    credentials_with_keys(config, remote_url, key_files)
}

/// [`credentials_callback`] mit fester Liste vorhandener Default-Keys.
pub(super) fn credentials_with_keys(
    config: Option<git2::Config>,
    remote_url: String,
    key_files: Vec<PathBuf>,
) -> impl FnMut(&str, Option<&str>, git2::CredentialType) -> Result<git2::Cred, git2::Error> {
    let mut state = CredState::default();
    move |callback_url, username_from_url, allowed| {
        let url = if callback_url.is_empty() {
            remote_url.as_str()
        } else {
            callback_url
        };
        let username = username_from_url.unwrap_or("git");
        while let Some(choice) = state.next(allowed, &key_files) {
            let cred = match &choice {
                CredChoice::SshAgent => git2::Cred::ssh_key_from_agent(username),
                CredChoice::SshKeyFile(key) => {
                    let pub_path = key.with_extension("pub");
                    let pub_opt = pub_path.exists().then_some(pub_path.as_path());
                    git2::Cred::ssh_key(username, pub_opt, key, None)
                }
                CredChoice::CredentialHelper => match &config {
                    Some(cfg) => git2::Cred::credential_helper(cfg, url, username_from_url),
                    None => continue,
                },
                CredChoice::Default => git2::Cred::default(),
                CredChoice::Username => git2::Cred::username(username),
            };
            if let Ok(cred) = cred {
                return Ok(cred);
            }
        }
        Err(state.exhausted())
    }
}

/// Kandidaten für Default-SSH-Keys (ohne Passphrase versucht).
fn default_ssh_key_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    if let Some(mut ssh_dir) = home.map(|h| h.join(".ssh")) {
        // Umgebungsvariable SSH_DIR als Override (Tests)?
        if let Some(env_dir) = std::env::var_os("GITMANAGER_SSH_DIR") {
            ssh_dir = PathBuf::from(env_dir);
        }
        for name in ["id_ed25519", "id_rsa", "id_ecdsa", "id_dsa"] {
            out.push(ssh_dir.join(name));
        }
    }
    out
}

/// Fetch eines einzelnen Remotes via System-`git` (Fallback für Auth).
///
/// Nutzt Credential-Manager/SSH-Agent des Users — daher funktioniert das auch
/// mit authentifizierten Repos, wo reines libgit2 ohne Callbacks scheitert.
/// `Ok` nur bei Exit-Code 0, sonst `Err` mit stderr (English-neutral via git,
/// aber nur fürs Log — strukturierter Fehler bleibt libgit2).
///
/// K1: ohne Refspecs `git -C p fetch --prune <remote>` (Refresh), mit
/// Refspecs `git -C p fetch --no-prune --no-tags <remote> <refspec…>`.
pub fn fetch_with_git_cli(
    repo_path: &Path,
    remote_name: &str,
    refspecs: &[&str],
    prune: bool,
) -> anyhow::Result<()> {
    let repo_str = repo_path.display().to_string();
    let mut args = vec!["-C", &repo_str, "fetch"];
    args.push(if prune { "--prune" } else { "--no-prune" });
    if !refspecs.is_empty() {
        args.push("--no-tags");
    }
    args.push(remote_name);
    args.extend_from_slice(refspecs);
    run_git_cli(
        &args,
        &format!("fetch '{remote_name}'"),
        GIT_CLI_FETCH_TIMEOUT,
    )
}

/// Obergrenzen für System-`git`: ein Prompt, den niemand sieht (SSH-
/// Passphrase ohne Agent auf der versteckten Konsole von
/// `CREATE_NO_WINDOW`), hielte sonst den Worker und damit den Ref-Lock
/// (`git_busy`, K1) für die ganze Sitzung.
const GIT_CLI_FETCH_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// `worktree add` checkt aus (ggf. mit LFS-Filtern).
pub(super) const GIT_CLI_WORKTREE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Ein Klon über eine langsame Leitung darf länger dauern.
pub(super) const GIT_CLI_CLONE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Behaltenes Ende von stdout/stderr: nur fürs Log, die Fehlermeldung steht
/// am Ende.
const GIT_CLI_OUTPUT_KEEP: usize = 8 * 1024;

/// System-`git` ohne Konsolenfenster und ohne Terminal-Prompt, höchstens
/// `timeout` lang (danach `kill`). `Ok` nur bei Exit-Code 0, sonst `Err`
/// mit stderr/stdout (nur fürs Log, siehe [`fetch_with_git_cli`]).
pub(super) fn run_git_cli(args: &[&str], what: &str, timeout: Duration) -> anyhow::Result<()> {
    let limits = Limits {
        timeout,
        stdout: Keep::Tail(GIT_CLI_OUTPUT_KEEP),
        stderr: Keep::Tail(GIT_CLI_OUTPUT_KEEP),
    };
    let out = process::output(&mut git_cli_command(args), limits)
        .map_err(|e| anyhow::anyhow!("git {what} {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let detail = if stderr.is_empty() {
        if stdout.is_empty() {
            format!("exit {}", out.status)
        } else {
            stdout
        }
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stderr} / {stdout}")
    };
    Err(anyhow::anyhow!("git {what} failed: {detail}"))
}

/// `git <args>` für [`run_git_cli`]: Windows-GUI ohne Konsolen-Flash
/// (CREATE_NO_WINDOW); `GIT_TERMINAL_PROMPT=0`, weil kein Terminal zu sehen
/// ist (git fragt sonst Benutzer/Passwort auf der versteckten Konsole ab und
/// wartet); Credential-Helper wie der Git Credential Manager fragen weiter
/// per Fenster.
pub(super) fn git_cli_command(args: &[&str]) -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    set_creation_flags(&mut cmd, CREATE_NO_WINDOW);
    cmd
}

/// Strukturierter Fetch-Fehler (alle Remotes failed UND partielle Fehlschläge).
/// Als anyhow-Error transportiert, damit die UI per [`is_partial_fetch_error`]
/// ohne String-Parsing `fetch_partial` statt `fetch_failed` wählen kann und das
/// Detail via [`PartialFetch::detail_en`] English-neutral (opake libgit2-Texte)
/// einbettet. Display ist English-neutral für Logs/Tests; die UI lokalisiert
/// Wrapper + Hinweis via i18n (`fetch_failed`/`fetch_partial`, `auth_hint`).
#[derive(Debug)]
pub struct PartialFetch {
    pub succeeded: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl PartialFetch {
    /// English-neutrales Detail ohne Wrapper/Hinweis: `'<remote>': <libgit2-msg>; ...`
    /// plus `; succeeded: '<ok>', ...` bei Teil-Erfolg. libgit2-Texte bleiben opak.
    pub fn detail_en(&self) -> String {
        let details = self
            .failed
            .iter()
            .map(|(n, e)| format!("'{n}': {e}"))
            .collect::<Vec<_>>()
            .join("; ");
        if self.succeeded.is_empty() {
            return details;
        }
        let ok_names = self
            .succeeded
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(", ");
        if details.is_empty() {
            format!("succeeded: {ok_names}")
        } else {
            format!("{details}; succeeded: {ok_names}")
        }
    }
}

impl std::fmt::Display for PartialFetch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // English-neutral für EN-UI + Logs; DE-UI nutzt detail_en() + i18n-Wrapper.
        const EN_HINT: &str = "Note: check Credentials/SSH (auth may be required)";
        if self.succeeded.is_empty() {
            let names = self
                .failed
                .iter()
                .map(|(n, _)| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ");
            write!(
                f,
                "Fetch failed for remote(s) {names}: {}. {EN_HINT}",
                self.detail_en()
            )
        } else {
            let failed_names = self
                .failed
                .iter()
                .map(|(n, _)| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ");
            write!(
                f,
                "Fetch partially failed for remote(s) {failed_names}: {}. {EN_HINT}",
                self.detail_en()
            )
        }
    }
}

impl std::error::Error for PartialFetch {}

/// True wenn `err` (aus [`fetch_all`]) einen teilweisen Fehlschlag beschreibt
/// (mind. 1 Remote ok, mind. 1 failed). Komplette Remote-Fehler sind ebenfalls
/// [`PartialFetch`] (mit leerem `succeeded`), zählen aber NICHT als partiell.
pub fn is_partial_fetch_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<PartialFetch>()
        .is_some_and(|p| !p.succeeded.is_empty())
}

/// True wenn `err` ein strukturierter Fetch-Remote-Fehler ist (komplett oder
/// partiell, d.h. [`PartialFetch`] mit beliebigem `succeeded`). Sonstige Fehler
/// (z.B. Repo öffnen) sind keine Remote-Fehler.
pub fn is_fetch_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<PartialFetch>().is_some()
}
