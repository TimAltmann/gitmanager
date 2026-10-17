//! Verdrahtung der Merge-Request-Ansicht in `MyApp` (Task 10): lokale
//! glab-Erkennung nach dem Scan (E16), Abruf im Hintergrund mit Epoche und
//! gemerktem Refresh, „Nicht beteiligt“ nachladen, Umschalter mit Badge (E9),
//! Statusleiste und Auswertung von `MrViewActions`. Fehler aus Erkennung und
//! Abruf bleiben im MR-Zustand, nie in `self.error` (E20). Hintergrundarbeit
//! wie beim Scan: `std::thread` + `mpsc` + `catch_unwind`, Übernahme in
//! `poll_mr` aus `logic()`.

use super::{should_auto_fetch, MainView, MyApp};
use crate::config::MrConfig;
use crate::git::RepoInfo;
use crate::gitlab::checkout::MenuLookup;
use crate::gitlab::glab::{glab_version, GlabErrorKind, GlabRunner};
use crate::gitlab::model::{
    sort_others, sort_uninvolved, MrPart, MrSnapshot, Reason, UninvolvedCursor,
};
use crate::gitlab::remote::{builtin_alias_target, local_projects, resolve_hosts, GlabHost};
use crate::gitlab::service::{
    discover_glab_hosts, fetch_snapshot, fetch_uninvolved_more, is_auth_error, MrFetchResult,
    UninvolvedPage,
};
use crate::gitlab::time::{now_unix, relative_time};
use crate::i18n::{tr, tr_fmt, Language};
use crate::ui::merge_requests::{show_merge_requests, MrViewActions, MrViewInput};
use crate::ui::settings::SettingsState;
use crate::ui::theme::mr_palette;
use egui::{
    vec2, Align2, Atom, Atoms, Button, FontId, Frame, Id, Image, ImageSource, Response, RichText,
    Stroke, Ui, WidgetInfo, WidgetType,
};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Unter dieser Fensterbreite zeigen „Aktualisieren“ und „Einstellungen“ nur
/// ihr Icon (mit Tooltip), damit die Top-Leiste mit Umschalter passt.
pub(super) const TOP_BAR_COMPACT_WIDTH: f32 = 1100.0;

/// Unter dieser Fensterbreite entfallen zusätzlich die Beschriftungen
/// „Profil:“, „AI:“ und „Sprache“ (dann Tooltip der Combos), damit die
/// Top-Leiste beim Mindestfenster (920 px) passt; das Standardfenster
/// (`DEFAULT_WINDOW_SIZE`, 1100 px) behält sie samt Button-Texten.
pub(super) const TOP_BAR_CAPTIONS_WIDTH: f32 = 1000.0;

/// Ergebnis der MR-Hintergrundarbeit (Kanal `mr_tx`/`mr_rx`).
pub(super) enum MrResult {
    /// Abruf fertig; `epoch` = `mr_auth_epoch` beim Start
    Loaded {
        epoch: u64,
        data: Box<MrFetchResult>,
    },
    /// Weitere Seite „Nicht beteiligt“ eines Hosts
    MorePage {
        epoch: u64,
        host_key: String,
        page: Result<UninvolvedPage, GlabErrorKind>,
    },
    /// Lokale glab-Erkennung (E16) fertig; `glab_error`, wenn glab fehlt
    /// oder zu alt ist (dann ohne Hosts)
    Discovered {
        hosts: Vec<GlabHost>,
        glab_error: Option<GlabErrorKind>,
    },
    /// Abruf-Worker vor `send` abgestürzt: alter Stand bleibt
    Aborted { reason_key: &'static str },
}

impl MyApp {
    /// GitLab-Hosts und lokale Projekte aus Repos, Erkennung und Config neu
    /// zuordnen (rein lokal).
    pub(super) fn update_mr_hosts(&mut self) {
        let cfg = &self.config.merge_requests;
        let remotes = self
            .repos
            .iter()
            .flat_map(|r| r.remotes.iter().filter_map(|x| x.parsed.as_ref()));
        self.mr_hosts = resolve_hosts(remotes, &self.glab_hosts, cfg);
        self.mr_locals = local_projects(&self.repos, &self.mr_hosts, cfg);
    }

    /// Nach jedem Scan: Hosts neu zuordnen, neue Kandidaten lokal erkennen
    /// (E16). Abruf, wenn „Speichern“ auf diesen Scan wartet, ein neuer
    /// GitLab-Host im Stand fehlt oder das Intervall abgelaufen ist (Badge).
    pub(super) fn mr_after_scan(&mut self) {
        let before = self.mr_hosts.gitlab_hosts.clone();
        self.update_mr_hosts();
        let discovering = self.start_mr_discovery();
        // Folge-Scan (gemerkt während eines Auto-Fetch) abwarten
        let saved =
            !self.scanning && !self.pending_scan && std::mem::take(&mut self.mr_refresh_after_scan);
        if saved || self.mr_lacks_new_hosts(&before) {
            // Wartet ggf. auf die Erkennung (`mr_pending_refresh`)
            self.start_mr_refresh();
        } else if !discovering {
            self.refresh_mr_if_due();
        }
    }

    /// Nach „Speichern“ in den Einstellungen: Hosts und Auswahl können sich
    /// geändert haben. Läuft der Rescan (neue Suchpfade/Tiefe), lädt erst
    /// `mr_after_scan` mit den neuen Repos; sonst neue Kandidaten erkennen
    /// und sofort neu laden.
    pub(super) fn mr_after_settings_save(&mut self) {
        self.update_mr_hosts();
        if self.scanning || self.pending_scan {
            self.mr_refresh_after_scan = true;
            return;
        }
        self.start_mr_discovery();
        // Wartet ggf. auf die Erkennung
        self.start_mr_refresh();
    }

    /// Ein seit `before` dazugekommener GitLab-Host fehlt im Stand bzw. im
    /// laufenden Abruf. Ohne beides entscheidet `refresh_mr_if_due`.
    fn mr_lacks_new_hosts(&self, before: &[String]) -> bool {
        (self.mr_loading || self.mr_snapshot.is_some())
            && self
                .mr_hosts
                .gitlab_hosts
                .iter()
                .any(|h| !before.contains(h))
    }

    /// Startet die lokale Erkennung für Kandidaten, die noch nicht im Cache
    /// sind. `true`, solange eine Erkennung läuft (danach folgt ggf. der
    /// Abruf). glab fehlt → nichts.
    fn start_mr_discovery(&mut self) -> bool {
        if self.mr_discovering {
            return true;
        }
        if self.mr_glab_missing {
            return false;
        }
        let known: HashSet<&str> = self.glab_hosts.iter().map(|h| h.key.as_str()).collect();
        let mut candidates: Vec<String> =
            discovery_candidates(&self.repos, &self.config.merge_requests)
                .into_iter()
                .filter(|c| !known.contains(c.as_str()))
                .collect();
        // Ausdrücklich angefragte Hosts (`rediscover_mr_hosts`), auch bekannte
        for host in std::mem::take(&mut self.mr_rediscover) {
            if !candidates.contains(&host) {
                candidates.push(host);
            }
        }
        if candidates.is_empty() {
            return false;
        }
        self.mr_discovering = true;
        let runner = Arc::clone(&self.glab);
        let tx = self.mr_tx.clone();
        std::thread::spawn(move || {
            let found = catch_unwind(AssertUnwindSafe(|| discover(&*runner, &candidates)));
            let msg = match found {
                Ok((hosts, glab_error)) => MrResult::Discovered { hosts, glab_error },
                Err(_) => {
                    eprintln!("glab-Erkennung abgestürzt (panic vor send)");
                    // Als nicht angemeldet merken, sonst startet sie endlos neu
                    let hosts = candidates
                        .iter()
                        .map(|key| GlabHost {
                            key: key.clone(),
                            ..GlabHost::default()
                        })
                        .collect();
                    MrResult::Discovered {
                        hosts,
                        glab_error: None,
                    }
                }
            };
            let _ = tx.send(msg);
        });
        true
    }

    /// Abruf, wenn das Intervall abgelaufen ist oder die Ansicht offen ist
    /// und noch keinen Stand hat (Umschalten während der Erkennung). Ein
    /// laufender Abruf genügt.
    fn refresh_mr_if_due(&mut self) {
        if self.mr_loading {
            return;
        }
        let interval = self.config.merge_requests.refresh_interval_min;
        let due = should_auto_fetch(self.mr_last_refresh, interval, Instant::now());
        let waiting = self.main_view == MainView::MergeRequests && self.mr_snapshot.is_none();
        if due || waiting {
            self.start_mr_refresh();
        }
    }

    /// Startet den Abruf aller GitLab-Hosts im Hintergrund. Läuft schon
    /// einer (oder „weitere laden“), wird der Wunsch gemerkt; während der
    /// Erkennung ebenso, damit der Abruf ihre Hosts mitnimmt.
    pub(super) fn start_mr_refresh(&mut self) {
        // glab fehlt → kein Aufruf mehr
        if self.mr_glab_missing {
            return;
        }
        if self.mr_discovering {
            self.mr_pending_refresh = true;
            return;
        }
        // E20: ohne GitLab-Host kein Netz-Aufruf
        if self.mr_hosts.gitlab_hosts.is_empty() {
            return;
        }
        if self.mr_loading || self.mr_more_pending > 0 {
            self.mr_pending_refresh = true;
            return;
        }
        // Dieser Abruf erfüllt einen gemerkten Wunsch
        self.mr_pending_refresh = false;
        self.mr_loading = true;
        let runner = Arc::clone(&self.glab);
        let hosts = self.mr_hosts.clone();
        let locals = self.mr_locals.clone();
        let cfg = self.config.merge_requests.clone();
        let include_uninvolved = self.mr_view.show_uninvolved;
        let epoch = self.mr_auth_epoch;
        let tx = self.mr_tx.clone();
        std::thread::spawn(move || {
            let fetched = catch_unwind(AssertUnwindSafe(|| {
                fetch_snapshot(&*runner, &hosts, &locals, &cfg, include_uninvolved)
            }));
            let msg = match fetched {
                Ok(data) => MrResult::Loaded {
                    epoch,
                    data: Box::new(data),
                },
                Err(_) => {
                    eprintln!("MR-Abruf abgestürzt (panic vor send)");
                    MrResult::Aborted {
                        reason_key: "mr_fetch_aborted",
                    }
                }
            };
            let _ = tx.send(msg);
        });
    }

    /// Nächste Seite „Nicht beteiligt“: ein Worker je Host mit Cursorn.
    fn start_mr_load_more(&mut self) {
        if self.mr_glab_missing || self.mr_loading || self.mr_more_pending > 0 {
            return;
        }
        let Some(snap) = self.mr_snapshot.as_deref() else {
            return;
        };
        let Some(list) = &snap.uninvolved else {
            return;
        };
        let mut by_host: BTreeMap<&str, Vec<UninvolvedCursor>> = BTreeMap::new();
        for cursor in &list.more {
            by_host
                .entry(cursor.host_key.as_str())
                .or_default()
                .push(cursor.clone());
        }
        // Ohne angemeldeten Benutzer (`currentUser`) keine Seite
        let jobs: Vec<(String, String, Vec<UninvolvedCursor>)> = by_host
            .into_iter()
            .filter_map(|(host, cursors)| {
                let status = snap.per_host.iter().find(|h| h.host_key == host)?;
                Some((host.to_string(), status.user.clone()?, cursors))
            })
            .collect();
        self.mr_more_pending = jobs.len();
        let epoch = self.mr_auth_epoch;
        for (host_key, me, cursors) in jobs {
            let runner = Arc::clone(&self.glab);
            let tx = self.mr_tx.clone();
            std::thread::spawn(move || {
                let page = catch_unwind(AssertUnwindSafe(|| {
                    fetch_uninvolved_more(&*runner, &host_key, &me, &cursors)
                }))
                .unwrap_or_else(|_| {
                    eprintln!("MR-Seite für {host_key} abgestürzt (panic vor send)");
                    Err(GlabErrorKind::Other)
                });
                let _ = tx.send(MrResult::MorePage {
                    epoch,
                    host_key,
                    page,
                });
            });
        }
    }

    /// „Aktualisieren“ in der MR-Ansicht bzw. auf der Karte „glab fehlt“:
    /// fehlte glab, beginnt die Erkennung von vorn (nach einer Installation);
    /// sonst werden nicht angemeldete Kandidaten erneut lokal geprüft
    /// (`glab auth login` im Terminal, E16). Der Abruf folgt danach.
    pub(super) fn mr_user_refresh(&mut self) {
        if self.mr_glab_missing {
            self.mr_glab_missing = false;
            self.mr_glab_error = None;
            self.glab_hosts.clear();
        } else {
            self.glab_hosts.retain(|h| !h.user.trim().is_empty());
        }
        self.update_mr_hosts();
        self.start_mr_discovery();
        // Wartet ggf. auf die Erkennung
        self.start_mr_refresh();
    }

    /// Lokale Erkennung (E16) für diese Hosts erneut, auch wenn der Cache
    /// sie kennt (Token gespeichert, GitLab-Host oder „gehört zu …“ im
    /// Einstellungs-Tab); läuft gerade eine, folgen sie danach.
    pub(super) fn rediscover_mr_hosts(&mut self, hosts: Vec<String>) {
        self.mr_rediscover.extend(
            hosts
                .into_iter()
                .map(|h| h.trim().to_lowercase())
                .filter(|h| !h.is_empty()),
        );
        self.start_mr_discovery();
    }

    /// Anmeldung geändert (Token gespeichert, Task 12): laufende Ergebnisse
    /// verwerfen und neu laden. Laufende Seiten „Nicht beteiligt“ zählen
    /// weiter mit (`mr_more_pending`), ihre Ergebnisse verwirft die Epoche.
    pub(super) fn bump_mr_auth_epoch(&mut self) {
        self.mr_auth_epoch = self.mr_auth_epoch.wrapping_add(1);
        self.start_mr_refresh();
    }

    /// Übernimmt Ergebnisse der MR-Worker (aus `logic()`, auch bei
    /// verstecktem Fenster).
    pub(super) fn poll_mr(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.mr_rx.try_recv() {
            match result {
                MrResult::Loaded { epoch, data } => self.on_mr_loaded(epoch, *data),
                MrResult::MorePage {
                    epoch,
                    host_key,
                    page,
                } => self.on_mr_more_page(epoch, &host_key, page),
                MrResult::Discovered { hosts, glab_error } => {
                    self.on_mr_discovered(hosts, glab_error)
                }
                MrResult::Aborted { reason_key } => self.on_mr_aborted(reason_key),
            }
            ctx.request_repaint();
        }
        if self.mr_loading || self.mr_discovering || self.mr_more_pending > 0 {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn on_mr_loaded(&mut self, epoch: u64, data: MrFetchResult) {
        self.mr_loading = false;
        if epoch != self.mr_auth_epoch {
            // Anmeldung hat sich während des Abrufs geändert: neu laden
            self.mr_pending_refresh = false;
            self.start_mr_refresh();
            return;
        }
        // I6 wie beim Auto-Fetch: Zeitstempel am Ende
        self.mr_last_refresh = Some(Instant::now());
        match data.glab_error {
            Some(kind) => self.set_glab_missing(kind),
            None => {
                self.mr_snapshot = Some(Box::new(data.snapshot));
                // E21: Badge im Tray beim nächsten Sync aktualisieren
                self.mark_tray_dirty();
            }
        }
        self.run_pending_mr_refresh();
    }

    fn on_mr_more_page(
        &mut self,
        epoch: u64,
        host_key: &str,
        page: Result<UninvolvedPage, GlabErrorKind>,
    ) {
        self.mr_more_pending = self.mr_more_pending.saturating_sub(1);
        if epoch == self.mr_auth_epoch {
            if let Some(snap) = self.mr_snapshot.as_deref_mut() {
                match page {
                    Ok(page) => {
                        merge_more_page(snap, page);
                        // Review-Anfragen der Seite ändern das Badge (E21)
                        self.mark_tray_dirty();
                    }
                    Err(kind) => note_more_error(snap, host_key, kind),
                }
            }
        }
        self.run_pending_mr_refresh();
    }

    fn on_mr_discovered(&mut self, hosts: Vec<GlabHost>, glab_error: Option<GlabErrorKind>) {
        self.mr_discovering = false;
        if let Some(kind) = glab_error {
            self.set_glab_missing(kind);
            return;
        }
        for host in hosts {
            match self.glab_hosts.iter_mut().find(|h| h.key == host.key) {
                Some(known) => *known = host,
                None => self.glab_hosts.push(host),
            }
        }
        let before = self.mr_hosts.gitlab_hosts.clone();
        self.update_mr_hosts();
        // Kandidaten aus einem Scan während der Erkennung
        let discovering = self.start_mr_discovery();
        if std::mem::take(&mut self.mr_pending_refresh) || self.mr_lacks_new_hosts(&before) {
            // Bei erneuter Erkennung wieder gemerkt
            self.start_mr_refresh();
        } else if !discovering {
            self.refresh_mr_if_due();
        }
    }

    fn on_mr_aborted(&mut self, reason_key: &'static str) {
        self.mr_loading = false;
        // Kein sofortiger Neustart bei einer Panic-Schleife
        self.mr_last_refresh = Some(Instant::now());
        // E20: nur in der MR-Ansicht, nie im Banner oder Tray
        if self.main_view == MainView::MergeRequests {
            self.status_message = Some(tr(self.config.language, reason_key));
            self.status_message_time = Some(Instant::now());
        }
        self.run_pending_mr_refresh();
    }

    /// glab fehlt oder ist zu alt: Karte statt Spalten, keine weiteren
    /// glab-Aufrufe in der Sitzung (außer „Aktualisieren“). E20: weder
    /// `self.error` noch Tray-Hinweis.
    fn set_glab_missing(&mut self, kind: GlabErrorKind) {
        self.mr_glab_missing = true;
        self.mr_glab_error = Some(kind);
        self.mr_pending_refresh = false;
    }

    /// Gemerkten Refresh nachholen, sobald nichts mehr läuft; während der
    /// Erkennung übernimmt `on_mr_discovered` (dann mit den neuen Hosts).
    fn run_pending_mr_refresh(&mut self) {
        let busy = self.mr_loading || self.mr_discovering || self.mr_more_pending > 0;
        if self.mr_pending_refresh && !busy {
            self.mr_pending_refresh = false;
            self.start_mr_refresh();
        }
    }

    /// Intervall-Abruf, nur bei sichtbarem Fenster (aus `logic()`).
    pub(super) fn maybe_refresh_mr(&mut self, ctx: &egui::Context) {
        let interval = self.config.merge_requests.refresh_interval_min;
        if interval == 0
            || !self.window_visible
            || self.mr_glab_missing
            || self.mr_hosts.gitlab_hosts.is_empty()
        {
            return;
        }
        // Nach der Erkennung entscheidet `on_mr_discovered` (mit ihren Hosts)
        let busy = self.mr_loading || self.mr_discovering || self.mr_more_pending > 0;
        if !busy && should_auto_fetch(self.mr_last_refresh, interval, Instant::now()) {
            self.start_mr_refresh();
        }
        // Weckt `logic()` für die nächste Prüfung
        ctx.request_repaint_after(Duration::from_secs(60));
    }

    /// Tray-Button „Merge Requests“ (E21): Hauptfenster direkt in der
    /// MR-Ansicht; neu geladen wird ohne Stand oder nach dem Intervall.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(super) fn show_mrs_from_tray(&mut self, ctx: &egui::Context) {
        self.main_view = MainView::MergeRequests;
        self.show_main_window(ctx);
        self.refresh_mr_if_due();
    }

    /// Tray „Hauptfenster“ (auch Doppelklick aufs Icon): Hauptfenster mit
    /// der Repo-Liste. Die MR-Ansicht hat ihren eigenen Tray-Button (E21);
    /// sonst täten beide Buttons nach einem Besuch der MR-Ansicht dasselbe.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(super) fn show_main_from_tray(&mut self, ctx: &egui::Context) {
        self.set_main_view(MainView::Repos);
        self.show_main_window(ctx);
    }

    /// Wechselt die Hauptansicht; der erste Wechsel in die MR-Ansicht lädt.
    pub(super) fn set_main_view(&mut self, view: MainView) {
        if self.main_view == view {
            return;
        }
        self.main_view = view;
        if view == MainView::MergeRequests && self.mr_snapshot.is_none() && !self.mr_loading {
            self.start_mr_refresh();
        }
    }

    /// Umschalter „Repositories N | Merge Requests [Badge]“ in der Top-Leiste.
    pub(super) fn view_switcher(&mut self, ui: &mut Ui) {
        let lang = self.config.language;
        let p = mr_palette(&self.config.theme, ui.visuals());
        let badge = mr_badge(self.mr_snapshot.as_deref());
        let mut clicked = None;
        Frame::new()
            .fill(ui.visuals().window_fill)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(8)
            .inner_margin(2)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let repos_on = self.main_view == MainView::Repos;
                let mut count = RichText::new(self.repos.len().to_string()).size(11.0);
                if !repos_on {
                    count = count.color(p.text2);
                }
                let mut atoms = Atoms::new(RichText::new(tr(lang, "view_repos")).size(12.0));
                atoms.push_right(count);
                if ui
                    .add(segment(repos_on, atoms))
                    .on_hover_text(tr(lang, "view_repos_tt"))
                    .clicked()
                {
                    clicked = Some(MainView::Repos);
                }

                let mrs_on = self.main_view == MainView::MergeRequests;
                let mut atoms = Atoms::new(RichText::new(tr(lang, "view_mrs")).size(12.0));
                let badge_id = Id::new("mr_view_badge");
                let galley = badge.map(|n| {
                    ui.painter().layout_no_wrap(
                        n.to_string(),
                        FontId::proportional(10.0),
                        p.badge_text,
                    )
                });
                if let Some(galley) = &galley {
                    let width = (galley.size().x + 10.0).max(18.0);
                    atoms.push_right(Atom::custom(badge_id, vec2(width, 16.0)));
                }
                let out = segment(mrs_on, atoms).atom_ui(ui);
                // Badge per Painter; Zahl steht zusätzlich im AccessKit-Namen
                if let (Some(galley), Some(rect)) = (galley, out.rect(badge_id)) {
                    let painter = ui.painter();
                    painter.rect_filled(rect, 8.0, p.badge);
                    let pos = Align2::CENTER_CENTER
                        .anchor_size(rect.center(), galley.size())
                        .min;
                    painter.galley(pos, galley, p.badge_text);
                }
                let name = match badge {
                    Some(n) => badge_tooltip(lang, n),
                    None => tr(lang, "view_mrs"),
                };
                out.response
                    .widget_info(|| WidgetInfo::selected(WidgetType::Button, true, mrs_on, &name));
                let tooltip = match badge {
                    Some(n) => badge_tooltip(lang, n),
                    None => tr(lang, "view_mrs_tt"),
                };
                if out.response.on_hover_text(tooltip).clicked() {
                    clicked = Some(MainView::MergeRequests);
                }
            });
        if let Some(view) = clicked {
            self.set_main_view(view);
        }
    }

    /// Statusleiste in der MR-Ansicht (Text, ist Statusmeldung).
    pub(super) fn mr_status_line(&self) -> (String, bool) {
        mr_status_text(
            self.config.language,
            self.status_message.as_deref(),
            self.mr_loading || self.mr_discovering,
            &self.mr_hosts.gitlab_hosts,
            self.mr_snapshot.as_ref().map(|s| s.fetched_at),
            now_unix(),
        )
    }

    /// Zeichnet die MR-Ansicht und wertet ihre Klicks aus.
    pub(super) fn show_mr_view(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        // K1: Hinweis aus dem Auschecken (überlebt Rescans)
        self.show_mr_notice(ui);
        let mut actions = MrViewActions::default();
        {
            let config = &self.config;
            let mru = |p: &Path| crate::ui::tray_popup::mru_timestamp(config, p);
            let exists = |p: &Path| p.exists();
            let repos = &self.repos;
            let ide_for = |p: &Path| super::mr_checkout::ide_name_for(config, repos, p);
            let agents = config.get_active_agents();
            let input = MrViewInput {
                lang: config.language,
                theme: &config.theme,
                cfg: &config.merge_requests,
                snapshot: self.mr_snapshot.as_deref(),
                uninvolved_loading: self.mr_more_pending > 0
                    || (self.mr_loading && self.mr_view.show_uninvolved),
                glab_error: self.mr_glab_error.as_ref(),
                discovering: self.scanning || self.mr_discovering,
                repos: &self.repos,
                hosts: &self.mr_hosts,
                locals: &self.mr_locals,
                agents: &agents,
                ide: config
                    .get_active_profile()
                    .visible_ides()
                    .into_iter()
                    .next(),
                ide_for: &ide_for,
                lookup: MenuLookup {
                    mru: &mru,
                    exists: &exists,
                },
                now: now_unix(),
            };
            show_merge_requests(ui, &input, &mut self.mr_view, &mut actions);
        }
        self.handle_mr_actions(&ctx, actions);
    }

    /// Klicks der MR-Ansicht; Auschecken und „Bereits ausgecheckt“ siehe
    /// `mr_checkout.rs`, den Einstellungs-Tab `mr_settings.rs`.
    fn handle_mr_actions(&mut self, ctx: &egui::Context, actions: MrViewActions) {
        if let Some(url) = actions.open_url {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        if let Some(choice) = actions.checkout {
            self.start_mr_checkout(choice);
        }
        if let Some((path, target)) = actions.open_existing {
            self.open_existing(&path, &target);
        }
        if actions.refresh {
            self.mr_user_refresh();
        } else if actions.fetch_uninvolved {
            // „Alle offenen“ eingeschaltet: Abruf mit „Nicht beteiligt“
            self.start_mr_refresh();
        }
        if actions.load_more {
            self.start_mr_load_more();
        }
        if actions.open_settings_mr_tab {
            self.open_settings_mr_tab();
        }
    }

    /// Deep-Link (glab fehlt, nicht angemeldet, kein GitLab-Host, alles
    /// abgewählt): Einstellungen direkt im Tab „Merge Requests“; ein offener
    /// Entwurf bleibt erhalten.
    fn open_settings_mr_tab(&mut self) {
        if !self.show_settings || self.settings_state.is_none() {
            self.settings_state = Some(SettingsState::from_config(&self.config));
        }
        self.show_settings = true;
        if let Some(state) = self.settings_state.as_mut() {
            state.select_merge_requests_tab();
        }
    }
}

/// glab fehlt oder ist zu alt → keine Erkennung (Karte statt „Kein
/// GitLab-Host“); sonst `discover_glab_hosts` (nur `config get`).
fn discover(
    runner: &dyn GlabRunner,
    candidates: &[String],
) -> (Vec<GlabHost>, Option<GlabErrorKind>) {
    match glab_version(runner) {
        Err(kind @ (GlabErrorKind::NotInstalled | GlabErrorKind::TooOld(_))) => {
            (Vec::new(), Some(kind))
        }
        _ => (discover_glab_hosts(runner, candidates), None),
    }
}

/// Kandidaten der lokalen glab-Erkennung (E16): Remote-Hosts ∪
/// `extra_hosts` ∪ Ziele aus `host_aliases` und den eingebauten Zuordnungen
/// der Remote-Hosts, kleingeschrieben, sortiert.
fn discovery_candidates(repos: &[RepoInfo], cfg: &MrConfig) -> Vec<String> {
    let remotes = repos
        .iter()
        .flat_map(|r| r.remotes.iter().filter_map(|x| x.parsed.as_ref()))
        .flat_map(|r| {
            let builtin = builtin_alias_target(&r.host).map(str::to_string);
            std::iter::once(r.host_key()).chain(builtin)
        });
    let configured = cfg
        .extra_hosts
        .iter()
        .chain(cfg.host_aliases.iter().map(|a| &a.to))
        .cloned();
    let keys: BTreeSet<String> = remotes
        .chain(configured)
        .map(|h| h.trim().to_lowercase())
        .filter(|h| !h.is_empty())
        .collect();
    keys.into_iter().collect()
}

/// Badge (E9): wartende Reviews; ausgeblendet bei 0, ohne Stand und nach
/// einem Anmeldefehler. Auch für den Tray-Button (E21).
pub(super) fn mr_badge(snapshot: Option<&MrSnapshot>) -> Option<usize> {
    let snap = snapshot?;
    let auth_failed = snap
        .per_host
        .iter()
        .any(|h| h.error.as_ref().is_some_and(is_auth_error));
    (snap.badge > 0 && !auth_failed).then_some(snap.badge)
}

/// „Merge Requests – {n} Reviews warten auf dich“ (Einzahl getrennt).
fn badge_tooltip(lang: Language, n: usize) -> String {
    if n == 1 {
        tr(lang, "mr_badge_tt_one")
    } else {
        tr_fmt(lang, "mr_badge_tt_many", &[&n.to_string()])
    }
}

/// Statusleiste der MR-Ansicht: eine Statusmeldung (3 s) hat Vorrang, sonst
/// „Merge Requests · {Hosts} · Stand: {Zeit}“ bzw. „Aktualisiere …“.
/// Liefert (Text, ist Statusmeldung).
fn mr_status_text(
    lang: Language,
    message: Option<&str>,
    loading: bool,
    hosts: &[String],
    fetched_at: Option<i64>,
    now: i64,
) -> (String, bool) {
    if let Some(msg) = message {
        return (msg.to_string(), true);
    }
    let stand = fetched_at
        .filter(|t| *t > 0)
        .map_or_else(|| "–".to_string(), |t| relative_time(t, now, lang));
    if loading {
        return (tr_fmt(lang, "mr_refreshing", &[&stand]), false);
    }
    let hosts = if hosts.is_empty() {
        "–".to_string()
    } else {
        hosts.join(", ")
    };
    (tr_fmt(lang, "mr_status_line", &[&hosts, &stand]), false)
}

/// Hängt eine Seite „Nicht beteiligt“ an: neue MRs per `id` dedupliziert,
/// Review-Anfragen nach „Von anderen“ (wie `build_snapshot`), Cursor des
/// Hosts ersetzt.
fn merge_more_page(snap: &mut MrSnapshot, page: UninvolvedPage) {
    let host = page.host_key.as_str();
    let mut known: HashSet<String> = snap
        .mine
        .iter()
        .chain(&snap.others)
        .chain(snap.uninvolved.iter().flat_map(|l| &l.mrs))
        .filter(|m| m.host_key.eq_ignore_ascii_case(host))
        .map(|m| m.id.clone())
        .collect();
    for mut mr in page.reviewing {
        if known.insert(mr.id.clone()) {
            if !mr.reasons.contains(&Reason::Reviewer) {
                mr.reasons.push(Reason::Reviewer);
                mr.reasons.sort();
            }
            snap.others.push(mr);
        }
    }
    sort_others(&mut snap.others);
    snap.badge = snap.others.iter().filter(|m| m.review_pending()).count();
    let list = snap.uninvolved.get_or_insert_with(Default::default);
    for mr in page.mrs {
        if known.insert(mr.id.clone()) {
            list.mrs.push(mr);
        }
    }
    sort_uninvolved(&mut list.mrs);
    list.more.retain(|c| !c.host_key.eq_ignore_ascii_case(host));
    list.more.extend(page.more);
    list.total = list.total.max(list.mrs.len());
}

/// Fehler beim Nachladen als Teilfehler „Nicht beteiligt“ des Hosts (Banner
/// in der Spalte, E20); die Cursor bleiben für einen neuen Versuch.
fn note_more_error(snap: &mut MrSnapshot, host_key: &str, kind: GlabErrorKind) {
    if let Some(status) = snap.per_host.iter_mut().find(|h| h.host_key == host_key) {
        let entry = (MrPart::Uninvolved, kind);
        if !status.partial.contains(&entry) {
            status.partial.push(entry);
        }
    }
}

/// Segment des Umschalters (h 20, r 6); aktiv in den Auswahlfarben.
fn segment(selected: bool, atoms: Atoms<'_>) -> Button<'_> {
    Button::selectable(selected, atoms)
        .corner_radius(6)
        .min_size(vec2(0.0, 20.0))
        .gap(6.0)
}

/// Button der Top-Leiste: Icon + Text, schmal nur das Icon (dann mit
/// AccessKit-Namen; den Tooltip setzt der Aufrufer).
pub(super) fn top_bar_button(
    ui: &mut Ui,
    icon: ImageSource<'static>,
    label: &str,
    compact: bool,
    enabled: bool,
) -> Response {
    let image = Image::new(icon)
        .fit_to_exact_size(vec2(14.0, 14.0))
        .tint(ui.visuals().text_color());
    if compact {
        let response = ui.add_enabled(enabled, Button::image(image));
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
        response
    } else {
        ui.add_enabled(
            enabled,
            Button::image_and_text(image, RichText::new(label).size(12.0)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{test_app, test_app_with_glab};
    use super::super::ScanResult;
    use super::*;
    use crate::gitlab::glab::fake::FakeGlab;
    use crate::gitlab::glab::{FakeReply, GatedRunner, GlabError, GlabRequest};
    use crate::gitlab::model::{HostStatus, MergeRequest, MyReview, UninvolvedList};
    use crate::gitlab::remote::RemoteInfo;
    use std::path::PathBuf;

    const HOST: &str = "gitlab.example.com";
    const HOST_B: &str = "git.intern.example";

    type Fake = Arc<GatedRunner<FakeGlab>>;

    fn fake_app() -> (MyApp, Fake) {
        let glab: Fake = Arc::new(GatedRunner::new(FakeGlab::new()));
        let app = test_app_with_glab(glab.clone());
        (app, glab)
    }

    /// GitLab-Host ohne Erkennung (eingetragen, E16).
    fn with_extra_host(app: &mut MyApp) {
        app.config.merge_requests.extra_hosts = vec![HOST.to_string()];
        app.update_mr_hosts();
    }

    fn repo(name: &str, url: &str) -> RepoInfo {
        let mut repo = RepoInfo::new(
            PathBuf::from(format!("/tmp/gm-mr-test/{name}")),
            "main".to_string(),
            false,
            false,
        );
        repo.remotes = vec![RemoteInfo::new("origin", url)];
        repo
    }

    fn signed_in(key: &str) -> GlabHost {
        GlabHost {
            key: key.to_string(),
            user: "taltmann".to_string(),
            ..GlabHost::default()
        }
    }

    fn is_version(r: &GlabRequest) -> bool {
        r.args.first().is_some_and(|a| a == "version")
    }

    fn version_reply() -> FakeReply {
        FakeReply::stdout(0, "glab 1.123.0 (a300719)\n", "")
    }

    fn not_installed() -> FakeReply {
        FakeReply::error(GlabError::NotFound)
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

    /// `glab api` an `host` läuft in den Timeout (beendet den Host im Abruf).
    fn push_api_timeout(fake: &FakeGlab, host: &'static str) {
        fake.push_when(
            move |r: &GlabRequest| {
                r.args.first().is_some_and(|a| a == "api") && r.host_key.as_deref() == Some(host)
            },
            FakeReply::error(GlabError::Timeout(Duration::from_secs(45))),
        );
    }

    /// Hosts im Stand, sortiert.
    fn fetched_hosts(app: &MyApp) -> Vec<String> {
        let mut hosts: Vec<String> = app
            .mr_snapshot
            .as_deref()
            .map(|s| s.per_host.iter().map(|h| h.host_key.clone()).collect())
            .unwrap_or_default();
        hosts.sort();
        hosts
    }

    /// Übernimmt Ergebnisse, bis kein MR-Worker mehr läuft (höchstens 5 s).
    fn settle(app: &mut MyApp) {
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            app.poll_mr(&ctx);
            if !app.mr_loading && !app.mr_discovering && app.mr_more_pending == 0 {
                return;
            }
            assert!(Instant::now() < until, "MR-Worker hängt");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn scan(app: &mut MyApp, repos: Vec<RepoInfo>) {
        app.scan_tx.send(ScanResult::Repos(repos)).expect("send");
        app.poll_scan(&egui::Context::default());
    }

    fn mr(n: u64) -> MergeRequest {
        MergeRequest {
            host_key: HOST.to_string(),
            id: format!("gid://gitlab/MergeRequest/{n}"),
            iid: n,
            title: format!("MR {n}"),
            project_path: "group/app".to_string(),
            updated_at: Some(1_000 + i64::try_from(n).unwrap()),
            ..MergeRequest::default()
        }
    }

    fn cursor(host: &str, path: &str, cursor: &str) -> UninvolvedCursor {
        UninvolvedCursor {
            host_key: host.to_string(),
            full_path: path.to_string(),
            cursor: cursor.to_string(),
        }
    }

    fn host_status(host: &str) -> HostStatus {
        HostStatus {
            host_key: host.to_string(),
            user: Some("taltmann".to_string()),
            ..HostStatus::default()
        }
    }

    fn loaded(epoch: u64, badge: usize) -> MrResult {
        let data = MrFetchResult {
            snapshot: MrSnapshot {
                badge,
                fetched_at: 100,
                ..MrSnapshot::default()
            },
            ..MrFetchResult::default()
        };
        MrResult::Loaded {
            epoch,
            data: Box::new(data),
        }
    }

    #[test]
    fn poll_mr_loaded_sets_snapshot_and_ends_loading() {
        let mut app = test_app();
        app.mr_loading = true;
        app.mr_tx.send(loaded(0, 2)).expect("send");
        app.poll_mr(&egui::Context::default());
        assert!(!app.mr_loading);
        assert_eq!(app.mr_snapshot.as_deref().map(|s| s.badge), Some(2));
        assert!(app.mr_last_refresh.is_some(), "Ende des Abrufs gemerkt");
        assert!(app.error.is_none());
    }

    /// Neuer Stand (auch nachgeladene Seiten) → Tray-Badge beim nächsten
    /// Sync aktualisieren (E21).
    #[test]
    fn new_snapshot_marks_tray_dirty() {
        let mut app = test_app();
        app.mr_loading = true;
        app.mr_tx.send(loaded(0, 2)).expect("send");
        app.poll_mr(&egui::Context::default());
        assert!(app.tray_dirty);
        app.tray_dirty = false;
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            uninvolved: Some(UninvolvedList::default()),
            per_host: vec![host_status(HOST)],
            ..MrSnapshot::default()
        }));
        app.mr_more_pending = 1;
        app.mr_tx
            .send(MrResult::MorePage {
                epoch: 0,
                host_key: HOST.to_string(),
                page: Ok(UninvolvedPage {
                    host_key: HOST.to_string(),
                    ..UninvolvedPage::default()
                }),
            })
            .expect("send");
        app.poll_mr(&egui::Context::default());
        assert!(app.tray_dirty);
        assert_eq!(mr_badge(app.mr_snapshot.as_deref()), None);
    }

    /// E21: Tray-Button „Merge Requests“ zeigt das Hauptfenster in der
    /// MR-Ansicht; neu geladen wird nur ohne Stand oder nach dem Intervall.
    #[test]
    fn tray_show_mrs_switches_view_and_refreshes_only_when_due() {
        let (mut app, glab) = fake_app();
        with_extra_host(&mut app);
        let ctx = egui::Context::default();
        app.mr_snapshot = Some(Box::default());
        app.mr_last_refresh = Some(Instant::now());
        app.window_visible = false;
        app.show_mrs_from_tray(&ctx);
        assert_eq!(app.main_view, MainView::MergeRequests);
        assert!(app.window_visible);
        assert!(!app.mr_loading, "innerhalb des Intervalls kein Abruf");
        assert!(glab.inner().take_calls().is_empty());
        // Intervall (3 min) abgelaufen → Refresh
        app.main_view = MainView::Repos;
        app.window_visible = false;
        app.mr_last_refresh = Instant::now().checked_sub(Duration::from_secs(4 * 60));
        glab.inner().push(not_installed());
        app.show_mrs_from_tray(&ctx);
        assert_eq!(app.main_view, MainView::MergeRequests);
        assert!(app.window_visible);
        assert!(app.mr_loading);
        settle(&mut app);
        assert_eq!(glab.inner().take_calls().len(), 1);
        // Ohne Stand lädt der erste Wechsel, auch im Intervall
        let (mut app, glab) = fake_app();
        with_extra_host(&mut app);
        app.mr_last_refresh = Some(Instant::now());
        glab.inner().push(not_installed());
        app.show_mrs_from_tray(&ctx);
        assert!(app.mr_loading);
        settle(&mut app);
        assert_eq!(glab.inner().take_calls().len(), 1);
        assert!(app.error.is_none());
    }

    /// E21: Tray „Hauptfenster“ zeigt die Repo-Liste, auch wenn zuletzt die
    /// MR-Ansicht offen war.
    #[test]
    fn tray_main_window_shows_repo_list() {
        let mut app = test_app();
        let ctx = egui::Context::default();
        app.main_view = MainView::MergeRequests;
        app.window_visible = false;
        app.show_main_from_tray(&ctx);
        assert_eq!(app.main_view, MainView::Repos);
        assert!(app.window_visible);
    }

    #[test]
    fn poll_mr_aborted_keeps_old_snapshot() {
        let mut app = test_app();
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            badge: 3,
            ..MrSnapshot::default()
        }));
        app.mr_loading = true;
        app.mr_tx
            .send(MrResult::Aborted {
                reason_key: "mr_fetch_aborted",
            })
            .expect("send");
        app.poll_mr(&egui::Context::default());
        assert!(!app.mr_loading);
        assert_eq!(app.mr_snapshot.as_deref().map(|s| s.badge), Some(3));
        // E20: nie im Banner, in der Repo-Ansicht auch keine Statusmeldung
        assert!(app.error.is_none());
        assert!(app.status_message.is_none());
    }

    #[test]
    fn refresh_while_loading_runs_afterwards() {
        let (mut app, glab) = fake_app();
        with_extra_host(&mut app);
        app.mr_loading = true;
        app.start_mr_refresh();
        assert!(app.mr_pending_refresh, "Wunsch gemerkt");
        assert!(glab.inner().take_calls().is_empty(), "kein zweiter Abruf");
        glab.inner().push(not_installed());
        app.mr_tx.send(loaded(0, 1)).expect("send");
        app.poll_mr(&egui::Context::default());
        assert_eq!(app.mr_snapshot.as_deref().map(|s| s.badge), Some(1));
        assert!(!app.mr_pending_refresh);
        assert!(app.mr_loading, "gemerkter Refresh läuft");
        settle(&mut app);
        assert_eq!(glab.inner().take_calls().len(), 1);
    }

    #[test]
    fn no_auto_refresh_while_window_hidden() {
        let (mut app, glab) = fake_app();
        with_extra_host(&mut app);
        let ctx = egui::Context::default();
        app.window_visible = false;
        app.maybe_refresh_mr(&ctx);
        assert!(!app.mr_loading);
        assert!(glab.inner().take_calls().is_empty());
        app.window_visible = true;
        glab.inner().push(not_installed());
        app.maybe_refresh_mr(&ctx);
        assert!(app.mr_loading, "sichtbar und fällig → Abruf");
        settle(&mut app);
    }

    #[test]
    fn no_gitlab_hosts_no_api_calls() {
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        fake.push_when(is_version, version_reply());
        fake.push_when(
            config_get("github.com", "user"),
            FakeReply::stdout(0, "\n", ""),
        );
        app.main_view = MainView::MergeRequests;
        scan(
            &mut app,
            vec![repo("app", "https://github.com/acme/app.git")],
        );
        assert!(app.mr_discovering, "lokale Erkennung nach dem Scan");
        settle(&mut app);
        assert!(app.mr_hosts.gitlab_hosts.is_empty());
        assert!(app.mr_snapshot.is_none());
        let calls = fake.take_calls();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|c| c.request.args[0] != "api"));
        // Umschalten, Intervall und erneuter Scan rufen nichts auf
        app.set_main_view(MainView::Repos);
        app.set_main_view(MainView::MergeRequests);
        app.maybe_refresh_mr(&egui::Context::default());
        scan(
            &mut app,
            vec![repo("app", "https://github.com/acme/app.git")],
        );
        assert!(!app.mr_loading && !app.mr_discovering);
        assert!(fake.take_calls().is_empty());
    }

    #[test]
    fn glab_not_installed_stays_in_mr_state() {
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        fake.push_when(is_version, not_installed());
        scan(
            &mut app,
            vec![repo("app", "git@gitlab.example.com:group/app.git")],
        );
        settle(&mut app);
        assert!(app.mr_glab_missing);
        assert_eq!(app.mr_glab_error, Some(GlabErrorKind::NotInstalled));
        // E20: kein Banner, kein Tray-Hinweis
        assert!(app.error.is_none());
        assert_eq!(
            crate::ui::tray_popup::tray_notice_text(app.config.language, None, app.error.is_some()),
            None
        );
        assert_eq!(fake.take_calls().len(), 1);
        // Keine weiteren glab-Aufrufe: neuer Remote-Host, Intervall, Umschalten
        app.mr_last_refresh = None;
        scan(
            &mut app,
            vec![
                repo("app", "git@gitlab.example.com:group/app.git"),
                repo("lib", "https://git.intern.example/group/lib.git"),
            ],
        );
        app.maybe_refresh_mr(&egui::Context::default());
        app.set_main_view(MainView::MergeRequests);
        app.start_mr_refresh();
        assert!(!app.mr_discovering && !app.mr_loading);
        assert!(fake.take_calls().is_empty());
    }

    #[test]
    fn glab_not_installed_from_fetch_stops_calls() {
        let (mut app, glab) = fake_app();
        with_extra_host(&mut app);
        glab.inner().push(not_installed());
        app.start_mr_refresh();
        settle(&mut app);
        assert!(app.mr_glab_missing);
        assert!(app.error.is_none());
        assert!(app.mr_snapshot.is_none());
        app.mr_last_refresh = None;
        app.maybe_refresh_mr(&egui::Context::default());
        assert!(!app.mr_loading);
        assert_eq!(glab.inner().take_calls().len(), 1);
        // „Aktualisieren“ versucht es nach einer Installation erneut
        glab.inner().push_when(is_version, not_installed());
        app.mr_user_refresh();
        assert!(app.mr_discovering);
        settle(&mut app);
        assert_eq!(glab.inner().take_calls().len(), 1);
    }

    #[test]
    fn scan_within_interval_does_not_refetch() {
        let (mut app, glab) = fake_app();
        app.glab_hosts = vec![signed_in(HOST)];
        app.mr_last_refresh = Some(Instant::now());
        let repos = vec![repo("app", "git@gitlab.example.com:group/app.git")];
        scan(&mut app, repos.clone());
        assert_eq!(app.mr_hosts.gitlab_hosts, vec![HOST.to_string()]);
        assert!(!app.mr_discovering && !app.mr_loading);
        assert!(glab.inner().take_calls().is_empty());
        // Intervall (3 min) abgelaufen → Abruf für das Badge
        app.mr_last_refresh = Instant::now().checked_sub(Duration::from_secs(4 * 60));
        glab.inner().push(not_installed());
        scan(&mut app, repos);
        assert!(app.mr_loading);
        settle(&mut app);
    }

    #[test]
    fn second_scan_without_new_hosts_skips_discovery() {
        let (mut app, glab) = fake_app();
        // Kein Abruf nach der Erkennung (Intervall aus, Repo-Ansicht)
        app.config.merge_requests.refresh_interval_min = 0;
        let fake = glab.inner();
        fake.push_when(is_version, version_reply());
        push_signed_in(fake, HOST);
        let repos = vec![repo("app", "git@gitlab.example.com:group/app.git")];
        scan(&mut app, repos.clone());
        settle(&mut app);
        assert_eq!(app.mr_hosts.gitlab_hosts, vec![HOST.to_string()]);
        assert_eq!(app.mr_locals.len(), 1);
        assert_eq!(fake.take_calls().len(), 4);
        scan(&mut app, repos);
        assert!(!app.mr_discovering);
        assert!(fake.take_calls().is_empty());
    }

    #[test]
    fn view_opened_during_discovery_loads_afterwards() {
        let (mut app, glab) = fake_app();
        app.config.merge_requests.refresh_interval_min = 0;
        let fake = glab.inner();
        fake.push_when(is_version, version_reply());
        push_signed_in(fake, HOST);
        // Abruf nach der Erkennung: glab bricht ab (kein Netz in Tests)
        fake.push_when(is_version, not_installed());
        scan(
            &mut app,
            vec![repo("app", "git@gitlab.example.com:group/app.git")],
        );
        app.set_main_view(MainView::MergeRequests);
        assert!(!app.mr_loading, "Hosts noch unbekannt");
        settle(&mut app);
        let calls = fake.take_calls();
        assert_eq!(calls.len(), 5, "Erkennung, dann Abruf");
        assert!(app.mr_glab_missing);
    }

    /// Intervall-Abruf und Umschalten während der Erkennung warten; der
    /// Abruf danach nimmt auch die neu erkannten Hosts mit.
    #[test]
    fn fetch_waits_for_discovery_and_includes_its_hosts() {
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        app.config.merge_requests.extra_hosts = vec![HOST.to_string()];
        fake.push_when(is_version, version_reply());
        fake.push_when(config_get(HOST, "user"), FakeReply::stdout(0, "\n", ""));
        push_signed_in(fake, HOST_B);
        scan(
            &mut app,
            vec![repo("lib", "https://git.intern.example/group/lib.git")],
        );
        assert!(app.mr_discovering);
        assert_eq!(app.mr_hosts.gitlab_hosts, vec![HOST.to_string()]);
        app.maybe_refresh_mr(&egui::Context::default());
        app.set_main_view(MainView::MergeRequests);
        assert!(!app.mr_loading, "wartet auf die Erkennung");
        fake.push_when(is_version, version_reply());
        push_api_timeout(fake, HOST);
        push_api_timeout(fake, HOST_B);
        settle(&mut app);
        assert_eq!(fetched_hosts(&app), [HOST_B, HOST]);
        assert_eq!(fake.pending(), 0);
    }

    /// Ein späterer Scan bringt einen angemeldeten Host: der vorhandene Stand
    /// wird sofort ergänzt, auch innerhalb des Intervalls.
    #[test]
    fn new_host_from_later_scan_reloads_within_interval() {
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        app.glab_hosts = vec![signed_in(HOST)];
        app.mr_last_refresh = Some(Instant::now());
        let first = repo("app", "git@gitlab.example.com:group/app.git");
        scan(&mut app, vec![first.clone()]);
        assert!(!app.mr_loading && !app.mr_discovering);
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            per_host: vec![host_status(HOST)],
            ..MrSnapshot::default()
        }));
        fake.push_when(is_version, version_reply());
        push_signed_in(fake, HOST_B);
        scan(
            &mut app,
            vec![
                first,
                repo("lib", "https://git.intern.example/group/lib.git"),
            ],
        );
        assert!(app.mr_discovering);
        fake.push_when(is_version, version_reply());
        push_api_timeout(fake, HOST);
        push_api_timeout(fake, HOST_B);
        settle(&mut app);
        assert_eq!(fetched_hosts(&app), [HOST_B, HOST]);
    }

    /// „Speichern“ während des Rescans lädt erst mit den neuen Repos (auch
    /// innerhalb des Intervalls), ohne laufenden Scan sofort.
    #[test]
    fn settings_save_during_scan_loads_after_rescan() {
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        app.glab_hosts = vec![signed_in(HOST)];
        app.mr_last_refresh = Some(Instant::now());
        app.scanning = true;
        app.mr_after_settings_save();
        assert!(!app.mr_loading && !app.mr_pending_refresh);
        assert!(app.mr_refresh_after_scan);
        fake.push_when(is_version, version_reply());
        push_api_timeout(fake, HOST);
        scan(
            &mut app,
            vec![repo("app", "git@gitlab.example.com:group/app.git")],
        );
        assert!(app.mr_loading, "Abruf nach dem Rescan");
        assert!(!app.mr_refresh_after_scan);
        assert_eq!(app.mr_locals.len(), 1, "mit den neuen Repos");
        settle(&mut app);
        assert_eq!(fetched_hosts(&app), [HOST]);
        // Ohne laufenden Scan sofort
        fake.push_when(is_version, version_reply());
        push_api_timeout(fake, HOST);
        app.mr_after_settings_save();
        assert!(app.mr_loading);
        settle(&mut app);
        assert_eq!(fake.pending(), 0);
    }

    /// „Aktualisieren“ prüft nicht angemeldete Kandidaten erneut lokal
    /// (Anmeldung mit glab im Terminal) und lädt danach.
    #[test]
    fn user_refresh_rechecks_signed_out_candidates() {
        let (mut app, glab) = fake_app();
        let fake = glab.inner();
        app.glab_hosts = vec![GlabHost {
            key: HOST.to_string(),
            ..GlabHost::default()
        }];
        app.repos = vec![repo("app", "git@gitlab.example.com:group/app.git")];
        app.update_mr_hosts();
        assert!(app.mr_hosts.gitlab_hosts.is_empty());
        fake.push_when(is_version, version_reply());
        push_signed_in(fake, HOST);
        app.mr_user_refresh();
        assert!(app.mr_discovering);
        fake.push_when(is_version, version_reply());
        push_api_timeout(fake, HOST);
        settle(&mut app);
        assert_eq!(app.mr_hosts.gitlab_hosts, vec![HOST.to_string()]);
        assert_eq!(fetched_hosts(&app), [HOST]);
    }

    #[test]
    fn first_switch_to_view_loads_once() {
        let (mut app, glab) = fake_app();
        with_extra_host(&mut app);
        app.mr_snapshot = Some(Box::default());
        app.set_main_view(MainView::MergeRequests);
        assert!(!app.mr_loading, "Stand vorhanden");
        app.set_main_view(MainView::Repos);
        app.mr_snapshot = None;
        glab.inner().push(not_installed());
        app.set_main_view(MainView::MergeRequests);
        assert!(app.mr_loading, "Wechsel ohne Stand lädt");
        settle(&mut app);
        assert_eq!(glab.inner().take_calls().len(), 1);
    }

    #[test]
    fn loaded_with_old_epoch_is_discarded_and_restarted() {
        let (mut app, glab) = fake_app();
        with_extra_host(&mut app);
        app.mr_loading = true;
        app.bump_mr_auth_epoch();
        assert_eq!(app.mr_auth_epoch, 1);
        assert!(app.mr_pending_refresh, "läuft schon → gemerkt");
        glab.inner().push(not_installed());
        app.mr_tx.send(loaded(0, 5)).expect("send");
        app.poll_mr(&egui::Context::default());
        assert!(app.mr_snapshot.is_none(), "alte Epoche verworfen");
        assert!(app.mr_loading, "Neustart");
        assert!(!app.mr_pending_refresh);
        settle(&mut app);
    }

    /// Laufende Seiten „Nicht beteiligt“ der alten Anmeldung zählen weiter;
    /// der Refresh startet erst, wenn die letzte gemeldet hat.
    #[test]
    fn bump_waits_for_running_pages() {
        let (mut app, glab) = fake_app();
        with_extra_host(&mut app);
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            uninvolved: Some(UninvolvedList::default()),
            ..MrSnapshot::default()
        }));
        app.mr_more_pending = 1;
        app.bump_mr_auth_epoch();
        assert_eq!(app.mr_more_pending, 1);
        assert!(!app.mr_loading && app.mr_pending_refresh);
        app.mr_tx
            .send(MrResult::MorePage {
                epoch: 0,
                host_key: HOST.to_string(),
                page: Ok(UninvolvedPage {
                    host_key: HOST.to_string(),
                    mrs: vec![mr(7)],
                    ..UninvolvedPage::default()
                }),
            })
            .expect("send");
        glab.inner().push(not_installed());
        app.poll_mr(&egui::Context::default());
        assert_eq!(app.mr_more_pending, 0);
        let list = app
            .mr_snapshot
            .as_deref()
            .and_then(|s| s.uninvolved.as_ref());
        assert!(
            list.is_some_and(|l| l.mrs.is_empty()),
            "alte Seite verworfen"
        );
        assert!(app.mr_loading, "Refresh nach der letzten alten Seite");
        settle(&mut app);
    }

    #[test]
    fn more_page_appends_deduplicated_by_id() {
        let mut app = test_app();
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            uninvolved: Some(UninvolvedList {
                mrs: vec![mr(2), mr(1)],
                total: 5,
                more: vec![cursor(HOST, "group/app", "c1"), cursor(HOST_B, "x/y", "c9")],
            }),
            per_host: vec![host_status(HOST), host_status(HOST_B)],
            ..MrSnapshot::default()
        }));
        app.mr_more_pending = 2;
        let mut reviewing = mr(4);
        reviewing.my_review = Some(MyReview::Pending);
        let page = UninvolvedPage {
            host_key: HOST.to_string(),
            mrs: vec![mr(2), mr(3)],
            reviewing: vec![reviewing],
            more: vec![cursor(HOST, "group/app", "c2")],
        };
        app.mr_tx
            .send(MrResult::MorePage {
                epoch: 0,
                host_key: HOST.to_string(),
                page: Ok(page),
            })
            .expect("send");
        app.mr_tx
            .send(MrResult::MorePage {
                epoch: 0,
                host_key: HOST_B.to_string(),
                page: Err(GlabErrorKind::Timeout),
            })
            .expect("send");
        app.poll_mr(&egui::Context::default());
        assert_eq!(app.mr_more_pending, 0);
        let snap = app.mr_snapshot.as_deref().expect("Stand");
        let list = snap.uninvolved.as_ref().expect("Liste");
        let iids: Vec<u64> = list.mrs.iter().map(|m| m.iid).collect();
        assert_eq!(iids, vec![3, 2, 1]);
        assert_eq!(list.total, 5);
        assert_eq!(
            list.more,
            vec![cursor(HOST_B, "x/y", "c9"), cursor(HOST, "group/app", "c2")]
        );
        // Review-Anfrage jenseits der ersten Seite → Von anderen, Badge
        assert_eq!(snap.others.len(), 1);
        assert_eq!(snap.others[0].reasons, vec![Reason::Reviewer]);
        assert_eq!(snap.badge, 1);
        // Fehler als Teilfehler des Hosts, Cursor bleiben
        assert_eq!(
            snap.per_host[1].partial,
            vec![(MrPart::Uninvolved, GlabErrorKind::Timeout)]
        );
        assert!(app.error.is_none());
    }

    #[test]
    fn more_page_with_old_epoch_is_dropped() {
        let mut app = test_app();
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            uninvolved: Some(UninvolvedList::default()),
            ..MrSnapshot::default()
        }));
        app.mr_auth_epoch = 2;
        app.mr_tx
            .send(MrResult::MorePage {
                epoch: 1,
                host_key: HOST.to_string(),
                page: Ok(UninvolvedPage {
                    host_key: HOST.to_string(),
                    mrs: vec![mr(7)],
                    ..UninvolvedPage::default()
                }),
            })
            .expect("send");
        app.poll_mr(&egui::Context::default());
        let list = app
            .mr_snapshot
            .as_deref()
            .and_then(|s| s.uninvolved.as_ref());
        assert!(list.is_some_and(|l| l.mrs.is_empty()));
    }

    #[test]
    fn load_more_action_fetches_next_page_per_host() {
        let (mut app, glab) = fake_app();
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            uninvolved: Some(UninvolvedList {
                more: vec![cursor(HOST, "group/app", "c1")],
                ..UninvolvedList::default()
            }),
            per_host: vec![host_status(HOST)],
            ..MrSnapshot::default()
        }));
        glab.inner()
            .push(FakeReply::error(GlabError::Timeout(Duration::from_secs(
                45,
            ))));
        let actions = MrViewActions {
            load_more: true,
            ..MrViewActions::default()
        };
        app.handle_mr_actions(&egui::Context::default(), actions);
        assert_eq!(app.mr_more_pending, 1);
        settle(&mut app);
        let snap = app.mr_snapshot.as_deref().expect("Stand");
        assert_eq!(
            snap.per_host[0].partial,
            vec![(MrPart::Uninvolved, GlabErrorKind::Timeout)]
        );
        assert_eq!(snap.uninvolved.as_ref().map(|l| l.more.len()), Some(1));
        let calls = glab.inner().take_calls();
        assert!(!calls.is_empty());
        assert!(calls.iter().all(|c| c.request.args[0] == "api"));
    }

    /// K1: Auschecken und „Bereits ausgecheckt“ aus der Ansicht landen bei
    /// `start_mr_checkout` bzw. `open_existing`.
    #[test]
    fn checkout_and_open_existing_actions_are_handled() {
        use crate::config::AfterAction;
        use crate::ui::merge_requests::{MrCheckoutChoice, MrCheckoutKind, OpenTarget};
        let mut app = test_app();
        app.main_view = MainView::MergeRequests;
        // Läuft ein Scan, wird das Auschecken mit Hinweis abgelehnt
        app.scanning = true;
        let path = PathBuf::from("/tmp/gm-mr-test/app");
        let actions = MrViewActions {
            checkout: Some(MrCheckoutChoice {
                host_key: HOST.to_string(),
                mr_id: "gid://gitlab/MergeRequest/1".to_string(),
                iid: 1,
                kind: MrCheckoutKind::Switch {
                    clone: path.clone(),
                },
                after: AfterAction::Shell,
            }),
            open_existing: Some((path, OpenTarget::ShowInRepoList)),
            ..MrViewActions::default()
        };
        app.handle_mr_actions(&egui::Context::default(), actions);
        assert_eq!(
            app.status_message,
            Some(tr(app.config.language, "mr_checkout_blocked"))
        );
        assert_eq!(
            app.config.merge_requests.after_checkout_last,
            AfterAction::Shell
        );
        assert_eq!(app.main_view, MainView::Repos);
    }

    #[test]
    fn settings_action_opens_settings() {
        let mut app = test_app();
        let actions = MrViewActions {
            open_settings_mr_tab: true,
            ..MrViewActions::default()
        };
        app.handle_mr_actions(&egui::Context::default(), actions.clone());
        assert!(app.show_settings);
        let state = app.settings_state.as_ref().expect("Einstellungen offen");
        assert!(state.shows_merge_requests_tab(), "Deep-Link in den Tab");
        // Offener Entwurf bleibt erhalten, nur der Tab wechselt
        app.settings_state = Some(SettingsState::from_config(&app.config));
        if let Some(state) = app.settings_state.as_mut() {
            state.draft.merge_requests.refresh_interval_min = 42;
        }
        app.handle_mr_actions(&egui::Context::default(), actions);
        let state = app.settings_state.as_ref().expect("Einstellungen offen");
        assert!(state.shows_merge_requests_tab());
        assert_eq!(state.draft.merge_requests.refresh_interval_min, 42);
    }

    #[test]
    fn show_mr_view_renders_without_actions() {
        let mut app = test_app();
        app.glab_hosts = vec![signed_in(HOST)];
        app.repos = vec![repo("app", "git@gitlab.example.com:group/app.git")];
        app.update_mr_hosts();
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx, &app.config.theme);
        let snapshots = [
            None,
            Some(MrSnapshot {
                mine: vec![mr(1)],
                others: vec![mr(2)],
                per_host: vec![host_status(HOST)],
                ..MrSnapshot::default()
            }),
        ];
        for snapshot in snapshots {
            app.mr_snapshot = snapshot.map(Box::new);
            for _ in 0..2 {
                let raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        vec2(1200.0, 800.0),
                    )),
                    ..egui::RawInput::default()
                };
                let output = ctx.run_ui(raw, |ui| app.show_mr_view(ui));
                output.drop_without_applying_deltas();
            }
        }
        assert!(!app.show_settings && !app.mr_loading && app.mr_more_pending == 0);
    }

    #[test]
    fn status_message_takes_precedence_in_mr_status_line() {
        let mut app = test_app();
        app.config.language = Language::De;
        app.main_view = MainView::MergeRequests;
        app.mr_hosts.gitlab_hosts = vec![HOST.to_string(), HOST_B.to_string()];
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            fetched_at: now_unix() - 90,
            ..MrSnapshot::default()
        }));
        assert_eq!(
            app.mr_status_line(),
            (
                "Merge Requests · gitlab.example.com, git.intern.example · Stand: vor 1 Min."
                    .to_string(),
                false
            )
        );
        app.mr_loading = true;
        assert_eq!(app.mr_status_line().0, "Aktualisiere … · Stand: vor 1 Min.");
        app.status_message = Some("Gespeichert".to_string());
        assert_eq!(app.mr_status_line(), ("Gespeichert".to_string(), true));
        // Ohne Stand und ohne Hosts
        assert_eq!(
            mr_status_text(Language::En, None, false, &[], None, 0).0,
            "Merge requests · – · updated –"
        );
    }

    #[test]
    fn badge_hidden_at_zero_and_after_auth_error() {
        let snap = |badge, error| MrSnapshot {
            badge,
            per_host: vec![HostStatus {
                host_key: HOST.to_string(),
                error,
                ..HostStatus::default()
            }],
            ..MrSnapshot::default()
        };
        assert_eq!(mr_badge(None), None);
        assert_eq!(mr_badge(Some(&snap(0, None))), None);
        assert_eq!(mr_badge(Some(&snap(2, None))), Some(2));
        assert_eq!(
            mr_badge(Some(&snap(2, Some(GlabErrorKind::NotAuthenticated)))),
            None
        );
        assert_eq!(
            mr_badge(Some(&snap(2, Some(GlabErrorKind::Timeout)))),
            Some(2)
        );
        assert_eq!(
            badge_tooltip(Language::De, 1),
            "Merge Requests – 1 Review wartet auf dich"
        );
        assert_eq!(
            badge_tooltip(Language::De, 3),
            "Merge Requests – 3 Reviews warten auf dich"
        );
        assert_eq!(
            badge_tooltip(Language::En, 1),
            "Merge requests – 1 review waiting for you"
        );
    }

    #[test]
    fn discovery_candidates_from_remotes_and_config() {
        let repos = vec![
            repo("a", "git@GitLab.Example.com:group/a.git"),
            repo("b", "https://github.com/acme/b.git"),
            // SSH-Port gehört nicht zum Host-Schlüssel, HTTPS-Port schon
            repo("c", "https://gitlab.example.com:2222/group/c.git"),
            repo("e", "ssh://git@gitlab.example.com:2200/group/e.git"),
            repo("d", "git@gitlab.example.com:group/d.git"),
            // GitLab.com-SSH über Port 443: gitlab.com mit prüfen
            repo("f", "ssh://git@altssh.gitlab.com:443/group/f.git"),
        ];
        let mut cfg = MrConfig::default();
        cfg.extra_hosts = vec![" Git.Intern.Example ".to_string()];
        cfg.host_aliases = vec![crate::config::HostAlias {
            from: "ssh.example.com".to_string(),
            to: "gitlab.example.com:8443".to_string(),
        }];
        assert_eq!(
            discovery_candidates(&repos, &cfg),
            vec![
                "altssh.gitlab.com",
                "git.intern.example",
                "github.com",
                "gitlab.com",
                "gitlab.example.com",
                "gitlab.example.com:2222",
                "gitlab.example.com:8443",
            ]
        );
    }

    /// Bedienelemente der Top-Leiste (AccessKit: Name, Rechteck), Zeile 1.
    fn top_bar_widgets(app: &mut MyApp, width: f32) -> Vec<(String, egui::Rect)> {
        use egui::accesskit::Role;
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx, &app.config.theme);
        // Echte Icon-Größen (SVG), sonst messen Icon-Buttons zu schmal
        egui_extras::install_image_loaders(&ctx);
        ctx.enable_accesskit();
        let mut widgets = Vec::new();
        // Mehrere Durchläufe: egui misst im ersten
        for _ in 0..3 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(width, 680.0),
                )),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| app.show_top_bar(ui));
            if let Some(update) = &output.platform_output.accesskit_update {
                widgets = update
                    .nodes
                    .iter()
                    .filter(|(_, n)| {
                        matches!(
                            n.role(),
                            Role::Button
                                | Role::ComboBox
                                | Role::Label
                                | Role::Link
                                | Role::Image
                                | Role::ProgressIndicator
                        )
                    })
                    .filter_map(|(_, n)| {
                        let b = n.bounds()?;
                        let name = n.label().or(n.value()).unwrap_or_default().to_string();
                        let rect = egui::Rect::from_min_max(
                            egui::pos2(b.x0 as f32, b.y0 as f32),
                            egui::pos2(b.x1 as f32, b.y1 as f32),
                        );
                        Some((name, rect))
                    })
                    .collect();
            }
            output.drop_without_applying_deltas();
        }
        widgets
    }

    /// Mindestfenster (920 px, nur Icons, ohne Beschriftungen), ab
    /// `TOP_BAR_CAPTIONS_WIDTH` mit Beschriftungen, 1080 px und ab
    /// `TOP_BAR_COMPACT_WIDTH` (Standardfenster) mit Button-Texten: in
    /// Zeile 1 überlappt nichts, alles bleibt im Fenster.
    #[test]
    fn top_bar_fits_at_min_width() {
        for width in [920.0, TOP_BAR_CAPTIONS_WIDTH, 1080.0, TOP_BAR_COMPACT_WIDTH] {
            for lang in [Language::De, Language::En] {
                for view in [MainView::Repos, MainView::MergeRequests] {
                    let case = format!("{width} {lang:?} {view:?}");
                    let mut app = test_app();
                    app.config.language = lang;
                    app.config.roots = vec![PathBuf::from("C:\\Dev")];
                    assert!(app.config.agents.len() > 1, "AI-Auswahl als Combo");
                    app.main_view = view;
                    // Spinner in beiden Ansichten, Badge zweistellig
                    app.scanning = true;
                    app.mr_loading = true;
                    app.mr_snapshot = Some(Box::new(MrSnapshot {
                        badge: 12,
                        ..MrSnapshot::default()
                    }));
                    app.repos = (0..120)
                        .map(|i| repo(&format!("r{i}"), "https://github.com/a/b.git"))
                        .collect();
                    app.last_window_size = [width, 680.0];
                    let row = assert_top_bar_row_fits(&mut app, width, &case);
                    let caption = tr(lang, "profile_label");
                    assert_eq!(
                        row.iter().any(|name| *name == caption),
                        width >= TOP_BAR_CAPTIONS_WIDTH,
                        "{case}: Beschriftung {caption} {row:?}"
                    );
                }
            }
        }
    }

    /// Prüft Zeile 1 und liefert die Namen ihrer Bedienelemente.
    fn assert_top_bar_row_fits(app: &mut MyApp, width: f32, case: &str) -> Vec<String> {
        let widgets = top_bar_widgets(app, width);
        let title = widgets
            .iter()
            .find(|(name, _)| name == "GitManager")
            .map(|(_, r)| *r)
            .expect("Titel");
        // Zeile 1: alles auf Höhe des Titels
        let row: Vec<&(String, egui::Rect)> = widgets
            .iter()
            .filter(|(_, r)| {
                r.center().y > title.top() - 4.0 && r.center().y < title.bottom() + 4.0
            })
            .collect();
        assert!(row.len() >= 8, "{case}: {row:?}");
        for (i, (a, ra)) in row.iter().enumerate() {
            assert!(
                ra.left() >= 0.0 && ra.right() <= width,
                "{case}: {a} außerhalb {ra:?}"
            );
            for (b, rb) in row.iter().skip(i + 1) {
                let overlap = ra.intersect(*rb);
                let contains = ra.contains_rect(*rb) || rb.contains_rect(*ra);
                assert!(
                    contains || overlap.width() <= 0.5 || overlap.height() <= 0.5,
                    "{case}: {a} {ra:?} überlappt {b} {rb:?}\n{row:#?}"
                );
            }
        }
        row.iter().map(|(name, _)| name.clone()).collect()
    }
}
