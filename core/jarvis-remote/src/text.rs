//! Text hygiene for everything that crosses the hub in either direction.

/// Zero-width, bidi and similar invisible format characters.
pub(crate) fn is_format(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}'
        | '\u{E0000}'..='\u{E007F}')
}

/// Removes control and invisible format characters and collapses every whitespace run
/// (tab, NBSP, U+2028…) into one space, trimmed. With `keep_newlines`, `\n` survives.
fn normalize(text: &str, keep_newlines: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        if c == '\n' && keep_newlines {
            out.push('\n');
            pending_space = false;
            continue;
        }
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if c.is_control() || is_format(c) {
            continue;
        }
        if pending_space && !out.ends_with('\n') {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
    }
    out
}

fn cap(text: String, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((i, _)) => text.get(..i).unwrap_or_default().to_owned(),
        None => text,
    }
}

/// [`normalize`] then cut to `max_chars` characters.
pub(crate) fn clean_text(text: &str, max_chars: usize, keep_newlines: bool) -> String {
    cap(normalize(text, keep_newlines), max_chars)
}

/// Free text from the phone, made safe to send to a remote: normalised to one line first
/// (so a card number split by tabs, newlines, NBSP or zero-width characters is seen in
/// its canonical form), then [`jarvis_safety::redact`], then normalised again and capped.
/// Capping last means truncation can never cut a secret out of the redactor's view.
pub(crate) fn redact_text(text: &str, max_chars: usize) -> String {
    let flat = normalize(text, false);
    cap(normalize(&jarvis_safety::redact(&flat), false), max_chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_strips_controls_and_caps() {
        assert_eq!(clean_text("a\u{0}b\u{202E}c  d\n e", 100, false), "abc d e");
        assert_eq!(clean_text("a\n\nb", 100, true), "a\n\nb");
        assert_eq!(clean_text("abcdef", 3, false), "abc");
        assert_eq!(clean_text("é".repeat(10).as_str(), 4, false), "éééé");
    }

    #[test]
    fn redaction_sees_the_normalised_text() {
        for sep in [
            "\t",
            "\n",
            "\u{2028}",
            "\u{00A0}",
            "\u{200D} ",
            " \u{200B}",
            "\r\n",
        ] {
            let card = format!("Paid with 4111{sep}1111{sep}1111{sep}1111 ok");
            let out = redact_text(&card, 500);
            assert!(!out.contains("4111"), "{sep:?}: {out}");
            assert!(out.contains("[redacted card]"), "{sep:?}: {out}");
            let otp = format!("Your code is 123{sep}456");
            let out = redact_text(&otp, 500);
            assert!(!out.contains("123"), "{sep:?}: {out}");
        }
        // Capping happens after redaction: a cut mask is fine, a revealed digit is not.
        let out = redact_text("4111 1111 1111 1111", 5);
        assert!(!out.chars().any(|c| c.is_ascii_digit()), "{out}");
    }
}
