//! Fourteenth review: the shapes of values the gate reads — a link, money, a number, a
//! value that names someone, the owner's own words. Each is decided by what the value *is*,
//! never by what the page calls the field holding it.

use std::collections::HashMap;

use crate::text::{fold_digits, is_currency_symbol, CURRENCY_WORDS};
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

/// The word tokens of number words and dates: maximal alphanumeric runs, case-folded.
fn word_tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// Function words (sixteenth review, F5/F6) a model-written text may use although the owner
/// did not write them: each at most once per text, at most [`MAX_FREE_WORDS`] of them in
/// all, written in small letters or with the first letter capitalised ([`owner_words`]).
/// Nothing else of one or two letters passes without the owner's text.
pub(crate) const FUNCTION_WORDS: &[&str] = &[
    "a", "an", "as", "at", "be", "by", "do", "go", "he", "hi", "i", "if", "in", "is", "it", "me",
    "my", "no", "of", "ok", "on", "or", "so", "to", "up", "us", "we",
];

/// Most function words not written by the owner one text may hold.
pub(crate) const MAX_FREE_WORDS: usize = 3;

/// Most characters of sentence punctuation not written by the owner one text may hold.
pub(crate) const MAX_FREE_PUNCT: usize = 3;

/// Format characters the shared invisible set ([`crate::text::is_invisible`]) does not list:
/// Arabic number signs and marks, the Syriac abbreviation mark, Kaithi and Brahmi number
/// joiners, Egyptian hieroglyph format controls.
fn format_char(c: char) -> bool {
    crate::text::is_invisible(c)
        || matches!(
            u32::from(c),
            0x0600..=0x0605
                | 0x06DD
                | 0x070F
                | 0x0890
                | 0x0891
                | 0x08E2
                | 0x110BD
                | 0x110CD
                | 0x13430..=0x1343F
        )
}

/// One piece of a whitespace-separated run: an alphanumeric token (as written, digits of any
/// script folded to ASCII, an apostrophe between two alphanumerics dropped) or a run of
/// other characters.
enum Piece {
    Word(String),
    Sym(String),
}

/// Splits one run (no whitespace) into [`Piece`]s.
fn pieces_of_run(run: &str) -> Vec<Piece> {
    let chars: Vec<char> = run.chars().map(crate::text::fold_digit).collect();
    let mut out: Vec<Piece> = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_alphanumeric() {
            match out.last_mut() {
                Some(Piece::Word(w)) => w.push(c),
                _ => out.push(Piece::Word(c.to_string())),
            }
        } else if crate::text::is_apostrophe(c)
            && matches!(out.last(), Some(Piece::Word(_)))
            && chars.get(i + 1).is_some_and(|n| n.is_alphanumeric())
        {
            // `don't`, `O'Brien`: one word.
        } else {
            match out.last_mut() {
                Some(Piece::Sym(w)) => w.push(c),
                _ => out.push(Piece::Sym(c.to_string())),
            }
        }
    }
    out
}

/// Whether `c` separates runs of a model-written text: space, tab, newline, carriage return.
/// Any other whitespace (no-break space, ideographic space, line separator …) makes a text
/// not the owner's.
fn run_break(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// `lower` with its first letter capitalised.
fn capitalised(lower: &str) -> String {
    let mut it = lower.chars();
    match it.next() {
        Some(f) => f.to_uppercase().chain(it).collect(),
        None => String::new(),
    }
}

/// The owner's tokens (lowercase key → how often written, the spellings written) and symbol
/// runs (→ how often written), summed over the owner's texts.
#[derive(Default)]
struct OwnerText {
    words: HashMap<String, (usize, Vec<String>)>,
    symbols: HashMap<String, usize>,
}

impl OwnerText {
    fn new(owner: &[&str]) -> Self {
        let mut me = Self::default();
        for text in owner {
            for run in text.split(char::is_whitespace).filter(|r| !r.is_empty()) {
                for piece in pieces_of_run(run) {
                    match piece {
                        Piece::Word(w) => {
                            let e = me.words.entry(w.to_lowercase()).or_default();
                            e.0 += 1;
                            if !e.1.contains(&w) {
                                e.1.push(w);
                            }
                        }
                        Piece::Sym(y) => *me.symbols.entry(y).or_default() += 1,
                    }
                }
            }
        }
        me
    }
}

/// Where a symbol run sits in its whitespace-separated run.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Alone,
    Leading,
    Inner,
    Trailing,
}

/// Whether `y` is ordinary sentence punctuation at `place`: at most 2 characters, leading
/// `( [ " ' “ ‘ «`; trailing `. , ; : ! ? ) ] " ' ” ’ »`; inside a word only `-`; never alone.
fn sentence_punctuation(y: &str, place: Place) -> bool {
    let ok = |c: char| match place {
        Place::Leading => matches!(
            c,
            '(' | '[' | '"' | '\'' | '\u{201C}' | '\u{2018}' | '\u{00AB}'
        ),
        Place::Trailing => matches!(
            c,
            '.' | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | ')'
                | ']'
                | '"'
                | '\''
                | '\u{201D}'
                | '\u{2019}'
                | '\u{00BB}'
        ),
        Place::Inner => c == '-',
        Place::Alone => false,
    };
    y.chars().count() <= 2 && y.chars().all(ok)
}

/// Sixteenth review (F1, F5, F6), fifteenth review's R1 (the typing gate): whether
/// model-written `text` is the owner's own words, the one test every egress of model text
/// passes (typing, Return, a search, a link, a tool's arguments). `owner` is the task's text
/// typed or spoken on the phone, the running skill's instructions and the arguments captured
/// on the phone, and the profile values the owner did not mark sensitive.
///
/// **Every character of `text` is accounted for.** In order:
/// 1. Invisible and format characters (zero-width, joiners, bidi marks, variation selectors,
///    tag characters, controls but tab, newline and carriage return), and any whitespace
///    other than space, tab, newline and carriage return, make it not the owner's.
/// 2. `text` is split on those whitespace characters into runs. In a run, a *token* is a
///    maximal run of letters and digits (digits of every script are folded to ASCII first;
///    an apostrophe between two alphanumerics joins them: `O'Brien`, `don't`); a *symbol
///    run* is a maximal run of the other characters.
/// 3. A token passes when the owner wrote it, counted as a multiset: it is used at most as
///    often as the owner wrote it (compared case-folded, summed over the owner texts), and
///    is spelled as the owner spelled it, all in small letters, or with only its first
///    letter capitalised (`SIGN`, `sIgN` are not the owner's unless the owner wrote them so).
///    Otherwise it passes only as one of [`FUNCTION_WORDS`] (small letters or first letter
///    capitalised), each at most once per text and at most [`MAX_FREE_WORDS`] in all. There is
///    no other allowance: a token of one or two letters is compared like any other (`ei cj
///    bd` spells digits).
/// 4. A symbol run passes when the owner wrote the same contiguous run (counted as a
///    multiset: `$`, `@`, `://`, the colon of `5:30`); otherwise only as ordinary sentence
///    punctuation ([`sentence_punctuation`]: at most 2 characters, attached to a word, in an
///    ordinary position), at most [`MAX_FREE_PUNCT`] characters of it in all. A run of
///    symbols only (`$*@(!#`, Morse, emoji) passes only when the owner wrote that exact run.
/// 5. Every link in `text` ([`links_in`]) is one the owner gave ([`crate::owner_gave_url`]).
///
/// The counts are per text. What a session types over several actions (an owner token and a
/// few function words and marks per action) is covered by the session's cross-app flow rule
/// (sixteenth review, Part A), not here; choosing and ordering the owner's own words stays a
/// known gap, bounded by the counts. Empty `text` is the owner's (it sends nothing).
pub(crate) fn owner_words(text: &str, owner: &[&str]) -> bool {
    if text.is_empty() {
        return true;
    }
    if text
        .chars()
        .any(|c| format_char(c) || (c.is_whitespace() && !run_break(c)))
    {
        return false;
    }
    let mut own: Option<OwnerText> = None;
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut free_words: Vec<String> = Vec::new();
    let mut free_punct = 0usize;
    for run in text.split(run_break).filter(|r| !r.is_empty()) {
        let pieces = pieces_of_run(run);
        let has_word = pieces.iter().any(|p| matches!(p, Piece::Word(_)));
        let last = pieces.len().saturating_sub(1);
        for (k, piece) in pieces.into_iter().enumerate() {
            let own = own.get_or_insert_with(|| OwnerText::new(owner));
            match piece {
                Piece::Word(w) => {
                    let key = w.to_lowercase();
                    let plain = w == key || w == capitalised(&key);
                    let owned = own.words.get(&key).is_some_and(|(n, spellings)| {
                        (plain || spellings.contains(&w))
                            && used.get(&key).copied().unwrap_or(0) < *n
                    });
                    if owned {
                        *used.entry(key).or_default() += 1;
                    } else if plain
                        && FUNCTION_WORDS.contains(&key.as_str())
                        && free_words.len() < MAX_FREE_WORDS
                        && !free_words.contains(&key)
                    {
                        free_words.push(key);
                    } else {
                        return false;
                    }
                }
                Piece::Sym(y) => {
                    let place = if !has_word {
                        Place::Alone
                    } else if k == 0 {
                        Place::Leading
                    } else if k == last {
                        Place::Trailing
                    } else {
                        Place::Inner
                    };
                    let key = format!("\u{0}{y}");
                    let n = own.symbols.get(&y).copied().unwrap_or(0);
                    if used.get(&key).copied().unwrap_or(0) < n {
                        *used.entry(key).or_default() += 1;
                    } else if sentence_punctuation(&y, place)
                        && free_punct + y.chars().count() <= MAX_FREE_PUNCT
                    {
                        free_punct += y.chars().count();
                    } else {
                        return false;
                    }
                }
            }
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

/// Sixteenth review (F4): a dotted abbreviation in a non-ASCII script (`د.إ`, `ر.س`, `դր.`):
/// letters separated by dots, two to four letters, at least one not ASCII. The way Arabic
/// and Armenian apps write a currency beside a number; ASCII ones (`a.m`, `e.g`) are not
/// taken.
pub(crate) fn dotted_abbreviation(w: &str) -> bool {
    let w = w.trim_end_matches('.');
    let letters = w.chars().filter(|c| c.is_alphabetic()).count();
    (2..=4).contains(&letters)
        && w.contains('.')
        && w.chars().all(|c| c.is_alphabetic() || c == '.')
        && !w.starts_with('.')
        && !w.contains("..")
        && !w.is_ascii()
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
    if dotted_abbreviation(piece) {
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
    let v = fold_digits(v);
    let (body, dashed) = strip_value(&v);
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

/// Sixteenth review (F4): whether `v` is a number with one non-ASCII letter or symbol
/// attached directly to it and nothing else (`Ξ0.5`, `0.5Ξ`, `₿2`): the shape of an amount in a
/// currency the gate's lists do not know (ether, a new token or coin sign, a local symbol).
/// Used only where a send or payment is judged ([`crate::evaluate`]'s read of the screen for
/// money), never as money in general: one letter next to a number is also a unit (`5Ω`), which
/// costs a question only on a screen that sends. A space or a second character is not this
/// shape.
pub(crate) fn symbol_amount(v: &str) -> bool {
    let v = fold_digits(v);
    let v = v.trim();
    let mark = |c: char| {
        !c.is_ascii()
            && (c.is_alphabetic()
                || is_currency_symbol(c)
                || matches!(u32::from(c), 0x2100..=0x214F))
    };
    let number = |n: &str| {
        n.chars().next().is_some_and(|c| c.is_ascii_digit())
            && n.chars().last().is_some_and(|c| c.is_ascii_digit())
            && n.chars()
                .all(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '\''))
    };
    let mut it = v.chars();
    let (first, last) = (it.next(), it.next_back());
    match (first, last) {
        (Some(f), _) if mark(f) => v.get(f.len_utf8()..).is_some_and(number),
        (_, Some(l)) if mark(l) => v.get(..v.len() - l.len_utf8()).is_some_and(number),
        _ => false,
    }
}

/// Rule C: whether the gate cannot rule `v` out as an amount: a plain or grouped number
/// (with or without a currency), money by its shape, or English number words alone.
pub(crate) fn number_shaped(v: &str) -> bool {
    let v = &*fold_digits(v);
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
    let v = fold_digits(v);
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
    let v = fold_digits(v);
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
