//! Autostart via Windows Registry Run-Key (`HKCU\...\Run\GitManager`).
//! Non-Windows: No-Op, damit Config + Settings plattformunabhängig bleiben.

use anyhow::Result;

#[allow(dead_code)]
pub const RUN_VALUE_NAME: &str = "GitManager";
#[allow(dead_code)]
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Schreibt (enabled) oder entfernt (!enabled) den Run-Key-Eintrag.
pub fn apply_autostart(enabled: bool) -> Result<()> {
    set_autostart(enabled)
}

/// Alias für `apply_autostart` — explizites Setzen.
pub fn set_autostart(enabled: bool) -> Result<()> {
    imp::set(enabled)
}

/// Meldet, ob der Run-Key-Eintrag aktuell gesetzt ist.
#[allow(dead_code)]
pub fn is_enabled() -> bool {
    imp::is_enabled()
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{RUN_SUBKEY, RUN_VALUE_NAME};
    use anyhow::{Context, Result};

    fn run_key() -> Result<winreg::RegKey> {
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_SUBKEY, winreg::enums::KEY_SET_VALUE)
            .or_else(|_| {
                winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
                    .create_subkey(RUN_SUBKEY)
                    .map(|(key, _)| key)
            })
            .context("Run-Key öffnen/erstellen fehlgeschlagen")
    }

    fn run_key_read() -> Result<winreg::RegKey> {
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey(RUN_SUBKEY)
            .context("Run-Key öffnen fehlgeschlagen")
    }

    /// Erwarteter Run-Value: exe in Quotes + --minimized (Tray-Start ohne
    /// volles Fenster, M1). Legacy-Werte ohne Flag werden beim nächsten
    /// Start einmalig migriert, danach kein Rewrite mehr.
    fn expected_value() -> Result<String> {
        let exe = std::env::current_exe().context("current_exe ermitteln fehlgeschlagen")?;
        Ok(format!("\"{}\" --minimized", exe.display()))
    }

    pub fn set(enabled: bool) -> Result<()> {
        if enabled {
            let want = expected_value()?;
            // M1: kein Rewrite bei jedem Start — nur schreiben wenn abweichend.
            if let Ok(key) = run_key_read() {
                if let Ok(cur) = key.get_value::<String, _>(RUN_VALUE_NAME) {
                    if cur == want {
                        return Ok(());
                    }
                }
            }
            let key = run_key()?;
            key.set_value(RUN_VALUE_NAME, &want)
                .context("Run-Key schreiben fehlgeschlagen")?;
        } else {
            // M1: kein Delete-Versuch wenn bereits aus.
            if !is_enabled() {
                return Ok(());
            }
            let key = run_key()?;
            match key.delete_value(RUN_VALUE_NAME) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e).context("Run-Key entfernen fehlgeschlagen"),
            }
        }
        Ok(())
    }

    pub fn is_enabled() -> bool {
        run_key_read()
            .and_then(|key| {
                let _: String = key
                    .get_value(RUN_VALUE_NAME)
                    .context("Run-Value lesen fehlgeschlagen")?;
                Ok(())
            })
            .is_ok()
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use anyhow::Result;

    pub fn set(_enabled: bool) -> Result<()> {
        Ok(())
    }

    pub fn is_enabled() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_value_name_is_gitmanager() {
        assert_eq!(RUN_VALUE_NAME, "GitManager");
    }

    #[test]
    fn run_subkey_path() {
        assert_eq!(RUN_SUBKEY, r"Software\Microsoft\Windows\CurrentVersion\Run");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn noop_apply_ok_and_disabled() {
        assert!(apply_autostart(true).is_ok());
        assert!(apply_autostart(false).is_ok());
        assert!(!is_enabled());
    }
}
