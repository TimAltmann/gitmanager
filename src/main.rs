#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod config_parser;
mod git;
mod i18n;
mod scanner;
mod tray;
mod tray_service;
mod ui;
mod updater;

use app::MyApp;

fn load_icon() -> Option<std::sync::Arc<egui::IconData>> {
    // Icon aus assets/icon.ico laden – wird via include_bytes! in die EXE eingebettet
    // ICO enthält 6 Größen (16-256); image crate wählt automatisch die größte/laut ICO.
    // Fallback: falls icon.png vorhanden ist, nutze PNG (höhere Qualität).
    // Wir versuchen zuerst ICO (immer vorhanden nach Konvertierung), dann PNG.
    let icon_bytes: &[u8] = include_bytes!("../assets/icon.ico");
    if let Ok(image) = image::load_from_memory(icon_bytes) {
        let rgba = image.to_rgba8();
        let (width, height) = rgba.dimensions();
        // Winit erwartet width/height als Vielfaches von 4 – ICO Größen sind bereits 4er-Vielfache
        return Some(std::sync::Arc::new(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        }));
    }
    // Fallback: versuche PNG falls ICO dekodieren fehlschlägt (sollte nicht passieren)
    None
}

fn crash_log_filename() -> String {
    // Prozessweiter Zähler: Doppel-Panic innerhalb derselben Millisekunde
    // (Panic-während-Panic, Destruktor-Panic bei Unwind) darf nie kollidieren.
    static CRASH_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CRASH_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("gitmanager_crash-{}-{}-{}.log", ms, std::process::id(), n)
}

fn main() -> eframe::Result<()> {
    // Panic hook für Tray-Crashes (F-17): vorherigen Hook chainen, Crash-Log mit
    // Millisekunden-Timestamp + PID (keine Kollision pro Sekunde) nach
    // ProjectDirs::from("com","gitmanager","gitmanager").data_local_dir()
    // (Windows: %LOCALAPPDATA%\com\gitmanager\gitmanager), Fallback CWD.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut msg = format!("PANIC: {}\n", info);
        if let Some(s) = info.payload().downcast_ref::<&str>() {
            msg.push_str(&format!("payload: {}\n", s));
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            msg.push_str(&format!("payload: {}\n", s));
        }
        if let Some(loc) = info.location() {
            msg.push_str(&format!(
                "at {}:{}:{}\n",
                loc.file(),
                loc.line(),
                loc.column()
            ));
        }
        let filename = crash_log_filename();
        let written =
            directories::ProjectDirs::from("com", "gitmanager", "gitmanager").map(|dirs| {
                let dir = dirs.data_local_dir().to_path_buf();
                let _ = std::fs::create_dir_all(&dir);
                let path = dir.join(&filename);
                std::fs::write(&path, &msg).map(|_| path.display().to_string())
            });
        match written {
            Some(Ok(path)) => eprintln!("{} (crash log: {})", msg, path),
            _ => {
                // Fallback CWD, Fehler nicht schlucken ohne Hinweis
                if let Err(e) = std::fs::write(&filename, &msg) {
                    eprintln!("{} (crash log failed: {})", msg, e);
                } else {
                    eprintln!("{} (crash log: {})", msg, filename);
                }
            }
        }
        previous_hook(info);
    }));

    let icon = load_icon();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1080.0, 680.0])
        .with_min_inner_size([920.0, 560.0])
        .with_title("GitManager - Git Repository Manager");

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "gitmanager",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_filenames_unique_without_sleep() {
        // Doppel-Panic im selben Prozess innerhalb einer ms darf nie kollidieren
        // (kein Sleep – muss auch unter Windows-Timer-Granularität halten).
        let mut names = std::collections::HashSet::new();
        for _ in 0..100 {
            names.insert(crash_log_filename());
        }
        assert_eq!(names.len(), 100);
    }

    #[test]
    fn crash_filename_contains_ms_and_pid() {
        let a = crash_log_filename();
        let b = crash_log_filename();
        assert!(a.starts_with("gitmanager_crash-"));
        assert!(a.contains(&format!("-{}-", std::process::id())));
        assert!(a.ends_with(".log"));
        // Zähler-Suffix statt Sleep: zwei Aufrufe kollidieren nie.
        assert_ne!(a, b);
    }
}
