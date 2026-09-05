# Agent Guide — lokal bauen & testen

> Für AI-Agents / lokale Entwicklung. Diese Datei ist in `.gitignore` und wird nicht committet.

## 1) Toolchain / PATH-Falle

Rust ist via `rustup` in `$HOME/.cargo/bin` installiert, **nicht** im Standard-PATH für non-interactive Shells (opencode Bash, CI-Skripte). `cargo`/`rustc` sind Symlinks auf `rustup`.

**Immer zuerst:**
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo --version   # sollte 1.98.0 zeigen (rustc 1.98)
rustup --version
```
Ohne den Export kommt `cargo: command not found` — die häufigste Fehlerquelle hier. Alternative mit absolutem Pfad:
```bash
/home/tim/.cargo/bin/cargo --version
```

## 2) Schnell testen & linten (Linux, ohne GUI)

```bash
export PATH="$HOME/.cargo/bin:$PATH"

cargo test                  # 234 Tests, ~8s, nutzt vendored-libgit2/openssl (kein Netzwerk)
cargo test -- --list        # nur Tests auflisten
cargo test -- --nocapture   # mit stdout

cargo check                 # schneller Type-Check
cargo clippy -- -D warnings # CI muss grün sein (siehe .github/workflows/ci.yml)
cargo fmt --check           # Format prüfen
cargo fmt                   # Format fixen
```

## 3) Release-Build Windows `.exe` (lokal ohne Docker)

Benötigt mingw + Target:

```bash
sudo apt-get update && sudo apt-get install -y mingw-w64 clang pkg-config libssl-dev
sudo ln -sf /usr/bin/x86_64-w64-mingw32-windres /usr/bin/windres  # falls fehlend

rustup target add x86_64-pc-windows-gnu
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --target x86_64-pc-windows-gnu
# -> target/x86_64-pc-windows-gnu/release/gitmanager.exe
ls -lh target/x86_64-pc-windows-gnu/release/gitmanager.exe
file target/x86_64-pc-windows-gnu/release/gitmanager.exe
```

MSVC (kleiner, empfohlen für Store/AV, benötigt `cargo-xwin`):
```bash
cargo install cargo-xwin
cargo xwin build --release --target x86_64-pc-windows-msvc
# -> target/x86_64-pc-windows-msvc/release/gitmanager.exe
```

## 4) Docker (ohne lokale Toolchain, wie in README)

```bash
./scripts/build.sh                          # Linux/WSL → ./gitmanager.exe + target/...
# Windows PowerShell:
# .\scripts\build.ps1

docker compose run --rm dev cargo test
docker compose run --rm dev cargo check
docker compose run --rm dev cargo build --target x86_64-pc-windows-gnu --release
docker compose run --rm dev cargo xwin build --target x86_64-pc-windows-msvc --release
```

CI-Ablauf (`.github/workflows/ci.yml`): `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test --verbose` → `cargo build --release --target x86_64-pc-windows-gnu`.
> **„Testen“ heißt hier immer alle drei Checks** — `cargo test` allein reicht nicht. Vor jedem Push lokal `cargo fmt --check && cargo clippy -- -D warnings && cargo test` grün machen (siehe Abschnitt 2).

## 5) Architektur & Patterns (kurz)

**Stack:** `eframe 0.36 / egui 0.36 + egui_extras (svg)` (immediate-mode), `git2 0.21` (vendored-libgit2+openssl), `serde/serde_json`, `directories`, `rayon` + `walkdir`, `rfd`, `anyhow`, `image`.

**Aufbau:**
- `src/main.rs` → `src/app.rs:MyApp` (`eframe::App`): hält `AppConfig`, `Vec<RepoInfo>`, `mpsc`-Channels (`scan_tx/rx`, `launch_err`, `config_update`), scannt async via `scan_repos`; verwaltet `tray_popup_open`/`tray_popup_rect`/`window_visible`/`should_quit` + `record_usage()` für MRU, zeigt `tray_popup` via `show_viewport_immediate` (AlwaysOnTop, undecorated, transparent).
- `src/config.rs:AppConfig` (versioniert, aktuell `10`): atomarer Save (`*.tmp`→`rename`), `try_load()` mit Migration `v2..v10`, validiert/clamped (`branch_display_limit 50..500`, `max_depth 1..10`, `tray_branch_limit 5..50`, `tray_icons.max_display 5..50`), `LanguageProfile`/`IdeConfig`/`AgentProfile`/`RepoUiState` + `TrayIconConfig` (`icon_order`/`hidden_icon_ids`/`max_display`) + `RepoUsage` (`last_opened`/`last_branch_switch`/`last_config_change` + Counter, Key via `repo_state_key(path)` normalisiert auf Windows via `to_lowercase`).
- `src/scanner.rs`: `WalkDir` + `rayon` parallel, respektiert `max_depth`/`max_scan_depth` pro Profil, skippt `node_modules/target/.git`, liefert `RepoInfo` via `git::get_repo_info`.
- `src/git.rs`: Wrapper um `git2::Repository` (`get_branch`, `list_branches`, `checkout_branch*`, `stash_and_checkout`, `fetch_all`, `is_dirty`), + Launch-Logik `launch_ide`/`launch_agent`/`open_shell` mit Placeholder-Substitution `{file}/{dir}/{repo}`, Shell-Guard `use_shell && !allow_unsafe`, VS (`vswhere`) / Rider (Toolbox) Discovery mit `OnceLock<CachedPath>` + 5-min TTL + `*_force()`.
- `src/ui/`: `app.rs` zeichnet `TopBar`/`CentralPanel`(`repo_list`)/`StatusBar`/`BranchDialog`/`SettingsWindow` + `tray_popup` via `show_tray_popup_viewport`; `repo_list.rs` rendert Repo-Cards (Branch-/Solution-Dropdowns als `Window`+Filter, IDE/Agent/Explorer/Shell-Icons via `include_image!`); `settings.rs` Tabs `General/Profiles/Agents/Terminal/Appearance/Language/Icons/TrayIcons` mit `SettingsState::from_config`/`merge_auto_detected` + `show_tray_icons_tab` (max_display Slider + reorder/hide); `tray_popup.rs` rendert pro Repo 3-Zeilen-Card (Header, Branch-Dropdown, Tools-Zeile) mit `calculate_popup_position` + Fokus-Verlust-Auto-Close; `theme.rs` 5 Themes.
- `src/tray.rs` (nur Windows): `create_tray_channels` + `TrayIcon`/`Menu` mit synchronem `MenuEvent::set_event_handler` + Weiterleitung in `mpsc` (`tray_rx`/`menu_rx`) zur Erhaltung von `TrayIconEvent.rect` für exakte Popup-Positionierung über dem Tray-Icon.
- `src/i18n.rs`: `match (Language,key)` + `tr`/`tr_fmt`, Keys in `tabs_*`/`icons_*`/`tray_*`/`tray_icons_*`/`tray_max_display` etc.
- `assets/icons/*.svg` + `assets/demo.*`, `build.rs` (Windows-Icon), `docker-compose.yml`/`scripts/build.*` für Cross-Build.

**Patterns:** Immediate-mode (kein retained state, `ctx.request_repaint*`), `mpsc` für Hintergrund-Threads (Scan/Launch/Config-Update), versionierte Config-Migration, gefilterte/geordnete Views (`visible_ides()`/`filtered_agents()`), atomares Schreiben, TTL-Cache für teure Pfad-Discovery.

## 6) Troubleshooting

- `cargo: command not found` / `rustc: command not found` → PATH-Export vergessen, siehe Abschnitt 1.
- `which cargo` leer in opencode Bash ist normal (non-login shell), trotzdem vorhanden via `/home/tim/.cargo/bin/cargo`.
- Hohe Build-Zeit beim ersten Mal normal (git2 vendored baut libgit2/openssl).
- Tests brauchen keine GUI, nutzen `tempdir` + `git2::Repository::init` — laufen auch im Docker ohne Display.
- `target/` ist in `.gitignore`, `gitmanager.exe` im Root ebenfalls (`*.exe`).
- **Close Icon Rendering:** Use `egui::Button::image(egui::Image::new(ICON_CROSS).fit_to_exact_size(Vec2::splat(12.0))).small()` statt Text `"✕"`. SVG-Asset `assets/icons/cross.svg` muss via `egui::include_image!("../../assets/icons/cross.svg")` als `const ICON_CROSS: egui::ImageSource` eingebettet werden. Text-Fallbacks respektieren Theme/Font-Skalierung nicht und rendern DPI-abhängig inkonsistent.
- `tray_branch_limit`/`tray_icons.max_display` werden auf `5..50` geclamped, `branch_display_limit` auf `50..500` — 0-Werte werden zu Defaults korrigiert (20 bzw. 10/200) und im `try_load()` migriert.

## 7) UI Assets

SVG-Icons unter `assets/icons/*.svg` werden via `egui::include_image!` zur Compile-Zeit eingebettet (benötigt `egui_extras` mit `svg`-Feature). Beispiel `ICON_CROSS` in `src/ui/tray_popup.rs:19` / `src/app.rs:16` / `src/ui/settings.rs:12` — `egui::Button::image(egui::Image::new(ICON_CROSS).fit_to_exact_size(Vec2::splat(12.0))).small()` skaliert mit Theme/DPI, reines Text-`RichText::new("✕")` nicht. Alle Tray-Icons (`vscode.svg`, `visualstudio.svg`, `rider.svg`, `folder.svg`, `terminal.svg`, `claude.svg`, `codex.svg`, `gemini.svg`, `copilot.svg`, `cursor.svg`, `aider.svg`, `cross.svg`, `gear.svg`, `refresh.svg` etc.) folgen demselben Pattern.

## 8) Usage Tracking / MRU (Config v10)

**Struktur `RepoUsage`** (`src/config.rs:39`):
```rust
pub struct RepoUsage {
    pub last_opened: Option<u64>,        // Unix secs, gesetzt bei IDE/Explorer/Terminal/Agent Open
    pub last_branch_switch: Option<u64>, // bei checkout_branch*
    pub last_config_change: Option<u64>, // bei Custom-Select/Config-Change
    pub open_count: u32,
    pub branch_switch_count: u32,
    pub config_change_count: u32,
}
```
Gespeichert in `AppConfig.repo_usage: HashMap<String, RepoUsage>` mit Key `AppConfig::repo_state_key(path)` (`src/config.rs:1307`) — auf Windows `to_lowercase()` (case-insensitiv), sonst unverändert. Auf Windows werden sowohl `repo_state` als auch `repo_usage` beim Laden normalisiert (dedupliziert).

**`record_usage`** (`src/app.rs:385`):
```rust
fn record_usage(&mut self, repo_path: &Path, usage_type: UsageType)  // Open | BranchSwitch | ConfigChange
```
Holt `now = SystemTime::now().duration_since(UNIX_EPOCH).as_secs()`, setzt passendes `last_*` und inkrementiert Counter via `wrapping_add(1)`, dann `config.save()` (atomar). Aufrufe:
- `UsageType::Open` bei jedem Öffnen: `ide_open` (auch via `launch_ide` im Tray und Hauptfenster), `explorer_open` (`open_in_explorer`), `shell_open` (`open_shell`), `agent_open` (`launch_agent`) — `src/app.rs:631,666,683,701,1426,1463,1515,1533`.
- `UsageType::BranchSwitch` bei erfolgreichem `execute_branch_switch` — `src/app.rs:769`.
- `UsageType::ConfigChange` bei Custom-Config Selector-Wechsel — `src/app.rs:1345`.

**Sortierung im Tray** (`src/app.rs:484` in `show_tray_popup_viewport`):
Repos werden vor Anzeige nach `max(last_opened, last_branch_switch, last_config_change)` absteigend sortiert (`time_b.cmp(&time_a)`, 0 falls nie genutzt) und auf `tray_icons.max_display` gekürzt (`truncate`). Damit stehen zuletzt genutzte Repos oben (MRU).

**Config & UI:**
- Feld `tray_icons.max_display: usize` (`src/config.rs:26`) default `10`, `#[serde(default = "default_tray_max_display")]`, geclamped `5..50` in `try_load()` und vor Anzeige (`clamp(5,50)` in `app.rs:428,481`).
- Slider/DragValue im Tab **Tray Icons** (`src/ui/settings.rs:2730` `show_tray_icons_tab`): Label `tray_max_display` (`Max repos in tray popup` / `Max Repos im Tray-Popup`), Slider `5..=50`, `DragValue` `5..=50`, Anzeige `(aktuell: N)` + `Zeigt bis zu N Repos…`.
- Migration `v10` (`src/config.rs:967`): setzt `max_display` falls `0` auf `10`, clamped, dedup/sort `hidden_icon_ids`, dedup `icon_order`, füllt `icon_order` mit Defaults falls leer, ergänzt fehlende Default-IDs, initialisiert `repo_usage` falls leer, normalisiert Keys auf Windows, speichert und bumped `config_version` auf `10`.

## 9) Tray Icon Management (TrayIconConfig, Config v10)

**Struktur `TrayIconConfig`** (`src/config.rs:17`):
```rust
pub struct TrayIconConfig {
    pub icon_order: Vec<String>,     // Reihenfolge-IDs in Anzeigereihenfolge
    pub hidden_icon_ids: Vec<String>,// ausgeblendete IDs
    pub max_display: usize,          // siehe Abschnitt 8, Repos-Limit
}
```
Default `icon_order=[]`, `hidden_icon_ids=[]`, `max_display=10` (`TrayIconConfig::default`).

**Default-IDs** (`src/ui/settings.rs:2674` `DEFAULT_TRAY_ICON_IDS` + `src/config.rs:970`):
`["vscode","vs2022","rider","folder","terminal","claude","codex","gemini","copilot","cursor","aider"]` (11). In `effective_tray_order()` (`settings.rs:2696`) werden fehlende Defaults ergänzt, unbekannte Custom-IDs am Ende erhalten, Entfernung/Filter nur gegen bekannte Liste.

**Anzeige-Logik im Popup** (`src/ui/tray_popup.rs:320` `show_tray_repo_row`):
- `tray_hidden = &config.tray_icons.hidden_icon_ids`, `tray_order = &config.tray_icons.icon_order`, `order_idx = |id| tray_order.position(id).unwrap_or(MAX)`.
- IDEs: `profile.visible_ides()` gefiltert `!tray_hidden.contains(id)`, sortiert nach `tray_order`, `take(3)`. Explorer/Terminal jeweils nur falls nicht hidden. Agents: `profile.filtered_agents(...).filter(!hidden)`, sortiert, `take(2)` + Fallback auf ersten aktiven Agent falls Filter leer aber nicht hidden. `separator` nur falls sichtbar.

**Settings Tab Tray Icons** (`src/ui/settings.rs:2718` `show_tray_icons_tab` via `SettingsTab::TrayIcons` / `tabs_tray_icons`):
- Oben Slider für `max_display` (siehe Abschnitt 8).
- Darunter Liste `effective_tray_order(&state.draft)` (≈11 Zeilen) je mit `eye`/`eye_off` Toggle (add/remove aus `hidden_icon_ids`, danach `sort+dedup`), Name `(id)`, rechts `chevron-up`/`chevron-down` zum Tauschen im `icon_order` (`swap(idx, new_idx)`; falls `icon_order` noch leer, vorher `effective.clone()` übernehmen). Anzeige `icons_hidden_info` mit `hidden_icon_ids.join(", ")` oder `—`, Warnung wenn alle hidden, `icons_reset` Button leert `hidden_icon_ids`/`icon_order` und setzt `max_display=10`.
- Validierung in `config.rs:1025,1029`: `max_display.clamp(5,50)`, `hidden_icon_ids` sort/dedup, `icon_order` dedup retain (first wins).

**Persistenz:** Ändern im Draft → `Save` → `AppConfig::save()` serialisiert als JSON (`serde_json::to_string_pretty` → `*.tmp`→`rename`). Im Tray sofort via `config.clone()` beim nächsten `show_tray_popup_viewport` sichtbar, im Hauptfenster nach `poll_scan`/`config_update_rx`.

**i18n Keys** (`src/i18n.rs:320`): `tray_max_display`, `tray_icons_title`/`tray_icons_desc`, `tabs_tray_icons`, `icons_toggle_visibility`/`icons_hidden_info`/`icons_reset`.
