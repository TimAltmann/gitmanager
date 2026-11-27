//! Kleine Texthelfer: einzeilige Kurzfassung und XML-Entities.

/// Einzeilig (Leerraum zusammengefasst) und auf `max_chars` Zeichen
/// gekürzt; gekürzter Text endet auf „…“.
pub fn snippet(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max_chars {
        let cut: String = flat.chars().take(max_chars).collect();
        format!("{}…", cut.trim_end())
    } else {
        flat
    }
}

/// Ersetzt die fünf XML-Sonderzeichen durch ihre Entities (Text und
/// Attributwerte). Steuerzeichen außer Tab und Zeilenumbruch entfallen: XML
/// 1.0 erlaubt sie nicht (ein ESC aus kopierter Terminal-Ausgabe ließe
/// sonst die ganze Benachrichtigung scheitern).
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() => {}
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Umkehrung von [`xml_escape`]; `&amp;` zuletzt, damit `&amp;lt;` nicht
/// doppelt dekodiert wird.
pub fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_flattens_and_cuts_char_safe() {
        assert_eq!(snippet("  a\n\tb  c ", 20), "a b c");
        assert_eq!(snippet("", 20), "");
        let long = "ä".repeat(30);
        let cut = snippet(&long, 10);
        assert_eq!(cut.chars().count(), 11);
        assert!(cut.ends_with('…'));
        // Kein Leerzeichen vor der Ellipse
        assert_eq!(snippet("abc def", 4), "abc…");
    }

    #[test]
    fn xml_escape_roundtrips() {
        let raw = r#"<a href="x">Tom & 'Jerry'</a> &lt;"#;
        let escaped = xml_escape(raw);
        assert_eq!(
            escaped,
            "&lt;a href=&quot;x&quot;&gt;Tom &amp; &apos;Jerry&apos;&lt;/a&gt; &amp;lt;"
        );
        assert_eq!(xml_unescape(&escaped), raw);
        assert_eq!(xml_escape("a\u{1b}[31mb\u{0}\tc"), "a[31mb\tc");
    }
}
