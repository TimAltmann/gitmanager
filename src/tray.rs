#[cfg(target_os = "windows")]
mod imp {
    use std::sync::mpsc::{Receiver, Sender};
    use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct TrayChannels {
        pub tray_rx: Receiver<TrayIconEvent>,
        pub tray_icon: TrayIcon,
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

        let tray = TrayIconBuilder::new()
            .with_tooltip("GitManager")
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build()
            .ok()?;

        let (tray_tx, tray_rx): (Sender<TrayIconEvent>, Receiver<TrayIconEvent>) =
            std::sync::mpsc::channel();

        // Robust handler: forward payload + wake egui. No native menu - custom popup on left/right click.
        let tray_service_id = egui::ViewportId::from_hash_of("tray_service");
        let ctx_clone = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            let _ = tray_tx.send(ev);
            ctx_clone.request_repaint();
            ctx_clone.request_repaint_of(tray_service_id);
        }));

        Some(TrayChannels {
            tray_rx,
            tray_icon: tray,
        })
    }
}

#[cfg(target_os = "windows")]
pub use imp::*;
