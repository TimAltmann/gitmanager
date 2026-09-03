use crate::config::ConfigSelector;
use std::io::Write;
use std::path::{Path, PathBuf};

fn escape_xml_attr(s: &str) -> String {
    // Escape XML attribute value (minimal set). Order matters: & first.
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn unescape_xml_attr(s: &str) -> String {
    // Reverse of escape – handle common entities. Do &amp; last to avoid double decoding
    // For key comparison fallback.
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn find_attr_range(tag: &str, attr_name: &str) -> Option<(usize, usize, char)> {
    let mut search_pos = 0;
    while search_pos < tag.len() {
        let rel = tag[search_pos..].find(attr_name)?;
        let attr_start = search_pos + rel;
        // left boundary: char before must be whitespace or '<' or '"'/'\''? but not alnum/_-:. to avoid substring
        let before_ok = if attr_start == 0 {
            false
        } else {
            let b = tag.as_bytes()[attr_start - 1] as char;
            b.is_whitespace()
                || b == '<'
                || b == '\''
                || b == '"'
                || b == '/'
                || b == '\n'
                || b == '\r'
                || b == '\t'
        };
        if !before_ok {
            search_pos = attr_start + attr_name.len();
            continue;
        }
        let attr_end = attr_start + attr_name.len();
        if attr_end >= tag.len() {
            search_pos = attr_end;
            continue;
        }
        // skip whitespace to '='
        let mut i = attr_end;
        while i < tag.len() && tag.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= tag.len() || tag.as_bytes()[i] != b'=' {
            search_pos = attr_end;
            continue;
        }
        i += 1;
        while i < tag.len() && tag.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= tag.len() {
            search_pos = attr_end;
            continue;
        }
        let quote = tag.as_bytes()[i] as char;
        if quote != '"' && quote != '\'' {
            search_pos = attr_end;
            continue;
        }
        i += 1;
        let val_start = i;
        // find closing quote
        if let Some(rel2) = tag[val_start..].find(quote) {
            let val_end = val_start + rel2;
            return Some((val_start, val_end, quote));
        } else {
            search_pos = attr_end;
            continue;
        }
    }
    None
}

fn replace_value_in_xml(
    xml: &str,
    key: &str,
    key_attr: &str,
    val_attr: &str,
    new_value: &str,
) -> Option<String> {
    let escaped = escape_xml_attr(new_value);
    let mut search_start = 0;
    while search_start < xml.len() {
        // find next "<add" prefix
        let start_rel = xml[search_start..].find("<add")?;
        let start = search_start + start_rel;
        // validate delimiter after "<add"
        if start + 4 < xml.len() {
            let after_char = xml.as_bytes()[start + 4] as char;
            if after_char.is_alphanumeric() || matches!(after_char, '_' | '-' | ':' | '.') {
                search_start = start + 4;
                continue;
            }
        }
        // find end of opening tag '>'
        let end_rel = xml[start..].find('>')?;
        let end = start + end_rel; // index of '>'
        let tag = &xml[start..=end];
        // check key attr matches
        if let Some((k_start, k_end, _)) = find_attr_range(tag, key_attr) {
            let k_val_raw = &tag[k_start..k_end];
            let k_val_decoded = unescape_xml_attr(k_val_raw);
            if k_val_raw == key || k_val_decoded == key {
                // found target element, now locate value attr
                if let Some((v_start_rel, v_end_rel, _quote)) = find_attr_range(tag, val_attr) {
                    let abs_v_start = start + v_start_rel;
                    let abs_v_end = start + v_end_rel;
                    let mut new_xml = String::with_capacity(xml.len() + escaped.len());
                    new_xml.push_str(&xml[..abs_v_start]);
                    new_xml.push_str(&escaped);
                    new_xml.push_str(&xml[abs_v_end..]);
                    return Some(new_xml);
                } else {
                    return None;
                }
            }
        }
        search_start = end + 1;
    }
    None
}

/// Find value of <add> element where key_attr==key, return val_attr value.
/// Implemented via roxmltree (read-only).
fn find_add_element_value(xml: &str, key: &str, key_attr: &str, val_attr: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    for node in doc.descendants() {
        if node.tag_name().name() == "add" {
            if let Some(k) = node.attribute(key_attr) {
                if k == key {
                    if let Some(v) = node.attribute(val_attr) {
                        return Some(v.to_string());
                    }
                }
            }
        }
    }
    None
}

fn is_invalid_file_path(file_path: &str) -> bool {
    let p = Path::new(file_path);
    if p.is_absolute() {
        return true;
    }
    // Check for ".." component on both Unix and Windows separators
    if file_path.split('/').any(|c| c == "..") || file_path.split('\\').any(|c| c == "..") {
        return true;
    }
    false
}

pub fn read_xml_value(
    repo_path: &Path,
    selector: &ConfigSelector,
) -> Result<Option<String>, String> {
    if is_invalid_file_path(&selector.file_path) {
        return Err(format!(
            "Ungültiger Dateipfad: '{}' darf nicht absolut sein oder .. enthalten",
            selector.file_path
        ));
    }
    let file_path = repo_path.join(&selector.file_path);
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Konnte {} nicht lesen: {}", file_path.display(), e))?;
    // validate XML is well-formed, map parse error to Err
    roxmltree::Document::parse(&content)
        .map_err(|e| format!("XML ungültig in {}: {}", file_path.display(), e))?;
    Ok(find_add_element_value(
        &content,
        &selector.key,
        &selector.key_attribute,
        &selector.value_attribute,
    ))
}

pub fn write_xml_value(
    repo_path: &Path,
    selector: &ConfigSelector,
    new_value: &str,
) -> Result<(), String> {
    if is_invalid_file_path(&selector.file_path) {
        return Err(format!(
            "Ungültiger Dateipfad: '{}' darf nicht absolut sein oder .. enthalten",
            selector.file_path
        ));
    }
    let file_path = repo_path.join(&selector.file_path);
    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Konnte {} nicht lesen: {}", file_path.display(), e))?;
    // validate XML
    roxmltree::Document::parse(&content)
        .map_err(|e| format!("XML ungültig in {}: {}", file_path.display(), e))?;
    if find_add_element_value(
        &content,
        &selector.key,
        &selector.key_attribute,
        &selector.value_attribute,
    )
    .is_none()
    {
        return Err(format!(
            "Key '{}' nicht gefunden in {}",
            selector.key, selector.file_path
        ));
    }
    let new_content = replace_value_in_xml(
        &content,
        &selector.key,
        &selector.key_attribute,
        &selector.value_attribute,
        new_value,
    )
    .ok_or_else(|| {
        format!(
            "Key '{}' nicht gefunden in {}",
            selector.key, selector.file_path
        )
    })?;
    // atomic write via tmp+rename
    let tmp_path = PathBuf::from(format!("{}.tmp", file_path.display()));
    {
        let mut file = std::fs::File::create(&tmp_path)
            .map_err(|e| format!("Konnte tmp nicht schreiben: {}", e))?;
        file.write_all(new_content.as_bytes())
            .map_err(|e| format!("Schreiben fehlgeschlagen: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("Sync fehlgeschlagen: {}", e))?;
    }
    std::fs::rename(&tmp_path, &file_path)
        .map_err(|e| format!("Konnte Config nicht finalisieren: {}", e))?;
    #[cfg(unix)]
    {
        if let Some(parent) = file_path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn make_selector(key: &str) -> ConfigSelector {
        ConfigSelector {
            id: "db".into(),
            display_name: "DB".into(),
            file_path: "App.config".into(),
            key: key.into(),
            key_attribute: "key".into(),
            value_attribute: "value".into(),
            kind: crate::config::XmlSelectorKind::AddKeyValue,
            options: vec![],
            allow_custom: false,
        }
    }

    #[test]
    fn read_add_key_value_found() {
        let xml = r#"<?xml version="1.0"?><configuration><appSettings><add key="Database" value="dev" /></appSettings></configuration>"#;
        assert_eq!(
            find_add_element_value(xml, "Database", "key", "value"),
            Some("dev".into())
        );
    }
    #[test]
    fn read_not_found_returns_none() {
        let xml = r#"<configuration><appSettings><add key="Other" value="x"/></appSettings></configuration>"#;
        assert_eq!(
            find_add_element_value(xml, "Database", "key", "value"),
            None
        );
    }
    #[test]
    fn write_preserves_comments_and_whitespace() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("App.config");
        fs::write(
            &path,
            "<!-- comment -->\n<configuration>\n  <appSettings>\n    <add key=\"Database\" value=\"dev\" />\n  </appSettings>\n</configuration>",
        )
        .unwrap();
        let sel = make_selector("Database");
        write_xml_value(dir.path(), &sel, "prod").unwrap();
        let out = fs::read_to_string(&path).unwrap();
        assert!(out.contains("<!-- comment -->"));
        assert!(out.contains(r#"value="prod""#));
    }
}
