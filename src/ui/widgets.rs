//! Kleine, gemeinsame UI-Bausteine über mehrere Ansichten hinweg.

use egui::{Color32, Response, RichText, Ui};

/// 11 px Label, nicht umbrechend.
pub fn small_text(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.label(RichText::new(text).size(11.0).color(color))
}
