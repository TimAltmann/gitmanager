//! Vorschau einer Session: die letzten Nachrichten (Frage, Antwort,
//! Werkzeugaufrufe) vom Ende der JSONL-Datei.

use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Bytes vom Dateiende für die Vorschau
const PREVIEW_BYTES: u64 = 256 * 1024;
/// Zeichen je Eintrag
const MAX_CHARS: usize = 320;
/// Einträge in der Vorschau
pub const PREVIEW_ITEMS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewRole {
    User,
    Assistant,
    /// Werkzeugaufruf von Claude (`Edit src/x.rs`)
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewItem {
    pub role: PreviewRole,
    pub text: String,
}

/// Session-Datei `<projects>/<ordner>/<id>.jsonl` suchen.
pub fn find_session_file(projects: &Path, id: &str) -> Option<PathBuf> {
    let name = format!("{id}.jsonl");
    std::fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|e| e.path().join(&name))
        .find(|p| p.is_file())
}

/// Die letzten `max` Einträge der Datei.
pub fn read_preview(path: &Path, max: usize) -> std::io::Result<Vec<PreviewItem>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(PREVIEW_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.take(PREVIEW_BYTES).read_to_end(&mut buf)?;
    Ok(parse_preview(&buf, start > 0, max))
}

/// Wertet JSONL aus; bei `cut` ist die erste Zeile angeschnitten und
/// wird verworfen.
pub fn parse_preview(bytes: &[u8], cut: bool, max: usize) -> Vec<PreviewItem> {
    let text = String::from_utf8_lossy(bytes);
    let mut items = Vec::new();
    for raw in text.split('\n').skip(usize::from(cut)) {
        let Ok(v) = serde_json::from_str::<Value>(raw.trim()) else {
            continue;
        };
        let flag = |k: &str| v.get(k).and_then(Value::as_bool) == Some(true);
        if flag("isMeta") || flag("isSidechain") {
            continue;
        }
        match v.get("type").and_then(Value::as_str) {
            Some("user") => {
                if let Some(t) = user_text(&v) {
                    items.push(item(PreviewRole::User, &t));
                }
            }
            Some("assistant") => {
                let Some(Value::Array(parts)) = v.pointer("/message/content") else {
                    continue;
                };
                for p in parts {
                    match p.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(t) = p.get("text").and_then(Value::as_str) {
                                if !t.trim().is_empty() {
                                    items.push(item(PreviewRole::Assistant, t));
                                }
                            }
                        }
                        Some("tool_use") => items.push(item(PreviewRole::Tool, &tool_text(p))),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let skip = items.len().saturating_sub(max);
    items.split_off(skip)
}

/// Text einer Nutzerfrage; Werkzeugergebnisse und Systemtexte (`<…>`) nicht.
fn user_text(v: &Value) -> Option<String> {
    let text = match v.pointer("/message/content")? {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    let t = text.trim();
    (!t.is_empty() && !t.starts_with('<')).then(|| t.to_string())
}

/// `Name Argument` eines Werkzeugaufrufs (Datei, Befehl, Muster …).
fn tool_text(p: &Value) -> String {
    let name = p.get("name").and_then(Value::as_str).unwrap_or("Tool");
    let arg = [
        "file_path",
        "command",
        "pattern",
        "description",
        "url",
        "path",
    ]
    .iter()
    .find_map(|k| p.pointer(&format!("/input/{k}")).and_then(Value::as_str));
    match arg {
        Some(a) => format!("{name} {a}"),
        None => name.to_string(),
    }
}

/// Eine Zeile, höchstens `MAX_CHARS` Zeichen.
fn item(role: PreviewRole, text: &str) -> PreviewItem {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = if flat.chars().count() > MAX_CHARS {
        let cut: String = flat.chars().take(MAX_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        flat
    };
    PreviewItem { role, text }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn jsonl(lines: &[Value]) -> Vec<u8> {
        lines
            .iter()
            .map(|l| format!("{l}\n"))
            .collect::<String>()
            .into_bytes()
    }

    fn user(text: &str) -> Value {
        json!({"type": "user", "message": {"role": "user", "content": text}})
    }

    fn assistant(content: Value) -> Value {
        json!({"type": "assistant", "message": {"role": "assistant", "content": content}})
    }

    #[test]
    fn collects_questions_answers_and_tool_calls_in_order() {
        let bytes = jsonl(&[
            user("Fix den Login"),
            assistant(json!([
                {"type": "thinking", "thinking": "hmm"},
                {"type": "text", "text": "Ich sehe mir das an."},
                {"type": "tool_use", "name": "Edit", "input": {"file_path": "src/login.rs"}}
            ])),
            json!({"type": "user", "message": {"content": [
                {"type": "tool_result", "content": "ok"}
            ]}}),
            assistant(
                json!([{"type": "tool_use", "name": "Bash", "input": {"command": "cargo test"}}]),
            ),
            assistant(json!([{"type": "text", "text": "Fertig,\n\nalle Tests grün."}])),
        ]);
        let items = parse_preview(&bytes, false, 10);
        let got: Vec<(PreviewRole, &str)> =
            items.iter().map(|i| (i.role, i.text.as_str())).collect();
        assert_eq!(
            got,
            [
                (PreviewRole::User, "Fix den Login"),
                (PreviewRole::Assistant, "Ich sehe mir das an."),
                (PreviewRole::Tool, "Edit src/login.rs"),
                (PreviewRole::Tool, "Bash cargo test"),
                (PreviewRole::Assistant, "Fertig, alle Tests grün."),
            ]
        );
    }

    #[test]
    fn skips_meta_sidechain_system_text_and_cut_first_line() {
        let mut bytes =
            b"{\"type\":\"user\",\"message\":{\"content\":\"angeschnitten\"}}\n".to_vec();
        bytes.extend(jsonl(&[
            json!({"type": "user", "isMeta": true, "message": {"content": "meta"}}),
            json!({"type": "user", "message": {"content": "<command-name>/clear</command-name>"}}),
            json!({"type": "assistant", "isSidechain": true, "message": {"content": [{"type": "text", "text": "subagent"}]}}),
            json!({"type": "summary", "summary": "x"}),
            user("echte Frage"),
        ]));
        let items = parse_preview(&bytes, true, 10);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "echte Frage");
    }

    #[test]
    fn keeps_only_the_last_items_and_truncates_long_text() {
        let lines: Vec<Value> = (0..12).map(|i| user(&format!("q{i}"))).collect();
        let items = parse_preview(&jsonl(&lines), false, 3);
        let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, ["q9", "q10", "q11"]);

        let long = "ä".repeat(MAX_CHARS + 50);
        let items = parse_preview(&jsonl(&[user(&long)]), false, 3);
        assert_eq!(items[0].text.chars().count(), MAX_CHARS + 1);
        assert!(items[0].text.ends_with('…'));
    }

    #[test]
    fn reads_tail_of_file_and_finds_it_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let proj = dir.path().join("C--dev-app");
        std::fs::create_dir(&proj).unwrap();
        let file = proj.join("abc-123.jsonl");
        let mut content = jsonl(&[user("alt")]);
        // Füllung größer als das Lesefenster: „alt“ fällt heraus
        let filler = json!({"type": "progress", "data": "x".repeat(1000)});
        for _ in 0..300 {
            content.extend(format!("{filler}\n").into_bytes());
        }
        content.extend(jsonl(&[user("neu")]));
        std::fs::write(&file, content).unwrap();

        assert_eq!(find_session_file(dir.path(), "abc-123"), Some(file.clone()));
        assert_eq!(find_session_file(dir.path(), "nope"), None);
        let items = read_preview(&file, 5).unwrap();
        let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, ["neu"]);
    }
}
