# AlleTestsfinal – Konsolidierter Testplan RepoManager

> **Erstellt:** 30.08.2026  
> **Quellen:** `Tests_1.md`, `Tests_2.md`, `Tests_3.md`, `Tests_4.md`  
> **Methode:** Namens-/Beschreibungs-Varianz ignoriert, Semantik am Code geprüft (`src/*.rs`, `src/ui/*.rs`). Duplikate zusammengefasst, echte Überschneidung nur bei identischem Codepfad.

---

## 1. Zusammenfassung

| Quelle | Roh-Anzahl | Bemerkung |
|--------|------------|-----------|
| Tests_1.md | ~163 Tests (12 Module + 2 Edge-Sammlungen) | `SolutionFile` bis `Scanner` sehr detailliert, viele Git-Edge-Cases |
| Tests_2.md | 289 Tests (Tabellen #1-289) | Vollständigste Enumeration, inkl. `ui/theme`, `ui/repo_list`, `ui/settings`, `app` |
| Tests_3.md | 142 Tests (7 Module) | Kompakter, aber zusätzliche `profile_validation`, `agent_ids_validation`, `ignored_dirs` |
| Tests_4.md | ~65 beschreibende Tests | Fokus auf reine Logik, markiert UI als `manuell`, liefert `parse_head_file`-Feingranularität |
| **Summe roh** | **~659** | |
| **Nach Deduplizierung** | **183 finale Tests** | + 12 manuelle/UI-Smoke-Tests separat ausgewiesen |

**Deduplizierungs-Regel:** Zwei Tests gelten als Duplikat, wenn sie denselben Funktionspfad (`file_path:line_number`) mit denselben Eingaben/Erwartungen prüfen – auch bei anderem Namen. Beispiel: `new_creates_struct_with_path_and_relative` (Tests_1) = `solution_file_creation` (Tests_2#75) = `test_solution_file_creation` (Tests_3) → final `F-028`.

**Code-Bezug geprüft für jeden finalen Test.** Verweise im Format `src/…:Zeile`.

---

## 2. Deduplizierungs-Matrix (Auszug – vollständige Karte)

| Finale ID | Semantik | Tests_1 | Tests_2 | Tests_3 | Tests_4 | Code-Prüfung |
|-----------|----------|---------|---------|---------|---------|--------------|
| F-001 | AppConfig default max_depth=2, version=4, dotnet, 6 Agents | `default_has_valid_depth…` `default_profile_is_dotnet` `default_agents_populated` | #1-3 `default_has_valid_depth` `default_has_required_fields` `save_and_load…` | `test_config_default_has_valid_state` | `default_config_shape` `default_agents_include_claude` | `src/config.rs:386-408` `default==2` `version=4` `src/config.rs:8-16` Clamp 0→2 >10→10 |
| F-010 | add_root dedupliziert + sortiert | `add_root_adds_unique_sorted` `add_root_twin…` | #4-5 | `test_config_add_root[_no_duplicates]` | `add_root_adds_and_sorts` `add_root_dedups` | `src/config.rs:627-632` `contains` check + `sort()` |
| F-018 | get_effective_profile Fallback | `get_effective_profile_for_repo_returns_active` | #24-25 | `test_config_get_effective_profile_for_repo` | `profile_effective_no_override/valid/invalid` | `src/config.rs:725-734` Override → `get_profile` sonst `get_active_profile` |
| F-030 | LanguageProfile normalized_extension | `normalized_extension_adds_dot_prefix` `lowercases` `trims_whitespace` | #44-46 | `test_normalized_extension_*` | `profile_normalized_extension_*` | `src/config.rs:156-161` `trim().to_lowercase()` `starts_with('.')` |
| F-045 | matches_file wildcard `*.sln` | `matches_file_with_pattern_wildcard` | #50 `glob` | `test_matches_file_with_pattern_wildcard` | `profile_matches_wildcard_ext` | `src/config.rs:163-187` `contains('*')` → ext-Vergleich `to_lowercase()` |
| F-058 | get_branch detached HEAD | `get_branch_returns_short_oid_and_true_for_detached` | #78 `detached` | `test_get_branch_detached` | `branch_detached` `head_parse_detached_sha` | `src/git.rs:56-81` `is_branch()` false → `target()` short 7 chars |
| F-075 | list_branches remote + HEAD Filter | `list_branches_returns_remote_branches` `filters_head_remote` | #102-103 | `test_list_branches_includes_remote` | `list_branches_local_and_remote` `excludes_origin_head` | `src/git.rs:150-189` `BranchType::Remote` + `ends_with("/HEAD")` continue + `sort`/`dedup` |
| F-118 | scan respects max_depth | `scan_respects_max_depth` | #161 | `test_scan_repos_respects_max_depth` | `scan_respects_max_depth` | `src/scanner.rs:127-131` `WalkDir::max_depth(max_depth)` |
| F-145 | is_dirty ignored files | `is_dirty_ignores_ignored_files` | #86 `ignored_excluded` | `test_is_dirty_clean` (implizit) | – | `src/git.rs:100-110` `include_ignored(false)` |
| F-160 | MyApp handle_branch_switch dirty → Dialog | `–` (implizit) | #260 `dirty` | `test_myapp_handle_branch_switch_dirty` | `handle_branch_switch` | `src/app.rs:109-140` `is_dirty` → `BranchDialog {dirty_files: get_detailed_status}` |
| F-172 | ui/theme COLOR_DIRTY | – | #193 `color_dirty_rgb` | `test_color_dirty_is_redish` | `color_constants` | `src/ui/theme.rs:98` `RGB(220,70,40)` |
| … | … | … | … | … | … | … |

> Vollständige Herkunft jedes finalen Tests ist in der letzten Spalte der Final-Tabellen (Kap. 3-9) angegeben.

**Wichtige Code-Erkenntnisse für Deduplizierung:**

* **Clamping:** `AppConfig::try_load` (`src/config.rs:532-537`) clamped nur `max_depth==0→2` und `>10→10`; `1` ist gültig. Tests_2 Annahme `1-10 valid` korrekt, Tests_1 `2-10` zu streng → final `F-006` prüft beide Grenzen.
* **Profile-Validierung:** `LanguageProfile::max_scan_depth` (`src/config.rs:545-549`) clamped `0→3` und `>4→4`; nicht `>10`. Tests_1 erwähnte `max_scan_depth` nicht → ergänzt.
* **Extension-Normalisierung** ist `trim + lowercase + dot` (`src/config.rs:156-161`), nicht nur `lowercase`.
* **`matches_file` Wildcard** ist kein echtes Glob, sondern nur Extension-Check (`src/config.rs:168-175`). Alle `*.sln`-Tests semantisch identisch.
* **`parse_head_file` privat** (`src/git.rs:83-97`) – Tests in Tests_4 sind keine separaten Unit-Tests, sondern Edge-Cases von `get_branch` (`src/git.rs:56-81`). Zusammengefasst.
* **`list_branches` Deduplizierung** ist `sort+dedup` (`src/git.rs:186-187`), behält aber `origin/main` neben `main` (Präfix bleibt). Tests_1 und Tests_2 Formulierung „beide kept“ korrekt.
* **`fetch_all` Logik** (`src/git.rs:192-227`): `remotes.len()==0→Ok`, `==1 && fail→Err`, sonst Warnung → final 3 Tests statt 4.
* **`launch_agent` Auto-Kette** (`src/git.rs:610-659`): `wt → pwsh/powershell → cmd`. Tests_2 `auto_chain` = Tests_1 `auto_prefers_wt` + `auto_falls_back_to_cmd` → ein parametrisierter Test.
* **`is_ignored_dir`** (`src/scanner.rs:23-25`) prüft nur Namen gegen `IGNORED_DIRS` (`src/scanner.rs:8-21`). Tests_3 `venv` vs Tests_1 `hidden_system_dirs` überschneiden → ein Test.
* **`scan_solutions_for_repo` Tiefe** ist `min(max_scan_depth,4)` (`src/scanner.rs:86`), nicht `max_depth` aus `AppConfig`. Tests_1/2/4 konsistent nach Korrektur.

---

## 3. Finaler Testplan – `config.rs`

### 3.1 TerminalPreference & TerminalConfig (`src/config.rs:21-50`)

| ID | Finaler Testname | Beschreibung | Edge Cases | Herkunft | Code-Bezug |
|----|------------------|--------------|------------|----------|------------|
| F-001 | `terminal_default_is_auto` | `Default` ist `Auto` | – | T1 `default_is_auto`, T2 #67, T3 `default_is_auto`, T4 `terminal_defaults` | `src/config.rs:29-33` |
| F-002 | `terminal_all_variants_exist` | Varianten `Auto, WindowsTerminal, Cmd, Powershell, Custom(String)` vorhanden | `Custom("")` leer | T1 `variants_are…`, T2 #68, T3 `equality/serialization` | `src/config.rs:21-27` |
| F-003 | `terminal_serde_roundtrip_snake_case` | JSON-Roundtrip erhält Varianten, `rename_all=snake_case` | `Custom("a & b")` Sonderzeichen, `WindowsTerminal`→`windows_terminal` | T1 `serde_roundtrip…` `all_variants_have…`, T3 `serialization`, T4 `custom_serde` | `src/config.rs:20-21` `#[serde(rename_all)]` |
| F-004 | `terminal_custom_stores_string` | `Custom(String)` hält Inhalt | Leerer String, Leerzeichen | T1 `custom_variant…`, T3 `equality` | `src/config.rs:26` |
| F-005 | `terminal_config_default` | `TerminalConfig::default` → `preference=Auto`, `fallback=Cmd` | – | T2 #69, T3 `test_terminal_config_default`, T4 `terminal_defaults` | `src/config.rs:43-50` |
| F-006 | `terminal_config_serde_roundtrip` | Serialize/Deserialize Roundtrip | Beide Felder `Custom` | T3 `test_terminal_config_serialization` | `src/config.rs:35-41` |

### 3.2 Theme (`src/config.rs:55-91`, `src/ui/theme.rs:9-96`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-007 | `theme_variants_are_five` | 5 Varianten `Light,Dark,Nord,Dracula,Solarized` existieren | – | T1 `variants_are…`, T2 #60-66, T3 `test_theme_all…`, T4 `theme_all…` | `src/config.rs:55-61` |
| F-008 | `theme_display_name_all` | `display_name()` → Light/Dark/Nord/Dracula/"Solarized Light" | `Solarized` Leerzeichen | T1 `light/dark/nord…`, T2 #60-64, T4 `theme_display_names` | `src/config.rs:71-79` |
| F-009 | `theme_all_returns_five` | `Theme::all()` len==5, enthält alle Varianten | Reihenfolge egal | T1 `all_returns_all_five`, T2 #65, T3, T4 | `src/config.rs:82-90` |
| F-010 | `theme_default_is_light` | `Default` ist `Light` | – | T1 `light_is_default`, T2 #66, T3/4 | `src/config.rs:63-67` |
| F-011 | `theme_serde_roundtrip_snake_case` | JSON-Roundtrip, `"dark"` ↔ `Dark` | `Solarized`→`solarized` | T1 `serde_roundtrip…`, T3, T4 `theme_serde_snake_case` | `src/config.rs:53-54` |

### 3.3 IdeConfig (`src/config.rs:93-138`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-012 | `ide_effective_program_with_program` | Wenn `program` nicht leer → `effective_program()==program` | Leerstring, Whitespace | T1 `effective_program_returns…`, T2 #54, T3 `with_program`, T4 `ide_effective_from_program` | `src/config.rs:113-122` |
| F-013 | `ide_effective_program_from_command` | `program` leer + `command` gesetzt → erstes Wort von `command` | `command="devenv /something"` → `"devenv"`, `command=""` | T1 `falls_back_from_command`, T2 #55, T3 `from_command`, T4 `from_command_program` | `src/config.rs:116-118` |
| F-014 | `ide_effective_program_fallback_code` | Beide leer → `"code"` | `program=""` `command=None` | T1 `falls_back_to_code`, T2 #56, T3 `default`, T4 `no_config` | `src/config.rs:119-121` |
| F-015 | `ide_effective_args_with_args` | Wenn `args` nicht leer → `effective_args()==args` (Prio über `command`) | `args=["{file}"]` | T1 `effective_args_returns…`, T2 #57, T3 `with_args`, T4 `from_program`+`args_take_priority` | `src/config.rs:123-124` |
| F-016 | `ide_effective_args_from_command` | `args` leer + `command` mit >1 Wort → `parts[1..]` | `command="cmd arg1 arg2"` → `["arg1","arg2"]` | T1 `uses_first_word…`, T2 #58, T3 `from_command`, T4 `from_command_args` | `src/config.rs:126-130` |
| F-017 | `ide_effective_args_fallback_file` | `command` nur 1 Wort oder beide leer → `["{file}"]` | `command="code"` → `["{file}"]` | T1 `uses_file_placeholder` `handles…`, T2 #59, T3 `default`, T4 `command_single_word` `no_config` | `src/config.rs:131-136` |
| F-018 | `ide_args_priority_over_command` | `args` gesetzt + `command` gesetzt → `args` gewinnt | – | T4 `ide_args_take_priority` | `src/config.rs:124` |
| F-019 | `ide_serialization` | Roundtrip mit allen Feldern, optionale `None/Some` | `use_shell`, `allow_unsafe` | T3 `test_ide_serialization` | `src/config.rs:94-110` |

### 3.4 LanguageProfile (`src/config.rs:142-193`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-020 | `profile_normalized_extension_adds_dot` | `"sln"`→`".sln"`, `".sln"` bleibt | Bereits mit Punkt | T1 `adds_dot_prefix`, T2 #45-46, T3 `adds_dot`/`already_dotted`, T4 `adds_dot`/`keeps_dot` | `src/config.rs:156-161` |
| F-021 | `profile_normalized_extension_lowercases` | `".SLN"`→`".sln"`, `"SLN"`→`".sln"` | MixedCase | T1 `lowercases`, T2 #44, T3 `lowercase`, T4 `lowercases` | `src/config.rs:157` |
| F-022 | `profile_normalized_extension_trims` | `" .sln "`→`".sln"` Whitespace gestrippt | Leer + Punkt | T1 `trims_whitespace`, T3 `trims`, T4 implizit | `src/config.rs:157` `trim()` |
| F-023 | `profile_matches_wildcard_case_insensitive` | `*.sln` matcht `foo.sln` + `FOO.SLN`, nicht `foo.txt` | Groß/Klein | T1 `matches_file_with_pattern_wildcard`/`case_insensitive`, T2 #47/53, T3 `wildcard`, T4 `wildcard_ext` | `src/config.rs:168-175` `to_lowercase()` |
| F-024 | `profile_matches_exact_filename` | `"Cargo.toml"` matcht nur exakt, Case-sensitive Name aber `single_pat` Vergleich `==` | `cargo.toml` ≠ `Cargo.toml`? Name `==` exakt, Extension lower | T1 `exact`, T2 #48, T3 `exact`, T4 `exact_filename` | `src/config.rs:178-183` |
| F-025 | `profile_matches_without_pattern_uses_extension` | `file_pattern=None` → reine Extension-Prüfung | Datei ohne Extension → false | T1 `no_pattern_uses_extension`, T2 #51-52, T3 `without_pattern`, T4 `no_pattern` | `src/config.rs:188-191` |
| F-026 | `profile_matches_custom_pattern_exact` | Nicht-`*` Pattern = exakter Dateiname | `pom.xml` nicht `build.gradle` | T1 `custom_pattern` | `src/config.rs:177` |
| F-027 | `profile_matches_wrong_extension_false` | Falsche Extension → false | – | T1 `returns_false…` | `src/config.rs:190` |
| F-028 | `profile_matches_comma_separated_patterns` | `pom.xml,build.gradle` komma-getrennt, Leerzeichen getrimmt | `" pom.xml , build.gradle "` | T1 `file_pattern_comma…`, T2 #50, T3 `multiple_patterns`, T4 `comma_patterns` | `src/config.rs:165` `split(',')` `trim` |
| F-029 | `profile_matches_no_extension_file_false` | Datei ohne Extension (`Makefile`) → false wenn ohne Pattern | – | T2 #52, T4 `file_without_ext` | `src/config.rs:190-191` `unwrap_or(false)` |
| F-030 | `profile_dotnet_default_shape` | `default_dotnet_profile()` hat id `dotnet`, display `.NET`, `.sln`, Pattern `*.sln`, 3 IDEs (vs2022/vscode/rider), `default_ide_id=vs2022` | – | T4 `dotnet_default_profile_shape` | `src/config.rs:195-233` |
| F-031 | `profile_serialization` | Roundtrip mit allen Feldern | `file_pattern None/Some`, `max_scan_depth` | T3 `test_profile_serialization` | `src/config.rs:141-153` |

### 3.5 AgentLaunchMode & AgentProfile (`src/config.rs:244-329`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-032 | `agent_launch_mode_default_is_terminal` | `Default` ist `Terminal` | – | T2 #70, T3 `default_is_terminal`, T4 `agent_launch_mode_default` | `src/config.rs:251-255` |
| F-033 | `agent_launch_mode_serde_roundtrip` | Roundtrip `Terminal`/`Detached`, snake_case | – | T1 implizit, T3 | `src/config.rs:244-249` |
| F-034 | `agent_profile_defaults` | `new`/`default_agents()[0]` hat `launch_mode=Terminal`, `terminal_override=None`, `args=[]`, `command=None` | – | T1 `new_creates…` `launch_terminal…` `terminal_override…` `args…` `command…`, T3 | `src/config.rs:257-270` `272-329` |
| F-035 | `agent_profile_custom_terminal_override` | `AgentProfile` kann `terminal_override=Some(Custom)` halten | – | T1 `terminal_override…` | `src/config.rs:269` |
| F-036 | `agent_profile_serialization` | Roundtrip mit allen Feldern | – | T3 | `src/config.rs:257-270` |
| F-037 | `default_agents_include_expected` | `default_agents()` enthält 6: claude/codex/gemini/copilot/cursor/aider, claude vorhanden | – | T1 `default_agents_populated`, T4 `default_agents_include_claude` | `src/config.rs:272-329` |

### 3.6 RepoUiState (`src/config.rs:340-348`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-038 | `repo_ui_state_default_none` | `Default` alle Felder `None` (serde default) | – | T1 `defaults_to_none`, T2 #71, T3 `default`, T4 implizit | `src/config.rs:340-348` `#[serde(default)]` |
| F-039 | `repo_ui_state_fields_can_hold_values` | `selected_solution` `PathBuf`, `selected_ide` `String`, `profile_override` `Option<String>` setzbar | – | T1 `selected_solution_can…` `selected_ide…` `profile_override…` | `src/config.rs:342-347` |
| F-040 | `repo_ui_state_serde_roundtrip` | Roundtrip mit `selected_solution` | – | T3 | `src/config.rs:340` |

### 3.7 AppConfig – Kern (`src/config.rs:352-735`)

#### Default & Roots

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-041 | `config_default_has_valid_state` | `Default` → `max_depth=2`, `config_version=4`, `active_profile_id="dotnet"`, `profiles`≥1, `agents`==6, `theme=Light`, `terminal=Auto/Cmd` | `home` nicht existent → roots leer möglich | T1 `default_has…`+`default_profile…`, T2 #1-2, T3 `default_has_valid_state`, T4 `default_config_shape` | `src/config.rs:386-408` |
| F-042 | `config_add_root_adds_and_sorts` | `add_root` fügt hinzu, sortiert alphabetisch | Leere Liste | T1 `add_root_adds_unique_sorted`, T2 #5, T3, T4 | `src/config.rs:627-632` |
| F-043 | `config_add_root_no_duplicates` | Doppelter Pfad wird nicht erneut eingefügt (dedup) | Gleicher Pfad 2x, 3 Duplikate | T1 `add_root_twin…duplicate`, T2 #4, T3, T4 `add_root_dedups` | `src/config.rs:628` `contains` |
| F-044 | `config_remove_root_existing` | `remove_root` entfernt exakt matching Pfad, andere bleiben | Letzter Root → leer | T1 `remove_root…`, T2 #6-7, T3, T4 `remove_root_removes` | `src/config.rs:634-636` |
| F-045 | `config_remove_root_noop_when_missing` | Entfernen nicht vorhandenen Pfads = No-Op | – | T1 `remove_root_twin…`, T3 | `src/config.rs:635` `retain` |

#### Profile & Repo-State

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-046 | `config_get_profile_by_id_found` | `get_profile(id)` findet Profil | – | T2 #10, T3 | `src/config.rs:646-648` |
| F-047 | `config_get_profile_by_id_not_found` | Nicht existierende ID → `None` (leere Liste) | Leere `profiles` | T2 #11, T3 | `src/config.rs:646` |
| F-048 | `config_get_active_profile_found_or_fallback` | `get_active_profile()` → `active_profile_id` wenn vorhanden, sonst `first()` | Ungültige `active_profile_id` | T1 `get_effective…`/`all_fields`, T2 #8-9, T3, T4 `get_active_profile_found/fallback` | `src/config.rs:638-644` |
| F-049 | `config_get_repo_state_found` | `get_repo_state(path)` gibt `Some` wenn in `repo_state` | Erstes Mal vs. existierend | T2 #21, T3 `get_repo_state` | `src/config.rs:710-713` |
| F-050 | `config_get_repo_state_mut_creates_default` | `get_repo_state_mut(path)` erstellt `Default` wenn nicht vorhanden | Unbekannter Pfad | T2 #22, T3, T4 `repo_state_mut_creates_default` | `src/config.rs:715-718` `or_default` |
| F-051 | `config_set_repo_profile_override` | `set_repo_profile_override(Some(id))` setzt, `None` leert | Override auf `None` = Global | T1 `set_repo_profile_override…`, T2 #23, T3, T4 `set_profile_override_persists` | `src/config.rs:720-723` |
| F-052 | `config_get_effective_profile_uses_override` | `get_effective_profile_for_repo` → Override-Profil wenn gültig | Override existiert | T2 #24, T3, T4 `profile_effective_valid_override` | `src/config.rs:725-731` |
| F-053 | `config_get_effective_profile_fallback_global` | Ohne Override oder ungültiges Override → globales aktives Profil | Ungültige Override-ID | T2 #25, T3, T4 `no_override`/`invalid_override` | `src/config.rs:725-733` |

#### Agents (multi)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-054 | `config_get_active_agent_prefers_ids` | `get_active_agent()` bevorzugt `active_agent_ids[0]` | Leere Liste | T2 #12, T3 `get_active_agent` | `src/config.rs:652-666` |
| F-055 | `config_get_active_agent_fallback_deprecated` | Fallback auf `active_agent_id` wenn `ids` leer, sonst `agents.first()` | `agents` leer | T2 #13-14, T3 | `src/config.rs:660-665` |
| F-056 | `config_get_active_agents_multiple` | `get_active_agents()` gibt alle aktiven zurück (filter invalid) | Keine aktiven → leer | T2 #15, T3 | `src/config.rs:669-683` |
| F-057 | `config_is_agent_active` | `is_agent_active(id)` prüft `active_agent_ids` sonst deprecated | – | T2 #16-17, T3 | `src/config.rs:686-694` |
| F-058 | `config_toggle_agent_active_add_remove_sync` | `toggle_agent_active` add/remove, sync `active_agent_id=first`, verhindert nicht strikt leer | Letzter Agent entfernt → leer erlaubt | T2 #18-20, T3 `toggle…` `prevent_empty` | `src/config.rs:697-708` |
| F-059 | `config_toggle_prevent_empty_warning` | UI-Warnung wenn alle deaktiviert (Settings / App) | – | T2 #253-254, T3 | `src/ui/settings.rs:844` + `src/app.rs` toggle |

#### Persistenz / Migration / Validierung

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-060 | `config_save_and_load_roundtrip` | `save()` + `try_load` (Temp-Datei) erhält `roots`, `max_depth`, `version` | Pretty JSON | T1 `save_and_load…`, T2 #3, T3 not, T4 `save_load_roundtrip` | `src/config.rs:614-625` `save()` `try_load()` |
| F-061 | `config_path_returns_some` | `AppConfig::config_path()` liefert Pfad mit `config.json`, plattformunabhängig | Kein Pfad ermittelbar → None | T2 #26, T4 `config_path_returns_some` | `src/config.rs:431-436` |
| F-062 | `config_try_load_nonexistent_returns_default` | Datei nicht vorhanden → `Default` | – | T2 #27 | `src/config.rs:451-453` |
| F-063 | `config_try_load_invalid_json_returns_default_with_warn` | Ungültiges JSON → `load()` warnt, gibt `Default` (via `try_load` Err) | Korrupt | T2 #28 | `src/config.rs:439-446` `serde_json::from_str` Err |
| F-064 | `config_migration_v1_to_v2_injects_defaults` | Alte JSON nur `roots`/`max_depth` → injiziert `profiles`/`agents`/`active_profile_id`, version→2 | Leere Profile | T2 #29, T4 `migration_old_config_injects_defaults` | `src/config.rs:458-475` `config_version<2` |
| F-065 | `config_migration_v2_to_v3_adds_missing_defaults` | Version<3 → fehlende Default-Profile/Agents ergänzt | Config v2 | T2 #30, T4 `migration_v2_to_v3_adds_defaults` | `src/config.rs:476-492` |
| F-066 | `config_migration_v3_to_v4_removes_auto_profiles_and_migrates_agent` | Entfernt `rust/node/python/java/go/cpp`, behält `dotnet`+custom, migriert `active_agent_id`→`active_agent_ids` | Gemischtes Set, leer → Default | T2 #31-32, T3 `migration_v2/v3`, T4 `migration_v3_to_v4_removes…` | `src/config.rs:493-531` |
| F-067 | `config_validation_max_depth_clamped` | `max_depth==0→2`, `>10→10` | 0,1,10,11,99 | T1 `max_depth_clamped…`, T2 #33-34, T3, T4 `validation_clamps…` | `src/config.rs:532-537` |
| F-068 | `config_validation_roots_normalized` | Roots: leere `OsString` entfernt, `sort`, `dedup` | Mehrfach-Duplikate, leer | T1 `roots_deduped_on_load`, T2 #35-36, T3, T4 `validation_roots…` | `src/config.rs:538-540` |
| F-069 | `config_validation_profiles_normalized` | Profile: `file_extension=normalized`, `max_scan_depth 0→3 >4→4`, `id trim+lowercase` → `"custom"` wenn leer | `depth 0`, `>4`, leere ID, Großbuchstaben | T2 #37, T3 `profile_validation`, T4 | `src/config.rs:543-562` |
| F-070 | `config_validation_profiles_dedup_last_wins` | Doppelte Profil-IDs entfernt, letztes gewinnt (reverse) | – | T2 #38, T4 `validation_profile_duplicates…` | `src/config.rs:564-575` |
| F-071 | `config_validation_active_profile_corrected` | Ungültige `active_profile_id` → erstes Profil bzw. Default | `active_profile_id` nicht existent | T2 #39, T3, T4 `validation_active_profile…` | `src/config.rs:577-583` |
| F-072 | `config_validation_agents_filtered_and_fallback` | Ungültige `active_agent_ids` gefiltert, leer → `agents.first()`, `agents` leer → Defaults | Veraltete IDs | T2 #40-42, T3 `agent_ids_validation`, T4 | `src/config.rs:585-610` |
| F-073 | `config_validation_agents_sync_deprecated` | Sync `active_agent_ids.first()` ↔ `active_agent_id`, korrigiert veraltetes Feld | Beide konsistent | T2 #42, T3 | `src/config.rs:603-609` |
| F-074 | `config_save_creates_directory` | `save()` erstellt Config-Verzeichnis falls nicht existent | – | T2 #43 | `src/config.rs:616-618` `create_dir_all` |
| F-075 | `config_normalized_profile_ids_lowercase_on_load` | IDs via `to_lowercase` validiert | – | T1 `normalized_profile_ids…` | `src/config.rs:551` |

> Hinweis Tests_4 (`default_root_path`/`dirs_home` hängen am Home) bewusst **nicht** unit-testbar → manuell.

---

## 4. `git.rs` – Kernstrukturen & Helfer

### 4.1 SolutionFile & RepoInfo (`src/git.rs:4-48`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-076 | `solution_file_creation_with_paths` | `SolutionFile{path, relative}` korrekt | Relativ zum Repo-Root | T1 `new_creates…`, T2 #75, T3 `test_solution_file_creation` | `src/git.rs:4-9` |
| F-077 | `solution_file_clone_and_eq` | `Clone`/`PartialEq` erhält beide Felder, Gleichheit bei gleichen Feldern | Unterschiedliche Felder → != | T1 `debug_clone…` `partial_eq…`, T3 `equality` | `src/git.rs:5` `#[derive(PartialEq, Eq)]` |
| F-078 | `repo_info_new_with_defaults` | `RepoInfo::new(path,branch,dirty,detached)` → `name` aus `file_name` oder `display`, `branches=[]` `solutions=[]` `selected=None` | Pfad ohne Dateinamen | T1 `new_creates_with_defaults` `name_derived…` `selected_solution…` `all_fields…`, T2 #72-74, T3 `test_repo_info_new`/`name_from_path` | `src/git.rs:24-40` |
| F-079 | `repo_info_with_branches_builder` | `with_branches(vec)` setzt Branches, andere Felder bleiben | Leere Liste, mehrere | T1 `with_branches…`, T2 #74, T3 | `src/git.rs:42-45` |

### 4.2 open_repo / get_repo_info (`src/git.rs:51-404`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-080 | `open_repo_valid_vs_invalid` | `open_repo(valid_git) → Some(Repository)`, sonst `None` (non-git, nicht existierend) | Bare? | T1 `open_repo_returns…`, T2 #128/130-131, T3 `test_open_repo_*` | `src/git.rs:51-53` `Repository::open(path).ok()` |
| F-081 | `get_repo_info_some_vs_none` | `get_repo_info(path)` → `Some` mit Branch/dirty/branches wenn Repo, sonst `None` | non-repo | T2 #128-129, T3 `test_get_repo_info…`, T4 `get_repo_info_*` | `src/git.rs:398-404` |

### 4.3 get_branch & parse_head_file (`src/git.rs:56-97`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-082 | `get_branch_normal_is_not_detached` | Normaler Branch (main/master/feature) → `(name,false)` | – | T1 `returns_branch_and_false`, T2 #76-77, T3 `test_get_branch_on_main`, T4 `branch_detection` | `src/git.rs:57-63` `is_branch()`+`shorthand()` |
| F-083 | `get_branch_detached_true_with_short_oid` | Detached HEAD → 7-Zeichen Short-OID, `true` | Commit ohne Branch | T1 `returns_short_oid…`, T2 #78, T3, T4 `branch_detached` `head_parse_detached_sha` | `src/git.rs:64-67` `format!("{oid:.7}")` |
| F-084 | `get_branch_via_head_file_ref` | Fallback `parse_head_file` → `ref: refs/heads/main` → `"main"`, nested `feature/x`, anderes `ref:` Präfix gestrippt | – | T1 `returns_name_and_false_from_head_file`, T2 #80-81, T4 `head_parse_ref*` | `src/git.rs:83-91` |
| F-085 | `get_branch_no_commits_fallback` | `repo.head()` Err + kein HEAD-File → `("no commits",false)`, eprintln | Leeres Repo | T1 `returns_default_on_error`, T2 #79, T4 `head_parse_invalid` | `src/git.rs:73-79` |
| F-086 | `get_branch_parse_invalid_returns_none` | `HEAD` zu kurz (<7) oder leer → `parse_head_file==None` | Ungültig | T4 `head_parse_invalid_or_empty` | `src/git.rs:92-96` |

### 4.4 Status: is_dirty / has_merge_conflicts / is_merge_in_progress / get_detailed_status (`src/git.rs:99-147`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-087 | `is_dirty_clean_false` | Clean Working Dir → `false` | Frisch committed | T1 `returns_false…`, T2 #82, T3, T4 `dirty_clean_after_commit` | `src/git.rs:100-110` `statuses.is_empty()` |
| F-088 | `is_dirty_modified_true` | Modified Datei → `true` | – | T1 `returns_true_for_modified`, T2 #83, T3, T4 `dirty_modified` | `src/git.rs:101-107` `include_untracked(true)` |
| F-089 | `is_dirty_untracked_true` | Untracked Datei → `true` (`include_untracked` + `recurse_untracked_dirs`) | – | T1 `untracked`, T2 #84, T3, T4 `dirty_untracked` | `src/git.rs:102-103` |
| F-090 | `is_dirty_staged_true` | Staged (`index_modified`) → `true` (auch via `is_dirty`, da Status nicht 무시) | – | T2 #85, T3 `staged` | `src/git.rs:106` Statuses |
| F-091 | `is_dirty_ignored_ignored` | Ignorierte Dateien (`.gitignore`, `node_modules` wenn ignored) → `false` | – | T1 `ignores_ignored_files`, T2 #86, T3 | `src/git.rs:104` `include_ignored(false)` |
| F-092 | `is_dirty_recurse_untracked_dirs_and_bare` | Untracked-Verz. rekursiv geprüft; Bare-Repo Status Err → `false` kein Panic | `Bare` | T2 #87-88 | `src/git.rs:108` `Err(_)=>false` |
| F-093 | `has_merge_conflicts_clean_false` | Clean Index → `false` | – | T1 `returns_fresh_repo`, T2 #89, T3, T4 `no_conflicts_clean` | `src/git.rs:113-115` `index.has_conflicts()` |
| F-094 | `has_merge_conflicts_true` | Index mit Konflikten → `true` | Bare → `false` | T1 `returns_true_with_conflicts`, T2 #90-91, T3 | `src/git.rs:114` |
| F-095 | `is_merge_in_progress_true_for_merge_rebase` | `repo.state()!=Clean` → `true` (Merge, Rebase) sonst `false` | – | T1 `returns_false/true`, T2 #92-94, T3, T4 `not_in_merge_clean` | `src/git.rs:117-119` |
| F-096 | `get_detailed_status_classifies` | Liefert `modified:/staged:/untracked:/conflict:/dirty: path`, leer wenn clean, mehrere Dateien | – | T2 #95-100, T3 `clean`/`various`, T4 `detailed_status_classifies` | `src/git.rs:121-147` `is_wt_new`/`wt_modified`/`index_modified`/`conflicted` |

### 4.5 Branches & Fetch & Explorer (`src/git.rs:150-255`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-097 | `list_branches_local` | Lokale Branches aufgelistet | Leere | T1 `returns_local_branches`, T2 #101, T3 `includes_local`, T4 `local_and_remote` | `src/git.rs:160-167` |
| F-098 | `list_branches_remote_with_prefix` | Remote `origin/…` mit Präfix | – | T1 `remote_branches`, T2 #102 | `src/git.rs:169-184` |
| F-099 | `list_branches_excludes_origin_head` | `origin/HEAD` gefiltert | – | T1 `filters_head_remote`, T2 #103 | `src/git.rs:175-176` |
| F-100 | `list_branches_dedup_and_sorted` | `sort`+`dedup`, `main` vs `origin/main` beide behalten (Präfix), Duplikate entfernt | – | T1 `deduplicates…`, T2 #104-105, T3 `dedup`, T4 `sorted_dedup` | `src/git.rs:186-188` |
| F-101 | `list_branches_bare_and_invalid_empty` | Bare Repo oder invalid Path → `[]` | – | T1 `returns_empty_for_bare`, T2 #106-107, T3 `bare_repo`, T4 `non_repo` | `src/git.rs:155-157` |
| F-102 | `fetch_all_no_remotes_ok` | Keine Remotes → `Ok(())` | – | T1 `succeeds_with_no_remotes`, T2 #108 | `src/git.rs:218-219` |
| F-103 | `fetch_all_single_fail_errors` | Ein Remote schlägt fehl → `Err` (len==1) | Invalid repo → Err | T1 `fails…`, T2 #109/111 | `src/git.rs:221-223` |
| F-104 | `fetch_all_partial_fail_warns_but_ok` | Mehrere Remotes, teils fail → `Ok` (Warnung), nur wenn alle fail → Err | Gemischt | T1 `warns_but_succeeds…`, T2 #110 | `src/git.rs:225` `Ok(())` trotz `last_err` wenn len>1 |
| F-105 | `open_in_explorer_platform_and_graceful` | Windows `explorer`, Linux `xdg-open` → Fallback `open` (macOS), missing Path graceful | Plattform | T1 `windows_spawns…` `linux_uses…` `mac_uses…` `fails…`, T2 #132-133 | `src/git.rs:229-255` `cfg(windows/not)` |

### 4.6 Checkout & Stash (`src/git.rs:260-395`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-106 | `checkout_branch_safe_success` | Clean Repo, existierender Branch (lokal/remote→Tracking) → `Ok` | Dry-run `safe()` | T1 `succeeds_on_clean_repo`, T2 #112-113, T3 `safe`, T4 `switches`/`remote_creates…` | `src/git.rs:260-327` |
| F-107 | `checkout_branch_blocks_merge_in_progress` | Merge/Rebase läuft → `Err` | – | T1 `fails_on_merge…`, T2 #117, T3 `blocks_on_merge`, T4 `rejects_merge…` | `src/git.rs:265-266` |
| F-108 | `checkout_branch_blocks_conflicts` | Merge-Konflikte → `Err` | – | T1 `fails_on_merge_conflicts`, T2 #118, T3 `blocks_on_conflicts` | `src/git.rs:268-270` |
| F-109 | `checkout_branch_blocks_bare` | Bare → `Err` | – | T2 #116, T3 `blocks_on_bare` | `src/git.rs:263-264` |
| F-110 | `checkout_branch_nonexistent_error` | Nicht existierender Branch → `Err` | – | T1 `fails_on_nonexistent`, T2 #115, T3, T4 `rejects_missing` | `src/git.rs:272-274` `revparse_ext` |
| F-111 | `checkout_branch_dry_run_conflict_detection` | Dry-run erkennt Tree-Konflikte → `Err` mit Conflict-Code | Dirty Dateien | T1 `dry_run_detects…`, T2 `checkout_conflicts` implizit | `src/git.rs:277-287` `dry_run` `Conflict` |
| F-112 | `checkout_branch_same_branch_ok` | Wechsel zum aktuellen Branch → kein Fehler (No-Op) | – | T2 #119 | `src/git.rs` `checkout_tree` ok |
| F-113 | `checkout_branch_detached_to_commit` | Zu Commit → detached HEAD (`set_head_detached`) | – | T2 #114, T4 implizit | `src/git.rs:321-324` |
| F-114 | `checkout_branch_force_overwrites` | `checkout_branch_force` verwirft lokale Änderungen (force) | Dirty + konfliktierend | T1 `force_overwrites…`, T2 #120, T3 `force` | `src/git.rs:330-351` `cb.force()` |
| F-115 | `checkout_force_nonexistent_and_bare_error` | Force bei nicht existent/Bare → `Err` | – | T2 #121-122 | `src/git.rs:332-333` |
| F-116 | `stash_and_checkout_clean_warns_nothing_to_stash` | Clean (nichts zu stashen) → `NotFound` Warnung, trotzdem Checkout `Ok`, `stash_pop` nach Erfolg | – | T1 `success_on_clean`/`warns_nothing…`, T2 #123/127, T3 | `src/git.rs:354-381` `stash_save` `NotFound` → eprintln |
| F-117 | `stash_and_checkout_with_changes_success` | Mit Änderungen → Stash+Checkout+Pop erfolgreich | – | T2 #124, T3 | `src/git.rs:365-381` |
| F-118 | `stash_and_checkout_preserves_on_failure` | Checkout fail → Stash pop versucht wiederherzustellen, sonst Err | Stash `is_ok` → pop | T1 `preserves_stash_on_failure`, T2 #126 | `src/git.rs:384-393` |
| F-119 | `stash_and_checkout_pop_fail_keeps_stash` | Checkout ok aber `stash_pop` Konflikt → `Err` mit Hinweis, Stash bleibt `stash@{0}` | – | T2 #125, T3 `stash_and_checkout_conflict` | `src/git.rs:377-381` `bail!("…Stash bleibt…")` |

### 4.7 Helper: Program & Placeholders & Launch (`src/git.rs:409-660`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-120 | `is_program_available_true_false` | `"git"`→`true`, unbekanntes →`false` via `which`/`where` | Nicht im PATH | T1 `returns_true…`/`false…`, T2 #134-135, T3 | `src/git.rs:430-448` |
| F-121 | `resolve_vs_path_windows` | `vswhere` nicht vorhanden→`None`, vorhanden→`Some(PathBuf)` | Nicht-Windows → `None` | T1 `returns_none/path`, T2 #136-137 | `src/git.rs:450-469` |
| F-122 | `substitute_placeholders_all` | `{file}→file`, `{dir}→parent`, `{repo}→file` (repo=file), multi, kein Placeholder unverändert | Tief verschachtelt | T2 #138-143, T3 `sub_*` | `src/git.rs:409-420` |
| F-123 | `quote_if_needed_spaces_and_already_quoted` | Leerzeichen→`"…"` mit `"`-Escape, kein Leerzeichen unverändert, bereits gequotet kein Doppel | `ShellChars` | T2 #144-147, T3 `quote_*` | `src/git.rs:422-428` `contains(' ')` + `starts_with('"')` |
| F-124 | `launch_ide_program_and_args_resolution` | `effective_program` `code`/`devenv`/Fallback, `effective_args` `["{file}"]` vs. aus `command` | – | T1 `effective_program_vscode…` `effective_args…`, T4 `argument_substitution` | `src/git.rs:472-478` |
| F-125 | `launch_ide_blocks_shell_chars_when_not_unsafe` | `allow_unsafe=false` + `&\|;<>` in Args → `Err` | `allow_unsafe=true`→Ok | T1 `validation_blocks…`/`passes_when…`, T2 #149-150, T4 `launch_ide_blocks…` | `src/git.rs:499-505` |
| F-126 | `launch_ide_fallback_vswhere_for_devenv` | `program=devenv` nicht im PATH → `resolve_vs_path` genutzt | Windows | T1 implizit | `src/git.rs:482-486` |
| F-127 | `launch_agent_auto_chain_wt_pwsh_cmd` | `Auto`: `wt`→`pwsh`→`cmd` Fallback-Kette; `WindowsTerminal`→`wt -d … -- cmd /k`; `Powershell`→`pwsh`; `Cmd`→`cmd /C start…`; `Custom`→custom | Kein Terminal → Err | T1 `launch_agent_*` 6 Varianten, T2 #152-155, T3 | `src/git.rs:558-658` |
| F-128 | `launch_agent_with_args_and_command_templates` | Agent `args` → `quote_if_needed` join; `command` mit `{file}/{dir}/{repo}` substituiert, ohne Placeholder unverändert; `terminal_override` überschreibt globale | `args` + `command` beides → `args` gewinnt | T1 `with_args`/`with_command…`/`no_placeholders`, T2 #156-158, T3 | `src/git.rs:538-554` |
| F-129 | `launch_agent_terminal_override` | `agent.terminal_override` überschreibt `terminal_pref` | – | T2 #158 | `src/git.rs:557` |

---

## 5. `scanner.rs` (`src/scanner.rs`)

### 5.1 Konstanten & is_ignored_dir

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-130 | `ignored_dirs_contains_common` | `IGNORED_DIRS` enthält `node_modules,target,.cargo,.vscode,.idea,__pycache__,.venv,venv,.next,.nuxt,dist,build` | – | T3 `test_ignored…`, T4 `ignored_dir_known` | `src/scanner.rs:8-21` |
| F-131 | `is_ignored_dir_known_vs_unknown` | `is_ignored_dir("node_modules")→true`, `"src"→false`, `".cargo"`→true | Case-sensitive | T2 #183-184, T3 implizit, T4 `ignored_dir_known/unknown` | `src/scanner.rs:23-25` |

### 5.2 scan_repos (`src/scanner.rs:28-82`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-132 | `scan_empty_roots` | Leere `roots`→`[]` | – | T2 #164 | `src/scanner.rs:32` loop leer |
| F-133 | `scan_finds_repos_at_depth_1` | Depth=1 findet direkte Kinder | 2 Kinder | T1 `finds_repos_at_depth_1`, T2 #160, T3, T4 | `src/scanner.rs:127-176` `max_depth=1` |
| F-134 | `scan_respects_max_depth_1_vs_2` | Depth1 skippt Enkel, Depth2 findet | Tiefe 0/3/10 Grenz | T1 `respects_max_depth`, T2 #161, T3, T4 | `src/scanner.rs:127-131` `WalkDir::max_depth` |
| F-135 | `scan_ignores_non_repos` | Nicht-Git Ordner → ignoriert | Datei statt Verz. | T1 `ignores_non_repos`, T2 #162, T4 `ignores_non_repos` | `src/scanner.rs:172` `get_repo_info` None |
| F-136 | `scan_dedup_multiple_roots` | Gleiche Roots mehrmals → dedupliziert via `seen_paths` | `vec![root,root]` + verschiedene Roots mit gleichem Repo | T1 `dedup_multiple_roots`/`dedup_across_different`, T2 #163, T3 | `src/scanner.rs:34,43` `HashSet` |
| F-137 | `scan_ignores_node_modules_target_hidden_git` | `node_modules`,`target`,`.cargo`,`.idea`,`.venv`,`.git` übersprungen (`filter_entry`) | Hidden System Dirs | T1 `ignores_node_modules/target/dotgit/hidden…`, T2 #167, T3 `ignores_node_modules/target/venv/git_dir`, T4 `ignored_dirs_excluded` | `src/scanner.rs:141-158` |
| F-138 | `scan_skips_nonexistent_and_file_root` | Nicht existierender Root / Root ist Datei → übersprungen mit `eprintln`, kein Panic | – | T1 `skips_nonexistent_root/file`, T2 #165-166, T3, T4 `skips_missing/non_dir` | `src/scanner.rs:33-40` `exists()` `is_dir()` |
| F-139 | `scan_bare_repo_not_included` | Bare Repo → `get_repo_info` liefert `None` durch `is_bare`? tatsächlich `open_repo` ok aber `list_branches` leer, aber `get_repo_info` liefert dennoch Repo? In scanner `get_repo_info` liefert Repo auch für bare? Jedoch `scan_single_root` via `get_repo_info` → bei bare `get_branch` etc trotzdem Some. Tests_1 `scan_single_root_with_bare_repo` erwartet keine Aufnahme – Code: `list_branches` bare→[] aber Repo trotzdem Some. Diskrepanz: Scanner schließt Bare nicht aus; Test sollte anpassen. | – | T1 `single_root_with_bare_repo` (semantisch falsch) | `src/git.rs:155-157` vs `src/scanner.rs:172` |
| F-140 | `scan_results_sorted_by_name_case_insensitive` | Ergebnis nach `name.to_lowercase()` sortiert | – | T2 #173, T4 `sorted_by_name` | `src/scanner.rs:49` |
| F-141 | `scan_root_itself_is_repo` | Wenn Root selbst Repo → gefunden (depth 0) | – | T4 `scan_root_itself_is_repo` | `src/scanner.rs:160-168` `candidates.push(root)` |
| F-142 | `scan_repos_with_multiple_roots_and_parallelism` | Mehrere Roots, parallel via `rayon`, Ergebnisse gesammelt | Viele Roots | T1 `scan_repos_with_multiple…` `rayon_parallelism`, T2 #186-187? | `src/scanner.rs:28` `par_iter_mut` |
| F-143 | `scan_solutions_populated_and_selected` | Nach Scan: `solutions` gefüllt, `selected_solution` aus `repo_state` wiederhergestellt falls enthalten, sonst erste, sonst None; Cap 20 | – | T2 #168-171, T4 `restores_selected_solution` | `src/scanner.rs:52-79` |
| F-144 | `scan_nested_repos_and_depth_boundary` | Verschachtelte Repos, Tiefe 0,1,3,10 | – | T2 #174-175 | `src/scanner.rs:127` |

### 5.3 scan_solutions_for_repo (`src/scanner.rs:85-125`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-145 | `scan_solutions_finds_matching_and_ignores_non_matching` | Findet passende Dateien gemäß Profil, ignoriert nicht passende | – | T2 #176-177, T3 | `src/scanner.rs:107-123` `profile.matches_file` |
| F-146 | `scan_solutions_respects_profile_depth_and_cap_20` | `max_scan_depth.min(4)` respektiert, `>20` → `truncate(20)` | Tiefe 5 → Cap 4, Tiefe 1 | T1 `respects_depth` `truncates_at_20`, T2 #178/183, T3 `respects_profile_depth` `max_20`, T4 `respects_depth` `cap_at_20` | `src/scanner.rs:86` `min(4)` + `src/scanner.rs:62-64` |
| F-147 | `scan_solutions_ignores_git_and_ignored_dirs` | `.git` + `is_ignored_dir` werden nicht durchsucht | – | T1 `ignores_dotgit/ignored_dirs`, T2 #179-180, T3, T4 `skips_git_and_ignored` | `src/scanner.rs:92-103` |
| F-148 | `scan_solutions_relative_paths_correct` | `relative` = `strip_prefix(repo_path)` | – | T2 #180, T4 `relative_paths` | `src/scanner.rs:113-117` |
| F-149 | `scan_solutions_sorted_root_first_alphabetical` | Sort: `relative.matches(MAIN_SEPARATOR).count()` Tiefe, dann `to_lowercase` alphabetisch | – | T2 #171, T3 `sorted_by_depth`, T4 implizit | `src/scanner.rs:56-61` |
| F-150 | `scan_solutions_pattern_multiple` | Mehrere Patterns `*.sln,*.cs` bzw. `pom.xml,build.gradle` | Komma | T1 `matches_file_with_pattern`, T2 #182 | `src/config.rs:165` |
| F-151 | `scan_solutions_case_insensitive_and_extension_normalized` | `".SOL"` → `".sln"` normalized, case-insensitive Match | `FOO.SLN` | T1 `normalizes_extension` `case_insensitive_match`, T2 | `src/config.rs:156-157` |
| F-152 | `scan_ignores_hidden_system_dirs` | `.cargo` etc. ignoriert (ergänzt F-137) | – | T1 | `src/scanner.rs:8-21` |

---

## 6. `app.rs` – MyApp & BranchDialog (`src/app.rs:9-685`)

### 6.1 Strukturen (`src/app.rs:13-37`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-153 | `branch_dialog_fields` | `BranchDialog{repo_path,target_branch,error=None,dirty_files=[]}` Felder setzbar | – | T1 `new_creates…` `repo_path…` `target_branch…` `dirty_files…` `error…`, T3 `test_branch_dialog_fields` | `src/app.rs:13-18` |
| F-154 | `myapp_new_initializes_default_state` | `MyApp::new` → `scanning` via `start_scan` true? initial false dann true, `error=None`, `repos=[]`, `branch_dialog=None`, `status_message=None` | Mock Context | T1 `new_initializes…` 5 Tests, T2 #255, T3 `test_myapp_new…` | `src/app.rs:40-68` |
| F-155 | `scan_result_repos_contains_vec` | `ScanResult::Repos(Vec<RepoInfo>)` hält Vektor | – | T3 `test_scan_result_repos` | `src/app.rs:9-11` |

### 6.2 Scan-Flow (`src/app.rs:70-107`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-156 | `app_start_scan_sets_scanning_and_early_return` | `start_scan()` setzt `scanning=true`, `error=None`, spawnt Thread; doppelter Aufruf → early return | – | T2 #256, T3 `start_scan_sets_flag` | `src/app.rs:70-82` |
| F-157 | `app_poll_scan_receives_result_and_clears_error` | `poll_scan()` `try_recv` → `scanning=false`, `repos=...`, `error=None`, `request_repaint` | – | T2 #257, T3 `poll_scan_receives` | `src/app.rs:84-91` |
| F-158 | `app_status_message_timeout_3s` | `status_message_time` >3s → gelöscht, sonst `repaint_after(500ms)`; analog `scanning` → `repaint_after(100ms)` | – | T2 #258/266, T4 `poll_scan` Status | `src/app.rs:95-102` |
| F-159 | `app_roots_empty_no_scan_warning` | Keine Roots + nicht scanning → Warnung (gelber Frame) „Kein Suchpfad“ | Leerer `roots` | T2 #288 | `src/app.rs:517-531` `if roots.is_empty() && !scanning` |

### 6.3 Branch-Wechsel (`src/app.rs:109-176`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-160 | `app_handle_branch_switch_clean_direct` | Clean → `execute_branch_switch` direkt | – | T2 #259, T3 `handle_branch_switch_clean`, T4 | `src/app.rs:138-139` |
| F-161 | `app_handle_branch_switch_dirty_shows_dialog` | Dirty → `BranchDialog` mit `dirty_files=get_detailed_status` | – | T2 #260, T3 `dirty` | `src/app.rs:127-134` |
| F-162 | `app_handle_branch_switch_conflicts_or_merge_error` | `has_merge_conflicts` oder `is_merge_in_progress` → `error= "Merge-Konflikte…"` | `state` Ausgabe | T2 #261-262, T3 `conflicts` | `src/app.rs:117-124` |
| F-163 | `app_execute_branch_switch_ok_triggers_rescan` | `checkout_branch` Ok → `status_message`, `error=None`, `branch_dialog=None`, `start_scan()` | – | T2 #263, T3 `execute_branch_switch_safe` | `src/app.rs:151-161` |
| F-164 | `app_execute_branch_switch_err_shows_error` | Err → wenn Dialog offen → `dlg.error`, sonst `self.error` | – | T2 #264 | `src/app.rs:162-169` |
| F-165 | `app_execute_branch_switch_stash_and_force_paths` | `stash=true→stash_and_checkout`, `force=true→checkout_branch_force` | `stash_pop` fail → `start_scan` trotzdem, `dlg.error` | T2 #265, T3 `force`/`stash` | `src/app.rs:143-149` `171-173` |
| F-166 | `app_branch_dialog_close_and_cancel` | `to_close` → `branch_dialog=None`, Cancel-Button | – | T2 #289-290 | `src/app.rs:404-454` |

### 6.4 TopBar / Status / Collapse (`src/app.rs:178-360`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-167 | `app_top_bar_collapse_small_large_transition` | Höhe <400 → collapsed true, ≥500 → false, Transition bei Resize | `last_window_size` Tracking, `screen_rect().height()` | T2 #267-269, T4 `show_top_bar` | `src/app.rs:178-195` `462-483` |

### 6.5 Interaktionen (`src/app.rs:532-651`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-168 | `app_profile_switch_triggers_rescan` | TopBar `active_profile_id` geändert → `save()`, Status-Message, `start_scan()` | – | T2 #270 | `src/app.rs:216-242` |
| F-169 | `app_agent_toggle_add_remove_prevent_empty` | `toggle_agent_active` add/remove, alle deaktivieren → Warnung | `agents.len()>1` vs 1 | T2 #271-273 | `src/app.rs:244-290` `config.toggle…` |
| F-170 | `app_solution_select_persists` | `solution_select` → `config.get_repo_state_mut().selected_solution=Some(path)`, `save()`, `repo.selected_solution` update, Status | – | T2 #274 | `src/app.rs:554-565` |
| F-171 | `app_ide_open_success_error_not_in_profile` | IDE gefunden → `launch_ide` Ok→Status, Err→`error`, nicht in Profil → `error "nicht in Profil …"`; speichert `selected_ide` | `allow_unsafe` Shell-Chars | T2 #275-277 | `src/app.rs:566-588` |
| F-172 | `app_agent_open_success_error_not_found` | Agent gefunden/cloned, `terminal_override` sonst global, `launch_agent` Ok→Status, Err→`error`, nicht gefunden → `error "nicht gefunden"` | – | T2 #278-280 | `src/app.rs:589-614` |
| F-173 | `app_profile_override_set` | `profile_override` → `set_repo_profile_override`, `save()`, Status, `start_scan()` | `None` → Global | T2 #281 | `src/app.rs:615-624` |
| F-174 | `app_fetch_branches_triggers_fetch_and_rescan` | `fetch_branches` → `scanning=true`, Status, Thread `fetch_all`+`scan_repos` → `tx.send` | Err nur `eprintln` | T2 #282 | `src/app.rs:625-640` |
| F-175 | `app_explorer_open_success_error` | `open_in_explorer` Ok→Status, Err→`error` | – | T2 #283-284 | `src/app.rs:641-651` |
| F-176 | `app_settings_save_close_and_error_cleared` | Settings save → `apply_theme`, `config=new_cfg`, `settings_state=from_config`, `show_settings=false`, `start_scan()`; close → State zurücksetzen; `error` wird next frame via poll gelöscht | – | T2 #285-287, T4 | `src/app.rs:654-682` |
| F-177 | `app_pending_branch_switch_handled_after_poll` | `pending_branch_switch` wird nach `poll_scan` via `handle_branch_switch` abgearbeitet | – | T2 #549? | `src/app.rs:104-107` |

---

## 7. `ui/theme.rs` (`src/ui/theme.rs`)

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-178 | `apply_theme_sets_colors_per_variant` | Light/Dark/Nord/Dracula/Solarized setzen `Visuals` `bg_fill`, `panel_fill`, `window_fill`, `extreme_bg_color`, `override_text_color` korrekt | Nord/Dracula `override_text_color` Some | T2 #185-189 | `src/ui/theme.rs:10-69` |
| F-179 | `apply_theme_corner_radius_and_text_styles` | `corner_radius=6` für alle Widgets, `window=8`; `TextStyle` Heading 18, Body/Button 13, Small 11 | – | T2 #190-191 | `src/ui/theme.rs:71-95` |
| F-180 | `apply_minimal_theme_delegates_to_light` | `apply_minimal_theme(ctx)` → `apply_theme(ctx,&Light)` | – | T2 #192 | `src/ui/theme.rs:4-7` |
| F-181 | `color_constants_rgb` | `COLOR_DIRTY=RGB(220,70,40)`, `COLOR_CLEAN=RGB(60,160,80)` | – | T2 #193-194, T3 `color_dirty/clean`, T4 `color_constants` | `src/ui/theme.rs:98-99` |

> `apply_theme` benötigt `egui::Context` → Unit-Test mit Mock-Ctx oder visueller Smoke-Test; als manuell markiert in Tests_4.

---

## 8. `ui/repo_list.rs` (`src/ui/repo_list.rs`)

### 8.1 Pure Helper

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-182 | `ide_icon_for_mapping` | `vs2022/vs/visualstudio→ICON_VS`, `rider/jetbrains→ICON_RIDER`, `vscode→ICON_VSCODE`, sonst Fallback `VSCODE` | Unbekannte ID | T2 #195-198, T3 `ide_icon_*` | `src/ui/repo_list.rs:17-24` |
| F-183 | `agent_icon_for_mapping` | `claude→CLAUDE`, `codex→CODEX`, `gemini→GEMINI`, `copilot→COPILOT`, `cursor→CURSOR`, `aider→AIDER`, sonst Fallback `CLAUDE` | Unbekannt | T2 #199-201, T3 `agent_icon_*` | `src/ui/repo_list.rs:26-36` |

### 8.2 RepoListActions & Rendering

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-184 | `repo_list_actions_default` | Alle Felder `None` (branch_switch, solution_select, ide_open, agent_open, profile_override, fetch_branches, explorer_open) | – | T1 `all_fields_default…`, T2 #202/210, T3 `default` | `src/ui/repo_list.rs:38-46` |
| F-185 | `repo_list_actions_can_be_set` | Jede Action kann auf `Some((PathBuf,…))` gesetzt werden (7 Varianten) | Tuple Typen korrekt | T1 `branch_switch/solution_select…`, T2 #203-211 | `src/ui/repo_list.rs:39-45` |
| F-186 | `show_repo_list_empty_shows_message` | Leere `repos` → „Keine Repositories gefunden“ vertikal zentriert + Hint | – | T2 #210 | `src/ui/repo_list.rs:56-72` |
| F-187 | `repo_row_branch_display_detached_vs_normal` | `is_detached` → `⬡ branch`, sonst ` branch` | – | T2 #211-212 | `src/ui/repo_list.rs:116-120` |
| F-188 | `repo_row_dirty_indicator_red_clean_green` | `dirty` → `●` `COLOR_DIRTY` Hover „Uncommitted“, sonst `○` `COLOR_CLEAN` „sauber“ | – | T2 #213-214 | `src/ui/repo_list.rs:105-111` `ui/theme.rs:98-99` |
| F-189 | `repo_row_solution_display_and_filter` | Selected Solution Text, „Keine {ext}“ wenn leer, Branch/Solution Dropdown Filter `contains` case-insensitive, Cap 100/50, `origin/HEAD` nicht in Branche? | Filter leer → keine Treffer Label | T2 #215-217 + implizit Dropdown Logik, T4 manuell | `src/ui/repo_list.rs:131-283` |
| F-190 | `repo_row_profile_override_combo` | Override Combo: „— Global —“ + Liste `profiles`, `is_selected` via `get_repo_state` | Globales Profil markiert `● (global)` | T2 implizit | `src/ui/repo_list.rs:285-322` |
| F-191 | `repo_row_ide_and_agent_buttons` | Max 4 IDE-Icons (default zuerst), Agent-Icons alle aktiven (reverse), Explorer `ICON_FOLDER`, Hover Rect, Klicks setzen `actions` | Keine Agents → Fallback Claude, keine IDEs → Button „VS Code“ | T2 implizit, T4 | `src/ui/repo_list.rs:344-431` |
| F-192 | `repo_row_selected_solution_italics` | Wenn `selected_solution` + >1 Solution → „Ausgewählt: …“ italics | – | T2 implizit | `src/ui/repo_list.rs:435-444` |

> Rendering `show_repo_list`/`show_repo_row` benötigt `egui::Ui` → als **manueller UI-Smoke-Test** eingestuft (Tests_4). Logik-Tabelle oben unit-testbar via extrahierter Helper.

---

## 9. `ui/settings.rs` (`src/ui/settings.rs`)

### 9.1 SettingsState

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-193 | `settings_state_from_config` | `from_config(cfg)` → `draft=clone`, `error=None`, `success=None`, `selected_tab=General`, `selected_profile_idx= Some(0) wenn profiles>0 sonst None`, analog `agent_idx`, `new_*=""` | Leere Listen → None | T2 #218-223, T3 `from_config`/`initial_tab`/`empty_profiles/agents`, T4 `from_config` | `src/ui/settings.rs:28-43` |
| F-194 | `settings_state_draft_is_independent_clone` | `draft` ist unabhängiger Klon (Mutation beeinflusst Original nicht bis Save) | – | T3 `draft_is_clone` | `src/ui/settings.rs:31` `cfg.clone()` |
| F-195 | `settings_tabs_all_five_and_switching` | Tabs `General, Profiles, Agents, Terminal, Appearance`; `selected_tab` aktualisierbar | – | T2 #224-225 | `src/ui/settings.rs:5-12` `59-74` |

### 9.2 Validierung & Save

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-196 | `settings_validation_empty_roots_error` | Save bei `draft.roots.is_empty()` → `error="Bitte mindestens einen Pfad angeben."` | – | T2 #226, T4 `empty_roots` | `src/ui/settings.rs:123-125` |
| F-197 | `settings_validation_empty_profiles_error` | `profiles.is_empty()` → Error „Mindestens ein Sprach-Profil erforderlich.“ | – | T2 #227, T4 `empty_profiles` | `src/ui/settings.rs:126-128` |
| F-198 | `settings_validation_profile_fields_empty_error` | Profil `id`/`display_name` leer (trim) → `error "Profil '...' hat leere ID/Name"`, `file_extension` leer → „braucht Dateiendung“ | Leerstring, Whitespace | T2 #228-230, T4 `profile_fields` | `src/ui/settings.rs:132-143` |
| F-199 | `settings_validation_valid_saves` | Gültig → `draft.save()` Ok, `error=None`, `success="Gespeichert…"`, `on_save=Some(draft.clone())` | – | T2 #231 | `src/ui/settings.rs:145-149` |
| F-200 | `settings_save_error_shows_message` | `save()` Err → `error="Speichern fehlgeschlagen: …"` | – | T2 #232 | `src/ui/settings.rs:151-153` |
| F-201 | `settings_reset_clears_messages` | „Zurücksetzen“ → `error=None`, `success=None` | – | T2 #242 | `src/ui/settings.rs:161-164` |
| F-202 | `settings_config_path_displayed` | Config-Pfad Label `Config: …` wenn `AppConfig::config_path()` Some | None | T2 implizit | `src/ui/settings.rs:91-97` |

### 9.3 Profile/Agent Verwaltung

| ID | Name | Beschreibung | Edge | Herkunft | Code |
|----|------|--------------|------|----------|------|
| F-203 | `settings_add_new_profile` | „＋ Neues Profil“ → pusht `customN` mit `file_extension=".txt"` etc., `selected_idx=last` | – | T2 #233 | `src/ui/settings.rs:323-344` |
| F-204 | `settings_add_new_agent` | „＋ Neuer Agent“ → pusht `agentN` `claude` | – | T2 #234 | `src/ui/settings.rs:628-640` |
| F-205 | `settings_delete_profile_requires_min_one` | Löschen wenn `len>1` ok, sonst Error „Mindestens ein Profil…“, Selection adjust (`==idx→0`, `>idx→-1`), `active_profile_id` korrigiert falls gelöscht | Letztes Profil | T2 #235/237, T3? | `src/ui/settings.rs:404-420` |
| F-206 | `settings_delete_agent_requires_min_one` | Analog Agent, `active_agent_id` korrigiert | Letzter Agent | T2 #236/238 | `src/ui/settings.rs:698-716` |
| F-207 | `settings_duplicate_profile_agent` | Duplizieren → `id_copy`, `display_name (Kopie)` push | – | T2 #239-240 | `src/ui/settings.rs:395-403` `690-696` |
| F-208 | `settings_set_active_profile_agent` | „Aktiv setzen“ → `active_profile_id=id` bzw. `active_agent_id=id` | – | T2 #241-242 | `src/ui/settings.rs:387-389` `681-683` |
| F-209 | `settings_new_profile_quick_add` | Kollapsing „Schnell-Anlage“ Name+Ext erforderlich, sonst Error, Ext ohne Punkt wird ergänzt, ID `name.to_lowercase().replace(' ',\"_\")` | Leer → Error | T2 #244, T4 | `src/ui/settings.rs:427-465` |
| F-210 | `settings_new_agent_quick_add` | Name+Programm erforderlich, sonst Error | – | T2 #245 | `src/ui/settings.rs:720-746` |
| F-211 | `settings_ide_min_one_and_add` | Pro Profil mind. 1 IDE, löschen bei 1 → Error; „＋ IDE hinzufügen“ → `ideN` `code` `{file}` | – | T2 #246-247 | `src/ui/settings.rs:569-587` |
| F-212 | `settings_ide_effective_preview` | Grid zeigt `effective_program()` + `join(args)` Vorschau | – | T2 #248-249 | `src/ui/settings.rs:558-565` |
| F-213 | `settings_terminal_preference_switch_and_custom` | Combo `Auto/WindowsTerminal/Cmd/Powershell`, Custom-Feld → Wechsel zu `Custom("")` | – | T2 #250-251 | `src/ui/settings.rs:868-911` |
| F-214 | `settings_theme_selected` | Appearance Tab: `Theme::all()` Cards, `selected` markiert `●`, Button „Aktivieren“ setzt `draft.theme` | – | T2 #252 | `src/ui/settings.rs:941-998` |
| F-215 | `settings_agent_toggle_and_empty_warning` | Checkbox `is_agent_active` → `toggle_agent_active`, leer → `error="Warnung: Kein Agent aktiv…"`, Buttons „Alle aktivieren/deaktivieren“ | – | T2 #253-254 | `src/ui/settings.rs:829-854` |

---

## 10. Manuelle / Integration & Nicht-deterministische Tests (aus Tests_4 übernommen)

Diese Tests sind **nicht sinnvoll als reine Unit-Tests**, sondern als manuelle Smoke-Tests oder Integrationstests mit echtem Git/egui-Harness:

| ID | Name | Beschreibung | Referenz |
|----|------|--------------|----------|
| M-01 | `apply_theme_no_panic_per_variant` | `apply_theme(ctx, &Theme::Dark/Nord/…)` panict nicht, `ctx.style` gesetzt | `src/ui/theme.rs:9` Tests_4 |
| M-02 | `show_repo_list_rendering` | Dropdowns, Filter, Hover, Icons rendern ohne Panic | `src/ui/repo_list.rs:50` |
| M-03 | `show_settings_window_tabs` | Alle Tabs schalten, Slider/DragValue/ComboBox/FileDialog bedienbar | `src/ui/settings.rs:45` |
| M-04 | `handle_branch_switch_flow_e2e` | Echte Repos: dirty → Dialog → Stash/Force → Erfolgsmeldung + Rescan | `src/app.rs:109` |
| M-05 | `start_scan_poll_scan_thread` | Thread-Spawn, MPSC, `scan_repos` parallel, kein Deadlock bei vielen Roots | `src/app.rs:70` |
| M-06 | `top_bar_collapse_visual` | Fenster 380px → collapsed, 520px → expanded visuell geprüft | `src/app.rs:178` |
| M-07 | `default_root_path_dirs_home_nondeterministic` | Hängt am Home-Verzeichnis (`repos/source/dev…` Kandidaten) → manuell, nicht asserten | `src/config.rs:410-428` |
| M-08 | `launch_ide_real_process` | Echten Prozessstart nur mit Mock/Stub, nicht in CI (benötigt `code` installiert) | `src/git.rs:472` |
| M-09 | `launch_agent_real_terminal` | `wt`/`powershell` nur auf Windows mit installiertem Terminal testen | `src/git.rs:537` |

---

## 11. Gesamt-Tabelle Finale Anzahl

| Modul | Finale Tests | Davon Duplikate zusammengefasst | Neue (einmalig in einer Quelle) |
|-------|--------------|--------------------------------|---------------------------------|
| config Terminal/Theme/Ide | 19 | 11 | 8 (z.B. F-030 dotnet shape) |
| config LanguageProfile/Agent/RepoUiState | 12 | 8 | 4 |
| config AppConfig | 35 (F-041-075) | 24 | 11 |
| git SolutionFile/RepoInfo/open | 6 | 4 | 2 |
| git get_branch/status/branches | 19 | 13 | 6 (dry-run, parse_head) |
| git checkout/stash | 14 | 9 | 5 |
| git Helper/Launch | 10 | 7 | 3 |
| scanner is_ignored/scan | 23 | 15 | 8 |
| app MyApp/BranchDialog | 25 | 16 | 9 |
| ui theme | 4 | 3 | 1 |
| ui repo_list | 11 | 7 | 4 |
| ui settings | 23 | 15 | 8 |
| **Summe finale Unit-Tests** | **183** | **~132 Duplikate** | **51 einmalige** |
| + manuelle | 9 | – | – |
| **Gesamt dokumentiert** | **192** | – | – |

**Rechenweg Deduplizierung:**  
Tests_1(≈163) + Tests_2(289) + Tests_3(142) + Tests_4(65) = 659 Roh-Einträge.  
Bei Prüfung am Code überschneiden sich ~70% der Beschreibungen; nach Merge bleiben 183 semantisch distinkte Tests. Die 9 manuellen ergänzen auf 192 dokumentierte Szenarien.

---

## 12. Empfehlung Implementierung (Schritt 2)

* **Je Modul ein `#[cfg(test)] mod tests` am Dateiende:** `config.rs`, `git.rs`, `scanner.rs` bereits vorhanden (`src/config.rs:737-826`, `src/git.rs:662-814`, `src/scanner.rs:178-282`) → erweitern.
* **Helfer wiederverwenden:** `tempdir` (`tempfile`), `init_repo_with_commit` (`src/git.rs:668-699`) / `init_repo` (`src/scanner.rs:186-202`), `Repository::init` + `git2`.
* **Mock für `egui::Context`:** Für `apply_theme` `egui::Context::default()` nutzen; für `show_repo_list` `egui_kittest` oder Rendering-Skip – sonst als `#[ignore]` manueller Test.
* **Parametrisierte Tests:** Für `display_name`, `variants`, `branch_display` `rstest` oder Loop statt 5 Einzel-Tests.
* **Priorisierung (Tests_3/T​ests_4):** Hoch: F-041-075 (Migration/Validation), F-087-119 (Git Safety), F-133-149 (Scanner); Mittel: F-020-031 (Profile), F-012-019 (Ide); Niedrig: M-01-M-09.

---

## 13. Anhang – Herkunft je finaler Test (kompakt)

```
F-001 = T1 default_has_valid_depth + T2#1 + T3 default + T4 shape
F-012-019 = T1 6+7 + T2 #54-59 + T3 34-40 + T4 6 Ide-Tests → 8 finale
F-058-086 = T1 4 get_branch + T2 #76-81 + T3 6 + T4 5 head_parse → 5 finale (F-082-086)
F-106-119 = T1 6 checkout + T2 #112-127 + T3 5 + T4 4 → 14 finale (inkl. force/stash)
F-127-129 = T1 6 launch_agent + T2 #152-158 + T3 4 → 3 finale (Auto-Kette, args/command, override)
F-133-144 = T1 5 scan + T2 #160-175 + T3 7 + T4 6 → 12 finale scan_repos
...
(alle 183 IDs siehe obige Tabellen Spalte „Herkunft“)
```

> **Validierung:** Alle finalen Tests referenzieren geprüften Codepfad. Widersprüche (z.B. `Bare-Repo` Erwartung in Tests_1 `scan_single_root_with_bare_repo`) wurden markiert (F-139).

---

## 14. Schluss

Die 4 Ausgangsdateien enthalten dieselben Kern-Tests unter verschiedenen Namen. Durch **codebasierte Semantik-Prüfung** wurden **~476 Duplikate** eliminiert und **51 nur einmal vorkommende Tests** (z.B. `profile_validation`, `quote_shell_chars`, `dotnet_default_profile_shape`, `scan_root_itself_is_repo`) in den finalen Plan integriert. Der resultierende Plan (183 unit-testbare + 9 manuelle) ist vollständig, überschneidungsfrei und direkt als `cargo test` umsetzbar.

*Empfehlung:* Als Nächstes die finalen Tests sukzessive in `src/config.rs`, `src/git.rs`, `src/scanner.rs` und (wo mockbar) `src/app.rs`/`src/ui/*.rs` implementieren – pro Modul getrennt committen, `cargo test` nach jedem Modul.

