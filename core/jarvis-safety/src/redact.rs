//! Redaction of secrets before text reaches a prompt or a log.
//!
//! Detectors, all hand-written:
//!
//! * **Card numbers**: 13–19 digits, optionally grouped by one or two spaces (incl. NBSP,
//!   thin and figure spaces), dashes or dots, that pass the Luhn check. Inside a longer
//!   grouped run, any run of whole groups that qualifies.
//! * **IBANs**: country letters, check digits and body, validated with mod-97.
//! * **One-time codes**: a standalone 4–8 digit number (or `123 456`, `48 29 13`,
//!   `123-456`) in the same sentence as `code`, `OTP`, `verification`, `passcode`,
//!   `password`, `PIN`, `login`, `sign in`... (`zip code`, `tracking code` and similar do
//!   not count; numbers right after `Order`, `Flight`... and phone numbers are kept).
//! * **API keys and tokens**: well-known prefixes (`sk-`, `ghp_`, `xoxb-`, `AKIA`, `AIza`,
//!   JWTs...), base64 secrets with `/`/`+` (AWS), and long high-entropy strings.
//! * **Labelled secrets**: the value after `password:`, `token=`, `Bearer `,
//!   `aws_secret_access_key=`, and the token after `password is`, `CVV`, `PIN`... when it
//!   is not a plain word.
//! * **Seed phrases**: 12+ consecutive short lowercase words with few chat words among
//!   them (heuristic).
//!
//! Phone numbers, dates, times, prices and ordinary numbers are left alone. Invisible
//! characters are dropped first so they cannot split a secret.

use jarvis_proto::{CompactScreen, Element, RiskCategory, Role};

use crate::phrases::{match_label, CREDENTIAL_FIELD_PHRASES, HIDDEN_FIELD_PHRASES};
use crate::text::{fold_fullwidth, is_invisible};

/// Replacement for a card number.
pub const CARD_MASK: &str = "[redacted card]";
/// Replacement for a one-time code.
pub const CODE_MASK: &str = "[redacted number]";
/// Replacement for an API key or token.
pub const KEY_MASK: &str = "[redacted key]";
/// Replacement for a labelled secret, a seed phrase or a secure field's value.
pub const SECRET_MASK: &str = "[redacted secret]";
/// Replacement for a bank account number (IBAN).
pub const ACCOUNT_MASK: &str = "[redacted account]";

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

/// Whether `el` is an entry field (a text input, or a picker, slider or other value
/// control) whose label or placeholder names a credential or a bank or card detail
/// (password, passcode, PIN, SSN, card number, CVV/CVC, security code, expiry,
/// recovery/seed phrase, one-time/verification code, IBAN, routing or account number...).
/// Its value is never shown: not in prompts ([`redact_element`]) and not in the owner's
/// summaries (listed by name, "(hidden)"). An identifier only counts where compaction used
/// it as the label.
pub fn credential_like_field(el: &Element) -> bool {
    let entry = el.role.is_text_input()
        || matches!(el.role, Role::Picker | Role::Slider)
        || matches!(&el.role, Role::Other(n) if {
            let n = n.to_lowercase();
            ["stepper", "slider", "picker", "segmented", "field"]
                .iter()
                .any(|k| n.contains(k))
        });
    let tables = [CREDENTIAL_FIELD_PHRASES, HIDDEN_FIELD_PHRASES];
    let named = |t: &str| match_label(t, &tables).category != RiskCategory::None;
    entry && (named(&el.label) || el.placeholder.as_deref().is_some_and(named))
}

/// A copy of `el` safe for prompts and logs: secure fields lose their value, a field
/// named as a credential or bank/card detail ("Verification code", "CVV", "IBAN"; see
/// [`credential_like_field`]) has its value masked, and label, value and placeholder are
/// passed through [`redact`].
pub fn redact_element(el: &Element) -> Element {
    let mut out = el.clone();
    out.label = redact(&el.label);
    out.placeholder = el.placeholder.as_deref().map(redact);
    let credential_field = credential_like_field(el);
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
    iban_spans(chars, &mut spans);
    code_spans(chars, &mut spans);
    labelled_secret_spans(chars, &mut spans);
    key_spans(chars, &mut spans);
    seed_spans(chars, &mut spans);
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
            let is_sep = |c: Option<char>| {
                matches!(
                    c,
                    Some(' ' | '-' | '.' | '\u{a0}' | '\u{2007}' | '\u{2009}' | '\u{202F}')
                )
            };
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
    "aws_secret_access_key",
    "aws_access_key_id",
    "aws_session_token",
    "secret_access_key",
    "secret_key",
    "private_key",
];

/// Words whose following token is a secret even without `:` (`password is hunter2`,
/// `CVV 123`, `Wi-Fi password hunter22`).
const VALUE_KEYWORDS: &[&str] = &[
    "password", "passcode", "passwd", "pwd", "pin", "cvv", "cvc", "cvv2",
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
        } else if SECRET_KEYS.contains(&word.as_str()) || VALUE_KEYWORDS.contains(&word.as_str()) {
            while at(chars, k) == Some(' ') {
                k += 1;
            }
            if matches!(at(chars, k), Some(':' | '=')) {
                k += 1;
            } else if VALUE_KEYWORDS.contains(&word.as_str()) {
                // `password is X`, or a following token that is not a plain word.
                let ns = k;
                let mut ne = k;
                while at(chars, ne).is_some_and(|c| !c.is_whitespace()) {
                    ne += 1;
                }
                let next: String = chars.get(ns..ne).unwrap_or_default().iter().collect();
                let next_l = next.to_ascii_lowercase();
                if matches!(next_l.as_str(), "is" | "was" | "are") {
                    k = ne;
                } else if next.is_empty()
                    || next
                        .trim_start_matches('(')
                        .trim_end_matches(['.', ',', ';', '!', '?', ')'])
                        .chars()
                        .all(char::is_alphabetic)
                {
                    i = j;
                    continue;
                }
            } else {
                i = j;
                continue;
            }
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
        // Sentence punctuation after the value is not part of it.
        while k > vs && matches!(at(chars, k - 1), Some('.' | '!' | '?' | ')')) {
            k -= 1;
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
    // Standard base64 (AWS secret keys contain `/` and `+`): judged whole.
    let b64 = tok.len() >= 30
        && (tok.contains('/') || tok.contains('+'))
        && !tok.contains("//")
        && !tok.starts_with('/')
        && tok
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='));
    if b64 {
        let core: String = tok.chars().filter(char::is_ascii_alphanumeric).collect();
        let mixed = core.bytes().any(|b| b.is_ascii_digit())
            && core.bytes().any(|b| b.is_ascii_lowercase())
            && core.bytes().any(|b| b.is_ascii_uppercase());
        if mixed && entropy(&core) >= 4.0 {
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
    "password",
    "passwd",
    "verification",
    "verify",
    "pin",
    "2fa",
    "authentication",
    "auth",
    "one-time",
    "onetime",
    "login",
    "signin",
];

/// Words that, right before `code`, make it something other than a one-time code.
const CODE_EXEMPT_BEFORE: &[&str] = &[
    "zip",
    "postal",
    "post",
    "area",
    "country",
    "promo",
    "coupon",
    "discount",
    "qr",
    "bar",
    "source",
    "error",
    "status",
    "dress",
    "morse",
    "dial",
    "sort",
    "tax",
    "product",
    "style",
    "tracking",
    "gift",
    "referral",
    "invite",
    "invitation",
];

/// Words that, right before a number, say it is an identifier, not a code.
const NUMBER_LABELS: &[&str] = &[
    "order",
    "invoice",
    "ref",
    "reference",
    "account",
    "acct",
    "booking",
    "ticket",
    "flight",
    "room",
    "tracking",
    "case",
    "member",
    "customer",
    "policy",
    "serial",
    "model",
    "no",
    "number",
    "id",
    "item",
    "version",
    "build",
    "page",
    "step",
    "chapter",
    "apt",
    "suite",
    "unit",
    "route",
    "bus",
    "train",
    "gate",
    "seat",
    "table",
    "year",
];

/// Most words between a code and its keyword (within one sentence).
const CODE_WINDOW: usize = 30;

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

/// Sentence number of every word: a new sentence starts after `!`, `?`, `;`, a newline,
/// or a `.` followed by whitespace.
fn sentences(chars: &[char], ws: &[Word]) -> Vec<usize> {
    let mut out = Vec::with_capacity(ws.len());
    let mut n = 0usize;
    let mut prev_end = 0usize;
    for (s, e, _) in ws {
        let gap = chars.get(prev_end..*s).unwrap_or_default();
        let ends = gap.iter().any(|c| matches!(c, '!' | '?' | ';' | '\n'))
            || (gap.contains(&'.') && gap.iter().any(|c| c.is_whitespace()));
        if ends && !out.is_empty() {
            n += 1;
        }
        out.push(n);
        prev_end = *e;
    }
    out
}

/// Whether the digits at `start..end` stand alone (not part of a price, decimal, time,
/// date, percentage, copyright year, phone number or longer number).
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
    let marker = |c: Option<char>| {
        c.is_some_and(|c| {
            matches!(
                fold_fullwidth(c),
                '$' | '€' | '£' | '¥' | '₹' | '#' | '+' | '©' | '®' | ')' | '№'
            )
        })
    };
    if marker(before) || (before == Some(' ') && marker(before2)) {
        return false;
    }
    !matches!(after, Some('%'))
}

/// One-time codes near a keyword in the same sentence.
///
/// Digit groups joined by single spaces or dashes form one cluster (`48 29 13`,
/// `123-456`); a cluster is a code candidate with 4–8 digits in total. Longer clusters,
/// clusters starting with `+` (phone numbers), dash-joined clusters of three or more groups
/// (dates, phone numbers) and numbers right after an identifier word (`Order 12345678`)
/// are not.
fn code_spans(chars: &[char], spans: &mut Vec<(usize, usize, &'static str)>) {
    let ws = words(chars);
    let sent = sentences(chars, &ws);
    let is_num = |w: &str| !w.is_empty() && w.chars().all(|c| digit(c).is_some());
    let word = |k: usize| ws.get(k).map(|(_, _, w)| w.as_str()).unwrap_or("");
    let keyword_at = |k: usize| -> bool {
        let w = word(k);
        let next = word(k + 1);
        if (w == "sign" || w == "log") && (next == "in" || next == "on") {
            return true;
        }
        if !CODE_KEYWORDS.contains(&w) {
            return false;
        }
        if w == "code" || w == "codes" {
            let prev = k.checked_sub(1).map(word).unwrap_or("");
            if CODE_EXEMPT_BEFORE.contains(&prev) {
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
        if !is_num(w) {
            k += 1;
            continue;
        }
        let (s, mut e, mut last) = (*s, *e, k);
        let mut digits = w.chars().count();
        let mut groups = 1usize;
        let mut dashed = false;
        while let Some((s2, e2, w2)) = ws.get(last + 1) {
            let gap = chars.get(e..*s2).unwrap_or_default();
            let joined = matches!(gap, [' ' | '-' | '\u{a0}' | '\u{2009}' | '\u{202F}']);
            if !(is_num(w2) && joined) {
                break;
            }
            dashed |= gap == ['-'];
            digits += w2.chars().count();
            groups += 1;
            e = *e2;
            last += 1;
        }
        let before = s.checked_sub(1).and_then(|i| at(chars, i));
        let candidate = (4..=8).contains(&digits)
            && before != Some('+')
            && !(dashed && groups > 2)
            && standalone_number(chars, s, e)
            && !k
                .checked_sub(1)
                .is_some_and(|p| NUMBER_LABELS.contains(&word(p)));
        if candidate {
            let here = sent.get(k).copied();
            let lo = k.saturating_sub(CODE_WINDOW);
            let hi = last.saturating_add(CODE_WINDOW);
            let near = (lo..=hi)
                .filter(|j| *j < k || *j > last)
                .any(|j| sent.get(j).copied() == here && keyword_at(j));
            if near {
                spans.push((s, e, CODE_MASK));
            }
        }
        k = last + 1;
    }
}

/// Words that rarely occur in a seed phrase but often in lowercase chat.
const STOPWORDS: &[&str] = &[
    "the", "and", "you", "are", "for", "with", "that", "this", "have", "was", "but", "not", "our",
    "your", "can", "will", "just", "its", "his", "her", "they", "she", "from", "what", "when",
    "how", "been", "hey", "lol", "yes", "yeah", "okay", "did", "dont", "cant", "im", "its", "too",
    "very", "then", "there", "here", "some", "any", "like", "want", "wants", "going", "got", "get",
    "see", "let", "know", "think", "tonight", "today", "tomorrow",
];

/// Seed phrases: 12 or more consecutive lowercase ASCII words of 3–8 letters separated by
/// single spaces, with few common chat words among them.
fn seed_spans(chars: &[char], spans: &mut Vec<(usize, usize, &'static str)>) {
    let ws = words(chars);
    let seedish = |w: &str| (3..=8).contains(&w.len()) && w.bytes().all(|b| b.is_ascii_lowercase());
    let mut k = 0usize;
    while k < ws.len() {
        let mut last = k;
        let first_ok = ws.get(k).is_some_and(|(s, e, w)| {
            seedish(w)
                && chars
                    .get(*s..*e)
                    .is_some_and(|raw| raw.iter().all(char::is_ascii_lowercase))
        });
        if !first_ok {
            k += 1;
            continue;
        }
        while let (Some((_, e1, _)), Some((s2, e2, w2))) = (ws.get(last), ws.get(last + 1)) {
            let gap_ok = chars.get(*e1..*s2) == Some(&[' '][..]);
            let raw_ok = chars
                .get(*s2..*e2)
                .is_some_and(|raw| raw.iter().all(char::is_ascii_lowercase));
            if !(gap_ok && raw_ok && seedish(w2)) {
                break;
            }
            last += 1;
        }
        let n = last + 1 - k;
        let stop = (k..=last)
            .filter(|j| {
                ws.get(*j)
                    .is_some_and(|(_, _, w)| STOPWORDS.contains(&w.as_str()))
            })
            .count();
        if n >= 12 && stop * 6 <= n {
            if let (Some((s, _, _)), Some((_, e, _))) = (ws.get(k), ws.get(last)) {
                spans.push((*s, *e, SECRET_MASK));
            }
        }
        k = last + 1;
    }
}

/// IBAN check: ISO 13616 mod-97 over the rearranged, letter-expanded string.
fn iban_valid(compact: &[char]) -> bool {
    if !(15..=34).contains(&compact.len()) {
        return false;
    }
    let mut rem = 0u32;
    for c in compact.iter().skip(4).chain(compact.iter().take(4)) {
        let v = match c.to_digit(36) {
            Some(v) => v,
            None => return false,
        };
        rem = if v < 10 {
            (rem * 10 + v) % 97
        } else {
            (rem * 100 + v) % 97
        };
    }
    rem == 1
}

/// IBANs: two letters, two digits, then up to 30 letters/digits in optional groups.
fn iban_spans(chars: &[char], spans: &mut Vec<(usize, usize, &'static str)>) {
    let up_or_digit =
        |c: Option<char>| c.is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    let mut i = 0usize;
    while i + 4 <= chars.len() {
        let head = at(chars, i).is_some_and(|c| c.is_ascii_uppercase())
            && at(chars, i + 1).is_some_and(|c| c.is_ascii_uppercase())
            && at(chars, i + 2).is_some_and(|c| c.is_ascii_digit())
            && at(chars, i + 3).is_some_and(|c| c.is_ascii_digit())
            && (i == 0 || !at(chars, i - 1).is_some_and(char::is_alphanumeric));
        if !head {
            i += 1;
            continue;
        }
        let mut compact: Vec<char> = chars.get(i..i + 4).unwrap_or_default().to_vec();
        // (compact length, end index) at each group boundary
        let mut ends: Vec<(usize, usize)> = Vec::new();
        let mut j = i + 4;
        loop {
            while up_or_digit(at(chars, j)) && compact.len() < 34 {
                if let Some(c) = at(chars, j) {
                    compact.push(c);
                }
                j += 1;
            }
            ends.push((compact.len(), j));
            if at(chars, j) == Some(' ') && up_or_digit(at(chars, j + 1)) && compact.len() < 34 {
                j += 1;
            } else {
                break;
            }
        }
        let found = ends
            .iter()
            .rev()
            .find(|(n, _)| compact.get(..*n).is_some_and(iban_valid));
        match found {
            Some(&(_, end)) => {
                spans.push((i, end, ACCOUNT_MASK));
                i = end;
            }
            None => i += 1,
        }
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
