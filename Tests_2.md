# Tests_2.md – Testmethoden für RepoManager

> **Hinweis:** Dies ist die Planungsdatei für Testmethoden. Die Implementierung ist ein separater Schritt.
> Alle Tests sind als `#[cfg(test)] mod tests` innerhalb der jeweiligen Module konzipiert (Rust-Unit-Tests),
> mit Ausnahme der UI-Module, die Mock-basierte oder Integrationstests benötigen.

---

## 1. `config.rs` – Konfigurationsmodul

### Strukturen / Enums

#### `AppConfig`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 1 | `default_has_valid_depth` | Default `max_depth` im Bereich 1–10 | Bereits in vorhandenem Test |
| 2 | `default_has_required_fields` | Profile, Agents, active_profile_id nicht leer | Bereits in vorhandenem Test |
| 3 | `save_and_load_roundtrip` | Serialisiert und deserialisiert Config über Datei | Bereits in vorhandenem Test |
| 4 | `add_root_deduplication` | Fügt doppelten Pfad hinzu – Duplikat wird ignoriert | Leere Liste, gleicher Pfad mehrfach |
| 5 | `add_root_sorting` | Pfade werden alphabetisch sortiert nach Einfügung | |
| 6 | `remove_root_existing` | Entfernt einen vorhandenen Root-Pfad | Root existiert, Root existiert nicht |
| 7 | `remove_root_last_one` | Entfernt den letzten Root → Liste wird leer | Initial mit einem Root |
| 8 | `get_active_profile_found` | Gibt aktives Profil zurück, wenn ID existiert | Profil-ID existiert |
| 9 | `get_active_profile_fallback` | Fallback auf erstes Profil, wenn ID nicht existiert | ID existiert nicht |
| 10 | `get_profile_by_id_found` | `get_profile()` findet Profil mit gültiger ID | ID existiert |
| 11 | `get_profile_by_id_not_found` | `get_profile()` gibt `None` bei nicht existierender ID | Leere Profil-Liste |
| 12 | `get_active_agent_single` | Gibt einzelnen aktiven Agenten zurück | `active_agent_ids` hat 1 Eintrag |
| 13 | `get_active_agent_fallback_deprecated` | Fallback auf deprecated `active_agent_id` | `active_agent_ids` leer, deprecated gesetzt |
| 14 | `get_active_agent_empty` | Kein Agent konfiguriert → Fallback auf `agents.first()` | |
| 15 | `get_active_agents_multiple` | Gibt mehrere aktive Agenten zurück | Mehrere aktive, keine aktiven |
| 16 | `is_agent_active_in_list` | Prüft ob Agent in `active_agent_ids` | Agent aktiv / inaktiv |
| 17 | `is_agent_active_deprecated` | Prüft über deprecated `active_agent_id` | Nur deprecated Feld gesetzt |
| 18 | `toggle_agent_active_add` | Fügt Agent zur aktiven Liste hinzu | Agent war inaktiv |
| 19 | `toggle_agent_active_remove` | Entfernt Agent aus aktiver Liste | Letzter Agent wird entfernt |
| 20 | `toggle_agent_active_syncs_deprecated` | Nach Toggle wird `active_agent_id` synchronisiert | Beide Felder konsistent |
| 21 | `get_repo_state_found` | Gibt `RepoUiState` für bekannten Pfad zurück | Pfad existiert in repo_state |
| 22 | `get_repo_state_mut_creates_entry` | `get_repo_state_mut()` erstellt Eintrag wenn nicht vorhanden | |
| 23 | `set_repo_profile_override` | Setzt Profile-Override für ein Repo | Override setzen, auf None setzen |
| 24 | `get_effective_profile_uses_override` | Gibt Override-Profil zurück wenn konfiguriert | Override existiert |
| 25 | `get_effective_profile_fallback_global` | Fallback auf globales Profil wenn kein Override | Override auf ungültige ID |
| 26 | `config_path_returns_path` | Gibt einen Config-Pfad zurück | Plattform-unabhängig |
| 27 | `try_load_nonexistent_config` | Lädt Default wenn Datei nicht existiert | Datei nicht vorhanden |
| 28 | `try_load_invalid_json` | Ungültiges JSON → Default mit Fehlerwarnung | Korrupte Config-Datei |
| 29 | `try_load_migration_v1_to_v2` | Alte Config ohne Version wird migriert | Config v1 mit leeren Profilen |
| 30 | `try_load_migration_add_missing_defaults` | Fehlende Profile/Agents seit v3 werden ergänzt | Config v2 |
| 31 | `try_load_migration_v3_to_v4` | Alte Auto-Profile entfernt, dotnet bleibt | Gemischtes Profil-Set |
| 32 | `try_load_migrate_active_agent_id_to_ids` | Migriert deprecated `active_agent_id` zu `active_agent_ids` | Altes Feld vorhanden |
| 33 | `try_load_max_depth_zero_clamped` | `max_depth == 0` wird auf 2 clamped | |
| 34 | `try_load_max_depth_over_clamped` | `max_depth > 10` wird auf 10 clamped | max_depth = 99 |
| 35 | `try_load_empty_roots_filtered` | Leere Os-Strings in Roots werden entfernt | |
| 36 | `try_load_roots_dedup_sorted` | Duplikate entfernt, Liste sortiert | Mehrfache Duplikate |
| 37 | `try_load_profile_validation` | Profile werden validiert: ID, Extension, max_scan_depth | Leere IDs, depth=0 oder >4 |
| 38 | `try_load_profile_dedup` | Doppelte Profile-IDs werden entfernt (letztes gewinnt) | Mehrere Profile mit gleicher ID |
| 39 | `try_load_active_profile_validated` | Aktives Profil wird auf existierendes Profil gesetzt | aktive_profile_id existiert nicht |
| 40 | `try_load_agents_filtered` | Ungültige Agent-IDs werden aus `active_agent_ids` entfernt | Veraltete IDs in Liste |
| 41 | `try_load_empty_agents_fallback` | Leere agents → Defaults werden geladen | agents-Liste ist leer |
| 42 | `try_load_agents_sync_deprecated` | Sync von `active_agent_ids` zu `active_agent_id` | Beide Felder konsistent |
| 43 | `save_creates_config_directory` | Config-Verzeichnis wird automatisch erstellt | Verzeichnis existiert nicht |

#### `LanguageProfile`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 44 | `normalized_extension_lowercase` | Extension wird kleingeschrieben | `"SLN"` → `".sln"` |
| 45 | `normalized_extension_adds_dot` | Fehlender Punkt wird hinzugefügt | `"sln"` → `".sln"` |
| 46 | `normalized_extension_already_dotted` | Bereits punktbeginnende Extension bleibt unverändert | `".sln"` → `".sln"` |
| 47 | `matches_file_extension` | Datei wird anhand Endung erkannt | `"foo.sln"`, `"FOO.SLN"` |
| 48 | `matches_file_pattern_exact` | Exact-Match-Muster funktioniert | `"Cargo.toml"` |
| 49 | `matches_file_pattern_glob` | Glob-Muster (`*.sln`) funktioniert | `"bar.sln"` ✓, `"bar.rs"` ✗ |
| 50 | `matches_file_pattern_multiple` | Mehrere Muster getrennt durch Komma | `"pom.xml,build.gradle"` |
| 51 | `matches_file_pattern_empty` | Kein Pattern → nur Extension prüfen | `file_pattern: None` |
| 52 | `matches_file_no_extension` | Datei ohne Extension | `"Makefile"` |
| 53 | `matches_file_case_insensitive` | Groß-/Kleinschreibung wird ignoriert | `"FOO.SLN"` matcht `.sln` |

#### `IdeConfig`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 54 | `effective_program_with_program` | Gibt `program` zurück wenn nicht leer | `"code"` → `"code"` |
| 55 | `effective_program_fallback_command` | Fallback auf erstes Wort aus `command` | `command: Some("devenv /something")` |
| 56 | `effective_program_empty_both` | Beide leer → Fallback `"code"` | |
| 57 | `effective_args_with_args` | Gibt `args` zurück wenn nicht leer | `args: vec!["{file}", "--reuse-window"]` |
| 58 | `effective_args_fallback_command` | Fallback: Argumente aus `command` nach erstem Wort | `command: Some("cmd arg1 arg2")` |
| 59 | `effective_args_empty_both` | Beide leer → `vec!["{file}"]` | |

#### `Theme`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 60 | `theme_display_name_light` | → `"Light"` | |
| 61 | `theme_display_name_dark` | → `"Dark"` | |
| 62 | `theme_display_name_nord` | → `"Nord"` | |
| 63 | `theme_display_name_dracula` | → `"Dracula"` | |
| 64 | `theme_display_name_solarized` | → `"Solarized Light"` | |
| 65 | `theme_all_returns_five` | `all()` gibt genau 5 Themes | |
| 66 | `theme_default_is_light` | `Default` für Theme ist Light | |

#### `TerminalPreference`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 67 | `terminal_preference_default` | Default ist `Auto` | |
| 68 | `terminal_preference_all_variants` | Alle Varianten serialisierbar | `Auto`, `WindowsTerminal`, `Cmd`, `Powershell`, `Custom("x")` |

#### `TerminalConfig`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 69 | `terminal_config_default` | Default: `preference=Auto`, `fallback=Cmd` | |

#### `AgentLaunchMode`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 70 | `agent_launch_mode_default` | Default ist `Terminal` | |

#### `RepoUiState`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 71 | `repo_ui_state_default` | Default: alle Felder `None` | |

---

## 2. `git.rs` – Git-Operations-Modul

### Strukturen

#### `RepoInfo`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 72 | `repo_info_new_basic` | Erstellt RepoInfo mit Basis-Feldern | Name aus Pfad abgeleitet |
| 73 | `repo_info_name_from_path` | Name wird aus Dateinamen des Pfads abgeleitet | Pfad mit/ohne Dateiname |
| 74 | `repo_info_with_branches` | Builder-Methode setzt branches korrekt | Leere Liste, mehrere Branches |

#### `SolutionFile`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 75 | `solution_file_creation` | Path und relative Path korrekt | Relativer Pfad zum Repo-Root |

### Funktionen

#### `get_branch`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 76 | `get_branch_main` | Erkennt `main`-Branch korrekt | `detached = false` |
| 77 | `get_branch_master` | Erkennt `master`-Branch (altes Git) | `detached = false` |
| 78 | `get_branch_detached` | Erkennt detached HEAD | `detached = true` |
| 79 | `get_branch_no_commits` | Repo ohne Commits → `"no commits"` | Head nicht setzbar |
| 80 | `get_branch_parse_ref` | HEAD-File parsen für `ref: refs/heads/...` | |
| 81 | `get_branch_parse_oid` | HEAD-File parsen für Commit-OID (7 Zeichen) | |

#### `is_dirty`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 82 | `is_dirty_clean_repo` | Sauberes Repository → `false` | |
| 83 | `is_dirty_modified_file` | Geänderte Datei → `true` | |
| 84 | `is_dirty_untracked_file` | Untracked Datei → `true` | |
| 85 | `is_dirty_staged_file` | Gestagte Datei → `true` | |
| 86 | `is_dirty_ignored_excluded` | Ignorierte Datei nicht gezählt | `.gitignore`-Dateien |
| 87 | `is_dirty_recurse_untracked_dirs` | Untracked-Verzeichnisse rekursiv geprüft | |
| 88 | `is_dirty_bare_repo` | Fehler bei Status-Abfrage → `false` (kein Panic) | |

#### `has_merge_conflicts`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 89 | `has_merge_conflicts_clean` | Sauberes Repository → `false` | |
| 90 | `has_merge_conflicts_with_conflicts` | Repository mit Konflikten → `true` | |
| 91 | `has_merge_conflicts_bare_repo` | Bare Repository → `false` | |

#### `is_merge_in_progress`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 92 | `is_merge_in_progress_merge` | Während Merge → `true` | |
| 93 | `is_merge_in_progress_rebase` | Während Rebase → `true` | |
| 94 | `is_merge_in_progress_clean` | Sauber → `false` | |

#### `get_detailed_status`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 95 | `detailed_status_modified` | → `"modified: datei"` | |
| 96 | `detailed_status_staged` | → `"staged: datei"` | |
| 97 | `detailed_status_untracked` | → `"untracked: datei"` | |
| 98 | `detailed_status_conflicted` | → `"conflict: datei"` | |
| 99 | `detailed_status_empty_repo` | Kein Status → leere Liste | |
| 100 | `detailed_status_multiple_files` | Mehrere Dateien mit verschiedenen Status | |

#### `list_branches`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 101 | `list_branches_local` | Lokale Branches gelistet | |
| 102 | `list_branches_includes_remote` | Remote-Branches mit `origin/`-Präfix | |
| 103 | `list_branches_excludes_head` | `origin/HEAD` wird ausgeschlossen | |
| 104 | `list_branches_dedup` | Duplikate lokal/remote entfernt | `main` und `origin/main` |
| 105 | `list_branches_sorted` | Alphabetisch sortiert | |
| 106 | `list_branches_bare_repo` | → leere Liste | |
| 107 | `list_branches_invalid_repo` | → leere Liste | |

#### `fetch_all`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 108 | `fetch_all_no_remotes` | Kein Remote → `Ok(())` | |
| 109 | `fetch_all_single_remote_fail` | Einzelnes Remote schlägt fehl → Fehler | |
| 110 | `fetch_all_multiple_partial_fail` | Teilweise erfolgreich → Fehler wenn alle fehlgeschlagen | |
| 111 | `fetch_all_invalid_repo` | Ungültiges Repository → Fehler | |

#### `checkout_branch`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 112 | `checkout_to_local` | Wechsel zu lokalem Branch | |
| 113 | `checkout_to_remote` | Remote-Branch → Tracking-Branch wird erstellt | |
| 114 | `checkout_detached` | Zu Commit → detached HEAD | |
| 115 | `checkout_nonexistent` | Branch existiert nicht → Fehler | |
| 116 | `checkout_bare_repo` | → Fehler | |
| 117 | `checkout_merge_in_progress` | Merge/Rebase läuft → Fehler | |
| 118 | `checkout_conflicts` | Merge-Konflikte → Fehler | |
| 119 | `checkout_same_branch` | Wechsel zum aktuellen Branch → kein Fehler | |

#### `checkout_branch_force`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 120 | `checkout_force_overwrites` | Force-Checkout verwirft lokale Änderungen | Dirty Repository |
| 121 | `checkout_force_nonexistent` | → Fehler | |
| 122 | `checkout_force_bare_repo` | → Fehler | |

#### `stash_and_checkout`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 123 | `stash_checkout_clean` | Nichts zu stashen → normaler Checkout | |
| 124 | `stash_checkout_with_changes` | Stash + Checkout + Pop erfolgreich | |
| 125 | `stash_checkout_stash_pop_fail` | Stash-Pop schlägt fehl → Stash bleibt erhalten | |
| 126 | `stash_checkout_checkout_fail` | Checkout fehlgeschlagen → Stash wiederherstellen | |
| 127 | `stash_checkout_nothing_to_stash` | `NotFound` → trotzdem Checkout | |

#### `get_repo_info` / `open_repo`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 128 | `get_repo_info_non_repo` | → `None` | |
| 129 | `get_repo_info_for_repo` | → `Some(RepoInfo)` mit Branch, dirty, branches | |
| 130 | `open_repo_valid` | → `Some(Repository)` | |
| 131 | `open_repo_invalid` | → `None` | |

#### `open_in_explorer`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 132 | `open_in_explorer_valid` | → `Ok(())` | Plattform-spezifisch |
| 133 | `open_in_explorer_invalid` | → Fehler | |

#### `is_program_available`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 134 | `is_program_available_existing` | `"git"` → `true` | |
| 135 | `is_program_available_missing` | `"xyz"` → `false` | |

#### `resolve_vs_path`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 136 | `resolve_vs_path_found` | vswhere.exe gefunden → VS-Pfad | Windows |
| 137 | `resolve_vs_path_not_found` | → `None` | |

#### `substitute_placeholders`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 138 | `sub_file_placeholder` | `{file}` → Dateipfad | |
| 139 | `sub_dir_placeholder` | `{dir}` → Verzeichnispfad | |
| 140 | `sub_repo_placeholder` | `{repo}` → Repo-Pfad | |
| 141 | `sub_multiple_placeholders` | Alle Platzhalter in einem String | |
| 142 | `sub_no_placeholders` | Kein Platzhalter → unverändert | |
| 143 | `sub_deep_nested_path` | Tief verschachtelter Pfad | |

#### `quote_if_needed`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 144 | `quote_no_spaces` | Kein Sonderzeichen → unverändert | `"code"` |
| 145 | `quote_with_spaces` | Leerzeichen → Anführungszeichen | `"C:\Program Files"` |
| 146 | `quote_already_quoted` | Bereits in Anführungszeichen → kein Doppelquoting | |
| 147 | `quote_shell_chars` | Shell-Zeichen werden escaped | `"a & b"` |

#### `launch_ide`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 148 | `launch_ide_basic` | Programm + Args gestartet | |
| 149 | `launch_ide_shell_safe` | Ohne `allow_unsafe` werden `&\|;<>` blockiert | |
| 150 | `launch_ide_shell_allowed` | Mit `allow_unsafe` erlaubt | |
| 151 | `launch_ide_not_found` | Nicht im PATH → Fehler mit Fallback | |

#### `launch_agent`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 152 | `launch_agent_auto_chain` | wt → pwsh → cmd Fallback-Kette | |
| 153 | `launch_agent_windows_terminal` | `wt` bevorzugt | |
| 154 | `launch_agent_powershell` | Powershell | |
| 155 | `launch_agent_cmd` | cmd | |
| 156 | `launch_agent_custom` | Custom-Terminal | |
| 157 | `launch_agent_with_args` | Args korrekt übergeben | |
| 158 | `launch_agent_with_command_template` | Command mit Platzhaltern | |
| 159 | `launch_agent_terminal_override` | Override überschreibt globale Preference | |

---

## 3. `scanner.rs` – Repo-Scanner-Modul

#### `scan_repos`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 160 | `scan_finds_repos_depth_1` | Findet Repos auf Tiefe 1 | ✅ Bereits vorhanden |
| 161 | `scan_respects_max_depth` | Max-Tiefe wird beachtet | ✅ Bereits vorhanden |
| 162 | `scan_ignores_non_repos` | Nicht-Git-Ordner ignoriert | ✅ Bereits vorhanden |
| 163 | `scan_dedup_multiple_roots` | Deduplizierung über mehrere Roots | ✅ Bereits vorhanden |
| 164 | `scan_empty_roots` | Leere Roots → leeres Ergebnis | |
| 165 | `scan_nonexistent_root` | Nicht-existierender Root → überspringen | |
| 166 | `scan_file_root_not_dir` | Root ist Datei → überspringen | |
| 167 | `scan_ignored_dirs_excluded` | `node_modules`, `target` etc. ignoriert | |
| 168 | `scan_solution_files_populated` | Solution-Dateien werden gescannt | |
| 169 | `scan_solution_selection_preserved` | Zuvor ausgewählte Solution bleibt | |
| 170 | `scan_solution_default_first` | Wenn keine Auswahl → erste Solution | |
| 171 | `scan_solution_limit_20` | Maximal 20 Solutions pro Repo | |
| 172 | `scan_solution_sorted` | Root-level zuerst, dann alphabetisch | |
| 173 | `scan_results_sorted` | Repos alphabetisch nach Name sortiert | |
| 174 | `scan_nested_repos` | Verschachtelte Repos erkannt | |
| 175 | `scan_depth_boundary` | Tiefe 0, 1, 3, 10 als Grenzwerte | |

#### `scan_solutions_for_repo`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 176 | `scan_solutions_finds_files` | Matching files gefunden | |
| 177 | `scan_solutions_no_match` | → leere Liste | |
| 178 | `scan_solutions_respects_depth` | Scan-Tiefe beachtet | |
| 179 | `scan_solutions_ignores_dotgit` | `.git` Verzeichnis ignoriert | |
| 180 | `scan_solutions_ignores_ignored_dirs` | `node_modules`, `target` ignoriert | |
| 181 | `scan_solutions_relative_paths` | Relative Pfade korrekt | |
| 182 | `scan_solutions_pattern_multiple` | Mehrere Patterns | `"*.sln,*.csproj"` |

#### `is_ignored_dir`

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 183 | `is_ignored_known` | `node_modules`, `target` → `true` | |
| 184 | `is_ignored_unknown` | Unbekanntes Verzeichnis → `false` | |

---

## 4. `ui/theme.rs` – Theme-Modul

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 185 | `apply_theme_light_colors` | Light-Theme setzt richtige Farben | `bg_fill`, `panel_fill`, `window_fill` |
| 186 | `apply_theme_dark_colors` | Dark-Theme setzt richtige Farben | |
| 187 | `apply_theme_nord_colors` | Nord-Theme + `override_text_color` | |
| 188 | `apply_theme_dracula_colors` | Dracula-Theme + `override_text_color` | |
| 189 | `apply_theme_solarized_colors` | Solarized-Theme + `override_text_color` | |
| 190 | `apply_theme_corner_radius` | Widgets `corner_radius = 6`, `window_corner_radius = 8` | |
| 191 | `apply_theme_text_styles` | Text-Stile (Heading/Body/Button/Small) gesetzt | Font-Größen korrekt |
| 192 | `apply_minimal_theme_delegates` | Ruft `apply_theme(ctx, &Theme::Light)` auf | |
| 193 | `color_dirty_rgb` | `COLOR_DIRTY` = RGB(220, 70, 40) | |
| 194 | `color_clean_rgb` | `COLOR_CLEAN` = RGB(60, 160, 80) | |

---

## 5. `ui/repo_list.rs` – Repo-Liste UI-Modul

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 195 | `ide_icon_for_vs2022` | → ICON_VS | |
| 196 | `ide_icon_for_vscode` | → ICON_VSCODE | |
| 197 | `ide_icon_for_rider` | → ICON_RIDER | |
| 198 | `ide_icon_for_unknown` | Unbekannte IDE-ID → Fallback VSCODE | |
| 199 | `agent_icon_for_claude` | → ICON_CLAUDE | |
| 200 | `agent_icon_for_codex` | → ICON_CODEX | |
| 201 | `agent_icon_for_unknown` | Unbekannte Agent-ID → Fallback CLAUDE | |
| 202 | `repo_list_actions_default` | Alle Felder auf `None` initialisiert | |
| 203 | `repo_list_actions_branch_switch` | `branch_switch` Tuple korrekt | |
| 204 | `repo_list_actions_solution_select` | `solution_select` Tuple korrekt | |
| 205 | `repo_list_actions_ide_open` | `ide_open` Tuple `(PathBuf, String, PathBuf)` korrekt | |
| 206 | `repo_list_actions_agent_open` | `agent_open` Tuple `(PathBuf, String)` korrekt | |
| 207 | `repo_list_actions_profile_override` | `profile_override` Tuple korrekt | |
| 208 | `repo_list_actions_fetch_branches` | `fetch_branches` wird korrekt gesetzt | |
| 209 | `repo_list_actions_explorer_open` | `explorer_open` wird korrekt gesetzt | |
| 210 | `repo_list_empty_repos` | Leere Liste zeigt "Keine Repositories gefunden" | |
| 211 | `repo_branch_display_detached` | Detached-Head → `"⬡ {branch}"` | |
| 212 | `repo_branch_display_normal` | Normaler Branch → `" {branch}"` | |
| 213 | `repo_dirty_indicator` | Dirty-Repo → ● in rot mit Tooltip | |
| 214 | `repo_clean_indicator` | Sauberes Repo → ○ in grün mit Tooltip | |
| 215 | `repo_solution_display_selected` | Selected Solution wird angezeigt | |
| 216 | `repo_solution_display_none` | `"Keine {extension} gefunden"` | |
| 217 | `repo_solution_filter` | Branch-Dropdown filtert korrekt | |

---

## 6. `ui/settings.rs` – Einstellungen UI-Modul

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 218 | `settings_state_from_config` | `draft` = Klone von `cfg`, Tab = General | |
| 219 | `settings_state_empty_profiles` | `selected_profile_idx = None` | |
| 220 | `settings_state_empty_agents` | `selected_agent_idx = None` | |
| 221 | `settings_state_has_profile_idx` | `selected_profile_idx = Some(0)` wenn Profile vorhanden | |
| 222 | `settings_state_has_agent_idx` | `selected_agent_idx = Some(0)` wenn Agents vorhanden | |
| 223 | `settings_state_new_fields_empty` | Neue Feld-Strings initial leer | |
| 224 | `settings_tabs_all_five` | `General`, `Profiles`, `Agents`, `Terminal`, `Appearance` | |
| 225 | `settings_tab_switching` | `selected_tab` wird aktualisiert | |
| 226 | `settings_validation_empty_roots` | Save verweigert bei leeren Roots | Fehler-Message |
| 227 | `settings_validation_empty_profiles` | Save verweigert bei leeren Profilen | |
| 228 | `settings_validation_profile_empty_id` | Profil mit leerer ID → Fehler | |
| 229 | `settings_validation_profile_empty_name` | Profil mit leerem Namen → Fehler | |
| 230 | `settings_validation_profile_empty_extension` | Profil ohne Extension → Fehler | |
| 231 | `settings_validation_valid` | Gültige Config → gespeichert | |
| 232 | `settings_save_error` | Speicherfehler → Fehler-Message | |
| 233 | `settings_add_new_profile` | Neues Profil wird hinzugefügt | ID, Name, Extension |
| 234 | `settings_add_new_agent` | Neuer Agent wird hinzugefügt | ID, Name, Programm |
| 235 | `settings_delete_profile_min_one` | Mindestens 1 Profil erforderlich | Letztes kann nicht gelöscht werden |
| 236 | `settings_delete_agent_min_one` | Mindestens 1 Agent erforderlich | Letzter kann nicht gelöscht werden |
| 237 | `settings_delete_profile_adjusts_selection` | Auswahl nach Löschung angepasst | |
| 238 | `settings_delete_agent_adjusts_selection` | Auswahl nach Löschung angepasst | |
| 239 | `settings_duplicate_profile` | Profil dupliziert mit angepasster ID | |
| 240 | `settings_duplicate_agent` | Agent dupliziert | |
| 241 | `settings_set_active_profile` | Profil wird auf aktiv gesetzt | |
| 242 | `settings_set_active_agent` | Agent wird auf aktiv gesetzt | |
| 243 | `settings_reset` | Error und Success geleert | |
| 244 | `settings_new_profile_quick_add` | Name + Extension erforderlich | |
| 245 | `settings_new_agent_quick_add` | Name + Programm erforderlich | |
| 246 | `settings_ide_minimum_one` | Mindestens eine IDE pro Profil | Letzte IDE kann nicht gelöscht werden |
| 247 | `settings_add_ide_to_profile` | Neue IDE wird hinzugefügt | |
| 248 | `settings_ide_effective_program` | `effective_program()` korrekt berechnet | |
| 249 | `settings_ide_effective_args` | `effective_args()` korrekt berechnet | |
| 250 | `settings_terminal_preference_switch` | Terminal-Preference wird gewechselt | |
| 251 | `settings_terminal_custom` | Custom-Terminal wird gesetzt | |
| 252 | `settings_theme_selected` | Theme wird ausgewählt | |
| 253 | `settings_agent_toggle` | Agent aktivieren/deaktivieren | |
| 254 | `settings_toggle_empty_warning` | Alle deaktiviert → Warnung | |

---

## 7. `app.rs` – Hauptanwendung (Integrationstests)

Da `MyApp` stark von egui abhängt, sind hier Mock-basierte Ansätze und reine Logik-Tests sinnvoll:

| # | Testmethode | Beschreibung | Randfälle / Edge Cases |
|---|------------|--------------|----------------------|
| 255 | `app_new_initializes` | `MyApp::new()` erstellt App mit Default-Werten | Mock-Context |
| 256 | `app_start_scan_sets_scanning` | `start_scan()` setzt `scanning = true` | Doppelter Aufruf → early return |
| 257 | `app_poll_scan_receives_result` | `poll_scan()` verarbeitet Scan-Ergebnis | |
| 258 | `app_poll_scan_status_timeout` | Status-Nachricht wird nach 3s entfernt | |
| 259 | `app_handle_branch_switch_clean` | Kein dirty → direkter Checkout | |
| 260 | `app_handle_branch_switch_dirty` | Dirty → BranchDialog wird geöffnet | |
| 261 | `app_handle_branch_switch_conflicts` | Konflikte → Fehler-Message | |
| 262 | `app_handle_branch_merge_in_progress` | Merge/Rebase → Fehler | |
| 263 | `app_execute_branch_switch_ok` | Checkout erfolgreich → Status-Message + Scan | |
| 264 | `app_execute_branch_switch_err` | Checkout fehlgeschlagen → Error-Message | |
| 265 | `app_stash_checkout_err_stash_pop` | Stash-Pop fehlgeschlagen → neuer Scan | |
| 266 | `app_status_message_timeout_expires` | `status_message_time` > 3s → Nachricht entfernt | |
| 267 | `app_top_bar_collapse_small` | Fensterhöhe < 400 → collapsed | |
| 268 | `app_top_bar_collapse_large` | Fensterhöhe ≥ 500 → nicht collapsed | |
| 269 | `app_top_bar_collapse_transition` | Fenster wird kleiner → Collapse-Logik | |
| 270 | `app_profile_switch_triggers_scan` | Profil-Change → neuer Scan + Status-Message | |
| 271 | `app_agent_toggle_add` | Agent aktivieren | |
| 272 | `app_agent_toggle_remove` | Agent deaktivieren | |
| 273 | `app_agent_toggle_prevent_empty` | Alle deaktivieren → Warnung | |
| 274 | `app_solution_select` | Solution auswählen → gespeichert in config | |
| 275 | `app_ide_open_success` | IDE startet → Status-Message | |
| 276 | `app_ide_open_error` | IDE nicht gefunden → Error | |
| 277 | `app_ide_open_not_in_profile` | IDE nicht in Profil → Error | |
| 278 | `app_agent_open_success` | Agent startet → Status-Message | |
| 279 | `app_agent_open_error` | Agent nicht gestartet → Error | |
| 280 | `app_agent_open_not_found` | Agent-ID nicht gefunden → Error | |
| 281 | `app_profile_override_set` | Profil-Override für Repo gesetzt | |
| 282 | `app_fetch_branches` | Fetch + Rescan gestartet | |
| 283 | `app_explorer_open` | Explorer geöffnet → Status-Message | |
| 284 | `app_explorer_open_error` | Explorer fehlgeschlagen → Error | |
| 285 | `app_settings_save` | Settings speichern → neues Config + Scan | |
| 286 | `app_settings_close` | Settings schließen → State zurücksetzen | |
| 287 | `app_error_cleared_next_frame` | Fehler wird nach Poll gelöscht | |
| 288 | `app_roots_empty_no_scan` | Keine Roots + nicht scanning → Warnung | |
| 289 | `app_branch_dialog_close` | Branch-Dialog schließen | |
| 290 | `app_branch_dialog_cancel` | Abbrechen → Dialog geschlossen | |

---

## 8. Edge-Cases-Übersicht

### Globale Randfälle
- **Leere/Null-Werte**: leere Listen, `None`-Werte, leere Strings
- **Ungültige Eingaben**: ungültige Pfade, nicht existierende IDs, fehlende Extensions
- **Randwerte**: `max_depth = 0, 1, 10, 11`, Tiefe 0
- **Fehlerbehandlung**: Repo-Öffnung fehlgeschlagen, Fetch-Fehler, Stash-Fehler, Checkout-Fehler
- **Deduplizierung**: doppelte Roots, doppelte Profile, doppelte Branches
- **Zustandsmaschine**: Branch-Dialog → stash → checkout → pop-Fehler
- **Migration**: alte Config-Versionen → neue Format-Versionen
- **Plattform**: Windows vs. Unix (explorer, cmd, terminal)
- **Concurrency**: MPSC-Kanal für Scan-Ergebnisse
- **Resource Limits**: max 20 Solutions, max 5 Scan-Tiefe, max 4 IDE-Buttons

---

## Zusammenfassung

| Modul | Getestet | Neu hinzuzufügen |
|-------|----------|-----------------|
| `config.rs` | 5 Tests | ~43 Tests |
| `git.rs` | ~12 Tests | ~80+ Tests |
| `scanner.rs` | 4 Tests | ~22 Tests |
| `ui/theme.rs` | 0 Tests | ~10 Tests |
| `ui/repo_list.rs` | 0 Tests | ~13 Tests |
| `ui/settings.rs` | 0 Tests | ~37 Tests |
| `app.rs` | 0 Tests | ~36 Tests |
| **Gesamt** | **~21 Tests** | **~241+ neue Tests** |