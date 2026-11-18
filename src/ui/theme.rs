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

/// Farben der Merge-Request-Ansicht (E14), je Theme eigene Werte. Text-
/// varianten halten ≥ 4.5:1, Icon-Varianten ≥ 3:1 gegen
/// `widgets.inactive.bg_fill` und `window_fill` (Solarized: Kontrast des
/// Theme-Fließtexts als Untergrenze); abgesichert in `mr_palette_contrast`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MrPalette {
    /// Nebentexte (Projekt, Zeit, Unterzeilen); Dark #A0A0A0 (User-Vorgabe)
    pub text2: Color32,
    /// Titel in „Meine“ und „Von anderen“
    pub title: Color32,
    /// Ruhigere Titel in „Nicht beteiligt“
    pub title_quiet: Color32,
    /// Spaltenfläche (`window_fill`)
    pub column: Color32,
    /// Spaltenrahmen und Trennlinien
    pub line: Color32,
    /// Zähler-Pill, Initialen, Erwähnt-Chip, „in Arbeit“
    pub pill_bg: Color32,
    pub pill_text: Color32,
    /// Grund-Chip „Reviewer“
    pub reviewer_bg: Color32,
    pub reviewer_text: Color32,
    /// Badge „Reviews warten“ (Umschalter, Tray)
    pub badge: Color32,
    pub badge_text: Color32,
    /// Approval-Balken: Spur, vollständig, teilweise
    pub track: Color32,
    pub bar_done: Color32,
    pub bar_partial: Color32,
    /// Pipeline grün, mergebereit, frei, approved
    pub ok_icon: Color32,
    pub ok_text: Color32,
    /// Pipeline fehlgeschlagen, Änderungen angefragt
    pub fail_icon: Color32,
    pub fail_text: Color32,
    /// Pipeline läuft / wartet
    pub run_icon: Color32,
    pub run_text: Color32,
    /// Offene Threads, Review ausstehend, Änderungen im Klon
    pub warn_icon: Color32,
    pub warn_text: Color32,
    /// Keine Pipeline, keine Threads, abgebrochen …
    pub neutral_icon: Color32,
    /// Teilfehler-Banner, wie das App-Banner in jedem Theme gleich
    pub error_bg: Color32,
    pub error_stroke: Color32,
    pub error_text: Color32,
    /// Hinweis-Banner („Nicht angemeldet“), wie der Suchpfad-Hinweis
    pub notice_bg: Color32,
    pub notice_stroke: Color32,
    pub notice_text: Color32,
}

/// Themespezifischer Teil der Palette.
struct MrThemeColors {
    text2: Color32,
    title_quiet: Color32,
    line: Color32,
    pill_bg: Color32,
    pill_text: Color32,
    reviewer_bg: Color32,
    reviewer_text: Color32,
    track: Color32,
    bar_done: Color32,
    bar_partial: Color32,
    /// (Icon, Text) je Status
    ok: (Color32, Color32),
    fail: (Color32, Color32),
    run: (Color32, Color32),
    warn: (Color32, Color32),
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Palette der MR-Ansicht für `theme`; Titel und Spalte kommen aus den
/// aktiven `visuals`.
pub fn mr_palette(theme: &Theme, visuals: &Visuals) -> MrPalette {
    let c = match theme {
        // Werte aus dem Mockup B
        Theme::Dark => MrThemeColors {
            text2: rgb(0xA0A0A0),
            title_quiet: rgb(0xB4B4B4),
            line: rgb(0x4A4A4A),
            pill_bg: rgb(0x4A4A4A),
            pill_text: rgb(0xE6E6E6),
            reviewer_bg: rgb(0x1E4A63),
            reviewer_text: rgb(0xC0DEFF),
            track: rgb(0x232323),
            bar_done: rgb(0x4CB860),
            bar_partial: rgb(0x5AAAFF),
            ok: (rgb(0x4CB860), rgb(0x4CB860)),
            fail: (rgb(0xFF8066), rgb(0xFF8066)),
            run: (rgb(0x5AAAFF), rgb(0x5AAAFF)),
            warn: (rgb(0xE0A040), rgb(0xE0A040)),
        },
        // Die Mockup-Farben liegen auf Hell unter 3:1 → dunklere Varianten
        Theme::Light => MrThemeColors {
            text2: rgb(0x666666),
            title_quiet: rgb(0x505050),
            line: rgb(0xD2D2D2),
            pill_bg: rgb(0xE6E6E6),
            pill_text: rgb(0x3C3C3C),
            reviewer_bg: rgb(0xD4E8FA),
            reviewer_text: rgb(0x00537D),
            track: rgb(0xE4E4E4),
            bar_done: rgb(0x388E3C),
            bar_partial: rgb(0x1976D2),
            ok: (rgb(0x388E3C), rgb(0x2A6E2E)),
            fail: (rgb(0xD32F2F), rgb(0xC62828)),
            run: (rgb(0x1976D2), rgb(0x1565C0)),
            warn: (rgb(0xB26A00), rgb(0x8A5A00)),
        },
        // Nord-Palette (Frost/Aurora), Rot für Text aufgehellt
        Theme::Nord => MrThemeColors {
            text2: rgb(0xB8B8B8),
            title_quiet: rgb(0xD8DEE9),
            line: rgb(0x4C566A),
            pill_bg: rgb(0x4C566A),
            pill_text: rgb(0xECEFF4),
            reviewer_bg: rgb(0x1E4A63),
            reviewer_text: rgb(0xC0DEFF),
            track: rgb(0x2E3440),
            bar_done: rgb(0xA3BE8C),
            bar_partial: rgb(0x88C0D0),
            ok: (rgb(0xA3BE8C), rgb(0xA3BE8C)),
            fail: (rgb(0xE08890), rgb(0xF0A0A8)),
            run: (rgb(0x88C0D0), rgb(0x88C0D0)),
            warn: (rgb(0xEBCB8B), rgb(0xEBCB8B)),
        },
        // Dracula-Palette, Rot für Text aufgehellt
        Theme::Dracula => MrThemeColors {
            text2: rgb(0xB8B8B8),
            title_quiet: rgb(0xE2E2DC),
            line: rgb(0x6272A4),
            pill_bg: rgb(0x5A5E78),
            pill_text: rgb(0xF8F8F2),
            reviewer_bg: rgb(0x1E4A63),
            reviewer_text: rgb(0xC0DEFF),
            track: rgb(0x282A36),
            bar_done: rgb(0x50FA7B),
            bar_partial: rgb(0x8BE9FD),
            ok: (rgb(0x50FA7B), rgb(0x50FA7B)),
            fail: (rgb(0xFF6E6E), rgb(0xFFA0A0)),
            run: (rgb(0x8BE9FD), rgb(0x8BE9FD)),
            warn: (rgb(0xFFB86C), rgb(0xFFB86C)),
        },
        // Solarized-Akzente, für Text abgedunkelt; Nebentext = Theme-Text
        Theme::Solarized => MrThemeColors {
            text2: visuals.text_color(),
            title_quiet: rgb(0x586E75),
            line: rgb(0xD3CBB7),
            pill_bg: rgb(0xDDD6C1),
            pill_text: rgb(0x073642),
            reviewer_bg: rgb(0xD5E6F2),
            reviewer_text: rgb(0x1A5A8A),
            track: rgb(0xDDD6C1),
            bar_done: rgb(0x6A7A00),
            bar_partial: rgb(0x2077B8),
            ok: (rgb(0x6A7A00), rgb(0x5F6F00)),
            fail: (rgb(0xDC322F), rgb(0xB02020)),
            run: (rgb(0x2077B8), rgb(0x1A6CA8)),
            warn: (rgb(0x9E7200), rgb(0x8A6400)),
        },
    };
    MrPalette {
        text2: c.text2,
        title: visuals.strong_text_color(),
        title_quiet: c.title_quiet,
        column: visuals.window_fill,
        line: c.line,
        pill_bg: c.pill_bg,
        pill_text: c.pill_text,
        reviewer_bg: c.reviewer_bg,
        reviewer_text: c.reviewer_text,
        badge: rgb(0x3068C8),
        badge_text: Color32::WHITE,
        track: c.track,
        bar_done: c.bar_done,
        bar_partial: c.bar_partial,
        ok_icon: c.ok.0,
        ok_text: c.ok.1,
        fail_icon: c.fail.0,
        fail_text: c.fail.1,
        run_icon: c.run.0,
        run_text: c.run.1,
        warn_icon: c.warn.0,
        warn_text: c.warn.1,
        neutral_icon: c.text2,
        error_bg: rgb(0xFFEBEB),
        error_stroke: rgb(0xDC6464),
        error_text: rgb(0xA02828),
        notice_bg: rgb(0xFFF8DC),
        notice_stroke: rgb(0xDCB450),
        notice_text: rgb(0x785A14),
    }
}

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

    /// Relative Leuchtdichte nach WCAG 2.x.
    fn luminance(c: Color32) -> f64 {
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    /// Kontrastverhältnis nach WCAG 2.x (1.0 … 21.0).
    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn mr_palette_contrast() {
        // E14: Text ≥ 4.5:1, Icons ≥ 3:1 gegen Fläche und Spalte; Solarized
        // hat selbst nur 3.64:1 im Fließtext, das gilt dort als Untergrenze.
        for theme in Theme::all() {
            let ctx = egui::Context::default();
            apply_theme(&ctx, &theme);
            let visuals = ctx.global_style().visuals.clone();
            let p = mr_palette(&theme, &visuals);
            assert_eq!(p.column, visuals.window_fill, "{theme:?}");
            for bg in [visuals.widgets.inactive.bg_fill, visuals.window_fill] {
                let body = contrast(visuals.text_color(), bg);
                let (text_min, icon_min) = if theme == Theme::Solarized {
                    (body.min(4.5), body.min(3.0))
                } else {
                    (4.5, 3.0)
                };
                let texts = [
                    ("text2", p.text2),
                    ("title", p.title),
                    ("title_quiet", p.title_quiet),
                    ("ok_text", p.ok_text),
                    ("fail_text", p.fail_text),
                    ("run_text", p.run_text),
                    ("warn_text", p.warn_text),
                ];
                for (name, c) in texts {
                    let got = contrast(c, bg);
                    assert!(
                        got >= text_min,
                        "{theme:?} {name}: {got:.2} < {text_min:.2}"
                    );
                }
                let icons = [
                    ("ok_icon", p.ok_icon),
                    ("fail_icon", p.fail_icon),
                    ("run_icon", p.run_icon),
                    ("warn_icon", p.warn_icon),
                    ("neutral_icon", p.neutral_icon),
                ];
                for (name, c) in icons {
                    let got = contrast(c, bg);
                    assert!(
                        got >= icon_min,
                        "{theme:?} {name}: {got:.2} < {icon_min:.2}"
                    );
                }
            }
            // Balken gegen die Spur, Chips und Banner gegen ihre eigene Fläche
            for (name, c) in [("bar_done", p.bar_done), ("bar_partial", p.bar_partial)] {
                let got = contrast(c, p.track);
                assert!(got >= 3.0, "{theme:?} {name}: {got:.2} < 3");
            }
            let pairs = [
                ("pill", p.pill_text, p.pill_bg),
                ("reviewer", p.reviewer_text, p.reviewer_bg),
                ("badge", p.badge_text, p.badge),
                ("error", p.error_text, p.error_bg),
                ("notice", p.notice_text, p.notice_bg),
            ];
            for (name, fg, bg) in pairs {
                let got = contrast(fg, bg);
                assert!(got >= 4.5, "{theme:?} {name}: {got:.2} < 4.5");
            }
        }
    }

    #[test]
    fn mr_palette_dark_text2_is_user_value() {
        // Spec: Nebentexte #A0A0A0 im Dark-Theme, Theme-Text bei Solarized
        let ctx = egui::Context::default();
        apply_theme(&ctx, &Theme::Dark);
        let p = mr_palette(&Theme::Dark, &ctx.global_style().visuals);
        assert_eq!(p.text2, Color32::from_rgb(0xA0, 0xA0, 0xA0));
        let ctx = egui::Context::default();
        apply_theme(&ctx, &Theme::Solarized);
        let visuals = ctx.global_style().visuals.clone();
        let p = mr_palette(&Theme::Solarized, &visuals);
        assert_eq!(p.text2, visuals.text_color());
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
