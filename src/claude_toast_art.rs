//! Logo für die Windows-Benachrichtigung der Claude Sessions: das
//! GitManager-Icon, rund mit farbigem Statusring (fertig / wartet). Beim ersten
//! Gebrauch gerendert (Toasts brauchen Bilddateien).

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

/// Pfad zum Logo für `kind`; rendert es beim ersten Mal nach `dir`.
pub fn toast_logo(dir: &Path, kind: NotifyKind) -> Option<PathBuf> {
    let name = match kind {
        NotifyKind::Done => "done",
        NotifyKind::Waiting => "waiting",
    };
    let name = format!("toast_{name}_v{ART_VERSION}.png");
    let path = dir.join(&name);
    if !path.is_file() {
        std::fs::create_dir_all(dir).ok()?;
        // Erst vollständig schreiben, dann umbenennen: kein halbes PNG
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, logo_png(kind)?).ok()?;
        std::fs::rename(&tmp, &path).ok()?;
    }
    Some(path)
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
}
