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

---

## v2 (2026-11-09): flache Liste, offene Sessions mit Live-Status

Ersetzt die Gruppierung nach Repo/Worktree.

| Thema | Entscheidung |
|---|---|
| Liste | Eine Liste, nach letzter Änderung sortiert (neueste zuerst); Repo-/Worktree-Name auf der Karte |
| Offene Sessions | Eigener Abschnitt „Offen (n)“ ganz oben, Rahmen in Statusfarbe, danach Trenner; auch außerhalb gescannter Repos |
| Sortierung offen | wartet auf dich → arbeitet → fertig, darin neuester Statuswechsel zuerst |
| Klick auf offene Session | Terminal-Fenster in den Vordergrund (bei Windows Terminal: Fenster, nicht zwingend der Tab) |
| Animationen | Arbeitet: pulsierender, rotierender Stern (gezeichnet) + schimmerndes Verb (eigene Liste, App-Sprache, Wechsel alle 3 s) + Laufzeit; Befehl: „Führt Befehl aus…“; Wartet: blau pulsierender Punkt; Fertig: grüner Haken |
| Umschalter | „● n“ in Blau, wenn n Sessions warten, sonst Gesamtzahl |

### Erkennung laufender Instanzen

- `<Claude-Ordner>/sessions/<pid>.json`: `pid`, `sessionId`, `cwd`, `kind`,
  `status` (`busy`, `shell`, `waiting`, `idle`), `statusUpdatedAt`, `name`,
  `procStart` (Windows: FILETIME der Prozess-Erzeugung).
- Nur `kind = interactive` (oder fehlend). Unbekannter Status → „arbeitet“,
  fehlender Status → „fertig“.
- Verwaiste Dateien: Prozess muss leben (`OpenProcess`/`GetExitCodeProcess`)
  und seine Erzeugungszeit ±1 s zu `procStart` passen.
- Mehrere Dateien zur selben Session: neuester Statuswechsel gewinnt.
- Abfrage im Hintergrund: sichtbar jede Sekunde, sonst alle 5 s (erst nach
  erstem Öffnen der Ansicht). Ändert sich die Menge offener Sessions, wird
  (höchstens alle 5 s) neu gescannt.

### Fenster-Fokus (Windows)

1. Konsole der Instanz: `AttachConsole(pid)` → `GetConsoleWindow()` →
   `FreeConsole()` (nur ohne eigene Konsole, also im Release-Build). Bei
   Windows Terminal ist das ein verstecktes Pseudo-Fenster; dessen Besitzer
   (`GetAncestor(GA_ROOTOWNER)`) ist das Terminal-Fenster mit diesem Tab. Bei
   conhost ist es das sichtbare Konsolenfenster selbst.
2. Rückfall: Elternkette von `claude.exe` (Toolhelp-Snapshot); Treffer nur,
   wenn der erste Prozess der Kette mit Fenstern genau ein sichtbares
   Hauptfenster hat (mehrere Windows-Terminal-Fenster im selben Prozess wären
   geraten). Nicht weiter nach oben (Explorer, GitManager).

`ShowWindow(SW_RESTORE)` falls minimiert, dann `SetForegroundWindow` aus dem
UI-Thread. Misserfolg → Statusmeldung „Terminal-Fenster nicht gefunden“.
Den Tab innerhalb eines Windows-Terminal-Fensters kann man von außen nicht
wählen.

### Offene Karten

Alle Karten volle Fensterbreite. Rechts oben wie in der Repo-Liste: IDEs des
Profils (Repo der Session, sonst ihr Ordner; IDE öffnet das Repo), Shell und
Explorer im Ordner der Session (Profil-Schalter `show_shell`/`show_explorer`).

## v4: Neue Claude Session

- Button „Neue Claude Session“ in der Werkzeugleiste der Repo-Liste neben „Neuer Worktree“.
- Dialog mit Suche und Liste aller gefundenen Repos und Worktrees (Name, Branch bzw. „Worktree von …“, Pfad); Pfeiltasten, Enter, Doppelklick oder „Starten“ starten, Esc bricht ab.
- Vorauswahl: Standard-Ordner aus den Einstellungen (`claude_default_dir`), sonst das zuletzt geöffnete Repo, sonst der erste Eintrag. Liegt der Standard-Ordner außerhalb der Repos, steht er als eigener Eintrag vorne.
- Start mit dem Claude-Agent-Profil ohne `--resume` (Terminal und Befehl aus den Agent-Einstellungen); zählt als Nutzung des Repos.
- Einstellung im Tab „Allgemein“ unter dem Worktree-Ordner: Textfeld, „Durchsuchen …“, „Zurücksetzen“; nur absolute Pfade gelten.
