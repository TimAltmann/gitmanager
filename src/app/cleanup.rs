//! „Aufräumen …“ aus der Listen-Werkzeugleiste: Hintergrund-Suche
//! (`fetch --prune`, verwaiste Upstreams, gemergte MRs auf GitLab), Dialog
//! mit Auswahl und Ausführung nacheinander im Hintergrund.
//!
//! Vorsicht vor Datenverlust: Einträge mit Änderungen, ungepushten Commits,
//! unerreichbarem HEAD oder ungemergtem Branch sind vorab abgewählt und
//! werden nur mit ausdrücklichem Haken (dann `--force` bzw. `-D`) entfernt.
//! Unmittelbar vor jeder Aktion prüft der Worker den Zustand erneut; ist ein
//! Eintrag unsicherer geworden als angezeigt, bleibt er unangetastet.

use super::{repo_badges, GitBusy, MyApp};
use crate::cleanup::{collect_candidates, CleanupCandidate, CleanupReason, CleanupTarget};
use crate::git::{self, RepoInfo};
use crate::gitlab::glab::GlabRunner;
use crate::gitlab::remote::HostMap;
use crate::gitlab::service;
use crate::i18n::{tr, tr_fmt};
use egui::{Color32, RichText};
use std::collections::{BTreeMap, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

const ORANGE: Color32 = Color32::from_rgb(220, 130, 40);
const RED: Color32 = Color32::from_rgb(200, 60, 60);
const GREEN: Color32 = Color32::from_rgb(60, 160, 80);

/// Offener Dialog und Kanal für die Ergebnisse der Worker.
pub(super) struct CleanupUi {
    pub dialog: Option<CleanupDialog>,
    tx: Sender<CleanupMsg>,
    rx: Receiver<CleanupMsg>,
}

impl Default for CleanupUi {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            dialog: None,
            tx,
            rx,
        }
    }
}

enum CleanupMsg {
    Scanned(CleanupScan),
    Done(Vec<CleanupResult>),
}

/// Ergebnis der Suche.
#[derive(Debug, Clone, Default)]
pub(super) struct CleanupScan {
    pub candidates: Vec<CleanupCandidate>,
    /// Gemergte MRs konnten nicht geprüft werden (glab/Host fehlt, Fehler)
    pub gitlab_unavailable: bool,
    /// Such-Worker abgestürzt
    pub failed: bool,
}

/// Ausgang eines Eintrags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CleanupOutcome {
    Ok,
    /// Unsicherer geworden als angezeigt: nichts angefasst
    Changed,
    /// Worktree entfernt, Branch inzwischen unsicherer: behalten
    BranchKept,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CleanupResult {
    pub label: String,
    pub outcome: CleanupOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CleanupPhase {
    /// Fetch und Suche laufen
    Scanning,
    /// Auswahl
    Ready,
    /// Entfernen läuft
    Running,
    /// Ergebnisliste
    Done,
}

#[derive(Debug, Clone)]
pub(super) struct CleanupDialog {
    pub phase: CleanupPhase,
    pub candidates: Vec<CleanupCandidate>,
    pub gitlab_unavailable: bool,
    pub scan_failed: bool,
    pub results: Vec<CleanupResult>,
}

impl CleanupDialog {
    fn scanning() -> Self {
        Self {
            phase: CleanupPhase::Scanning,
            candidates: Vec::new(),
            gitlab_unavailable: false,
            scan_failed: false,
            results: Vec::new(),
        }
    }

    /// Solange ein Worker läuft, bleibt der Dialog offen.
    fn busy(&self) -> bool {
        matches!(self.phase, CleanupPhase::Scanning | CleanupPhase::Running)
    }

    fn selected_count(&self) -> usize {
        self.candidates.iter().filter(|c| c.selected).count()
    }
}

/// Anzeigename eines Eintrags (Dialog und Ergebnisliste).
fn label(c: &CleanupCandidate) -> String {
    match &c.target {
        CleanupTarget::Worktree { path, .. } => path.display().to_string(),
        CleanupTarget::Branch { repo, branch } => format!("{branch} ({})", repo.display()),
    }
}

/// Lokale Branches mit verwaistem Upstream: bei Worktrees nur ihr eigener
/// Branch, bei Haupt-Klonen alle (ohne ausgecheckte).
fn gone_branches(repos: &[RepoInfo]) -> HashMap<PathBuf, Vec<String>> {
    let mut gone = HashMap::new();
    for repo in repos {
        if repo.is_linked_worktree {
            if repo.is_detached || repo.branch.is_empty() {
                continue;
            }
            let Ok(handle) = git2::Repository::open(&repo.path) else {
                continue;
            };
            if git::upstream_gone(&handle, &repo.branch) {
                gone.insert(repo.path.clone(), vec![repo.branch.clone()]);
            }
        } else {
            let branches = git::gone_local_branches(&repo.path);
            if !branches.is_empty() {
                gone.insert(repo.path.clone(), branches);
            }
        }
    }
    gone
}

/// Mögliche Quell-Branch-Namen eines Worktree-Branches: der Name selbst und
/// der Upstream-Name auf `origin` (z. B. `mr-<iid>` → `feature/x`).
fn source_names(path: &Path, branch: &str) -> Vec<String> {
    let mut names = vec![branch.to_string()];
    let upstream = git2::Repository::open(path).ok().and_then(|r| {
        let cfg = r.config().ok()?;
        let remote = cfg.get_string(&format!("branch.{branch}.remote")).ok()?;
        let merge = cfg.get_string(&format!("branch.{branch}.merge")).ok()?;
        (remote == "origin")
            .then(|| merge.strip_prefix("refs/heads/").map(str::to_string))
            .flatten()
    });
    if let Some(up) = upstream {
        if up != branch {
            names.push(up);
        }
    }
    names
}

/// Gemergte MRs der Worktree-Branches je GitLab-Host (Quellprojekt =
/// Projekt des `origin`-Remotes).
struct MergedInfo {
    /// Worktree-Pfad → höchste gemergte MR-iid (Grund im Dialog)
    iids: HashMap<PathBuf, u64>,
    /// Worktree-Pfad → Kopf-Commits aller gemergten MRs (Branch-Sicherheit)
    heads: HashMap<PathBuf, Vec<git2::Oid>>,
    /// Prüfung trotz GitLab-Projekt nicht möglich (kein glab, Fehler)
    unavailable: bool,
}

fn merged_worktrees(
    repos: &[RepoInfo],
    hosts: &HostMap,
    runner: Option<&dyn GlabRunner>,
) -> MergedInfo {
    // Host → Projekt → (Worktree, Namen)
    type Wanted = BTreeMap<String, BTreeMap<String, Vec<(PathBuf, Vec<String>)>>>;
    let mut wanted: Wanted = BTreeMap::new();
    for repo in repos.iter().filter(|r| r.is_linked_worktree) {
        if repo.is_detached || repo.branch.is_empty() {
            continue;
        }
        let origin = repo
            .remotes
            .iter()
            .find(|r| r.name == "origin")
            .and_then(|r| r.parsed.as_ref());
        let Some((host, project)) = origin.and_then(|p| {
            hosts
                .glab_key(p)
                .map(|h| (h.to_string(), p.full_path.clone()))
        }) else {
            continue;
        };
        wanted
            .entry(host)
            .or_default()
            .entry(project)
            .or_default()
            .push((repo.path.clone(), source_names(&repo.path, &repo.branch)));
    }
    // Ohne Worktree auf einem GitLab-Projekt entfällt der Grund still;
    // der Hinweis gilt nur, wenn GitLab gefragt werden müsste, aber nicht kann
    let mut info = MergedInfo {
        iids: HashMap::new(),
        heads: HashMap::new(),
        unavailable: false,
    };
    let Some(runner) = runner else {
        info.unavailable = !wanted.is_empty();
        return info;
    };
    for (host, projects) in wanted {
        let query: Vec<(String, Vec<String>)> = projects
            .iter()
            .map(|(project, worktrees)| {
                let mut names: Vec<String> = worktrees
                    .iter()
                    .flat_map(|(_, n)| n.iter().cloned())
                    .collect();
                names.sort();
                names.dedup();
                (project.clone(), names)
            })
            .collect();
        let got = catch_unwind(AssertUnwindSafe(|| {
            service::fetch_merged_branches(runner, &host, &query)
        }));
        match got {
            Ok(Ok(maps)) => {
                for (worktrees, map) in projects.values().zip(maps) {
                    for (path, names) in worktrees {
                        // Nur MRs, deren Kopf die lokale Spitze enthält: ein
                        // gleichnamiger, weiterbenutzter Branch (develop,
                        // wiederverwendeter Name) ist nicht „gemergt“.
                        // Unlesbare SHAs zählen nicht.
                        let Ok(handle) = git2::Repository::open(path) else {
                            continue;
                        };
                        let Some(tip) = handle.head().ok().and_then(|h| h.target()) else {
                            continue;
                        };
                        let mrs: Vec<(u64, git2::Oid)> = names
                            .iter()
                            .filter_map(|n| map.get(n))
                            .flatten()
                            .filter_map(|(iid, sha)| Some((*iid, git2::Oid::from_str(sha).ok()?)))
                            .filter(|(_, head)| git::tip_in_merged_heads(&handle, tip, &[*head]))
                            .collect();
                        let Some(iid) = mrs.iter().map(|(iid, _)| *iid).max() else {
                            continue;
                        };
                        info.iids.insert(path.clone(), iid);
                        info.heads
                            .insert(path.clone(), mrs.iter().map(|(_, h)| *h).collect());
                    }
                }
            }
            Ok(Err(kind)) => {
                eprintln!("Aufräumen: gemergte MRs auf {host} nicht abrufbar: {kind:?}");
                info.unavailable = true;
            }
            Err(_) => {
                eprintln!("Aufräumen: Abfrage gemergter MRs auf {host} abgestürzt");
                info.unavailable = true;
            }
        }
    }
    info
}

/// Spitze eines lokalen Branches.
fn branch_tip(repo: &Path, branch: &str) -> Option<git2::Oid> {
    git2::Repository::open(repo)
        .ok()?
        .refname_to_id(&format!("refs/heads/{branch}"))
        .ok()
}

/// Kandidaten aus frisch gelesenen Repos (ohne Fetch), inklusive der
/// Prüfungen, die ein Repository brauchen. `runner == None`: glab fehlt.
pub(super) fn find_candidates(
    repos: &[RepoInfo],
    hosts: &HostMap,
    runner: Option<&dyn GlabRunner>,
) -> CleanupScan {
    let gone = gone_branches(repos);
    let merged = merged_worktrees(repos, hosts, runner);
    let gitlab_unavailable = merged.unavailable;
    let mut candidates = collect_candidates(repos, &gone, &merged.iids);
    for c in &mut candidates {
        let (unreachable, safety) = match &c.target {
            CleanupTarget::Worktree { path, main, branch } => {
                c.merged_heads = merged.heads.get(path).cloned().unwrap_or_default();
                c.tip = branch.as_deref().and_then(|b| branch_tip(main, b));
                (
                    git::head_unreachable(path),
                    branch
                        .as_deref()
                        .and_then(|b| git::branch_safety(main, b, &c.merged_heads)),
                )
            }
            CleanupTarget::Branch { repo, branch } => {
                c.tip = branch_tip(repo, branch);
                (false, git::branch_safety(repo, branch, &[]))
            }
        };
        c.apply_checks(unreachable, safety);
    }
    CleanupScan {
        candidates,
        gitlab_unavailable,
        failed: false,
    }
}

/// Such-Worker: `fetch --prune` je Fetch-Ziel, Repos frisch lesen, suchen.
fn scan(repos: Vec<RepoInfo>, hosts: HostMap, runner: Option<Arc<dyn GlabRunner>>) -> CleanupScan {
    for target in git::fetch_targets(&repos) {
        // Ohne Fetch fehlen nur „gone“-Treffer; Daten gehen nicht verloren
        if let Err(e) = git::fetch_all(&target) {
            eprintln!(
                "Aufräumen: Fetch für {} fehlgeschlagen: {e:#}",
                target.display()
            );
        }
    }
    let fresh: Vec<RepoInfo> = repos
        .into_iter()
        .filter_map(|r| git::get_repo_info(r.path))
        .collect();
    find_candidates(&fresh, &hosts, runner.as_deref())
}

/// Ein ausgewählter Eintrag, unmittelbar vor der Aktion erneut geprüft.
fn execute(c: &CleanupCandidate) -> CleanupOutcome {
    let result = match &c.target {
        CleanupTarget::Worktree { path, main, branch } => {
            execute_worktree(c, path, main, branch.as_deref())
        }
        CleanupTarget::Branch { repo, branch } => execute_branch(c, repo, branch),
    };
    result.unwrap_or_else(|e| CleanupOutcome::Failed(format!("{e:#}")))
}

fn execute_worktree(
    c: &CleanupCandidate,
    path: &Path,
    main: &Path,
    branch: Option<&str>,
) -> anyhow::Result<CleanupOutcome> {
    if path.exists() {
        let Some(info) = git::get_repo_info(path.to_path_buf()) else {
            return Ok(CleanupOutcome::Changed);
        };
        let fresh_branch = (!info.is_detached && !info.branch.is_empty()).then_some(info.branch);
        let unreachable = git::head_unreachable(path);
        let worse = fresh_branch.as_deref() != branch
            || info.dirty_files > c.dirty_files
            || info.ahead > c.ahead
            || (unreachable && !c.head_unreachable);
        if worse {
            return Ok(CleanupOutcome::Changed);
        }
    }
    // `--force` nur, wenn der Eintrag eine Warnung zeigte und angehakt wurde
    git::remove_worktree(path, main, c.needs_force(), None)?;
    if let (true, Some(b)) = (c.delete_branch && c.can_delete_branch(), branch) {
        // Erst löschen, wenn der Branch noch auf demselben Commit steht und
        // genauso sicher ist wie angezeigt
        if c.tip.is_none() || branch_tip(main, b) != c.tip {
            return Ok(CleanupOutcome::BranchKept);
        }
        let fresh = git::branch_safety(main, b, &c.merged_heads);
        let Some(safety) = fresh.filter(|s| Some(*s) == c.branch_safety) else {
            return Ok(CleanupOutcome::BranchKept);
        };
        git::delete_local_branch(main, b, safety.needs_force_delete())?;
    }
    Ok(CleanupOutcome::Ok)
}

fn execute_branch(
    c: &CleanupCandidate,
    repo: &Path,
    branch: &str,
) -> anyhow::Result<CleanupOutcome> {
    // Noch verwaist und nirgends ausgecheckt?
    if !git::gone_local_branches(repo).iter().any(|b| b == branch) {
        return Ok(CleanupOutcome::Changed);
    }
    // Neuer Commit seit der Anzeige: auch ein angehakter „nicht gemergt“-
    // Eintrag galt nur für den damals gezeigten Stand
    if c.tip.is_none() || branch_tip(repo, branch) != c.tip {
        return Ok(CleanupOutcome::Changed);
    }
    let fresh = git::branch_safety(repo, branch, &[]);
    let force = match (c.branch_safety, fresh) {
        // Als sicher gezeigt: muss genauso sicher geblieben sein
        (Some(shown), Some(now)) if shown == now => now.needs_force_delete(),
        (Some(_), _) => return Ok(CleanupOutcome::Changed),
        // Als „nicht gemergt“ gezeigt und ausdrücklich angehakt: `-D`
        (None, _) => true,
    };
    git::delete_local_branch(repo, branch, force)?;
    Ok(CleanupOutcome::Ok)
}

impl MyApp {
    /// Toolbar „Aufräumen …“: Dialog öffnen und die Suche starten.
    pub(super) fn open_cleanup(&mut self) {
        if self.cleanup_ui.dialog.as_ref().is_some_and(|d| d.busy()) {
            return;
        }
        let lang = self.config.language;
        if self.auto_fetching {
            self.set_status(tr(lang, "auto_fetch_running"));
            return;
        }
        if self.git_busy.holds_refs() {
            self.set_status(tr(lang, "fetch_busy"));
            return;
        }
        self.cleanup_ui.dialog = Some(CleanupDialog::scanning());
        self.set_git_busy(GitBusy::Cleanup);
        let repos = self.repos.clone();
        let hosts = self.mr_hosts.clone();
        let runner = (!self.mr_glab_missing).then(|| Arc::clone(&self.glab));
        let tx = self.cleanup_ui.tx.clone();
        std::thread::spawn(move || {
            // Panic vor dem Senden darf `git_busy` nicht hängen lassen
            let result = catch_unwind(AssertUnwindSafe(|| scan(repos, hosts, runner)))
                .unwrap_or_else(|_| {
                    eprintln!("Aufräumen-Suche abgestürzt");
                    CleanupScan {
                        failed: true,
                        ..Default::default()
                    }
                });
            let _ = tx.send(CleanupMsg::Scanned(result));
        });
    }

    /// „{n} entfernen“: ausgewählte Einträge nacheinander im Hintergrund.
    pub(super) fn confirm_cleanup(&mut self) {
        let lang = self.config.language;
        if self.auto_fetching {
            self.set_status(tr(lang, "auto_fetch_running"));
            return;
        }
        if self.git_busy.holds_refs() {
            self.set_status(tr(lang, "fetch_busy"));
            return;
        }
        let Some(dialog) = self.cleanup_ui.dialog.as_mut() else {
            return;
        };
        if dialog.phase != CleanupPhase::Ready {
            return;
        }
        let jobs: Vec<CleanupCandidate> = dialog
            .candidates
            .iter()
            .filter(|c| c.selected)
            .cloned()
            .collect();
        if jobs.is_empty() {
            return;
        }
        dialog.phase = CleanupPhase::Running;
        self.set_git_busy(GitBusy::Cleanup);
        let tx = self.cleanup_ui.tx.clone();
        std::thread::spawn(move || {
            let results = jobs
                .iter()
                .map(|c| {
                    let outcome =
                        catch_unwind(AssertUnwindSafe(|| execute(c))).unwrap_or_else(|_| {
                            eprintln!("Aufräumen abgestürzt bei {}", label(c));
                            CleanupOutcome::Failed("cleanup aborted".to_string())
                        });
                    CleanupResult {
                        label: label(c),
                        outcome,
                    }
                })
                .collect();
            let _ = tx.send(CleanupMsg::Done(results));
        });
    }

    /// Ergebnisse der Worker übernehmen; nach beiden neu scannen (Fetch
    /// bzw. Entfernen haben die Repos verändert).
    pub(super) fn poll_cleanup(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.cleanup_ui.rx.try_recv() {
            if self.git_busy == GitBusy::Cleanup {
                self.set_git_busy(GitBusy::Idle);
            }
            if let Some(dialog) = self.cleanup_ui.dialog.as_mut() {
                match msg {
                    CleanupMsg::Scanned(found) => {
                        dialog.phase = CleanupPhase::Ready;
                        dialog.candidates = found.candidates;
                        // Offene MRs und Claude Sessions kennt nur die UI
                        for c in &mut dialog.candidates {
                            if let CleanupTarget::Worktree { path, .. } = &c.target {
                                let mr = self.repo_mrs.get(path).map(|b| b.iid);
                                let claude =
                                    repo_badges::claude_running_at(&self.claude.live, path);
                                c.mark_in_use(mr, claude);
                            }
                        }
                        dialog.gitlab_unavailable = found.gitlab_unavailable;
                        dialog.scan_failed = found.failed;
                    }
                    CleanupMsg::Done(results) => {
                        for r in &results {
                            if let CleanupOutcome::Failed(e) = &r.outcome {
                                eprintln!("Aufräumen fehlgeschlagen für {}: {e}", r.label);
                            }
                        }
                        dialog.phase = CleanupPhase::Done;
                        dialog.results = results;
                    }
                }
            }
            self.start_scan();
            ctx.request_repaint();
        }
        if self.git_busy == GitBusy::Cleanup {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    pub(super) fn show_cleanup_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.cleanup_ui.dialog.take() else {
            return;
        };
        let lang = self.config.language;
        let busy = (self.git_busy.holds_refs() && !dialog.busy()) || self.auto_fetching;
        let mut open = true;
        let mut confirm = false;
        let mut close = false;

        egui::Window::new(tr(lang, "cleanup_title"))
            .id(egui::Id::new("cleanup_dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(560.0);
                match dialog.phase {
                    CleanupPhase::Scanning => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr(lang, "cleanup_scanning"));
                        });
                    }
                    CleanupPhase::Ready => {
                        show_candidates(ui, lang, &mut dialog);
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let n = dialog.selected_count();
                            if !dialog.candidates.is_empty() {
                                let button = egui::Button::new(tr_fmt(
                                    lang,
                                    "cleanup_remove_n",
                                    &[&n.to_string()],
                                ));
                                if ui.add_enabled(n > 0 && !busy, button).clicked() {
                                    confirm = true;
                                }
                            }
                            let key = if dialog.candidates.is_empty() {
                                "close"
                            } else {
                                "cancel"
                            };
                            if ui.button(tr(lang, key)).clicked() {
                                close = true;
                            }
                        });
                    }
                    CleanupPhase::Running => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr(lang, "cleanup_running"));
                        });
                    }
                    CleanupPhase::Done => {
                        show_results(ui, lang, &dialog.results);
                        ui.add_space(10.0);
                        if ui.button(tr(lang, "close")).clicked() {
                            close = true;
                        }
                    }
                }
            });

        if confirm {
            self.cleanup_ui.dialog = Some(dialog);
            self.confirm_cleanup();
        } else if (open && !close) || dialog.busy() {
            self.cleanup_ui.dialog = Some(dialog);
        }
    }
}

fn reason_text(lang: crate::i18n::Language, reason: &CleanupReason) -> String {
    match reason {
        CleanupReason::MrMerged { iid } => tr_fmt(lang, "cleanup_reason_mr", &[&iid.to_string()]),
        CleanupReason::UpstreamGone => tr(lang, "cleanup_reason_gone"),
    }
}

fn show_candidates(ui: &mut egui::Ui, lang: crate::i18n::Language, dialog: &mut CleanupDialog) {
    if dialog.scan_failed {
        ui.label(RichText::new(tr(lang, "cleanup_scan_failed")).color(RED));
    }
    if dialog.gitlab_unavailable {
        ui.label(RichText::new(tr(lang, "cleanup_gitlab_unavailable")).color(ORANGE));
        ui.add_space(4.0);
    }
    if dialog.candidates.is_empty() {
        ui.label(tr(lang, "cleanup_nothing"));
        return;
    }
    egui::ScrollArea::vertical()
        .max_height(380.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (i, c) in dialog.candidates.iter_mut().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                show_candidate(ui, lang, c);
            }
        });
}

/// Orange Warnungen eines Eintrags (Verlustgefahr, noch in Benutzung).
fn candidate_warnings(lang: crate::i18n::Language, c: &CleanupCandidate) -> Vec<String> {
    let mut out = Vec::new();
    if c.dirty_files > 0 {
        out.push(tr_fmt(lang, "cleanup_dirty", &[&c.dirty_files.to_string()]));
    }
    if c.ahead > 0 {
        out.push(tr_fmt(lang, "cleanup_ahead", &[&c.ahead.to_string()]));
    }
    if c.head_unreachable {
        out.push(tr(lang, "rm_wt_detached"));
    }
    if let Some(iid) = c.open_mr {
        out.push(tr_fmt(lang, "cleanup_open_mr", &[&iid.to_string()]));
    }
    if c.claude_running {
        out.push(tr(lang, "rm_wt_claude"));
    }
    out
}

fn show_candidate(ui: &mut egui::Ui, lang: crate::i18n::Language, c: &mut CleanupCandidate) {
    let (title, detail) = match &c.target {
        CleanupTarget::Worktree { path, branch, .. } => {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            let title = match branch {
                // Kein „⎇“: die egui-Standardschrift zeichnet es als Quadrat
                Some(b) => format!("{name} · {b}"),
                None => name,
            };
            (title, path.display().to_string())
        }
        CleanupTarget::Branch { repo, branch } => (
            branch.clone(),
            tr_fmt(lang, "cleanup_branch_in", &[&repo.display().to_string()]),
        ),
    };
    ui.checkbox(&mut c.selected, RichText::new(title).strong());
    ui.indent(egui::Id::new(("cleanup_entry", label(c))), |ui| {
        ui.label(RichText::new(detail).size(11.0).weak());
        let reasons: Vec<String> = c.reasons.iter().map(|r| reason_text(lang, r)).collect();
        ui.label(RichText::new(reasons.join(" · ")).size(11.0));
        for w in candidate_warnings(lang, c) {
            ui.label(RichText::new(w).color(ORANGE));
        }
        match &c.target {
            CleanupTarget::Branch { .. } if c.branch_safety.is_none() => {
                ui.label(RichText::new(tr(lang, "cleanup_branch_unmerged")).color(ORANGE));
            }
            CleanupTarget::Worktree {
                branch: Some(_), ..
            } => {
                let enabled = c.can_delete_branch();
                if !enabled {
                    c.delete_branch = false;
                }
                let resp = ui.add_enabled(
                    enabled && c.selected,
                    egui::Checkbox::new(&mut c.delete_branch, tr(lang, "cleanup_delete_branch")),
                );
                if !enabled {
                    resp.on_disabled_hover_text(tr(lang, "rm_wt_branch_unsafe"));
                }
            }
            _ => {}
        }
    });
}

fn show_results(ui: &mut egui::Ui, lang: crate::i18n::Language, results: &[CleanupResult]) {
    egui::ScrollArea::vertical()
        .max_height(380.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for r in results {
                let (icon, color, text) = match &r.outcome {
                    CleanupOutcome::Ok => ("✓", GREEN, tr(lang, "cleanup_ok")),
                    CleanupOutcome::Changed => ("✗", ORANGE, tr(lang, "cleanup_changed")),
                    CleanupOutcome::BranchKept => ("✓", ORANGE, tr(lang, "cleanup_branch_kept")),
                    CleanupOutcome::Failed(e) => ("✗", RED, tr_fmt(lang, "cleanup_failed", &[e])),
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(icon).color(color).strong());
                    ui.label(&r.label);
                    ui.label(RichText::new(text).color(color).size(11.0));
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{test_app, test_app_with_glab};
    use crate::gitlab::glab::{FakeGlab, FakeReply, GatedRunner, GlabError};
    use crate::gitlab::remote::{HostTarget, RemoteInfo};

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
    }

    fn branch_exists(repo: &Path, b: &str) -> bool {
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "--verify", "-q", &format!("refs/heads/{b}")])
            .output()
            .unwrap()
            .status
            .success()
    }

    /// Remote (bare), Klon `main` mit Worktree `wt` auf `feat` und lokalem
    /// Branch `old`; beide Remote-Branches sind auf dem Remote gelöscht.
    fn setup(tmp: &Path) -> (PathBuf, PathBuf) {
        let remote = tmp.join("remote.git");
        std::fs::create_dir(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare", "-b", "main"]);
        let main = tmp.join("main");
        git(
            tmp,
            &[
                "clone",
                "-q",
                remote.to_str().unwrap(),
                main.to_str().unwrap(),
            ],
        );
        git(&main, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
        git(&main, &["push", "-q", "-u", "origin", "main"]);
        for b in ["feat", "old"] {
            git(&main, &["branch", b]);
            git(&main, &["push", "-q", "-u", "origin", b]);
        }
        let wt = tmp.join("wt");
        git(
            &main,
            &["worktree", "add", "-q", wt.to_str().unwrap(), "feat"],
        );
        git(&remote, &["branch", "-D", "feat", "old"]);
        (main, wt)
    }

    fn repos(main: &Path, wt: &Path) -> Vec<RepoInfo> {
        vec![
            git::get_repo_info(main.to_path_buf()).unwrap(),
            git::get_repo_info(wt.to_path_buf()).unwrap(),
        ]
    }

    fn wait(app: &mut MyApp, phase: CleanupPhase) {
        let ctx = egui::Context::default();
        for _ in 0..400 {
            app.poll_cleanup(&ctx);
            if app.cleanup_ui.dialog.as_ref().map(|d| d.phase) == Some(phase) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("Phase {phase:?} nicht erreicht");
    }

    #[test]
    fn cleanup_rejected_while_busy() {
        let mut app = test_app();
        let lang = app.config.language;
        app.git_busy = GitBusy::Scan;
        app.open_cleanup();
        assert!(app.cleanup_ui.dialog.is_none());
        assert_eq!(app.status_message, Some(tr(lang, "fetch_busy")));

        app.git_busy = GitBusy::Idle;
        app.auto_fetching = true;
        app.open_cleanup();
        assert!(app.cleanup_ui.dialog.is_none());
        assert_eq!(app.status_message, Some(tr(lang, "auto_fetch_running")));

        // Bestätigen während eines Laufs startet nichts
        app.auto_fetching = false;
        let mut dialog = CleanupDialog::scanning();
        dialog.phase = CleanupPhase::Ready;
        dialog.candidates = collect_candidates(
            &[{
                let mut r = RepoInfo::new(PathBuf::from("/x/wt"), "f".into(), false, false);
                r.is_linked_worktree = true;
                r
            }],
            &HashMap::from([(PathBuf::from("/x/wt"), vec!["f".to_string()])]),
            &HashMap::new(),
        );
        app.cleanup_ui.dialog = Some(dialog);
        app.git_busy = GitBusy::Pull {
            repo: PathBuf::from("/x"),
        };
        app.confirm_cleanup();
        assert_eq!(
            app.cleanup_ui.dialog.as_ref().unwrap().phase,
            CleanupPhase::Ready
        );
        assert_eq!(app.status_message, Some(tr(lang, "fetch_busy")));

        // Laufende Suche: erneuter Klick öffnet nichts Neues
        app.git_busy = GitBusy::Idle;
        app.cleanup_ui.dialog.as_mut().unwrap().phase = CleanupPhase::Scanning;
        app.open_cleanup();
        assert_eq!(app.git_busy, GitBusy::Idle);
    }

    #[test]
    fn cleanup_without_glab_skips_mr_reason_and_flags_hint() {
        let tmp = tempfile::tempdir().unwrap();
        let (main, wt) = setup(tmp.path());
        git(&main, &["fetch", "-q", "--prune", "origin"]);
        let mut repos = repos(&main, &wt);
        // Worktree zeigt (nur im Speicher) auf ein GitLab-Projekt
        repos[1].remotes = vec![RemoteInfo::new(
            "origin",
            "https://gitlab.example.com/acme/shop.git",
        )];
        let mut hosts = HostMap::default();
        hosts.remotes.insert(
            "gitlab.example.com".into(),
            HostTarget::GitLab("gitlab.example.com".into()),
        );
        hosts.gitlab_hosts = vec!["gitlab.example.com".into()];
        let runner = GatedRunner::new(FakeGlab::new());
        runner.inner().push(FakeReply::error(GlabError::NotFound));

        let scan = find_candidates(&repos, &hosts, Some(&runner));
        assert!(scan.gitlab_unavailable);
        assert_eq!(runner.inner().pending(), 0, "GitLab wurde gefragt");
        assert!(scan
            .candidates
            .iter()
            .all(|c| c.reasons == vec![CleanupReason::UpstreamGone]));
        assert_eq!(scan.candidates.len(), 2, "{:?}", scan.candidates);

        // Ohne glab (`None`) bei bekanntem GitLab-Host ebenso, ohne Aufruf
        let scan = find_candidates(&repos, &hosts, None);
        assert!(scan.gitlab_unavailable);
        assert_eq!(scan.candidates.len(), 2);

        // Ohne GitLab-Host (z. B. nur GitHub): Grund entfällt still
        let scan = find_candidates(&repos, &HostMap::default(), None);
        assert!(!scan.gitlab_unavailable);
        let scan = find_candidates(&repos, &HostMap::default(), Some(&runner));
        assert!(!scan.gitlab_unavailable);
        assert_eq!(runner.inner().pending(), 0);

        // MR ohne Kopf-SHA ist nicht verwertbar: kein Grund „MR gemergt“
        runner.inner().push(FakeReply::http(
            200,
            r#"{"data":{"p0":{"mergeRequests":{"nodes":[{"iid":"9","sourceBranch":"feat"}]}}}}"#,
        ));
        let scan = find_candidates(&repos, &hosts, Some(&runner));
        assert!(!scan.gitlab_unavailable && !scan.candidates[0].mr_merged());

        // Mit Antwort samt Kopf: Grund „MR gemergt“
        let head = rev(&main, "feat");
        runner.inner().push(FakeReply::http(
            200,
            &format!(
                r#"{{"data":{{"p0":{{"mergeRequests":{{"nodes":[{{"iid":"9","sourceBranch":"feat","diffHeadSha":"{head}"}}]}}}}}}}}"#
            ),
        ));
        let scan = find_candidates(&repos, &hosts, Some(&runner));
        assert!(!scan.gitlab_unavailable);
        let wt_c = &scan.candidates[0];
        assert_eq!(
            wt_c.reasons,
            vec![
                CleanupReason::MrMerged { iid: 9 },
                CleanupReason::UpstreamGone
            ]
        );

        // Dialog übernimmt das Flag
        let mut app = test_app_with_glab(Arc::new(GatedRunner::new(FakeGlab::new())));
        app.cleanup_ui.dialog = Some(CleanupDialog::scanning());
        app.git_busy = GitBusy::Cleanup;
        app.cleanup_ui
            .tx
            .send(CleanupMsg::Scanned(CleanupScan {
                candidates: Vec::new(),
                gitlab_unavailable: true,
                failed: false,
            }))
            .unwrap();
        app.poll_cleanup(&egui::Context::default());
        let d = app.cleanup_ui.dialog.as_ref().unwrap();
        assert!(d.gitlab_unavailable && d.phase == CleanupPhase::Ready);
        assert_eq!(app.git_busy, GitBusy::Scan, "Rescan nach dem Fetch");
    }

    #[test]
    fn cleanup_end_to_end_removes_selected() {
        let tmp = tempfile::tempdir().unwrap();
        let (main, wt) = setup(tmp.path());
        let mut app = test_app();
        app.repos = repos(&main, &wt);
        app.open_cleanup();
        assert_eq!(app.git_busy, GitBusy::Cleanup);
        wait(&mut app, CleanupPhase::Ready);
        {
            let d = app.cleanup_ui.dialog.as_ref().unwrap();
            assert!(!d.gitlab_unavailable, "ohne GitLab-Host still");
            let targets: Vec<&CleanupTarget> = d.candidates.iter().map(|c| &c.target).collect();
            assert_eq!(targets.len(), 2, "{:?}", d.candidates);
            assert!(d.candidates.iter().all(|c| c.selected));
            assert!(d.candidates[0].delete_branch, "feat ist im Default-Branch");
        }
        app.git_busy = GitBusy::Idle; // Rescan aus der Suche überspringen
        app.confirm_cleanup();
        assert_eq!(app.git_busy, GitBusy::Cleanup);
        wait(&mut app, CleanupPhase::Done);
        let d = app.cleanup_ui.dialog.as_ref().unwrap();
        assert!(
            d.results.iter().all(|r| r.outcome == CleanupOutcome::Ok),
            "{:?}",
            d.results
        );
        assert!(!wt.exists());
        assert!(!branch_exists(&main, "feat") && !branch_exists(&main, "old"));
        assert!(branch_exists(&main, "main"));
        assert_eq!(app.git_busy, GitBusy::Scan, "Rescan angestoßen");
    }

    #[test]
    fn cleanup_skips_entry_that_became_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let (main, wt) = setup(tmp.path());
        git(&main, &["fetch", "-q", "--prune", "origin"]);
        let scan = find_candidates(&repos(&main, &wt), &HostMap::default(), None);
        let mut wt_c = scan.candidates[0].clone();
        assert!(!wt_c.needs_force() && wt_c.selected);
        std::fs::write(wt.join("neu.txt"), "x").unwrap();
        assert_eq!(execute(&wt_c), CleanupOutcome::Changed);
        assert!(wt.exists() && wt.join("neu.txt").exists());
        assert!(branch_exists(&main, "feat"));

        // Branch mit neuem Commit: Worktree weg, Branch bleibt
        std::fs::remove_file(wt.join("neu.txt")).unwrap();
        git(&wt, &["commit", "-q", "--allow-empty", "-m", "neu"]);
        wt_c.ahead = 0;
        let fresh = git::get_repo_info(wt.clone()).unwrap();
        assert_eq!(fresh.ahead, 0, "verwaister Upstream: kein ↑");
        assert_eq!(execute(&wt_c), CleanupOutcome::BranchKept);
        assert!(!wt.exists());
        assert!(branch_exists(&main, "feat"), "Commit nicht verloren");
    }

    #[test]
    fn unmerged_gone_branch_needs_explicit_check_and_force() {
        let tmp = tempfile::tempdir().unwrap();
        let (main, wt) = setup(tmp.path());
        git(&main, &["checkout", "-q", "old"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "lokal"]);
        git(&main, &["checkout", "-q", "main"]);
        git(&main, &["fetch", "-q", "--prune", "origin"]);
        let scan = find_candidates(&repos(&main, &wt), &HostMap::default(), None);
        let old = scan
            .candidates
            .iter()
            .find(|c| matches!(&c.target, CleanupTarget::Branch { branch, .. } if branch == "old"))
            .expect("old ist Kandidat")
            .clone();
        assert_eq!(old.branch_safety, None);
        assert!(old.needs_force() && !old.selected, "vorab abgewählt");
        // Nach der Anzeige ein weiterer Commit: übersprungen, nichts verloren
        git(&main, &["checkout", "-q", "old"]);
        git(
            &main,
            &["commit", "-q", "--allow-empty", "-m", "noch einer"],
        );
        git(&main, &["checkout", "-q", "main"]);
        assert_eq!(execute(&old), CleanupOutcome::Changed);
        assert!(branch_exists(&main, "old"));
        // Neu angezeigt und ausdrücklich angehakt: `-D`
        let scan = find_candidates(&repos(&main, &wt), &HostMap::default(), None);
        let old = scan
            .candidates
            .iter()
            .find(|c| matches!(&c.target, CleanupTarget::Branch { branch, .. } if branch == "old"))
            .unwrap()
            .clone();
        assert_eq!(execute(&old), CleanupOutcome::Ok);
        assert!(!branch_exists(&main, "old"));
    }

    fn rev(dir: &Path, r: &str) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", r])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    fn gitlab_hosts() -> HostMap {
        let mut hosts = HostMap::default();
        hosts.remotes.insert(
            "gitlab.example.com".into(),
            HostTarget::GitLab("gitlab.example.com".into()),
        );
        hosts.gitlab_hosts = vec!["gitlab.example.com".into()];
        hosts
    }

    /// Review-Befund: MR mit Kopf = alte Spitze gemergt, danach lokal
    /// weitergearbeitet. Der Branch darf nicht als sicher gelten.
    #[test]
    fn merged_mr_with_later_local_commit_keeps_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let (main, wt) = setup(tmp.path());
        let merged_head = rev(&wt, "HEAD");
        git(
            &wt,
            &["commit", "-q", "--allow-empty", "-m", "nach dem merge"],
        );
        git(&main, &["fetch", "-q", "--prune", "origin"]);
        let mut repos = repos(&main, &wt);
        repos[1].remotes = vec![RemoteInfo::new(
            "origin",
            "https://gitlab.example.com/acme/shop.git",
        )];
        let runner = GatedRunner::new(FakeGlab::new());
        runner.inner().push(FakeReply::http(
            200,
            &format!(
                r#"{{"data":{{"p0":{{"mergeRequests":{{"nodes":[{{"iid":"9","sourceBranch":"feat","diffHeadSha":"{merged_head}"}}]}}}}}}}}"#
            ),
        ));
        let scan = find_candidates(&repos, &gitlab_hosts(), Some(&runner));
        assert!(!scan.gitlab_unavailable);
        let c = scan
            .candidates
            .iter()
            .find(|c| matches!(c.target, CleanupTarget::Worktree { .. }))
            .expect("Worktree angeboten")
            .clone();
        // Spitze liegt nicht im MR-Kopf: kein „MR gemergt“, nur „verwaist“
        assert!(!c.mr_merged());
        assert_eq!(c.branch_safety, None);
        assert!(!c.can_delete_branch() && !c.delete_branch);
        assert_eq!(execute(&c), CleanupOutcome::Ok);
        assert!(!wt.exists(), "Worktree selbst ist sauber");
        assert!(branch_exists(&main, "feat"), "Commit nach dem Merge bleibt");

        // Gegenprobe: Kopf == Spitze → per MR sicher
        let mut c2 = c.clone();
        c2.merged_heads = vec![git2::Oid::from_str(&rev(&main, "feat")).unwrap()];
        assert_eq!(
            git::branch_safety(&main, "feat", &c2.merged_heads),
            Some(git::BranchSafety::MrMerged)
        );
    }

    /// Review-Befund: langlebiger Branch (develop), dessen früherer MR
    /// gemergt ist. Die Spitze liegt nicht im MR-Kopf: kein „MR gemergt“,
    /// ohne anderen Grund also kein Kandidat.
    #[test]
    fn merged_mr_head_not_containing_tip_is_no_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let (main, wt) = setup(tmp.path());
        // Alter MR-Kopf = init; danach weitergearbeitet und gepusht
        let old_head = rev(&main, "main");
        git(&wt, &["commit", "-q", "--allow-empty", "-m", "weiter"]);
        git(&wt, &["push", "-q", "-u", "origin", "feat"]);
        git(&main, &["fetch", "-q", "--prune", "origin"]);
        let mut repos = repos(&main, &wt);
        repos[1].remotes = vec![RemoteInfo::new(
            "origin",
            "https://gitlab.example.com/acme/shop.git",
        )];
        let runner = GatedRunner::new(FakeGlab::new());
        runner.inner().push(FakeReply::http(
            200,
            &format!(
                r#"{{"data":{{"p0":{{"mergeRequests":{{"nodes":[{{"iid":"4","sourceBranch":"feat","diffHeadSha":"{old_head}"}}]}}}}}}}}"#
            ),
        ));
        let scan = find_candidates(&repos, &gitlab_hosts(), Some(&runner));
        assert_eq!(runner.inner().pending(), 0, "GitLab wurde gefragt");
        assert!(!scan.gitlab_unavailable);
        assert!(
            scan.candidates
                .iter()
                .all(|c| !matches!(c.target, CleanupTarget::Worktree { .. })),
            "{:?}",
            scan.candidates
        );
    }

    /// Sauberer, sicherer Worktree-Kandidat unter `/x/wt` (vorausgewählt,
    /// Branch mitlöschen an).
    fn clean_wt_candidate() -> CleanupCandidate {
        let mut r = RepoInfo::new(PathBuf::from("/x/wt"), "f".into(), false, false);
        r.is_linked_worktree = true;
        let mut c = collect_candidates(
            &[r],
            &HashMap::from([(PathBuf::from("/x/wt"), vec!["f".to_string()])]),
            &HashMap::new(),
        )
        .remove(0);
        c.apply_checks(false, Some(git::BranchSafety::Pushed));
        assert!(c.selected && c.delete_branch);
        c
    }

    fn deliver_scan(app: &mut MyApp, c: CleanupCandidate) -> CleanupCandidate {
        app.cleanup_ui.dialog = Some(CleanupDialog::scanning());
        app.git_busy = GitBusy::Cleanup;
        app.cleanup_ui
            .tx
            .send(CleanupMsg::Scanned(CleanupScan {
                candidates: vec![c],
                gitlab_unavailable: false,
                failed: false,
            }))
            .unwrap();
        app.poll_cleanup(&egui::Context::default());
        app.cleanup_ui.dialog.as_ref().unwrap().candidates[0].clone()
    }

    #[test]
    fn worktree_with_open_mr_is_not_preselected() {
        let mut app = test_app();
        let mr = crate::gitlab::model::MergeRequest {
            iid: 12,
            ..Default::default()
        };
        app.repo_mrs.insert(
            PathBuf::from("/x/wt"),
            super::super::repo_badges::MrBadge::from_mr(&mr, true),
        );
        let c = deliver_scan(&mut app, clean_wt_candidate());
        assert!(!c.selected && !c.delete_branch, "{c:?}");
        assert_eq!(c.open_mr, Some(12));
        let de = candidate_warnings(crate::i18n::Language::De, &c);
        assert!(de.contains(&"Offener MR !12".to_string()), "{de:?}");
        let en = candidate_warnings(crate::i18n::Language::En, &c);
        assert!(en.contains(&"Open MR !12".to_string()), "{en:?}");

        // Ohne offenen MR bleibt die Vorauswahl
        let mut app = test_app();
        let c = deliver_scan(&mut app, clean_wt_candidate());
        assert!(c.selected && c.delete_branch && c.open_mr.is_none());
        assert!(candidate_warnings(crate::i18n::Language::De, &c).is_empty());
    }

    #[test]
    fn worktree_with_running_claude_is_not_preselected() {
        let mut app = test_app();
        app.claude.live = vec![crate::claude_live::LiveSession {
            pid: 1,
            session_id: "s".into(),
            cwd: "/x/wt".into(),
            status: crate::claude_live::LiveStatus::Idle,
            status_since_ms: 0,
            name: None,
            proc_start: None,
        }];
        let c = deliver_scan(&mut app, clean_wt_candidate());
        assert!(!c.selected && !c.delete_branch, "{c:?}");
        assert!(c.claude_running);
        let lang = crate::i18n::Language::De;
        assert!(candidate_warnings(lang, &c).contains(&tr(lang, "rm_wt_claude")));
    }

    #[test]
    fn worktree_branch_with_new_commit_after_scan_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let (main, wt) = setup(tmp.path());
        git(&main, &["fetch", "-q", "--prune", "origin"]);
        let scan = find_candidates(&repos(&main, &wt), &HostMap::default(), None);
        let mut c = scan.candidates[0].clone();
        assert!(c.delete_branch);
        // Gleich sicher, aber andere Spitze (z. B. Commit, der auch im
        // Default-Branch liegt): Branch bleibt trotzdem
        git(&wt, &["commit", "-q", "--allow-empty", "-m", "neu"]);
        git(&main, &["update-ref", "refs/remotes/origin/main", "feat"]);
        c.ahead = 0;
        assert_eq!(execute(&c), CleanupOutcome::BranchKept);
        assert!(!wt.exists());
        assert!(branch_exists(&main, "feat"));
    }
}
