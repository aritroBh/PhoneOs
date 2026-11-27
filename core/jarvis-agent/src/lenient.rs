//! A forgiving JSON-ish reader for small-model output, written by hand so it can never
//! panic: single or double quotes, unquoted keys and words, trailing commas, `True`/`None`,
//! `=` instead of `:`, and a balanced-brace scanner that finds objects in surrounding prose.
//!
//! Everything works on a `&[char]` with bounds-checked `get`, and recursion is capped.

/// Deepest nesting accepted; deeper input is rejected rather than risking the stack.
const MAX_DEPTH: usize = 24;

/// A parsed value. Objects keep their key order and duplicate keys (the interpreter decides
/// what a duplicate means).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LVal {
    /// An object as ordered key/value pairs.
    Obj(Vec<(String, LVal)>),
    /// An array.
    Arr(Vec<LVal>),
    /// A string (quoted, or a bare word).
    Str(String),
    /// A finite number.
    Num(f64),
    /// A boolean.
    Bool(bool),
    /// `null` / `None`.
    Null,
}

impl LVal {
    /// Converts to a `serde_json::Value` (duplicate keys: the first one wins).
    pub(crate) fn to_json(&self) -> serde_json::Value {
        use serde_json::Value;
        match self {
            LVal::Obj(pairs) => {
                let mut map = serde_json::Map::new();
                for (k, v) in pairs {
                    if !map.contains_key(k) {
                        map.insert(k.clone(), v.to_json());
                    }
                }
                Value::Object(map)
            }
            LVal::Arr(items) => Value::Array(items.iter().map(LVal::to_json).collect()),
            LVal::Str(s) => Value::String(s.clone()),
            LVal::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 9.0e15 {
                    // Integral and exactly representable: keep it an integer.
                    Value::from(*n as i64)
                } else {
                    serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number)
                }
            }
            LVal::Bool(b) => Value::Bool(*b),
            LVal::Null => Value::Null,
        }
    }
}

/// Where a top-level `{...}` sits in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    /// Index of `{`.
    pub start: usize,
    /// Index one past the matching `}`.
    pub end: usize,
}

/// Result of scanning text for top-level objects.
#[derive(Debug, Clone, Default)]
pub(crate) struct Scan {
    /// Complete, balanced objects in order.
    pub objects: Vec<Span>,
    /// An object was opened but never closed (output cut off).
    pub truncated: bool,
}

fn is_quote(c: char) -> bool {
    c == '"' || c == '\''
}

/// Finds balanced top-level `{...}` spans. Quotes are honoured inside objects: `"` always
/// opens a string, `'` only where a JSON token may start (after `{ [ , :` or space), so an
/// apostrophe in an unquoted word does not swallow the rest of the text.
pub(crate) fn scan_objects(chars: &[char]) -> Scan {
    let mut scan = Scan::default();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut prev_sig = ' ';
    for (i, &c) in chars.iter().enumerate() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
                prev_sig = c;
            }
            continue;
        }
        if depth > 0 && is_quote(c) && (c == '"' || matches!(prev_sig, '{' | '[' | ',' | ':' | ' '))
        {
            quote = Some(c);
            continue;
        }
        match c {
            '{' => {
                if depth == 0 {
                    start = i;
                }
                depth = depth.saturating_add(1);
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    scan.objects.push(Span { start, end: i + 1 });
                }
            }
            _ => {}
        }
        prev_sig = if c.is_whitespace() { ' ' } else { c };
    }
    scan.truncated = depth > 0;
    scan
}

/// Parses `chars` as exactly one value (surrounding whitespace allowed).
pub(crate) fn parse_value(chars: &[char]) -> Option<LVal> {
    let mut p = Reader { s: chars, i: 0 };
    let v = p.value(0)?;
    p.ws();
    (p.i >= chars.len()).then_some(v)
}

/// Parses one value at the start of `chars` and returns it with the number of chars used.
pub(crate) fn parse_prefix(chars: &[char]) -> Option<(LVal, usize)> {
    let mut p = Reader { s: chars, i: 0 };
    let v = p.value(0)?;
    Some((v, p.i))
}

struct Reader<'a> {
    s: &'a [char],
    i: usize,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '$' | '@' | '/' | '+')
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }

    fn ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Option<LVal> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.ws();
        match self.peek()? {
            '{' => self.object(depth),
            '[' => self.array(depth),
            '"' | '\'' => self.string().map(LVal::Str),
            c if c.is_ascii_digit() || matches!(c, '-' | '+' | '.') => self.number(),
            c if is_word_char(c) => {
                let w = self.word();
                Some(match w.as_str() {
                    "true" | "True" | "TRUE" => LVal::Bool(true),
                    "false" | "False" | "FALSE" => LVal::Bool(false),
                    "null" | "None" | "NULL" | "nil" | "undefined" => LVal::Null,
                    _ => LVal::Str(w),
                })
            }
            _ => None,
        }
    }

    fn word(&mut self) -> String {
        let mut w = String::new();
        while let Some(c) = self.peek().filter(|c| is_word_char(*c)) {
            w.push(c);
            self.i += 1;
        }
        w
    }

    fn object(&mut self, depth: usize) -> Option<LVal> {
        self.bump(); // '{'
        let mut pairs = Vec::new();
        loop {
            self.ws();
            match self.peek()? {
                '}' => {
                    self.i += 1;
                    return Some(LVal::Obj(pairs));
                }
                ',' => {
                    // Stray or trailing comma.
                    self.i += 1;
                    continue;
                }
                _ => {}
            }
            let key = match self.peek()? {
                '"' | '\'' => self.string()?,
                c if is_word_char(c) => self.word(),
                _ => return None,
            };
            self.ws();
            match self.bump()? {
                ':' | '=' => {}
                _ => return None,
            }
            let v = self.value(depth + 1)?;
            pairs.push((key, v));
            self.ws();
            match self.peek()? {
                ',' => self.i += 1,
                '}' => {}
                _ => return None,
            }
        }
    }

    fn array(&mut self, depth: usize) -> Option<LVal> {
        self.bump(); // '['
        let mut items = Vec::new();
        loop {
            self.ws();
            match self.peek()? {
                ']' => {
                    self.i += 1;
                    return Some(LVal::Arr(items));
                }
                ',' => {
                    self.i += 1;
                    continue;
                }
                _ => {}
            }
            items.push(self.value(depth + 1)?);
            self.ws();
            match self.peek()? {
                ',' => self.i += 1,
                ']' => {}
                _ => return None,
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..4 {
            let d = self.bump()?.to_digit(16)?;
            v = v * 16 + d;
        }
        Some(v)
    }

    fn string(&mut self) -> Option<String> {
        let q = self.bump()?;
        let mut out = String::new();
        loop {
            let c = self.bump()?;
            if c == q {
                return Some(out);
            }
            if c != '\\' {
                out.push(c);
                continue;
            }
            let e = self.bump()?;
            match e {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'u' => {
                    let hi = self.hex4()?;
                    let code = if (0xD800..0xDC00).contains(&hi) {
                        // A surrogate pair, if the low half follows.
                        let save = self.i;
                        let low = if self.bump() == Some('\\') && self.bump() == Some('u') {
                            self.hex4()
                        } else {
                            None
                        };
                        match low {
                            Some(lo) if (0xDC00..0xE000).contains(&lo) => {
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            }
                            _ => {
                                self.i = save;
                                0xFFFD
                            }
                        }
                    } else {
                        hi
                    };
                    out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                }
                other => out.push(other),
            }
        }
    }

    fn number(&mut self) -> Option<LVal> {
        let mut t = String::new();
        if let Some(c) = self.peek().filter(|c| matches!(c, '-' | '+')) {
            t.push(c);
            self.i += 1;
        }
        let mut seen_digit = false;
        while let Some(c) = self
            .peek()
            .filter(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E'))
        {
            if matches!(c, 'e' | 'E') {
                t.push(c);
                self.i += 1;
                if let Some(s) = self.peek().filter(|c| matches!(c, '-' | '+')) {
                    t.push(s);
                    self.i += 1;
                }
                continue;
            }
            seen_digit |= c.is_ascii_digit();
            t.push(c);
            self.i += 1;
        }
        if !seen_digit {
            return None;
        }
        // A word glued to the number ("17th", "3px") makes it a word, not a number.
        if self.peek().is_some_and(|c| c.is_alphabetic() || c == '_') {
            let rest = self.word();
            return Some(LVal::Str(format!("{t}{rest}")));
        }
        let n: f64 = t.trim_start_matches('+').parse().ok()?;
        n.is_finite().then_some(LVal::Num(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn scanner_respects_strings_and_escapes() {
        let c = chars(r#"pre {"a":"}{\"}"} mid {'b': 'x}'} {"open": "#);
        let scan = scan_objects(&c);
        assert_eq!(scan.objects.len(), 2);
        assert!(scan.truncated);
        let first: String = c[scan.objects[0].start..scan.objects[0].end]
            .iter()
            .collect();
        assert_eq!(first, r#"{"a":"}{\"}"}"#);
    }

    #[test]
    fn apostrophes_in_words_do_not_open_strings() {
        let c = chars("{thought: it's fine, action: tap, element: 3}");
        let scan = scan_objects(&c);
        assert_eq!(scan.objects.len(), 1);
        assert!(!scan.truncated);
    }

    #[test]
    fn reader_accepts_loose_forms() {
        let v = parse_value(&chars("{'a': 1, b = True, \"c\": [1, 2,], d: None,}")).unwrap();
        assert_eq!(
            v,
            LVal::Obj(vec![
                ("a".into(), LVal::Num(1.0)),
                ("b".into(), LVal::Bool(true)),
                ("c".into(), LVal::Arr(vec![LVal::Num(1.0), LVal::Num(2.0)])),
                ("d".into(), LVal::Null),
            ])
        );
    }

    #[test]
    fn deep_nesting_is_rejected_not_overflowed() {
        let s = "[".repeat(10_000);
        assert!(parse_value(&chars(&s)).is_none());
    }

    #[test]
    fn unicode_escapes() {
        let v = parse_value(&chars(r#""é😀\ud800x""#)).unwrap();
        assert_eq!(v, LVal::Str("é😀\u{FFFD}x".into()));
    }
}
