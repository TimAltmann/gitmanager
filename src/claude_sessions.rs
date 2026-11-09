//! Claude-Code-Sessions finden, einlesen und Repos zuordnen.
//!
//! Claude Code legt jede Session als JSONL-Datei unter
//! `<config>/projects/<kodierter-pfad>/<session-id>.jsonl` ab. Der
//! Ordnername ist verlustbehaftet kodiert, maßgeblich ist das Feld `cwd`
//! in den Zeilen.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

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
    /// `custom-title` > `ai-title` > erste Frage > ID
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

/// Liest Kopf und Ende einer Session-Datei. `None` ohne `cwd` oder bei
/// Lesefehlern.
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

    let title = custom_title
        .or(ai_title)
        .or(first_question)
        .unwrap_or_else(|| id.to_string());
    Some(ClaudeSession {
        id: id.to_string(),
        cwd: display_cwd(&cwd?),
        title,
        last_prompt,
        git_branch: branch,
        last_activity: mtime,
    })
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
        let no_user = line(json!({"type":"system","cwd":"C:\\Code\\a"}));
        assert_eq!(parse(&no_user).unwrap().title, "abc");
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
}
