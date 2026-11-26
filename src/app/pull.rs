//! Pull per Klick auf den „↓n“-Chip der Repo-Karte: Fast-Forward im
//! Hintergrund (nie Reset), Status-Meldung, danach Rescan.

use super::{fetch_error_i18n_key, fetch_error_parts, GitBusy, MyApp};
use crate::git::{self, FfOutcome};
use crate::i18n::{tr, tr_fmt, Language};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

/// Kanal für das Ergebnis des Pull-Workers.
pub(super) struct PullUi {
    tx: Sender<PullDone>,
    rx: Receiver<PullDone>,
}

impl Default for PullUi {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx }
    }
}

struct PullDone {
    repo: PathBuf,
    result: anyhow::Result<FfOutcome>,
}

/// Text zum Ergebnis eines Pulls. Fetch-Fehler laufen durch dieselbe
/// Abbildung wie beim Fetch (inkl. Auth-Hinweis), andere Fehler bekommen
/// den Pull-Text.
pub(super) fn pull_message(lang: Language, repo: &str, r: &anyhow::Result<FfOutcome>) -> String {
    match r {
        Ok(FfOutcome::UpToDate) => tr_fmt(lang, "pull_up_to_date", &[repo]),
        Ok(FfOutcome::FastForwarded { commits: 1 }) => tr_fmt(lang, "pull_ff_one", &[repo]),
        Ok(FfOutcome::FastForwarded { commits }) => {
            tr_fmt(lang, "pull_ff", &[repo, &commits.to_string()])
        }
        Ok(FfOutcome::Dirty { files }) => tr_fmt(lang, "pull_dirty", &[repo, &files.to_string()]),
        Ok(FfOutcome::Diverged { ahead, behind }) => tr_fmt(
            lang,
            "pull_diverged",
            &[repo, &ahead.to_string(), &behind.to_string()],
        ),
        Ok(FfOutcome::NoUpstream) => tr_fmt(lang, "pull_no_upstream", &[repo]),
        Ok(FfOutcome::Detached) => tr_fmt(lang, "pull_detached", &[repo]),
        Ok(FfOutcome::Changed) => tr_fmt(lang, "pull_changed", &[repo]),
        Err(e) => {
            let (raw, partial, is_fetch) = fetch_error_parts(e);
            // Auth-Hinweis nur bei echten Remote-Fehlern
            let detail = if is_fetch {
                format!("{raw}. {}", tr(lang, "auth_hint"))
            } else {
                raw
            };
            // Nur echte Fetch-Fehler heißen „Fetch fehlgeschlagen“
            let key = if is_fetch {
                fetch_error_i18n_key(partial)
            } else {
                "pull_failed"
            };
            let text = tr_fmt(lang, key, &[&detail]);
            format!("{repo}: {text}")
        }
    }
}

/// Fehler und Verweigerungen (nichts geändert) bleiben als Fehlerbanner
/// stehen; nur Erfolg und „schon aktuell“ sind flüchtige Statusmeldungen.
fn pull_is_error(r: &anyhow::Result<FfOutcome>) -> bool {
    !matches!(
        r,
        Ok(FfOutcome::UpToDate) | Ok(FfOutcome::FastForwarded { .. })
    )
}

impl MyApp {
    /// Fast-Forward des Repos im Hintergrund (nie Reset); Ergebnis kommt über
    /// `poll_pull`.
    pub(super) fn start_pull(&mut self, path: PathBuf) {
        let lang = self.config.language;
        // Wie beim manuellen Fetch: keine zwei libgit2-Läufe auf denselben Refs
        if self.auto_fetching {
            self.set_status(tr(lang, "auto_fetch_running"));
            return;
        }
        if self.git_busy.holds_refs() {
            self.set_status(tr(lang, "fetch_busy"));
            return;
        }
        self.set_git_busy(GitBusy::Pull { repo: path.clone() });
        let tx = self.pull_ui.tx.clone();
        std::thread::spawn(move || {
            // Panic vor dem Senden darf `git_busy` nicht hängen lassen
            let result = catch_unwind(AssertUnwindSafe(|| git::fast_forward(&path)))
                .unwrap_or_else(|_| {
                    eprintln!("Pull-Thread abgestürzt");
                    Err(anyhow::anyhow!("pull aborted"))
                });
            let _ = tx.send(PullDone { repo: path, result });
        });
    }

    /// Ergebnis des Workers: Meldung zeigen, Busy lösen, neu scannen.
    pub(super) fn poll_pull(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.pull_ui.rx.try_recv() {
            let lang = self.config.language;
            if matches!(&self.git_busy, GitBusy::Pull { repo } if *repo == done.repo) {
                self.set_git_busy(GitBusy::Idle);
            }
            let name = done
                .repo
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| done.repo.display().to_string());
            if let Err(e) = &done.result {
                eprintln!("Pull fehlgeschlagen für {}: {e:#}", done.repo.display());
            }
            let text = pull_message(lang, &name, &done.result);
            self.start_scan();
            // Agent aus der MR-Ansicht wartete auf diesen Pull
            let then = self.start_agent_after_pull(&done.repo);
            if pull_is_error(&done.result) {
                // Banner bleibt über den Rescan hinweg stehen
                self.error = Some(text.clone());
                self.error_after_scan = Some(text);
            } else {
                self.set_status(match then {
                    Some(then) => format!("{text} {then}"),
                    None => text,
                });
            }
            ctx.request_repaint();
        }
        if matches!(self.git_busy, GitBusy::Pull { .. }) {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::test_app;

    fn ok(o: FfOutcome) -> anyhow::Result<FfOutcome> {
        Ok(o)
    }

    #[test]
    fn pull_message_texts() {
        let de = Language::De;
        let en = Language::En;
        let m = |l, r| pull_message(l, "shop-backend", &r);
        assert_eq!(
            m(de, ok(FfOutcome::FastForwarded { commits: 3 })),
            "shop-backend: 3 Commits übernommen"
        );
        assert_eq!(
            m(de, ok(FfOutcome::FastForwarded { commits: 1 })),
            "shop-backend: 1 Commit übernommen"
        );
        assert_eq!(
            m(en, ok(FfOutcome::FastForwarded { commits: 3 })),
            "shop-backend: pulled 3 commits"
        );
        assert_eq!(
            m(en, ok(FfOutcome::FastForwarded { commits: 1 })),
            "shop-backend: pulled 1 commit"
        );
        assert_eq!(
            m(de, ok(FfOutcome::Dirty { files: 2 })),
            "shop-backend: 2 lokale Änderungen – erst committen oder stashen"
        );
        assert_eq!(
            m(en, ok(FfOutcome::Dirty { files: 2 })),
            "shop-backend: 2 local changes – commit or stash first"
        );
        assert_eq!(
            m(
                de,
                ok(FfOutcome::Diverged {
                    ahead: 1,
                    behind: 2
                })
            ),
            "shop-backend: Branch ist divergiert (↑1 ↓2) – bitte manuell lösen"
        );
        assert_eq!(
            m(
                en,
                ok(FfOutcome::Diverged {
                    ahead: 1,
                    behind: 2
                })
            ),
            "shop-backend: branch has diverged (↑1 ↓2) – please resolve manually"
        );
        assert_eq!(
            m(de, ok(FfOutcome::UpToDate)),
            "shop-backend: schon aktuell"
        );
        assert_eq!(
            m(en, ok(FfOutcome::UpToDate)),
            "shop-backend: already up to date"
        );
        assert_eq!(
            m(en, ok(FfOutcome::NoUpstream)),
            "shop-backend: no upstream branch configured"
        );
        assert_eq!(
            m(de, ok(FfOutcome::NoUpstream)),
            "shop-backend: kein Upstream-Branch eingerichtet"
        );
        assert_eq!(
            m(en, ok(FfOutcome::Detached)),
            "shop-backend: detached HEAD – nothing to pull"
        );
        assert_eq!(
            m(de, ok(FfOutcome::Detached)),
            "shop-backend: losgelöster HEAD – nichts zu pullen"
        );
        // Fehler: gleiche Texte wie beim Fetch (inkl. Auth-Hinweis bei Remote-Fehlern)
        let err = anyhow::Error::new(git::PartialFetch {
            succeeded: Vec::new(),
            failed: vec![("origin".to_string(), "auth failed".to_string())],
        });
        let text = m(en, Err(err));
        assert!(text.starts_with("shop-backend: Fetch failed: "), "{text}");
        assert!(text.contains("'origin': auth failed"), "{text}");
        assert!(text.contains("auth may be required"), "{text}");
        // Kein Fetch-Fehler (Fast-Forward, Absturz): eigener Pull-Text
        let plain = m(de, Err(anyhow::anyhow!("kaputt")));
        assert_eq!(plain, "shop-backend: Pull fehlgeschlagen: kaputt");
        let plain = m(en, Err(anyhow::anyhow!("kaputt")));
        assert_eq!(plain, "shop-backend: Pull failed: kaputt");
        assert_eq!(
            m(de, ok(FfOutcome::Changed)),
            "shop-backend: Branch hat sich während des Abrufs geändert – nichts geändert"
        );
        assert_eq!(
            m(en, ok(FfOutcome::Changed)),
            "shop-backend: branch changed while fetching – nothing changed"
        );
    }

    /// Ergebnis über den Kanal liefern und abholen.
    fn deliver(r: anyhow::Result<FfOutcome>) -> MyApp {
        let mut app = test_app();
        let repo = PathBuf::from("/x/shop-backend");
        app.set_git_busy(GitBusy::Pull { repo: repo.clone() });
        app.pull_ui.tx.send(PullDone { repo, result: r }).unwrap();
        app.poll_pull(&egui::Context::default());
        app
    }

    fn wait_scan(app: &mut MyApp) {
        let ctx = egui::Context::default();
        for _ in 0..400 {
            app.poll_scan(&ctx);
            if app.git_busy != GitBusy::Scan {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("Scan nicht fertig");
    }

    #[test]
    fn pull_refusals_and_errors_go_to_the_error_banner() {
        let refusals = [
            ok(FfOutcome::Dirty { files: 1 }),
            ok(FfOutcome::Diverged {
                ahead: 1,
                behind: 1,
            }),
            ok(FfOutcome::NoUpstream),
            ok(FfOutcome::Detached),
            ok(FfOutcome::Changed),
            Err(anyhow::anyhow!("kaputt")),
        ];
        for r in refusals {
            let lang = Language::En;
            let text = pull_message(lang, "shop-backend", &r);
            let mut app = deliver(r);
            assert_eq!(app.error.as_deref(), Some(text.as_str()));
            assert_eq!(app.status_message, None, "{text}");
            // Der anschließende Rescan löscht den Fehler nicht
            assert_eq!(app.git_busy, GitBusy::Scan);
            wait_scan(&mut app);
            assert_eq!(app.error.as_deref(), Some(text.as_str()), "nach Scan");
            // Ein späterer Scan schon (Fehler gilt für diesen Pull)
            app.start_scan();
            wait_scan(&mut app);
            assert_eq!(app.error, None, "{text}");
        }
        for r in [
            ok(FfOutcome::UpToDate),
            ok(FfOutcome::FastForwarded { commits: 2 }),
        ] {
            let text = pull_message(Language::En, "shop-backend", &r);
            let app = deliver(r);
            assert_eq!(app.status_message.as_deref(), Some(text.as_str()));
            assert_eq!(app.error, None, "{text}");
        }
    }

    #[test]
    fn start_pull_rejected_while_busy() {
        let path = PathBuf::from("/x/shop-backend");
        let lang = Language::En;

        let mut app = test_app();
        app.set_git_busy(GitBusy::Scan);
        app.start_pull(path.clone());
        assert_eq!(app.status_message, Some(tr(lang, "fetch_busy")));
        assert_eq!(app.git_busy, GitBusy::Scan, "kein Pull gestartet");

        let mut app = test_app();
        app.auto_fetching = true;
        app.start_pull(path.clone());
        assert_eq!(app.status_message, Some(tr(lang, "auto_fetch_running")));
        assert_eq!(app.git_busy, GitBusy::Idle, "kein Pull gestartet");
    }

    #[test]
    fn start_pull_marks_repo_busy() {
        let mut app = test_app();
        // Pfad existiert nicht: Worker meldet Fehler, der Zustand wird trotzdem gesetzt
        app.start_pull(PathBuf::from("/nonexistent/pull-test"));
        assert_eq!(
            app.git_busy,
            GitBusy::Pull {
                repo: PathBuf::from("/nonexistent/pull-test")
            }
        );
    }
}
