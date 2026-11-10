//! Benachrichtigung, wenn eine laufende Claude Session von „arbeitet“ auf
//! „wartet auf dich“ oder „fertig“ springt (Windows-Toast; Klick holt das
//! Terminal nach vorne).
//!
//! Das Toast-XML entsteht hier selbst (Kopfzeile, Zuordnungszeile,
//! Button-Stil kann `tauri-winrt-notification` nicht) und geht direkt an
//! die WinRT-API.

use crate::claude_live::{LiveSession, LiveStatus};
use crate::claude_preview::{PreviewItem, PreviewRole};
use std::collections::HashMap;
use std::path::Path;

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
    /// Wie lange Claude vorher gearbeitet hat (ms), falls bekannt
    pub busy_ms: Option<i64>,
}

/// Sessions, die zwischen `prev` und `next` von arbeitet (auch Shell) auf
/// wartet bzw. fertig gewechselt sind. Neue Sessions lösen nichts aus.
pub fn transitions(prev: &[LiveSession], next: &[LiveSession]) -> Vec<Notice> {
    let before: HashMap<&str, &LiveSession> =
        prev.iter().map(|l| (l.session_id.as_str(), l)).collect();
    next.iter()
        .filter_map(|l| {
            let old = before.get(l.session_id.as_str())?;
            if !matches!(old.status, LiveStatus::Busy | LiveStatus::Shell) {
                return None;
            }
            let kind = match l.status {
                LiveStatus::Waiting => NotifyKind::Waiting,
                LiveStatus::Idle => NotifyKind::Done,
                LiveStatus::Busy | LiveStatus::Shell => return None,
            };
            // Beide Zeitpunkte aus Claudes Statusdatei: Beginn der Arbeit
            // und Wechsel auf wartet/fertig
            let busy_ms = (old.status_since_ms > 0 && l.status_since_ms > old.status_since_ms)
                .then(|| l.status_since_ms - old.status_since_ms);
            Some(Notice {
                session_id: l.session_id.clone(),
                pid: l.pid,
                cwd: l.cwd.clone(),
                kind,
                busy_ms,
            })
        })
        .collect()
}

/// Inhalt einer Benachrichtigung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub kind: NotifyKind,
    /// Kopfzeile, die alle Benachrichtigungen bündelt („Claude Sessions“)
    pub header: String,
    /// Titel der Session
    pub title: String,
    /// Letzte Antwort bzw. Rückfrage von Claude; sonst `status`
    pub snippet: Option<String>,
    /// „Claude ist fertig“ (wenn keine Vorschau da ist)
    pub status: String,
    /// Kleine Zeile unten: „fertig nach 4m 12s · repo · branch“
    pub attribution: String,
    /// Beschriftungen der Buttons „Terminal“ und „in GitManager anzeigen“
    pub button_terminal: String,
    pub button_show: String,
}

/// Was in der Benachrichtigung geklickt wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastClick {
    /// Benachrichtigung selbst oder Button „Terminal“
    Terminal,
    /// Button „in GitManager anzeigen“ oder die Kopfzeile
    Show,
}

const ACTION_TERMINAL: &str = "terminal";
const ACTION_SHOW: &str = "show";
/// Kennung der Kopfzeile (bündelt die Toasts im Benachrichtigungscenter)
const HEADER_ID: &str = "claude-sessions";

/// Klick aus dem Argument der Benachrichtigung (leer: Körper geklickt).
#[cfg_attr(not(windows), allow(dead_code))]
fn click_of(action: &str) -> ToastClick {
    match action {
        ACTION_SHOW => ToastClick::Show,
        _ => ToastClick::Terminal,
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Toast-XML (Vorlage `ToastGeneric`); `logo` = rundes Logo mit Statusring.
pub fn toast_xml(toast: &Toast, logo: Option<&Path>) -> String {
    let e = xml_escape;
    let image = logo.map_or_else(String::new, |p| {
        format!(
            r#"<image placement="appLogoOverride" hint-crop="circle" src="file:///{}"/>"#,
            e(&p.display().to_string())
        )
    });
    let line = toast.snippet.as_deref().unwrap_or(&toast.status);
    // Grüner „Erfolg“-Stil nur für fertig; für wartet gibt Windows keine
    // passende Farbe her
    let terminal_style = match toast.kind {
        NotifyKind::Done => r#" hint-buttonStyle="Success""#,
        NotifyKind::Waiting => "",
    };
    let audio = match toast.kind {
        NotifyKind::Waiting => r#"<audio src="ms-winsoundevent:Notification.Reminder"/>"#,
        NotifyKind::Done => "",
    };
    format!(
        r#"<toast launch="{ACTION_TERMINAL}" useButtonStyle="true"><header id="{HEADER_ID}" title="{header}" arguments="{ACTION_SHOW}"/><visual><binding template="ToastGeneric"><text>{title}</text><text>{line}</text><text placement="attribution">{attribution}</text>{image}</binding></visual><actions><action content="{terminal}" arguments="{ACTION_TERMINAL}"{terminal_style}/><action content="{show}" arguments="{ACTION_SHOW}"/></actions>{audio}</toast>"#,
        header = e(&toast.header),
        title = e(&toast.title),
        line = e(line),
        attribution = e(&toast.attribution),
        terminal = e(&toast.button_terminal),
        show = e(&toast.button_show),
    )
}

/// Zeichen der Vorschau in der Benachrichtigung
const SNIPPET_CHARS: usize = 160;

/// Letzte Antwort bzw. letzter Werkzeugaufruf von Claude, einzeilig und
/// gekürzt; Markdown-Zeichen entfernt.
pub fn snippet_of(items: &[PreviewItem]) -> Option<String> {
    let last = items.iter().rev().find(|i| i.role != PreviewRole::User)?;
    let plain: String = last
        .text
        .chars()
        .filter(|c| !matches!(c, '`' | '*' | '#'))
        .collect();
    let flat = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    Some(if flat.chars().count() > SNIPPET_CHARS {
        let cut: String = flat.chars().take(SNIPPET_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        flat
    })
}

/// Zeigt eine Benachrichtigung; `on_click` läuft beim Klick darauf (in
/// einem fremden Thread). Außerhalb von Windows ohne Wirkung.
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn show_notification(toast: Toast, on_click: impl Fn(ToastClick) + Send + 'static) {
    #[cfg(windows)]
    win::show(toast, on_click);
}

#[cfg(windows)]
mod win {
    use super::{click_of, toast_xml, Toast, ToastClick};
    use windows::core::{IInspectable, Interface, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{
        ToastActivatedEventArgs, ToastNotification, ToastNotificationManager,
    };

    /// Eigene App-ID für Toasts (unter HKCU registriert, damit Windows
    /// Name und Icon kennt); sonst die von PowerShell.
    const APP_ID: &str = "TimAltmann.GitManager";
    const POWERSHELL_APP_ID: &str =
        "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";

    fn app_id() -> &'static str {
        static REGISTERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *REGISTERED.get_or_init(register) {
            APP_ID
        } else {
            POWERSHELL_APP_ID
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

    fn data_dir() -> Option<std::path::PathBuf> {
        let dir = directories::ProjectDirs::from("com", "gitmanager", "gitmanager")?
            .data_local_dir()
            .to_path_buf();
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn icon_path() -> Option<std::path::PathBuf> {
        let path = data_dir()?.join("notify_icon.png");
        if !path.is_file() {
            std::fs::write(&path, include_bytes!("../assets/icon.png")).ok()?;
        }
        Some(path)
    }

    pub fn show(toast: Toast, on_click: impl Fn(ToastClick) + Send + 'static) {
        // WinRT-Aufrufe und Rendern des Logos nicht im UI-Thread
        std::thread::spawn(move || {
            if let Err(e) = show_now(&toast, on_click) {
                eprintln!("Benachrichtigung fehlgeschlagen: {e}");
            }
        });
    }

    fn show_now(
        toast: &Toast,
        on_click: impl Fn(ToastClick) + Send + 'static,
    ) -> windows::core::Result<()> {
        let logo = data_dir()
            .and_then(|d| crate::claude_toast_art::toast_logo(&d.join("toast"), toast.kind));
        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(toast_xml(toast, logo.as_deref())))?;
        let notification = ToastNotification::CreateToastNotification(&xml)?;
        notification.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, args| {
                let args: &Option<IInspectable> = &args;
                let action = args
                    .as_ref()
                    .and_then(|a| a.cast::<ToastActivatedEventArgs>().ok())
                    .and_then(|a| a.Arguments().ok())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                on_click(click_of(&action));
                Ok(())
            },
        ))?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id()))?
            .Show(&notification)
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
        assert_eq!(n[0].busy_ms, None, "ohne Zeitpunkte keine Dauer");
    }

    #[test]
    fn busy_duration_from_status_timestamps() {
        let mut busy = live("a", LiveStatus::Busy);
        busy.status_since_ms = 1_000_000;
        let mut done = live("a", LiveStatus::Idle);
        done.status_since_ms = 1_252_000;
        let n = transitions(&[busy], &[done]);
        assert_eq!(n[0].busy_ms, Some(252_000));
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

    fn item(role: PreviewRole, text: &str) -> PreviewItem {
        PreviewItem {
            role,
            text: text.to_string(),
        }
    }

    #[test]
    fn snippet_takes_last_claude_entry_without_markdown() {
        let items = [
            item(PreviewRole::Assistant, "alt"),
            item(PreviewRole::Tool, "Edit src/x.rs"),
            item(
                PreviewRole::Assistant,
                "## Fertig\n\n**Tests** laufen in `cargo test`",
            ),
            item(PreviewRole::User, "danke"),
        ];
        assert_eq!(
            snippet_of(&items).as_deref(),
            Some("Fertig Tests laufen in cargo test")
        );
        assert_eq!(snippet_of(&[item(PreviewRole::User, "nur ich")]), None);
    }

    #[test]
    fn snippet_is_shortened() {
        let long = "wort ".repeat(100);
        let s = snippet_of(&[item(PreviewRole::Assistant, &long)]).unwrap();
        assert!(s.ends_with('…'));
        assert!(s.chars().count() <= SNIPPET_CHARS + 1);
    }

    #[test]
    fn click_maps_actions() {
        assert_eq!(click_of(""), ToastClick::Terminal);
        assert_eq!(click_of(ACTION_TERMINAL), ToastClick::Terminal);
        assert_eq!(click_of(ACTION_SHOW), ToastClick::Show);
    }

    fn toast(kind: NotifyKind) -> Toast {
        Toast {
            kind,
            header: "Claude Sessions".into(),
            title: "Login <fixen> & \"mehr\"".into(),
            snippet: None,
            status: "Claude ist fertig".into(),
            attribution: "fertig nach 4m 12s · repo · main".into(),
            button_terminal: "Terminal öffnen".into(),
            button_show: "In GitManager anzeigen".into(),
        }
    }

    #[test]
    fn toast_xml_has_header_attribution_and_escapes() {
        let xml = toast_xml(
            &toast(NotifyKind::Done),
            Some(Path::new("C:\\a&b\\logo.png")),
        );
        assert!(xml.contains(
            r#"<header id="claude-sessions" title="Claude Sessions" arguments="show"/>"#
        ));
        assert!(xml.contains("<text>Login &lt;fixen&gt; &amp; &quot;mehr&quot;</text>"));
        assert!(
            xml.contains("<text>Claude ist fertig</text>"),
            "ohne Vorschau der Status"
        );
        assert!(xml
            .contains(r#"<text placement="attribution">fertig nach 4m 12s · repo · main</text>"#));
        assert!(xml.contains(r#"hint-crop="circle" src="file:///C:\a&amp;b\logo.png""#));
        assert!(xml.contains(r#"arguments="terminal" hint-buttonStyle="Success""#));
        assert!(!xml.contains("<audio"), "fertig: Standardton");
        assert!(
            roxmltree::Document::parse(&xml).is_ok(),
            "gültiges XML: {xml}"
        );
    }

    #[test]
    fn toast_xml_waiting_uses_snippet_reminder_and_plain_button() {
        let mut t = toast(NotifyKind::Waiting);
        t.snippet = Some("Darf ich cargo test ausführen?".into());
        let xml = toast_xml(&t, None);
        assert!(xml.contains("<text>Darf ich cargo test ausführen?</text>"));
        assert!(!xml.contains("hint-buttonStyle"));
        assert!(!xml.contains("<image"));
        assert!(xml.contains("Notification.Reminder"));
        assert!(roxmltree::Document::parse(&xml).is_ok());
    }
}
