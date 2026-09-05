#[cfg(target_os = "windows")]
mod imp {
    use std::sync::mpsc::{Receiver, Sender};
    use tray_icon::menu::MenuId;
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
        TrayIcon, TrayIconBuilder, TrayIconEvent,
    };

    pub const MENU_ID_SHOW: &str = "tray_show_main";
    pub const MENU_ID_REFRESH: &str = "tray_refresh";
    pub const MENU_ID_SETTINGS: &str = "tray_settings";
    pub const MENU_ID_QUIT: &str = "tray_quit";

    pub struct TrayChannels {
        pub tray_rx: Receiver<TrayIconEvent>,
        pub menu_rx: Receiver<MenuEvent>,
        pub tray_icon: TrayIcon,
        pub tray_menu: Menu,
    }

    fn load_tray_icon_data() -> Option<(Vec<u8>, u32, u32)> {
        // Prefer tray-optimized icon: try embedded icon_tray.png, then icon_tray.ico if present, fallback to icon.ico
        // Also try runtime filesystem fallback for icon_tray.ico/png in case user placed file after compile
        for fs_path in ["assets/icon_tray.png", "assets/icon_tray.ico"] {
            if let Ok(bytes) = std::fs::read(fs_path) {
                if let Ok(image) = image::load_from_memory(&bytes) {
                    let rgba = image.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    if w > 0 && h > 0 {
                        return Some((rgba.into_raw(), w, h));
                    }
                }
            }
        }
        let try_embedded: &[&[u8]] = &[
            include_bytes!("../assets/icon_tray.png"),
            include_bytes!("../assets/icon.ico"),
        ];
        for bytes in try_embedded {
            if bytes.is_empty() {
                continue;
            }
            if let Ok(image) = image::load_from_memory(bytes) {
                let rgba = image.to_rgba8();
                let (w, h) = rgba.dimensions();
                return Some((rgba.into_raw(), w, h));
            }
        }
        None
    }

    pub fn create_tray_channels(ctx: egui::Context) -> Option<TrayChannels> {
        let (rgba, w, h) = load_tray_icon_data()?;
        let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;

        let menu = Menu::new();
        let show_item = MenuItem::with_id(
            MenuId::new(MENU_ID_SHOW),
            "Hauptfenster anzeigen",
            true,
            None,
        );
        let refresh_item = MenuItem::with_id(
            MenuId::new(MENU_ID_REFRESH),
            "Repositories aktualisieren",
            true,
            None,
        );
        let settings_item =
            MenuItem::with_id(MenuId::new(MENU_ID_SETTINGS), "Einstellungen", true, None);
        let quit_item = MenuItem::with_id(MenuId::new(MENU_ID_QUIT), "Beenden", true, None);

        let _ = menu.append(&show_item);
        let _ = menu.append(&refresh_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&settings_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit_item);

        let tray = TrayIconBuilder::new()
            .with_tooltip("GitManager")
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build()
            .ok()?;
        // Attach menu dynamically via set_menu (tray created without with_menu per Task 2 spec).
        // Use set_menu to keep Menu stored in TrayChannels for later detach/attach in poll_tray_events.
        // Also disables left-click menu via with_menu_on_left_click(false) as defense-in-depth.
        tray.set_menu(Some(Box::new(menu.clone())));

        let (tray_tx, tray_rx): (Sender<TrayIconEvent>, Receiver<TrayIconEvent>) =
            std::sync::mpsc::channel();
        let (menu_tx, menu_rx): (Sender<MenuEvent>, Receiver<MenuEvent>) =
            std::sync::mpsc::channel();

        // Robust handlers: forward payload + wake egui. This disables the global receiver() but we use our own channels.
        // Use cloned Context for repaint.
        let ctx_clone = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            let _ = tray_tx.send(ev);
            ctx_clone.request_repaint();
        }));
        let ctx_clone2 = ctx;
        MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
            let _ = menu_tx.send(ev);
            ctx_clone2.request_repaint();
        }));

        Some(TrayChannels {
            tray_rx,
            menu_rx,
            tray_icon: tray,
            tray_menu: menu,
        })
    }

    // Keep old helpers for compatibility but now they delegate
    pub fn create_tray_icon() -> Option<TrayIcon> {
        // Fallback: create without channels (not used anymore)
        let (rgba, w, h) = load_tray_icon_data()?;
        let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;
        let menu = Menu::new();
        let show_item = MenuItem::with_id(
            MenuId::new(MENU_ID_SHOW),
            "Hauptfenster anzeigen",
            true,
            None,
        );
        let refresh_item = MenuItem::with_id(
            MenuId::new(MENU_ID_REFRESH),
            "Repositories aktualisieren",
            true,
            None,
        );
        let settings_item =
            MenuItem::with_id(MenuId::new(MENU_ID_SETTINGS), "Einstellungen", true, None);
        let quit_item = MenuItem::with_id(MenuId::new(MENU_ID_QUIT), "Beenden", true, None);
        let _ = menu.append(&show_item);
        let _ = menu.append(&refresh_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&settings_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit_item);
        let tray = TrayIconBuilder::new()
            .with_tooltip("GitManager")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .ok()?;
        Some(tray)
    }

    pub fn setup_event_handlers(_ctx: egui::Context) {
        // Deprecated: use create_tray_channels instead
    }

    pub use tray_icon::menu::MenuEvent as TrayMenuEvent;
    pub use tray_icon::TrayIconEvent as TrayEvent;
    pub use tray_icon::{MouseButton, MouseButtonState};
}

#[cfg(target_os = "windows")]
pub use imp::*;

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
mod imp_dummy {
    // Dummy types for non-windows to allow compilation
    pub const MENU_ID_SHOW: &str = "";
    pub const MENU_ID_REFRESH: &str = "";
    pub const MENU_ID_SETTINGS: &str = "";
    pub const MENU_ID_QUIT: &str = "";
    pub fn create_tray_icon() -> Option<()> {
        None
    }
    pub fn setup_event_handlers(_ctx: egui::Context) {}
}

#[cfg(not(target_os = "windows"))]
#[allow(unused_imports)]
pub use imp_dummy::{
    create_tray_icon, setup_event_handlers, MENU_ID_QUIT, MENU_ID_REFRESH, MENU_ID_SETTINGS,
    MENU_ID_SHOW,
};
