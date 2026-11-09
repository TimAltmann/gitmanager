//! Animationen für laufende Claude Sessions: pulsierender Stern, schimmernde
//! Verben, pulsierender Punkt, Haken.

use crate::claude_live::LiveStatus;
use crate::i18n::Language;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Pos2, Sense, Stroke, Ui, Vec2};
use std::f32::consts::TAU;

/// Claude-Orange (arbeitet)
pub const COLOR_BUSY: Color32 = Color32::from_rgb(217, 119, 87);
/// Blau (wartet auf dich)
pub const COLOR_WAITING: Color32 = Color32::from_rgb(95, 135, 255);
/// Grün (fertig)
pub const COLOR_IDLE: Color32 = Color32::from_rgb(78, 170, 100);

/// Sekunden, die ein Verb stehen bleibt
const VERB_SECONDS: f64 = 3.0;

const VERBS_DE: &[&str] = &[
    "Grübelt",
    "Tüftelt",
    "Sinniert",
    "Brütet",
    "Werkelt",
    "Knobelt",
    "Fabuliert",
    "Jongliert",
    "Destilliert",
    "Orchestriert",
    "Sortiert Gedanken",
    "Schraubt",
    "Zaubert",
    "Puzzelt",
    "Kombiniert",
    "Feilt",
    "Wuselt",
    "Hext",
    "Verknüpft",
    "Ersinnt",
];

const VERBS_EN: &[&str] = &[
    "Pondering",
    "Tinkering",
    "Noodling",
    "Mulling",
    "Brewing",
    "Puzzling",
    "Untangling",
    "Juggling",
    "Distilling",
    "Orchestrating",
    "Wrangling",
    "Conjuring",
    "Percolating",
    "Whittling",
    "Synthesizing",
    "Ruminating",
    "Scheming",
    "Fiddling",
    "Sleuthing",
    "Dreaming up",
];

/// Statusfarbe.
pub fn status_color(status: LiveStatus) -> Color32 {
    match status {
        LiveStatus::Busy | LiveStatus::Shell => COLOR_BUSY,
        LiveStatus::Waiting => COLOR_WAITING,
        LiveStatus::Idle => COLOR_IDLE,
    }
}

/// Stabiler Startwert je Session (unterschiedliche Verben nebeneinander).
pub fn seed_of(id: &str) -> u64 {
    id.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}

/// Verb für `t_secs`; wechselt alle drei Sekunden.
pub fn verb(lang: Language, seed: u64, t_secs: f64) -> &'static str {
    let list = match lang {
        Language::De => VERBS_DE,
        _ => VERBS_EN,
    };
    let step = (t_secs.max(0.0) / VERB_SECONDS) as u64;
    list[(seed.wrapping_add(step) % list.len() as u64) as usize]
}

/// Laufzeit wie „12s“, „2m 13s“, „1h 05m“.
pub fn format_elapsed(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m {:02}s", secs / 60, secs % 60),
        _ => format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60),
    }
}

/// Farbe je Zeichen: ein heller Fleck läuft von links nach rechts.
pub fn shimmer_colors(text: &str, t_secs: f64, base: Color32, highlight: Color32) -> Vec<Color32> {
    let n = text.chars().count();
    let span = n as f64 + 8.0;
    // ~12 Zeichen pro Sekunde, Start links vor dem Text
    let center = (t_secs * 12.0) % span - 4.0;
    (0..n)
        .map(|i| {
            let k = (1.0 - (i as f64 - center).abs() / 3.0).clamp(0.0, 1.0) as f32;
            lerp_color(base, highlight, k)
        })
        .collect()
}

/// Text mit Schimmer (siehe `shimmer_colors`).
pub fn shimmer_job(
    text: &str,
    t_secs: f64,
    font: FontId,
    base: Color32,
    highlight: Color32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    let colors = shimmer_colors(text, t_secs, base, highlight);
    for (c, color) in text.chars().zip(colors) {
        job.append(
            &c.to_string(),
            0.0,
            TextFormat {
                font_id: font.clone(),
                color,
                ..Default::default()
            },
        );
    }
    job
}

fn lerp_color(a: Color32, b: Color32, k: f32) -> Color32 {
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * k).round() as u8;
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

/// Heller als `c` (für den Schimmer).
pub fn brighten(c: Color32) -> Color32 {
    lerp_color(c, Color32::WHITE, 0.55)
}

/// Pulsierender, sich drehender Stern (sechs Strahlen).
pub fn paint_star(ui: &mut Ui, size: f32, t_secs: f64, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let t = t_secs as f32;
    let pulse = 0.62 + 0.38 * (0.5 + 0.5 * (t * TAU / 1.2).sin());
    let radius = size * 0.5 * pulse;
    let angle0 = t * 1.4;
    let center = rect.center();
    let painter = ui.painter();
    for i in 0..6 {
        let a = angle0 + i as f32 * TAU / 6.0;
        let long = if i % 2 == 0 { 1.0 } else { 0.7 };
        let end = center + Vec2::angled(a) * radius * long;
        painter.line_segment([center, end], Stroke::new(size * 0.13, color));
    }
    painter.circle_filled(center, size * 0.1, color);
}

/// Pulsierender Punkt (wartet auf dich).
pub fn paint_pulse_dot(ui: &mut Ui, size: f32, t_secs: f64, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let phase = (0.5 + 0.5 * (t_secs as f32 * TAU / 1.6).sin()).clamp(0.0, 1.0);
    let center = rect.center();
    let painter = ui.painter();
    let halo = color.gamma_multiply(0.35 * (1.0 - phase));
    painter.circle_filled(center, size * (0.3 + 0.2 * phase), halo);
    painter.circle_filled(center, size * 0.26, color);
}

/// Haken (fertig).
pub fn paint_check(ui: &mut Ui, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = |x: f32, y: f32| Pos2::new(rect.left() + x * size, rect.top() + y * size);
    let stroke = Stroke::new(size * 0.14, color);
    ui.painter()
        .line_segment([p(0.18, 0.55), p(0.42, 0.78)], stroke);
    ui.painter()
        .line_segment([p(0.42, 0.78), p(0.84, 0.26)], stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verb_is_stable_within_three_seconds_and_changes_after() {
        let seed = seed_of("abc");
        assert_eq!(verb(Language::De, seed, 0.1), verb(Language::De, seed, 2.9));
        assert_ne!(verb(Language::De, seed, 0.1), verb(Language::De, seed, 3.1));
        assert!(VERBS_EN.contains(&verb(Language::En, seed, 0.0)));
        assert!(VERBS_DE.contains(&verb(Language::De, seed, 0.0)));
    }

    #[test]
    fn different_sessions_start_with_different_verbs() {
        let a = verb(Language::En, seed_of("ddc775b3"), 0.0);
        let b = verb(Language::En, seed_of("ff772219"), 0.0);
        assert_ne!(a, b);
    }

    #[test]
    fn format_elapsed_units() {
        assert_eq!(format_elapsed(-5), "0s");
        assert_eq!(format_elapsed(12_400), "12s");
        assert_eq!(format_elapsed(133_000), "2m 13s");
        assert_eq!(format_elapsed(3_900_000), "1h 05m");
    }

    #[test]
    fn shimmer_moves_highlight_across_text() {
        let colors = |t: f64| shimmer_colors("Tüftelt", t, Color32::BLACK, Color32::WHITE);
        let brightest = |c: &[Color32]| (0..c.len()).max_by_key(|&i| c[i].r()).unwrap();
        let early = colors(0.5);
        let later = colors(0.7);
        assert_eq!(early.len(), 7, "eine Farbe je Zeichen (auch Umlaut)");
        assert!(brightest(&later) > brightest(&early));
        let job = shimmer_job(
            "Tüftelt",
            0.5,
            FontId::proportional(12.0),
            Color32::BLACK,
            Color32::WHITE,
        );
        assert_eq!(job.text, "Tüftelt");
    }

    #[test]
    fn status_colors_differ() {
        assert_eq!(status_color(LiveStatus::Shell), COLOR_BUSY);
        assert_ne!(
            status_color(LiveStatus::Waiting),
            status_color(LiveStatus::Idle)
        );
    }
}
