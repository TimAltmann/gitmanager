//! Kurzfassung eines Merge Requests für den Chip auf der Repo-Karte.

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
    pub(crate) fn with_cfg(mr: &MergeRequest, mine: bool, cfg: &MrConfig) -> Self {
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
