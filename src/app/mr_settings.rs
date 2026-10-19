//! Einstellungs-Tab „Merge Requests“ in der App (Task 12): Worker für die
//! glab-Prüfung, die Prüfung je Host (`probe_host`) sowie Token prüfen und
//! speichern. Der Empfänger liegt in `MyApp` (`mr_settings_rx`), nicht im
//! `SettingsState`: die App verwirft den Entwurf beim Schließen und nach
//! „Speichern“, Ergebnisse kommen trotzdem an (dann als Statusmeldung).
//! Nach dem Speichern eines Tokens laufen die lokale Erkennung für den Host
//! und der Abruf sofort neu (`bump_mr_auth_epoch`), auch ohne „Speichern“.

use super::MyApp;
use crate::config::MrConfig;
use crate::gitlab::glab::{
    glab_version, GatedRunner, GlabErrorKind, GlabGate, GlabRunner, SystemGlab,
};
use crate::gitlab::secret::SecretString;
use crate::gitlab::service::{
    env_token_override, login_key, probe_host, save_token, verify_token, HostProbe, Protocol,
    SavedWhere, TokenCheck,
};
use crate::gitlab::time::now_unix;
use crate::i18n::tr_fmt;
use crate::ui::settings::{
    error_reason, forget_token_fields, project_paths_for_host, rediscover_candidates, set_alias,
    verify_error, GlabCheck, MrSettingsInput, MrSettingsOutput, TokenEvent,
};
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Baut den glab-Runner für ein Programm (immer `GatedRunner`); Tests geben
/// ihren `FakeGlab` zurück.
pub(super) type GlabFactory = Box<dyn Fn(&str) -> Arc<dyn GlabRunner>>;

/// Echtes glab; alle Runner der Sitzung teilen sich ein Gate.
pub(super) fn system_glab_factory() -> GlabFactory {
    gated_factory(|program| SystemGlab::new(program))
}

/// Runner je Programm hinter einem gemeinsamen Gate: ein Programmwechsel
/// nach „Speichern“ oder die Prüfung eines Entwurfs-Programms laufen nie an
/// einem `auth login` vorbei, das noch mit dem alten Runner schreibt.
fn gated_factory<R: GlabRunner + 'static>(make: impl Fn(&str) -> R + 'static) -> GlabFactory {
    let gate = Arc::new(GlabGate::default());
    Box::new(move |program| Arc::new(GatedRunner::with_gate(make(program), Arc::clone(&gate))))
}

/// Ergebnis der Einstellungs-Worker (Kanal `mr_settings_tx`/`mr_settings_rx`).
pub(super) enum MrSettingsResult {
    GlabChecked(GlabCheck),
    Probed {
        host: String,
        probe: Box<HostProbe>,
    },
    Verified {
        host: String,
        result: Result<TokenCheck, GlabErrorKind>,
    },
    Saved {
        host: String,
        result: Result<SavedWhere, GlabErrorKind>,
    },
}

/// Ergebnisse und laufende Arbeit des Tabs für die Sitzung.
#[derive(Default)]
pub(super) struct MrSettingsCache {
    probes: BTreeMap<String, HostProbe>,
    probing: BTreeSet<String>,
    /// Nach dem laufenden Ergebnis erneut prüfen (Token inzwischen gespeichert)
    reprobe: BTreeSet<String>,
    glab_check: Option<GlabCheck>,
    glab_checking: bool,
    /// Token wird geprüft oder gespeichert (Host); höchstens einer zugleich
    token_busy: Option<String>,
}

impl MrSettingsCache {
    fn busy(&self) -> bool {
        self.glab_checking || self.token_busy.is_some() || !self.probing.is_empty()
    }
}

/// Token-Arbeit eines Workers; das Token wird nur verschoben (E3).
enum TokenJob {
    Verify {
        token: SecretString,
        protocol: Protocol,
        paths: Vec<String>,
    },
    Save {
        token: SecretString,
        protocol: Protocol,
    },
}

impl MyApp {
    /// Eingabe des Tabs aus dem Cache der Sitzung.
    pub(super) fn mr_settings_input(&self) -> MrSettingsInput<'_> {
        MrSettingsInput {
            theme: &self.config.theme,
            glab_program: &self.glab_program,
            repos: &self.repos,
            glab_hosts: &self.glab_hosts,
            probes: &self.mr_settings.probes,
            probing: &self.mr_settings.probing,
            glab_check: self.mr_settings.glab_check.as_ref(),
            glab_checking: self.mr_settings.glab_checking,
            token_busy: self.mr_settings.token_busy.as_deref(),
            env_token: env_token_override(),
            now: now_unix(),
        }
    }

    /// Startet die vom Tab angeforderte Arbeit; `draft` = MR-Entwurf (Hosts,
    /// „gehört zu …“ und Auswahl gelten für die Pfade schon vor „Speichern“).
    pub(super) fn handle_mr_settings_output(&mut self, out: MrSettingsOutput, draft: &MrConfig) {
        let MrSettingsOutput {
            start_probe,
            start_verify,
            start_save,
            hosts_changed,
            start_glab_check,
        } = out;
        if let Some(program) = start_glab_check {
            self.start_mr_glab_check(program);
        }
        for host in start_probe {
            self.start_mr_probe(host, draft);
        }
        if let Some((host, token, protocol)) = start_verify {
            let paths = project_paths_for_host(&self.repos, &self.glab_hosts, draft, &host);
            let job = TokenJob::Verify {
                token,
                protocol,
                paths,
            };
            self.start_mr_token_job(host, job);
        }
        if let Some((host, token, protocol)) = start_save {
            self.start_mr_token_job(host, TokenJob::Save { token, protocol });
        }
        if hosts_changed {
            // Neue GitLab-Hosts bzw. Ziele von „gehört zu …“: angemeldet?
            let hosts = rediscover_candidates(draft)
                .into_iter()
                .filter(|c| {
                    !self
                        .glab_hosts
                        .iter()
                        .any(|h| h.key == *c && !h.user.trim().is_empty())
                })
                .collect();
            self.rediscover_mr_hosts(hosts);
        }
    }

    /// Übernimmt Ergebnisse der Worker (aus `logic()`, auch bei
    /// geschlossenen Einstellungen) und räumt Token-Felder verworfener
    /// Entwürfe aus dem egui-Speicher.
    pub(super) fn poll_mr_settings(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.mr_settings_rx.try_recv() {
            match result {
                MrSettingsResult::GlabChecked(check) => self.on_glab_checked(check),
                MrSettingsResult::Probed { host, probe } => self.on_mr_probed(host, *probe),
                MrSettingsResult::Verified { host, result } => {
                    self.on_token_verified(ctx, host, result)
                }
                MrSettingsResult::Saved { host, result } => self.on_token_saved(ctx, host, result),
            }
            ctx.request_repaint();
        }
        if self.mr_settings.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let active = self
            .open_settings_state()
            .and_then(|state| state.token_field_id());
        forget_token_fields(ctx, active);
    }

    /// Nach „Speichern“: anderes glab-Programm → neuer Runner; Erkennung
    /// und glab-Prüfung beginnen von vorn (Version und Anmeldungen können
    /// abweichen). Laufende Worker behalten den alten Runner, beide hängen am
    /// selben Gate (`gated_factory`).
    pub(super) fn apply_glab_program(&mut self) {
        let program = self.config.merge_requests.glab_program.clone();
        if program == self.glab_program {
            return;
        }
        self.glab = (self.glab_factory)(&program);
        self.glab_program = program;
        self.mr_glab_missing = false;
        self.mr_glab_error = None;
        self.glab_hosts.clear();
    }

    /// Offener Entwurf (Ergebnisse an den Tab statt Statusmeldung).
    pub(super) fn open_settings_state(
        &mut self,
    ) -> Option<&mut crate::ui::settings::SettingsState> {
        if self.show_settings {
            self.settings_state.as_mut()
        } else {
            None
        }
    }

    /// Runner für ein Programm: der der Sitzung oder ein eigener für ein
    /// noch nicht gespeichertes Programm (am selben Gate).
    fn runner_for(&self, program: &str) -> Arc<dyn GlabRunner> {
        if program == self.glab_program {
            Arc::clone(&self.glab)
        } else {
            (self.glab_factory)(program)
        }
    }

    /// „Erneut prüfen“: `glab version` und Fundort des Programms.
    fn start_mr_glab_check(&mut self, program: String) {
        if self.mr_settings.glab_checking {
            return;
        }
        self.mr_settings.glab_checking = true;
        let runner = self.runner_for(&program);
        let tx = self.mr_settings_tx.clone();
        std::thread::spawn(move || {
            let version =
                catch_unwind(AssertUnwindSafe(|| glab_version(&*runner))).unwrap_or_else(|_| {
                    eprintln!("glab-Prüfung abgestürzt (panic vor send)");
                    Err(GlabErrorKind::Other)
                });
            let path = resolve_program(&program);
            let _ = tx.send(MrSettingsResult::GlabChecked(GlabCheck {
                program,
                version,
                path,
            }));
        });
    }

    fn on_glab_checked(&mut self, check: GlabCheck) {
        self.mr_settings.glab_checking = false;
        // glab der Sitzung nachinstalliert: Erkennung und Abruf von vorn
        let found = check.version.is_ok() && check.program == self.glab_program;
        self.mr_settings.glab_check = Some(check);
        if found && self.mr_glab_missing {
            self.mr_user_refresh();
        }
    }

    /// Prüfung eines Hosts (`probe_host`) mit den Projekten des Entwurfs.
    /// Läuft schon eine, folgt nach ihrem Ergebnis eine neue.
    fn start_mr_probe(&mut self, host: String, cfg: &MrConfig) {
        if self.mr_settings.probing.contains(&host) {
            self.mr_settings.reprobe.insert(host);
            return;
        }
        self.mr_settings.probing.insert(host.clone());
        let paths = project_paths_for_host(&self.repos, &self.glab_hosts, cfg, &host);
        let runner = Arc::clone(&self.glab);
        let tx = self.mr_settings_tx.clone();
        std::thread::spawn(move || {
            let probe = catch_unwind(AssertUnwindSafe(|| probe_host(&*runner, &host, &paths)))
                .unwrap_or_else(|_| {
                    eprintln!("Prüfung von {host} abgestürzt (panic vor send)");
                    HostProbe {
                        host_key: host.clone(),
                        error: Some(GlabErrorKind::Other),
                        ..HostProbe::default()
                    }
                });
            let _ = tx.send(MrSettingsResult::Probed {
                host,
                probe: Box::new(probe),
            });
        });
    }

    fn on_mr_probed(&mut self, host: String, probe: HostProbe) {
        self.mr_settings.probing.remove(&host);
        self.mr_settings.probes.insert(host.clone(), probe);
        if self.mr_settings.reprobe.remove(&host) {
            let cfg = self.mr_settings_cfg();
            self.start_mr_probe(host, &cfg);
        } else if self.open_settings_state().is_none() {
            let lang = self.config.language;
            self.set_status(tr_fmt(lang, "mrs_check_finished", &[&host]));
        }
    }

    /// MR-Einstellungen des offenen Entwurfs, sonst der gespeicherten Config.
    fn mr_settings_cfg(&self) -> MrConfig {
        match &self.settings_state {
            Some(state) if self.show_settings => state.draft.merge_requests.clone(),
            _ => self.config.merge_requests.clone(),
        }
    }

    /// Token prüfen oder speichern im Hintergrund; höchstens ein Token-Worker.
    fn start_mr_token_job(&mut self, host: String, job: TokenJob) {
        let empty = match &job {
            TokenJob::Verify { token, .. } | TokenJob::Save { token, .. } => token.is_empty(),
        };
        if empty || self.mr_settings.token_busy.is_some() {
            return;
        }
        self.mr_settings.token_busy = Some(host.clone());
        let runner = Arc::clone(&self.glab);
        let tx = self.mr_settings_tx.clone();
        std::thread::spawn(move || {
            let msg = match job {
                TokenJob::Verify {
                    token,
                    protocol,
                    paths,
                } => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        verify_token(&*runner, &host, token, protocol, &paths)
                    }))
                    .unwrap_or_else(|_| token_worker_panicked(&host));
                    MrSettingsResult::Verified { host, result }
                }
                TokenJob::Save { token, protocol } => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        save_token(&*runner, &host, token, protocol)
                    }))
                    .unwrap_or_else(|_| token_worker_panicked(&host));
                    MrSettingsResult::Saved { host, result }
                }
            };
            let _ = tx.send(msg);
        });
    }

    /// Token geprüft: Vorschau im offenen Formular; bei geschlossenem
    /// Fenster (Token mit dem Formular verworfen) nur eine Statusmeldung.
    fn on_token_verified(
        &mut self,
        ctx: &egui::Context,
        host: String,
        result: Result<TokenCheck, GlabErrorKind>,
    ) {
        self.mr_settings.token_busy = None;
        let lang = self.config.language;
        match self.open_settings_state() {
            Some(state) => state.on_token_event(ctx, &host, TokenEvent::Verified(result)),
            None => {
                let message = match &result {
                    Ok(_) => tr_fmt(lang, "mrs_check_finished", &[&host]),
                    Err(kind) => verify_error(lang, kind, &host),
                };
                self.set_status(message);
            }
        }
    }

    /// Token gespeichert: Statusmeldung, lokale Erkennung für den Host neu,
    /// sofortiger Refresh (auch ohne „Speichern“, wartet auf die Erkennung);
    /// die Karte prüft der offene Tab neu, sonst die nächste Öffnung.
    fn on_token_saved(
        &mut self,
        ctx: &egui::Context,
        host: String,
        result: Result<SavedWhere, GlabErrorKind>,
    ) {
        self.mr_settings.token_busy = None;
        let lang = self.config.language;
        match &result {
            Ok(_) => {
                self.set_status(tr_fmt(lang, "mrs_token_saved", &[&host]));
                self.mr_settings.probes.remove(&host);
                let mut hosts = vec![host.clone()];
                // Schlüssel mit Port meldet sich unter dem Hostnamen an
                if login_key(&host) != host {
                    hosts.push(login_key(&host).to_string());
                    self.alias_port_host(&host);
                }
                self.rediscover_mr_hosts(hosts);
                self.bump_mr_auth_epoch();
            }
            Err(kind) if self.open_settings_state().is_none() => {
                let reason = error_reason(lang, kind, &host);
                self.set_status(tr_fmt(lang, "mrs_token_failed", &[&reason]));
            }
            Err(_) => {}
        }
        if let Some(state) = self.open_settings_state() {
            state.on_token_event(ctx, &host, TokenEvent::Saved(result));
        }
    }

    /// Remote-Host `host:port` erreicht die Anmeldung unter dem Hostnamen
    /// nur über „gehört zu …“ (E16): Zuordnung gehört zur Token-Aktion und
    /// wirkt wie sie sofort (Config und offener Entwurf). Ein eingetragener
    /// GitLab-Host `host:port` trifft schon exakt.
    fn alias_port_host(&mut self, host: &str) {
        let cfg = &mut self.config.merge_requests;
        if cfg.extra_hosts.iter().any(|h| h.eq_ignore_ascii_case(host)) {
            return;
        }
        let target = login_key(host).to_string();
        if set_alias(cfg, host, &target) {
            self.config_save_pending = true;
        }
        if let Some(state) = self.open_settings_state() {
            set_alias(&mut state.draft.merge_requests, host, &target);
        }
    }
}

/// Absturz eines Token-Workers vor `send`: als Fehler melden, damit das
/// Formular nicht ewig prüft.
fn token_worker_panicked<T>(host: &str) -> Result<T, GlabErrorKind> {
    eprintln!("Token-Worker für {host} abgestürzt (panic vor send)");
    Err(GlabErrorKind::Other)
}

/// Fundort des Programms wie beim Start: ein Pfad selbst, sonst die erste
/// Fundstelle im PATH (unter Windows auch mit `.exe`). Startet nichts.
fn resolve_program(program: &str) -> Option<PathBuf> {
    let program = program.trim();
    if program.is_empty() {
        return None;
    }
    if program.contains(['/', '\\']) {
        let path = PathBuf::from(program);
        return path.is_file().then_some(path);
    }
    let names: Vec<String> = if cfg!(windows) && !program.to_lowercase().ends_with(".exe") {
        vec![format!("{program}.exe"), program.to_string()]
    } else {
        vec![program.to_string()]
    };
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        names
            .iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_app_with_glab;
    use super::*;
    use crate::config::HostAlias;
    use crate::git::RepoInfo;
    use crate::gitlab::glab::fake::FakeGlab;
    use crate::gitlab::glab::{CallKind, FakeReply, GlabError, GlabRequest, StdinBody};
    use crate::gitlab::remote::{GlabHost, RemoteInfo};
    use crate::gitlab::secret::SecretString;
    use crate::gitlab::service::Protocol;
    use crate::i18n::{tr, tr_fmt, Language};
    use crate::ui::settings::SettingsState;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const HOST: &str = "gitlab.example.com";
    const HOST_B: &str = "git.intern.example";
    const TOKEN: &str = "glpat-Neu0815Geheim";
    const PAT_SELF: &str = include_str!("../gitlab/fixtures/pat_self.json");
    const AUTH_STATUS: &str = include_str!("../gitlab/fixtures/auth_status.json");

    type Fake = Arc<GatedRunner<FakeGlab>>;

    fn fake_app() -> (MyApp, Fake) {
        let glab: Fake = Arc::new(GatedRunner::new(FakeGlab::new()));
        crate::app::merge_requests::tests::no_glab_config(glab.inner());
        let mut app = test_app_with_glab(glab.clone());
        app.config.language = Language::De;
        (app, glab)
    }

    fn repo(name: &str, url: &str) -> RepoInfo {
        let mut repo = RepoInfo::new(
            PathBuf::from(format!("/tmp/gm-mrs-test/{name}")),
            "main".to_string(),
            false,
            false,
        );
        repo.remotes = vec![RemoteInfo::new("origin", url)];
        repo
    }

    fn is_version(r: &GlabRequest) -> bool {
        r.args.first().is_some_and(|a| a == "version")
    }

    fn version_reply() -> FakeReply {
        FakeReply::stdout(0, "glab 1.123.0 (a300719)\n", "")
    }

    fn config_get(host: &'static str, key: &'static str) -> impl Fn(&GlabRequest) -> bool + Send {
        move |r: &GlabRequest| {
            r.args.first().is_some_and(|a| a == "config")
                && r.args.last().is_some_and(|k| k == key)
                && r.host_key.as_deref() == Some(host)
        }
    }

    /// Erkennung: `host` ist bei glab angemeldet.
    fn push_signed_in(fake: &FakeGlab, host: &'static str) {
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

    /// ACCESS nur mit `currentUser` (ohne Projekte).
    fn access_user() -> FakeReply {
        FakeReply::http(
            200,
            r#"{"data":{"currentUser":{"username":"taltmann","name":"Tim Altmann","bot":false},"metadata":{"version":"18.5.1-ee","enterprise":true}}}"#,
        )
    }

    /// `save_token` mit glab 1.123: Prüfen, `auth login`, Prüfung der
    /// gespeicherten Anmeldung.
    fn push_save_ok(fake: &FakeGlab) {
        fake.push_when(is_version, version_reply());
        fake.push(FakeReply::http(200, PAT_SELF));
        fake.push(access_user());
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(
                0,
                "",
                "✓ Stored your credentials in the operating system keyring\n",
            ),
        );
        fake.push(FakeReply::stdout(0, AUTH_STATUS, ""));
        fake.push(FakeReply::http(200, PAT_SELF));
        fake.push(access_user());
    }

    /// Ergebnisse übernehmen, bis kein Einstellungs- oder MR-Worker mehr
    /// läuft (höchstens 5 s).
    fn settle(app: &mut MyApp) {
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            app.poll_mr_settings(&ctx);
            app.poll_mr(&ctx);
            let settings_busy = app.mr_settings.token_busy.is_some()
                || app.mr_settings.glab_checking
                || !app.mr_settings.probing.is_empty();
            if !settings_busy && !app.mr_loading && !app.mr_discovering {
                return;
            }
            assert!(Instant::now() < until, "Worker hängt");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn save_output(host: &str) -> MrSettingsOutput {
        MrSettingsOutput {
            start_save: Some((
                host.to_string(),
                SecretString::new(TOKEN.to_string()),
                Protocol::Https,
            )),
            ..MrSettingsOutput::default()
        }
    }

    /// Fenster während `save_token` geschlossen: das Ergebnis kommt über
    /// `mr_settings_rx` an, Statusmeldung, Epoche erhöht, Erkennung für den
    /// Host neu und danach der Abruf mit dem neu angemeldeten Host.
    #[test]
    fn token_saved_while_settings_closed() {
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        app.repos = vec![repo("app", "git@gitlab.example.com:group/app.git")];
        // Erkennung kannte den Host als nicht angemeldet
        app.glab_hosts = vec![GlabHost {
            key: HOST.to_string(),
            ..GlabHost::default()
        }];
        app.update_mr_hosts();
        assert!(app.mr_hosts.gitlab_hosts.is_empty());
        app.show_settings = true;
        app.settings_state = Some(SettingsState::from_config(&app.config));
        push_save_ok(fake);
        // danach: Erkennung (jetzt angemeldet) und Abruf
        fake.push_when(is_version, version_reply());
        push_signed_in(fake, HOST);
        fake.push_when(is_version, version_reply());
        fake.push_when(
            |r: &GlabRequest| r.args.first().is_some_and(|a| a == "api"),
            FakeReply::error(GlabError::Timeout(Duration::from_secs(45))),
        );
        let draft = app.config.merge_requests.clone();
        app.handle_mr_settings_output(save_output(HOST), &draft);
        assert_eq!(app.mr_settings.token_busy.as_deref(), Some(HOST));
        // Einstellungen geschlossen, während glab speichert
        app.show_settings = false;
        app.settings_state = None;
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(5);
        while app.mr_settings.token_busy.is_some() {
            assert!(Instant::now() < until, "Speichern hängt");
            std::thread::sleep(Duration::from_millis(5));
            app.poll_mr_settings(&ctx);
        }
        assert_eq!(
            app.status_message.as_deref(),
            Some("Token für gitlab.example.com gespeichert")
        );
        assert_eq!(app.mr_auth_epoch, 1, "Epoche erhöht");
        assert!(app.mr_discovering, "Erkennung für den Host neu");
        assert!(app.mr_pending_refresh, "Refresh wartet auf die Erkennung");
        settle(&mut app);
        assert_eq!(app.mr_hosts.gitlab_hosts, vec![HOST.to_string()]);
        let hosts: Vec<&str> = app
            .mr_snapshot
            .as_deref()
            .map(|s| s.per_host.iter().map(|h| h.host_key.as_str()).collect())
            .unwrap_or_default();
        assert_eq!(hosts, [HOST], "Abruf mit dem neu angemeldeten Host");
        let calls = fake.take_calls();
        assert!(!format!("{calls:?}").contains(TOKEN));
        let login = calls
            .iter()
            .find(|c| c.request.kind == CallKind::Write)
            .expect("auth login");
        assert!(matches!(login.request.stdin, Some(StdinBody::Secret(_))));
        assert!(app.error.is_none(), "E20: kein Banner");
    }

    /// Fehler beim Speichern bei geschlossenem Fenster → Statusmeldung.
    #[test]
    fn token_save_failure_while_closed_is_status_message() {
        let (mut app, _glab) = fake_app();
        app.mr_settings.token_busy = Some(HOST.to_string());
        app.mr_settings_tx
            .send(MrSettingsResult::Saved {
                host: HOST.to_string(),
                result: Err(GlabErrorKind::NotInstalled),
            })
            .expect("send");
        app.poll_mr_settings(&egui::Context::default());
        assert!(app.mr_settings.token_busy.is_none());
        let reason = tr_fmt(Language::De, "mrs_glab_not_found", &[]);
        assert_eq!(
            app.status_message,
            Some(tr_fmt(Language::De, "mrs_token_failed", &[&reason]))
        );
        assert_eq!(app.mr_auth_epoch, 0);
        assert!(app.error.is_none());
    }

    /// Prüfung fertig, Fenster inzwischen geschlossen (Token mit dem
    /// Formular verworfen): Statusmeldung statt Vorschau.
    #[test]
    fn token_verified_while_closed_is_status_message() {
        let (mut app, _glab) = fake_app();
        let cases = [
            (
                Ok(TokenCheck::default()),
                "Prüfung für gitlab.example.com abgeschlossen".to_string(),
            ),
            (
                Err(GlabErrorKind::InvalidToken),
                tr(Language::De, "mrs_token_invalid"),
            ),
        ];
        for (result, expected) in cases {
            app.mr_settings.token_busy = Some(HOST.to_string());
            app.mr_settings_tx
                .send(MrSettingsResult::Verified {
                    host: HOST.to_string(),
                    result,
                })
                .expect("send");
            app.poll_mr_settings(&egui::Context::default());
            assert!(app.mr_settings.token_busy.is_none());
            assert_eq!(app.status_message.as_deref(), Some(expected.as_str()));
        }
        assert!(app.error.is_none());
    }

    /// Token für `host:port` gespeichert (glab meldet sich unter `host` an):
    /// „gehört zu …“ kommt sofort in Config und offenen Entwurf, der Remote
    /// erreicht die Anmeldung über den Hostnamen.
    #[test]
    fn port_host_token_maps_remote_to_login_key() {
        const PORT_HOST: &str = "git.local:8443";
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        app.repos = vec![repo("tool", "https://git.local:8443/alt/tool.git")];
        app.show_settings = true;
        app.settings_state = Some(SettingsState::from_config(&app.config));
        app.mr_settings.token_busy = Some(PORT_HOST.to_string());
        // Erkennung beider Schlüssel, danach der Abruf
        fake.push_when(is_version, version_reply());
        push_signed_in(fake, "git.local");
        fake.push_when(config_get(PORT_HOST, "user"), FakeReply::stdout(0, "", ""));
        fake.push_when(is_version, version_reply());
        fake.push_when(
            |r: &GlabRequest| r.args.first().is_some_and(|a| a == "api"),
            FakeReply::error(GlabError::Timeout(Duration::from_secs(45))),
        );
        app.mr_settings_tx
            .send(MrSettingsResult::Saved {
                host: PORT_HOST.to_string(),
                result: Ok(SavedWhere::Keyring),
            })
            .expect("send");
        settle(&mut app);
        let alias = vec![HostAlias {
            from: PORT_HOST.to_string(),
            to: "git.local".to_string(),
        }];
        assert_eq!(app.config.merge_requests.host_aliases, alias);
        assert!(app.config_save_pending, "Config wird gespeichert");
        let draft = &app.settings_state.as_ref().expect("Entwurf").draft;
        assert_eq!(draft.merge_requests.host_aliases, alias);
        assert_eq!(app.mr_hosts.gitlab_hosts, vec!["git.local".to_string()]);
        assert!(app.error.is_none());
    }

    /// Runner verschiedener Programme aus derselben Fabrik teilen ein Gate:
    /// `auth login` mit dem alten Runner läuft nicht neben einem Aufruf des
    /// neuen (Programmwechsel nach „Speichern“, während ein Token speichert).
    #[test]
    fn factory_runners_share_one_gate() {
        let fake = Arc::new(FakeGlab::new());
        let factory = gated_factory({
            let fake = Arc::clone(&fake);
            move |_| Arc::clone(&fake)
        });
        let old = factory("glab");
        let new = factory("C:\\Tools\\glab.exe");
        fake.push_when(
            |r| r.kind == CallKind::Write,
            FakeReply::stdout(0, "", "").delay(Duration::from_millis(40)),
        );
        fake.push_when(
            |r| r.kind == CallKind::Read,
            FakeReply::http(200, "{}").delay(Duration::from_millis(20)),
        );
        let write = std::thread::spawn(move || {
            let login =
                GlabRequest::write(Some(HOST), ["auth", "login", "--hostname", HOST, "--stdin"])
                    .with_stdin(StdinBody::Secret(SecretString::new(TOKEN.to_string())));
            old.run(login)
        });
        let read = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            new.run(GlabRequest::api(HOST, ["user"]))
        });
        assert!(write.join().expect("join").is_ok());
        assert!(read.join().expect("join").is_ok());
        assert!(!fake.write_overlapped(), "ein Gate für alle Runner");
        assert_eq!(fake.take_calls().len(), 2);
    }

    /// `hosts_changed` → lokale Erkennung für die Hosts des Entwurfs neu,
    /// auch wenn der Cache sie schon kennt.
    #[test]
    fn hosts_changed_rediscovers_draft_hosts() {
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        app.repos = vec![repo("lib", "https://git.intern.example/group/lib.git")];
        app.glab_hosts = vec![GlabHost {
            key: HOST_B.to_string(),
            ..GlabHost::default()
        }];
        app.config.merge_requests.refresh_interval_min = 0;
        let mut draft = app.config.merge_requests.clone();
        draft.host_aliases = vec![HostAlias {
            from: "ssh.intern.example".to_string(),
            to: HOST_B.to_string(),
        }];
        fake.push_when(is_version, version_reply());
        push_signed_in(fake, HOST_B);
        let out = MrSettingsOutput {
            hosts_changed: true,
            ..MrSettingsOutput::default()
        };
        app.handle_mr_settings_output(out, &draft);
        assert!(app.mr_discovering, "Erkennung läuft");
        settle(&mut app);
        let known = app.glab_hosts.iter().find(|h| h.key == HOST_B);
        assert_eq!(known.map(|h| h.user.as_str()), Some("taltmann"));
        let calls = fake.take_calls();
        assert!(calls
            .iter()
            .any(|c| c.request.args.contains(&"user".to_string())
                && c.request.host_key.as_deref() == Some(HOST_B)));
        assert!(calls.iter().all(|c| c.request.args[0] != "api"));
    }

    /// Prüfung bei geschlossenem Fenster → Statusmeldung; Ergebnis im Cache.
    #[test]
    fn probe_result_is_cached_and_announced_when_closed() {
        let (mut app, _glab) = fake_app();
        app.mr_settings.probing.insert(HOST.to_string());
        app.mr_settings_tx
            .send(MrSettingsResult::Probed {
                host: HOST.to_string(),
                probe: Box::new(HostProbe {
                    host_key: HOST.to_string(),
                    ..HostProbe::default()
                }),
            })
            .expect("send");
        app.poll_mr_settings(&egui::Context::default());
        assert!(app.mr_settings.probing.is_empty());
        assert!(app.mr_settings.probes.contains_key(HOST));
        assert_eq!(
            app.status_message.as_deref(),
            Some("Prüfung für gitlab.example.com abgeschlossen")
        );
        // Offenes Fenster: keine Statusmeldung
        app.status_message = None;
        app.show_settings = true;
        app.settings_state = Some(SettingsState::from_config(&app.config));
        app.mr_settings.probing.insert(HOST.to_string());
        app.mr_settings_tx
            .send(MrSettingsResult::Probed {
                host: HOST.to_string(),
                probe: Box::default(),
            })
            .expect("send");
        app.poll_mr_settings(&egui::Context::default());
        assert!(app.status_message.is_none());
    }

    /// Anderes glab-Programm nach „Speichern“: neuer Runner, Erkennung von
    /// vorn (glab war als fehlend gemerkt).
    #[test]
    fn changed_glab_program_replaces_runner() {
        let (mut app, _glab) = fake_app();
        app.mr_glab_missing = true;
        app.glab_hosts = vec![GlabHost {
            key: HOST.to_string(),
            ..GlabHost::default()
        }];
        app.apply_glab_program();
        assert!(app.mr_glab_missing, "unverändert → nichts");
        app.config.merge_requests.glab_program = "C:\\Tools\\glab.exe".to_string();
        app.apply_glab_program();
        assert_eq!(app.glab_program, "C:\\Tools\\glab.exe");
        assert!(!app.mr_glab_missing);
        assert!(app.glab_hosts.is_empty());
    }
}
