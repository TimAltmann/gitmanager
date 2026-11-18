# Repo-Karte, Pull, Aufräumen, Favoriten – Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Repo-Karte mit Status-Chips/letztem Commit/`⋯`-Menü, Fast-Forward-Pull per Klick, Worktree entfernen + „Aufräumen …“, Favoriten für Repos und Claude Sessions.

**Architecture:** Git-Logik als reine, getestete Funktionen in `src/git.rs`, Kandidatenlogik fürs Aufräumen in neuem `src/cleanup.rs`, App-Verdrahtung je Feature als eigenes `impl MyApp`-Submodul unter `src/app/` (Muster: `src/app/new_worktree.rs` mit eigenem Kanal + `GitBusy`-Variante), UI in `src/ui/repo_list.rs` (Karte) und neuem `src/ui/repo_card_chips.rs`. Alle Hintergrundarbeit über Threads + `mpsc`, Ergebnis per `poll_*` im Frame.

**Tech Stack:** Rust, egui/eframe, git2 (libgit2), System-`git` für `worktree remove`/`branch -d`, glab-GraphQL über `gitlab::graphql`/`gitlab::service`.

**Spec:** `docs/superpowers/specs/2026-11-18-repo-karte-favoriten-design.md`

## Global Constraints

- Kein lokales `cargo`: alle Befehle über `docker compose run --rm dev cargo …` aus dem Repo-Root.
- Vor jedem Commit: `docker compose run --rm dev cargo fmt` und `docker compose run --rm dev cargo clippy --all-targets -- -D warnings` sauber.
- Commits mit der repo-lokalen Identität (`git config --local user.email` = `63297571+TimAltmann@users.noreply.github.com`), Nachricht klein, deutsch, Präfix `feat:`/`fix:`/`docs:`, Trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Jeder neue UI-Text als i18n-Key in `src/i18n.rs` für `Language::En` **und** `Language::De` (Muster `(Language::De, "key") => "…".to_string()`); Platzhalter `{}` bzw. über `tr_fmt`.
- Config: neue Felder nur mit `#[serde(default)]`, **kein** `config_version`-Sprung.
- GitManager setzt nie einen Branch zurück und forciert nichts ohne ausdrückliche Bestätigung im Dialog.
- Neue Icons nur aus Phosphor (MIT) nach `assets/icons/`, in `THIRD_PARTY_LICENSES.md` eintragen.
- `.exe` bleibt < 15 MB; keine neuen Crates außer bereits vorhandenen.

## Review Focus

1. **Worktree, dessen Ordner schon gelöscht ist** (Explorer): „Entfernen“ und „Aufräumen“ müssen ihn per `git worktree prune` bereinigen statt mit Fehler abzubrechen → Test in Task 3.
2. **Pull auf Branch ohne Upstream / detached HEAD / leeres Repo**: Ergebnis `NoUpstream`/`Detached`, keine Panik, Chip erscheint dort gar nicht (`behind == 0`) → Tests in Task 2.
3. **Aufräumen bei laufendem Auto-Fetch oder anderem `GitBusy`**: Start wird mit Hinweis `fetch_busy` abgewiesen, kein paralleler Ref-Zugriff → Test in Task 10.
4. **Favorit auf Repo, dessen Pfad sich nur in der Groß-/Kleinschreibung unterscheidet (Windows)**: Schlüssel über `AppConfig::repo_state_key` → Test in Task 4.
5. **Zwei MRs auf demselben Ordner (eigener + fremder mit `mr-<iid>`)**: eigener gewinnt, dann neuestes `updated_at` → Test in Task 6.

---

### Task 1: `RepoState` und `LastCommit` im Scan

**Files:**
- Modify: `src/git.rs` (Structs bei `RepoInfo` ~Zeile 70, `RepoInfo::new`, `get_repo_info` ~Zeile 2212, Tests in `mod tests`)

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct LastCommit { pub summary: String, pub author: String, pub time: i64 }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub enum InProgressKind { #[default] Other, Merge, Rebase, CherryPick, Revert }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub enum RepoState { #[default] Clean, Dirty, Conflict, InProgress(InProgressKind) }
  // RepoInfo neu: pub last_commit: Option<LastCommit>, pub state: RepoState
  pub fn repo_state_of(repo: &Repository, dirty_files: usize) -> RepoState
  pub fn last_commit_of(repo: &Repository) -> Option<LastCommit>
  ```
  `RepoInfo::new` setzt `state = if dirty { Dirty } else { Clean }`, `last_commit = None`.

- [ ] **Step 1: Failing tests** in `src/git.rs` `mod tests` (Helfer `init_repo_with_commit`, `commit_file`, `git_cmd` existieren):
  - `repo_state_clean_dirty`: frisches Repo → `RepoState::Clean`; Datei ändern → `Dirty`.
  - `repo_state_conflict_and_merge`: zwei Branches ändern dieselbe Zeile, `git merge` via `git_cmd` (Fehler ignorieren) → `Conflict`; nach `git add` der Datei (Konflikt gelöst, Merge offen) → `InProgress(InProgressKind::Merge)`.
  - `last_commit_reads_head`: nach `commit_file(dir, "a.txt", "x", "Fix rounding\n\nbody")` → `summary == "Fix rounding"`, `author` = Name aus Test-Signatur, `time > 0`; leeres `git init` → `None`.
  - `get_repo_info_fills_state_and_last_commit`.
- [ ] **Step 2:** `docker compose run --rm dev cargo test repo_state_ last_commit_ get_repo_info_fills` → FAIL (nicht definiert).
- [ ] **Step 3: Implementieren.** `repo_state_of`: Konflikte (`has_merge_conflicts`) zuerst → `Conflict`; sonst `repo.state()` ≠ `Clean` → `InProgress` (Mapping: `Merge`→Merge, `Rebase*`/`ApplyMailbox*`→Rebase, `CherryPick*`→CherryPick, `Revert*`→Revert, Rest→Other); sonst `dirty_files > 0` → `Dirty`, sonst `Clean`. `last_commit_of`: `head().peel_to_commit()`, `summary()`, `author().name()`, `time().seconds()`; jede Fehlerstelle → `None`. In `get_repo_info` beide setzen (`dirty` bleibt `dirty_files > 0`).
- [ ] **Step 4:** Tests aus Step 2 + `docker compose run --rm dev cargo test` komplett → PASS.
- [ ] **Step 5: Commit** `feat: repo-zustand und letzter commit im scan`.

---

### Task 2: `fast_forward`

**Files:**
- Modify: `src/git.rs` (neue Funktion nahe `ahead_behind`, Tests)

**Interfaces:**
- Consumes: `fetch_refspecs(path, remote, refspecs)`, `ahead_behind_in`, `dirty_count`, bestehender Fallback auf System-`git` in den Fetch-Funktionen.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum FfOutcome { UpToDate, FastForwarded { commits: usize }, Dirty { files: usize },
                       Diverged { ahead: usize, behind: usize }, NoUpstream, Detached }
  pub fn fast_forward(path: &Path) -> anyhow::Result<FfOutcome>
  ```

- [ ] **Step 1: Failing tests** (Upstream-Setup: Bare-Repo als `origin`, Klon A pusht, Klon B ist behind; vorhandene Muster für Clone/Push in `mod tests` verwenden):
  - `ff_fast_forwards_clean_branch` → `FastForwarded { commits: 2 }`, HEAD von B == HEAD von A, Arbeitsverzeichnis enthält neue Datei.
  - `ff_up_to_date` → `UpToDate`.
  - `ff_refuses_dirty` (B hat geänderte Datei) → `Dirty { files: 1 }`, HEAD unverändert.
  - `ff_refuses_diverged` (B hat eigenen Commit) → `Diverged { ahead: 1, behind: 2 }`, HEAD unverändert.
  - `ff_no_upstream` (Branch ohne Tracking) → `NoUpstream`; `ff_detached` → `Detached`.
- [ ] **Step 2:** `docker compose run --rm dev cargo test ff_` → FAIL.
- [ ] **Step 3: Implementieren.** Reihenfolge: detached → `Detached`; Upstream über `find_branch(..).upstream()` (Remote-Name aus `branch.<name>.remote`, Refspec `+refs/heads/<merge>:refs/remotes/<remote>/<merge>`) fehlt → `NoUpstream`; fetchen; `ahead_behind_in`; `behind == 0` → `UpToDate`; `ahead > 0` → `Diverged`; `dirty_count > 0` → `Dirty`; sonst Checkout des Upstream-Trees mit `CheckoutBuilder::safe()` und danach `reference.set_target(up_oid, "gitmanager: fast-forward")` → `FastForwarded { commits: behind }`. Fetch-Fehler als `Err` durchreichen.
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: fast-forward eines branches auf seinen upstream`.

---

### Task 3: Worktree entfernen, Branch-Sicherheit, Upstream „gone“

**Files:**
- Modify: `src/git.rs` (neue Funktionen + Tests)

**Interfaces:**
- Produces:
  ```rust
  /// Warum ein lokaler Branch gefahrlos gelöscht werden darf; None = nicht sicher
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum BranchSafety { Pushed, MergedIntoDefault, MrMerged }
  pub fn branch_safety(repo_path: &Path, branch: &str, mr_merged: bool) -> Option<BranchSafety>
  pub fn upstream_gone(repo: &Repository, branch: &str) -> bool
  pub fn gone_local_branches(repo_path: &Path) -> Vec<String> // ohne aktuellen + in Worktrees ausgecheckte
  pub fn remove_worktree(worktree_path: &Path, main_repo: &Path, force: bool,
                         delete_branch: Option<(&str, bool /*force_delete*/)>) -> anyhow::Result<()>
  pub fn delete_local_branch(repo_path: &Path, branch: &str, force: bool) -> anyhow::Result<()>
  ```
  Hauptrepo eines Worktrees = Elternordner von `RepoInfo.common_dir` (wenn dessen Name `.git` ist), sonst `common_dir` selbst.

- [ ] **Step 1: Failing tests:**
  - `remove_clean_worktree`: `git worktree add` → `remove_worktree(.., false, None)` → Ordner weg, `git worktree list` ohne Eintrag.
  - `remove_dirty_worktree_needs_force`: geänderte Datei → `force=false` liefert `Err`; `force=true` → Ok.
  - `remove_worktree_already_deleted_folder` (Review Focus 1): Ordner per `fs::remove_dir_all` gelöscht → `remove_worktree` Ok, Eintrag geprunt.
  - `remove_worktree_deletes_branch`: mit `Some(("feat", false))` → Branch weg; nicht gemergter Branch mit `false` → Err nach Entfernen des Worktrees (Fehlertext nennt Branch), mit `true` → weg.
  - `branch_safety_cases`: gepushter Branch → `Pushed`; in `origin/main` gemergt, Upstream weg → `MergedIntoDefault`; eigener Commit, `mr_merged=true` → `MrMerged`; eigener Commit, `false` → `None`.
  - `gone_upstream_detected`: Branch mit Upstream, Remote-Branch gelöscht + `fetch --prune` → `upstream_gone == true`; `gone_local_branches` enthält ihn, nicht aber den aktuellen Branch.
- [ ] **Step 2:** `docker compose run --rm dev cargo test remove_ branch_safety gone_upstream` → FAIL.
- [ ] **Step 3: Implementieren.** `remove_worktree` über System-`git` (`std::process::Command`, wie `fetch_with_git_cli`; unter Windows `CREATE_NO_WINDOW` wie dort): `git -C <main> worktree remove [--force] <path>`; existiert der Ordner nicht → stattdessen `git -C <main> worktree prune`. Danach immer `worktree prune`, dann optional `delete_local_branch` (`git branch -d`/`-D`). `branch_safety`: `Pushed` wenn Upstream existiert und `ahead == 0`; `MergedIntoDefault` wenn Branch-Commit Vorfahre von `refs/remotes/origin/HEAD` (Fallback `origin/main`, `origin/master`) via `graph_descendant_of`/Gleichheit; sonst `MrMerged` wenn `mr_merged`; sonst `None`. `upstream_gone`: `branch.<b>.merge` konfiguriert, aber `refs/remotes/<remote>/<name>` fehlt.
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: worktree entfernen und verwaiste branches erkennen`.

---

### Task 4: Favoriten in der Config und Sortierung

**Files:**
- Modify: `src/config.rs` (`AppConfig` + Default ~Zeile 1003 + Tests), `src/ui/repo_list.rs` (`sort_main_repos` ~151, `sorted_main_repos` ~172, `split_visible` ~223, Tests), `src/ui/claude_sessions.rs` (`visible_lists` ~48, Tests)

**Interfaces:**
- Produces:
  ```rust
  // AppConfig
  #[serde(default)] pub favorite_repos: Vec<String>,     // AppConfig::repo_state_key(path)
  #[serde(default)] pub favorite_sessions: Vec<String>,  // Session-IDs
  impl AppConfig {
      pub fn is_favorite_repo(&self, path: &Path) -> bool;
      pub fn toggle_favorite_repo(&mut self, path: &Path);   // fügt hinzu / entfernt
      pub fn is_favorite_session(&self, id: &str) -> bool;
      pub fn toggle_favorite_session(&mut self, id: &str);
  }
  // visible_lists bekommt zusätzlich `favorites: &[String]` als letzten Parameter
  ```

- [ ] **Step 1: Failing tests:**
  - `config.rs`: `favorites_roundtrip` (serialisieren/deserialisieren erhält beide Listen); `old_config_without_favorites_loads` (JSON ohne Felder → leer); `toggle_favorite_repo_twice_removes`; `favorite_repo_key_is_normalized` (unter `cfg(windows)`: `C:\Dev\App` vs. `c:\dev\app` → beide favorisiert; unter Linux: Schlüssel von `/a/App` und `/a/App/` gleich).
  - `repo_list.rs`: `favorites_first_in_last_used_mode` und `favorites_first_in_name_mode` (Favoriten zuerst, untereinander nach Modus, Rest danach unverändert); `split_visible_keeps_favorites_first_per_section` (Favorit-Worktree steht in „Worktrees“ oben, nicht in „Repositories“).
  - `claude_sessions.rs`: `visible_lists_favorites_first` (Favorit mit ältester `last_activity` steht zuerst; offene Favoriten erscheinen weiter nur in der Open-Liste).
- [ ] **Step 2:** `docker compose run --rm dev cargo test favorite visible_lists` → FAIL.
- [ ] **Step 3: Implementieren.** Sortierung als stabiler Sort nach `!is_favorite` **nach** der bestehenden Sortierung; `split_visible` behält Reihenfolge der sortierten `repos` bei. `visible_lists`: `rest` stabil nach `!favorites.contains(id)` sortieren. Aufrufer von `visible_lists` anpassen (`config.favorite_sessions`).
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: favoriten für repos und claude sessions in der config`.

---

### Task 5: GraphQL – gemergte MRs je Quell-Branch

**Files:**
- Modify: `src/gitlab/graphql.rs` (neben `recent_notes` ~645), `src/gitlab/service.rs` (neben `fetch_recent_notes` ~759), `scripts/demo/fake-glab.py`

**Interfaces:**
- Produces:
  ```rust
  pub const MERGED_BRANCHES_CHUNK: usize = 10; // Projekte je Abfrage
  /// Aliase p0.., je Projekt mergeRequests(state: merged, sourceBranches: $b<i>, first: 100) { nodes { iid sourceBranch } }
  pub fn merged_by_branches(projects: &[(String, Vec<String>)]) -> Vec<Query>
  pub fn parse_merged_by_branches(data: &Value, count: usize) -> Vec<Vec<(u64, String)>>
  /// Je Eingabeprojekt die gemergten Quell-Branches; Fehler → Err der ersten Abfrage
  pub fn fetch_merged_branches(runner: &dyn GlabRunner, host: &str,
                               projects: &[(String, Vec<String>)])
      -> Result<Vec<HashSet<String>>, GlabErrorKind>
  ```

- [ ] **Step 1: Failing tests** (Muster: Query-Text-Test ~Zeile 2397, Parser-Tests mit `serde_json::json!`):
  - `merged_by_branches_query_text`: Text enthält `p0: project(fullPath: $p0) { mergeRequests(state: merged, sourceBranches: $b0, first: 100) { nodes { iid sourceBranch } } }` und Variablen `$p0: ID!`, `$b0: [String!]`.
  - `merged_by_branches_chunks`: 11 Projekte → 2 Queries.
  - `parse_merged_by_branches_reads_nodes_and_missing_project` (fehlendes Projekt → leerer Vec).
  - `service.rs`: `fetch_merged_branches_with_fake_runner` (vorhandener `FakeGlab` mit vorbereiteter Antwort).
- [ ] **Step 2:** `docker compose run --rm dev cargo test merged_by_branches fetch_merged_branches` → FAIL.
- [ ] **Step 3: Implementieren** nach dem Muster von `recent_notes`/`fetch_recent_notes` (inkl. `schema_for`/`store_schema`). `fake-glab.py`: Operation `GmMergedBranches` beantworten (für `acme/shop-backend` Branch `feature/region-prices` als gemergt melden, sonst leer).
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: gemergte merge requests je quell-branch abfragen`.

---

### Task 6: MR-Zuordnung zu Repos und Sprung in die MR-Ansicht

**Files:**
- Create: `src/app/repo_badges.rs` (`impl MyApp` + reine Funktion + Tests)
- Modify: `src/app.rs` (`mod repo_badges;`, Feld `repo_mrs: HashMap<PathBuf, MrBadge>`, Aufruf nach MR-Abruf in `src/app/merge_requests.rs` ~515 und nach jedem Scan neben `update_mr_clones`), `src/ui/merge_requests.rs` (`MrViewState` ~122 + `draw_row` ~1495)

**Interfaces:**
- Consumes: `MyApp::mr_checked_out_at(&MergeRequest) -> Option<PathBuf>` (`src/app/mr_checkout.rs:180`), `MrSnapshot { mine, others, uninvolved }`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct MrBadge { pub host_key: String, pub id: String, pub iid: u64, pub title: String,
                       pub draft: bool, pub approved: u32, pub required: Option<u32>,
                       pub pipeline: PipelineState, pub mine: bool, pub updated_at: Option<i64> }
  impl MrBadge { pub fn from_mr(mr: &MergeRequest, mine: bool) -> Self }
  /// Ein Badge je Ordner; Priorität: mine vor fremd, dann neuestes updated_at
  pub(crate) fn pick_badges(hits: Vec<(PathBuf, MrBadge)>) -> HashMap<PathBuf, MrBadge>
  impl MyApp { pub(super) fn update_repo_mrs(&mut self); pub(super) fn focus_mr(&mut self, host_key: &str, id: &str) }
  // MrViewState neu: pub focus: Option<(String /*host_key*/, String /*id*/)>
  ```
  `approved` = `approved_by.len()` ohne Bots wenn `ignore_bot_approvals` (bestehende Zählhilfe der MR-Zeile wiederverwenden), `required` = `approvals_required` bzw. `approval_goal` aus Config wie in der Approval-Leiste.

- [ ] **Step 1: Failing tests** in `src/app/repo_badges.rs`:
  - `pick_badges_prefers_mine_then_newest` (Review Focus 5).
  - `update_repo_mrs_maps_checked_out_branch` mit `test_app_with_glab` (Muster `src/app.rs:3049`): Snapshot mit MR auf `feature/x`, Repo auf `feature/x` mit passendem Remote → Map enthält Repo-Pfad; ohne Snapshot → leer.
  - `focus_mr_switches_view_and_sets_focus`: `main_view == MainView::MergeRequests`, `mr_view.focus == Some(..)`.
- [ ] **Step 2:** `docker compose run --rm dev cargo test repo_badges` → FAIL.
- [ ] **Step 3: Implementieren.** In `draw_row`: Zeile mit `state.focus == (mr.host_key, mr.id)` bekommt Rahmen `selection.stroke` (2 px, wie `repo_highlight`) und `ui.scroll_to_rect(.., Some(Align::Center))` einmalig; danach `focus = None` setzen (über eine Rückgabe in `MrViewActions`, Feld `focus_done: bool`). Liegt der MR unter „Von anderen“, aber Chips filtern ihn weg, `show_uninvolved`/Chips nicht ändern – nur Fokus verwerfen.
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: merge requests den repo-karten zuordnen`.

---

### Task 7: Neue Repo-Karte (Chips, letzter Commit, `⋯`-Menü, eine IDE, Stern)

**Files:**
- Create: `src/ui/repo_card_chips.rs` (Chip-Zeichnen + reine Text-/Farb-Funktionen + Tests), `assets/icons/star.svg`, `assets/icons/star-fill.svg`, `assets/icons/dots-three.svg`, `assets/icons/arrow-up.svg`, `assets/icons/arrow-down.svg` (Phosphor)
- Modify: `src/ui/mod.rs` (Modul), `src/ui/repo_list.rs` (`show_repo_list` ~249, `show_repo_row` ~380, `RepoListActions` ~178, Tests), `src/app.rs` (Aufruf ~2732, Aktionen auswerten), `src/i18n.rs`, `THIRD_PARTY_LICENSES.md`

**Interfaces:**
- Consumes: `RepoState`, `LastCommit` (Task 1), `MrBadge` (Task 6), `AppConfig::is_favorite_repo` (Task 4), `claude_live::OpenSession { repo_path, live.status }`, `gitlab::time::relative_time(ts, now, lang)`.
- Produces:
  ```rust
  /// Zusatzdaten für die Karten (statt weiterer Einzelparameter)
  pub struct RepoListContext<'a> {
      pub mrs: &'a HashMap<PathBuf, MrBadge>,
      pub claude: &'a HashMap<PathBuf, LiveStatus>, // aus OpenSession.repo_path
      pub pulling: Option<&'a Path>,
      pub now: i64,
  }
  // show_repo_list(ui, repos, config, actions, fetching, highlight, search, ctx: &RepoListContext)
  // RepoListActions neu:
  pub toggle_favorite: Option<PathBuf>,
  pub pull: Option<PathBuf>,
  pub open_mr: Option<(String, String)>,      // host_key, id
  pub focus_claude: Option<PathBuf>,          // Terminal der Session in diesem Ordner
  pub copy_text: Option<String>,
  pub open_url: Option<String>,
  pub remove_worktree: Option<PathBuf>,
  pub cleanup: bool,                           // Toolbar „Aufräumen …“
  // repo_card_chips.rs
  pub fn state_chip(lang: Language, state: RepoState, dirty_files: usize) -> (String, Color32)
  pub fn card_ide<'a>(profile: &'a LanguageProfile, selected: Option<&str>) -> Option<&'a IdeConfig>
  pub fn remote_web_url(repo: &RepoInfo, aliases: &[HostAlias]) -> Option<String>
  ```

- [ ] **Step 1: Failing tests:**
  - `repo_card_chips.rs`: `state_chip_texts_and_colors` (Clean → „clean“/`COLOR_CLEAN`, Dirty 3 → „3 changed“/`COLOR_DIRTY`, Conflict → rot, `InProgress(Rebase)` → „Rebase in progress“, je En/De); `card_ide_prefers_selected_then_default_then_first` (ausgeblendete `selected` → Default); `remote_web_url_ssh_and_https` (`git@gitlab.example.com:acme/app.git` → `https://gitlab.example.com/acme/app`; Alias `ssh.gitlab.example.com`→`gitlab.example.com` angewandt; lokaler Pfad → `None`).
  - `repo_list.rs` (egui-Tests nach Muster `toolbar_new_claude_session_button_sets_action` ~1792): `star_click_sets_toggle_favorite`; `behind_chip_click_sets_pull` (nur bei `behind > 0` vorhanden); `mr_chip_click_sets_open_mr`; `only_one_ide_button_visible`; `more_menu_has_remove_only_for_worktrees`; `toolbar_cleanup_button_sets_action`. Bestehende Tests, die das alte Layout prüfen (`repo_dirty_indicator_logic`, `branch_button_tooltip_embeds_behind`, IDE-Button-Tests), an das neue Layout anpassen, nicht löschen.
- [ ] **Step 2:** `docker compose run --rm dev cargo test repo_card_chips repo_list` → FAIL.
- [ ] **Step 3: Implementieren** nach Spec Abschnitt 1 (Layout-Skizze dort ist verbindlich): Zeile 1 Stern · Name (Tooltip Pfad) · „Worktree von“ · Branch-Dropdown · Solution · Config-Dropdowns; Zeile 2 Chips (Stil wie MR-Chips: `corner_radius(10)`, `inner_margin(6,2)`, 11 px, Icon 12 px); Zeile 3 letzter Commit (Betreff gekürzt auf verfügbare Breite mit `…`) links, rechts Explorer, Shell, `card_ide`, Agents, `⋯` (`egui::menu`/`Popup` wie Profil-Dropdown). `⋯`-Einträge: übrige IDEs (setzen `ide_open`; App setzt `selected_ide` wie im Tray-Pfad `src/app.rs:1402`), Profil-Untermenü, „Neuer Worktree …“ (nur `!is_linked_worktree`), „Pfad kopieren“, „Branch kopieren“, „Im Browser öffnen“ (nur wenn `remote_web_url` Some), „Worktree entfernen …“ (nur `is_linked_worktree`). Das alte Warn-Icon neben dem Branch und der `+`-Button der Karte entfallen. Toolbar: Button „Aufräumen …“ neben „Neuer Worktree“. In `app.rs`: `copy_text` → `ctx.copy_text`, `open_url` → `ctx.open_url(OpenUrl::new_tab)`, `toggle_favorite` → `config.toggle_favorite_repo` + `save` (Fehler wie `note_config_save_error`) + `repo_sort_dirty = true`, `open_mr` → `focus_mr`, `focus_claude` → bestehende Fokus-Funktion der Claude-Ansicht für die PID der Session mit diesem `repo_path`. `pull`/`remove_worktree`/`cleanup` werden in Task 8–10 verdrahtet (bis dahin ignoriert). `claude`-Map in `app.rs` aus `build_open(&claude.live, &claude.scan)` ableiten (leer, solange kein Live-Status vorliegt).
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS; `docker compose run --rm dev cargo build --release --target x86_64-pc-windows-gnu` baut.
- [ ] **Step 5: Commit** `feat: neue repo-karte mit status-chips und menü`.

---

### Task 8: Pull per Klick verdrahten

**Files:**
- Create: `src/app/pull.rs`
- Modify: `src/app.rs` (`mod pull;`, `GitBusy::Pull { repo: PathBuf }` ~270 inkl. `holds_refs`, Feld `pull_ui`, `poll_pull` im Frame neben `poll_new_worktree` ~2656, Aktion `pull` auswerten, `RepoListContext.pulling`), `src/i18n.rs`

**Interfaces:**
- Consumes: `git::fast_forward`, `FfOutcome` (Task 2), `RepoListActions.pull` (Task 7).
- Produces: `impl MyApp { pub(super) fn start_pull(&mut self, path: PathBuf); pub(super) fn poll_pull(&mut self, ctx: &egui::Context) }`, `pub(super) fn pull_message(lang: Language, repo: &str, r: &anyhow::Result<FfOutcome>) -> String`.

- [ ] **Step 1: Failing tests** in `src/app/pull.rs`: `pull_message_texts` (alle `FfOutcome`-Varianten + Fehler, En und De; z. B. De `FastForwarded{3}` → „shop-backend: 3 Commits übernommen“, `Dirty{2}` → „shop-backend: 2 lokale Änderungen – erst committen oder stashen“, `Diverged{1,2}` → „shop-backend: Branch ist divergiert (↑1 ↓2) – bitte manuell lösen“); `start_pull_rejected_while_busy` (bei `git_busy != Idle` oder `auto_fetching` → `status_message == tr("fetch_busy")` bzw. `auto_fetch_running`, kein Thread).
- [ ] **Step 2:** `docker compose run --rm dev cargo test pull_` → FAIL.
- [ ] **Step 3: Implementieren** nach Muster `start_repo_fetch` (~1556): Busy-Prüfungen, `GitBusy::Pull`, Thread mit `catch_unwind`, Ergebnis über eigenen Kanal; `poll_pull` setzt `status_message`, `GitBusy::Idle`, startet `start_scan()`.
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: pull per klick auf den behind-chip`.

---

### Task 9: Dialog „Worktree entfernen“

**Files:**
- Create: `src/app/remove_worktree.rs`
- Modify: `src/app.rs` (`mod`, `GitBusy::Remove`, Feld, `poll_*`/`show_*` im Frame, Aktion `remove_worktree`), `src/i18n.rs`

**Interfaces:**
- Consumes: `git::remove_worktree`, `git::branch_safety`, `BranchSafety` (Task 3), `repo_mrs` (Task 6; `mr_merged` kommt erst über Aufräumen – hier `false`), offene Claude Sessions (`OpenSession.repo_path`).
- Produces:
  ```rust
  pub(super) struct RemoveDialog { pub path: PathBuf, pub main: PathBuf, pub branch: Option<String>,
      pub dirty_files: usize, pub ahead: usize, pub safety: Option<BranchSafety>,
      pub delete_branch: bool, pub claude_running: bool }
  impl RemoveDialog { pub fn new(repo: &RepoInfo, claude_running: bool) -> Self
                      pub fn needs_force(&self) -> bool /* dirty_files > 0 || ahead > 0 */ }
  impl MyApp { pub(super) fn open_remove_worktree(&mut self, path: &Path);
               pub(super) fn show_remove_worktree_dialog(&mut self, ctx: &egui::Context);
               pub(super) fn poll_remove_worktree(&mut self, ctx: &egui::Context) }
  ```

- [ ] **Step 1: Failing tests:** `dialog_needs_force_when_dirty_or_ahead`; `delete_branch_disabled_without_safety` (`safety == None` → Checkbox deaktiviert, `delete_branch` bleibt false); `remove_runs_and_rescans` (Temp-Repo + Worktree, `test_app` mit Repos, Dialog bestätigen, Worker abwarten → Ordner weg, `start_scan` angestoßen).
- [ ] **Step 2:** `docker compose run --rm dev cargo test remove_worktree_dialog dialog_needs_force delete_branch_disabled remove_runs` → FAIL.
- [ ] **Step 3: Implementieren** (Texte laut Spec Abschnitt 3; Button `Entfernen` bzw. roter `Trotzdem entfernen`; Hinweis „Eine Claude Session läuft in diesem Ordner“; Branch-Löschen mit `force_delete = safety == Some(MrMerged)`). Fehler → `self.error` mit übersetztem Text.
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: worktree aus der repo-liste entfernen`.

---

### Task 10: Dialog „Aufräumen …“

**Files:**
- Create: `src/cleanup.rs` (reine Kandidatenlogik + Tests), `src/app/cleanup.rs` (Worker, Dialog)
- Modify: `src/main.rs` (`mod cleanup;`), `src/app.rs` (`mod`, `GitBusy::Cleanup`, Feld, Frame-Aufrufe, Aktion `cleanup`), `src/i18n.rs`

**Interfaces:**
- Consumes: `git::fetch_targets`, `git::fetch_all` (prunt bereits: `fetch_one_remote` → `prune = true`), `upstream_gone`, `gone_local_branches`, `branch_safety`, `remove_worktree`, `delete_local_branch` (Task 3), `service::fetch_merged_branches` (Task 5), `mr_hosts`/`ProjectRef` für Projektpfade der Worktrees.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum CleanupReason { MrMerged { iid: u64 }, UpstreamGone }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum CleanupTarget { Worktree { path: PathBuf, main: PathBuf, branch: Option<String> },
                           Branch { repo: PathBuf, branch: String } }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct CleanupCandidate { pub target: CleanupTarget, pub reasons: Vec<CleanupReason>,
      pub dirty_files: usize, pub ahead: usize, pub selected: bool,
      pub delete_branch: bool, pub branch_safety: Option<BranchSafety> }
  /// merged: (Repo-Pfad des Worktrees → gemergte MR-iid) aus der GitLab-Abfrage
  pub fn collect_candidates(repos: &[RepoInfo], gone: &HashMap<PathBuf, Vec<String>>,
                            merged: &HashMap<PathBuf, u64>) -> Vec<CleanupCandidate>
  ```
  `selected = dirty_files == 0 && ahead == 0`; Worktree-Kandidat, wenn Branch gone **oder** in `merged`; Branch-Kandidat aus `gone` nur für Haupt-Klone (`!is_linked_worktree`), ohne Branches, die irgendwo als Worktree ausgecheckt sind (`RepoInfo.worktrees`).

- [ ] **Step 1: Failing tests** in `src/cleanup.rs`: `worktree_with_merged_mr_is_candidate`; `worktree_with_gone_upstream_is_candidate_with_both_reasons_merged`; `dirty_or_ahead_candidate_is_preselected_off`; `gone_branch_checked_out_in_worktree_is_skipped`; `no_candidates_empty`. In `src/app/cleanup.rs`: `cleanup_rejected_while_busy` (Review Focus 3); `cleanup_without_glab_skips_mr_reason_and_flags_hint` (Runner liefert Fehler → nur `UpstreamGone`-Kandidaten, Dialog-Flag `gitlab_unavailable == true`).
- [ ] **Step 2:** `docker compose run --rm dev cargo test cleanup` → FAIL.
- [ ] **Step 3: Implementieren.** Worker: Fetch (prune) aller `fetch_targets` → `gone` je Repo (`upstream_gone` für Worktree-Branches, `gone_local_branches` für Klone) → für Worktrees mit Remote auf bekanntem GitLab-Host ihre Branchnamen je Projekt sammeln (Quellprojekt = Projekt des `origin`-Remotes) → `fetch_merged_branches` je Host → `collect_candidates`. Dialog: Liste mit Checkbox, Name/Pfad, Gründe („MR !{iid} gemergt“, „Remote-Branch gelöscht“), orange Warnzeile bei Änderungen/↑, je Worktree Checkbox „Branch mitlöschen“ (aktiv nur mit `branch_safety`), Hinweisbanner bei `gitlab_unavailable`, Button „{n} entfernen“; danach Ergebnisliste ok/Fehler je Eintrag und `start_scan()`. Keine Kandidaten → „Nichts aufzuräumen“.
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: aufräumen von worktrees und verwaisten branches`.

---

### Task 11: Favoriten-Stern auf Claude-Session-Karten

**Files:**
- Modify: `src/ui/claude_sessions.rs` (`ClaudeSessionActions` ~31, `show_session_card` ~405, `show_open_card` ~558, `ViewData` ~67, Tests), `src/app/claude_sessions.rs` (Aktion auswerten)

**Interfaces:**
- Consumes: `AppConfig::{is_favorite_session, toggle_favorite_session}` (Task 4), Stern-Icons (Task 7).
- Produces: `ClaudeSessionActions.toggle_favorite: Option<String>` (Session-ID).

- [ ] **Step 1: Failing tests** (Muster `resume_button_sits_above_card_click_area` ~724): `star_click_toggles_favorite_without_resume` (Klick auf Stern setzt `toggle_favorite`, **nicht** `resume`); `open_card_shows_filled_star_for_favorite`.
- [ ] **Step 2:** `docker compose run --rm dev cargo test star_click_toggles open_card_shows_filled_star` → FAIL.
- [ ] **Step 3: Implementieren.** Stern links in der Titelzeile; Button-Klick wie beim Fortsetzen-Button vom Karten-Klick ausnehmen (`button_clicked`). App: toggeln, speichern (Fehler wie `note_config_save_error`).
- [ ] **Step 4:** `docker compose run --rm dev cargo test` → PASS.
- [ ] **Step 5: Commit** `feat: claude sessions als favoriten markieren`.

---

### Task 12: README, Demo-Medien, Abschluss

**Files:**
- Modify: `README.md` (Feature-Tabelle: Zeilen für Status-Chips/letzter Commit, Pull, Aufräumen, Favoriten; Dirty-Zeile ersetzen; Projektlayout um neue Dateien), `scripts/demo/make-demo-repos.sh` (ein Repo mit ↑-Commit, ein Worktree mit gelöschtem Remote-Branch, ein Favorit in der Demo-Config), `scripts/demo/record.sh` (Szene `shots`: Karte mit `⋯`-Menü, Aufräumen-Dialog), `assets/media/*`

- [ ] **Step 1:** Demo-Skripte anpassen; `./scripts/demo/run.sh shots repos` ausführen → neue PNG/GIF in `assets/media/`, visuell prüfen (Read der PNGs).
- [ ] **Step 2:** README aktualisieren (Englisch, Stil der bestehenden Tabelle).
- [ ] **Step 3:** `docker compose run --rm dev cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release --target x86_64-pc-windows-gnu` → alles grün; `.exe`-Größe < 15 MB (`ls -l target/x86_64-pc-windows-gnu/release/gitmanager.exe`).
- [ ] **Step 4: Commit** `docs: readme und demo-medien für repo-karte, aufräumen und favoriten`.
