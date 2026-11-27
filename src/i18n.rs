use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

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

/// Alle Texte: (Schlüssel, Englisch, Deutsch). Schlüssel sind eindeutig
/// (Test `texts_keys_unique`).
const TEXTS: &[(&str, &str, &str)] = &[
    // ── App / Top Bar ──────────────────────────────────────────────
    ("app_title", "GitManager", "GitManager"),
    ("profile_label", "Profile:", "Profil:"),
    ("ai_label", "AI:", "AI:"),
    ("settings", "Settings", "Einstellungen"),
    ("settings_tooltip", "Configure paths, profiles, agents & depth", "Pfade, Profile, Agents & Tiefe konfigurieren"),
    ("refresh", "Refresh", "Aktualisieren"),
    ("scanning", "Scanning...", "Scanne..."),
    ("search_paths", "Search paths:", "Suchpfade:"),
    ("depth_label", "Depth:", "Tiefe:"),
    ("not_found", "(not found)", "(nicht gefunden)"),
    ("language", "Language", "Sprache"),
    ("error_prefix", "Error", "Fehler"),
    ("config_load_failed", "Settings could not be read, defaults are in use: {}", "Einstellungen nicht lesbar, es gelten die Standardwerte: {}"),
    ("launch_ide_failed", "Could not start IDE '{}'", "IDE '{}' konnte nicht gestartet werden"),
    ("launch_agent_failed", "Could not start agent '{}'", "Agent '{}' konnte nicht gestartet werden"),
    ("launch_explorer_failed", "Could not open the file manager", "Explorer konnte nicht geöffnet werden"),
    ("launch_shell_failed", "Could not open the shell", "Shell konnte nicht geöffnet werden"),
    ("agents_active_count", "{} active", "{} aktiv"),
    ("agents_enable_all", "Enable all", "Alle aktivieren"),
    ("agents_disable_all", "Disable all", "Alle deaktivieren"),

    // ── Status Bar ─────────────────────────────────────────────────
    ("status_repos_paths", "Repos", "Repos"),
    ("status_paths", "paths", "Pfade"),
    ("scanning_repos", "Scanning for repositories...", "Scanne nach Repositories..."),
    ("no_search_path", "No search path configured. Open settings and add a folder.", "Kein Suchpfad konfiguriert. Öffne die Einstellungen und füge einen Ordner hinzu."),

    // ── Branch dialog ──────────────────────────────────────────────
    ("branch_switch_title", "Switch branch", "Branch wechseln"),
    ("branch_dirty_msg", "Uncommitted changes present. How to proceed?", "Uncommitted changes vorhanden. Wie fortfahren?"),
    ("affected_files", "affected files:", "betroffene Dateien:"),
    ("cancel", "Cancel", "Abbrechen"),
    ("stash_switch", "Stash & Switch", "Stash & Switch"),
    ("stash_switch_tooltip", "Stash changes, switch branch, then pop", "Änderungen stashen, Branch wechseln, stash pop"),
    ("force_discard", "Force Discard", "Force Discard"),
    ("force_discard_tooltip", "Discard local changes and switch (dangerous)", "Lokale Änderungen verwerfen und wechseln (gefährlich)"),
    ("branch_tip", "Tip: 'Stash & Switch' keeps changes, 'Force' discards them.", "Tipp: 'Stash & Switch' behält Änderungen, 'Force' verwirft sie."),

    // ── Branch-Wechsel auf Remote-Branch (E15) ─────────────────────
    ("branch_local_ahead", "Local branch {} is {} commits ahead – local state checked out", "Lokaler Branch {} ist {} Commits voraus – lokaler Stand ausgecheckt"),
    ("branch_local_ahead_one", "Local branch {} is 1 commit ahead – local state checked out", "Lokaler Branch {} ist 1 Commit voraus – lokaler Stand ausgecheckt"),
    ("branch_local_diverged", "Local branch {} differs ({} ahead, {} behind) – local state checked out", "Lokaler Branch {} weicht ab ({} voraus, {} hinterher) – lokaler Stand ausgecheckt"),
    ("branch_fast_forwarded", "Fast-forwarded local branch {} by {} commits", "Lokaler Branch {} um {} Commits vorgespult"),
    ("branch_fast_forwarded_one", "Fast-forwarded local branch {} by 1 commit", "Lokaler Branch {} um 1 Commit vorgespult"),
    ("branch_upstream_mismatch", "Local branch {} tracks {}", "Lokaler Branch {} verfolgt {}"),
    ("branch_checked_out_elsewhere", "Branch {} is checked out in {}", "Branch {} ist in {} ausgecheckt"),
    ("branch_switched", "Switched to branch “{}”: {}", "Branch zu „{}“ gewechselt: {}"),
    ("branch_switching", "{}: switching to “{}” …", "{}: wechsle zu „{}“ …"),
    ("branch_switch_failed", "Branch switch failed: {}", "Branch-Wechsel fehlgeschlagen: {}"),
    ("branch_switch_blocked", "Cannot switch to branch “{}”: merge conflicts or a merge/rebase in progress ({})", "Branch-Wechsel zu „{}“ nicht möglich: Merge-Konflikte oder Merge/Rebase läuft ({})"),

    // ── Repo list ──────────────────────────────────────────────────
    ("no_repos_found", "No repositories found", "Keine Repositories gefunden"),
    ("no_repos_hint", "Check the path in settings and increase search depth if needed.", "Pfad in den Einstellungen prüfen und ggf. Suchtiefe erhöhen."),
    ("dirty_tooltip", "Uncommitted changes present", "Uncommitted changes vorhanden"),
    ("clean_tooltip", "Working directory clean", "Arbeitsverzeichnis sauber"),
    // ── Claude Sessions ───────────────────────────────────────────
    ("view_claude", "Claude Sessions", "Claude Sessions"),
    ("view_claude_tt", "Claude Code sessions in your repos and worktrees", "Claude-Code-Sessions in deinen Repos und Worktrees"),
    ("claude_search_hint", "Search sessions (Ctrl+F)", "Sessions durchsuchen (Strg+F)"),
    ("claude_none_found", "No Claude sessions found", "Keine Claude Sessions gefunden"),
    ("claude_none_hint", "Looking in {} for sessions inside your search paths.", "Gesucht wird in {} nach Sessions in deinen Suchpfaden."),
    ("claude_sln_searching", "Searching for solutions…", "Suche Solutions…"),
    ("claude_no_matches", "No matches for “{}”", "Keine Treffer für „{}“"),
    ("claude_resume", "Resume", "Fortsetzen"),
    ("claude_resume_tt", "Resume session in {} with Claude Code", "Session in {} mit Claude Code fortsetzen"),
    ("claude_scanning", "Searching Claude sessions…", "Suche Claude Sessions…"),
    ("claude_status", "{} open · {} sessions", "{} offen · {} Sessions"),
    ("claude_open_section", "Open ({})", "Offen ({})"),
    ("claude_all_section", "Sessions ({})", "Sessions ({})"),
    ("claude_st_waiting", "Waiting for you", "Wartet auf dich"),
    ("claude_st_idle", "Done", "Fertig"),
    ("claude_st_shell", "Running command…", "Führt Befehl aus…"),
    ("claude_focus_tt", "Bring terminal to front", "Terminal in den Vordergrund holen"),
    ("claude_sln_fav_add", "Prefer this solution (preselected next time)", "Bevorzugen (nächstes Mal vorausgewählt)"),
    ("claude_sln_fav_remove", "No longer prefer", "Nicht mehr bevorzugen"),
    ("claude_sln_folder", "Folder only", "Nur Ordner"),
    ("claude_focus_failed", "Terminal window not found", "Terminal-Fenster nicht gefunden"),
    ("claude_waiting_tt", "{} waiting for you", "{} warten auf dich"),
    // ── Repo-Liste: Suche & Worktrees ─────────────────────────────
    ("repo_search_hint", "Search repos, branches, paths… (Ctrl+F)", "Repos, Branches, Pfade durchsuchen… (Strg+F)"),
    ("repo_search_clear", "Clear search", "Suche leeren"),
    ("repo_search_no_matches", "No repositories or worktrees match “{}”", "Keine Repositories oder Worktrees passen zu „{}“"),
    ("repo_section_repos", "Repositories", "Repositories"),
    ("repo_section_worktrees", "Worktrees", "Worktrees"),
    ("claude_notify_label", "Notify when a Claude session needs you", "Benachrichtigen, wenn eine Claude Session dich braucht"),
    ("claude_notify_hint", "Windows notification when a running session waits for an answer or has finished. Click it to bring the terminal to the front.", "Windows-Benachrichtigung, wenn eine laufende Session auf eine Antwort wartet oder fertig ist. Ein Klick darauf holt das Terminal nach vorne."),
    ("claude_notify_waiting", "Claude is waiting for you", "Claude wartet auf dich"),
    ("claude_notify_done", "Claude has finished", "Claude ist fertig"),
    ("claude_notify_btn_terminal", "Open terminal", "Terminal öffnen"),
    ("claude_notify_btn_show", "Show in GitManager", "In GitManager anzeigen"),
    ("claude_notify_attr_done", "done", "fertig"),
    ("claude_notify_attr_done_after", "done after {}", "fertig nach {}"),
    ("claude_notify_attr_waiting", "waiting for you", "wartet auf dich"),
    ("claude_notify_attr_waiting_after", "question after {}", "Rückfrage nach {}"),
    ("claude_preview_show", "Show recent messages", "Letzte Nachrichten zeigen"),
    ("claude_preview_hide", "Hide recent messages", "Letzte Nachrichten ausblenden"),
    ("claude_preview_empty", "No messages found.", "Keine Nachrichten gefunden."),
    ("claude_preview_you", "You", "Du"),
    ("claude_new_button", "New Claude session", "Neue Claude Session"),
    ("claude_new_tooltip", "Start a new Claude Code session in a repository", "Neue Claude-Code-Session in einem Repository starten"),
    ("claude_new_title", "New Claude session", "Neue Claude Session"),
    ("claude_new_hint", "Choose the folder the session starts in.", "Wähle den Ordner, in dem die Session startet."),
    ("claude_new_search_hint", "Search repositories and worktrees…", "Repositories und Worktrees durchsuchen…"),
    ("claude_new_no_matches", "No folder matches the search.", "Kein Ordner passt zur Suche."),
    ("claude_new_no_folders", "No repositories found yet – set the search paths or a default folder in the settings.", "Noch keine Repositories gefunden – Suchpfade oder einen Standard-Ordner in den Einstellungen festlegen."),
    ("claude_new_start", "Start", "Starten"),
    ("claude_new_default_badge", "default folder", "Standard-Ordner"),
    ("folder_files_label", "Files for new worktrees and Claude sessions", "Dateien für neue Worktrees und Claude Sessions"),
    ("folder_files_hint", "Created in a new worktree and in the folder of a new Claude session – only if the file does not exist there yet. Path relative to the folder, e.g. dev_config.xml or config/local.json.", "Werden in einem neuen Worktree und im Ordner einer neuen Claude Session angelegt – nur, wenn die Datei dort noch fehlt. Pfad relativ zum Ordner, z. B. dev_config.xml oder config/local.json."),
    ("folder_files_path", "Path", "Pfad"),
    ("folder_files_path_hint", "e.g. dev_config.xml", "z. B. dev_config.xml"),
    ("folder_files_content", "Content", "Inhalt"),
    ("folder_files_add", "+ Add file", "+ Datei hinzufügen"),
    ("folder_files_remove", "Remove", "Entfernen"),
    ("folder_files_invalid", "Path must stay inside the folder (relative, without ..)", "Pfad muss im Ordner bleiben (relativ, ohne ..)"),
    ("folder_files_written", "created: {}", "angelegt: {}"),
    ("folder_files_failed", "{} not created: {}", "{} nicht angelegt: {}"),
    ("claude_dir_label", "Default folder for new Claude sessions", "Standard-Ordner für neue Claude Sessions"),
    ("claude_dir_hint", "Preselected in “New Claude session”. Empty = last used repository.", "Wird bei „Neue Claude Session“ vorausgewählt. Leer = zuletzt benutztes Repository."),
    ("claude_dir_placeholder", "last used repository", "zuletzt benutztes Repository"),
    ("wt_new_button", "New worktree", "Neuer Worktree"),
    ("wt_new_tooltip", "Create a new worktree for a repository", "Neuen Worktree für ein Repository anlegen"),
    ("wt_of_repo", "worktree of {}", "Worktree von {}"),
    ("wt_no_base_repo", "No repository found to create a worktree from", "Kein Repository gefunden, von dem ein Worktree angelegt werden kann"),
    ("wt_dialog_title", "New worktree", "Neuer Worktree"),
    ("wt_repo_label", "Repository:", "Repository:"),
    ("wt_branch_label", "Branch:", "Branch:"),
    ("wt_mode_existing", "Existing branch", "Vorhandener Branch"),
    ("wt_mode_new", "New branch", "Neuer Branch"),
    ("wt_new_branch_name", "Name:", "Name:"),
    ("wt_start_point", "Based on:", "Basierend auf:"),
    ("wt_start_head", "Current HEAD", "Aktueller HEAD"),
    ("wt_branch_in_use", "Already checked out in {}", "Bereits ausgecheckt in {}"),
    ("wt_target_label", "Folder:", "Ordner:"),
    ("wt_target_suggest", "Suggestion", "Vorschlag"),
    ("wt_target_suggest_tooltip", "Use the suggested folder again", "Wieder den vorgeschlagenen Ordner verwenden"),
    ("wt_default_root_hint", "Default folder: {} (Settings › General)", "Standard-Ordner: {} (Einstellungen › Allgemein)"),
    ("wt_default_next_to_repo_hint", "Default: next to the repository (change in Settings › General)", "Standard: neben dem Repository (änderbar unter Einstellungen › Allgemein)"),
    ("wt_outside_search_paths", "This folder is outside the search paths: the worktree is created but not shown in the list.", "Dieser Ordner liegt außerhalb der Suchpfade: Der Worktree wird angelegt, erscheint aber nicht in der Liste."),
    ("wt_create", "Create worktree", "Worktree anlegen"),
    ("wt_creating", "Creating worktree…", "Worktree wird angelegt…"),
    ("rm_wt_title", "Remove worktree", "Worktree entfernen"),
    ("rm_wt_question", "Remove worktree {}?", "Worktree {} entfernen?"),
    ("rm_wt_delete_branch", "Delete local branch {}", "Lokalen Branch {} löschen"),
    ("rm_wt_branch_unsafe", "Branch is not fully pushed or merged", "Branch ist weder vollständig gepusht noch gemergt"),
    ("rm_wt_dirty", "{} uncommitted changes", "{} ungespeicherte Änderungen"),
    ("rm_wt_ahead", "{} unpushed commits", "{} ungepushte Commits"),
    ("rm_wt_detached", "Detached HEAD with commits that no branch contains", "Losgelöster HEAD mit Commits, die in keinem Branch liegen"),
    ("rm_wt_remove", "Remove", "Entfernen"),
    ("rm_wt_remove_anyway", "Remove anyway", "Trotzdem entfernen"),
    ("rm_wt_claude", "A Claude session is running in this folder", "Eine Claude Session läuft in diesem Ordner"),
    ("rm_wt_running", "Removing worktree…", "Worktree wird entfernt…"),
    ("rm_wt_changed", "State has changed – please confirm again", "Zustand hat sich geändert – bitte erneut bestätigen"),
    ("rm_wt_done", "Worktree {} removed", "Worktree {} entfernt"),
    ("rm_wt_checking", "Checking branch and commits…", "Prüfe Branch und Commits…"),
    ("rm_wt_branch_kept", "Worktree {} removed; branch {} was kept: {}", "Worktree {} entfernt; Branch {} blieb erhalten: {}"),
    ("rm_wt_branch_changed", "it changed in the meantime", "er hat sich inzwischen geändert"),
    ("rm_wt_failed", "Removing worktree {} failed: {}", "Worktree {} entfernen fehlgeschlagen: {}"),
    ("cleanup_title", "Clean up", "Aufräumen"),
    ("cleanup_scanning", "Fetching and looking for things to clean up…", "Fetch läuft, suche nach Aufräumbarem…"),
    ("cleanup_nothing", "Nothing to clean up", "Nichts aufzuräumen"),
    ("cleanup_scan_failed", "The search was aborted", "Die Suche wurde abgebrochen"),
    ("cleanup_fetch_failed", "Fetch failed for: {} – deleted remote branches may be missing there", "Fetch fehlgeschlagen für: {} – gelöschte Remote-Branches fehlen dort eventuell"),
    ("cleanup_gitlab_unavailable", "GitLab not available – merged merge requests were not checked", "GitLab nicht verfügbar – gemergte Merge Requests wurden nicht geprüft"),
    ("cleanup_reason_mr", "MR !{} merged", "MR !{} gemergt"),
    ("cleanup_open_mr", "Open MR !{}", "Offener MR !{}"),
    ("cleanup_reason_gone", "Remote branch deleted", "Remote-Branch gelöscht"),
    ("cleanup_dirty", "{} changes", "{} Änderungen"),
    ("cleanup_ahead", "↑{} unpushed", "↑{} ungepusht"),
    ("cleanup_branch_unmerged", "not merged – local commits will be lost", "nicht gemergt – lokale Commits gehen verloren"),
    ("cleanup_branch_in", "in {}", "in {}"),
    ("cleanup_delete_branch", "Delete branch too", "Branch mitlöschen"),
    ("cleanup_remove_n", "Remove {}", "{} entfernen"),
    ("cleanup_running", "Cleaning up…", "Räume auf…"),
    ("cleanup_ok", "removed", "entfernt"),
    ("cleanup_changed", "State has changed – skipped", "Zustand hat sich geändert – übersprungen"),
    ("cleanup_branch_kept", "worktree removed; branch changed meanwhile and was kept", "Worktree entfernt; Branch hat sich geändert und wurde behalten"),
    ("cleanup_failed", "Error: {}", "Fehler: {}"),
    ("pull_running", "Pulling…", "Pull läuft…"),
    ("pull_ff", "{}: pulled {} commits", "{}: {} Commits übernommen"),
    ("pull_ff_one", "{}: pulled 1 commit", "{}: 1 Commit übernommen"),
    ("pull_up_to_date", "{}: already up to date", "{}: schon aktuell"),
    ("pull_dirty", "{}: {} local changes – commit or stash first", "{}: {} lokale Änderungen – erst committen oder stashen"),
    ("pull_diverged", "{}: branch has diverged (↑{} ↓{}) – please resolve manually", "{}: Branch ist divergiert (↑{} ↓{}) – bitte manuell lösen"),
    ("pull_no_upstream", "{}: no upstream branch configured", "{}: kein Upstream-Branch eingerichtet"),
    ("pull_detached", "{}: detached HEAD – nothing to pull", "{}: losgelöster HEAD – nichts zu pullen"),
    ("pull_changed", "{}: branch changed while fetching – nothing changed", "{}: Branch hat sich während des Abrufs geändert – nichts geändert"),
    ("pull_failed", "Pull failed: {}", "Pull fehlgeschlagen: {}"),
    ("wt_wait_busy", "Waiting for the running scan/fetch…", "Warte auf laufenden Scan/Fetch…"),
    ("wt_target_not_absolute", "Please enter an absolute folder path.", "Bitte einen absoluten Ordnerpfad angeben."),
    ("wt_invalid_branch", "Invalid branch name: {}", "Ungültiger Branch-Name: {}"),
    ("wt_branch_exists", "Branch {} already exists – choose it under “Existing branch”.", "Branch {} existiert bereits – unter „Vorhandener Branch“ auswählen."),
    ("wt_created", "Worktree for {} created in {}", "Worktree für {} in {} angelegt"),
    ("wt_root_label", "Default folder for new worktrees", "Standard-Ordner für neue Worktrees"),
    ("wt_root_hint", "Suggested when creating a worktree from the repository list. Empty = next to the repository.", "Wird beim Anlegen eines Worktrees aus der Repo-Liste vorgeschlagen. Leer = neben dem Repository."),
    ("wt_root_placeholder", "next to the repository", "neben dem Repository"),
    ("wt_root_reset", "Reset", "Zurücksetzen"),
    ("wt_root_relative", "Relative path – ignored, please enter an absolute path.", "Relativer Pfad – wird ignoriert, bitte einen absoluten Pfad angeben."),
    ("search_label", "Search:", "Suche:"),
    ("filter_hint", "filter...", "filtern..."),
    ("branch_search", "Search branches...", "Branches suchen..."),
    ("branch_refresh_tooltip", "Reload branches (git fetch --all)", "Branches neu laden (git fetch --all)"),
    ("current", "Current:", "Aktuell:"),
    ("no_branches", "No branches found – try fetch", "Keine Branches gefunden – evtl. fetch nötig"),
    ("fetch_now", "Fetch now", "Jetzt fetchen"),
    ("no_matches", "No matches", "Keine Treffer"),
    ("branch_switch_tooltip", "Switch branch – type to filter, ↻ to fetch", "Branch wechseln – tippe zum Filtern, ↻ für fetch"),
    ("detached_tooltip", "Detached HEAD – commit checked out, not a branch", "Detached HEAD – Commit ausgecheckt, kein Branch"),
    ("profile_override_tooltip", "Override language profile for this repo", "Sprach-Profil für dieses Repo überschreiben"),
    ("open_in_explorer", "Show in Explorer", "Im Explorer öffnen"),
    ("open_in_shell", "Open shell", "Shell öffnen"),
    ("open_in", "Open in", "Mit"),
    ("tray_no_ide", "No IDE", "Keine IDE"),
    ("open_in_terminal", "Open in {} (Terminal)", "In {} öffnen (Terminal)"),
    ("no_solution", "No {}", "Keine {}"), // formatted with extension
    ("selected", "Selected: {}", "Ausgewählt: {}"),

    // ── Settings General ───────────────────────────────────────────
    ("add", "Add", "Hinzufügen"),
    ("remove", "Remove", "Entfernen"),
    ("browse", "Browse", "Durchsuchen"),

    // ── Settings Profiles ──────────────────────────────────────────
    ("profiles_count", "Profiles ({}):", "Profile ({}):"),
    ("set_active", "Set active", "Aktiv setzen"),
    ("name", "Name:", "Name:"),
    ("id", "ID:", "ID:"),
    ("shell", "Shell:", "Shell:"),
    ("none", "None", "Keine"),
    ("active", "Active", "Aktiv"),




    // ── Settings common ────────────────────────────────────────────
    ("close", "Close", "Schließen"),
    ("save", "Save", "Speichern"),
    ("reset", "Reset", "Zurücksetzen"),
    ("config_path", "Config:", "Config:"),
    ("tabs_general", "General", "Allgemein"),
    ("tabs_profiles", "Languages/Profiles", "Sprachen/Profile"),
    ("tabs_agents", "AI Agents", "AI Agents"),
    ("tabs_terminal", "Terminal", "Terminal"),
    ("tabs_appearance", "Appearance", "Erscheinungsbild"),
    ("tabs_language", "Language", "Sprache"),
    ("tabs_icons", "Icons", "Icons"),
    ("tabs_tray_icons", "Tray Icons", "Tray Icons"),
    ("tray_icons_title", "Tray Icons & Popup", "Tray Icons & Popup"),
    ("tray_behavior_title", "System Tray Behavior", "System Tray Verhalten"),
    ("tray_minimize_label", "Minimize to tray on close (instead of quit)", "Beim Schließen in Tray minimieren (statt beenden)"),
    ("tray_minimize_hint_on", "✓ Window is hidden on close and keeps running in tray (left/right click: custom menu).", "✓ Das Fenster wird beim Schließen ausgeblendet und läuft im Tray weiter (Links-/Rechts-Klick: eigenes Menü)."),
    ("tray_minimize_hint_off", "Window quits on close.", "Das Fenster wird beim Schließen beendet."),
    ("tray_update_label", "Check for updates on startup (GitHub Releases)", "Beim Start auf Updates prüfen (GitHub Releases)"),
    ("tray_update_hint_on", "✓ api.github.com is queried once on startup (5 s timeout). Disable for offline/corporate networks. Takes effect after restart.", "✓ Beim Start wird einmalig api.github.com abgefragt (5 s Timeout). Deaktivieren für Offline-/Firmennetze. Änderung wirkt ab Neustart."),
    ("tray_update_hint_off", "Update check disabled — no network request on startup. Takes effect after restart.", "Update-Check deaktiviert — keine Netzwerkabfrage beim Start. Änderung wirkt ab Neustart."),
    ("autostart_label", "Start automatically with Windows", "Automatisch mit Windows starten"),
    ("autostart_hint", "Adds/removes GitManager in Windows Autostart (Registry Run key). Takes effect on next login.", "Trägt GitManager in den Windows-Autostart ein (Registry Run-Key). Wirkt ab der nächsten Anmeldung."),
    ("tray_update_error", "Last update check failed: {}", "Letzter Update-Check fehlgeschlagen: {}"),
    ("tray_branch_limit_title", "Tray Branch Limit", "Tray Branch-Limit"),
    ("tray_branch_limit_desc", "Max branches shown in tray popup dropdown (5–50, default 20).", "Wie viele Branches maximal im Tray-Popup Dropdown angezeigt werden (5–50, Standard 20)."),
    ("tray_branch_limit_summary", "Shows up to {} branches in the tray popup. More in the main window.", "Zeigt bis zu {} Branches im Tray-Popup. Weitere im Hauptfenster."),
    ("tray_creation_error", "Tray icon creation failed – tray popup disabled.", "Tray-Icon konnte nicht erstellt werden – Tray-Popup deaktiviert."),
    ("tray_all_hidden_warn", "Warning: all tray icons hidden – no icons will be shown in the tray row.", "Warnung: alle Tray-Icons ausgeblendet – es werden keine Icons in der Tray-Zeile angezeigt."),
    ("tray_limit_label", "Limit:", "Limit:"),
    ("tray_direct_label", "Or direct:", "Oder direkt:"),
    ("tray_branch_more_filtered", "+ {} more (filter)", "+ {} weitere (filtere)"),
    ("tray_branch_more_main", "+ {} more (in main window)", "+ {} weitere (in Hauptfenster)"),
    ("tray_notice_branch_dialog", "Branch switch for '{}' waits for a decision – click to open", "Branch-Wechsel für '{}' wartet auf Entscheidung – klicken zum Öffnen"),
    ("tray_notice_error", "Note in main window – click to open", "Hinweis im Hauptfenster – klicken zum Öffnen"),
    ("update_available_title", "Update available", "Update verfügbar"),
    ("update_available_msg", "New version available", "Neue Version verfügbar"),
    ("open_link", "Open", "Öffnen"),
    ("tray_icons_desc", "Reorder, hide and limit tray popup icons. Hidden icons disappear from tray rows. Max display limits visible repos.", "Tray-Popup Icons neu anordnen, ausblenden und limitieren. Ausgeblendete Icons verschwinden aus den Tray-Zeilen. Max Anzeige limitiert sichtbare Repos."),
    ("tray_max_display", "Max repos in tray popup", "Max Repos im Tray-Popup"),
    ("icons_title", "Icon order & visibility (per profile)", "Icon-Reihenfolge & Sichtbarkeit (pro Profil)"),
    ("icons_desc", "Reorder and hide IDE / terminal / AI icons. Hidden icons disappear from repo list without gaps.", "Icons für IDE / Terminal / AI neu anordnen und ausblenden. Ausgeblendete Icons verschwinden ohne Lücke."),
    ("icons_profile", "Profile:", "Profil:"),
    ("icons_ides", "IDEs", "IDEs"),
    ("icons_terminal", "Terminal / Explorer", "Terminal / Explorer"),
    ("icons_agents", "AI Agents", "AI Agents"),
    ("icons_show_shell", "Show shell icon", "Shell-Icon anzeigen"),
    ("icons_show_explorer", "Show explorer icon", "Explorer-Icon anzeigen"),
    ("icons_toggle_visibility", "Toggle visibility", "Sichtbarkeit umschalten"),
    ("icons_hidden_info", "Hidden", "Ausgeblendet"),
    ("icons_inactive", "inactive", "inaktiv"),
    ("icons_no_profile", "No profile selected", "Kein Profil ausgewählt"),
    ("icons_profile_missing", "Profile not found", "Profil nicht gefunden"),
    ("limit_current", "(current: {})", "(aktuell: {})"),
    ("tray_max_display_desc", "How many repos the tray popup shows at most (5–50, default 10).", "Wie viele Repos maximal im Tray-Popup angezeigt werden (5–50, Standard 10)."),
    ("tray_max_display_summary", "Shows up to {} repos in the tray popup. More in the main window.", "Zeigt bis zu {} Repos im Tray-Popup. Weitere im Hauptfenster."),
    ("tray_icons_order_title", "Icons (order & visibility)", "Icons (Reihenfolge & Sichtbarkeit)"),
    ("tray_icons_order_desc", "Reorder icons (↑/↓) and hide them (eye). Hidden icons leave no gap in the tray row.", "Icons neu anordnen (↑/↓) und ausblenden (Auge). Ausgeblendete Icons verschwinden ohne Lücke in der Tray-Zeile."),
    ("icons_reset", "Reset visibility & order", "Sichtbarkeit & Reihenfolge zurücksetzen"),
    ("icons_all_hidden_warn", "Warning: all IDE icons hidden – no IDE button will be shown.", "Warnung: alle IDE-Icons ausgeblendet – es wird kein IDE-Button angezeigt."),
    ("icons_terminal_hint", "Explorer and shell buttons appear next to IDE icons in the repo list.", "Explorer- und Shell-Buttons erscheinen neben den IDE-Icons in der Repo-Liste."),

    // ── Errors / validation ────────────────────────────────────────
    ("error_need_path", "Please specify at least one path.", "Bitte mindestens einen Pfad angeben."),
    ("error_need_profile", "At least one language profile is required.", "Mindestens ein Sprach-Profil erforderlich."),
    ("error_profile_empty", "Profile '{}' has empty ID/name", "Profil '{}' hat leere ID/Name"),
    ("error_profile_ext", "Profile '{}' needs a file extension", "Profil '{}' braucht Dateiendung"),
    ("saved_scan_restart", "Saved. Restarting scan...", "Gespeichert. Scan wird neu gestartet."),
    ("profile_switched", "Profile switched to '{}' – rescanning solutions...", "Profil gewechselt zu '{}' – scanne Solutions neu..."),
    ("opening_with", "Opening {} with {}...", "Öffne {} mit {}..."),
    ("starting_agent_in", "Starting {} in {}...", "Starte {} in {}..."),
    ("profile_changed_for", "Profile for {} changed – rescanning...", "Profil für {} geändert – scanne neu..."),
    ("save_failed", "Save failed: {}", "Speichern fehlgeschlagen: {}"),
    ("solution_selected", "Solution selected: {}", "Solution ausgewählt: {}"),
    ("explorer_opened", "Explorer opened: {}", "Explorer geöffnet: {}"),
    ("shell_opened", "Shell opened: {}", "Shell geöffnet: {}"),
    ("fetching_branches", "Fetching branches for {}...", "Fetche Branches für {}..."),
    ("fetch_failed", "Fetch failed: {}", "Fetch fehlgeschlagen: {}"),
    ("fetch_partial", "Fetch partially failed: {}", "Fetch teilweise fehlgeschlagen: {}"),
    ("auth_hint", "Note: check Credentials/SSH (auth may be required)", "Hinweis: Credentials/SSH prüfen (ggf. Auth erforderlich)"),
    ("fetch_aborted", "Background fetch aborted (internal error)", "Hintergrund-Fetch abgebrochen (interner Fehler)"),
    ("ide_not_found", "IDE '{}' not found", "IDE '{}' nicht in Profil '{}' gefunden"),
    ("agent_not_found", "AI agent '{}' not found", "AI-Agent '{}' nicht gefunden"),

    // ── Config Selectors ─────────────────────────────────────────────
    ("config_selector_title", "Config dropdowns for this profile", "Config-Dropdowns für dieses Profil"),
    ("config_selector_desc", "XML values from .config files as dropdowns in the repo row. Each selector reads/writes a key in file_path (e.g. App.config <add key=\"Database\" value=\"...\"/>).", "XML-Werte aus .config Dateien als Dropdowns in der Repo-Zeile. Jeder Selector liest/schreibt einen Key in file_path (z.B. App.config <add key=\"Database\" value=\"...\"/>)."),
    ("add_config_selector", "Add Config Selector", "Selector hinzufügen"),
    ("config_file", "Config file:", "Config-Datei:"),
    ("config_key", "Key:", "Key:"),
    ("config_options", "Options:", "Optionen:"),
    ("allow_custom", "Allow custom values", "Eigene Werte erlauben"),
    ("config_saved", "{} set to '{}'", "{} auf '{}' gesetzt"),
    ("config_save_failed", "Failed to save config '{}': {}", "Config '{}' speichern fehlgeschlagen: {}"),
    ("config_missing", "Key '{}' not found in {}", "Key '{}' nicht gefunden in {}"),
    ("key_attr", "Key attribute:", "Key-Attribut:"),
    ("value_attr", "Value attribute:", "Value-Attribut:"),

    // ── Main list sort (Task 4) ──────────────────────────────────
    ("sort_by_name", "Sort by name", "Nach Name sortieren"),
    ("sort_by_last_used", "Sort by last used", "Nach letzter Nutzung sortieren"),
    ("sort_tooltip", "Sort repositories", "Repositories sortieren"),
    ("sort_title", "Sorting", "Sortierung"),

    // ── Behind indicator + auto-fetch (Task 5) ───────────────────
    ("branch_behind", "Behind by {} commit(s)", "{} Commit(s) hinterher"),
    ("auto_fetch_label", "Auto-fetch interval (minutes, 0 = off)", "Auto-Fetch Intervall (Minuten, 0 = aus)"),
    ("auto_fetch_hint", "Periodically runs git fetch --all for all scanned repos in background. 0 disables.", "Führt periodisch git fetch --all für alle gescannten Repos im Hintergrund aus. 0 deaktiviert."),
    ("auto_fetch_off", "Disabled", "Deaktiviert"),
    ("auto_fetch_running", "Auto-fetch running, try again shortly", "Auto-Fetch läuft, bitte kurz warten"),
    ("fetch_busy", "A scan or fetch is already running, try again shortly", "Scan oder Fetch läuft bereits, bitte kurz warten"),

    // ── Tray icon theme (Dark/Light) ─────────────────────────────
    ("tray_icon_theme_title", "Tray icon style", "Tray-Icon Stil"),
    ("tray_icon_theme_desc", "White icon for dark taskbar (Dark) or dark icon for light taskbar (Light).", "Weißes Icon für dunkle Taskleiste (Dark) oder dunkles Icon für helle Taskleiste (Light)."),
    ("tray_icon_theme_dark", "Dark (white icon)", "Dark (weißes Icon)"),
    ("tray_icon_theme_light", "Light (dark icon)", "Light (dunkles Icon)"),

    // ── Merge Requests: Approvals, Pipeline, Threads, Review ─────
    ("mr_appr_of", "{} of {} approvals", "{} von {} Approvals"),
    ("mr_appr_one", "1 approval", "1 Approval"),
    ("mr_appr_n", "{} approvals", "{} Approvals"),
    ("mr_appr_short", "{}/{}", "{}/{}"),
    ("mr_appr_no_rule", "No approval rule – no approvals required", "Keine Approval-Regel – keine Approvals erforderlich"),
    ("mr_appr_goal_tt", "No approval rule in GitLab – goal from the settings", "Keine Approval-Regel in GitLab – Ziel aus den Einstellungen"),
    ("mr_appr_you", "{} (you)", "{} (du)"),
    ("mr_rev_started", "{} started reviewing", "{} hat das Review begonnen"),
    ("mr_rev_waiting", "{} pending", "{} ausstehend"),
    ("mr_pipe_success", "Pipeline passed", "Pipeline grün"),
    ("mr_pipe_failed", "Pipeline failed", "Pipeline fehlgeschlagen"),
    ("mr_pipe_running", "Pipeline running", "Pipeline läuft"),
    ("mr_pipe_pending", "Pipeline pending", "Pipeline wartet"),
    ("mr_pipe_canceled", "Pipeline canceled", "Pipeline abgebrochen"),
    ("mr_pipe_skipped", "Pipeline skipped", "Pipeline übersprungen"),
    ("mr_pipe_manual", "Pipeline manual", "Pipeline manuell"),
    ("mr_pipe_none", "No pipeline", "Keine Pipeline"),
    ("mr_threads_one", "1 open thread", "1 offener Thread"),
    ("mr_threads_many", "{} open threads", "{} offene Threads"),
    ("mr_threads_resolved", "all threads resolved", "alle Threads gelöst"),
    ("mr_threads_none", "no threads", "keine Threads"),
    ("mr_threads_tt", "{} open · {} resolved", "{} offen · {} gelöst"),
    ("mr_threads_none_tt", "No discussion threads", "Keine Diskussions-Threads"),
    ("mr_rev_pending", "Review pending", "Review ausstehend"),
    ("mr_rev_approved", "Approved by you", "Von dir approved"),
    ("mr_rev_changes", "Changes requested", "Änderungen angefragt"),
    ("mr_rev_commented", "Commented", "Kommentiert"),

    // ── Merge Requests: Abruf- und Prüffehler, Abfrageteile ─────────
    ("mr_err_scope", "The token needs read_api.", "Das Token braucht read_api."),
    ("mr_err_network", "{} is not reachable.", "{} nicht erreichbar."),
    ("mr_err_tls", "The certificate of {} is not trusted.", "Zertifikat von {} wird nicht vertraut."),
    ("mr_err_timeout", "{} timed out after {} s.", "Zeitüberschreitung bei {} nach {} s."),
    ("mr_err_content_type", "glab sent the request without a JSON content type.", "glab hat die Anfrage ohne JSON-Content-Type gesendet."),
    ("mr_part_mentions", "Mentions", "Erwähnungen"),
    ("mr_part_reviews", "Review requests", "Review-Anfragen"),
    ("mr_part_uninvolved", "Other open MRs", "Weitere offene MRs"),
    ("mr_part_mine", "Own MRs", "Eigene MRs"),

    // ── Merge Requests: Ansicht (Design B) ───────────────────────
    ("mr_open_settings", "Open settings", "Einstellungen öffnen"),
    ("mr_loading", "Loading merge requests …", "Lade Merge Requests …"),
    ("mr_mine_title", "My merge requests", "Meine Merge Requests"),
    ("mr_others_title", "From others", "Von anderen"),
    ("mr_count_of", "{} of {}", "{} von {}"),
    ("mr_sorted", "Sorted by approvals", "Sortiert nach Approvals"),
    ("mr_sorted_tt", "Most approvals first; ties: MRs with an approval rule and fewer missing approvals first, then most recently updated", "Meiste Approvals zuerst; bei Gleichstand zuerst MRs mit Approval-Regel und weniger fehlenden Approvals, dann zuletzt aktualisiert"),
    ("mr_search_mine_hint", "Search my MRs …", "In meinen MRs suchen …"),
    ("mr_search_others_hint", "Search MRs from others …", "In MRs von anderen suchen …"),
    ("mr_search_clear", "Clear search (Esc)", "Suche leeren (Esc)"),
    ("mr_search_clear_link", "Clear search", "Suche leeren"),
    ("mr_no_hits", "No matches for “{}”", "Keine Treffer für „{}“"),
    ("mr_seg_open", "Open", "Offen"),
    ("mr_seg_open_tt", "Show open merge requests", "Offene Merge Requests anzeigen"),
    ("mr_seg_merged", "Merged", "Gemergt"),
    ("mr_seg_merged_tt", "Show merge requests merged in the last {} days", "Gemergte Merge Requests der letzten {} Tage anzeigen"),
    ("mr_sorted_merged", "Sorted by merge time", "Sortiert nach Merge-Zeitpunkt"),
    ("mr_sorted_merged_tt", "Most recently merged first", "Zuletzt gemergte zuerst"),
    ("mr_merged_caption", "{} merged in the last {} days", "{} gemergt in den letzten {} Tagen"),
    ("mr_period", "Last {} days", "Letzte {} Tage"),
    ("mr_period_tt", "Period of the merged merge requests", "Zeitraum der gemergten Merge Requests"),
    ("mr_merged_by", "merged {} by {}", "gemergt {} von {}"),
    ("mr_merged_by_me", "merged {} by you", "gemergt {} von dir"),
    ("mr_merged_at", "merged {}", "gemergt {}"),
    ("mr_loading_merged", "Loading merged merge requests …", "Gemergte Merge Requests werden geladen …"),
    ("mr_empty_merged", "No merged merge requests in the last {} days.", "Keine gemergten Merge Requests in den letzten {} Tagen."),
    ("mr_part_merged", "Merged MRs", "Gemergte MRs"),
    ("mr_chip_reviewer", "Reviewer", "Reviewer"),
    ("mr_chip_reviewer_tt", "MRs where you are a reviewer", "MRs, bei denen du als Reviewer eingetragen bist"),
    ("mr_chip_mention", "Mentioned", "Erwähnt"),
    ("mr_chip_mention_tt", "MRs mentioning you in the description or a comment", "MRs, in deren Beschreibung oder Kommentaren du erwähnt wirst"),
    ("mr_chip_all", "All open", "Alle offenen"),
    ("mr_chip_all_tt", "Also show MRs I'm not involved in", "Auch MRs ohne meine Beteiligung anzeigen"),
    ("mr_chip_all_tt_n", "Also show MRs I'm not involved in ({} open in {} local projects)", "Auch MRs ohne meine Beteiligung anzeigen ({} offene in {} lokalen Projekten)"),
    ("mr_chip_all_tt_n_one", "Also show MRs I'm not involved in ({} open in 1 local project)", "Auch MRs ohne meine Beteiligung anzeigen ({} offene in 1 lokalen Projekt)"),
    ("mr_reason_reviewer", "Reviewer", "Reviewer"),
    ("mr_reason_desc", "@ description", "@ Beschreibung"),
    ("mr_reason_comment", "@ comment", "@ Kommentar"),
    ("mr_reasons_prefix", "Shown because:", "Angezeigt wegen:"),
    ("mr_no_filter", "No filter active", "Kein Filter aktiv"),
    ("mr_no_filter_hint", "Turn on “Reviewer” or “Mentioned” to see merge requests you're involved in.", "Schalte „Reviewer“ oder „Erwähnt“ ein, um Merge Requests zu sehen, an denen du beteiligt bist."),
    ("mr_filter_no_match", "No merge requests match this filter", "Keine Merge Requests für diesen Filter"),
    ("mr_empty_mine", "You have no open merge requests.", "Du hast keine offenen Merge Requests."),
    ("mr_empty_others", "No review is waiting for you.", "Gerade wartet kein Review auf dich."),
    ("mr_hidden_by_selection_one", "1 MR from deselected repositories hidden", "1 MR aus abgewählten Repositories ausgeblendet"),
    ("mr_hidden_by_selection_many", "{} MRs from deselected repositories hidden", "{} MRs aus abgewählten Repositories ausgeblendet"),
    ("mr_todos_hidden", "Mentions are missing: GitLab hides your to-dos from this token", "Erwähnungen fehlen: GitLab verbirgt die To-dos vor diesem Token"),
    ("mr_todos_hidden_fix", "Fine-grained tokens need the permission “Todo: Read” – add it to the token in GitLab or use a token with read_api.", "Feingranulare Tokens brauchen die Berechtigung „Todo: Read“ – ergänze sie am Token in GitLab oder nutze ein Token mit read_api."),
    ("mrs_state_limited", "Limited", "Eingeschränkt"),
    ("mrs_todos_hidden", "Mentions are missing: the token can't read to-dos", "Erwähnungen fehlen: das Token darf keine To-dos lesen"),
    ("mr_uninvolved_h", "Not involved · {}", "Nicht beteiligt · {}"),
    ("mr_uninvolved_sub", "open MRs in the {} local projects without your involvement", "offene MRs der {} lokalen Projekte ohne deine Beteiligung"),
    ("mr_uninvolved_sub_one", "open MRs in the local project without your involvement", "offene MRs des lokalen Projekts ohne deine Beteiligung"),
    ("mr_shown_of", "{} of {} shown", "{} von {} angezeigt"),
    ("mr_load_more", "Load {} more", "{} weitere laden"),
    ("mr_loading_uninvolved", "Loading more open merge requests …", "Lade weitere offene Merge Requests …"),
    ("mr_uninvolved_not_loaded", "Not loaded yet.", "Noch nicht geladen."),
    ("mr_load_now", "Load now", "Jetzt laden"),
    ("mr_new_comment_one", "1 new comment", "1 neuer Kommentar"),
    ("mr_new_comments", "{} new comments", "{} neue Kommentare"),
    ("mr_new_commit_one", "1 new commit", "1 neuer Commit"),
    ("mr_new_commits", "{} new commits", "{} neue Commits"),
    ("mr_new_push", "new push", "neu gepusht"),
    ("mr_new_tt", "New since you last opened this MR (browser, Claude, IDE or checkout)", "Neu seit du den MR zuletzt geöffnet hast (Browser, Claude, IDE oder Auschecken)"),
    ("mr_notify_label", "Notify about new review requests, mentions and comments", "Benachrichtigen bei neuen Review-Anfragen, Erwähnungen und Kommentaren"),
    ("mr_notify_review", "Review requested", "Review angefragt"),
    ("mr_notify_review_line", "{} added you as reviewer", "{} hat dich als Reviewer eingetragen"),
    ("mr_notify_mention_comment", "Mentioned in a comment", "In Kommentar erwähnt"),
    ("mr_notify_mention_desc", "Mentioned in the description", "In Beschreibung erwähnt"),
    ("mr_notify_mention_line", "Merge request by {}", "Merge Request von {}"),
    ("mr_notify_comments", "New comments", "Neue Kommentare"),
    ("mr_notify_comments_line", "{} new comments", "{} neue Kommentare"),
    ("mr_notify_comments_more", "{} and {} more: {}", "{} und {} weitere: {}"),
    ("mr_notify_more", "{} more merge request updates", "{} weitere Neuigkeiten in Merge Requests"),
    ("mr_notify_btn_open", "Open in browser", "Im Browser öffnen"),
    ("mr_notify_btn_show", "Show in GitManager", "In GitManager anzeigen"),
    ("mr_draft", "Draft", "Entwurf"),
    ("mr_ready", "ready to merge", "mergebereit"),
    ("mr_conflict", "Conflicts", "Konflikte"),
    ("mr_conflict_tt", "GitLab: merge conflicts with the target branch – rebase or merge the target branch", "GitLab: Merge-Konflikte mit dem Ziel-Branch – Ziel-Branch rebasen oder mergen"),
    ("mr_ready_tt", "GitLab: can be merged", "GitLab: kann gemergt werden"),
    ("mr_pipe_open_tt", "{} – open pipeline in browser", "{} – Pipeline im Browser öffnen"),

    // ── Merge Requests: Aktionen und Checkout-Menü (K1) ──────────
    ("mr_open_browser", "Open in browser", "Im Browser öffnen"),
    ("mr_open_title_tt", "Open merge request in browser", "Merge Request im Browser öffnen"),
    ("mr_checkout_menu_tt", "Check out – choose target", "Auschecken – Ziel wählen"),
    ("mr_menu_header_clones", "{} · {} local clones", "{} · {} lokale Klone"),
    ("mr_menu_header_clone_one", "{} · 1 local clone", "{} · 1 lokaler Klon"),
    ("mr_menu_new_folder", "In a new folder", "In neuem Ordner"),
    ("mr_menu_worktree_review", "New worktree in the review folder (recommended)", "Neuer Worktree im Review-Ordner (empfohlen)"),
    ("mr_menu_worktree", "New worktree (recommended)", "Neuer Worktree (empfohlen)"),
    ("mr_menu_worktree_sub", "{} · shares git data with {}", "{} · teilt Git-Daten mit {}"),
    ("mr_menu_other_folder", "Choose another folder …", "Anderen Ordner wählen …"),
    ("mr_menu_clone", "New clone …", "Neuer Klon …"),
    ("mr_menu_clone_sub", "fully independent, downloads again", "vollständig unabhängig, lädt neu herunter"),
    ("mr_menu_switch", "Switch an existing clone", "In bestehendem Klon wechseln"),
    ("mr_state_free", "free", "frei"),
    ("mr_state_in_use", "in use for !{}", "in Arbeit für !{}"),
    ("mr_state_in_use_branch", "in use", "in Arbeit"),
    ("mr_state_dirty_one", "1 change", "1 Änderung"),
    ("mr_state_dirty_many", "{} changes", "{} Änderungen"),
    ("mr_state_dirty_hint", "asks to stash/discard", "fragt nach Stash/Verwerfen"),
    ("mr_last_used", "last used {}", "zuletzt genutzt {}"),
    ("mr_menu_after", "Then open", "Danach öffnen"),
    ("mr_after_shell", "Shell", "Shell"),
    ("mr_after_explorer", "Explorer", "Explorer"),
    ("mr_after_nothing", "Nothing", "Nichts"),
    ("mr_after_remembered", "remembers the last choice", "merkt sich die letzte Wahl"),
    ("mr_already_header", "Already checked out in {}", "Bereits ausgecheckt in {}"),
    ("mr_open_show_in_list", "Show in repository list", "In Repo-Liste zeigen"),
    ("mr_open_agent", "Start {}", "{} starten"),
    ("mr_open_shell", "Open shell", "Shell öffnen"),
    ("mr_open_explorer", "Open in Explorer", "Im Explorer öffnen"),
    ("mr_open_ide", "Open in {}", "In {} öffnen"),
    ("mr_checked_out_tt", "Branch is already checked out in {}", "Branch ist in {} bereits ausgecheckt"),
    ("mr_not_local", "not local", "nicht lokal"),
    ("mr_not_local_tt", "Project isn't cloned locally – checkout not possible", "Projekt ist nicht lokal geklont – Auschecken nicht möglich"),
    ("mr_fork_tt", "Fork MR – checkout will come in a later version", "Fork-MR – Auschecken folgt in einer späteren Version"),
    ("mr_branch_deleted_tt", "Source branch no longer exists", "Quell-Branch existiert nicht mehr"),

    // ── Merge Requests: Auschecken in der App (K1) ───────────────
    ("mr_checkout_busy", "Fetching {} for !{} …", "Hole {} für !{} …"),
    ("mr_cloning", "Cloning {} to {} …", "Klone {} nach {} …"),
    ("mr_checkout_blocked", "Checkout not possible while a scan or fetch is running", "Auschecken nicht möglich, solange ein Scan oder Fetch läuft"),
    ("mr_switch_waiting", "A branch switch is still waiting for your decision – finish that dialog first", "Ein Branch-Wechsel wartet noch auf deine Entscheidung – erst den Dialog abschließen"),
    ("mr_checkout_gone", "!{} is no longer in the current list – checkout skipped", "!{} ist nicht mehr im aktuellen Stand – Auschecken übersprungen"),
    ("mr_checkout_running", "Checking out !{} …", "Auschecken für !{} läuft …"),
    ("mr_worktree_created", "Worktree {} created for !{}", "Worktree {} für !{} angelegt"),
    ("mr_clone_done", "!{} cloned to {}", "!{} geklont nach {}"),
    ("mr_checkout_done", "!{} checked out: {} in {}", "!{} ausgecheckt: {} in {}"),
    ("mr_checkout_done_ff", "!{} checked out: {} in {} (+{} commits)", "!{} ausgecheckt: {} in {} (+{} Commits)"),
    ("mr_checkout_done_ff_one", "!{} checked out: {} in {} (+1 commit)", "!{} ausgecheckt: {} in {} (+1 Commit)"),
    ("mr_already_head", "Already checked out in {}", "Bereits ausgecheckt in {}"),
    ("mr_already_head_behind", "Already checked out in {} ({} behind)", "Bereits ausgecheckt in {} ({} hinterher)"),
    ("mr_then_pull_ask", "– new commits in the MR, pull first?", "– neue Commits im MR, vorher pullen?"),
    ("mr_pull_ask_title", "New commits in the merge request", "Neue Commits im Merge Request"),
    ("mr_pull_ask", "The merge request has commits that are missing in {}. Pull before starting?", "Der Merge Request hat Commits, die in {} noch fehlen. Vor dem Start pullen?"),
    ("mr_pull_ask_behind", "{} is {} commit(s) behind the remote. Pull before starting?", "{} ist {} Commit(s) hinter dem Remote. Vor dem Start pullen?"),
    ("mr_pull_and_start", "Pull and start {}", "Pullen und {} starten"),
    ("mr_start_without_pull", "Start without pulling", "Ohne Pull starten"),
    ("mr_then_agent", "– starting {}", "– {} startet"),
    ("mr_then_shell", "– opening shell", "– Shell öffnet"),
    ("mr_then_explorer", "– opening Explorer", "– Explorer öffnet"),
    ("mr_then_ide", "– opening {}", "– {} öffnet"),
    ("mr_then_no_ide", "– no IDE in the profile, nothing opened", "– keine IDE im Profil, nichts geöffnet"),
    ("mr_no_ide", "No IDE in this folder's profile", "Keine IDE im Profil dieses Ordners"),
    ("mr_diverged", "Local branch {} differs from the MR ({} ahead, {} behind) – local state checked out", "Lokaler Branch {} weicht vom MR-Stand ab ({} voraus, {} hinterher) – lokaler Stand ausgecheckt"),
    ("mr_checkout_failed", "Checking out !{} failed: {}", "Auschecken von !{} fehlgeschlagen: {}"),
    ("mr_checkout_no_remote", "No remote in {} points to {}", "Kein Remote in {} zeigt auf {}"),
    ("mr_repo_open_failed", "Cannot open repository {}", "Repository {} lässt sich nicht öffnen"),
    ("mr_folder_exists", "The folder {} already exists and is not empty", "Der Ordner {} existiert bereits und ist nicht leer"),
    ("mr_remote_branch_missing", "Remote branch {} not found", "Remote-Branch {} nicht gefunden"),
    ("mr_branch_name_taken", "Neither {} nor {} can be used as a local branch", "Weder {} noch {} lassen sich als lokaler Branch nutzen"),

    // ── Merge Requests: Zustände der Ansicht ─────────────────────
    ("mr_no_remotes", "No git remote was found in the scanned repositories.", "In den gescannten Repositories wurde kein Git-Remote gefunden."),
    ("mr_no_gitlab_host", "No GitLab host detected – glab isn't signed in to any of your remote hosts.", "Kein GitLab-Host erkannt – glab ist für keinen deiner Remote-Hosts angemeldet."),
    ("mr_add_token", "Add token", "Token hinzufügen"),
    ("mr_add_gitlab_host", "Add as GitLab host", "Als GitLab-Host hinzufügen"),
    ("mr_all_deselected", "All repositories are deselected in the settings.", "Alle Repositories sind in den Einstellungen abgewählt."),
    ("mr_glab_missing", "glab was not found", "glab wurde nicht gefunden"),
    ("mr_glab_missing_desc", "GitManager queries merge requests via the GitLab CLI (glab).", "GitManager fragt Merge Requests über die GitLab CLI (glab) ab."),
    ("mr_glab_restart_hint", "After installing, restart GitManager or set the path to glab.exe.", "Nach der Installation GitManager neu starten oder den Pfad zu glab.exe angeben."),
    ("mr_glab_too_old", "glab {} is too old – please update: winget upgrade GLab.GLab", "glab {} ist zu alt – bitte aktualisieren: winget upgrade GLab.GLab"),
    ("mr_copy_cmd", "Copy command", "Befehl kopieren"),
    ("mr_not_logged_in", "Not signed in to {}", "Nicht bei {} angemeldet"),
    ("mr_partial", "{} could not be loaded ({}).", "{} konnten nicht geladen werden ({})."),
    ("mr_host_failed", "Merge requests from {} could not be loaded.", "Merge Requests von {} konnten nicht geladen werden."),
    ("mr_err_unknown", "unknown error", "unbekannter Fehler"),
    ("mr_details", "Details", "Details"),

    // ── Merge Requests: Umschalter, Top-Leiste, Statusleiste ──────
    ("view_repos", "Repositories", "Repositories"),
    ("view_repos_tt", "Show repository list", "Repository-Liste anzeigen"),
    ("view_mrs", "Merge Requests", "Merge Requests"),
    ("view_mrs_tt", "Show merge requests", "Merge Requests anzeigen"),
    ("mr_badge_tt_one", "Merge requests – 1 review waiting for you", "Merge Requests – 1 Review wartet auf dich"),
    ("mr_badge_tt_many", "Merge requests – {} reviews waiting for you", "Merge Requests – {} Reviews warten auf dich"),
    ("mr_reload_tt", "Reload merge requests", "Merge Requests neu laden"),
    ("mr_status_line", "Merge requests · {} · updated {}", "Merge Requests · {} · Stand: {}"),
    ("mr_refreshing", "Refreshing … · updated {}", "Aktualisiere … · Stand: {}"),
    ("mr_fetch_aborted", "Loading merge requests was aborted (internal error)", "Abruf der Merge Requests abgebrochen (interner Fehler)"),

    // ── Merge Requests: Tray-Fußzeile (E21) ───────────────────────
    ("tray_main_window", "Main window", "Hauptfenster"),
    ("tray_mrs", "Merge Requests", "Merge Requests"),
    ("tray_mrs_tt_one", "Merge requests – 1 review waiting for you", "Merge Requests – 1 Review wartet auf dich"),
    ("tray_mrs_tt_many", "Merge requests – {} reviews waiting for you", "Merge Requests – {} Reviews warten auf dich"),
    ("tray_mrs_tt_none", "Open main window with merge requests", "Hauptfenster mit Merge Requests öffnen"),
    ("tray_quit", "Quit", "Beenden"),

    // ── Merge Requests: Einstellungs-Tab (Task 12) ────────────────
    ("tabs_merge_requests", "Merge Requests", "Merge Requests"),
    ("mrs_sec_cli", "GitLab CLI", "GitLab CLI"),
    ("mrs_glab_program", "glab program", "glab-Programm"),
    ("mrs_glab_found", "glab {} found · {}", "glab {} gefunden · {}"),
    ("mrs_glab_not_found", "glab not found", "glab nicht gefunden"),
    ("mrs_glab_invalid", "Invalid glab program: characters like & | ; < > ` $ are only allowed in a full path.", "Ungültiges glab-Programm: Sonderzeichen wie & | ; < > ` $ sind nur in einem vollen Pfad erlaubt."),
    ("mrs_recheck", "Check again", "Erneut prüfen"),
    ("mrs_sec_auth", "Sign-in & token", "Anmeldung & Token"),
    ("mrs_state_checking", "Checking …", "Prüfe …"),
    ("mrs_state_ready", "Ready", "Bereit"),
    ("mrs_state_rights_missing", "Missing rights (read_api or api)", "Rechte fehlen (read_api oder api)"),
    ("mrs_state_not_signed", "Not signed in", "Nicht angemeldet"),
    ("mrs_state_expired", "Expired", "Abgelaufen"),
    ("mrs_state_revoked", "Revoked", "Widerrufen"),
    ("mrs_state_unreachable", "Not reachable", "Nicht erreichbar"),
    ("mrs_state_tls", "Certificate rejected", "Zertifikat abgelehnt"),
    ("mrs_rights_gitmanager_ok", "Enough for the merge request view (needs read_api or api)", "Reicht für die Merge-Request-Ansicht (benötigt read_api oder api)"),
    ("mrs_glab_full", "glab: full", "glab: Voll"),
    ("mrs_glab_readonly", "glab: read-only – glab mr approve/merge/note/create will fail", "glab: Nur lesen – glab mr approve/merge/note/create schlagen fehl"),
    ("mrs_glab_insufficient", "glab: insufficient", "glab: Unzureichend"),
    ("mrs_glab_unknown", "glab: rights unknown – scopes not readable (e.g. fine-grained token)", "glab: Rechte unbekannt – Scopes nicht lesbar (z. B. feingranulares Token)"),
    ("mrs_glab_scopes_hint", "glab recommends the scopes api and write_repository.", "glab empfiehlt die Scopes api und write_repository."),
    ("mrs_signed_in_as", "Signed in as {} (@{})", "Angemeldet als {} (@{})"),
    ("mrs_token_kind", "Token type: {}", "Token-Art: {}"),
    ("mrs_kind_pat", "Personal access token", "Persönliches Zugriffstoken"),
    ("mrs_kind_oauth", "OAuth (glab)", "OAuth (glab)"),
    ("mrs_kind_bot", "Project/group token", "Projekt-/Gruppen-Token"),
    ("mrs_kind_granular", "Fine-grained token", "Feingranulares Token"),
    ("mrs_token_source", "Stored in: {}", "Gespeichert in: {}"),
    ("mrs_src_keyring", "Windows Credential Manager", "Windows-Anmeldeinformationsverwaltung"),
    ("mrs_src_config", "glab config", "glab-Konfiguration"),
    ("mrs_src_env", "Environment variable {}", "Umgebungsvariable {}"),
    ("mrs_expires", "expires on {}", "läuft ab am {}"),
    ("mrs_expires_soon", "Expires in {} days", "Läuft in {} Tagen ab"),
    ("mrs_expires_soon_one", "Expires in 1 day", "Läuft in 1 Tag ab"),
    ("mrs_scopes", "Scopes:", "Scopes:"),
    ("mrs_check", "Check", "Prüfen"),
    ("mrs_add_token", "Add token …", "Token hinzufügen …"),
    ("mrs_replace_token", "Replace token …", "Token ersetzen …"),
    ("mrs_token_label", "Token", "Token"),
    ("mrs_show_token", "Show", "Anzeigen"),
    ("mrs_purpose", "Purpose", "Zweck"),
    ("mrs_purpose_read", "Read-only – read_api (enough for GitManager)", "Nur lesen – read_api (reicht für GitManager)"),
    ("mrs_purpose_full", "Full – api, write_repository (also glab write commands)", "Voll – api, write_repository (auch glab-Schreibbefehle)"),
    ("mrs_create_token", "Create token in GitLab", "Token in GitLab erstellen"),
    ("mrs_protocol", "Protocol", "Protokoll"),
    ("mrs_token_note", "The token is stored in glab and replaces the glab sign-in for {} – also for glab in the terminal.", "Das Token wird in glab gespeichert und ersetzt die glab-Anmeldung für {} – auch für glab im Terminal."),
    ("mrs_http_warning", "{} uses HTTP – the token is sent unencrypted.", "{} nutzt HTTP – das Token wird unverschlüsselt übertragen."),
    ("mrs_http_chip", "http – unencrypted", "http – unverschlüsselt"),
    ("mrs_token_empty", "Please paste a token.", "Bitte ein Token einfügen."),
    ("mrs_token_whitespace", "The token contains spaces or line breaks.", "Das Token enthält Leerzeichen oder Zeilenumbrüche."),
    ("mrs_token_invalid", "The token is invalid.", "Das Token ist ungültig."),
    ("mrs_verify", "Verify", "Prüfen"),
    ("mrs_preview", "@{} · {} · expires {} · {}/{} repos accessible", "@{} · {} · läuft ab {} · {}/{} Repos zugreifbar"),
    ("mrs_preview_no_expiry", "@{} · {} · {}/{} repos accessible", "@{} · {} · {}/{} Repos zugreifbar"),
    ("mrs_downgrade_warning", "The current sign-in has api. With this token glab loses write access (mr approve/merge/note).", "Die bisherige Anmeldung hat api. Mit diesem Token verliert glab Schreibrechte (mr approve/merge/note)."),
    ("mrs_downgrade_confirm", "Save anyway", "Trotzdem speichern"),
    ("mrs_token_saved", "Token saved for {}", "Token für {} gespeichert"),
    ("mrs_token_failed", "glab could not store the token: {}", "glab konnte das Token nicht speichern: {}"),
    ("mrs_env_banner", "The environment variable {} is set and overrides stored tokens for all hosts.", "Die Umgebungsvariable {} ist gesetzt und überschreibt gespeicherte Tokens für alle Hosts."),
    ("mrs_unrecognized", "Remote hosts not signed in to glab ({})", "Nicht bei glab angemeldete Remote-Hosts ({})"),
    ("mrs_unrecognized_hint", "GitManager sends no requests to these hosts until you add a token or add them as a GitLab host. You can ignore hosts that aren't GitLab (e.g. github.com).", "An diese Hosts schickt GitManager keine Anfragen, bis du ein Token hinzufügst oder sie als GitLab-Host einträgst. Hosts, die kein GitLab sind (z. B. github.com), kannst du ignorieren."),
    ("mrs_host_repos", "{} repos", "{} Repos"),
    ("mrs_host_repos_one", "1 repo", "1 Repo"),
    ("mrs_belongs_to", "belongs to …", "gehört zu …"),
    ("mrs_sec_repos", "Repositories for merge requests", "Repositories für Merge Requests"),
    ("mrs_included", "{} of {} included · {} accessible", "{} von {} einbezogen · {} zugreifbar"),
    ("mrs_all", "All", "Alle"),
    ("mrs_none", "None", "Keine"),
    ("mrs_check_access", "Check access", "Zugriff prüfen"),
    ("mrs_col_repo", "Repository", "Repository"),
    ("mrs_col_clones", "Clones", "Klone"),
    ("mrs_col_project", "GitLab project", "GitLab-Projekt"),
    ("mrs_col_access", "Access", "Zugriff"),
    ("mrs_clones", "{} clones", "{} Klone"),
    ("mrs_acc_ok", "Access OK ({})", "Zugriff ok ({})"),
    ("mrs_acc_ok_plain", "Access OK", "Zugriff ok"),
    ("mrs_acc_public", "public, not a member", "öffentlich, kein Mitglied"),
    ("mrs_acc_renamed", "renamed → {}", "umbenannt → {}"),
    ("mrs_acc_denied", "no access with this token", "kein Zugriff mit diesem Token"),
    ("mrs_acc_mr_off", "MRs disabled in project", "MRs im Projekt deaktiviert"),
    ("mrs_acc_archived", "archived", "archiviert"),
    ("mrs_acc_host_auth", "Host not signed in", "Host nicht angemeldet"),
    ("mrs_acc_checking", "Checking …", "Prüfe …"),
    ("mrs_acc_skipped", "skipped (no access)", "wird übersprungen (kein Zugriff)"),
    ("mrs_include_nonlocal", "Also show MRs from projects without a local clone", "Auch MRs aus Projekten ohne lokalen Klon anzeigen"),
    ("mrs_sec_checkout", "Checkout", "Auschecken"),
    ("mrs_review_root", "Folder for reviews (MRs from others)", "Ordner für Reviews (MRs von anderen)"),
    ("mrs_review_root_ok", "Inside search path {} – new worktrees appear in the repository list.", "Liegt im Suchpfad {} – neue Worktrees erscheinen in der Repo-Liste."),
    ("mrs_review_root_warn", "Not inside a search path – new worktrees won't appear in the repository list.", "Liegt in keinem Suchpfad – neue Worktrees erscheinen nicht in der Repo-Liste."),
    ("mrs_review_root_relative", "Not an absolute path – ignored, new worktrees go next to the base clone.", "Kein absoluter Pfad – wird ignoriert, neue Worktrees landen neben dem Basis-Klon."),
    ("mrs_add_search_path", "Add as search path", "Als Suchpfad hinzufügen"),
    ("mrs_folder_name", "Folder name", "Ordnername"),
    ("mrs_folder_name_example", "e.g. {}", "z. B. {}"),
    ("mrs_after_checkout", "Open after checkout", "Nach dem Auschecken öffnen"),
    ("mrs_after_last", "Last choice", "Zuletzt gewählt"),
    ("mrs_after_last_hint", "last: {}", "zuletzt: {}"),
    ("mrs_sec_display", "Display & refresh", "Anzeige & Aktualisierung"),
    ("mrs_refresh", "Refresh automatically", "Automatisch aktualisieren"),
    ("mrs_refresh_unit", "min (0 = off)", "Min. (0 = aus)"),
    ("mrs_show_all_default", "“All open” on by default", "„Alle offenen“ standardmäßig aktiv"),
    ("mrs_ignore_bots", "Don't count bot approvals", "Approvals von Bots nicht mitzählen"),
    ("mrs_approval_goal", "Number of approvals for a full bar", "Approvals für vollen Balken"),
    ("mrs_approval_goal_unit", "(0 = off; only without an approval rule in GitLab)", "(0 = aus; nur ohne Approval-Regel in GitLab)"),
    ("mrs_check_finished", "Check for {} finished", "Prüfung für {} abgeschlossen"),
    ("mrs_belongs_to_tt", "Another name (e.g. SSH host) for a signed-in GitLab host", "Anderer Name (z. B. SSH-Host) für einen angemeldeten GitLab-Host"),
    ("mrs_alias_apply", "Assign", "Zuordnen"),
    ("mrs_alias_of", "{} belongs to this host", "{} gehört zu diesem Host"),
    ("mrs_alias_remove", "Remove mapping", "Zuordnung entfernen"),
    ("mrs_extra_redundant", "Belongs to {} and is queried through it – this entry isn't needed and can be removed.", "Gehört zu {} und wird darüber abgefragt – dieser Eintrag ist nicht nötig und kann entfernt werden."),
    ("mrs_glab_pending", "Applies after “Save” – until then GitManager uses {}.", "Gilt nach „Speichern“ – bis dahin nutzt GitManager {}."),

    // ── Relative Zeiten ──────────────────────────────────────────
    ("rel_now", "just now", "gerade eben"),
    ("rel_min", "{} min ago", "vor {} Min."),
    ("rel_hour", "{} h ago", "vor {} Std."),
    ("rel_yesterday", "yesterday", "gestern"),
    ("rel_days", "{} days ago", "vor {} Tagen"),

    // ── Repo-Karte: Chips, Stern, ⋯-Menü ──────────────────────────
    ("repo_state_clean", "clean", "sauber"),
    ("repo_state_dirty", "{} changed", "{} geändert"),
    ("repo_state_conflict", "Conflict", "Konflikt"),
    ("repo_state_merge", "Merge in progress", "Merge läuft"),
    ("repo_state_rebase", "Rebase in progress", "Rebase läuft"),
    ("repo_state_cherry_pick", "Cherry-pick in progress", "Cherry-Pick läuft"),
    ("repo_state_revert", "Revert in progress", "Revert läuft"),
    ("repo_state_other", "Git operation in progress", "Git-Vorgang läuft"),
    ("repo_state_conflict_tt", "Unresolved merge conflicts – resolve them in the IDE or shell", "Ungelöste Merge-Konflikte – in IDE oder Shell auflösen"),
    ("repo_state_in_progress_tt", "A Git operation was started but not finished", "Ein Git-Vorgang wurde begonnen, aber nicht abgeschlossen"),
    ("repo_ahead_tt", "{} unpushed commit(s)", "{} ungepushte Commit(s)"),
    ("repo_behind_tt", "{} commit(s) behind – click: fast-forward", "{} Commit(s) hinterher – Klick: Fast-Forward"),
    ("repo_mr_chip_tt", "{}\n{}\nClick: show in Merge Requests", "{}\n{}\nKlick: in Merge Requests zeigen"),
    ("repo_mr_draft", "Draft", "Entwurf"),
    ("repo_claude_waiting", "Claude waiting", "Claude wartet"),
    ("repo_claude_working", "Claude working", "Claude arbeitet"),
    ("repo_claude_tt", "Click: bring the session's terminal to the front", "Klick: Terminal der Session in den Vordergrund"),
    ("repo_fav_add", "Add to favorites", "Als Favorit markieren"),
    ("repo_fav_remove", "Remove from favorites", "Favorit entfernen"),
    ("repo_more", "More actions", "Weitere Aktionen"),
    ("repo_menu_open_in", "Open in", "Öffnen in"),
    ("repo_menu_profile", "Profile", "Profil"),
    ("repo_menu_new_worktree", "New worktree …", "Neuer Worktree …"),
    ("repo_menu_copy_path", "Copy path", "Pfad kopieren"),
    ("repo_menu_copy_branch", "Copy branch name", "Branch-Namen kopieren"),
    ("repo_menu_open_browser", "Open remote in browser", "Remote im Browser öffnen"),
    ("repo_menu_remove_worktree", "Remove worktree …", "Worktree entfernen …"),
    ("repo_cleanup_button", "Clean up …", "Aufräumen …"),
    ("repo_cleanup_tooltip", "Find merged worktrees and branches that can be removed", "Gemergte Worktrees und Branches zum Entfernen suchen"),
    ("repo_commit_subject", "“{}”", "„{}“"),
];

/// Nachschlagetabelle über [`TEXTS`], beim ersten Aufruf aufgebaut.
fn texts() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    static TABLE: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();
    TABLE.get_or_init(|| TEXTS.iter().map(|&(key, en, de)| (key, (en, de))).collect())
}

/// Translate a key for the given language.
/// Unbekannte Schlüssel liefern den Schlüssel selbst.
pub fn tr(lang: Language, key: &str) -> String {
    match texts().get(key) {
        Some(&(en, de)) => match lang {
            Language::En => en,
            Language::De => de,
        }
        .to_string(),
        None => key.to_string(),
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
            "mr_conflict",
            "mr_conflict_tt",
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
            "mr_then_pull_ask",
            "mr_pull_ask_title",
            "mr_pull_ask",
            "mr_pull_ask_behind",
            "mr_pull_and_start",
            "mr_start_without_pull",
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
    #[test]
    fn texts_keys_unique() {
        let mut seen = std::collections::HashSet::new();
        for (key, _, _) in TEXTS {
            assert!(seen.insert(*key), "doppelter Schlüssel: {key}");
        }
    }

    #[test]
    fn tr_unknown_key_returns_key() {
        assert_eq!(tr(Language::En, "zz_missing_key"), "zz_missing_key");
        assert_eq!(tr(Language::De, "zz_missing_key"), "zz_missing_key");
    }
}
