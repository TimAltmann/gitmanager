# Tests_4 — Testplan für RepoManager

Dieses Dokument beschreibt geplante Tests (Schritt 1) für die Module/Klassen des RepoManager.
Es werden **nur** Testmethoden mit kurzer Beschreibung sowie ihre Edge Cases dokumentiert —
**keine Implementierung**. Die Umsetzung erfolgt in Schritt 2 als `#[test]`-Blöcke in den
jeweiligen Modulen (`src/*.rs`).

## Konventionen für die Umsetzung

- Sprache/Tooling: Rust `#[test]`-Blöcke, gestartet mit `cargo test`.
- Für temporäre Verzeichnisse wird die bereits vorhandene Dev-Dependency `tempfile` (`tempdir`) genutzt.
- Für Tests, die echte Git-Repos brauchen, werden die vorhandenen Helfer
  `init_repo_with_commit(dir, branch)` (in `git.rs`) bzw. `init_repo(path)` (in `scanner.rs`) wiederverwendet.
- Empfehlung: je Modul ein `#[cfg(test)] mod tests` am Dateiende.

---

## Modul `config.rs` (ausführlich)

### `Theme`
- `theme_display_names`: `display_name()` liefert für jedes Theme den passenden Anzeigenamen
  (Light → "Light", Dark → "Dark", Nord → "Nord", Dracula → "Dracula", Solarized → "Solarized Light").
- `theme_all_lists_five`: `all()` enthält genau die 5 Themes.
- `theme_default_is_light`: `Theme::default()` ist `Light`.
- `theme_serde_snake_case`: Serialisierung nutzt `snake_case` (z. B. `Theme::Dark` ↔ `"dark"`), Roundtrip konsistent.

### `TerminalPreference` / `TerminalConfig`
- `terminal_defaults`: Default-Preference ist `Auto`, Fallback ist `Cmd`.
- `terminal_custom_serde_roundtrip`: `TerminalPreference::Custom(String)` überlebt serde-Roundtrip (mit Inhalt).

### `IdeConfig::effective_program` / `effective_args`
- `ide_effective_from_program`: nur `program` gesetzt → `effective_program()` liefert program; `effective_args()` liefert args.
- `ide_effective_from_command_program`: `program` leer + `command` gesetzt → erstes Wort von command wird program.
- `ide_effective_from_command_args`: `command` mit mehreren Wörtern → Rest-Wörter werden args.
- `ide_effective_command_single_word`: `command` mit nur einem Wort → args fallen auf `["{file}"]` zurück.
- `ide_effective_no_config`: weder `program` noch `command` → program Fallback `"code"`, args `["{file}"]`.
- `ide_args_take_priority`: wenn `args` gesetzt sind, hat `command` für `effective_args()` keine Wirkung (arm-priority).

### `LanguageProfile`
- `profile_normalized_extension_adds_dot`: `"sln"` → `".sln"`.
- `profile_normalized_extension_keeps_dot`: `".sln"` bleibt `".sln"`.
- `profile_normalized_extension_lowercases`: `".SLN"` / `"SLN"` → `".sln"`.
- `profile_matches_wildcard_ext`: Pattern `*.sln` matcht `foo.sln` und `FOO.SLN`, aber nicht `foo.txt`.
- `profile_matches_exact_filename`: Pattern `Cargo.toml` matcht `Cargo.toml`, aber nicht `foo.toml`.
- `profile_matches_comma_patterns`: kommagetrenntes Pattern (`pom.xml,build.gradle`) matcht jeden Namen.
- `profile_matches_no_pattern`: ohne Pattern reiner Endungsvergleich.
- `profile_matches_file_without_ext`: Datei ohne Extension liefert `false`.
- `dotnet_default_profile_shape`: Default-Profil `.NET` hat ID `dotnet`, IDEs (vs2022/vscode/rider) und `default_ide_id`.

### `AgentLaunchMode` / `AgentProfile`
- `agent_launch_mode_default`: `AgentLaunchMode::default()` ist `Terminal`.
- `default_agents_include_claude`: `default_agents()` enthält u. a. `claude`.

### `AppConfig` (Kernstück)
- `default_config_shape`: `Default` hat `max_depth=2`, `config_version=4`, `active_profile_id="dotnet"`, nicht-leere `profiles`/`agents`.
- `add_root_adds_and_sorts`: `add_root` fügt Pfad hinzu und sortiert die Liste.
- `add_root_dedups`: doppelter Pfad wird nicht erneut eingefügt.
- `remove_root_removes`: `remove_root` entfernt genau den übergebenen Pfad (andere bleiben).
- `get_active_profile_found`: liefert das Profil zur `active_profile_id`.
- `get_active_profile_fallback`: bei ungültiger ID Fallback auf das erste Profil.
- `profile_effective_no_override`: ohne Override liefert `get_effective_profile_for_repo` das aktive Profil.
- `profile_effective_valid_override`: mit gültigem per-Repo-Override wird das Override-Profil geliefert.
- `profile_effective_invalid_override`: mit ungültigem Override-ID fällt es auf das aktive Profil zurück.
- `repo_state_mut_creates_default`: `get_repo_state_mut` erzeugt einen Default-State für unbekannten Pfad.
- `set_profile_override_persists`: `set_repo_profile_override(Some(id))` setzt den Override; mit `None` wird er geleert.
- `migration_old_config_injects_defaults`: alte JSON (nur `roots`/`max_depth`) → Defaults werden injiziert, Version hochgesetzt, `dotnet` vorhanden.
- `migration_v2_to_v3_adds_defaults`: fehlende Default-Profile/-Agents werden bei Version < 3 ergänzt.
- `migration_v3_to_v4_removes_auto_profiles`: alte Auto-Profile (rust/node/python/java/go/cpp) werden entfernt, `dotnet` + custom bleiben; `active_agent_id` → `active_agent_ids` migriert.
- `validation_clamps_max_depth_zero`: `max_depth == 0` → 2.
- `validation_clamps_max_depth_high`: `max_depth > 10` → 10.
- `validation_roots_normalized`: Roots werden sortiert, dedupliziert, leere Einträge entfernt.
- `validation_profile_duplicates_dedup`: Profil-Duplikate (gleiche ID) werden entfernt (letztes gewinnt).
- `validation_active_profile_bowel`: ungültige `active_profile_id` wird auf gültiges Profil korrigiert.
- `save_load_roundtrip`: nach `save()` und `load()` auf Temp-Datei sind die Werte (roots, depth, version) identisch.
- `config_path_returns_some`: `AppConfig::config_path()` liefert einen Pfad (mit `config.json`).

> Hinweis: `default_root_path()`/`dirs_home()` hängen am Home-Verzeichnis und sind nicht deterministisch testbar
> → bewusst nicht in den Unit-Testplan aufgenommen (manuell/Umgang).

---

## Modul `scanner.rs` (ausführlich)

### `is_ignored_dir`
- `ignored_dir_known`: bekannte Namen (`node_modules`, `target`, `.cargo`, `dist`, …) → `true`.
- `ignored_dir_unknown`: andere Namen (z. B. `src`, `myapp`) → `false`.

### `scan_solutions_for_repo`
- `solutions_finds_matches`: findet gematchte Dateien gemäß Profil (`.sln` mit Pattern `*.sln`).
- `solutions_respects_depth`: `max_scan_depth` wird respektiert (Cap `min(max, 4)`); tiefere Dateien nicht gefunden.
- `solutions_skips_git_and_ignored`: `.git` und ignorierte Verzeichnisse werden nicht durchsucht.
- `solutions_relative_paths`: `relative`-Pfade sind korrekt relativ zum Repo.
- `solutions_cap_at_20`: mehr als 20 Lösungen werden auf 20 gekappt.

### `scan_repos`
- `scan_finds_repos_at_depth_1`: findet direkte Repos bei `max_depth=1`.
- `scan_respects_max_depth`: Tiefe 1 findet Enkel-Repo nicht, Tiefe 2 schon.
- `scan_ignores_non_repos`: Verzeichnisse ohne `.git` werden ignoriert.
- `scan_dedup_duplicate_roots`: mehrere gleiche Root-Einträge → Repo nur einmal.
- `scan_skips_missing_root`: nicht existierender Root wird übersprungen (kein Panic), Ergebnis leer.
- `scan_skips_non_dir_root`: Root der eine Datei ist → übersprungen, kein Panic.
- `scan_sorted_by_name`: Ergebnisse sind (case-insensitiv) nach Name sortiert.
- `scan_root_itself_is_repo`: wenn der Root selbst ein Repo ist, wird es gefunden.
- `scan_restores_selected_solution`: `selected_solution` wird aus dem `repo_state` wiederhergestellt (falls in solutions enthalten).

---

## Modul `git.rs` (ausführlich)

### `parse_head_file` (pure)
- `head_parse_ref_branch`: `ref: refs/heads/main` → `Some("main")`.
- `head_parse_ref_nested`: `ref: refs/heads/feature/x` → `Some("feature/x")`.
- `head_parse_ref_other_prefix`: anderes `ref:`-Präfix → gestrippter String.
- `head_parse_detached_sha`: abgesetzter SHA (≥ 7 Zeichen) → 7-Zeichen-Kurzform.
- `head_parse_invalid_or_empty`: ungültig/zu kurz → `None`.

### `RepoInfo::new` / `with_branches` / `get_repo_info`
- `repo_info_name_from_file_name`: `name` stammt aus `file_name`.
- `repo_info_name_fallback`: ohne `file_name` → Pfad-Darstellung als Name.
- `repo_info_fields`: branch/dirty/detached korrekt im Konstruktor gesetzt.
- `repo_info_with_branches`: `with_branches` setzt die Branches-Liste.
- `get_repo_info_none_for_non_repo`: kein Repo → `None`.
- `get_repo_info_some_for_repo`: Repo → `Some` mit nicht-leerem branch.

### `substitute_placeholders` / `quote_if_needed` (pure)
- `substitute_all_placeholders`: ersetzt `{file}`, `{dir}` (Parent), `{repo}`.
- `quote_needed_with_spaces`: String mit Leerzeichen ohne Quotes → wird gequotet.
- `quote_no_space`: String ohne Leerzeichen → unverändert.
- `quote_already_quoted`: bereits gequoteter String → unverändert.

### `get_branch`
- `branch_detection`: normaler Branch → Name, `is_detached == false`.
- `branch_detached`: detach → Kurz-SHA, `is_detached == true`.

### `is_dirty`
- `dirty_clean_after_commit`: frisch committet → `false`.
- `dirty_modified`: nach Modifikation → `true`.
- `dirty_untracked`: untracked-Datei → `true`.

### `get_detailed_status`
- `detailed_status_classifies_flags`: modified → `modified: path`, untracked → `untracked: path`, staged → `staged: path`.

### `has_merge_conflicts` / `is_merge_in_progress`
- `no_conflicts_clean` / `not_in_merge_clean`: sauberes Repo → beide `false`.

### `list_branches`
- `list_branches_local_and_remote`: lokale + remote Branches enthalten.
- `list_branches_excludes_origin_head`: `origin/HEAD` wird ausgeschlossen.
- `list_branches_sorted_dedup`: sortiert und dedupliziert.
- `list_branches_non_repo`: kein Repo → leere Liste.

### `checkout_branch`
- `checkout_branch_switches`: wechselt zu einem existierenden Branch und zurück.
- `checkout_rejects_missing_branch`: nicht existierender Branch → `Err`.
- `checkout_rejects_merge_in_progress`: bei Merge-in-progress → `Err`.
- `checkout_remote_creates_local_tracking`: Remote-Branch → lokaler Tracking-Branch wird angelegt.
- (dirty-Verhalten: je nach Git-Stand ok oder `Err`, kein Panic)

### `launch_ide` Sicherheits-Guard (Logik, ohne echten Prozessstart)
- `launch_ide_blocks_shell_chars`: bei `allow_unsafe=false` und Shell-Zeichen (`&`, `|`, `;`, `<`, `>`) in args → `Err`.
- `launch_ide_allows_shell_chars_if_unsafe`: mit `allow_unsafe=true` → keine Blockade durch dieses Guard.
- `launch_ide_argument_substitution`: `{file}` in args wird durch den Dateipfad ersetzt (über `substitute_placeholders`).

---

## UI-Module — Rahmen-Testbarkeit

Diese Teile sind überwiegend egui-Rendering bzw. erfordern einen egui/Harness-Kontext und sind daher
**nur manuell oder als Integrationstests** sinnvoll. Die rein logischen Anteile sind separat vermerkt.

### `ui/theme.rs`
- `color_constants`: `COLOR_DIRTY`/`COLOR_CLEAN` sind gültig gesetzt (Konstanten, trivial).
- `apply_theme`: benötigt `egui::Context` → manuell (einmal pro Theme: keine Panics, Theme wird gesetzt).

### `ui/repo_list.rs`
- `ide_icon_for`/`agent_icon_for`: Mapping IDE-/Agent-ID → Icon; unbekannte IDs auf Fallback.
  - Testbarkeit eingeschränkt, da Rückgabetyp `egui::ImageSource` (egui-Abhängigkeit) → manuell/prüfbar, sofern isolierbar.
- `show_repo_list`/`show_repo_row`: Rendering, Dropdowns, Filter, Hover → manuell (UI-Smoke-Test).

### `ui/settings.rs`
- `settings_state_from_config`: `SettingsState::from_config` initialisiert `draft` (Clone), leere Fehler/Meldungen, `selected_profile_idx`/`selected_agent_idx` (None bei leeren Listen).
  - **rein logisch, unit-testbar** (kein egui im Konstruktor).
- `settings_save_validates_empty_roots`: Speichern mit leeren Roots → Validierungsfehler "mindestens ein Pfad".
- `settings_save_validates_empty_profiles`: Speichern ohne Profile → Validierungsfehler.
- `settings_save_validates_profile_fields`: Profil mit leerer ID/Name/Endung → Validierungsfehler.
- übrige Multi-Tab-UI (DragValue, Slider, ComboBox, FileDialog) → manuell.

### `app.rs`
- `handle_branch_switch`/`execute_branch_switch`: Branch-Wechsel-Flow inkl. Dirty-Dialog, Force/Stash-Pfade → Integrationstest (echte Repos + egui) bzw. manuell.
- `start_scan`/`poll_scan`: Scan-Polling + Status-Timing → manuell/Integration.
- `show_top_bar`-Kollaps-Logik (Größen-Schwellen 400/500): einfach, aber an egui-`ctx` gebunden → manuell.

---

## Abschluss

Dies ist **Schritt 1** (Testbeschreibungen, keine Implementierung). Die eigentliche Implementierung
erfolgt in **Schritt 2** als `#[test]`-Funktionen in den Modulen. Die beschriebenen Tests decken
sowohl den geforderten Normalbetrieb als auch die relevanten Edge Cases (leere Config, Migrationen,
fehlende Dateien, Tiefen-Limits, Shell-Zeichen, Detached-HEAD, Duplikate, Overrides) ab.
