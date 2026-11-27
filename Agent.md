# Agent Guide — lokal bauen & testen

> Für AI-Agents / lokale Entwicklung. Die Datei ist versioniert.

## 1) Toolchain / PATH-Falle

Rust ist via `rustup` in `$HOME/.cargo/bin` installiert, **nicht** im Standard-PATH für non-interactive Shells (opencode Bash, CI-Skripte). `cargo`/`rustc` sind Symlinks auf `rustup`.

**Immer zuerst:**
```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo --version   # stable (CI nutzt `stable`, das Docker-Image eine feste Version)
rustup --version
```
Ohne den Export kommt `cargo: command not found` — die häufigste Fehlerquelle hier. Alternative mit absolutem Pfad:
```bash
$HOME/.cargo/bin/cargo --version
```

## 2) Schnell testen & linten (Linux, ohne GUI)

```bash
export PATH="$HOME/.cargo/bin:$PATH"

cargo test                  # über 1000 Tests, nutzt vendored-libgit2/openssl (kein Netzwerk, kein glab)
cargo test -- --list        # nur Tests auflisten
cargo test -- --nocapture   # mit stdout

cargo check                 # schneller Type-Check
cargo clippy -- -D warnings # CI muss grün sein (siehe .github/workflows/ci.yml)
cargo fmt --check           # Format prüfen
cargo fmt                   # Format fixen
```

## 3) Release-Build Windows `.exe` (lokal ohne Docker)

Empfohlen in dieser Umgebung (WSL ohne sudo-Passwort) via Homebrew — einmalig:

```bash
brew install mingw-w64   # liefert x86_64-w64-mingw32-gcc + -windres (~1,6 GB)
ln -sf /home/linuxbrew/.linuxbrew/bin/x86_64-w64-mingw32-windres $HOME/.local/bin/windres
rustup target add x86_64-pc-windows-gnu
```

Bauen (Dauer ca. 5 min, `~/.local/bin` für `windres` + Linuxbrew für den Cross-GCC müssen im PATH sein):

```bash
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
cargo build --release --target x86_64-pc-windows-gnu
# -> target/x86_64-pc-windows-gnu/release/gitmanager.exe
ls -lh target/x86_64-pc-windows-gnu/release/gitmanager.exe
file target/x86_64-pc-windows-gnu/release/gitmanager.exe
```

Alternative mit sudo (Rechner mit apt-Rechten, wie CI in `.github/workflows/ci.yml`):

```bash
sudo apt-get update && sudo apt-get install -y mingw-w64 clang pkg-config libssl-dev
sudo ln -sf /usr/bin/x86_64-w64-mingw32-windres /usr/bin/windres  # falls fehlend
rustup target add x86_64-pc-windows-gnu
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --target x86_64-pc-windows-gnu
```

Test-Build mit anderer Version (z. B. `0.0.1` für Updater-Test — Latest Release ist `v0.0.4`, daher schlägt der Update-Dialog an). `CARGO_PKG_VERSION` kommt aus `Cargo.toml`, die Windows-Manifest-Version in `build.rs` wird daraus abgeleitet:

```bash
cp Cargo.toml /tmp/opencode_Cargo.toml.bak && cp Cargo.lock /tmp/opencode_Cargo.lock.bak
sed -i -E '0,/^version = ".*"/s//version = "0.0.1"/' Cargo.toml  # nur [package]-Zeile!
grep -m1 '^version' Cargo.toml
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
cargo build --release --target x86_64-pc-windows-gnu
strings target/x86_64-pc-windows-gnu/release/gitmanager.exe | grep -m2 'v0.0.1'
cp target/x86_64-pc-windows-gnu/release/gitmanager.exe ./gitmanager-0.0.1-test.exe
cp /tmp/opencode_Cargo.toml.bak Cargo.toml && cp /tmp/opencode_Cargo.lock.bak Cargo.lock
grep -m1 '^version' Cargo.toml  # muss wieder 0.1.1 zeigen
```

Hinweis: In CI schreibt der Step `Sync version from tag` die Tag-Version automatisch nach `Cargo.toml` (nur bei `refs/tags/v*`); lokal ist das obige `sed`-Backup-Restore das Äquivalent.

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

**DoD über Docker** (ohne lokales `cargo`, Image mit rustfmt + clippy):

```bash
docker compose run --rm dev sh -c "cargo fmt --check && cargo clippy -- -D warnings && cargo test --verbose"
# zusätzlich bei Windows-Code (cfg(windows)-Nachbarschaft, app.rs, Prozessstart wie glab/git CLI):
docker compose run --rm dev cargo clippy --target x86_64-pc-windows-gnu -- -D warnings
docker compose run --rm dev cargo build --release --target x86_64-pc-windows-gnu
```

Hinweis: Das Image nutzt eine feste rustc-Version, CI `stable` — neue clippy-Lints können abweichen.

CI-Ablauf (`.github/workflows/ci.yml`, exakt diese Reihenfolge): `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test --verbose` → `cargo build --release --target x86_64-pc-windows-gnu`.
> **Pflicht-Definition-of-Done bei JEDER Aufgabe:** „Fertig“ heißt immer **alle CI-Stages lokal grün** in obiger Reihenfolge — kein Fix/Feature ist done, solange eine Stage rot ist. `cargo test` allein reicht nicht; auch `fmt`+`clippy` allein reichen nicht. Vor jedem Push/Commit lokal ausführen:
> ```bash
> export PATH="$HOME/.cargo/bin:$PATH"
> cargo fmt --check && cargo clippy -- -D warnings && cargo test --verbose
> ```
> Wer Windows-Code (`tray*`, `#[cfg(target_os = "windows")]`, `assets/`, `build.rs`) angefasst hat, zusätzlich:
> ```bash
> cargo build --release --target x86_64-pc-windows-gnu
> ```
> Hintergrund: `clippy` läuft in CI auf Linux — Windows-only-Code (`tray_popup`, `tray_service`) ist dort `dead_code` und muss per `cfg_attr(..., allow(dead_code))` gegated sein; ungenutzte Stubs (z. B. `poll_tray_events`) ersatzlos löschen statt `allow`. Ein grünes `cargo test` beweist nichts über `fmt`/`clippy`/`Windows-Build` (siehe Abschnitt 2).

## 5) Architektur & Patterns (kurz)

**Stack:** `eframe 0.36 / egui 0.36 + egui_extras (svg, file, image)` (immediate-mode), `git2 0.21` (vendored-libgit2+openssl), `serde/serde_json`, `directories`, `rayon` + `walkdir`, `rfd`, `anyhow`, `image`.

**Aufbau:**
- `src/main.rs` → `src/app.rs:MyApp` (`eframe::App`): hält `AppConfig`, `Vec<RepoInfo>`, `mpsc`-Channels (`scan_tx/rx`, `launch_err`, `detected_ides`, `config_saved`), scannt async via `scan_repos`; verwaltet `window_visible`/`should_quit` + `record_usage()` für MRU; das Tray-Popup zeigt `src/tray_service.rs` via `show_viewport_immediate` (AlwaysOnTop, undecorated, transparent).
- `src/config.rs:AppConfig` (versioniert, aktuell `15`): atomarer Save (`ConfigSnapshot::write`: `*.tmp`→`rename`, auch aus dem Schreib-Thread), `load()` sichert eine unlesbare Datei als `config.json.bad` und meldet den Fehler, `try_load()` mit Migration `v2..v15` (ein Save am Ende), validiert/clamped (`branch_display_limit 50..500`, `max_depth 1..10`, `tray_branch_limit 5..50`, `tray_icons.max_display 5..50`), `LanguageProfile`/`IdeConfig`/`AgentProfile`/`RepoUiState` + `TrayIconConfig` (`icon_order`/`hidden_icon_ids`/`max_display`) + `RepoUsage` (`last_opened`/`last_branch_switch`/`last_config_change` + Counter, Key via `repo_state_key(path)` normalisiert auf Windows via `to_lowercase`).
- **Config v15 / `MrConfig`** (`AppConfig.merge_requests`, `#[serde(default)]` auf Feld- und Struct-Ebene): glab-Programm, Intervall, Chip-Vorgabe „Alle offenen“, Bot-Approvals, `include_nonlocal`, Opt-out-Liste `excluded_projects` (`host/full/path`, klein), `extra_hosts`, `host_aliases`, `review_root`, „Danach öffnen“. `MrConfig::normalize()` läuft in `try_load()` und vor dem Speichern in den Einstellungen. **Downgrade:** Eine ältere Version (< v15) ignoriert `merge_requests` und verwirft es beim nächsten Speichern; nach erneutem Update gelten die Defaults. Das Token liegt nie in der Config, sondern in glab.
- `src/scanner.rs`: `WalkDir` + `rayon` parallel, respektiert `max_depth`/`max_scan_depth` pro Profil, skippt `node_modules/target/.git`, liefert `RepoInfo` via `git::get_repo_info`.
- `src/git/`: Wrapper um `git2::Repository`, aufgeteilt in `mod.rs` (Typen `RepoInfo` u. a., `get_branch`, `list_branches_in`, `is_dirty`, `ahead_behind_in`, `get_repo_info`), `fetch.rs` (`fetch_all`, `fast_forward`, Zugangsdaten, Rückfall auf System-`git`), `checkout.rs` (`checkout_branch*`, `stash_and_checkout`), `worktree.rs` (MR-Checkout, `add_worktree`/`create_worktree`, `clone_repo`), `cleanup.rs` (`branch_safety`, `remove_worktree`, `delete_local_branch`), `launch.rs` (`launch_ide`/`launch_agent`/`open_shell`/`open_in_explorer` mit Placeholder-Substitution `{file}/{dir}/{repo}`, Shell-Guard `use_shell && !allow_unsafe`), `ide_discovery.rs` (VS via `vswhere` / Rider via Toolbox mit `OnceLock<CachedPath>` + 5-min TTL + `*_force()`) und `tests.rs`. Alle öffentlichen Namen bleiben unter `crate::git::…` erreichbar (`pub use` in `mod.rs`).
- `src/ui/`: `app.rs` zeichnet `TopBar`/`CentralPanel`(`repo_list`)/`StatusBar`/`BranchDialog`/`SettingsWindow` (das Tray-Popup zeichnet `tray_service.rs:show_tray_popup_viewport`); `repo_list.rs` rendert Repo-Cards (Branch-/Solution-Dropdowns als `Window`+Filter, IDE/Agent/Explorer/Shell-Icons via `include_image!`); `settings.rs` (Fenster, `SettingsState::from_config`, `DetectedIde::apply`, Tabs Terminal/Appearance/Language) + je Tab eine Datei unter `settings/` (`general.rs`, `profiles.rs`, `agents.rs`, `icons.rs`, `tray_icons.rs` mit max_display Slider + reorder/hide, `merge_requests.rs`); `icons.rs` bündelt alle `include_image!`-Konstanten (MR-Icons in `icons::mr`) samt `ide_icon_for`/`agent_icon_for`, `ide_image`/`agent_image` (eigene Icons als `file_uri`, Existenz gecacht), `widgets.rs` kleine gemeinsame Bausteine (`icon_image`, `icon_button`, `hover_outline`, `banner`, `culled`); `tray_popup.rs` rendert pro Repo 3-Zeilen-Card (Header, Branch-Dropdown, Tools-Zeile) mit `calculate_popup_position` + Fokus-Verlust-Auto-Close; `theme.rs` 5 Themes.
- `src/tray.rs` (nur Windows): `create_tray_channels_with_theme` + `TrayIcon`/`Menu` mit synchronem `MenuEvent::set_event_handler` + Weiterleitung in `mpsc` zur Erhaltung von `TrayIconEvent.rect` für exakte Popup-Positionierung über dem Tray-Icon.
- `src/gitlab/` (Merge Requests, ohne UI): `remote.rs` (Remotes, Host-Zuordnung), `glab.rs` (`GlabRunner`, `SystemGlab` mit Timeout/ohne Konsolenfenster, `GatedRunner`, `FakeGlab` nur für Tests), `graphql.rs` (Abfragen über `glab api graphql`, Parser), `model.rs` (Gruppen, Sortierung, Badge, Pipeline-Link), `badge.rs` (`MrBadge` für den Chip der Repo-Karte), `service.rs` (Erkennung, Abruf, Prüfung, Token prüfen/speichern), `checkout.rs` (Checkout-Menü K1), `secret.rs` (`SecretString`), `time.rs`. Netz-Aufrufe nur an GitLab-Hosts, bei denen glab angemeldet ist oder die in den Einstellungen stehen.
- `src/app/` (Teile von `MyApp`): `merge_requests.rs` (Erkennung, Abruf, Umschalter, Badge, Statusleiste), `mr_checkout.rs` (Worktree/Klon/Umschalten, „Danach öffnen“), `mr_settings.rs` (Worker des Einstellungs-Tabs, Empfänger `mr_settings_rx` in `MyApp`). UI: `src/ui/merge_requests.rs` (Ansicht, Design B), `src/ui/settings/merge_requests.rs` (Tab „Merge Requests“); Farben über `mr_palette` in `theme.rs`.
- **Tests und glab:** Kein Test startet glab oder geht ins Netz — alles über `Arc<dyn GlabRunner>`, Tests nutzen `FakeGlab` mit Fixtures (`src/gitlab/fixtures/*.json`); `SystemGlab::run` panict unter `cfg(test)` bei `program == "glab"`. Tokens nur als `SecretString` in Kind-stdin/-Env, nie in argv, Logs oder `{:?}`.
- `src/i18n.rs`: Tabelle `TEXTS` (Key, En, De) + `tr`/`tr_fmt` (unbekannter Key liefert den Key), Keys in `tabs_*`/`icons_*`/`tray_*`/`tray_icons_*`/`tray_max_display` etc. Neue Merge-Request-Keys (`mr_*`, `mrs_*`, `view_*`, `rel_*`, `tray_mrs*`) gehören in die Liste von `tr_mr_keys_exist` (prüft auch: En ohne Deutsch, gleiche Platzhalteranzahl De/En, deutsche Anführungszeichen „…“).
- `src/text.rs`: `snippet` (einzeilig, gekürzt), `xml_escape`/`xml_unescape` (Toast-XML, Config-Selectors). `src/path_probe.rs`: Existenzprüfung von Pfaden im Hintergrund für die Oberfläche.
- `assets/icons/*.svg` + `assets/demo.*`, `build.rs` (Windows-Icon), `docker-compose.yml`/`scripts/build.*` für Cross-Build.

**Patterns:** Immediate-mode (kein retained state, `ctx.request_repaint*`), `mpsc` für Hintergrund-Threads (Scan/Launch/Branch-Wechsel/Config-Schreiben), versionierte Config-Migration, gefilterte/geordnete Views (`visible_ides()`/`filtered_agents()`), atomares Schreiben, TTL-Cache für teure Pfad-Discovery.

**UI-Thread-Regel (nie blockieren):** `logic()`/`ui()` und alles, was sie synchron aufrufen, machen keine git2-Aufrufe, keinen Dateizugriff (auch kein `exists()`/`is_dir()`), keinen Prozessstart mit Warten und kein `config.save()`. Stattdessen:
- git-Arbeit in `std::thread::spawn` mit `catch_unwind`, Ergebnis über einen Kanal, danach `ctx.request_repaint()` (`self.ctx`); der Zustand (`GitBusy`, `branch_switch`, `checking`/`running` der Dialoge) ist sichtbar (Spinner, gesperrte Buttons, Statusleiste).
- Branch-Wechsel: `start_branch_switch` → Worker `run_branch_switch` → `ScanResult::BranchSwitched` → `on_branch_switched`; höchstens einer zur Zeit, weitere warten in `pending_branch_switches`.
- Programme starten: `spawn_launch` (IDE, Agent, Shell, Explorer); Fehler kommen über `launch_err_rx`.
- Config speichern: `save_config()` merkt nur vor; `flush_config_save` gibt höchstens alle 200 ms einen `ConfigSnapshot` an den Schreib-Thread (`spawn_config_writer`), Fehler über `config_saved_rx`. Blockierend (`save_config_now`) nur beim Beenden.
- Pfad vorhanden? `path_probe::exists(ctx, path)` (Prüfung im Thread, `None` bis zum ersten Ergebnis) statt `Path::exists()`.
- Lange Listen: Einträge über `ui::widgets::culled` zeichnen (unsichtbare werden übersprungen).
- Tests warten auf Worker mit `app::tests::settle_git` bzw. `switch_branch`.

## 6) Troubleshooting

- `cargo: command not found` / `rustc: command not found` → PATH-Export vergessen, siehe Abschnitt 1.
- `which cargo` leer in opencode Bash ist normal (non-login shell), trotzdem vorhanden via `$HOME/.cargo/bin/cargo`.
- Hohe Build-Zeit beim ersten Mal normal (git2 vendored baut libgit2/openssl).
- Tests brauchen keine GUI, nutzen `tempdir` + `git2::Repository::init` — laufen auch im Docker ohne Display.
- `target/` ist in `.gitignore`, `gitmanager.exe` im Root ebenfalls (`*.exe`).
- **Close Icon Rendering:** Use `egui::Button::image(egui::Image::new(ICON_CROSS).fit_to_exact_size(Vec2::splat(12.0))).small()` statt Text `"✕"`. SVG-Asset `assets/icons/cross.svg` ist als `ICON_CROSS` in `src/ui/icons.rs` eingebettet (neue Icons dort ergänzen, nicht je Datei). Text-Fallbacks respektieren Theme/Font-Skalierung nicht und rendern DPI-abhängig inkonsistent.
- `tray_branch_limit`/`tray_icons.max_display` werden auf `5..50` geclamped, `branch_display_limit` auf `50..500` — 0-Werte werden zu Defaults korrigiert (20 bzw. 10/200) und im `try_load()` migriert.

## 7) UI Assets

SVG-Icons unter `assets/icons/*.svg` werden via `egui::include_image!` zur Compile-Zeit eingebettet (benötigt `egui_extras` mit `svg`-Feature). Alle Konstanten liegen in `src/ui/icons.rs` (MR-Icons in `icons::mr`), z. B. `ICON_CROSS` — `egui::Button::image(egui::Image::new(ICON_CROSS).fit_to_exact_size(Vec2::splat(12.0))).small()` skaliert mit Theme/DPI, reines Text-`RichText::new("✕")` nicht. Alle Tray-Icons (`vscode.svg`, `visualstudio.svg`, `rider.svg`, `folder.svg`, `terminal.svg`, `claude.svg`, `codex.svg`, `gemini.svg`, `copilot.svg`, `cursor.svg`, `aider.svg`, `cross.svg`, `gear.svg`, `refresh.svg` etc.) folgen demselben Pattern.

## 8) Usage Tracking / MRU (Config v10)

**Struktur `RepoUsage`** (`src/config.rs`):
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
Gespeichert in `AppConfig.repo_usage: HashMap<String, RepoUsage>` mit Key `AppConfig::repo_state_key(path)` (`src/config.rs`) — auf Windows `to_lowercase()` (case-insensitiv), sonst unverändert. Auf Windows werden sowohl `repo_state` als auch `repo_usage` beim Laden normalisiert (dedupliziert).

**`record_usage`** (`src/app.rs`):
```rust
fn record_usage(&mut self, repo_path: &Path, usage_type: UsageType)  // Open | BranchSwitch | ConfigChange
```
Holt `now = SystemTime::now().duration_since(UNIX_EPOCH).as_secs()`, setzt passendes `last_*` und inkrementiert Counter via `wrapping_add(1)`, dann `config_save_pending` (gebatcht über `flush_config_save` an den Schreib-Thread). Aufrufe:
- `UsageType::Open` bei jedem Öffnen: `ide_open` (auch via `launch_ide` im Tray und Hauptfenster), `explorer_open` (`open_in_explorer`), `shell_open` (`open_shell`), `agent_open` (`launch_agent`) — `src/app.rs`.
- `UsageType::BranchSwitch` nach erfolgreichem Wechsel (`on_branch_switched`) — `src/app.rs`.
- `UsageType::ConfigChange` bei Custom-Config Selector-Wechsel — `src/app.rs`.

**Sortierung im Tray** (`tray_popup::sorted_visible_refs`, aufgerufen aus `src/tray_service.rs:show_tray_popup_viewport`):
Repos werden vor Anzeige nach `max(last_opened, last_branch_switch, last_config_change)` absteigend sortiert (0 falls nie genutzt) und auf `tray_icons.max_display` gekürzt (`truncate`). Damit stehen zuletzt genutzte Repos oben (MRU).

**Config & UI:**
- Feld `tray_icons.max_display: usize` (`src/config.rs`) default `10`, `#[serde(default = "default_tray_max_display")]`, geclamped `5..50` in `try_load()` und vor Anzeige (`clamp(5,50)` in `app.rs`).
- Slider/DragValue im Tab **Tray Icons** (`show_tray_icons_tab` in `src/ui/settings/tray_icons.rs`): Label `tray_max_display` (`Max repos in tray popup` / `Max Repos im Tray-Popup`), Slider `5..=50`, `DragValue` `5..=50`, Anzeige `(aktuell: N)` + `Zeigt bis zu N Repos…`.
- Migration `v10` (`src/config.rs`): ergänzt fehlende Default-IDs in `icon_order` und setzt `config_version` auf `10`. Clamp von `max_display`, Sortieren/Deduplizieren von `hidden_icon_ids`/`icon_order` und die Normalisierung der Schlüssel (Windows) laufen bei jedem Laden in der Validierung; gespeichert wird einmal am Ende von `try_load()`.

## 9) Tray Icon Management (TrayIconConfig, Config v10)

**Struktur `TrayIconConfig`** (`src/config.rs`):
```rust
pub struct TrayIconConfig {
    pub icon_order: Vec<String>,     // Reihenfolge-IDs in Anzeigereihenfolge
    pub hidden_icon_ids: Vec<String>,// ausgeblendete IDs
    pub max_display: usize,          // siehe Abschnitt 8, Repos-Limit
}
```
Default `icon_order=[]`, `hidden_icon_ids=[]`, `max_display=10` (`TrayIconConfig::default`).

**Default-IDs** (`DEFAULT_TRAY_ICON_IDS` in `src/ui/settings/tray_icons.rs` + `src/config.rs`):
`["vscode","vs2022","rider","folder","terminal","claude","codex","gemini","copilot","cursor","aider"]` (11). In `effective_tray_order()` (`settings/tray_icons.rs`) werden fehlende Defaults ergänzt, unbekannte Custom-IDs am Ende erhalten, Entfernung/Filter nur gegen bekannte Liste.

**Anzeige-Logik im Popup** (`src/ui/tray_popup.rs` `show_tray_repo_row`):
- `tray_hidden = &config.tray_icons.hidden_icon_ids`, `tray_order = &config.tray_icons.icon_order`, `order_idx = |id| tray_order.position(id).unwrap_or(MAX)`.
- IDEs: `profile.visible_ides()` gefiltert `!tray_hidden.contains(id)`, sortiert nach `tray_order`, `take(3)`. Explorer/Terminal jeweils nur falls nicht hidden. Agents: `profile.filtered_agents(...).filter(!hidden)`, sortiert, `take(2)` + Fallback auf ersten aktiven Agent falls Filter leer aber nicht hidden. `separator` nur falls sichtbar.

**Settings Tab Tray Icons** (`show_tray_icons_tab` in `src/ui/settings/tray_icons.rs` via `SettingsTab::TrayIcons` / `tabs_tray_icons`):
- Oben Slider für `max_display` (siehe Abschnitt 8).
- Darunter Liste `effective_tray_order(&state.draft)` (≈11 Zeilen) je mit `eye`/`eye_off` Toggle (add/remove aus `hidden_icon_ids`, danach `sort+dedup`), Name `(id)`, rechts `chevron-up`/`chevron-down` zum Verschieben im `icon_order` (`move_in_order`, ausgehend von der angezeigten Reihenfolge). Anzeige `icons_hidden_info` mit `hidden_icon_ids.join(", ")` oder `—`, Warnung wenn alle hidden, `icons_reset` Button leert `hidden_icon_ids`/`icon_order` und setzt `max_display=10`.
- Validierung in `config.rs`: `max_display.clamp(5,50)`, `hidden_icon_ids` sort/dedup, `icon_order` dedup retain (first wins).

**Persistenz:** Ändern im Draft → `Save` → die App übernimmt den Entwurf (ihre Laufzeitfelder `repo_usage`, `repo_state`, Favoriten bleiben) und speichert über `save_config()` im Hintergrund (`ConfigSnapshot`: `serde_json::to_string_pretty` → `*.tmp`→`rename`). Im Tray per Sync (`sync_tray_service`) sichtbar.

**i18n Keys** (`src/i18n.rs`): `tray_max_display`, `tray_icons_title`/`tray_icons_desc`, `tabs_tray_icons`, `icons_toggle_visibility`/`icons_hidden_info`/`icons_reset`.
