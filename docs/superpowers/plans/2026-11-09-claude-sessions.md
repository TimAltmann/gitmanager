# Claude Sessions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dritte Hauptansicht „Claude Sessions“, die alle Claude-Code-Sessions in gescannten Repos/Worktrees gruppiert anzeigt, durchsuchbar macht und per Klick mit `claude --resume <id>` im Session-Ordner fortsetzt.

**Architecture:** Neues Logik-Modul `src/claude_sessions.rs` (Parsen der JSONL-Dateien, Zuordnung zu `RepoInfo`, Suche, Resume-Profil) ohne UI; neues UI-Modul `src/ui/claude_sessions.rs` (Werkzeugleiste, Abschnitte, Karten) liefert nur Aktionen zurück; `src/app.rs` + `src/app/merge_requests.rs` binden die Ansicht ein (MainView, Umschalter, Hintergrund-Scan, Start über `git::launch_agent`).

**Tech Stack:** Rust 2021, egui/eframe 0.36, serde_json, tempfile (Tests).

**Spec:** `docs/superpowers/specs/2026-11-09-claude-sessions-design.md`

## Global Constraints

- Auf Branch `feature/worktrees-getrennt-suche` arbeiten, keinen neuen Branch anlegen.
- Commits mit repo-lokaler Identität `Tim Altmann <63297571+TimAltmann@users.noreply.github.com>` (vor dem Commit `git config --local user.email` prüfen); Commit-Message endet mit `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Commit-Messages wie im Repo: Kleinschreibung, deutsch, `feat:`/`docs:`/`test:`-Präfix.
- Toolchain: vor jedem cargo-Befehl `export PATH="$HOME/.cargo/bin:$PATH"`.
- Tests laufen unter Linux: Pfadlogik darf nicht von `std::path` des Hosts abhängen (Windows-Pfade als Strings normalisieren).
- Ablage: `CLAUDE_CONFIG_DIR` oder Home + `.claude`, darin `projects/*/*.jsonl`; Unterordner von `projects/<dir>/` ignorieren.
- Titel-Priorität: `custom-title` > `ai-title` > erste echte User-Nachricht (`isMeta` ≠ true) > Session-ID; jeweils der zuletzt in der Datei stehende Eintrag gewinnt.
- Lesegrenzen: Kopf max. 256 KiB, Ende 64 KiB.
- Verwaiste Sessions (cwd existiert nicht) und Sessions ohne passendes Repo werden ausgeblendet.
- Suche nur über Metadaten (Titel, letzter Prompt, Repo-Name, Repo-Pfad, Branch), alle Wörter, case-insensitiv.
- Alle UI-Texte DE und EN in `src/i18n.rs`.
- CI: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` grün.

## Review Focus

1. Session wurde in einem Unterordner eines Repos gestartet (`C:\Code\repo1\src`) → erscheint beim Repo `C:\Code\repo1` (Test in Task 2: `assign_subfolder_maps_to_repo`).
2. `cwd` als `/c/Code/x` (Git-Bash) bzw. abweichende Groß-/Kleinschreibung gegenüber dem Scan-Pfad → trotzdem zugeordnet (Task 1: `path_key_*`, Task 2: `assign_matches_gitbash_and_case`).
3. Agent-Profil nutzt ein `command`-Template statt `args` → `--resume <id>` wird an das Template angehängt, nicht verloren (Task 2: `resume_profile_appends_to_command_template`).
4. Session-Datei wird gerade von einem laufenden Claude geschrieben (letzte Zeile halb) → kein Absturz, Datei trotzdem gelesen (Task 1: `parse_ignores_truncated_last_line`).
5. Erste Zeile im Ende-Puffer beginnt mitten in einer Zeile oder in einem UTF-8-Zeichen → wird verworfen, kein Panic (Task 1: `parse_tail_starts_mid_utf8`).

---

### Task 1: Session-Datei parsen

**Files:**
- Create: `src/claude_sessions.rs`
- Modify: `src/main.rs` (Zeile `mod claude_sessions;` in die alphabetische `mod`-Liste)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct ClaudeSession {
      pub id: String,                 // Dateiname ohne ".jsonl"
      pub cwd: String,                // Anzeige-/Startpfad, Git-Bash-Form → "C:\\Code\\x"
      pub title: String,
      pub last_prompt: Option<String>,
      pub git_branch: Option<String>, // letzter nicht-leerer gitBranch
      pub last_activity: i64,         // mtime, Unix-Sekunden
  }
  pub fn path_key(p: &str) -> String;
  pub fn display_cwd(raw: &str) -> String;
  pub fn parse_session(path: &Path) -> Option<ClaudeSession>;
  pub(crate) fn parse_session_bytes(id: &str, head: &[u8], tail: &[u8], mtime: i64) -> Option<ClaudeSession>;
  ```

- [ ] **Step 1: Write the failing tests** (in `#[cfg(test)] mod tests` von `src/claude_sessions.rs`; Hilfsfunktion `line(v: serde_json::Value) -> String` erzeugt eine JSONL-Zeile)

  - `path_key_normalizes_windows_and_gitbash`: `path_key("C:\\Code\\Repo1\\")`, `path_key("/c/Code/repo1")` und `path_key("c:/code/REPO1")` sind alle `"c:/code/repo1"`.
  - `display_cwd_converts_gitbash`: `display_cwd("/c/Code/repo1")` == `"C:\\Code\\repo1"`; `display_cwd("C:\\Code\\x")` unverändert; `display_cwd("/home/u/x")` unverändert.
  - `parse_title_priority`: Datei mit `{"type":"user","cwd":"C:\\Code\\a","message":{"role":"user","content":"Erste Frage"}}`, `{"type":"ai-title","aiTitle":"KI 1"}`, `{"type":"ai-title","aiTitle":"KI 2"}`, `{"type":"custom-title","customTitle":"Mein Name"}` → `title == "Mein Name"`; ohne custom-title → `"KI 2"`; ohne beide → `"Erste Frage"`; ohne User-Nachricht → `title == id`.
  - `parse_skips_meta_user_messages`: erste User-Nachricht mit `"isMeta":true` wird nicht als Titel genommen, die nächste schon. `content` kann String oder Array `[{"type":"text","text":"…"}]` sein – beides liefert den Text.
  - `parse_last_prompt_and_branch`: zwei `last-prompt`-Zeilen → die letzte; `gitBranch` der letzten Zeile mit nicht-leerem Wert.
  - `parse_without_cwd_is_none`: keine Zeile mit `cwd` → `None`.
  - `parse_skips_garbage_lines`: kaputte JSON-Zeile zwischen gültigen Zeilen → Ergebnis wie ohne sie.
  - `parse_ignores_truncated_last_line`: letzte Zeile ohne `\n` und abgeschnitten (`{"type":"ai-ti`) → `Some`, Titel aus vorherigen Zeilen.
  - `parse_tail_starts_mid_utf8`: `tail` beginnt mit `&"ä\n…".as_bytes()[1..]` gefolgt von gültiger `ai-title`-Zeile → kein Panic, Titel aus dieser Zeile.
  - `parse_large_file_head_and_tail` (über `parse_session` mit `tempfile`): Datei mit `cwd` in Zeile 1, dann > 512 KiB Füllzeilen (`{"type":"assistant","message":{"content":"xxx…"}}`), `ai-title` am Ende → `cwd` und Titel korrekt; `id` == Dateistamm; `last_activity` > 0.

- [ ] **Step 2: Run tests to verify they fail**

  Run: `cargo test claude_sessions`
  Expected: Kompilierfehler (Funktionen fehlen).

- [ ] **Step 3: Implement**

  - `path_key`: `\` → `/`, lowercase, `^/([a-z])/` → `$1:/`, trailing `/` entfernen.
  - `display_cwd`: nur Git-Bash-Form `^/[a-zA-Z]/` → `X:\…` mit Backslashes; sonst unverändert.
  - `parse_session_bytes`: Kopf und Ende mit `String::from_utf8_lossy` dekodieren und zeilenweise durchgehen. Im Ende die erste Zeile verwerfen, wenn `tail` nicht der Dateianfang ist (Aufrufer übergibt dann `tail` ohne Überlappung; ist die Datei ≤ 256 KiB, ist `tail` leer und nur `head` zählt). Jede Zeile als `serde_json::Value` versuchen, Fehler überspringen. Felder: erstes `cwd`, letzter nicht-leerer `gitBranch`, letzter `customTitle`/`aiTitle`/`lastPrompt`, erste User-Nachricht ohne `isMeta`. Kopf zuerst, Ende danach (spätere Werte überschreiben). User-Nachrichten, die mit `<` beginnen (`<command-name>`, `<local-command-…>`), zählen nicht als Titel. Titel/Prompt auf eine Zeile kürzen (Zeilenumbrüche → Leerzeichen, trim).
  - `parse_session`: Datei öffnen, `metadata().len()` und `modified()`; Kopf = erste `min(len, 256 KiB)` Bytes; wenn `len > 256 KiB`, Ende = letzte `min(64 KiB, len − 256 KiB)` Bytes per `seek`. `id` = `file_stem`.

- [ ] **Step 4: Run tests to verify they pass**

  Run: `cargo test claude_sessions`
  Expected: alle Tests PASS.

- [ ] **Step 5: Commit**

  ```bash
  git add src/claude_sessions.rs src/main.rs
  git commit -m "feat: claude-session-dateien einlesen"
  ```

---

### Task 2: Zuordnung, Scan, Suche, Resume-Profil

**Files:**
- Modify: `src/claude_sessions.rs`

**Interfaces:**
- Consumes: `ClaudeSession`, `path_key`, `parse_session` (Task 1); `crate::git::RepoInfo` (`path`, `name`, `is_linked_worktree`); `crate::config::AgentProfile`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct RepoSessions {
      pub repo_path: PathBuf,
      pub repo_name: String,
      pub is_worktree: bool,          // = RepoInfo.is_linked_worktree
      pub sessions: Vec<ClaudeSession>,
  }
  #[derive(Default)]
  pub struct SessionCache { /* HashMap<PathBuf, (i64 mtime, u64 len, Option<ClaudeSession>)> */ }
  pub fn claude_projects_dir() -> Option<PathBuf>;
  pub fn assign_to_repos(sessions: Vec<ClaudeSession>, repos: &[RepoInfo], exists: impl Fn(&str) -> bool) -> Vec<RepoSessions>;
  pub fn scan_sessions(projects_dir: &Path, repos: &[RepoInfo], cache: &mut SessionCache) -> Vec<RepoSessions>;
  pub fn session_matches_search(group: &RepoSessions, session: &ClaudeSession, query: &str) -> bool;
  pub fn resume_profile(claude: Option<&AgentProfile>, session_id: &str) -> AgentProfile;
  ```

- [ ] **Step 1: Write the failing tests** (`RepoInfo` in Tests über vorhandenen Test-Helper in `src/git.rs`/`src/ui/repo_list.rs` bauen, falls vorhanden, sonst `RepoInfo { path, name, is_linked_worktree, ..Default }` — vorher prüfen, ob `RepoInfo: Default`; sonst lokaler Helper `repo(path, name, wt)`)

  - `assign_subfolder_maps_to_repo`: Session-cwd `C:\Code\repo1\src`, Repos `C:\Code\repo1` → eine Gruppe `repo1`.
  - `assign_component_prefix_only`: cwd `C:\Code\repo10`, Repo `C:\Code\repo1` → keine Gruppe.
  - `assign_longest_match_wins`: Repo `C:\Code\repo1` und Worktree `C:\Code\repo1\wt\feat`; cwd `C:\Code\repo1\wt\feat` → Gruppe des Worktrees mit `is_worktree == true`.
  - `assign_matches_gitbash_and_case`: cwd `/c/code/REPO1`, Repo `C:\Code\repo1` → zugeordnet.
  - `assign_hides_orphans`: `exists` liefert `false` → keine Gruppe.
  - `assign_sorts_by_activity`: Sessions innerhalb einer Gruppe nach `last_activity` absteigend; Gruppen nach neuester Session absteigend.
  - `scan_ignores_subdirectories` (tempfile): `projects/C--a/s1.jsonl` und `projects/C--a/s1/subagents/x.jsonl` → nur `s1`. Repo-Pfad in diesem Test = tempdir-Pfad, cwd in der Datei = derselbe Pfad (existiert).
  - `scan_cache_drops_deleted_files`: nach erstem Scan enthält der Cache 1 Eintrag (`cache.len() == 1`, `SessionCache::len` als `pub(crate)` für Tests); nach Löschen der Datei und erneutem Scan `cache.len() == 0` und Ergebnis leer.
  - `search_all_words_case_insensitive`: Titel „Login-Bug fixen“, Branch `feature/auth`, Repo `pvsmefa`: `""` → true; `"login AUTH"` → true; `"pvsmefa"` → true; `"login logout"` → false; Treffer auch im `last_prompt`.
  - `resume_profile_appends_args`: Profil `args: ["--model","opus"]` → `["--model","opus","--resume","abc"]`, übrige Felder gleich.
  - `resume_profile_appends_to_command_template`: `args: []`, `command: Some("claude --verbose")` → `command == Some("claude --verbose --resume abc")`, `args` leer.
  - `resume_profile_default_without_profile`: `None` → `program == "claude"`, `args == ["--resume","abc"]`, `id == "claude"`.

- [ ] **Step 2: Run tests to verify they fail**

  Run: `cargo test claude_sessions`
  Expected: Kompilierfehler (neue Funktionen fehlen).

- [ ] **Step 3: Implement**

  - `claude_projects_dir`: `CLAUDE_CONFIG_DIR` (nicht leer) sonst `directories::BaseDirs::new()?.home_dir().join(".claude")`, jeweils `.join("projects")`, nur wenn `is_dir()`.
  - `assign_to_repos`: Repo-Keys per `path_key(&repo.path.to_string_lossy())`; Treffer, wenn `key == repo_key || key.starts_with(&format!("{repo_key}/"))`; längster `repo_key` gewinnt. `exists(&session.cwd)` vorher prüfen.
  - `scan_sessions`: `read_dir(projects_dir)` → nur Unterordner → darin nur Dateien mit Endung `jsonl`. Cache-Treffer bei gleichem (mtime, len), sonst `parse_session`. Nicht mehr gefundene Pfade aus dem Cache entfernen. Dann `assign_to_repos(…, |p| Path::new(p).exists())`.
  - `session_matches_search`: gleiche Wortlogik wie `crate::ui::repo_list::repo_matches_search`.
  - `resume_profile`: Profil klonen (sonst `AgentProfile { id: "claude", display_name: "Claude Code", program: "claude", args: vec![], command: None, launch_mode: Terminal, terminal_override: None, icon: None }`); wenn `args` leer und `command` gesetzt → `" --resume <id>"` an `command` anhängen, sonst `["--resume", id]` an `args` anhängen. `launch_mode` auf `AgentLaunchMode::Terminal` erzwingen.

- [ ] **Step 4: Run tests to verify they pass**

  Run: `cargo test claude_sessions`
  Expected: PASS.

- [ ] **Step 5: Commit**

  ```bash
  git add src/claude_sessions.rs
  git commit -m "feat: claude-sessions repos zuordnen, suchen und fortsetzen"
  ```

---

### Task 3: UI-Modul und Texte

**Files:**
- Create: `src/ui/claude_sessions.rs`
- Modify: `src/ui/mod.rs` (`pub mod claude_sessions;`), `src/i18n.rs`

**Interfaces:**
- Consumes: `RepoSessions`, `ClaudeSession`, `session_matches_search` (Task 2); `crate::gitlab::time::{relative_time, now_unix}`; Icons `assets/icons/claude.svg`, `assets/icons/search.svg`, `assets/icons/cross.svg`, `assets/icons/git-branch.svg`.
- Produces:
  ```rust
  #[derive(Default)]
  pub struct ClaudeSessionActions { pub resume: Option<(String /*cwd*/, String /*session id*/)> }
  pub fn filter_groups<'a>(groups: &'a [RepoSessions], query: &str) -> (Vec<(&'a RepoSessions, Vec<&'a ClaudeSession>)>, Vec<(&'a RepoSessions, Vec<&'a ClaudeSession>)>); // (repos, worktrees), leere Gruppen entfernt
  pub fn show_claude_sessions(ui: &mut egui::Ui, groups: &[RepoSessions], scanning: bool, lang: Language, search: &mut String, actions: &mut ClaudeSessionActions);
  ```
- i18n-Schlüssel (DE / EN):
  - `view_claude` – „Claude Sessions“ / „Claude Sessions“
  - `view_claude_tt` – „Claude-Code-Sessions in deinen Repos und Worktrees“ / „Claude Code sessions in your repos and worktrees“
  - `claude_search_hint` – „Sessions durchsuchen (Strg+F)“ / „Search sessions (Ctrl+F)“
  - `claude_none_found` – „Keine Claude Sessions gefunden“ / „No Claude sessions found“
  - `claude_none_hint` – „Gesucht wird in {} nach Sessions in deinen Suchpfaden.“ / „Looking in {} for sessions inside your search paths.“
  - `claude_no_matches` – „Keine Treffer für „{}““ / „No matches for “{}”“
  - `claude_resume` – „Fortsetzen“ / „Resume“
  - `claude_resume_tt` – „Session in {} mit Claude Code fortsetzen“ / „Resume session in {} with Claude Code“
  - `claude_scanning` – „Suche Claude Sessions…“ / „Searching Claude sessions…“
  - `claude_status` – „{} Sessions in {} Repos“ / „{} sessions in {} repos“
  - Abschnittstitel nutzen die vorhandenen Schlüssel `repo_section_repos` / `repo_section_worktrees`.

- [ ] **Step 1: Write the failing tests** (in `src/ui/claude_sessions.rs`)

  - `filter_groups_splits_and_drops_empty`: drei Gruppen (2 Repos, 1 Worktree), Query trifft nur eine Session in Repo A und die im Worktree → `repos.len()==1` mit 1 Session, `worktrees.len()==1`.
  - `filter_groups_empty_query_keeps_all`: Reihenfolge der Gruppen und Sessions unverändert.
  - Falls `src/i18n.rs` einen Test hat, der jeden Schlüssel in beiden Sprachen verlangt, die neuen Schlüssel dort eintragen; sonst `claude_keys_translated`: für jeden obigen Schlüssel ist `tr(De, k) != k` und `tr(En, k) != k`.

- [ ] **Step 2: Run tests to verify they fail**

  Run: `cargo test ui::claude_sessions`
  Expected: Kompilierfehler.

- [ ] **Step 3: Implement**

  - i18n-Einträge im Stil der vorhandenen `(Language::En, "…") => "…".to_string()`-Arme.
  - `show_claude_sessions`: Aufbau wie `show_repo_list`/`show_list_toolbar` in `src/ui/repo_list.rs` (Suchfeld mit eigener Id `claude_sessions_search`, Strg+F fokussiert, Lösch-Button mit `cross.svg`; Leerzustände mit gleicher Größe/Farbe; `ScrollArea` + `CollapsingHeader` je Abschnitt mit `id_salt(("claude_section", salt))`). Pro Gruppe ein innerer `CollapsingHeader` „{repo_name} ({n})“ (`default_open(true)`, Salt = repo_path). Karte: `Frame` wie die Repo-Karte in `show_repo_row` (gleiche Füllung, Rahmen, Radius, Innenabstand); Titel `strong`, `last_prompt` `weak` einzeilig mit `truncate()`; Fußzeile Branch-Icon + Branch · `relative_time(last_activity, now_unix(), lang)`; rechts Button mit `claude.svg` + `claude_resume`. Karte als `Sense::click()`, Hover-Text: cwd + Session-ID. Klick auf Karte oder Button setzt `actions.resume`. Bei `scanning && groups.is_empty()` Spinner statt Leerzustand. `claude_none_hint` bekommt `claude_projects_dir()` oder `~/.claude/projects`.

- [ ] **Step 4: Run tests to verify they pass**

  Run: `cargo test ui::claude_sessions && cargo test i18n`
  Expected: PASS.

- [ ] **Step 5: Commit**

  ```bash
  git add src/ui/claude_sessions.rs src/ui/mod.rs src/i18n.rs
  git commit -m "feat: ansicht für claude sessions"
  ```

---

### Task 4: App-Integration

**Files:**
- Modify: `src/app.rs` (`MainView` ~Z. 254, `MyApp`-Felder + beide Konstruktoren ~Z. 492 und ~Z. 3068, `ScanResult` ~Z. 38 und dessen Verarbeitung ~Z. 678, Refresh-Logik in `show_top_bar` ~Z. 2155, `show_status_bar` ~Z. 2317, CentralPanel ~Z. 2666)
- Modify: `src/app/merge_requests.rs` (`view_switcher` ~Z. 648)

**Interfaces:**
- Consumes: `claude_sessions::{scan_sessions, claude_projects_dir, SessionCache, RepoSessions, resume_profile}` (Task 2); `ui::claude_sessions::{show_claude_sessions, ClaudeSessionActions}` (Task 3); `git::launch_agent`; `self.agent_by_id("claude")`; `self.launch_err_tx`.
- Produces: `MainView::ClaudeSessions`; `MyApp::start_claude_scan(&mut self)`; `MyApp::resume_claude_session(&mut self, cwd: &str, id: &str)`.

- [ ] **Step 1: Write the failing tests** (im bestehenden `#[cfg(test)]`-Block von `src/app.rs`, mit dem dort vorhandenen Test-Konstruktor ~Z. 3068)

  - `claude_view_first_open_starts_scan`: neue App, `main_view = ClaudeSessions`, `app.ensure_claude_scan()` → `claude_scanning == true` und `claude_scanned_once == true`; zweiter Aufruf startet keinen weiteren Scan (Flag bleibt, kein zweiter Thread – über Zähler `claude_scan_generation` prüfen, der nur in `start_claude_scan` erhöht wird).
  - `claude_result_replaces_sessions`: `app.apply_claude_result(vec![group])` → `claude_sessions.len()==1`, `claude_scanning == false`.
  - `repo_scan_rescans_claude_only_after_first_open`: nach `ScanResult::Repos` ohne vorheriges Öffnen bleibt `claude_scan_generation == 0`; nach einmaligem Öffnen erhöht ein weiterer Repo-Scan die Generation.

- [ ] **Step 2: Run tests to verify they fail**

  Run: `cargo test app::tests::claude`
  Expected: Kompilierfehler.

- [ ] **Step 3: Implement**

  - Felder: `claude_sessions: Vec<RepoSessions>`, `claude_scanning: bool`, `claude_scanned_once: bool`, `claude_scan_generation: u64`, `claude_search: String`, `claude_cache: Arc<Mutex<SessionCache>>`, `claude_tx/claude_rx: (Sender, Receiver)<Vec<RepoSessions>>`.
  - `start_claude_scan`: `claude_scanning = true`, Generation +1; Thread mit Klon von `self.repos` und Cache-Arc ruft `scan_sessions` auf (leerer Vec, wenn `claude_projects_dir()` `None`), Ergebnis über `claude_tx`, danach `ctx.request_repaint()` (wie beim Repo-Scan).
  - `ensure_claude_scan`: wenn `!claude_scanned_once` → `claude_scanned_once = true; start_claude_scan()`.
  - `apply_claude_result`: setzt Liste, `claude_scanning = false`. In `update()` `claude_rx.try_recv()` abfragen.
  - Nach `ScanResult::Repos` (und `FetchDone`, wo neue Repos ankommen): `if claude_scanned_once { start_claude_scan() }`.
  - `view_switcher`: dritter `segment` nach dem MR-Segment, Text `view_claude`, rechts Anzahl aller Sessions (gleicher Stil wie die Repo-Anzahl), Tooltip `view_claude_tt`; Klick → `main_view = ClaudeSessions` + `ensure_claude_scan()`.
  - `show_top_bar`: in der Claude-Ansicht `busy = claude_scanning`, Refresh → `start_claude_scan()`, Tooltip-Logik wie in der MR-Ansicht, Sortier-Dropdown ausblenden (`if mr_view || claude_view { return; }`).
  - `show_status_bar`: Claude-Ansicht zeigt `status_message` (Vorrang) bzw. `claude_scanning` → `claude_scanning`, sonst `claude_status` mit Session- und Gruppenanzahl.
  - CentralPanel: nach dem MR-Zweig `if main_view == ClaudeSessions { show_claude_sessions(…); if let Some((cwd,id)) = actions.resume { self.resume_claude_session(&cwd,&id) } return; }`.
  - `resume_claude_session`: Profil = `resume_profile(self.agent_by_id("claude"), id)`; Terminal-Präferenz und Start-Thread, Statusmeldung (`starting_agent_in`) und Fehlerweg exakt wie `open_agent` (~Z. 1747); kein `record_usage`.

- [ ] **Step 4: Run tests to verify they pass**

  Run: `cargo test`
  Expected: alle Tests PASS (bisherige + neue).

- [ ] **Step 5: Commit**

  ```bash
  git add src/app.rs src/app/merge_requests.rs
  git commit -m "feat: claude sessions als dritte ansicht einbinden"
  ```

---

### Task 5: Abschlussprüfung

**Files:** keine neuen (nur Korrekturen aus den Prüfungen)

- [ ] **Step 1: Format und Lints**

  Run: `cargo fmt && cargo clippy --all-targets -- -D warnings`
  Expected: keine Warnungen.

- [ ] **Step 2: Komplette Tests**

  Run: `cargo test`
  Expected: `test result: ok`, 0 failed.

- [ ] **Step 3: Echtdaten-Stichprobe** (Linux, nur Parser): kleines Test-Binary ist nicht nötig – stattdessen temporärer `#[test] #[ignore]` `real_windows_sessions_parse`, der `/mnt/c/Users/<user>/.claude/projects` (falls vorhanden) über `parse_session` liest und Anzahl/Beispieltitel ausgibt; mit `cargo test real_windows_sessions_parse -- --ignored --nocapture` laufen lassen, Ergebnis prüfen (Titel und cwd plausibel, keine Panics), Test danach wieder entfernen.

- [ ] **Step 4: Commit** (nur falls Schritt 1–3 Änderungen erzeugt haben)

  ```bash
  git add -A src
  git commit -m "chore: fmt und clippy für claude sessions"
  ```
