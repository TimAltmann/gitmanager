//! Agent (z. B. Claude Code) aus der MR-Ansicht starten: fehlen im Klon
//! Commits des MR (Branch hinter dem Remote oder der Kopf-Commit des MR ist
//! lokal nicht enthalten), fragt ein Dialog, ob vorher gepullt wird. Der
//! Pull läuft wie per „↓n“-Chip (`pull.rs`, nur Fast-Forward); der Agent
//! startet, sobald er fertig ist – auch wenn er nichts übernehmen konnte
//! (der Grund steht dann im Fehlerbanner).

use super::{GitBusy, MyApp};
use crate::git;
use crate::i18n::{tr, tr_fmt};
use egui::RichText;
use std::path::{Path, PathBuf};

/// Wartende Frage „Vorher pullen?“.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MrPullPrompt {
    pub path: PathBuf,
    pub agent_id: String,
    /// Commits hinter dem Remote laut letztem Scan bzw. Fetch (0 = unbekannt
    /// oder nur der MR-Kopf fehlt)
    pub behind: usize,
}

/// Ergebnis von [`MyApp::start_mr_agent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MrAgentStart {
    Started,
    NotStarted,
    /// Dialog „Vorher pullen?“ offen; gestartet wird nach der Antwort
    Asking,
}

/// Fehlen im Klon `path` Commits des MR? `behind` aus Scan/Fetch,
/// `mr_head` = Kopf-Commit des MR laut GitLab.
pub(super) fn needs_pull(path: &Path, behind: usize, mr_head: Option<&str>) -> bool {
    behind > 0 || mr_head.is_some_and(|sha| git::head_contains(path, sha) == Some(false))
}

/// Antwort im Dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    PullAndStart,
    StartWithoutPull,
    Cancel,
}

impl MyApp {
    /// „Commits hinter dem Remote“ des Klons bzw. Worktrees laut Scan.
    pub(super) fn behind_of(&self, path: &Path) -> usize {
        self.repos
            .iter()
            .find(|r| super::mr_checkout::same_dir(&r.path, path))
            .map_or(0, |r| r.behind)
    }

    /// Agent im ausgecheckten MR starten; fehlen Commits, erst fragen.
    pub(super) fn start_mr_agent(
        &mut self,
        path: &Path,
        agent_id: &str,
        behind: usize,
        mr_head: Option<&str>,
    ) -> MrAgentStart {
        // Eine offene Frage nicht durch eine zweite ersetzen
        if self.mr_pull_prompt.is_none() && needs_pull(path, behind, mr_head) {
            self.mr_pull_prompt = Some(MrPullPrompt {
                path: path.to_path_buf(),
                agent_id: agent_id.to_string(),
                behind,
            });
            return MrAgentStart::Asking;
        }
        if self.open_agent(path, agent_id) {
            MrAgentStart::Started
        } else {
            MrAgentStart::NotStarted
        }
    }

    fn answer_mr_pull_prompt(&mut self, answer: Answer) {
        let Some(prompt) = self.mr_pull_prompt.take() else {
            return;
        };
        match answer {
            Answer::Cancel => {}
            Answer::StartWithoutPull => {
                self.open_agent(&prompt.path, &prompt.agent_id);
            }
            Answer::PullAndStart => {
                self.start_pull(prompt.path.clone());
                let pulling =
                    matches!(&self.git_busy, GitBusy::Pull { repo } if *repo == prompt.path);
                if pulling {
                    self.agent_after_pull = Some((prompt.path, prompt.agent_id));
                } else {
                    // Pull gerade nicht möglich (Fetch/Scan läuft, Meldung in
                    // der Statusleiste): die Frage bleibt offen, statt den
                    // Agenten stillschweigend ohne Pull zu starten
                    self.mr_pull_prompt = Some(prompt);
                }
            }
        }
    }

    /// Pull für `repo` fertig: wartenden Agent starten. Liefert den Zusatz
    /// für die Meldung („– Claude Code startet“).
    pub(super) fn start_agent_after_pull(&mut self, repo: &Path) -> Option<String> {
        let (path, agent_id) = self.agent_after_pull.take_if(|(p, _)| p == repo)?;
        let name = self.agent_by_id(&agent_id).map(|a| a.display_name.clone());
        let started = self.open_agent(&path, &agent_id);
        let lang = self.config.language;
        name.filter(|_| started)
            .map(|name| tr_fmt(lang, "mr_then_agent", &[&name]))
    }

    /// Dialog „Neue Commits im MR – vorher pullen?“.
    pub(super) fn show_mr_pull_prompt(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &self.mr_pull_prompt else {
            return;
        };
        let lang = self.config.language;
        let agent = self
            .agent_by_id(&prompt.agent_id)
            .map(|a| a.display_name.clone())
            .unwrap_or_else(|| prompt.agent_id.clone());
        let place = prompt.path.display().to_string();
        let text = if prompt.behind > 0 {
            tr_fmt(
                lang,
                "mr_pull_ask_behind",
                &[&place, &prompt.behind.to_string()],
            )
        } else {
            tr_fmt(lang, "mr_pull_ask", &[&place])
        };
        let mut open = true;
        let mut answer = None;
        egui::Window::new(tr(lang, "mr_pull_ask_title"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.label(RichText::new(text).size(12.5));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let pull = ui.button(tr_fmt(lang, "mr_pull_and_start", &[&agent]));
                    if pull.clicked() {
                        answer = Some(Answer::PullAndStart);
                    }
                    if ui.button(tr(lang, "mr_start_without_pull")).clicked() {
                        answer = Some(Answer::StartWithoutPull);
                    }
                    if ui.button(tr(lang, "cancel")).clicked() {
                        answer = Some(Answer::Cancel);
                    }
                });
                if answer.is_none() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    answer = Some(Answer::PullAndStart);
                }
                if answer.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    answer = Some(Answer::Cancel);
                }
            });
        if !open {
            answer = Some(Answer::Cancel);
        }
        if let Some(answer) = answer {
            self.answer_mr_pull_prompt(answer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Repo mit zwei Commits; liefert (Ordner, erster, zweiter Commit).
    fn repo_two_commits() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q"]);
        git(p, &["config", "user.email", "t@example.com"]);
        git(p, &["config", "user.name", "T"]);
        git(p, &["commit", "-q", "--allow-empty", "-m", "a"]);
        let first = git(p, &["rev-parse", "HEAD"]);
        git(p, &["commit", "-q", "--allow-empty", "-m", "b"]);
        let second = git(p, &["rev-parse", "HEAD"]);
        (dir, first, second)
    }

    #[test]
    fn needs_pull_from_behind_or_missing_mr_head() {
        let (dir, first, second) = repo_two_commits();
        let p = dir.path();
        assert!(!needs_pull(p, 0, None));
        assert!(needs_pull(p, 2, None));
        // MR-Kopf ist HEAD bzw. Vorfahre: nichts zu holen
        assert!(!needs_pull(p, 0, Some(&second)));
        assert!(!needs_pull(p, 0, Some(&first)));
        // MR-Kopf lokal unbekannt: es gibt Neues
        let unknown = "0123456789abcdef0123456789abcdef01234567";
        assert!(needs_pull(p, 0, Some(unknown)));
        // Lokal vorhanden, aber nicht in HEAD (z. B. nur gefetcht)
        git(p, &["reset", "-q", "--hard", &first]);
        assert!(needs_pull(p, 0, Some(&second)));
        // Unlesbar: lieber nicht fragen
        assert!(!needs_pull(p, 0, Some("kein-sha")));
    }

    #[test]
    fn agent_start_asks_only_when_commits_are_missing() {
        let (dir, _, second) = repo_two_commits();
        let mut app = test_app();
        let start = app.start_mr_agent(dir.path(), "claude", 0, Some(&second));
        assert_ne!(start, MrAgentStart::Asking);
        assert!(app.mr_pull_prompt.is_none());

        let start = app.start_mr_agent(dir.path(), "claude", 3, Some(&second));
        assert_eq!(start, MrAgentStart::Asking);
        let prompt = app.mr_pull_prompt.clone().unwrap();
        assert_eq!(prompt.path, dir.path());
        assert_eq!(prompt.agent_id, "claude");
        assert_eq!(prompt.behind, 3);
    }

    #[test]
    fn cancel_and_start_without_pull_clear_the_prompt() {
        let (dir, ..) = repo_two_commits();
        let mut app = test_app();
        app.start_mr_agent(dir.path(), "claude", 1, None);
        app.answer_mr_pull_prompt(Answer::Cancel);
        assert!(app.mr_pull_prompt.is_none());
        assert!(app.agent_after_pull.is_none());

        app.start_mr_agent(dir.path(), "claude", 1, None);
        app.answer_mr_pull_prompt(Answer::StartWithoutPull);
        assert!(app.mr_pull_prompt.is_none());
        assert!(app.agent_after_pull.is_none());
    }

    #[test]
    fn pull_and_start_waits_for_pull_then_starts_once() {
        let (dir, ..) = repo_two_commits();
        let path = dir.path().to_path_buf();
        let mut app = test_app();
        app.start_mr_agent(&path, "claude", 1, None);
        app.answer_mr_pull_prompt(Answer::PullAndStart);
        assert!(app.mr_pull_prompt.is_none());
        assert_eq!(
            app.agent_after_pull,
            Some((path.clone(), "claude".to_string()))
        );
        // Anderer Pull: wartender Agent bleibt
        assert_eq!(app.start_agent_after_pull(Path::new("/elsewhere")), None);
        assert!(app.agent_after_pull.is_some());
        app.start_agent_after_pull(&path);
        assert!(app.agent_after_pull.is_none());
    }

    /// Pull gerade nicht möglich: die Frage bleibt offen (mit Meldung),
    /// der Agent startet nicht stillschweigend ohne Pull.
    #[test]
    fn pull_and_start_while_busy_keeps_asking() {
        let (dir, ..) = repo_two_commits();
        let mut app = test_app();
        app.auto_fetching = true;
        app.start_mr_agent(dir.path(), "claude", 1, None);
        app.answer_mr_pull_prompt(Answer::PullAndStart);
        assert!(app.mr_pull_prompt.is_some());
        assert!(app.agent_after_pull.is_none());
        assert_eq!(
            app.status_message,
            Some(tr(app.config.language, "auto_fetch_running"))
        );
        // „Ohne Pull starten“ bleibt möglich
        app.answer_mr_pull_prompt(Answer::StartWithoutPull);
        assert!(app.mr_pull_prompt.is_none());
    }
}
