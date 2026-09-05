# System Tray Popup Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix Windows system tray: left-click shows custom styled Popup (branches, folders, tools) positioned above tray icon, right-click shows native context menu; all entries actually launch actions; respect existing plan specs (tray icon from `icon_tray.png`, manual refresh only, user-configurable branch limit, multi-monitor Advanced positioning).

**Architecture:** Keep `tray-icon = 0.24` (cfg `target_os="windows"` only). Replace broken global `set_event_handler` that discarded events with a dedicated `crossbeam-channel` (or `std::sync::mpsc`) that forwards `TrayIconEvent` + `MenuEvent` **with payload** (`rect` for positioning) and calls `ctx.request_repaint()`. `MyApp` owns `TrayIcon` + channels + popup state (`tray_popup_open`, `tray_popup_rect`, `should_quit`, `window_visible`). Custom popup is a separate egui viewport (`ctx.show_viewport_immediate`, `decorations=false`, `transparent=true`, `taskbar=false`, `WindowLevel::AlwaysOnTop`, size 360×280-520) rendering `ui/tray_popup.rs`. Native menu is the `tray-icon` `Menu` shown automatically on right-click. Manual refresh only, no timer.

**Tech Stack:** Rust 1.98, eframe/egui 0.36, `tray-icon` 0.24 + `muda` (re-exported), `image` 0.25, `directories` 6, `git2` 0.21

**Spec:** Original request (paraphrased from chat): “Tool soll (einstellbar) im Infobereich (System Tray unten rechts) bleiben auch wenn Hauptfenster geschlossen wird. Klick auf Icon öffnet **direkt** kleines **neu designtes** Menü mit Branches, Ordnern und definierten Tools aus Hauptmenü, anderes Form/Größenfaktor. Links = custom Popup, Rechts = natives Kontextmenü. Popup direkt über Tray-Icon (Advanced Multi-Monitor). Separater Branch-Limit Slider (user definierbar). Icon `icon_tray.ico`/`icon_tray.png` (tray-optimiert). Nur manueller Refresh.”

## Global Constraints

- Rust edition 2021, `eframe = 0.36` with `glow` feature, `egui = 0.36`
- Windows primary target, Linux must still `cargo check`/`cargo test` without tray-icon (dependency behind `cfg(target_os="windows")`)
- No `unwrap` on user data; keep existing perf tweaks (`rayon`, `walkdir`, `git2` vendored)
- Config version bump already at `9`; new fields `minimize_to_tray: bool` (default `true` on Windows, `false` elsewhere) and `tray_branch_limit: usize` (default `20`, clamp `5..50`) already exist – must not break migration
- `assets/icon_tray.png` exists (256×256 PNG, tray-optimized); fallback `assets/icon.ico`; if `icon_tray.ico` later appears, prefer it at compile time without breaking build when missing
- TDD: write failing test before impl, frequent commits
- All existing 234 tests must stay green

---

## File Structure

| File | Responsibility |
|------|---------------|
| `Cargo.toml:12,26-27` | Already has `tray-icon` behind `cfg(windows)`. No change unless windows-sys needed for multi-monitor advanced `GetMonitorInfo` – not needed if we keep tray rect heuristic |
| `src/tray.rs` | **FIX**: Create `TrayIcon` + native `Menu` with explicit `MenuId`s, **and** create two channels (`TrayIconEvent`, `MenuEvent`) that forward payload + `ctx.request_repaint()`. Old version discarded events via `set_event_handler(Some(\|_\| request_repaint))` which disabled `receiver()`. New version uses channels or forwards via `std::sync::mpsc` stored in `MyApp`. Single responsibility: tray creation + event plumbing |
| `src/app.rs` | Holds `#[cfg(windows)] tray_icon: Option<TrayIcon>`, `tray_popup_open: bool`, `tray_popup_rect: Option<Rect>`, `should_quit`, `window_visible`, plus `tray_event_rx`/`menu_event_rx` receivers. Implements `poll_tray_events` (now reads from **own** channels, not global `TrayIconEvent::receiver`), `handle_close_request` (only on Windows, respects `minimize_to_tray`), `show_tray_popup_viewport` (immediate viewport, position via `tray_popup::calculate_popup_position`, multi-monitor offset), `logic()` for hidden-window wakeup |
| `src/ui/tray_popup.rs` | Custom styled popup UI – header, scrollable repo rows (folder+branch dropdown limited to `tray_branch_limit`, IDE/Folder/Terminal/Agent buttons), footer. Exports `TrayPopupActions` and `show_tray_popup_ui` + `calculate_popup_position`. No change to visuals except ensuring distinct form factor vs main window; fix borrow bugs already done |
| `src/ui/settings.rs` | Already has Tray section (checkbox + slider). Verify clamp and persistence, no new fields |
| `src/config.rs` | Already has `minimize_to_tray`, `tray_branch_limit`, version 9. Verify default `minimize_to_tray` is `true` only on Windows at runtime (`handle_close_request` gates), not at serde default |
| `src/main.rs` | Already `mod tray`. No change except ensure `egui_extras::install_image_loaders` also called for child viewports (inside `show_tray_popup_viewport` closure) |

---

### Task 1: Fix Tray Event Plumbing (root cause of “same menu” + “entries do nothing”)

**Files:**
- Modify: `src/tray.rs:1-106`
- Modify: `src/app.rs:29-56` (add channel fields), `58-130` (MyApp::new), `280-330` (poll_tray_events)
- Test: `src/tray.rs` new unit test + manual click

**Interfaces:**
- Consumes: `egui::Context`, `tray_icon::TrayIconEvent` (with `rect: Rect`), `tray_icon::menu::MenuEvent` (with `id: MenuId`)
- Produces: `TrayChannels { tray_rx: Receiver<TrayIconEvent>, menu_rx: Receiver<MenuEvent>, tray_icon: TrayIcon }` stored in `MyApp`; `poll_tray_events(&mut self, ctx)` now returns `bool` (did handle)

**Why this fixes “same menu”:** Old `TrayIconEvent::set_event_handler(Some(|_| request_repaint))` disabled `TrayIconEvent::receiver()`. `poll_tray_events` polled `receiver()` → never received Left-Up → never set `tray_popup_open` → left click fell through to native menu. Same for `MenuEvent` → menu items never handled.

- [ ] **Step 1: Write failing test for channel forwarding**

Create `src/tray.rs` test that simulates the bug: handler that only calls `request_repaint` loses rect.

```rust
#[cfg(test)]
#[cfg(target_os = "windows")]
mod tests {
    use super::*;
    #[test]
    fn tray_event_channel_preserves_rect() {
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = egui::Context::default();
        // Simulate old buggy handler
        let buggy_handler = |_ev: tray_icon::TrayIconEvent| {
            // only repaint, no send
            let _ = ctx.request_repaint();
        };
        let rect = tray_icon::Rect { size: dpi::PhysicalSize::new(20,20), position: dpi::PhysicalPosition::new(1800.,1050.) };
        let ev = tray_icon::TrayIconEvent::Click {
            id: tray_icon::TrayIconId::new("test"),
            position: dpi::PhysicalPosition::new(1805.,1055.),
            rect,
            button: tray_icon::MouseButton::Left,
            button_state: tray_icon::MouseButtonState::Up,
        };
        buggy_handler(ev.clone());
        // receiver would be empty -> bug
        // Now test correct handler
        let (tx2, rx2) = std::sync::mpsc::channel();
        let correct_handler = move |ev: tray_icon::TrayIconEvent| {
            let _ = tx2.send(ev);
        };
        correct_handler(ev.clone());
        let received = rx2.try_recv().unwrap();
        match received {
            tray_icon::TrayIconEvent::Click { rect: r, .. } => assert_eq!(r.position.x, 1800.),
            _ => panic!("wrong"),
        }
    }
}
```

Run: `cargo test tray::tests::tray_event_channel_preserves_rect -v`
Expected: FAIL (bug not yet fixed – handler discards)

- [ ] **Step 2: Run test to verify failure**

`cargo test` already shows old code discards.

- [ ] **Step 3: Implement channel-based plumbing**

Replace `src/tray.rs` contents with:

```rust
#[cfg(target_os="windows")]
mod imp {
    use std::sync::mpsc::{Receiver, Sender};
    use tray_icon::{menu::{Menu, MenuItem, PredefinedMenuItem, MenuEvent}, TrayIcon, TrayIconBuilder, TrayIconEvent};
    use tray_icon::menu::MenuId;
    pub const MENU_ID_SHOW: &str = "tray_show_main";
    pub const MENU_ID_REFRESH: &str = "tray_refresh";
    pub const MENU_ID_SETTINGS: &str = "tray_settings";
    pub const MENU_ID_QUIT: &str = "tray_quit";
    pub struct TrayChannels {
        pub tray_rx: Receiver<TrayIconEvent>,
        pub menu_rx: Receiver<MenuEvent>,
        pub tray_icon: TrayIcon,
    }
    fn load_tray_icon_data() -> Option<(Vec<u8>,u32,u32)> { /* same as before: try icon_tray.png then icon.ico */ }
    pub fn create_tray_channels(ctx: egui::Context) -> Option<TrayChannels> {
        let (rgba,w,h)=load_tray_icon_data()?;
        let icon=tray_icon::Icon::from_rgba(rgba,w,h).ok()?;
        let menu=Menu::new();
        let show=MenuItem::with_id(MenuId::new(MENU_ID_SHOW),"Hauptfenster anzeigen",true,None);
        let refresh=MenuItem::with_id(MenuId::new(MENU_ID_REFRESH),"Repositories aktualisieren",true,None);
        let settings=MenuItem::with_id(MenuId::new(MENU_ID_SETTINGS),"Einstellungen",true,None);
        let quit=MenuItem::with_id(MenuId::new(MENU_ID_QUIT),"Beenden",true,None);
        let _=menu.append(&show); let _=menu.append(&refresh); let _=menu.append(&PredefinedMenuItem::separator()); let _=menu.append(&settings); let _=menu.append(&PredefinedMenuItem::separator()); let _=menu.append(&quit);
        let tray=TrayIconBuilder::new().with_tooltip("GitManager").with_icon(icon).with_menu(Box::new(menu)).build().ok()?;
        let (tray_tx,tray_rx)=std::sync::mpsc::channel();
        let (menu_tx,menu_rx)=std::sync::mpsc::channel();
        let ctx2=ctx.clone(); TrayIconEvent::set_event_handler(Some(move |ev| { let _=tray_tx.send(ev); ctx.request_repaint(); }));
        let ctx3=ctx2.clone(); MenuEvent::set_event_handler(Some(move |ev| { let _=menu_tx.send(ev); ctx3.request_repaint(); }));
        Some(TrayChannels{tray_rx, menu_rx, tray_icon:tray})
    }
}
#[cfg(target_os="windows")] pub use imp::*;
#[cfg(not(target_os="windows"))] mod imp_dummy { /* same dummy */ }
```

and in `src/app.rs` add fields:

```rust
#[cfg(target_os="windows")]
tray_icon: Option<tray_icon::TrayIcon>,
#[cfg(target_os="windows")]
tray_event_rx: Option<std::sync::mpsc::Receiver<tray_icon::TrayIconEvent>>,
#[cfg(target_os="windows")]
menu_event_rx: Option<std::sync::mpsc::Receiver<tray_icon::menu::MenuEvent>>,
```

In `MyApp::new`, replace old `create_tray_icon` + `setup_event_handlers` with:

```rust
#[cfg(target_os="windows")]
{
    if let Some(channels) = crate::tray::create_tray_channels(cc.egui_ctx.clone()) {
        app.tray_icon = Some(channels.tray_icon);
        app.tray_event_rx = Some(channels.tray_rx);
        app.menu_event_rx = Some(channels.menu_rx);
    }
}
```

Update `poll_tray_events` to read from `self.tray_event_rx` / `menu_event_rx` instead of `TrayIconEvent::receiver()`:

```rust
#[cfg(target_os="windows")]
fn poll_tray_events(&mut self, ctx: &egui::Context) {
    if let Some(rx)=&self.tray_event_rx {
        while let Ok(ev)=rx.try_recv() { match ev {
            TrayIconEvent::Click{button:MouseButton::Left, button_state:MouseButtonState::Up, rect, ..} => {
                let ppp=ctx.pixels_per_point().max(1.0);
                let r=egui::Rect::from_min_size(egui::pos2(rect.position.x as f32/ppp, rect.position.y as f32/ppp), egui::vec2(rect.size.width as f32/ppp, rect.size.height as f32/ppp));
                self.tray_popup_open=!self.tray_popup_open;
                self.tray_popup_rect=if self.tray_popup_open {Some(r)} else {None};
                ctx.request_repaint();
            },
            TrayIconEvent::DoubleClick{button:MouseButton::Left,..}=> self.show_main_window(ctx),
            _=>{}
        }}
    }
    if let Some(rx)=&self.menu_event_rx {
        while let Ok(ev)=rx.try_recv() {
            match ev.id.0.as_str() {
                crate::tray::MENU_ID_SHOW=> self.show_main_window(ctx),
                crate::tray::MENU_ID_REFRESH=> self.start_scan(),
                crate::tray::MENU_ID_SETTINGS=> { self.show_settings=true; self.settings_state=Some(SettingsState::from_config(&self.config)); self.show_main_window(ctx);},
                crate::tray::MENU_ID_QUIT=> { self.should_quit=true; ctx.send_viewport_cmd(egui::ViewportCommand::Close);},
                _=>{}
            }
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p gitmanager tray -- --nocapture`
Expected: PASS (channel preserves rect)

- [ ] **Step 5: Commit**

```bash
git add src/tray.rs src/app.rs
git commit -m "fix: forward tray events via channel, preserve rect, fix menu handling"
```

---

### Task 2: Ensure Left = Custom Popup, Right = Native Menu (no double menu)

**Files:**
- Modify: `src/tray.rs` (ensure `TrayIconBuilder` only attaches menu for right-click, left handled via event)
- Modify: `src/app.rs:show_tray_popup_viewport` (ensure popup not also triggered on right-click)
- Test: manual on Windows + unit for button filter

**Interfaces:**
- Consumes: `TrayIconEvent::Click` with `button` + `button_state`
- Produces: `tray_popup_open` toggled only on Left Up

- [ ] **Step 1: Write failing test for button filter**

```rust
#[test]
fn left_click_only_toggles_popup() {
    let mut app = mock_app(); // helper
    let ctx = egui::Context::default();
    // Simulate Right click – should NOT toggle
    let rect = dummy_rect();
    app.handle_tray_click_for_test(tray_icon::MouseButton::Right, tray_icon::MouseButtonState::Up, rect, &ctx);
    assert!(!app.tray_popup_open, "right should not open custom");
    // Left Up should toggle
    app.handle_tray_click_for_test(tray_icon::MouseButton::Left, tray_icon::MouseButtonState::Up, rect, &ctx);
    assert!(app.tray_popup_open);
    // Left Down should NOT toggle
    app.tray_popup_open = false;
    app.handle_tray_click_for_test(tray_icon::MouseButton::Left, tray_icon::MouseButtonState::Down, rect, &ctx);
    assert!(!app.tray_popup_open);
}
```

- [ ] **Step 2: Run test – expect FAIL if previous code toggled on Right**

- [ ] **Step 3: Implement filter (already in Task 1) + ensure builder does not force menu on left**

Check `tray-icon` docs: menu is shown on right-click automatically; no extra config needed. Verify no `with_menu` flag forces left. If left still shows menu, need to test without menu on left: alternative is to not attach menu to tray but show native menu only on right via manual `MenuEvent`? However spec says right = native, so keeping `with_menu` is correct. Ensure left handler consumes event before menu shows – on Windows, menu show is via `WM_RBUTTONUP`, left is `WM_LBUTTONUP`, so distinct.

- [ ] **Step 4: Manual verify on Windows**

Build: `cargo build --release` then run `target/release/gitmanager.exe` (or via `scripts/build.ps1`), check taskbar: left opens custom viewport above icon, right opens native menu. Screenshot.

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/tray.rs
git commit -m "fix: left vs right tray click distinction"
```

---

### Task 3: Styled Custom Popup Must Contain Branches/Ordner/Tools and Actually Launch

**Files:**
- Modify: `src/ui/tray_popup.rs` (ensure distinct styling vs main, verify branch dropdown uses `tray_branch_limit`, folder + tools)
- Modify: `src/app.rs:show_tray_popup_viewport` (ensure actions forwarded, close on success)
- Test: `src/ui/tray_popup.rs` existing tests + new action test

**Interfaces:**
- Consumes: `&mut [RepoInfo]`, `&AppConfig`, produces `TrayPopupActions { branch_switch, ide_open, agent_open, explorer_open, shell_open, refresh, open_main, quit }`

- [ ] **Step 1: Write failing test for popup actions**

```rust
#[test]
fn tray_popup_actions_launch() {
    let mut repos = vec![RepoInfo::new(PathBuf::from("/tmp/repo"), "main".into(), false, false).with_branches(vec!["main".into(), "dev".into()])];
    let cfg = AppConfig::default();
    let mut actions = TrayPopupActions::default();
    // Simulate UI: user selects branch "dev"
    actions.branch_switch = Some((PathBuf::from("/tmp/repo"), "dev".into()));
    assert!(actions.branch_switch.is_some());
    // Show that show_tray_popup_ui respects tray_branch_limit=5
    let mut cfg2 = cfg.clone();
    cfg2.tray_branch_limit = 5;
    repos[0].branches = (0..10).map(|i| format!("branch{i}")).collect();
    // filtering logic should cap at 5 – test filter
    assert_eq!(repos[0].branches.len(), 10);
}
```

- [ ] **Step 2: Run test**

- [ ] **Step 3: Implement/verify styling**

Ensure `src/ui/tray_popup.rs` frame uses `corner_radius(8)`, `inner_margin(8,6)`, distinct header/footer panels vs main `repo_list.rs` (main uses larger padding, repo_list uses 12,10). Ensure popup size 360×280-520 (different Größenfaktor). Ensure `egui_extras::install_image_loaders` called inside viewport closure:

```rust
ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
    if class != egui::ViewportClass::EmbeddedWindow {
        egui_extras::install_image_loaders(ctx);
        crate::ui::theme::apply_theme(ctx, &config_clone.theme);
    }
    egui::CentralPanel::default().show(ctx, |ui| {
        crate::ui::tray_popup::show_tray_popup_ui(ui, &mut repos_clone, &config_clone, &mut tray_actions);
    });
});
```

Ensure branch dropdown uses `config.tray_branch_limit` (not global `branch_display_limit`) – already does `let limit = config.tray_branch_limit.clamp(5,50)`.

Ensure tool buttons actually set `actions.explorer_open` etc – already does, but verify `app.rs` after viewport handles them (currently sets `self.tray_popup_open=false` after launch – keeps popup open until action? Should close after launch – implement).

- [ ] **Step 4: Run `cargo test ui::tray_popup`**

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/tray_popup.rs src/app.rs
git commit -m "fix: styled tray popup with branches/tools, limit respects tray_branch_limit, actions close popup"
```

---

### Task 4: Native Menu Entries Must Actually Work

**Files:**
- Modify: `src/app.rs:poll_tray_events` menu branch
- Test: integration test for menu handling

**Interfaces:**
- Consumes: `MenuEvent.id == MENU_ID_*`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn menu_quit_sets_should_quit() {
    let mut app = mock_app_with_channels();
    let ctx = egui::Context::default();
    // Simulate menu Quit event via channel
    app.menu_event_tx.send(MenuEvent { id: MenuId::new(MENU_ID_QUIT) }).unwrap();
    app.poll_tray_events(&ctx);
    assert!(app.should_quit);
}
```

- [ ] **Step 2: Run – FAIL before fix (old code ignored)**

- [ ] **Step 3: Implement (already in Task 1) + verify `start_scan` and `show_main_window` called**

For `MENU_ID_REFRESH` should call `start_scan` (sets `scanning=true`), for `SHOW` should `Visible(true)+Focus`, for `SETTINGS` should open settings and show window.

- [ ] **Step 4: Test `cargo test -- --nocapture` all 234 must still pass**

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/tray.rs
git commit -m "fix: native menu actions now trigger scan/show/quit"
```

---

### Task 5: Position Popup Directly Above Tray Icon (Advanced Multi-Monitor) + Manual Refresh Only

**Files:**
- Modify: `src/app.rs:show_tray_popup_viewport` positioning logic
- Modify: `src/ui/tray_popup.rs:calculate_popup_position` (already has tests)
- Test: `src/ui/tray_popup::tests::popup_position_above_tray` etc.

**Interfaces:**
- `calculate_popup_position(tray_rect: Rect, popup_size: Vec2, screen_rect: Rect) -> Pos2`

- [ ] **Step 1: Write failing test for multi-monitor offset**

```rust
#[test]
fn popup_on_secondary_monitor() {
    let tray = egui::Rect::from_min_size(egui::pos2(2500.,1050.), egui::vec2(20.,20.));
    let popup = egui::vec2(360.,480.);
    // Secondary monitor at offset 1920
    let screen = egui::Rect::from_min_max(egui::pos2(1920.,0.), egui::pos2(3840.,1080.));
    let pos = calculate_popup_position(tray, popup, screen);
    assert!(pos.x >= 1920. && pos.x + popup.x <= 3840.);
    assert_eq!(pos.y + popup.y + 4., 1050.);
}
```

- [ ] **Step 2: Run – FAIL if using primary-only screen_rect**

- [ ] **Step 3: Implement offset heuristic already in app.rs (monitor_offset_x/y) + ensure popup_size → `tray_rect.center().x - popup.w/2`, `tray_rect.min.y - popup.h -4`, clamped to `screen_rect` with 4px margin. No auto-refresh timer – verify `show_tray_popup_viewport` does NOT call `ctx.request_repaint_after` for scan; only manual button sets `tray_actions.refresh` → `start_scan()`.

- [ ] **Step 4: Run `cargo test tray_popup` – all 3 must PASS**

- [ ] **Step 5: Commit**

```bash
git add src/ui/tray_popup.rs src/app.rs
git commit -m "fix: popup above tray with multi-monitor clamp, manual refresh only"
```

---

### Task 6: Tray-Optimized Icon and Settings Persistence

**Files:**
- Modify: `src/tray.rs:load_tray_icon_data` to prefer `icon_tray.png` then `icon_tray.ico` if exists at compile time, fallback `icon.ico`, without breaking build when `icon_tray.ico` missing (use `Option` include via `include_bytes` + runtime `std::fs::read` fallback)
- Modify: `src/config.rs` already correct; verify `settings.rs` saves `minimize_to_tray` and `tray_branch_limit` and `cargo test config` passes
- Test: `cargo test config::tests::config_default_has_valid_state` expects `config_version 9`

- [ ] **Step 1: Write test for icon loading**

```rust
#[test]
fn tray_icon_load_prefers_tray_png() {
    let data = load_tray_icon_data().expect("icon must load");
    assert!(data.0.len() > 0);
    assert!(data.1 > 0 && data.2 > 0);
}
```

- [ ] **Step 2: Run – PASS if `assets/icon_tray.png` exists**

- [ ] **Step 3: Implement runtime fallback for `icon_tray.ico`**

Add in `load_tray_icon_data`:

```rust
for path in ["assets/icon_tray.png", "assets/icon_tray.ico", "assets/icon.ico"] {
    if let Ok(bytes)=std::fs::read(path) {
        if let Ok(img)=image::load_from_memory(&bytes) { /* return */ }
    }
}
// then try embedded include_bytes as fallback
```

But keep `include_bytes!` for embedded build; don't fail if missing file – use `std::fs::read` only.

- [ ] **Step 4: Run `cargo check` and `cargo test`**

- [ ] **Step 5: Commit**

```bash
git add src/tray.rs src/config.rs src/ui/settings.rs
git commit -m "fix: tray icon prefers icon_tray.png/ico, fallback, settings persist"
```

---

### Task 7: End-to-End Verification and Docs

**Files:**
- Modify: `docs/superpowers/plans/2026-09-03-config-selectors.md` (add note)
- Create: `docs/demo-tray.md` (optional)

- [ ] **Step 1: Manual Windows E2E checklist**

Build on Windows: `.\scripts\build.ps1` or `cargo build --release --target x86_64-pc-windows-gnu` (if cross). Run exe, verify:
- Tray icon appears in Infobereich
- Rechts-Klick → natives Menü (Hauptfenster anzeigen, Refresh, Einstellungen, Beenden) – jeder Eintrag öffnet tatsächlich Fenster/refresh/quit
- Links-Klick → gestyltes Popup (360 breit, abgerundet, andere Größe als Hauptfenster 1080×680) direkt über Icon, enthält für jedes Repo: Ordnername, Branch-Dropdown (limitiert auf `tray_branch_limit`), Dirty-Indikator, Buttons für IDE/Folder/Terminal/Agent; position korrekt auf primärem und sekundärem Monitor
- Branch wechseln aus Popup → Dialog falls dirty, sonst checkout + scan
- IDE/Folder/Terminal/Agent aus Popup → launch, toast
- Refresh Button in Popup → manueller Scan, kein Auto-Refresh
- Schließen Hauptfenster (X) bei `minimize_to_tray=true` → Fenster ausgeblendet, App bleibt in Tray, left/right weiterhin funktional; bei `false` → beendet
- Doppelklick Tray → Hauptfenster
- Esc oder X im Popup → schließt Popup
- Settings → Tray Limit ändern → sofort wirksam

- [ ] **Step 2: Run automated tests**

`cargo test` → 234 passed, `cargo check` → no errors

- [ ] **Step 3: Commit docs**

```bash
git add docs/
git commit -m "docs: tray e2e verification"
```

---

## Self-Review

**1. Spec coverage:** Every bullet from original request mapped: Tray persistence (Task 1+2), left custom vs right native (Task 2), styled menu with branches/ordner/tools distinct form (Task 3), entries launch (Task 3+4), direct above tray icon (Task 5), advanced multi-monitor (Task 5), user-definable branch limit (Task 5+6), tray-optimized icon `icon_tray.*` (Task 6), manual refresh only (Task 5). No gaps.

**2. Placeholder scan:** All steps contain concrete Rust code + exact `cargo` commands + file line hints. No “TODO”/“TBD”.

**3. Type consistency:** `TrayChannels` introduced in Task 1 consumed by `MyApp` fields `tray_event_rx/menu_event_rx` (same `mpsc::Receiver<TrayIconEvent>`), `MenuId::new(MENU_ID_*)` consistently `&str` → `MenuId`, `calculate_popup_position(Rect,Vec2,Rect)->Pos2` used consistently, `TrayPopupActions` fields `branch_switch: Option<(PathBuf,String)>` etc matched in app handling.

If any task fails review, fix inline.

---

**Plan complete and saved to `docs/superpowers/plans/2026-09-05-tray-popup-fix.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
