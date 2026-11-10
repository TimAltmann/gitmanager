//! Benachrichtigung, wenn eine laufende Claude Session von „arbeitet“ auf
//! „wartet auf dich“ oder „fertig“ springt (Windows-Toast; Klick holt das
//! Terminal nach vorne).

use crate::claude_live::{LiveSession, LiveStatus};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    /// Rückfrage oder Freigabe nötig
    Waiting,
    /// Antwort fertig
    Done,
}

/// Eine fällige Benachrichtigung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub session_id: String,
    pub pid: u32,
    pub cwd: String,
    pub kind: NotifyKind,
}

/// Sessions, die zwischen `prev` und `next` von arbeitet (auch Shell) auf
/// wartet bzw. fertig gewechselt sind. Neue Sessions lösen nichts aus.
pub fn transitions(prev: &[LiveSession], next: &[LiveSession]) -> Vec<Notice> {
    let before: HashMap<&str, LiveStatus> = prev
        .iter()
        .map(|l| (l.session_id.as_str(), l.status))
        .collect();
    next.iter()
        .filter_map(|l| {
            let was_busy = matches!(
                before.get(l.session_id.as_str()),
                Some(LiveStatus::Busy | LiveStatus::Shell)
            );
            let kind = match l.status {
                LiveStatus::Waiting => NotifyKind::Waiting,
                LiveStatus::Idle => NotifyKind::Done,
                LiveStatus::Busy | LiveStatus::Shell => return None,
            };
            was_busy.then(|| Notice {
                session_id: l.session_id.clone(),
                pid: l.pid,
                cwd: l.cwd.clone(),
                kind,
            })
        })
        .collect()
}

/// Zeigt eine Benachrichtigung; `on_click` läuft beim Klick darauf (in
/// einem fremden Thread). Außerhalb von Windows ohne Wirkung.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn show_notification(title: &str, body: &str, on_click: impl Fn() + Send + 'static) {
    #[cfg(windows)]
    win::show(title, body, on_click);
}

#[cfg(windows)]
mod win {
    use tauri_winrt_notification::{Duration, Sound, Toast};

    /// Eigene App-ID für Toasts (unter HKCU registriert, damit Windows
    /// Name und Icon kennt); sonst die von PowerShell.
    const APP_ID: &str = "TimAltmann.GitManager";

    fn app_id() -> &'static str {
        static REGISTERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *REGISTERED.get_or_init(register) {
            APP_ID
        } else {
            Toast::POWERSHELL_APP_ID
        }
    }

    /// `HKCU\Software\Classes\AppUserModelId\<APP_ID>` mit Anzeigename und
    /// Icon (PNG im Konfigurationsordner).
    fn register() -> bool {
        use winreg::enums::HKEY_CURRENT_USER;
        let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
        let Ok((key, _)) =
            hkcu.create_subkey(format!("Software\\Classes\\AppUserModelId\\{APP_ID}"))
        else {
            return false;
        };
        if key.set_value("DisplayName", &"GitManager").is_err() {
            return false;
        }
        if let Some(icon) = icon_path() {
            let _ = key.set_value("IconUri", &icon.display().to_string());
        }
        true
    }

    fn icon_path() -> Option<std::path::PathBuf> {
        let dir = directories::ProjectDirs::from("com", "gitmanager", "gitmanager")?
            .data_local_dir()
            .to_path_buf();
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("notify_icon.png");
        if !path.is_file() {
            std::fs::write(&path, include_bytes!("../assets/icon.png")).ok()?;
        }
        Some(path)
    }

    pub fn show(title: &str, body: &str, on_click: impl Fn() + Send + 'static) {
        let title = title.to_string();
        let body = body.to_string();
        // WinRT-Aufrufe nicht im UI-Thread
        std::thread::spawn(move || {
            let _ = Toast::new(app_id())
                .title(&title)
                .text1(&body)
                .sound(Some(Sound::Default))
                .duration(Duration::Short)
                .on_activated(move |_| {
                    on_click();
                    Ok(())
                })
                .show();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(id: &str, status: LiveStatus) -> LiveSession {
        LiveSession {
            pid: 7,
            session_id: id.into(),
            cwd: "C:\\dev\\app".into(),
            status,
            status_since_ms: 0,
            name: None,
            proc_start: None,
        }
    }

    #[test]
    fn busy_to_waiting_and_done_notify() {
        let prev = [live("a", LiveStatus::Busy), live("b", LiveStatus::Shell)];
        let next = [live("a", LiveStatus::Waiting), live("b", LiveStatus::Idle)];
        let n = transitions(&prev, &next);
        assert_eq!(n.len(), 2);
        assert_eq!(
            (n[0].session_id.as_str(), n[0].kind),
            ("a", NotifyKind::Waiting)
        );
        assert_eq!(
            (n[1].session_id.as_str(), n[1].kind),
            ("b", NotifyKind::Done)
        );
        assert_eq!(n[0].pid, 7);
    }

    #[test]
    fn unchanged_new_or_idle_to_waiting_do_not_notify() {
        let prev = [
            live("a", LiveStatus::Waiting),
            live("b", LiveStatus::Idle),
            live("c", LiveStatus::Busy),
        ];
        let next = [
            live("a", LiveStatus::Waiting),
            live("b", LiveStatus::Waiting),
            live("c", LiveStatus::Busy),
            live("new", LiveStatus::Waiting),
        ];
        assert!(transitions(&prev, &next).is_empty());
    }
}
