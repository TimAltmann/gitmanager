//! Kleine, ansichtsübergreifende Bausteine.

use egui::{
    vec2, Button, Color32, Image, ImageSource, Response, RichText, Ui, WidgetInfo, WidgetType,
};

/// Quadratisches, eingefärbtes Icon.
pub fn icon_image(icon: ImageSource<'static>, size: f32, tint: Color32) -> Image<'static> {
    Image::new(icon)
        .fit_to_exact_size(vec2(size, size))
        .tint(tint)
}

/// 11 px Text, nicht umbrechend.
pub fn small(ui: &mut Ui, text: &str, color: Color32) -> Response {
    ui.label(RichText::new(text).size(11.0).color(color))
}

/// Icon-Button 22 × 22 mit Tooltip und AccessKit-Namen.
pub fn icon_button(
    ui: &mut Ui,
    icon: ImageSource<'static>,
    tooltip: &str,
    selected: bool,
    enabled: bool,
) -> Response {
    let tint = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };
    let button = Button::new(icon_image(icon, 14.0, tint))
        .min_size(vec2(22.0, 22.0))
        .corner_radius(6)
        .selected(selected);
    let resp = ui.add_enabled(enabled, button);
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, tooltip));
    if enabled {
        resp.on_hover_text(tooltip)
    } else {
        resp
    }
}

/// Rahmen um einen Icon-Button beim Überfahren (Theme-Farbe).
pub fn hover_outline(ui: &Ui, resp: &Response) {
    if resp.hovered() {
        ui.painter().rect_stroke(
            resp.rect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.5_f32, ui.visuals().widgets.hovered.fg_stroke.color),
            egui::StrokeKind::Outside,
        );
    }
}

/// Banner (Fehler, Hinweis): Icon, umbrechender Text und rechts das Kreuz
/// zum Schließen. Der Text bricht um, statt das Kreuz bei langen Meldungen
/// aus dem Fenster zu schieben. `true` beim Klick auf das Kreuz.
pub fn banner(
    ui: &mut Ui,
    frame: egui::Frame,
    icon: ImageSource<'static>,
    text: RichText,
    tint: Color32,
    close_label: &str,
) -> bool {
    use crate::ui::icons::ICON_CROSS;
    let mut closed = false;
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(icon_image(icon, 16.0, tint));
            // Platz für das Kreuz (12 px + Button-Rand) frei halten
            let text_width = (ui.available_width() - 34.0).max(80.0);
            ui.scope(|ui| {
                ui.set_max_width(text_width);
                ui.add(egui::Label::new(text).wrap());
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                let resp = ui.add(Button::image(icon_image(ICON_CROSS, 12.0, tint)));
                resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, close_label));
                closed = resp.on_hover_text(close_label).clicked();
            });
        });
    });
    closed
}

/// Zeichnet einen Listeneintrag (Karte, Zeile) nur, wenn er im sichtbaren
/// Bereich liegt. Außerhalb wird lediglich seine zuletzt gemessene Höhe
/// freigehalten: lange Listen bauen sonst in jedem Frame jeden Eintrag neu
/// auf (bei laufendem Spinner mit der Bildwiederholrate). Ohne Messwert,
/// nach geänderter Breite (Umbruch) oder mit `always` (Eintrag, zu dem
/// gescrollt wird) wird gezeichnet und neu gemessen.
pub fn culled(ui: &mut Ui, id: egui::Id, always: bool, draw: impl FnOnce(&mut Ui)) {
    let key = id.with("culled_size");
    let width = ui.available_width();
    let top = ui.cursor().min.y;
    if !always {
        if let Some((w, h)) = ui.ctx().data(|d| d.get_temp::<(f32, f32)>(key)) {
            let clip = ui.clip_rect();
            if (w - width).abs() < 0.5 && (top > clip.max.y || top + h < clip.min.y) {
                ui.allocate_space(vec2(width, h));
                return;
            }
        }
    }
    let height = ui.scope(draw).response.rect.height();
    ui.ctx().data_mut(|d| d.insert_temp(key, (width, height)));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 50 Einträge zu 40 px in einem 200 px hohen Bereich: nach dem ersten
    /// Frame (Messen) werden nur noch die sichtbaren aufgebaut, die
    /// Gesamthöhe bleibt gleich.
    #[test]
    fn culled_draws_only_visible_entries_after_measuring() {
        let ctx = egui::Context::default();
        let run = |ctx: &egui::Context| {
            let mut drawn = 0;
            let mut content_height = 0.0;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(300.0, 200.0),
                )),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                let out = egui::ScrollArea::vertical().show(ui, |ui| {
                    for i in 0..50 {
                        culled(ui, egui::Id::new(("entry", i)), i == 49, |ui| {
                            drawn += 1;
                            ui.allocate_space(vec2(ui.available_width(), 40.0));
                        });
                    }
                });
                content_height = out.content_size.y;
            });
            output.drop_without_applying_deltas();
            (drawn, content_height)
        };
        let (first, height) = run(&ctx);
        assert_eq!(first, 50, "ohne Messwert wird alles gezeichnet");
        let (second, height_after) = run(&ctx);
        // Sichtbar sind etwa 5; der letzte (`always`) kommt dazu
        assert!((2..=10).contains(&second), "{second}");
        assert_eq!(height, height_after);
    }
}
