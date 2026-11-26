//! Alle eingebetteten SVG-Icons an einer Stelle. `include_image!` leitet die
//! Bild-URI aus dem Pfad ab: eine Konstante je Datei heißt auch ein Eintrag
//! im Bild-Cache je Datei.

use egui::{Image, ImageSource, Vec2};

pub const ICON_AIDER: ImageSource = egui::include_image!("../../assets/icons/aider.svg");
pub const ICON_ARROW_DOWN: ImageSource = egui::include_image!("../../assets/icons/arrow-down.svg");
pub const ICON_ARROW_UP: ImageSource = egui::include_image!("../../assets/icons/arrow-up.svg");
pub const ICON_CHEVRON_DOWN: ImageSource =
    egui::include_image!("../../assets/icons/chevron-down.svg");
pub const ICON_CHEVRON_UP: ImageSource = egui::include_image!("../../assets/icons/chevron-up.svg");
pub const ICON_CLAUDE: ImageSource = egui::include_image!("../../assets/icons/claude.svg");
pub const ICON_CODEX: ImageSource = egui::include_image!("../../assets/icons/codex.svg");
pub const ICON_COPILOT: ImageSource = egui::include_image!("../../assets/icons/copilot.svg");
pub const ICON_CROSS: ImageSource = egui::include_image!("../../assets/icons/cross.svg");
pub const ICON_CURSOR: ImageSource = egui::include_image!("../../assets/icons/cursor.svg");
pub const ICON_DOTS: ImageSource = egui::include_image!("../../assets/icons/dots-three.svg");
pub const ICON_EYE_OFF: ImageSource = egui::include_image!("../../assets/icons/eye-off.svg");
pub const ICON_EYE: ImageSource = egui::include_image!("../../assets/icons/eye.svg");
pub const ICON_FOLDER: ImageSource = egui::include_image!("../../assets/icons/folder.svg");
pub const ICON_GEAR: ImageSource = egui::include_image!("../../assets/icons/gear.svg");
pub const ICON_GEMINI: ImageSource = egui::include_image!("../../assets/icons/gemini.svg");
pub const ICON_GIT_BRANCH: ImageSource = egui::include_image!("../../assets/icons/git-branch.svg");
pub const ICON_GIT_COMMIT: ImageSource = egui::include_image!("../../assets/icons/git-commit.svg");
pub const ICON_PLUS: ImageSource = egui::include_image!("../../assets/icons/plus.svg");
pub const ICON_REFRESH: ImageSource = egui::include_image!("../../assets/icons/refresh.svg");
pub const ICON_RIDER: ImageSource = egui::include_image!("../../assets/icons/rider.svg");
pub const ICON_SEARCH: ImageSource = egui::include_image!("../../assets/icons/search.svg");
pub const ICON_STAR_FILL: ImageSource = egui::include_image!("../../assets/icons/star-fill.svg");
pub const ICON_STAR: ImageSource = egui::include_image!("../../assets/icons/star.svg");
pub const ICON_TERMINAL: ImageSource = egui::include_image!("../../assets/icons/terminal.svg");
pub const ICON_TRASH: ImageSource = egui::include_image!("../../assets/icons/trash.svg");
pub const ICON_VS: ImageSource = egui::include_image!("../../assets/icons/visualstudio.svg");
pub const ICON_VSCODE: ImageSource = egui::include_image!("../../assets/icons/vscode.svg");
pub const ICON_WARNING: ImageSource = egui::include_image!("../../assets/icons/warning.svg");

/// Eingebettetes Icon zur IDE-ID; unbekannte IDs zeigen VS Code.
pub fn ide_icon_for(ide_id: &str) -> ImageSource<'static> {
    match ide_id {
        "vs2022" | "vs" | "visualstudio" => ICON_VS,
        "rider" | "jetbrains" => ICON_RIDER,
        _ => ICON_VSCODE,
    }
}

/// Eingebettetes Icon zur Agent-ID; unbekannte IDs zeigen Claude.
pub fn agent_icon_for(agent_id: &str) -> ImageSource<'static> {
    match agent_id {
        "codex" => ICON_CODEX,
        "gemini" => ICON_GEMINI,
        "copilot" => ICON_COPILOT,
        "cursor" => ICON_CURSOR,
        "aider" => ICON_AIDER,
        _ => ICON_CLAUDE,
    }
}

/// Gecachtes `Path::exists` für Icon-Dateien: ein stat je Pfad und Prozess
/// statt je Repo-Zeile und Frame. Icons ändern sich zur Laufzeit praktisch
/// nie; eine später angelegte Datei greift ab dem Neustart.
pub fn icon_exists_cached(path: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(&hit) = cache.get(path) {
        return hit;
    }
    let exists = std::path::Path::new(path).exists();
    cache.insert(path.to_string(), exists);
    exists
}

/// `file://`-URI, wie sie der Loader von egui_extras versteht: lokale Pfade
/// als `file:///C:/x.png` bzw. `file:///home/x.png` (mit nur zwei Slashes
/// läse er unter Windows `C:` als Rechnernamen), UNC-Pfade als
/// `file://server/freigabe/x.png`.
pub fn file_uri(path: &str) -> String {
    let path = path.replace('\\', "/");
    match path.strip_prefix("//") {
        Some(unc) => format!("file://{unc}"),
        None => format!("file:///{}", path.trim_start_matches('/')),
    }
}

/// Eigenes Icon aus der Datei (`file://`, von egui_extras im Hintergrund
/// geladen und gecacht), sonst das eingebettete.
fn custom_or(icon: Option<&str>, fallback: ImageSource<'static>, size: f32) -> Image<'static> {
    let image = match icon {
        Some(path) if icon_exists_cached(path) => Image::new(file_uri(path)),
        _ => Image::new(fallback),
    };
    image.fit_to_exact_size(Vec2::splat(size))
}

/// Icon der IDE in `size` × `size` Punkten.
pub fn ide_image(ide: &crate::config::IdeConfig, size: f32) -> Image<'static> {
    custom_or(ide.icon.as_deref(), ide_icon_for(&ide.id), size)
}

/// Icon des Agenten in `size` × `size` Punkten.
pub fn agent_image(agent: &crate::config::AgentProfile, size: f32) -> Image<'static> {
    custom_or(agent.icon.as_deref(), agent_icon_for(&agent.id), size)
}

/// Icons der Merge-Request-Ansicht (`assets/icons/mr`).
pub mod mr {
    use egui::ImageSource;

    pub const ICON_OPEN_EXTERNAL: ImageSource =
        egui::include_image!("../../assets/icons/mr/arrow-square-out.svg");
    pub const ICON_ARROW_U: ImageSource =
        egui::include_image!("../../assets/icons/mr/arrow-u-up-left.svg");
    pub const ICON_CHAT_CHECK: ImageSource =
        egui::include_image!("../../assets/icons/mr/chat-circle-check.svg");
    pub const ICON_CHAT_DOTS: ImageSource =
        egui::include_image!("../../assets/icons/mr/chat-circle-dots.svg");
    pub const ICON_CHAT: ImageSource =
        egui::include_image!("../../assets/icons/mr/chat-circle.svg");
    pub const ICON_CHECK_CIRCLE: ImageSource =
        egui::include_image!("../../assets/icons/mr/check-circle.svg");
    pub const ICON_CHECK: ImageSource = egui::include_image!("../../assets/icons/mr/check.svg");
    pub const ICON_CIRCLE_NOTCH: ImageSource =
        egui::include_image!("../../assets/icons/mr/circle-notch.svg");
    pub const ICON_CLOCK: ImageSource = egui::include_image!("../../assets/icons/mr/clock.svg");
    pub const ICON_CODE: ImageSource = egui::include_image!("../../assets/icons/mr/code.svg");
    pub const ICON_COPY: ImageSource = egui::include_image!("../../assets/icons/mr/copy.svg");
    pub const ICON_DOWNLOAD: ImageSource =
        egui::include_image!("../../assets/icons/mr/download-simple.svg");
    pub const ICON_FOLDER_ARROW: ImageSource =
        egui::include_image!("../../assets/icons/mr/folder-arrow.svg");
    pub const ICON_FOLDER_PLUS: ImageSource =
        egui::include_image!("../../assets/icons/mr/folder-plus.svg");
    pub const ICON_FOLDER: ImageSource = egui::include_image!("../../assets/icons/mr/folder.svg");
    pub const ICON_BRANCH: ImageSource =
        egui::include_image!("../../assets/icons/mr/git-branch.svg");
    pub const ICON_PULL_REQUEST: ImageSource =
        egui::include_image!("../../assets/icons/mr/git-pull-request.svg");
    pub const ICON_LOCK: ImageSource =
        egui::include_image!("../../assets/icons/mr/lock-simple.svg");
    pub const ICON_SEARCH: ImageSource =
        egui::include_image!("../../assets/icons/mr/magnifying-glass.svg");
    pub const ICON_MINUS_CIRCLE: ImageSource =
        egui::include_image!("../../assets/icons/mr/minus-circle.svg");
    pub const ICON_PLAY_CIRCLE: ImageSource =
        egui::include_image!("../../assets/icons/mr/play-circle.svg");
    pub const ICON_PROHIBIT: ImageSource =
        egui::include_image!("../../assets/icons/mr/prohibit.svg");
    pub const ICON_SKIP_CIRCLE: ImageSource =
        egui::include_image!("../../assets/icons/mr/skip-forward-circle.svg");
    pub const ICON_SORT: ImageSource =
        egui::include_image!("../../assets/icons/mr/sort-descending.svg");
    pub const ICON_SPARKLE: ImageSource = egui::include_image!("../../assets/icons/mr/sparkle.svg");
    pub const ICON_TERMINAL: ImageSource =
        egui::include_image!("../../assets/icons/mr/terminal-window.svg");
    pub const ICON_USER: ImageSource = egui::include_image!("../../assets/icons/mr/user.svg");
    pub const ICON_WARNING: ImageSource =
        egui::include_image!("../../assets/icons/mr/warning-circle.svg");
    pub const ICON_X_CIRCLE: ImageSource =
        egui::include_image!("../../assets/icons/mr/x-circle.svg");
    pub const ICON_CROSS: ImageSource = egui::include_image!("../../assets/icons/mr/x.svg");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ide_and_agent_icons_map_ids_and_fall_back() {
        for (id, icon) in [
            ("vs2022", ICON_VS),
            ("visualstudio", ICON_VS),
            ("rider", ICON_RIDER),
            ("jetbrains", ICON_RIDER),
            ("vscode", ICON_VSCODE),
            ("unbekannt", ICON_VSCODE),
        ] {
            assert_eq!(ide_icon_for(id).uri(), icon.uri(), "{id}");
        }
        for (id, icon) in [
            ("claude", ICON_CLAUDE),
            ("codex", ICON_CODEX),
            ("gemini", ICON_GEMINI),
            ("copilot", ICON_COPILOT),
            ("cursor", ICON_CURSOR),
            ("aider", ICON_AIDER),
            ("unbekannt", ICON_CLAUDE),
        ] {
            assert_eq!(agent_icon_for(id).uri(), icon.uri(), "{id}");
        }
    }

    #[test]
    fn file_uri_for_local_and_unc_paths() {
        assert_eq!(file_uri(r"C:\Icons\vs.png"), "file:///C:/Icons/vs.png");
        assert_eq!(file_uri("/home/me/vs.svg"), "file:///home/me/vs.svg");
        assert_eq!(
            file_uri(r"\\server\share\vs.ico"),
            "file://server/share/vs.ico"
        );
    }

    #[test]
    fn icon_exists_cached_reports_fs_and_caches() {
        let dir = tempfile::tempdir().unwrap();
        let present = dir.path().join("present.png");
        std::fs::write(&present, b"x").unwrap();
        let missing = dir.path().join("missing.png");
        assert!(icon_exists_cached(present.to_str().unwrap()));
        assert!(!icon_exists_cached(missing.to_str().unwrap()));
        // Gecacht: nach dem Löschen weiterhin true (kein erneuter stat)
        std::fs::remove_file(&present).unwrap();
        assert!(icon_exists_cached(present.to_str().unwrap()));
    }
}
