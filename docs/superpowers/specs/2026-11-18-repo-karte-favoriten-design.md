# Repo-Karte, Pull, Aufräumen, Favoriten – Design

Datum: 2026-11-18 · Branch: `feature/sessions-worktrees-mr-notifications`

## Ziel

Die Repo-Liste soll optisch zur MR-Ansicht aufschließen und im Git-Alltag
mehr abnehmen:

1. **Neue Repo-Karte** mit Status-Chips (Dirty-Indikator, ↑ ungepusht,
   ↓ behind, MR-Chip, Claude-Status), letztem Commit und `⋯`-Menü.
2. **Pull per Klick** (nur Fast-Forward) über den ↓-Chip.
3. **Worktree entfernen** und Dialog **„Aufräumen …“**.
4. **Favoriten** für Repos/Worktrees und Claude Sessions.

## Entscheidungen (mit dem Nutzer abgestimmt)

| Thema | Entscheidung |
|---|---|
| Favoriten-Darstellung | Oben im jeweiligen Abschnitt angepinnt (kein eigener Abschnitt) |
| Pull | Nur Fast-Forward; bei Änderungen/divergiert Hinweis, keine Änderung |
| Worktree entfernen | Streng: sauber → Bestätigung, sonst Warndialog „Trotzdem entfernen“; Branch optional löschen (nur wenn gemerged/gepusht) |
| Aufräumen-Kandidaten | Worktrees mit gemergtem MR, Worktrees mit Upstream „gone“, lokale Branches mit Upstream „gone“ |
| IDE-Icons | Nur Standard-IDE auf der Karte, übrige IDEs im `⋯`-Menü |

## 1. Repo-Karte

```
┌────────────────────────────────────────────────────────────────────────────┐
│ ☆ shop-backend  worktree von shop   ⎇ feature/checkout ▾   ShopBackend.sln ▾│
│   ● 3 geändert   ↑2   ↓5 ⇩   !128 · 1/2 ✓ · ✗ Pipeline   ✦ Claude wartet  │
│   „Fix rounding in totals“ · Tim · vor 2 h          📁 >_ [VS] [✱]  ⋯     │
└────────────────────────────────────────────────────────────────────────────┘
```

### Zeile 1
Stern (☆/★, Klick schaltet Favorit), Name (Tooltip: voller Pfad),
„Worktree von {repo}“ bei verknüpften Worktrees, Branch-Dropdown (unverändert),
Solution-Dropdown (unverändert), Config-Dropdowns (unverändert). Das
Profil-Override-Dropdown entfällt hier und wandert ins `⋯`-Menü.

### Zeile 2: Status-Chips
Chips im Stil der MR-Ansicht (abgerundet, Icon + kurzer Text, Tooltip):

- **Dirty-Chip** (immer sichtbar), abgeleitet aus `RepoInfo.state`:
  - `Clean` → grün `✓ sauber`
  - `Dirty` → orange `● {n} geändert` (`dirty_files`)
  - `Conflict` → rot `⚠ Konflikt`
  - `InProgress` (Merge/Rebase/Cherry-Pick ohne Konflikt) → rot `Merge läuft` bzw. `Rebase läuft`
- **↑n** (nur `ahead > 0`): „{n} ungepushte Commit(s)“.
- **↓n ⇩** (nur `behind > 0`): Tooltip „{n} Commit(s) hinterher – Klick: Fast-Forward“.
  Klick löst den Pull aus (Abschnitt 2). Während des Pulls Spinner statt ⇩.
  Ersetzt das bisherige Warn-Icon neben dem Branch.
- **MR-Chip** (nur wenn ein geladener offener MR auf diesem Ordner
  ausgecheckt ist): `!{iid}`, Approvals `{n}/{m} ✓`, Pipeline-Icon in
  Statusfarbe; Draft grau. Tooltip: MR-Titel. Klick: wechselt in die
  MR-Ansicht und hebt den MR hervor (scrollt hin, Rahmen wie „In Repo-Liste
  zeigen“).
- **Claude-Chip** (nur wenn eine offene Claude Session mit
  `OpenSession.repo_path == repo.path` läuft): `✦ Claude wartet` / `✦ Claude arbeitet` aus dem
  vorhandenen Live-Status. Klick: fokussiert das Terminal (wie die offene
  Session-Karte).

### Zeile 3
Links der letzte Commit: Betreff (eine Zeile, gekürzt mit …) · Autor ·
relative Zeit (`gitlab::time::relative_time`). Ohne Commit (leeres Repo): nichts.

Rechts: Explorer, Shell (je nach Profil), **eine** IDE, aktive Agents, `⋯`.
Die sichtbare IDE ist `RepoUiState.selected_ide`, falls gesetzt und im Profil
sichtbar, sonst `default_ide_id` des Profils, sonst die erste sichtbare IDE.
Ein Klick auf eine IDE im `⋯`-Menü setzt `selected_ide` (die Karte zeigt
danach diese IDE).

### `⋯`-Menü
- „Öffnen in“: alle übrigen sichtbaren IDEs des Profils
- Profil: — Global — / Profile (wie bisheriges Override-Dropdown)
- Neuer Worktree … (nur Haupt-Klone; ersetzt den `+`-Button der Karte)
- Pfad kopieren · Branch-Namen kopieren
- Remote im Browser öffnen (nur wenn `origin` eine http(s)-/SSH-URL eines
  bekannten Hosts hat; SSH-URLs werden in `https://host/pfad` umgeformt)
- Worktree entfernen … (nur verknüpfte Worktrees, Abschnitt 3)

### Datenmodell
`RepoInfo` bekommt (Scan-only, im bestehenden `get_repo_info` per git2):

```rust
pub struct LastCommit { pub summary: String, pub author: String, pub time: i64 }
pub enum RepoState { Clean, Dirty, Conflict, InProgress(InProgressKind) }
pub enum InProgressKind { Merge, Rebase, CherryPick, Revert, Other }

pub last_commit: Option<LastCommit>,
pub state: RepoState,
```

`dirty` bleibt erhalten (wird von vielen Stellen genutzt) und entspricht
`state != Clean`.

### MR-Zuordnung
Nach jedem MR-Laden (und nach jedem Repo-Scan) baut die App eine Map
`repo_mrs: HashMap<PathBuf, MrBadge>` aus allen geladenen offenen MRs
(meine + von anderen inkl. „Alle offenen“, soweit geladen) über die
vorhandene `mr_checked_out_at`. `MrBadge` enthält Host-Key, MR-ID, iid,
Titel, Draft, Approvals (n/m), Pipeline-Status. Bei mehreren MRs auf
demselben Ordner gewinnt der eigene, dann der zuletzt aktualisierte. Ohne
glab/MRs bleibt die Map leer – kein Chip, kein Fehler.

## 2. Pull per Klick (Fast-Forward)

Neue Funktion in `git.rs`:

```rust
pub enum FfOutcome { UpToDate, FastForwarded { commits: usize },
                     Dirty { files: usize }, Diverged { ahead: usize, behind: usize },
                     NoUpstream, Detached }
pub fn fast_forward(path: &Path) -> anyhow::Result<FfOutcome>
```

Ablauf: Upstream des aktuellen Branches bestimmen → Remote-Branch fetchen
(vorhandene Fetch-Logik inkl. Fallback auf System-`git`) → Ahead/Behind neu
berechnen → nur bei sauberem Arbeitsverzeichnis, `ahead == 0`,
`behind > 0` den Branch-Ref auf den Upstream setzen und Checkout (safe).
Sonst entsprechendes `FfOutcome`, nichts wird verändert.

Läuft im Hintergrund-Thread wie Fetch; Ergebnis als Status-/Toast-Meldung
(„{n} Commits übernommen“, „{n} lokale Änderungen – erst committen oder
stashen“, „Branch ist divergiert (↑a ↓b) – bitte manuell lösen“). Danach
Rescan dieses Repos. Tray-Popup: nicht in diesem Schritt.

## 3. Worktree entfernen und Aufräumen

### Entfernen (Einzel-Worktree)
Dialog aus `⋯ → Worktree entfernen …`:

- Sauber und `ahead == 0`: „Worktree {pfad} entfernen?“ + Checkbox
  „Lokalen Branch {branch} löschen“ → `Entfernen` / `Abbrechen`.
- Sonst Warnung („{n} ungespeicherte Änderungen“ / „{n} ungepushte
  Commits“) und Button `Trotzdem entfernen` (rot).
- Checkbox „Branch löschen“ ist nur aktiv, wenn der Branch sicher ist:
  vollständig im Upstream enthalten (`ahead == 0` mit Upstream) **oder** in
  den Default-Branch von `origin` gemerged **oder** sein MR ist gemerged. Sonst
  ausgegraut mit Tooltip-Grund.
- Ausführung über System-`git`: `git -C <hauptrepo> worktree remove [--force] <pfad>`,
  danach `git worktree prune`, optional `git branch -d <branch>` (bei
  gemergtem MR, aber nicht lokal gemergtem Branch `-D`, weil der Merge auf
  GitLab-Seite belegt ist). Fehler werden als Meldung angezeigt; ein
  halb entfernter Worktree wird beim nächsten Prune bereinigt.
- Danach Rescan; offene Claude Sessions in dem Ordner: Warnhinweis im
  Dialog („Eine Claude Session läuft in diesem Ordner“), keine Sperre.

### „Aufräumen …“
Button in der Listen-Werkzeugleiste. Ablauf im Hintergrund:

1. `fetch --prune` für alle Fetch-Ziele (vorhandene `fetch_targets`).
2. Kandidaten sammeln:
   - **Worktree, Upstream gone:** verknüpfter Worktree, dessen Branch einen
     Upstream konfiguriert hat, der nicht mehr existiert.
   - **Worktree, MR gemerged:** verknüpfter Worktree, dessen Branch (oder
     `mr-<iid>`) Quell-Branch eines gemergten MR des Projekts ist.
     Abfrage per GraphQL je Projekt:
     `project(fullPath) { mergeRequests(state: merged, sourceBranches: [...]) { nodes { iid sourceBranch mergedAt } } }`,
     nur für die Branches der Worktree-Kandidaten. Ohne glab/Host entfällt
     dieser Grund still.
   - **Lokaler Branch, Upstream gone:** in Haupt-Klonen, nicht in einem
     Worktree ausgecheckt und nicht der aktuelle Branch.
3. Dialog: Liste mit Checkbox, Name/Pfad, Grund(en). Dirty oder ungepushte
   Kandidaten sind vorab abgewählt und orange markiert („{n} Änderungen“,
   „↑{n} ungepusht“). Worktree-Einträge haben zusätzlich die Checkbox
   „Branch mitlöschen“ (Regeln wie oben). Button `{n} entfernen`.
4. Ausführung nacheinander im Hintergrund, Ergebnisliste (ok / Fehler je
   Eintrag), danach Rescan.

Keine Kandidaten: Dialog zeigt „Nichts aufzuräumen“.

## 4. Favoriten

### Config
```rust
#[serde(default)] pub favorite_repos: Vec<String>,     // repo_state_key(path)
#[serde(default)] pub favorite_sessions: Vec<String>,  // Session-IDs
```
Ältere Versionen ignorieren die Felder (wie `folder_files`), kein
Versionssprung. Toggle-Hilfen `toggle_favorite_repo(path)` /
`toggle_favorite_session(id)`; Speichern wie bei anderen Repo-State-Änderungen.

### Repo-Liste
☆/★ ganz links auf der Karte. Sortierung in `sort_main_repos` /
`split_visible`: zuerst Favoriten (untereinander nach aktueller Sortierung),
dann der Rest – je Abschnitt (Repositories / Worktrees). Suche filtert
Favoriten wie alle anderen Karten.

### Claude Sessions
☆/★ auf jeder Session-Karte (nicht auf offenen Sessions, die stehen ohnehin
oben; ist eine offene Session Favorit, zeigt ihre Karte trotzdem ★). Die
Liste der nicht offenen Sessions sortiert Favoriten zuerst, dann nach
`last_activity`. Favoriten werden nie durch eine Mengen-/Altersgrenze
ausgeblendet. Session-IDs, die nicht mehr existieren, bleiben in der Config
(harmlos) und werden beim Speichern nicht aktiv bereinigt.

## Fehlerbehandlung

- Scan-Fehler beim letzten Commit/State: Feld `None` bzw. `Dirty` als
  vorsichtiger Default, kein Abbruch des Scans.
- Pull, Entfernen, Aufräumen: Fehlertexte (übersetzt) in Status-/Toast-Meldung;
  nie automatischer Reset oder Force ohne ausdrückliche Bestätigung.
- MR-/GraphQL-Fehler beim Aufräumen: Grund „MR gemerged“ entfällt, Hinweis
  im Dialog („GitLab nicht erreichbar – nur Upstream-Prüfung“).

## i18n

Alle neuen Texte in `i18n.rs` für Deutsch und Englisch.

## Tests

- `git.rs` (Temp-Repos): `fast_forward` für FF, UpToDate, Dirty, Diverged,
  NoUpstream, Detached; `RepoState`-Ableitung (Clean, Dirty, Conflict,
  Merge/Rebase); `last_commit`; Worktree entfernen (sauber, dirty mit
  Force, mit/ohne Branch-Löschung); Erkennung „Upstream gone“.
- Aufräumen: Kandidatenermittlung aus Repos + Mock-Antwort für gemergte MRs;
  Vorab-Abwahl bei dirty/ungepusht.
- GraphQL: Query-Text für `sourceBranches` (Muster wie bestehende Tests).
- MR-Zuordnung: `repo_mrs`-Aufbau, Priorität bei mehreren MRs.
- Favoriten: Sortierung Repos (beide Modi) und Sessions; Config-Roundtrip
  und Laden alter Config ohne Felder.
- UI (egui-Tests wie in `repo_list.rs`): Stern-Toggle setzt Aktion, ↓-Chip
  löst Pull-Aktion aus, `⋯`-Menü enthält „Worktree entfernen“ nur bei
  Worktrees, nur eine IDE sichtbar.
- Demo: `fake-glab.py` beantwortet die Merged-Abfrage; README-Screenshots
  und Feature-Tabelle aktualisieren.

## Nicht im Umfang

Pull im Tray-Popup, Rebase/Stash beim Pull, Command Palette, Kompaktmodus,
Gruppen/Ordner, Filter-Chips über der Liste.
