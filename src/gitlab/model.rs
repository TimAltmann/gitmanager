//! Domänenlogik der Merge-Request-Ansicht (rein, ohne Netz und UI): MR-Modell
//! aus den GraphQL-Rohdaten, Einteilung in Meine / Von anderen / Nicht
//! beteiligt mit Repo-Auswahl (E1), Sortierung (E11), Badge (E9),
//! „mergebereit“ (E10), Chips und Leerzustände (E13), Pipeline-Link (E7),
//! eigene gemergte MRs und die Suche je Spalte.

use super::glab::GlabErrorKind;
use super::graphql::{
    Dashboard, MentionKind, MentionNode, MergedPage, MrNode, ProjectMrs, UserNode,
};
use super::remote::{project_key, LocalProject};
use super::time::parse_rfc3339;
use crate::config::MrConfig;
use crate::i18n::{tr, tr_fmt, Language};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Pipeline-Zustand aus `headPipeline.status` (`PipelineStatusEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PipelineState {
    Success,
    Failed,
    Running,
    Pending,
    Canceled,
    Skipped,
    Manual,
    /// Kein `headPipeline`
    #[default]
    NoPipeline,
}

impl PipelineState {
    /// `null` → keine Pipeline. Groß-/Kleinschreibung egal (REST liefert
    /// klein); unbekannte Werte neuerer GitLab-Versionen zählen als wartend.
    pub fn from_status(status: Option<&str>) -> Self {
        let Some(status) = status else {
            return Self::NoPipeline;
        };
        match status.to_ascii_uppercase().as_str() {
            "SUCCESS" => Self::Success,
            "FAILED" => Self::Failed,
            "RUNNING" => Self::Running,
            "CANCELED" | "CANCELING" => Self::Canceled,
            "SKIPPED" => Self::Skipped,
            "MANUAL" => Self::Manual,
            // PENDING, CREATED, PREPARING, WAITING_FOR_*, SCHEDULED
            _ => Self::Pending,
        }
    }

    /// i18n-Schlüssel der Beschriftung („Pipeline grün“ …)
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Success => "mr_pipe_success",
            Self::Failed => "mr_pipe_failed",
            Self::Running => "mr_pipe_running",
            Self::Pending => "mr_pipe_pending",
            Self::Canceled => "mr_pipe_canceled",
            Self::Skipped => "mr_pipe_skipped",
            Self::Manual => "mr_pipe_manual",
            Self::NoPipeline => "mr_pipe_none",
        }
    }
}

/// Warum ein MR unter „Von anderen“ steht; Reihenfolge = Anzeige-Reihenfolge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    Reviewer,
    MentionDescription,
    MentionComment,
}

impl Reason {
    /// Ohne `note` (Feld nicht abfragbar) ist die Stelle unbekannt; die
    /// Erwähnung zählt dann als Kommentar.
    fn from_mention(kind: Option<MentionKind>) -> Self {
        match kind {
            Some(MentionKind::Description) => Self::MentionDescription,
            Some(MentionKind::Comment) | None => Self::MentionComment,
        }
    }

    pub fn is_mention(self) -> bool {
        self != Self::Reviewer
    }
}

/// Eigener Review-Status als Reviewer (`mergeRequestInteraction.reviewState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MyReview {
    /// `UNREVIEWED`, `REVIEW_STARTED`
    Pending,
    Approved,
    ChangesRequested,
    /// `REVIEWED`, `UNAPPROVED`
    Commented,
}

impl MyReview {
    /// Ohne `reviewState` (altes GitLab) zählt ein eigenes Approval, sonst
    /// gilt das Review als offen.
    fn from_state(state: Option<&str>, approved_by_me: bool) -> Self {
        match state {
            Some("APPROVED") => Self::Approved,
            Some("REQUESTED_CHANGES") => Self::ChangesRequested,
            Some("REVIEWED" | "UNAPPROVED") => Self::Commented,
            Some("UNREVIEWED" | "REVIEW_STARTED") => Self::Pending,
            _ if approved_by_me => Self::Approved,
            _ => Self::Pending,
        }
    }

    /// i18n-Schlüssel („Review ausstehend“ …)
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Pending => "mr_rev_pending",
            Self::Approved => "mr_rev_approved",
            Self::ChangesRequested => "mr_rev_changes",
            Self::Commented => "mr_rev_commented",
        }
    }
}

/// Autor, Approver oder Reviewer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Person {
    pub username: String,
    /// Anzeigename; fällt auf den Benutzernamen zurück
    pub name: String,
    pub bot: bool,
    /// Der angemeldete Benutzer dieses Hosts
    pub is_me: bool,
}

impl Person {
    fn new(username: Option<&str>, name: Option<&str>, bot: bool, me: Option<&str>) -> Self {
        let username = username.unwrap_or_default().to_string();
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(|| username.clone(), str::to_string);
        // GitLab-Benutzernamen sind ohne Groß-/Kleinschreibung eindeutig
        let is_me = me.is_some_and(|me| !me.is_empty() && username.eq_ignore_ascii_case(me));
        Self {
            username,
            name,
            bot,
            is_me,
        }
    }

    fn from_user(user: &UserNode, me: Option<&str>) -> Self {
        Self::new(
            user.username.as_deref(),
            user.name.as_deref(),
            user.bot.unwrap_or(false),
            me,
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reviewer {
    pub person: Person,
    /// `MergeRequestReviewState`, großgeschrieben; `None` = nicht abfragbar
    pub state: Option<String>,
}

/// Ein MR, wie die Ansicht ihn braucht. Fehlende Felder sind auf neutrale
/// Werte gesetzt (Schema-Fallback, alte GitLab-Versionen).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeRequest {
    /// glab-Host-Schlüssel
    pub host_key: String,
    /// Globale ID (`gid://…`), sonst `projekt!iid`; eindeutig nur je Host
    pub id: String,
    pub iid: u64,
    pub title: String,
    pub draft: bool,
    pub web_url: String,
    pub web_path: Option<String>,
    /// Zielprojekt wie von GitLab geliefert (Groß-/Kleinschreibung bleibt)
    pub project_path: String,
    /// Quellprojekt; weicht bei Fork-MRs ab (E6)
    pub source_project_path: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
    pub source_branch_exists: Option<bool>,
    pub diff_head_sha: Option<String>,
    pub author: Person,
    pub approved_by: Vec<Person>,
    /// Nur EE
    pub approvals_required: Option<u32>,
    /// Nur EE
    pub approvals_left: Option<u32>,
    pub pipeline: PipelineState,
    /// `headPipeline.path` (E7)
    pub pipeline_path: Option<String>,
    pub threads_open: u32,
    pub threads_resolved: u32,
    pub detailed_merge_status: Option<String>,
    pub reviewers: Vec<Reviewer>,
    /// Unix-Sekunden
    pub updated_at: Option<i64>,
    /// Nur unter „Von anderen“ gefüllt, sortiert
    pub reasons: Vec<Reason>,
    /// Nur wenn ich Reviewer bin
    pub my_review: Option<MyReview>,
    /// Projekt hat einen gescannten Klon (auch wenn abgewählt)
    pub local: bool,
}

impl MergeRequest {
    /// Baut den MR aus dem GraphQL-Knoten. Ohne `iid` oder Projektpfad
    /// (auch nicht aus `webPath` ableitbar) → `None`.
    pub fn from_node(host_key: &str, node: MrNode, me: Option<&str>) -> Option<Self> {
        let MrNode {
            id,
            iid,
            title,
            draft,
            web_url,
            web_path,
            updated_at,
            source_branch,
            target_branch,
            source_branch_exists,
            diff_head_sha,
            project_path,
            source_project_path,
            author,
            approved_by,
            approvals_required,
            approvals_left,
            head_pipeline,
            resolvable_discussions,
            resolved_discussions,
            detailed_merge_status,
            reviewers,
            state: _,
        } = node;
        let iid = iid?;
        let project_path = project_path
            .filter(|p| !p.is_empty())
            .or_else(|| web_path.as_deref().and_then(project_from_web_path))?;
        let approved_by: Vec<Person> = approved_by
            .unwrap_or_default()
            .iter()
            .map(|u| Person::from_user(u, me))
            .collect();
        let reviewers: Vec<Reviewer> = reviewers
            .unwrap_or_default()
            .into_iter()
            .map(|r| Reviewer {
                person: Person::new(r.username.as_deref(), r.name.as_deref(), false, me),
                state: r.review_state.map(|s| s.to_ascii_uppercase()),
            })
            .collect();
        let approved_by_me = approved_by.iter().any(|p| p.is_me);
        let my_review = reviewers
            .iter()
            .find(|r| r.person.is_me)
            .map(|r| MyReview::from_state(r.state.as_deref(), approved_by_me));
        // Pipeline ohne lesbaren Status gilt als wartend
        let pipeline = head_pipeline
            .as_ref()
            .map_or(PipelineState::NoPipeline, |p| {
                PipelineState::from_status(Some(p.status.as_deref().unwrap_or_default()))
            });
        let resolved = resolved_discussions.unwrap_or(0);
        Some(Self {
            host_key: host_key.to_string(),
            id: id.unwrap_or_else(|| format!("{project_path}!{iid}")),
            iid,
            title: title.unwrap_or_default(),
            draft: draft.unwrap_or(false),
            web_url: web_url.unwrap_or_default(),
            web_path,
            project_path,
            source_project_path,
            source_branch: source_branch.unwrap_or_default(),
            target_branch: target_branch.unwrap_or_default(),
            source_branch_exists,
            diff_head_sha,
            author: author
                .map(|a| Person::from_user(&a, me))
                .unwrap_or_default(),
            approved_by,
            approvals_required,
            approvals_left,
            pipeline,
            pipeline_path: head_pipeline.and_then(|p| p.path),
            threads_open: resolvable_discussions.unwrap_or(0).saturating_sub(resolved),
            threads_resolved: resolved,
            detailed_merge_status,
            reviewers,
            updated_at: updated_at.as_deref().and_then(parse_rfc3339),
            reasons: Vec::new(),
            my_review,
            local: false,
        })
    }

    pub fn is_reviewer(&self) -> bool {
        self.reasons.contains(&Reason::Reviewer)
    }

    pub fn is_mentioned(&self) -> bool {
        self.reasons.iter().any(|r| r.is_mention())
    }

    /// Offene Review-Anfrage an mich (Badge, Sortierung „Von anderen“).
    pub fn review_pending(&self) -> bool {
        self.is_reviewer() && self.my_review == Some(MyReview::Pending)
    }

    /// „1 offener Thread“, „{n} offene Threads“, „alle Threads gelöst“, „keine Threads“
    pub fn threads_label(&self, lang: Language) -> String {
        match (self.threads_open, self.threads_resolved) {
            (1, _) => tr(lang, "mr_threads_one"),
            (0, 0) => tr(lang, "mr_threads_none"),
            (0, _) => tr(lang, "mr_threads_resolved"),
            (open, _) => tr_fmt(lang, "mr_threads_many", &[&open.to_string()]),
        }
    }

    /// „{n} offen · {m} gelöst“ bzw. „Keine Diskussions-Threads“
    pub fn threads_tooltip(&self, lang: Language) -> String {
        if self.threads_open == 0 && self.threads_resolved == 0 {
            return tr(lang, "mr_threads_none_tt");
        }
        tr_fmt(
            lang,
            "mr_threads_tt",
            &[
                &self.threads_open.to_string(),
                &self.threads_resolved.to_string(),
            ],
        )
    }

    fn add_reason(&mut self, reason: Reason) {
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
            self.reasons.sort();
        }
    }
}

/// `/gruppe/projekt/-/merge_requests/1` → `gruppe/projekt`. Nur Ersatz, wenn
/// `project` fehlt; bei GitLab in einem Unterordner ungenau.
fn project_from_web_path(web_path: &str) -> Option<String> {
    let (project, _) = web_path.split_once("/-/")?;
    let project = project.trim_matches('/');
    (!project.is_empty()).then(|| project.to_string())
}

/// Approvals eines MR für Balken, Pips und Tooltip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Approvals {
    pub given: u32,
    /// 0 = keine Approval-Regel (auch CE)
    pub required: u32,
    pub left: u32,
    /// In GitLab-Reihenfolge, ohne Bots, wenn ausgeblendet
    pub approvers: Vec<Person>,
    /// Reviewer ohne Approval mit offenem Review; nur bei eigenen MRs
    /// (design-b §3), bei anderen nennt der Tooltip nur die Approver
    pub pending_reviewers: Vec<PendingReviewer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingReviewer {
    pub name: String,
    /// `REVIEW_STARTED` statt `UNREVIEWED`
    pub started: bool,
    /// Der angemeldete Benutzer (Review der eigenen MR)
    pub is_me: bool,
}

/// Name, beim angemeldeten Benutzer mit „(du)“.
fn display_name(name: &str, is_me: bool, lang: Language) -> String {
    if is_me {
        tr_fmt(lang, "mr_appr_you", &[name])
    } else {
        name.to_string()
    }
}

impl Approvals {
    pub fn has_rule(&self) -> bool {
        self.required > 0
    }

    /// „{g} von {r} Approvals“ bzw. ohne Regel „1 Approval“ / „{g} Approvals“
    pub fn label(&self, lang: Language) -> String {
        let given = self.given.to_string();
        if self.has_rule() {
            tr_fmt(lang, "mr_appr_of", &[&given, &self.required.to_string()])
        } else if self.given == 1 {
            tr(lang, "mr_appr_one")
        } else {
            tr_fmt(lang, "mr_appr_n", &[&given])
        }
    }

    /// „{g}/{r}“ bzw. „{g}“
    pub fn short(&self, lang: Language) -> String {
        let given = self.given.to_string();
        if self.has_rule() {
            tr_fmt(lang, "mr_appr_short", &[&given, &self.required.to_string()])
        } else {
            given
        }
    }

    /// „1 von 2 Approvals · Jonas Krüger; Miriam Becker hat das Review begonnen“
    pub fn tooltip(&self, lang: Language) -> String {
        let mut details = Vec::new();
        if !self.approvers.is_empty() {
            let names: Vec<String> = self
                .approvers
                .iter()
                .map(|p| display_name(&p.name, p.is_me, lang))
                .collect();
            details.push(names.join(", "));
        }
        details.extend(self.pending_reviewers.iter().map(|r| {
            let key = if r.started {
                "mr_rev_started"
            } else {
                "mr_rev_waiting"
            };
            tr_fmt(lang, key, &[&display_name(&r.name, r.is_me, lang)])
        }));
        match (details.is_empty(), self.has_rule()) {
            (true, false) => tr(lang, "mr_appr_no_rule"),
            (true, true) => self.label(lang),
            (false, _) => format!("{} · {}", self.label(lang), details.join("; ")),
        }
    }
}

/// Abfrageteil eines Hosts (Teilfehler).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MrPart {
    /// Eigene MRs und Review-Anfragen
    Dashboard,
    Mentions,
    Uninvolved,
}

/// Ergebnis eines GitLab-Hosts im letzten Abruf. Fehler stehen nur hier und
/// in der MR-Ansicht, nie in `self.error` (E20).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostStatus {
    pub host_key: String,
    /// `currentUser.username`
    pub user: Option<String>,
    /// `metadata.version`
    pub version: Option<String>,
    /// Fehler, der den ganzen Host betrifft (Anmeldung, Netz, …)
    pub error: Option<GlabErrorKind>,
    /// Teilfehler; die übrigen Daten des Hosts bleiben gültig
    pub partial: Vec<(MrPart, GlabErrorKind)>,
    /// GitLab verbirgt die To-dos vor dem Token (feingranular ohne „Todo:
    /// Read“): Erwähnungen fehlen ganz
    pub todos_hidden: bool,
}

/// Rohdaten eines GitLab-Hosts aus einem Abruf (Service, Task 7).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostRaw {
    pub host_key: String,
    /// `None`, wenn das Dashboard nicht geladen werden konnte
    pub dashboard: Option<Dashboard>,
    /// Offene Erwähnungen aller geladenen Seiten
    pub mentions: Vec<MentionNode>,
    /// `None` = „Nicht beteiligt“ nicht abgefragt oder fehlgeschlagen
    pub uninvolved: Option<Vec<ProjectMrs>>,
    /// `count` in `uninvolved` enthält auch meine MRs, weil der Server
    /// `not: {authorUsername}` nicht kennt (Schema-Fallback; der Service
    /// liest das aus `HostSchema::removed`)
    pub uninvolved_counts_own: bool,
    pub status: HostStatus,
    /// Unix-Sekunden zu Beginn des Abrufs
    pub fetched_at: i64,
}

/// Weitere Seite „Nicht beteiligt“ eines Projekts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninvolvedCursor {
    pub host_key: String,
    pub full_path: String,
    pub cursor: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninvolvedList {
    pub mrs: Vec<MergeRequest>,
    /// Offene MRs ohne meine Beteiligung laut GitLab (`count`), mindestens
    /// die geladenen („+N“, „{n} weitere laden“)
    pub total: usize,
    pub more: Vec<UninvolvedCursor>,
}

/// Eigener gemergter MR (Umschalter „Gemergt“).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedMr {
    pub mr: MergeRequest,
    /// Unix-Sekunden
    pub merged_at: Option<i64>,
    pub merged_by: Option<Person>,
}

/// Nächste Seite „Gemergt“ eines Hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedCursor {
    pub host_key: String,
    pub cursor: String,
}

/// Seite eines Hosts aus `fetch_merged`.
#[derive(Debug, Clone, PartialEq)]
pub struct MergedHostPage {
    pub host_key: String,
    /// Angemeldeter Benutzer (für „von dir“)
    pub me: Option<String>,
    pub page: Result<MergedPage, GlabErrorKind>,
}

/// Eigene gemergte MRs eines Zeitraums über alle Hosts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergedList {
    /// Zeitraum in Tagen
    pub days: u32,
    /// Beginn des Zeitraums (Unix-Sekunden); gilt auch für weitere Seiten
    pub since: i64,
    /// Zuletzt gemergte zuerst
    pub mrs: Vec<MergedMr>,
    /// Gemergt im Zeitraum laut GitLab (`count`) ohne ausgeblendete,
    /// mindestens die geladenen
    pub total: usize,
    pub more: Vec<MergedCursor>,
    /// Hosts, deren Seite scheiterte
    pub errors: Vec<(String, GlabErrorKind)>,
    /// `count` je Host (erste Seite)
    counts: BTreeMap<String, usize>,
    /// Geladen, aber durch die Repo-Auswahl (E1) ausgeblendet
    hidden: usize,
}

impl MergedList {
    pub fn new(days: u32, since: i64) -> Self {
        Self {
            days,
            since,
            ..Self::default()
        }
    }

    /// Fügt Seiten hinzu (erste oder weitere): Repo-Auswahl wie bei Meine
    /// (E1), ohne Dubletten, nur ab `since` (falls der Server `mergedAfter`
    /// nicht kennt), zuletzt gemergte zuerst. Cursor und Fehler eines Hosts
    /// ersetzt seine neue Seite.
    pub fn add_pages(
        &mut self,
        pages: Vec<MergedHostPage>,
        locals: &[LocalProject],
        cfg: &MrConfig,
    ) {
        let since = self.since;
        let selection = RepoSelection::new(locals, cfg);
        for MergedHostPage { host_key, me, page } in pages {
            self.more.retain(|c| c.host_key != host_key);
            self.errors.retain(|(h, _)| *h != host_key);
            let page = match page {
                Ok(page) => page,
                Err(kind) => {
                    self.errors.push((host_key, kind));
                    continue;
                }
            };
            let me = me.or_else(|| page.me.and_then(|m| m.username));
            if let Some(count) = page.count {
                let count = usize::try_from(count).unwrap_or(usize::MAX);
                self.counts.entry(host_key.clone()).or_insert(count);
            }
            let next = page.page.filter(|p| p.has_next_page);
            if let Some(cursor) = next.and_then(|p| p.end_cursor) {
                self.more.push(MergedCursor {
                    host_key: host_key.clone(),
                    cursor,
                });
            }
            for node in page.items {
                let merged_at = node.merged_at.as_deref().and_then(parse_rfc3339);
                if merged_at.is_some_and(|t| t < since) {
                    continue;
                }
                let Some(mut mr) = MergeRequest::from_node(&host_key, node.mr, me.as_deref())
                else {
                    continue;
                };
                if self
                    .mrs
                    .iter()
                    .any(|m| m.mr.id == mr.id && m.mr.host_key == mr.host_key)
                {
                    continue;
                }
                if selection.apply(&mut mr) != Visibility::Shown {
                    self.hidden += 1;
                    continue;
                }
                let merged_by = node
                    .merge_user
                    .as_ref()
                    .map(|u| Person::from_user(u, me.as_deref()));
                self.mrs.push(MergedMr {
                    mr,
                    merged_at,
                    merged_by,
                });
            }
        }
        self.mrs
            .sort_by_key(|m| Reverse(m.merged_at.unwrap_or(i64::MIN)));
        let counted: usize = self.counts.values().sum();
        self.total = counted.saturating_sub(self.hidden).max(self.mrs.len());
    }
}

/// Suche je Spalte: jedes Wort (ohne Groß-/Kleinschreibung) steht in Titel,
/// `!iid`, Quell- oder Zielbranch, Projektpfad oder Autor (Name,
/// Benutzername). Leere Suche passt immer.
pub fn mr_matches(mr: &MergeRequest, query: &str) -> bool {
    query.trim().is_empty() || matches_fields(query, &mr_fields(mr))
}

/// Wie `mr_matches`, zusätzlich „gemergt von“.
pub fn merged_matches(merged: &MergedMr, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let mut fields = mr_fields(&merged.mr);
    if let Some(by) = &merged.merged_by {
        fields.extend([by.name.to_lowercase(), by.username.to_lowercase()]);
    }
    matches_fields(query, &fields)
}

/// Suchfelder in Kleinbuchstaben; `!412` findet auch „412“ und „!41“.
fn mr_fields(mr: &MergeRequest) -> Vec<String> {
    [
        &mr.title,
        &mr.source_branch,
        &mr.target_branch,
        &mr.project_path,
        &mr.author.name,
        &mr.author.username,
    ]
    .iter()
    .map(|f| f.to_lowercase())
    .chain([format!("!{}", mr.iid)])
    .collect()
}

fn matches_fields(query: &str, fields: &[String]) -> bool {
    query.split_whitespace().all(|word| {
        let word = word.to_lowercase();
        fields.iter().any(|f| f.contains(&word))
    })
}

/// Stand der Ansicht nach einem Abruf.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MrSnapshot {
    /// Sortiert nach E11
    pub mine: Vec<MergeRequest>,
    /// Review-Anfragen und Erwähnungen, ungefiltert durch die Chips
    pub others: Vec<MergeRequest>,
    /// `None` = nicht abgefragt („Alle offenen“ aus)
    pub uninvolved: Option<UninvolvedList>,
    /// MRs von anderen aus abgewählten Projekten (Hinweiszeile, E1)
    pub hidden_by_selection: usize,
    /// Wartende Reviews (E9)
    pub badge: usize,
    pub per_host: Vec<HostStatus>,
    /// Unix-Sekunden des ältesten Host-Abrufs (0 ohne Hosts)
    pub fetched_at: i64,
}

/// Sichtbarkeit eines Projekts nach der Repo-Auswahl (E1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visibility {
    Shown,
    /// In `excluded_projects` abgewählt – bleibt immer ausgeblendet, auch
    /// ohne lokalen Klon
    Deselected,
    /// Ohne lokalen Klon und „Auch MRs aus Projekten ohne lokalen Klon“ aus
    NotLocal,
}

struct RepoSelection {
    /// `host/pfad` → einbezogen
    locals: HashMap<String, bool>,
    excluded: HashSet<String>,
    include_nonlocal: bool,
}

impl RepoSelection {
    fn new(locals: &[LocalProject], cfg: &MrConfig) -> Self {
        Self {
            locals: locals
                .iter()
                .map(|p| (project_key(&p.host_key, &p.full_path), p.included))
                .collect(),
            excluded: cfg
                .excluded_projects
                .iter()
                .map(|k| k.trim().to_lowercase())
                .collect(),
            include_nonlocal: cfg.include_nonlocal,
        }
    }

    /// (Sichtbarkeit, lokal geklont)
    fn of(&self, host_key: &str, full_path: &str) -> (Visibility, bool) {
        let key = project_key(host_key, full_path);
        let included = self.locals.get(&key).copied();
        let vis = if self.excluded.contains(&key) || included == Some(false) {
            Visibility::Deselected
        } else if included.is_some() || self.include_nonlocal {
            Visibility::Shown
        } else {
            Visibility::NotLocal
        };
        (vis, included.is_some())
    }

    /// Setzt `local` und liefert die Sichtbarkeit.
    fn apply(&self, mr: &mut MergeRequest) -> Visibility {
        let (vis, local) = self.of(&mr.host_key, &mr.project_path);
        mr.local = local;
        vis
    }
}

/// Meine, Von anderen und Nicht beteiligt eines Hosts vor der Repo-Auswahl.
struct HostGroups {
    mine: Vec<MergeRequest>,
    others: Vec<MergeRequest>,
    uninvolved: Vec<MergeRequest>,
}

/// Fügt den MR zu „Von anderen“ hinzu oder ergänzt den Grund beim schon
/// bekannten MR gleicher `id`.
fn merge_other(others: &mut Vec<MergeRequest>, mut mr: MergeRequest, reason: Reason) {
    match others.iter_mut().find(|o| o.id == mr.id) {
        Some(known) => known.add_reason(reason),
        None => {
            mr.add_reason(reason);
            others.push(mr);
        }
    }
}

/// Von anderen = (Review-Anfragen ∪ offene Erwähnungen) − eigene, per `id`
/// zusammengeführt; Nicht beteiligt = offene MRs der Projekte − eigene −
/// Von anderen. IDs gelten nur je Host.
fn classify_host(raw: &HostRaw, me: Option<&str>) -> HostGroups {
    let convert = |node: &MrNode| MergeRequest::from_node(&raw.host_key, node.clone(), me);
    let (authored, reviewing) = raw
        .dashboard
        .as_ref()
        .map_or((&[][..], &[][..]), |d| (&d.authored[..], &d.reviewing[..]));
    let mine: Vec<MergeRequest> = authored.iter().filter_map(convert).collect();
    let own: HashSet<String> = mine.iter().map(|m| m.id.clone()).collect();
    // Eigene MRs gewinnen, auch jenseits der ersten Seite „authored“
    let is_own = |mr: &MergeRequest| mr.author.is_me || own.contains(&mr.id);

    let mut others: Vec<MergeRequest> = Vec::new();
    let reviews = reviewing.iter().map(|n| (n, Reason::Reviewer));
    let mentions = raw
        .mentions
        .iter()
        .map(|m| (&m.mr, Reason::from_mention(m.kind)));
    for (node, reason) in reviews.chain(mentions) {
        if let Some(mr) = convert(node).filter(|mr| !is_own(mr)) {
            merge_other(&mut others, mr, reason);
        }
    }
    // Offene MRs der Projekte, bei denen ich Reviewer bin (Review-Anfrage
    // jenseits der ersten Seite oder Dashboard fehlgeschlagen), gehören
    // ebenfalls zu Von anderen
    let mut open = Vec::new();
    let projects = raw.uninvolved.iter().flatten().flat_map(|p| &p.mrs);
    for mr in projects.filter_map(convert).filter(|mr| !is_own(mr)) {
        if mr.my_review.is_some() {
            merge_other(&mut others, mr, Reason::Reviewer);
        } else {
            open.push(mr);
        }
    }
    for mr in &mut others {
        // Reviewer laut `reviewers`, z. B. Review-Anfrage jenseits der
        // ersten Seite, die nur über eine Erwähnung kam
        if mr.my_review.is_some() {
            mr.add_reason(Reason::Reviewer);
        }
        // Review-Anfrage ohne `reviewers` (altes GitLab)
        if mr.is_reviewer() && mr.my_review.is_none() {
            let approved = mr.approved_by.iter().any(|p| p.is_me);
            mr.my_review = Some(MyReview::from_state(None, approved));
        }
    }

    let involved: HashSet<&str> = others.iter().map(|m| m.id.as_str()).collect();
    let mut seen = HashSet::new();
    let uninvolved = open
        .into_iter()
        .filter(|mr| !involved.contains(mr.id.as_str()))
        .filter(|mr| seen.insert(mr.id.clone()))
        .collect();
    HostGroups {
        mine,
        others,
        uninvolved,
    }
}

/// Angemeldeter Benutzer eines Hosts: aus `me_per_host`, sonst aus dem
/// Abruf selbst (Host-Status, `currentUser` im Dashboard).
fn host_user<'a>(raw: &'a HostRaw, me_per_host: &'a BTreeMap<String, String>) -> Option<&'a str> {
    let known = |user: Option<&'a str>| user.filter(|u| !u.is_empty());
    known(me_per_host.get(&raw.host_key).map(String::as_str))
        .or_else(|| known(raw.status.user.as_deref()))
        .or_else(|| known(raw.dashboard.as_ref()?.me.as_ref()?.username.as_deref()))
}

/// Teilt die Rohdaten aller Hosts in Meine / Von anderen / Nicht beteiligt,
/// wendet die Repo-Auswahl an (E1) und sortiert (E11). `me_per_host`:
/// glab-Host-Schlüssel → angemeldeter Benutzername.
pub fn build_snapshot(
    raw_per_host: &[HostRaw],
    locals: &[LocalProject],
    cfg: &MrConfig,
    me_per_host: &BTreeMap<String, String>,
) -> MrSnapshot {
    let selection = RepoSelection::new(locals, cfg);
    let mut snap = MrSnapshot::default();
    let mut uninvolved: Option<UninvolvedList> = None;
    for raw in raw_per_host {
        let groups = classify_host(raw, host_user(raw, me_per_host));
        if let Some(projects) = &raw.uninvolved {
            let list = uninvolved.get_or_insert_with(UninvolvedList::default);
            for project in projects {
                let Some(path) = project.full_path.as_deref() else {
                    continue;
                };
                if selection.of(&raw.host_key, path).0 != Visibility::Shown {
                    continue;
                }
                // `count` enthält die MRs von anderen, an denen ich beteiligt
                // bin, und ohne `not`-Filter auch meine eigenen
                let in_project = |mr: &&MergeRequest| mr.project_path.eq_ignore_ascii_case(path);
                let mut involved = groups.others.iter().filter(in_project).count();
                if raw.uninvolved_counts_own {
                    involved += groups.mine.iter().filter(in_project).count();
                }
                let count = project.count.map_or(project.mrs.len(), |c| {
                    usize::try_from(c).unwrap_or(usize::MAX)
                });
                list.total += count.saturating_sub(involved);
                let next = project.page.as_ref().filter(|p| p.has_next_page);
                if let Some(cursor) = next.and_then(|p| p.end_cursor.clone()) {
                    list.more.push(UninvolvedCursor {
                        host_key: raw.host_key.clone(),
                        full_path: path.to_string(),
                        cursor,
                    });
                }
            }
            for mut mr in groups.uninvolved {
                if selection.apply(&mut mr) == Visibility::Shown {
                    list.mrs.push(mr);
                }
            }
        }
        for mut mr in groups.mine {
            if selection.apply(&mut mr) == Visibility::Shown {
                snap.mine.push(mr);
            }
        }
        for mut mr in groups.others {
            match selection.apply(&mut mr) {
                Visibility::Shown => snap.others.push(mr),
                Visibility::Deselected => snap.hidden_by_selection += 1,
                Visibility::NotLocal => {}
            }
        }
        snap.per_host.push(raw.status.clone());
    }
    sort_mine(&mut snap.mine, cfg.ignore_bot_approvals);
    sort_others(&mut snap.others);
    if let Some(list) = &mut uninvolved {
        sort_uninvolved(&mut list.mrs);
        list.total = list.total.max(list.mrs.len());
    }
    snap.uninvolved = uninvolved;
    // E9: Entwürfe zählen mit, Abgewählte sind schon herausgefiltert
    snap.badge = snap.others.iter().filter(|m| m.review_pending()).count();
    snap.fetched_at = raw_per_host.iter().map(|r| r.fetched_at).min().unwrap_or(0);
    snap
}

/// Zeitstempel für „zuletzt aktualisiert“; ohne Wert ganz hinten.
fn updated(mr: &MergeRequest) -> Reverse<i64> {
    Reverse(mr.updated_at.unwrap_or(i64::MIN))
}

/// E11: meiste Approvals, dann mit Regel, dann weniger fehlende, dann zuletzt
/// aktualisiert. „Mit Regel“ steht vor `left`, sonst gewönne ein MR ohne
/// Regel (0 fehlend) gegen 0/1.
pub fn sort_mine(mrs: &mut [MergeRequest], ignore_bots: bool) {
    mrs.sort_by_key(|mr| {
        let (given, required, left) = approval_counts(mr, ignore_bots);
        (Reverse(given), Reverse(required > 0), left, updated(mr))
    });
}

/// E11: offene Review-Anfragen zuerst, dann zuletzt aktualisiert.
pub fn sort_others(mrs: &mut [MergeRequest]) {
    mrs.sort_by_key(|mr| (Reverse(mr.review_pending()), updated(mr)));
}

/// E11: zuletzt aktualisiert über alle Projekte.
pub fn sort_uninvolved(mrs: &mut [MergeRequest]) {
    mrs.sort_by_key(updated);
}

/// Filter-Chips über „Von anderen“ (E13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chips {
    pub reviewer: bool,
    pub mention: bool,
    /// „Alle offenen“
    pub all: bool,
}

impl Chips {
    /// Reviewer und Erwähnt an, „Alle offenen“ aus der Einstellung.
    pub fn session_start(cfg: &MrConfig) -> Self {
        Self {
            reviewer: true,
            mention: true,
            all: cfg.show_uninvolved_default,
        }
    }

    fn shows(&self, mr: &MergeRequest) -> bool {
        (self.reviewer && mr.is_reviewer()) || (self.mention && mr.is_mentioned())
    }
}

/// ODER-Filter: ein MR bleibt, solange einer seiner Gründe eingeschaltet ist.
pub fn filter_others<'a>(others: &'a [MergeRequest], chips: &Chips) -> Vec<&'a MergeRequest> {
    others.iter().filter(|mr| chips.shows(mr)).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChipCounts {
    pub reviewer: usize,
    /// Überschneidung mit Reviewer zählt in beiden
    pub mention: usize,
    /// „+N“ erst, wenn „Nicht beteiligt“ geladen ist
    pub uninvolved: Option<usize>,
}

pub fn chip_counts(snapshot: &MrSnapshot) -> ChipCounts {
    let others = &snapshot.others;
    ChipCounts {
        reviewer: others.iter().filter(|m| m.is_reviewer()).count(),
        mention: others.iter().filter(|m| m.is_mentioned()).count(),
        uninvolved: snapshot.uninvolved.as_ref().map(|u| u.total),
    }
}

/// Kopf-Pill „Von anderen“.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderCount {
    /// Beide Chips an: „{gesamt}“
    Total(usize),
    /// „{sichtbar} von {gesamt}“
    Of { visible: usize, total: usize },
}

pub fn others_header_count(others: &[MergeRequest], chips: &Chips) -> HeaderCount {
    let total = others.len();
    if chips.reviewer && chips.mention {
        return HeaderCount::Total(total);
    }
    HeaderCount::Of {
        visible: others.iter().filter(|mr| chips.shows(mr)).count(),
        total,
    }
}

/// Leerzustand „Von anderen“ (E13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyKind {
    /// Es gibt sichtbare MRs
    NotEmpty,
    /// Keine MRs von anderen überhaupt (auch bei ausgeschalteten Chips)
    NoData,
    /// Reviewer und Erwähnt aus
    NoFilter,
    /// Filter an, aber ohne Treffer
    NoMatch,
}

pub fn empty_state(chips: &Chips, others: &[MergeRequest]) -> EmptyKind {
    if others.is_empty() {
        EmptyKind::NoData
    } else if !chips.reviewer && !chips.mention {
        EmptyKind::NoFilter
    } else if !others.iter().any(|mr| chips.shows(mr)) {
        EmptyKind::NoMatch
    } else {
        EmptyKind::NotEmpty
    }
}

/// Link zur Pipeline (E7): Ursprung aus `webUrl` ohne `webPath` (behält einen
/// Unterordner), dazu der Pipeline-Pfad. `None` bei fehlender Pipeline,
/// unsicherem Pfad, fremdem Schema oder wenn sich der Host von `webUrl`
/// ändern würde (nicht der glab-Host-Schlüssel oder Remote-Host).
pub fn pipeline_url(mr: &MergeRequest) -> Option<String> {
    let path = mr.pipeline_path.as_deref().filter(|p| safe_path(p))?;
    let (scheme, authority) = split_origin(&mr.web_url)?;
    let origin = mr
        .web_path
        .as_deref()
        .filter(|p| p.starts_with('/'))
        .and_then(|p| mr.web_url.strip_suffix(p))
        .map_or_else(|| format!("{scheme}://{authority}"), str::to_string);
    let url = format!("{origin}{path}");
    let (url_scheme, url_authority) = split_origin(&url)?;
    let same_host = url_scheme == scheme && url_authority.eq_ignore_ascii_case(authority);
    same_host.then_some(url)
}

/// E7: Pfad beginnt mit genau einem `/`, ohne `\`, `@` und Steuerzeichen.
/// Link zum MR (E12, „Im Browser öffnen“): `webUrl` nur mit Schema
/// http/https, ohne Userinfo, `\`, Leer- und Steuerzeichen; sonst `None`
/// (kein `file:`, UNC- oder Laufwerkspfad an die Shell).
pub fn mr_url(mr: &MergeRequest) -> Option<&str> {
    let url = mr.web_url.as_str();
    let clean = !url
        .chars()
        .any(|c| c == '\\' || c.is_control() || c.is_whitespace());
    (clean && split_origin(url).is_some()).then_some(url)
}

fn safe_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains(['\\', '@'])
        && !path.chars().any(char::is_control)
}

/// Schema (klein, nur http/https) und `host[:port]` einer absoluten URL;
/// Userinfo, leerer Host, `\` oder Steuerzeichen → `None`.
fn split_origin(url: &str) -> Option<(String, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let bad = |c: char| c == '@' || c == '\\' || c.is_control() || c.is_whitespace();
    if authority.is_empty() || authority.chars().any(bad) {
        return None;
    }
    Some((scheme, authority))
}

/// „mergebereit“ (E10): `detailedMergeStatus == MERGEABLE`. Ohne das Feld
/// (altes GitLab): kein Entwurf, alle Approvals da oder keine Regel,
/// Pipeline grün oder keine, keine offenen Threads.
pub fn ready(mr: &MergeRequest) -> bool {
    if let Some(status) = mr.detailed_merge_status.as_deref() {
        return status.eq_ignore_ascii_case("MERGEABLE");
    }
    // Wie GitLab: Bot-Approvals zählen
    let (_, _, left) = approval_counts(mr, false);
    !mr.draft
        && left == 0
        && matches!(
            mr.pipeline,
            PipelineState::Success | PipelineState::NoPipeline
        )
        && mr.threads_open == 0
}

/// (gegeben, erforderlich, fehlend). `left` von GitLab (kennt z. B. Code
/// Owner); fallen Bot-Approvals weg, nie weniger als die eigene Lücke.
fn approval_counts(mr: &MergeRequest, ignore_bots: bool) -> (u32, u32, u32) {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let all = count(mr.approved_by.len());
    let given = if ignore_bots {
        count(mr.approved_by.iter().filter(|p| !p.bot).count())
    } else {
        all
    };
    let required = mr.approvals_required.unwrap_or(0);
    let gap = required.saturating_sub(given);
    let left = match mr.approvals_left {
        Some(left) if given < all => left.max(gap),
        Some(left) => left,
        None => gap,
    };
    (given, required, left)
}

pub fn approvals(mr: &MergeRequest, ignore_bots: bool) -> Approvals {
    let (given, required, left) = approval_counts(mr, ignore_bots);
    let approved = |r: &Reviewer| {
        !r.person.username.is_empty()
            && mr
                .approved_by
                .iter()
                .any(|p| p.username.eq_ignore_ascii_case(&r.person.username))
    };
    // Offene Reviews nur bei eigenen MRs (design-b §3, Mockup)
    let reviewers = if mr.author.is_me {
        &mr.reviewers[..]
    } else {
        &[]
    };
    let pending_reviewers = reviewers
        .iter()
        .filter(|r| !approved(r))
        .filter_map(|r| {
            let started = match r.state.as_deref()? {
                "UNREVIEWED" => false,
                "REVIEW_STARTED" => true,
                _ => return None,
            };
            Some(PendingReviewer {
                name: r.person.name.clone(),
                started,
                is_me: r.person.is_me,
            })
        })
        .collect();
    Approvals {
        given,
        required,
        left,
        approvers: mr
            .approved_by
            .iter()
            .filter(|p| !(ignore_bots && p.bot))
            .cloned()
            .collect(),
        pending_reviewers,
    }
}

/// Anfangsbuchstaben des ersten und letzten Worts, großgeschrieben; Wörter
/// ohne Buchstaben oder Ziffern zählen nicht. Leer → „?“.
pub fn initials(name: &str) -> String {
    let firsts: Vec<char> = name
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .collect();
    let picked = match firsts.as_slice() {
        [] => return "?".to_string(),
        [only] => vec![*only],
        [first, .., last] => vec![*first, *last],
    };
    picked.into_iter().flat_map(char::to_uppercase).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitlab::graphql::{MergedNode, PageInfo, PipelineNode, ReviewerNode, UserNode};
    use std::path::PathBuf;

    const HOST: &str = "gitlab.example.com";
    const ME: &str = "taltmann";

    fn name_of(username: &str) -> &'static str {
        match username {
            "taltmann" => "Tim Altmann",
            "mbecker" => "Miriam Becker",
            "syilmaz" => "Sara Yilmaz",
            "jkrueger" => "Jonas Krüger",
            "lhoffmann" => "Lukas Hoffmann",
            "pschmid" => "Petra Schmid",
            "dwagner" => "David Wagner",
            "alehmann" => "Anna Lehmann",
            "fbraun" => "Felix Braun",
            "renovate-bot" => "Renovate Bot",
            _ => "Unbekannt",
        }
    }

    fn user(username: &str, bot: bool) -> UserNode {
        UserNode {
            username: Some(username.to_string()),
            name: Some(name_of(username).to_string()),
            bot: Some(bot),
        }
    }

    /// Baukasten für GraphQL-Knoten mit den Beispieldaten aus
    /// `anhang/design-brief.md` §5 (Stand 2026-10-17 10:00 UTC).
    struct Mr(MrNode);

    impl Mr {
        fn new(iid: u64, project: &str, author: &str, updated: &str) -> Self {
            Self(MrNode {
                id: Some(format!("gid://gitlab/MergeRequest/{iid}")),
                iid: Some(iid),
                title: Some(format!("MR {iid}")),
                draft: Some(false),
                web_url: Some(format!("https://{HOST}/{project}/-/merge_requests/{iid}")),
                web_path: Some(format!("/{project}/-/merge_requests/{iid}")),
                updated_at: Some(updated.to_string()),
                source_branch: Some(format!("feature/mr-{iid}")),
                target_branch: Some("main".to_string()),
                source_branch_exists: Some(true),
                diff_head_sha: Some("0".repeat(40)),
                project_path: Some(project.to_string()),
                source_project_path: Some(project.to_string()),
                author: Some(user(author, false)),
                approved_by: Some(Vec::new()),
                approvals_required: Some(0),
                approvals_left: Some(0),
                head_pipeline: None,
                resolvable_discussions: Some(0),
                resolved_discussions: Some(0),
                detailed_merge_status: None,
                reviewers: Some(Vec::new()),
                state: None,
            })
        }

        /// Approvals von Menschen; `approvalsLeft` wie GitLab.
        fn approved(mut self, by: &[&str], required: u32) -> Self {
            let list = self.0.approved_by.get_or_insert_with(Vec::new);
            list.extend(by.iter().map(|u| user(u, false)));
            let given = list.len() as u32;
            self.0.approvals_required = Some(required);
            self.0.approvals_left = Some(required.saturating_sub(given));
            self
        }

        /// Bot-Approval; GitLab zählt es bei `approvalsLeft` mit.
        fn bot_approval(mut self, by: &str) -> Self {
            let list = self.0.approved_by.get_or_insert_with(Vec::new);
            list.push(user(by, true));
            let given = list.len() as u32;
            let required = self.0.approvals_required.unwrap_or(0);
            self.0.approvals_left = Some(required.saturating_sub(given));
            self
        }

        /// GitLab CE: keine Approval-Regeln (`undefinedField`).
        fn ce(mut self) -> Self {
            self.0.approvals_required = None;
            self.0.approvals_left = None;
            self
        }

        fn reviewer(mut self, username: &str, state: &str) -> Self {
            self.0
                .reviewers
                .get_or_insert_with(Vec::new)
                .push(ReviewerNode {
                    username: Some(username.to_string()),
                    name: Some(name_of(username).to_string()),
                    review_state: Some(state.to_string()),
                });
            self
        }

        fn pipeline(mut self, status: &str) -> Self {
            let project = self.0.project_path.clone().unwrap_or_default();
            let path = format!("/{project}/-/pipelines/{}", 9000 + self.0.iid.unwrap_or(0));
            self.0.head_pipeline = Some(PipelineNode {
                status: Some(status.to_string()),
                path: Some(path),
            });
            self
        }

        fn threads(mut self, open: u32, resolved: u32) -> Self {
            self.0.resolvable_discussions = Some(open + resolved);
            self.0.resolved_discussions = Some(resolved);
            self
        }

        fn draft(mut self) -> Self {
            self.0.draft = Some(true);
            self
        }

        fn merge_status(mut self, status: &str) -> Self {
            self.0.detailed_merge_status = Some(status.to_string());
            self
        }

        fn node(self) -> MrNode {
            self.0
        }

        fn mr(self) -> MergeRequest {
            MergeRequest::from_node(HOST, self.0, Some(ME)).expect("gültiger MR")
        }
    }

    /// Meine (design-brief §5), absichtlich nicht in Sortierreihenfolge.
    fn mine_nodes() -> Vec<MrNode> {
        vec![
            Mr::new(305, "plattform/ci-templates", ME, "2026-10-15T09:00:00Z")
                .merge_status("MERGEABLE")
                .node(),
            Mr::new(233, "kern/dokumentenservice", ME, "2026-10-17T07:00:00Z")
                .approved(&["mbecker"], 2)
                .reviewer("mbecker", "APPROVED")
                .reviewer("dwagner", "UNREVIEWED")
                .pipeline("FAILED")
                .merge_status("CI_MUST_PASS")
                .node(),
            Mr::new(19, "tools/gitmanager", ME, "2026-10-16T09:00:00Z")
                .approved(&[], 1)
                .reviewer("syilmaz", "UNREVIEWED")
                .pipeline("PENDING")
                .threads(1, 0)
                .draft()
                .merge_status("DRAFT_STATUS")
                .node(),
            Mr::new(412, "kern/backend-api", ME, "2026-10-17T09:35:00Z")
                .approved(&["mbecker", "syilmaz"], 2)
                .reviewer("mbecker", "APPROVED")
                .reviewer("syilmaz", "APPROVED")
                .pipeline("SUCCESS")
                .threads(0, 3)
                .merge_status("MERGEABLE")
                .node(),
            Mr::new(87, "web/kundenportal", ME, "2026-10-17T09:00:00Z")
                .approved(&["jkrueger"], 2)
                .reviewer("jkrueger", "APPROVED")
                .reviewer("mbecker", "REVIEW_STARTED")
                .pipeline("RUNNING")
                .threads(2, 0)
                .merge_status("NOT_APPROVED")
                .node(),
        ]
    }

    /// Review-Anfragen an mich (design-brief §5, „Von anderen“).
    fn reviewing_nodes() -> Vec<MrNode> {
        vec![
            Mr::new(92, "web/kundenportal", "jkrueger", "2026-10-17T08:00:00Z")
                .approved(&[ME, "syilmaz"], 2)
                .reviewer(ME, "APPROVED")
                .pipeline("SUCCESS")
                .threads(0, 2)
                .node(),
            Mr::new(421, "kern/backend-api", "dwagner", "2026-10-17T09:55:00Z")
                .approved(&[], 2)
                .reviewer(ME, "UNREVIEWED")
                .pipeline("PENDING")
                .node(),
            Mr::new(
                240,
                "kern/dokumentenservice",
                "mbecker",
                "2026-10-17T06:00:00Z",
            )
            .approved(&[], 2)
            .reviewer(ME, "REQUESTED_CHANGES")
            .pipeline("FAILED")
            .threads(4, 0)
            .node(),
            Mr::new(418, "kern/backend-api", "syilmaz", "2026-10-17T09:50:00Z")
                .approved(&["jkrueger"], 2)
                .reviewer(ME, "REVIEW_STARTED")
                .pipeline("SUCCESS")
                .threads(3, 0)
                .node(),
        ]
    }

    fn mention(node: MrNode, kind: MentionKind) -> MentionNode {
        MentionNode {
            todo_id: Some(format!("gid://gitlab/Todo/{}", node.iid.unwrap_or(0))),
            action: Some("mentioned".to_string()),
            kind: Some(kind),
            mr: node,
        }
    }

    fn mention_nodes() -> Vec<MentionNode> {
        vec![
            mention(
                Mr::new(
                    240,
                    "kern/dokumentenservice",
                    "mbecker",
                    "2026-10-17T06:00:00Z",
                )
                .approved(&[], 2)
                .reviewer(ME, "REQUESTED_CHANGES")
                .pipeline("FAILED")
                .threads(4, 0)
                .node(),
                MentionKind::Comment,
            ),
            mention(
                Mr::new(
                    77,
                    "plattform/auth-service",
                    "lhoffmann",
                    "2026-10-16T08:00:00Z",
                )
                .approved(&["pschmid"], 2)
                .pipeline("RUNNING")
                .threads(1, 0)
                .node(),
                MentionKind::Description,
            ),
            mention(
                Mr::new(
                    310,
                    "plattform/ci-templates",
                    "pschmid",
                    "2026-10-15T08:00:00Z",
                )
                .approved(&["dwagner", "fbraun"], 2)
                .pipeline("SUCCESS")
                .node(),
                MentionKind::Comment,
            ),
        ]
    }

    fn project_mrs(path: &str, count: u32, more: Option<&str>, mrs: Vec<MrNode>) -> ProjectMrs {
        ProjectMrs {
            full_path: Some(path.to_string()),
            count: Some(count),
            page: Some(PageInfo {
                has_next_page: more.is_some(),
                end_cursor: Some(more.unwrap_or("ende").to_string()),
            }),
            mrs,
        }
    }

    /// Offene MRs anderer in den lokalen Projekten (ohne eigene, wie `not`).
    fn uninvolved_projects() -> Vec<ProjectMrs> {
        let others = reviewing_nodes();
        let find = |iid: u64| {
            others
                .iter()
                .find(|n| n.iid == Some(iid))
                .cloned()
                .expect("Beispiel-MR")
        };
        vec![
            project_mrs(
                "web/kundenportal",
                2,
                None,
                vec![
                    Mr::new(95, "web/kundenportal", "alehmann", "2026-10-17T09:30:00Z")
                        .approved(&["jkrueger"], 2)
                        .pipeline("SUCCESS")
                        .node(),
                    find(92),
                ],
            ),
            project_mrs(
                "kern/backend-api",
                5,
                Some("c-backend"),
                vec![
                    find(421),
                    Mr::new(423, "kern/backend-api", "fbraun", "2026-10-17T09:00:00Z")
                        .approved(&[], 2)
                        .pipeline("RUNNING")
                        .node(),
                    find(418),
                ],
            ),
            project_mrs(
                "kern/dokumentenservice",
                2,
                None,
                vec![
                    Mr::new(
                        242,
                        "kern/dokumentenservice",
                        "mbecker",
                        "2026-10-16T07:00:00Z",
                    )
                    .approved(&["dwagner", "fbraun"], 2)
                    .pipeline("SUCCESS")
                    .threads(1, 0)
                    .node(),
                    find(240),
                ],
            ),
            project_mrs(
                "plattform/ci-templates",
                2,
                None,
                vec![Mr::new(
                    311,
                    "plattform/ci-templates",
                    "pschmid",
                    "2026-10-14T09:00:00Z",
                )
                .approved(&[], 1)
                .pipeline("FAILED")
                .node()],
            ),
            project_mrs("tools/gitmanager", 0, None, Vec::new()),
        ]
    }

    fn dashboard(authored: Vec<MrNode>, reviewing: Vec<MrNode>) -> Dashboard {
        Dashboard {
            version: Some("18.5.1-ee".to_string()),
            enterprise: Some(true),
            me: Some(user(ME, false)),
            authored,
            authored_page: None,
            reviewing,
            reviewing_page: None,
        }
    }

    fn host_raw(uninvolved: bool) -> HostRaw {
        HostRaw {
            host_key: HOST.to_string(),
            dashboard: Some(dashboard(mine_nodes(), reviewing_nodes())),
            mentions: mention_nodes(),
            uninvolved: uninvolved.then(uninvolved_projects),
            uninvolved_counts_own: false,
            status: HostStatus {
                host_key: HOST.to_string(),
                user: Some(ME.to_string()),
                ..HostStatus::default()
            },
            fetched_at: 1_792_231_200,
        }
    }

    fn local(path: &str, included: bool) -> LocalProject {
        LocalProject {
            host_key: HOST.to_string(),
            full_path: path.to_string(),
            repos: vec![PathBuf::from(format!(
                "C:\\Dev\\{}",
                path.rsplit('/').next().unwrap()
            ))],
            included,
        }
    }

    /// Lokale Klone aus design-brief §5; plattform/auth-service ist nicht lokal.
    fn locals(excluded: &[&str]) -> Vec<LocalProject> {
        [
            "kern/backend-api",
            "kern/dokumentenservice",
            "plattform/ci-templates",
            "tools/gitmanager",
            "web/kundenportal",
        ]
        .iter()
        .map(|p| local(p, !excluded.contains(p)))
        .collect()
    }

    fn me_map() -> BTreeMap<String, String> {
        BTreeMap::from([(HOST.to_string(), ME.to_string())])
    }

    fn iids(mrs: &[MergeRequest]) -> Vec<u64> {
        mrs.iter().map(|m| m.iid).collect()
    }

    fn iids_of(mrs: &[&MergeRequest]) -> Vec<u64> {
        mrs.iter().map(|m| m.iid).collect()
    }

    fn sample_snapshot(uninvolved: bool) -> MrSnapshot {
        build_snapshot(
            &[host_raw(uninvolved)],
            &locals(&[]),
            &MrConfig::default(),
            &me_map(),
        )
    }

    fn by_iid(mrs: &[MergeRequest], iid: u64) -> &MergeRequest {
        mrs.iter()
            .find(|m| m.iid == iid)
            .unwrap_or_else(|| panic!("!{iid} fehlt"))
    }

    // --- Sortierung (E11) ---

    #[test]
    fn sort_mine_design_brief_order() {
        let mut mine: Vec<MergeRequest> = mine_nodes().into_iter().map(|n| Mr(n).mr()).collect();
        sort_mine(&mut mine, true);
        assert_eq!(iids(&mine), vec![412, 87, 233, 19, 305]);
        // Unabhängig von der Eingangsreihenfolge
        mine.reverse();
        sort_mine(&mut mine, false);
        assert_eq!(iids(&mine), vec![412, 87, 233, 19, 305]);
    }

    #[test]
    fn sort_mine_ignores_bot_approvals_when_set() {
        // !19 bekommt ein Bot-Approval: 0/1 (+Bot)
        let mut nodes = mine_nodes();
        let i = nodes.iter().position(|n| n.iid == Some(19)).unwrap();
        nodes[i] = Mr(nodes[i].clone()).bot_approval("renovate-bot").node();
        let mut mine: Vec<MergeRequest> = nodes.into_iter().map(|n| Mr(n).mr()).collect();

        sort_mine(&mut mine, true);
        assert_eq!(iids(&mine), vec![412, 87, 233, 19, 305]);
        let a = approvals(by_iid(&mine, 19), true);
        assert_eq!((a.given, a.required, a.left), (0, 1, 1));
        assert!(a.approvers.is_empty());

        // Bots zählen mit: !19 hat 1 Approval und keins mehr offen → vor !87
        sort_mine(&mut mine, false);
        assert_eq!(iids(&mine), vec![412, 19, 87, 233, 305]);
        let a = approvals(by_iid(&mine, 19), false);
        assert_eq!((a.given, a.required, a.left), (1, 1, 0));
        assert_eq!(a.approvers[0].name, "Renovate Bot");
    }

    #[test]
    fn sort_mine_rule_before_left_but_after_given() {
        // Ohne Regel mit 2 Approvals vor einer Regel mit 0/2: Approvals bleiben vorn
        let mut mine = vec![
            Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
                .approved(&[], 2)
                .mr(),
            Mr::new(2, "g/p", ME, "2026-10-10T09:00:00Z")
                .approved(&["mbecker", "syilmaz"], 0)
                .mr(),
            // Gleichstand bei 0: Regel 0/2 vor „keine Regel“, obwohl neuer
            Mr::new(3, "g/p", ME, "2026-10-17T09:59:00Z").mr(),
        ];
        sort_mine(&mut mine, true);
        assert_eq!(iids(&mine), vec![2, 1, 3]);
    }

    #[test]
    fn sort_mine_ce_without_rule_uses_updated() {
        // CE: keine Regeln → nur Approvals und Aktualisierung; !305 ist hier
        // neuer als !19 und rückt deshalb vor (mit Regel stünde !19 vorn)
        let mut mine: Vec<MergeRequest> = mine_nodes()
            .into_iter()
            .map(|n| {
                let mut node = Mr(n).ce().node();
                if node.iid == Some(305) {
                    node.updated_at = Some("2026-10-17T05:00:00Z".to_string());
                }
                Mr(node).mr()
            })
            .collect();
        sort_mine(&mut mine, true);
        assert_eq!(iids(&mine), vec![412, 87, 233, 305, 19]);
        let a = approvals(by_iid(&mine, 412), true);
        assert_eq!((a.given, a.required, a.left), (2, 0, 0));
        assert!(!a.has_rule());
    }

    #[test]
    fn sort_others_pending_reviews_first() {
        let with_reason = |node: MrNode, reason: Reason| {
            let mut mr = Mr(node).mr();
            mr.reasons = vec![reason];
            mr
        };
        let mut others: Vec<MergeRequest> = reviewing_nodes()
            .into_iter()
            .map(|n| with_reason(n, Reason::Reviewer))
            .chain(
                mention_nodes()
                    .into_iter()
                    .skip(1)
                    .map(|m| with_reason(m.mr, Reason::MentionComment)),
            )
            .collect();
        sort_others(&mut others);
        assert_eq!(iids(&others), vec![421, 418, 92, 240, 77, 310]);
        // Offene Review-Anfrage bleibt vorn, auch wenn sie älter ist
        others.iter_mut().find(|m| m.iid == 421).unwrap().updated_at =
            parse_rfc3339("2026-10-10T09:00:00Z");
        sort_others(&mut others);
        assert_eq!(iids(&others), vec![418, 421, 92, 240, 77, 310]);
    }

    #[test]
    fn sort_uninvolved_by_updated() {
        let mut list: Vec<MergeRequest> = uninvolved_projects()
            .into_iter()
            .flat_map(|p| p.mrs)
            .map(|n| Mr(n).mr())
            .collect();
        sort_uninvolved(&mut list);
        assert_eq!(iids(&list), vec![421, 418, 95, 423, 92, 240, 242, 311]);
    }

    // --- Einteilung (E1) ---

    #[test]
    fn build_snapshot_design_brief() {
        let snap = sample_snapshot(true);
        assert_eq!(iids(&snap.mine), vec![412, 87, 233, 19, 305]);
        assert_eq!(iids(&snap.others), vec![421, 418, 92, 240, 77, 310]);
        assert_eq!(
            by_iid(&snap.others, 240).reasons,
            vec![Reason::Reviewer, Reason::MentionComment]
        );
        assert_eq!(by_iid(&snap.others, 421).reasons, vec![Reason::Reviewer]);
        assert_eq!(
            by_iid(&snap.others, 77).reasons,
            vec![Reason::MentionDescription]
        );
        assert_eq!(
            by_iid(&snap.others, 310).reasons,
            vec![Reason::MentionComment]
        );
        assert_eq!(by_iid(&snap.others, 421).my_review, Some(MyReview::Pending));
        assert_eq!(by_iid(&snap.others, 92).my_review, Some(MyReview::Approved));
        assert_eq!(
            by_iid(&snap.others, 240).my_review,
            Some(MyReview::ChangesRequested)
        );
        assert_eq!(by_iid(&snap.others, 77).my_review, None);
        // „nicht lokal“
        assert!(!by_iid(&snap.others, 77).local);
        assert!(by_iid(&snap.others, 421).local);
        assert!(snap.mine.iter().all(|m| m.local));
        // Nicht beteiligt: ohne Meine und ohne Von anderen
        let un = snap.uninvolved.as_ref().expect("abgefragt");
        assert_eq!(iids(&un.mrs), vec![95, 423, 242, 311]);
        // 11 offene laut GitLab − 5 beteiligte (92, 421, 418, 240, 310)
        assert_eq!(un.total, 6);
        assert_eq!(
            un.more,
            vec![UninvolvedCursor {
                host_key: HOST.to_string(),
                full_path: "kern/backend-api".to_string(),
                cursor: "c-backend".to_string(),
            }]
        );
        assert_eq!(snap.badge, 2);
        assert_eq!(snap.hidden_by_selection, 0);
        assert_eq!(snap.fetched_at, 1_792_231_200);
        assert_eq!(snap.per_host.len(), 1);
        assert_eq!(snap.per_host[0].host_key, HOST);
    }

    #[test]
    fn build_snapshot_without_uninvolved() {
        let snap = sample_snapshot(false);
        assert_eq!(snap.uninvolved, None);
        assert_eq!(chip_counts(&snap).uninvolved, None);
    }

    #[test]
    fn own_mr_with_mention_stays_in_mine() {
        let mut raw = host_raw(false);
        let own = mine_nodes()
            .into_iter()
            .find(|n| n.iid == Some(412))
            .unwrap();
        raw.mentions
            .push(mention(own.clone(), MentionKind::Comment));
        // Eigene MR außerhalb der ersten 50 „authored“: Autor entscheidet
        let mut own_old = Mr::new(7, "kern/backend-api", ME, "2026-09-01T00:00:00Z").node();
        own_old.author = Some(UserNode {
            username: Some("TAltmann".to_string()),
            name: Some("Tim Altmann".to_string()),
            bot: Some(false),
        });
        raw.mentions
            .push(mention(own_old, MentionKind::Description));
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &me_map());
        assert_eq!(iids(&snap.mine), vec![412, 87, 233, 19, 305]);
        assert!(!snap.others.iter().any(|m| m.iid == 412 || m.iid == 7));
        assert!(by_iid(&snap.mine, 412).reasons.is_empty());
    }

    #[test]
    fn mentions_merge_by_id_with_both_kinds() {
        let mut raw = host_raw(false);
        let mr77 = raw.mentions[1].mr.clone();
        raw.mentions.push(mention(mr77, MentionKind::Comment));
        // Gleiche Erwähnungsart doppelt (zwei To-dos) zählt einmal
        let mr310 = raw.mentions[2].mr.clone();
        raw.mentions.push(mention(mr310, MentionKind::Comment));
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &me_map());
        assert_eq!(iids(&snap.others), vec![421, 418, 92, 240, 77, 310]);
        assert_eq!(
            by_iid(&snap.others, 77).reasons,
            vec![Reason::MentionDescription, Reason::MentionComment]
        );
        assert_eq!(
            by_iid(&snap.others, 310).reasons,
            vec![Reason::MentionComment]
        );
    }

    #[test]
    fn mention_with_me_as_reviewer_counts_as_reviewer() {
        // Review-Anfrage jenseits der ersten 50, aber per Erwähnung geladen
        let mut raw = host_raw(false);
        raw.mentions.push(mention(
            Mr::new(500, "kern/backend-api", "dwagner", "2026-10-17T09:58:00Z")
                .reviewer(ME, "UNREVIEWED")
                .node(),
            MentionKind::Description,
        ));
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &me_map());
        let mr = by_iid(&snap.others, 500);
        assert_eq!(
            mr.reasons,
            vec![Reason::Reviewer, Reason::MentionDescription]
        );
        assert_eq!(mr.my_review, Some(MyReview::Pending));
        assert_eq!(snap.others[0].iid, 500);
        assert_eq!(snap.badge, 3);
    }

    #[test]
    fn reviewing_without_review_state_is_pending() {
        // Altes GitLab ohne `reviewers`: Review-Anfrage gilt als offen,
        // außer ich habe schon approved
        let mut open = Mr::new(600, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z").node();
        open.reviewers = None;
        let mut done = Mr::new(601, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z")
            .approved(&[ME], 1)
            .node();
        done.reviewers = None;
        let raw = HostRaw {
            host_key: HOST.to_string(),
            dashboard: Some(dashboard(Vec::new(), vec![open, done])),
            ..HostRaw::default()
        };
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &me_map());
        assert_eq!(by_iid(&snap.others, 600).my_review, Some(MyReview::Pending));
        assert_eq!(
            by_iid(&snap.others, 601).my_review,
            Some(MyReview::Approved)
        );
        assert_eq!(snap.badge, 1);
    }

    #[test]
    fn selection_hides_deselected_and_counts_them() {
        // kern/backend-api abgewählt (lokal), plattform/auth-service abgewählt
        // und nicht lokal: beide bleiben trotz include_nonlocal ausgeblendet
        let cfg = MrConfig {
            excluded_projects: vec![
                "gitlab.example.com/kern/backend-api".to_string(),
                "gitlab.example.com/plattform/auth-service".to_string(),
            ],
            include_nonlocal: true,
            ..MrConfig::default()
        };
        let snap = build_snapshot(
            &[host_raw(true)],
            &locals(&["kern/backend-api"]),
            &cfg,
            &me_map(),
        );
        assert_eq!(iids(&snap.mine), vec![87, 233, 19, 305]);
        assert_eq!(iids(&snap.others), vec![92, 240, 310]);
        // !421, !418 (backend-api) und !77 (auth-service)
        assert_eq!(snap.hidden_by_selection, 3);
        // Badge zählt Abgewählte nicht (!421 und !418 warten eigentlich)
        assert_eq!(snap.badge, 0);
        let un = snap.uninvolved.as_ref().unwrap();
        assert_eq!(iids(&un.mrs), vec![95, 242, 311]);
        // 6 offene in den einbezogenen Projekten − 3 beteiligte
        assert_eq!(un.total, 3);
        assert!(un.more.is_empty());
    }

    #[test]
    fn selection_excluded_key_is_case_insensitive() {
        let cfg = MrConfig {
            excluded_projects: vec![" GitLab.Example.com/Kern/Backend-API ".to_string()],
            ..MrConfig::default()
        };
        let mut raw = host_raw(false);
        // GitLab liefert den Pfad in Original-Schreibweise
        for node in &mut raw.dashboard.as_mut().unwrap().reviewing {
            if node.iid == Some(421) {
                node.project_path = Some("Kern/Backend-API".to_string());
            }
        }
        let snap = build_snapshot(&[raw], &locals(&[]), &cfg, &me_map());
        assert!(!snap.others.iter().any(|m| m.iid == 421 || m.iid == 418));
        assert_eq!(snap.hidden_by_selection, 2);
    }

    #[test]
    fn nonlocal_hidden_without_option_but_not_counted() {
        let cfg = MrConfig {
            include_nonlocal: false,
            ..MrConfig::default()
        };
        let snap = build_snapshot(&[host_raw(false)], &locals(&[]), &cfg, &me_map());
        assert_eq!(iids(&snap.others), vec![421, 418, 92, 240, 310]);
        // Nicht abgewählt, nur nicht lokal → keine Hinweiszeile
        assert_eq!(snap.hidden_by_selection, 0);
        assert_eq!(snap.badge, 2);
    }

    #[test]
    fn ids_are_per_host() {
        // Gleiche globale ID auf zwei Hosts: zwei verschiedene MRs
        let other_host = "gitlab.internal:8443";
        let node = Mr::new(421, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z")
            .reviewer(ME, "UNREVIEWED")
            .node();
        let raw_b = HostRaw {
            host_key: other_host.to_string(),
            dashboard: Some(dashboard(Vec::new(), vec![node])),
            status: HostStatus {
                host_key: other_host.to_string(),
                ..HostStatus::default()
            },
            fetched_at: 1_792_231_100,
            ..HostRaw::default()
        };
        let mut locals = locals(&[]);
        locals.push(LocalProject {
            host_key: other_host.to_string(),
            ..local("kern/backend-api", true)
        });
        let mut me = me_map();
        me.insert(other_host.to_string(), ME.to_string());
        let snap = build_snapshot(
            &[host_raw(false), raw_b],
            &locals,
            &MrConfig::default(),
            &me,
        );
        let hosts: Vec<&str> = snap
            .others
            .iter()
            .filter(|m| m.iid == 421)
            .map(|m| m.host_key.as_str())
            .collect();
        assert_eq!(hosts.len(), 2);
        assert!(hosts.contains(&other_host));
        assert_eq!(snap.badge, 3);
        // Stand = ältester Abruf
        assert_eq!(snap.fetched_at, 1_792_231_100);
        assert_eq!(snap.per_host.len(), 2);
    }

    #[test]
    fn host_without_data_contributes_nothing() {
        // Anmeldefehler: kein Dashboard → keine MRs, Badge 0, Status bleibt
        let raw = HostRaw {
            host_key: HOST.to_string(),
            status: HostStatus {
                host_key: HOST.to_string(),
                error: Some(GlabErrorKind::InvalidToken),
                ..HostStatus::default()
            },
            ..HostRaw::default()
        };
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &BTreeMap::new());
        assert!(snap.mine.is_empty() && snap.others.is_empty());
        assert_eq!(snap.badge, 0);
        assert_eq!(snap.per_host[0].error, Some(GlabErrorKind::InvalidToken));
        assert_eq!(
            build_snapshot(&[], &[], &MrConfig::default(), &BTreeMap::new()).fetched_at,
            0
        );
    }

    #[test]
    fn partial_failures_keep_remaining_data() {
        // Erwähnungen (403) und „Nicht beteiligt“ fehlgeschlagen: Dashboard bleibt
        let mut raw = host_raw(false);
        raw.mentions.clear();
        raw.status.partial = vec![
            (MrPart::Mentions, GlabErrorKind::InsufficientScope),
            (MrPart::Uninvolved, GlabErrorKind::Timeout),
        ];
        let partial = raw.status.partial.clone();
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &me_map());
        assert_eq!(iids(&snap.mine), vec![412, 87, 233, 19, 305]);
        assert_eq!(iids(&snap.others), vec![421, 418, 92, 240]);
        assert_eq!(snap.uninvolved, None);
        assert_eq!(snap.per_host[0].partial, partial);
        // Dashboard fehlgeschlagen: Erwähnungen bleiben
        let mut raw = host_raw(false);
        raw.dashboard = None;
        raw.status.partial = vec![(MrPart::Dashboard, GlabErrorKind::Other)];
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &me_map());
        assert!(snap.mine.is_empty());
        assert_eq!(iids(&snap.others), vec![240, 77, 310]);
        assert_eq!(snap.badge, 0);
    }

    #[test]
    fn me_falls_back_to_fetched_user() {
        // Ohne Eintrag in `me_per_host`: Benutzer aus Host-Status bzw. Dashboard
        let expected = sample_snapshot(false);
        let mut raw = host_raw(false);
        let snap = build_snapshot(
            std::slice::from_ref(&raw),
            &locals(&[]),
            &MrConfig::default(),
            &BTreeMap::new(),
        );
        assert_eq!(snap, expected);
        raw.status.user = None;
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &BTreeMap::new());
        assert_eq!(snap.mine, expected.mine);
        assert_eq!(snap.others, expected.others);
        // !92 bleibt „Von dir approved“, nur !421 und !418 warten
        assert_eq!(by_iid(&snap.others, 92).my_review, Some(MyReview::Approved));
        assert_eq!(snap.badge, 2);
    }

    #[test]
    fn uninvolved_with_me_as_reviewer_moves_to_others() {
        // Dashboard fehlgeschlagen: Review-Anfragen kommen nur über die
        // offenen MRs der Projekte und gehören trotzdem zu Von anderen
        let mut raw = host_raw(true);
        raw.dashboard = None;
        raw.mentions.clear();
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &me_map());
        assert_eq!(iids(&snap.others), vec![421, 418, 92, 240]);
        assert!(snap
            .others
            .iter()
            .all(|m| m.reasons == vec![Reason::Reviewer]));
        assert_eq!(snap.badge, 2);
        let un = snap.uninvolved.as_ref().unwrap();
        assert_eq!(iids(&un.mrs), vec![95, 423, 242, 311]);
        // 11 offene − 4 beteiligte (92, 421, 418, 240)
        assert_eq!(un.total, 7);
        assert_eq!(chip_counts(&snap).reviewer, 4);
    }

    #[test]
    fn build_snapshot_from_parsed_fixtures() {
        use crate::gitlab::graphql::{parse_dashboard, parse_mentions, parse_uninvolved};
        let data = |fixture: &str| -> serde_json::Value {
            serde_json::from_str::<serde_json::Value>(fixture).expect("Fixture ist gültiges JSON")
                ["data"]
                .clone()
        };
        let mut raw = HostRaw {
            host_key: HOST.to_string(),
            dashboard: Some(parse_dashboard(&data(include_str!(
                "fixtures/dashboard_ee.json"
            )))),
            mentions: parse_mentions(&data(include_str!("fixtures/mentions.json"))).items,
            uninvolved: Some(parse_uninvolved(
                &data(include_str!("fixtures/uninvolved.json")),
                ME,
            )),
            // Die Fixture stammt von einem Server ohne `not`-Filter (!99)
            uninvolved_counts_own: true,
            ..HostRaw::default()
        };
        let snap = build_snapshot(
            std::slice::from_ref(&raw),
            &locals(&[]),
            &MrConfig::default(),
            &me_map(),
        );
        // !305 hat nur ein Bot-Approval (ohne Bots: 0)
        assert_eq!(iids(&snap.mine), vec![412, 87, 305]);
        assert_eq!(iids(&snap.others), vec![421, 92, 77, 310]);
        // Zwei To-dos zu !77: Beschreibung und Kommentar
        let mr77 = by_iid(&snap.others, 77);
        assert_eq!(
            mr77.reasons,
            vec![Reason::MentionDescription, Reason::MentionComment]
        );
        assert!(!mr77.local);
        assert_eq!(by_iid(&snap.others, 92).my_review, Some(MyReview::Approved));
        let un = snap.uninvolved.as_ref().unwrap();
        // !99 ist meiner (TAltmann), !423 ohne `project` erbt den Projektpfad
        assert_eq!(iids(&un.mrs), vec![95, 423]);
        assert_eq!(un.mrs[1].project_path, "kern/backend-api");
        // web/kundenportal: 5 offene inkl. meiner − !92 − eigener !87;
        // kern/backend-api: 1 − !421 − eigener !412 (nie unter 0)
        assert_eq!(un.total, 3);
        assert_eq!(un.more.len(), 1);
        assert_eq!(un.more[0].full_path, "web/kundenportal");
        assert_eq!(snap.badge, 1);
        assert!(ready(by_iid(&snap.mine, 412)));
        assert_eq!(
            pipeline_url(by_iid(&snap.mine, 412)).as_deref(),
            Some("https://gitlab.example.com/kern/backend-api/-/pipelines/9001")
        );
        // Mit `not`-Filter zählt `count` nur MRs anderer: 5 − !92
        raw.uninvolved_counts_own = false;
        let snap = build_snapshot(&[raw], &locals(&[]), &MrConfig::default(), &me_map());
        assert_eq!(snap.uninvolved.unwrap().total, 4);
    }

    // --- Badge (E9) ---

    #[test]
    fn badge_counts_pending_reviews_including_drafts() {
        let reviewing = vec![
            Mr::new(1, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z")
                .reviewer(ME, "UNREVIEWED")
                .draft()
                .node(),
            Mr::new(2, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z")
                .reviewer(ME, "REVIEW_STARTED")
                .node(),
            Mr::new(3, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z")
                .reviewer(ME, "APPROVED")
                .node(),
            Mr::new(4, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z")
                .reviewer(ME, "REQUESTED_CHANGES")
                .node(),
            Mr::new(5, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z")
                .reviewer(ME, "REVIEWED")
                .node(),
            Mr::new(6, "web/kundenportal", "dwagner", "2026-10-17T09:00:00Z")
                .reviewer(ME, "UNREVIEWED")
                .node(),
        ];
        let raw = HostRaw {
            host_key: HOST.to_string(),
            dashboard: Some(dashboard(Vec::new(), reviewing)),
            // Erwähnt, aber nicht Reviewer → kein Review offen
            mentions: vec![mention(
                Mr::new(7, "kern/backend-api", "dwagner", "2026-10-17T09:00:00Z").node(),
                MentionKind::Comment,
            )],
            ..HostRaw::default()
        };
        let cfg = MrConfig {
            excluded_projects: vec!["gitlab.example.com/web/kundenportal".to_string()],
            ..MrConfig::default()
        };
        let snap = build_snapshot(&[raw], &locals(&["web/kundenportal"]), &cfg, &me_map());
        assert_eq!(snap.badge, 2);
        assert_eq!(by_iid(&snap.others, 5).my_review, Some(MyReview::Commented));
    }

    #[test]
    fn my_review_states() {
        let state = |s: &str| {
            Mr::new(1, "g/p", "dwagner", "2026-10-17T09:00:00Z")
                .reviewer(ME, s)
                .mr()
                .my_review
        };
        assert_eq!(state("UNREVIEWED"), Some(MyReview::Pending));
        assert_eq!(state("REVIEW_STARTED"), Some(MyReview::Pending));
        assert_eq!(state("APPROVED"), Some(MyReview::Approved));
        assert_eq!(state("REQUESTED_CHANGES"), Some(MyReview::ChangesRequested));
        assert_eq!(state("REVIEWED"), Some(MyReview::Commented));
        assert_eq!(state("UNAPPROVED"), Some(MyReview::Commented));
        // Nicht Reviewer
        let mr = Mr::new(1, "g/p", "dwagner", "2026-10-17T09:00:00Z")
            .reviewer("mbecker", "UNREVIEWED")
            .mr();
        assert_eq!(mr.my_review, None);
        assert_eq!(
            tr(Language::De, MyReview::Pending.label_key()),
            "Review ausstehend"
        );
        assert_eq!(
            tr(Language::De, MyReview::Approved.label_key()),
            "Von dir approved"
        );
        assert_eq!(
            tr(Language::De, MyReview::ChangesRequested.label_key()),
            "Änderungen angefragt"
        );
        assert_eq!(
            tr(Language::En, MyReview::Commented.label_key()),
            "Commented"
        );
    }

    // --- Modell ---

    #[test]
    fn from_node_fills_defaults() {
        let node = MrNode {
            iid: Some(5),
            web_url: Some("https://h.example.com/g/sub/p/-/merge_requests/5".to_string()),
            web_path: Some("/g/sub/p/-/merge_requests/5".to_string()),
            head_pipeline: Some(PipelineNode {
                status: Some("MANUAL".to_string()),
                path: Some("/g/sub/p/-/pipelines/3".to_string()),
            }),
            resolvable_discussions: Some(5),
            resolved_discussions: Some(2),
            updated_at: Some("2026-10-17T09:00:00+02:00".to_string()),
            ..MrNode::default()
        };
        let mr = MergeRequest::from_node(HOST, node, Some(ME)).unwrap();
        assert_eq!(mr.id, "g/sub/p!5");
        assert_eq!(mr.project_path, "g/sub/p");
        assert_eq!(mr.pipeline, PipelineState::Manual);
        assert_eq!(mr.pipeline_path.as_deref(), Some("/g/sub/p/-/pipelines/3"));
        assert_eq!((mr.threads_open, mr.threads_resolved), (3, 2));
        assert_eq!(mr.updated_at, parse_rfc3339("2026-10-17T07:00:00Z"));
        assert!(!mr.draft);
        assert_eq!(mr.author, Person::default());
        // Ohne iid oder ohne Projekt kein MR
        let no_iid = MrNode {
            id: Some("gid://gitlab/MergeRequest/1".to_string()),
            project_path: Some("g/p".to_string()),
            ..MrNode::default()
        };
        assert_eq!(MergeRequest::from_node(HOST, no_iid, Some(ME)), None);
        let no_project = MrNode {
            iid: Some(1),
            ..MrNode::default()
        };
        assert_eq!(MergeRequest::from_node(HOST, no_project, Some(ME)), None);
    }

    #[test]
    fn from_node_marks_me_and_falls_back_to_username() {
        let mut node = Mr::new(92, "web/kundenportal", "jkrueger", "2026-10-17T08:00:00Z")
            .approved(&[ME, "syilmaz"], 2)
            .node();
        node.approved_by.as_mut().unwrap()[1].name = None;
        let mr = MergeRequest::from_node(HOST, node, Some("TALTMANN")).unwrap();
        assert!(mr.approved_by[0].is_me);
        assert!(!mr.approved_by[1].is_me);
        assert_eq!(mr.approved_by[1].name, "syilmaz");
        assert!(!mr.author.is_me);
    }

    #[test]
    fn pipeline_state_from_status() {
        use PipelineState as P;
        for (status, want) in [
            (Some("SUCCESS"), P::Success),
            (Some("success"), P::Success),
            (Some("FAILED"), P::Failed),
            (Some("RUNNING"), P::Running),
            (Some("PENDING"), P::Pending),
            (Some("CREATED"), P::Pending),
            (Some("PREPARING"), P::Pending),
            (Some("WAITING_FOR_RESOURCE"), P::Pending),
            (Some("WAITING_FOR_CALLBACK"), P::Pending),
            (Some("SCHEDULED"), P::Pending),
            (Some("CANCELED"), P::Canceled),
            (Some("CANCELING"), P::Canceled),
            (Some("SKIPPED"), P::Skipped),
            (Some("MANUAL"), P::Manual),
            (None, P::NoPipeline),
        ] {
            assert_eq!(P::from_status(status), want, "{status:?}");
        }
        assert_eq!(tr(Language::De, P::Success.label_key()), "Pipeline grün");
        assert_eq!(
            tr(Language::De, P::NoPipeline.label_key()),
            "Keine Pipeline"
        );
        assert_eq!(
            tr(Language::En, P::Canceled.label_key()),
            "Pipeline canceled"
        );
    }

    #[test]
    fn threads_labels() {
        let label = |open, resolved, lang| {
            Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
                .threads(open, resolved)
                .mr()
                .threads_label(lang)
        };
        assert_eq!(label(1, 0, Language::De), "1 offener Thread");
        assert_eq!(label(3, 1, Language::De), "3 offene Threads");
        assert_eq!(label(0, 3, Language::De), "alle Threads gelöst");
        assert_eq!(label(0, 0, Language::De), "keine Threads");
        assert_eq!(label(2, 0, Language::En), "2 open threads");
        let tip = |open, resolved| {
            Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
                .threads(open, resolved)
                .mr()
                .threads_tooltip(Language::De)
        };
        assert_eq!(tip(2, 3), "2 offen · 3 gelöst");
        assert_eq!(tip(0, 0), "Keine Diskussions-Threads");
    }

    #[test]
    fn approvals_tooltips_design_brief() {
        let snap = sample_snapshot(false);
        let tip =
            |mrs: &[MergeRequest], iid| approvals(by_iid(mrs, iid), true).tooltip(Language::De);
        assert_eq!(
            tip(&snap.mine, 412),
            "2 von 2 Approvals · Miriam Becker, Sara Yilmaz"
        );
        assert_eq!(
            tip(&snap.mine, 87),
            "1 von 2 Approvals · Jonas Krüger; Miriam Becker hat das Review begonnen"
        );
        assert_eq!(
            tip(&snap.mine, 233),
            "1 von 2 Approvals · Miriam Becker; David Wagner ausstehend"
        );
        assert_eq!(
            tip(&snap.mine, 19),
            "0 von 1 Approvals · Sara Yilmaz ausstehend"
        );
        assert_eq!(
            tip(&snap.mine, 305),
            "Keine Approval-Regel – keine Approvals erforderlich"
        );
        assert_eq!(
            tip(&snap.others, 92),
            "2 von 2 Approvals · Tim Altmann (du), Sara Yilmaz"
        );
        // Von anderen: nur Approver, keine ausstehenden Reviewer (auch nicht ich)
        assert_eq!(tip(&snap.others, 421), "0 von 2 Approvals");
        assert!(approvals(by_iid(&snap.others, 421), true)
            .pending_reviewers
            .is_empty());
        let a = approvals(by_iid(&snap.others, 92), true);
        assert_eq!(a.label(Language::En), "2 of 2 approvals");
        assert_eq!(a.short(Language::De), "2/2");
        assert_eq!(
            approvals(by_iid(&snap.mine, 87), true).pending_reviewers,
            vec![PendingReviewer {
                name: "Miriam Becker".to_string(),
                started: true,
                is_me: false,
            }]
        );
        // Eigene MR mit mir als Reviewer
        let self_review = Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
            .approved(&[], 1)
            .reviewer(ME, "UNREVIEWED")
            .mr();
        assert_eq!(
            approvals(&self_review, true).tooltip(Language::De),
            "0 von 1 Approvals · Tim Altmann (du) ausstehend"
        );
        // Ohne Regel mit genau einem Approval: Einzahl
        let one = Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
            .approved(&["mbecker"], 0)
            .ce()
            .mr();
        let a = approvals(&one, true);
        assert_eq!(a.label(Language::De), "1 Approval");
        assert_eq!(a.label(Language::En), "1 approval");
        assert_eq!(a.short(Language::De), "1");
        assert_eq!(a.tooltip(Language::De), "1 Approval · Miriam Becker");
        // Ohne Regel, aber mit Approvals
        let no_rule = Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
            .approved(&["mbecker", "syilmaz"], 0)
            .mr();
        let a = approvals(&no_rule, true);
        assert_eq!(a.label(Language::De), "2 Approvals");
        assert_eq!(a.short(Language::De), "2");
        assert_eq!(
            a.tooltip(Language::De),
            "2 Approvals · Miriam Becker, Sara Yilmaz"
        );
        assert_eq!(
            a.tooltip(Language::En),
            "2 approvals · Miriam Becker, Sara Yilmaz"
        );
    }

    #[test]
    fn approvals_left_prefers_gitlab_value() {
        // GitLab kennt Regeln (z. B. Code Owner), die mehr verlangen als
        // `required − given`
        let mut node = Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
            .approved(&["mbecker", "syilmaz"], 2)
            .node();
        node.approvals_left = Some(1);
        let mr = Mr(node).mr();
        assert_eq!(approvals(&mr, true).left, 1);
        // Ein Bot fällt weg: nie weniger offen als GitLab meldet
        let mr = Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
            .approved(&["mbecker"], 2)
            .bot_approval("renovate-bot")
            .mr();
        assert_eq!(approvals(&mr, false).left, 0);
        assert_eq!(approvals(&mr, true).left, 1);
    }

    // --- Pipeline-Link (E7) ---

    fn with_pipeline(web_url: &str, web_path: Option<&str>, path: &str) -> MergeRequest {
        let mut mr = Mr::new(412, "kern/backend-api", ME, "2026-10-17T09:00:00Z")
            .pipeline("SUCCESS")
            .mr();
        mr.web_url = web_url.to_string();
        mr.web_path = web_path.map(str::to_string);
        mr.pipeline_path = Some(path.to_string());
        mr
    }

    #[test]
    fn pipeline_url_normal_and_fork() {
        let mr = Mr::new(412, "kern/backend-api", ME, "2026-10-17T09:00:00Z")
            .pipeline("SUCCESS")
            .mr();
        assert_eq!(
            pipeline_url(&mr).as_deref(),
            Some("https://gitlab.example.com/kern/backend-api/-/pipelines/9412")
        );
        // Fork-MR: Pipeline läuft im Quellprojekt
        let mr = with_pipeline(
            "https://gitlab.example.com/web/kundenportal/-/merge_requests/92",
            Some("/web/kundenportal/-/merge_requests/92"),
            "/jkrueger/kundenportal/-/pipelines/555",
        );
        assert_eq!(
            pipeline_url(&mr).as_deref(),
            Some("https://gitlab.example.com/jkrueger/kundenportal/-/pipelines/555")
        );
    }

    #[test]
    fn pipeline_url_subfolder_port_and_http() {
        let mr = with_pipeline(
            "https://h.example.com/gitlab/g/p/-/merge_requests/5",
            Some("/gitlab/g/p/-/merge_requests/5"),
            "/gitlab/g/p/-/pipelines/7",
        );
        assert_eq!(
            pipeline_url(&mr).as_deref(),
            Some("https://h.example.com/gitlab/g/p/-/pipelines/7")
        );
        let mr = with_pipeline(
            "https://h.example.com:8443/g/p/-/merge_requests/5",
            Some("/g/p/-/merge_requests/5"),
            "/g/p/-/pipelines/7",
        );
        assert_eq!(
            pipeline_url(&mr).as_deref(),
            Some("https://h.example.com:8443/g/p/-/pipelines/7")
        );
        let mr = with_pipeline(
            "http://gitlab.local/g/p/-/merge_requests/5",
            Some("/g/p/-/merge_requests/5"),
            "/g/p/-/pipelines/7",
        );
        assert_eq!(
            pipeline_url(&mr).as_deref(),
            Some("http://gitlab.local/g/p/-/pipelines/7")
        );
        // Ohne webPath: Schema und Host von webUrl
        let mr = with_pipeline(
            "https://h.example.com/g/p/-/merge_requests/5",
            None,
            "/g/p/-/pipelines/7",
        );
        assert_eq!(
            pipeline_url(&mr).as_deref(),
            Some("https://h.example.com/g/p/-/pipelines/7")
        );
    }

    #[test]
    fn pipeline_url_uses_web_url_host_not_glab_host() {
        let mut mr = with_pipeline(
            "https://gitlab.example.com/g/p/-/merge_requests/5",
            Some("/g/p/-/merge_requests/5"),
            "/g/p/-/pipelines/7",
        );
        mr.host_key = "gitlab-api.internal:8443".to_string();
        assert_eq!(
            pipeline_url(&mr).as_deref(),
            Some("https://gitlab.example.com/g/p/-/pipelines/7")
        );
    }

    #[test]
    fn pipeline_url_missing_pipeline_or_path() {
        let mr = Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z").mr();
        assert_eq!(mr.pipeline, PipelineState::NoPipeline);
        assert_eq!(pipeline_url(&mr), None);
        // Status ohne Pfad: angezeigt, aber nicht klickbar
        let mut node = Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
            .pipeline("FAILED")
            .node();
        node.head_pipeline.as_mut().unwrap().path = None;
        let mr = Mr(node).mr();
        assert_eq!(mr.pipeline, PipelineState::Failed);
        assert_eq!(pipeline_url(&mr), None);
    }

    #[test]
    fn pipeline_url_rejects_unsafe_paths() {
        let web = "https://gitlab.example.com/g/p/-/merge_requests/5";
        let web_path = Some("/g/p/-/merge_requests/5");
        for path in [
            "//evil.example/x",
            "/g/p\\..\\x",
            "/\\evil.example",
            "/@evil.example/x",
            "/g/p/-/pipelines/7@evil",
            "/g/p/-/pipe\nlines/7",
            "/g/p/-/pipe\u{7f}lines",
            "g/p/-/pipelines/7",
            "",
        ] {
            let mr = with_pipeline(web, web_path, path);
            assert_eq!(pipeline_url(&mr), None, "{path:?} muss abgelehnt werden");
        }
    }

    #[test]
    fn pipeline_url_rejects_foreign_host_and_scheme() {
        // webPath frisst den Host: Ursprung wäre „https:/“ → Host „g“
        let mr = with_pipeline(
            "https://gitlab.example.com/g/p/-/merge_requests/5",
            Some("/gitlab.example.com/g/p/-/merge_requests/5"),
            "/g/p/-/pipelines/7",
        );
        assert_eq!(pipeline_url(&mr), None);
        for web in [
            "javascript://gitlab.example.com/g/p/-/merge_requests/5",
            "file://gitlab.example.com/g/p/-/merge_requests/5",
            "https://user@gitlab.example.com/g/p/-/merge_requests/5",
            "https:///g/p/-/merge_requests/5",
            "gitlab.example.com/g/p/-/merge_requests/5",
            "",
        ] {
            let mr = with_pipeline(web, None, "/g/p/-/pipelines/7");
            assert_eq!(pipeline_url(&mr), None, "{web:?} muss abgelehnt werden");
        }
    }

    #[test]
    fn mr_url_only_http_and_https() {
        let mut mr = Mr::new(412, "kern/backend-api", ME, "2026-10-17T09:00:00Z").mr();
        for web in [
            "https://gitlab.example.com/kern/backend-api/-/merge_requests/412",
            "HTTP://gitlab.local:8080/gitlab/g/p/-/merge_requests/5",
        ] {
            mr.web_url = web.to_string();
            assert_eq!(mr_url(&mr), Some(web));
        }
        for web in [
            "file:///C:/x.exe",
            "\\\\host\\share\\x.exe",
            "C:\\x.exe",
            "javascript:alert(1)",
            "javascript://gitlab.example.com/%0Aalert(1)",
            "https://user@gitlab.example.com/g/p/-/merge_requests/5",
            "https://gitlab.example.com\\@evil.example/x",
            "https://gitlab.example.com/g/p/-/merge_requests/5 --flag",
            "https://gitlab.example.com/g/p\n/-/merge_requests/5",
            "https:///g/p/-/merge_requests/5",
            "",
        ] {
            mr.web_url = web.to_string();
            assert_eq!(mr_url(&mr), None, "{web:?} muss abgelehnt werden");
        }
    }

    // --- mergebereit (E10) ---

    #[test]
    fn ready_uses_detailed_merge_status() {
        let snap = sample_snapshot(false);
        assert!(ready(by_iid(&snap.mine, 412)));
        assert!(!ready(by_iid(&snap.mine, 87)));
        // MERGEABLE schlägt alles andere (GitLab weiß es besser)
        assert!(ready(by_iid(&snap.mine, 305)));
        let mut mr = by_iid(&snap.mine, 412).clone();
        mr.detailed_merge_status = Some("CONFLICT".to_string());
        assert!(!ready(&mr));
    }

    #[test]
    fn ready_fallback_without_detailed_merge_status() {
        let base = || {
            Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
                .approved(&["mbecker", "syilmaz"], 2)
                .pipeline("SUCCESS")
                .threads(0, 2)
        };
        assert!(ready(&base().mr()));
        assert!(!ready(&base().draft().mr()));
        assert!(!ready(&base().pipeline("FAILED").mr()));
        assert!(!ready(&base().pipeline("RUNNING").mr()));
        assert!(!ready(&base().threads(1, 2).mr()));
        // Approval fehlt
        assert!(!ready(
            &Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z")
                .approved(&["mbecker"], 2)
                .pipeline("SUCCESS")
                .mr()
        ));
        // Keine Regel, keine Pipeline, keine Threads
        assert!(ready(&Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z").mr()));
        assert!(ready(
            &Mr::new(1, "g/p", ME, "2026-10-17T09:00:00Z").ce().mr()
        ));
    }

    // --- Chips, Kopfzahl, Leerzustände (E13) ---

    fn chips(reviewer: bool, mention: bool) -> Chips {
        Chips {
            reviewer,
            mention,
            all: false,
        }
    }

    #[test]
    fn chips_start_values() {
        let cfg = MrConfig {
            show_uninvolved_default: true,
            ..MrConfig::default()
        };
        assert_eq!(
            Chips::session_start(&cfg),
            Chips {
                reviewer: true,
                mention: true,
                all: true,
            }
        );
        assert!(!Chips::session_start(&MrConfig::default()).all);
    }

    #[test]
    fn filter_others_is_or() {
        let snap = sample_snapshot(false);
        let o = &snap.others;
        assert_eq!(
            iids_of(&filter_others(o, &chips(true, true))),
            vec![421, 418, 92, 240, 77, 310]
        );
        assert_eq!(
            iids_of(&filter_others(o, &chips(true, false))),
            vec![421, 418, 92, 240]
        );
        // !240 (Reviewer + @ Kommentar) bleibt auch nur mit „Erwähnt“
        assert_eq!(
            iids_of(&filter_others(o, &chips(false, true))),
            vec![240, 77, 310]
        );
        assert!(filter_others(o, &chips(false, false)).is_empty());
    }

    #[test]
    fn chip_counts_overlap_counts_twice() {
        let counts = chip_counts(&sample_snapshot(true));
        assert_eq!(
            counts,
            ChipCounts {
                reviewer: 4,
                mention: 3,
                uninvolved: Some(6),
            }
        );
    }

    #[test]
    fn header_count() {
        let snap = sample_snapshot(false);
        let o = &snap.others;
        assert_eq!(
            others_header_count(o, &chips(true, true)),
            HeaderCount::Total(6)
        );
        assert_eq!(
            others_header_count(o, &chips(true, false)),
            HeaderCount::Of {
                visible: 4,
                total: 6,
            }
        );
        assert_eq!(
            others_header_count(o, &chips(false, true)),
            HeaderCount::Of {
                visible: 3,
                total: 6,
            }
        );
        assert_eq!(
            others_header_count(o, &chips(false, false)),
            HeaderCount::Of {
                visible: 0,
                total: 6,
            }
        );
    }

    #[test]
    fn empty_states() {
        let snap = sample_snapshot(false);
        let o = &snap.others;
        assert_eq!(empty_state(&chips(true, true), o), EmptyKind::NotEmpty);
        assert_eq!(empty_state(&chips(false, false), o), EmptyKind::NoFilter);
        assert_eq!(empty_state(&chips(true, true), &[]), EmptyKind::NoData);
        assert_eq!(empty_state(&chips(false, false), &[]), EmptyKind::NoData);
        // Nur Reviewer-MRs, aber nur „Erwähnt“ an
        let reviewers_only: Vec<MergeRequest> =
            o.iter().filter(|m| !m.is_mentioned()).cloned().collect();
        assert_eq!(
            empty_state(&chips(false, true), &reviewers_only),
            EmptyKind::NoMatch
        );
        assert_eq!(
            empty_state(&chips(true, false), &reviewers_only),
            EmptyKind::NotEmpty
        );
    }

    // --- Initialen ---

    #[test]
    fn initials_first_and_last_word() {
        assert_eq!(initials("Tim Altmann"), "TA");
        assert_eq!(initials("Jonas Krüger"), "JK");
        assert_eq!(initials("  miriam   becker "), "MB");
        assert_eq!(initials("Anna-Lena von Müller"), "AM");
        assert_eq!(initials("Özil Ünal"), "ÖÜ");
        assert_eq!(initials("admin"), "A");
        assert_eq!(initials("(Bot) Renovate"), "BR");
        assert_eq!(initials(""), "?");
        assert_eq!(initials(" - "), "?");
    }

    // --- Gemergt und Suche ---------------------------------------------

    fn merged_node(iid: u64, project: &str, at: &str, by: &str) -> MergedNode {
        MergedNode {
            mr: Mr::new(iid, project, ME, "2026-10-17T09:00:00Z")
                .approved(&["mbecker"], 2)
                .0,
            merged_at: Some(at.to_string()),
            merge_user: Some(user(by, false)),
        }
    }

    fn merged_page(count: u32, cursor: Option<&str>, items: Vec<MergedNode>) -> MergedHostPage {
        MergedHostPage {
            host_key: HOST.to_string(),
            me: Some(ME.to_string()),
            page: Ok(MergedPage {
                me: None,
                count: Some(count),
                page: Some(PageInfo {
                    has_next_page: cursor.is_some(),
                    end_cursor: cursor.map(str::to_string),
                }),
                items,
            }),
        }
    }

    fn merged_iids(list: &MergedList) -> Vec<u64> {
        list.mrs.iter().map(|m| m.mr.iid).collect()
    }

    #[test]
    fn merged_list_sorts_filters_and_pages() {
        let since = parse_rfc3339("2026-09-17T10:00:00Z").expect("Zeit");
        let locals = locals(&["tools/gitmanager"]);
        let cfg = MrConfig::default();
        let mut list = MergedList::new(30, since);
        let first = vec![
            merged_node(84, "web/kundenportal", "2026-10-16T08:00:00Z", "jkrueger"),
            merged_node(408, "kern/backend-api", "2026-10-17T08:00:00Z", "mbecker"),
            // abgewählt
            merged_node(17, "tools/gitmanager", "2026-10-12T08:00:00Z", ME),
            // vor dem Zeitraum (Server ohne `mergedAfter`)
            merged_node(5, "kern/backend-api", "2026-08-01T08:00:00Z", ME),
        ];
        list.add_pages(vec![merged_page(6, Some("c1"), first)], &locals, &cfg);
        assert_eq!(merged_iids(&list), vec![408, 84]);
        let newest = &list.mrs[0];
        let by = newest.merged_by.as_ref().expect("gemergt von");
        assert_eq!((by.name.as_str(), by.is_me), ("Miriam Becker", false));
        assert!(newest.mr.author.is_me && newest.mr.local);
        assert_eq!(newest.merged_at, parse_rfc3339("2026-10-17T08:00:00Z"));
        // 6 laut GitLab, davon einer abgewählt
        assert_eq!(list.total, 5);
        assert_eq!(
            list.more,
            vec![MergedCursor {
                host_key: HOST.to_string(),
                cursor: "c1".to_string(),
            }]
        );

        // Weitere Seite: Dubletten fallen weg, `count` der ersten Seite gilt
        let next = vec![
            merged_node(84, "web/kundenportal", "2026-10-16T08:00:00Z", "jkrueger"),
            merged_node(229, "kern/dokumentenservice", "2026-10-14T08:00:00Z", ME),
        ];
        list.add_pages(vec![merged_page(99, None, next)], &locals, &cfg);
        assert_eq!(merged_iids(&list), vec![408, 84, 229]);
        assert!(list.mrs[2].merged_by.as_ref().is_some_and(|p| p.is_me));
        assert_eq!(list.total, 5);
        assert!(list.more.is_empty());
    }

    #[test]
    fn merged_list_records_host_errors() {
        let mut list = MergedList::new(7, 0);
        let failed = MergedHostPage {
            host_key: "git.intern".to_string(),
            me: Some(ME.to_string()),
            page: Err(GlabErrorKind::Timeout),
        };
        let ok = merged_page(
            1,
            None,
            vec![merged_node(
                408,
                "kern/backend-api",
                "2026-10-17T08:00:00Z",
                "mbecker",
            )],
        );
        list.add_pages(vec![failed, ok], &locals(&[]), &MrConfig::default());
        assert_eq!(
            list.errors,
            vec![("git.intern".to_string(), GlabErrorKind::Timeout)]
        );
        assert_eq!((list.total, merged_iids(&list)), (1, vec![408]));
    }

    #[test]
    fn search_matches_all_words_in_fields() {
        let mr = MergeRequest {
            iid: 412,
            title: "CSV-Export für Mandanten".to_string(),
            project_path: "kern/backend-api".to_string(),
            source_branch: "feature/csv-export".to_string(),
            target_branch: "main".to_string(),
            author: Person {
                username: "dwagner".to_string(),
                name: "David Wagner".to_string(),
                ..Person::default()
            },
            ..MergeRequest::default()
        };
        for (query, hit) in [
            ("", true),
            ("  ", true),
            ("csv", true),
            ("MANDANTEN", true),
            ("!412", true),
            ("412", true),
            ("!41", true),
            ("!413", false),
            ("backend-api", true),
            ("feature/csv", true),
            ("main", true),
            ("Wagner", true),
            ("dwagner", true),
            ("csv  wagner", true),
            ("csv müller", false),
            ("xyz", false),
        ] {
            assert_eq!(mr_matches(&mr, query), hit, "{query}");
        }
        let merged = MergedMr {
            mr,
            merged_at: None,
            merged_by: Some(Person {
                username: "mbecker".to_string(),
                name: "Miriam Becker".to_string(),
                ..Person::default()
            }),
        };
        assert!(merged_matches(&merged, "becker"));
        assert!(merged_matches(&merged, "csv mbecker"));
        assert!(merged_matches(&merged, ""));
        assert!(!merged_matches(&merged, "krüger"));
    }
}
