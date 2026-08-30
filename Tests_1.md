# Tests_1.md - Test Method Descriptions for All Classes

## 1. SolutionFile (src/git.rs:6-9)
- `new_creates_struct_with_path_and_relative` — constructs SolutionFile, verifies path and relative fields
- `debug_clone_preserves_values` — clone preserves both path and relative
- `partial_eq_equality_when_fields_match` — two SolutionFiles with equal fields are equal

## 2. RepoInfo (src/git.rs:13-22)
- `new_creates_with_defaults` — `RepoInfo::new()` initializes branches/empty, solutions/empty, selected_solution/None
- `with_branches_adds_branches` — `with_branches()` mutates and returns self with updated branches
- `name_derived_from_path` — name field derived from path file_name or display string
- `selected_solution_defaults_to_none` — selected_solution is None by default
- `all_fields_accessible` — all pub fields readable after construction

## 3. BranchDialog (src/app.rs:13-18)
- `new_creates_with_all_fields` — constructs BranchDialog, verifies all four fields
- `repo_path_accessible` — repo_path field accessible
- `target_branch_accessible` — target_branch field accessible  
- `dirty_files_defaults_to_empty` — dirty_files defaults to empty Vec
- `error_defaults_to_none` — error defaults to None

## 4. MyApp (src/app.rs:20-37)
- `new_initializes_with_default_state` — `MyApp::new()` sets scanning=false, error=None, repos=[], branch_dialog=None
- `scanning_starts_as_false` — initial scanning state is false
- `repos_empty_initialy` — initial repos vector is empty
- `error_none_initialy` — initial error is None
- `branch_dialog_none_initialy` — initial branch_dialog is None

## 5. AppConfig (src/config.rs:352-384)
- `default_has_valid_depth_between_1_and_10` — default max_depth is 2 (clamped 2–10)
- `default_profile_is_dotnet` — default active_profile_id is "dotnet"
- `default_agents_populated` — default agents vec has 6 entries (claude, codex, gemini, copilot, cursor, aider)
- `save_and_load_roundtrip_preserves_fields` — serialize/deserialize preserves all fields
- `add_root_ adds_unique_sorted` — `add_root()` adds path if not present, keeps sorted
- `remove_root_ removes_matching` — `remove_root()` removes matching path
- `config_version_migrates_v2_v3_v4` — try_load migrates old configs through v2→v3→v4 rules
- `max_depth_clamped_between_2_and_10` — max_depth clamped to [2, 10] if out of range
- `roots_deduped_on_load` — duplicate roots removed during config load
- `get_effective_profile_for_repo_returns_active` — returns active profile or first profile fallback
- `set_repo_profile_override_updates_state` — `set_repo_profile_override()` updates repo state
- `normalized_profile_ids_are_lowercase` — profile IDs are lowercased during validation
- `add_root_twin_though_duplicate_prevented` — adding same root twice is prevented
- `remove_root_twin_when_not_present` — removing non-existent root is no-op

## 6. LanguageProfile (src/config.rs:142-193)
- `normalized_extension_adds_dot_prefix` — `normalized_extension()` adds "." if missing
- `normalized_extension_lowercases` — extension is lowercased
- `matches_file_with_pattern_wildcard` — `*.sln` pattern matches .sln files case-insensitively
- `matches_file_with_exact_pattern` — exact name pattern like "Cargo.toml" matches that file
- `matches_file_case_insensitive` — pattern matching is case-insensitive
- `matches_file_no_pattern_uses_extension` — without file_pattern, matches by extension only
- `matches_file_custom_pattern` — custom patterns (non-*) are exact filename matches
- `matches_file_returns_false_for_wrong_extension` — wrong extension returns false
- `normalized_extension_trims_whitespace` — leading/trailing whitespace stripped
- `file_pattern_comma_separated` — multiple patterns separated by comma are evaluated independently

## 7. IdeConfig (src/config.rs:95-110)
- `effective_program_returns_program_if_set` — if program set, returns it
- `effective_program_falls_back_to_code` — if program empty, falls back to "code"
- `effective_program_falls_back_from_command` — if program empty, uses first word from command
- `effective_args_returns_args_if_set` — if args set, returns them
- `effective_args_uses_first_word_from_command` — if no args but command, uses parts[1..]
- `effective_args_uses_file_placeholder` — if neither, returns ["{file}"]
- `effective_args_handles_command_with_placeholders` — command parsing respects placeholder syntax

## 8. AgentProfile (src/config.rs:258-270)
- `new_creates_with_defaults` — constructs AgentProfile, verifies all fields including defaults
- `launch_terminal_defaults_to_terminal` — default launch_mode is Terminal
- `launch_detached_defaults_to_detached` — launch_mode::Detached is valid
- `terminal_override_defaults_to_none` — terminal_override is None by default
- `args_defaults_to_empty` — args defaults to empty Vec
- `command_defaults_to_none` — command defaults to None

## 9. TerminalPreference (src/config.rs:21-33)
- `variants_are_auto_windowsterminal_cmd_powershell_custom` — all five variants exist
- `default_is_auto` — `Default` impl returns `TerminalPreference::Auto`
- `serde_roundtrip_preserves_variant` — roundtrip through JSON preserves variant name
- `custom_variant_stores_string` — `Custom(String)` variant holds the string
- `all_variants_have_rename_all_snake_case` — serde rename_all ensures snake_case serialization

## 10. Theme (src/config.rs:55-91)
- `variants_are_light_dark_nord_dracula_solarized` — five theme variants exist
- `light_has_display_name_light` — `display_name()` returns "Light"
- `dark_has_display_name_dark` — `display_name()` returns "Dark"
- `nord_has_display_name_nord` — returns "Nord"
- `dracula_has_display_name_dracula` — returns "Dracula"
- `solarized_has_display_name_solarized_light` — returns "Solarized Light"
- `all_returns_all_five` — `Theme::all()` returns all five variants
- `serde_roundtrip_preserves_theme` — JSON roundtrip preserves theme enum
- `light_is_default` — `Default` impl returns `Theme::Light`

## 11. RepoUiState (src/config.rs:341-348)
- `defaults_to_none_for_both_fields` — both fields default via `#[serde(default)]`
- `selected_solution_can_be_path` — selected_solution can hold Option<PathBuf>
- `selected_ide_can_be_string` — selected_ide can hold Option<String>
- `profile_override_can_be_option` — profile_override can hold Option<String>

## 12. RepoListActions (src/ui/repo_list.rs:38-46)
- `all_fields_default_to_none_or_empty` — all fields are Option types, default to None/empty
- `branch_switch_can_be_set` — branch_switch set to Some((path, branch))
- `solution_select_can_be_set` — solution_select set to Some((path, sln_path))
- `ide_open_can_be_set` — ide_open set to Some((path, ide_id, file_path))
- `agent_open_can_be_set` — agent_open set to Some((path, agent_id))
- `profile_override_can_be_set` — profile_override set to Some((path, profile_id))
- `fetch_branches_can_be_set` — fetch_branches set to Some(path)
- `explorer_open_can_be_set` — explorer_open set to Some(path)

## 13. Git Functions (src/git.rs) — Additional Edge Case Tests
- `open_repo_returns_none_for_non_git_path` — non-git path returns None
- `open_repo_returns_some_for_valid_repo` — valid git repo returns Some(Repository)
- `get_branch_returns_branch_and_false_for_local_branch` — normal branch returns (name, false)
- `get_branch_returns_short_oid_and_true_for_detached` — detached HEAD returns short oid, true
- `get_branch_returns_name_and_false_from_head_file` — HEAD file parsing returns branch name
- `get_branch_returns_default_on_error` — error case returns ("no commits", false)
- `is_dirty_returns_false_for_clean_repo` — clean working directory returns false
- `is_dirty_returns_true_for_modified_file` — modified file returns true
- `is_dirty_returns_true_for_untracked_file` — untracked file returns true (with include_untracked)
- `is_dirty_ignores_ignored_files` — ignored files (e.g. node_modules) not counted
- `has_merge_conflicts_returns_fresh_repo` — clean repo index has no conflicts
- `has_merge_conflicts_returns_true_with_conflicts` — repo with conflicted files returns true
- `is_merge_in_progress_returns_false_for_clean` — clean state returns false
- `is_merge_in_progress_returns_true_in_merge` — merge-in-progress returns true
- `list_branches_returns_empty_for_bare_repo` — bare repo returns empty Vec
- `list_branches_returns_local_branches` — local branches are listed
- `list_branches_returns_remote_branches` — remote branches (origin/...) are listed
- `list_branches_deduplicates_same_name` — if local "main" and remote "origin/main" both exist, both kept with prefix
- `list_branches_filters_head_remote` — "origin/HEAD" is filtered out
- `fetch_all_succeeds_with_no_remotes` — no remotes is ok
- `fetch_all_fails_with_single_failing_remote` — one failing remote raises error (len==1)
- `fetch_all_warns_but_succeeds_with_mixed` — some remotes fail, some succeed, returns Ok
- `open_in_explorer_windows_spawns_explorer` — Windows: spawns explorer with path
- `open_in_explorer_linux_uses_xdg_open` — Linux: uses xdg-open
- `open_in_explorer_mac_uses_open` — macOS: uses open
- `open_in_explorer_fails_gracefully_missing_path` — missing path handled gracefully
- `checkout_branch_succeeds_on_clean_repo` — clean repo, existing branch → Ok
- `checkout_branch_fails_on_merge_in_progress` — merge in progress → Err
- `checkout_branch_fails_on_merge_conflicts` — merge conflicts → Err
- `checkout_branch_fails_on_nonexistent_branch` — branch not found → Err
- `checkout_branch_dry_run_detects_conflicts` — dry-run detects tree conflicts
- `checkout_branch_force_overwrites_local_changes` — force checkout discards local modifications
- `stash_and_checkout_success_on_clean` — clean repo: stash (nothing to stash) + checkout OK
- `stash_and_checkout_warns_nothing_to_stash` — no changes: stash warning, but continues
- `stash_and_checkout_preserves_stash_on_failure` — checkout fails: stash pop attempted, stash kept
- `is_program_available_returns_true_for_common_tools` — "which"/"where" finds common programs
- `is_program_available_returns_false_for_unknown` — unknown program returns false
- `resolve_vs_path_returns_none_when_not_installed` — vswhere not present → None
- `resolve_vs_path_returns_path_when_installed` — vswhere present → Some(PathBuf)
- `launch_ide_effective_program_vscode` — IdeConfig with program="code" → effective_program="code"
- `launch_ide_effective_program_devenv` — IdeConfig with program="devenv" → effective_program="devenv"
- `launch_ide_effective_program_fallback` — empty program → "code"
- `launch_ide_effective_args_with_file_arg` — args=["{file}"] → returned as-is
- `launch_ide_effective_args_from_command` — command="code {file}" → args=["{file}"]
- `launch_ide_validation_blocks_shell_chars` — if allow_unsafe=false, args with &|;()> bail
- `launch_ide_validation_passes_when_allow_unsafe` — allow_unsafe=true bypasses check
- `launch_agent_windows_terminal_preference` — TerminalPreference::WindowsTerminal uses `wt -d <dir> -- cmd /k`
- `launch_agent_powershell_preference` — uses pwsh/powershell -NoExit -Command "Set-Location ..."
- `launch_agent_cmd_preference` — uses cmd /C start "" /D <dir> cmd /K
- `launch_agent_auto_prefers_wt` — if wt available, uses it; otherwise falls back powershell/cmd
- `launch_agent_auto_falls_back_to_cmd` — if no wt/powershell, falls back to cmd
- `launch_agent_custom_uses_specified_terminal` — Custom preference launches specified terminal
- `launch_agent_with_args` — agent args are quote-if-needed wrapped
- `launch_agent_with_command_containing_placeholders` — command with {file}/{dir}/{repo} substituted
- `launch_agent_with_command_no_placeholders` — plain command used as-is

## 14. Scanner Functions (src/scanner.rs) — Additional Edge Case Tests
- `scan_finds_repos_at_depth_1` — depth 1 finds direct child repos
- `scan_respects_max_depth` — depth 1 skips repos in subdirectories; depth 2 finds them
- `scan_ignores_non_repos` — non-git directories produce no RepoInfo
- `scan_dedup_multiple_roots` — duplicate root paths deduplicated (same path twice in roots vec)
- `scan_ignores_node_modules` — node_modules directory skipped
- `scan_ignores_target` — target directory skipped
- `scan_ignores_dotgit` — .git directory not treated as repo root
- `scan_ignores_hidden_system_dirs` — .cargo, .idea, .venv etc. are ignored
- `scan_single_root_with_bare_repo` — bare repo not included as RepoInfo
- `scan_single_root_with_non_git_dir` — non-git directory filtered out
- `scan_dedup_across_different_roots` — same repo discovered via different root paths deduplicated
- `scan_solutions_for_repo_with_dotnet_profile` — .sln files found with .NET profile
- `scan_solutions_for_repo_with_rust_profile` — .rs/.toml files found with Rust profile
- `scan_solutions_normalizes_extension` — extension ".SOL" normalized to ".sln"
- `scan_solutions_matches_file_with_pattern` — file_pattern "*.sln,*.cs" matches both
- `scan_solutions_no_file_pattern_uses_default` — no file_pattern → matches by normalized extension
- `scan_solutions_case_insensitive_match` — case-insensitive extension matching
- `scan_solutions_truncates_at_20` — more than 20 solutions truncated
- `scan_solutions_preserves_selected_from_state` — selected_solution restored from repo_state
- `scan_solutions_defaults_to_first_when_no_selection` — if no selected_solution in state, first solution used
- `scan_repos_with_multiple_roots` — multiple roots scanned, repos from all roots collected
- `scan_repos_respects_max_depth_per_root` — each root scanned independently with max_depth
- `scan_repos_skips_nonexistent_root` — root path that doesn't exist is skipped with eprintln
- `scan_repos_skips_non_directory_root` — root that is file not directory is skipped
- `scan_rayon_parallelism` — scan uses rayon for parallel iteration (no panic with many roots)