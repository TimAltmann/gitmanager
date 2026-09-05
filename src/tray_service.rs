// Tray service - dedicated deferred viewport for system tray
// Ensures tray popup works even when main window is hidden (Visible(false))
// Uses Arc + Mutex for efficient sharing, deferred viewport for independent event loop

#[cfg(target_os = "windows")]
mod imp {
    use crate::config::AppConfig;
    use crate::git::RepoInfo;
    use crate::ui::tray_popup::{self, TrayPopupActions};
    use egui::{Context, Rect, Vec2, ViewportBuilder, ViewportClass, ViewportId};
    use std::path::PathBuf;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Instant;

    /// Actions that tray viewport requests main app to perform
    #[derive(Debug)]
    pub enum TrayAction {
        ShowMainWindow,
        Refresh,
        OpenSettings,
        Quit,
        BranchSwitch(PathBuf, String),
        SolutionSelect(PathBuf, PathBuf),
        IdeOpen(PathBuf, String, PathBuf),
        AgentOpen(PathBuf, String),
        ExplorerOpen(PathBuf),
        ShellOpen(PathBuf),
    }

    /// Shared state between main app and tray service viewport
    /// Wrapped in Arc<Mutex<>> for Send+Sync required by deferred viewport
    pub struct TrayShared {
        // Data for popup rendering - use Arc to avoid expensive clones
        pub repos: Arc<Vec<RepoInfo>>,
        pub config: Arc<AppConfig>,
        // Popup state
        pub popup_open: bool,
        pub popup_rect: Option<Rect>,
        pub popup_opened_at: Option<Instant>,
        // Channel to send actions to main app
        pub action_tx: mpsc::Sender<TrayAction>,
    }

    impl TrayShared {
        pub fn new(action_tx: mpsc::Sender<TrayAction>) -> Self {
            Self {
                repos: Arc::new(Vec::new()),
                config: Arc::new(AppConfig::default()),
                popup_open: false,
                popup_rect: None,
                popup_opened_at: None,
                action_tx,
            }
        }

        pub fn update_data(&mut self, repos: Vec<RepoInfo>, config: AppConfig) {
            // Use Arc to avoid cloning large data on each read
            self.repos = Arc::new(repos);
            self.config = Arc::new(config);
        }
    }

    /// The deferred viewport callback - runs on tray service viewport's independent event loop
    /// This viewport is hidden (1x1 off-screen) but visible to OS, so it stays responsive
    pub fn tray_service_callback(
        ui: &mut egui::Ui,
        _class: ViewportClass,
        shared: Arc<Mutex<TrayShared>>,
        tray_rx: Arc<Mutex<mpsc::Receiver<tray_icon::TrayIconEvent>>>,
    ) {
        let ctx = ui.ctx().clone();

        // Poll tray events (nur TrayIconEvent für Custom-Popup, kein natives Menü)
        poll_tray_events(&shared, &tray_rx, &ctx);

        // Show popup if needed - as immediate child of tray service viewport
        let should_show = {
            let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
            guard.popup_open
        };

        if should_show {
            show_tray_popup_viewport(&shared, &ctx);
        }

        // Keep repainting to stay responsive, but throttle slightly to save CPU
        // Request repaint only if popup open or we want to poll quickly
        // For efficiency, use 500ms interval when idle (handler wakes immediately on event), immediate when popup open
        let is_popup_open = {
            let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
            guard.popup_open
        };
        if is_popup_open {
            // Gedrosselt statt Dauer-Repaint (F-18): ~30fps reicht für Popup
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        } else {
            // Poll at 500ms when idle - handler's request_repaint_for wakes immediately on tray event
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn toggle_popup(
        shared: &Arc<Mutex<TrayShared>>,
        ctx: &Context,
        rect: tray_icon::Rect,
    ) {
        let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
        let ppp = ctx.pixels_per_point();
        let ppp = if ppp == 0.0 { 1.0 } else { ppp };
        let tray_rect = Rect::from_min_size(
            egui::pos2(rect.position.x as f32 / ppp, rect.position.y as f32 / ppp),
            egui::vec2(rect.size.width as f32 / ppp, rect.size.height as f32 / ppp),
        );
        if guard.popup_open {
            guard.popup_open = false;
            guard.popup_rect = None;
            guard.popup_opened_at = None;
        } else {
            guard.popup_open = true;
            guard.popup_rect = Some(tray_rect);
            guard.popup_opened_at = Some(Instant::now());
        }
        ctx.request_repaint();
    }

    fn poll_tray_events(
        shared: &Arc<Mutex<TrayShared>>,
        tray_rx: &Arc<Mutex<mpsc::Receiver<tray_icon::TrayIconEvent>>>,
        ctx: &Context,
    ) {
        // Nur TrayIconEvent für Custom-Popup (kein natives Menü mehr).
        // This function only handles TrayIconEvent for custom popup.
        use tray_icon::MouseButton;
        use tray_icon::MouseButtonState;
        use tray_icon::TrayIconEvent;

        // Drain tray events
        let events: Vec<TrayIconEvent> = {
            let guard = tray_rx.lock().unwrap_or_else(|e| e.into_inner());
            let mut v = Vec::new();
            while let Ok(ev) = guard.try_recv() {
                v.push(ev);
            }
            v
        };

        for event in events {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                } => {
                    toggle_popup(shared, ctx, rect);
                }
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                } => {
                    // Rechtsklick togglet ebenfalls das Custom-Popup (kein natives Menü).
                    toggle_popup(shared, ctx, rect);
                }
                TrayIconEvent::Click {
                    button: MouseButton::Right,
                    ..
                } => {
                    // Right Down etc. ignorieren
                }
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => {
                    let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
                    if guard.popup_open {
                        guard.popup_open = false;
                        guard.popup_rect = None;
                        guard.popup_opened_at = None;
                    }
                    let _ = guard.action_tx.send(TrayAction::ShowMainWindow);
                    drop(guard);
                    ctx.request_repaint();
                    ctx.request_repaint_of(ViewportId::ROOT);
                }
                _ => {}
            }
        }
    }

    fn show_tray_popup_viewport(shared: &Arc<Mutex<TrayShared>>, ctx: &Context) {
        // Clone needed data
        let (tray_rect, popup_opened_at) = {
            let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
            let rect = guard.popup_rect.unwrap_or_else(|| {
                let screen = ctx.input(|i| {
                    i.viewport()
                        .monitor_size
                        .unwrap_or(Vec2::new(1920.0, 1080.0))
                });
                Rect::from_min_size(
                    egui::pos2(screen.x - 380.0, screen.y - 500.0),
                    egui::vec2(20.0, 20.0),
                )
            });
            (rect, guard.popup_opened_at)
        };

        let (repos_arc, config_arc) = {
            let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
            (guard.repos.clone(), guard.config.clone())
        };

        // Popup size - add extra height if any *sichtbare* repo has solution dropdown
        // (nur truncate-Menge prüfen, sonst Höhe bei vielen Repos überschätzt)
        let popup_width: f32 = 360.0;
        let tray_limit = config_arc.tray_icons.max_display.clamp(5, 50);
        let visible_repos = repos_arc.len().min(tray_limit);
        let has_solution_dropdown = repos_arc.iter().take(tray_limit).any(|r| r.solutions.len() > 1);
        let row_height: f32 = if has_solution_dropdown { 94.0 } else { 66.0 };
        let popup_height: f32 = (visible_repos as f32 * row_height + 90.0).clamp(280.0, 560.0);
        let popup_size = Vec2::new(popup_width, popup_height);

        // Position calculation (Heuristik F-14: nimmt horizontale, gleich große
        // Monitore mit Ursprung 0,0 an; monitor_size liefert keine Position.
        // Korrekt bräuchte winit-Monitor-API. Fallback unten: unten-rechts Primary.)
        let monitor_size = ctx.input(|i| {
            i.viewport()
                .monitor_size
                .unwrap_or(Vec2::new(1920.0, 1080.0))
        });
        let monitor_offset_x =
            if tray_rect.center().x > monitor_size.x || tray_rect.center().x < 0.0 {
                (tray_rect.center().x / monitor_size.x).floor() * monitor_size.x
            } else {
                0.0
            };
        let monitor_offset_y =
            if tray_rect.center().y > monitor_size.y || tray_rect.center().y < 0.0 {
                (tray_rect.center().y / monitor_size.y).floor() * monitor_size.y
            } else {
                0.0
            };
        let screen_rect = Rect::from_min_max(
            egui::pos2(monitor_offset_x, monitor_offset_y),
            egui::pos2(
                monitor_offset_x + monitor_size.x,
                monitor_offset_y + monitor_size.y,
            ),
        );
        let popup_pos = tray_popup::calculate_popup_position(tray_rect, popup_size, screen_rect);

        let mut close_popup = false;
        let mut tray_actions = TrayPopupActions::default();
        let viewport_id = ViewportId::from_hash_of("tray_popup");
        let builder = ViewportBuilder::default()
            .with_title("GitManager Tray")
            .with_inner_size(popup_size)
            .with_position(popup_pos)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop)
            .with_close_button(false)
            .with_minimize_button(false)
            .with_maximize_button(false);

        // MRU sort and truncate
        let mut repos_clone = (*repos_arc).clone();
        repos_clone.sort_by(|a, b| {
            let usage_a = config_arc
                .repo_usage
                .get(&crate::config::AppConfig::repo_state_key(&a.path));
            let usage_b = config_arc
                .repo_usage
                .get(&crate::config::AppConfig::repo_state_key(&b.path));
            let time_a = usage_a
                .map(|u| {
                    u.last_opened
                        .unwrap_or(0)
                        .max(u.last_branch_switch.unwrap_or(0))
                        .max(u.last_config_change.unwrap_or(0))
                })
                .unwrap_or(0);
            let time_b = usage_b
                .map(|u| {
                    u.last_opened
                        .unwrap_or(0)
                        .max(u.last_branch_switch.unwrap_or(0))
                        .max(u.last_config_change.unwrap_or(0))
                })
                .unwrap_or(0);
            time_b.cmp(&time_a)
        });
        repos_clone.truncate(tray_limit);
        let config_clone = (*config_arc).clone();

        let viewport_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
                if class == ViewportClass::EmbeddedWindow {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.label("Tray popup (embedded) – viewports not supported");
                        if ui.button("Schließen").clicked() {
                            close_popup = true;
                        }
                    });
                    return;
                }
                crate::ui::theme::apply_theme(ctx, &config_clone.theme);
                // install_image_loaders gehört einmalig in MyApp::new (F-19), nicht pro Frame
                if ctx.input(|i| i.viewport().close_requested()) {
                    close_popup = true;
                }
                egui::CentralPanel::default().show(ctx, |ui| {
                    tray_popup::show_tray_popup_ui(
                        ui,
                        &mut repos_clone,
                        &config_clone,
                        &mut tray_actions,
                    );
                });
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close_popup = true;
                }
                if tray_actions.close_popup {
                    close_popup = true;
                }
                if tray_actions.quit {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if tray_actions.open_main || tray_actions.open_settings || tray_actions.quit {
                    close_popup = true;
                }
                if tray_actions.branch_switch.is_some()
                    || tray_actions.solution_select.is_some()
                    || tray_actions.ide_open.is_some()
                    || tray_actions.agent_open.is_some()
                    || tray_actions.explorer_open.is_some()
                    || tray_actions.shell_open.is_some()
                {
                    close_popup = true;
                }
                if popup_opened_at
                    .map(|t| t.elapsed() > std::time::Duration::from_millis(500))
                    .unwrap_or(true)
                    && ctx.input(|i| {
                        i.events
                            .iter()
                            .any(|e| matches!(e, egui::Event::WindowFocused(false)))
                    })
                {
                    close_popup = true;
                }
            });
        }));

        if viewport_result.is_err() {
            eprintln!("Tray popup viewport panicked, closing popup");
            let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
            guard.popup_open = false;
            guard.popup_rect = None;
            guard.popup_opened_at = None;
            return;
        }

        // Handle close
        if close_popup {
            let mut guard = shared.lock().unwrap_or_else(|e| e.into_inner());
            guard.popup_open = false;
            guard.popup_rect = None;
            guard.popup_opened_at = None;
        }

        // Send actions to main app and wake main viewport (service is throttled, main needs instant)
        let needs_wake = tray_actions.open_main
            || tray_actions.open_settings
            || tray_actions.quit
            || tray_actions.refresh
            || tray_actions.branch_switch.is_some()
            || tray_actions.solution_select.is_some()
            || tray_actions.ide_open.is_some()
            || tray_actions.agent_open.is_some()
            || tray_actions.explorer_open.is_some()
            || tray_actions.shell_open.is_some();
        {
            let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
            if tray_actions.refresh {
                let _ = guard.action_tx.send(TrayAction::Refresh);
            }
            if tray_actions.open_main {
                let _ = guard.action_tx.send(TrayAction::ShowMainWindow);
            }
            if tray_actions.quit {
                let _ = guard.action_tx.send(TrayAction::Quit);
            }
            if let Some((path, branch)) = tray_actions.branch_switch {
                let _ = guard.action_tx.send(TrayAction::BranchSwitch(path, branch));
            }
            if let Some((repo_path, sln_path)) = tray_actions.solution_select {
                let _ = guard
                    .action_tx
                    .send(TrayAction::SolutionSelect(repo_path, sln_path));
            }
            if let Some((path, ide_id, file)) = tray_actions.ide_open {
                let _ = guard
                    .action_tx
                    .send(TrayAction::IdeOpen(path, ide_id, file));
            }
            if let Some((path, agent_id)) = tray_actions.agent_open {
                let _ = guard.action_tx.send(TrayAction::AgentOpen(path, agent_id));
            }
            if let Some(path) = tray_actions.explorer_open {
                let _ = guard.action_tx.send(TrayAction::ExplorerOpen(path));
            }
            if let Some(path) = tray_actions.shell_open {
                let _ = guard.action_tx.send(TrayAction::ShellOpen(path));
            }
            if tray_actions.open_settings {
                let _ = guard.action_tx.send(TrayAction::OpenSettings);
            }
        }
        if needs_wake {
            // Wake main viewport immediately (service viewport is 500ms throttled)
            ctx.request_repaint_of(ViewportId::ROOT);
        }
    }

    pub fn create_tray_service_viewport(
        ctx: &Context,
        shared: Arc<Mutex<TrayShared>>,
        tray_rx: Arc<Mutex<mpsc::Receiver<tray_icon::TrayIconEvent>>>,
    ) {
        let viewport_id = ViewportId::from_hash_of("tray_service");
        // Hidden service viewport: 1x1 off-screen, but visible to OS so not throttled
        // Use with_visible(true) + off-screen pos to keep event loop running
        // But with_taskbar(false) so no taskbar entry
        let builder = ViewportBuilder::default()
            .with_title("GitManager Tray Service")
            .with_inner_size(Vec2::new(1.0, 1.0))
            .with_position(egui::pos2(-10000.0, -10000.0))
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_taskbar(false)
            .with_visible(true) // Keep visible to avoid throttling
            .with_close_button(false)
            .with_minimize_button(false)
            .with_maximize_button(false);

        ctx.show_viewport_deferred(viewport_id, builder, move |ui, class| {
            tray_service_callback(ui, class, shared.clone(), tray_rx.clone());
        });
    }
}

#[cfg(target_os = "windows")]
pub use imp::{create_tray_service_viewport, TrayAction, TrayShared};
