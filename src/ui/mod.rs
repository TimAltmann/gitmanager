pub mod repo_list;
pub mod settings;
pub mod theme;
// Nur auf Windows via tray_service live verdrahtet; auf Linux nur für Tests
// (calculate_popup_position) kompiliert — daher dead_code dort erlaubt,
// auf Windows bleibt -D warnings strikt.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub mod tray_popup;
