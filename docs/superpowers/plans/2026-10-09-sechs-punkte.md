# Sechs Punkte Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 6 User-Punkte fixen (Fetch, Autostart, Icons, Sortierung, Veraltet-Icon, Tray-Suche)

**Architecture:** Incremental fixes auf bestehender egui+git2 Architektur, Config v10→11 Migration, MRU-Reuse.

**Tech Stack:** Rust, eframe/egui 0.36, git2 0.21, serde, winreg (neu, windows-only)

**Spec:** User-Antworten 2026-10-09: Autostart=Registry Run-Key, kein Auto-Pull sondern Fetch+Veraltet-Icon, Sort-Default=Zuletzt genutzt, Icons=Tint für Phosphor, Tray=Branch-Filter pro Dropdown.

## Global Constraints

- `cargo fmt --check && cargo clippy -- -D warnings && cargo test --verbose` muss grün sein (Agent.md DoD).
- Windows-Build `cargo build --release --target x86_64-pc-windows-gnu` muss grün sein bei tray/windows/config-Änderungen.
- Config-Migration einmalig v10→11, alle neuen Felder `#[serde(default)]`.
- i18n En/De für jeden neuen UI-String, Coverage-Testliste in `src/i18n.rs` ergänzen.
- Keine Pushes (User-Vorgabe), Branch `feature/sechs-punkte-fetch-autostart-icons-sort-tray`.
- TDD wo sinnvoll: erst failing Test, dann Implementierung.

---

### Task 1: Git fetch Fix (Fehler anzeigen, Multi-Remote, UX)

**Files:**
- Modify: `src/git.rs:249-294` (`fetch_all`)
- Modify: `src/app.rs:1640-1659` (fetch handler)
- Modify: `src/i18n.rs` (neue Keys)
- Test: bestehende `src/git.rs` Tests + neuer UI-Error-Pfad Test

**Interfaces:**
- Consumes: `fetch_all(path: &Path) -> anyhow::Result<()>`
- Produces: ehrliche Fehler-Semantik (Err wenn alle Remotes fehlschlagen), UI zeigt Fehler in `self.error`/Status statt `eprintln!`.

- [ ] Step 1: Failing Test für Multi-Remote-Masking schreiben (`>1` Remote, einer fails → aktuell Ok, soll Err/Warn sein; sowie Single-Remote Err bleibt).
- [ ] Step 2: `fetch_all` fixen: Fehler pro Remote sammeln, `Ok` nur wenn mind. 1 ok oder keine Remotes, sonst `Err` mit Remote-Namen + Hinweis auf Auth (Credentials/SSH). `eprintln!` behalten als Log, aber Rückgabe ehrlich.
- [ ] Step 3: `app.rs` Handler: `fetch_res` Fehler in `self.error` + Status-Bar anzeigen (neue Keys `fetch_failed`, `fetch_partial`), Spinner/Disabled während Fetch.
- [ ] Step 4: i18n En/De + Coverage-Test ergänzen.
- [ ] Step 5: `cargo test`, `cargo clippy`, `cargo fmt` grün.
- [ ] Step 6: Commit `fix: git fetch Fehler ehrlich + UI-Anzeige`.

### Task 2: Autostart Registry + Einstellung

**Files:**
- Modify: `Cargo.toml` (winreg windows-only)
- Modify: `src/config.rs` (Feld `autostart: bool`, Default false, Migration v11)
- Modify: `src/ui/settings.rs` (`show_general_tab`, Checkbox neben minimize_to_tray)
- Modify: `src/i18n.rs` (`autostart_label/hint`)
- Modify: `src/app.rs` (apply nach Save + Startup-Sync)
- New: `src/autostart.rs` (windows Registry Run-Key, non-windows No-Op) oder inline in `app.rs` wenn kleiner.

**Interfaces:**
- Consumes: `AppConfig.autostart: bool`
- Produces: `apply_autostart(enabled: bool) -> Result<()>` schreibt/entfernt `HKCU\...\Run\GitManager`.

- [ ] Step 1: Config-Feld + Migration v10→11 + serde roundtrip Test.
- [ ] Step 2: `autostart.rs` mit `winreg`: `set(enabled)` + `is_enabled()`; Linux No-Op.
- [ ] Step 3: Settings-Checkbox + i18n.
- [ ] Step 4: Apply nach Save + beim Start syncen (falls exe verschoben).
- [ ] Step 5: CI-Stages grün + Windows-Build.
- [ ] Step 6: Commit `feat: autostart via Registry Run-Key`.

### Task 3: Icon Light/Dark Tint Phosphor

**Files:**
- Modify: `src/app.rs:1027-1047` (Top-Bar GEAR/REFRESH tint)
- Modify: `src/ui/repo_list.rs` (Suche, Refresh, Chevron, Folder tint)
- Modify: `src/ui/tray_popup.rs:196-222,569-586` (Header + Tools tint)
- Modify: `src/ui/settings.rs` (Eye/Chevron/Trash tint wo Phosphor)
- Kein Anfassen: Brand-Icons (vscode, rider, claude...) bleiben unverändert.

**Interfaces:**
- Consumes: `ui.visuals().text_color()` / `weak_text_color()`
- Produces: alle Phosphor-`Image` mit `.tint(...)`.

- [ ] Step 1: Audit-Liste aller Phosphor-Verwendungen ohne tint.
- [ ] Step 2: `.tint()` ergänzen (aktiv=text_color, inaktiv=weak).
- [ ] Step 3: Manuell in 5 Themes prüfen (Light/Dark/Nord/Dracula/Solarized).
- [ ] Step 4: fmt/clippy/test grün.
- [ ] Step 5: Commit `fix: phosphor icons theme-aware tint`.

### Task 4: Sortierung letzte Nutzung + Icon oben rechts

**Files:**
- Modify: `src/config.rs` (Enum `RepoSortMode { Name, LastUsed }` + Feld `main_sort_mode`, Default LastUsed, Migration v11 zusammen mit Task 2 — falls Task 2 zuerst gemergt, hier v11→12; Koordination nötig)
- Modify: `src/ui/repo_list.rs:129-163` + `src/app.rs:1464` (Sort vor Render, nicht in scanner.rs)
- Modify: `src/app.rs:1025-1054` Top-Bar (Sort-Icon-Button + Dropdown)
- Modify: `src/ui/settings.rs` General-Tab (ComboBox Spiegel)
- Modify: `src/i18n.rs` (`sort_by_name/sort_by_last_used/sort_tooltip`)
- Reuse: `mru_timestamp` Logik aus `tray_popup.rs:669-680`, `repo_state_key` Normalisierung.

**Interfaces:**
- Consumes: `repo_usage: HashMap<String,RepoUsage>`
- Produces: `sorted_main_repos()` — LastUsed desc, nie-genutzte alphabetisch am Ende.

- [ ] Step 1: Config Enum + Feld + Migration + Test.
- [ ] Step 2: Sort-Funktion + Unit-Test (MRU desc, tie=name).
- [ ] Step 3: Top-Bar Button + Settings ComboBox + i18n.
- [ ] Step 4: CI grün.
- [ ] Step 5: Commit `feat: main sort by last used + topbar toggle`.

### Task 5: Fetch-Intervall + Veraltet-Icon statt Auto-Pull

**Files:**
- Modify: `src/git.rs` (neu `ahead_behind(path) -> (usize,usize)` via `graph_ahead_behind`, Upstream=`branch_upstream` oder `origin/<branch>`)
- Modify: `src/git.rs:RepoInfo` + `src/scanner.rs:get_repo_info` (Felder `ahead/behind`, Default 0)
- Modify: `src/ui/repo_list.rs:show_repo_row` (Warn-Icon wenn `behind>0`, Tooltip `branch_behind`)
- Modify: `src/config.rs` (`auto_fetch_interval_min: u64`, Default 10, 0=aus, Migration v11)
- Modify: `src/app.rs` (Hintergrund-Fetch throttled, Pattern `should_sync 381-393`)
- Modify: `src/i18n.rs` (`branch_behind/auto_fetch_*`)

**Interfaces:**
- Consumes: Fix aus Task 1 (`fetch_all` ehrlich)
- Produces: `ahead_behind()`, RepoInfo mit behind-Count, periodischer Fetch.

- [ ] Step 1: `ahead_behind` + Test mit 2 Clones (1 Commit voraus).
- [ ] Step 2: RepoInfo-Felder + Scanner füllen.
- [ ] Step 3: Veraltet-Icon + Tooltip in repo_list.
- [ ] Step 4: Auto-Fetch Intervall Config + Thread (nur sichtbare Repos, throttled).
- [ ] Step 5: CI grün + Windows-Build.
- [ ] Step 6: Commit `feat: behind-indicator + periodic fetch`.

### Task 6: Tray Branch-Suche

**Files:**
- Modify: `src/ui/tray_popup.rs:409-440` (TextEdit Filter in ComboBox)
- Modify: `src/tray_service.rs` oder `TrayShared` (Filter-State `HashMap<PathBuf,String>`, geleert bei Close)
- Reuse: `filter_branches()` aus `repo_list.rs` (pub machen)
- Modify: `src/i18n.rs` (`branch_search` Placeholder existiert evtl., sonst neu)

**Interfaces:**
- Consumes: `filter_branches(branches, filter, limit)`
- Produces: pro-Repo Branch-Filter im Tray, Popup bleibt offen bei Auswahl (bestehend).

- [ ] Step 1: `filter_branches` pub + Reuse.
- [ ] Step 2: Filter-State + TextEdit oben in ComboBox + `+N weitere (filtere)` Hinweis.
- [ ] Step 3: Manuell im Tray-Popup prüfen (Suche, Limit, Auswahl bleibt offen).
- [ ] Step 4: CI grün + Windows-Build.
- [ ] Step 5: Commit `feat: tray branch search filter`.
