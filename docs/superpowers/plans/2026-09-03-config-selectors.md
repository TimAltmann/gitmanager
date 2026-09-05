# Config-Selectors (XML, pro Profil) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generische per-Profil Dropdowns, die einen `key` in einer XML `.config` im Repo-Root lesen/schreiben (z.B. `App.config` `<add key="Database" value="dev"/>`), mit vordefinierten Werten in `Settings` konfigurierbar – Ansatz A minimal.

**Architecture:** `LanguageProfile` (`src/config.rs:146`) bekommt `config_selectors: Vec<ConfigSelector>` (XML-only v1). Neuer `src/config_parser.rs` liest/schreibt XML via `roxmltree` + string-preserve-write. `scan_repos` (`src/scanner.rs:28`) füllt `RepoInfo.custom_values` (`src/git.rs:67`), `repo_list.rs:139` rendert pro effektivem Profil ein `dropdown_button` (`src/ui/repo_list.rs:51`) wie Branch, `app.rs:740` schreibt bei Auswahl atomar auf Platte.

**Tech Stack:** Rust 1.75+, eframe/egui 0.36, git2 0.21, serde/serde_json, walkdir+rayon, **neu:** `roxmltree 0.20` (nur read, kein re-serialize – write via string replace um Kommentare/Whitespace zu erhalten)

**Spec:** Brainstorming-Ergebnis 2026-09-03 – XML, direkt auf Platte ändern, Dropdown definiert pro Profil (kein separater Spec-File – dieses Doc ist Spec+Plan)

## Global Constraints

- Portable single `.exe` <15 MB, <60 MB RAM, <300 ms Start, kein Admin, `windows_subsystem` Release (`src/main.rs:1`), via `Cargo.toml:31` Release `opt-level="z"` `lto=true`
- Config-Datei `%APPDATA%\gitmanager\config\config.json` atomar (`src/config.rs:868` tmp+rename+sync), `config_version` aktuell `6` (`src/config.rs:9`) → bump auf `7`, Migration muss alte Configs laden
- `LanguageProfile` Pattern: `file_extension` normalisiert (`src/config.rs:177`), `matches_file` (`src/config.rs:184`), `visible_ides`/`filtered_agents` – neues Feld folgt gleichem Validierungsstil
- UI Patterns: `dropdown_button` (`src/ui/repo_list.rs:51`), Popup via `egui::Window` + `Id::new(...).with(&repo.path)` + Filter (`src/ui/repo_list.rs:185`/`src/ui/repo_list.rs:327`), `RepoListActions` (`src/ui/repo_list.rs:90`) für Callbacks, `show_repo_row` Frame (`src/ui/repo_list.rs:146`)
- Settings Pattern: `SettingsState` Draft (`src/ui/settings.rs:25`), `selected_profile_idx` (`src/ui/settings.rs:31`), Tabs enum (`src/ui/settings.rs:14`), Validierung vor Save (`src/ui/settings.rs:233`)
- i18n via `tr(lang,key)` (`src/i18n.rs:39`), Keys snake_case, EN/DE vorhanden
- Tests: `cargo test` (auch `docker compose run --rm dev cargo test`), TDD, bestehende Tests nicht brechen

---

## File Structure

**Modify:**
- `Cargo.toml:12` – add `roxmltree = "0.20"`
- `src/config.rs:1` – add `ConfigSelector`, `ConfigOption`, Feld `LanguageProfile.config_selectors`, Migration v6→v7, Validierung
- `src/git.rs:58` – extend `RepoInfo` with `custom_values: HashMap<String,String>` + `custom_errors: HashMap<String,String>`
- `src/scanner.rs:27` – after `solutions` block, populate `custom_values` per effective profile selectors
- `src/ui/repo_list.rs:1` – render per-selector dropdowns, extend `RepoListActions` with `custom_select`
- `src/app.rs:1` – handle `custom_select` (write + scan), import `config_parser`
- `src/ui/settings.rs:1` – extend Profiles-Tab um "Config Dropdowns" Editor für selektiertes Profil
- `src/i18n.rs:39` – add keys für neuen Tab/Editor
- `src/main.rs:5` – `mod config_parser;`

**Create:**
- `src/config_parser.rs` – `read_xml_value`, `write_xml_value`, helpers, Tests (reines Modul, keine UI)

---

### Task 1: Datenmodell – `ConfigSelector` pro `LanguageProfile`

**Files:**
- Modify: `src/config.rs:146` (`LanguageProfile` struct)
- Modify: `src/config.rs:9` (`default_config_version` → 7)
- Modify: `Cargo.toml:12` (add roxmltree – wird hier schon gebraucht für späteren Import, aber erst kompiliert ab Task 2)

**Interfaces:**
- Consumes: `LanguageProfile` existing fields (`src/config.rs:147-173`)
- Produces: `pub struct ConfigSelector`, `pub struct ConfigOption`, `pub enum XmlSelectorKind`, Feld `LanguageProfile.config_selectors: Vec<ConfigSelector>` + Methoden

**Design v1 minimal:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOption {
    pub value: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum XmlSelectorKind {
    AddKeyValue,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSelector {
    pub id: String,
    pub display_name: String,
    pub file_path: String,
    pub key: String,
    #[serde(default="default_key_attr")] pub key_attribute: String,
    #[serde(default="default_val_attr")] pub value_attribute: String,
    #[serde(default)] pub kind: XmlSelectorKind,
    pub options: Vec<ConfigOption>,
    #[serde(default)] pub allow_custom: bool,
}
fn default_key_attr() -> String { "key".to_string() }
fn default_val_attr() -> String { "value".to_string() }
```

- [ ] **Step 1: Write the failing test** – `src/config.rs:1023` tests Modul erweitern

```rust
#[test]
fn config_selector_default_and_serde() {
    let sel = ConfigSelector {
        id: "db".into(), display_name: "Datenbank".into(),
        file_path: "App.config".into(), key: "Database".into(),
        key_attribute: "key".into(), value_attribute: "value".into(),
        kind: XmlSelectorKind::AddKeyValue,
        options: vec![ConfigOption{value:"dev".into(), label:"Dev".into()}],
        allow_custom: false,
    };
    let json = serde_json::to_string(&sel).unwrap();
    let de: ConfigSelector = serde_json::from_str(&json).unwrap();
    assert_eq!(de.id, "db");
}
#[test]
fn profile_has_config_selectors_empty_by_default() {
    let cfg = AppConfig::default();
    assert!(cfg.get_active_profile().config_selectors.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test config_selector_default_and_serde -- --nocapture`
Expected: FAIL `cannot find type ConfigSelector`

- [ ] **Step 3: Write minimal implementation**

In `src/config.rs:146` nach `show_explorer: bool,` einfügen:
```rust
#[serde(default)]
pub config_selectors: Vec<ConfigSelector>,
```
Davor neue Structs/enums wie oben. `default_dotnet_profile()` (`src/config.rs:310`) `config_selectors: Vec::new()` ergänzen. Alle `LanguageProfile { ... }` Konstruktionen in Tests (`src/config.rs:1298ff`, `src/scanner.rs:506` etc.) um `config_selectors: Vec::new()` erweitern.

- [ ] **Step 4: Migration und Validierung in `AppConfig::try_load`**

Nach `src/config.rs:688` v6-Block, neuer Block:
```rust
if cfg.config_version < 7 {
    for p in &mut cfg.profiles {
        for sel in &mut p.config_selectors {
            sel.id = sel.id.trim().to_lowercase().replace(' ', "_");
            sel.file_path = sel.file_path.trim().to_string();
            sel.key = sel.key.trim().to_string();
            if sel.key_attribute.trim().is_empty() { sel.key_attribute = "key".to_string(); }
            if sel.value_attribute.trim().is_empty() { sel.value_attribute = "value".to_string(); }
            sel.options.retain(|o| !o.value.trim().is_empty());
        }
    }
    cfg.config_version = 7;
    let _ = cfg.save();
}
```
Im Validierungs-Loop ab `src/config.rs:754` analog `p.file_extension` auch `config_selectors` normalisieren.

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib config::tests::config_selector -- --nocapture`
Expected: PASS, plus `cargo test` overall grün

- [ ] **Step 6: Commit**

```bash
git add src/config.rs Cargo.toml
git commit -m "feat(config): add per-profile ConfigSelector (XML AddKeyValue) with v7 migration"
```

---

### Task 2: XML Parser Modul `src/config_parser.rs` (read/write, format-erhaltend)

**Files:**
- Create: `src/config_parser.rs`
- Modify: `Cargo.toml:12` – `roxmltree = "0.20"` (falls noch nicht), `src/main.rs:5` add `mod config_parser;`
- Test: `src/config_parser.rs::tests`

**Interfaces:**
- Consumes: `ConfigSelector` (`src/config.rs`)
- Produces:
```rust
pub fn read_xml_value(repo_path: &Path, selector: &ConfigSelector) -> Result<Option<String>, String>
pub fn write_xml_value(repo_path: &Path, selector: &ConfigSelector, new_value: &str) -> Result<(), String>
fn find_add_element_value(xml: &str, key: &str, key_attr: &str, val_attr: &str) -> Option<String>
```

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
 use super::*;
 use tempfile::tempdir;
 use std::fs;
 fn make_selector(key: &str) -> ConfigSelector { /* ... */ }

 #[test]
 fn read_add_key_value_found() {
   let xml = r#"<?xml version="1.0"?><configuration><appSettings><add key="Database" value="dev" /></appSettings></configuration>"#;
   assert_eq!(find_add_element_value(xml, "Database", "key", "value"), Some("dev".into()));
 }
 #[test]
 fn read_not_found_returns_none() {
   let xml = r#"<configuration><appSettings><add key="Other" value="x"/></appSettings></configuration>"#;
   assert_eq!(find_add_element_value(xml, "Database", "key", "value"), None);
 }
 #[test]
 fn write_preserves_comments_and_whitespace() {
   let dir = tempdir().unwrap();
   let path = dir.path().join("App.config");
   fs::write(&path, "<!-- comment -->\n<configuration>\n  <appSettings>\n    <add key=\"Database\" value=\"dev\" />\n  </appSettings>\n</configuration>").unwrap();
   let sel = make_selector("Database");
   write_xml_value(dir.path(), &sel, "prod").unwrap();
   let out = fs::read_to_string(&path).unwrap();
   assert!(out.contains("<!-- comment -->"));
   assert!(out.contains(r#"value="prod""#));
 }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test config_parser -- --nocapture`
Expected: FAIL `module not found`

- [ ] **Step 3: Write minimal implementation**

- `Cargo.toml` add `roxmltree = "0.20"`
- `src/main.rs` add `mod config_parser;`
- `src/config_parser.rs` implement `find_add_element_value` via `roxmltree`, `read_xml_value`, `write_xml_value` via String find/replace preserving comments

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test config_parser -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/config_parser.rs src/main.rs Cargo.lock
git commit -m "feat(parser): add roxmltree-based XML read/write preserving formatting"
```

---

### Task 3: `RepoInfo` + `scanner` Integration

**Files:**
- Modify: `src/git.rs:67` – `RepoInfo`
- Modify: `src/scanner.rs:54` – `scan_repos` second stage

**Interfaces:**
- Consumes: `AppConfig` (`src/config.rs:469`), `ConfigSelector`, `config_parser::read_xml_value`
- Produces: `RepoInfo.custom_values: HashMap<String,String>`, `custom_errors: HashMap<String,String>`

- [ ] **Step 1: Write the failing test** – `src/scanner.rs:188`

```rust
#[test]
fn scan_repos_populates_custom_values_per_profile() {
  let tmp = tempdir().unwrap();
  let root = tmp.path();
  let repo = root.join("repo");
  fs::create_dir(&repo).unwrap();
  init_repo(&repo);
  fs::write(repo.join("App.config"), r#"<configuration><appSettings><add key="Database" value="dev"/></appSettings></configuration>"#).unwrap();
  let mut cfg = AppConfig::default();
  cfg.roots = vec![root.to_path_buf()];
  cfg.max_depth = 1;
  cfg.profiles[0].config_selectors.push(ConfigSelector{...});
  let repos = scan_repos(&cfg);
  assert_eq!(repos[0].custom_values.get("db"), Some(&"dev".to_string()));
}
```

- [ ] **Step 2: Run to fail**

`cargo test scan_repos_populates_custom -- --nocapture` → FAIL `field custom_values not found`

- [ ] **Step 3: Implement**

In `src/git.rs:67`:
```rust
pub custom_values: HashMap<String, String>,
pub custom_errors: HashMap<String, String>,
```
In `RepoInfo::new` init `HashMap::new()`.

In `src/scanner.rs:54` nach `repo.solutions`:
```rust
let profile = config.get_effective_profile_for_repo(&repo.path);
let mut cvals = HashMap::new();
let mut cerrs = HashMap::new();
for sel in &profile.config_selectors {
  match crate::config_parser::read_xml_value(&repo.path, sel) {
    Ok(Some(v)) => { cvals.insert(sel.id.clone(), v); },
    Ok(None) => { cerrs.insert(sel.id.clone(), format!("Key '{}' nicht gefunden in {}", sel.key, sel.file_path)); },
    Err(e) => { cerrs.insert(sel.id.clone(), e); }
  }
}
repo.custom_values = cvals;
repo.custom_errors = cerrs;
```

- [ ] **Step 4: Pass**

`cargo test scan_repos_populates -- --nocapture` → PASS

- [ ] **Step 5: Commit**

```bash
git add src/git.rs src/scanner.rs
git commit -m "feat(scan): populate custom_values per-profile XML selectors"
```

---

### Task 4: UI – Repo-Zeile Dropdowns `src/ui/repo_list.rs`

**Files:**
- Modify: `src/ui/repo_list.rs:90` – `RepoListActions`
- Modify: `src/ui/repo_list.rs:139` – `show_repo_row`

**Interfaces:**
- Consumes: `RepoInfo.custom_values` (`src/git.rs`), `AppConfig.get_effective_profile_for_repo` (`src/config.rs:1011`)
- Produces: `actions.custom_select: Option<(PathBuf, String, String)>`

- [ ] **Step 1: Write failing test** – `src/ui/repo_list.rs:899`

```rust
#[test]
fn repo_list_actions_has_custom_select() {
  let mut a = RepoListActions { branch_switch: None, solution_select: None, ide_open: None, agent_open: None, profile_override: None, fetch_branches: None, explorer_open: None, shell_open: None, custom_select: None };
  a.custom_select = Some((PathBuf::from("/tmp/repo"), "db".into(), "prod".into()));
  assert_eq!(a.custom_select, Some((PathBuf::from("/tmp/repo"), "db".into(), "prod".into())));
}
```

- [ ] **Step 2: Fail** → `custom_select` unknown

- [ ] **Step 3: Implement**

In `RepoListActions` add:
```rust
pub custom_select: Option<(PathBuf, String, String)>,
```

In `show_repo_row` nach Solution-Dropdown Block einfügen: per effective profile selectors render `dropdown_button` width 140, popup lists `sel.options`, on click => `actions.custom_select = Some((repo.path.clone(), sel.id.clone(), opt.value.clone()))`, tooltip shows file:key = current or error.

- [ ] **Step 4: Pass**

`cargo test repo_list_actions -- --nocapture` → PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/repo_list.rs
git commit -m "feat(ui): render per-profile XML config selectors as dropdowns in repo row"
```

---

### Task 5: App-Handler – Schreiben auf Platte `src/app.rs`

**Files:**
- Modify: `src/app.rs:740` – `RepoListActions` handling

**Interfaces:**
- Consumes: `actions.custom_select`, `config_parser::write_xml_value`

- [ ] **Step 1: Write failing test** – `src/app.rs:955`

```rust
#[test]
fn custom_select_writes_file() {
  let dir = tempdir().unwrap();
  let sel = ConfigSelector{ id:"db".into(), display_name:"DB".into(), file_path:"App.config".into(), key:"Database".into(), key_attribute:"key".into(), value_attribute:"value".into(), kind: XmlSelectorKind::AddKeyValue, options: vec![], allow_custom:true };
  std::fs::write(dir.path().join("App.config"), r#"<add key="Database" value="dev"/>"#).unwrap();
  crate::config_parser::write_xml_value(dir.path(), &sel, "prod").unwrap();
  let out = std::fs::read_to_string(dir.path().join("App.config")).unwrap();
  assert!(out.contains(r#"value="prod""#));
}
```

- [ ] **Step 3: Implement Handler** in `src/app.rs:744`:

```rust
if let Some((repo_path, selector_id, new_value)) = actions.custom_select {
  let profile = self.config.get_effective_profile_for_repo(&repo_path);
  if let Some(sel) = profile.config_selectors.iter().find(|s| s.id == selector_id) {
    match crate::config_parser::write_xml_value(&repo_path, sel, &new_value) {
      Ok(()) => {
        self.status_message = Some(tr_fmt(lang, "config_saved", &[&sel.display_name, &new_value]));
        self.status_message_time = Some(std::time::Instant::now());
        if let Some(repo) = self.repos.iter_mut().find(|r| r.path == repo_path) {
          repo.custom_values.insert(selector_id.clone(), new_value.clone());
          repo.custom_errors.remove(&selector_id);
          repo.dirty = true;
        }
        self.start_scan();
      },
      Err(e) => { self.error = Some(format!("Config '{}' speichern fehlgeschlagen: {e}", sel.display_name)); }
    }
  }
}
```

- [ ] **Step 4: Pass**

`cargo test` + `cargo build` → PASS

- [ ] **Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): handle custom_select, write XML on disk and rescans"
```

---

### Task 6: Settings – Per-Profil Selector Editor `src/ui/settings.rs`

**Files:**
- Modify: `src/ui/settings.rs:539` – `show_profiles_tab`

**Interfaces:**
- Consumes: `LanguageProfile.config_selectors` (`src/config.rs`)

- [ ] **Step 1: Write failing test** – `src/ui/settings.rs:1911`

```rust
#[test]
fn settings_add_config_selector_to_profile() {
  let mut cfg = AppConfig::default();
  let p = &mut cfg.profiles[0];
  assert!(p.config_selectors.is_empty());
  p.config_selectors.push(ConfigSelector{...});
  assert_eq!(p.config_selectors.len(), 1);
}
```

- [ ] **Step 3: Implement Editor** – Abschnitt `Config-Dropdowns für dieses Profil` im Profiles-Tab nach IDEs

- [ ] **Step 4: Pass**

`cargo test settings_add_config_selector -- --nocapture` → PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/settings.rs
git commit -m "feat(settings): per-profile XML config selector editor in Profiles tab"
```

---

### Task 7: i18n `src/i18n.rs`

**Files:**
- Modify: `src/i18n.rs:39` – `tr` match

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn tr_config_keys_exist() {
  assert_ne!(tr(Language::En, "config_selector_title"), "config_selector_title");
}
```

- [ ] **Step 3: Add match arms** für `config_selector_title`, `config_selector_desc`, `add_config_selector`, `config_file`, `config_key`, `config_options`, `allow_custom`, `config_saved`, etc.

- [ ] **Step 4: Pass**

- [ ] **Step 5: Commit**

```bash
git add src/i18n.rs
git commit -m "feat(i18n): add config selector strings EN/DE"
```

---

### Task 8: Integration-Test & Doku

**Files:**
- Modify: `README.md:71` – Features Tabelle erweitern

- [ ] **Step 1: Write E2E failing test**

E2E: tmp root mit 2 Repos, eines mit App.config dev, eines ohne, definiere Selector db, scan → erstes repo hat custom_values["db"]=="dev", zweites hat custom_errors

- [ ] **Step 4: Pass**

Run: `cargo test && cargo clippy && cargo fmt --check`

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: document per-profile XML config selectors"
```

