# Refactoring-Bericht: Dubletten entfernen & Code vereinfachen

## Was getan wurde (alle verifiziert)

### 1. Icon-Helfer zentralisiert (src/ui/icons.rs)
Die öffentlichen Helfer in `icons.rs` waren tot (6 `dead_code`-Warnings), während lokale Kopien andernorts genutzt wurden.

**Entfernte Dubletten:**
| Datei | Entfernt |
|---|---|
| `src/ui/tray_popup.rs` | `icon_exists_cached`, `ide_image`, `agent_image` |
| `src/ui/repo_list.rs` | `ide_image`, `agent_image` |
| `src/ui/merge_requests.rs` | privates `icon_image` (24 Aufrufer) |
| `src/ui/settings/merge_requests.rs` | privates `icon_image` (7 Aufrufer) |

**Caller umgestellt:** `claude_sessions.rs`, `tray_popup.rs` (3 Call-Stellen), `repo_card_chips.rs`.

### 2. Escape-Funktionen konsolidiert (src/git.rs)
Drei fast identische Funktionen → eine `shell_escape`-Kernfunktion + dünne Wrapper:
- `escape_for_powershell` → `shell_escape(s, '\'', "''", &[...meta...], false)`
- `escape_for_powershell_force` → `shell_escape(s, '\'', "''", &[], true)`
- `escape_for_cmd` → `shell_escape(s, '"', "\"\"", &[...meta...], false)`

### 3. Spawn-Funktionen konsolidiert (src/git.rs)
Sechs Spawn-Funktionen mit wiederholtem Boilerplate (`current_dir`/`set_creation_flags`/`spawn_locked`/Fehlermeldung) → gemeinsame `spawn_finish`-Hilfsfunktion:
- `spawn_wt`, `spawn_powershell`, `spawn_powershell_shell`, `spawn_wt_shell`, `spawn_cmd_shell`, `spawn_cmd`

### 4. Code-Reduktion
Insgesamt: **92 Zeilen hinzugefügt, 153 Zeilen entfernt** (netto -61 Zeilen)

## Verifikation
- `cargo build`: 0 Warnungen, 0 Fehler
- `cargo test`: **1007 passed, 0 failed**

## Geprüft, bewusst NICHT zusammengeführt

| Kandidat | Grund |
|---|---|
| `path_key` (claude_sessions.rs vs gitlab/checkout.rs) | Unterschiedliche Semantik (string-based/gitbash-aware vs Path-based/repo_state_key) |
| `folder_name` (claude_live.rs vs ui/merge_requests.rs) | Unterschiedliche Signaturen (`&str` vs `&Path`), leicht unterschiedliches Verhalten |
| cfg-Paare (`with_schemas`, `fresh_path`, `process_alive`, `focus_session_window`) | Absichtliche plattform-/test-spezifische Varianten |
| `open_ide_in`/`open_agent`/`open_explorer`/`open_shell` (app.rs) | Ähnliches Muster (status_message + record_usage), aber unterschiedliche Logik |
| `resolve_vs_path`/`resolve_rider_path` (git.rs) | Unterschiedliche Auflösungs-Logik (vswhere vs. Toolbox-Verzeichnis) |
| `small`/`unit`/`wrapped`/`icon_button` | Private UI-Helfer pro Modul |

## Offene Kandidaten für einen weiteren Durchgang
- `src/git.rs` (7140 Zeilen): `resolve_vs_path_uncached`/`resolve_rider_path_uncached` sind ähnlich strukturiert, aber unterschiedlich genug
- `src/app.rs` (4424 Zeilen): `poll_scan` (~224 Zeilen), `show_top_bar` (~364 Zeilen) sind lang, aber keine Dubletten
- `src/ui/merge_requests.rs` (4558 Zeilen): `show_row`/`show_merged_row`, `mine_*`/`others_*` Parallelen sind UI-spezifisch

## Empfehlung
Die offensichtlichsten Dubletten sind entfernt. Weitere Refactoring-Schritte wären eher "Funktionen aufteilen" als "Dubletten entfernen" – das sollte gezielt pro Modul angegangen werden.