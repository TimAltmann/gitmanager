use crate::config::Theme;
use egui::{Color32, Visuals};

pub fn apply_theme(ctx: &egui::Context, theme: &Theme) {
    let mut visuals = match theme {
        Theme::Light => {
            let mut v = Visuals::light();
            v.widgets.noninteractive.bg_fill = Color32::from_rgb(248, 248, 248);
            v.widgets.inactive.bg_fill = Color32::from_rgb(240, 240, 240);
            v.widgets.hovered.bg_fill = Color32::from_rgb(230, 230, 230);
            v.widgets.active.bg_fill = Color32::from_rgb(220, 220, 220);
            v.panel_fill = Color32::from_rgb(250, 250, 250);
            v.window_fill = Color32::from_rgb(255, 255, 255);
            v.extreme_bg_color = Color32::from_rgb(255, 255, 255);
            v
        }
        Theme::Dark => {
            let mut v = Visuals::dark();
            v.widgets.noninteractive.bg_fill = Color32::from_rgb(45, 45, 45);
            v.widgets.inactive.bg_fill = Color32::from_rgb(55, 55, 55);
            v.widgets.hovered.bg_fill = Color32::from_rgb(65, 65, 65);
            v.widgets.active.bg_fill = Color32::from_rgb(75, 75, 75);
            v.panel_fill = Color32::from_rgb(35, 35, 35);
            v.window_fill = Color32::from_rgb(45, 45, 45);
            v.extreme_bg_color = Color32::from_rgb(35, 35, 35);
            v
        }
        Theme::Nord => {
            let mut v = Visuals::dark();
            v.widgets.noninteractive.bg_fill = Color32::from_rgb(46, 52, 64);
            v.widgets.inactive.bg_fill = Color32::from_rgb(59, 66, 82);
            v.widgets.hovered.bg_fill = Color32::from_rgb(67, 76, 94);
            v.widgets.active.bg_fill = Color32::from_rgb(76, 86, 106);
            v.panel_fill = Color32::from_rgb(46, 52, 64);
            v.window_fill = Color32::from_rgb(59, 66, 82);
            v.extreme_bg_color = Color32::from_rgb(46, 52, 64);
            v.override_text_color = Some(Color32::from_rgb(236, 239, 244));
            v
        }
        Theme::Dracula => {
            let mut v = Visuals::dark();
            v.widgets.noninteractive.bg_fill = Color32::from_rgb(40, 42, 54);
            v.widgets.inactive.bg_fill = Color32::from_rgb(68, 71, 90);
            v.widgets.hovered.bg_fill = Color32::from_rgb(98, 114, 164);
            v.widgets.active.bg_fill = Color32::from_rgb(80, 90, 130);
            v.panel_fill = Color32::from_rgb(40, 42, 54);
            v.window_fill = Color32::from_rgb(68, 71, 90);
            v.extreme_bg_color = Color32::from_rgb(40, 42, 54);
            v.override_text_color = Some(Color32::from_rgb(248, 248, 242));
            v
        }
        Theme::Solarized => {
            let mut v = Visuals::light();
            v.widgets.noninteractive.bg_fill = Color32::from_rgb(253, 246, 227);
            v.widgets.inactive.bg_fill = Color32::from_rgb(238, 232, 213);
            v.widgets.hovered.bg_fill = Color32::from_rgb(221, 214, 193);
            v.widgets.active.bg_fill = Color32::from_rgb(207, 200, 180);
            v.panel_fill = Color32::from_rgb(253, 246, 227);
            v.window_fill = Color32::from_rgb(238, 232, 213);
            v.extreme_bg_color = Color32::from_rgb(253, 246, 227);
            v.override_text_color = Some(Color32::from_rgb(101, 123, 131));
            v
        }
    };

    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);
    visuals.window_corner_radius = egui::CornerRadius::same(8);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.global_style()).clone();
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(18.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(11.0, egui::FontFamily::Proportional),
    );
    ctx.set_global_style(style);
}

pub const COLOR_DIRTY: Color32 = Color32::from_rgb(220, 70, 40);
pub const COLOR_CLEAN: Color32 = Color32::from_rgb(60, 160, 80);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Theme;

    #[test]
    fn apply_theme_sets_colors_per_variant() {
        for theme in Theme::all() {
            let ctx = egui::Context::default();
            apply_theme(&ctx, &theme);
            let visuals = ctx.global_style().visuals.clone();
            match theme {
                Theme::Light => {
                    assert_eq!(visuals.panel_fill, Color32::from_rgb(250, 250, 250));
                    assert_eq!(visuals.window_fill, Color32::from_rgb(255, 255, 255));
                    assert_eq!(visuals.override_text_color, None);
                }
                Theme::Dark => {
                    assert_eq!(visuals.panel_fill, Color32::from_rgb(35, 35, 35));
                    assert_eq!(visuals.override_text_color, None);
                }
                Theme::Nord => {
                    assert_eq!(visuals.panel_fill, Color32::from_rgb(46, 52, 64));
                    assert_eq!(
                        visuals.override_text_color,
                        Some(Color32::from_rgb(236, 239, 244))
                    );
                }
                Theme::Dracula => {
                    assert_eq!(visuals.panel_fill, Color32::from_rgb(40, 42, 54));
                    assert_eq!(
                        visuals.override_text_color,
                        Some(Color32::from_rgb(248, 248, 242))
                    );
                }
                Theme::Solarized => {
                    assert_eq!(visuals.panel_fill, Color32::from_rgb(253, 246, 227));
                    assert_eq!(
                        visuals.override_text_color,
                        Some(Color32::from_rgb(101, 123, 131))
                    );
                }
            }
        }
    }

    #[test]
    fn apply_theme_corner_radius_and_text_styles() {
        let ctx = egui::Context::default();
        apply_theme(&ctx, &Theme::Light);
        let visuals = ctx.global_style().visuals.clone();
        assert_eq!(
            visuals.widgets.noninteractive.corner_radius,
            egui::CornerRadius::same(6)
        );
        assert_eq!(
            visuals.widgets.inactive.corner_radius,
            egui::CornerRadius::same(6)
        );
        assert_eq!(
            visuals.widgets.hovered.corner_radius,
            egui::CornerRadius::same(6)
        );
        assert_eq!(
            visuals.widgets.active.corner_radius,
            egui::CornerRadius::same(6)
        );
        assert_eq!(visuals.window_corner_radius, egui::CornerRadius::same(8));
        let style = ctx.global_style();
        assert_eq!(style.text_styles[&egui::TextStyle::Heading].size, 18.0);
        assert_eq!(style.text_styles[&egui::TextStyle::Body].size, 13.0);
        assert_eq!(style.text_styles[&egui::TextStyle::Button].size, 13.0);
        assert_eq!(style.text_styles[&egui::TextStyle::Small].size, 11.0);
    }

    #[test]
    fn color_constants_rgb() {
        assert_eq!(COLOR_DIRTY, Color32::from_rgb(220, 70, 40));
        assert_eq!(COLOR_CLEAN, Color32::from_rgb(60, 160, 80));
        // verify they are distinct
        assert_ne!(COLOR_DIRTY, COLOR_CLEAN);
    }

    #[test]
    fn apply_theme_no_panic_per_variant() {
        for theme in Theme::all() {
            let ctx = egui::Context::default();
            // should not panic
            apply_theme(&ctx, &theme);
            // style should be set
            assert!(ctx.global_style().visuals.panel_fill != Color32::TRANSPARENT);
        }
    }
}
