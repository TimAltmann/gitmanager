//! Ansicht „Claude Sessions“: Zustand, Hintergrund-Scan, Live-Status der
//! laufenden Instanzen, Fortsetzen und Terminal-Fokus.

use super::{MainView, MyApp};
use crate::claude_live::{
    build_open, claude_live_dir, focus_session_window, process_alive, read_live_sessions,
    LiveSession, LiveStatus,
};
use crate::claude_sessions::{
    claude_projects_dir, resume_profile, scan_sessions, SessionCache, SessionScan,
};
use crate::git::launch_agent;
use crate::i18n::{tr, tr_fmt};
use crate::ui::claude_sessions::{show_claude_sessions, ClaudeSessionActions, ViewData};
use egui::Ui;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Live-Status neu lesen: sichtbar jede Sekunde, sonst alle 5 s (Kennzahl
/// im Umschalter).
const LIVE_POLL_VISIBLE: Duration = Duration::from_secs(1);
const LIVE_POLL_HIDDEN: Duration = Duration::from_secs(5);
/// Frühestens so oft neu scannen, wenn sich die offenen Sessions ändern
const RESCAN_ON_LIVE_CHANGE: Duration = Duration::from_secs(5);

/// Zustand der Ansicht (nicht gespeichert).
pub(super) struct ClaudeViewState {
    pub(super) scan: SessionScan,
    pub(super) scanning: bool,
    /// Ansicht wurde einmal geöffnet: ab dann folgt jedem Repo-Scan ein
    /// Session-Scan, und der Live-Status wird abgefragt
    pub(super) scanned_once: bool,
    /// Zählt gestartete Scans; ein Ergebnis gilt nur für den neuesten
    pub(super) scan_generation: u64,
    last_scan_start: Option<Instant>,
    pub(super) search: String,
    cache: Arc<Mutex<SessionCache>>,
    tx: Sender<(u64, SessionScan)>,
    rx: Receiver<(u64, SessionScan)>,
    /// Laufende Instanzen (sortiert: wartet, arbeitet, fertig)
    pub(super) live: Vec<LiveSession>,
    pub(super) live_inflight: bool,
    pub(super) last_live_poll: Option<Instant>,
    live_tx: Sender<Vec<LiveSession>>,
    live_rx: Receiver<Vec<LiveSession>>,
}

impl ClaudeViewState {
    pub(super) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (live_tx, live_rx) = mpsc::channel();
        Self {
            scan: SessionScan::default(),
            scanning: false,
            scanned_once: false,
            scan_generation: 0,
            last_scan_start: None,
            search: String::new(),
            cache: Arc::default(),
            tx,
            rx,
            live: Vec::new(),
            live_inflight: false,
            last_live_poll: None,
            live_tx,
            live_rx,
        }
    }

    /// Sessions in Repos plus offene außerhalb davon (für den Umschalter).
    pub(super) fn session_count(&self) -> usize {
        let in_repos: HashSet<&str> = self
            .scan
            .entries
            .iter()
            .map(|e| e.session.id.as_str())
            .collect();
        let extra = self
            .live
            .iter()
            .filter(|l| !in_repos.contains(l.session_id.as_str()))
            .count();
        self.scan.entries.len() + extra
    }

    /// Offene Sessions, die auf eine Antwort warten.
    pub(super) fn waiting_count(&self) -> usize {
        self.live
            .iter()
            .filter(|l| l.status == LiveStatus::Waiting)
            .count()
    }
}

impl MyApp {
    /// Startet einen Session-Scan im Hintergrund über die aktuellen Repos.
    pub(super) fn start_claude_scan(&mut self) {
        self.claude.scanning = true;
        self.claude.scan_generation += 1;
        self.claude.last_scan_start = Some(Instant::now());
        let generation = self.claude.scan_generation;
        let repos = self.repos.clone();
        let cache = Arc::clone(&self.claude.cache);
        let tx = self.claude.tx.clone();
        std::thread::spawn(move || {
            let scan = match claude_projects_dir() {
                Some(dir) => {
                    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
                    scan_sessions(&dir, &repos, &mut cache)
                }
                None => SessionScan::default(),
            };
            let _ = tx.send((generation, scan));
        });
    }

    /// Erster Aufruf der Ansicht: einmal scannen.
    pub(super) fn ensure_claude_scan(&mut self) {
        if !self.claude.scanned_once {
            self.claude.scanned_once = true;
            self.start_claude_scan();
        }
    }

    /// Nach einem Repo-Scan: Sessions neu zuordnen, sobald die Ansicht
    /// einmal benutzt wurde.
    pub(super) fn claude_after_scan(&mut self) {
        if self.claude.scanned_once {
            self.start_claude_scan();
        }
    }

    pub(super) fn apply_claude_result(&mut self, scan: SessionScan) {
        self.claude.scan = scan;
        self.claude.scanning = false;
    }

    /// Ist eine neue Live-Abfrage fällig?
    pub(super) fn live_poll_due(&self, now: Instant) -> bool {
        if self.claude.live_inflight || !self.claude.scanned_once {
            return false;
        }
        let interval = if self.main_view == MainView::ClaudeSessions {
            LIVE_POLL_VISIBLE
        } else {
            LIVE_POLL_HIDDEN
        };
        self.claude
            .last_live_poll
            .is_none_or(|last| now.duration_since(last) >= interval)
    }

    fn start_live_poll(&mut self) {
        self.claude.live_inflight = true;
        self.claude.last_live_poll = Some(Instant::now());
        let tx = self.claude.live_tx.clone();
        std::thread::spawn(move || {
            let live = claude_live_dir()
                .map(|dir| read_live_sessions(&dir, process_alive))
                .unwrap_or_default();
            let _ = tx.send(live);
        });
    }

    /// Neuer Live-Stand. Kommt eine Session dazu oder fällt weg, wird
    /// (gedrosselt) neu gescannt: Titel neuer Sessions, geschlossene wandern
    /// in die Liste.
    pub(super) fn apply_live_result(&mut self, live: Vec<LiveSession>) {
        self.claude.live_inflight = false;
        let ids = |v: &[LiveSession]| -> HashSet<String> {
            v.iter().map(|l| l.session_id.clone()).collect()
        };
        let changed = ids(&live) != ids(&self.claude.live);
        self.claude.live = live;
        let throttled = self
            .claude
            .last_scan_start
            .is_some_and(|t| t.elapsed() < RESCAN_ON_LIVE_CHANGE);
        if changed && !self.claude.scanning && !throttled {
            self.start_claude_scan();
        }
    }

    /// Holt fertige Scans und Live-Stände ab und startet fällige Abfragen.
    pub(super) fn poll_claude(&mut self, ctx: &egui::Context) {
        while let Ok((generation, scan)) = self.claude.rx.try_recv() {
            if generation == self.claude.scan_generation {
                self.apply_claude_result(scan);
            }
        }
        while let Ok(live) = self.claude.live_rx.try_recv() {
            self.apply_live_result(live);
        }
        if self.live_poll_due(Instant::now()) {
            self.start_live_poll();
        }
        if self.claude.scanning {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if self.claude.scanned_once {
            let interval = if self.main_view == MainView::ClaudeSessions {
                LIVE_POLL_VISIBLE
            } else {
                LIVE_POLL_HIDDEN
            };
            ctx.request_repaint_after(interval);
        }
    }

    pub(super) fn show_claude_view(&mut self, ui: &mut Ui) {
        let mut actions = ClaudeSessionActions::default();
        let open = build_open(&self.claude.live, &self.claude.scan);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let data = ViewData {
            open: &open,
            entries: &self.claude.scan.entries,
            scanning: self.claude.scanning,
            lang: self.config.language,
            now_ms,
        };
        show_claude_sessions(ui, &data, &mut self.claude.search, &mut actions);
        self.apply_claude_actions(actions);
    }

    /// Klicks der Ansicht ausführen.
    pub(super) fn apply_claude_actions(&mut self, actions: ClaudeSessionActions) {
        if let Some((cwd, id)) = actions.resume {
            self.resume_claude_session(&cwd, &id);
        }
        if let Some(pid) = actions.focus {
            // Im UI-Thread: nur der Vordergrund-Prozess darf den Fokus abgeben
            if !focus_session_window(pid) {
                self.status_message = Some(tr(self.config.language, "claude_focus_failed"));
                self.status_message_time = Some(Instant::now());
            }
        }
    }

    /// Statuszeile der Ansicht (ohne Statusmeldung).
    pub(super) fn claude_status_line(&self) -> String {
        let lang = self.config.language;
        if self.claude.scanning && self.claude.scan.entries.is_empty() {
            return tr(lang, "claude_scanning");
        }
        tr_fmt(
            lang,
            "claude_status",
            &[
                &self.claude.live.len().to_string(),
                &self.claude.session_count().to_string(),
            ],
        )
    }

    /// `claude --resume <id>` in `cwd` im Terminal (wie `open_agent`).
    pub(super) fn resume_claude_session(&mut self, cwd: &str, id: &str) {
        let lang = self.config.language;
        // Gezielt das Claude-Profil: der aktive Agent kann ein anderer sein
        let claude = self.config.agents.iter().find(|a| a.id == "claude");
        let profile = resume_profile(claude, id);
        let term_pref = profile
            .terminal_override
            .clone()
            .unwrap_or_else(|| self.config.terminal.preference.clone());
        self.status_message = Some(tr_fmt(
            lang,
            "starting_agent_in",
            &[&profile.display_name, cwd],
        ));
        self.status_message_time = Some(Instant::now());
        let path = PathBuf::from(cwd);
        let err_tx = self.launch_err_tx.clone();
        std::thread::spawn(move || {
            if let Err(e) = launch_agent(&profile, &path, &term_pref) {
                let _ = err_tx.send(format!(
                    "Agent '{}' konnte nicht gestartet werden: {e:#}",
                    profile.display_name
                ));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_app;
    use super::super::{MainView, ScanResult};
    use super::*;
    use crate::claude_sessions::{ClaudeSession, SessionEntry};

    fn entry(id: &str) -> SessionEntry {
        SessionEntry {
            session: ClaudeSession {
                id: id.into(),
                cwd: "C:\\a".into(),
                title: "t".into(),
                last_prompt: None,
                git_branch: None,
                last_activity: 1,
            },
            repo_path: PathBuf::from("C:\\a"),
            repo_name: "a".to_string(),
            is_worktree: false,
        }
    }

    fn live(id: &str, status: LiveStatus) -> LiveSession {
        LiveSession {
            pid: 1,
            session_id: id.into(),
            cwd: "C:\\a".into(),
            status,
            status_since_ms: 0,
            name: None,
            proc_start: None,
        }
    }

    #[test]
    fn claude_view_first_open_starts_scan() {
        let mut app = test_app();
        app.main_view = MainView::ClaudeSessions;
        app.ensure_claude_scan();
        assert!(app.claude.scanning);
        assert!(app.claude.scanned_once);
        assert_eq!(app.claude.scan_generation, 1);
        app.ensure_claude_scan();
        assert_eq!(app.claude.scan_generation, 1);
    }

    #[test]
    fn switching_to_claude_view_starts_scan_once() {
        let mut app = test_app();
        app.set_main_view(MainView::ClaudeSessions);
        assert_eq!(app.main_view, MainView::ClaudeSessions);
        assert_eq!(app.claude.scan_generation, 1);
        app.set_main_view(MainView::Repos);
        app.set_main_view(MainView::ClaudeSessions);
        assert_eq!(app.claude.scan_generation, 1);
    }

    #[test]
    fn claude_result_replaces_sessions() {
        let mut app = test_app();
        app.claude.scanning = true;
        app.apply_claude_result(SessionScan {
            entries: vec![entry("a")],
            ..SessionScan::default()
        });
        assert_eq!(app.claude.scan.entries.len(), 1);
        assert!(!app.claude.scanning);
    }

    #[test]
    fn repo_scan_rescans_claude_only_after_first_open() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        app.scan_tx.send(ScanResult::Repos(vec![])).unwrap();
        app.poll_scan(&ctx);
        assert_eq!(app.claude.scan_generation, 0);

        app.ensure_claude_scan();
        app.scan_tx.send(ScanResult::Repos(vec![])).unwrap();
        app.poll_scan(&ctx);
        assert_eq!(app.claude.scan_generation, 2);
    }

    #[test]
    fn session_count_adds_open_sessions_outside_repos() {
        let mut app = test_app();
        app.apply_claude_result(SessionScan {
            entries: vec![entry("a"), entry("b")],
            ..SessionScan::default()
        });
        app.claude.live = vec![live("a", LiveStatus::Busy), live("x", LiveStatus::Waiting)];
        assert_eq!(app.claude.session_count(), 3);
        assert_eq!(app.claude.waiting_count(), 1);
    }

    #[test]
    fn live_poll_interval_depends_on_view() {
        let mut app = test_app();
        let now = Instant::now();
        assert!(!app.live_poll_due(now), "vor dem ersten Öffnen nie");
        app.claude.scanned_once = true;
        assert!(app.live_poll_due(now));

        app.claude.last_live_poll = Some(now);
        app.main_view = MainView::ClaudeSessions;
        assert!(!app.live_poll_due(now + Duration::from_millis(500)));
        assert!(app.live_poll_due(now + Duration::from_millis(1000)));
        app.main_view = MainView::Repos;
        assert!(!app.live_poll_due(now + Duration::from_secs(2)));
        assert!(app.live_poll_due(now + Duration::from_secs(5)));

        app.claude.live_inflight = true;
        assert!(!app.live_poll_due(now + Duration::from_secs(60)));
    }

    #[test]
    fn live_change_triggers_throttled_rescan() {
        let mut app = test_app();
        app.claude.scanned_once = true;
        app.claude.live_inflight = true;
        app.apply_live_result(vec![live("a", LiveStatus::Busy)]);
        assert!(!app.claude.live_inflight);
        assert_eq!(app.claude.scan_generation, 1, "neue Session → Scan");

        // gleiche Sessions, nur Status anders → kein Scan
        app.apply_claude_result(SessionScan::default());
        app.claude.last_scan_start = None;
        app.apply_live_result(vec![live("a", LiveStatus::Idle)]);
        assert_eq!(app.claude.scan_generation, 1);

        // Session weg, aber gerade erst gescannt → gedrosselt
        app.claude.last_scan_start = Some(Instant::now());
        app.apply_live_result(vec![]);
        assert_eq!(app.claude.scan_generation, 1);
    }

    #[test]
    fn failed_focus_shows_status_message() {
        let mut app = test_app();
        app.apply_claude_actions(ClaudeSessionActions {
            focus: Some(u32::MAX),
            ..ClaudeSessionActions::default()
        });
        assert_eq!(
            app.status_message.as_deref(),
            Some(tr(app.config.language, "claude_focus_failed").as_str())
        );
    }
}
