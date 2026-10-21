#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod config;
mod config_parser;
mod git;
mod gitlab;
mod i18n;
mod process;
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

/// F3#6: max. Crash-Logs im Verzeichnis — älteste (Namen enthalten
/// ms-Timestamp, lexikografisch ≈ chronologisch) löschen.
const MAX_CRASH_LOGS: usize = 20;

fn prune_crash_logs(dir: &std::path::Path) {
    let mut logs: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("gitmanager_crash-"))
                        && p.extension().is_some_and(|x| x == "log")
                })
                .collect()
        })
        .unwrap_or_default();
    logs.sort();
    // Auf < MAX kürzen (neue Datei danach → max. MAX gesamt).
    // len >= MAX impliziert nicht-leer — Index 0 existiert.
    while logs.len() >= MAX_CRASH_LOGS {
        let _ = std::fs::remove_file(&logs[0]);
        logs.remove(0);
    }
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

/// N6: `--minimized` an genau einer Stelle entschieden (Single Source) —
/// `main()` (Viewport) und `MyApp::new` (window_visible) nutzen beide das.
/// Nur auf Windows (ohne Tray kein Rückweg ins unsichtbare Fenster).
pub fn start_minimized_flag() -> bool {
    cfg!(target_os = "windows") && std::env::args().any(|a| a == "--minimized")
}

fn main() -> eframe::Result<()> {
    // P2a: globaler Connect-Timeout für libgit2 (Default: OS-TCP, bei
    // Blackhole-Remotes minutenlang). Gilt prozessweit für alle Fetches;
    // betrifft nur die Connect-Phase, nicht langsame Transfers.
    // P2a: globaler Connect-Timeout für libgit2 (Default: OS-TCP, bei
    // Blackhole-Remotes minutenlang). Gilt prozessweit für alle Fetches;
    // betrifft nur die Connect-Phase, nicht langsame Transfers.
    let _ = unsafe { git2::opts::set_server_connect_timeout_in_milliseconds(30_000) };
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
                // F3#6: Rotation — max. N Logs behalten (abgefangene Panics
                // aus catch_unwind würden sonst unbegrenzt ansammeln).
                prune_crash_logs(&dir);
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
    // M1: --minimized für Autostart (Tray-Start ohne volles Fenster).
    let start_minimized = start_minimized_flag();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(app::DEFAULT_WINDOW_SIZE)
        .with_min_inner_size([920.0, 560.0])
        .with_title("GitManager - Git Repository Manager");
    if start_minimized {
        viewport = viewport.with_visible(false);
    }

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
    fn start_minimized_flag_off_without_arg() {
        // N6: Test-Runner läuft ohne --minimized → Flag aus.
        // (Mit Flag ließe sich env nicht setzen — Verhalten via Codepfad klar.)
        assert!(!start_minimized_flag());
    }

    #[test]
    fn prune_crash_logs_keeps_newest_twenty() {
        // F3#6: Rotation deckelt die Dateien (abgefangene Panics sonst endlos).
        let dir = tempfile::tempdir().unwrap();
        for i in 0..25 {
            std::fs::write(
                dir.path().join(format!("gitmanager_crash-{i:03}-1-0.log")),
                "x",
            )
            .unwrap();
        }
        std::fs::write(dir.path().join("other.txt"), "x").unwrap();
        prune_crash_logs(dir.path());
        let logs: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("gitmanager_crash-"))
            .collect();
        // Prune kürzt auf MAX-1 (neue Datei danach → max. MAX gesamt).
        assert_eq!(
            logs.len(),
            MAX_CRASH_LOGS - 1,
            "must prune to MAX-1, got {logs:?}"
        );
        assert!(
            !logs.iter().any(|n| n.contains("-000-")),
            "oldest must be gone: {logs:?}"
        );
        assert!(
            logs.iter().any(|n| n.contains("-024-")),
            "newest must be kept: {logs:?}"
        );
        assert!(
            dir.path().join("other.txt").exists(),
            "foreign files untouched"
        );
    }

    #[test]
    fn libgit2_connect_timeout_roundtrips() {
        // P2a: Timeout-Optik existiert und ist setzbar (schützt vor
        // Blackhole-Hängern); main() setzt 30 s, hier Roundtrip prüfen.
        unsafe {
            git2::opts::set_server_connect_timeout_in_milliseconds(30_000).unwrap();
            assert_eq!(
                git2::opts::get_server_connect_timeout_in_milliseconds().unwrap(),
                30_000
            );
        }
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
