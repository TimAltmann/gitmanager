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
        (Language::En, "profile_override_tooltip") => "Override language profile for this repo".to_string(),
        (Language::De, "profile_override_tooltip") => "Sprach-Profil für dieses Repo überschreiben".to_string(),
        (Language::En, "global") => "— Global —".to_string(),
        (Language::De, "global") => "— Global —".to_string(),
        (Language::En, "open_in_explorer") => "Show in Explorer".to_string(),
        (Language::De, "open_in_explorer") => "Im Explorer öffnen".to_string(),
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
        (Language::En, "fetching_branches") => "Fetching branches for {}...".to_string(),
        (Language::De, "fetching_branches") => "Fetche Branches für {}...".to_string(),
        (Language::En, "branch_switched_to") => "Branch switched to '{}': {}".to_string(),
        (Language::De, "branch_switched_to") => "Branch zu '{}' gewechselt: {}".to_string(),
        (Language::En, "ide_not_found") => "IDE '{}' not found".to_string(),
        (Language::De, "ide_not_found") => "IDE '{}' nicht in Profil '{}' gefunden".to_string(),
        (Language::En, "agent_not_found") => "AI agent '{}' not found".to_string(),
        (Language::De, "agent_not_found") => "AI-Agent '{}' nicht gefunden".to_string(),

        // fallback for any missing key -> return key itself
        (Language::En, _) => key.to_string(),
        (Language::De, _) => key.to_string(),
    }
}

/// Format helper: replaces `{}` placeholders sequentially.
#[allow(dead_code)]
pub fn tr_fmt(lang: Language, key: &str, args: &[&str]) -> String {
    let mut s = tr(lang, key);
    for arg in args {
        if let Some(pos) = s.find("{}") {
            s.replace_range(pos..pos + 2, arg);
        } else {
            break;
        }
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
    }

    #[test]
    fn tr_fallback_returns_key() {
        assert_eq!(
            tr(Language::En, "nonexistent_key_xyz"),
            "nonexistent_key_xyz"
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
