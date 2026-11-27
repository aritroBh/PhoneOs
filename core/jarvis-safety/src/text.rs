//! Character classes and the word tokenizer shared by classification, sanitising and
//! redaction.
//!
//! Matching works on *tokens*: lowercase alphanumeric runs after invisible characters are
//! dropped, look-alike letters are folded to ASCII and apostrophes are removed. Comparing
//! whole tokens gives word-boundary matching without regular expressions.

/// True for characters that render as nothing (or only reorder text) and so can be used to
/// split a keyword without changing how it looks: zero-width and joiner characters, bidi
/// controls, soft hyphen, variation selectors, Hangul fillers, Unicode tag characters,
/// interlinear annotation marks, blank-rendering letters (Braille blank, Hangul and
/// musical fillers) and non-whitespace C0/C1 controls. Review pass 35: every character of
/// Unicode general category Cf (15.1) is among them — the prepended concatenation marks
/// (U+0600..U+0605, U+06DD, U+070F, U+0890, U+0891, U+08E2, U+110BD, U+110CD) and the
/// Egyptian hieroglyph format controls (U+13430..U+1343F) were missing; the table the test
/// walks is `jarvis_remote::text::FORMAT_RANGES` (copied: this crate does not depend on it).
pub fn is_invisible(c: char) -> bool {
    let u = u32::from(c);
    if c.is_control() {
        // Whitespace controls (tab, newline...) are handled as whitespace by callers.
        return !c.is_whitespace();
    }
    matches!(
        u,
        0x00AD
            | 0x034F
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
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
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D159
            | 0x1D173..=0x1D17A
            | 0x2800
            | 0xE0000..=0xE0FFF
    )
}

/// True for combining marks, which are dropped before matching so `P\u{301}ay` reads `pay`.
/// Review pass 37 (F4): every nonspacing or enclosing mark (General_Category Mn and Me,
/// Unicode 15.1, [`crate::norm::is_mark`]), and every code point of the five combining
/// blocks read before (their unassigned ones too).
pub fn is_combining(c: char) -> bool {
    in_combining_blocks(c) || crate::norm::is_mark(c)
}

/// The five combining-mark blocks (U+0300.., U+1AB0.., U+1DC0.., U+20D0.., U+FE20..).
fn in_combining_blocks(c: char) -> bool {
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

/// First code points (digit zero) of the Unicode decimal-digit blocks (category Nd) other
/// than ASCII, each block holding ten consecutive digits. The mathematical digits
/// (U+1D7CE..U+1D7FF) are five such blocks.
const DIGIT_ZEROS: &[u32] = &[
    0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6, 0x0D66,
    0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
    0x104A0, 0x10D30, 0x10D40, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0,
    0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950, 0x11BF0, 0x11C50, 0x11D50, 0x11DA0, 0x11F50,
    0x16130, 0x16A60, 0x16AC0, 0x16B50, 0x16D70, 0x1CCF0, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC,
    0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0, 0x1E5F1, 0x1E950, 0x1FBF0,
];

/// Sixteenth review (F4/F8): the value of a decimal digit in any script — ASCII, Arabic-Indic,
/// Extended Arabic-Indic, Devanagari, Bengali, Thai, fullwidth, mathematical … (Unicode
/// category Nd) — or `None`. The one digit test of the gate: every rule that asks "is this a
/// digit" or reads a number goes through it ([`fold_digit`], [`fold_digits`], [`is_digit`]).
pub(crate) fn digit_value(c: char) -> Option<u32> {
    if c.is_ascii() {
        return c.to_digit(10);
    }
    let u = u32::from(c);
    DIGIT_ZEROS
        .iter()
        .find(|&&z| u >= z && u - z < 10)
        .map(|&z| u - z)
}

/// Whether `c` is a decimal digit of any script ([`digit_value`]).
pub(crate) fn is_digit(c: char) -> bool {
    digit_value(c).is_some()
}

/// `c`, or the ASCII digit with its value when it is a decimal digit of another script (the
/// Arabic decimal separator U+066B and thousands separator U+066C become `.` and `,`).
pub(crate) fn fold_digit(c: char) -> char {
    if c.is_ascii() {
        return c;
    }
    // The Arabic decimal and thousands separators beside Arabic-Indic digits.
    match c {
        '\u{066B}' => return '.',
        '\u{066C}' => return ',',
        _ => {}
    }
    digit_value(c)
        .and_then(|v| char::from_digit(v, 10))
        .unwrap_or(c)
}

/// `s` with every decimal digit folded to ASCII ([`fold_digit`]); borrowed when `s` is ASCII
/// (the common case costs one scan).
pub(crate) fn fold_digits(s: &str) -> std::borrow::Cow<'_, str> {
    if s.is_ascii() {
        std::borrow::Cow::Borrowed(s)
    } else {
        std::borrow::Cow::Owned(s.chars().map(fold_digit).collect())
    }
}

/// Unicode category Sc (currency symbols), any script: the dollar and cent signs, the Latin-1
/// currency signs, the Armenian, Arabic, N'Ko, Bengali, Gujarati, Tamil, Thai, Khmer and
/// Currency Symbols blocks, and the other code points of the category, as plain or
/// fullwidth forms.
pub(crate) fn is_sc(c: char) -> bool {
    matches!(
        u32::from(fold_fullwidth(c)),
        0x24 | 0xA2..=0xA5
            | 0x58F
            | 0x60B
            | 0x7FE
            | 0x7FF
            | 0x9F2
            | 0x9F3
            | 0x9FB
            | 0xAF1
            | 0xBF9
            | 0xE3F
            | 0x17DB
            | 0x20A0..=0x20CF
            | 0xA838
            | 0xFDFC
            | 0xFE69
            | 0xFFE0
            | 0xFFE1
            | 0xFFE5
            | 0xFFE6
            | 0x11FDD..=0x11FE0
            | 0x1E2FF
            | 0x1ECB0
    )
}

/// Maps a letter from Unicode's styled alphabets (mathematical bold/italic/script...,
/// circled letters) back to ASCII.
pub(crate) fn fold_styled(c: char) -> char {
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
        _ => fold_letterlike(c),
    }
}

/// Letterlike symbols (U+2100 block) that read as an ASCII letter.
fn fold_letterlike(c: char) -> char {
    match c {
        'ℂ' => 'C',
        'ℊ' => 'g',
        'ℋ' | 'ℌ' | 'ℍ' => 'H',
        'ℎ' | 'ℏ' => 'h',
        'ℐ' | 'ℑ' => 'I',
        'ℒ' => 'L',
        'ℓ' => 'l',
        'ℕ' => 'N',
        '℘' | 'ℙ' => 'P',
        'ℚ' => 'Q',
        'ℛ' | 'ℜ' | 'ℝ' => 'R',
        'ℤ' => 'Z',
        'ℬ' => 'B',
        'ℰ' => 'E',
        'ℯ' | '℮' => 'e',
        'ℱ' => 'F',
        'ℳ' => 'M',
        'ℴ' => 'o',
        'ℹ' => 'i',
        'K' => 'K',
        'Å' => 'A',
        'ⅅ' => 'D',
        'ⅆ' => 'd',
        'ⅇ' => 'e',
        'ⅈ' => 'i',
        'ⅉ' => 'j',
        _ => c,
    }
}

/// Folds a lowercase character that looks like an ASCII letter (Cyrillic, Greek, accented
/// Latin) to that letter. Applied after lowercasing.
pub(crate) fn fold_confusable(c: char) -> char {
    match c {
        'а' | 'α' | 'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'ᴀ' | 'ɑ' => {
            'a'
        }
        'в' | 'β' | 'ь' | 'ʙ' => 'b',
        'с' | 'ϲ' | 'ç' | 'ć' | 'č' | 'ᴄ' => 'c',
        'ԁ' | 'ď' | 'đ' | 'ᴅ' => 'd',
        'е' | 'ё' | 'ε' | 'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' | 'ᴇ' => 'e',
        'ꜰ' => 'f',
        'ɡ' | 'ğ' | 'ɢ' | 'ց' => 'g',
        'н' | 'һ' | 'ʜ' | 'հ' => 'h',
        'і' | 'ї' | 'ι' | 'ì' | 'í' | 'î' | 'ï' | 'ı' | 'ɪ' => 'i',
        'ј' | 'ᴊ' => 'j',
        'к' | 'κ' | 'ᴋ' => 'k',
        'ł' | 'ľ' | 'ĺ' | 'ʟ' => 'l',
        'м' | 'ᴍ' => 'm',
        'η' | 'ñ' | 'п' | 'ń' | 'ň' | 'ɴ' | 'ո' => 'n',
        'о' | 'ο' | 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' | 'ᴏ' | 'օ' => 'o',
        'р' | 'ρ' | 'ᴘ' => 'p',
        'ԛ' => 'q',
        'г' | 'ř' | 'ʀ' => 'r',
        'ѕ' | 'ś' | 'ş' | 'š' | 'ș' | 'ꜱ' => 's',
        'т' | 'τ' | 'ť' | 'ț' | 'ᴛ' => 't',
        'υ' | 'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ᴜ' | 'ս' => 'u',
        'ν' | 'ᴠ' => 'v',
        'ԝ' | 'ω' | 'ᴡ' => 'w',
        'х' | 'χ' => 'x',
        'у' | 'ý' | 'ÿ' | 'γ' | 'ʏ' => 'y',
        'ž' | 'ź' | 'ż' | 'ᴢ' => 'z',
        _ => c,
    }
}

/// Folds one character for look-alike-insensitive comparison: fullwidth and styled forms
/// to ASCII, lowercase, then Cyrillic/Greek/accented look-alikes to ASCII letters.
pub fn fold_char(c: char) -> char {
    let c = fold_digit(fold_styled(fold_fullwidth(c)));
    if let Some(a) = fold_upper_confusable(c) {
        return a;
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    fold_confusable(lower)
}

/// Folds an uppercase Greek or Cyrillic letter that looks like an ASCII capital to that
/// letter, lowercased. Checked before lowercasing, because some capitals look like a
/// different Latin letter than their lowercase forms do (Greek `Ν` is `N`, `ν` is `v`).
pub(crate) fn fold_upper_confusable(c: char) -> Option<char> {
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

/// Most characters of one label that matching reads; longer labels are cut to their head
/// and tail (see [`bounded`]) and count as unreadable when tapped.
pub(crate) const MAX_LABEL_CHARS: usize = 1024;

/// `text` if it has at most [`MAX_LABEL_CHARS`] characters, else its first three quarters
/// and last quarter of that budget joined by a space. Keeps all matching linear and
/// bounded per element.
pub(crate) fn bounded(text: &str) -> std::borrow::Cow<'_, str> {
    if text.len() <= MAX_LABEL_CHARS {
        return std::borrow::Cow::Borrowed(text);
    }
    if text.is_ascii() {
        let n = text.len();
        let head = MAX_LABEL_CHARS / 4 * 3;
        let tail = MAX_LABEL_CHARS / 4;
        let mut out = String::with_capacity(MAX_LABEL_CHARS + 1);
        out.push_str(text.get(..head).unwrap_or_default());
        out.push(' ');
        out.push_str(text.get(n.saturating_sub(tail)..).unwrap_or_default());
        return std::borrow::Cow::Owned(out);
    }
    let n = text.chars().count();
    if n <= MAX_LABEL_CHARS {
        return std::borrow::Cow::Borrowed(text);
    }
    let head = MAX_LABEL_CHARS / 4 * 3;
    let tail = MAX_LABEL_CHARS / 4;
    let mut out: String = text.chars().take(head).collect();
    out.push(' ');
    out.extend(text.chars().skip(n.saturating_sub(tail)));
    std::borrow::Cow::Owned(out)
}

/// Whether `c` is shaped like a colon (eleventh review: the list is taken from Unicode's
/// two-dots-stacked and colon punctuation, not from the look-alikes seen so far): ASCII
/// and fullwidth colons, small and presentation forms (U+FE55, U+FE13, U+FE30), modifier
/// letters (U+A789, U+02D0, U+02F8), the ratio (U+2236), the Hebrew sof pasuq (U+05C3),
/// the Armenian full stop (U+0589), the Syriac colons (U+0703–U+0705, U+0708, U+0709), the
/// Ethiopic wordspace and colons (U+1361, U+1365, U+1366), the Mongolian colon (U+1804),
/// the Tibetan gter tsheg (U+0F14), two-dot punctuation (U+205A), vertical dot runs
/// (U+205D, U+205E, U+22EE), the Lisu tone letter (U+A4FD), the Bamum colon (U+A6F4), the
/// runic multiple punctuation (U+16EC), the Z notation type colon (U+2982) and the
/// composition operator (U+2A3E).
pub(crate) fn colon_shaped(c: char) -> bool {
    matches!(
        c,
        ':' | '\u{FF1A}'
            | '\u{FE55}'
            | '\u{FE13}'
            | '\u{FE30}'
            | '\u{A789}'
            | '\u{02D0}'
            | '\u{02F8}'
            | '\u{2236}'
            | '\u{05C3}'
            | '\u{0589}'
            | '\u{0703}'
            | '\u{0704}'
            | '\u{0705}'
            | '\u{0708}'
            | '\u{0709}'
            | '\u{0F14}'
            | '\u{1361}'
            | '\u{1365}'
            | '\u{1366}'
            | '\u{1804}'
            | '\u{205A}'
            | '\u{205D}'
            | '\u{205E}'
            | '\u{22EE}'
            | '\u{A4FD}'
            | '\u{A6F4}'
            | '\u{16EC}'
            | '\u{2982}'
            | '\u{2A3E}'
    )
}

/// Whether `c` is a letter (or a combining mark) outside ASCII: in page text before a
/// colon it makes the run of words unable to be a heading Jarvis writes (eleventh review).
pub(crate) fn non_ascii_letter(c: char) -> bool {
    !c.is_ascii() && (c.is_alphabetic() || is_combining(c))
}

/// The letters of U+00C0–U+017F that have no canonical decomposition into an ASCII letter
/// plus diacritics but that real names are written with (thirteenth review): ß, æ/Æ, ø/Ø,
/// đ/Đ, ł/Ł, þ/Þ, ð/Ð, œ/Œ, ŋ/Ŋ, ħ/Ħ, ŧ/Ŧ.
const NAME_LETTERS: &[char] = &[
    'ß', 'æ', 'Æ', 'ø', 'Ø', 'đ', 'Đ', 'ł', 'Ł', 'þ', 'Þ', 'ð', 'Ð', 'œ', 'Œ', 'ŋ', 'Ŋ', 'ħ', 'Ħ',
    'ŧ', 'Ŧ',
];

/// The rest of U+00C0–U+017F without a canonical decomposition, which are never exempt
/// (thirteenth review): the signs × ÷; the ligature and compatibility forms ĳ/Ĳ (reads
/// "ij"), Ŀ/ŀ (L with middle dot, "l·"), ŉ ("'n"); the dotless, long and kra forms ı, ſ,
/// ĸ (read as i, f, k).
const NOT_NAME_LETTERS: &[char] = &['×', '÷', 'ĳ', 'Ĳ', 'Ŀ', 'ŀ', 'ŉ', 'ı', 'ſ', 'ĸ'];

/// Whether `c` is an accented Latin letter a name may hold next to ASCII ones without
/// being flagged (twelfth review; thirteenth review: by structure, not by block). Exempt
/// are, within U+00C0–U+017F, exactly (1) the precomposed letters — an ASCII letter plus
/// one or more diacritics (José, Müller, Ångström, Čapek, İzmir) — and (2) the stand-alone
/// letters of [`NAME_LETTERS`] (Łukasz, Søren, Þórr, Œuvre). The block holds nothing else
/// but [`NOT_NAME_LETTERS`] (ligatures, compatibility, dotless and long forms, two signs),
/// which are not exempt; an exhaustive test over the block checks the partition.
pub(crate) fn accented_latin(c: char) -> bool {
    matches!(u32::from(c), 0x00C0..=0x017F)
        && (NAME_LETTERS.contains(&c) || !NOT_NAME_LETTERS.contains(&c))
}

/// Eleventh review: whether a recipient value mixes scripts the owner cannot tell apart.
/// Twelfth review (no dependence on [`fold_char`]'s table): a **word** holding ASCII
/// letters together with **any** other non-ASCII letter or combining mark (Cherokee `Ꮪ`,
/// Latin `ǀ` U+01C0, Cyrillic `а`, Greek `ο`, Armenian `հ`, …) is flagged, except accented
/// Latin letters ([`accented_latin`]: `José Smith`, `Müller` are not); the value as a whole
/// is also flagged when it holds ASCII letters and a letter [`fold_char`] reads as an ASCII
/// one (a word wholly of look-alikes beside an ASCII word), accented Latin again excepted;
/// an email's host or an `@handle` holding any non-ASCII letter is flagged. A name written
/// wholly in another script (no ASCII letter) is not.
pub(crate) fn mixed_script(value: &str) -> bool {
    let odd = |c: char| (non_ascii_letter(c) || is_combining(c)) && !accented_latin(c);
    let word_mixes = value
        .split(|c: char| c.is_whitespace())
        .any(|w| w.chars().any(|c| c.is_ascii_alphabetic()) && w.chars().any(odd));
    if word_mixes {
        return true;
    }
    let ascii = value.chars().any(|c| c.is_ascii_alphabetic());
    let foldable = value.chars().any(|c| {
        !c.is_ascii()
            && c.is_alphabetic()
            && !accented_latin(c)
            && fold_char(c).is_ascii_alphabetic()
    });
    if ascii && foldable {
        return true;
    }
    let tail = match value.trim().split_once('@') {
        Some((_, host)) => host,
        None => return false,
    };
    tail.chars().any(|c| !c.is_ascii() && c.is_alphabetic())
}

/// Whether a letter can be read by the phrase net: ASCII, foldable to ASCII, or Latin
/// script (whose accented forms the tables approximate).
pub(crate) fn is_readable_letter(c: char) -> bool {
    fold_char(c).is_ascii()
        || matches!(u32::from(c), 0x00C0..=0x024F | 0x1E00..=0x1EFF)
        // Review pass 37: only the five blocks read before. A mark of another block that is
        // also alphabetic (a Devanagari vowel sign) keeps the label unreadable: the
        // normalised reading adds its words' category, `Unverified` stays carried.
        || in_combining_blocks(c)
}

/// True when the phrase tables cannot read `label`: it contains letters of a script they
/// do not cover (CJK, Arabic, Cyrillic words beyond look-alikes...), or it is longer than
/// the matcher reads. `classify` treats a tap on such a pressable element as risky.
pub fn label_is_unreadable(label: &str) -> bool {
    if label.chars().count() > MAX_LABEL_CHARS {
        return true;
    }
    label
        .chars()
        .filter(|c| !is_invisible(*c))
        .any(|c| c.is_alphabetic() && !is_readable_letter(c))
}

/// Common UI words of other languages written in Latin script (German, French, Spanish,
/// Italian, Portuguese, Dutch), as tokens (accents folded: `löschen` is `loschen`). The
/// English tables read these as harmless, so a control labelled with one is unverified.
/// Not a translation table: it only says "this is not English".
pub const FOREIGN_UI_WORDS: &[&str] = &[
    // German
    "bezahlen",
    "zahlen",
    "jetzt",
    "kaufen",
    "bestellen",
    "senden",
    "absenden",
    "schicken",
    "loschen",
    "entfernen",
    "bestatigen",
    "weiter",
    "abbrechen",
    "zuruck",
    "anrufen",
    "buchen",
    "speichern",
    "andern",
    "abmelden",
    "anmelden",
    "kasse",
    "zahlungspflichtig",
    "teilen",
    "veroffentlichen",
    "uberweisen",
    // French
    "payer",
    "acheter",
    "envoyer",
    "supprimer",
    "effacer",
    "confirmer",
    "valider",
    "commander",
    "appeler",
    "annuler",
    "enregistrer",
    "publier",
    "partager",
    "maintenant",
    "virement",
    // Spanish / Portuguese
    "pagar",
    "comprar",
    "enviar",
    "eliminar",
    "borrar",
    "confirmar",
    "aceptar",
    "llamar",
    "ligar",
    "excluir",
    "apagar",
    "publicar",
    "compartir",
    "compartilhar",
    "guardar",
    "cancelar",
    "pedir",
    "ahora",
    "agora",
    "transferir",
    // Italian
    "pagare",
    "acquista",
    "acquistare",
    "invia",
    "inviare",
    "elimina",
    "eliminare",
    "cancella",
    "conferma",
    "confermare",
    "chiama",
    "ordina",
    "compra",
    "condividi",
    // Dutch
    "betalen",
    "kopen",
    "verzenden",
    "verwijderen",
    "bevestigen",
    "bellen",
    "afrekenen",
    "versturen",
    "delen",
];

/// True when `label` contains a word of [`FOREIGN_UI_WORDS`].
pub(crate) fn has_foreign_word(label: &str) -> bool {
    tokens(&bounded(label))
        .iter()
        .any(|t| FOREIGN_UI_WORDS.contains(&t.as_str()))
}

/// True when `label` contains an accented or extended Latin letter (`ä`, `é`, `ß`, `ñ`,
/// `ł`...), which English control labels do not use.
pub(crate) fn has_non_english_letters(label: &str) -> bool {
    bounded(label).chars().any(|c| {
        c.is_alphabetic()
            && matches!(u32::from(c), 0x00C0..=0x00FF | 0x0100..=0x024F | 0x1E00..=0x1EFF)
    })
}

/// True when `label` reads as a language other than English: it has a common non-English
/// UI word ([`FOREIGN_UI_WORDS`]) or accented/extended Latin letters. `classify` treats a
/// tap on such a control as `Unverified` (rows and texts only count the word list, since
/// names carry accents).
pub fn label_is_foreign(label: &str) -> bool {
    has_foreign_word(label) || has_non_english_letters(label)
}

/// True for apostrophe-like characters, which are deleted inside words (`don't` → `dont`).
pub(crate) fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | '‘' | 'ʼ' | '`' | '´')
}

/// Icon characters used as button labels, read as the word they stand for.
pub(crate) fn emoji_word(c: char) -> Option<&'static str> {
    match c {
        '\u{1F5D1}' => Some("trash"),
        '\u{1F4B8}' => Some("pay"),
        '\u{1F4E4}' => Some("send"),
        _ => None,
    }
}

/// Splits `text` into lowercase, look-alike-folded word tokens.
///
/// Boundaries: any non-alphanumeric character (a narrow space too), a lowercase→uppercase
/// transition (`PlaceOrder` → `place order`) and a letter↔digit transition (`Pay42` → `pay
/// 42`). Every character first goes through [`crate::norm::piece`] (review pass 37):
/// invisible characters, every nonspacing or enclosing mark and apostrophes are removed
/// without splitting, a precomposed letter reads as its base letter; a few icon emoji
/// (wastebasket, money, outbox) read as words.
pub(crate) fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    // Class of the previous kept character: 0 none, 1 lower, 2 upper, 3 digit.
    let mut prev_class = 0u8;
    for raw in text.chars() {
        // ASCII needs no folding (tenth review: the same result without the per-character
        // folding calls, which made a page of long labels cost seconds in a debug build).
        if raw.is_ascii() {
            if (raw.is_ascii_control() && !raw.is_ascii_whitespace()) || raw == '\'' || raw == '`' {
                continue;
            }
            if !raw.is_ascii_alphanumeric() {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                prev_class = 0;
                continue;
            }
            let class = if raw.is_ascii_digit() {
                3
            } else if raw.is_ascii_uppercase() {
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
            cur.push(raw.to_ascii_lowercase());
            prev_class = class;
            continue;
        }
        // Review pass 37: the one normalisation both readings share (`crate::norm`): marks
        // of every block dropped, precomposed letters read as their base, narrow spaces a
        // space here (a soft boundary to the sets).
        let c = match crate::norm::piece(raw) {
            None => continue,
            Some(crate::norm::Piece::Emoji(word)) => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(word.to_owned());
                prev_class = 0;
                continue;
            }
            Some(crate::norm::Piece::Narrow) => ' ',
            Some(crate::norm::Piece::Char(c)) => c,
        };
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
pub(crate) fn is_currency_symbol(c: char) -> bool {
    is_sc(c)
        || matches!(
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
                | '円'
                | '元'
                | '원'
                | '₨'
        )
}

/// ISO currency codes and words that turn an adjacent number into a price.
pub(crate) const CURRENCY_WORDS: &[&str] = &[
    "usd", "eur", "gbp", "cad", "aud", "nzd", "jpy", "cny", "rmb", "inr", "chf", "sek", "nok",
    "dkk", "pln", "mxn", "brl", "zar", "sgd", "hkd", "krw", "ils", "aed", "dollar", "dollars",
    "euro", "euros", "pound", "pounds", "bucks", "rupees", "yen", "kr", "kc", "zl", "rs", "lei",
    "ft", "kn", "rub", "uah", "php", "thb", "vnd", "idr", "myr", "won", "yuan", "sar", "egp",
    "ngn", "kes", "czk", "huf", "ron", "bgn", "isk",
    // Crypto tickers (the common ones; "sol" and the like are ordinary words too).
    "btc", "xbt", "eth", "usdt", "usdc", "sats",
];

/// True when `text` contains something that reads as a money amount: a currency symbol
/// next to a digit (`$42.10`, `42,10 €`), or a number next to a currency code or word
/// (`42 USD`, `EUR 10`, `20 dollars`).
pub(crate) fn has_money(text: &str) -> bool {
    has_money_in(text, None)
}

/// [`has_money`] reusing `text`'s [`tokens`] when the caller has them.
pub(crate) fn has_money_in(text: &str, toks: Option<&[String]>) -> bool {
    let words = |t: &str| match toks {
        Some(k) => money_tokens(k),
        None => money_tokens(&tokens(t)),
    };
    // Plain ASCII without control characters (tenth review, the same result faster): the
    // only currency symbol is `$`, and a money token needs an ASCII digit.
    let b = text.as_bytes();
    if text.is_ascii()
        && !b
            .iter()
            .any(|c| c.is_ascii_control() && !c.is_ascii_whitespace())
    {
        if !b.iter().any(u8::is_ascii_digit) {
            return false;
        }
        let digit_near = |a: Option<&u8>, z: Option<&u8>| match (a, z) {
            (Some(a), _) if a.is_ascii_digit() => true,
            (Some(b' ' | b'-'), Some(z)) => z.is_ascii_digit(),
            _ => false,
        };
        for (i, c) in b.iter().enumerate() {
            if *c != b'$' {
                continue;
            }
            let at = |j: Option<usize>| j.and_then(|j| b.get(j));
            if digit_near(at(i.checked_add(1)), at(i.checked_add(2)))
                || digit_near(at(i.checked_sub(1)), at(i.checked_sub(2)))
            {
                return true;
            }
        }
        return words(text);
    }
    let chars: Vec<char> = text
        .chars()
        .filter(|c| !is_invisible(*c))
        .map(|c| fold_digit(fold_fullwidth(c)))
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
    words(text)
}

/// Whether `toks` hold a currency code or word next to a number (see [`has_money`]).
fn money_tokens(toks: &[String]) -> bool {
    // Review pass 36 (F8): the neighbours first (cheap), the word list only beside a number.
    let is_num = |j: Option<usize>| {
        j.and_then(|j| toks.get(j))
            .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
    };
    toks.iter().enumerate().any(|(i, t)| {
        (is_num(i.checked_sub(1)) || is_num(i.checked_add(1)))
            && CURRENCY_WORDS.contains(&t.as_str())
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
    fn digits_of_every_script_fold_to_ascii() {
        // Arabic-Indic, Extended Arabic-Indic (Persian), Devanagari, Bengali, Thai,
        // fullwidth, mathematical, N'Ko, Myanmar, Khmer.
        for (written, folded) in [
            ("\u{0665}\u{0660}\u{0660}", "500"),
            ("\u{06F5}\u{06F0}\u{06F0}", "500"),
            ("\u{096B}\u{0966}\u{0966}", "500"),
            ("\u{09EB}\u{09E6}\u{09E6}", "500"),
            ("\u{0E55}\u{0E50}\u{0E50}", "500"),
            ("\u{FF15}\u{FF10}\u{FF10}", "500"),
            ("\u{1D7D9}\u{1D7D8}", "10"),
            ("\u{07C5}\u{07C0}", "50"),
            ("\u{1045}\u{1040}", "50"),
            ("\u{17E5}\u{17E0}", "50"),
            ("500", "500"),
        ] {
            assert_eq!(fold_digits(written), folded, "{written:?}");
            assert!(written.chars().all(is_digit), "{written:?}");
        }
        // Mixed scripts in one number fold digit by digit; the Arabic separators too.
        assert_eq!(fold_digits("\u{0665}0\u{096B}"), "505");
        assert_eq!(
            fold_digits("\u{0665}\u{066C}\u{0660}\u{0660}\u{0660}\u{066B}\u{0665}"),
            "5,000.5"
        );
        // Not digits: letters, other numerals, a superscript.
        for c in [
            'a', '\u{00B2}', '\u{2460}', '\u{2166}', '\u{4E94}', '.', ' ',
        ] {
            assert!(!is_digit(c), "{c:?}");
            assert_eq!(fold_digit(c), c);
        }
        // Every block's ten digits are consecutive values.
        for &z in DIGIT_ZEROS {
            for v in 0..10u32 {
                let c = char::from_u32(z + v).unwrap_or('x');
                assert_eq!(digit_value(c), Some(v), "{c:?}");
            }
        }
        // The folded forms are read as money.
        for t in [
            "\u{20B9}\u{096B},\u{0966}\u{0966}\u{0966}",
            "\u{0665}\u{0660}\u{0660} $",
            "\u{FF04}\u{FF15}\u{FF10}",
            "$\u{0665}\u{0660}",
            "\u{0E3F}\u{0E55}\u{0E50}",
        ] {
            assert!(has_money(t), "{t:?}");
        }
        assert!(tokens("\u{0665}\u{0660}\u{0660}") == ["500"]);
        assert!(is_sc('\u{0E3F}') && is_sc('\u{20BF}') && is_sc('\u{FFE5}') && !is_sc('x'));
    }

    /// Every character of Unicode general category Cf, version 15.1, as inclusive ranges (a
    /// copy of `jarvis_remote::text::FORMAT_RANGES`, generated from the Unicode Character
    /// Database).
    const FORMAT_RANGES: [(char, char); 21] = [
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

    /// Review pass 35: every Cf character is invisible to matching: it splits no word.
    #[test]
    fn every_format_character_is_invisible() {
        let mut count = 0u32;
        for (lo, hi) in FORMAT_RANGES {
            for c in lo..=hi {
                count += 1;
                assert!(is_invisible(c), "U+{:04X}", u32::from(c));
                assert_eq!(
                    tokens(&format!("De{c}lete")),
                    ["delete"],
                    "U+{:04X}",
                    u32::from(c)
                );
            }
        }
        assert_eq!(count, 170, "Unicode 15.1 has 170 Cf characters");
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
        // The forms the approval record and the summary facts rely on.
        for t in [
            "5.00 USD", "USD 5", "€5", "5 €", "£12", "¥500", "₹99", "120 kr", "0.01 BTC",
            "2.5 ETH", "100 USDC", "₿0.5",
        ] {
            assert!(has_money(t), "{t}");
        }
        for t in ["Pickup in 3 min", "9:58", "45%", "Quantity 2", "2 tickets"] {
            assert!(!has_money(t), "{t}");
        }
    }
}
