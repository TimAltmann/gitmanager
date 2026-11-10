//! Suchfeld der Listen (Repo-Liste, Claude Sessions): Lupe, Eingabe,
//! Strg+F zum Fokussieren, Esc und ✕ zum Leeren.

use crate::i18n::{tr, Language};
use egui::{Ui, Vec2};

const ICON_SEARCH: egui::ImageSource = egui::include_image!("../../assets/icons/search.svg");
const ICON_CROSS: egui::ImageSource = egui::include_image!("../../assets/icons/cross.svg");

/// Zeichnet das Suchfeld in die aktuelle (horizontale) Zeile.
pub fn search_field(ui: &mut Ui, id: &str, hint: &str, search: &mut String, lang: Language) {
    ui.add(
        egui::Image::new(ICON_SEARCH)
            .fit_to_exact_size(Vec2::splat(14.0))
            .tint(ui.visuals().weak_text_color()),
    );
    let edit = ui.add(
        egui::TextEdit::singleline(search)
            .id(egui::Id::new(id))
            .hint_text(hint)
            .desired_width(340.0),
    );
    let focus_shortcut = ui.ctx().input_mut(|i| {
        i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::F,
        ))
    });
    if focus_shortcut {
        edit.request_focus();
    }
    // Das TextEdit gibt den Fokus bei Esc im selben Frame ab: dann zählt
    // `lost_focus`, sonst (falls es ihn behält) `has_focus`
    if (edit.has_focus() || edit.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        search.clear();
    }
    if !search.is_empty()
        && ui
            .add(
                egui::Button::image(
                    egui::Image::new(ICON_CROSS)
                        .fit_to_exact_size(Vec2::splat(12.0))
                        .tint(ui.visuals().text_color()),
                )
                .small(),
            )
            .on_hover_text(tr(lang, "repo_search_clear"))
            .clicked()
    {
        search.clear();
    }
}
