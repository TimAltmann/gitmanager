# Test Plan - RepoManager

## Übersicht
Dieser Test-Plan beschreibt alle vorgeschlagenen Tests für die Modules/Klassen des RepoManager-Projekts.

---

## 1. Module: `config.rs`

### 1.1 TerminalPreference
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_terminal_preference_default_is_auto` | Default von TerminalPreference ist Auto | - |
| `test_terminal_preference_serialization` | Serialisierung/Deserialisierung aller Varianten | Custom(String) mit Sonderzeichen |
| `test_terminal_preference_equality` | PartialEq Vergleich aller Varianten | - |

### 1.2 TerminalConfig
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_terminal_config_default` | Default hat Auto als preference, Cmd als fallback | - |
| `test_terminal_config_serialization` | Serialize/Deserialize Roundtrip | - |

### 1.3 Theme
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_theme_default_is_light` | Default Theme ist Light | - |
| `test_theme_display_name` | display_name() liefert korrekte Namen | Alle 5 Themes |
| `test_theme_all_returns_five` | all() gibt 5 Themes zurück | - |
| `test_theme_serialization` | Roundtrip aller Theme-Varianten | - |

### 1.4 IdeConfig
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_ide_effective_program_with_program` | Gibt program zurück wenn gesetzt | Leerer String |
| `test_ide_effective_program_from_command` | Fallback auf command wenn program leer | command mit/ohne Leerzeichen |
| `test_ide_effective_program_default` | Fallback "code" wenn beides leer | - |
| `test_ide_effective_args_with_args` | Gibt args zurück wenn gesetzt | Leere Args |
| `test_ide_effective_args_from_command` | Parse args aus command string | command nur ein Wort |
| `test_ide_effective_args_default` | Fallback ["{file}"] wenn nichts gesetzt | - |
| `test_ide_serialization` | Roundtrip mit allen Feldern | optionale Felder None/Some |

### 1.5 LanguageProfile
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_normalized_extension_adds_dot` | "sln" -> ".sln" | Bereits mit Punkt |
| `test_normalized_extension_lowercase` | ".SLN" -> ".sln" | Mixed Case |
| `test_normalized_extension_trims` | " .sln " -> ".sln" | Whitespace |
| `test_matches_file_with_pattern_wildcard` | "*.sln" matched alle .sln | Groß/Klein |
| `test_matches_file_with_pattern_exact` | "Cargo.toml" matched nur genau | - |
| `test_matches_file_without_pattern` | Extension-Matching ohne pattern | Datei ohne Extension |
| `test_matches_file_multiple_patterns` | "pom.xml,build.gradle" komma-getrennt | Leerzeichen um Komma |
| `test_matches_file_no_match` | Kein Match wenn Extension nicht passt | - |
| `test_profile_serialization` | Roundtrip mit allen Feldern | optionale Felder |

### 1.6 AgentLaunchMode
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_agent_launch_mode_default_is_terminal` | Default ist Terminal | - |
| `test_agent_launch_mode_serialization` | Roundtrip Terminal/Detached | - |

### 1.7 AgentProfile
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_agent_profile_defaults` | optionale Felder haben sinnvolle Defaults | - |
| `test_agent_profile_serialization` | Roundtrip mit allen Feldern | terminal_override None/Some |

### 1.8 RepoUiState
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_repo_ui_state_default` | Default hat None für alle Optionen | - |
| `test_repo_ui_state_serialization` | Roundtrip mit selected_solution | - |

### 1.9 AppConfig
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_config_default_has_valid_state` | Defaults sind konsistent | - |
| `test_config_add_root` | Pfad wird hinzugefügt | Duplikat, leere Pfade |
| `test_config_add_root_no_duplicates` | Doppelte Pfade werden nicht hinzugefügt | - |
| `test_config_remove_root` | Pfad wird entfernt | Nicht vorhandener Pfad |
| `test_config_get_active_profile` | Gibt aktives Profil zurück | Ungültige active_profile_id |
| `test_config_get_profile` | Gibt Profil nach ID zurück | Nicht existierende ID |
| `test_config_get_active_agent` | Bevorzugt active_agent_ids[0] | Leere Liste, deprecated Feld |
| `test_config_get_active_agents` | Gibt alle aktiven Agents zurück | Keine aktiven |
| `test_config_is_agent_active` | Prüft ob Agent aktiv ist | - |
| `test_config_toggle_agent_active` | Aktiv/Deaktiviert Toggle | Erstes Element deaktivieren |
| `test_config_toggle_agent_active_prevent_empty` | Verhindere leere Agent-Liste | - |
| `test_config_get_repo_state` | Gibt Repo-UI-State zurück | Erstes Mal vs. existierend |
| `test_config_get_repo_state_mut` | Erstellt neuen State wenn nicht vorhanden | - |
| `test_config_set_repo_profile_override` | Setzt Profil-Override | None = Global |
| `test_config_get_effective_profile_for_repo` | Gibt Override oder aktives Profil zurück | - |
| `test_config_migration_v2_to_v4` | Migration von v2 Config | - |
| `test_config_migration_v3_to_v4` | Migration von v3 Config | - |
| `test_config_max_depth_clamping` | max_depth 0->2, >10->10 | - |
| `test_config_roots_dedup_sort` | Roots werden sortiert und dedupliziert | - |
| `test_config_profile_validation` | Profile werden validiert (ID, Extension, Depth) | - |
| `test_config_agent_ids_validation` | Ungültige agent_ids werden gefiltert | - |

---

## 2. Module: `git.rs`

### 2.1 SolutionFile
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_solution_file_creation` | Erstellung mit Pfad und relativem Pfad | - |
| `test_solution_file_equality` | PartialEq Vergleich | - |

### 2.2 RepoInfo
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_repo_info_new` | Erstellung mit Name-Extraktion aus Pfad | Pfad ohne Dateinamen |
| `test_repo_info_with_branches` | Builder-Pattern für Branches | Leere Liste |
| `test_repo_info_name_from_path` | Name wird aus letztem Pfad-Element extrahiert | - |

### 2.3 Öffentliche Funktionen
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_open_repo_valid` | Gibt Repository für gültigen Pfad | - |
| `test_open_repo_invalid` | Gibt None für ungültigen Pfad | Nicht existierend |
| `test_get_branch_on_main` | Erkennt main/master Branch | - |
| `test_get_branch_detached` | Erkennt detached HEAD | - |
| `test_is_dirty_clean` | Clean Repo ist nicht dirty | - |
| `test_is_dirty_modified` | Modified Datei -> dirty | - |
| `test_is_dirty_untracked` | Untracked Datei -> dirty | - |
| `test_is_dirty_staged` | Staged Datei -> dirty | - |
| `test_has_merge_conflicts_clean` | Keine Konflikte -> false | - |
| `test_has_merge_conflicts_with_conflicts` | Konflikte -> true | - |
| `test_is_merge_in_progress_clean` | Kein Merge -> false | - |
| `test_is_merge_in_progress_during_merge` | Während Merge -> true | - |
| `test_get_detailed_status_clean` | Leere Liste bei clean | - |
| `test_get_detailed_status_various` | Verschiedene Statustypen | - |
| `test_list_branches_includes_local` | Lokale Branches werden aufgelistet | - |
| `test_list_branches_includes_remote` | Remote Branches werden aufgelistet | - |
| `test_list_branches_dedup` | Duplikate werden entfernt | - |
| `test_list_branches_bare_repo` | Bare Repo gibt leere Liste | - |
| `test_checkout_branch_safe` | Safe Checkout funktioniert | - |
| `test_checkout_branch_blocks_on_conflicts` | Blockiert bei Konflikten | - |
| `test_checkout_branch_blocks_on_merge` | Blockiert bei Merge in Progress | - |
| `test_checkout_branch_blocks_on_bare` | Blockiert bei Bare Repo | - |
| `test_checkout_branch_nonexistent` | Fehler bei nicht existierendem Branch | - |
| `test_checkout_branch_force` | Force Checkout funktioniert | - |
| `test_stash_and_checkout` | Stash + Checkout + Pop | Nichts zu stashen |
| `test_stash_and_checkout_conflict` | Stash pop mit Konflikt | - |
| `test_get_repo_info_for_repo` | Gibt Some für gültiges Repo | - |
| `test_get_repo_info_for_non_repo` | Gibt None für Nicht-Repo | - |
| `test_is_program_available_true` | Programm im PATH | - |
| `test_is_program_available_false` | Programm nicht im PATH | - |
| `test_substitute_placeholders_file` | {file} wird ersetzt | - |
| `test_substitute_placeholders_dir` | {dir} wird ersetzt | - |
| `test_substitute_placeholders_repo` | {repo} wird ersetzt | - |
| `test_quote_if_needed_with_spaces` | Pfad mit Leerzeichen wird zitiert | - |
| `test_quote_if_needed_without_spaces` | Pfad ohne Leerzeichen unverändert | - |
| `test_quote_if_needed_already_quoted` | Bereits zitierter Pfad unverändert | - |

---

## 3. Module: `scanner.rs`

### 3.1 Konstanten
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_ignored_dirs_contains_common` | Enthält node_modules, target, etc. | - |

### 3.2 Funktionen
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_scan_repos_empty_roots` | Leere Roots -> leere Liste | - |
| `test_scan_repos_nonexistent_root` | Nicht existierender Root wird übersprungen | - |
| `test_scan_repos_file_as_root` | Datei als Root wird übersprungen | - |
| `test_scan_repos_depth_1` | Findet direkte Kinder | - |
| `test_scan_repos_depth_2` | Findet Enkel | - |
| `test_scan_repos_respects_max_depth` | Tiefe wird respektiert | - |
| `test_scan_repos_ignores_node_modules` | node_modules wird ignoriert | - |
| `test_scan_repos_ignores_target` | target (Rust) wird ignoriert | - |
| `test_scan_repos_ignores_venv` | venv/.venv wird ignoriert | - |
| `test_scan_repos_ignores_git_dir` | .git wird ignoriert | - |
| `test_scan_repos_dedup_across_roots` | Gleiche Repos über mehrere Roots | - |
| `test_scan_repos_sorted_by_name` | Ergebnis ist sortiert | - |
| `test_scan_solutions_finds_matching` | Findet passende Dateien | - |
| `test_scan_solutions_ignores_non_matching` | Ignoriert nicht-passende Dateien | - |
| `test_scan_solutions_respects_profile_depth` | Profil-Tiefe wird respektiert | - |
| `test_scan_solutions_ignores_git_dir` | .git wird ignoriert | - |
| `test_scan_solutions_ignores_node_modules` | Ignorierte Dirs werden ignoriert | - |
| `test_scan_solutions_max_20` | Maximal 20 Lösungen | - |
| `test_scan_solutions_sorted_by_depth` | Sortiert nach Tiefe | - |

---

## 4. Module: `app.rs`

### 4.1 ScanResult
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_scan_result_repos` | Enthält Vec<RepoInfo> | - |

### 4.2 BranchDialog
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_branch_dialog_fields` | Alle Felder korrekt befüllbar | - |

### 4.3 MyApp (Integrationstests)
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_myapp_new_creates_valid_state` | Konstruktor erstellt gültigen Zustand | - |
| `test_myapp_start_scan_sets_flag` | Scan-Flag wird gesetzt | - |
| `test_myapp_poll_scan_receives_result` | Empfängt Scan-Ergebnis | - |
| `test_myapp_handle_branch_switch_clean` | Clean Repo -> direkter Wechsel | - |
| `test_myapp_handle_branch_switch_dirty` | Dirty Repo -> Dialog | - |
| `test_myapp_handle_branch_switch_conflicts` | Konflikte -> Fehler | - |
| `test_myapp_execute_branch_switch_safe` | Safe Checkout | - |
| `test_myapp_execute_branch_switch_force` | Force Checkout | - |
| `test_myapp_execute_branch_switch_stash` | Stash + Checkout | - |

---

## 5. Module: `ui/repo_list.rs`

### 5.1 RepoListActions
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_repo_list_actions_default` | Alle Felder None | - |

### 5.2 Funktionen
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_ide_icon_for_vscode` | Gibt VS Code Icon | - |
| `test_ide_icon_for_vs2022` | Gibt VS Icon | - |
| `test_ide_icon_for_rider` | Gibt Rider Icon | - |
| `test_ide_icon_for_unknown` | Fallback auf VS Code Icon | - |
| `test_agent_icon_for_claude` | Gibt Claude Icon | - |
| `test_agent_icon_for_unknown` | Fallback auf Claude Icon | - |

---

## 6. Module: `ui/settings.rs`

### 6.1 SettingsState
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_settings_state_from_config` | Erstellt State aus Config | - |
| `test_settings_state_draft_is_clone` | Draft ist unabhängiger Klon | - |
| `test_settings_state_initial_tab` | Initialer Tab ist General | - |
| `test_settings_state_empty_profiles` | selected_profile_idx ist None | - |
| `test_settings_state_empty_agents` | selected_agent_idx ist None | - |

---

## 7. Module: `ui/theme.rs`

### 7.1 Konstanten
| Test-Methode | Beschreibung | Edge Cases |
|-------------|--------------|------------|
| `test_color_dirty_is_redish` | COLOR_DIRTY hat erwarteten Farbwert | - |
| `test_color_clean_is_greenish` | COLOR_CLEAN hat erwarteten Farbwert | - |

---

## Zusammenfassung

| Module | Existierende Tests | Neue Tests | Gesamt |
|--------|-------------------|------------|--------|
| config.rs | 5 | 45 | 50 |
| git.rs | 9 | 35 | 44 |
| scanner.rs | 4 | 20 | 24 |
| app.rs | 0 | 9 | 9 |
| ui/repo_list.rs | 0 | 8 | 8 |
| ui/settings.rs | 0 | 5 | 5 |
| ui/theme.rs | 0 | 2 | 2 |
| **Gesamt** | **18** | **124** | **142** |

---

## Priorisierung

### Hoch (Kernfunktionalität)
1. AppConfig - Agent-Management, Repo-State, Migration
2. Git - merge_conflicts, merge_in_progress, detailed_status, stash_and_checkout
3. Scanner - Ignored Dirs, Solution-Scanning

### Mittel (Wichtige Features)
4. LanguageProfile - Pattern-Matching, Multiple Patterns
5. IdeConfig - Fallback-Logik
6. MyApp - Branch-Handling Logik

### Niedrig (UI/Edge Cases)
7. UI State/Defaults
8. Theme-Konstanten
9. Icon-Mapping
