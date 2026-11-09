//! Claude-Code-Sessions finden, einlesen und Repos zuordnen.
//!
//! Claude Code legt jede Session als JSONL-Datei unter
//! `<config>/projects/<kodierter-pfad>/<session-id>.jsonl` ab. Der
//! Ordnername ist verlustbehaftet kodiert, maßgeblich ist das Feld `cwd`
//! in den Zeilen.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::{AgentLaunchMode, AgentProfile};
use crate::git::RepoInfo;

/// Bytes vom Dateianfang (bis zum ersten `cwd` und zur ersten Frage)
const HEAD_BYTES: u64 = 256 * 1024;
/// Bytes vom Dateiende (neuester Titel und letzter Prompt)
const TAIL_BYTES: u64 = 64 * 1024;

/// Eine Claude-Code-Session (eine `.jsonl`-Datei).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeSession {
    /// Dateiname ohne `.jsonl` (= Session-ID für `claude --resume`)
    pub id: String,
    /// Arbeitsordner; Git-Bash-Form (`/c/x`) als Windows-Pfad
    pub cwd: String,
    /// `custom-title` > `ai-title` > erste Frage > letzter Prompt
    pub title: String,
    pub last_prompt: Option<String>,
    /// Letzter nicht-leerer `gitBranch`
    pub git_branch: Option<String>,
    /// Änderungszeit der Datei (Unix-Sekunden)
    pub last_activity: i64,
}

/// Vergleichsschlüssel für Pfade unabhängig von Schreibweise:
/// `C:\Code\X\`, `/c/Code/x` und `c:/code/x` → `c:/code/x`.
pub fn path_key(p: &str) -> String {
    let mut s = p.replace('\\', "/").to_lowercase();
    let b = s.as_bytes();
    if b.len() >= 2 && b[0] == b'/' && b[1].is_ascii_alphabetic() && (b.len() == 2 || b[2] == b'/')
    {
        s = format!("{}:{}", b[1] as char, &s[2..]);
    }
    while s.len() > 1 && s.ends_with('/') && !s.ends_with(":/") {
        s.pop();
    }
    s
}

/// Git-Bash-Pfad (`/c/Code/x`) als Windows-Pfad (`C:\Code\x`), sonst
/// unverändert.
pub fn display_cwd(raw: &str) -> String {
    let b = raw.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b'/' {
        let drive = (b[1] as char).to_ascii_uppercase();
        return format!("{drive}:{}", raw[2..].replace('/', "\\"));
    }
    raw.to_string()
}

/// Liest Kopf und Ende einer Session-Datei. `None` ohne `cwd`, ohne
/// Unterhaltung (kein Titel, keine Frage, kein Prompt) oder bei Lesefehlern.
pub fn parse_session(path: &Path) -> Option<ClaudeSession> {
    let id = path.file_stem()?.to_string_lossy().into_owned();
    let mut file = File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    let len = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);

    let mut head = Vec::new();
    (&mut file).take(HEAD_BYTES).read_to_end(&mut head).ok()?;
    let mut tail = Vec::new();
    if len > HEAD_BYTES {
        let n = TAIL_BYTES.min(len - HEAD_BYTES);
        file.seek(SeekFrom::Start(len - n)).ok()?;
        file.take(n).read_to_end(&mut tail).ok()?;
    }
    parse_session_bytes(&id, &head, &tail, mtime)
}

/// Text einer Nutzer-Nachricht (String oder Liste von Text-Blöcken).
fn message_text(v: &Value) -> Option<String> {
    match v.pointer("/message/content")? {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let text: Vec<&str> = parts
                .iter()
                .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect();
            (!text.is_empty()).then(|| text.join(" "))
        }
        _ => None,
    }
}

/// Eine Zeile: Zeilenumbrüche zu Leerzeichen, außen getrimmt.
fn one_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ").trim().to_string()
}

fn non_empty(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(one_line)
        .filter(|s| !s.is_empty())
}

/// Wertet Kopf und Ende aus. `tail` ist leer, wenn der Kopf die ganze
/// Datei enthält; sonst wird seine erste (angeschnittene) Zeile verworfen.
pub(crate) fn parse_session_bytes(
    id: &str,
    head: &[u8],
    tail: &[u8],
    mtime: i64,
) -> Option<ClaudeSession> {
    let head = String::from_utf8_lossy(head);
    let tail = String::from_utf8_lossy(tail);
    let tail_lines = tail.split('\n').skip(1);

    let mut cwd = None;
    let mut branch = None;
    let mut custom_title = None;
    let mut ai_title = None;
    let mut last_prompt = None;
    let mut first_question = None;

    for raw in head.split('\n').chain(tail_lines) {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        if cwd.is_none() {
            cwd = non_empty(&v, "cwd");
        }
        if let Some(b) = non_empty(&v, "gitBranch") {
            branch = Some(b);
        }
        match v.get("type").and_then(Value::as_str) {
            Some("custom-title") => custom_title = non_empty(&v, "customTitle").or(custom_title),
            Some("ai-title") => ai_title = non_empty(&v, "aiTitle").or(ai_title),
            Some("last-prompt") => last_prompt = non_empty(&v, "lastPrompt").or(last_prompt),
            Some("user") if first_question.is_none() => {
                let is_meta = v.get("isMeta").and_then(Value::as_bool) == Some(true);
                if !is_meta {
                    first_question = message_text(&v)
                        .map(|t| one_line(&t))
                        .filter(|t| !t.is_empty() && !t.starts_with('<'));
                }
            }
            _ => {}
        }
    }

    // Ohne Titel, Frage und Prompt (nur Befehle wie `/effort`) gibt es
    // nichts fortzusetzen
    let title = custom_title
        .or(ai_title)
        .or(first_question)
        .or_else(|| last_prompt.clone())?;
    Some(ClaudeSession {
        id: id.to_string(),
        cwd: display_cwd(&cwd?),
        title,
        last_prompt,
        git_branch: branch,
        last_activity: mtime,
    })
}

/// Sessions eines Repos oder Worktrees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSessions {
    pub repo_path: PathBuf,
    pub repo_name: String,
    /// Verknüpfter Worktree (`RepoInfo::is_linked_worktree`)
    pub is_worktree: bool,
    /// Neueste zuerst
    pub sessions: Vec<ClaudeSession>,
}

/// Ergebnisse je Datei, gültig solange Änderungszeit und Größe gleich sind.
#[derive(Debug, Default)]
pub struct SessionCache {
    entries: HashMap<PathBuf, (i64, u64, Option<ClaudeSession>)>,
}

impl SessionCache {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// `<CLAUDE_CONFIG_DIR oder ~/.claude>/projects`, falls vorhanden.
pub fn claude_projects_dir() -> Option<PathBuf> {
    let base = match std::env::var_os("CLAUDE_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => directories::BaseDirs::new()?.home_dir().join(".claude"),
    };
    let projects = base.join("projects");
    projects.is_dir().then_some(projects)
}

/// Ordnet jede Session dem Repo/Worktree mit dem längsten passenden Pfad zu
/// (ganze Pfadkomponenten). Sessions ohne Treffer oder mit fehlendem Ordner
/// (`exists`) fallen weg. Gruppen und Sessions: neueste zuerst.
pub fn assign_to_repos(
    sessions: Vec<ClaudeSession>,
    repos: &[RepoInfo],
    exists: impl Fn(&str) -> bool,
) -> Vec<RepoSessions> {
    let keys: Vec<String> = repos
        .iter()
        .map(|r| path_key(&r.path.to_string_lossy()))
        .collect();
    let mut groups: Vec<Option<RepoSessions>> = vec![None; repos.len()];
    for session in sessions {
        let key = path_key(&session.cwd);
        let best = keys
            .iter()
            .enumerate()
            .filter(|(_, k)| key == **k || key.starts_with(&format!("{k}/")))
            .max_by_key(|(_, k)| k.len())
            .map(|(i, _)| i);
        let Some(i) = best else { continue };
        if !exists(&session.cwd) {
            continue;
        }
        let repo = &repos[i];
        groups[i]
            .get_or_insert_with(|| RepoSessions {
                repo_path: repo.path.clone(),
                repo_name: repo.name.clone(),
                is_worktree: repo.is_linked_worktree,
                sessions: Vec::new(),
            })
            .sessions
            .push(session);
    }
    let mut groups: Vec<RepoSessions> = groups.into_iter().flatten().collect();
    for g in &mut groups {
        g.sessions
            .sort_by_key(|s| std::cmp::Reverse(s.last_activity));
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.sessions[0].last_activity));
    groups
}

/// Liest alle `projects/*/*.jsonl` (Unterordner der Projektordner werden
/// ignoriert) und ordnet sie `repos` zu.
pub fn scan_sessions(
    projects_dir: &Path,
    repos: &[RepoInfo],
    cache: &mut SessionCache,
) -> Vec<RepoSessions> {
    let mut seen = HashSet::new();
    let mut sessions = Vec::new();
    let project_dirs = std::fs::read_dir(projects_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()));
    for project in project_dirs {
        let files = std::fs::read_dir(project.path())
            .into_iter()
            .flatten()
            .flatten();
        for file in files {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let Ok(meta) = file.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            let len = meta.len();
            let parsed = match cache.entries.get(&path) {
                Some((m, l, s)) if *m == mtime && *l == len => s.clone(),
                _ => {
                    let s = parse_session(&path);
                    cache.entries.insert(path.clone(), (mtime, len, s.clone()));
                    s
                }
            };
            seen.insert(path);
            sessions.extend(parsed);
        }
    }
    cache.entries.retain(|p, _| seen.contains(p));
    assign_to_repos(sessions, repos, |p| Path::new(p).exists())
}

/// Alle Wörter von `query` kommen (ohne Groß-/Kleinschreibung) in Titel,
/// letztem Prompt, Repo-Name, Repo-Pfad oder Branch vor.
pub fn session_matches_search(group: &RepoSessions, session: &ClaudeSession, query: &str) -> bool {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return true;
    }
    let haystack = format!(
        "{}\n{}\n{}\n{}\n{}",
        session.title,
        session.last_prompt.as_deref().unwrap_or(""),
        group.repo_name,
        group.repo_path.display(),
        session.git_branch.as_deref().unwrap_or(""),
    )
    .to_lowercase();
    words.iter().all(|w| haystack.contains(w.as_str()))
}

/// Profil zum Fortsetzen: das Claude-Profil (sonst `claude`) mit
/// `--resume <id>` am Ende der Argumente bzw. des Befehls-Templates.
pub fn resume_profile(claude: Option<&AgentProfile>, session_id: &str) -> AgentProfile {
    let mut p = claude.cloned().unwrap_or_else(|| AgentProfile {
        id: "claude".to_string(),
        display_name: "Claude Code".to_string(),
        program: "claude".to_string(),
        args: Vec::new(),
        command: None,
        launch_mode: AgentLaunchMode::Terminal,
        terminal_override: None,
        icon: None,
    });
    p.launch_mode = AgentLaunchMode::Terminal;
    match &mut p.command {
        Some(cmd) if p.args.is_empty() => {
            cmd.push_str(" --resume ");
            cmd.push_str(session_id);
        }
        _ => p
            .args
            .extend(["--resume".to_string(), session_id.to_string()]),
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(v: serde_json::Value) -> String {
        format!("{v}\n")
    }

    fn user(cwd: &str, text: &str) -> String {
        line(json!({"type":"user","cwd":cwd,"message":{"role":"user","content":text}}))
    }

    fn parse(text: &str) -> Option<ClaudeSession> {
        parse_session_bytes("abc", text.as_bytes(), b"", 100)
    }

    #[test]
    fn path_key_normalizes_windows_and_gitbash() {
        assert_eq!(path_key("C:\\Code\\Repo1\\"), "c:/code/repo1");
        assert_eq!(path_key("/c/Code/repo1"), "c:/code/repo1");
        assert_eq!(path_key("c:/code/REPO1"), "c:/code/repo1");
    }

    #[test]
    fn display_cwd_converts_gitbash() {
        assert_eq!(display_cwd("/c/Code/repo1"), "C:\\Code\\repo1");
        assert_eq!(display_cwd("C:\\Code\\x"), "C:\\Code\\x");
        assert_eq!(display_cwd("/home/u/x"), "/home/u/x");
    }

    #[test]
    fn parse_title_priority() {
        let base = user("C:\\Code\\a", "Erste Frage");
        let ai = line(json!({"type":"ai-title","aiTitle":"KI 1"}))
            + &line(json!({"type":"ai-title","aiTitle":"KI 2"}));
        let custom = line(json!({"type":"custom-title","customTitle":"Mein Name"}));

        let all = format!("{base}{ai}{custom}");
        assert_eq!(parse(&all).unwrap().title, "Mein Name");
        assert_eq!(parse(&format!("{base}{ai}")).unwrap().title, "KI 2");
        assert_eq!(parse(&base).unwrap().title, "Erste Frage");
        let prompt_only = line(json!({"type":"system","cwd":"C:\\Code\\a"}))
            + &line(json!({"type":"last-prompt","lastPrompt":"/review MR 96"}));
        assert_eq!(parse(&prompt_only).unwrap().title, "/review MR 96");
    }

    #[test]
    fn parse_hides_sessions_without_conversation() {
        // Nur Slash-Befehle wie `/effort`, nie eine Frage: nichts fortzusetzen
        let text = user("C:\\a", "<command-name>/effort</command-name>")
            + &user("C:\\a", "<local-command-stdout>ok</local-command-stdout>");
        assert!(parse(&text).is_none());
    }

    #[test]
    fn parse_skips_meta_user_messages() {
        let text = line(json!({"type":"user","cwd":"C:\\a","isMeta":true,
                "message":{"role":"user","content":"meta"}}))
            + &line(json!({"type":"user","cwd":"C:\\a",
                "message":{"role":"user","content":[{"type":"text","text":"Echte Frage"}]}}));
        assert_eq!(parse(&text).unwrap().title, "Echte Frage");
    }

    #[test]
    fn parse_skips_command_user_messages() {
        let text = user("C:\\a", "<command-name>/model</command-name>")
            + &user("C:\\a", "Bitte  fixen\nzweite Zeile");
        assert_eq!(parse(&text).unwrap().title, "Bitte  fixen zweite Zeile");
    }

    #[test]
    fn parse_last_prompt_and_branch() {
        let text = line(json!({"type":"user","cwd":"C:\\a","gitBranch":"main",
                "message":{"role":"user","content":"x"}}))
            + &line(json!({"type":"last-prompt","lastPrompt":"alt"}))
            + &line(json!({"type":"assistant","gitBranch":"feature/b"}))
            + &line(json!({"type":"assistant","gitBranch":""}))
            + &line(json!({"type":"last-prompt","lastPrompt":"neu"}));
        let s = parse(&text).unwrap();
        assert_eq!(s.last_prompt.as_deref(), Some("neu"));
        assert_eq!(s.git_branch.as_deref(), Some("feature/b"));
        assert_eq!(s.cwd, "C:\\a");
        assert_eq!(s.id, "abc");
        assert_eq!(s.last_activity, 100);
    }

    #[test]
    fn parse_without_cwd_is_none() {
        let text = line(json!({"type":"ai-title","aiTitle":"x"}));
        assert!(parse(&text).is_none());
    }

    #[test]
    fn parse_skips_garbage_lines() {
        let text = user("C:\\a", "Frage")
            + "{kaputt\n"
            + &line(json!({"type":"ai-title","aiTitle":"Titel"}));
        assert_eq!(parse(&text).unwrap().title, "Titel");
    }

    #[test]
    fn parse_ignores_truncated_last_line() {
        let text = user("C:\\a", "Frage")
            + &line(json!({"type":"ai-title","aiTitle":"Titel"}))
            + "{\"type\":\"ai-ti";
        assert_eq!(parse(&text).unwrap().title, "Titel");
    }

    #[test]
    fn parse_tail_starts_mid_utf8() {
        let head = user("C:\\a", "Frage");
        let mut tail = "ä\n".as_bytes()[1..].to_vec();
        tail.extend_from_slice(line(json!({"type":"ai-title","aiTitle":"Ende"})).as_bytes());
        let s = parse_session_bytes("abc", head.as_bytes(), &tail, 1).unwrap();
        assert_eq!(s.title, "Ende");
    }

    #[test]
    fn parse_large_file_head_and_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("1234-abcd.jsonl");
        let mut text = user("C:\\Code\\gross", "Frage");
        let filler = line(json!({"type":"assistant","message":{"content":"x".repeat(1000)}}));
        while text.len() < 600 * 1024 {
            text.push_str(&filler);
        }
        text.push_str(&line(json!({"type":"ai-title","aiTitle":"Am Ende"})));
        std::fs::write(&path, text).unwrap();

        let s = parse_session(&path).unwrap();
        assert_eq!(s.id, "1234-abcd");
        assert_eq!(s.cwd, "C:\\Code\\gross");
        assert_eq!(s.title, "Am Ende");
        assert!(s.last_activity > 0);
    }

    // ── Task 2: Zuordnung, Scan, Suche, Resume ──────────────────────────

    fn repo(path: &str, name: &str, wt: bool) -> RepoInfo {
        let mut r = RepoInfo::new(PathBuf::from(path), "main".to_string(), false, false);
        r.name = name.to_string();
        r.is_linked_worktree = wt;
        r
    }

    fn session(id: &str, cwd: &str, t: i64) -> ClaudeSession {
        ClaudeSession {
            id: id.to_string(),
            cwd: cwd.to_string(),
            title: format!("Titel {id}"),
            last_prompt: None,
            git_branch: None,
            last_activity: t,
        }
    }

    fn all(_: &str) -> bool {
        true
    }

    #[test]
    fn assign_subfolder_maps_to_repo() {
        let groups = assign_to_repos(
            vec![session("a", "C:\\Code\\repo1\\src", 1)],
            &[repo("C:\\Code\\repo1", "repo1", false)],
            all,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].repo_name, "repo1");
        assert_eq!(groups[0].sessions[0].id, "a");
    }

    #[test]
    fn assign_component_prefix_only() {
        let groups = assign_to_repos(
            vec![session("a", "C:\\Code\\repo10", 1)],
            &[repo("C:\\Code\\repo1", "repo1", false)],
            all,
        );
        assert!(groups.is_empty());
    }

    #[test]
    fn assign_longest_match_wins() {
        let groups = assign_to_repos(
            vec![session("a", "C:\\Code\\repo1\\wt\\feat", 1)],
            &[
                repo("C:\\Code\\repo1", "repo1", false),
                repo("C:\\Code\\repo1\\wt\\feat", "feat", true),
            ],
            all,
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].repo_name, "feat");
        assert!(groups[0].is_worktree);
    }

    #[test]
    fn assign_matches_gitbash_and_case() {
        let groups = assign_to_repos(
            vec![session("a", "/c/code/REPO1", 1)],
            &[repo("C:\\Code\\repo1", "repo1", false)],
            all,
        );
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn assign_hides_orphans() {
        let groups = assign_to_repos(
            vec![session("a", "C:\\Code\\repo1", 1)],
            &[repo("C:\\Code\\repo1", "repo1", false)],
            |_| false,
        );
        assert!(groups.is_empty());
    }

    #[test]
    fn assign_sorts_by_activity() {
        let groups = assign_to_repos(
            vec![
                session("a1", "C:\\a", 5),
                session("b1", "C:\\b", 9),
                session("a2", "C:\\a", 7),
            ],
            &[repo("C:\\a", "a", false), repo("C:\\b", "b", false)],
            all,
        );
        let names: Vec<&str> = groups.iter().map(|g| g.repo_name.as_str()).collect();
        assert_eq!(names, ["b", "a"]);
        let ids: Vec<&str> = groups[1].sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["a2", "a1"]);
    }

    fn write_session(dir: &Path, rel: &str, cwd: &Path) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, user(&cwd.to_string_lossy(), "Frage")).unwrap();
    }

    #[test]
    fn scan_ignores_subdirectories() {
        let projects = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        write_session(projects.path(), "C--a/s1.jsonl", work.path());
        write_session(projects.path(), "C--a/s1/subagents/x.jsonl", work.path());
        let repos = [repo(&work.path().to_string_lossy(), "work", false)];
        let mut cache = SessionCache::default();

        let groups = scan_sessions(projects.path(), &repos, &mut cache);
        assert_eq!(groups.len(), 1);
        let ids: Vec<&str> = groups[0].sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["s1"]);
    }

    #[test]
    fn scan_cache_drops_deleted_files() {
        let projects = tempfile::tempdir().unwrap();
        let work = tempfile::tempdir().unwrap();
        write_session(projects.path(), "C--a/s1.jsonl", work.path());
        let repos = [repo(&work.path().to_string_lossy(), "work", false)];
        let mut cache = SessionCache::default();

        scan_sessions(projects.path(), &repos, &mut cache);
        assert_eq!(cache.len(), 1);
        std::fs::remove_file(projects.path().join("C--a/s1.jsonl")).unwrap();
        let groups = scan_sessions(projects.path(), &repos, &mut cache);
        assert_eq!(cache.len(), 0);
        assert!(groups.is_empty());
    }

    #[test]
    fn search_all_words_case_insensitive() {
        let mut s = session("a", "C:\\Code\\pvsmefa", 1);
        s.title = "Login-Bug fixen".to_string();
        s.git_branch = Some("feature/auth".to_string());
        s.last_prompt = Some("bitte Tests ergänzen".to_string());
        let group = RepoSessions {
            repo_path: PathBuf::from("C:\\Code\\pvsmefa"),
            repo_name: "pvsmefa".to_string(),
            is_worktree: false,
            sessions: vec![],
        };
        assert!(session_matches_search(&group, &s, ""));
        assert!(session_matches_search(&group, &s, "login AUTH"));
        assert!(session_matches_search(&group, &s, "pvsmefa"));
        assert!(session_matches_search(&group, &s, "TESTS"));
        assert!(!session_matches_search(&group, &s, "login logout"));
    }

    fn profile(args: &[&str], command: Option<&str>) -> AgentProfile {
        AgentProfile {
            id: "claude".to_string(),
            display_name: "Claude Code".to_string(),
            program: "claude".to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            command: command.map(str::to_string),
            launch_mode: AgentLaunchMode::Terminal,
            terminal_override: None,
            icon: None,
        }
    }

    #[test]
    fn resume_profile_appends_args() {
        let p = resume_profile(Some(&profile(&["--model", "opus"], None)), "abc");
        assert_eq!(p.args, ["--model", "opus", "--resume", "abc"]);
        assert_eq!(p.program, "claude");
        assert_eq!(p.command, None);
    }

    #[test]
    fn resume_profile_appends_to_command_template() {
        let p = resume_profile(Some(&profile(&[], Some("claude --verbose"))), "abc");
        assert_eq!(p.command.as_deref(), Some("claude --verbose --resume abc"));
        assert!(p.args.is_empty());
    }

    #[test]
    fn resume_profile_default_without_profile() {
        let p = resume_profile(None, "abc");
        assert_eq!(p.id, "claude");
        assert_eq!(p.program, "claude");
        assert_eq!(p.args, ["--resume", "abc"]);
    }
}
