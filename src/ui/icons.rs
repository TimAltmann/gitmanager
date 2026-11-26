//! Zentrale Icon-Definitionen und Helfer für die UI.
//!
//! Alle Icons werden zur Compile-Zeit via `egui::include_image!` eingebettet
//! und als statische Konstanten bereitgestellt, um redundante Einbettungen
//! und Duplikationen über verschiedene UI-Module hinweg zu vermeiden.

use egui::{Image, ImageSource, Response, Ui, Vec2};

// --- Allgemeine Icons (assets/icons/) ---
pub const ICON_SEARCH: ImageSource = egui::include_image!("../../assets/icons/search.svg");
pub const ICON_CROSS: ImageSource = egui::include_image!("../../assets/icons/cross.svg");
pub const ICON_FOLDER: ImageSource = egui::include_image!("../../assets/icons/folder.svg");
pub const ICON_TERMINAL: ImageSource = egui::include_image!("../../assets/icons/terminal.svg");
pub const ICON_PLUS: ImageSource = egui::include_image!("../../assets/icons/plus.svg");
pub const ICON_TRASH: ImageSource = egui::include_image!("../../assets/icons/trash.svg");
pub const ICON_REFRESH: ImageSource = egui::include_image!("../../assets/icons/refresh.svg");
pub const ICON_GEAR: ImageSource = egui::include_image!("../../assets/icons/gear.svg");
pub const ICON_WARNING: ImageSource = egui::include_image!("../../assets/icons/warning.svg");
pub const ICON_CHEVRON_DOWN: ImageSource =
    egui::include_image!("../../assets/icons/chevron-down.svg");
pub const ICON_CHEVRON_UP: ImageSource = egui::include_image!("../../assets/icons/chevron-up.svg");
pub const ICON_STAR: ImageSource = egui::include_image!("../../assets/icons/star.svg");
pub const ICON_STAR_FILL: ImageSource = egui::include_image!("../../assets/icons/star-fill.svg");
pub const ICON_EYE: ImageSource = egui::include_image!("../../assets/icons/eye.svg");
pub const ICON_EYE_OFF: ImageSource = egui::include_image!("../../assets/icons/eye-off.svg");
pub const ICON_GIT_BRANCH: ImageSource = egui::include_image!("../../assets/icons/git-branch.svg");
pub const ICON_GIT_COMMIT: ImageSource = egui::include_image!("../../assets/icons/git-commit.svg");
pub const ICON_DOTS: ImageSource = egui::include_image!("../../assets/icons/dots-three.svg");
pub const ICON_ARROW_UP: ImageSource = egui::include_image!("../../assets/icons/arrow-up.svg");
pub const ICON_ARROW_DOWN: ImageSource = egui::include_image!("../../assets/icons/arrow-down.svg");

// --- IDE & Agent Icons (assets/icons/) ---
pub const ICON_VSCODE: ImageSource = egui::include_image!("../../assets/icons/vscode.svg");
pub const ICON_VS: ImageSource = egui::include_image!("../../assets/icons/visualstudio.svg");
pub const ICON_RIDER: ImageSource = egui::include_image!("../../assets/icons/rider.svg");
pub const ICON_CLAUDE: ImageSource = egui::include_image!("../../assets/icons/claude.svg");
pub const ICON_CODEX: ImageSource = egui::include_image!("../../assets/icons/codex.svg");
pub const ICON_GEMINI: ImageSource = egui::include_image!("../../assets/icons/gemini.svg");
pub const ICON_COPILOT: ImageSource = egui::include_image!("../../assets/icons/copilot.svg");
pub const ICON_CURSOR: ImageSource = egui::include_image!("../../assets/icons/cursor.svg");
pub const ICON_AIDER: ImageSource = egui::include_image!("../../assets/icons/aider.svg");

// --- Merge Request Icons (assets/icons/mr/) ---
pub const ICON_MR_CHECK_CIRCLE: ImageSource =
    egui::include_image!("../../assets/icons/mr/check-circle.svg");
pub const ICON_MR_X_CIRCLE: ImageSource =
    egui::include_image!("../../assets/icons/mr/x-circle.svg");
pub const ICON_MR_CIRCLE_NOTCH: ImageSource =
    egui::include_image!("../../assets/icons/mr/circle-notch.svg");
pub const ICON_MR_CLOCK: ImageSource = egui::include_image!("../../assets/icons/mr/clock.svg");
pub const ICON_MR_MINUS_CIRCLE: ImageSource =
    egui::include_image!("../../assets/icons/mr/minus-circle.svg");
pub const ICON_MR_PROHIBIT: ImageSource =
    egui::include_image!("../../assets/icons/mr/prohibit.svg");
pub const ICON_MR_SKIP_CIRCLE: ImageSource =
    egui::include_image!("../../assets/icons/mr/skip-forward-circle.svg");
pub const ICON_MR_PLAY_CIRCLE: ImageSource =
    egui::include_image!("../../assets/icons/mr/play-circle.svg");
pub const ICON_MR_CHAT: ImageSource = egui::include_image!("../../assets/icons/mr/chat-circle.svg");
pub const ICON_MR_CHAT_DOTS: ImageSource =
    egui::include_image!("../../assets/icons/mr/chat-circle-dots.svg");
pub const ICON_MR_CHAT_CHECK: ImageSource =
    egui::include_image!("../../assets/icons/mr/chat-circle-check.svg");
pub const ICON_MR_CHECK: ImageSource = egui::include_image!("../../assets/icons/mr/check.svg");
pub const ICON_MR_ARROW_U: ImageSource =
    egui::include_image!("../../assets/icons/mr/arrow-u-up-left.svg");
pub const ICON_MR_USER: ImageSource = egui::include_image!("../../assets/icons/mr/user.svg");
pub const ICON_MR_SORT: ImageSource =
    egui::include_image!("../../assets/icons/mr/sort-descending.svg");
pub const ICON_MR_OPEN_EXTERNAL: ImageSource =
    egui::include_image!("../../assets/icons/mr/arrow-square-out.svg");
pub const ICON_MR_PULL_REQUEST: ImageSource =
    egui::include_image!("../../assets/icons/mr/git-pull-request.svg");
pub const ICON_MR_BRANCH: ImageSource =
    egui::include_image!("../../assets/icons/mr/git-branch.svg");
pub const ICON_MR_TERMINAL: ImageSource =
    egui::include_image!("../../assets/icons/mr/terminal-window.svg");
pub const ICON_MR_LOCK: ImageSource = egui::include_image!("../../assets/icons/mr/lock-simple.svg");
pub const ICON_MR_COPY: ImageSource = egui::include_image!("../../assets/icons/mr/copy.svg");
pub const ICON_MR_FOLDER_PLUS: ImageSource =
    egui::include_image!("../../assets/icons/mr/folder-plus.svg");
pub const ICON_MR_DOWNLOAD: ImageSource =
    egui::include_image!("../../assets/icons/mr/download-simple.svg");
pub const ICON_MR_FOLDER: ImageSource = egui::include_image!("../../assets/icons/mr/folder.svg");
pub const ICON_MR_FOLDER_ARROW: ImageSource =
    egui::include_image!("../../assets/icons/mr/folder-arrow.svg");
pub const ICON_MR_CODE: ImageSource = egui::include_image!("../../assets/icons/mr/code.svg");
pub const ICON_MR_SPARKLE: ImageSource = egui::include_image!("../../assets/icons/mr/sparkle.svg");
pub const ICON_MR_WARNING: ImageSource =
    egui::include_image!("../../assets/icons/mr/warning-circle.svg");
pub const ICON_MR_SEARCH: ImageSource =
    egui::include_image!("../../assets/icons/mr/magnifying-glass.svg");
pub const ICON_MR_CROSS: ImageSource = egui::include_image!("../../assets/icons/mr/x.svg");

/// Liefert das Icon für eine IDE-ID ("vs", "rider", "vscode", etc.).
pub fn ide_icon_for(ide_id: &str) -> ImageSource<'static> {
    match ide_id {
        "vs2022" | "vs" | "visualstudio" => ICON_VS,
        "rider" | "jetbrains" => ICON_RIDER,
        "vscode" => ICON_VSCODE,
        _ => ICON_VSCODE,
    }
}

/// Liefert das Icon für eine Agent-ID ("claude", "codex", "gemini", etc.).
pub fn agent_icon_for(agent_id: &str) -> ImageSource<'static> {
    match agent_id {
        "claude" => ICON_CLAUDE,
        "codex" => ICON_CODEX,
        "gemini" => ICON_GEMINI,
        "copilot" => ICON_COPILOT,
        "cursor" => ICON_CURSOR,
        "aider" => ICON_AIDER,
        _ => ICON_CLAUDE,
    }
}

/// Gecachter `Path.exists` für Icon-Dateien. Ein stat-Syscall pro Pfad
/// pro Prozess statt pro Repo-Zeile pro Frame.
pub fn icon_exists_cached(path: &str) -> bool {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
    let mut guard = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(&exists) = map.get(path) {
        return exists;
    }
    let exists = std::path::Path::new(path).exists();
    map.insert(path.to_string(), exists);
    exists
}

/// Erzeugt ein `egui::Image` für die übergebene IDE-Konfiguration in Standardgröße (18x18).
pub fn ide_image(ide: &crate::config::IdeConfig) -> Image<'static> {
    ide_image_sized(ide, 18.0)
}

/// Erzeugt ein `egui::Image` für die übergebene IDE-Konfiguration in Wunschgröße.
pub fn ide_image_sized(ide: &crate::config::IdeConfig, size: f32) -> Image<'static> {
    if let Some(path) = &ide.icon {
        if icon_exists_cached(path) {
            let uri = format!("file://{}", path.replace('\\', "/"));
            return Image::new(uri).fit_to_exact_size(Vec2::splat(size));
        }
    }
    Image::new(ide_icon_for(&ide.id)).fit_to_exact_size(Vec2::splat(size))
}

/// Erzeugt ein `egui::Image` für das übergebene Agent-Profil in Standardgröße (18x18).
pub fn agent_image(agent: &crate::config::AgentProfile) -> Image<'static> {
    agent_image_sized(agent, 18.0)
}

/// Erzeugt ein `egui::Image` für das übergebene Agent-Profil in Wunschgröße.
pub fn agent_image_sized(agent: &crate::config::AgentProfile, size: f32) -> Image<'static> {
    if let Some(path) = &agent.icon {
        if icon_exists_cached(path) {
            let uri = format!("file://{}", path.replace('\\', "/"));
            return Image::new(uri).fit_to_exact_size(Vec2::splat(size));
        }
    }
    Image::new(agent_icon_for(&agent.id)).fit_to_exact_size(Vec2::splat(size))
}

/// Erzeugt ein `egui::Image` mit gegebener quadratischer Kantenlänge und Einfärbung.
pub fn icon_image(icon: ImageSource<'static>, size: f32, tint: egui::Color32) -> Image<'static> {
    Image::new(icon)
        .fit_to_exact_size(Vec2::splat(size))
        .tint(tint)
}

/// Rahmen um einen Button beim Überfahren.
pub fn hover_frame(ui: &Ui, resp: &Response) {
    if resp.hovered() {
        ui.painter().rect_stroke(
            resp.rect,
            egui::CornerRadius::same(4),
            egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(100, 100, 100)),
            egui::StrokeKind::Outside,
        );
    }
}
