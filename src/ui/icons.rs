//! Alle eingebetteten SVG-Icons an einer Stelle. `include_image!` leitet die
//! Bild-URI aus dem Pfad ab: eine Konstante je Datei heißt auch ein Eintrag
//! im Bild-Cache je Datei.

use egui::ImageSource;

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
    pub const ICON_DOTS: ImageSource = egui::include_image!("../../assets/icons/mr/dots-three.svg");
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
