//! Offene Merge Requests den Repo-Karten zuordnen (Chip auf der Karte) und
//! Sprung von dort in die MR-Ansicht.

use std::collections::HashMap;
use std::path::PathBuf;

use super::{MainView, MyApp};
use crate::config::MrConfig;
use crate::gitlab::model::{approvals, MergeRequest, PipelineState};

/// Kurzfassung eines MR für den Chip `!iid · n/m ✓ · Pipeline`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrBadge {
    pub host_key: String,
    pub id: String,
    pub iid: u64,
    pub title: String,
    pub draft: bool,
    pub approved: u32,
    /// Regel bzw. Ziel aus den Einstellungen; `None` = keines
    pub required: Option<u32>,
    pub pipeline: PipelineState,
    pub mine: bool,
    pub updated_at: Option<i64>,
}

impl MrBadge {
    /// Mit Standard-Einstellungen; `update_repo_mrs` nutzt die echten.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_mr(mr: &MergeRequest, mine: bool) -> Self {
        Self::with_cfg(mr, mine, &MrConfig::default())
    }

    /// Zählung wie die Approval-Leiste der MR-Zeile (Bots, Ziel).
    fn with_cfg(mr: &MergeRequest, mine: bool, cfg: &MrConfig) -> Self {
        let appr = approvals(mr, cfg.ignore_bot_approvals, cfg.approval_goal);
        Self {
            host_key: mr.host_key.clone(),
            id: mr.id.clone(),
            iid: mr.iid,
            title: mr.title.clone(),
            draft: mr.draft,
            approved: appr.given,
            required: (appr.goal > 0).then_some(appr.goal),
            pipeline: mr.pipeline,
            mine,
            updated_at: mr.updated_at,
        }
    }
}

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
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn focus_mr(&mut self, host_key: &str, id: &str) {
        self.main_view = MainView::MergeRequests;
        self.mr_view.focus = Some((host_key.to_string(), id.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;
    use crate::git::RepoInfo;
    use crate::gitlab::model::{MrSnapshot, UninvolvedList};
    use crate::gitlab::remote::GlabHost;
    use crate::gitlab::remote::RemoteInfo;

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
        assert_eq!(
            app.mr_view.focus,
            Some((HOST.to_string(), "gid://x/1".to_string()))
        );
    }

    #[test]
    fn focus_is_consumed_after_rendering() {
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
        app.focus_mr(HOST, "gid://gitlab/MergeRequest/1");
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| app.show_mr_view(ui));
        output.drop_without_applying_deltas();
        assert_eq!(app.mr_view.focus, None);
    }
}
