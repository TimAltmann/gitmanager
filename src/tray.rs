#[cfg(target_os = "windows")]
mod imp {
    use crate::config::TrayIconTheme;
    use std::sync::mpsc::{Receiver, Sender};
    use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct TrayChannels {
        pub tray_rx: Receiver<TrayIconEvent>,
        pub tray_icon: TrayIcon,
    }

    fn decode_to_rgba(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
        if bytes.is_empty() {
            return None;
        }
        let image = image::load_from_memory(bytes).ok()?;
        let rgba = image.to_rgba8();
        let (w, h) = rgba.dimensions();
        if w == 0 || h == 0 {
            return None;
        }
        Some((rgba.into_raw(), w, h))
    }

    /// Eingebettete Bytes für das Theme (Dark = weiß, Light = dunkel).
    fn embedded_bytes_for_theme(theme: TrayIconTheme) -> &'static [&'static [u8]] {
        match theme {
            TrayIconTheme::Dark => {
                const DARK: &[&[u8]] = &[
                    include_bytes!("../assets/icon_tray.png") as &[u8],
                    include_bytes!("../assets/icon.ico") as &[u8],
                ];
                DARK
            }
            TrayIconTheme::Light => {
                const LIGHT: &[&[u8]] = &[
                    include_bytes!("../assets/icon_tray_light.png") as &[u8],
                    include_bytes!("../assets/icon.png") as &[u8],
                    include_bytes!("../assets/icon.ico") as &[u8],
                ];
                LIGHT
            }
        }
    }

    fn load_tray_icon_data_for_theme(theme: TrayIconTheme) -> Option<(Vec<u8>, u32, u32)> {
        // Filesystem-Versuch nur in Debug (cargo run aus Repo-Root); im installierten
        // Release-EXE liegt assets/ nicht neben dem Binary (Dead Code + Syscall).
        #[cfg(debug_assertions)]
        {
            let fs_candidates: &[&str] = match theme {
                TrayIconTheme::Dark => &["assets/icon_tray.png", "assets/icon_tray.ico"],
                TrayIconTheme::Light => &[
                    "assets/icon_tray_light.png",
                    "assets/icon.png",
                    "assets/icon_tray.ico",
                ],
            };
            for fs_path in fs_candidates {
                if let Ok(bytes) = std::fs::read(fs_path) {
                    if let Some(data) = decode_to_rgba(&bytes) {
                        return Some(data);
                    }
                }
            }
        }
        for bytes in embedded_bytes_for_theme(theme) {
            if let Some(data) = decode_to_rgba(bytes) {
                return Some(data);
            }
        }
        // Fallback: jeweils anderes Theme (falls ein Asset fehlt).
        let fallback = match theme {
            TrayIconTheme::Dark => TrayIconTheme::Light,
            TrayIconTheme::Light => TrayIconTheme::Dark,
        };
        for bytes in embedded_bytes_for_theme(fallback) {
            if let Some(data) = decode_to_rgba(bytes) {
                return Some(data);
            }
        }
        None
    }

    #[allow(dead_code)]
    fn load_tray_icon_data() -> Option<(Vec<u8>, u32, u32)> {
        // Prefer tray-optimized icon: embedded icon_tray.png, then icon.ico.
        // Default Dark = bisheriges Verhalten (Weiß-Icon).
        load_tray_icon_data_for_theme(TrayIconTheme::Dark)
    }

    pub fn build_tray_icon(theme: TrayIconTheme) -> Option<tray_icon::Icon> {
        let (rgba, w, h) = load_tray_icon_data_for_theme(theme)?;
        tray_icon::Icon::from_rgba(rgba, w, h).ok()
    }

    /// Wechselt das Tray-Icon zur Laufzeit (z.B. nach Settings-Save).
    pub fn set_tray_icon_theme(tray_icon: &TrayIcon, theme: TrayIconTheme) -> bool {
        if let Some(icon) = build_tray_icon(theme) {
            tray_icon.set_icon(Some(icon)).is_ok()
        } else {
            false
        }
    }

    #[allow(dead_code)]
    pub fn create_tray_channels(ctx: egui::Context) -> Option<TrayChannels> {
        create_tray_channels_with_theme(ctx, TrayIconTheme::Dark)
    }

    pub fn create_tray_channels_with_theme(
        ctx: egui::Context,
        theme: TrayIconTheme,
    ) -> Option<TrayChannels> {
        let (rgba, w, h) = load_tray_icon_data_for_theme(theme)?;
        let icon = tray_icon::Icon::from_rgba(rgba, w, h).ok()?;

        let tray = TrayIconBuilder::new()
            .with_tooltip("GitManager")
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build()
            .ok()?;

        let (tray_tx, tray_rx): (Sender<TrayIconEvent>, Receiver<TrayIconEvent>) =
            std::sync::mpsc::channel();

        // Robust handler: forward payload + wake service viewport.
        // No native menu - custom popup on left/right click.
        // Nur Service-Viewport wecken (N6): ROOT-Repaint wäre bei hidden Main sinnlos.
        let tray_service_id = egui::ViewportId::from_hash_of("tray_service");
        let ctx_clone = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            let _ = tray_tx.send(ev);
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
