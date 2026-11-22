//! Character classes and the word tokenizer shared by classification, sanitising and
//! redaction.
//!
//! Matching works on *tokens*: lowercase alphanumeric runs after invisible characters are
//! dropped, look-alike letters are folded to ASCII and apostrophes are removed. Comparing
//! whole tokens gives word-boundary matching without regular expressions.

/// True for characters that render as nothing (or only reorder text) and so can be used to
/// split a keyword without changing how it looks: zero-width and joiner characters, bidi
/// controls, soft hyphen, variation selectors, Hangul fillers, Unicode tag characters,
/// interlinear annotation marks and non-whitespace C0/C1 controls.
pub(crate) fn is_invisible(c: char) -> bool {
    let u = u32::from(c);
    if c.is_control() {
        // Whitespace controls (tab, newline...) are handled as whitespace by callers.
        return !c.is_whitespace();
    }
    matches!(
        u,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F
            | 0x1160
            | 0x17B4
            | 0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0xFFF0..=0xFFFB
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

/// True for combining marks, which are dropped before matching so `P\u{301}ay` reads `pay`.
fn is_combining(c: char) -> bool {
    matches!(
        u32::from(c),
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F
    )
}

/// Folds fullwidth ASCII (U+FF01..U+FF5E) and the ideographic space to plain ASCII.
pub(crate) fn fold_fullwidth(c: char) -> char {
    let u = u32::from(c);
    match u {
        0xFF01..=0xFF5E => char::from_u32(u - 0xFF01 + 0x21).unwrap_or(c),
        0x3000 => ' ',
        _ => c,
    }
}

/// Maps a letter from Unicode's styled alphabets (mathematical bold/italic/script...,
/// circled letters) back to ASCII.
fn fold_styled(c: char) -> char {
    let u = u32::from(c);
    let letter = |i: u32| -> char {
        let base = if i < 26 { 0x41 + i } else { 0x61 + (i - 26) };
        char::from_u32(base).unwrap_or(c)
    };
    match u {
        0x1D400..=0x1D6A3 => letter((u - 0x1D400) % 52),
        0x1D7CE..=0x1D7FF => char::from_u32(0x30 + (u - 0x1D7CE) % 10).unwrap_or(c),
        0x24B6..=0x24CF => letter(u - 0x24B6),
        0x24D0..=0x24E9 => letter(26 + (u - 0x24D0)),
        _ => c,
    }
}

/// Folds a lowercase character that looks like an ASCII letter (Cyrillic, Greek, accented
/// Latin) to that letter. Applied after lowercasing.
fn fold_confusable(c: char) -> char {
    match c {
        'а' | 'α' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'в' | 'β' | 'ь' => 'b',
        'с' | 'ϲ' | 'ç' => 'c',
        'ԁ' => 'd',
        'е' | 'ё' | 'ε' | 'è' | 'é' | 'ê' | 'ë' | 'ē' => 'e',
        'ɡ' => 'g',
        'н' | 'һ' => 'h',
        'і' | 'ї' | 'ι' | 'ì' | 'í' | 'î' | 'ï' | 'ı' => 'i',
        'ј' => 'j',
        'к' | 'κ' => 'k',
        'м' => 'm',
        'η' | 'ñ' | 'п' => 'n',
        'о' | 'ο' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
        'р' | 'ρ' => 'p',
        'ԛ' => 'q',
        'г' => 'r',
        'ѕ' => 's',
        'т' | 'τ' => 't',
        'υ' | 'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
        'ν' => 'v',
        'ԝ' | 'ω' => 'w',
        'х' | 'χ' => 'x',
        'у' | 'ý' | 'ÿ' | 'γ' => 'y',
        _ => c,
    }
}

/// Folds one character for look-alike-insensitive comparison: fullwidth and styled forms
/// to ASCII, lowercase, then Cyrillic/Greek/accented look-alikes to ASCII letters.
pub(crate) fn fold_char(c: char) -> char {
    let c = fold_styled(fold_fullwidth(c));
    if let Some(a) = fold_upper_confusable(c) {
        return a;
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    fold_confusable(lower)
}

/// Folds an uppercase Greek or Cyrillic letter that looks like an ASCII capital to that
/// letter, lowercased. Checked before lowercasing, because some capitals look like a
/// different Latin letter than their lowercase forms do (Greek `Ν` is `N`, `ν` is `v`).
fn fold_upper_confusable(c: char) -> Option<char> {
    Some(match c {
        'Α' | 'А' => 'a',
        'Β' | 'В' => 'b',
        'С' | 'Ϲ' => 'c',
        'Ε' | 'Е' | 'Ё' => 'e',
        'Η' | 'Н' => 'h',
        'Ι' | 'І' | 'Ї' | 'Ӏ' => 'i',
        'Ј' => 'j',
        'Κ' | 'К' => 'k',
        'Μ' | 'М' => 'm',
        'Ν' => 'n',
        'Ο' | 'О' => 'o',
        'Ρ' | 'Р' => 'p',
        'Ѕ' => 's',
        'Τ' | 'Т' => 't',
        'Υ' | 'Ү' => 'y',
        'Χ' | 'Х' => 'x',
        'Ζ' => 'z',
        _ => return None,
    })
}

/// True for apostrophe-like characters, which are deleted inside words (`don't` → `dont`).
fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | '‘' | 'ʼ' | '`' | '´')
}

/// Icon characters used as button labels, read as the word they stand for.
fn emoji_word(c: char) -> Option<&'static str> {
    match c {
        '\u{1F5D1}' => Some("trash"),
        '\u{1F4B8}' => Some("pay"),
        '\u{1F4E4}' => Some("send"),
        _ => None,
    }
}

/// Splits `text` into lowercase, look-alike-folded word tokens.
///
/// Boundaries: any non-alphanumeric character, a lowercase→uppercase transition
/// (`PlaceOrder` → `place order`) and a letter↔digit transition (`Pay42` → `pay 42`).
/// Invisible characters, combining marks and apostrophes are removed without splitting;
/// a few icon emoji (wastebasket, money, outbox) read as words.
pub(crate) fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    // Class of the previous kept character: 0 none, 1 lower, 2 upper, 3 digit.
    let mut prev_class = 0u8;
    for raw in text.chars() {
        if is_invisible(raw) || is_combining(raw) || is_apostrophe(raw) {
            continue;
        }
        if let Some(word) = emoji_word(raw) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(word.to_owned());
            prev_class = 0;
            continue;
        }
        let c = fold_styled(fold_fullwidth(raw));
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_class = 0;
            continue;
        }
        let class = if c.is_numeric() {
            3
        } else if c.is_uppercase() {
            2
        } else {
            1
        };
        let split = !cur.is_empty()
            && ((prev_class == 1 && class == 2)
                || (prev_class == 3 && class != 3)
                || (prev_class != 3 && prev_class != 0 && class == 3));
        if split {
            out.push(std::mem::take(&mut cur));
        }
        match fold_upper_confusable(c) {
            Some(a) => cur.push(a),
            None => cur.extend(c.to_lowercase().map(fold_confusable)),
        }
        prev_class = class;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// True for currency symbols that mark a price.
fn is_currency_symbol(c: char) -> bool {
    matches!(
        fold_fullwidth(c),
        '$' | '€'
            | '£'
            | '¥'
            | '₹'
            | '₩'
            | '₽'
            | '₺'
            | '₪'
            | '฿'
            | '₫'
            | '₱'
            | '₴'
            | '₦'
            | '¢'
            | '₣'
            | '₡'
            | '₲'
            | '₵'
            | '₸'
            | '₼'
            | '₾'
            | '₿'
            | '￥'
            | '￡'
    )
}

/// ISO currency codes and words that turn an adjacent number into a price.
const CURRENCY_WORDS: &[&str] = &[
    "usd", "eur", "gbp", "cad", "aud", "nzd", "jpy", "cny", "rmb", "inr", "chf", "sek", "nok",
    "dkk", "pln", "mxn", "brl", "zar", "sgd", "hkd", "krw", "ils", "aed", "dollar", "dollars",
    "euro", "euros", "pound", "pounds", "bucks", "rupees", "yen",
];

/// True when `text` contains something that reads as a money amount: a currency symbol
/// next to a digit (`$42.10`, `42,10 €`), or a number next to a currency code or word
/// (`42 USD`, `EUR 10`, `20 dollars`).
pub(crate) fn has_money(text: &str) -> bool {
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !is_invisible(*c))
        .map(fold_fullwidth)
        .collect();
    for (i, c) in chars.iter().enumerate() {
        if !is_currency_symbol(*c) {
            continue;
        }
        // Look up to two characters either side, skipping one space or sign.
        let near = |j: Option<usize>| j.and_then(|j| chars.get(j)).copied();
        let after = [near(i.checked_add(1)), near(i.checked_add(2))];
        let before = [near(i.checked_sub(1)), near(i.checked_sub(2))];
        let digit_near = |pair: [Option<char>; 2]| match pair {
            [Some(a), _] if a.is_ascii_digit() => true,
            [Some(a), Some(b)] if (a == ' ' || a == '\u{a0}' || a == '-') && b.is_ascii_digit() => {
                true
            }
            _ => false,
        };
        if digit_near(after) || digit_near(before) {
            return true;
        }
    }
    let toks = tokens(text);
    toks.iter().enumerate().any(|(i, t)| {
        CURRENCY_WORDS.contains(&t.as_str()) && {
            let is_num = |j: Option<usize>| {
                j.and_then(|j| toks.get(j))
                    .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
            };
            is_num(i.checked_sub(1)) || is_num(i.checked_add(1))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_splits_case_and_digits_and_drops_invisibles() {
        assert_eq!(tokens("PlaceOrder"), ["place", "order"]);
        assert_eq!(tokens("Pay$42.10"), ["pay", "42", "10"]);
        assert_eq!(tokens("De\u{200B}lete"), ["delete"]);
        assert_eq!(tokens("Don’t Allow"), ["dont", "allow"]);
        assert_eq!(tokens("ＳＥＮＤ"), ["send"]);
        assert_eq!(tokens("Ѕеnd"), ["send"]);
        assert_eq!(tokens("𝐏𝐚𝐲"), ["pay"]);
        assert_eq!(tokens("SEND"), ["send"]);
    }

    #[test]
    fn money_detection() {
        assert!(has_money("Pay $42.10"));
        assert!(has_money("Total: 42,10 €"));
        assert!(has_money("Send 20 USD"));
        assert!(has_money("$ 5"));
        assert!(!has_money("Call 555-1234"));
        assert!(!has_money("Price list"));
        assert!(!has_money("$HOME"));
    }
}
