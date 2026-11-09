//! Laufende Claude-Code-Instanzen.
//!
//! Jede interaktive Instanz schreibt `<Claude-Ordner>/sessions/<pid>.json`
//! mit Session-ID, Ordner und Status (`busy`, `shell`, `waiting`, `idle`).
//! Nach einem Absturz bleibt die Datei liegen, deshalb zählt sie nur, wenn
//! der Prozess mit dieser PID und Startzeit noch läuft.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::claude_sessions::{claude_config_dir, display_cwd, matches_words, SessionScan};

/// Zustand einer laufenden Instanz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiveStatus {
    /// Wartet auf Berechtigung oder Rückfrage
    Waiting,
    /// Arbeitet
    Busy,
    /// Führt einen Shell-Befehl aus
    Shell,
    /// Fertig, wartet auf den nächsten Prompt
    Idle,
}

impl LiveStatus {
    /// Unbekannte Werte (neuere Claude-Versionen) gelten als „arbeitet“.
    pub fn from_raw(s: &str) -> Self {
        match s {
            "waiting" => Self::Waiting,
            "shell" => Self::Shell,
            "idle" => Self::Idle,
            _ => Self::Busy,
        }
    }

    /// Reihenfolge in der Liste: wartet, arbeitet, fertig
    pub fn rank(self) -> u8 {
        match self {
            Self::Waiting => 0,
            Self::Busy | Self::Shell => 1,
            Self::Idle => 2,
        }
    }

    /// Animiert dargestellt (alles außer „fertig“)
    pub fn animated(self) -> bool {
        self != Self::Idle
    }
}

/// Eine laufende Claude-Code-Instanz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSession {
    pub pid: u32,
    pub session_id: String,
    /// Arbeitsordner (Git-Bash-Form als Windows-Pfad)
    pub cwd: String,
    pub status: LiveStatus,
    /// Letzter Statuswechsel (Unix-Millisekunden)
    pub status_since_ms: i64,
    /// Anzeigename der Instanz (`name`)
    pub name: Option<String>,
    /// Prozess-Startzeit wie von Claude notiert (Windows: FILETIME)
    pub proc_start: Option<String>,
}

/// `<Claude-Ordner>/sessions`.
pub fn claude_live_dir() -> Option<PathBuf> {
    Some(claude_config_dir()?.join("sessions"))
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Eine `<pid>.json`. `None` bei fehlenden Pflichtfeldern oder für
/// Hintergrund-Instanzen (`kind` ≠ `interactive`).
pub fn parse_live(text: &str) -> Option<LiveSession> {
    let v: Value = serde_json::from_str(text).ok()?;
    if let Some(kind) = v.get("kind").and_then(Value::as_str) {
        if kind != "interactive" {
            return None;
        }
    }
    let pid = u32::try_from(v.get("pid")?.as_u64()?).ok()?;
    let status = v
        .get("status")
        .and_then(Value::as_str)
        .map_or(LiveStatus::Idle, LiveStatus::from_raw);
    let since = ["statusUpdatedAt", "updatedAt", "startedAt"]
        .iter()
        .find_map(|k| v.get(*k).and_then(Value::as_i64))
        .unwrap_or(0);
    Some(LiveSession {
        pid,
        session_id: str_field(&v, "sessionId")?,
        cwd: display_cwd(&str_field(&v, "cwd")?),
        status,
        status_since_ms: since,
        name: str_field(&v, "name"),
        proc_start: str_field(&v, "procStart"),
    })
}

/// Wartet zuerst, dann arbeitet, dann fertig; darin neuester Wechsel zuerst.
pub fn sort_live(sessions: &mut [LiveSession]) {
    sessions.sort_by_key(|s| (s.status.rank(), std::cmp::Reverse(s.status_since_ms)));
}

/// Liest alle `*.json` in `dir`, behält nur lebende Prozesse (`alive(pid,
/// procStart)`), je Session die neueste Instanz, sortiert.
pub fn read_live_sessions(
    dir: &Path,
    alive: impl Fn(u32, Option<&str>) -> bool,
) -> Vec<LiveSession> {
    let mut by_id: HashMap<String, LiveSession> = HashMap::new();
    let files = std::fs::read_dir(dir).into_iter().flatten().flatten();
    for file in files {
        let path = file.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(s) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| parse_live(&t))
        else {
            continue;
        };
        if !alive(s.pid, s.proc_start.as_deref()) {
            continue;
        }
        match by_id.get(&s.session_id) {
            Some(old) if old.status_since_ms >= s.status_since_ms => {}
            _ => {
                by_id.insert(s.session_id.clone(), s);
            }
        }
    }
    let mut live: Vec<LiveSession> = by_id.into_values().collect();
    sort_live(&mut live);
    live
}

/// Notierte Startzeit (FILETIME, 100 ns) passt auf ±1 s zur echten; fehlt
/// sie oder ist unlesbar, zählt nur, dass der Prozess lebt.
#[cfg_attr(not(windows), allow(dead_code))] // nur Windows (und Tests)
pub fn proc_start_matches(recorded: Option<&str>, actual_filetime: u64) -> bool {
    match recorded.and_then(|s| s.parse::<u64>().ok()) {
        Some(start) => start.abs_diff(actual_filetime) <= 10_000_000,
        None => true,
    }
}

/// Läuft der Prozess `pid` (mit dieser Startzeit) noch?
#[cfg(windows)]
pub fn process_alive(pid: u32, proc_start: Option<&str>) -> bool {
    win::process_alive(pid, proc_start)
}

/// Außerhalb von Windows (Tests, Entwicklung): `/proc/<pid>` existiert.
#[cfg(not(windows))]
pub fn process_alive(pid: u32, _proc_start: Option<&str>) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// `pid` und seine Eltern bis zur Wurzel (Zyklen werden abgebrochen).
#[cfg_attr(not(windows), allow(dead_code))] // nur Windows (und Tests)
pub fn parent_chain(pid: u32, parents: &HashMap<u32, u32>) -> Vec<u32> {
    let mut chain = vec![pid];
    let mut seen = HashSet::from([pid]);
    let mut current = pid;
    while let Some(&parent) = parents.get(&current) {
        if parent == 0 || !seen.insert(parent) || chain.len() >= 16 {
            break;
        }
        chain.push(parent);
        current = parent;
    }
    chain
}

/// Fenster des ersten Prozesses der Kette, der überhaupt Fenster hat – nur
/// wenn es genau eines ist. Mehrere (z. B. zwei Windows-Terminal-Fenster im
/// selben Prozess) wären geraten: dann `None`, und es wird auch nicht weiter
/// oben (Explorer, GitManager) gesucht.
/// `windows`: (Prozess-ID, Fenster-Handle) sichtbarer Hauptfenster.
#[cfg_attr(not(windows), allow(dead_code))] // nur Windows (und Tests)
pub fn pick_window(chain: &[u32], windows: &[(u32, isize)]) -> Option<isize> {
    let pid = chain
        .iter()
        .find(|pid| windows.iter().any(|(p, _)| p == *pid))?;
    let mut of_pid = windows.iter().filter(|(p, _)| p == pid);
    let (_, hwnd) = of_pid.next()?;
    of_pid.next().is_none().then_some(*hwnd)
}

/// Fenster zum Konsolenfenster der Instanz: bei Windows Terminal ist es ein
/// verstecktes Pseudo-Fenster, dessen Besitzer (`root_owner`) das Terminal-
/// Fenster mit diesem Tab ist; bei conhost das sichtbare Fenster selbst.
#[cfg_attr(not(windows), allow(dead_code))] // nur Windows (und Tests)
pub fn console_target(
    console: isize,
    console_visible: bool,
    root_owner: Option<isize>,
) -> Option<isize> {
    match root_owner {
        Some(owner) if owner != console => Some(owner),
        _ => console_visible.then_some(console),
    }
}

/// Holt das Terminal-Fenster der Instanz `pid` in den Vordergrund.
#[cfg(windows)]
pub fn focus_session_window(pid: u32) -> bool {
    win::focus(pid)
}

#[cfg(not(windows))]
pub fn focus_session_window(_pid: u32) -> bool {
    false
}

/// Offene Session für die Anzeige.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSession {
    pub live: LiveSession,
    /// Session-Titel, sonst Instanzname, sonst Ordnername
    pub title: String,
    /// Repo-/Worktree-Name, sonst Ordnername
    pub place: String,
    /// Repo/Worktree der Session, falls sie in einem gescannten liegt
    pub repo_path: Option<PathBuf>,
    pub branch: Option<String>,
    pub last_prompt: Option<String>,
}

fn folder_name(cwd: &str) -> String {
    cwd.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(cwd)
        .to_string()
}

/// Verbindet laufende Instanzen mit den eingelesenen Sessions (Reihenfolge
/// von `live` bleibt).
pub fn build_open(live: &[LiveSession], scan: &SessionScan) -> Vec<OpenSession> {
    live.iter()
        .map(|l| {
            let session = scan.all.get(&l.session_id);
            let entry = scan.entries.iter().find(|e| e.session.id == l.session_id);
            let folder = folder_name(&l.cwd);
            OpenSession {
                title: session
                    .map(|s| s.title.clone())
                    .or_else(|| l.name.clone())
                    .unwrap_or_else(|| folder.clone()),
                place: entry.map_or(folder, |e| e.repo_name.clone()),
                repo_path: entry.map(|e| e.repo_path.clone()),
                branch: session.and_then(|s| s.git_branch.clone()),
                last_prompt: session.and_then(|s| s.last_prompt.clone()),
                live: l.clone(),
            }
        })
        .collect()
}

/// Suche über Titel, Ort, Ordner, Branch, letzten Prompt und Instanzname.
pub fn open_matches_search(o: &OpenSession, query: &str) -> bool {
    matches_words(
        &[
            &o.title,
            &o.place,
            &o.live.cwd,
            o.branch.as_deref().unwrap_or(""),
            o.last_prompt.as_deref().unwrap_or(""),
            o.live.name.as_deref().unwrap_or(""),
        ],
        query,
    )
}

#[cfg(windows)]
mod win {
    use super::{console_target, parent_chain, pick_window, proc_start_matches};
    use std::collections::HashMap;
    use windows_sys::Win32::Foundation::{
        CloseHandle, BOOL, FILETIME, HWND, INVALID_HANDLE_VALUE, LPARAM, STILL_ACTIVE,
    };
    use windows_sys::Win32::System::Console::{AttachConsole, FreeConsole, GetConsoleWindow};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetAncestor, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, GA_ROOTOWNER, GW_OWNER,
        SW_RESTORE,
    };

    pub(super) fn process_alive(pid: u32, proc_start: Option<&str>) -> bool {
        // SAFETY: Handle wird geprüft und immer geschlossen; Ausgabeparameter
        // sind lokale, initialisierte Werte.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut code = 0u32;
            let running = GetExitCodeProcess(handle, &mut code) != 0 && code == STILL_ACTIVE as u32;
            let zero = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
            let have_times =
                GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
            CloseHandle(handle);
            if !running {
                return false;
            }
            if !have_times {
                return true;
            }
            let created =
                (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
            proc_start_matches(proc_start, created)
        }
    }

    fn parent_map() -> HashMap<u32, u32> {
        let mut parents = HashMap::new();
        // SAFETY: Snapshot-Handle wird geprüft und geschlossen; `dwSize` ist
        // wie verlangt gesetzt.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return parents;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut entry);
            while ok != 0 {
                parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                ok = Process32NextW(snap, &mut entry);
            }
            CloseHandle(snap);
        }
        parents
    }

    unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` zeigt auf den Vec aus `top_windows`, der während
        // `EnumWindows` lebt.
        let windows = &mut *(lparam as *mut Vec<(u32, isize)>);
        let visible = IsWindowVisible(hwnd) != 0;
        let unowned = GetWindow(hwnd, GW_OWNER).is_null();
        if visible && unowned && GetWindowTextLengthW(hwnd) > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            windows.push((pid, hwnd as isize));
        }
        1
    }

    fn top_windows() -> Vec<(u32, isize)> {
        let mut windows: Vec<(u32, isize)> = Vec::new();
        // SAFETY: Callback schreibt nur in `windows`, das länger lebt als der
        // Aufruf.
        unsafe {
            EnumWindows(Some(collect_window), &mut windows as *mut _ as LPARAM);
        }
        windows
    }

    /// Fenster über die Konsole der Instanz: kurz anhängen, Konsolenfenster
    /// holen, wieder lösen (siehe `console_target`).
    fn console_window_of(pid: u32) -> Option<isize> {
        // SAFETY: Nur wenn dieser Prozess keine eigene Konsole hat (Release-
        // Build); die Konsole wird sofort wieder gelöst.
        unsafe {
            if !GetConsoleWindow().is_null() || AttachConsole(pid) == 0 {
                return None;
            }
            let hwnd = GetConsoleWindow();
            FreeConsole();
            if hwnd.is_null() {
                return None;
            }
            let owner = GetAncestor(hwnd, GA_ROOTOWNER);
            let owner = (!owner.is_null() && IsWindowVisible(owner) != 0).then_some(owner as isize);
            console_target(hwnd as isize, IsWindowVisible(hwnd) != 0, owner)
        }
    }

    pub(super) fn focus(pid: u32) -> bool {
        let found = console_window_of(pid)
            .or_else(|| pick_window(&parent_chain(pid, &parent_map()), &top_windows()));
        let Some(hwnd) = found else {
            return false;
        };
        let hwnd = hwnd as HWND;
        // SAFETY: `hwnd` stammt aus EnumWindows/GetConsoleWindow.
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd) != 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_sessions::{ClaudeSession, SessionEntry, SessionScan};
    use serde_json::json;
    use std::path::PathBuf;

    fn raw(pid: u32, id: &str, status: &str, since: i64) -> String {
        json!({"pid":pid,"sessionId":id,"cwd":"C:\\Code\\repo2","kind":"interactive",
            "status":status,"statusUpdatedAt":since,"updatedAt":since,
            "name":"repo2-a3","procStart":"134387037640181301"})
        .to_string()
    }

    #[test]
    fn status_from_raw_maps_known_and_unknown() {
        assert_eq!(LiveStatus::from_raw("busy"), LiveStatus::Busy);
        assert_eq!(LiveStatus::from_raw("shell"), LiveStatus::Shell);
        assert_eq!(LiveStatus::from_raw("waiting"), LiveStatus::Waiting);
        assert_eq!(LiveStatus::from_raw("idle"), LiveStatus::Idle);
        assert_eq!(LiveStatus::from_raw("thinking-hard"), LiveStatus::Busy);
    }

    #[test]
    fn parse_live_reads_fields() {
        let s = parse_live(&raw(14488, "ddc7", "waiting", 1_794_236_131_611)).unwrap();
        assert_eq!(s.pid, 14488);
        assert_eq!(s.session_id, "ddc7");
        assert_eq!(s.cwd, "C:\\Code\\repo2");
        assert_eq!(s.status, LiveStatus::Waiting);
        assert_eq!(s.status_since_ms, 1_794_236_131_611);
        assert_eq!(s.name.as_deref(), Some("repo2-a3"));
        assert_eq!(s.proc_start.as_deref(), Some("134387037640181301"));
    }

    #[test]
    fn parse_live_missing_status_is_idle() {
        let text = json!({"pid":1,"sessionId":"a","cwd":"C:\\x","startedAt":5}).to_string();
        let s = parse_live(&text).unwrap();
        assert_eq!(s.status, LiveStatus::Idle);
        assert_eq!(s.status_since_ms, 5);
    }

    #[test]
    fn parse_live_rejects_incomplete_and_background() {
        assert!(parse_live("{kaputt").is_none());
        assert!(parse_live(&json!({"pid":1,"cwd":"C:\\x"}).to_string()).is_none());
        let daemon = json!({"pid":1,"sessionId":"a","cwd":"C:\\x","kind":"daemon"}).to_string();
        assert!(parse_live(&daemon).is_none());
    }

    #[test]
    fn read_live_filters_dead_dedupes_and_sorts() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, text: String| std::fs::write(dir.path().join(name), text).unwrap();
        write("1.json", raw(1, "idle-old", "idle", 100));
        write("2.json", raw(2, "busy", "busy", 200));
        write("3.json", raw(3, "wait", "waiting", 50));
        write("4.json", raw(4, "dead", "busy", 900));
        write("5.json", raw(5, "busy", "busy", 300)); // gleiche Session, neuer
        write("x.key", "geheim".to_string());
        write("6.json", "{kaputt".to_string());

        let live = read_live_sessions(dir.path(), |pid, _| pid != 4);
        let ids: Vec<(&str, u32)> = live
            .iter()
            .map(|s| (s.session_id.as_str(), s.pid))
            .collect();
        assert_eq!(ids, [("wait", 3), ("busy", 5), ("idle-old", 1)]);
    }

    #[test]
    fn read_live_missing_dir_is_empty() {
        let live = read_live_sessions(Path::new("/gibt/es/nicht"), |_, _| true);
        assert!(live.is_empty());
    }

    #[test]
    fn proc_start_matches_with_tolerance() {
        assert!(proc_start_matches(
            Some("134387037640181301"),
            134_387_037_640_181_301
        ));
        assert!(proc_start_matches(
            Some("134387037640181301"),
            134_387_037_645_000_000
        ));
        assert!(!proc_start_matches(
            Some("134387037640181301"),
            134_387_037_700_000_000
        ));
        // Fehlt/kaputt: nur „lebt“ zählt
        assert!(proc_start_matches(None, 1));
        assert!(proc_start_matches(Some("abc"), 1));
    }

    #[test]
    fn pick_window_needs_unique_window_of_first_ancestor_with_windows() {
        // claude.exe(10) → pwsh(20) → WindowsTerminal(30) → explorer(40)
        let chain = [10, 20, 30, 40];
        // WT hat zwei Fenster: nicht eindeutig → kein Treffer (und nicht
        // weiter hoch zum Explorer)
        let ambiguous = [(30, 7), (30, 8), (40, 9)];
        assert_eq!(pick_window(&chain, &ambiguous), None);
        // genau ein Fenster → Treffer
        let unique = [(99, 1), (30, 7), (40, 9)];
        assert_eq!(pick_window(&chain, &unique), Some(7));
        // eigenes Konsolenfenster der Shell zuerst
        let shell = [(20, 5), (30, 7)];
        assert_eq!(pick_window(&chain, &shell), Some(5));
        assert_eq!(pick_window(&[10], &unique), None);
    }

    #[test]
    fn console_target_prefers_terminal_owner_then_visible_console() {
        // Windows Terminal: verstecktes Pseudo-Konsolenfenster, Besitzer = WT-Fenster
        assert_eq!(console_target(11, false, Some(22)), Some(22));
        // klassische Konsole: sichtbar, Besitzer ist sie selbst
        assert_eq!(console_target(11, true, Some(11)), Some(11));
        assert_eq!(console_target(11, true, None), Some(11));
        // weder sichtbar noch Besitzer
        assert_eq!(console_target(11, false, None), None);
        assert_eq!(console_target(11, false, Some(11)), None);
    }

    #[test]
    fn parent_chain_stops_at_root_and_loops() {
        let parents = HashMap::from([(10, 20), (20, 30), (30, 0)]);
        assert_eq!(parent_chain(10, &parents), [10, 20, 30]);
        let looped = HashMap::from([(1, 2), (2, 1)]);
        assert_eq!(parent_chain(1, &looped), [1, 2]);
    }

    fn session(id: &str, cwd: &str, title: &str) -> ClaudeSession {
        ClaudeSession {
            id: id.to_string(),
            cwd: cwd.to_string(),
            title: title.to_string(),
            last_prompt: Some("letzter".to_string()),
            git_branch: Some("main".to_string()),
            last_activity: 1,
        }
    }

    fn live(pid: u32, id: &str, cwd: &str, name: Option<&str>) -> LiveSession {
        LiveSession {
            pid,
            session_id: id.to_string(),
            cwd: cwd.to_string(),
            status: LiveStatus::Busy,
            status_since_ms: 0,
            name: name.map(str::to_string),
            proc_start: None,
        }
    }

    #[test]
    fn build_open_uses_scan_titles_repo_names_and_fallbacks() {
        let in_repo = session("a", "C:\\Code\\repo2", "Titel A");
        let outside = session("b", "C:\\Users\\tw", "Titel B");
        let scan = SessionScan {
            entries: vec![SessionEntry {
                session: in_repo.clone(),
                repo_path: PathBuf::from("C:\\Code\\repo2"),
                repo_name: "repo2".to_string(),
                is_worktree: false,
            }],
            all: HashMap::from([("a".into(), in_repo), ("b".into(), outside)]),
        };
        let open = build_open(
            &[
                live(1, "a", "C:\\Code\\repo2", Some("x")),
                live(2, "b", "C:\\Users\\tw", None),
                live(3, "neu", "C:\\Code\\neu", Some("neu-a1")),
                live(4, "leer", "/c/Code/leer", None),
            ],
            &scan,
        );
        let got: Vec<(&str, &str)> = open
            .iter()
            .map(|o| (o.title.as_str(), o.place.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("Titel A", "repo2"),
                ("Titel B", "tw"),
                ("neu-a1", "neu"),
                ("leer", "leer")
            ]
        );
        assert_eq!(open[0].branch.as_deref(), Some("main"));
        assert_eq!(open[0].repo_path, Some(PathBuf::from("C:\\Code\\repo2")));
        assert_eq!(open[1].repo_path, None);
    }

    #[test]
    fn parse_live_converts_gitbash_cwd() {
        let text = json!({"pid":1,"sessionId":"a","cwd":"/c/Code/leer"}).to_string();
        assert_eq!(parse_live(&text).unwrap().cwd, "C:\\Code\\leer");
    }

    #[test]
    fn open_search_covers_title_place_and_branch() {
        let scan = SessionScan {
            all: HashMap::from([("a".into(), session("a", "C:\\Code\\repo2", "Login fixen"))]),
            ..SessionScan::default()
        };
        let open = build_open(&[live(1, "a", "C:\\Code\\repo2", None)], &scan);
        assert!(open_matches_search(&open[0], "login repo2"));
        assert!(open_matches_search(&open[0], "MAIN"));
        assert!(!open_matches_search(&open[0], "logout"));
    }
}
