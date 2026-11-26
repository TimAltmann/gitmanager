use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    En,
    De,
}

impl Language {
    pub fn code(&self) -> &'static str {
        match self {
            Language::En => "en",
            Language::De => "de",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Language::En => "English",
            Language::De => "Deutsch",
        }
    }

    pub fn all() -> Vec<Language> {
        vec![Language::En, Language::De]
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Translate a key for the given language.
/// Fallback is English if key not found.
pub fn tr(lang: Language, key: &str) -> String {
    // We use match on (lang, key) for compile-time safety and performance.
    // Keys are snake_case identifiers.
    match (lang, key) {
        // ── App / Top Bar ──────────────────────────────────────────────
        (Language::En | Language::De, "app_title") => "GitManager",
        (Language::En | Language::De, "repositories") => "Repositories",
        (Language::En, "profile_label") => "Profile:",
        (Language::De, "profile_label") => "Profil:",
        (Language::En | Language::De, "ai_label") => "AI:",
        (Language::En, "settings") => "Settings",
        (Language::De, "settings") => "Einstellungen",
        (Language::En, "settings_tooltip") => "Configure paths, profiles, agents & depth",
        (Language::De, "settings_tooltip") => "Pfade, Profile, Agents & Tiefe konfigurieren",
        (Language::En, "refresh") => "Refresh",
        (Language::De, "refresh") => "Aktualisieren",
        (Language::En, "scanning") => "Scanning...",
        (Language::De, "scanning") => "Scanne...",
        (Language::En, "search_paths") => "Search paths:",
        (Language::De, "search_paths") => "Suchpfade:",
        (Language::En, "depth_label") => "Depth:",
        (Language::De, "depth_label") => "Tiefe:",
        (Language::En, "not_found") => "(not found)",
        (Language::De, "not_found") => "(nicht gefunden)",
        (Language::En, "language") => "Language",
        (Language::De, "language") => "Sprache",

        // ── Status Bar ─────────────────────────────────────────────────
        (Language::En | Language::De, "status_repos_paths") => "Repos",
        (Language::En, "status_paths") => "paths",
        (Language::De, "status_paths") => "Pfade",
        (Language::En, "scanning_repos") => "Scanning for repositories...",
        (Language::De, "scanning_repos") => "Scanne nach Repositories...",
        (Language::En, "no_search_path") => "No search path configured. Open settings and add a folder.",
        (Language::De, "no_search_path") => "Kein Suchpfad konfiguriert. Öffne die Einstellungen und füge einen Ordner hinzu.",

        // ── Branch dialog ──────────────────────────────────────────────
        (Language::En, "branch_switch_title") => "Switch branch",
        (Language::De, "branch_switch_title") => "Branch wechseln",
        (Language::En, "branch_dirty_msg") => "Uncommitted changes present. How to proceed?",
        (Language::De, "branch_dirty_msg") => "Uncommitted changes vorhanden. Wie fortfahren?",
        (Language::En, "affected_files") => "affected files:",
        (Language::De, "affected_files") => "betroffene Dateien:",
        (Language::En, "cancel") => "Cancel",
        (Language::De, "cancel") => "Abbrechen",
        (Language::En | Language::De, "stash_switch") => "Stash & Switch",
        (Language::En, "stash_switch_tooltip") => "Stash changes, switch branch, then pop",
        (Language::De, "stash_switch_tooltip") => "Änderungen stashen, Branch wechseln, stash pop",
        (Language::En | Language::De, "force_discard") => "Force Discard",
        (Language::En, "force_discard_tooltip") => "Discard local changes and switch (dangerous)",
        (Language::De, "force_discard_tooltip") => "Lokale Änderungen verwerfen und wechseln (gefährlich)",
        (Language::En, "branch_tip") => "Tip: 'Stash & Switch' keeps changes, 'Force' discards them.",
        (Language::De, "branch_tip") => "Tipp: 'Stash & Switch' behält Änderungen, 'Force' verwirft sie.",
        (Language::En, "branch_conflict_error") => "Branch switch not possible",
        (Language::De, "branch_conflict_error") => "Branch-Wechsel nicht möglich",

        // ── Branch-Wechsel auf Remote-Branch (E15) ─────────────────────
        (Language::En, "branch_local_ahead") => "Local branch {} is {} commits ahead – local state checked out",
        (Language::De, "branch_local_ahead") => "Lokaler Branch {} ist {} Commits voraus – lokaler Stand ausgecheckt",
        (Language::En, "branch_local_ahead_one") => "Local branch {} is 1 commit ahead – local state checked out",
        (Language::De, "branch_local_ahead_one") => "Lokaler Branch {} ist 1 Commit voraus – lokaler Stand ausgecheckt",
        (Language::En, "branch_local_diverged") => "Local branch {} differs ({} ahead, {} behind) – local state checked out",
        (Language::De, "branch_local_diverged") => "Lokaler Branch {} weicht ab ({} voraus, {} hinterher) – lokaler Stand ausgecheckt",
        (Language::En, "branch_fast_forwarded") => "Fast-forwarded local branch {} by {} commits",
        (Language::De, "branch_fast_forwarded") => "Lokaler Branch {} um {} Commits vorgespult",
        (Language::En, "branch_fast_forwarded_one") => "Fast-forwarded local branch {} by 1 commit",
        (Language::De, "branch_fast_forwarded_one") => "Lokaler Branch {} um 1 Commit vorgespult",
        (Language::En, "branch_upstream_mismatch") => "Local branch {} tracks {}",
        (Language::De, "branch_upstream_mismatch") => "Lokaler Branch {} verfolgt {}",
        (Language::En, "branch_checked_out_elsewhere") => "Branch {} is checked out in {}",
        (Language::De, "branch_checked_out_elsewhere") => "Branch {} ist in {} ausgecheckt",
        (Language::En, "branch_switched") => "Switched to branch “{}”: {}",
        (Language::De, "branch_switched") => "Branch zu „{}“ gewechselt: {}",
        (Language::En, "branch_switch_failed") => "Branch switch failed: {}",
        (Language::De, "branch_switch_failed") => "Branch-Wechsel fehlgeschlagen: {}",
        (Language::En, "branch_switch_blocked") => "Cannot switch to branch “{}”: merge conflicts or a merge/rebase in progress ({})",
        (Language::De, "branch_switch_blocked") => "Branch-Wechsel zu „{}“ nicht möglich: Merge-Konflikte oder Merge/Rebase läuft ({})",

        // ── Repo list ──────────────────────────────────────────────────
        (Language::En, "no_repos_found") => "No repositories found",
        (Language::De, "no_repos_found") => "Keine Repositories gefunden",
        (Language::En, "no_repos_hint") => "Check the path in settings and increase search depth if needed.",
        (Language::De, "no_repos_hint") => "Pfad in den Einstellungen prüfen und ggf. Suchtiefe erhöhen.",
        (Language::En, "dirty_tooltip") => "Uncommitted changes present",
        (Language::De, "dirty_tooltip") => "Uncommitted changes vorhanden",
        (Language::En, "clean_tooltip") => "Working directory clean",
        (Language::De, "clean_tooltip") => "Arbeitsverzeichnis sauber",
        // ── Claude Sessions ───────────────────────────────────────────
        (Language::En | Language::De, "view_claude") => "Claude Sessions",
        (Language::En, "view_claude_tt") => "Claude Code sessions in your repos and worktrees",
        (Language::De, "view_claude_tt") => "Claude-Code-Sessions in deinen Repos und Worktrees",
        (Language::En, "claude_search_hint") => "Search sessions (Ctrl+F)",
        (Language::De, "claude_search_hint") => "Sessions durchsuchen (Strg+F)",
        (Language::En, "claude_none_found") => "No Claude sessions found",
        (Language::De, "claude_none_found") => "Keine Claude Sessions gefunden",
        (Language::En, "claude_none_hint") => "Looking in {} for sessions inside your search paths.",
        (Language::De, "claude_none_hint") => "Gesucht wird in {} nach Sessions in deinen Suchpfaden.",
        (Language::En, "claude_no_matches") => "No matches for “{}”",
        (Language::De, "claude_no_matches") => "Keine Treffer für „{}“",
        (Language::En, "claude_resume") => "Resume",
        (Language::De, "claude_resume") => "Fortsetzen",
        (Language::En, "claude_resume_tt") => "Resume session in {} with Claude Code",
        (Language::De, "claude_resume_tt") => "Session in {} mit Claude Code fortsetzen",
        (Language::En, "claude_scanning") => "Searching Claude sessions…",
        (Language::De, "claude_scanning") => "Suche Claude Sessions…",
        (Language::En, "claude_status") => "{} open · {} sessions",
        (Language::De, "claude_status") => "{} offen · {} Sessions",
        (Language::En, "claude_open_section") => "Open ({})",
        (Language::De, "claude_open_section") => "Offen ({})",
        (Language::En | Language::De, "claude_all_section") => "Sessions ({})",
        (Language::En, "claude_st_waiting") => "Waiting for you",
        (Language::De, "claude_st_waiting") => "Wartet auf dich",
        (Language::En, "claude_st_idle") => "Done",
        (Language::De, "claude_st_idle") => "Fertig",
        (Language::En, "claude_st_shell") => "Running command…",
        (Language::De, "claude_st_shell") => "Führt Befehl aus…",
        (Language::En, "claude_focus_tt") => "Bring terminal to front",
        (Language::De, "claude_focus_tt") => "Terminal in den Vordergrund holen",
        (Language::En, "claude_sln_fav_add") => "Prefer this solution (preselected next time)",
        (Language::De, "claude_sln_fav_add") => "Bevorzugen (nächstes Mal vorausgewählt)",
        (Language::En, "claude_sln_fav_remove") => "No longer prefer",
        (Language::De, "claude_sln_fav_remove") => "Nicht mehr bevorzugen",
        (Language::En, "claude_sln_folder") => "Folder only",
        (Language::De, "claude_sln_folder") => "Nur Ordner",
        (Language::En, "claude_focus_failed") => "Terminal window not found",
        (Language::De, "claude_focus_failed") => "Terminal-Fenster nicht gefunden",
        (Language::En, "claude_waiting_tt") => "{} waiting for you",
        (Language::De, "claude_waiting_tt") => "{} warten auf dich",
        // ── Repo-Liste: Suche & Worktrees ─────────────────────────────
        (Language::En, "repo_search_hint") => "Search repos, branches, paths… (Ctrl+F)",
        (Language::De, "repo_search_hint") => "Repos, Branches, Pfade durchsuchen… (Strg+F)",
        (Language::En, "repo_search_clear") => "Clear search",
        (Language::De, "repo_search_clear") => "Suche leeren",
        (Language::En, "repo_search_no_matches") => "No repositories or worktrees match “{}”",
        (Language::De, "repo_search_no_matches") => "Keine Repositories oder Worktrees passen zu „{}“",
        (Language::En | Language::De, "repo_section_repos") => "Repositories",
        (Language::En | Language::De, "repo_section_worktrees") => "Worktrees",
        (Language::En, "claude_notify_label") => "Notify when a Claude session needs you",
        (Language::De, "claude_notify_label") => "Benachrichtigen, wenn eine Claude Session dich braucht",
        (Language::En, "claude_notify_hint") => "Windows notification when a running session waits for an answer or has finished. Click it to bring the terminal to the front.",
        (Language::De, "claude_notify_hint") => "Windows-Benachrichtigung, wenn eine laufende Session auf eine Antwort wartet oder fertig ist. Ein Klick darauf holt das Terminal nach vorne.",
        (Language::En, "claude_notify_waiting") => "Claude is waiting for you",
        (Language::De, "claude_notify_waiting") => "Claude wartet auf dich",
        (Language::En, "claude_notify_done") => "Claude has finished",
        (Language::De, "claude_notify_done") => "Claude ist fertig",
        (Language::En, "claude_notify_btn_terminal") => "Open terminal",
        (Language::De, "claude_notify_btn_terminal") => "Terminal öffnen",
        (Language::En, "claude_notify_btn_show") => "Show in GitManager",
        (Language::De, "claude_notify_btn_show") => "In GitManager anzeigen",
        (Language::En, "claude_notify_attr_done") => "done",
        (Language::De, "claude_notify_attr_done") => "fertig",
        (Language::En, "claude_notify_attr_done_after") => "done after {}",
        (Language::De, "claude_notify_attr_done_after") => "fertig nach {}",
        (Language::En, "claude_notify_attr_waiting") => "waiting for you",
        (Language::De, "claude_notify_attr_waiting") => "wartet auf dich",
        (Language::En, "claude_notify_attr_waiting_after") => "question after {}",
        (Language::De, "claude_notify_attr_waiting_after") => "Rückfrage nach {}",
        (Language::En, "claude_preview_show") => "Show recent messages",
        (Language::De, "claude_preview_show") => "Letzte Nachrichten zeigen",
        (Language::En, "claude_preview_hide") => "Hide recent messages",
        (Language::De, "claude_preview_hide") => "Letzte Nachrichten ausblenden",
        (Language::En, "claude_preview_empty") => "No messages found.",
        (Language::De, "claude_preview_empty") => "Keine Nachrichten gefunden.",
        (Language::En, "claude_preview_you") => "You",
        (Language::De, "claude_preview_you") => "Du",
        (Language::En, "claude_new_button") => "New Claude session",
        (Language::De, "claude_new_button") => "Neue Claude Session",
        (Language::En, "claude_new_tooltip") => "Start a new Claude Code session in a repository",
        (Language::De, "claude_new_tooltip") => "Neue Claude-Code-Session in einem Repository starten",
        (Language::En, "claude_new_title") => "New Claude session",
        (Language::De, "claude_new_title") => "Neue Claude Session",
        (Language::En, "claude_new_hint") => "Choose the folder the session starts in.",
        (Language::De, "claude_new_hint") => "Wähle den Ordner, in dem die Session startet.",
        (Language::En, "claude_new_search_hint") => "Search repositories and worktrees…",
        (Language::De, "claude_new_search_hint") => "Repositories und Worktrees durchsuchen…",
        (Language::En, "claude_new_no_matches") => "No folder matches the search.",
        (Language::De, "claude_new_no_matches") => "Kein Ordner passt zur Suche.",
        (Language::En, "claude_new_no_folders") => "No repositories found yet – set the search paths or a default folder in the settings.",
        (Language::De, "claude_new_no_folders") => "Noch keine Repositories gefunden – Suchpfade oder einen Standard-Ordner in den Einstellungen festlegen.",
        (Language::En, "claude_new_start") => "Start",
        (Language::De, "claude_new_start") => "Starten",
        (Language::En, "claude_new_default_badge") => "default folder",
        (Language::De, "claude_new_default_badge") => "Standard-Ordner",
        (Language::En, "folder_files_label") => "Files for new worktrees and Claude sessions",
        (Language::De, "folder_files_label") => "Dateien für neue Worktrees und Claude Sessions",
        (Language::En, "folder_files_hint") => "Created in a new worktree and in the folder of a new Claude session – only if the file does not exist there yet. Path relative to the folder, e.g. dev_config.xml or config/local.json.",
        (Language::De, "folder_files_hint") => "Werden in einem neuen Worktree und im Ordner einer neuen Claude Session angelegt – nur, wenn die Datei dort noch fehlt. Pfad relativ zum Ordner, z. B. dev_config.xml oder config/local.json.",
        (Language::En, "folder_files_path") => "Path",
        (Language::De, "folder_files_path") => "Pfad",
        (Language::En, "folder_files_path_hint") => "e.g. dev_config.xml",
        (Language::De, "folder_files_path_hint") => "z. B. dev_config.xml",
        (Language::En, "folder_files_content") => "Content",
        (Language::De, "folder_files_content") => "Inhalt",
        (Language::En, "folder_files_add") => "+ Add file",
        (Language::De, "folder_files_add") => "+ Datei hinzufügen",
        (Language::En, "folder_files_remove") => "Remove",
        (Language::De, "folder_files_remove") => "Entfernen",
        (Language::En, "folder_files_invalid") => "Path must stay inside the folder (relative, without ..)",
        (Language::De, "folder_files_invalid") => "Pfad muss im Ordner bleiben (relativ, ohne ..)",
        (Language::En, "folder_files_written") => "created: {}",
        (Language::De, "folder_files_written") => "angelegt: {}",
        (Language::En, "folder_files_failed") => "{} not created: {}",
        (Language::De, "folder_files_failed") => "{} nicht angelegt: {}",
        (Language::En, "claude_dir_label") => "Default folder for new Claude sessions",
        (Language::De, "claude_dir_label") => "Standard-Ordner für neue Claude Sessions",
        (Language::En, "claude_dir_hint") => "Preselected in “New Claude session”. Empty = last used repository.",
        (Language::De, "claude_dir_hint") => "Wird bei „Neue Claude Session“ vorausgewählt. Leer = zuletzt benutztes Repository.",
        (Language::En, "claude_dir_placeholder") => "last used repository",
        (Language::De, "claude_dir_placeholder") => "zuletzt benutztes Repository",
        (Language::En, "wt_new_button") => "New worktree",
        (Language::De, "wt_new_button") => "Neuer Worktree",
        (Language::En, "wt_new_tooltip") => "Create a new worktree for a repository",
        (Language::De, "wt_new_tooltip") => "Neuen Worktree für ein Repository anlegen",
        (Language::En, "wt_new_for_repo_tooltip") => "Create a new worktree of this repository",
        (Language::De, "wt_new_for_repo_tooltip") => "Neuen Worktree dieses Repositories anlegen",
        (Language::En, "wt_of_repo") => "worktree of {}",
        (Language::De, "wt_of_repo") => "Worktree von {}",
        (Language::En, "wt_no_base_repo") => "No repository found to create a worktree from",
        (Language::De, "wt_no_base_repo") => "Kein Repository gefunden, von dem ein Worktree angelegt werden kann",
        (Language::En, "wt_dialog_title") => "New worktree",
        (Language::De, "wt_dialog_title") => "Neuer Worktree",
        (Language::En | Language::De, "wt_repo_label") => "Repository:",
        (Language::En | Language::De, "wt_branch_label") => "Branch:",
        (Language::En, "wt_mode_existing") => "Existing branch",
        (Language::De, "wt_mode_existing") => "Vorhandener Branch",
        (Language::En, "wt_mode_new") => "New branch",
        (Language::De, "wt_mode_new") => "Neuer Branch",
        (Language::En | Language::De, "wt_new_branch_name") => "Name:",
        (Language::En, "wt_start_point") => "Based on:",
        (Language::De, "wt_start_point") => "Basierend auf:",
        (Language::En, "wt_start_head") => "Current HEAD",
        (Language::De, "wt_start_head") => "Aktueller HEAD",
        (Language::En, "wt_branch_in_use") => "Already checked out in {}",
        (Language::De, "wt_branch_in_use") => "Bereits ausgecheckt in {}",
        (Language::En, "wt_target_label") => "Folder:",
        (Language::De, "wt_target_label") => "Ordner:",
        (Language::En, "wt_target_suggest") => "Suggestion",
        (Language::De, "wt_target_suggest") => "Vorschlag",
        (Language::En, "wt_target_suggest_tooltip") => "Use the suggested folder again",
        (Language::De, "wt_target_suggest_tooltip") => "Wieder den vorgeschlagenen Ordner verwenden",
        (Language::En, "wt_default_root_hint") => "Default folder: {} (Settings › General)",
        (Language::De, "wt_default_root_hint") => "Standard-Ordner: {} (Einstellungen › Allgemein)",
        (Language::En, "wt_default_next_to_repo_hint") => "Default: next to the repository (change in Settings › General)",
        (Language::De, "wt_default_next_to_repo_hint") => "Standard: neben dem Repository (änderbar unter Einstellungen › Allgemein)",
        (Language::En, "wt_outside_search_paths") => "This folder is outside the search paths: the worktree is created but not shown in the list.",
        (Language::De, "wt_outside_search_paths") => "Dieser Ordner liegt außerhalb der Suchpfade: Der Worktree wird angelegt, erscheint aber nicht in der Liste.",
        (Language::En, "wt_create") => "Create worktree",
        (Language::De, "wt_create") => "Worktree anlegen",
        (Language::En, "wt_creating") => "Creating worktree…",
        (Language::De, "wt_creating") => "Worktree wird angelegt…",
        (Language::En, "rm_wt_title") => "Remove worktree",
        (Language::De, "rm_wt_title") => "Worktree entfernen",
        (Language::En, "rm_wt_question") => "Remove worktree {}?",
        (Language::De, "rm_wt_question") => "Worktree {} entfernen?",
        (Language::En, "rm_wt_delete_branch") => "Delete local branch {}",
        (Language::De, "rm_wt_delete_branch") => "Lokalen Branch {} löschen",
        (Language::En, "rm_wt_branch_unsafe") => "Branch is not fully pushed or merged",
        (Language::De, "rm_wt_branch_unsafe") => "Branch ist weder vollständig gepusht noch gemergt",
        (Language::En, "rm_wt_dirty") => "{} uncommitted changes",
        (Language::De, "rm_wt_dirty") => "{} ungespeicherte Änderungen",
        (Language::En, "rm_wt_ahead") => "{} unpushed commits",
        (Language::De, "rm_wt_ahead") => "{} ungepushte Commits",
        (Language::En, "rm_wt_detached") => "Detached HEAD with commits that no branch contains",
        (Language::De, "rm_wt_detached") => "Losgelöster HEAD mit Commits, die in keinem Branch liegen",
        (Language::En, "rm_wt_remove") => "Remove",
        (Language::De, "rm_wt_remove") => "Entfernen",
        (Language::En, "rm_wt_remove_anyway") => "Remove anyway",
        (Language::De, "rm_wt_remove_anyway") => "Trotzdem entfernen",
        (Language::En, "rm_wt_claude") => "A Claude session is running in this folder",
        (Language::De, "rm_wt_claude") => "Eine Claude Session läuft in diesem Ordner",
        (Language::En, "rm_wt_running") => "Removing worktree…",
        (Language::De, "rm_wt_running") => "Worktree wird entfernt…",
        (Language::En, "rm_wt_changed") => "State has changed – please confirm again",
        (Language::De, "rm_wt_changed") => "Zustand hat sich geändert – bitte erneut bestätigen",
        (Language::En, "rm_wt_done") => "Worktree {} removed",
        (Language::De, "rm_wt_done") => "Worktree {} entfernt",
        (Language::En, "rm_wt_failed") => "Removing worktree {} failed: {}",
        (Language::De, "rm_wt_failed") => "Worktree {} entfernen fehlgeschlagen: {}",
        (Language::En, "cleanup_title") => "Clean up",
        (Language::De, "cleanup_title") => "Aufräumen",
        (Language::En, "cleanup_scanning") => "Fetching and looking for things to clean up…",
        (Language::De, "cleanup_scanning") => "Fetch läuft, suche nach Aufräumbarem…",
        (Language::En, "cleanup_nothing") => "Nothing to clean up",
        (Language::De, "cleanup_nothing") => "Nichts aufzuräumen",
        (Language::En, "cleanup_scan_failed") => "The search was aborted",
        (Language::De, "cleanup_scan_failed") => "Die Suche wurde abgebrochen",
        (Language::En, "cleanup_gitlab_unavailable") => "GitLab not available – merged merge requests were not checked",
        (Language::De, "cleanup_gitlab_unavailable") => "GitLab nicht verfügbar – gemergte Merge Requests wurden nicht geprüft",
        (Language::En, "cleanup_reason_mr") => "MR !{} merged",
        (Language::De, "cleanup_reason_mr") => "MR !{} gemergt",
        (Language::En, "cleanup_open_mr") => "Open MR !{}",
        (Language::De, "cleanup_open_mr") => "Offener MR !{}",
        (Language::En, "cleanup_reason_gone") => "Remote branch deleted",
        (Language::De, "cleanup_reason_gone") => "Remote-Branch gelöscht",
        (Language::En, "cleanup_dirty") => "{} changes",
        (Language::De, "cleanup_dirty") => "{} Änderungen",
        (Language::En, "cleanup_ahead") => "↑{} unpushed",
        (Language::De, "cleanup_ahead") => "↑{} ungepusht",
        (Language::En, "cleanup_branch_unmerged") => "not merged – local commits will be lost",
        (Language::De, "cleanup_branch_unmerged") => "nicht gemergt – lokale Commits gehen verloren",
        (Language::En | Language::De, "cleanup_branch_in") => "in {}",
        (Language::En, "cleanup_delete_branch") => "Delete branch too",
        (Language::De, "cleanup_delete_branch") => "Branch mitlöschen",
        (Language::En, "cleanup_remove_n") => "Remove {}",
        (Language::De, "cleanup_remove_n") => "{} entfernen",
        (Language::En, "cleanup_running") => "Cleaning up…",
        (Language::De, "cleanup_running") => "Räume auf…",
        (Language::En, "cleanup_ok") => "removed",
        (Language::De, "cleanup_ok") => "entfernt",
        (Language::En, "cleanup_changed") => "State has changed – skipped",
        (Language::De, "cleanup_changed") => "Zustand hat sich geändert – übersprungen",
        (Language::En, "cleanup_branch_kept") => "worktree removed; branch changed meanwhile and was kept",
        (Language::De, "cleanup_branch_kept") => "Worktree entfernt; Branch hat sich geändert und wurde behalten",
        (Language::En, "cleanup_failed") => "Error: {}",
        (Language::De, "cleanup_failed") => "Fehler: {}",
        (Language::En, "pull_running") => "Pulling…",
        (Language::De, "pull_running") => "Pull läuft…",
        (Language::En, "pull_ff") => "{}: pulled {} commits",
        (Language::De, "pull_ff") => "{}: {} Commits übernommen",
        (Language::En, "pull_ff_one") => "{}: pulled 1 commit",
        (Language::De, "pull_ff_one") => "{}: 1 Commit übernommen",
        (Language::En, "pull_up_to_date") => "{}: already up to date",
        (Language::De, "pull_up_to_date") => "{}: schon aktuell",
        (Language::En, "pull_dirty") => "{}: {} local changes – commit or stash first",
        (Language::De, "pull_dirty") => "{}: {} lokale Änderungen – erst committen oder stashen",
        (Language::En, "pull_diverged") => "{}: branch has diverged (↑{} ↓{}) – please resolve manually",
        (Language::De, "pull_diverged") => "{}: Branch ist divergiert (↑{} ↓{}) – bitte manuell lösen",
        (Language::En, "pull_no_upstream") => "{}: no upstream branch configured",
        (Language::De, "pull_no_upstream") => "{}: kein Upstream-Branch eingerichtet",
        (Language::En, "pull_detached") => "{}: detached HEAD – nothing to pull",
        (Language::De, "pull_detached") => "{}: losgelöster HEAD – nichts zu pullen",
        (Language::En, "pull_changed") => "{}: branch changed while fetching – nothing changed",
        (Language::De, "pull_changed") => "{}: Branch hat sich während des Abrufs geändert – nichts geändert",
        (Language::En, "pull_failed") => "Pull failed: {}",
        (Language::De, "pull_failed") => "Pull fehlgeschlagen: {}",
        (Language::En, "wt_wait_busy") => "Waiting for the running scan/fetch…",
        (Language::De, "wt_wait_busy") => "Warte auf laufenden Scan/Fetch…",
        (Language::En, "wt_target_not_absolute") => "Please enter an absolute folder path.",
        (Language::De, "wt_target_not_absolute") => "Bitte einen absoluten Ordnerpfad angeben.",
        (Language::En, "wt_invalid_branch") => "Invalid branch name: {}",
        (Language::De, "wt_invalid_branch") => "Ungültiger Branch-Name: {}",
        (Language::En, "wt_branch_exists") => "Branch {} already exists – choose it under “Existing branch”.",
        (Language::De, "wt_branch_exists") => "Branch {} existiert bereits – unter „Vorhandener Branch“ auswählen.",
        (Language::En, "wt_created") => "Worktree for {} created in {}",
        (Language::De, "wt_created") => "Worktree für {} in {} angelegt",
        (Language::En, "wt_root_label") => "Default folder for new worktrees",
        (Language::De, "wt_root_label") => "Standard-Ordner für neue Worktrees",
        (Language::En, "wt_root_hint") => "Suggested when creating a worktree from the repository list. Empty = next to the repository.",
        (Language::De, "wt_root_hint") => "Wird beim Anlegen eines Worktrees aus der Repo-Liste vorgeschlagen. Leer = neben dem Repository.",
        (Language::En, "wt_root_placeholder") => "next to the repository",
        (Language::De, "wt_root_placeholder") => "neben dem Repository",
        (Language::En, "wt_root_reset") => "Reset",
        (Language::De, "wt_root_reset") => "Zurücksetzen",
        (Language::En, "wt_root_relative") => "Relative path – ignored, please enter an absolute path.",
        (Language::De, "wt_root_relative") => "Relativer Pfad – wird ignoriert, bitte einen absoluten Pfad angeben.",
        (Language::En, "search_label") => "Search:",
        (Language::De, "search_label") => "Suche:",
        (Language::En, "filter_hint") => "filter...",
        (Language::De, "filter_hint") => "filtern...",
        (Language::En, "branch_search") => "Search branches...",
        (Language::De, "branch_search") => "Branches suchen...",
        (Language::En, "branch_refresh_tooltip") => "Reload branches (git fetch --all)",
        (Language::De, "branch_refresh_tooltip") => "Branches neu laden (git fetch --all)",
        (Language::En, "current") => "Current:",
        (Language::De, "current") => "Aktuell:",
        (Language::En, "no_branches") => "No branches found – try fetch",
        (Language::De, "no_branches") => "Keine Branches gefunden – evtl. fetch nötig",
        (Language::En, "fetch_now") => "Fetch now",
        (Language::De, "fetch_now") => "Jetzt fetchen",
        (Language::En, "no_matches") => "No matches",
        (Language::De, "no_matches") => "Keine Treffer",
        (Language::En, "branch_switch_tooltip") => "Switch branch – type to filter, ↻ to fetch",
        (Language::De, "branch_switch_tooltip") => "Branch wechseln – tippe zum Filtern, ↻ für fetch",
        (Language::En, "detached_tooltip") => "Detached HEAD – commit checked out, not a branch",
        (Language::De, "detached_tooltip") => "Detached HEAD – Commit ausgecheckt, kein Branch",
        (Language::En, "profile_override_tooltip") => "Override language profile for this repo",
        (Language::De, "profile_override_tooltip") => "Sprach-Profil für dieses Repo überschreiben",
        (Language::En | Language::De, "global") => "— Global —",
        (Language::En, "open_in_explorer") => "Show in Explorer",
        (Language::De, "open_in_explorer") => "Im Explorer öffnen",
        (Language::En, "open_in_shell") => "Open shell",
        (Language::De, "open_in_shell") => "Shell öffnen",
        (Language::En, "open_in") => "Open in",
        (Language::De, "open_in") => "Mit",
        (Language::En, "open_in_terminal") => "Open in {} (Terminal)",
        (Language::De, "open_in_terminal") => "In {} öffnen (Terminal)",
        (Language::En, "no_solution") => "No {}", // formatted with extension
        (Language::De, "no_solution") => "Keine {}",
        (Language::En, "selected") => "Selected: {}",
        (Language::De, "selected") => "Ausgewählt: {}",

        // ── Settings General ───────────────────────────────────────────
        (Language::En, "settings_general_desc") => "Choose folders to scan for Git repositories. All direct subfolders up to the selected depth are scanned.",
        (Language::De, "settings_general_desc") => "Wähle die Ordner, die nach Git-Repositories durchsucht werden sollen. Es werden alle direkten Unterordner bis zur eingestellten Tiefe geprüft.",
        (Language::En, "search_paths_title") => "Search paths",
        (Language::De, "search_paths_title") => "Suchpfade",
        (Language::En, "add") => "Add",
        (Language::De, "add") => "Hinzufügen",
        (Language::En, "no_paths_configured") => "No paths configured. Add a folder.",
        (Language::De, "no_paths_configured") => "Keine Pfade konfiguriert. Füge einen Ordner hinzu.",
        (Language::En, "remove") => "Remove",
        (Language::De, "remove") => "Entfernen",
        (Language::En, "browse") => "Browse",
        (Language::De, "browse") => "Durchsuchen",
        (Language::En, "path_exists_error") => "Path does not exist:",
        (Language::De, "path_exists_error") => "Pfad existiert nicht:",
        (Language::En, "search_depth") => "Search depth",
        (Language::De, "search_depth") => "Suchtiefe",
        (Language::En, "search_depth_desc") => "How many levels deep to search (1 = direct children, 2 = children + grandchildren, ...)",
        (Language::De, "search_depth_desc") => "Wie viele Ebenen tief gesucht wird (1 = nur direkte Kinder, 2 = Kinder + Enkel, ...)",
        (Language::En, "depth") => "Depth:",
        (Language::De, "depth") => "Tiefe:",
        (Language::En, "levels") => "levels",
        (Language::De, "levels") => "Ebenen",
        (Language::En, "or_direct") => "Or direct:",
        (Language::De, "or_direct") => "Oder direkt:",
        (Language::En, "current_depth") => "(current: {})",
        (Language::De, "current_depth") => "(aktuell: {})",
        (Language::En, "active_profile_global") => "Active language profile (global)",
        (Language::De, "active_profile_global") => "Aktives Sprach-Profil (global)",
        (Language::En, "per_repo_override_hint") => "Per-repo override can be chosen in the overview.",
        (Language::De, "per_repo_override_hint") => "Pro Repo kann in der Übersicht ein Override gewählt werden.",

        // ── Settings Profiles ──────────────────────────────────────────
        (Language::En, "manage_profiles") => "Manage language profiles",
        (Language::De, "manage_profiles") => "Sprach-Profile verwalten",
        (Language::En, "profiles_desc") => "Each profile defines a file extension (e.g. .sln) and the IDEs that can open it.",
        (Language::De, "profiles_desc") => "Jedes Profil definiert eine Dateiendung (z.B. .sln) und die IDEs, mit denen diese geöffnet werden kann.",
        (Language::En, "profiles_count") => "Profiles ({}):",
        (Language::De, "profiles_count") => "Profile ({}):",
        (Language::En, "new_profile") => "New profile",
        (Language::De, "new_profile") => "Neues Profil",
        (Language::En, "duplicate") => "Duplicate",
        (Language::De, "duplicate") => "Duplizieren",
        (Language::En, "delete") => "Delete",
        (Language::De, "delete") => "Löschen",
        (Language::En, "set_active") => "Set active",
        (Language::De, "set_active") => "Aktiv setzen",
        (Language::En, "quick_create_profile") => "Quick create new profile",
        (Language::De, "quick_create_profile") => "Schnell-Anlage neues Profil",
        (Language::En | Language::De, "name") => "Name:",
        (Language::En, "extension") => "Extension:",
        (Language::De, "extension") => "Endung:",
        (Language::En, "create") => "Create",
        (Language::De, "create") => "Anlegen",
        (Language::En, "edit_profile") => "Edit profile:",
        (Language::De, "edit_profile") => "Profil bearbeiten:",
        (Language::En | Language::De, "id") => "ID:",
        (Language::En, "display_name") => "Display name:",
        (Language::De, "display_name") => "Anzeigename:",
        (Language::En, "file_extension") => "File extension:",
        (Language::De, "file_extension") => "Dateiendung:",
        (Language::En, "pattern_optional") => "Pattern (optional):",
        (Language::De, "pattern_optional") => "Muster (optional):",
        (Language::En, "scan_depth") => "Scan depth:",
        (Language::De, "scan_depth") => "Scan-Tiefe:",
        (Language::En, "ides_for_profile") => "IDEs for this profile:",
        (Language::De, "ides_for_profile") => "IDEs für dieses Profil:",
        (Language::En | Language::De, "default") => "Default",
        (Language::En, "set_as_default") => "Set as default",
        (Language::De, "set_as_default") => "Als Default",
        (Language::En, "program") => "Program:",
        (Language::De, "program") => "Programm:",
        (Language::En | Language::De, "args") => "Args:",
        (Language::En | Language::De, "shell") => "Shell:",
        (Language::En, "via_cmd") => "via cmd /C (unsafe)",
        (Language::De, "via_cmd") => "via cmd /C (unsicher)",
        (Language::En, "preview") => "Preview:",
        (Language::De, "preview") => "Vorschau:",
        (Language::En, "add_ide") => "Add IDE",
        (Language::De, "add_ide") => "IDE hinzufügen",
        (Language::En | Language::De, "default_ide") => "Default IDE:",
        (Language::En, "none") => "None",
        (Language::De, "none") => "Keine",
        (Language::En, "active") => "Active",
        (Language::De, "active") => "Aktiv",

        // ── Settings Agents ────────────────────────────────────────────
        (Language::En, "manage_agents") => "Manage AI agents",
        (Language::De, "manage_agents") => "AI Agents verwalten",
        (Language::En, "agents_desc") => "Agents open a terminal in the repo directory and run the configured command. Claude is preconfigured.",
        (Language::De, "agents_desc") => "Agenten starten ein Terminal im Repo-Verzeichnis und führen den konfigurierten Befehl aus. Claude ist vordefiniert.",
        (Language::En | Language::De, "agents_count") => "Agents ({}):",
        (Language::En, "new_agent") => "New agent",
        (Language::De, "new_agent") => "Neuer Agent",
        (Language::En, "quick_create_agent") => "Quick create new agent",
        (Language::De, "quick_create_agent") => "Schnell-Anlage neuer Agent",
        (Language::En, "program_label") => "Program:",
        (Language::De, "program_label") => "Programm:",
        (Language::En, "edit_agent") => "Edit agent:",
        (Language::De, "edit_agent") => "Agent bearbeiten:",
        (Language::En, "args_space_separated") => "Args (space-separated):",
        (Language::De, "args_space_separated") => "Args (Leerzeichen-getrennt):",
        (Language::En, "terminal_override") => "Terminal override:",
        (Language::De, "terminal_override") => "Terminal-Override:",
        (Language::En | Language::De, "auto_global") => "— Auto (global) —",
        (Language::En, "active_agents") => "Active agents (multiple possible — all icons appear side-by-side):",
        (Language::De, "active_agents") => "Aktive Agents (mehrere möglich — alle Icons erscheinen nebeneinander):",
        (Language::En, "enable_all") => "Enable all",
        (Language::De, "enable_all") => "Alle aktivieren",
        (Language::En, "disable_all") => "Disable all",
        (Language::De, "disable_all") => "Alle deaktivieren",

        // ── Settings Terminal ──────────────────────────────────────────
        (Language::En, "terminal_settings") => "Terminal settings",
        (Language::De, "terminal_settings") => "Terminal-Einstellungen",
        (Language::En, "terminal_desc") => "Choose the preferred terminal for launching AI agents. 'Auto' tries Windows Terminal → Powershell → cmd.",
        (Language::De, "terminal_desc") => "Wähle das bevorzugte Terminal zum Starten von AI-Agents. 'Auto' probiert Windows Terminal → Powershell → cmd.",
        (Language::En, "preferred_terminal") => "Preferred terminal:",
        (Language::De, "preferred_terminal") => "Bevorzugtes Terminal:",
        (Language::En, "custom_terminal_optional") => "Custom terminal (optional):",
        (Language::De, "custom_terminal_optional") => "Custom Terminal (optional):",
        (Language::En, "only_custom_relevant") => "Only relevant for 'Custom'",
        (Language::De, "only_custom_relevant") => "Nur bei 'Custom' relevant",
        (Language::En, "switch_to_custom") => "Switch to Custom",
        (Language::De, "switch_to_custom") => "Auf Custom wechseln",
        (Language::En, "fallback_terminal") => "Fallback terminal:",
        (Language::De, "fallback_terminal") => "Fallback Terminal:",

        // ── Settings Appearance ────────────────────────────────────────
        (Language::En, "appearance") => "Appearance",
        (Language::De, "appearance") => "Erscheinungsbild",
        (Language::En, "appearance_desc") => "Choose one of 5 themes — applies immediately after saving.",
        (Language::De, "appearance_desc") => "Wähle eines von 5 Themes — wirkt sofort nach Speichern.",
        (Language::En | Language::De, "theme") => "Theme:",
        (Language::En, "theme_hint") => "Theme is applied automatically on next launch and immediately after saving.",
        (Language::De, "theme_hint") => "Theme wird beim nächsten Start automatisch angewendet und nach Speichern sofort.",

        // ── Settings common ────────────────────────────────────────────
        (Language::En, "close") => "Close",
        (Language::De, "close") => "Schließen",
        (Language::En, "save") => "Save",
        (Language::De, "save") => "Speichern",
        (Language::En, "reset") => "Reset",
        (Language::De, "reset") => "Zurücksetzen",
        (Language::En | Language::De, "config_path") => "Config:",
        (Language::En, "tabs_general") => "General",
        (Language::De, "tabs_general") => "Allgemein",
        (Language::En, "tabs_profiles") => "Languages/Profiles",
        (Language::De, "tabs_profiles") => "Sprachen/Profile",
        (Language::En | Language::De, "tabs_agents") => "AI Agents",
        (Language::En | Language::De, "tabs_terminal") => "Terminal",
        (Language::En, "tabs_appearance") => "Appearance",
        (Language::De, "tabs_appearance") => "Erscheinungsbild",
        (Language::En, "tabs_language") => "Language",
        (Language::De, "tabs_language") => "Sprache",
        (Language::En | Language::De, "tabs_icons") => "Icons",
        (Language::En | Language::De, "tabs_tray_icons") => "Tray Icons",
        (Language::En | Language::De, "tray_icons_title") => "Tray Icons & Popup",
        (Language::En, "tray_behavior_title") => "System Tray Behavior",
        (Language::De, "tray_behavior_title") => "System Tray Verhalten",
        (Language::En, "tray_minimize_label") => "Minimize to tray on close (instead of quit)",
        (Language::De, "tray_minimize_label") => "Beim Schließen in Tray minimieren (statt beenden)",
        (Language::En, "tray_minimize_hint_on") => "✓ Window is hidden on close and keeps running in tray (left/right click: custom menu).",
        (Language::De, "tray_minimize_hint_on") => "✓ Das Fenster wird beim Schließen ausgeblendet und läuft im Tray weiter (Links-/Rechts-Klick: eigenes Menü).",
        (Language::En, "tray_minimize_hint_off") => "Window quits on close.",
        (Language::De, "tray_minimize_hint_off") => "Das Fenster wird beim Schließen beendet.",
        (Language::En, "tray_update_label") => "Check for updates on startup (GitHub Releases)",
        (Language::De, "tray_update_label") => "Beim Start auf Updates prüfen (GitHub Releases)",
        (Language::En, "tray_update_hint_on") => "✓ api.github.com is queried once on startup (5 s timeout). Disable for offline/corporate networks. Takes effect after restart.",
        (Language::De, "tray_update_hint_on") => "✓ Beim Start wird einmalig api.github.com abgefragt (5 s Timeout). Deaktivieren für Offline-/Firmennetze. Änderung wirkt ab Neustart.",
        (Language::En, "tray_update_hint_off") => "Update check disabled — no network request on startup. Takes effect after restart.",
        (Language::De, "tray_update_hint_off") => "Update-Check deaktiviert — keine Netzwerkabfrage beim Start. Änderung wirkt ab Neustart.",
        (Language::En, "autostart_label") => "Start automatically with Windows",
        (Language::De, "autostart_label") => "Automatisch mit Windows starten",
        (Language::En, "autostart_hint") => "Adds/removes GitManager in Windows Autostart (Registry Run key). Takes effect on next login.",
        (Language::De, "autostart_hint") => "Trägt GitManager in den Windows-Autostart ein (Registry Run-Key). Wirkt ab der nächsten Anmeldung.",
        (Language::En, "tray_update_error") => "Last update check failed: {}",
        (Language::De, "tray_update_error") => "Letzter Update-Check fehlgeschlagen: {}",
        (Language::En, "tray_branch_limit_title") => "Tray Branch Limit",
        (Language::De, "tray_branch_limit_title") => "Tray Branch-Limit",
        (Language::En, "tray_branch_limit_desc") => "Max branches shown in tray popup dropdown (5–50, default 20).",
        (Language::De, "tray_branch_limit_desc") => "Wie viele Branches maximal im Tray-Popup Dropdown angezeigt werden (5–50, Standard 20).",
        (Language::En, "tray_branch_limit_summary") => "Shows up to {} branches in the tray popup. More in the main window.",
        (Language::De, "tray_branch_limit_summary") => "Zeigt bis zu {} Branches im Tray-Popup. Weitere im Hauptfenster.",
        (Language::En, "tray_creation_error") => "Tray icon creation failed – tray popup disabled.",
        (Language::De, "tray_creation_error") => "Tray-Icon konnte nicht erstellt werden – Tray-Popup deaktiviert.",
        (Language::En, "tray_all_hidden_warn") => "Warning: all tray icons hidden – no icons will be shown in the tray row.",
        (Language::De, "tray_all_hidden_warn") => "Warnung: alle Tray-Icons ausgeblendet – es werden keine Icons in der Tray-Zeile angezeigt.",
        (Language::En | Language::De, "tray_limit_label") => "Limit:",
        (Language::En, "tray_direct_label") => "Or direct:",
        (Language::De, "tray_direct_label") => "Oder direkt:",
        (Language::En, "tray_branch_more_filtered") => "+ {} more (filter)",
        (Language::De, "tray_branch_more_filtered") => "+ {} weitere (filtere)",
        (Language::En, "tray_branch_more_main") => "+ {} more (in main window)",
        (Language::De, "tray_branch_more_main") => "+ {} weitere (in Hauptfenster)",
        (Language::En, "tray_notice_branch_dialog") => "Branch switch for '{}' waits for a decision – click to open",
        (Language::De, "tray_notice_branch_dialog") => "Branch-Wechsel für '{}' wartet auf Entscheidung – klicken zum Öffnen",
        (Language::En, "tray_notice_error") => "Note in main window – click to open",
        (Language::De, "tray_notice_error") => "Hinweis im Hauptfenster – klicken zum Öffnen",
        (Language::En, "update_available_title") => "Update available",
        (Language::De, "update_available_title") => "Update verfügbar",
        (Language::En, "update_available_msg") => "New version available",
        (Language::De, "update_available_msg") => "Neue Version verfügbar",
        (Language::En, "open_link") => "Open",
        (Language::De, "open_link") => "Öffnen",
        (Language::En, "tray_icons_desc") => "Reorder, hide and limit tray popup icons. Hidden icons disappear from tray rows. Max display limits visible repos.",
        (Language::De, "tray_icons_desc") => "Tray-Popup Icons neu anordnen, ausblenden und limitieren. Ausgeblendete Icons verschwinden aus den Tray-Zeilen. Max Anzeige limitiert sichtbare Repos.",
        (Language::En, "tray_max_display") => "Max repos in tray popup",
        (Language::De, "tray_max_display") => "Max Repos im Tray-Popup",
        (Language::En, "icons_title") => "Icon order & visibility (per profile)",
        (Language::De, "icons_title") => "Icon-Reihenfolge & Sichtbarkeit (pro Profil)",
        (Language::En, "icons_desc") => "Reorder and hide IDE / terminal / AI icons. Hidden icons disappear from repo list without gaps.",
        (Language::De, "icons_desc") => "Icons für IDE / Terminal / AI neu anordnen und ausblenden. Ausgeblendete Icons verschwinden ohne Lücke.",
        (Language::En, "icons_profile") => "Profile:",
        (Language::De, "icons_profile") => "Profil:",
        (Language::En | Language::De, "icons_ides") => "IDEs",
        (Language::En | Language::De, "icons_terminal") => "Terminal / Explorer",
        (Language::En | Language::De, "icons_agents") => "AI Agents",
        (Language::En, "icons_show_shell") => "Show shell icon",
        (Language::De, "icons_show_shell") => "Shell-Icon anzeigen",
        (Language::En, "icons_show_explorer") => "Show explorer icon",
        (Language::De, "icons_show_explorer") => "Explorer-Icon anzeigen",
        (Language::En, "icons_toggle_visibility") => "Toggle visibility",
        (Language::De, "icons_toggle_visibility") => "Sichtbarkeit umschalten",
        (Language::En, "icons_hidden_info") => "Hidden",
        (Language::De, "icons_hidden_info") => "Ausgeblendet",
        (Language::En, "icons_reset") => "Reset visibility & order",
        (Language::De, "icons_reset") => "Sichtbarkeit & Reihenfolge zurücksetzen",
        (Language::En, "icons_all_hidden_warn") => "Warning: all IDE icons hidden – no IDE button will be shown.",
        (Language::De, "icons_all_hidden_warn") => "Warnung: alle IDE-Icons ausgeblendet – es wird kein IDE-Button angezeigt.",
        (Language::En, "icons_terminal_hint") => "Explorer and shell buttons appear next to IDE icons in the repo list.",
        (Language::De, "icons_terminal_hint") => "Explorer- und Shell-Buttons erscheinen neben den IDE-Icons in der Repo-Liste.",

        // ── Errors / validation ────────────────────────────────────────
        (Language::En, "error_need_path") => "Please specify at least one path.",
        (Language::De, "error_need_path") => "Bitte mindestens einen Pfad angeben.",
        (Language::En, "error_need_profile") => "At least one language profile is required.",
        (Language::De, "error_need_profile") => "Mindestens ein Sprach-Profil erforderlich.",
        (Language::En, "error_profile_empty") => "Profile '{}' has empty ID/name",
        (Language::De, "error_profile_empty") => "Profil '{}' hat leere ID/Name",
        (Language::En, "error_profile_ext") => "Profile '{}' needs a file extension",
        (Language::De, "error_profile_ext") => "Profil '{}' braucht Dateiendung",
        (Language::En, "saved_scan_restart") => "Saved. Restarting scan...",
        (Language::De, "saved_scan_restart") => "Gespeichert. Scan wird neu gestartet.",
        (Language::En, "profile_switched") => "Profile switched to '{}' – rescanning solutions...",
        (Language::De, "profile_switched") => "Profil gewechselt zu '{}' – scanne Solutions neu...",
        (Language::En, "opening_with") => "Opening {} with {}...",
        (Language::De, "opening_with") => "Öffne {} mit {}...",
        (Language::En, "starting_agent_in") => "Starting {} in {}...",
        (Language::De, "starting_agent_in") => "Starte {} in {}...",
        (Language::En, "profile_changed_for") => "Profile for {} changed – rescanning...",
        (Language::De, "profile_changed_for") => "Profil für {} geändert – scanne neu...",
        (Language::En, "save_failed") => "Save failed: {}",
        (Language::De, "save_failed") => "Speichern fehlgeschlagen: {}",
        (Language::En, "solution_selected") => "Solution selected: {}",
        (Language::De, "solution_selected") => "Solution ausgewählt: {}",
        (Language::En, "explorer_opened") => "Explorer opened: {}",
        (Language::De, "explorer_opened") => "Explorer geöffnet: {}",
        (Language::En, "shell_opened") => "Shell opened: {}",
        (Language::De, "shell_opened") => "Shell geöffnet: {}",
        (Language::En, "fetching_branches") => "Fetching branches for {}...",
        (Language::De, "fetching_branches") => "Fetche Branches für {}...",
        (Language::En, "fetch_failed") => "Fetch failed: {}",
        (Language::De, "fetch_failed") => "Fetch fehlgeschlagen: {}",
        (Language::En, "fetch_partial") => "Fetch partially failed: {}",
        (Language::De, "fetch_partial") => "Fetch teilweise fehlgeschlagen: {}",
        (Language::En, "auth_hint") => "Note: check Credentials/SSH (auth may be required)",
        (Language::De, "auth_hint") => "Hinweis: Credentials/SSH prüfen (ggf. Auth erforderlich)",
        (Language::En, "fetch_aborted") => "Background fetch aborted (internal error)",
        (Language::De, "fetch_aborted") => "Hintergrund-Fetch abgebrochen (interner Fehler)",
        (Language::En, "branch_switched_to") => "Branch switched to '{}': {}",
        (Language::De, "branch_switched_to") => "Branch zu '{}' gewechselt: {}",
        (Language::En, "ide_not_found") => "IDE '{}' not found",
        (Language::De, "ide_not_found") => "IDE '{}' nicht in Profil '{}' gefunden",
        (Language::En, "agent_not_found") => "AI agent '{}' not found",
        (Language::De, "agent_not_found") => "AI-Agent '{}' nicht gefunden",

        // ── Config Selectors ─────────────────────────────────────────────
        (Language::En, "config_selector_title") => "Config dropdowns for this profile",
        (Language::De, "config_selector_title") => "Config-Dropdowns für dieses Profil",
        (Language::En, "config_selector_desc") => "XML values from .config files as dropdowns in the repo row. Each selector reads/writes a key in file_path (e.g. App.config <add key=\"Database\" value=\"...\"/>).",
        (Language::De, "config_selector_desc") => "XML-Werte aus .config Dateien als Dropdowns in der Repo-Zeile. Jeder Selector liest/schreibt einen Key in file_path (z.B. App.config <add key=\"Database\" value=\"...\"/>).",
        (Language::En, "add_config_selector") => "Add Config Selector",
        (Language::De, "add_config_selector") => "Selector hinzufügen",
        (Language::En, "config_file") => "Config file:",
        (Language::De, "config_file") => "Config-Datei:",
        (Language::En | Language::De, "config_key") => "Key:",
        (Language::En, "config_options") => "Options:",
        (Language::De, "config_options") => "Optionen:",
        (Language::En, "allow_custom") => "Allow custom values",
        (Language::De, "allow_custom") => "Eigene Werte erlauben",
        (Language::En, "config_saved") => "{} set to '{}'",
        (Language::De, "config_saved") => "{} auf '{}' gesetzt",
        (Language::En, "config_save_failed") => "Failed to save config '{}': {}",
        (Language::De, "config_save_failed") => "Config '{}' speichern fehlgeschlagen: {}",
        (Language::En, "config_missing") => "Key '{}' not found in {}",
        (Language::De, "config_missing") => "Key '{}' nicht gefunden in {}",
        (Language::En, "key_attr") => "Key attribute:",
        (Language::De, "key_attr") => "Key-Attribut:",
        (Language::En, "value_attr") => "Value attribute:",
        (Language::De, "value_attr") => "Value-Attribut:",

        // ── Main list sort (Task 4) ──────────────────────────────────
        (Language::En, "sort_by_name") => "Sort by name",
        (Language::De, "sort_by_name") => "Nach Name sortieren",
        (Language::En, "sort_by_last_used") => "Sort by last used",
        (Language::De, "sort_by_last_used") => "Nach letzter Nutzung sortieren",
        (Language::En, "sort_tooltip") => "Sort repositories",
        (Language::De, "sort_tooltip") => "Repositories sortieren",
        (Language::En, "sort_title") => "Sorting",
        (Language::De, "sort_title") => "Sortierung",

        // ── Behind indicator + auto-fetch (Task 5) ───────────────────
        (Language::En, "branch_behind") => "Behind by {} commit(s)",
        (Language::De, "branch_behind") => "{} Commit(s) hinterher",
        (Language::En, "auto_fetch_label") => "Auto-fetch interval (minutes, 0 = off)",
        (Language::De, "auto_fetch_label") => "Auto-Fetch Intervall (Minuten, 0 = aus)",
        (Language::En, "auto_fetch_hint") => "Periodically runs git fetch --all for all scanned repos in background. 0 disables.",
        (Language::De, "auto_fetch_hint") => "Führt periodisch git fetch --all für alle gescannten Repos im Hintergrund aus. 0 deaktiviert.",
        (Language::En, "auto_fetch_off") => "Disabled",
        (Language::De, "auto_fetch_off") => "Deaktiviert",
        (Language::En, "auto_fetch_running") => "Auto-fetch running, try again shortly",
        (Language::De, "auto_fetch_running") => "Auto-Fetch läuft, bitte kurz warten",
        (Language::En, "fetch_busy") => "A scan or fetch is already running, try again shortly",
        (Language::De, "fetch_busy") => "Scan oder Fetch läuft bereits, bitte kurz warten",

        // ── Tray icon theme (Dark/Light) ─────────────────────────────
        (Language::En, "tray_icon_theme_title") => "Tray icon style",
        (Language::De, "tray_icon_theme_title") => "Tray-Icon Stil",
        (Language::En, "tray_icon_theme_desc") => "White icon for dark taskbar (Dark) or dark icon for light taskbar (Light).",
        (Language::De, "tray_icon_theme_desc") => "Weißes Icon für dunkle Taskleiste (Dark) oder dunkles Icon für helle Taskleiste (Light).",
        (Language::En, "tray_icon_theme_dark") => "Dark (white icon)",
        (Language::De, "tray_icon_theme_dark") => "Dark (weißes Icon)",
        (Language::En, "tray_icon_theme_light") => "Light (dark icon)",
        (Language::De, "tray_icon_theme_light") => "Light (dunkles Icon)",

        // ── Merge Requests: Approvals, Pipeline, Threads, Review ─────
        (Language::En, "mr_appr_of") => "{} of {} approvals",
        (Language::De, "mr_appr_of") => "{} von {} Approvals",
        (Language::En, "mr_appr_one") => "1 approval",
        (Language::De, "mr_appr_one") => "1 Approval",
        (Language::En, "mr_appr_n") => "{} approvals",
        (Language::De, "mr_appr_n") => "{} Approvals",
        (Language::En | Language::De, "mr_appr_short") => "{}/{}",
        (Language::En, "mr_appr_no_rule") => "No approval rule – no approvals required",
        (Language::De, "mr_appr_no_rule") => "Keine Approval-Regel – keine Approvals erforderlich",
        (Language::En, "mr_appr_goal_tt") => "No approval rule in GitLab – goal from the settings",
        (Language::De, "mr_appr_goal_tt") => "Keine Approval-Regel in GitLab – Ziel aus den Einstellungen",
        (Language::En, "mr_appr_you") => "{} (you)",
        (Language::De, "mr_appr_you") => "{} (du)",
        (Language::En, "mr_rev_started") => "{} started reviewing",
        (Language::De, "mr_rev_started") => "{} hat das Review begonnen",
        (Language::En, "mr_rev_waiting") => "{} pending",
        (Language::De, "mr_rev_waiting") => "{} ausstehend",
        (Language::En, "mr_pipe_success") => "Pipeline passed",
        (Language::De, "mr_pipe_success") => "Pipeline grün",
        (Language::En, "mr_pipe_failed") => "Pipeline failed",
        (Language::De, "mr_pipe_failed") => "Pipeline fehlgeschlagen",
        (Language::En, "mr_pipe_running") => "Pipeline running",
        (Language::De, "mr_pipe_running") => "Pipeline läuft",
        (Language::En, "mr_pipe_pending") => "Pipeline pending",
        (Language::De, "mr_pipe_pending") => "Pipeline wartet",
        (Language::En, "mr_pipe_canceled") => "Pipeline canceled",
        (Language::De, "mr_pipe_canceled") => "Pipeline abgebrochen",
        (Language::En, "mr_pipe_skipped") => "Pipeline skipped",
        (Language::De, "mr_pipe_skipped") => "Pipeline übersprungen",
        (Language::En, "mr_pipe_manual") => "Pipeline manual",
        (Language::De, "mr_pipe_manual") => "Pipeline manuell",
        (Language::En, "mr_pipe_none") => "No pipeline",
        (Language::De, "mr_pipe_none") => "Keine Pipeline",
        (Language::En, "mr_threads_one") => "1 open thread",
        (Language::De, "mr_threads_one") => "1 offener Thread",
        (Language::En, "mr_threads_many") => "{} open threads",
        (Language::De, "mr_threads_many") => "{} offene Threads",
        (Language::En, "mr_threads_resolved") => "all threads resolved",
        (Language::De, "mr_threads_resolved") => "alle Threads gelöst",
        (Language::En, "mr_threads_none") => "no threads",
        (Language::De, "mr_threads_none") => "keine Threads",
        (Language::En, "mr_threads_tt") => "{} open · {} resolved",
        (Language::De, "mr_threads_tt") => "{} offen · {} gelöst",
        (Language::En, "mr_threads_none_tt") => "No discussion threads",
        (Language::De, "mr_threads_none_tt") => "Keine Diskussions-Threads",
        (Language::En, "mr_rev_pending") => "Review pending",
        (Language::De, "mr_rev_pending") => "Review ausstehend",
        (Language::En, "mr_rev_approved") => "Approved by you",
        (Language::De, "mr_rev_approved") => "Von dir approved",
        (Language::En, "mr_rev_changes") => "Changes requested",
        (Language::De, "mr_rev_changes") => "Änderungen angefragt",
        (Language::En, "mr_rev_commented") => "Commented",
        (Language::De, "mr_rev_commented") => "Kommentiert",

        // ── Merge Requests: Abruf- und Prüffehler, Abfrageteile ─────────
        (Language::En, "mr_err_scope") => "The token needs read_api.",
        (Language::De, "mr_err_scope") => "Das Token braucht read_api.",
        (Language::En, "mr_err_network") => "{} is not reachable.",
        (Language::De, "mr_err_network") => "{} nicht erreichbar.",
        (Language::En, "mr_err_tls") => "The certificate of {} is not trusted.",
        (Language::De, "mr_err_tls") => "Zertifikat von {} wird nicht vertraut.",
        (Language::En, "mr_err_timeout") => "{} timed out after {} s.",
        (Language::De, "mr_err_timeout") => "Zeitüberschreitung bei {} nach {} s.",
        (Language::En, "mr_err_content_type") => "glab sent the request without a JSON content type.",
        (Language::De, "mr_err_content_type") => "glab hat die Anfrage ohne JSON-Content-Type gesendet.",
        (Language::En, "mr_part_mentions") => "Mentions",
        (Language::De, "mr_part_mentions") => "Erwähnungen",
        (Language::En, "mr_part_reviews") => "Review requests",
        (Language::De, "mr_part_reviews") => "Review-Anfragen",
        (Language::En, "mr_part_uninvolved") => "Other open MRs",
        (Language::De, "mr_part_uninvolved") => "Weitere offene MRs",
        (Language::En, "mr_part_mine") => "Own MRs",
        (Language::De, "mr_part_mine") => "Eigene MRs",

        // ── Merge Requests: Ansicht (Design B) ───────────────────────
        (Language::En, "mr_open_settings") => "Open settings",
        (Language::De, "mr_open_settings") => "Einstellungen öffnen",
        (Language::En, "mr_loading") => "Loading merge requests …",
        (Language::De, "mr_loading") => "Lade Merge Requests …",
        (Language::En, "mr_mine_title") => "My merge requests",
        (Language::De, "mr_mine_title") => "Meine Merge Requests",
        (Language::En, "mr_others_title") => "From others",
        (Language::De, "mr_others_title") => "Von anderen",
        (Language::En, "mr_count_of") => "{} of {}",
        (Language::De, "mr_count_of") => "{} von {}",
        (Language::En, "mr_sorted") => "Sorted by approvals",
        (Language::De, "mr_sorted") => "Sortiert nach Approvals",
        (Language::En, "mr_sorted_tt") => "Most approvals first; ties: MRs with an approval rule and fewer missing approvals first, then most recently updated",
        (Language::De, "mr_sorted_tt") => "Meiste Approvals zuerst; bei Gleichstand zuerst MRs mit Approval-Regel und weniger fehlenden Approvals, dann zuletzt aktualisiert",
        (Language::En, "mr_search_mine_hint") => "Search my MRs …",
        (Language::De, "mr_search_mine_hint") => "In meinen MRs suchen …",
        (Language::En, "mr_search_others_hint") => "Search MRs from others …",
        (Language::De, "mr_search_others_hint") => "In MRs von anderen suchen …",
        (Language::En, "mr_search_clear") => "Clear search (Esc)",
        (Language::De, "mr_search_clear") => "Suche leeren (Esc)",
        (Language::En, "mr_search_clear_link") => "Clear search",
        (Language::De, "mr_search_clear_link") => "Suche leeren",
        (Language::En, "mr_no_hits") => "No matches for “{}”",
        (Language::De, "mr_no_hits") => "Keine Treffer für „{}“",
        (Language::En, "mr_seg_open") => "Open",
        (Language::De, "mr_seg_open") => "Offen",
        (Language::En, "mr_seg_open_tt") => "Show open merge requests",
        (Language::De, "mr_seg_open_tt") => "Offene Merge Requests anzeigen",
        (Language::En, "mr_seg_merged") => "Merged",
        (Language::De, "mr_seg_merged") => "Gemergt",
        (Language::En, "mr_seg_merged_tt") => "Show merge requests merged in the last {} days",
        (Language::De, "mr_seg_merged_tt") => "Gemergte Merge Requests der letzten {} Tage anzeigen",
        (Language::En, "mr_sorted_merged") => "Sorted by merge time",
        (Language::De, "mr_sorted_merged") => "Sortiert nach Merge-Zeitpunkt",
        (Language::En, "mr_sorted_merged_tt") => "Most recently merged first",
        (Language::De, "mr_sorted_merged_tt") => "Zuletzt gemergte zuerst",
        (Language::En, "mr_merged_caption") => "{} merged in the last {} days",
        (Language::De, "mr_merged_caption") => "{} gemergt in den letzten {} Tagen",
        (Language::En, "mr_period") => "Last {} days",
        (Language::De, "mr_period") => "Letzte {} Tage",
        (Language::En, "mr_period_tt") => "Period of the merged merge requests",
        (Language::De, "mr_period_tt") => "Zeitraum der gemergten Merge Requests",
        (Language::En, "mr_merged_by") => "merged {} by {}",
        (Language::De, "mr_merged_by") => "gemergt {} von {}",
        (Language::En, "mr_merged_by_me") => "merged {} by you",
        (Language::De, "mr_merged_by_me") => "gemergt {} von dir",
        (Language::En, "mr_merged_at") => "merged {}",
        (Language::De, "mr_merged_at") => "gemergt {}",
        (Language::En, "mr_loading_merged") => "Loading merged merge requests …",
        (Language::De, "mr_loading_merged") => "Gemergte Merge Requests werden geladen …",
        (Language::En, "mr_empty_merged") => "No merged merge requests in the last {} days.",
        (Language::De, "mr_empty_merged") => "Keine gemergten Merge Requests in den letzten {} Tagen.",
        (Language::En, "mr_part_merged") => "Merged MRs",
        (Language::De, "mr_part_merged") => "Gemergte MRs",
        (Language::En | Language::De, "mr_chip_reviewer") => "Reviewer",
        (Language::En, "mr_chip_reviewer_tt") => "MRs where you are a reviewer",
        (Language::De, "mr_chip_reviewer_tt") => "MRs, bei denen du als Reviewer eingetragen bist",
        (Language::En, "mr_chip_mention") => "Mentioned",
        (Language::De, "mr_chip_mention") => "Erwähnt",
        (Language::En, "mr_chip_mention_tt") => "MRs mentioning you in the description or a comment",
        (Language::De, "mr_chip_mention_tt") => "MRs, in deren Beschreibung oder Kommentaren du erwähnt wirst",
        (Language::En, "mr_chip_all") => "All open",
        (Language::De, "mr_chip_all") => "Alle offenen",
        (Language::En, "mr_chip_all_tt") => "Also show MRs I'm not involved in",
        (Language::De, "mr_chip_all_tt") => "Auch MRs ohne meine Beteiligung anzeigen",
        (Language::En, "mr_chip_all_tt_n") => "Also show MRs I'm not involved in ({} open in {} local projects)",
        (Language::De, "mr_chip_all_tt_n") => "Auch MRs ohne meine Beteiligung anzeigen ({} offene in {} lokalen Projekten)",
        (Language::En, "mr_chip_all_tt_n_one") => "Also show MRs I'm not involved in ({} open in 1 local project)",
        (Language::De, "mr_chip_all_tt_n_one") => "Auch MRs ohne meine Beteiligung anzeigen ({} offene in 1 lokalen Projekt)",
        (Language::En | Language::De, "mr_reason_reviewer") => "Reviewer",
        (Language::En, "mr_reason_desc") => "@ description",
        (Language::De, "mr_reason_desc") => "@ Beschreibung",
        (Language::En, "mr_reason_comment") => "@ comment",
        (Language::De, "mr_reason_comment") => "@ Kommentar",
        (Language::En, "mr_reasons_prefix") => "Shown because:",
        (Language::De, "mr_reasons_prefix") => "Angezeigt wegen:",
        (Language::En, "mr_no_filter") => "No filter active",
        (Language::De, "mr_no_filter") => "Kein Filter aktiv",
        (Language::En, "mr_no_filter_hint") => "Turn on “Reviewer” or “Mentioned” to see merge requests you're involved in.",
        (Language::De, "mr_no_filter_hint") => "Schalte „Reviewer“ oder „Erwähnt“ ein, um Merge Requests zu sehen, an denen du beteiligt bist.",
        (Language::En, "mr_filter_no_match") => "No merge requests match this filter",
        (Language::De, "mr_filter_no_match") => "Keine Merge Requests für diesen Filter",
        (Language::En, "mr_empty_mine") => "You have no open merge requests.",
        (Language::De, "mr_empty_mine") => "Du hast keine offenen Merge Requests.",
        (Language::En, "mr_empty_others") => "No review is waiting for you.",
        (Language::De, "mr_empty_others") => "Gerade wartet kein Review auf dich.",
        (Language::En, "mr_hidden_by_selection_one") => "1 MR from deselected repositories hidden",
        (Language::De, "mr_hidden_by_selection_one") => "1 MR aus abgewählten Repositories ausgeblendet",
        (Language::En, "mr_hidden_by_selection_many") => "{} MRs from deselected repositories hidden",
        (Language::De, "mr_hidden_by_selection_many") => "{} MRs aus abgewählten Repositories ausgeblendet",
        (Language::En, "mr_todos_hidden") => "Mentions are missing: GitLab hides your to-dos from this token",
        (Language::De, "mr_todos_hidden") => "Erwähnungen fehlen: GitLab verbirgt die To-dos vor diesem Token",
        (Language::En, "mr_todos_hidden_fix") => "Fine-grained tokens need the permission “Todo: Read” – add it to the token in GitLab or use a token with read_api.",
        (Language::De, "mr_todos_hidden_fix") => "Feingranulare Tokens brauchen die Berechtigung „Todo: Read“ – ergänze sie am Token in GitLab oder nutze ein Token mit read_api.",
        (Language::En, "mrs_state_limited") => "Limited",
        (Language::De, "mrs_state_limited") => "Eingeschränkt",
        (Language::En, "mrs_todos_hidden") => "Mentions are missing: the token can't read to-dos",
        (Language::De, "mrs_todos_hidden") => "Erwähnungen fehlen: das Token darf keine To-dos lesen",
        (Language::En, "mr_uninvolved_h") => "Not involved · {}",
        (Language::De, "mr_uninvolved_h") => "Nicht beteiligt · {}",
        (Language::En, "mr_uninvolved_sub") => "open MRs in the {} local projects without your involvement",
        (Language::De, "mr_uninvolved_sub") => "offene MRs der {} lokalen Projekte ohne deine Beteiligung",
        (Language::En, "mr_uninvolved_sub_one") => "open MRs in the local project without your involvement",
        (Language::De, "mr_uninvolved_sub_one") => "offene MRs des lokalen Projekts ohne deine Beteiligung",
        (Language::En, "mr_shown_of") => "{} of {} shown",
        (Language::De, "mr_shown_of") => "{} von {} angezeigt",
        (Language::En, "mr_load_more") => "Load {} more",
        (Language::De, "mr_load_more") => "{} weitere laden",
        (Language::En, "mr_loading_uninvolved") => "Loading more open merge requests …",
        (Language::De, "mr_loading_uninvolved") => "Lade weitere offene Merge Requests …",
        (Language::En, "mr_uninvolved_not_loaded") => "Not loaded yet.",
        (Language::De, "mr_uninvolved_not_loaded") => "Noch nicht geladen.",
        (Language::En, "mr_load_now") => "Load now",
        (Language::De, "mr_load_now") => "Jetzt laden",
        (Language::En, "mr_new_comment_one") => "1 new comment",
        (Language::De, "mr_new_comment_one") => "1 neuer Kommentar",
        (Language::En, "mr_new_comments") => "{} new comments",
        (Language::De, "mr_new_comments") => "{} neue Kommentare",
        (Language::En, "mr_new_commit_one") => "1 new commit",
        (Language::De, "mr_new_commit_one") => "1 neuer Commit",
        (Language::En, "mr_new_commits") => "{} new commits",
        (Language::De, "mr_new_commits") => "{} neue Commits",
        (Language::En, "mr_new_push") => "new push",
        (Language::De, "mr_new_push") => "neu gepusht",
        (Language::En, "mr_new_tt") => "New since you last opened this MR (browser, Claude, IDE or checkout)",
        (Language::De, "mr_new_tt") => "Neu seit du den MR zuletzt geöffnet hast (Browser, Claude, IDE oder Auschecken)",
        (Language::En, "mr_notify_label") => "Notify about new review requests, mentions and comments",
        (Language::De, "mr_notify_label") => "Benachrichtigen bei neuen Review-Anfragen, Erwähnungen und Kommentaren",
        (Language::En, "mr_notify_review") => "Review requested",
        (Language::De, "mr_notify_review") => "Review angefragt",
        (Language::En, "mr_notify_review_line") => "{} added you as reviewer",
        (Language::De, "mr_notify_review_line") => "{} hat dich als Reviewer eingetragen",
        (Language::En, "mr_notify_mention_comment") => "Mentioned in a comment",
        (Language::De, "mr_notify_mention_comment") => "In Kommentar erwähnt",
        (Language::En, "mr_notify_mention_desc") => "Mentioned in the description",
        (Language::De, "mr_notify_mention_desc") => "In Beschreibung erwähnt",
        (Language::En, "mr_notify_mention_line") => "Merge request by {}",
        (Language::De, "mr_notify_mention_line") => "Merge Request von {}",
        (Language::En, "mr_notify_comments") => "New comments",
        (Language::De, "mr_notify_comments") => "Neue Kommentare",
        (Language::En, "mr_notify_comments_line") => "{} new comments",
        (Language::De, "mr_notify_comments_line") => "{} neue Kommentare",
        (Language::En, "mr_notify_comments_more") => "{} and {} more: {}",
        (Language::De, "mr_notify_comments_more") => "{} und {} weitere: {}",
        (Language::En, "mr_notify_more") => "{} more merge request updates",
        (Language::De, "mr_notify_more") => "{} weitere Neuigkeiten in Merge Requests",
        (Language::En, "mr_notify_btn_open") => "Open in browser",
        (Language::De, "mr_notify_btn_open") => "Im Browser öffnen",
        (Language::En, "mr_notify_btn_show") => "Show in GitManager",
        (Language::De, "mr_notify_btn_show") => "In GitManager anzeigen",
        (Language::En, "mr_draft") => "Draft",
        (Language::De, "mr_draft") => "Entwurf",
        (Language::En, "mr_ready") => "ready to merge",
        (Language::De, "mr_ready") => "mergebereit",
        (Language::En, "mr_ready_tt") => "GitLab: can be merged",
        (Language::De, "mr_ready_tt") => "GitLab: kann gemergt werden",
        (Language::En, "mr_pipe_open_tt") => "{} – open pipeline in browser",
        (Language::De, "mr_pipe_open_tt") => "{} – Pipeline im Browser öffnen",

        // ── Merge Requests: Aktionen und Checkout-Menü (K1) ──────────
        (Language::En, "mr_open_browser") => "Open in browser",
        (Language::De, "mr_open_browser") => "Im Browser öffnen",
        (Language::En, "mr_open_title_tt") => "Open merge request in browser",
        (Language::De, "mr_open_title_tt") => "Merge Request im Browser öffnen",
        (Language::En, "mr_checkout_menu_tt") => "Check out – choose target",
        (Language::De, "mr_checkout_menu_tt") => "Auschecken – Ziel wählen",
        (Language::En, "mr_menu_header_clones") => "{} · {} local clones",
        (Language::De, "mr_menu_header_clones") => "{} · {} lokale Klone",
        (Language::En, "mr_menu_header_clone_one") => "{} · 1 local clone",
        (Language::De, "mr_menu_header_clone_one") => "{} · 1 lokaler Klon",
        (Language::En, "mr_menu_new_folder") => "In a new folder",
        (Language::De, "mr_menu_new_folder") => "In neuem Ordner",
        (Language::En, "mr_menu_worktree_review") => "New worktree in the review folder (recommended)",
        (Language::De, "mr_menu_worktree_review") => "Neuer Worktree im Review-Ordner (empfohlen)",
        (Language::En, "mr_menu_worktree") => "New worktree (recommended)",
        (Language::De, "mr_menu_worktree") => "Neuer Worktree (empfohlen)",
        (Language::En, "mr_menu_worktree_sub") => "{} · shares git data with {}",
        (Language::De, "mr_menu_worktree_sub") => "{} · teilt Git-Daten mit {}",
        (Language::En, "mr_menu_other_folder") => "Choose another folder …",
        (Language::De, "mr_menu_other_folder") => "Anderen Ordner wählen …",
        (Language::En, "mr_menu_clone") => "New clone …",
        (Language::De, "mr_menu_clone") => "Neuer Klon …",
        (Language::En, "mr_menu_clone_sub") => "fully independent, downloads again",
        (Language::De, "mr_menu_clone_sub") => "vollständig unabhängig, lädt neu herunter",
        (Language::En, "mr_menu_switch") => "Switch an existing clone",
        (Language::De, "mr_menu_switch") => "In bestehendem Klon wechseln",
        (Language::En, "mr_state_free") => "free",
        (Language::De, "mr_state_free") => "frei",
        (Language::En, "mr_state_in_use") => "in use for !{}",
        (Language::De, "mr_state_in_use") => "in Arbeit für !{}",
        (Language::En, "mr_state_in_use_branch") => "in use",
        (Language::De, "mr_state_in_use_branch") => "in Arbeit",
        (Language::En, "mr_state_dirty_one") => "1 change",
        (Language::De, "mr_state_dirty_one") => "1 Änderung",
        (Language::En, "mr_state_dirty_many") => "{} changes",
        (Language::De, "mr_state_dirty_many") => "{} Änderungen",
        (Language::En, "mr_state_dirty_hint") => "asks to stash/discard",
        (Language::De, "mr_state_dirty_hint") => "fragt nach Stash/Verwerfen",
        (Language::En, "mr_last_used") => "last used {}",
        (Language::De, "mr_last_used") => "zuletzt genutzt {}",
        (Language::En, "mr_menu_after") => "Then open",
        (Language::De, "mr_menu_after") => "Danach öffnen",
        (Language::En | Language::De, "mr_after_shell") => "Shell",
        (Language::En | Language::De, "mr_after_explorer") => "Explorer",
        (Language::En, "mr_after_nothing") => "Nothing",
        (Language::De, "mr_after_nothing") => "Nichts",
        (Language::En, "mr_after_remembered") => "remembers the last choice",
        (Language::De, "mr_after_remembered") => "merkt sich die letzte Wahl",
        (Language::En, "mr_already_header") => "Already checked out in {}",
        (Language::De, "mr_already_header") => "Bereits ausgecheckt in {}",
        (Language::En, "mr_open_show_in_list") => "Show in repository list",
        (Language::De, "mr_open_show_in_list") => "In Repo-Liste zeigen",
        (Language::En, "mr_open_agent") => "Start {}",
        (Language::De, "mr_open_agent") => "{} starten",
        (Language::En, "mr_open_shell") => "Open shell",
        (Language::De, "mr_open_shell") => "Shell öffnen",
        (Language::En, "mr_open_explorer") => "Open in Explorer",
        (Language::De, "mr_open_explorer") => "Im Explorer öffnen",
        (Language::En, "mr_open_ide") => "Open in {}",
        (Language::De, "mr_open_ide") => "In {} öffnen",
        (Language::En, "mr_checked_out_tt") => "Branch is already checked out in {}",
        (Language::De, "mr_checked_out_tt") => "Branch ist in {} bereits ausgecheckt",
        (Language::En, "mr_not_local") => "not local",
        (Language::De, "mr_not_local") => "nicht lokal",
        (Language::En, "mr_not_local_tt") => "Project isn't cloned locally – checkout not possible",
        (Language::De, "mr_not_local_tt") => "Projekt ist nicht lokal geklont – Auschecken nicht möglich",
        (Language::En, "mr_fork_tt") => "Fork MR – checkout will come in a later version",
        (Language::De, "mr_fork_tt") => "Fork-MR – Auschecken folgt in einer späteren Version",
        (Language::En, "mr_branch_deleted_tt") => "Source branch no longer exists",
        (Language::De, "mr_branch_deleted_tt") => "Quell-Branch existiert nicht mehr",

        // ── Merge Requests: Auschecken in der App (K1) ───────────────
        (Language::En, "mr_checkout_busy") => "Fetching {} for !{} …",
        (Language::De, "mr_checkout_busy") => "Hole {} für !{} …",
        (Language::En, "mr_cloning") => "Cloning {} to {} …",
        (Language::De, "mr_cloning") => "Klone {} nach {} …",
        (Language::En, "mr_checkout_blocked") => "Checkout not possible while a scan or fetch is running",
        (Language::De, "mr_checkout_blocked") => "Auschecken nicht möglich, solange ein Scan oder Fetch läuft",
        (Language::En, "mr_switch_waiting") => "A branch switch is still waiting for your decision – finish that dialog first",
        (Language::De, "mr_switch_waiting") => "Ein Branch-Wechsel wartet noch auf deine Entscheidung – erst den Dialog abschließen",
        (Language::En, "mr_checkout_gone") => "!{} is no longer in the current list – checkout skipped",
        (Language::De, "mr_checkout_gone") => "!{} ist nicht mehr im aktuellen Stand – Auschecken übersprungen",
        (Language::En, "mr_checkout_running") => "Checking out !{} …",
        (Language::De, "mr_checkout_running") => "Auschecken für !{} läuft …",
        (Language::En, "mr_worktree_created") => "Worktree {} created for !{}",
        (Language::De, "mr_worktree_created") => "Worktree {} für !{} angelegt",
        (Language::En, "mr_clone_done") => "!{} cloned to {}",
        (Language::De, "mr_clone_done") => "!{} geklont nach {}",
        (Language::En, "mr_checkout_done") => "!{} checked out: {} in {}",
        (Language::De, "mr_checkout_done") => "!{} ausgecheckt: {} in {}",
        (Language::En, "mr_checkout_done_ff") => "!{} checked out: {} in {} (+{} commits)",
        (Language::De, "mr_checkout_done_ff") => "!{} ausgecheckt: {} in {} (+{} Commits)",
        (Language::En, "mr_checkout_done_ff_one") => "!{} checked out: {} in {} (+1 commit)",
        (Language::De, "mr_checkout_done_ff_one") => "!{} ausgecheckt: {} in {} (+1 Commit)",
        (Language::En, "mr_already_head") => "Already checked out in {}",
        (Language::De, "mr_already_head") => "Bereits ausgecheckt in {}",
        (Language::En, "mr_already_head_behind") => "Already checked out in {} ({} behind)",
        (Language::De, "mr_already_head_behind") => "Bereits ausgecheckt in {} ({} hinterher)",
        (Language::En, "mr_then_agent") => "– starting {}",
        (Language::De, "mr_then_agent") => "– {} startet",
        (Language::En, "mr_then_shell") => "– opening shell",
        (Language::De, "mr_then_shell") => "– Shell öffnet",
        (Language::En, "mr_then_explorer") => "– opening Explorer",
        (Language::De, "mr_then_explorer") => "– Explorer öffnet",
        (Language::En, "mr_then_ide") => "– opening {}",
        (Language::De, "mr_then_ide") => "– {} öffnet",
        (Language::En, "mr_then_no_ide") => "– no IDE in the profile, nothing opened",
        (Language::De, "mr_then_no_ide") => "– keine IDE im Profil, nichts geöffnet",
        (Language::En, "mr_no_ide") => "No IDE in this folder's profile",
        (Language::De, "mr_no_ide") => "Keine IDE im Profil dieses Ordners",
        (Language::En, "mr_diverged") => "Local branch {} differs from the MR ({} ahead, {} behind) – local state checked out",
        (Language::De, "mr_diverged") => "Lokaler Branch {} weicht vom MR-Stand ab ({} voraus, {} hinterher) – lokaler Stand ausgecheckt",
        (Language::En, "mr_checkout_failed") => "Checking out !{} failed: {}",
        (Language::De, "mr_checkout_failed") => "Auschecken von !{} fehlgeschlagen: {}",
        (Language::En, "mr_checkout_no_remote") => "No remote in {} points to {}",
        (Language::De, "mr_checkout_no_remote") => "Kein Remote in {} zeigt auf {}",
        (Language::En, "mr_repo_open_failed") => "Cannot open repository {}",
        (Language::De, "mr_repo_open_failed") => "Repository {} lässt sich nicht öffnen",
        (Language::En, "mr_folder_exists") => "The folder {} already exists and is not empty",
        (Language::De, "mr_folder_exists") => "Der Ordner {} existiert bereits und ist nicht leer",
        (Language::En, "mr_remote_branch_missing") => "Remote branch {} not found",
        (Language::De, "mr_remote_branch_missing") => "Remote-Branch {} nicht gefunden",
        (Language::En, "mr_branch_name_taken") => "Neither {} nor {} can be used as a local branch",
        (Language::De, "mr_branch_name_taken") => "Weder {} noch {} lassen sich als lokaler Branch nutzen",

        // ── Merge Requests: Zustände der Ansicht ─────────────────────
        (Language::En, "mr_no_remotes") => "No git remote was found in the scanned repositories.",
        (Language::De, "mr_no_remotes") => "In den gescannten Repositories wurde kein Git-Remote gefunden.",
        (Language::En, "mr_no_gitlab_host") => "No GitLab host detected – glab isn't signed in to any of your remote hosts.",
        (Language::De, "mr_no_gitlab_host") => "Kein GitLab-Host erkannt – glab ist für keinen deiner Remote-Hosts angemeldet.",
        (Language::En, "mr_add_token") => "Add token",
        (Language::De, "mr_add_token") => "Token hinzufügen",
        (Language::En, "mr_add_gitlab_host") => "Add as GitLab host",
        (Language::De, "mr_add_gitlab_host") => "Als GitLab-Host hinzufügen",
        (Language::En, "mr_all_deselected") => "All repositories are deselected in the settings.",
        (Language::De, "mr_all_deselected") => "Alle Repositories sind in den Einstellungen abgewählt.",
        (Language::En, "mr_glab_missing") => "glab was not found",
        (Language::De, "mr_glab_missing") => "glab wurde nicht gefunden",
        (Language::En, "mr_glab_missing_desc") => "GitManager queries merge requests via the GitLab CLI (glab).",
        (Language::De, "mr_glab_missing_desc") => "GitManager fragt Merge Requests über die GitLab CLI (glab) ab.",
        (Language::En, "mr_glab_restart_hint") => "After installing, restart GitManager or set the path to glab.exe.",
        (Language::De, "mr_glab_restart_hint") => "Nach der Installation GitManager neu starten oder den Pfad zu glab.exe angeben.",
        (Language::En, "mr_glab_too_old") => "glab {} is too old – please update: winget upgrade GLab.GLab",
        (Language::De, "mr_glab_too_old") => "glab {} ist zu alt – bitte aktualisieren: winget upgrade GLab.GLab",
        (Language::En, "mr_copy_cmd") => "Copy command",
        (Language::De, "mr_copy_cmd") => "Befehl kopieren",
        (Language::En, "mr_not_logged_in") => "Not signed in to {}",
        (Language::De, "mr_not_logged_in") => "Nicht bei {} angemeldet",
        (Language::En, "mr_partial") => "{} could not be loaded ({}).",
        (Language::De, "mr_partial") => "{} konnten nicht geladen werden ({}).",
        (Language::En, "mr_host_failed") => "Merge requests from {} could not be loaded.",
        (Language::De, "mr_host_failed") => "Merge Requests von {} konnten nicht geladen werden.",
        (Language::En, "mr_err_unknown") => "unknown error",
        (Language::De, "mr_err_unknown") => "unbekannter Fehler",
        (Language::En | Language::De, "mr_details") => "Details",

        // ── Merge Requests: Umschalter, Top-Leiste, Statusleiste ──────
        (Language::En | Language::De, "view_repos") => "Repositories",
        (Language::En, "view_repos_tt") => "Show repository list",
        (Language::De, "view_repos_tt") => "Repository-Liste anzeigen",
        (Language::En | Language::De, "view_mrs") => "Merge Requests",
        (Language::En, "view_mrs_tt") => "Show merge requests",
        (Language::De, "view_mrs_tt") => "Merge Requests anzeigen",
        (Language::En, "mr_badge_tt_one") => "Merge requests – 1 review waiting for you",
        (Language::De, "mr_badge_tt_one") => "Merge Requests – 1 Review wartet auf dich",
        (Language::En, "mr_badge_tt_many") => "Merge requests – {} reviews waiting for you",
        (Language::De, "mr_badge_tt_many") => "Merge Requests – {} Reviews warten auf dich",
        (Language::En, "mr_reload_tt") => "Reload merge requests",
        (Language::De, "mr_reload_tt") => "Merge Requests neu laden",
        (Language::En, "mr_status_line") => "Merge requests · {} · updated {}",
        (Language::De, "mr_status_line") => "Merge Requests · {} · Stand: {}",
        (Language::En, "mr_refreshing") => "Refreshing … · updated {}",
        (Language::De, "mr_refreshing") => "Aktualisiere … · Stand: {}",
        (Language::En, "mr_fetch_aborted") => "Loading merge requests was aborted (internal error)",
        (Language::De, "mr_fetch_aborted") => "Abruf der Merge Requests abgebrochen (interner Fehler)",

        // ── Merge Requests: Tray-Fußzeile (E21) ───────────────────────
        (Language::En, "tray_main_window") => "Main window",
        (Language::De, "tray_main_window") => "Hauptfenster",
        (Language::En | Language::De, "tray_mrs") => "Merge Requests",
        (Language::En, "tray_mrs_tt_one") => "Merge requests – 1 review waiting for you",
        (Language::De, "tray_mrs_tt_one") => "Merge Requests – 1 Review wartet auf dich",
        (Language::En, "tray_mrs_tt_many") => "Merge requests – {} reviews waiting for you",
        (Language::De, "tray_mrs_tt_many") => "Merge Requests – {} Reviews warten auf dich",
        (Language::En, "tray_mrs_tt_none") => "Open main window with merge requests",
        (Language::De, "tray_mrs_tt_none") => "Hauptfenster mit Merge Requests öffnen",
        (Language::En, "tray_quit") => "Quit",
        (Language::De, "tray_quit") => "Beenden",

        // ── Merge Requests: Einstellungs-Tab (Task 12) ────────────────
        (Language::En | Language::De, "tabs_merge_requests") => "Merge Requests",
        (Language::En | Language::De, "mrs_sec_cli") => "GitLab CLI",
        (Language::En, "mrs_glab_program") => "glab program",
        (Language::De, "mrs_glab_program") => "glab-Programm",
        (Language::En, "mrs_glab_found") => "glab {} found · {}",
        (Language::De, "mrs_glab_found") => "glab {} gefunden · {}",
        (Language::En, "mrs_glab_not_found") => "glab not found",
        (Language::De, "mrs_glab_not_found") => "glab nicht gefunden",
        (Language::En, "mrs_glab_invalid") => "Invalid glab program: characters like & | ; < > ` $ are only allowed in a full path.",
        (Language::De, "mrs_glab_invalid") => "Ungültiges glab-Programm: Sonderzeichen wie & | ; < > ` $ sind nur in einem vollen Pfad erlaubt.",
        (Language::En, "mrs_recheck") => "Check again",
        (Language::De, "mrs_recheck") => "Erneut prüfen",
        (Language::En, "mrs_sec_auth") => "Sign-in & token",
        (Language::De, "mrs_sec_auth") => "Anmeldung & Token",
        (Language::En, "mrs_state_checking") => "Checking …",
        (Language::De, "mrs_state_checking") => "Prüfe …",
        (Language::En, "mrs_state_ready") => "Ready",
        (Language::De, "mrs_state_ready") => "Bereit",
        (Language::En, "mrs_state_rights_missing") => "Missing rights (read_api or api)",
        (Language::De, "mrs_state_rights_missing") => "Rechte fehlen (read_api oder api)",
        (Language::En, "mrs_state_not_signed") => "Not signed in",
        (Language::De, "mrs_state_not_signed") => "Nicht angemeldet",
        (Language::En, "mrs_state_expired") => "Expired",
        (Language::De, "mrs_state_expired") => "Abgelaufen",
        (Language::En, "mrs_state_revoked") => "Revoked",
        (Language::De, "mrs_state_revoked") => "Widerrufen",
        (Language::En, "mrs_state_unreachable") => "Not reachable",
        (Language::De, "mrs_state_unreachable") => "Nicht erreichbar",
        (Language::En, "mrs_state_tls") => "Certificate rejected",
        (Language::De, "mrs_state_tls") => "Zertifikat abgelehnt",
        (Language::En, "mrs_rights_gitmanager_ok") => "Enough for the merge request view (needs read_api or api)",
        (Language::De, "mrs_rights_gitmanager_ok") => "Reicht für die Merge-Request-Ansicht (benötigt read_api oder api)",
        (Language::En, "mrs_glab_full") => "glab: full",
        (Language::De, "mrs_glab_full") => "glab: Voll",
        (Language::En, "mrs_glab_readonly") => "glab: read-only – glab mr approve/merge/note/create will fail",
        (Language::De, "mrs_glab_readonly") => "glab: Nur lesen – glab mr approve/merge/note/create schlagen fehl",
        (Language::En, "mrs_glab_insufficient") => "glab: insufficient",
        (Language::De, "mrs_glab_insufficient") => "glab: Unzureichend",
        (Language::En, "mrs_glab_unknown") => "glab: rights unknown – scopes not readable (e.g. fine-grained token)",
        (Language::De, "mrs_glab_unknown") => "glab: Rechte unbekannt – Scopes nicht lesbar (z. B. feingranulares Token)",
        (Language::En, "mrs_glab_scopes_hint") => "glab recommends the scopes api and write_repository.",
        (Language::De, "mrs_glab_scopes_hint") => "glab empfiehlt die Scopes api und write_repository.",
        (Language::En, "mrs_signed_in_as") => "Signed in as {} (@{})",
        (Language::De, "mrs_signed_in_as") => "Angemeldet als {} (@{})",
        (Language::En, "mrs_token_kind") => "Token type: {}",
        (Language::De, "mrs_token_kind") => "Token-Art: {}",
        (Language::En, "mrs_kind_pat") => "Personal access token",
        (Language::De, "mrs_kind_pat") => "Persönliches Zugriffstoken",
        (Language::En | Language::De, "mrs_kind_oauth") => "OAuth (glab)",
        (Language::En, "mrs_kind_bot") => "Project/group token",
        (Language::De, "mrs_kind_bot") => "Projekt-/Gruppen-Token",
        (Language::En, "mrs_kind_granular") => "Fine-grained token",
        (Language::De, "mrs_kind_granular") => "Feingranulares Token",
        (Language::En, "mrs_token_source") => "Stored in: {}",
        (Language::De, "mrs_token_source") => "Gespeichert in: {}",
        (Language::En, "mrs_src_keyring") => "Windows Credential Manager",
        (Language::De, "mrs_src_keyring") => "Windows-Anmeldeinformationsverwaltung",
        (Language::En, "mrs_src_config") => "glab config",
        (Language::De, "mrs_src_config") => "glab-Konfiguration",
        (Language::En, "mrs_src_env") => "Environment variable {}",
        (Language::De, "mrs_src_env") => "Umgebungsvariable {}",
        (Language::En, "mrs_expires") => "expires on {}",
        (Language::De, "mrs_expires") => "läuft ab am {}",
        (Language::En, "mrs_expires_soon") => "Expires in {} days",
        (Language::De, "mrs_expires_soon") => "Läuft in {} Tagen ab",
        (Language::En, "mrs_expires_soon_one") => "Expires in 1 day",
        (Language::De, "mrs_expires_soon_one") => "Läuft in 1 Tag ab",
        (Language::En | Language::De, "mrs_scopes") => "Scopes:",
        (Language::En, "mrs_check") => "Check",
        (Language::De, "mrs_check") => "Prüfen",
        (Language::En, "mrs_add_token") => "Add token …",
        (Language::De, "mrs_add_token") => "Token hinzufügen …",
        (Language::En, "mrs_replace_token") => "Replace token …",
        (Language::De, "mrs_replace_token") => "Token ersetzen …",
        (Language::En | Language::De, "mrs_token_label") => "Token",
        (Language::En, "mrs_show_token") => "Show",
        (Language::De, "mrs_show_token") => "Anzeigen",
        (Language::En, "mrs_purpose") => "Purpose",
        (Language::De, "mrs_purpose") => "Zweck",
        (Language::En, "mrs_purpose_read") => "Read-only – read_api (enough for GitManager)",
        (Language::De, "mrs_purpose_read") => "Nur lesen – read_api (reicht für GitManager)",
        (Language::En, "mrs_purpose_full") => "Full – api, write_repository (also glab write commands)",
        (Language::De, "mrs_purpose_full") => "Voll – api, write_repository (auch glab-Schreibbefehle)",
        (Language::En, "mrs_create_token") => "Create token in GitLab",
        (Language::De, "mrs_create_token") => "Token in GitLab erstellen",
        (Language::En, "mrs_protocol") => "Protocol",
        (Language::De, "mrs_protocol") => "Protokoll",
        (Language::En, "mrs_token_note") => "The token is stored in glab and replaces the glab sign-in for {} – also for glab in the terminal.",
        (Language::De, "mrs_token_note") => "Das Token wird in glab gespeichert und ersetzt die glab-Anmeldung für {} – auch für glab im Terminal.",
        (Language::En, "mrs_http_warning") => "{} uses HTTP – the token is sent unencrypted.",
        (Language::De, "mrs_http_warning") => "{} nutzt HTTP – das Token wird unverschlüsselt übertragen.",
        (Language::En, "mrs_http_chip") => "http – unencrypted",
        (Language::De, "mrs_http_chip") => "http – unverschlüsselt",
        (Language::En, "mrs_token_empty") => "Please paste a token.",
        (Language::De, "mrs_token_empty") => "Bitte ein Token einfügen.",
        (Language::En, "mrs_token_whitespace") => "The token contains spaces or line breaks.",
        (Language::De, "mrs_token_whitespace") => "Das Token enthält Leerzeichen oder Zeilenumbrüche.",
        (Language::En, "mrs_token_invalid") => "The token is invalid.",
        (Language::De, "mrs_token_invalid") => "Das Token ist ungültig.",
        (Language::En, "mrs_verify") => "Verify",
        (Language::De, "mrs_verify") => "Prüfen",
        (Language::En, "mrs_preview") => "@{} · {} · expires {} · {}/{} repos accessible",
        (Language::De, "mrs_preview") => "@{} · {} · läuft ab {} · {}/{} Repos zugreifbar",
        (Language::En, "mrs_preview_no_expiry") => "@{} · {} · {}/{} repos accessible",
        (Language::De, "mrs_preview_no_expiry") => "@{} · {} · {}/{} Repos zugreifbar",
        (Language::En, "mrs_downgrade_warning") => "The current sign-in has api. With this token glab loses write access (mr approve/merge/note).",
        (Language::De, "mrs_downgrade_warning") => "Die bisherige Anmeldung hat api. Mit diesem Token verliert glab Schreibrechte (mr approve/merge/note).",
        (Language::En, "mrs_downgrade_confirm") => "Save anyway",
        (Language::De, "mrs_downgrade_confirm") => "Trotzdem speichern",
        (Language::En, "mrs_token_saved") => "Token saved for {}",
        (Language::De, "mrs_token_saved") => "Token für {} gespeichert",
        (Language::En, "mrs_token_failed") => "glab could not store the token: {}",
        (Language::De, "mrs_token_failed") => "glab konnte das Token nicht speichern: {}",
        (Language::En, "mrs_env_banner") => "The environment variable {} is set and overrides stored tokens for all hosts.",
        (Language::De, "mrs_env_banner") => "Die Umgebungsvariable {} ist gesetzt und überschreibt gespeicherte Tokens für alle Hosts.",
        (Language::En, "mrs_unrecognized") => "Remote hosts not signed in to glab ({})",
        (Language::De, "mrs_unrecognized") => "Nicht bei glab angemeldete Remote-Hosts ({})",
        (Language::En, "mrs_unrecognized_hint") => "GitManager sends no requests to these hosts until you add a token or add them as a GitLab host. You can ignore hosts that aren't GitLab (e.g. github.com).",
        (Language::De, "mrs_unrecognized_hint") => "An diese Hosts schickt GitManager keine Anfragen, bis du ein Token hinzufügst oder sie als GitLab-Host einträgst. Hosts, die kein GitLab sind (z. B. github.com), kannst du ignorieren.",
        (Language::En, "mrs_host_repos") => "{} repos",
        (Language::De, "mrs_host_repos") => "{} Repos",
        (Language::En, "mrs_host_repos_one") => "1 repo",
        (Language::De, "mrs_host_repos_one") => "1 Repo",
        (Language::En, "mrs_belongs_to") => "belongs to …",
        (Language::De, "mrs_belongs_to") => "gehört zu …",
        (Language::En, "mrs_sec_repos") => "Repositories for merge requests",
        (Language::De, "mrs_sec_repos") => "Repositories für Merge Requests",
        (Language::En, "mrs_included") => "{} of {} included · {} accessible",
        (Language::De, "mrs_included") => "{} von {} einbezogen · {} zugreifbar",
        (Language::En, "mrs_all") => "All",
        (Language::De, "mrs_all") => "Alle",
        (Language::En, "mrs_none") => "None",
        (Language::De, "mrs_none") => "Keine",
        (Language::En, "mrs_check_access") => "Check access",
        (Language::De, "mrs_check_access") => "Zugriff prüfen",
        (Language::En | Language::De, "mrs_col_repo") => "Repository",
        (Language::En, "mrs_col_clones") => "Clones",
        (Language::De, "mrs_col_clones") => "Klone",
        (Language::En, "mrs_col_project") => "GitLab project",
        (Language::De, "mrs_col_project") => "GitLab-Projekt",
        (Language::En, "mrs_col_access") => "Access",
        (Language::De, "mrs_col_access") => "Zugriff",
        (Language::En, "mrs_clones") => "{} clones",
        (Language::De, "mrs_clones") => "{} Klone",
        (Language::En, "mrs_acc_ok") => "Access OK ({})",
        (Language::De, "mrs_acc_ok") => "Zugriff ok ({})",
        (Language::En, "mrs_acc_ok_plain") => "Access OK",
        (Language::De, "mrs_acc_ok_plain") => "Zugriff ok",
        (Language::En, "mrs_acc_public") => "public, not a member",
        (Language::De, "mrs_acc_public") => "öffentlich, kein Mitglied",
        (Language::En, "mrs_acc_renamed") => "renamed → {}",
        (Language::De, "mrs_acc_renamed") => "umbenannt → {}",
        (Language::En, "mrs_acc_denied") => "no access with this token",
        (Language::De, "mrs_acc_denied") => "kein Zugriff mit diesem Token",
        (Language::En, "mrs_acc_mr_off") => "MRs disabled in project",
        (Language::De, "mrs_acc_mr_off") => "MRs im Projekt deaktiviert",
        (Language::En, "mrs_acc_archived") => "archived",
        (Language::De, "mrs_acc_archived") => "archiviert",
        (Language::En, "mrs_acc_host_auth") => "Host not signed in",
        (Language::De, "mrs_acc_host_auth") => "Host nicht angemeldet",
        (Language::En, "mrs_acc_checking") => "Checking …",
        (Language::De, "mrs_acc_checking") => "Prüfe …",
        (Language::En, "mrs_acc_skipped") => "skipped (no access)",
        (Language::De, "mrs_acc_skipped") => "wird übersprungen (kein Zugriff)",
        (Language::En, "mrs_include_nonlocal") => "Also show MRs from projects without a local clone",
        (Language::De, "mrs_include_nonlocal") => "Auch MRs aus Projekten ohne lokalen Klon anzeigen",
        (Language::En, "mrs_sec_checkout") => "Checkout",
        (Language::De, "mrs_sec_checkout") => "Auschecken",
        (Language::En, "mrs_review_root") => "Folder for reviews (MRs from others)",
        (Language::De, "mrs_review_root") => "Ordner für Reviews (MRs von anderen)",
        (Language::En, "mrs_review_root_ok") => "Inside search path {} – new worktrees appear in the repository list.",
        (Language::De, "mrs_review_root_ok") => "Liegt im Suchpfad {} – neue Worktrees erscheinen in der Repo-Liste.",
        (Language::En, "mrs_review_root_warn") => "Not inside a search path – new worktrees won't appear in the repository list.",
        (Language::De, "mrs_review_root_warn") => "Liegt in keinem Suchpfad – neue Worktrees erscheinen nicht in der Repo-Liste.",
        (Language::En, "mrs_review_root_relative") => "Not an absolute path – ignored, new worktrees go next to the base clone.",
        (Language::De, "mrs_review_root_relative") => "Kein absoluter Pfad – wird ignoriert, neue Worktrees landen neben dem Basis-Klon.",
        (Language::En, "mrs_add_search_path") => "Add as search path",
        (Language::De, "mrs_add_search_path") => "Als Suchpfad hinzufügen",
        (Language::En, "mrs_folder_name") => "Folder name",
        (Language::De, "mrs_folder_name") => "Ordnername",
        (Language::En, "mrs_folder_name_example") => "e.g. {}",
        (Language::De, "mrs_folder_name_example") => "z. B. {}",
        (Language::En, "mrs_after_checkout") => "Open after checkout",
        (Language::De, "mrs_after_checkout") => "Nach dem Auschecken öffnen",
        (Language::En, "mrs_after_last") => "Last choice",
        (Language::De, "mrs_after_last") => "Zuletzt gewählt",
        (Language::En, "mrs_after_last_hint") => "last: {}",
        (Language::De, "mrs_after_last_hint") => "zuletzt: {}",
        (Language::En, "mrs_sec_display") => "Display & refresh",
        (Language::De, "mrs_sec_display") => "Anzeige & Aktualisierung",
        (Language::En, "mrs_refresh") => "Refresh automatically",
        (Language::De, "mrs_refresh") => "Automatisch aktualisieren",
        (Language::En, "mrs_refresh_unit") => "min (0 = off)",
        (Language::De, "mrs_refresh_unit") => "Min. (0 = aus)",
        (Language::En, "mrs_show_all_default") => "“All open” on by default",
        (Language::De, "mrs_show_all_default") => "„Alle offenen“ standardmäßig aktiv",
        (Language::En, "mrs_ignore_bots") => "Don't count bot approvals",
        (Language::De, "mrs_ignore_bots") => "Approvals von Bots nicht mitzählen",
        (Language::En, "mrs_approval_goal") => "Number of approvals for a full bar",
        (Language::De, "mrs_approval_goal") => "Approvals für vollen Balken",
        (Language::En, "mrs_approval_goal_unit") => "(0 = off; only without an approval rule in GitLab)",
        (Language::De, "mrs_approval_goal_unit") => "(0 = aus; nur ohne Approval-Regel in GitLab)",
        (Language::En, "mrs_check_finished") => "Check for {} finished",
        (Language::De, "mrs_check_finished") => "Prüfung für {} abgeschlossen",
        (Language::En, "mrs_belongs_to_tt") => "Another name (e.g. SSH host) for a signed-in GitLab host",
        (Language::De, "mrs_belongs_to_tt") => "Anderer Name (z. B. SSH-Host) für einen angemeldeten GitLab-Host",
        (Language::En, "mrs_alias_apply") => "Assign",
        (Language::De, "mrs_alias_apply") => "Zuordnen",
        (Language::En, "mrs_alias_of") => "{} belongs to this host",
        (Language::De, "mrs_alias_of") => "{} gehört zu diesem Host",
        (Language::En, "mrs_alias_remove") => "Remove mapping",
        (Language::De, "mrs_alias_remove") => "Zuordnung entfernen",
        (Language::En, "mrs_extra_redundant") => "Belongs to {} and is queried through it – this entry isn't needed and can be removed.",
        (Language::De, "mrs_extra_redundant") => "Gehört zu {} und wird darüber abgefragt – dieser Eintrag ist nicht nötig und kann entfernt werden.",
        (Language::En, "mrs_glab_pending") => "Applies after “Save” – until then GitManager uses {}.",
        (Language::De, "mrs_glab_pending") => "Gilt nach „Speichern“ – bis dahin nutzt GitManager {}.",

        // ── Relative Zeiten ──────────────────────────────────────────
        (Language::En, "rel_now") => "just now",
        (Language::De, "rel_now") => "gerade eben",
        (Language::En, "rel_min") => "{} min ago",
        (Language::De, "rel_min") => "vor {} Min.",
        (Language::En, "rel_hour") => "{} h ago",
        (Language::De, "rel_hour") => "vor {} Std.",
        (Language::En, "rel_yesterday") => "yesterday",
        (Language::De, "rel_yesterday") => "gestern",
        (Language::En, "rel_days") => "{} days ago",
        (Language::De, "rel_days") => "vor {} Tagen",

        // fallback for any missing key -> return key itself
        // ── Repo-Karte: Chips, Stern, ⋯-Menü ──────────────────────────
        (Language::En, "repo_state_clean") => "clean",
        (Language::De, "repo_state_clean") => "sauber",
        (Language::En, "repo_state_dirty") => "{} changed",
        (Language::De, "repo_state_dirty") => "{} geändert",
        (Language::En, "repo_state_conflict") => "Conflict",
        (Language::De, "repo_state_conflict") => "Konflikt",
        (Language::En, "repo_state_merge") => "Merge in progress",
        (Language::De, "repo_state_merge") => "Merge läuft",
        (Language::En, "repo_state_rebase") => "Rebase in progress",
        (Language::De, "repo_state_rebase") => "Rebase läuft",
        (Language::En, "repo_state_cherry_pick") => "Cherry-pick in progress",
        (Language::De, "repo_state_cherry_pick") => "Cherry-Pick läuft",
        (Language::En, "repo_state_revert") => "Revert in progress",
        (Language::De, "repo_state_revert") => "Revert läuft",
        (Language::En, "repo_state_other") => "Git operation in progress",
        (Language::De, "repo_state_other") => "Git-Vorgang läuft",
        (Language::En, "repo_state_conflict_tt") => "Unresolved merge conflicts – resolve them in the IDE or shell",
        (Language::De, "repo_state_conflict_tt") => "Ungelöste Merge-Konflikte – in IDE oder Shell auflösen",
        (Language::En, "repo_state_in_progress_tt") => "A Git operation was started but not finished",
        (Language::De, "repo_state_in_progress_tt") => "Ein Git-Vorgang wurde begonnen, aber nicht abgeschlossen",
        (Language::En, "repo_ahead_tt") => "{} unpushed commit(s)",
        (Language::De, "repo_ahead_tt") => "{} ungepushte Commit(s)",
        (Language::En, "repo_behind_tt") => "{} commit(s) behind – click: fast-forward",
        (Language::De, "repo_behind_tt") => "{} Commit(s) hinterher – Klick: Fast-Forward",
        (Language::En, "repo_mr_chip_tt") => "{}\n{}\nClick: show in Merge Requests",
        (Language::De, "repo_mr_chip_tt") => "{}\n{}\nKlick: in Merge Requests zeigen",
        (Language::En, "repo_mr_draft") => "Draft",
        (Language::De, "repo_mr_draft") => "Entwurf",
        (Language::En, "repo_claude_waiting") => "Claude waiting",
        (Language::De, "repo_claude_waiting") => "Claude wartet",
        (Language::En, "repo_claude_working") => "Claude working",
        (Language::De, "repo_claude_working") => "Claude arbeitet",
        (Language::En, "repo_claude_tt") => "Click: bring the session's terminal to the front",
        (Language::De, "repo_claude_tt") => "Klick: Terminal der Session in den Vordergrund",
        (Language::En, "repo_fav_add") => "Add to favorites",
        (Language::De, "repo_fav_add") => "Als Favorit markieren",
        (Language::En, "repo_fav_remove") => "Remove from favorites",
        (Language::De, "repo_fav_remove") => "Favorit entfernen",
        (Language::En, "repo_more") => "More actions",
        (Language::De, "repo_more") => "Weitere Aktionen",
        (Language::En, "repo_menu_open_in") => "Open in",
        (Language::De, "repo_menu_open_in") => "Öffnen in",
        (Language::En, "repo_menu_profile") => "Profile",
        (Language::De, "repo_menu_profile") => "Profil",
        (Language::En, "repo_menu_new_worktree") => "New worktree …",
        (Language::De, "repo_menu_new_worktree") => "Neuer Worktree …",
        (Language::En, "repo_menu_copy_path") => "Copy path",
        (Language::De, "repo_menu_copy_path") => "Pfad kopieren",
        (Language::En, "repo_menu_copy_branch") => "Copy branch name",
        (Language::De, "repo_menu_copy_branch") => "Branch-Namen kopieren",
        (Language::En, "repo_menu_open_browser") => "Open remote in browser",
        (Language::De, "repo_menu_open_browser") => "Remote im Browser öffnen",
        (Language::En, "repo_menu_remove_worktree") => "Remove worktree …",
        (Language::De, "repo_menu_remove_worktree") => "Worktree entfernen …",
        (Language::En, "repo_cleanup_button") => "Clean up …",
        (Language::De, "repo_cleanup_button") => "Aufräumen …",
        (Language::En, "repo_cleanup_tooltip") => "Find merged worktrees and branches that can be removed",
        (Language::De, "repo_cleanup_tooltip") => "Gemergte Worktrees und Branches zum Entfernen suchen",
        (Language::En, "repo_commit_subject") => "“{}”",
        (Language::De, "repo_commit_subject") => "„{}“",
        (Language::En | Language::De, _) => key,
    }.to_string()
}

/// Format helper: replaces `{}` placeholders sequentially.
/// Sucht hinter dem eingesetzten Text weiter — Argumente mit `{}` (Branch-
/// namen, Pfade) verschlucken sonst den nächsten Platzhalter.
pub fn tr_fmt(lang: Language, key: &str, args: &[&str]) -> String {
    let mut s = tr(lang, key);
    let mut from = 0;
    for arg in args {
        let Some(rel) = s[from..].find("{}") else {
            break;
        };
        let pos = from + rel;
        s.replace_range(pos..pos + 2, arg);
        from = pos + arg.len();
    }
    s
}

/// Einzahl/Mehrzahl über getrennte Schlüssel: `one` ohne Platzhalter,
/// `many` mit `{}` für die Anzahl.
pub fn count_text(lang: Language, n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        tr(lang, one)
    } else {
        tr_fmt(lang, many, &[&n.to_string()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_default_is_en() {
        assert_eq!(Language::default(), Language::En);
        assert_eq!(Language::En.code(), "en");
        assert_eq!(Language::De.code(), "de");
    }

    #[test]
    fn display_names() {
        assert_eq!(Language::En.display_name(), "English");
        assert_eq!(Language::De.display_name(), "Deutsch");
        assert_eq!(Language::all().len(), 2);
    }

    #[test]
    fn tr_returns_not_empty() {
        assert!(!tr(Language::En, "settings").is_empty());
        assert!(!tr(Language::De, "settings").is_empty());
        assert_eq!(tr(Language::En, "settings"), "Settings");
        assert_eq!(tr(Language::De, "settings"), "Einstellungen");
    }

    #[test]
    fn tr_fmt_replaces() {
        assert_eq!(
            tr_fmt(Language::En, "profiles_count", &["3"]),
            "Profiles (3):"
        );
        assert_eq!(
            tr_fmt(Language::De, "profiles_count", &["3"]),
            "Profile (3):"
        );
        // Argument mit `{}` (gültiger Branchname) bleibt wörtlich, der
        // nächste Platzhalter bekommt das nächste Argument.
        assert_eq!(
            tr_fmt(
                Language::En,
                "branch_checked_out_elsewhere",
                &["feat/{}", "C:\\wt"]
            ),
            "Branch feat/{} is checked out in C:\\wt"
        );
    }

    #[test]
    fn tr_fallback_returns_key() {
        assert_eq!(
            tr(Language::En, "nonexistent_key_xyz"),
            "nonexistent_key_xyz"
        );
    }

    #[test]
    fn tr_config_keys_exist() {
        let keys = [
            "config_selector_title",
            "config_selector_desc",
            "add_config_selector",
            "config_file",
            "config_key",
            "config_options",
            "allow_custom",
            "config_saved",
            "config_missing",
            "key_attr",
            "value_attr",
        ];
        for key in keys {
            assert_ne!(tr(Language::En, key), key, "missing EN key: {key}");
            assert_ne!(tr(Language::De, key), key, "missing DE key: {key}");
        }
        // config_saved must support tr_fmt with 2 args without fallback
        let en = tr_fmt(Language::En, "config_saved", &["DB", "prod"]);
        assert_ne!(en, "config_saved");
        assert!(
            en.contains("DB") && en.contains("prod"),
            "config_saved EN fmt: {en}"
        );
        let de = tr_fmt(Language::De, "config_saved", &["DB", "prod"]);
        assert_ne!(de, "config_saved");
        assert!(
            de.contains("DB") && de.contains("prod"),
            "config_saved DE fmt: {de}"
        );
    }

    #[test]
    fn tr_tray_keys_exist() {
        for key in [
            "tray_behavior_title",
            "tray_minimize_label",
            "tray_minimize_hint_on",
            "tray_minimize_hint_off",
            "tray_update_label",
            "tray_update_hint_on",
            "tray_update_hint_off",
            "tray_update_error",
            "tray_creation_error",
            "tray_branch_limit_title",
            "tray_branch_limit_desc",
            "tray_branch_limit_summary",
            "tray_all_hidden_warn",
            "tray_limit_label",
            "tray_direct_label",
            "tray_notice_branch_dialog",
            "tray_notice_error",
            "update_available_title",
            "update_available_msg",
            "open_link",
            "tray_icon_theme_title",
            "tray_icon_theme_desc",
            "tray_icon_theme_dark",
            "tray_icon_theme_light",
        ] {
            assert_ne!(tr(Language::En, key), key, "missing EN key: {key}");
            assert_ne!(tr(Language::De, key), key, "missing DE key: {key}");
        }
        let e = tr_fmt(Language::En, "tray_update_error", &["timeout"]);
        assert!(e.contains("timeout"));
        let s = tr_fmt(Language::En, "tray_branch_limit_summary", &["7"]);
        assert!(s.contains('7'), "summary must embed limit: {s}");
        let sde = tr_fmt(Language::De, "tray_branch_limit_summary", &["7"]);
        assert!(sde.contains('7'), "summary DE must embed limit: {sde}");
        assert!(
            sde.contains("Tray-Popup"),
            "DE summary should mention Tray-Popup: {sde}"
        );
    }

    #[test]
    fn tr_claude_new_keys_exist() {
        for key in [
            "claude_new_button",
            "claude_new_tooltip",
            "claude_new_title",
            "claude_new_hint",
            "claude_new_search_hint",
            "claude_new_no_matches",
            "claude_notify_label",
            "claude_notify_hint",
            "claude_notify_btn_terminal",
            "claude_notify_btn_show",
            "claude_notify_attr_done",
            "claude_notify_attr_done_after",
            "claude_notify_attr_waiting",
            "claude_notify_attr_waiting_after",
            "claude_notify_waiting",
            "claude_notify_done",
            "claude_preview_show",
            "claude_preview_hide",
            "claude_preview_empty",
            "claude_preview_you",
            "claude_new_no_folders",
            "claude_new_start",
            "claude_new_default_badge",
            "folder_files_label",
            "folder_files_hint",
            "folder_files_path",
            "folder_files_path_hint",
            "folder_files_content",
            "folder_files_add",
            "folder_files_remove",
            "folder_files_invalid",
            "folder_files_written",
            "folder_files_failed",
            "claude_dir_label",
            "claude_dir_hint",
            "claude_dir_placeholder",
        ] {
            assert_ne!(tr(Language::En, key), key, "missing EN key: {key}");
            assert_ne!(tr(Language::De, key), key, "missing DE key: {key}");
        }
    }

    #[test]
    fn tr_autostart_keys_exist() {
        for key in ["autostart_label", "autostart_hint"] {
            assert_ne!(tr(Language::En, key), key, "missing EN key: {key}");
            assert_ne!(tr(Language::De, key), key, "missing DE key: {key}");
        }
    }

    #[test]
    fn tr_sort_keys_exist() {
        for key in [
            "sort_by_name",
            "sort_by_last_used",
            "sort_tooltip",
            "sort_title",
        ] {
            assert_ne!(tr(Language::En, key), key, "missing EN key: {key}");
            assert_ne!(tr(Language::De, key), key, "missing DE key: {key}");
        }
    }

    #[test]
    fn tr_fetch_keys_exist() {
        for key in [
            "fetch_failed",
            "fetch_partial",
            "auth_hint",
            "fetch_aborted",
        ] {
            assert_ne!(tr(Language::En, key), key, "missing EN key: {key}");
            assert_ne!(tr(Language::De, key), key, "missing DE key: {key}");
        }
        // DE wrapper texts stay byte-identical (review I1).
        assert_eq!(tr(Language::De, "fetch_failed"), "Fetch fehlgeschlagen: {}");
        assert_eq!(
            tr(Language::De, "fetch_partial"),
            "Fetch teilweise fehlgeschlagen: {}"
        );
        assert_eq!(
            tr(Language::De, "auth_hint"),
            "Hinweis: Credentials/SSH prüfen (ggf. Auth erforderlich)"
        );
        let en = tr_fmt(Language::En, "fetch_failed", &["boom"]);
        assert!(
            en.contains("boom"),
            "fetch_failed EN must embed detail: {en}"
        );
        let de = tr_fmt(Language::De, "fetch_failed", &["boom"]);
        assert!(
            de.contains("boom"),
            "fetch_failed DE must embed detail: {de}"
        );
        let en_p = tr_fmt(Language::En, "fetch_partial", &["boom"]);
        assert!(
            en_p.contains("boom"),
            "fetch_partial EN must embed detail: {en_p}"
        );
        let de_p = tr_fmt(Language::De, "fetch_partial", &["boom"]);
        assert!(
            de_p.contains("boom"),
            "fetch_partial DE must embed detail: {de_p}"
        );
        // I1: EN wrapper + EN hint must be pure EN (no German embeds).
        for s in [
            tr(Language::En, "fetch_failed"),
            tr(Language::En, "fetch_partial"),
            tr(Language::En, "auth_hint"),
        ] {
            for german in [
                "Hinweis",
                "prüfen",
                "fehlgeschlagen",
                "teilweise",
                "Erfolgreich",
            ] {
                assert!(
                    !s.contains(german),
                    "EN fetch text must not embed German '{german}': {s}"
                );
            }
        }
        // Full EN message (wrapper + opaque detail + hint) stays pure EN.
        let hint_en = tr(Language::En, "auth_hint");
        let full_en = tr_fmt(
            Language::En,
            "fetch_failed",
            &[&format!("'bad': timeout. {hint_en}")],
        );
        assert!(full_en.contains("timeout"));
        for german in ["Hinweis", "prüfen", "fehlgeschlagen", "teilweise"] {
            assert!(
                !full_en.contains(german),
                "EN full fetch message must not embed German '{german}': {full_en}"
            );
        }
    }

    #[test]
    fn tr_behind_auto_fetch_keys_exist() {
        for key in [
            "branch_behind",
            "auto_fetch_label",
            "auto_fetch_hint",
            "auto_fetch_off",
            "auto_fetch_running",
        ] {
            assert_ne!(tr(Language::En, key), key, "missing EN key: {key}");
            assert_ne!(tr(Language::De, key), key, "missing DE key: {key}");
        }
        let en = tr_fmt(Language::En, "branch_behind", &["3"]);
        assert!(en.contains('3'), "branch_behind EN must embed count: {en}");
        let de = tr_fmt(Language::De, "branch_behind", &["3"]);
        assert!(de.contains('3'), "branch_behind DE must embed count: {de}");
    }

    #[test]
    fn tr_branch_search_keys_exist() {
        // Task 6: Tray-Branch-Suche braucht einen eigenen Placeholder.
        assert_ne!(tr(Language::En, "branch_search"), "branch_search");
        assert_ne!(tr(Language::De, "branch_search"), "branch_search");
        assert!(
            !tr(Language::En, "branch_search").is_empty(),
            "EN branch_search must not be empty"
        );
        assert!(
            !tr(Language::De, "branch_search").is_empty(),
            "DE branch_search must not be empty"
        );
    }

    #[test]
    fn tr_tray_branch_more_keys_exist() {
        // Task 6 fix (Finding 2): Overflow-Hinweis sprachabhängig.
        for key in ["tray_branch_more_filtered", "tray_branch_more_main"] {
            assert_ne!(tr(Language::En, key), key, "missing EN key: {key}");
            assert_ne!(tr(Language::De, key), key, "missing DE key: {key}");
        }
        // De-Texte identisch zum bisherigen Hardcode.
        let de_f = tr_fmt(Language::De, "tray_branch_more_filtered", &["5"]);
        assert_eq!(
            de_f, "+ 5 weitere (filtere)",
            "DE filtered must stay: {de_f}"
        );
        let de_m = tr_fmt(Language::De, "tray_branch_more_main", &["5"]);
        assert_eq!(
            de_m, "+ 5 weitere (in Hauptfenster)",
            "DE main must stay: {de_m}"
        );
        // En darf kein Deutsch enthalten.
        let en_f = tr_fmt(Language::En, "tray_branch_more_filtered", &["5"]);
        assert!(en_f.contains('5'), "EN filtered must embed count: {en_f}");
        assert!(
            !en_f.contains("weitere"),
            "EN filtered must not be German: {en_f}"
        );
        let en_m = tr_fmt(Language::En, "tray_branch_more_main", &["5"]);
        assert!(en_m.contains('5'), "EN main must embed count: {en_m}");
        assert!(
            !en_m.contains("weitere"),
            "EN main must not be German: {en_m}"
        );
    }

    #[test]
    fn tr_mr_keys_exist() {
        // Merge-Requests-Plan: jeder Task trägt seine Schlüssel hier nach
        // (Tabelle anhang/i18n-keys.md).
        let keys = [
            // Task 1 (E15): Branch-Wechsel auf Remote-Branch
            "branch_local_ahead",
            "branch_local_ahead_one",
            "branch_local_diverged",
            "branch_fast_forwarded",
            "branch_fast_forwarded_one",
            "branch_upstream_mismatch",
            "branch_checked_out_elsewhere",
            // Task 6: Approvals, Pipeline, Threads, Review, relative Zeiten
            "mr_appr_of",
            "mr_appr_one",
            "mr_appr_n",
            "mr_appr_short",
            "mr_appr_no_rule",
            "mr_appr_goal_tt",
            "mr_appr_you",
            "mr_rev_started",
            "mr_rev_waiting",
            "mr_pipe_success",
            "mr_pipe_failed",
            "mr_pipe_running",
            "mr_pipe_pending",
            "mr_pipe_canceled",
            "mr_pipe_skipped",
            "mr_pipe_manual",
            "mr_pipe_none",
            "mr_threads_one",
            "mr_threads_many",
            "mr_threads_resolved",
            "mr_threads_none",
            "mr_threads_tt",
            "mr_threads_none_tt",
            "mr_rev_pending",
            "mr_rev_approved",
            "mr_rev_changes",
            "mr_rev_commented",
            "rel_now",
            "rel_min",
            "rel_hour",
            "rel_yesterday",
            "rel_days",
            // Task 7: Abruf- und Prüffehler, Abfrageteile
            "mr_err_scope",
            "mr_err_network",
            "mr_err_tls",
            "mr_err_timeout",
            "mr_err_content_type",
            "mr_part_mentions",
            "mr_part_reviews",
            "mr_part_uninvolved",
            // Task 9: Ansicht, Spalten, Chips, Zeilen, Checkout-Menü, Zustände
            "mr_part_mine",
            "mr_open_settings",
            "mr_loading",
            "mr_mine_title",
            "mr_others_title",
            "mr_count_of",
            "mr_sorted",
            "mr_sorted_tt",
            "mr_chip_reviewer",
            "mr_chip_reviewer_tt",
            "mr_chip_mention",
            "mr_chip_mention_tt",
            "mr_chip_all",
            "mr_chip_all_tt",
            "mr_chip_all_tt_n",
            "mr_chip_all_tt_n_one",
            "mr_reason_reviewer",
            "mr_reason_desc",
            "mr_reason_comment",
            "mr_reasons_prefix",
            "mr_no_filter",
            "mr_no_filter_hint",
            "mr_filter_no_match",
            "mr_empty_mine",
            "mr_empty_others",
            "mr_hidden_by_selection_one",
            "mr_hidden_by_selection_many",
            "mr_todos_hidden",
            "mr_search_mine_hint",
            "mr_search_others_hint",
            "mr_search_clear",
            "mr_search_clear_link",
            "mr_no_hits",
            "mr_seg_open",
            "mr_seg_open_tt",
            "mr_seg_merged",
            "mr_seg_merged_tt",
            "mr_sorted_merged",
            "mr_sorted_merged_tt",
            "mr_merged_caption",
            "mr_period",
            "mr_period_tt",
            "mr_merged_by",
            "mr_merged_by_me",
            "mr_merged_at",
            "mr_loading_merged",
            "mr_empty_merged",
            "mr_part_merged",
            "mr_todos_hidden_fix",
            "mrs_state_limited",
            "mrs_todos_hidden",
            "mr_uninvolved_h",
            "mr_uninvolved_sub",
            "mr_uninvolved_sub_one",
            "mr_shown_of",
            "mr_load_more",
            "mr_loading_uninvolved",
            "mr_uninvolved_not_loaded",
            "mr_load_now",
            "mr_new_comment_one",
            "mr_new_comments",
            "mr_new_commit_one",
            "mr_new_commits",
            "mr_new_push",
            "mr_new_tt",
            "mr_notify_label",
            "mr_notify_review",
            "mr_notify_review_line",
            "mr_notify_mention_comment",
            "mr_notify_mention_desc",
            "mr_notify_mention_line",
            "mr_notify_comments",
            "mr_notify_comments_line",
            "mr_notify_comments_more",
            "mr_notify_more",
            "mr_notify_btn_open",
            "mr_notify_btn_show",
            "mr_draft",
            "mr_ready",
            "mr_ready_tt",
            "mr_pipe_open_tt",
            "mr_open_browser",
            "mr_open_title_tt",
            "mr_checkout_menu_tt",
            "mr_menu_header_clones",
            "mr_menu_header_clone_one",
            "mr_menu_new_folder",
            "mr_menu_worktree_review",
            "mr_menu_worktree",
            "mr_menu_worktree_sub",
            "mr_menu_other_folder",
            "mr_menu_clone",
            "mr_menu_clone_sub",
            "mr_menu_switch",
            "mr_state_free",
            "mr_state_in_use",
            "mr_state_in_use_branch",
            "mr_state_dirty_one",
            "mr_state_dirty_many",
            "mr_state_dirty_hint",
            "mr_last_used",
            "mr_menu_after",
            "mr_after_shell",
            "mr_after_explorer",
            "mr_after_nothing",
            "mr_after_remembered",
            "mr_already_header",
            "mr_open_show_in_list",
            "mr_open_agent",
            "mr_open_shell",
            "mr_open_explorer",
            "mr_open_ide",
            "mr_checked_out_tt",
            "mr_not_local",
            "mr_not_local_tt",
            "mr_fork_tt",
            "mr_branch_deleted_tt",
            "mr_no_remotes",
            "mr_no_gitlab_host",
            "mr_add_token",
            "mr_add_gitlab_host",
            "mr_all_deselected",
            "mr_glab_missing",
            "mr_glab_missing_desc",
            "mr_glab_restart_hint",
            "mr_glab_too_old",
            "mr_copy_cmd",
            "mr_not_logged_in",
            "mr_partial",
            "mr_host_failed",
            "mr_err_unknown",
            "mr_details",
            // Task 10: Umschalter, Badge, Top-Leiste, Statusleiste
            "view_repos",
            "view_repos_tt",
            "view_mrs",
            "view_mrs_tt",
            "view_claude",
            "view_claude_tt",
            "mr_badge_tt_one",
            "mr_badge_tt_many",
            "mr_reload_tt",
            "mr_status_line",
            "mr_refreshing",
            "mr_fetch_aborted",
            // Task 10b (E21): Tray-Fußzeile
            "tray_main_window",
            "tray_mrs",
            "tray_mrs_tt_one",
            "tray_mrs_tt_many",
            "tray_mrs_tt_none",
            "tray_quit",
            // Task 11: Auschecken in der App, Branch-Wechsel
            "mr_worktree_created",
            "mr_clone_done",
            "mr_cloning",
            "mr_then_agent",
            "mr_then_shell",
            "mr_then_explorer",
            "mr_then_ide",
            "mr_then_no_ide",
            "mr_no_ide",
            "mr_folder_exists",
            "mr_checkout_busy",
            "mr_checkout_done",
            "mr_checkout_done_ff",
            "mr_checkout_done_ff_one",
            "mr_already_head",
            "mr_already_head_behind",
            "mr_diverged",
            "mr_checkout_blocked",
            "mr_switch_waiting",
            "mr_checkout_gone",
            "mr_checkout_running",
            "mr_checkout_failed",
            "mr_checkout_no_remote",
            "mr_repo_open_failed",
            "mr_remote_branch_missing",
            "mr_branch_name_taken",
            "branch_switched",
            "branch_switch_failed",
            "branch_switch_blocked",
            // Task 12: Einstellungs-Tab „Merge Requests“
            "tabs_merge_requests",
            "mrs_sec_cli",
            "mrs_glab_program",
            "mrs_glab_found",
            "mrs_glab_not_found",
            "mrs_glab_invalid",
            "mrs_recheck",
            "mrs_sec_auth",
            "mrs_state_checking",
            "mrs_state_ready",
            "mrs_state_rights_missing",
            "mrs_state_not_signed",
            "mrs_state_expired",
            "mrs_state_revoked",
            "mrs_state_unreachable",
            "mrs_state_tls",
            "mrs_rights_gitmanager_ok",
            "mrs_glab_full",
            "mrs_glab_readonly",
            "mrs_glab_insufficient",
            "mrs_glab_unknown",
            "mrs_glab_scopes_hint",
            "mrs_signed_in_as",
            "mrs_token_kind",
            "mrs_kind_pat",
            "mrs_kind_oauth",
            "mrs_kind_bot",
            "mrs_kind_granular",
            "mrs_token_source",
            "mrs_src_keyring",
            "mrs_src_config",
            "mrs_src_env",
            "mrs_expires",
            "mrs_expires_soon",
            "mrs_expires_soon_one",
            "mrs_scopes",
            "mrs_check",
            "mrs_add_token",
            "mrs_replace_token",
            "mrs_token_label",
            "mrs_show_token",
            "mrs_purpose",
            "mrs_purpose_read",
            "mrs_purpose_full",
            "mrs_create_token",
            "mrs_protocol",
            "mrs_token_note",
            "mrs_http_warning",
            "mrs_http_chip",
            "mrs_token_empty",
            "mrs_token_whitespace",
            "mrs_token_invalid",
            "mrs_verify",
            "mrs_preview",
            "mrs_preview_no_expiry",
            "mrs_downgrade_warning",
            "mrs_downgrade_confirm",
            "mrs_token_saved",
            "mrs_token_failed",
            "mrs_env_banner",
            "mrs_unrecognized",
            "mrs_unrecognized_hint",
            "mrs_host_repos",
            "mrs_host_repos_one",
            "mrs_belongs_to",
            "mrs_sec_repos",
            "mrs_included",
            "mrs_all",
            "mrs_none",
            "mrs_check_access",
            "mrs_col_repo",
            "mrs_col_clones",
            "mrs_col_project",
            "mrs_col_access",
            "mrs_clones",
            "mrs_acc_ok",
            "mrs_acc_ok_plain",
            "mrs_acc_public",
            "mrs_acc_renamed",
            "mrs_acc_denied",
            "mrs_acc_mr_off",
            "mrs_acc_archived",
            "mrs_acc_host_auth",
            "mrs_acc_checking",
            "mrs_acc_skipped",
            "mrs_include_nonlocal",
            "mrs_sec_checkout",
            "mrs_review_root",
            "mrs_review_root_ok",
            "mrs_review_root_warn",
            "mrs_review_root_relative",
            "mrs_add_search_path",
            "mrs_folder_name",
            "mrs_folder_name_example",
            "mrs_after_checkout",
            "mrs_after_last",
            "mrs_after_last_hint",
            "mrs_sec_display",
            "mrs_refresh",
            "mrs_refresh_unit",
            "mrs_show_all_default",
            "mrs_ignore_bots",
            "mrs_approval_goal",
            "mrs_approval_goal_unit",
            "mrs_check_finished",
            "mrs_belongs_to_tt",
            "mrs_alias_apply",
            "mrs_alias_of",
            "mrs_alias_remove",
            "mrs_extra_redundant",
            "fetch_busy",
            "mrs_glab_pending",
        ];
        // Vollständigkeit: Liste ohne Dubletten, und jeder MR-Schlüssel aus
        // `tr` steht in der Liste (sonst prüft niemand seine Texte). Beide
        // Sprachen scannen: ein Schlüssel nur mit En-Arm fällt sonst durch.
        let listed: std::collections::HashSet<&str> = keys.iter().copied().collect();
        assert_eq!(listed.len(), keys.len(), "doppelter Schlüssel in der Liste");
        for line in include_str!("i18n.rs").lines() {
            let t = line.trim_start();
            let Some(rest) = t
                .strip_prefix("(Language::De, \"")
                .or_else(|| t.strip_prefix("(Language::En, \""))
            else {
                continue;
            };
            let key = rest.split('"').next().unwrap_or_default();
            if ["mr_", "mrs_", "view_", "rel_", "tray_mrs"]
                .iter()
                .any(|prefix| key.starts_with(prefix))
            {
                assert!(listed.contains(key), "MR key not listed: {key}");
            }
        }
        for key in keys {
            let en = tr(Language::En, key);
            let de = tr(Language::De, key);
            assert_ne!(en, key, "missing EN key: {key}");
            assert_ne!(de, key, "missing DE key: {key}");
            // tr_fmt füllt `{}` der Reihe nach: beide Sprachen gleich viele.
            assert_eq!(
                en.matches("{}").count(),
                de.matches("{}").count(),
                "placeholder count differs for {key}: EN {en} / DE {de}"
            );
            // En nur ASCII plus Typografie (keine Umlaute, kein „).
            assert!(
                en.chars().all(|c| c.is_ascii() || "–…“”·→↗’".contains(c)),
                "EN {key} has non-English characters: {en}"
            );
            // Anführungszeichen paarweise: De „…“, En “…”.
            assert_eq!(
                de.matches('„').count(),
                de.matches('“').count(),
                "DE {key} unbalanced quotes: {de}"
            );
            assert_eq!(
                en.matches('“').count(),
                en.matches('”').count(),
                "EN {key} unbalanced quotes: {en}"
            );
            // En enthält kein Deutsch.
            for german in [
                "ä",
                "ö",
                "ü",
                "ß",
                "„",
                "Lokaler",
                "ausgecheckt",
                "verfolgt",
                "voraus",
                "hinterher",
                "vorgespult",
                "Approval",
                "Diskussions",
                "gelöst",
                "offen",
                "ausstehend",
                "begonnen",
                "Änderungen",
                "angefragt",
                "Kommentiert",
                "grün",
                "läuft",
                "wartet",
                "Keine",
                "gerade",
                "gestern",
                "vor ",
                "Std.",
                "Tagen",
                "(du)",
                "nicht",
                "Zertifikat",
                "Zeitüberschreitung",
                "braucht",
                "Erwähnungen",
                "Weitere",
                "Anfrage",
                "Entwurf",
                "mergebereit",
                "Erwähnt",
                "Beschreibung",
                "Kommentar",
                "Klon",
                "Ordner",
                "Nichts",
                "angemeldet",
                "abgewählt",
                "Einstellungen",
                "Befehl",
                "zuletzt",
                "Arbeit",
                "Sortiert",
                "Meine",
                "anderen",
                "weitere",
                "Lade ",
                "Projekt",
                "unbekannt",
                "Eigene",
                // Task 10–12: Tray, Auschecken, Einstellungs-Tab
                "Hauptfenster",
                "Beenden",
                "angelegt",
                "geklont",
                "Bereits",
                "gewechselt",
                "Auschecken",
                "fehlgeschlagen",
                "Bereit",
                "Rechte",
                "Abgelaufen",
                "Widerrufen",
                "erreichbar",
                "Anmeldung",
                "Speichern",
                "gespeichert",
                "Zugriff",
                "einbezogen",
                "zugreifbar",
                "Anzeige",
                "Aktualis",
                "Umgebungsvariable",
                "Zuordn",
                "Zweck",
                "Leerzeichen",
                "Trotzdem",
                " und ",
                " mit ",
                " von ",
                " der ",
                " die ",
                " das ",
                " ist ",
            ] {
                assert!(
                    !en.contains(german),
                    "EN {key} must not contain German '{german}': {en}"
                );
            }
            // Deutsche Texte ohne ASCII-Anführungszeichen.
            assert!(!de.contains('"'), "DE {key} uses ASCII quotes: {de}");
        }
        assert_eq!(
            tr_fmt(Language::De, "branch_local_diverged", &["feature/x", "2", "3"]),
            "Lokaler Branch feature/x weicht ab (2 voraus, 3 hinterher) – lokaler Stand ausgecheckt"
        );
        assert_eq!(
            tr_fmt(
                Language::En,
                "branch_checked_out_elsewhere",
                &["feature/x", "C:\\wt"]
            ),
            "Branch feature/x is checked out in C:\\wt"
        );
        assert_eq!(
            tr_fmt(Language::De, "branch_fast_forwarded_one", &["main"]),
            "Lokaler Branch main um 1 Commit vorgespult"
        );
        assert_eq!(
            tr_fmt(Language::De, "mr_appr_of", &["1", "2"]),
            "1 von 2 Approvals"
        );
        assert_eq!(
            tr_fmt(Language::En, "mr_threads_tt", &["2", "3"]),
            "2 open · 3 resolved"
        );
        assert_eq!(
            tr_fmt(Language::De, "mr_rev_started", &["Miriam Becker"]),
            "Miriam Becker hat das Review begonnen"
        );
        assert_eq!(tr_fmt(Language::De, "rel_hour", &["3"]), "vor 3 Std.");
        assert_eq!(tr_fmt(Language::En, "rel_days", &["4"]), "4 days ago");
        assert_eq!(
            tr_fmt(Language::De, "mr_err_timeout", &["gitlab.local", "45"]),
            "Zeitüberschreitung bei gitlab.local nach 45 s."
        );
        assert_eq!(
            tr_fmt(Language::En, "mr_err_network", &["gitlab.local"]),
            "gitlab.local is not reachable."
        );
        assert_eq!(
            tr_fmt(
                Language::De,
                "mr_partial",
                &["Erwähnungen", "Das Token braucht read_api."]
            ),
            "Erwähnungen konnten nicht geladen werden (Das Token braucht read_api.)."
        );
        assert_eq!(
            tr_fmt(
                Language::De,
                "mr_menu_worktree_sub",
                &["C:\\R\\a-mr-1", "a"]
            ),
            "C:\\R\\a-mr-1 · teilt Git-Daten mit a"
        );
        assert_eq!(tr_fmt(Language::En, "mr_load_more", &["8"]), "Load 8 more");
        assert_eq!(
            tr_fmt(
                Language::De,
                "mr_status_line",
                &["gitlab.example.com", "vor 1 Min."]
            ),
            "Merge Requests · gitlab.example.com · Stand: vor 1 Min."
        );
        assert_eq!(
            tr_fmt(Language::En, "mr_badge_tt_many", &["2"]),
            "Merge requests – 2 reviews waiting for you"
        );
        assert_eq!(
            tr_fmt(
                Language::De,
                "mr_checkout_done_ff",
                &["412", "feature/x", "C:\\Dev\\api", "3"]
            ),
            "!412 ausgecheckt: feature/x in C:\\Dev\\api (+3 Commits)"
        );
        assert_eq!(
            tr_fmt(
                Language::De,
                "mr_worktree_created",
                &["C:\\R\\api-mr-5", "5"]
            ),
            "Worktree C:\\R\\api-mr-5 für !5 angelegt"
        );
        assert_eq!(
            tr_fmt(Language::En, "mr_then_agent", &["Claude Code"]),
            "– starting Claude Code"
        );
        assert_eq!(
            tr_fmt(Language::De, "branch_switched", &["main", "C:\\Dev\\api"]),
            "Branch zu „main“ gewechselt: C:\\Dev\\api"
        );
        assert_eq!(
            tr_fmt(Language::De, "branch_switch_blocked", &["main", "Merge"]),
            "Branch-Wechsel zu „main“ nicht möglich: Merge-Konflikte oder Merge/Rebase läuft (Merge)"
        );
        assert_eq!(
            tr_fmt(
                Language::De,
                "mrs_preview",
                &["taltmann", "read_api", "31.03.2027", "4", "5"]
            ),
            "@taltmann · read_api · läuft ab 31.03.2027 · 4/5 Repos zugreifbar"
        );
        assert_eq!(
            tr_fmt(Language::En, "mrs_included", &["4", "5", "3"]),
            "4 of 5 included · 3 accessible"
        );
        assert_eq!(
            tr_fmt(
                Language::De,
                "mrs_glab_found",
                &["1.123.0", "C:\\glab\\glab.exe"]
            ),
            "glab 1.123.0 gefunden · C:\\glab\\glab.exe"
        );
        assert_eq!(
            tr_fmt(Language::De, "mrs_token_saved", &["gitlab.example.com"]),
            "Token für gitlab.example.com gespeichert"
        );
        assert_eq!(
            tr_fmt(Language::De, "mrs_alias_of", &["ssh.intern.example"]),
            "ssh.intern.example gehört zu diesem Host"
        );
        assert_eq!(
            tr_fmt(Language::En, "mrs_glab_pending", &["glab"]),
            "Applies after “Save” – until then GitManager uses glab."
        );
    }

    #[test]
    fn serde_roundtrip() {
        for lang in [Language::En, Language::De] {
            let json = serde_json::to_string(&lang).unwrap();
            let de: Language = serde_json::from_str(&json).unwrap();
            assert_eq!(lang, de);
        }
        assert_eq!(serde_json::to_string(&Language::En).unwrap(), "\"en\"");
        assert_eq!(serde_json::to_string(&Language::De).unwrap(), "\"de\"");
    }
}
