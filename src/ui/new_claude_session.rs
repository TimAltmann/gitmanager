//! Inhalt des Dialogs „Neue Claude Session“: Suche, Ordnerliste,
//! Starten/Abbrechen. Das Fenster drumherum liegt in `app::claude_sessions`.

use crate::claude_new::{target_matches_search, NewSessionTarget};
use crate::i18n::{tr, tr_fmt, Language};
use egui::{Color32, Key, RichText, Ui};
use std::path::PathBuf;

/// Zustand des offenen Dialogs.
pub struct NewClaudeDialog {
    pub targets: Vec<NewSessionTarget>,
    /// Index in `targets`
    pub selected: Option<usize>,
    pub search: String,
    /// Suchfeld im ersten Frame fokussieren
    pub focus_search: bool,
}

impl NewClaudeDialog {
    pub fn new(targets: Vec<NewSessionTarget>, selected: Option<usize>) -> Self {
        Self {
            targets,
            selected,
            search: String::new(),
            focus_search: true,
        }
    }

    /// Indizes der Einträge, die zur Suche passen.
    pub fn visible(&self) -> Vec<usize> {
        (0..self.targets.len())
            .filter(|&i| target_matches_search(&self.targets[i], &self.search))
            .collect()
    }

    /// Auswahl, falls sichtbar; sonst der erste sichtbare Eintrag.
    pub fn effective_selection(&self) -> Option<usize> {
        let visible = self.visible();
        self.selected
            .filter(|s| visible.contains(s))
            .or_else(|| visible.first().copied())
    }

    /// Auswahl um `delta` Zeilen in der sichtbaren Liste verschieben.
    fn move_selection(&mut self, delta: isize) {
        let visible = self.visible();
        if visible.is_empty() {
            return;
        }
        let pos = self
            .effective_selection()
            .and_then(|s| visible.iter().position(|&v| v == s))
            .unwrap_or(0) as isize;
        let next = (pos + delta).clamp(0, visible.len() as isize - 1) as usize;
        self.selected = Some(visible[next]);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DialogOutcome {
    None,
    Start(PathBuf),
    Cancel,
}

/// Höhe der Ordnerliste
const LIST_HEIGHT: f32 = 340.0;

/// Zeichnet den Dialoginhalt; Enter/Doppelklick/„Starten“ starten, Esc bricht ab.
pub fn show_new_claude_contents(
    ui: &mut Ui,
    dialog: &mut NewClaudeDialog,
    lang: Language,
) -> DialogOutcome {
    let mut outcome = DialogOutcome::None;
    let (up, down, enter, esc) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::Enter),
            i.key_pressed(Key::Escape),
        )
    });
    if up {
        dialog.move_selection(-1);
    }
    if down {
        dialog.move_selection(1);
    }

    ui.label(RichText::new(tr(lang, "claude_new_hint")).size(11.5).weak());
    ui.add_space(6.0);
    let search = ui.add(
        egui::TextEdit::singleline(&mut dialog.search)
            .hint_text(tr(lang, "claude_new_search_hint"))
            .desired_width(f32::INFINITY),
    );
    if dialog.focus_search {
        search.request_focus();
        dialog.focus_search = false;
    }
    ui.add_space(6.0);

    let visible = dialog.visible();
    let selected = dialog.effective_selection();
    egui::ScrollArea::vertical()
        .max_height(LIST_HEIGHT)
        .min_scrolled_height(LIST_HEIGHT)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if visible.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new(tr(lang, "claude_new_no_matches")).weak());
                return;
            }
            for &i in &visible {
                let is_sel = selected == Some(i);
                let resp = target_row(ui, &dialog.targets[i], is_sel, lang);
                if is_sel && (up || down) {
                    resp.scroll_to_me(None);
                }
                if resp.clicked() {
                    dialog.selected = Some(i);
                }
                if resp.double_clicked() {
                    outcome = DialogOutcome::Start(dialog.targets[i].path.clone());
                }
            }
        });

    ui.add_space(10.0);
    let chosen = dialog.effective_selection();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                chosen.is_some(),
                egui::Button::new(tr(lang, "claude_new_start")),
            )
            .clicked()
            || (enter && chosen.is_some())
        {
            if let Some(i) = chosen {
                outcome = DialogOutcome::Start(dialog.targets[i].path.clone());
            }
        }
        if ui.button(tr(lang, "cancel")).clicked() || esc {
            outcome = DialogOutcome::Cancel;
        }
    });
    outcome
}

/// Eine Zeile: Name, Branch bzw. „Worktree von …“, Pfad.
fn target_row(ui: &mut Ui, t: &NewSessionTarget, selected: bool, lang: Language) -> egui::Response {
    let visuals = ui.visuals().clone();
    let fill = if selected {
        visuals.selection.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let inner = egui::Frame::new()
        .fill(fill)
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.style_mut().interaction.selectable_labels = false;
            ui.horizontal(|ui| {
                ui.label(RichText::new(&t.name).strong());
                match (&t.branch, &t.main_name) {
                    (_, Some(main)) if t.is_worktree => {
                        ui.label(
                            RichText::new(tr_fmt(lang, "wt_of_repo", &[main]))
                                .size(11.0)
                                .weak(),
                        );
                    }
                    (None, _) => {
                        ui.label(
                            RichText::new(tr(lang, "claude_new_default_badge"))
                                .size(11.0)
                                .weak(),
                        );
                    }
                    _ => {}
                }
                if let Some(branch) = &t.branch {
                    ui.label(
                        RichText::new(format!("⎇ {branch}"))
                            .size(11.0)
                            .color(visuals.hyperlink_color),
                    );
                }
            });
            ui.label(
                RichText::new(t.path.display().to_string())
                    .size(10.5)
                    .weak(),
            );
        });
    ui.interact(
        inner.response.rect,
        ui.id().with(("claude_new_row", &t.path)),
        egui::Sense::click(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str) -> NewSessionTarget {
        NewSessionTarget {
            path: PathBuf::from(format!("/dev/{name}")),
            name: name.to_string(),
            branch: Some("main".to_string()),
            is_worktree: false,
            main_name: None,
        }
    }

    fn dialog() -> NewClaudeDialog {
        NewClaudeDialog::new(
            vec![target("alpha"), target("beta"), target("gamma")],
            Some(1),
        )
    }

    fn run(d: &mut NewClaudeDialog, events: Vec<egui::Event>) -> DialogOutcome {
        let ctx = egui::Context::default();
        let mut outcome = DialogOutcome::None;
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            outcome = show_new_claude_contents(ui, d, Language::De);
        });
        output.drop_without_applying_deltas();
        outcome
    }

    fn key(k: Key) -> egui::Event {
        egui::Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn selection_falls_back_to_first_visible_when_filtered_out() {
        let mut d = dialog();
        assert_eq!(d.effective_selection(), Some(1));
        d.search = "gam".into();
        assert_eq!(d.visible(), vec![2]);
        assert_eq!(d.effective_selection(), Some(2));
        d.search = "nothing".into();
        assert_eq!(d.effective_selection(), None);
    }

    #[test]
    fn enter_starts_selected_and_escape_cancels() {
        let mut d = dialog();
        assert_eq!(run(&mut d, vec![]), DialogOutcome::None);
        assert_eq!(
            run(&mut d, vec![key(Key::Enter)]),
            DialogOutcome::Start(PathBuf::from("/dev/beta"))
        );
        assert_eq!(run(&mut d, vec![key(Key::Escape)]), DialogOutcome::Cancel);
    }

    #[test]
    fn arrow_keys_move_within_visible_entries() {
        let mut d = dialog();
        run(&mut d, vec![key(Key::ArrowDown)]);
        assert_eq!(d.selected, Some(2));
        run(&mut d, vec![key(Key::ArrowDown)]);
        assert_eq!(d.selected, Some(2), "bleibt am Ende stehen");
        run(&mut d, vec![key(Key::ArrowUp)]);
        run(&mut d, vec![key(Key::ArrowUp)]);
        run(&mut d, vec![key(Key::ArrowUp)]);
        assert_eq!(d.selected, Some(0));
    }

    #[test]
    fn enter_without_matches_does_nothing() {
        let mut d = dialog();
        d.search = "zzz".into();
        assert_eq!(run(&mut d, vec![key(Key::Enter)]), DialogOutcome::None);
    }
}
