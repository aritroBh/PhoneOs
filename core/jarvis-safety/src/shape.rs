//! Fourteenth review: the shapes of values the gate reads — a link, money, a number, a
//! value that names someone, the owner's own words. Each is decided by what the value *is*,
//! never by what the page calls the field holding it.

use std::collections::HashSet;

use crate::text::{is_currency_symbol, CURRENCY_WORDS};
use crate::validate::url_scheme;

/// Schemes that make a link without `//` (`mailto:a@b`, `sms:+1555`): any of them, or a
/// scheme followed by `//`, makes a value link-shaped.
pub(crate) const BARE_LINK_SCHEMES: &[&str] = &[
    "mailto",
    "sms",
    "smsto",
    "mms",
    "imessage",
    "tel",
    "telprompt",
    "facetime",
    "facetime-audio",
    "javascript",
    "vbscript",
    "data",
    "file",
    "blob",
    "shortcuts",
    "prefs",
    "app-prefs",
    "itms-services",
    "itms-apps",
    "maps",
];

/// Rule A: whether `text` (trimmed) is a link — one word that has a scheme (followed by
/// `//`, or one of [`BARE_LINK_SCHEMES`]), or parses as `host[:port][/path][?query][#…]`
/// (no `user@` part without a scheme: `ada@example.com` is an e-mail address) with a
/// dot-separated host whose last label is a
/// plausible top-level domain (2–63 letters, or a punycode `xn--` label) or that is an IP
/// literal (four numbers up to 255, or a bracketed IPv6 address). `file.txt` and
/// `Mr.Smith` are link-shaped too (asked, never let through).
pub(crate) fn link_shaped(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.chars().any(char::is_whitespace) {
        return false;
    }
    if let Some(scheme) = url_scheme(t) {
        let rest = t.get(scheme.len().saturating_add(1)..).unwrap_or_default();
        if rest.starts_with("//") || BARE_LINK_SCHEMES.contains(&scheme.as_str()) {
            return true;
        }
    }
    let authority = t.split(['/', '?', '#']).next().unwrap_or_default();
    // Without a scheme, `name@host` is an e-mail address, not a link with a user part.
    if authority.contains('@') {
        return false;
    }
    let host = authority;
    if host.starts_with('[') {
        return host.find(']').is_some_and(|end| {
            host.get(1..end).is_some_and(|ip| {
                ip.contains(':')
                    && ip
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
            })
        });
    }
    let host = match host.rsplit_once(':') {
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
        _ => host,
    };
    let host = host.strip_suffix('.').unwrap_or(host);
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() < 2
        || labels.iter().any(|l| {
            l.is_empty()
                || !l
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        })
    {
        return false;
    }
    if labels.len() == 4
        && labels
            .iter()
            .all(|l| l.len() <= 3 && l.parse::<u16>().is_ok_and(|n| n <= 255))
    {
        return true;
    }
    let tld = labels.last().copied().unwrap_or_default();
    let n = tld.chars().count();
    tld.to_ascii_lowercase().starts_with("xn--")
        || ((2..=63).contains(&n) && tld.chars().all(char::is_alphabetic))
}

/// The word tokens of the owner's-words rule: maximal alphanumeric runs, case-folded.
fn word_tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// Most tokens of 1–2 letters (no digit) a text may hold unchecked ([`owner_words`]).
pub(crate) const MAX_SHORT_TOKENS: usize = 3;

/// Fifteenth review (R1, the typing gate): whether model-written `text` is the owner's own
/// words — the one test every egress of model text passes (typing, Return, a search, a
/// link, a tool's string arguments). Every token of `text` (a maximal alphanumeric run,
/// case-folded) occurs as a token of the owner's text (`owner`: the task's text typed or
/// spoken on the phone, the running skill's instructions and the arguments captured on the
/// phone, the profile values the owner did not mark sensitive), except tokens of 1–2
/// letters without a digit, of which at most [`MAX_SHORT_TOKENS`] are let through; a
/// token holding a digit is always compared, whatever its length (F7: `a4 b5` spells
/// digits). Every link in `text` ([`links_in`]) must also be one the owner gave
/// ([`crate::owner_gave_url`]). Empty `text` is the owner's (it sends nothing).
pub(crate) fn owner_words(text: &str, owner: &[&str]) -> bool {
    let mut own: Option<HashSet<String>> = None;
    let mut short = 0usize;
    for w in word_tokens(text) {
        let digit = w.chars().any(|c| c.is_numeric());
        if !digit && w.chars().count() <= 2 {
            short += 1;
            if short > MAX_SHORT_TOKENS {
                return false;
            }
            continue;
        }
        let own = own.get_or_insert_with(|| owner.iter().flat_map(|o| word_tokens(o)).collect());
        if !own.contains(&w) {
            return false;
        }
    }
    links_in(text)
        .iter()
        .all(|l| crate::evaluate::owner_gave_url(l, owner))
}

/// Fifteenth review (F6): the links inside `text` — the whole text when it is one
/// ([`link_shaped`]), and every piece of it split on whitespace and brackets and quotes,
/// with sentence punctuation at its end dropped, that is a link, or whose part after a
/// leading `label:` is (`URL:https://…`).
pub(crate) fn links_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let t = text.trim();
    if link_shaped(t) {
        out.push(t.to_owned());
    }
    let pieces = t.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '(' | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '<'
                    | '>'
                    | '"'
                    | '\''
                    | '`'
                    | '\u{201C}'
                    | '\u{201D}'
                    | '\u{2018}'
                    | '\u{2019}'
                    | '\u{00AB}'
                    | '\u{00BB}'
            )
    });
    for p in pieces {
        let p = p.trim_end_matches(['.', ',', ';', '!', '?', ':']);
        if p.is_empty() || out.iter().any(|l| l == p) {
            continue;
        }
        if link_shaped(p) {
            out.push(p.to_owned());
        } else if let Some((_, after)) = p.split_once(':') {
            let after = after.trim_start_matches(':');
            if link_shaped(after) {
                out.push(after.to_owned());
            }
        }
    }
    out
}

/// English number words (Rule D; other languages are a known gap).
const NUMBER_WORDS: &[&str] = &[
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
    "hundred",
    "thousand",
    "million",
    "billion",
];

/// Whether `w` (lowercase) is an English number word.
fn number_word(w: &str) -> bool {
    NUMBER_WORDS.contains(&w)
}

/// Whether `text` holds an English number word.
pub(crate) fn has_number_word(text: &str) -> bool {
    word_tokens(text).any(|w| number_word(&w))
}

/// A currency symbol: Unicode category Sc (the dollar, cent, pound, currency sign, yen, the
/// Currency Symbols block and the other Sc code points), and the CJK currency signs.
fn currency_sign(c: char) -> bool {
    is_currency_symbol(c)
        || matches!(
            c,
            '$' | '¢'..='¥'
                | '\u{058F}'
                | '\u{060B}'
                | '\u{07FE}'
                | '\u{07FF}'
                | '\u{09F2}'
                | '\u{09F3}'
                | '\u{09FB}'
                | '\u{0AF1}'
                | '\u{0BF9}'
                | '\u{0E3F}'
                | '\u{17DB}'
                | '\u{20A0}'..='\u{20CF}'
                | '\u{A838}'
                | '\u{FDFC}'
                | '\u{FE69}'
                | '\u{FF04}'
                | '\u{FFE0}'
                | '\u{FFE1}'
                | '\u{FFE5}'
                | '\u{FFE6}'
        )
}

/// Rule D: whether a piece next to a number is currency-like: it holds a currency sign
/// (`$`, `US$`, `R$`, `€`); or it is a known currency word or code (`EUR`, `dollars`,
/// `sats`); or it is 2–4 letters (any script; a trailing `.` dropped: `Fr.`) written all
/// in capitals (`CHF`, `USDC`) or all in small letters (`zł`, `kr`), or of two letters in
/// any case (`Kč`, `Rs`), or with the trailing `.`. A capitalised word of 3–4 letters
/// ("Code", "Room") is not.
fn currency_like(piece: &str) -> bool {
    if piece.chars().any(currency_sign) {
        return true;
    }
    let dotted = piece.ends_with('.');
    let word = piece.trim_end_matches('.');
    let n = word.chars().count();
    if word.is_empty() || !word.chars().all(char::is_alphabetic) {
        return false;
    }
    if CURRENCY_WORDS.contains(&word.to_lowercase().as_str()) {
        return true;
    }
    (2..=4).contains(&n)
        && (dotted
            || n == 2
            || word.chars().all(char::is_uppercase)
            || word.chars().all(char::is_lowercase))
}

/// The pieces of a value: number runs (digits with `,` `.` `'` between digits) and the
/// other runs of non-space characters, split where a number starts or ends (`US$900` →
/// `US$`, `900`).
fn pieces(text: &str) -> Vec<(bool, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<(bool, String)> = Vec::new();
    let mut i = 0usize;
    while let Some(&c) = chars.get(i) {
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let mut cur = String::new();
        if c.is_ascii_digit() {
            while let Some(&d) = chars.get(i) {
                let sep = matches!(d, ',' | '.' | '\'')
                    && chars.get(i + 1).is_some_and(char::is_ascii_digit);
                if d.is_ascii_digit() || sep {
                    cur.push(d);
                    i += 1;
                } else {
                    break;
                }
            }
            out.push((true, cur));
        } else {
            while let Some(&d) = chars.get(i) {
                if d.is_whitespace() || d.is_ascii_digit() {
                    break;
                }
                cur.push(d);
                i += 1;
            }
            out.push((false, cur));
        }
    }
    out
}

/// Strips a sign, a trailing period ("/ month", "per month") and a trailing `,-` / `.-`
/// (whether one was there) from a value.
fn strip_value(v: &str) -> (String, bool) {
    let t = v.trim();
    let t = t
        .strip_prefix(['-', '+', '\u{2212}'])
        .unwrap_or(t)
        .trim_start();
    let lower = t.to_owned();
    let body = match lower.rsplit_once('/') {
        Some((head, period))
            if crate::evaluate::PER_PERIOD_WORDS
                .contains(&period.trim().to_lowercase().as_str()) =>
        {
            head.trim_end().to_owned()
        }
        _ => {
            let words: Vec<&str> = lower.split_whitespace().collect();
            match words.as_slice() {
                [head @ .., per, p]
                    if matches!(per.to_lowercase().as_str(), "per" | "a")
                        && crate::evaluate::PER_PERIOD_WORDS
                            .contains(&p.to_lowercase().as_str()) =>
                {
                    head.join(" ")
                }
                _ => lower.clone(),
            }
        }
    };
    for tail in [",-", ".-", ",–", ".–", ",—", ".—"] {
        if let Some(b) = body.strip_suffix(tail) {
            return (b.trim_end().to_owned(), true);
        }
    }
    (body, false)
}

/// Rule D: whether `v` is money by its shape: a number (digit groups, or English number
/// words) with a currency-like piece next to it ([`currency_like`]: before or after,
/// attached or beside: `US$900`, `R$ 900,00`, `900 zł`, `CHF 900.-`, `50000 sats`, `0.5
/// BTC`, `nine hundred dollars`) or a trailing `,-` / `.-` (`900,-`), and nothing else.
pub(crate) fn money_shaped(v: &str) -> bool {
    let (body, dashed) = strip_value(v);
    let ps = pieces(&body);
    let mut numbers = 0usize;
    let mut currency = dashed;
    let mut words = false;
    let mut prev_number = false;
    for (number, p) in &ps {
        if *number {
            // Space-grouped thousands ("50 000") are one number.
            if !prev_number {
                numbers += 1;
            }
            prev_number = true;
            continue;
        }
        prev_number = false;
        let w = p.to_lowercase();
        if number_word(&w) || (w == "and" && words) {
            if !words {
                numbers += 1;
            }
            words = true;
            continue;
        }
        if currency_like(p) {
            currency = true;
        } else {
            return false;
        }
    }
    numbers == 1 && currency
}

/// Rule C: whether the gate cannot rule `v` out as an amount: a plain or grouped number
/// (with or without a currency), money by its shape, or English number words alone.
pub(crate) fn number_shaped(v: &str) -> bool {
    if crate::evaluate::money_or_number(v) || money_shaped(v) {
        return true;
    }
    let mut toks = word_tokens(v).peekable();
    toks.peek().is_some() && toks.all(|w| number_word(&w) || w == "and")
}

/// Month and weekday names, full and abbreviated (English), for [`date_shaped`].
const DATE_WORDS: &[&str] = &[
    "jan",
    "january",
    "feb",
    "february",
    "mar",
    "march",
    "apr",
    "april",
    "may",
    "jun",
    "june",
    "jul",
    "july",
    "aug",
    "august",
    "sep",
    "sept",
    "september",
    "oct",
    "october",
    "nov",
    "november",
    "dec",
    "december",
    "mon",
    "monday",
    "tue",
    "tues",
    "tuesday",
    "wed",
    "wednesday",
    "thu",
    "thur",
    "thurs",
    "thursday",
    "fri",
    "friday",
    "sat",
    "saturday",
    "sun",
    "sunday",
    "st",
    "nd",
    "rd",
    "th",
    "of",
];

/// Rule G: whether `v` is a date: two or three digit groups separated by `/` `-` `.`
/// (`12/23/2026`, `2026-12-23`, `23.12.26`), or words that are all month or weekday names
/// with numbers (`Nov 23, 2026`, `Monday 23rd`).
pub(crate) fn date_shaped(v: &str) -> bool {
    let t = v.trim();
    if t.is_empty() {
        return false;
    }
    let groups: Vec<&str> = t.split(['/', '-', '.']).map(str::trim).collect();
    if groups
        .iter()
        .all(|g| !g.is_empty() && g.chars().all(|c| c.is_ascii_digit()))
    {
        let sizes: Vec<usize> = groups.iter().map(|g| g.len()).collect();
        let day = |n: usize| (1..=2).contains(&n);
        let year = |n: usize| n == 2 || n == 4;
        let date = match sizes.as_slice() {
            // 23/12 (a slash only: 555-1234 is a phone number).
            [a, b] => day(*a) && day(*b) && t.contains('/'),
            [a, b, c] => (day(*a) && day(*b) && year(*c)) || (*a == 4 && day(*b) && day(*c)),
            _ => false,
        };
        if date {
            return true;
        }
    }
    let mut named = false;
    for w in word_tokens(t) {
        if w.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let w = w.trim_start_matches(|c: char| c.is_ascii_digit());
        if DATE_WORDS.contains(&w) {
            named |= w.len() >= 3;
            continue;
        }
        return false;
    }
    named
}

/// Rule G: whether a recipient field's value names someone: not a date, and it has a
/// letter (a name, an email, a handle, an IBAN) or is a phone or account number (7–34
/// digits, with only spaces, `-`, `.`, `(`, `)` and a leading `+` besides).
pub(crate) fn names_someone(v: &str) -> bool {
    let t = v.trim();
    if t.is_empty() || date_shaped(t) {
        return false;
    }
    if t.chars().any(char::is_alphabetic) {
        return true;
    }
    let body = t.strip_prefix('+').unwrap_or(t);
    let digits = body.chars().filter(char::is_ascii_digit).count();
    (7..=34).contains(&digits)
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '.' | '(' | ')'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        for yes in [
            "https://a.example",
            "collector.example/c?d=1",
            "bank.example.",
            "192.168.0.1:8080/x",
            "[2001:db8::1]/c",
            "mailto:a@b.example",
            "https://user@host.example/x",
            "xn--bnk-sna.example",
        ] {
            assert!(link_shaped(yes), "{yes}");
        }
        for no in [
            "weather paris",
            "3.14",
            "e.g.",
            "hello",
            "Note: hi",
            "a.b",
            "ada@example.com",
            "user@host.example/x",
        ] {
            assert!(!link_shaped(no), "{no}");
        }
    }

    #[test]
    fn money() {
        for yes in [
            "US$900",
            "R$ 900,00",
            "900 zł",
            "CHF 900.-",
            "900,-",
            "Fr. 900",
            "₹ 9,00,000",
            "900 kr",
            "50000 sats",
            "900 USDC",
            "0.5 BTC",
            "nine hundred dollars",
            "900 €",
            "$ 900",
            "¥90000",
            "$9.99/mo",
        ] {
            assert!(money_shaped(yes), "{yes}");
        }
        for no in [
            "900",
            "Code 482913",
            "My home address is 1 Elm St",
            "nine hundred",
            "Room 12",
        ] {
            assert!(!money_shaped(no), "{no}");
        }
        assert!(number_shaped("nine hundred"));
        assert!(number_shaped("900"));
        assert!(!number_shaped("Code 482913"));
    }

    #[test]
    fn someone() {
        for yes in [
            "Mallory",
            "bob@example.com",
            "@alex",
            "+1 415 555 0100",
            "DE89 3704 0044 0532 0130 00",
        ] {
            assert!(names_someone(yes), "{yes}");
        }
        for no in ["12/23/2026", "2026-12-23", "Nov 23, 2026", "900", ""] {
            assert!(!names_someone(no), "{no}");
        }
    }

    #[test]
    fn owner() {
        let o = ["What's the weather in Paris"];
        assert!(owner_words("weather Paris", &o));
        assert!(owner_words("Paris in", &o));
        assert!(!owner_words("Paris weather today", &o));
        assert!(!owner_words("paris 4521", &o));
        // Fifteenth review (F7): a token with a digit is compared whatever its length; at
        // most three 1–2 letter tokens go unchecked.
        assert!(!owner_words("a4 b5", &o));
        assert!(!owner_words("4a", &o));
        assert!(owner_words("Paris is ok", &o));
        assert!(!owner_words("Paris is ok in my st", &o));
        // A link must be one the owner gave, wherever it is in the text.
        let l = ["Open https://a.example/x and read it"];
        assert!(owner_words("https://a.example/x", &l));
        assert!(!owner_words("read https://a.example/x/", &l));
    }

    #[test]
    fn links_inside_text() {
        let has = |t: &str, l: &str| links_in(t).iter().any(|x| x == l);
        assert!(has(
            "read later collector.example/c?d=1",
            "collector.example/c?d=1"
        ));
        assert!(has("<https://c.example/a>", "https://c.example/a"));
        assert!(has("URL:https://c.example/a", "https://c.example/a"));
        assert!(has("(https://c.example/a).", "https://c.example/a"));
        assert!(links_in("write to ada@example.com please").is_empty());
        assert!(links_in("Dinner at 8").is_empty());
    }
}
