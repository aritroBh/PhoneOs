//! Text hygiene for everything that crosses the hub in either direction.

/// Every character of Unicode general category Cf (format), as inclusive ranges in code
/// point order. Generated from the Unicode Character Database, **version 15.1**
/// (`unicodedata.category(c) == "Cf"`, Python 3.13); regenerate on a Unicode update. These
/// render as nothing or steer how the text around them renders (zero-width, bidi, joiners,
/// interlinear annotation, tags, musical, shorthand and Egyptian format controls, the
/// prepended concatenation marks), so they are never kept in text a person reads.
pub(crate) const FORMAT_RANGES: [(char, char); 21] = [
    ('\u{00AD}', '\u{00AD}'),
    ('\u{0600}', '\u{0605}'),
    ('\u{061C}', '\u{061C}'),
    ('\u{06DD}', '\u{06DD}'),
    ('\u{070F}', '\u{070F}'),
    ('\u{0890}', '\u{0891}'),
    ('\u{08E2}', '\u{08E2}'),
    ('\u{180E}', '\u{180E}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{202A}', '\u{202E}'),
    ('\u{2060}', '\u{2064}'),
    ('\u{2066}', '\u{206F}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{FFF9}', '\u{FFFB}'),
    ('\u{110BD}', '\u{110BD}'),
    ('\u{110CD}', '\u{110CD}'),
    ('\u{13430}', '\u{1343F}'),
    ('\u{1BCA0}', '\u{1BCA3}'),
    ('\u{1D173}', '\u{1D17A}'),
    ('\u{E0001}', '\u{E0001}'),
    ('\u{E0020}', '\u{E007F}'),
];

/// Zero-width, bidi and every other invisible format character: all of general category Cf
/// ([`FORMAT_RANGES`]), plus the unassigned code points the earlier hand list covered
/// (U+2065 and the rest of the tag block, U+E0000 and U+E0002..U+E001F), kept so nothing
/// it dropped is let through.
pub(crate) fn is_format(c: char) -> bool {
    if matches!(c, '\u{2065}' | '\u{E0000}'..='\u{E007F}') {
        return true;
    }
    // Binary search over the sorted, disjoint ranges: the last range starting at or
    // before `c`.
    let i = FORMAT_RANGES.partition_point(|(lo, _)| *lo <= c);
    i.checked_sub(1)
        .and_then(|i| FORMAT_RANGES.get(i))
        .is_some_and(|(_, hi)| c <= *hi)
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

    /// Every Cf character of the table (all of them, range by range) is dropped by the
    /// text cleaning and by the device-name cleaning; what the earlier hand list dropped
    /// still is; neighbours of each range and ordinary text are kept.
    #[test]
    fn every_format_character_is_dropped() {
        let mut count = 0u32;
        let mut previous_end: Option<char> = None;
        for (lo, hi) in FORMAT_RANGES {
            assert!(lo <= hi, "{lo:?}..{hi:?}");
            if let Some(end) = previous_end {
                assert!(
                    u32::from(lo) > u32::from(end) + 1,
                    "ranges unsorted or adjacent at {lo:?}"
                );
            }
            previous_end = Some(hi);
            for c in lo..=hi {
                count += 1;
                assert!(is_format(c), "U+{:04X}", u32::from(c));
                let text = format!("a{c}b");
                assert_eq!(clean_text(&text, 100, true), "ab", "U+{:04X}", u32::from(c));
                assert_eq!(
                    crate::store::clean_name(&text),
                    "ab",
                    "U+{:04X}",
                    u32::from(c)
                );
            }
            for n in [u32::from(lo) - 1, u32::from(hi) + 1] {
                let Some(c) = char::from_u32(n) else { continue };
                if c == '\u{2065}' || ('\u{E0000}'..='\u{E007F}').contains(&c) {
                    continue;
                }
                assert!(!is_format(c), "neighbour U+{n:04X} dropped");
            }
        }
        // Unicode 15.1 has 170 Cf characters.
        assert_eq!(count, 170);
        // The earlier hand list, kept whole.
        for c in [
            '\u{00AD}',
            '\u{061C}',
            '\u{180E}',
            '\u{2065}',
            '\u{FEFF}',
            '\u{E0000}',
            '\u{E0002}',
            '\u{E007F}',
        ]
        .into_iter()
        .chain('\u{200B}'..='\u{200F}')
        .chain('\u{202A}'..='\u{202E}')
        .chain('\u{2060}'..='\u{206F}')
        .chain('\u{FFF9}'..='\u{FFFB}')
        {
            assert!(is_format(c), "U+{:04X}", u32::from(c));
        }
        for c in [
            'a',
            'é',
            '\u{0627}',
            '\u{1D172}',
            '\u{1D17B}',
            '\u{1342F}',
            '\u{1F600}',
            '\u{FFFC}',
        ] {
            assert!(!is_format(c), "U+{:04X}", u32::from(c));
        }
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
