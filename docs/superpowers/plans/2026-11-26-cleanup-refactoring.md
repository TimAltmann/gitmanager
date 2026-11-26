# Cleanup & Refactoring – Plan (Branch `refactor/icons-and-deduplication`)

Datum: 2026-11-26 · Freigegeben: Ansatz 2 (strukturell mitziehen, verhaltensneutral)

## Rahmen
- Keine Verhaltensänderung. Keine Änderung an `config::try_load()`-Save-Semantik
  und kein roxmltree-Umbau in `config_parser.rs` (bewusst ausgeschlossen).
- Keine Angleichung der Tray-/Hauptfenster-Unterschiede (Punkt 3, nicht freigegeben).
- Nach jedem Batch: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.

## Batch 1 – echte Leichen / wirkungslose Guards ✅ (commit d3c557f)
- [x] `app.rs`: `tray_branch_switch_opens_main_window()` + toter `if` + Test entfernen
- [x] `autostart.rs`: `is_enabled()` (inkl. Test) entfernen, `allow(dead_code)` auf `cfg_attr` präzisieren
- [x] `config.rs`: unerreichbare/leere Blöcke (~1278, ~1573, ~1693) entfernen
- [x] `ui/repo_list.rs`: No-Op-Block (~657) entfernen
- [x] `ui/icons.rs`: `ICON_MR_DOTS` entfernen, Alias in `ui/merge_requests.rs` auflösen
- [x] `git.rs`: `has_placeholders` + wirkungslose Guards entfernen
- [x] `git.rs`: `clear_vs_cache`/`clear_rider_cache` privatisieren

## Batch 2 – vorhandene Helfer konsequent nutzen
- [ ] `app.rs`/`app/*`: `config.save()`-Fehler über `note_config_save_error()`/Helfer
- [ ] `app.rs`: `git::is_vs_program`/`is_rider_program` statt lokaler Listen
- [ ] `ui/settings.rs`: `ide_icon_for`/`agent_icon_for` statt 9× `include_image!`
- [ ] `ui/repo_card_chips.rs`: `icons::icon_image` statt lokalem `icon()`
- [ ] `ui/merge_requests.rs` + `ui/settings/merge_requests.rs`: `small()`/Icon-Button vereinen
- [ ] XML-Escape in `config_parser.rs` + `claude_notify.rs` zentralisieren; Logo-XML einmal bauen
- [ ] `app/repo_badges.rs`: `LiveStatus::rank` nutzen
- [ ] `gitlab/checkout.rs`: `config::absolute_dir` nutzen
- [ ] `gitlab/service.rs`: `with_schema`-Helfer

## Batch 3 – klare strukturelle Extraktionen
- [ ] `i18n.rs`: 50 identische DE/EN-Paare zusammenführen, `.to_string()` einmal
- [ ] `git.rs`: `worktree_locations()`-Iterator für drei Enumerationen
- [ ] `git.rs`: `branch_upstream()` für doppelte Config-Leselogik
- [ ] `git.rs`: gemeinsamer Kern für `build_agent_command`/`build_agent_cmd_raw`
- [ ] `git.rs`: gemeinsame PowerShell-Command-Basis
- [ ] `app.rs`: `apply_scan_intake()` für `poll_scan`-Arme
- [ ] `app/*`: `WorkerChan<T>` für sechs Default-/Poll-Blöcke
- [ ] `ui/settings.rs`: `icon_picker_row()` (~95 Zeilen)
- [ ] `app.rs`: `refuse_while_busy()` für sechs Aufrufer
- [ ] `app/claude_sessions.rs`: `spawn_agent()` für drei Pfade
