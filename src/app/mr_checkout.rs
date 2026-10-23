//! Auschecken eines MR in der App (K1, Task 11): Worker für „Neuer
//! Worktree“, „Neuer Klon“ und das Vorbereiten eines bestehenden Klons
//! (`git.rs`), der Wechsel über den bestehenden Ablauf
//! `pending_branch_switches` (Dialog bei Änderungen), „Danach öffnen“ und das
//! Menü „Bereits ausgecheckt“. Der Worker hält den Ref-Lock (`GitBusy`, kein
//! paralleler Scan oder Auto-Fetch); Fehler gehen wie beim Fetch über
//! `self.error` und damit ins Banner beider Ansichten (E20).

use super::{fetch_error_i18n_key, fetch_error_parts, GitBusy, MainView, MyApp, ScanResult};
use super::{ICON_CROSS, ICON_WARNING};
use crate::config::{AfterAction, AppConfig};
use crate::git::{self, CheckoutError, MrPrep, MrPrepOutcome, RepoInfo};
use crate::gitlab::checkout::{find_checked_out, path_key, MrCheckoutPlan, MrRefInfo, ProjectRef};
use crate::gitlab::model::{MergeRequest, MrSnapshot};
use crate::i18n::{tr, tr_fmt, Language};
use crate::ui::merge_requests::{MrCheckoutChoice, MrCheckoutKind, OpenTarget};
use crate::ui::repo_list::RepoHighlight;
use crate::ui::theme::mr_palette;
use egui::{vec2, Button, Frame, Image, Margin, RichText, Stroke, Ui, WidgetInfo, WidgetType};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// So lange bleibt eine Karte nach „In Repo-Liste zeigen“ hervorgehoben.
const HIGHLIGHT_DURATION: Duration = Duration::from_secs(2);

/// Warum das Auschecken gescheitert ist; den Text baut erst die App
/// (Sprache, Auth-Hinweis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MrCheckoutFailure {
    /// Klon bzw. Basis-Klon lässt sich nicht öffnen
    RepoOpen { path: PathBuf },
    /// Fetch des Quell-Branches (Text English-neutral wie beim Fetch)
    Fetch { detail: String },
    /// Konflikte oder laufender Merge/Rebase im Klon, vor jedem Netz-Aufruf
    Blocked { branch: String, state: String },
    /// Branch vorbereiten, Worktree anlegen oder klonen; `network` beim
    /// Klonen (dann mit Auth-Hinweis)
    Checkout { error: CheckoutError, network: bool },
}

/// Wechsel eines bestehenden Klons (K1): läuft über
/// `pending_branch_switches`; Meldung, Hinweis und „Danach öffnen“ erst nach
/// erfolgreichem `execute_branch_switch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MrSwitch {
    pub clone: PathBuf,
    pub local_branch: String,
    pub iid: u64,
    pub outcome: MrPrepOutcome,
    pub after: AfterAction,
}

/// git-Schritte des Workers je Ziel.
enum CheckoutJob {
    Worktree {
        base: PathBuf,
        target: PathBuf,
        plan: MrCheckoutPlan,
    },
    Clone {
        url: String,
        target: PathBuf,
        branch: String,
    },
    Switch {
        clone: PathBuf,
        plan: MrCheckoutPlan,
    },
}

/// IDE für „Danach öffnen“ bzw. „In {IDE} öffnen“.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IdeLaunch {
    id: String,
    name: String,
    /// Solution/Projektdatei, sonst der Ordner
    file: PathBuf,
    /// Ordner, dessen wirksames Profil die IDE liefert (Basis-Klon bei
    /// einem Worktree außerhalb der Suchpfade)
    profile: PathBuf,
}

impl MyApp {
    /// Startet das Auschecken aus dem Menü K1 im Hintergrund. Nicht während
    /// eines Scans oder Fetches (Ref-Race wie beim manuellen Fetch); danach
    /// hält es `GitBusy::Checkout` bis `MrCheckoutReady`.
    pub(super) fn start_mr_checkout(&mut self, choice: MrCheckoutChoice) {
        let lang = self.config.language;
        // Vorbelegung fürs nächste Menü; gespeichert wird gebatcht
        if self
            .config
            .merge_requests
            .remember_after_checkout(&choice.after)
        {
            self.config_save_pending = true;
        }
        // Offenes Einstellungsfenster: auch im Entwurf, sonst setzte dessen
        // „Speichern“ die Wahl auf den Stand beim Öffnen zurück (wie
        // `alias_port_host`)
        if let Some(state) = self.open_settings_state() {
            state
                .draft
                .merge_requests
                .remember_after_checkout(&choice.after);
        }
        if self.git_busy.holds_refs() || self.auto_fetching {
            self.set_status(tr(lang, "mr_checkout_blocked"));
            return;
        }
        // Ein MR-Wechsel wartet im Dialog auf die Entscheidung: ein zweiter
        // Wechsel ersetzte oder schlösse diesen Dialog, der erste MR ginge
        // verloren (ein `mr_switch`)
        if matches!(choice.kind, MrCheckoutKind::Switch { .. }) && self.mr_switch.is_some() {
            self.set_status(tr(lang, "mr_switch_waiting"));
            return;
        }
        let snapshot = self.mr_snapshot.as_deref();
        let Some(mr) = snapshot.and_then(|s| find_mr(s, &choice.host_key, &choice.mr_id)) else {
            // Menü seit dem Öffnen veraltet: ein Refresh hat den MR entfernt
            let iid = choice.iid.to_string();
            self.set_status(tr_fmt(lang, "mr_checkout_gone", &[&iid]));
            return;
        };
        let mr = mr.clone();
        // Menü seit dem Öffnen veraltet: der Branch ist inzwischen ausgecheckt
        if let Some(path) = self.mr_checked_out_at(&mr) {
            let behind = self
                .repos
                .iter()
                .find(|r| same_dir(&r.path, &path))
                .map_or(0, |r| r.behind);
            self.mr_already_checked_out(&path, behind, &choice.after);
            return;
        }
        let job = match self.checkout_job(&mr, &choice.kind) {
            Ok(job) => job,
            Err(detail) => {
                let iid = choice.iid.to_string();
                self.error = Some(tr_fmt(lang, "mr_checkout_failed", &[&iid, &detail]));
                self.mark_tray_dirty();
                return;
            }
        };
        let busy = match &job {
            CheckoutJob::Clone { url, target, .. } => {
                tr_fmt(lang, "mr_cloning", &[url, &target.display().to_string()])
            }
            _ => tr_fmt(
                lang,
                "mr_checkout_busy",
                &[&mr.source_branch, &choice.iid.to_string()],
            ),
        };
        self.set_status(busy);
        self.mr_notice = None;
        // Ref-Lock wie beim manuellen Fetch: kein Scan, kein Auto-Fetch
        self.set_git_busy(GitBusy::Checkout { iid: choice.iid });
        let tx = self.scan_tx.clone();
        std::thread::spawn(move || {
            // H3: Panic vor send darf `git_busy` nicht hängen lassen
            let msg = match catch_unwind(AssertUnwindSafe(|| run_job(&job))) {
                Ok(result) => ScanResult::MrCheckoutReady {
                    choice: Box::new(choice),
                    result,
                },
                Err(_) => {
                    eprintln!("MR-Checkout-Thread abgestürzt (panic vor send)");
                    ScanResult::FetchAborted {
                        reason_key: "fetch_aborted",
                    }
                }
            };
            let _ = tx.send(msg);
        });
    }

    /// Ordner, in dem der MR-Branch (`sourceBranch` oder `mr-<iid>`) laut
    /// Scan schon ausgecheckt ist, auch als Worktree außerhalb der Suchpfade.
    fn mr_checked_out_at(&self, mr: &MergeRequest) -> Option<PathBuf> {
        let project = ProjectRef {
            hosts: &self.mr_hosts,
            host_key: &mr.host_key,
            full_path: mr.source_project_path.as_deref()?,
        };
        [mr.source_branch.clone(), format!("mr-{}", mr.iid)]
            .iter()
            .find_map(|branch| find_checked_out(&self.repos, branch, &project))
    }

    /// git-Schritte für das gewählte Ziel; Worktree und Wechsel brauchen
    /// einen gescannten Klon mit Remote auf das Quellprojekt.
    fn checkout_job(
        &self,
        mr: &MergeRequest,
        kind: &MrCheckoutKind,
    ) -> Result<CheckoutJob, String> {
        let info = MrRefInfo::from_mr(mr);
        let plan_for = |path: &Path| {
            self.repos
                .iter()
                .find(|r| same_dir(&r.path, path))
                .and_then(|repo| MrCheckoutPlan::for_repo(&info, repo, &self.mr_hosts))
                .ok_or_else(|| {
                    let project = mr
                        .source_project_path
                        .as_deref()
                        .unwrap_or(&mr.project_path);
                    tr_fmt(
                        self.config.language,
                        "mr_checkout_no_remote",
                        &[&path.display().to_string(), project],
                    )
                })
        };
        Ok(match kind {
            MrCheckoutKind::NewWorktree { path, base } => CheckoutJob::Worktree {
                base: base.clone(),
                target: path.clone(),
                plan: plan_for(base)?,
            },
            MrCheckoutKind::NewClone { path, url } => CheckoutJob::Clone {
                url: url.clone(),
                target: path.clone(),
                branch: mr.source_branch.clone(),
            },
            MrCheckoutKind::Switch { clone } => CheckoutJob::Switch {
                clone: clone.clone(),
                plan: plan_for(clone)?,
            },
        })
    }

    /// Ergebnis des Workers (aus `poll_scan`, der Ref-Lock ist schon frei):
    /// Worktree/Klon → Rescan und „Danach öffnen“, bestehender Klon →
    /// `pending_branch_switches`, Fehler → Banner (E20).
    pub(super) fn on_mr_checkout_ready(
        &mut self,
        choice: MrCheckoutChoice,
        result: Result<Option<MrPrep>, MrCheckoutFailure>,
    ) {
        let lang = self.config.language;
        let iid = choice.iid;
        let prep = match result {
            Ok(prep) => prep,
            Err(failure) => {
                self.error = Some(failure_text(lang, iid, &failure));
                self.status_message = None;
                self.status_message_time = None;
                return;
            }
        };
        match (choice.kind, prep) {
            (
                MrCheckoutKind::NewWorktree { base, .. } | MrCheckoutKind::Switch { clone: base },
                Some(MrPrep {
                    outcome: MrPrepOutcome::AlreadyHead { behind },
                    ..
                }),
            ) => {
                // Branch ist dort schon HEAD: kein Worktree, kein Wechsel
                self.mr_already_checked_out(&base, behind, &choice.after);
            }
            (MrCheckoutKind::NewWorktree { path, base }, prep) => {
                if let Some(MrPrep {
                    local_branch,
                    outcome: MrPrepOutcome::Diverged { ahead, behind },
                }) = &prep
                {
                    self.mr_notice = Some(diverged_text(lang, local_branch, *ahead, *behind));
                }
                let message = tr_fmt(
                    lang,
                    "mr_worktree_created",
                    &[&path.display().to_string(), &iid.to_string()],
                );
                // Basis-Klon kennt den neuen Worktree erst nach dem Rescan
                self.finish_mr_checkout(&path, Some(&base), &choice.after, message);
            }
            (MrCheckoutKind::NewClone { path, .. }, _) => {
                let message = tr_fmt(
                    lang,
                    "mr_clone_done",
                    &[&iid.to_string(), &path.display().to_string()],
                );
                self.finish_mr_checkout(&path, None, &choice.after, message);
            }
            (MrCheckoutKind::Switch { clone }, Some(prep)) => {
                // Bestehender Ablauf: Dialog bei Änderungen, dann
                // `execute_branch_switch` → `mr_switch_succeeded`
                self.pending_branch_switches
                    .push((clone.clone(), prep.local_branch.clone()));
                self.mr_switch = Some(MrSwitch {
                    clone,
                    local_branch: prep.local_branch,
                    iid,
                    outcome: prep.outcome,
                    after: choice.after,
                });
            }
            (MrCheckoutKind::Switch { .. }, None) => {
                eprintln!("MR !{iid}: Wechsel ohne vorbereiteten Branch – übersprungen");
            }
        }
    }

    /// Worktree bzw. Klon fertig oder schon ausgecheckt: Rescan (danach
    /// zeigt die Zeile „Bereits ausgecheckt“), „Danach öffnen“, Meldung. Die
    /// MR-Ansicht bleibt aktiv. `base` = Basis-Klon eines neuen Worktrees.
    fn finish_mr_checkout(
        &mut self,
        path: &Path,
        base: Option<&Path>,
        after: &AfterAction,
        message: String,
    ) {
        self.start_scan();
        let then = self.run_after_checkout(path, base, after);
        self.set_status(with_then(message, then));
    }

    /// MR-Branch ist in `path` schon HEAD: nur „Danach öffnen“ dort.
    fn mr_already_checked_out(&mut self, path: &Path, behind: usize, after: &AfterAction) {
        let lang = self.config.language;
        let shown = path.display().to_string();
        let message = if behind > 0 {
            tr_fmt(
                lang,
                "mr_already_head_behind",
                &[&shown, &behind.to_string()],
            )
        } else {
            tr_fmt(lang, "mr_already_head", &[&shown])
        };
        self.finish_mr_checkout(path, None, after, message);
    }

    /// Nach erfolgreichem `execute_branch_switch`: gehörte der Wechsel zu
    /// einem MR, MR-Meldung statt „Branch gewechselt“, Hinweis bei
    /// divergiertem Branch und „Danach öffnen“ im Klon.
    pub(super) fn mr_switch_succeeded(&mut self, path: &Path, branch: &str) {
        let Some(switch) = self.take_mr_switch(path, branch) else {
            return;
        };
        let lang = self.config.language;
        let iid = switch.iid.to_string();
        let shown = switch.clone.display().to_string();
        let local = switch.local_branch.as_str();
        let message = match switch.outcome {
            MrPrepOutcome::FastForwarded { by: 1 } => {
                tr_fmt(lang, "mr_checkout_done_ff_one", &[&iid, local, &shown])
            }
            MrPrepOutcome::FastForwarded { by } => tr_fmt(
                lang,
                "mr_checkout_done_ff",
                &[&iid, local, &shown, &by.to_string()],
            ),
            _ => tr_fmt(lang, "mr_checkout_done", &[&iid, local, &shown]),
        };
        if let MrPrepOutcome::Diverged { ahead, behind } = switch.outcome {
            self.mr_notice = Some(diverged_text(lang, local, ahead, behind));
        }
        let then = self.run_after_checkout(&switch.clone, None, &switch.after);
        self.set_status(with_then(message, then));
    }

    /// Wechsel abgebrochen oder gescheitert: kein „Danach öffnen“.
    pub(super) fn cancel_mr_switch(&mut self, path: &Path, branch: &str) {
        self.take_mr_switch(path, branch);
    }

    /// Nach dem Abarbeiten von `pending_branch_switches` (aus `poll_scan`):
    /// ein MR-Wechsel, der weder ausgeführt wurde noch im offenen Dialog
    /// wartet (Dialog durch einen anderen Wechsel ersetzt oder geschlossen,
    /// Eintrag per Dedup verdrängt), entfällt samt „Danach öffnen“ – sonst
    /// feuerte er bei einem späteren, manuellen Wechsel auf denselben Branch.
    pub(super) fn drop_stale_mr_switch(&mut self) {
        let waiting = match (&self.mr_switch, &self.branch_dialog) {
            (Some(switch), Some(dlg)) => {
                same_dir(&switch.clone, &dlg.repo_path) && switch.local_branch == dlg.target_branch
            }
            _ => false,
        };
        if !waiting {
            self.mr_switch = None;
        }
    }

    fn take_mr_switch(&mut self, path: &Path, branch: &str) -> Option<MrSwitch> {
        let matches = self
            .mr_switch
            .as_ref()
            .is_some_and(|s| same_dir(&s.clone, path) && s.local_branch == branch);
        if matches {
            self.mr_switch.take()
        } else {
            None
        }
    }

    /// „Danach öffnen“ (E5): Agent, Shell und Explorer sofort im Zielordner,
    /// die IDE erst nach dem Rescan (`pending_after_checkout`). Liefert den
    /// Zusatz der Meldung („– {Agent} startet“ …), wenn etwas startet.
    /// `base` = Basis-Klon eines neuen Worktrees (für die IDE).
    fn run_after_checkout(
        &mut self,
        path: &Path,
        base: Option<&Path>,
        after: &AfterAction,
    ) -> Option<String> {
        let lang = self.config.language;
        match after {
            AfterAction::Nothing => None,
            AfterAction::Agent(id) => {
                let name = self.agent_by_id(id).map(|a| a.display_name.clone());
                let started = self.open_agent(path, id);
                name.filter(|_| started)
                    .map(|name| tr_fmt(lang, "mr_then_agent", &[&name]))
            }
            AfterAction::Shell => {
                self.open_shell(path);
                Some(tr(lang, "mr_then_shell"))
            }
            AfterAction::Explorer => self
                .open_explorer(path)
                .then(|| tr(lang, "mr_then_explorer")),
            AfterAction::Ide => {
                // Dieselbe Auflösung wie nach dem Rescan: Zusatz und
                // gestartete IDE stimmen überein. Ohne IDE im wirksamen
                // Profil sagt die Meldung das, statt still nichts zu tun.
                let Some(ide) = self.checkout_ide(path, base) else {
                    return Some(tr(lang, "mr_then_no_ide"));
                };
                self.pending_after_checkout = Some((path.to_path_buf(), AfterAction::Ide));
                Some(tr_fmt(lang, "mr_then_ide", &[&ide.name]))
            }
        }
    }

    /// IDE nach dem Auschecken, sobald der Rescan den Ordner liefert (aus
    /// `poll_scan`); außerhalb der Suchpfade mit der IDE des Basis-Klons.
    pub(super) fn run_pending_after_checkout(&mut self) {
        if let Some((path, ide)) = self.take_pending_ide() {
            self.open_ide_in(&path, &ide.profile, &ide.id, &ide.file);
        }
    }

    /// Wartende IDE (einmalig) samt Auflösung mit dem aktuellen Scan.
    fn take_pending_ide(&mut self) -> Option<(PathBuf, IdeLaunch)> {
        let (path, after) = self.pending_after_checkout.take()?;
        // Agent, Shell und Explorer starten sofort, hier wartet nur die IDE
        if after != AfterAction::Ide {
            return None;
        }
        let ide = self.checkout_ide(&path, None)?;
        Some((path, ide))
    }

    /// IDE wie auf der Repo-Karte, siehe [`resolve_ide`].
    fn checkout_ide(&self, path: &Path, base: Option<&Path>) -> Option<IdeLaunch> {
        resolve_ide(&self.config, &self.repos, path, base)
    }

    /// Menü „Bereits ausgecheckt in {Pfad}“ (E5): Repo-Liste oder die
    /// Methoden der Repo-Karte.
    pub(super) fn open_existing(&mut self, path: &Path, target: &OpenTarget) {
        match target {
            OpenTarget::ShowInRepoList => self.show_in_repo_list(path),
            OpenTarget::Agent(id) => {
                self.open_agent(path, id);
            }
            OpenTarget::Shell => self.open_shell(path),
            OpenTarget::Explorer => {
                self.open_explorer(path);
            }
            OpenTarget::Ide => match self.checkout_ide(path, None) {
                Some(ide) => {
                    self.open_ide_in(path, &ide.profile, &ide.id, &ide.file);
                }
                None => {
                    let lang = self.config.language;
                    self.set_status(tr(lang, "mr_no_ide"));
                }
            },
        }
    }

    /// Wechselt in die Repo-Liste und hebt die Karte kurz hervor; ein
    /// Worktree außerhalb der Suchpfade zeigt die Karte seines Repos.
    fn show_in_repo_list(&mut self, path: &Path) {
        self.set_main_view(MainView::Repos);
        let card = self
            .repos
            .iter()
            .find(|r| same_dir(&r.path, path))
            .or_else(|| {
                self.repos
                    .iter()
                    .find(|r| r.worktrees.iter().any(|w| same_dir(&w.path, path)))
            })
            .map(|r| r.path.clone());
        self.repo_highlight = card.map(|path| {
            let highlight = RepoHighlight { path, scroll: true };
            (highlight, Instant::now())
        });
    }

    /// Hervorhebung der Repo-Liste für diesen Frame; gescrollt wird nur im
    /// ersten, nach `HIGHLIGHT_DURATION` endet sie.
    pub(super) fn repo_highlight_frame(&mut self, ctx: &egui::Context) -> Option<RepoHighlight> {
        let since = self.repo_highlight.as_ref()?.1;
        let Some(left) = HIGHLIGHT_DURATION.checked_sub(since.elapsed()) else {
            self.repo_highlight = None;
            return None;
        };
        ctx.request_repaint_after(left);
        let (highlight, _) = self.repo_highlight.as_mut()?;
        let frame = highlight.clone();
        highlight.scroll = false;
        Some(frame)
    }

    /// Hinweis aus dem Auschecken (`mr_notice`, z. B. divergierter lokaler
    /// Branch) über der MR-Ansicht; das Kreuz schließt ihn.
    pub(super) fn show_mr_notice(&mut self, ui: &mut Ui) {
        let Some(text) = self.mr_notice.clone() else {
            return;
        };
        let p = mr_palette(&self.config.theme, ui.visuals());
        let close = tr(self.config.language, "close");
        let mut closed = false;
        Frame::new()
            .fill(p.notice_bg)
            .stroke(Stroke::new(1.0, p.notice_stroke))
            .corner_radius(6)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        Image::new(ICON_WARNING)
                            .fit_to_exact_size(vec2(16.0, 16.0))
                            .tint(p.notice_text),
                    );
                    ui.label(RichText::new(&text).size(12.0).color(p.notice_text));
                    let resp = ui.add(Button::image(
                        Image::new(ICON_CROSS)
                            .fit_to_exact_size(vec2(12.0, 12.0))
                            .tint(p.notice_text),
                    ));
                    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, &close));
                    if resp.on_hover_text(close.as_str()).clicked() {
                        closed = true;
                    }
                });
            });
        ui.add_space(8.0);
        if closed {
            self.mr_notice = None;
        }
    }

    pub(super) fn set_status(&mut self, message: String) {
        self.status_message = Some(message);
        self.status_message_time = Some(Instant::now());
    }
}

/// Worker: git-Schritte je Ziel. Beim Vorbereiten ändern sich nur Refs; den
/// Wechsel eines bestehenden Klons macht danach `execute_branch_switch`.
fn run_job(job: &CheckoutJob) -> Result<Option<MrPrep>, MrCheckoutFailure> {
    let checkout = |error| MrCheckoutFailure::Checkout {
        error,
        network: false,
    };
    match job {
        CheckoutJob::Worktree { base, target, plan } => {
            preflight(base, plan, false)?;
            fetch_source(base, plan)?;
            let prep = git::prepare_mr_branch(base, plan).map_err(checkout)?;
            // Branch ist im Basis-Klon HEAD: „Bereits ausgecheckt“ statt Worktree
            if !matches!(prep.outcome, MrPrepOutcome::AlreadyHead { .. }) {
                git::add_worktree(base, target, &prep.local_branch).map_err(checkout)?;
            }
            Ok(Some(prep))
        }
        CheckoutJob::Clone {
            url,
            target,
            branch,
        } => {
            git::clone_repo(url, target, branch).map_err(|error| {
                // Außer dem belegten Zielordner: Netz oder Anmeldung
                let network = !matches!(error, CheckoutError::TargetNotEmpty { .. });
                MrCheckoutFailure::Checkout { error, network }
            })?;
            Ok(None)
        }
        CheckoutJob::Switch { clone, plan } => {
            preflight(clone, plan, true)?;
            fetch_source(clone, plan)?;
            git::prepare_mr_branch(clone, plan)
                .map(Some)
                .map_err(checkout)
        }
    }
}

/// Klon öffnen; vor einem Wechsel brechen Konflikte oder ein laufender
/// Merge/Rebase vor dem Netz ab (wie `handle_branch_switch`).
fn preflight(path: &Path, plan: &MrCheckoutPlan, switch: bool) -> Result<(), MrCheckoutFailure> {
    let repo = git::open_repo(path).ok_or_else(|| MrCheckoutFailure::RepoOpen {
        path: path.to_path_buf(),
    })?;
    if switch && (git::has_merge_conflicts(&repo) || git::is_merge_in_progress(&repo)) {
        return Err(MrCheckoutFailure::Blocked {
            branch: plan.source_branch.clone(),
            state: format!("{:?}", repo.state()),
        });
    }
    Ok(())
}

/// Holt nur den Quell-Branch (`+refs/heads/<src>:refs/remotes/<remote>/<src>`,
/// ohne Prune). Das Repo ist offen, ein Fehler kommt also vom Remote.
fn fetch_source(path: &Path, plan: &MrCheckoutPlan) -> Result<(), MrCheckoutFailure> {
    let refspec = plan.refspec();
    git::fetch_refspecs(path, &plan.remote, &[&refspec]).map_err(|e| {
        let (detail, _, _) = fetch_error_parts(&e);
        eprintln!("MR-Fetch für {} fehlgeschlagen: {detail}", path.display());
        MrCheckoutFailure::Fetch { detail }
    })
}

/// Bannertext eines gescheiterten Auscheckens (E20: `self.error`).
fn failure_text(lang: Language, iid: u64, failure: &MrCheckoutFailure) -> String {
    let with_hint = |detail: &str| format!("{detail}. {}", tr(lang, "auth_hint"));
    match failure {
        MrCheckoutFailure::RepoOpen { path } => {
            let detail = tr_fmt(lang, "mr_repo_open_failed", &[&path.display().to_string()]);
            tr_fmt(lang, "mr_checkout_failed", &[&iid.to_string(), &detail])
        }
        MrCheckoutFailure::Fetch { detail } => {
            tr_fmt(lang, fetch_error_i18n_key(false), &[&with_hint(detail)])
        }
        MrCheckoutFailure::Blocked { branch, state } => {
            tr_fmt(lang, "branch_switch_blocked", &[branch, state])
        }
        MrCheckoutFailure::Checkout { error, network } => {
            let detail = checkout_error_text(lang, error);
            let detail = if *network { with_hint(&detail) } else { detail };
            tr_fmt(lang, "mr_checkout_failed", &[&iid.to_string(), &detail])
        }
    }
}

/// `CheckoutError` lokalisiert (Display bleibt Deutsch wie in `git.rs`);
/// sonstige git-Fehler unverändert.
fn checkout_error_text(lang: Language, error: &CheckoutError) -> String {
    match error {
        CheckoutError::CheckedOutElsewhere { branch, path } => tr_fmt(
            lang,
            "branch_checked_out_elsewhere",
            &[branch, &path.display().to_string()],
        ),
        CheckoutError::RemoteBranchMissing { upstream } => {
            tr_fmt(lang, "mr_remote_branch_missing", &[upstream])
        }
        CheckoutError::BranchNameTaken { branch, fallback } => {
            tr_fmt(lang, "mr_branch_name_taken", &[branch, fallback])
        }
        CheckoutError::TargetNotEmpty { path } => {
            tr_fmt(lang, "mr_folder_exists", &[&path.display().to_string()])
        }
        CheckoutError::Git(message) => message.clone(),
    }
}

/// „Lokaler Branch {L} weicht vom MR-Stand ab …“ (`mr_notice`).
fn diverged_text(lang: Language, branch: &str, ahead: usize, behind: usize) -> String {
    tr_fmt(
        lang,
        "mr_diverged",
        &[branch, &ahead.to_string(), &behind.to_string()],
    )
}

/// IDE wie auf der Repo-Karte: erste sichtbare IDE des wirksamen
/// Profils, Projektdatei = gewählte Solution. Ordner ohne `RepoInfo`:
/// in den Suchpfaden wie die künftige Karte (Profil des Ordners, der
/// Ordner selbst); außerhalb Profil des Basis-Klons (Repo, zu dem er als
/// Worktree gehört, vor dem Rescan `base`), dessen Solution relativ, wenn
/// es sie dort gibt; sonst das aktive Profil und der Ordner selbst.
fn resolve_ide(
    config: &AppConfig,
    repos: &[RepoInfo],
    path: &Path,
    base: Option<&Path>,
) -> Option<IdeLaunch> {
    let scanned = |p: &Path| repos.iter().find(|r| same_dir(&r.path, p));
    let base_repo = || {
        repos
            .iter()
            .find(|r| r.worktrees.iter().any(|w| same_dir(&w.path, path)))
            .or_else(|| base.and_then(scanned))
    };
    let (profile_path, file) = if let Some(repo) = scanned(path) {
        (repo.path.clone(), repo.selected_solution.clone())
    } else if in_search_roots(config, path) {
        (path.to_path_buf(), None)
    } else if let Some(base) = base_repo() {
        let file = base
            .selected_solution
            .as_ref()
            .and_then(|s| s.strip_prefix(&base.path).ok())
            .map(|rel| path.join(rel))
            .filter(|f| f.is_file());
        (base.path.clone(), file)
    } else {
        (path.to_path_buf(), None)
    };
    let profile = config.get_effective_profile_for_repo(&profile_path);
    let ide = profile.visible_ides().into_iter().next()?;
    Some(IdeLaunch {
        id: ide.id.clone(),
        name: ide.display_name.clone(),
        file: file.unwrap_or_else(|| path.to_path_buf()),
        profile: profile_path,
    })
}

/// Name der IDE, die „In {IDE} öffnen“ der MR-Ansicht für `path` startet
/// (`open_existing`): dieselbe Auflösung, also auch ein Profil-Override des
/// Repos; `None` = keine IDE im wirksamen Profil.
pub(super) fn ide_name_for(config: &AppConfig, repos: &[RepoInfo], path: &Path) -> Option<String> {
    resolve_ide(config, repos, path, None).map(|ide| ide.name)
}

/// Liegt `path` unter einem Suchpfad? Dann liefert ihn der Rescan als
/// eigene Karte.
fn in_search_roots(config: &AppConfig, path: &Path) -> bool {
    let path = PathBuf::from(path_key(path));
    config
        .roots
        .iter()
        .any(|root| path.starts_with(path_key(root)))
}

/// Meldung mit Zusatz „– {Agent} startet“ usw.
fn with_then(message: String, then: Option<String>) -> String {
    match then {
        Some(then) => format!("{message} {then}"),
        None => message,
    }
}

/// MR des Stands per Host und globaler ID (`iid` ist nur je Projekt
/// eindeutig).
fn find_mr<'s>(snapshot: &'s MrSnapshot, host_key: &str, id: &str) -> Option<&'s MergeRequest> {
    let uninvolved = snapshot.uninvolved.iter().flat_map(|u| &u.mrs);
    snapshot
        .mine
        .iter()
        .chain(&snapshot.others)
        .chain(uninvolved)
        .find(|m| m.host_key == host_key && m.id == id)
}

/// Gleicher Ordner trotz Schreibweise (Separatoren, unter Windows Groß-/
/// Kleinschreibung, siehe [`path_key`]).
fn same_dir(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_app;
    use super::*;
    use crate::git::WorktreeHead;
    use crate::gitlab::remote::{GlabHost, RemoteInfo};
    use tempfile::tempdir;

    const HOST: &str = "gitlab.example.com";
    const MR_ID: &str = "gid://gitlab/MergeRequest/5";
    const REMOTE: &str = "git@gitlab.example.com:group/app.git";

    fn mr5() -> MergeRequest {
        MergeRequest {
            host_key: HOST.to_string(),
            id: MR_ID.to_string(),
            iid: 5,
            title: "MR 5".to_string(),
            project_path: "group/app".to_string(),
            source_project_path: Some("group/app".to_string()),
            source_branch: "feature/x".to_string(),
            target_branch: "main".to_string(),
            source_branch_exists: Some(true),
            ..MergeRequest::default()
        }
    }

    fn choice(kind: MrCheckoutKind, after: AfterAction) -> MrCheckoutChoice {
        MrCheckoutChoice {
            host_key: HOST.to_string(),
            mr_id: MR_ID.to_string(),
            iid: 5,
            kind,
            after,
        }
    }

    fn worktree(target: &Path, base: &Path, after: AfterAction) -> MrCheckoutChoice {
        let kind = MrCheckoutKind::NewWorktree {
            path: target.to_path_buf(),
            base: base.to_path_buf(),
        };
        choice(kind, after)
    }

    fn switch(clone: &Path, after: AfterAction) -> MrCheckoutChoice {
        let kind = MrCheckoutKind::Switch {
            clone: clone.to_path_buf(),
        };
        choice(kind, after)
    }

    fn prep(outcome: MrPrepOutcome) -> Result<Option<MrPrep>, MrCheckoutFailure> {
        Ok(Some(MrPrep {
            local_branch: "feature/x".to_string(),
            outcome,
        }))
    }

    /// Klon von `group/app` (Remote wie bei GitLab, auf der Platte zeigt
    /// `origin` ggf. auf einen lokalen Upstream).
    fn clone_info(path: &Path, branch: &str) -> RepoInfo {
        let mut repo = RepoInfo::new(path.to_path_buf(), branch.to_string(), false, false);
        repo.remotes = vec![RemoteInfo::new("origin", REMOTE)];
        repo
    }

    /// App mit MR !5 im Stand und `repos` als Scan. Folge-Scans ohne
    /// Suchpfade (keine Platte), kein MR-Abruf (Intervall läuft noch).
    fn app_with(repos: Vec<RepoInfo>) -> MyApp {
        let mut app = test_app();
        app.config.roots.clear();
        app.config.language = Language::En;
        app.mr_last_refresh = Some(Instant::now());
        app.glab_hosts = vec![GlabHost {
            key: HOST.to_string(),
            user: "taltmann".to_string(),
            ..GlabHost::default()
        }];
        app.repos = repos;
        app.update_mr_hosts();
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            others: vec![mr5()],
            ..MrSnapshot::default()
        }));
        app
    }

    /// Übernimmt Scan-Ergebnisse, bis weder Auschecken noch Scan laufen
    /// (höchstens 10 s).
    fn settle(app: &mut MyApp) {
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            app.poll_scan(&ctx);
            if !app.git_busy.holds_refs() {
                return;
            }
            assert!(Instant::now() < until, "Checkout-Worker hängt");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Upstream mit `main` und dem MR-Branch `feature/x` (ein Commit
    /// weiter), Klon nur mit `main`; `origin` des Klons zeigt auf den
    /// Upstream-Ordner (git2, lokal, kein Netz).
    fn upstream_and_clone(dir: &Path) -> PathBuf {
        let upstream = dir.join("upstream");
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        let repo = git2::Repository::init_opts(&upstream, &opts).unwrap();
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();
        std::fs::write(upstream.join("a.txt"), "one\n").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(Path::new("a.txt")).unwrap();
        idx.write().unwrap();
        let tree = repo.find_tree(idx.write_tree().unwrap()).unwrap();
        let first = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let first = repo.find_commit(first).unwrap();
        let clone = dir.join("app");
        git2::Repository::clone(upstream.to_str().unwrap(), &clone).unwrap();
        // MR-Branch erst nach dem Klonen: der Klon muss ihn holen
        let blob = repo.blob(b"two\n").unwrap();
        let mut tb = repo.treebuilder(Some(&tree)).unwrap();
        tb.insert("b.txt", blob, 0o100644).unwrap();
        let tree2 = repo.find_tree(tb.write().unwrap()).unwrap();
        repo.commit(
            Some("refs/heads/feature/x"),
            &sig,
            &sig,
            "mr",
            &tree2,
            &[&first],
        )
        .unwrap();
        clone
    }

    fn head_of(path: &Path) -> String {
        let repo = git2::Repository::open(path).unwrap();
        let head = repo.head().unwrap();
        head.shorthand().unwrap().to_string()
    }

    /// Die ausgelagerten Handler der Repo-Karte verhalten sich wie vorher.
    #[test]
    fn open_methods_behave_like_repo_card() {
        let mut app = test_app();
        let path = PathBuf::from("/tmp/gm-open/app");
        assert!(app.open_explorer(&path));
        assert_eq!(
            app.status_message.as_deref(),
            Some("Explorer opened: /tmp/gm-open/app")
        );
        app.open_shell(&path);
        assert_eq!(
            app.status_message.as_deref(),
            Some("Shell opened: /tmp/gm-open/app")
        );
        assert!(app.open_agent(&path, "claude"));
        assert_eq!(
            app.status_message.as_deref(),
            Some("Starting Claude Code in /tmp/gm-open/app...")
        );
        // Unbekannte ID: der aktive Agent startet
        assert!(app.open_agent(&path, "gone"));
        assert!(app.error.is_none());
        app.config.agents.clear();
        assert!(!app.open_agent(&path, "claude"));
        assert_eq!(app.error.as_deref(), Some("AI agent 'claude' not found"));
        let key = AppConfig::repo_state_key(&path);
        assert_eq!(app.config.repo_usage[&key].open_count, 5);
        assert!(app.config_save_pending);
    }

    /// Neuer Worktree mit echtem git (lokaler Upstream): Fetch, Branch
    /// vorbereiten, Worktree anlegen, Rescan, „Danach öffnen“.
    #[test]
    fn worktree_checkout_end_to_end() {
        let dir = tempdir().unwrap();
        let clone = upstream_and_clone(dir.path());
        let target = dir.path().join("review").join("app-mr-5");
        let mut app = app_with(vec![clone_info(&clone, "main")]);
        app.main_view = MainView::MergeRequests;
        app.mr_notice = Some("alt".to_string());
        app.start_mr_checkout(worktree(&target, &clone, AfterAction::Shell));
        assert_eq!(
            app.git_busy,
            GitBusy::Checkout { iid: 5 },
            "Ref-Lock während des Auscheckens"
        );
        assert_eq!(
            app.status_message.as_deref(),
            Some("Fetching feature/x for !5 …")
        );
        assert!(
            app.mr_notice.is_none(),
            "neues Auschecken räumt den Hinweis"
        );
        // Danach-Wahl gemerkt, gespeichert wird gebatcht
        assert_eq!(
            app.config.merge_requests.after_checkout_last,
            AfterAction::Shell
        );
        assert!(app.config_save_pending);
        settle(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(head_of(&target), "feature/x");
        assert!(target.join("b.txt").exists());
        assert_eq!(head_of(&clone), "main", "Basis-Klon unverändert");
        assert_eq!(
            app.status_message,
            Some(format!(
                "Worktree {} created for !5 – opening shell",
                target.display()
            ))
        );
        assert_eq!(app.main_view, MainView::MergeRequests);
    }

    /// Neuer Klon mit echtem git: klont den Upstream mit dem MR-Branch.
    #[test]
    fn clone_checkout_end_to_end() {
        let dir = tempdir().unwrap();
        let clone = upstream_and_clone(dir.path());
        let upstream = dir.path().join("upstream");
        let target = dir.path().join("review").join("app-mr-5");
        let mut app = app_with(vec![clone_info(&clone, "main")]);
        let kind = MrCheckoutKind::NewClone {
            path: target.clone(),
            url: upstream.display().to_string(),
        };
        app.start_mr_checkout(choice(kind, AfterAction::Nothing));
        assert_eq!(
            app.status_message,
            Some(format!(
                "Cloning {} to {} …",
                upstream.display(),
                target.display()
            ))
        );
        settle(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(head_of(&target), "feature/x");
        assert_eq!(
            app.status_message,
            Some(format!("!5 cloned to {}", target.display()))
        );
    }

    /// Ergebnis „Worktree“: Rescan, Agent sofort; die IDE wartet auf den
    /// Rescan, „Nichts“ startet nichts.
    #[test]
    fn ready_worktree_starts_agent_and_ide_waits_for_rescan() {
        let mut app = app_with(Vec::new());
        app.main_view = MainView::MergeRequests;
        let target = PathBuf::from("/tmp/gm-review/app-mr-5");
        let base = PathBuf::from("/tmp/gm-mr/app");
        let agent = AfterAction::Agent("claude".to_string());
        app.on_mr_checkout_ready(
            worktree(&target, &base, agent),
            prep(MrPrepOutcome::Created),
        );
        assert_eq!(app.git_busy, GitBusy::Scan, "Rescan angestoßen");
        assert!(app.pending_after_checkout.is_none(), "Agent startet sofort");
        assert_eq!(
            app.status_message,
            Some(format!(
                "Worktree {} created for !5 – starting Claude Code",
                target.display()
            ))
        );
        let key = AppConfig::repo_state_key(&target);
        assert_eq!(app.config.repo_usage[&key].open_count, 1);
        assert_eq!(app.main_view, MainView::MergeRequests);
        settle(&mut app);
        // Nichts: nur die Meldung
        app.on_mr_checkout_ready(
            worktree(&target, &base, AfterAction::Nothing),
            prep(MrPrepOutcome::UpToDate),
        );
        assert_eq!(
            app.status_message,
            Some(format!("Worktree {} created for !5", target.display()))
        );
        assert!(app.pending_after_checkout.is_none());
        assert_eq!(app.config.repo_usage[&key].open_count, 1);
        settle(&mut app);
        // IDE: Profil und Projektdatei erst mit dem Rescan
        app.on_mr_checkout_ready(
            worktree(&target, &base, AfterAction::Ide),
            prep(MrPrepOutcome::Created),
        );
        assert_eq!(
            app.pending_after_checkout,
            Some((target.clone(), AfterAction::Ide))
        );
        assert_eq!(
            app.status_message,
            Some(format!(
                "Worktree {} created for !5 – opening Visual Studio",
                target.display()
            ))
        );
        assert_eq!(app.config.repo_usage[&key].open_count, 1);
    }

    /// Die wartende IDE wird mit dem nächsten Scan genau einmal behandelt
    /// (Profil ohne IDE: nichts zu starten, kein Fehler).
    #[test]
    fn pending_ide_is_handled_once_by_next_scan() {
        let mut app = app_with(Vec::new());
        for profile in &mut app.config.profiles {
            profile.ides.clear();
        }
        let target = PathBuf::from("/tmp/gm-review/app-mr-5");
        app.pending_after_checkout = Some((target.clone(), AfterAction::Ide));
        app.scan_tx.send(ScanResult::Repos(Vec::new())).unwrap();
        app.poll_scan(&egui::Context::default());
        assert!(app.pending_after_checkout.is_none());
        assert!(app.error.is_none());
        let key = AppConfig::repo_state_key(&target);
        assert!(!app.config.repo_usage.contains_key(&key));
    }

    /// IDE wie auf der Repo-Karte: erste sichtbare IDE des Profils, gewählte
    /// Solution; außerhalb der Suchpfade die des Basis-Klons.
    #[test]
    fn checkout_ide_like_repo_card() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("app");
        let outside = dir.path().join("review").join("app-mr-5");
        std::fs::create_dir_all(outside.join("src")).unwrap();
        std::fs::write(outside.join("src").join("App.sln"), "").unwrap();
        let mut base_info = clone_info(&base, "main");
        base_info.selected_solution = Some(base.join("src").join("App.sln"));
        base_info.worktrees = vec![WorktreeHead {
            path: outside.clone(),
            branch: Some("feature/x".to_string()),
        }];
        let lib = dir.path().join("lib");
        let mut lib_info = clone_info(&lib, "main");
        lib_info.selected_solution = Some(lib.join("Lib.sln"));
        let mut app = app_with(vec![base_info, lib_info]);
        let ide = app.checkout_ide(&lib, None).expect("IDE");
        assert_eq!(ide.id, "vs2022");
        assert_eq!(ide.name, "Visual Studio");
        assert_eq!(ide.file, lib.join("Lib.sln"));
        assert_eq!(ide.profile, lib);
        // Worktree außerhalb: Profil des Basis-Klons, Solution relativ
        let mut web = app.config.profiles[0].clone();
        web.id = "web".to_string();
        web.ide_order = vec!["vscode".to_string()];
        app.config.profiles.push(web);
        app.config
            .set_repo_profile_override(&base, Some("web".to_string()));
        let ide = app.checkout_ide(&outside, None).expect("IDE");
        assert_eq!(ide.id, "vscode");
        assert_eq!(ide.file, outside.join("src").join("App.sln"));
        assert_eq!(ide.profile, base, "IDE aus dem Profil des Basis-Klons");
        // Vor dem Rescan kennt der Basis-Klon den Worktree noch nicht: der
        // Hinweis `base` liefert dieselbe IDE
        let fresh = dir.path().join("review").join("app-mr-6");
        std::fs::create_dir_all(fresh.join("src")).unwrap();
        std::fs::write(fresh.join("src").join("App.sln"), "").unwrap();
        let ide = app.checkout_ide(&fresh, Some(&base)).expect("IDE");
        assert_eq!(
            (ide.id.as_str(), ide.profile.as_path()),
            ("vscode", base.as_path())
        );
        assert_eq!(ide.file, fresh.join("src").join("App.sln"));
        // In den Suchpfaden wie die künftige Karte: Profil des Ordners
        app.config.roots = vec![dir.path().join("review")];
        let ide = app.checkout_ide(&fresh, Some(&base)).expect("IDE");
        assert_eq!(
            (ide.id.as_str(), ide.file.as_path()),
            ("vs2022", fresh.as_path())
        );
        assert_eq!(ide.profile, fresh);
        app.config.roots.clear();
        // Ohne Solution im Worktree: der Ordner
        std::fs::remove_file(outside.join("src").join("App.sln")).unwrap();
        assert_eq!(app.checkout_ide(&outside, None).expect("IDE").file, outside);
        // Unbekannter Ordner (neuer Klon außerhalb): aktives Profil
        let other = dir.path().join("other");
        let ide = app.checkout_ide(&other, None).expect("IDE");
        assert_eq!(ide.id, "vs2022");
        assert_eq!(ide.file, other);
    }

    /// Basis-Klon mit Override „java“, dessen einzige IDE das aktive Profil
    /// nicht kennt (Programm `code`: startet in Tests nicht wirklich); das
    /// aktive Profil hat keine IDE.
    fn base_with_own_ide(app: &mut MyApp, base: &Path) {
        let active = app.config.get_active_profile().id.clone();
        let mut java = app.config.get_active_profile().clone();
        java.id = "java".to_string();
        java.ides = vec![crate::config::IdeConfig {
            id: "gm-test-ide".to_string(),
            display_name: "Test IDE".to_string(),
            program: "code".to_string(),
            args: Vec::new(),
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,
            icon: None,
        }];
        java.default_ide_id = None;
        java.ide_order.clear();
        java.hidden_ide_ids.clear();
        app.config.profiles.push(java);
        for profile in app.config.profiles.iter_mut().filter(|p| p.id == active) {
            profile.ides.clear();
        }
        app.config
            .set_repo_profile_override(base, Some("java".to_string()));
    }

    /// Worktree außerhalb der Suchpfade mit „IDE“: schon vor dem Rescan die
    /// IDE des Basis-Klons (Zusatz der Meldung), nach dem Rescan startet
    /// genau sie mit dem Profil des Basis-Klons.
    #[test]
    fn pending_ide_uses_base_clone_profile() {
        let dir = tempdir().unwrap();
        let base = dir.path().join("app");
        let target = dir.path().join("review").join("app-mr-5");
        std::fs::create_dir_all(target.join("src")).unwrap();
        std::fs::write(target.join("src").join("App.sln"), "").unwrap();
        let mut base_info = clone_info(&base, "main");
        base_info.selected_solution = Some(base.join("src").join("App.sln"));
        let mut app = app_with(vec![base_info.clone()]);
        base_with_own_ide(&mut app, &base);
        app.on_mr_checkout_ready(
            worktree(&target, &base, AfterAction::Ide),
            prep(MrPrepOutcome::Created),
        );
        assert_eq!(
            app.pending_after_checkout,
            Some((target.clone(), AfterAction::Ide)),
            "wartet trotz aktivem Profil ohne IDE"
        );
        assert_eq!(
            app.status_message,
            Some(format!(
                "Worktree {} created for !5 – opening Test IDE",
                target.display()
            ))
        );
        // Rescan: der Basis-Klon listet den neuen Worktree
        base_info.worktrees = vec![WorktreeHead {
            path: target.clone(),
            branch: Some("feature/x".to_string()),
        }];
        app.repos = vec![base_info];
        let (path, ide) = app.take_pending_ide().expect("IDE startet");
        assert_eq!(path, target);
        assert_eq!(
            ide,
            IdeLaunch {
                id: "gm-test-ide".to_string(),
                name: "Test IDE".to_string(),
                file: target.join("src").join("App.sln"),
                profile: base.clone(),
            }
        );
        assert!(app.pending_after_checkout.is_none(), "genau einmal");
        assert!(app.take_pending_ide().is_none());
    }

    /// Bestehender Klon: Eintrag in `pending_branch_switches`; Meldung,
    /// Hinweis und „Danach öffnen“ erst nach erfolgreichem Wechsel.
    #[test]
    fn switch_queues_branch_switch_and_opens_after_success() {
        let clone = PathBuf::from("/tmp/gm-mr/app");
        let mut app = app_with(vec![clone_info(&clone, "main")]);
        app.on_mr_checkout_ready(
            switch(&clone, AfterAction::Explorer),
            prep(MrPrepOutcome::FastForwarded { by: 2 }),
        );
        assert_eq!(
            app.pending_branch_switches,
            vec![(clone.clone(), "feature/x".to_string())]
        );
        assert!(app.mr_switch.is_some());
        assert!(
            app.status_message.is_none(),
            "Meldung erst nach dem Wechsel"
        );
        assert_eq!(app.git_busy, GitBusy::Idle, "Rescan erst nach dem Wechsel");
        // Anderer Branch im selben Klon: kein MR-Wechsel
        app.mr_switch_succeeded(&clone, "main");
        assert!(app.mr_switch.is_some());
        assert!(app.status_message.is_none());
        app.mr_switch_succeeded(&clone, "feature/x");
        assert!(app.mr_switch.is_none());
        assert_eq!(
            app.status_message,
            Some(format!(
                "!5 checked out: feature/x in {} (+2 commits) – opening Explorer",
                clone.display()
            ))
        );
        // Ein Commit: Einzahl
        app.on_mr_checkout_ready(
            switch(&clone, AfterAction::Nothing),
            prep(MrPrepOutcome::FastForwarded { by: 1 }),
        );
        app.mr_switch_succeeded(&clone, "feature/x");
        assert_eq!(
            app.status_message,
            Some(format!(
                "!5 checked out: feature/x in {} (+1 commit)",
                clone.display()
            ))
        );
        // Bereits HEAD im Klon: kein Wechsel, nur „Danach öffnen“
        app.pending_branch_switches.clear();
        app.on_mr_checkout_ready(
            switch(&clone, AfterAction::Shell),
            prep(MrPrepOutcome::AlreadyHead { behind: 2 }),
        );
        assert!(app.pending_branch_switches.is_empty());
        assert!(app.mr_switch.is_none());
        assert_eq!(
            app.status_message,
            Some(format!(
                "Already checked out in {} (2 behind) – opening shell",
                clone.display()
            ))
        );
    }

    /// Bestehender Klon mit echtem git: Fetch, Branch vorbereiten, Wechsel
    /// im nächsten `poll_scan`, dann Meldung und Rescan.
    #[test]
    fn switch_checkout_end_to_end() {
        let dir = tempdir().unwrap();
        let clone = upstream_and_clone(dir.path());
        let mut app = app_with(vec![clone_info(&clone, "main")]);
        app.start_mr_checkout(switch(&clone, AfterAction::Shell));
        settle(&mut app);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(head_of(&clone), "feature/x");
        assert!(app.mr_switch.is_none());
        assert_eq!(
            app.status_message,
            Some(format!(
                "!5 checked out: feature/x in {} – opening shell",
                clone.display()
            ))
        );
    }

    /// Änderungen im Klon: Dialog; Abbrechen startet nichts, „Stash &
    /// Switch“ wechselt und öffnet danach.
    #[test]
    fn switch_dialog_cancel_runs_no_after_action() {
        let dir = tempdir().unwrap();
        let clone = upstream_and_clone(dir.path());
        // Scan-Stand mit Änderungen → Dialog (wie `handle_branch_switch`)
        let mut info = clone_info(&clone, "main");
        info.dirty = true;
        let mut app = app_with(vec![info]);
        app.start_mr_checkout(switch(&clone, AfterAction::Shell));
        settle(&mut app);
        assert!(app.branch_dialog.is_some(), "Dialog bei Änderungen");
        assert!(app.mr_switch.is_some());
        app.close_branch_dialog();
        assert!(app.branch_dialog.is_none());
        assert!(app.mr_switch.is_none());
        assert_eq!(head_of(&clone), "main");
        assert!(!app
            .status_message
            .as_deref()
            .is_some_and(|m| m.contains("shell")));
        // Zweiter Versuch mit „Stash & Switch“
        app.start_mr_checkout(switch(&clone, AfterAction::Shell));
        settle(&mut app);
        let dlg = app.branch_dialog.as_ref().expect("Dialog");
        let (path, branch) = (dlg.repo_path.clone(), dlg.target_branch.clone());
        app.execute_branch_switch(path, branch, false, true);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(head_of(&clone), "feature/x");
        assert_eq!(
            app.status_message,
            Some(format!(
                "!5 checked out: feature/x in {} – opening shell",
                clone.display()
            ))
        );
        settle(&mut app);
    }

    /// „Stash & Switch“, dessen Stash sich danach nicht poppen lässt: der
    /// Wechsel zählt (MR-Meldung, „Danach öffnen“), der Dialog zeigt wie
    /// beim normalen Wechsel zusätzlich den Pop-Hinweis.
    #[test]
    fn switch_with_failed_stash_pop_still_succeeds() {
        let dir = tempdir().unwrap();
        let clone = upstream_and_clone(dir.path());
        std::fs::write(clone.join("a.txt"), "local\n").unwrap();
        // Lock-Ordner auf dem Stash-Reflog: Pop scheitert nach dem Wechsel
        std::fs::create_dir_all(clone.join(".git/logs/refs/stash.lock")).unwrap();
        let mut info = clone_info(&clone, "main");
        info.dirty = true;
        let mut app = app_with(vec![info]);
        app.start_mr_checkout(switch(&clone, AfterAction::Shell));
        settle(&mut app);
        let dlg = app.branch_dialog.as_ref().expect("Dialog");
        let (path, branch) = (dlg.repo_path.clone(), dlg.target_branch.clone());
        app.execute_branch_switch(path, branch, false, true);
        assert_eq!(head_of(&clone), "feature/x");
        assert!(app.mr_switch.is_none());
        assert_eq!(
            app.status_message,
            Some(format!(
                "!5 checked out: feature/x in {} – opening shell",
                clone.display()
            ))
        );
        let hint = app.branch_dialog.as_ref().and_then(|d| d.error.as_deref());
        assert!(
            hint.is_some_and(|h| h.contains("Stash bleibt unter stash@{0}")),
            "{hint:?}"
        );
        assert!(app.error.is_none(), "{:?}", app.error);
        app.close_branch_dialog();
        settle(&mut app);
    }

    /// Fehler gehen ins Banner (`self.error`, beide Ansichten), ohne Rescan.
    #[test]
    fn checkout_errors_go_to_banner() {
        let mut app = app_with(Vec::new());
        app.main_view = MainView::MergeRequests;
        let clone = PathBuf::from("/tmp/gm-mr/app");
        let target = PathBuf::from("/tmp/gm-review/app-mr-5");
        let fetch = MrCheckoutFailure::Fetch {
            detail: "'origin': auth failed".to_string(),
        };
        app.on_mr_checkout_ready(switch(&clone, AfterAction::Shell), Err(fetch));
        assert_eq!(
            app.error.as_deref(),
            Some(
                "Fetch failed: 'origin': auth failed. Note: check Credentials/SSH (auth may be required)"
            )
        );
        assert_eq!(app.git_busy, GitBusy::Idle, "kein Rescan");
        assert!(app.pending_branch_switches.is_empty() && app.mr_switch.is_none());
        assert_eq!(app.main_view, MainView::MergeRequests);
        let taken = MrCheckoutFailure::Checkout {
            error: CheckoutError::TargetNotEmpty {
                path: target.clone(),
            },
            network: false,
        };
        app.on_mr_checkout_ready(worktree(&target, &clone, AfterAction::Nothing), Err(taken));
        assert_eq!(
            app.error,
            Some(format!(
                "Checking out !5 failed: The folder {} already exists and is not empty",
                target.display()
            ))
        );
        let blocked = MrCheckoutFailure::Blocked {
            branch: "feature/x".to_string(),
            state: "Merge".to_string(),
        };
        app.on_mr_checkout_ready(switch(&clone, AfterAction::Nothing), Err(blocked));
        assert_eq!(
            app.error.as_deref(),
            Some(
                "Cannot switch to branch “feature/x”: merge conflicts or a merge/rebase in progress (Merge)"
            )
        );
        let elsewhere = MrCheckoutFailure::Checkout {
            error: CheckoutError::CheckedOutElsewhere {
                branch: "feature/x".to_string(),
                path: target.clone(),
            },
            network: false,
        };
        app.on_mr_checkout_ready(switch(&clone, AfterAction::Nothing), Err(elsewhere));
        assert_eq!(
            app.error,
            Some(format!(
                "Checking out !5 failed: Branch feature/x is checked out in {}",
                target.display()
            ))
        );
        // Klonen: Netz/Anmeldung → Auth-Hinweis
        let cloning = MrCheckoutFailure::Checkout {
            error: CheckoutError::Git("Klonen fehlgeschlagen: denied".to_string()),
            network: true,
        };
        let kind = MrCheckoutKind::NewClone {
            path: target.clone(),
            url: REMOTE.to_string(),
        };
        app.on_mr_checkout_ready(choice(kind, AfterAction::Nothing), Err(cloning));
        assert_eq!(
            app.error.as_deref(),
            Some("Checking out !5 failed: Klonen fehlgeschlagen: denied. Note: check Credentials/SSH (auth may be required)")
        );
        assert_eq!(app.git_busy, GitBusy::Idle);
        // Deutsch: Remote-Branch fehlt
        app.config.language = Language::De;
        let missing = MrCheckoutFailure::Checkout {
            error: CheckoutError::RemoteBranchMissing {
                upstream: "origin/feature/x".to_string(),
            },
            network: false,
        };
        app.on_mr_checkout_ready(switch(&clone, AfterAction::Nothing), Err(missing));
        assert_eq!(
            app.error.as_deref(),
            Some("Auschecken von !5 fehlgeschlagen: Remote-Branch origin/feature/x nicht gefunden")
        );
    }

    #[test]
    fn fetch_aborted_clears_checkout_busy() {
        let mut app = app_with(Vec::new());
        app.git_busy = GitBusy::Checkout { iid: 5 };
        app.scan_tx
            .send(ScanResult::FetchAborted {
                reason_key: "fetch_aborted",
            })
            .unwrap();
        app.poll_scan(&egui::Context::default());
        assert_eq!(app.git_busy, GitBusy::Idle);
        assert!(app.error.is_some());
    }

    /// Während Scan oder Auto-Fetch kein Start (Ref-Race), mit Hinweis.
    #[test]
    fn no_checkout_while_scan_or_auto_fetch() {
        let clone = PathBuf::from("/tmp/gm-mr/app");
        let mut app = app_with(vec![clone_info(&clone, "main")]);
        app.git_busy = GitBusy::Scan;
        app.start_mr_checkout(switch(&clone, AfterAction::Explorer));
        assert_eq!(
            app.status_message.as_deref(),
            Some("Checkout not possible while a scan or fetch is running")
        );
        assert_eq!(app.git_busy, GitBusy::Scan);
        // Die Wahl „Danach öffnen“ bleibt trotzdem gemerkt
        assert_eq!(
            app.config.merge_requests.after_checkout_last,
            AfterAction::Explorer
        );
        app.git_busy = GitBusy::Idle;
        app.auto_fetching = true;
        app.status_message = None;
        app.start_mr_checkout(switch(&clone, AfterAction::Explorer));
        assert_eq!(
            app.status_message.as_deref(),
            Some("Checkout not possible while a scan or fetch is running")
        );
        assert_eq!(app.git_busy, GitBusy::Idle);
    }

    /// Das Auschecken hält den Ref-Lock: der Auto-Fetch wartet.
    #[test]
    fn auto_fetch_waits_for_mr_checkout() {
        let clone = PathBuf::from("/tmp/gm-mr-missing/app");
        let mut app = app_with(vec![clone_info(&clone, "main")]);
        app.last_auto_fetch = None;
        app.start_mr_checkout(switch(&clone, AfterAction::Nothing));
        assert_eq!(app.git_busy, GitBusy::Checkout { iid: 5 });
        app.maybe_auto_fetch(&egui::Context::default());
        assert!(
            !app.auto_fetching,
            "kein Auto-Fetch während des Auscheckens"
        );
        settle(&mut app);
        // Klon fehlt auf der Platte → Fehler im Banner, kein Wechsel
        assert_eq!(
            app.error,
            Some(format!(
                "Checking out !5 failed: Cannot open repository {}",
                clone.display()
            ))
        );
        assert!(app.pending_branch_switches.is_empty());
    }

    /// Während des Auscheckens angefragter Scan (Einstellungen, Tray) wird
    /// danach nachgeholt; der Fehler des Auscheckens bleibt dabei stehen.
    #[test]
    fn scan_requested_during_checkout_runs_afterwards() {
        let clone = PathBuf::from("/tmp/gm-mr-missing/app");
        let mut app = app_with(vec![clone_info(&clone, "main")]);
        app.start_mr_checkout(switch(&clone, AfterAction::Nothing));
        assert_eq!(app.git_busy, GitBusy::Checkout { iid: 5 });
        app.start_scan();
        assert!(app.pending_scan, "Scan-Wunsch gemerkt");
        let ctx = egui::Context::default();
        let until = Instant::now() + Duration::from_secs(10);
        while matches!(app.git_busy, GitBusy::Checkout { .. }) {
            assert!(Instant::now() < until, "Checkout-Worker hängt");
            std::thread::sleep(Duration::from_millis(5));
            app.poll_scan(&ctx);
        }
        assert_eq!(app.git_busy, GitBusy::Scan, "Scan nachgeholt");
        assert!(!app.pending_scan);
        assert!(app.error.is_some(), "Fehler bleibt bis zum Scan-Ergebnis");
        settle(&mut app);
    }

    /// Offenes Einstellungsfenster: die gemerkte „Danach öffnen“-Wahl steht
    /// auch im Entwurf, dessen „Speichern“ setzt sie nicht zurück.
    #[test]
    fn after_choice_reaches_open_settings_draft() {
        let mut app = app_with(Vec::new());
        app.show_settings = true;
        app.settings_state = Some(crate::ui::settings::SettingsState::from_config(&app.config));
        assert_ne!(
            app.config.merge_requests.after_checkout_last,
            AfterAction::Explorer
        );
        // Nur merken: während eines Scans startet kein Auschecken
        app.git_busy = GitBusy::Scan;
        app.start_mr_checkout(switch(Path::new("/tmp/gm-x/app"), AfterAction::Explorer));
        let draft = &app.settings_state.as_ref().expect("offen").draft;
        assert_eq!(
            draft.merge_requests.after_checkout_last,
            AfterAction::Explorer
        );
        assert_eq!(
            app.config.merge_requests.after_checkout_last,
            AfterAction::Explorer
        );
        // Geschlossenes Fenster: der alte Entwurf bleibt unberührt
        app.show_settings = false;
        app.start_mr_checkout(switch(Path::new("/tmp/gm-x/app"), AfterAction::Shell));
        let draft = &app.settings_state.as_ref().expect("Zustand").draft;
        assert_eq!(
            draft.merge_requests.after_checkout_last,
            AfterAction::Explorer
        );
    }

    /// MR im Menü, aber nicht mehr im Stand (Refresh nach dem Öffnen): eine
    /// Meldung statt eines stillen Abbruchs.
    #[test]
    fn checkout_of_vanished_mr_is_reported() {
        let mut app = app_with(Vec::new());
        app.mr_snapshot = Some(Box::default());
        let target = PathBuf::from("/tmp/gm-gone/app-mr-5");
        let base = PathBuf::from("/tmp/gm-gone/app");
        app.start_mr_checkout(worktree(&target, &base, AfterAction::Nothing));
        assert_eq!(
            app.status_message.as_deref(),
            Some("!5 is no longer in the current list – checkout skipped")
        );
        assert_eq!(app.git_busy, GitBusy::Idle);
    }

    /// Ein MR-Wechsel wartet im Dialog (Änderungen im Klon): ein zweiter
    /// Wechsel startet nicht, sonst ersetzte oder schlösse er den Dialog und
    /// der erste MR ginge still verloren.
    #[test]
    fn second_switch_waits_for_open_mr_switch_dialog() {
        let dir = tempdir().unwrap();
        for sub in ["a", "b"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        let clone = upstream_and_clone(&dir.path().join("a"));
        let other = upstream_and_clone(&dir.path().join("b"));
        let mut info = clone_info(&clone, "main");
        info.dirty = true;
        let mut app = app_with(vec![info, clone_info(&other, "main")]);
        app.start_mr_checkout(switch(&clone, AfterAction::Shell));
        settle(&mut app);
        assert!(app.mr_switch.is_some(), "wartet im Dialog");
        app.start_mr_checkout(switch(&other, AfterAction::Explorer));
        assert_eq!(
            app.status_message.as_deref(),
            Some("A branch switch is still waiting for your decision – finish that dialog first")
        );
        assert_eq!(app.git_busy, GitBusy::Idle, "kein zweiter Worker");
        assert_eq!(
            app.branch_dialog.as_ref().map(|d| &d.repo_path),
            Some(&clone)
        );
        assert_eq!(app.mr_switch.as_ref().map(|s| &s.clone), Some(&clone));
        assert_eq!(head_of(&other), "main");
    }

    /// Ohne IDE im wirksamen Profil sagen „Danach öffnen: IDE“ und „In
    /// {IDE} öffnen“ das, statt still nichts zu tun; die Beschriftung der
    /// MR-Ansicht folgt dem Profil-Override des Ordners.
    #[test]
    fn missing_ide_is_reported_and_label_follows_override() {
        let base = PathBuf::from("/tmp/gm-noide/app");
        let mut app = app_with(vec![clone_info(&base, "main")]);
        base_with_own_ide(&mut app, &base);
        assert_eq!(
            ide_name_for(&app.config, &app.repos, &base).as_deref(),
            Some("Test IDE")
        );
        let target = PathBuf::from("/tmp/gm-noide/other-mr-5");
        assert_eq!(ide_name_for(&app.config, &app.repos, &target), None);
        app.open_existing(&target, &OpenTarget::Ide);
        assert_eq!(
            app.status_message.as_deref(),
            Some("No IDE in this folder's profile")
        );
        let kind = MrCheckoutKind::NewClone {
            path: target.clone(),
            url: "https://gitlab.example.com/group/app.git".to_string(),
        };
        app.on_mr_checkout_ready(choice(kind, AfterAction::Ide), Ok(None));
        assert_eq!(
            app.status_message,
            Some(format!(
                "!5 cloned to {} – no IDE in the profile, nothing opened",
                target.display()
            ))
        );
        assert!(app.pending_after_checkout.is_none());
    }

    /// Ein MR-Wechsel, dessen Dialog ersetzt oder durch einen anderen
    /// Wechsel geschlossen wird, entfällt: ein späterer manueller Wechsel
    /// auf denselben Branch startet kein „Danach öffnen“.
    #[test]
    fn stale_mr_switch_is_dropped() {
        let dir = tempdir().unwrap();
        for sub in ["a", "b"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        let clone = upstream_and_clone(&dir.path().join("a"));
        let other = upstream_and_clone(&dir.path().join("b"));
        let mut info = clone_info(&clone, "main");
        info.dirty = true;
        let mut other_info = clone_info(&other, "main");
        other_info.dirty = true;
        let mut app = app_with(vec![info, other_info]);
        let ctx = egui::Context::default();
        app.start_mr_checkout(switch(&clone, AfterAction::Shell));
        settle(&mut app);
        assert!(app.mr_switch.is_some(), "wartet im Dialog");
        // Wechsel im anderen Klon mit Änderungen ersetzt den Dialog
        app.pending_branch_switches
            .push((other.clone(), "main".to_string()));
        app.poll_scan(&ctx);
        assert_eq!(
            app.branch_dialog.as_ref().map(|d| &d.repo_path),
            Some(&other)
        );
        assert!(app.mr_switch.is_none(), "Dialog ersetzt");
        app.close_branch_dialog();
        // Sauberer Wechsel im anderen Klon schließt den offenen MR-Dialog
        app.start_mr_checkout(switch(&clone, AfterAction::Shell));
        settle(&mut app);
        assert!(app.mr_switch.is_some());
        for repo in app.repos.iter_mut().filter(|r| r.path == other) {
            repo.dirty = false;
        }
        app.pending_branch_switches
            .push((other.clone(), "main".to_string()));
        app.poll_scan(&ctx);
        assert!(app.branch_dialog.is_none());
        assert!(app.mr_switch.is_none(), "Dialog geschlossen");
        settle(&mut app);
        // Späterer manueller Wechsel auf den MR-Branch: nur die Standardmeldung
        app.execute_branch_switch(clone.clone(), "feature/x".to_string(), false, false);
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(
            app.status_message,
            Some(format!(
                "Switched to branch “feature/x”: {}",
                clone.display()
            ))
        );
        settle(&mut app);
    }

    /// Fehler des Auscheckens erscheinen im Banner über der MR-Ansicht
    /// (Reihenfolge wie im zentralen Panel, E20).
    #[test]
    fn checkout_error_banner_in_mr_view() {
        let clone = PathBuf::from("/tmp/gm-mr/app");
        let mut app = app_with(vec![clone_info(&clone, "main")]);
        app.main_view = MainView::MergeRequests;
        let failure = MrCheckoutFailure::RepoOpen {
            path: clone.clone(),
        };
        app.on_mr_checkout_ready(switch(&clone, AfterAction::Nothing), Err(failure));
        let ctx = egui::Context::default();
        crate::ui::theme::apply_theme(&ctx, &app.config.theme);
        ctx.enable_accesskit();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1200.0, 800.0),
            )),
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            app.show_error_banner(ui);
            if app.main_view == MainView::MergeRequests {
                app.show_mr_view(ui);
            }
        });
        let nodes = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|u| u.nodes.clone())
            .unwrap_or_default();
        output.drop_without_applying_deltas();
        let text = |t: &str| {
            nodes
                .iter()
                .any(|(_, n)| n.value().or(n.label()) == Some(t))
        };
        let banner = format!(
            "Error: Checking out !5 failed: Cannot open repository {}",
            clone.display()
        );
        assert!(text(&banner), "Banner in der MR-Ansicht");
        assert!(text("My merge requests"), "MR-Ansicht darunter");
    }

    /// Menü veraltet (Branch inzwischen ausgecheckt): kein Worker, nur
    /// „Danach öffnen“ dort.
    #[test]
    fn stale_menu_already_checked_out_opens_existing() {
        let clone = PathBuf::from("/tmp/gm-mr/app");
        let mut app = app_with(vec![clone_info(&clone, "feature/x")]);
        let target = PathBuf::from("/tmp/gm-review/app-mr-5");
        app.start_mr_checkout(worktree(&target, &clone, AfterAction::Explorer));
        // Nur der Rescan, kein Worker
        assert_eq!(app.git_busy, GitBusy::Scan, "kein Worker");
        assert_eq!(
            app.status_message,
            Some(format!(
                "Already checked out in {} – opening Explorer",
                clone.display()
            ))
        );
        settle(&mut app);
    }

    /// Divergierter Branch: Hinweis in der MR-Ansicht, der den Rescan
    /// überlebt; beim Wechsel erst nach Erfolg.
    #[test]
    fn diverged_notice_survives_rescan() {
        let mut app = app_with(Vec::new());
        let target = PathBuf::from("/tmp/gm-review/app-mr-5");
        let clone = PathBuf::from("/tmp/gm-mr/app");
        let diverged = MrPrepOutcome::Diverged {
            ahead: 1,
            behind: 2,
        };
        app.on_mr_checkout_ready(
            worktree(&target, &clone, AfterAction::Nothing),
            prep(diverged.clone()),
        );
        let text =
            "Local branch feature/x differs from the MR (1 ahead, 2 behind) – local state checked out";
        assert_eq!(app.mr_notice.as_deref(), Some(text));
        settle(&mut app);
        assert_eq!(app.mr_notice.as_deref(), Some(text), "überlebt den Rescan");
        app.mr_notice = None;
        app.on_mr_checkout_ready(switch(&clone, AfterAction::Nothing), prep(diverged));
        assert!(app.mr_notice.is_none(), "erst nach dem Wechsel");
        app.mr_switch_succeeded(&clone, "feature/x");
        assert_eq!(app.mr_notice.as_deref(), Some(text));
    }

    /// „Bereits ausgecheckt“: Repo-Liste zeigt die Karte (bei einem Worktree
    /// außerhalb der Suchpfade die des Basis-Klons), die übrigen Ziele
    /// nutzen die Methoden der Repo-Karte.
    #[test]
    fn open_existing_targets() {
        let base = PathBuf::from("/tmp/gm-mr/app");
        let outside = PathBuf::from("/tmp/gm-review/app-mr-5");
        let mut info = clone_info(&base, "main");
        info.worktrees = vec![WorktreeHead {
            path: outside.clone(),
            branch: Some("feature/x".to_string()),
        }];
        let mut app = app_with(vec![info]);
        app.main_view = MainView::MergeRequests;
        app.open_existing(&outside, &OpenTarget::ShowInRepoList);
        assert_eq!(app.main_view, MainView::Repos);
        let ctx = egui::Context::default();
        let first = app.repo_highlight_frame(&ctx).expect("Hervorhebung");
        assert_eq!(
            first,
            RepoHighlight {
                path: base.clone(),
                scroll: true
            }
        );
        let second = app.repo_highlight_frame(&ctx).expect("noch hervorgehoben");
        assert!(!second.scroll, "nur einmal scrollen");
        if let Some((_, since)) = app.repo_highlight.as_mut() {
            *since = Instant::now().checked_sub(Duration::from_secs(3)).unwrap();
        }
        assert!(app.repo_highlight_frame(&ctx).is_none());
        assert!(app.repo_highlight.is_none());
        app.open_existing(&outside, &OpenTarget::Explorer);
        assert_eq!(
            app.status_message,
            Some(format!("Explorer opened: {}", outside.display()))
        );
        app.open_existing(&outside, &OpenTarget::Shell);
        assert_eq!(
            app.status_message,
            Some(format!("Shell opened: {}", outside.display()))
        );
        app.open_existing(&outside, &OpenTarget::Agent("claude".to_string()));
        assert_eq!(
            app.status_message,
            Some(format!("Starting Claude Code in {}...", outside.display()))
        );
        // Ordner ohne Karte: nur die Ansicht wechselt
        app.main_view = MainView::MergeRequests;
        app.open_existing(Path::new("/tmp/elsewhere"), &OpenTarget::ShowInRepoList);
        assert_eq!(app.main_view, MainView::Repos);
        assert!(app.repo_highlight.is_none());
    }

    /// Hinweis über der MR-Ansicht mit AccessKit-Namen; das Kreuz schließt ihn.
    #[test]
    fn mr_notice_banner_closes() {
        use egui::accesskit::Role;
        let mut app = app_with(Vec::new());
        app.mr_notice = Some("Local branch feature/x differs".to_string());
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let frame = |app: &mut MyApp, events: Vec<egui::Event>| {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 600.0),
                )),
                events,
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| app.show_mr_notice(ui));
            let nodes = output
                .platform_output
                .accesskit_update
                .as_ref()
                .map(|u| u.nodes.clone())
                .unwrap_or_default();
            output.drop_without_applying_deltas();
            nodes
        };
        let nodes = frame(&mut app, Vec::new());
        assert!(
            nodes
                .iter()
                .any(|(_, n)| n.value().or(n.label()) == Some("Local branch feature/x differs")),
            "Hinweis sichtbar"
        );
        let bounds = nodes
            .iter()
            .find(|(_, n)| n.role() == Role::Button && n.label() == Some("Close"))
            .and_then(|(_, n)| n.bounds())
            .expect("Schließen-Button mit Namen");
        let pos = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        );
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        frame(&mut app, vec![button(true)]);
        assert!(app.mr_notice.is_some());
        frame(&mut app, vec![button(false)]);
        assert!(app.mr_notice.is_none(), "Kreuz schließt den Hinweis");
    }
}
