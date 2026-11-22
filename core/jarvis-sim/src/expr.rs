//! A tiny expression language shared by app definitions, templates and eval predicates.
//!
//! Grammar, loosest binding first:
//!
//! ```text
//! expr    := or ('?' expr ':' expr)?
//! or      := and ('||' and)*
//! and     := eq ('&&' eq)*
//! eq      := rel (('==' | '!=') rel)*
//! rel     := unary (('<' | '<=' | '>' | '>=') unary)*
//! unary   := '!' unary | primary
//! primary := number | 'string' | "string" | true | false | null
//!          | len '(' expr ')' | has '(' expr ',' expr ')' | '(' expr ')' | path
//! path    := ident ('.' ident | '[' expr ']')*
//! ```
//!
//! Paths are looked up in the layers of an [`Env`] (item scope first, then app state). A
//! missing path is `null`. On arrays `.last`, `.first` and `.len` are pseudo-fields; on
//! strings `.len` is the character count.
//!
//! `has(text, part)` is a tolerant text check for predicates on what the agent typed or
//! sent: true when `part` occurs in `text` once both are normalised (lowercase, curly quotes
//! and apostrophes dropped, other punctuation turned into spaces, whitespace collapsed), so
//! `has(messages.thread["Mom"].last, "running late")` accepts "I'm running late!". A
//! missing or non-string `text` is false; an empty `part` is true for any string.

use serde_json::{Map, Value};
use thiserror::Error;

/// Why an expression could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExprError {
    /// The text is not a valid expression.
    #[error("bad expression `{text}`: {reason}")]
    Parse {
        /// The expression text.
        text: String,
        /// What was wrong.
        reason: String,
    },
}

/// Name lookup layers, searched in order.
#[derive(Debug, Clone, Default)]
pub struct Env<'a> {
    layers: Vec<&'a Value>,
}

impl<'a> Env<'a> {
    /// An environment with no variables.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a lookup layer after the existing ones (an object whose keys are variables).
    pub fn with(mut self, layer: &'a Value) -> Self {
        self.layers.push(layer);
        self
    }

    fn lookup(&self, name: &str) -> Value {
        self.layers
            .iter()
            .find_map(|l| l.get(name))
            .cloned()
            .unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone)]
enum Seg {
    Field(String),
    Index(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone)]
enum Expr {
    Lit(Value),
    Path(Vec<Seg>),
    Not(Box<Expr>),
    Len(Box<Expr>),
    Has(Box<Expr>, Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, s: &str) -> bool {
        self.skip_ws();
        let n = s.chars().count();
        let matches = self
            .chars
            .get(self.pos..self.pos + n)
            .is_some_and(|w| w.iter().copied().eq(s.chars()));
        if matches {
            self.pos += n;
        }
        matches
    }

    fn eat_op(&mut self, s: &str, not_followed_by: &[char]) -> bool {
        self.skip_ws();
        let save = self.pos;
        if self.eat(s) && !self.peek().is_some_and(|c| not_followed_by.contains(&c)) {
            return true;
        }
        self.pos = save;
        false
    }

    fn err<T>(&self, reason: &str) -> Result<T, String> {
        Err(format!("{reason} at column {}", self.pos + 1))
    }

    fn expr(&mut self) -> Result<Expr, String> {
        let cond = self.or()?;
        if self.eat("?") {
            let a = self.expr()?;
            if !self.eat(":") {
                return self.err("expected `:` in `? :`");
            }
            let b = self.expr()?;
            return Ok(Expr::Ternary(Box::new(cond), Box::new(a), Box::new(b)));
        }
        Ok(cond)
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.eat("||") {
            let right = self.and()?;
            left = Expr::Bin(Op::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.eq()?;
        while self.eat("&&") {
            let right = self.eq()?;
            left = Expr::Bin(Op::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn eq(&mut self) -> Result<Expr, String> {
        let mut left = self.rel()?;
        loop {
            let op = if self.eat("==") {
                Op::Eq
            } else if self.eat("!=") {
                Op::Ne
            } else {
                return Ok(left);
            };
            let right = self.rel()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn rel(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        loop {
            let op = if self.eat("<=") {
                Op::Le
            } else if self.eat(">=") {
                Op::Ge
            } else if self.eat_op("<", &[]) {
                Op::Lt
            } else if self.eat_op(">", &[]) {
                Op::Gt
            } else {
                return Ok(left);
            };
            let right = self.unary()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat_op("!", &['=']) {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn string(&mut self, quote: char) -> Result<Expr, String> {
        self.pos += 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return self.err("unterminated string"),
                Some('\\') => {
                    self.pos += 1;
                    if let Some(c) = self.peek() {
                        s.push(c);
                        self.pos += 1;
                    }
                }
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(Expr::Lit(Value::String(s)));
                }
                Some(c) => {
                    s.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.pos += 1;
            } else {
                break;
            }
        }
        s
    }

    fn primary(&mut self) -> Result<Expr, String> {
        self.skip_ws();
        match self.peek() {
            None => self.err("unexpected end"),
            Some('(') => {
                self.pos += 1;
                let e = self.expr()?;
                if !self.eat(")") {
                    return self.err("expected `)`");
                }
                Ok(e)
            }
            Some(q @ ('"' | '\'')) => self.string(q),
            Some(c) if c.is_ascii_digit() || c == '-' => {
                let start = self.pos;
                self.pos += 1;
                while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                    self.pos += 1;
                }
                let text: String = self
                    .chars
                    .get(start..self.pos)
                    .unwrap_or_default()
                    .iter()
                    .collect();
                match text.parse::<f64>() {
                    Ok(n) => Ok(Expr::Lit(number(n))),
                    Err(_) => self.err("bad number"),
                }
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                let name = self.ident();
                match name.as_str() {
                    "true" => return Ok(Expr::Lit(Value::Bool(true))),
                    "false" => return Ok(Expr::Lit(Value::Bool(false))),
                    "null" => return Ok(Expr::Lit(Value::Null)),
                    "len" if self.eat("(") => {
                        let e = self.expr()?;
                        if !self.eat(")") {
                            return self.err("expected `)` after len(");
                        }
                        return Ok(Expr::Len(Box::new(e)));
                    }
                    "has" if self.eat("(") => {
                        let text = self.expr()?;
                        if !self.eat(",") {
                            return self.err("expected `,` in has(text, part)");
                        }
                        let part = self.expr()?;
                        if !self.eat(")") {
                            return self.err("expected `)` after has(");
                        }
                        return Ok(Expr::Has(Box::new(text), Box::new(part)));
                    }
                    _ => {}
                }
                let mut segs = vec![Seg::Field(name)];
                loop {
                    if self.peek() == Some('.') {
                        self.pos += 1;
                        let f = self.ident();
                        if f.is_empty() {
                            return self.err("expected a field name after `.`");
                        }
                        segs.push(Seg::Field(f));
                    } else if self.peek() == Some('[') {
                        self.pos += 1;
                        let e = self.expr()?;
                        if !self.eat("]") {
                            return self.err("expected `]`");
                        }
                        segs.push(Seg::Index(e));
                    } else {
                        return Ok(Expr::Path(segs));
                    }
                }
            }
            Some(c) => self.err(&format!("unexpected `{c}`")),
        }
    }
}

/// A JSON number that is an integer when it can be.
pub fn number(n: f64) -> Value {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 9e15 {
        // Truncation is exact: the value is integral and within i64 range.
        #[allow(clippy::cast_possible_truncation)]
        return Value::from(n as i64);
    }
    serde_json::Number::from_f64(n).map_or(Value::Null, Value::Number)
}

fn parse(text: &str) -> Result<Expr, ExprError> {
    let mut p = Parser {
        chars: text.chars().collect(),
        pos: 0,
    };
    let fail = |reason: String| ExprError::Parse {
        text: text.to_owned(),
        reason,
    };
    let e = p.expr().map_err(fail)?;
    p.skip_ws();
    if p.pos < p.chars.len() {
        return Err(fail(format!("trailing text at column {}", p.pos + 1)));
    }
    Ok(e)
}

/// Whether a value counts as true: non-null, non-false, non-zero, non-empty.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

fn compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64()?.partial_cmp(&y.as_f64()?),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn len_of(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::from(a.len()),
        Value::String(s) => Value::from(s.chars().count()),
        Value::Object(o) => Value::from(o.len()),
        _ => Value::from(0),
    }
}

fn field(v: &Value, name: &str) -> Value {
    match v {
        Value::Object(o) => o.get(name).cloned().unwrap_or(Value::Null),
        Value::Array(a) => match name {
            "last" => a.last().cloned().unwrap_or(Value::Null),
            "first" => a.first().cloned().unwrap_or(Value::Null),
            "len" => Value::from(a.len()),
            _ => Value::Null,
        },
        Value::String(_) if name == "len" => len_of(v),
        _ => Value::Null,
    }
}

fn index(v: &Value, i: &Value) -> Value {
    match (v, i) {
        (Value::Array(a), Value::Number(n)) => n
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| a.get(n))
            .cloned()
            .unwrap_or(Value::Null),
        (Value::Object(o), Value::String(k)) => o.get(k).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn eval_expr(e: &Expr, env: &Env<'_>) -> Value {
    match e {
        Expr::Lit(v) => v.clone(),
        Expr::Path(segs) => {
            let mut it = segs.iter();
            let mut cur = match it.next() {
                Some(Seg::Field(root)) => env.lookup(root),
                _ => return Value::Null,
            };
            for s in it {
                cur = match s {
                    Seg::Field(f) => field(&cur, f),
                    Seg::Index(ix) => index(&cur, &eval_expr(ix, env)),
                };
            }
            cur
        }
        Expr::Not(x) => Value::Bool(!truthy(&eval_expr(x, env))),
        Expr::Len(x) => len_of(&eval_expr(x, env)),
        Expr::Has(text, part) => match (eval_expr(text, env), eval_expr(part, env)) {
            (Value::String(t), Value::String(p)) => {
                Value::Bool(normalize_text(&t).contains(&normalize_text(&p)))
            }
            _ => Value::Bool(false),
        },
        Expr::Ternary(c, a, b) => {
            if truthy(&eval_expr(c, env)) {
                eval_expr(a, env)
            } else {
                eval_expr(b, env)
            }
        }
        Expr::Bin(op, a, b) => {
            if *op == Op::Or {
                let l = eval_expr(a, env);
                return if truthy(&l) { l } else { eval_expr(b, env) };
            }
            if *op == Op::And {
                let l = eval_expr(a, env);
                return if truthy(&l) { eval_expr(b, env) } else { l };
            }
            let (l, r) = (eval_expr(a, env), eval_expr(b, env));
            Value::Bool(match op {
                Op::Eq => values_equal(&l, &r),
                Op::Ne => !values_equal(&l, &r),
                Op::Lt => compare(&l, &r).is_some_and(|o| o.is_lt()),
                Op::Le => compare(&l, &r).is_some_and(|o| o.is_le()),
                Op::Gt => compare(&l, &r).is_some_and(|o| o.is_gt()),
                Op::Ge => compare(&l, &r).is_some_and(|o| o.is_ge()),
                Op::Or | Op::And => false,
            })
        }
    }
}

/// Lowercase, apostrophes and quotes dropped, other punctuation as spaces, whitespace
/// collapsed, padded with one space each side (so `has` matches whole words at the edges).
fn normalize_text(s: &str) -> String {
    let mut out = String::from(" ");
    for c in s.chars().flat_map(char::to_lowercase) {
        if matches!(
            c,
            '\'' | '\u{2019}' | '\u{2018}' | '"' | '\u{201C}' | '\u{201D}'
        ) {
            continue;
        }
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

/// Evaluates `text` as an expression.
pub fn eval(text: &str, env: &Env<'_>) -> Result<Value, ExprError> {
    Ok(eval_expr(&parse(text)?, env))
}

/// Evaluates `text` and returns its truthiness.
pub fn eval_bool(text: &str, env: &Env<'_>) -> Result<bool, ExprError> {
    eval(text, env).map(|v| truthy(&v))
}

/// How a value reads inside a label: strings verbatim, numbers without a trailing `.0`,
/// `null` as nothing, lists and objects as compact JSON.
pub fn display(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && n.is_i64() => n.to_string(),
            Some(f) => format!("{f}"),
            None => n.to_string(),
        },
        other => other.to_string(),
    }
}

fn money(v: &Value) -> String {
    let f = v.as_f64().unwrap_or(0.0);
    format!("${f:.2}")
}

/// Index of the `|` that starts a filter (`{total|money}`), skipping `||` and quoted text.
fn filter_split(s: &str) -> Option<usize> {
    let bytes: Vec<(usize, char)> = s.char_indices().collect();
    let mut quote: Option<char> = None;
    let mut found = None;
    for (n, (at, c)) in bytes.iter().enumerate() {
        match (quote, *c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(*c),
            (None, '|') => {
                let prev = n.checked_sub(1).and_then(|p| bytes.get(p)).map(|x| x.1);
                let next = bytes.get(n + 1).map(|x| x.1);
                if prev != Some('|') && next != Some('|') {
                    found = Some(*at);
                }
            }
            _ => {}
        }
    }
    found
}

/// One `{...}` placeholder: `Some(value)` when it parses, `None` to keep the text literally.
fn placeholder(inner: &str, env: &Env<'_>) -> Option<Value> {
    let (expr, filter) = match filter_split(inner) {
        Some(at) => (inner.get(..at)?, inner.get(at + 1..)?.trim()),
        None => (inner, ""),
    };
    let v = eval(expr, env).ok()?;
    Some(match filter {
        "" => v,
        "money" => Value::String(money(&v)),
        "upper" => Value::String(display(&v).to_uppercase()),
        _ => return None,
    })
}

/// Expands `{expr}` and `{expr|money}` placeholders in `template`.
///
/// A template that is exactly one placeholder yields the typed value (so `"{price}"` is a
/// number); anything else yields a string. `{{` and `}}` are literal braces. A `{...}` that
/// is not a valid expression stays as written.
pub fn render(template: &str, env: &Env<'_>) -> Value {
    if let Some(inner) = template
        .strip_prefix('{')
        .and_then(|t| t.strip_suffix('}'))
        .filter(|i| !i.contains(['{', '}']))
    {
        if let Some(v) = placeholder(inner, env) {
            return v;
        }
    }
    Value::String(render_string(template, env))
}

/// Like [`render`] but always a string.
pub fn render_string(template: &str, env: &Env<'_>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(c) = rest.chars().next() {
        let after = rest.get(c.len_utf8()..).unwrap_or_default();
        if c == '{' && after.starts_with('{') {
            out.push('{');
            rest = after.get(1..).unwrap_or_default();
        } else if c == '}' && after.starts_with('}') {
            out.push('}');
            rest = after.get(1..).unwrap_or_default();
        } else if c == '{' {
            match after.split_once('}') {
                Some((inner, tail)) => {
                    match placeholder(inner, env) {
                        Some(v) => out.push_str(&display(&v)),
                        None => {
                            out.push('{');
                            out.push_str(inner);
                            out.push('}');
                        }
                    }
                    rest = tail;
                }
                None => {
                    out.push(c);
                    rest = after;
                }
            }
        } else {
            out.push(c);
            rest = after;
        }
    }
    out
}

/// Recursively renders every string inside a JSON value (used for effect arguments).
pub fn render_json(v: &Value, env: &Env<'_>) -> Value {
    match v {
        Value::String(s) => render(s, env),
        Value::Array(a) => Value::Array(a.iter().map(|x| render_json(x, env)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), render_json(x, env)))
                .collect::<Map<_, _>>(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state() -> Value {
        json!({
            "wifi": false, "n": 3, "name": "Mom", "draft": "",
            "thread": {"Mom": ["hi", "running late"], "Alex": []},
            "cart": [{"name": "Lamp", "price": 35.5}],
            "total": 129
        })
    }

    #[test]
    fn evaluates_paths_comparisons_and_pseudo_fields() {
        let s = state();
        let env = Env::new().with(&s);
        let t = |e: &str| eval_bool(e, &env).unwrap();
        assert!(t("wifi == false"));
        assert!(t("!wifi && n >= 3 && n < 4"));
        assert!(t("thread[\"Mom\"].last == 'running late'"));
        assert!(t("thread[name].len == 2 && len(thread.Alex) == 0"));
        assert!(t("cart.first.price == 35.5"));
        assert!(!t("draft != ''"));
        assert!(t("missing.deep.path == null"));
        assert!(t("wifi || n == 3"));
        assert_eq!(eval("n == 3 ? 'yes' : 'no'", &env).unwrap(), json!("yes"));
    }

    #[test]
    fn templates_keep_types_and_escape_braces() {
        let s = state();
        let env = Env::new().with(&s);
        assert_eq!(render("{total}", &env), json!(129));
        assert_eq!(
            render("Total: {total|money}", &env),
            json!("Total: $129.00")
        );
        assert_eq!(render("{{literal}} {name}", &env), json!("{literal} Mom"));
        assert_eq!(render("{wifi ? '1' : '0'}", &env), json!("0"));
        assert_eq!(render("{ not valid", &env), json!("{ not valid"));
        assert_eq!(render("{1 +}", &env), json!("{1 +}"));
    }

    #[test]
    fn layers_shadow_in_order() {
        let item = json!({"name": "Alex"});
        let s = state();
        let env = Env::new().with(&item).with(&s);
        assert_eq!(render("{name}", &env), json!("Alex"));
        assert_eq!(render("{n}", &env), json!(3));
    }

    #[test]
    fn has_is_a_tolerant_contains_for_sent_text() {
        let s = json!({"last": "I\u{2019}m running LATE!", "n": 3});
        let env = Env::new().with(&s);
        let t = |e: &str| eval_bool(e, &env).unwrap();
        assert!(t("has(last, 'running late')"));
        assert!(t("has(last, \"I'm running late\")"));
        assert!(t("has(last, 'late')"));
        assert!(!t("has(last, 'lat')"), "whole words only");
        assert!(!t("has(last, 'on time')"));
        assert!(!t("has(missing, 'x')"));
        assert!(!t("has(n, '3')"));
        assert!(eval("has(last)", &env).is_err());
    }

    #[test]
    fn bad_expressions_are_errors() {
        let env = Env::new();
        assert!(eval("a ==", &env).is_err());
        assert!(eval("(a", &env).is_err());
        assert!(eval("a b", &env).is_err());
        assert!(eval("'x", &env).is_err());
    }
}
