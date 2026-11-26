//! Kleine, ansichtsübergreifende Bausteine der Merge-Request-Oberflächen
//! (Ansicht und Einstellungs-Tab).

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
