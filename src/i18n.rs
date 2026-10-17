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
        (Language::En, "app_title") => "GitManager".to_string(),
        (Language::De, "app_title") => "GitManager".to_string(),
        (Language::En, "repositories") => "Repositories".to_string(),
        (Language::De, "repositories") => "Repositories".to_string(),
        (Language::En, "profile_label") => "Profile:".to_string(),
        (Language::De, "profile_label") => "Profil:".to_string(),
        (Language::En, "ai_label") => "AI:".to_string(),
        (Language::De, "ai_label") => "AI:".to_string(),
        (Language::En, "settings") => "Settings".to_string(),
        (Language::De, "settings") => "Einstellungen".to_string(),
        (Language::En, "settings_tooltip") => "Configure paths, profiles, agents & depth".to_string(),
        (Language::De, "settings_tooltip") => "Pfade, Profile, Agents & Tiefe konfigurieren".to_string(),
        (Language::En, "refresh") => "Refresh".to_string(),
        (Language::De, "refresh") => "Aktualisieren".to_string(),
        (Language::En, "scanning") => "Scanning...".to_string(),
        (Language::De, "scanning") => "Scanne...".to_string(),
        (Language::En, "search_paths") => "Search paths:".to_string(),
        (Language::De, "search_paths") => "Suchpfade:".to_string(),
        (Language::En, "depth_label") => "Depth:".to_string(),
        (Language::De, "depth_label") => "Tiefe:".to_string(),
        (Language::En, "not_found") => "(not found)".to_string(),
        (Language::De, "not_found") => "(nicht gefunden)".to_string(),
        (Language::En, "language") => "Language".to_string(),
        (Language::De, "language") => "Sprache".to_string(),

        // ── Status Bar ─────────────────────────────────────────────────
        (Language::En, "status_repos_paths") => "Repos".to_string(),
        (Language::De, "status_repos_paths") => "Repos".to_string(),
        (Language::En, "status_paths") => "paths".to_string(),
        (Language::De, "status_paths") => "Pfade".to_string(),
        (Language::En, "scanning_repos") => "Scanning for repositories...".to_string(),
        (Language::De, "scanning_repos") => "Scanne nach Repositories...".to_string(),
        (Language::En, "no_search_path") => "No search path configured. Open settings and add a folder.".to_string(),
        (Language::De, "no_search_path") => "Kein Suchpfad konfiguriert. Öffne die Einstellungen und füge einen Ordner hinzu.".to_string(),

        // ── Branch dialog ──────────────────────────────────────────────
        (Language::En, "branch_switch_title") => "Switch branch".to_string(),
        (Language::De, "branch_switch_title") => "Branch wechseln".to_string(),
        (Language::En, "branch_dirty_msg") => "Uncommitted changes present. How to proceed?".to_string(),
        (Language::De, "branch_dirty_msg") => "Uncommitted changes vorhanden. Wie fortfahren?".to_string(),
        (Language::En, "affected_files") => "affected files:".to_string(),
        (Language::De, "affected_files") => "betroffene Dateien:".to_string(),
        (Language::En, "cancel") => "Cancel".to_string(),
        (Language::De, "cancel") => "Abbrechen".to_string(),
        (Language::En, "stash_switch") => "Stash & Switch".to_string(),
        (Language::De, "stash_switch") => "Stash & Switch".to_string(),
        (Language::En, "stash_switch_tooltip") => "Stash changes, switch branch, then pop".to_string(),
        (Language::De, "stash_switch_tooltip") => "Änderungen stashen, Branch wechseln, stash pop".to_string(),
        (Language::En, "force_discard") => "Force Discard".to_string(),
        (Language::De, "force_discard") => "Force Discard".to_string(),
        (Language::En, "force_discard_tooltip") => "Discard local changes and switch (dangerous)".to_string(),
        (Language::De, "force_discard_tooltip") => "Lokale Änderungen verwerfen und wechseln (gefährlich)".to_string(),
        (Language::En, "branch_tip") => "Tip: 'Stash & Switch' keeps changes, 'Force' discards them.".to_string(),
        (Language::De, "branch_tip") => "Tipp: 'Stash & Switch' behält Änderungen, 'Force' verwirft sie.".to_string(),
        (Language::En, "branch_conflict_error") => "Branch switch not possible".to_string(),
        (Language::De, "branch_conflict_error") => "Branch-Wechsel nicht möglich".to_string(),

        // ── Branch-Wechsel auf Remote-Branch (E15) ─────────────────────
        (Language::En, "branch_local_ahead") => "Local branch {} is {} commits ahead – local state checked out".to_string(),
        (Language::De, "branch_local_ahead") => "Lokaler Branch {} ist {} Commits voraus – lokaler Stand ausgecheckt".to_string(),
        (Language::En, "branch_local_ahead_one") => "Local branch {} is 1 commit ahead – local state checked out".to_string(),
        (Language::De, "branch_local_ahead_one") => "Lokaler Branch {} ist 1 Commit voraus – lokaler Stand ausgecheckt".to_string(),
        (Language::En, "branch_local_diverged") => "Local branch {} differs ({} ahead, {} behind) – local state checked out".to_string(),
        (Language::De, "branch_local_diverged") => "Lokaler Branch {} weicht ab ({} voraus, {} hinterher) – lokaler Stand ausgecheckt".to_string(),
        (Language::En, "branch_fast_forwarded") => "Fast-forwarded local branch {} by {} commits".to_string(),
        (Language::De, "branch_fast_forwarded") => "Lokaler Branch {} um {} Commits vorgespult".to_string(),
        (Language::En, "branch_fast_forwarded_one") => "Fast-forwarded local branch {} by 1 commit".to_string(),
        (Language::De, "branch_fast_forwarded_one") => "Lokaler Branch {} um 1 Commit vorgespult".to_string(),
        (Language::En, "branch_upstream_mismatch") => "Local branch {} tracks {}".to_string(),
        (Language::De, "branch_upstream_mismatch") => "Lokaler Branch {} verfolgt {}".to_string(),
        (Language::En, "branch_checked_out_elsewhere") => "Branch {} is checked out in {}".to_string(),
        (Language::De, "branch_checked_out_elsewhere") => "Branch {} ist in {} ausgecheckt".to_string(),
        (Language::En, "branch_switched") => "Switched to branch “{}”: {}".to_string(),
        (Language::De, "branch_switched") => "Branch zu „{}“ gewechselt: {}".to_string(),
        (Language::En, "branch_switch_failed") => "Branch switch failed: {}".to_string(),
        (Language::De, "branch_switch_failed") => "Branch-Wechsel fehlgeschlagen: {}".to_string(),
        (Language::En, "branch_switch_blocked") => "Cannot switch to branch “{}”: merge conflicts or a merge/rebase in progress ({})".to_string(),
        (Language::De, "branch_switch_blocked") => "Branch-Wechsel zu „{}“ nicht möglich: Merge-Konflikte oder Merge/Rebase läuft ({})".to_string(),

        // ── Repo list ──────────────────────────────────────────────────
        (Language::En, "no_repos_found") => "No repositories found".to_string(),
        (Language::De, "no_repos_found") => "Keine Repositories gefunden".to_string(),
        (Language::En, "no_repos_hint") => "Check the path in settings and increase search depth if needed.".to_string(),
        (Language::De, "no_repos_hint") => "Pfad in den Einstellungen prüfen und ggf. Suchtiefe erhöhen.".to_string(),
        (Language::En, "dirty_tooltip") => "Uncommitted changes present".to_string(),
        (Language::De, "dirty_tooltip") => "Uncommitted changes vorhanden".to_string(),
        (Language::En, "clean_tooltip") => "Working directory clean".to_string(),
        (Language::De, "clean_tooltip") => "Arbeitsverzeichnis sauber".to_string(),
        (Language::En, "search_label") => "Search:".to_string(),
        (Language::De, "search_label") => "Suche:".to_string(),
        (Language::En, "filter_hint") => "filter...".to_string(),
        (Language::De, "filter_hint") => "filtern...".to_string(),
        (Language::En, "branch_search") => "Search branches...".to_string(),
        (Language::De, "branch_search") => "Branches suchen...".to_string(),
        (Language::En, "branch_refresh_tooltip") => "Reload branches (git fetch --all)".to_string(),
        (Language::De, "branch_refresh_tooltip") => "Branches neu laden (git fetch --all)".to_string(),
        (Language::En, "current") => "Current:".to_string(),
        (Language::De, "current") => "Aktuell:".to_string(),
        (Language::En, "no_branches") => "No branches found – try fetch".to_string(),
        (Language::De, "no_branches") => "Keine Branches gefunden – evtl. fetch nötig".to_string(),
        (Language::En, "fetch_now") => "Fetch now".to_string(),
        (Language::De, "fetch_now") => "Jetzt fetchen".to_string(),
        (Language::En, "no_matches") => "No matches".to_string(),
        (Language::De, "no_matches") => "Keine Treffer".to_string(),
        (Language::En, "branch_switch_tooltip") => "Switch branch – type to filter, ↻ to fetch".to_string(),
        (Language::De, "branch_switch_tooltip") => "Branch wechseln – tippe zum Filtern, ↻ für fetch".to_string(),
        (Language::En, "detached_tooltip") => "Detached HEAD – commit checked out, not a branch".to_string(),
        (Language::De, "detached_tooltip") => "Detached HEAD – Commit ausgecheckt, kein Branch".to_string(),
        (Language::En, "profile_override_tooltip") => "Override language profile for this repo".to_string(),
        (Language::De, "profile_override_tooltip") => "Sprach-Profil für dieses Repo überschreiben".to_string(),
        (Language::En, "global") => "— Global —".to_string(),
        (Language::De, "global") => "— Global —".to_string(),
        (Language::En, "open_in_explorer") => "Show in Explorer".to_string(),
        (Language::De, "open_in_explorer") => "Im Explorer öffnen".to_string(),
        (Language::En, "open_in_shell") => "Open shell".to_string(),
        (Language::De, "open_in_shell") => "Shell öffnen".to_string(),
        (Language::En, "open_in") => "Open in".to_string(),
        (Language::De, "open_in") => "Mit".to_string(),
        (Language::En, "open_in_terminal") => "Open in {} (Terminal)".to_string(),
        (Language::De, "open_in_terminal") => "In {} öffnen (Terminal)".to_string(),
        (Language::En, "no_solution") => "No {}".to_string(), // formatted with extension
        (Language::De, "no_solution") => "Keine {}".to_string(),
        (Language::En, "selected") => "Selected: {}".to_string(),
        (Language::De, "selected") => "Ausgewählt: {}".to_string(),

        // ── Settings General ───────────────────────────────────────────
        (Language::En, "settings_general_desc") => "Choose folders to scan for Git repositories. All direct subfolders up to the selected depth are scanned.".to_string(),
        (Language::De, "settings_general_desc") => "Wähle die Ordner, die nach Git-Repositories durchsucht werden sollen. Es werden alle direkten Unterordner bis zur eingestellten Tiefe geprüft.".to_string(),
        (Language::En, "search_paths_title") => "Search paths".to_string(),
        (Language::De, "search_paths_title") => "Suchpfade".to_string(),
        (Language::En, "add") => "Add".to_string(),
        (Language::De, "add") => "Hinzufügen".to_string(),
        (Language::En, "no_paths_configured") => "No paths configured. Add a folder.".to_string(),
        (Language::De, "no_paths_configured") => "Keine Pfade konfiguriert. Füge einen Ordner hinzu.".to_string(),
        (Language::En, "remove") => "Remove".to_string(),
        (Language::De, "remove") => "Entfernen".to_string(),
        (Language::En, "browse") => "Browse".to_string(),
        (Language::De, "browse") => "Durchsuchen".to_string(),
        (Language::En, "path_exists_error") => "Path does not exist:".to_string(),
        (Language::De, "path_exists_error") => "Pfad existiert nicht:".to_string(),
        (Language::En, "search_depth") => "Search depth".to_string(),
        (Language::De, "search_depth") => "Suchtiefe".to_string(),
        (Language::En, "search_depth_desc") => "How many levels deep to search (1 = direct children, 2 = children + grandchildren, ...)".to_string(),
        (Language::De, "search_depth_desc") => "Wie viele Ebenen tief gesucht wird (1 = nur direkte Kinder, 2 = Kinder + Enkel, ...)".to_string(),
        (Language::En, "depth") => "Depth:".to_string(),
        (Language::De, "depth") => "Tiefe:".to_string(),
        (Language::En, "levels") => "levels".to_string(),
        (Language::De, "levels") => "Ebenen".to_string(),
        (Language::En, "or_direct") => "Or direct:".to_string(),
        (Language::De, "or_direct") => "Oder direkt:".to_string(),
        (Language::En, "current_depth") => "(current: {})".to_string(),
        (Language::De, "current_depth") => "(aktuell: {})".to_string(),
        (Language::En, "active_profile_global") => "Active language profile (global)".to_string(),
        (Language::De, "active_profile_global") => "Aktives Sprach-Profil (global)".to_string(),
        (Language::En, "per_repo_override_hint") => "Per-repo override can be chosen in the overview.".to_string(),
        (Language::De, "per_repo_override_hint") => "Pro Repo kann in der Übersicht ein Override gewählt werden.".to_string(),

        // ── Settings Profiles ──────────────────────────────────────────
        (Language::En, "manage_profiles") => "Manage language profiles".to_string(),
        (Language::De, "manage_profiles") => "Sprach-Profile verwalten".to_string(),
        (Language::En, "profiles_desc") => "Each profile defines a file extension (e.g. .sln) and the IDEs that can open it.".to_string(),
        (Language::De, "profiles_desc") => "Jedes Profil definiert eine Dateiendung (z.B. .sln) und die IDEs, mit denen diese geöffnet werden kann.".to_string(),
        (Language::En, "profiles_count") => "Profiles ({}):".to_string(),
        (Language::De, "profiles_count") => "Profile ({}):".to_string(),
        (Language::En, "new_profile") => "New profile".to_string(),
        (Language::De, "new_profile") => "Neues Profil".to_string(),
        (Language::En, "duplicate") => "Duplicate".to_string(),
        (Language::De, "duplicate") => "Duplizieren".to_string(),
        (Language::En, "delete") => "Delete".to_string(),
        (Language::De, "delete") => "Löschen".to_string(),
        (Language::En, "set_active") => "Set active".to_string(),
        (Language::De, "set_active") => "Aktiv setzen".to_string(),
        (Language::En, "quick_create_profile") => "Quick create new profile".to_string(),
        (Language::De, "quick_create_profile") => "Schnell-Anlage neues Profil".to_string(),
        (Language::En, "name") => "Name:".to_string(),
        (Language::De, "name") => "Name:".to_string(),
        (Language::En, "extension") => "Extension:".to_string(),
        (Language::De, "extension") => "Endung:".to_string(),
        (Language::En, "create") => "Create".to_string(),
        (Language::De, "create") => "Anlegen".to_string(),
        (Language::En, "edit_profile") => "Edit profile:".to_string(),
        (Language::De, "edit_profile") => "Profil bearbeiten:".to_string(),
        (Language::En, "id") => "ID:".to_string(),
        (Language::De, "id") => "ID:".to_string(),
        (Language::En, "display_name") => "Display name:".to_string(),
        (Language::De, "display_name") => "Anzeigename:".to_string(),
        (Language::En, "file_extension") => "File extension:".to_string(),
        (Language::De, "file_extension") => "Dateiendung:".to_string(),
        (Language::En, "pattern_optional") => "Pattern (optional):".to_string(),
        (Language::De, "pattern_optional") => "Muster (optional):".to_string(),
        (Language::En, "scan_depth") => "Scan depth:".to_string(),
        (Language::De, "scan_depth") => "Scan-Tiefe:".to_string(),
        (Language::En, "ides_for_profile") => "IDEs for this profile:".to_string(),
        (Language::De, "ides_for_profile") => "IDEs für dieses Profil:".to_string(),
        (Language::En, "default") => "Default".to_string(),
        (Language::De, "default") => "Default".to_string(),
        (Language::En, "set_as_default") => "Set as default".to_string(),
        (Language::De, "set_as_default") => "Als Default".to_string(),
        (Language::En, "program") => "Program:".to_string(),
        (Language::De, "program") => "Programm:".to_string(),
        (Language::En, "args") => "Args:".to_string(),
        (Language::De, "args") => "Args:".to_string(),
        (Language::En, "shell") => "Shell:".to_string(),
        (Language::De, "shell") => "Shell:".to_string(),
        (Language::En, "via_cmd") => "via cmd /C (unsafe)".to_string(),
        (Language::De, "via_cmd") => "via cmd /C (unsicher)".to_string(),
        (Language::En, "preview") => "Preview:".to_string(),
        (Language::De, "preview") => "Vorschau:".to_string(),
        (Language::En, "add_ide") => "Add IDE".to_string(),
        (Language::De, "add_ide") => "IDE hinzufügen".to_string(),
        (Language::En, "default_ide") => "Default IDE:".to_string(),
        (Language::De, "default_ide") => "Default IDE:".to_string(),
        (Language::En, "none") => "None".to_string(),
        (Language::De, "none") => "Keine".to_string(),
        (Language::En, "active") => "Active".to_string(),
        (Language::De, "active") => "Aktiv".to_string(),

        // ── Settings Agents ────────────────────────────────────────────
        (Language::En, "manage_agents") => "Manage AI agents".to_string(),
        (Language::De, "manage_agents") => "AI Agents verwalten".to_string(),
        (Language::En, "agents_desc") => "Agents open a terminal in the repo directory and run the configured command. Claude is preconfigured.".to_string(),
        (Language::De, "agents_desc") => "Agenten starten ein Terminal im Repo-Verzeichnis und führen den konfigurierten Befehl aus. Claude ist vordefiniert.".to_string(),
        (Language::En, "agents_count") => "Agents ({}):".to_string(),
        (Language::De, "agents_count") => "Agents ({}):".to_string(),
        (Language::En, "new_agent") => "New agent".to_string(),
        (Language::De, "new_agent") => "Neuer Agent".to_string(),
        (Language::En, "quick_create_agent") => "Quick create new agent".to_string(),
        (Language::De, "quick_create_agent") => "Schnell-Anlage neuer Agent".to_string(),
        (Language::En, "program_label") => "Program:".to_string(),
        (Language::De, "program_label") => "Programm:".to_string(),
        (Language::En, "edit_agent") => "Edit agent:".to_string(),
        (Language::De, "edit_agent") => "Agent bearbeiten:".to_string(),
        (Language::En, "args_space_separated") => "Args (space-separated):".to_string(),
        (Language::De, "args_space_separated") => "Args (Leerzeichen-getrennt):".to_string(),
        (Language::En, "terminal_override") => "Terminal override:".to_string(),
        (Language::De, "terminal_override") => "Terminal-Override:".to_string(),
        (Language::En, "auto_global") => "— Auto (global) —".to_string(),
        (Language::De, "auto_global") => "— Auto (global) —".to_string(),
        (Language::En, "active_agents") => "Active agents (multiple possible — all icons appear side-by-side):".to_string(),
        (Language::De, "active_agents") => "Aktive Agents (mehrere möglich — alle Icons erscheinen nebeneinander):".to_string(),
        (Language::En, "enable_all") => "Enable all".to_string(),
        (Language::De, "enable_all") => "Alle aktivieren".to_string(),
        (Language::En, "disable_all") => "Disable all".to_string(),
        (Language::De, "disable_all") => "Alle deaktivieren".to_string(),

        // ── Settings Terminal ──────────────────────────────────────────
        (Language::En, "terminal_settings") => "Terminal settings".to_string(),
        (Language::De, "terminal_settings") => "Terminal-Einstellungen".to_string(),
        (Language::En, "terminal_desc") => "Choose the preferred terminal for launching AI agents. 'Auto' tries Windows Terminal → Powershell → cmd.".to_string(),
        (Language::De, "terminal_desc") => "Wähle das bevorzugte Terminal zum Starten von AI-Agents. 'Auto' probiert Windows Terminal → Powershell → cmd.".to_string(),
        (Language::En, "preferred_terminal") => "Preferred terminal:".to_string(),
        (Language::De, "preferred_terminal") => "Bevorzugtes Terminal:".to_string(),
        (Language::En, "custom_terminal_optional") => "Custom terminal (optional):".to_string(),
        (Language::De, "custom_terminal_optional") => "Custom Terminal (optional):".to_string(),
        (Language::En, "only_custom_relevant") => "Only relevant for 'Custom'".to_string(),
        (Language::De, "only_custom_relevant") => "Nur bei 'Custom' relevant".to_string(),
        (Language::En, "switch_to_custom") => "Switch to Custom".to_string(),
        (Language::De, "switch_to_custom") => "Auf Custom wechseln".to_string(),
        (Language::En, "fallback_terminal") => "Fallback terminal:".to_string(),
        (Language::De, "fallback_terminal") => "Fallback Terminal:".to_string(),

        // ── Settings Appearance ────────────────────────────────────────
        (Language::En, "appearance") => "Appearance".to_string(),
        (Language::De, "appearance") => "Erscheinungsbild".to_string(),
        (Language::En, "appearance_desc") => "Choose one of 5 themes — applies immediately after saving.".to_string(),
        (Language::De, "appearance_desc") => "Wähle eines von 5 Themes — wirkt sofort nach Speichern.".to_string(),
        (Language::En, "theme") => "Theme:".to_string(),
        (Language::De, "theme") => "Theme:".to_string(),
        (Language::En, "theme_hint") => "Theme is applied automatically on next launch and immediately after saving.".to_string(),
        (Language::De, "theme_hint") => "Theme wird beim nächsten Start automatisch angewendet und nach Speichern sofort.".to_string(),

        // ── Settings common ────────────────────────────────────────────
        (Language::En, "close") => "Close".to_string(),
        (Language::De, "close") => "Schließen".to_string(),
        (Language::En, "save") => "Save".to_string(),
        (Language::De, "save") => "Speichern".to_string(),
        (Language::En, "reset") => "Reset".to_string(),
        (Language::De, "reset") => "Zurücksetzen".to_string(),
        (Language::En, "config_path") => "Config:".to_string(),
        (Language::De, "config_path") => "Config:".to_string(),
        (Language::En, "tabs_general") => "General".to_string(),
        (Language::De, "tabs_general") => "Allgemein".to_string(),
        (Language::En, "tabs_profiles") => "Languages/Profiles".to_string(),
        (Language::De, "tabs_profiles") => "Sprachen/Profile".to_string(),
        (Language::En, "tabs_agents") => "AI Agents".to_string(),
        (Language::De, "tabs_agents") => "AI Agents".to_string(),
        (Language::En, "tabs_terminal") => "Terminal".to_string(),
        (Language::De, "tabs_terminal") => "Terminal".to_string(),
        (Language::En, "tabs_appearance") => "Appearance".to_string(),
        (Language::De, "tabs_appearance") => "Erscheinungsbild".to_string(),
        (Language::En, "tabs_language") => "Language".to_string(),
        (Language::De, "tabs_language") => "Sprache".to_string(),
        (Language::En, "tabs_icons") => "Icons".to_string(),
        (Language::De, "tabs_icons") => "Icons".to_string(),
        (Language::En, "tabs_tray_icons") => "Tray Icons".to_string(),
        (Language::De, "tabs_tray_icons") => "Tray Icons".to_string(),
        (Language::En, "tray_icons_title") => "Tray Icons & Popup".to_string(),
        (Language::De, "tray_icons_title") => "Tray Icons & Popup".to_string(),
        (Language::En, "tray_behavior_title") => "System Tray Behavior".to_string(),
        (Language::De, "tray_behavior_title") => "System Tray Verhalten".to_string(),
        (Language::En, "tray_minimize_label") => "Minimize to tray on close (instead of quit)".to_string(),
        (Language::De, "tray_minimize_label") => "Beim Schließen in Tray minimieren (statt beenden)".to_string(),
        (Language::En, "tray_minimize_hint_on") => "✓ Window is hidden on close and keeps running in tray (left/right click: custom menu).".to_string(),
        (Language::De, "tray_minimize_hint_on") => "✓ Das Fenster wird beim Schließen ausgeblendet und läuft im Tray weiter (Links-/Rechts-Klick: eigenes Menü).".to_string(),
        (Language::En, "tray_minimize_hint_off") => "Window quits on close.".to_string(),
        (Language::De, "tray_minimize_hint_off") => "Das Fenster wird beim Schließen beendet.".to_string(),
        (Language::En, "tray_update_label") => "Check for updates on startup (GitHub Releases)".to_string(),
        (Language::De, "tray_update_label") => "Beim Start auf Updates prüfen (GitHub Releases)".to_string(),
        (Language::En, "tray_update_hint_on") => "✓ api.github.com is queried once on startup (5 s timeout). Disable for offline/corporate networks. Takes effect after restart.".to_string(),
        (Language::De, "tray_update_hint_on") => "✓ Beim Start wird einmalig api.github.com abgefragt (5 s Timeout). Deaktivieren für Offline-/Firmennetze. Änderung wirkt ab Neustart.".to_string(),
        (Language::En, "tray_update_hint_off") => "Update check disabled — no network request on startup. Takes effect after restart.".to_string(),
        (Language::De, "tray_update_hint_off") => "Update-Check deaktiviert — keine Netzwerkabfrage beim Start. Änderung wirkt ab Neustart.".to_string(),
        (Language::En, "autostart_label") => "Start automatically with Windows".to_string(),
        (Language::De, "autostart_label") => "Automatisch mit Windows starten".to_string(),
        (Language::En, "autostart_hint") => "Adds/removes GitManager in Windows Autostart (Registry Run key). Takes effect on next login.".to_string(),
        (Language::De, "autostart_hint") => "Trägt GitManager in den Windows-Autostart ein (Registry Run-Key). Wirkt ab der nächsten Anmeldung.".to_string(),
        (Language::En, "tray_update_error") => "Last update check failed: {}".to_string(),
        (Language::De, "tray_update_error") => "Letzter Update-Check fehlgeschlagen: {}".to_string(),
        (Language::En, "tray_branch_limit_title") => "Tray Branch Limit".to_string(),
        (Language::De, "tray_branch_limit_title") => "Tray Branch-Limit".to_string(),
        (Language::En, "tray_branch_limit_desc") => "Max branches shown in tray popup dropdown (5–50, default 20).".to_string(),
        (Language::De, "tray_branch_limit_desc") => "Wie viele Branches maximal im Tray-Popup Dropdown angezeigt werden (5–50, Standard 20).".to_string(),
        (Language::En, "tray_branch_limit_summary") => "Shows up to {} branches in the tray popup. More in the main window.".to_string(),
        (Language::De, "tray_branch_limit_summary") => "Zeigt bis zu {} Branches im Tray-Popup. Weitere im Hauptfenster.".to_string(),
        (Language::En, "tray_creation_error") => "Tray icon creation failed – tray popup disabled.".to_string(),
        (Language::De, "tray_creation_error") => "Tray-Icon konnte nicht erstellt werden – Tray-Popup deaktiviert.".to_string(),
        (Language::En, "tray_all_hidden_warn") => "Warning: all tray icons hidden – no icons will be shown in the tray row.".to_string(),
        (Language::De, "tray_all_hidden_warn") => "Warnung: alle Tray-Icons ausgeblendet – es werden keine Icons in der Tray-Zeile angezeigt.".to_string(),
        (Language::En, "tray_limit_label") => "Limit:".to_string(),
        (Language::De, "tray_limit_label") => "Limit:".to_string(),
        (Language::En, "tray_direct_label") => "Or direct:".to_string(),
        (Language::De, "tray_direct_label") => "Oder direkt:".to_string(),
        (Language::En, "tray_branch_more_filtered") => "+ {} more (filter)".to_string(),
        (Language::De, "tray_branch_more_filtered") => "+ {} weitere (filtere)".to_string(),
        (Language::En, "tray_branch_more_main") => "+ {} more (in main window)".to_string(),
        (Language::De, "tray_branch_more_main") => "+ {} weitere (in Hauptfenster)".to_string(),
        (Language::En, "tray_notice_branch_dialog") => "Branch switch for '{}' waits for a decision – click to open".to_string(),
        (Language::De, "tray_notice_branch_dialog") => "Branch-Wechsel für '{}' wartet auf Entscheidung – klicken zum Öffnen".to_string(),
        (Language::En, "tray_notice_error") => "Note in main window – click to open".to_string(),
        (Language::De, "tray_notice_error") => "Hinweis im Hauptfenster – klicken zum Öffnen".to_string(),
        (Language::En, "update_available_title") => "Update available".to_string(),
        (Language::De, "update_available_title") => "Update verfügbar".to_string(),
        (Language::En, "update_available_msg") => "New version available".to_string(),
        (Language::De, "update_available_msg") => "Neue Version verfügbar".to_string(),
        (Language::En, "open_link") => "Open".to_string(),
        (Language::De, "open_link") => "Öffnen".to_string(),
        (Language::En, "tray_icons_desc") => "Reorder, hide and limit tray popup icons. Hidden icons disappear from tray rows. Max display limits visible repos.".to_string(),
        (Language::De, "tray_icons_desc") => "Tray-Popup Icons neu anordnen, ausblenden und limitieren. Ausgeblendete Icons verschwinden aus den Tray-Zeilen. Max Anzeige limitiert sichtbare Repos.".to_string(),
        (Language::En, "tray_max_display") => "Max repos in tray popup".to_string(),
        (Language::De, "tray_max_display") => "Max Repos im Tray-Popup".to_string(),
        (Language::En, "icons_title") => "Icon order & visibility (per profile)".to_string(),
        (Language::De, "icons_title") => "Icon-Reihenfolge & Sichtbarkeit (pro Profil)".to_string(),
        (Language::En, "icons_desc") => "Reorder and hide IDE / terminal / AI icons. Hidden icons disappear from repo list without gaps.".to_string(),
        (Language::De, "icons_desc") => "Icons für IDE / Terminal / AI neu anordnen und ausblenden. Ausgeblendete Icons verschwinden ohne Lücke.".to_string(),
        (Language::En, "icons_profile") => "Profile:".to_string(),
        (Language::De, "icons_profile") => "Profil:".to_string(),
        (Language::En, "icons_ides") => "IDEs".to_string(),
        (Language::De, "icons_ides") => "IDEs".to_string(),
        (Language::En, "icons_terminal") => "Terminal / Explorer".to_string(),
        (Language::De, "icons_terminal") => "Terminal / Explorer".to_string(),
        (Language::En, "icons_agents") => "AI Agents".to_string(),
        (Language::De, "icons_agents") => "AI Agents".to_string(),
        (Language::En, "icons_show_shell") => "Show shell icon".to_string(),
        (Language::De, "icons_show_shell") => "Shell-Icon anzeigen".to_string(),
        (Language::En, "icons_show_explorer") => "Show explorer icon".to_string(),
        (Language::De, "icons_show_explorer") => "Explorer-Icon anzeigen".to_string(),
        (Language::En, "icons_toggle_visibility") => "Toggle visibility".to_string(),
        (Language::De, "icons_toggle_visibility") => "Sichtbarkeit umschalten".to_string(),
        (Language::En, "icons_hidden_info") => "Hidden".to_string(),
        (Language::De, "icons_hidden_info") => "Ausgeblendet".to_string(),
        (Language::En, "icons_reset") => "Reset visibility & order".to_string(),
        (Language::De, "icons_reset") => "Sichtbarkeit & Reihenfolge zurücksetzen".to_string(),
        (Language::En, "icons_all_hidden_warn") => "Warning: all IDE icons hidden – no IDE button will be shown.".to_string(),
        (Language::De, "icons_all_hidden_warn") => "Warnung: alle IDE-Icons ausgeblendet – es wird kein IDE-Button angezeigt.".to_string(),
        (Language::En, "icons_terminal_hint") => "Explorer and shell buttons appear next to IDE icons in the repo list.".to_string(),
        (Language::De, "icons_terminal_hint") => "Explorer- und Shell-Buttons erscheinen neben den IDE-Icons in der Repo-Liste.".to_string(),

        // ── Errors / validation ────────────────────────────────────────
        (Language::En, "error_need_path") => "Please specify at least one path.".to_string(),
        (Language::De, "error_need_path") => "Bitte mindestens einen Pfad angeben.".to_string(),
        (Language::En, "error_need_profile") => "At least one language profile is required.".to_string(),
        (Language::De, "error_need_profile") => "Mindestens ein Sprach-Profil erforderlich.".to_string(),
        (Language::En, "error_profile_empty") => "Profile '{}' has empty ID/name".to_string(),
        (Language::De, "error_profile_empty") => "Profil '{}' hat leere ID/Name".to_string(),
        (Language::En, "error_profile_ext") => "Profile '{}' needs a file extension".to_string(),
        (Language::De, "error_profile_ext") => "Profil '{}' braucht Dateiendung".to_string(),
        (Language::En, "saved_scan_restart") => "Saved. Restarting scan...".to_string(),
        (Language::De, "saved_scan_restart") => "Gespeichert. Scan wird neu gestartet.".to_string(),
        (Language::En, "profile_switched") => "Profile switched to '{}' – rescanning solutions...".to_string(),
        (Language::De, "profile_switched") => "Profil gewechselt zu '{}' – scanne Solutions neu...".to_string(),
        (Language::En, "opening_with") => "Opening {} with {}...".to_string(),
        (Language::De, "opening_with") => "Öffne {} mit {}...".to_string(),
        (Language::En, "starting_agent_in") => "Starting {} in {}...".to_string(),
        (Language::De, "starting_agent_in") => "Starte {} in {}...".to_string(),
        (Language::En, "profile_changed_for") => "Profile for {} changed – rescanning...".to_string(),
        (Language::De, "profile_changed_for") => "Profil für {} geändert – scanne neu...".to_string(),
        (Language::En, "save_failed") => "Save failed: {}".to_string(),
        (Language::De, "save_failed") => "Speichern fehlgeschlagen: {}".to_string(),
        (Language::En, "solution_selected") => "Solution selected: {}".to_string(),
        (Language::De, "solution_selected") => "Solution ausgewählt: {}".to_string(),
        (Language::En, "explorer_opened") => "Explorer opened: {}".to_string(),
        (Language::De, "explorer_opened") => "Explorer geöffnet: {}".to_string(),
        (Language::En, "shell_opened") => "Shell opened: {}".to_string(),
        (Language::De, "shell_opened") => "Shell geöffnet: {}".to_string(),
        (Language::En, "fetching_branches") => "Fetching branches for {}...".to_string(),
        (Language::De, "fetching_branches") => "Fetche Branches für {}...".to_string(),
        (Language::En, "fetch_failed") => "Fetch failed: {}".to_string(),
        (Language::De, "fetch_failed") => "Fetch fehlgeschlagen: {}".to_string(),
        (Language::En, "fetch_partial") => "Fetch partially failed: {}".to_string(),
        (Language::De, "fetch_partial") => "Fetch teilweise fehlgeschlagen: {}".to_string(),
        (Language::En, "auth_hint") => "Note: check Credentials/SSH (auth may be required)".to_string(),
        (Language::De, "auth_hint") => "Hinweis: Credentials/SSH prüfen (ggf. Auth erforderlich)".to_string(),
        (Language::En, "fetch_aborted") => "Background fetch aborted (internal error)".to_string(),
        (Language::De, "fetch_aborted") => "Hintergrund-Fetch abgebrochen (interner Fehler)".to_string(),
        (Language::En, "branch_switched_to") => "Branch switched to '{}': {}".to_string(),
        (Language::De, "branch_switched_to") => "Branch zu '{}' gewechselt: {}".to_string(),
        (Language::En, "ide_not_found") => "IDE '{}' not found".to_string(),
        (Language::De, "ide_not_found") => "IDE '{}' nicht in Profil '{}' gefunden".to_string(),
        (Language::En, "agent_not_found") => "AI agent '{}' not found".to_string(),
        (Language::De, "agent_not_found") => "AI-Agent '{}' nicht gefunden".to_string(),

        // ── Config Selectors ─────────────────────────────────────────────
        (Language::En, "config_selector_title") => "Config dropdowns for this profile".to_string(),
        (Language::De, "config_selector_title") => "Config-Dropdowns für dieses Profil".to_string(),
        (Language::En, "config_selector_desc") => "XML values from .config files as dropdowns in the repo row. Each selector reads/writes a key in file_path (e.g. App.config <add key=\"Database\" value=\"...\"/>).".to_string(),
        (Language::De, "config_selector_desc") => "XML-Werte aus .config Dateien als Dropdowns in der Repo-Zeile. Jeder Selector liest/schreibt einen Key in file_path (z.B. App.config <add key=\"Database\" value=\"...\"/>).".to_string(),
        (Language::En, "add_config_selector") => "Add Config Selector".to_string(),
        (Language::De, "add_config_selector") => "Selector hinzufügen".to_string(),
        (Language::En, "config_file") => "Config file:".to_string(),
        (Language::De, "config_file") => "Config-Datei:".to_string(),
        (Language::En, "config_key") => "Key:".to_string(),
        (Language::De, "config_key") => "Key:".to_string(),
        (Language::En, "config_options") => "Options:".to_string(),
        (Language::De, "config_options") => "Optionen:".to_string(),
        (Language::En, "allow_custom") => "Allow custom values".to_string(),
        (Language::De, "allow_custom") => "Eigene Werte erlauben".to_string(),
        (Language::En, "config_saved") => "{} set to '{}'".to_string(),
        (Language::De, "config_saved") => "{} auf '{}' gesetzt".to_string(),
        (Language::En, "config_save_failed") => "Failed to save config '{}': {}".to_string(),
        (Language::De, "config_save_failed") => "Config '{}' speichern fehlgeschlagen: {}".to_string(),
        (Language::En, "config_missing") => "Key '{}' not found in {}".to_string(),
        (Language::De, "config_missing") => "Key '{}' nicht gefunden in {}".to_string(),
        (Language::En, "key_attr") => "Key attribute:".to_string(),
        (Language::De, "key_attr") => "Key-Attribut:".to_string(),
        (Language::En, "value_attr") => "Value attribute:".to_string(),
        (Language::De, "value_attr") => "Value-Attribut:".to_string(),

        // ── Main list sort (Task 4) ──────────────────────────────────
        (Language::En, "sort_by_name") => "Sort by name".to_string(),
        (Language::De, "sort_by_name") => "Nach Name sortieren".to_string(),
        (Language::En, "sort_by_last_used") => "Sort by last used".to_string(),
        (Language::De, "sort_by_last_used") => "Nach letzter Nutzung sortieren".to_string(),
        (Language::En, "sort_tooltip") => "Sort repositories".to_string(),
        (Language::De, "sort_tooltip") => "Repositories sortieren".to_string(),
        (Language::En, "sort_title") => "Sorting".to_string(),
        (Language::De, "sort_title") => "Sortierung".to_string(),

        // ── Behind indicator + auto-fetch (Task 5) ───────────────────
        (Language::En, "branch_behind") => "Behind by {} commit(s)".to_string(),
        (Language::De, "branch_behind") => "{} Commit(s) hinterher".to_string(),
        (Language::En, "auto_fetch_label") => "Auto-fetch interval (minutes, 0 = off)".to_string(),
        (Language::De, "auto_fetch_label") => "Auto-Fetch Intervall (Minuten, 0 = aus)".to_string(),
        (Language::En, "auto_fetch_hint") => "Periodically runs git fetch --all for all scanned repos in background. 0 disables.".to_string(),
        (Language::De, "auto_fetch_hint") => "Führt periodisch git fetch --all für alle gescannten Repos im Hintergrund aus. 0 deaktiviert.".to_string(),
        (Language::En, "auto_fetch_off") => "Disabled".to_string(),
        (Language::De, "auto_fetch_off") => "Deaktiviert".to_string(),
        (Language::En, "auto_fetch_running") => "Auto-fetch running, try again shortly".to_string(),
        (Language::De, "auto_fetch_running") => "Auto-Fetch läuft, bitte kurz warten".to_string(),

        // ── Tray icon theme (Dark/Light) ─────────────────────────────
        (Language::En, "tray_icon_theme_title") => "Tray icon style".to_string(),
        (Language::De, "tray_icon_theme_title") => "Tray-Icon Stil".to_string(),
        (Language::En, "tray_icon_theme_desc") => "White icon for dark taskbar (Dark) or dark icon for light taskbar (Light).".to_string(),
        (Language::De, "tray_icon_theme_desc") => "Weißes Icon für dunkle Taskleiste (Dark) oder dunkles Icon für helle Taskleiste (Light).".to_string(),
        (Language::En, "tray_icon_theme_dark") => "Dark (white icon)".to_string(),
        (Language::De, "tray_icon_theme_dark") => "Dark (weißes Icon)".to_string(),
        (Language::En, "tray_icon_theme_light") => "Light (dark icon)".to_string(),
        (Language::De, "tray_icon_theme_light") => "Light (dunkles Icon)".to_string(),

        // ── Merge Requests: Approvals, Pipeline, Threads, Review ─────
        (Language::En, "mr_appr_of") => "{} of {} approvals".to_string(),
        (Language::De, "mr_appr_of") => "{} von {} Approvals".to_string(),
        (Language::En, "mr_appr_one") => "1 approval".to_string(),
        (Language::De, "mr_appr_one") => "1 Approval".to_string(),
        (Language::En, "mr_appr_n") => "{} approvals".to_string(),
        (Language::De, "mr_appr_n") => "{} Approvals".to_string(),
        (Language::En, "mr_appr_short") => "{}/{}".to_string(),
        (Language::De, "mr_appr_short") => "{}/{}".to_string(),
        (Language::En, "mr_appr_no_rule") => "No approval rule – no approvals required".to_string(),
        (Language::De, "mr_appr_no_rule") => "Keine Approval-Regel – keine Approvals erforderlich".to_string(),
        (Language::En, "mr_appr_you") => "{} (you)".to_string(),
        (Language::De, "mr_appr_you") => "{} (du)".to_string(),
        (Language::En, "mr_rev_started") => "{} started reviewing".to_string(),
        (Language::De, "mr_rev_started") => "{} hat das Review begonnen".to_string(),
        (Language::En, "mr_rev_waiting") => "{} pending".to_string(),
        (Language::De, "mr_rev_waiting") => "{} ausstehend".to_string(),
        (Language::En, "mr_pipe_success") => "Pipeline passed".to_string(),
        (Language::De, "mr_pipe_success") => "Pipeline grün".to_string(),
        (Language::En, "mr_pipe_failed") => "Pipeline failed".to_string(),
        (Language::De, "mr_pipe_failed") => "Pipeline fehlgeschlagen".to_string(),
        (Language::En, "mr_pipe_running") => "Pipeline running".to_string(),
        (Language::De, "mr_pipe_running") => "Pipeline läuft".to_string(),
        (Language::En, "mr_pipe_pending") => "Pipeline pending".to_string(),
        (Language::De, "mr_pipe_pending") => "Pipeline wartet".to_string(),
        (Language::En, "mr_pipe_canceled") => "Pipeline canceled".to_string(),
        (Language::De, "mr_pipe_canceled") => "Pipeline abgebrochen".to_string(),
        (Language::En, "mr_pipe_skipped") => "Pipeline skipped".to_string(),
        (Language::De, "mr_pipe_skipped") => "Pipeline übersprungen".to_string(),
        (Language::En, "mr_pipe_manual") => "Pipeline manual".to_string(),
        (Language::De, "mr_pipe_manual") => "Pipeline manuell".to_string(),
        (Language::En, "mr_pipe_none") => "No pipeline".to_string(),
        (Language::De, "mr_pipe_none") => "Keine Pipeline".to_string(),
        (Language::En, "mr_threads_one") => "1 open thread".to_string(),
        (Language::De, "mr_threads_one") => "1 offener Thread".to_string(),
        (Language::En, "mr_threads_many") => "{} open threads".to_string(),
        (Language::De, "mr_threads_many") => "{} offene Threads".to_string(),
        (Language::En, "mr_threads_resolved") => "all threads resolved".to_string(),
        (Language::De, "mr_threads_resolved") => "alle Threads gelöst".to_string(),
        (Language::En, "mr_threads_none") => "no threads".to_string(),
        (Language::De, "mr_threads_none") => "keine Threads".to_string(),
        (Language::En, "mr_threads_tt") => "{} open · {} resolved".to_string(),
        (Language::De, "mr_threads_tt") => "{} offen · {} gelöst".to_string(),
        (Language::En, "mr_threads_none_tt") => "No discussion threads".to_string(),
        (Language::De, "mr_threads_none_tt") => "Keine Diskussions-Threads".to_string(),
        (Language::En, "mr_rev_pending") => "Review pending".to_string(),
        (Language::De, "mr_rev_pending") => "Review ausstehend".to_string(),
        (Language::En, "mr_rev_approved") => "Approved by you".to_string(),
        (Language::De, "mr_rev_approved") => "Von dir approved".to_string(),
        (Language::En, "mr_rev_changes") => "Changes requested".to_string(),
        (Language::De, "mr_rev_changes") => "Änderungen angefragt".to_string(),
        (Language::En, "mr_rev_commented") => "Commented".to_string(),
        (Language::De, "mr_rev_commented") => "Kommentiert".to_string(),

        // ── Merge Requests: Abruf- und Prüffehler, Abfrageteile ─────────
        (Language::En, "mr_err_scope") => "The token needs read_api.".to_string(),
        (Language::De, "mr_err_scope") => "Das Token braucht read_api.".to_string(),
        (Language::En, "mr_err_network") => "{} is not reachable.".to_string(),
        (Language::De, "mr_err_network") => "{} nicht erreichbar.".to_string(),
        (Language::En, "mr_err_tls") => "The certificate of {} is not trusted.".to_string(),
        (Language::De, "mr_err_tls") => "Zertifikat von {} wird nicht vertraut.".to_string(),
        (Language::En, "mr_err_timeout") => "{} timed out after {} s.".to_string(),
        (Language::De, "mr_err_timeout") => "Zeitüberschreitung bei {} nach {} s.".to_string(),
        (Language::En, "mr_err_content_type") => "glab sent the request without a JSON content type.".to_string(),
        (Language::De, "mr_err_content_type") => "glab hat die Anfrage ohne JSON-Content-Type gesendet.".to_string(),
        (Language::En, "mr_part_mentions") => "Mentions".to_string(),
        (Language::De, "mr_part_mentions") => "Erwähnungen".to_string(),
        (Language::En, "mr_part_reviews") => "Review requests".to_string(),
        (Language::De, "mr_part_reviews") => "Review-Anfragen".to_string(),
        (Language::En, "mr_part_uninvolved") => "Other open MRs".to_string(),
        (Language::De, "mr_part_uninvolved") => "Weitere offene MRs".to_string(),
        (Language::En, "mr_part_mine") => "Own MRs".to_string(),
        (Language::De, "mr_part_mine") => "Eigene MRs".to_string(),

        // ── Merge Requests: Ansicht (Design B) ───────────────────────
        (Language::En, "mr_open_settings") => "Open settings".to_string(),
        (Language::De, "mr_open_settings") => "Einstellungen öffnen".to_string(),
        (Language::En, "mr_loading") => "Loading merge requests …".to_string(),
        (Language::De, "mr_loading") => "Lade Merge Requests …".to_string(),
        (Language::En, "mr_mine_title") => "My merge requests".to_string(),
        (Language::De, "mr_mine_title") => "Meine Merge Requests".to_string(),
        (Language::En, "mr_others_title") => "From others".to_string(),
        (Language::De, "mr_others_title") => "Von anderen".to_string(),
        (Language::En, "mr_count_of") => "{} of {}".to_string(),
        (Language::De, "mr_count_of") => "{} von {}".to_string(),
        (Language::En, "mr_sorted") => "Sorted by approvals".to_string(),
        (Language::De, "mr_sorted") => "Sortiert nach Approvals".to_string(),
        (Language::En, "mr_sorted_tt") => "Most approvals first; ties: MRs with an approval rule and fewer missing approvals first, then most recently updated".to_string(),
        (Language::De, "mr_sorted_tt") => "Meiste Approvals zuerst; bei Gleichstand zuerst MRs mit Approval-Regel und weniger fehlenden Approvals, dann zuletzt aktualisiert".to_string(),
        (Language::En, "mr_chip_reviewer") => "Reviewer".to_string(),
        (Language::De, "mr_chip_reviewer") => "Reviewer".to_string(),
        (Language::En, "mr_chip_reviewer_tt") => "MRs where you are a reviewer".to_string(),
        (Language::De, "mr_chip_reviewer_tt") => "MRs, bei denen du als Reviewer eingetragen bist".to_string(),
        (Language::En, "mr_chip_mention") => "Mentioned".to_string(),
        (Language::De, "mr_chip_mention") => "Erwähnt".to_string(),
        (Language::En, "mr_chip_mention_tt") => "MRs mentioning you in the description or a comment".to_string(),
        (Language::De, "mr_chip_mention_tt") => "MRs, in deren Beschreibung oder Kommentaren du erwähnt wirst".to_string(),
        (Language::En, "mr_chip_all") => "All open".to_string(),
        (Language::De, "mr_chip_all") => "Alle offenen".to_string(),
        (Language::En, "mr_chip_all_tt") => "Also show MRs I'm not involved in".to_string(),
        (Language::De, "mr_chip_all_tt") => "Auch MRs ohne meine Beteiligung anzeigen".to_string(),
        (Language::En, "mr_chip_all_tt_n") => "Also show MRs I'm not involved in ({} open in {} local projects)".to_string(),
        (Language::De, "mr_chip_all_tt_n") => "Auch MRs ohne meine Beteiligung anzeigen ({} offene in {} lokalen Projekten)".to_string(),
        (Language::En, "mr_chip_all_tt_n_one") => "Also show MRs I'm not involved in ({} open in 1 local project)".to_string(),
        (Language::De, "mr_chip_all_tt_n_one") => "Auch MRs ohne meine Beteiligung anzeigen ({} offene in 1 lokalen Projekt)".to_string(),
        (Language::En, "mr_reason_reviewer") => "Reviewer".to_string(),
        (Language::De, "mr_reason_reviewer") => "Reviewer".to_string(),
        (Language::En, "mr_reason_desc") => "@ description".to_string(),
        (Language::De, "mr_reason_desc") => "@ Beschreibung".to_string(),
        (Language::En, "mr_reason_comment") => "@ comment".to_string(),
        (Language::De, "mr_reason_comment") => "@ Kommentar".to_string(),
        (Language::En, "mr_reasons_prefix") => "Shown because:".to_string(),
        (Language::De, "mr_reasons_prefix") => "Angezeigt wegen:".to_string(),
        (Language::En, "mr_no_filter") => "No filter active".to_string(),
        (Language::De, "mr_no_filter") => "Kein Filter aktiv".to_string(),
        (Language::En, "mr_no_filter_hint") => "Turn on “Reviewer” or “Mentioned” to see merge requests you're involved in.".to_string(),
        (Language::De, "mr_no_filter_hint") => "Schalte „Reviewer“ oder „Erwähnt“ ein, um Merge Requests zu sehen, an denen du beteiligt bist.".to_string(),
        (Language::En, "mr_filter_no_match") => "No merge requests match this filter".to_string(),
        (Language::De, "mr_filter_no_match") => "Keine Merge Requests für diesen Filter".to_string(),
        (Language::En, "mr_empty_mine") => "You have no open merge requests.".to_string(),
        (Language::De, "mr_empty_mine") => "Du hast keine offenen Merge Requests.".to_string(),
        (Language::En, "mr_empty_others") => "No review is waiting for you.".to_string(),
        (Language::De, "mr_empty_others") => "Gerade wartet kein Review auf dich.".to_string(),
        (Language::En, "mr_hidden_by_selection_one") => "1 MR from deselected repositories hidden".to_string(),
        (Language::De, "mr_hidden_by_selection_one") => "1 MR aus abgewählten Repositories ausgeblendet".to_string(),
        (Language::En, "mr_hidden_by_selection_many") => "{} MRs from deselected repositories hidden".to_string(),
        (Language::De, "mr_hidden_by_selection_many") => "{} MRs aus abgewählten Repositories ausgeblendet".to_string(),
        (Language::En, "mr_uninvolved_h") => "Not involved · {}".to_string(),
        (Language::De, "mr_uninvolved_h") => "Nicht beteiligt · {}".to_string(),
        (Language::En, "mr_uninvolved_sub") => "open MRs in the {} local projects without your involvement".to_string(),
        (Language::De, "mr_uninvolved_sub") => "offene MRs der {} lokalen Projekte ohne deine Beteiligung".to_string(),
        (Language::En, "mr_uninvolved_sub_one") => "open MRs in the local project without your involvement".to_string(),
        (Language::De, "mr_uninvolved_sub_one") => "offene MRs des lokalen Projekts ohne deine Beteiligung".to_string(),
        (Language::En, "mr_shown_of") => "{} of {} shown".to_string(),
        (Language::De, "mr_shown_of") => "{} von {} angezeigt".to_string(),
        (Language::En, "mr_load_more") => "Load {} more".to_string(),
        (Language::De, "mr_load_more") => "{} weitere laden".to_string(),
        (Language::En, "mr_loading_uninvolved") => "Loading more open merge requests …".to_string(),
        (Language::De, "mr_loading_uninvolved") => "Lade weitere offene Merge Requests …".to_string(),
        (Language::En, "mr_uninvolved_not_loaded") => "Not loaded yet.".to_string(),
        (Language::De, "mr_uninvolved_not_loaded") => "Noch nicht geladen.".to_string(),
        (Language::En, "mr_load_now") => "Load now".to_string(),
        (Language::De, "mr_load_now") => "Jetzt laden".to_string(),
        (Language::En, "mr_draft") => "Draft".to_string(),
        (Language::De, "mr_draft") => "Entwurf".to_string(),
        (Language::En, "mr_ready") => "ready to merge".to_string(),
        (Language::De, "mr_ready") => "mergebereit".to_string(),
        (Language::En, "mr_ready_tt") => "GitLab: can be merged".to_string(),
        (Language::De, "mr_ready_tt") => "GitLab: kann gemergt werden".to_string(),
        (Language::En, "mr_pipe_open_tt") => "{} – open pipeline in browser".to_string(),
        (Language::De, "mr_pipe_open_tt") => "{} – Pipeline im Browser öffnen".to_string(),

        // ── Merge Requests: Aktionen und Checkout-Menü (K1) ──────────
        (Language::En, "mr_open_browser") => "Open in browser".to_string(),
        (Language::De, "mr_open_browser") => "Im Browser öffnen".to_string(),
        (Language::En, "mr_open_title_tt") => "Open merge request in browser".to_string(),
        (Language::De, "mr_open_title_tt") => "Merge Request im Browser öffnen".to_string(),
        (Language::En, "mr_checkout_menu_tt") => "Check out – choose target".to_string(),
        (Language::De, "mr_checkout_menu_tt") => "Auschecken – Ziel wählen".to_string(),
        (Language::En, "mr_menu_header_clones") => "{} · {} local clones".to_string(),
        (Language::De, "mr_menu_header_clones") => "{} · {} lokale Klone".to_string(),
        (Language::En, "mr_menu_header_clone_one") => "{} · 1 local clone".to_string(),
        (Language::De, "mr_menu_header_clone_one") => "{} · 1 lokaler Klon".to_string(),
        (Language::En, "mr_menu_new_folder") => "In a new folder".to_string(),
        (Language::De, "mr_menu_new_folder") => "In neuem Ordner".to_string(),
        (Language::En, "mr_menu_worktree_review") => "New worktree in the review folder (recommended)".to_string(),
        (Language::De, "mr_menu_worktree_review") => "Neuer Worktree im Review-Ordner (empfohlen)".to_string(),
        (Language::En, "mr_menu_worktree") => "New worktree (recommended)".to_string(),
        (Language::De, "mr_menu_worktree") => "Neuer Worktree (empfohlen)".to_string(),
        (Language::En, "mr_menu_worktree_sub") => "{} · shares git data with {}".to_string(),
        (Language::De, "mr_menu_worktree_sub") => "{} · teilt Git-Daten mit {}".to_string(),
        (Language::En, "mr_menu_other_folder") => "Choose another folder …".to_string(),
        (Language::De, "mr_menu_other_folder") => "Anderen Ordner wählen …".to_string(),
        (Language::En, "mr_menu_clone") => "New clone …".to_string(),
        (Language::De, "mr_menu_clone") => "Neuer Klon …".to_string(),
        (Language::En, "mr_menu_clone_sub") => "fully independent, downloads again".to_string(),
        (Language::De, "mr_menu_clone_sub") => "vollständig unabhängig, lädt neu herunter".to_string(),
        (Language::En, "mr_menu_switch") => "Switch an existing clone".to_string(),
        (Language::De, "mr_menu_switch") => "In bestehendem Klon wechseln".to_string(),
        (Language::En, "mr_state_free") => "free".to_string(),
        (Language::De, "mr_state_free") => "frei".to_string(),
        (Language::En, "mr_state_in_use") => "in use for !{}".to_string(),
        (Language::De, "mr_state_in_use") => "in Arbeit für !{}".to_string(),
        (Language::En, "mr_state_in_use_branch") => "in use".to_string(),
        (Language::De, "mr_state_in_use_branch") => "in Arbeit".to_string(),
        (Language::En, "mr_state_dirty_one") => "1 change".to_string(),
        (Language::De, "mr_state_dirty_one") => "1 Änderung".to_string(),
        (Language::En, "mr_state_dirty_many") => "{} changes".to_string(),
        (Language::De, "mr_state_dirty_many") => "{} Änderungen".to_string(),
        (Language::En, "mr_state_dirty_hint") => "asks to stash/discard".to_string(),
        (Language::De, "mr_state_dirty_hint") => "fragt nach Stash/Verwerfen".to_string(),
        (Language::En, "mr_last_used") => "last used {}".to_string(),
        (Language::De, "mr_last_used") => "zuletzt genutzt {}".to_string(),
        (Language::En, "mr_menu_after") => "Then open".to_string(),
        (Language::De, "mr_menu_after") => "Danach öffnen".to_string(),
        (Language::En, "mr_after_shell") => "Shell".to_string(),
        (Language::De, "mr_after_shell") => "Shell".to_string(),
        (Language::En, "mr_after_explorer") => "Explorer".to_string(),
        (Language::De, "mr_after_explorer") => "Explorer".to_string(),
        (Language::En, "mr_after_nothing") => "Nothing".to_string(),
        (Language::De, "mr_after_nothing") => "Nichts".to_string(),
        (Language::En, "mr_after_remembered") => "remembers the last choice".to_string(),
        (Language::De, "mr_after_remembered") => "merkt sich die letzte Wahl".to_string(),
        (Language::En, "mr_already_header") => "Already checked out in {}".to_string(),
        (Language::De, "mr_already_header") => "Bereits ausgecheckt in {}".to_string(),
        (Language::En, "mr_open_show_in_list") => "Show in repository list".to_string(),
        (Language::De, "mr_open_show_in_list") => "In Repo-Liste zeigen".to_string(),
        (Language::En, "mr_open_agent") => "Start {}".to_string(),
        (Language::De, "mr_open_agent") => "{} starten".to_string(),
        (Language::En, "mr_open_shell") => "Open shell".to_string(),
        (Language::De, "mr_open_shell") => "Shell öffnen".to_string(),
        (Language::En, "mr_open_explorer") => "Open in Explorer".to_string(),
        (Language::De, "mr_open_explorer") => "Im Explorer öffnen".to_string(),
        (Language::En, "mr_open_ide") => "Open in {}".to_string(),
        (Language::De, "mr_open_ide") => "In {} öffnen".to_string(),
        (Language::En, "mr_checked_out_tt") => "Branch is already checked out in {}".to_string(),
        (Language::De, "mr_checked_out_tt") => "Branch ist in {} bereits ausgecheckt".to_string(),
        (Language::En, "mr_not_local") => "not local".to_string(),
        (Language::De, "mr_not_local") => "nicht lokal".to_string(),
        (Language::En, "mr_not_local_tt") => "Project isn't cloned locally – checkout not possible".to_string(),
        (Language::De, "mr_not_local_tt") => "Projekt ist nicht lokal geklont – Auschecken nicht möglich".to_string(),
        (Language::En, "mr_fork_tt") => "Fork MR – checkout will come in a later version".to_string(),
        (Language::De, "mr_fork_tt") => "Fork-MR – Auschecken folgt in einer späteren Version".to_string(),
        (Language::En, "mr_branch_deleted_tt") => "Source branch no longer exists".to_string(),
        (Language::De, "mr_branch_deleted_tt") => "Quell-Branch existiert nicht mehr".to_string(),

        // ── Merge Requests: Auschecken in der App (K1) ───────────────
        (Language::En, "mr_checkout_busy") => "Fetching {} for !{} …".to_string(),
        (Language::De, "mr_checkout_busy") => "Hole {} für !{} …".to_string(),
        (Language::En, "mr_cloning") => "Cloning {} to {} …".to_string(),
        (Language::De, "mr_cloning") => "Klone {} nach {} …".to_string(),
        (Language::En, "mr_checkout_blocked") => "Checkout not possible while a scan or fetch is running".to_string(),
        (Language::De, "mr_checkout_blocked") => "Auschecken nicht möglich, solange ein Scan oder Fetch läuft".to_string(),
        (Language::En, "mr_switch_waiting") => "A branch switch is still waiting for your decision – finish that dialog first".to_string(),
        (Language::De, "mr_switch_waiting") => "Ein Branch-Wechsel wartet noch auf deine Entscheidung – erst den Dialog abschließen".to_string(),
        (Language::En, "mr_checkout_gone") => "!{} is no longer in the current list – checkout skipped".to_string(),
        (Language::De, "mr_checkout_gone") => "!{} ist nicht mehr im aktuellen Stand – Auschecken übersprungen".to_string(),
        (Language::En, "mr_checkout_running") => "Checking out !{} …".to_string(),
        (Language::De, "mr_checkout_running") => "Auschecken für !{} läuft …".to_string(),
        (Language::En, "mr_worktree_created") => "Worktree {} created for !{}".to_string(),
        (Language::De, "mr_worktree_created") => "Worktree {} für !{} angelegt".to_string(),
        (Language::En, "mr_clone_done") => "!{} cloned to {}".to_string(),
        (Language::De, "mr_clone_done") => "!{} geklont nach {}".to_string(),
        (Language::En, "mr_checkout_done") => "!{} checked out: {} in {}".to_string(),
        (Language::De, "mr_checkout_done") => "!{} ausgecheckt: {} in {}".to_string(),
        (Language::En, "mr_checkout_done_ff") => "!{} checked out: {} in {} (+{} commits)".to_string(),
        (Language::De, "mr_checkout_done_ff") => "!{} ausgecheckt: {} in {} (+{} Commits)".to_string(),
        (Language::En, "mr_checkout_done_ff_one") => "!{} checked out: {} in {} (+1 commit)".to_string(),
        (Language::De, "mr_checkout_done_ff_one") => "!{} ausgecheckt: {} in {} (+1 Commit)".to_string(),
        (Language::En, "mr_already_head") => "Already checked out in {}".to_string(),
        (Language::De, "mr_already_head") => "Bereits ausgecheckt in {}".to_string(),
        (Language::En, "mr_already_head_behind") => "Already checked out in {} ({} behind)".to_string(),
        (Language::De, "mr_already_head_behind") => "Bereits ausgecheckt in {} ({} hinterher)".to_string(),
        (Language::En, "mr_then_agent") => "– starting {}".to_string(),
        (Language::De, "mr_then_agent") => "– {} startet".to_string(),
        (Language::En, "mr_then_shell") => "– opening shell".to_string(),
        (Language::De, "mr_then_shell") => "– Shell öffnet".to_string(),
        (Language::En, "mr_then_explorer") => "– opening Explorer".to_string(),
        (Language::De, "mr_then_explorer") => "– Explorer öffnet".to_string(),
        (Language::En, "mr_then_ide") => "– opening {}".to_string(),
        (Language::De, "mr_then_ide") => "– {} öffnet".to_string(),
        (Language::En, "mr_then_no_ide") => "– no IDE in the profile, nothing opened".to_string(),
        (Language::De, "mr_then_no_ide") => "– keine IDE im Profil, nichts geöffnet".to_string(),
        (Language::En, "mr_no_ide") => "No IDE in this folder's profile".to_string(),
        (Language::De, "mr_no_ide") => "Keine IDE im Profil dieses Ordners".to_string(),
        (Language::En, "mr_diverged") => "Local branch {} differs from the MR ({} ahead, {} behind) – local state checked out".to_string(),
        (Language::De, "mr_diverged") => "Lokaler Branch {} weicht vom MR-Stand ab ({} voraus, {} hinterher) – lokaler Stand ausgecheckt".to_string(),
        (Language::En, "mr_checkout_failed") => "Checking out !{} failed: {}".to_string(),
        (Language::De, "mr_checkout_failed") => "Auschecken von !{} fehlgeschlagen: {}".to_string(),
        (Language::En, "mr_checkout_no_remote") => "No remote in {} points to {}".to_string(),
        (Language::De, "mr_checkout_no_remote") => "Kein Remote in {} zeigt auf {}".to_string(),
        (Language::En, "mr_repo_open_failed") => "Cannot open repository {}".to_string(),
        (Language::De, "mr_repo_open_failed") => "Repository {} lässt sich nicht öffnen".to_string(),
        (Language::En, "mr_folder_exists") => "The folder {} already exists and is not empty".to_string(),
        (Language::De, "mr_folder_exists") => "Der Ordner {} existiert bereits und ist nicht leer".to_string(),
        (Language::En, "mr_remote_branch_missing") => "Remote branch {} not found".to_string(),
        (Language::De, "mr_remote_branch_missing") => "Remote-Branch {} nicht gefunden".to_string(),
        (Language::En, "mr_branch_name_taken") => "Neither {} nor {} can be used as a local branch".to_string(),
        (Language::De, "mr_branch_name_taken") => "Weder {} noch {} lassen sich als lokaler Branch nutzen".to_string(),

        // ── Merge Requests: Zustände der Ansicht ─────────────────────
        (Language::En, "mr_no_remotes") => "No git remote was found in the scanned repositories.".to_string(),
        (Language::De, "mr_no_remotes") => "In den gescannten Repositories wurde kein Git-Remote gefunden.".to_string(),
        (Language::En, "mr_no_gitlab_host") => "No GitLab host detected – glab isn't signed in to any of your remote hosts.".to_string(),
        (Language::De, "mr_no_gitlab_host") => "Kein GitLab-Host erkannt – glab ist für keinen deiner Remote-Hosts angemeldet.".to_string(),
        (Language::En, "mr_add_token") => "Add token".to_string(),
        (Language::De, "mr_add_token") => "Token hinzufügen".to_string(),
        (Language::En, "mr_add_gitlab_host") => "Add as GitLab host".to_string(),
        (Language::De, "mr_add_gitlab_host") => "Als GitLab-Host hinzufügen".to_string(),
        (Language::En, "mr_all_deselected") => "All repositories are deselected in the settings.".to_string(),
        (Language::De, "mr_all_deselected") => "Alle Repositories sind in den Einstellungen abgewählt.".to_string(),
        (Language::En, "mr_glab_missing") => "glab was not found".to_string(),
        (Language::De, "mr_glab_missing") => "glab wurde nicht gefunden".to_string(),
        (Language::En, "mr_glab_missing_desc") => "GitManager queries merge requests via the GitLab CLI (glab).".to_string(),
        (Language::De, "mr_glab_missing_desc") => "GitManager fragt Merge Requests über die GitLab CLI (glab) ab.".to_string(),
        (Language::En, "mr_glab_restart_hint") => "After installing, restart GitManager or set the path to glab.exe.".to_string(),
        (Language::De, "mr_glab_restart_hint") => "Nach der Installation GitManager neu starten oder den Pfad zu glab.exe angeben.".to_string(),
        (Language::En, "mr_glab_too_old") => "glab {} is too old – please update: winget upgrade GLab.GLab".to_string(),
        (Language::De, "mr_glab_too_old") => "glab {} ist zu alt – bitte aktualisieren: winget upgrade GLab.GLab".to_string(),
        (Language::En, "mr_copy_cmd") => "Copy command".to_string(),
        (Language::De, "mr_copy_cmd") => "Befehl kopieren".to_string(),
        (Language::En, "mr_not_logged_in") => "Not signed in to {}".to_string(),
        (Language::De, "mr_not_logged_in") => "Nicht bei {} angemeldet".to_string(),
        (Language::En, "mr_partial") => "{} could not be loaded ({}).".to_string(),
        (Language::De, "mr_partial") => "{} konnten nicht geladen werden ({}).".to_string(),
        (Language::En, "mr_host_failed") => "Merge requests from {} could not be loaded.".to_string(),
        (Language::De, "mr_host_failed") => "Merge Requests von {} konnten nicht geladen werden.".to_string(),
        (Language::En, "mr_err_unknown") => "unknown error".to_string(),
        (Language::De, "mr_err_unknown") => "unbekannter Fehler".to_string(),
        (Language::En, "mr_details") => "Details".to_string(),
        (Language::De, "mr_details") => "Details".to_string(),

        // ── Merge Requests: Umschalter, Top-Leiste, Statusleiste ──────
        (Language::En, "view_repos") => "Repositories".to_string(),
        (Language::De, "view_repos") => "Repositories".to_string(),
        (Language::En, "view_repos_tt") => "Show repository list".to_string(),
        (Language::De, "view_repos_tt") => "Repository-Liste anzeigen".to_string(),
        (Language::En, "view_mrs") => "Merge Requests".to_string(),
        (Language::De, "view_mrs") => "Merge Requests".to_string(),
        (Language::En, "view_mrs_tt") => "Show merge requests".to_string(),
        (Language::De, "view_mrs_tt") => "Merge Requests anzeigen".to_string(),
        (Language::En, "mr_badge_tt_one") => "Merge requests – 1 review waiting for you".to_string(),
        (Language::De, "mr_badge_tt_one") => "Merge Requests – 1 Review wartet auf dich".to_string(),
        (Language::En, "mr_badge_tt_many") => "Merge requests – {} reviews waiting for you".to_string(),
        (Language::De, "mr_badge_tt_many") => "Merge Requests – {} Reviews warten auf dich".to_string(),
        (Language::En, "mr_reload_tt") => "Reload merge requests".to_string(),
        (Language::De, "mr_reload_tt") => "Merge Requests neu laden".to_string(),
        (Language::En, "mr_status_line") => "Merge requests · {} · updated {}".to_string(),
        (Language::De, "mr_status_line") => "Merge Requests · {} · Stand: {}".to_string(),
        (Language::En, "mr_refreshing") => "Refreshing … · updated {}".to_string(),
        (Language::De, "mr_refreshing") => "Aktualisiere … · Stand: {}".to_string(),
        (Language::En, "mr_fetch_aborted") => "Loading merge requests was aborted (internal error)".to_string(),
        (Language::De, "mr_fetch_aborted") => "Abruf der Merge Requests abgebrochen (interner Fehler)".to_string(),

        // ── Merge Requests: Tray-Fußzeile (E21) ───────────────────────
        (Language::En, "tray_main_window") => "Main window".to_string(),
        (Language::De, "tray_main_window") => "Hauptfenster".to_string(),
        (Language::En, "tray_mrs") => "Merge Requests".to_string(),
        (Language::De, "tray_mrs") => "Merge Requests".to_string(),
        (Language::En, "tray_mrs_tt_one") => "Merge requests – 1 review waiting for you".to_string(),
        (Language::De, "tray_mrs_tt_one") => "Merge Requests – 1 Review wartet auf dich".to_string(),
        (Language::En, "tray_mrs_tt_many") => "Merge requests – {} reviews waiting for you".to_string(),
        (Language::De, "tray_mrs_tt_many") => "Merge Requests – {} Reviews warten auf dich".to_string(),
        (Language::En, "tray_mrs_tt_none") => "Open main window with merge requests".to_string(),
        (Language::De, "tray_mrs_tt_none") => "Hauptfenster mit Merge Requests öffnen".to_string(),
        (Language::En, "tray_quit") => "Quit".to_string(),
        (Language::De, "tray_quit") => "Beenden".to_string(),

        // ── Merge Requests: Einstellungs-Tab (Task 12) ────────────────
        (Language::En, "tabs_merge_requests") => "Merge Requests".to_string(),
        (Language::De, "tabs_merge_requests") => "Merge Requests".to_string(),
        (Language::En, "mrs_sec_cli") => "GitLab CLI".to_string(),
        (Language::De, "mrs_sec_cli") => "GitLab CLI".to_string(),
        (Language::En, "mrs_glab_program") => "glab program".to_string(),
        (Language::De, "mrs_glab_program") => "glab-Programm".to_string(),
        (Language::En, "mrs_glab_found") => "glab {} found · {}".to_string(),
        (Language::De, "mrs_glab_found") => "glab {} gefunden · {}".to_string(),
        (Language::En, "mrs_glab_not_found") => "glab not found".to_string(),
        (Language::De, "mrs_glab_not_found") => "glab nicht gefunden".to_string(),
        (Language::En, "mrs_glab_invalid") => "Invalid glab program: characters like & | ; < > ` $ are only allowed in a full path.".to_string(),
        (Language::De, "mrs_glab_invalid") => "Ungültiges glab-Programm: Sonderzeichen wie & | ; < > ` $ sind nur in einem vollen Pfad erlaubt.".to_string(),
        (Language::En, "mrs_recheck") => "Check again".to_string(),
        (Language::De, "mrs_recheck") => "Erneut prüfen".to_string(),
        (Language::En, "mrs_sec_auth") => "Sign-in & token".to_string(),
        (Language::De, "mrs_sec_auth") => "Anmeldung & Token".to_string(),
        (Language::En, "mrs_state_checking") => "Checking …".to_string(),
        (Language::De, "mrs_state_checking") => "Prüfe …".to_string(),
        (Language::En, "mrs_state_ready") => "Ready".to_string(),
        (Language::De, "mrs_state_ready") => "Bereit".to_string(),
        (Language::En, "mrs_state_rights_missing") => "Missing rights (read_api or api)".to_string(),
        (Language::De, "mrs_state_rights_missing") => "Rechte fehlen (read_api oder api)".to_string(),
        (Language::En, "mrs_state_not_signed") => "Not signed in".to_string(),
        (Language::De, "mrs_state_not_signed") => "Nicht angemeldet".to_string(),
        (Language::En, "mrs_state_expired") => "Expired".to_string(),
        (Language::De, "mrs_state_expired") => "Abgelaufen".to_string(),
        (Language::En, "mrs_state_revoked") => "Revoked".to_string(),
        (Language::De, "mrs_state_revoked") => "Widerrufen".to_string(),
        (Language::En, "mrs_state_unreachable") => "Not reachable".to_string(),
        (Language::De, "mrs_state_unreachable") => "Nicht erreichbar".to_string(),
        (Language::En, "mrs_state_tls") => "Certificate rejected".to_string(),
        (Language::De, "mrs_state_tls") => "Zertifikat abgelehnt".to_string(),
        (Language::En, "mrs_rights_gitmanager_ok") => "Enough for the merge request view (needs read_api or api)".to_string(),
        (Language::De, "mrs_rights_gitmanager_ok") => "Reicht für die Merge-Request-Ansicht (benötigt read_api oder api)".to_string(),
        (Language::En, "mrs_glab_full") => "glab: full".to_string(),
        (Language::De, "mrs_glab_full") => "glab: Voll".to_string(),
        (Language::En, "mrs_glab_readonly") => "glab: read-only – glab mr approve/merge/note/create will fail".to_string(),
        (Language::De, "mrs_glab_readonly") => "glab: Nur lesen – glab mr approve/merge/note/create schlagen fehl".to_string(),
        (Language::En, "mrs_glab_insufficient") => "glab: insufficient".to_string(),
        (Language::De, "mrs_glab_insufficient") => "glab: Unzureichend".to_string(),
        (Language::En, "mrs_glab_unknown") => "glab: rights unknown – scopes not readable (e.g. fine-grained token)".to_string(),
        (Language::De, "mrs_glab_unknown") => "glab: Rechte unbekannt – Scopes nicht lesbar (z. B. feingranulares Token)".to_string(),
        (Language::En, "mrs_glab_scopes_hint") => "glab recommends the scopes api and write_repository.".to_string(),
        (Language::De, "mrs_glab_scopes_hint") => "glab empfiehlt die Scopes api und write_repository.".to_string(),
        (Language::En, "mrs_signed_in_as") => "Signed in as {} (@{})".to_string(),
        (Language::De, "mrs_signed_in_as") => "Angemeldet als {} (@{})".to_string(),
        (Language::En, "mrs_token_kind") => "Token type: {}".to_string(),
        (Language::De, "mrs_token_kind") => "Token-Art: {}".to_string(),
        (Language::En, "mrs_kind_pat") => "Personal access token".to_string(),
        (Language::De, "mrs_kind_pat") => "Persönliches Zugriffstoken".to_string(),
        (Language::En, "mrs_kind_oauth") => "OAuth (glab)".to_string(),
        (Language::De, "mrs_kind_oauth") => "OAuth (glab)".to_string(),
        (Language::En, "mrs_kind_bot") => "Project/group token".to_string(),
        (Language::De, "mrs_kind_bot") => "Projekt-/Gruppen-Token".to_string(),
        (Language::En, "mrs_kind_granular") => "Fine-grained token".to_string(),
        (Language::De, "mrs_kind_granular") => "Feingranulares Token".to_string(),
        (Language::En, "mrs_token_source") => "Stored in: {}".to_string(),
        (Language::De, "mrs_token_source") => "Gespeichert in: {}".to_string(),
        (Language::En, "mrs_src_keyring") => "Windows Credential Manager".to_string(),
        (Language::De, "mrs_src_keyring") => "Windows-Anmeldeinformationsverwaltung".to_string(),
        (Language::En, "mrs_src_config") => "glab config".to_string(),
        (Language::De, "mrs_src_config") => "glab-Konfiguration".to_string(),
        (Language::En, "mrs_src_env") => "Environment variable {}".to_string(),
        (Language::De, "mrs_src_env") => "Umgebungsvariable {}".to_string(),
        (Language::En, "mrs_expires") => "expires on {}".to_string(),
        (Language::De, "mrs_expires") => "läuft ab am {}".to_string(),
        (Language::En, "mrs_expires_soon") => "Expires in {} days".to_string(),
        (Language::De, "mrs_expires_soon") => "Läuft in {} Tagen ab".to_string(),
        (Language::En, "mrs_expires_soon_one") => "Expires in 1 day".to_string(),
        (Language::De, "mrs_expires_soon_one") => "Läuft in 1 Tag ab".to_string(),
        (Language::En, "mrs_scopes") => "Scopes:".to_string(),
        (Language::De, "mrs_scopes") => "Scopes:".to_string(),
        (Language::En, "mrs_check") => "Check".to_string(),
        (Language::De, "mrs_check") => "Prüfen".to_string(),
        (Language::En, "mrs_add_token") => "Add token …".to_string(),
        (Language::De, "mrs_add_token") => "Token hinzufügen …".to_string(),
        (Language::En, "mrs_replace_token") => "Replace token …".to_string(),
        (Language::De, "mrs_replace_token") => "Token ersetzen …".to_string(),
        (Language::En, "mrs_token_label") => "Token".to_string(),
        (Language::De, "mrs_token_label") => "Token".to_string(),
        (Language::En, "mrs_show_token") => "Show".to_string(),
        (Language::De, "mrs_show_token") => "Anzeigen".to_string(),
        (Language::En, "mrs_purpose") => "Purpose".to_string(),
        (Language::De, "mrs_purpose") => "Zweck".to_string(),
        (Language::En, "mrs_purpose_read") => "Read-only – read_api (enough for GitManager)".to_string(),
        (Language::De, "mrs_purpose_read") => "Nur lesen – read_api (reicht für GitManager)".to_string(),
        (Language::En, "mrs_purpose_full") => "Full – api, write_repository (also glab write commands)".to_string(),
        (Language::De, "mrs_purpose_full") => "Voll – api, write_repository (auch glab-Schreibbefehle)".to_string(),
        (Language::En, "mrs_create_token") => "Create token in GitLab".to_string(),
        (Language::De, "mrs_create_token") => "Token in GitLab erstellen".to_string(),
        (Language::En, "mrs_protocol") => "Protocol".to_string(),
        (Language::De, "mrs_protocol") => "Protokoll".to_string(),
        (Language::En, "mrs_token_note") => "The token is stored in glab and replaces the glab sign-in for {} – also for glab in the terminal.".to_string(),
        (Language::De, "mrs_token_note") => "Das Token wird in glab gespeichert und ersetzt die glab-Anmeldung für {} – auch für glab im Terminal.".to_string(),
        (Language::En, "mrs_http_warning") => "{} uses HTTP – the token is sent unencrypted.".to_string(),
        (Language::De, "mrs_http_warning") => "{} nutzt HTTP – das Token wird unverschlüsselt übertragen.".to_string(),
        (Language::En, "mrs_http_chip") => "http – unencrypted".to_string(),
        (Language::De, "mrs_http_chip") => "http – unverschlüsselt".to_string(),
        (Language::En, "mrs_token_empty") => "Please paste a token.".to_string(),
        (Language::De, "mrs_token_empty") => "Bitte ein Token einfügen.".to_string(),
        (Language::En, "mrs_token_whitespace") => "The token contains spaces or line breaks.".to_string(),
        (Language::De, "mrs_token_whitespace") => "Das Token enthält Leerzeichen oder Zeilenumbrüche.".to_string(),
        (Language::En, "mrs_token_invalid") => "The token is invalid.".to_string(),
        (Language::De, "mrs_token_invalid") => "Das Token ist ungültig.".to_string(),
        (Language::En, "mrs_verify") => "Verify".to_string(),
        (Language::De, "mrs_verify") => "Prüfen".to_string(),
        (Language::En, "mrs_preview") => "@{} · {} · expires {} · {}/{} repos accessible".to_string(),
        (Language::De, "mrs_preview") => "@{} · {} · läuft ab {} · {}/{} Repos zugreifbar".to_string(),
        (Language::En, "mrs_preview_no_expiry") => "@{} · {} · {}/{} repos accessible".to_string(),
        (Language::De, "mrs_preview_no_expiry") => "@{} · {} · {}/{} Repos zugreifbar".to_string(),
        (Language::En, "mrs_downgrade_warning") => "The current sign-in has api. With this token glab loses write access (mr approve/merge/note).".to_string(),
        (Language::De, "mrs_downgrade_warning") => "Die bisherige Anmeldung hat api. Mit diesem Token verliert glab Schreibrechte (mr approve/merge/note).".to_string(),
        (Language::En, "mrs_downgrade_confirm") => "Save anyway".to_string(),
        (Language::De, "mrs_downgrade_confirm") => "Trotzdem speichern".to_string(),
        (Language::En, "mrs_token_saved") => "Token saved for {}".to_string(),
        (Language::De, "mrs_token_saved") => "Token für {} gespeichert".to_string(),
        (Language::En, "mrs_token_failed") => "glab could not store the token: {}".to_string(),
        (Language::De, "mrs_token_failed") => "glab konnte das Token nicht speichern: {}".to_string(),
        (Language::En, "mrs_env_banner") => "The environment variable {} is set and overrides stored tokens for all hosts.".to_string(),
        (Language::De, "mrs_env_banner") => "Die Umgebungsvariable {} ist gesetzt und überschreibt gespeicherte Tokens für alle Hosts.".to_string(),
        (Language::En, "mrs_unrecognized") => "Remote hosts not signed in to glab ({})".to_string(),
        (Language::De, "mrs_unrecognized") => "Nicht bei glab angemeldete Remote-Hosts ({})".to_string(),
        (Language::En, "mrs_unrecognized_hint") => "GitManager sends no requests to these hosts until you add a token or add them as a GitLab host. You can ignore hosts that aren't GitLab (e.g. github.com).".to_string(),
        (Language::De, "mrs_unrecognized_hint") => "An diese Hosts schickt GitManager keine Anfragen, bis du ein Token hinzufügst oder sie als GitLab-Host einträgst. Hosts, die kein GitLab sind (z. B. github.com), kannst du ignorieren.".to_string(),
        (Language::En, "mrs_host_repos") => "{} repos".to_string(),
        (Language::De, "mrs_host_repos") => "{} Repos".to_string(),
        (Language::En, "mrs_host_repos_one") => "1 repo".to_string(),
        (Language::De, "mrs_host_repos_one") => "1 Repo".to_string(),
        (Language::En, "mrs_belongs_to") => "belongs to …".to_string(),
        (Language::De, "mrs_belongs_to") => "gehört zu …".to_string(),
        (Language::En, "mrs_sec_repos") => "Repositories for merge requests".to_string(),
        (Language::De, "mrs_sec_repos") => "Repositories für Merge Requests".to_string(),
        (Language::En, "mrs_included") => "{} of {} included · {} accessible".to_string(),
        (Language::De, "mrs_included") => "{} von {} einbezogen · {} zugreifbar".to_string(),
        (Language::En, "mrs_all") => "All".to_string(),
        (Language::De, "mrs_all") => "Alle".to_string(),
        (Language::En, "mrs_none") => "None".to_string(),
        (Language::De, "mrs_none") => "Keine".to_string(),
        (Language::En, "mrs_check_access") => "Check access".to_string(),
        (Language::De, "mrs_check_access") => "Zugriff prüfen".to_string(),
        (Language::En, "mrs_col_repo") => "Repository".to_string(),
        (Language::De, "mrs_col_repo") => "Repository".to_string(),
        (Language::En, "mrs_col_clones") => "Clones".to_string(),
        (Language::De, "mrs_col_clones") => "Klone".to_string(),
        (Language::En, "mrs_col_project") => "GitLab project".to_string(),
        (Language::De, "mrs_col_project") => "GitLab-Projekt".to_string(),
        (Language::En, "mrs_col_access") => "Access".to_string(),
        (Language::De, "mrs_col_access") => "Zugriff".to_string(),
        (Language::En, "mrs_clones") => "{} clones".to_string(),
        (Language::De, "mrs_clones") => "{} Klone".to_string(),
        (Language::En, "mrs_acc_ok") => "Access OK ({})".to_string(),
        (Language::De, "mrs_acc_ok") => "Zugriff ok ({})".to_string(),
        (Language::En, "mrs_acc_ok_plain") => "Access OK".to_string(),
        (Language::De, "mrs_acc_ok_plain") => "Zugriff ok".to_string(),
        (Language::En, "mrs_acc_public") => "public, not a member".to_string(),
        (Language::De, "mrs_acc_public") => "öffentlich, kein Mitglied".to_string(),
        (Language::En, "mrs_acc_renamed") => "renamed → {}".to_string(),
        (Language::De, "mrs_acc_renamed") => "umbenannt → {}".to_string(),
        (Language::En, "mrs_acc_denied") => "no access with this token".to_string(),
        (Language::De, "mrs_acc_denied") => "kein Zugriff mit diesem Token".to_string(),
        (Language::En, "mrs_acc_mr_off") => "MRs disabled in project".to_string(),
        (Language::De, "mrs_acc_mr_off") => "MRs im Projekt deaktiviert".to_string(),
        (Language::En, "mrs_acc_archived") => "archived".to_string(),
        (Language::De, "mrs_acc_archived") => "archiviert".to_string(),
        (Language::En, "mrs_acc_host_auth") => "Host not signed in".to_string(),
        (Language::De, "mrs_acc_host_auth") => "Host nicht angemeldet".to_string(),
        (Language::En, "mrs_acc_checking") => "Checking …".to_string(),
        (Language::De, "mrs_acc_checking") => "Prüfe …".to_string(),
        (Language::En, "mrs_acc_skipped") => "skipped (no access)".to_string(),
        (Language::De, "mrs_acc_skipped") => "wird übersprungen (kein Zugriff)".to_string(),
        (Language::En, "mrs_include_nonlocal") => "Also show MRs from projects without a local clone".to_string(),
        (Language::De, "mrs_include_nonlocal") => "Auch MRs aus Projekten ohne lokalen Klon anzeigen".to_string(),
        (Language::En, "mrs_sec_checkout") => "Checkout".to_string(),
        (Language::De, "mrs_sec_checkout") => "Auschecken".to_string(),
        (Language::En, "mrs_review_root") => "Folder for reviews (MRs from others)".to_string(),
        (Language::De, "mrs_review_root") => "Ordner für Reviews (MRs von anderen)".to_string(),
        (Language::En, "mrs_review_root_ok") => "Inside search path {} – new worktrees appear in the repository list.".to_string(),
        (Language::De, "mrs_review_root_ok") => "Liegt im Suchpfad {} – neue Worktrees erscheinen in der Repo-Liste.".to_string(),
        (Language::En, "mrs_review_root_warn") => "Not inside a search path – new worktrees won't appear in the repository list.".to_string(),
        (Language::De, "mrs_review_root_warn") => "Liegt in keinem Suchpfad – neue Worktrees erscheinen nicht in der Repo-Liste.".to_string(),
        (Language::En, "mrs_review_root_relative") => "Not an absolute path – ignored, new worktrees go next to the base clone.".to_string(),
        (Language::De, "mrs_review_root_relative") => "Kein absoluter Pfad – wird ignoriert, neue Worktrees landen neben dem Basis-Klon.".to_string(),
        (Language::En, "mrs_add_search_path") => "Add as search path".to_string(),
        (Language::De, "mrs_add_search_path") => "Als Suchpfad hinzufügen".to_string(),
        (Language::En, "mrs_folder_name") => "Folder name".to_string(),
        (Language::De, "mrs_folder_name") => "Ordnername".to_string(),
        (Language::En, "mrs_folder_name_example") => "e.g. {}".to_string(),
        (Language::De, "mrs_folder_name_example") => "z. B. {}".to_string(),
        (Language::En, "mrs_after_checkout") => "Open after checkout".to_string(),
        (Language::De, "mrs_after_checkout") => "Nach dem Auschecken öffnen".to_string(),
        (Language::En, "mrs_after_last") => "Last choice".to_string(),
        (Language::De, "mrs_after_last") => "Zuletzt gewählt".to_string(),
        (Language::En, "mrs_after_last_hint") => "last: {}".to_string(),
        (Language::De, "mrs_after_last_hint") => "zuletzt: {}".to_string(),
        (Language::En, "mrs_sec_display") => "Display & refresh".to_string(),
        (Language::De, "mrs_sec_display") => "Anzeige & Aktualisierung".to_string(),
        (Language::En, "mrs_refresh") => "Refresh automatically".to_string(),
        (Language::De, "mrs_refresh") => "Automatisch aktualisieren".to_string(),
        (Language::En, "mrs_refresh_unit") => "min (0 = off)".to_string(),
        (Language::De, "mrs_refresh_unit") => "Min. (0 = aus)".to_string(),
        (Language::En, "mrs_show_all_default") => "“All open” on by default".to_string(),
        (Language::De, "mrs_show_all_default") => "„Alle offenen“ standardmäßig aktiv".to_string(),
        (Language::En, "mrs_ignore_bots") => "Don't count bot approvals".to_string(),
        (Language::De, "mrs_ignore_bots") => "Approvals von Bots nicht mitzählen".to_string(),
        (Language::En, "mrs_check_finished") => "Check for {} finished".to_string(),
        (Language::De, "mrs_check_finished") => "Prüfung für {} abgeschlossen".to_string(),
        (Language::En, "mrs_belongs_to_tt") => "Another name (e.g. SSH host) for a signed-in GitLab host".to_string(),
        (Language::De, "mrs_belongs_to_tt") => "Anderer Name (z. B. SSH-Host) für einen angemeldeten GitLab-Host".to_string(),
        (Language::En, "mrs_alias_apply") => "Assign".to_string(),
        (Language::De, "mrs_alias_apply") => "Zuordnen".to_string(),
        (Language::En, "mrs_alias_of") => "{} belongs to this host".to_string(),
        (Language::De, "mrs_alias_of") => "{} gehört zu diesem Host".to_string(),
        (Language::En, "mrs_alias_remove") => "Remove mapping".to_string(),
        (Language::De, "mrs_alias_remove") => "Zuordnung entfernen".to_string(),
        (Language::En, "mrs_glab_pending") => "Applies after “Save” – until then GitManager uses {}.".to_string(),
        (Language::De, "mrs_glab_pending") => "Gilt nach „Speichern“ – bis dahin nutzt GitManager {}.".to_string(),

        // ── Relative Zeiten ──────────────────────────────────────────
        (Language::En, "rel_now") => "just now".to_string(),
        (Language::De, "rel_now") => "gerade eben".to_string(),
        (Language::En, "rel_min") => "{} min ago".to_string(),
        (Language::De, "rel_min") => "vor {} Min.".to_string(),
        (Language::En, "rel_hour") => "{} h ago".to_string(),
        (Language::De, "rel_hour") => "vor {} Std.".to_string(),
        (Language::En, "rel_yesterday") => "yesterday".to_string(),
        (Language::De, "rel_yesterday") => "gestern".to_string(),
        (Language::En, "rel_days") => "{} days ago".to_string(),
        (Language::De, "rel_days") => "vor {} Tagen".to_string(),

        // fallback for any missing key -> return key itself
        (Language::En, _) => key.to_string(),
        (Language::De, _) => key.to_string(),
    }
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
            "mr_uninvolved_h",
            "mr_uninvolved_sub",
            "mr_uninvolved_sub_one",
            "mr_shown_of",
            "mr_load_more",
            "mr_loading_uninvolved",
            "mr_uninvolved_not_loaded",
            "mr_load_now",
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
            "mrs_check_finished",
            "mrs_belongs_to_tt",
            "mrs_alias_apply",
            "mrs_alias_of",
            "mrs_alias_remove",
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
