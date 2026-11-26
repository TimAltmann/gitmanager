//! Existenz von Pfaden prüfen, ohne den UI-Thread zu blockieren: ein
//! `stat` auf einem nicht erreichbaren Netzlaufwerk dauert Sekunden und
//! hielte jeden Frame an. Die Prüfung läuft in einem Thread; die Ansicht
//! fragt je Frame nur den zuletzt bekannten Stand ab.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// So lange gilt ein Ergebnis, bevor im Hintergrund neu geprüft wird.
const TTL: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Probe {
    known: Option<bool>,
    checked: Option<Instant>,
    in_flight: bool,
}

static PROBES: Mutex<Option<HashMap<PathBuf, Probe>>> = Mutex::new(None);

fn with_probes<T>(f: impl FnOnce(&mut HashMap<PathBuf, Probe>) -> T) -> T {
    let mut guard = PROBES.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(HashMap::new))
}

/// Gibt es `path`? `None`, solange die erste Prüfung läuft. Ein älteres
/// Ergebnis bleibt stehen, während neu geprüft wird; ist die Prüfung
/// fertig, zeichnet `ctx` neu.
pub fn exists(ctx: &egui::Context, path: &Path) -> Option<bool> {
    let (known, start) = with_probes(|probes| {
        let probe = probes.entry(path.to_path_buf()).or_default();
        let stale = probe.checked.is_none_or(|t| t.elapsed() >= TTL);
        let start = stale && !probe.in_flight;
        probe.in_flight |= start;
        (probe.known, start)
    });
    if start {
        let ctx = ctx.clone();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let found = path.exists();
            with_probes(|probes| {
                let probe = probes.entry(path).or_default();
                probe.known = Some(found);
                probe.checked = Some(Instant::now());
                probe.in_flight = false;
            });
            ctx.request_repaint();
        });
    }
    known
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(ctx: &egui::Context, path: &Path) -> bool {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(found) = exists(ctx, path) {
                return found;
            }
            assert!(Instant::now() < until, "Prüfung hängt");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn exists_reports_after_background_check_and_keeps_result() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("fehlt");
        assert!(settled(&ctx, dir.path()));
        assert!(!settled(&ctx, &missing));
        // Innerhalb der Gültigkeit kommt das Ergebnis sofort, ohne neuen stat
        std::fs::create_dir(&missing).unwrap();
        assert_eq!(exists(&ctx, &missing), Some(false));
    }
}
