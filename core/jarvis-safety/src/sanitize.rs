//! Screen-text hygiene: text read from other apps is untrusted data and must not be able to
//! close the prompt's data block, imitate chat-template control tokens or hide characters.

use crate::text::{fold_char, fold_fullwidth, is_invisible};

/// Opens the data block that holds the rendered screen in a prompt. `jarvis-agent` puts
/// screen text only between this and [`SCREEN_BLOCK_END`].
pub const SCREEN_BLOCK_START: &str = "<<<SCREEN_DATA>>>";
/// Closes the screen data block.
pub const SCREEN_BLOCK_END: &str = "<<<END_SCREEN_DATA>>>";
/// Default cap applied by [`sanitize_screen_text`], in characters (ellipsis included).
pub const MAX_SCREEN_TEXT_CHARS: usize = 500;

/// [`sanitize_screen_text_with_limit`] with [`MAX_SCREEN_TEXT_CHARS`].
pub fn sanitize_screen_text(text: &str) -> String {
    sanitize_screen_text_with_limit(text, MAX_SCREEN_TEXT_CHARS)
}

/// Makes untrusted screen text safe to place inside the prompt's data block.
///
/// 1. Fullwidth forms fold to ASCII; control, zero-width, bidi, tag and variation-selector
///    characters are removed; every kind of whitespace becomes a space.
/// 2. `<` `>` (and look-alikes) become `‹` `›`, `[` `]` become `(` `)` and backticks
///    become `'`. This breaks every chat-template token we know of (`<|im_start|>`,
///    `</s>`, `<start_of_turn>`, `[INST]`, `<<SYS>>`, DeepSeek's `<｜…｜>`), code fences,
///    fake element indices (`[17]`) and [`SCREEN_BLOCK_START`]/[`SCREEN_BLOCK_END`],
///    since no output can contain an ASCII `<`.
/// 3. Whitespace collapses to single spaces and is trimmed; runs of three or more of
///    `~ # = * _ - '` shrink to two.
/// 4. `screen data` in any case, spacing or look-alike spelling (`SCREEN_DATA`,
///    `ScreenData`, `scrеen-data` with a Cyrillic `е`) becomes `screen data`.
/// 5. Longer than `max_chars` characters: cut and end with `…` (total `max_chars`).
///
/// Idempotent: `sanitize(sanitize(x)) == sanitize(x)` for the same limit.
pub fn sanitize_screen_text_with_limit(text: &str, max_chars: usize) -> String {
    let mapped = map_chars(text);
    let collapsed = collapse(&mapped);
    let neutral = neutralize_keyword(&collapsed);
    truncate(&neutral, max_chars)
}

/// Step 1 and 2.
fn map_chars(text: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    for raw in text.chars() {
        let c = fold_fullwidth(raw);
        if c.is_whitespace() {
            out.push(' ');
            continue;
        }
        if is_invisible(c) {
            continue;
        }
        out.push(match c {
            '<' | '\u{2039}' | '\u{FE64}' | '\u{3008}' | '\u{27E8}' | '\u{2329}' | '\u{276E}'
            | '\u{2770}' | '\u{29FC}' | '\u{1438}' => '\u{2039}',
            '>' | '\u{203A}' | '\u{FE65}' | '\u{3009}' | '\u{27E9}' | '\u{232A}' | '\u{276F}'
            | '\u{2771}' | '\u{29FD}' | '\u{1433}' => '\u{203A}',
            '[' | '\u{3010}' | '\u{3014}' | '\u{3016}' | '\u{301A}' | '\u{27E6}' => '(',
            ']' | '\u{3011}' | '\u{3015}' | '\u{3017}' | '\u{301B}' | '\u{27E7}' => ')',
            '`' => '\'',
            other => other,
        });
    }
    out
}

/// Step 3.
fn collapse(chars: &[char]) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    for &c in chars {
        if c == ' ' && matches!(out.last(), None | Some(' ')) {
            continue;
        }
        if matches!(c, '~' | '#' | '=' | '*' | '_' | '-' | '\'') {
            let n = out.len();
            let tail = out.get(n.saturating_sub(2)..).unwrap_or_default();
            if tail.len() == 2 && tail.iter().all(|t| *t == c) {
                continue;
            }
        }
        out.push(c);
    }
    while out.last() == Some(&' ') {
        out.pop();
    }
    out
}

/// Whether `chars[i..]` starts with the ASCII word `word`, compared look-alike-insensitively.
fn starts_with_folded(chars: &[char], i: usize, word: &str) -> bool {
    let mut k = i;
    for w in word.chars() {
        match chars.get(k) {
            Some(&c) if fold_char(c) == w => k += 1,
            _ => return false,
        }
    }
    true
}

/// Step 4.
fn neutralize_keyword(chars: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0usize;
    while i < chars.len() {
        if starts_with_folded(chars, i, "screen") {
            let mut j = i + 6;
            while matches!(chars.get(j), Some(' ' | '_' | '-' | '.' | '·' | ':' | '/')) {
                j += 1;
            }
            if starts_with_folded(chars, j, "data") {
                out.extend("screen data".chars());
                i = j + 4;
                continue;
            }
        }
        if let Some(&c) = chars.get(i) {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Step 5.
fn truncate(chars: &[char], max_chars: usize) -> String {
    if chars.len() <= max_chars {
        return chars.iter().collect();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut s: String = chars
        .get(..max_chars.saturating_sub(1))
        .unwrap_or_default()
        .iter()
        .collect();
    let trimmed = s.trim_end().len();
    s.truncate(trimmed);
    s.push('…');
    s
}
