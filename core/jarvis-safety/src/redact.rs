//! Redaction of secrets before text reaches a prompt or a log.
//!
//! Detectors, all hand-written:
//!
//! * **Card numbers**: 13–19 digits, optionally grouped by one or two spaces, dashes or dots,
//!   that pass the Luhn check. Inside a longer grouped run, any run of whole groups that qualifies.
//! * **One-time codes**: a standalone 4–8 digit number (or `123 456` / `123-456`) within five
//!   words of `code`, `OTP`, `verification`, `passcode`, `PIN`... `zip code`, `area code`,
//!   `promo code` and similar do not count as keywords.
//! * **API keys and tokens**: well-known prefixes (`sk-`, `ghp_`, `xoxb-`, `AKIA`, `AIza`,
//!   JWTs...) and long high-entropy strings mixing letters and digits.
//! * **Labelled secrets**: the value after `password:`, `token=`, `Bearer `...
//!
//! Phone numbers, dates, times, prices and ordinary numbers are left alone. Invisible
//! characters are dropped first so they cannot split a secret.

use jarvis_proto::{CompactScreen, Element, RiskCategory};

use crate::phrases::{match_label, CREDENTIAL_FIELD_PHRASES};
use crate::text::{fold_fullwidth, is_invisible};

/// Replacement for a card number.
pub const CARD_MASK: &str = "[redacted card]";
/// Replacement for a one-time code.
pub const CODE_MASK: &str = "[redacted number]";
/// Replacement for an API key or token.
pub const KEY_MASK: &str = "[redacted key]";
/// Replacement for a labelled secret or a secure field's value.
pub const SECRET_MASK: &str = "[redacted secret]";

/// Masks card numbers, one-time codes, API keys and labelled secrets in `text`.
///
/// The masks contain no digits or trigger words, so re-redacting never alters an existing
/// mask (it can, rarely, mask a code that has moved closer to its keyword).
pub fn redact(text: &str) -> String {
    let chars = visible_chars(text);
    let spans = find_spans(&chars);
    if spans.is_empty() {
        return chars.iter().collect();
    }
    let mut out = String::with_capacity(text.len());
    let mut pos = 0usize;
    for (start, end, mask) in spans {
        if start < pos {
            // Overlaps the previous mask: swallow the tail too.
            pos = pos.max(end);
            continue;
        }
        out.extend(chars.get(pos..start).unwrap_or_default());
        out.push_str(mask);
        pos = end;
    }
    out.extend(chars.get(pos..).unwrap_or_default());
    out
}

/// True when `text` contains anything [`redact`] would mask, or is a single password-like
/// word (8+ characters mixing lowercase, uppercase, digits and symbols).
pub fn contains_secret(text: &str) -> bool {
    let chars = visible_chars(text);
    !find_spans(&chars).is_empty() || is_password_like(text)
}

/// A copy of `el` safe for prompts and logs: secure fields lose their value, a field
/// labelled as a credential ("Verification code", "CVV") has its value masked, and label,
/// value and placeholder are passed through [`redact`].
pub fn redact_element(el: &Element) -> Element {
    let mut out = el.clone();
    out.label = redact(&el.label);
    out.placeholder = el.placeholder.as_deref().map(redact);
    let credential_field = el.role.is_text_input()
        && match_label(&el.label, &[CREDENTIAL_FIELD_PHRASES]).category != RiskCategory::None;
    out.value = if el.secure {
        None
    } else if credential_field {
        el.value.as_ref().map(|v| {
            if v.is_empty() {
                String::new()
            } else {
                SECRET_MASK.to_owned()
            }
        })
    } else {
        el.value.as_deref().map(redact)
    };
    out
}

/// A copy of `screen` with every element passed through [`redact_element`].
pub fn redact_screen(screen: &CompactScreen) -> CompactScreen {
    let mut out = screen.clone();
    out.elements = screen.elements.iter().map(redact_element).collect();
    out
}

/// The characters of `text` without invisible ones, so `4111\u{200B}1111…` cannot dodge a
/// detector. [`redact`] output therefore never contains invisible characters either.
fn visible_chars(text: &str) -> Vec<char> {
    text.chars().filter(|c| !is_invisible(*c)).collect()
}

/// A decimal digit's value, accepting fullwidth digits.
fn digit(c: char) -> Option<u32> {
    fold_fullwidth(c).to_digit(10)
}

/// Spans `(start, end, mask)` over `chars`, sorted by start, possibly overlapping.
fn find_spans(chars: &[char]) -> Vec<(usize, usize, &'static str)> {
    let mut spans = Vec::new();
    card_spans(chars, &mut spans);
    labelled_secret_spans(chars, &mut spans);
    key_spans(chars, &mut spans);
    code_spans(chars, &mut spans);
    spans.sort_by_key(|&(s, e, _)| (s, std::cmp::Reverse(e)));
    spans
}

fn at(chars: &[char], i: usize) -> Option<char> {
    chars.get(i).copied()
}

/// Luhn checksum over digit values.
fn luhn(digits: &[u32]) -> bool {
    let mut sum = 0u32;
    for (i, d) in digits.iter().rev().enumerate() {
        let v = if i % 2 == 1 {
            let x = d.saturating_mul(2);
            if x > 9 {
                x - 9
            } else {
                x
            }
        } else {
            *d
        };
        sum = sum.saturating_add(v);
    }
    sum.is_multiple_of(10)
}

/// Card numbers: grouped digit runs with 13–19 digits passing Luhn.
fn card_spans(chars: &[char], spans: &mut Vec<(usize, usize, &'static str)>) {
    let mut i = 0usize;
    while i < chars.len() {
        let starts_run = at(chars, i).and_then(digit).is_some()
            && (i == 0 || at(chars, i - 1).and_then(digit).is_none());
        if !starts_run {
            i += 1;
            continue;
        }
        // groups: (start, end, digit values)
        let mut groups: Vec<(usize, usize, Vec<u32>)> = Vec::new();
        let mut j = i;
        loop {
            let gs = j;
            let mut ds = Vec::new();
            while let Some(d) = at(chars, j).and_then(digit) {
                ds.push(d);
                j += 1;
            }
            groups.push((gs, j, ds));
            // One or two separators between groups.
            let is_sep = |c: Option<char>| matches!(c, Some(' ' | '-' | '.' | '\u{a0}'));
            let width = if is_sep(at(chars, j)) && is_sep(at(chars, j + 1)) {
                2
            } else if is_sep(at(chars, j)) {
                1
            } else {
                0
            };
            if width > 0 && at(chars, j + width).and_then(digit).is_some() {
                j += width;
            } else {
                break;
            }
        }
        for a in 0..groups.len() {
            let mut all: Vec<u32> = Vec::new();
            for b in a..groups.len() {
                if let Some((_, _, ds)) = groups.get(b) {
                    all.extend(ds);
                }
                if (13..=19).contains(&all.len()) && luhn(&all) {
                    if let (Some(first), Some(last)) = (groups.get(a), groups.get(b)) {
                        spans.push((first.0, last.1, CARD_MASK));
                    }
                }
                if all.len() > 19 {
                    break;
                }
            }
        }
        i = j.max(i + 1);
    }
}

/// Words whose value after `:`/`=` is a secret.
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "pass",
    "passcode",
    "secret",
    "client_secret",
    "token",
    "access_token",
    "refresh_token",
    "auth_token",
    "apikey",
    "api_key",
    "api-key",
    "x-api-key",
];

/// Values after `password:`, `token=`, `Bearer ` and similar.
fn labelled_secret_spans(chars: &[char], spans: &mut Vec<(usize, usize, &'static str)>) {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    let mut i = 0usize;
    while i < chars.len() {
        if !at(chars, i).is_some_and(is_word) || (i > 0 && at(chars, i - 1).is_some_and(is_word)) {
            i += 1;
            continue;
        }
        let mut j = i;
        while at(chars, j).is_some_and(is_word) {
            j += 1;
        }
        let word: String = chars
            .get(i..j)
            .unwrap_or_default()
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        let mut k = j;
        let bearer = word == "bearer";
        if bearer {
            if at(chars, k) != Some(' ') {
                i = j;
                continue;
            }
        } else if SECRET_KEYS.contains(&word.as_str()) {
            while at(chars, k) == Some(' ') {
                k += 1;
            }
            if !matches!(at(chars, k), Some(':' | '=')) {
                i = j;
                continue;
            }
            k += 1;
        } else {
            i = j;
            continue;
        }
        while at(chars, k) == Some(' ') {
            k += 1;
        }
        let vs = k;
        while at(chars, k).is_some_and(|c| !c.is_whitespace() && c != ',' && c != ';') {
            k += 1;
        }
        let value: String = chars.get(vs..k).unwrap_or_default().iter().collect();
        if k > vs && !value.starts_with("[redacted") {
            spans.push((vs, k, SECRET_MASK));
        }
        i = k.max(j);
    }
}

/// Known key prefixes and the minimum total length for a match.
const KEY_PREFIXES: &[(&str, usize)] = &[
    ("sk-", 20),
    ("sk_live_", 16),
    ("sk_test_", 16),
    ("rk_live_", 16),
    ("pk_live_", 16),
    ("ghp_", 30),
    ("gho_", 30),
    ("ghu_", 30),
    ("ghs_", 30),
    ("ghr_", 30),
    ("github_pat_", 30),
    ("xoxb-", 20),
    ("xoxp-", 20),
    ("xoxa-", 20),
    ("xoxs-", 20),
    ("xapp-", 20),
    ("glpat-", 20),
    ("hf_", 30),
    ("npm_", 30),
    ("shpat_", 30),
    ("AKIA", 20),
    ("ASIA", 20),
    ("AIza", 35),
    ("ya29.", 30),
    ("SG.", 40),
];

/// Shannon entropy of `s` in bits per character.
fn entropy(s: &str) -> f64 {
    let mut counts = [0u32; 128];
    let mut n = 0u32;
    for b in s.bytes() {
        if let Some(c) = counts.get_mut(usize::from(b & 0x7F)) {
            *c = c.saturating_add(1);
        }
        n = n.saturating_add(1);
    }
    if n == 0 {
        return 0.0;
    }
    let total = f64::from(n);
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = f64::from(c) / total;
            -p * p.log2()
        })
        .sum()
}

/// Whether `tok` (a run of key characters) looks like a credential.
fn is_key_like(tok: &str) -> bool {
    if KEY_PREFIXES
        .iter()
        .any(|(p, min)| tok.starts_with(p) && tok.len() >= *min)
    {
        return true;
    }
    // JSON Web Token: three base64url parts.
    if tok.starts_with("eyJ") {
        let parts: Vec<&str> = tok.split('.').collect();
        if parts.len() == 3 && parts.iter().all(|p| p.len() >= 8) {
            return true;
        }
    }
    // Generic: one long random-looking part. Splitting on separators keeps URLs, slugs
    // (`Some-Article-2024`) and UUIDs out.
    tok.split(['-', '_', '.', '/']).any(is_random_part)
}

/// Whether one separator-free part looks random: 24+ characters mixing upper, lower and
/// digits with high entropy, or 32+ hex characters mixing letters and digits.
fn is_random_part(part: &str) -> bool {
    let core = part.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    let has_digit = core.bytes().any(|b| b.is_ascii_digit());
    let has_lower = core.bytes().any(|b| b.is_ascii_lowercase());
    let has_upper = core.bytes().any(|b| b.is_ascii_uppercase());
    let hex = core.bytes().all(|b| b.is_ascii_hexdigit());
    if hex {
        return core.len() >= 32 && has_digit && (has_lower || has_upper) && entropy(core) >= 3.0;
    }
    core.len() >= 24 && has_digit && has_lower && has_upper && entropy(core) >= 3.5
}

/// API keys and tokens.
fn key_spans(chars: &[char], spans: &mut Vec<(usize, usize, &'static str)>) {
    let is_key_char =
        |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | '/' | '=');
    let mut i = 0usize;
    while i < chars.len() {
        if !at(chars, i).is_some_and(is_key_char) {
            i += 1;
            continue;
        }
        let mut j = i;
        while at(chars, j).is_some_and(is_key_char) {
            j += 1;
        }
        // Do not swallow sentence punctuation at the end.
        let mut end = j;
        while end > i && matches!(at(chars, end - 1), Some('.' | '-' | '/')) {
            end -= 1;
        }
        let mut start = i;
        while start < end && matches!(at(chars, start), Some('.' | '/' | '=' | '+')) {
            start += 1;
        }
        let tok: String = chars.get(start..end).unwrap_or_default().iter().collect();
        if is_key_like(&tok) {
            spans.push((start, end, KEY_MASK));
        }
        i = j;
    }
}

/// Keywords that make a nearby short number a one-time code.
const CODE_KEYWORDS: &[&str] = &[
    "code",
    "codes",
    "otp",
    "passcode",
    "verification",
    "verify",
    "pin",
    "2fa",
    "authentication",
    "auth",
    "one-time",
    "onetime",
];

/// Words that, right before `code`, make it something other than a one-time code.
const CODE_EXEMPT_BEFORE: &[&str] = &[
    "zip", "postal", "post", "area", "country", "promo", "coupon", "discount", "qr", "bar",
    "source", "error", "status", "dress", "morse", "dial", "sort", "tax", "product", "style",
];

/// One word of `chars`: `(start, end, lowercase text)`.
type Word = (usize, usize, String);

/// Words for code detection: alphanumeric runs (with inner `-`), lowercase.
fn words(chars: &[char]) -> Vec<Word> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if !at(chars, i).is_some_and(char::is_alphanumeric) {
            i += 1;
            continue;
        }
        let mut j = i;
        while at(chars, j).is_some_and(|c| {
            c.is_alphanumeric()
                || (c == '-'
                    && at(chars, j + 1).is_some_and(char::is_alphabetic)
                    && at(chars, j.wrapping_sub(1)).is_some_and(char::is_alphabetic))
        }) {
            j += 1;
        }
        let w: String = chars
            .get(i..j)
            .unwrap_or_default()
            .iter()
            .flat_map(|c| fold_fullwidth(*c).to_lowercase())
            .collect();
        out.push((i, j, w));
        i = j;
    }
    out
}

/// Whether the digits at `start..end` stand alone (not part of a price, decimal, time,
/// date, percentage or longer number).
fn standalone_number(chars: &[char], start: usize, end: usize) -> bool {
    let before = start.checked_sub(1).and_then(|i| at(chars, i));
    let before2 = start.checked_sub(2).and_then(|i| at(chars, i));
    let after = at(chars, end);
    let after2 = at(chars, end + 1);
    let joins = |c: Option<char>| matches!(c, Some('.' | ',' | ':' | '/'));
    let is_d = |c: Option<char>| c.and_then(digit).is_some();
    if (joins(before) && is_d(before2)) || (joins(after) && is_d(after2)) {
        return false;
    }
    let currency = |c: Option<char>| {
        c.is_some_and(|c| matches!(fold_fullwidth(c), '$' | '€' | '£' | '¥' | '₹' | '#' | '+'))
    };
    if currency(before) || (before == Some(' ') && currency(before2)) {
        return false;
    }
    !matches!(after, Some('%'))
}

/// One-time codes near a keyword.
fn code_spans(chars: &[char], spans: &mut Vec<(usize, usize, &'static str)>) {
    let ws = words(chars);
    let is_num = |w: &str| !w.is_empty() && w.chars().all(|c| digit(c).is_some());
    let keyword_at = |k: usize| -> bool {
        let Some((_, _, w)) = ws.get(k) else {
            return false;
        };
        if !CODE_KEYWORDS.contains(&w.as_str()) {
            return false;
        }
        if w == "code" || w == "codes" {
            let prev = k.checked_sub(1).and_then(|p| ws.get(p));
            if prev.is_some_and(|(_, _, p)| CODE_EXEMPT_BEFORE.contains(&p.as_str())) {
                return false;
            }
        }
        true
    };
    let mut k = 0usize;
    while k < ws.len() {
        let Some((s, e, w)) = ws.get(k) else {
            break;
        };
        let (s, mut e, mut len, mut last) = (*s, *e, w.chars().count(), k);
        if !is_num(w) {
            k += 1;
            continue;
        }
        // `123 456` / `123-456`
        if len == 3 {
            if let Some((s2, e2, w2)) = ws.get(k + 1) {
                let gap = chars.get(e..*s2).unwrap_or_default();
                if is_num(w2) && w2.chars().count() == 3 && matches!(gap, [' ' | '-']) {
                    e = *e2;
                    len = 6;
                    last = k + 1;
                }
            }
        }
        if (4..=8).contains(&len) && standalone_number(chars, s, e) {
            let lo = k.saturating_sub(5);
            let hi = last.saturating_add(5);
            if (lo..=hi).any(|j| j != k && j != last && keyword_at(j)) {
                spans.push((s, e, CODE_MASK));
            }
        }
        k = last + 1;
    }
}

/// A single word of 8–128 characters with lowercase, uppercase, digit and a symbol such as
/// `!#$%&*`, that is not an email address or URL.
fn is_password_like(text: &str) -> bool {
    let t = text.trim();
    let n = t.chars().count();
    if !(8..=128).contains(&n) || t.chars().any(char::is_whitespace) {
        return false;
    }
    if t.contains('@') || t.contains("://") || t.starts_with("www.") {
        return false;
    }
    let lower = t.chars().any(|c| c.is_lowercase());
    let upper = t.chars().any(|c| c.is_uppercase());
    let num = t.chars().any(|c| c.is_ascii_digit());
    // Punctuation common in ordinary words and ids (`iPhone15,2`, `Wi-Fi_2`) does not count.
    let sym = t
        .chars()
        .any(|c| !c.is_alphanumeric() && !matches!(c, ',' | '.' | '-' | '_' | '\'' | '/' | ':'));
    lower && upper && num && sym
}
