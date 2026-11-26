//! Offene Merge Requests den Repo-Karten zuordnen (Chip auf der Karte) und
//! Sprung von dort in die MR-Ansicht.

use std::collections::HashMap;
use std::path::PathBuf;

use super::{MainView, MyApp};
use crate::claude_live::{LiveSession, LiveStatus};
use crate::claude_sessions::path_key;
use crate::git::RepoInfo;
use crate::gitlab::badge::MrBadge;
use crate::ui::merge_requests::MrFocus;

/// Ein Badge je Ordner; Priorität: eigene vor fremden, dann neuestes
/// `updated_at`. Bei Gleichstand gewinnt der erste Treffer.
pub(crate) fn pick_badges(hits: Vec<(PathBuf, MrBadge)>) -> HashMap<PathBuf, MrBadge> {
    let rank = |b: &MrBadge| (b.mine, b.updated_at);
    let mut out: HashMap<PathBuf, MrBadge> = HashMap::new();
    for (path, badge) in hits {
        match out.get(&path) {
            Some(old) if rank(old) >= rank(&badge) => {}
            _ => {
                out.insert(path, badge);
            }
        }
    }
    out
}

/// Rang für den Claude-Chip: „wartet“ vor „arbeitet“; `Idle` zählt nicht.
fn claude_rank(status: LiveStatus) -> u8 {
    match status {
        LiveStatus::Waiting => 2,
        LiveStatus::Busy | LiveStatus::Shell => 1,
        LiveStatus::Idle => 0,
    }
}

/// Live-Status je Repo-Ordner (gleicher `path_key` wie `cwd` der Session);
/// teilen sich mehrere Sessions einen Ordner, gewinnt „wartet“.
pub(crate) fn claude_card_status(
    live: &[LiveSession],
    repos: &[RepoInfo],
) -> HashMap<PathBuf, LiveStatus> {
    let active: Vec<(String, LiveStatus)> = live
        .iter()
        .filter(|s| claude_rank(s.status) > 0)
        .map(|s| (path_key(&s.cwd), s.status))
        .collect();
    let mut out = HashMap::new();
    if active.is_empty() {
        return out;
    }
    for repo in repos {
        let key = path_key(&repo.path.to_string_lossy());
        let best = active
            .iter()
            .filter(|(k, _)| *k == key)
            .map(|(_, s)| *s)
            .max_by_key(|s| claude_rank(*s));
        if let Some(status) = best {
            out.insert(repo.path.clone(), status);
        }
    }
    out
}

/// PID der Session für den Klick auf den Claude-Chip (wartende zuerst).
pub(crate) fn claude_pid_for(live: &[LiveSession], path: &std::path::Path) -> Option<u32> {
    let key = path_key(&path.to_string_lossy());
    live.iter()
        .filter(|s| claude_rank(s.status) > 0 && path_key(&s.cwd) == key)
        .max_by_key(|s| claude_rank(s.status))
        .map(|s| s.pid)
}

/// Läuft (egal in welchem Zustand) eine Claude Session in diesem Ordner?
pub(crate) fn claude_running_at(live: &[LiveSession], path: &std::path::Path) -> bool {
    let key = path_key(&path.to_string_lossy());
    live.iter().any(|s| path_key(&s.cwd) == key)
}

impl MyApp {
    /// Karte je Repo-Ordner neu: offene MRs, deren Quell-Branch dort laut
    /// Scan ausgecheckt ist. Nach jedem Abruf und Umsortieren von `repos`.
    pub(super) fn update_repo_mrs(&mut self) {
        let Some(snap) = self.mr_snapshot.as_deref() else {
            self.repo_mrs.clear();
            return;
        };
        let cfg = &self.config.merge_requests;
        let uninvolved = snap.uninvolved.iter().flat_map(|u| u.mrs.iter());
        let tagged = snap
            .mine
            .iter()
            .map(|m| (m, true))
            .chain(snap.others.iter().map(|m| (m, false)))
            .chain(uninvolved.map(|m| (m, false)));
        let hits = tagged
            .filter_map(|(mr, mine)| {
                let path = self.mr_checked_out_at(mr)?;
                Some((path, MrBadge::with_cfg(mr, mine, cfg)))
            })
            .collect();
        self.repo_mrs = pick_badges(hits);
    }

    /// Chip der Repo-Karte: in die MR-Ansicht wechseln und die Zeile
    /// hervorheben.
    pub(super) fn focus_mr(&mut self, host_key: &str, id: &str) {
        self.main_view = MainView::MergeRequests;
        // Ein offener MR steht nicht unter „Gemergt“
        self.mr_view.mine_merged = false;
        // Eine aktive Suche könnte die Zeile ausblenden: Fokus ginge verloren
        self.mr_view.mine_query.clear();
        self.mr_view.others_query.clear();
        self.mr_view.focus = Some(MrFocus::new(host_key, id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;
    use crate::git::RepoInfo;
    use crate::gitlab::model::{MergeRequest, MrSnapshot, UninvolvedList};
    use crate::gitlab::remote::GlabHost;
    use crate::gitlab::remote::RemoteInfo;
    use std::path::Path;

    const HOST: &str = "gitlab.example.com";

    fn badge(id: &str, mine: bool, updated_at: Option<i64>) -> MrBadge {
        let mr = MergeRequest {
            id: id.to_string(),
            updated_at,
            ..MergeRequest::default()
        };
        MrBadge::from_mr(&mr, mine)
    }

    #[test]
    fn pick_badges_prefers_mine_then_newest() {
        let dir = PathBuf::from("/r/a");
        let picked = pick_badges(vec![
            (dir.clone(), badge("old-other", false, Some(1))),
            (dir.clone(), badge("new-other", false, Some(9))),
            (dir.clone(), badge("old-mine", true, Some(2))),
            (dir.clone(), badge("new-mine", true, Some(5))),
            (PathBuf::from("/r/b"), badge("b", false, None)),
        ]);
        assert_eq!(picked.len(), 2);
        assert_eq!(picked[&dir].id, "new-mine");
        assert_eq!(picked[&PathBuf::from("/r/b")].id, "b");
        // ohne eigene gewinnt der neueste fremde
        let picked = pick_badges(vec![
            (dir.clone(), badge("old", false, Some(1))),
            (dir.clone(), badge("new", false, Some(9))),
        ]);
        assert_eq!(picked[&dir].id, "new");
    }

    fn app_with_repo() -> MyApp {
        let mut app = test_app();
        app.glab_hosts = vec![GlabHost {
            key: HOST.to_string(),
            user: "taltmann".to_string(),
            ..GlabHost::default()
        }];
        let mut repo = RepoInfo::new(
            PathBuf::from("/tmp/gm-badge-test/app"),
            "feature/x".to_string(),
            false,
            false,
        );
        repo.remotes = vec![RemoteInfo::new(
            "origin",
            "git@gitlab.example.com:group/app.git",
        )];
        app.repos = vec![repo];
        app.update_mr_hosts();
        app
    }

    fn mr(n: u64, branch: &str) -> MergeRequest {
        MergeRequest {
            host_key: HOST.to_string(),
            id: format!("gid://gitlab/MergeRequest/{n}"),
            iid: n,
            source_branch: branch.to_string(),
            source_project_path: Some("group/app".to_string()),
            project_path: "group/app".to_string(),
            ..MergeRequest::default()
        }
    }

    #[test]
    fn update_repo_mrs_maps_checked_out_branch() {
        let mut app = app_with_repo();
        app.update_repo_mrs();
        assert!(app.repo_mrs.is_empty(), "ohne Snapshot leer");

        app.mr_snapshot = Some(Box::new(MrSnapshot {
            mine: vec![mr(1, "feature/x")],
            others: vec![mr(2, "other")],
            uninvolved: Some(UninvolvedList {
                mrs: vec![mr(3, "feature/y")],
                ..UninvolvedList::default()
            }),
            ..MrSnapshot::default()
        }));
        app.update_repo_mrs();
        let path = PathBuf::from("/tmp/gm-badge-test/app");
        assert_eq!(app.repo_mrs.len(), 1);
        assert_eq!(app.repo_mrs[&path].iid, 1);
        assert!(app.repo_mrs[&path].mine);

        app.mr_snapshot = None;
        app.update_repo_mrs();
        assert!(app.repo_mrs.is_empty());
    }

    #[test]
    fn focus_mr_switches_view_and_sets_focus() {
        let mut app = test_app();
        assert_eq!(app.main_view, MainView::Repos);
        app.focus_mr(HOST, "gid://x/1");
        assert_eq!(app.main_view, MainView::MergeRequests);
        let focus = app.mr_view.focus.as_ref().expect("focus");
        assert_eq!(
            (focus.host_key.as_str(), focus.id.as_str()),
            (HOST, "gid://x/1")
        );
        assert!(!focus.scrolled);
    }

    #[test]
    fn focus_mr_leaves_merged_toggle() {
        let mut app = test_app();
        app.mr_view.mine_merged = true;
        app.focus_mr(HOST, "gid://x/1");
        assert!(!app.mr_view.mine_merged);
    }

    #[test]
    fn focus_mr_clears_mr_search() {
        let mut app = test_app();
        app.mr_view.mine_query = "shop".into();
        app.mr_view.others_query = "fix".into();
        app.focus_mr(HOST, "gid://x/1");
        assert!(app.mr_view.mine_query.is_empty());
        assert!(app.mr_view.others_query.is_empty());
    }

    #[test]
    fn more_page_updates_repo_mrs() {
        use crate::app::merge_requests::MrResult;
        use crate::gitlab::service::UninvolvedPage;
        let mut app = app_with_repo();
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            uninvolved: Some(UninvolvedList::default()),
            ..MrSnapshot::default()
        }));
        app.mr_more_pending = 1;
        app.mr_tx
            .send(MrResult::MorePage {
                epoch: 0,
                host_key: HOST.to_string(),
                page: Ok(UninvolvedPage {
                    host_key: HOST.to_string(),
                    mrs: vec![mr(7, "feature/x")],
                    ..UninvolvedPage::default()
                }),
            })
            .expect("send");
        app.poll_mr(&egui::Context::default());
        let path = PathBuf::from("/tmp/gm-badge-test/app");
        assert_eq!(app.repo_mrs[&path].iid, 7);
    }

    fn render(app: &mut MyApp, ctx: &egui::Context) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| app.show_mr_view(ui));
        output.drop_without_applying_deltas();
    }

    fn app_with_mr_snapshot() -> MyApp {
        let mut app = app_with_repo();
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            mine: vec![mr(1, "feature/x")],
            per_host: vec![crate::gitlab::model::HostStatus {
                host_key: HOST.to_string(),
                user: Some("taltmann".to_string()),
                ..Default::default()
            }],
            ..MrSnapshot::default()
        }));
        app
    }

    /// Aus dem Tray: zwischen Klick und erstem Zeichnen kann Zeit vergehen
    #[test]
    fn focus_highlight_starts_when_first_drawn() {
        let mut app = app_with_mr_snapshot();
        let ctx = egui::Context::default();
        app.focus_mr(HOST, "gid://gitlab/MergeRequest/1");
        let past = std::time::Instant::now() - crate::app::mr_checkout::HIGHLIGHT_DURATION;
        app.mr_view.focus.as_mut().expect("focus").since = past;
        render(&mut app, &ctx);
        let focus = app.mr_view.focus.as_ref().expect("noch hervorgehoben");
        assert!(focus.since > past);
    }

    #[test]
    fn focus_survives_frames_then_expires() {
        let mut app = app_with_mr_snapshot();
        let ctx = egui::Context::default();
        app.focus_mr(HOST, "gid://gitlab/MergeRequest/1");
        render(&mut app, &ctx);
        // Zeile gefunden und angescrollt, Fokus bleibt
        assert!(app.mr_view.focus.as_ref().is_some_and(|f| f.scrolled));
        render(&mut app, &ctx);
        assert!(app.mr_view.focus.is_some());
        // abgelaufen
        let past = std::time::Instant::now()
            - crate::app::mr_checkout::HIGHLIGHT_DURATION
            - std::time::Duration::from_millis(10);
        app.mr_view.focus.as_mut().expect("focus").since = past;
        render(&mut app, &ctx);
        assert_eq!(app.mr_view.focus, None);
    }

    #[test]
    fn focus_dropped_when_row_not_drawn() {
        let mut app = app_with_mr_snapshot();
        let ctx = egui::Context::default();
        app.focus_mr(HOST, "gid://gitlab/MergeRequest/99");
        render(&mut app, &ctx);
        assert_eq!(app.mr_view.focus, None);
    }

    fn live(pid: u32, cwd: &str, status: LiveStatus) -> LiveSession {
        LiveSession {
            pid,
            session_id: format!("s{pid}"),
            cwd: cwd.to_string(),
            status,
            status_since_ms: 0,
            name: None,
            proc_start: None,
        }
    }

    #[test]
    fn claude_card_status_waiting_wins_and_idle_hidden() {
        let repos = vec![
            RepoInfo::new(PathBuf::from("/code/app"), "main".into(), false, false),
            RepoInfo::new(PathBuf::from("/code/web"), "main".into(), false, false),
            RepoInfo::new(PathBuf::from("/code/idle"), "main".into(), false, false),
        ];
        let sessions = vec![
            live(1, "/code/app", LiveStatus::Busy),
            live(2, "/code/app", LiveStatus::Waiting),
            live(3, "/code/web", LiveStatus::Shell),
            live(4, "/code/idle", LiveStatus::Idle),
            live(5, "/elsewhere", LiveStatus::Waiting),
        ];
        let map = claude_card_status(&sessions, &repos);
        assert_eq!(map.len(), 2);
        assert_eq!(map[&PathBuf::from("/code/app")], LiveStatus::Waiting);
        assert_eq!(map[&PathBuf::from("/code/web")], LiveStatus::Shell);
        assert!(claude_card_status(&[], &repos).is_empty());
        // Klick fokussiert die wartende Session
        assert_eq!(claude_pid_for(&sessions, Path::new("/code/app")), Some(2));
        assert_eq!(claude_pid_for(&sessions, Path::new("/code/idle")), None);
    }
}
