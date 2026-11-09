//! Ansicht „Claude Sessions“: Zustand, Hintergrund-Scan und Fortsetzen.

use super::MyApp;
use crate::claude_sessions::{claude_projects_dir, resume_profile, scan_sessions, RepoSessions, SessionCache};
use crate::git::launch_agent;
use crate::i18n::{tr, tr_fmt};
use crate::ui::claude_sessions::{show_claude_sessions, ClaudeSessionActions};
use egui::Ui;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Zustand der Ansicht (nicht gespeichert).
pub(super) struct ClaudeViewState {
    pub(super) groups: Vec<RepoSessions>,
    pub(super) scanning: bool,
    /// Ansicht wurde einmal geöffnet: ab dann folgt jedem Repo-Scan ein
    /// Session-Scan
    pub(super) scanned_once: bool,
    /// Zählt gestartete Scans; ein Ergebnis gilt nur für den neuesten
    pub(super) scan_generation: u64,
    pub(super) search: String,
    cache: Arc<Mutex<SessionCache>>,
    tx: Sender<(u64, Vec<RepoSessions>)>,
    rx: Receiver<(u64, Vec<RepoSessions>)>,
}

impl ClaudeViewState {
    pub(super) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            groups: Vec::new(),
            scanning: false,
            scanned_once: false,
            scan_generation: 0,
            search: String::new(),
            cache: Arc::default(),
            tx,
            rx,
        }
    }

    pub(super) fn session_count(&self) -> usize {
        self.groups.iter().map(|g| g.sessions.len()).sum()
    }
}

impl MyApp {
    /// Startet einen Session-Scan im Hintergrund über die aktuellen Repos.
    pub(super) fn start_claude_scan(&mut self) {
        self.claude.scanning = true;
        self.claude.scan_generation += 1;
        let generation = self.claude.scan_generation;
        let repos = self.repos.clone();
        let cache = Arc::clone(&self.claude.cache);
        let tx = self.claude.tx.clone();
        std::thread::spawn(move || {
            let groups = match claude_projects_dir() {
                Some(dir) => {
                    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
                    scan_sessions(&dir, &repos, &mut cache)
                }
                None => Vec::new(),
            };
            let _ = tx.send((generation, groups));
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

    pub(super) fn apply_claude_result(&mut self, groups: Vec<RepoSessions>) {
        self.claude.groups = groups;
        self.claude.scanning = false;
    }

    /// Holt fertige Scans ab; ältere Generationen werden verworfen.
    pub(super) fn poll_claude(&mut self, ctx: &egui::Context) {
        while let Ok((generation, groups)) = self.claude.rx.try_recv() {
            if generation == self.claude.scan_generation {
                self.apply_claude_result(groups);
            }
        }
        if self.claude.scanning {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub(super) fn show_claude_view(&mut self, ui: &mut Ui) {
        let mut actions = ClaudeSessionActions::default();
        show_claude_sessions(
            ui,
            &self.claude.groups,
            self.claude.scanning,
            self.config.language,
            &mut self.claude.search,
            &mut actions,
        );
        if let Some((cwd, id)) = actions.resume {
            self.resume_claude_session(&cwd, &id);
        }
    }

    /// Statuszeile der Ansicht (ohne Statusmeldung).
    pub(super) fn claude_status_line(&self) -> String {
        let lang = self.config.language;
        if self.claude.scanning {
            return tr(lang, "claude_scanning");
        }
        tr_fmt(
            lang,
            "claude_status",
            &[
                &self.claude.session_count().to_string(),
                &self.claude.groups.len().to_string(),
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
        self.status_message_time = Some(std::time::Instant::now());
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
    use crate::claude_sessions::RepoSessions;
    use std::path::PathBuf;

    fn group() -> RepoSessions {
        RepoSessions {
            repo_path: PathBuf::from("C:\\a"),
            repo_name: "a".to_string(),
            is_worktree: false,
            sessions: vec![],
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
        app.apply_claude_result(vec![group()]);
        assert_eq!(app.claude.groups.len(), 1);
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
    fn claude_session_count_sums_groups() {
        let mut app = test_app();
        let mut g = group();
        g.sessions = vec![
            crate::claude_sessions::ClaudeSession {
                id: "x".into(),
                cwd: "C:\\a".into(),
                title: "t".into(),
                last_prompt: None,
                git_branch: None,
                last_activity: 1,
            };
            3
        ];
        app.apply_claude_result(vec![g.clone(), g]);
        assert_eq!(app.claude.session_count(), 6);
    }
}
