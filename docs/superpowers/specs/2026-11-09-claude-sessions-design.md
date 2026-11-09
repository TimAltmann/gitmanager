# Claude Sessions – Design

Datum: 2026-11-09 · Branch: `feature/worktrees-getrennt-suche`

## Ziel

Neben „Repositories“ und „Merge Requests“ gibt es eine dritte Hauptansicht
**„Claude Sessions“**. Sie zeigt alle vorhandenen Claude-Code-Sessions, deren
Arbeitsordner in einem gescannten Repo oder Worktree liegt, ist durchsuchbar
und setzt eine gewählte Session per Klick in ihrem Ordner fort.

## Entscheidungen (mit dem Nutzer abgestimmt)

| Thema | Entscheidung |
|---|---|
| Anordnung | Nach Repo gruppiert, getrennt in „Repositories“ und „Worktrees“ |
| Suche | Nur Metadaten (Titel, letzter Prompt, Repo-/Worktree-Name, Branch) |
| Verwaiste Sessions (Ordner fehlt) | Ausblenden |
| Sessions außerhalb gescannter Repos | Ausblenden |
| WSL-Sessions (`~/.claude` in WSL, Linux-Pfade) | Nicht unterstützt |

## Hintergrund: Ablage der Sessions

- Ort: `%CLAUDE_CONFIG_DIR%` oder `%USERPROFILE%\.claude`, darin
  `projects\<kodierter-pfad>\<session-uuid>.jsonl`.
- Der Ordnername ist verlustbehaftet kodiert (jedes Nicht-Alphanumerische → `-`)
  und wird **nicht** dekodiert. Maßgeblich ist das Feld `cwd` in den Zeilen.
- `cwd` kommt als `C:\Code\x` oder `/c/Code/x` vor.
- Unterordner `<uuid>/` (subagents, tool-results, workflows) werden ignoriert.
- JSONL, ein Objekt pro Zeile. Relevante Felder/Typen:
  - beliebige Zeile mit `cwd`, `gitBranch`, `sessionId`
  - `{"type":"custom-title","customTitle":…}` (aus `/rename`)
  - `{"type":"ai-title","aiTitle":…}`
  - `{"type":"last-prompt","lastPrompt":…}`
  - `{"type":"user","message":{"content":…}}` (erste echte Nutzer-Nachricht,
    `isMeta` ≠ true, als Titel-Fallback)
- Fortsetzen: `claude --resume <session-id>`, ausgeführt **im Original-`cwd`**
  (Claude sucht Sessions pro Projektordner).

## Architektur

### 1. `src/claude_sessions.rs` (Logik, keine UI)

```rust
pub struct ClaudeSession {
    pub id: String,              // Dateiname ohne .jsonl
    pub cwd: PathBuf,            // normalisiert auf Windows-Form
    pub title: String,           // custom-title > ai-title > erste User-Nachricht > id
    pub last_prompt: Option<String>,
    pub git_branch: Option<String>,
    pub last_activity: SystemTime, // mtime der Datei
}
```

Funktionen (alle ohne Seiteneffekte außer Dateilesen, einzeln testbar):

- `claude_projects_dir() -> Option<PathBuf>` – `CLAUDE_CONFIG_DIR` bzw.
  Home + `.claude`, plus `projects`.
- `parse_session(path) -> Option<ClaudeSession>` – liest Kopf (Zeilen bis
  zum ersten `cwd`/erster User-Nachricht, max. ~256 KB) und Ende (~64 KB)
  der Datei. Titel/letzter Prompt: jeweils der **letzte** Eintrag im Ende;
  fehlt er dort, der aus dem Kopf. Unparsbare Zeilen werden übersprungen.
  Ohne `cwd` → `None`.
- `normalize_path(&str) -> PathBuf` – `/c/Code/x` → `C:\Code\x`;
  Vergleich für die Zuordnung case-insensitiv und mit einheitlichen
  Trennern.
- `assign_to_repos(sessions, repos) -> Vec<RepoSessions>` – längster
  passender Repo-/Worktree-Pfad (Präfix auf Pfadkomponenten, nicht auf
  Zeichen: `C:\Code\repo1` passt nicht auf `C:\Code\repo10`). Sessions ohne
  Treffer oder mit nicht existierendem `cwd` fallen weg. Innerhalb einer
  Gruppe nach `last_activity` absteigend; Gruppen nach ihrer neuesten
  Session absteigend.
- `scan_sessions(projects_dir, repos, cache) -> Vec<RepoSessions>` –
  iteriert `projects/*/*.jsonl`; Cache-Schlüssel (Pfad, mtime, Größe).
- `session_matches_search(group, session, query) -> bool` – alle
  Wörter (whitespace-getrennt) müssen case-insensitiv in Titel, letztem
  Prompt, Repo-Name, Repo-Pfad oder Branch vorkommen (wie
  `repo_matches_search`).
- `resume_profile(claude_profile: Option<&AgentProfile>, id) -> AgentProfile`
  – klont das Claude-Profil (sonst Default `claude`) und hängt
  `--resume <id>` an die Argumente an.

`RepoSessions { repo_path, repo_name, is_worktree, sessions: Vec<ClaudeSession> }`.

### 2. App-Integration (`src/app.rs`, `src/app/merge_requests.rs`)

- `MainView::ClaudeSessions`.
- `view_switcher`: dritter Segment-Button „Claude Sessions“ mit Anzahl (wie
  Repos), Tooltip.
- Zustand in `MyApp`: `claude_sessions: Vec<RepoSessions>`,
  `claude_scanning: bool`, `claude_search: String`, Cache, Kanal
  (`Sender/Receiver`) für das Scan-Ergebnis.
- Scan im Hintergrund-Thread: beim ersten Öffnen der Ansicht, über
  „Aktualisieren“ in der Kopfzeile (in dieser Ansicht) und nach jedem
  abgeschlossenen Repo-Scan, sofern bereits einmal gescannt wurde.
- Sortier-Dropdown nur in der Repo-Ansicht (wie bisher bei MR).
- Statusleiste: „N Sessions in M Repos“ bzw. „Suche Claude Sessions…“.

### 3. UI `src/ui/claude_sessions.rs`

- Werkzeugleiste: Suchfeld (Strg+F fokussiert, Lösch-Button), gleicher
  Stil wie `show_list_toolbar` der Repo-Liste.
- Abschnitte „Repositories (n)“ / „Worktrees (n)“ als `CollapsingHeader`
  wie in der Repo-Liste; darin pro Repo ein einklappbarer Untereintrag
  „Name (Anzahl)“, darin Session-Karten.
- Karte (Frame wie Repo-Karte): Titel fett; darunter letzter Prompt
  einzeilig gekürzt (schwach); Fußzeile Branch-Icon + Branch · relative Zeit.
  Rechts Button „Fortsetzen“ mit Claude-Icon. Klick auf Karte oder Button
  → Aktion `resume: Option<(PathBuf, String)>`.
- Tooltip auf Karte: vollständiger Ordnerpfad und Session-ID.
- Leere Zustände: „Keine Claude Sessions gefunden“ (+ Hinweis auf
  `.claude`-Ordner) bzw. „Keine Treffer für „…““; während des ersten
  Scans Spinner.
- Alle Texte DE/EN in `i18n.rs`.

### 4. Fortsetzen

`resume_profile(...)` + vorhandenes `git::launch_agent(profile, cwd,
terminal_pref)` im Thread (wie `open_agent`), Statusmeldung „Starte Claude
Code in …“, Fehler über `launch_err_tx` ins Fehlerbanner.

## Fehlerbehandlung

- Kein `.claude`-Ordner → leere Liste mit Hinweis, kein Fehler.
- Nicht lesbare/kaputte Dateien und Zeilen → still überspringen.
- Start-Fehler → Fehlerbanner wie bei Agenten.

## Tests

Unit-Tests (mit `tempfile`, synthetische JSONL-Dateien):
- Titel-Priorität custom > ai > erste User-Nachricht > id; letzter Eintrag gewinnt.
- `cwd` in beiden Schreibweisen; Datei ohne `cwd` → `None`; kaputte Zeilen.
- Große Datei: Titel nur im Ende, `cwd` nur im Kopf.
- `normalize_path`, komponentenweiser Präfix (`repo1` vs. `repo10`),
  längster Treffer (Worktree in Repo-Unterordner), Groß-/Kleinschreibung.
- Verwaiste/fremde Sessions werden ausgeblendet; Sortierung.
- Subagent-Unterordner werden ignoriert.
- Suche (mehrere Wörter, case-insensitiv, alle Felder).
- `resume_profile` hängt `--resume <id>` an, übernimmt vorhandene Argumente.

Abschluss: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`.

## Nicht im Umfang

Volltextsuche, WSL-Sessions, Löschen/Umbenennen von Sessions, Anzeige
verwaister Sessions.
