//! Logos für Windows-Benachrichtigungen. Claude Sessions: das
//! GitManager-Icon, rund mit farbigem Statusring (fertig / wartet). Merge
//! Requests: ein Personen-Avatar, weil dort ein anderer Mensch etwas getan
//! hat; mit bekanntem Autor wie in Teams seine Initialen auf einer Farbe
//! je Name. Beim ersten Gebrauch gerendert (Toasts brauchen Bilddateien).

// Nur die Windows-Benachrichtigung nutzt die Bilder (Tests überall)
#![cfg_attr(not(windows), allow(dead_code))]

use crate::claude_notify::NotifyKind;
use std::path::{Path, PathBuf};

/// Bei geänderten Bildern erhöhen: neue Dateinamen statt veralteter PNGs
const ART_VERSION: u32 = 3;

/// Kantenlänge des Logos in Pixeln
const SIZE: u32 = 256;

const ICON: &[u8] = include_bytes!("../assets/icon.png");

/// Wie `COLOR_IDLE` bzw. `COLOR_WAITING` in der Session-Ansicht
const GREEN: &str = "#4EAA64";
const BLUE: &str = "#5F87FF";
/// Grund des Personen-Avatars: gedecktes Violett, klar unterscheidbar vom
/// GitManager-Icon und den Statusringen
const PERSON_BG: &str = "#7A5FC4";

/// Statusring am Rand (Windows schneidet das Logo kreisrund aus).
fn ring_svg(kind: NotifyKind) -> String {
    let accent = match kind {
        NotifyKind::Done => GREEN,
        NotifyKind::Waiting => BLUE,
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{SIZE}" height="{SIZE}" viewBox="0 0 256 256">
<circle cx="128" cy="128" r="119" fill="none" stroke="{accent}" stroke-width="16"/>
</svg>"##
    )
}

/// Rundes GitManager-Logo mit Statusring als PNG.
fn logo_png(kind: NotifyKind) -> Option<Vec<u8>> {
    use resvg::tiny_skia::{Color, IntSize, Pixmap, PixmapPaint, Transform};
    let icon = image::load_from_memory(ICON).ok()?.to_rgba8();
    // Hintergrund wie die Ecke des Icons, damit der Rand nahtlos ist
    let [r, g, b, _] = icon.get_pixel(4, 4).0;
    let inner = (SIZE as f32 * 0.86) as u32;
    let icon = image::imageops::resize(&icon, inner, inner, image::imageops::FilterType::Lanczos3);
    // Icon ist deckend: RGBA entspricht vormultipliziertem RGBA
    let icon = Pixmap::from_vec(icon.into_raw(), IntSize::from_wh(inner, inner)?)?;
    let mut pixmap = Pixmap::new(SIZE, SIZE)?;
    pixmap.fill(Color::from_rgba8(r, g, b, 255));
    let offset = ((SIZE - inner) / 2) as i32;
    pixmap.draw_pixmap(
        offset,
        offset,
        icon.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    let ring =
        resvg::usvg::Tree::from_str(&ring_svg(kind), &resvg::usvg::Options::default()).ok()?;
    resvg::render(&ring, Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// Personen-Avatar: weiße Silhouette (Kopf, Schultern) auf farbigem Grund.
/// Windows schneidet rund aus, die Schultern dürfen über den Rand ragen.
fn person_svg() -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{SIZE}" height="{SIZE}" viewBox="0 0 256 256">
<rect width="256" height="256" fill="{PERSON_BG}"/>
<circle cx="128" cy="100" r="46" fill="#FFFFFF"/>
<ellipse cx="128" cy="236" rx="86" ry="70" fill="#FFFFFF"/>
</svg>"##
    )
}

/// Avatar-Logo als PNG.
fn person_png() -> Option<Vec<u8>> {
    use resvg::tiny_skia::{Pixmap, Transform};
    let tree = resvg::usvg::Tree::from_str(&person_svg(), &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = Pixmap::new(SIZE, SIZE)?;
    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// Hintergründe der Initialen-Avatare (gedeckt, weiße Schrift lesbar)
const AUTHOR_COLORS: [&str; 10] = [
    "#C4314B", "#CA5010", "#986F0B", "#498205", "#038387", "#0078D4", "#4F6BED", "#8764B8",
    "#C239B3", "#7A7574",
];

/// Initialen wie in Teams: erster Buchstabe von Vor- und Nachname
/// („Muster, Anna“ → „AM“), bei einem Wort nur einer; leer ohne Buchstaben.
pub fn initials(name: &str) -> String {
    let ordered = match name.split_once(',') {
        Some((last, first)) => format!("{first} {last}"),
        None => name.to_string(),
    };
    // Zusätze in Klammern (z. B. „(Extern)“) zählen nicht
    let words: Vec<&str> = ordered
        .split_whitespace()
        .filter(|w| !w.starts_with('('))
        .filter(|w| w.chars().next().is_some_and(char::is_alphanumeric))
        .collect();
    let first_char = |w: &str| {
        w.chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
    };
    match words.as_slice() {
        [] => String::new(),
        [one] => first_char(one).unwrap_or_default(),
        [first, .., last] => {
            first_char(first).unwrap_or_default() + &first_char(last).unwrap_or_default()
        }
    }
}

/// Feste Farbe je Name (gleicher Name, gleiche Farbe; FNV-1a).
fn author_color(name: &str) -> &'static str {
    let hash = name.to_lowercase().bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    AUTHOR_COLORS[hash as usize % AUTHOR_COLORS.len()]
}

/// Systemschriften einmal laden (Windows: Segoe UI).
fn fontdb() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static DB: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        std::sync::Arc::new(db)
    })
    .clone()
}

fn author_svg(initials: &str, color: &str) -> String {
    // Zwei Buchstaben etwas kleiner; Grundlinie so, dass Großbuchstaben
    // mittig stehen
    let size = if initials.chars().count() > 1 {
        104
    } else {
        120
    };
    let baseline = 128 + size * 36 / 100;
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{SIZE}" height="{SIZE}" viewBox="0 0 256 256">
<rect width="256" height="256" fill="{color}"/>
<text x="128" y="{baseline}" text-anchor="middle" font-family="Segoe UI, Ubuntu, DejaVu Sans, Arial, sans-serif" font-weight="600" font-size="{size}" fill="#FFFFFF">{initials}</text>
</svg>"##
    )
}

/// Initialen-Avatar als PNG; `None` ohne passende Schrift.
fn author_png(initials: &str, color: &str) -> Option<Vec<u8>> {
    use resvg::tiny_skia::{Pixmap, Transform};
    let db = fontdb();
    if db.is_empty() {
        return None;
    }
    let options = resvg::usvg::Options {
        fontdb: db,
        ..resvg::usvg::Options::default()
    };
    let svg = author_svg(&crate::text::xml_escape(initials), color);
    let tree = resvg::usvg::Tree::from_str(&svg, &options).ok()?;
    // Ohne gefundene Glyphen bliebe nur die Farbfläche
    if !tree.root().has_children() || tree.root().children().len() < 2 {
        return None;
    }
    let mut pixmap = Pixmap::new(SIZE, SIZE)?;
    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// `dir/<name>_v<ART_VERSION>.png`, beim ersten Mal mit `render` erzeugt.
fn cached_png(dir: &Path, name: &str, render: impl FnOnce() -> Option<Vec<u8>>) -> Option<PathBuf> {
    let name = format!("{name}_v{ART_VERSION}.png");
    let path = dir.join(&name);
    if !path.is_file() {
        std::fs::create_dir_all(dir).ok()?;
        // Erst vollständig schreiben, dann umbenennen: kein halbes PNG
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, render()?).ok()?;
        std::fs::rename(&tmp, &path).ok()?;
    }
    Some(path)
}

/// Pfad zum Logo für `kind`; rendert es beim ersten Mal nach `dir`.
pub fn toast_logo(dir: &Path, kind: NotifyKind) -> Option<PathBuf> {
    let name = match kind {
        NotifyKind::Done => "toast_done",
        NotifyKind::Waiting => "toast_waiting",
    };
    cached_png(dir, name, || logo_png(kind))
}

/// Pfad zum Personen-Avatar (MR-Benachrichtigungen); rendert es beim
/// ersten Mal nach `dir`.
pub fn person_logo(dir: &Path) -> Option<PathBuf> {
    cached_png(dir, "toast_person", person_png)
}

/// Avatar mit den Initialen von `author` (MR-Benachrichtigungen); ohne
/// Namen oder Schrift der allgemeine Personen-Avatar.
pub fn author_logo(dir: &Path, author: &str) -> Option<PathBuf> {
    let letters = initials(author);
    if !letters.is_empty() {
        let color = author_color(author);
        // Name aus Buchstaben und Farbe: gleiche Bilder teilen sich die Datei
        let hex: String = letters
            .chars()
            .map(|c| format!("{:x}", u32::from(c)))
            .collect::<Vec<_>>()
            .join("-");
        let name = format!("toast_author_{hex}_{}", color.trim_start_matches('#'));
        if let Some(path) = cached_png(dir, &name, || author_png(&letters, color)) {
            return Some(path);
        }
    }
    person_logo(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logos_render_and_are_cached() {
        let dir = tempfile::tempdir().unwrap();
        for kind in [NotifyKind::Done, NotifyKind::Waiting] {
            let logo = toast_logo(dir.path(), kind).expect("Logo");
            let png = std::fs::read(&logo).unwrap();
            assert!(png.starts_with(b"\x89PNG"), "kein PNG");
            assert_eq!(toast_logo(dir.path(), kind), Some(logo.clone()));
            if let Ok(out) = std::env::var("TOAST_ART_OUT") {
                std::fs::copy(&logo, Path::new(&out).join(logo.file_name().unwrap())).unwrap();
            }
        }
        let files = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(files, 2, "je Art ein Logo, keine Reste");
    }

    #[test]
    fn initials_like_teams() {
        assert_eq!(initials("Anna Muster"), "AM");
        assert_eq!(initials("anna maria muster"), "AM");
        assert_eq!(initials("Muster, Anna"), "AM");
        assert_eq!(initials("Anna Muster (Extern)"), "AM");
        assert_eq!(initials("SonarQube"), "S");
        assert_eq!(initials("Ömer Çelik"), "ÖÇ");
        assert_eq!(initials("  "), "");
        assert_eq!(initials("- ."), "");
    }

    #[test]
    fn author_color_is_stable_per_name() {
        assert_eq!(author_color("Anna Muster"), author_color("anna muster"));
        let colors: std::collections::HashSet<_> = ["Anna", "Bert", "Carla", "Dieter", "Eva"]
            .iter()
            .map(|n| author_color(n))
            .collect();
        assert!(colors.len() > 1, "nicht alle gleich");
    }

    #[test]
    fn author_logo_draws_initials_or_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        let logo = author_logo(dir.path(), "Anna Muster").expect("Logo");
        assert_eq!(
            author_logo(dir.path(), "Anna Muster"),
            Some(logo.clone()),
            "gecacht"
        );
        if fontdb().is_empty() {
            // Ohne Systemschrift: Personen-Avatar
            assert_eq!(person_logo(dir.path()), Some(logo));
            return;
        }
        let img = image::open(&logo).unwrap().to_rgba8();
        let bg = img.get_pixel(10, 10).0;
        let white = img.pixels().filter(|p| p.0 == [255, 255, 255, 255]).count();
        assert_ne!(bg, [255, 255, 255, 255]);
        assert!(white > 2000, "Initialen fehlen: {white} weiße Pixel");
        // Ohne Namen: Personen-Avatar
        assert_eq!(author_logo(dir.path(), ""), person_logo(dir.path()));
        if let Ok(out) = std::env::var("TOAST_ART_OUT") {
            std::fs::copy(&logo, Path::new(&out).join(logo.file_name().unwrap())).unwrap();
        }
    }

    #[test]
    fn person_logo_renders_white_figure_on_color() {
        let dir = tempfile::tempdir().unwrap();
        let logo = person_logo(dir.path()).expect("Avatar");
        assert_eq!(person_logo(dir.path()), Some(logo.clone()), "gecacht");
        let img = image::open(&logo).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (SIZE, SIZE));
        // Kopfmitte weiß, Ecke oben links im Grund
        assert_eq!(img.get_pixel(128, 100).0, [255, 255, 255, 255]);
        assert_eq!(img.get_pixel(10, 10).0, [0x7A, 0x5F, 0xC4, 255]);
        if let Ok(out) = std::env::var("TOAST_ART_OUT") {
            std::fs::copy(&logo, Path::new(&out).join(logo.file_name().unwrap())).unwrap();
        }
    }
}
