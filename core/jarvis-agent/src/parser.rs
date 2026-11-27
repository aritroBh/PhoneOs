//! Tolerant parsing of small-model output into one [`Action`].
//!
//! Accepted, besides the canonical form of `schema`:
//! * prose before/after, code fences, several copies of the same action;
//! * single quotes, unquoted keys/words, trailing commas, `True`/`None`, `=` for `:`;
//! * numbers as strings (`"17"`, `"[17]"`, `"#17"`, `17.0`);
//! * alternate keys: `index`/`id`/`target` for `element`, `action_type`/`type`/`command`/
//!   `function` (and `name` when it is an action name) for `action`, `input`/`value` for
//!   `text`, and the other aliases in `schema::ACTIONS`;
//! * alternate action spellings (`click`, `longPress`, `TYPE_TEXT`, `finish` ...);
//! * nested forms `{"action":{"type":"tap","element":3}}`, `{"function":"tap","arguments":{...}}`,
//!   and `{"actions":[...]}` holding one action;
//! * call syntax: `tap(17)`, `type(4, "hi", submit=true)`, `scroll("down")`, `done("sent")`.
//!
//! Two *different* actions anywhere in the output are rejected as ambiguous; the parser never
//! picks one. Input is capped at [`MAX_INPUT_CHARS`]; nothing here can panic.

use jarvis_proto::{Action, Direction};
use thiserror::Error;

use crate::lenient::{parse_prefix, parse_value, scan_objects, LVal, Span};
use crate::schema::{ActionSpec, FieldKind, FieldSpec, ACTIONS, THOUGHT_KEYS};

/// Characters of model output looked at; the rest is ignored.
pub const MAX_INPUT_CHARS: usize = 16_384;
/// Longest thought kept, in characters.
const MAX_THOUGHT_KEEP: usize = 500;
/// Default `long_press` duration when the model gives none.
pub(crate) const DEFAULT_LONG_PRESS_MS: u64 = 800;
/// Default `wait` duration when the model gives none.
pub(crate) const DEFAULT_WAIT_MS: u64 = 1_000;

/// A successfully parsed model reply.
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// The model's stated reasoning, if any (trimmed, capped).
    pub thought: Option<String>,
    /// The action.
    pub action: Action,
}

/// Why model output could not be turned into an action.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    /// Nothing but whitespace.
    #[error("the reply was empty")]
    Empty,
    /// No JSON object or call was found.
    #[error("no action found")]
    NoAction,
    /// An object was opened but not closed.
    #[error("the reply was cut off before the JSON object closed")]
    Truncated,
    /// A `{...}` was found but is not readable JSON.
    #[error("the JSON object is malformed")]
    Malformed,
    /// The object has no action name.
    #[error("the object has no \"action\" key")]
    MissingAction,
    /// The action name is not one of ours.
    #[error("unknown action \"{0}\"")]
    UnknownAction(String),
    /// A required field is missing.
    #[error("{action} needs \"{field}\"")]
    MissingField {
        /// Canonical action name.
        action: &'static str,
        /// Canonical field name.
        field: &'static str,
    },
    /// A field has a value of the wrong shape.
    #[error("{action}: \"{field}\" {problem}")]
    InvalidField {
        /// Canonical action name.
        action: &'static str,
        /// Canonical field name.
        field: &'static str,
        /// What is wrong, e.g. `must be an element number`.
        problem: &'static str,
    },
    /// The same field was given twice with different values.
    #[error("{action}: \"{field}\" is given twice with different values")]
    ConflictingField {
        /// Canonical action name.
        action: &'static str,
        /// Canonical field name.
        field: &'static str,
    },
    /// Two different actions were present.
    #[error("two different actions ({first} and {second}); give exactly one")]
    Ambiguous {
        /// Name of the first action found.
        first: &'static str,
        /// Name of a different action found.
        second: &'static str,
    },
}

/// Parses model output into one action, tolerating the usual small-model deviations.
pub fn parse_model_output(text: &str) -> Result<Parsed, ParseError> {
    let chars: Vec<char> = text.chars().take(MAX_INPUT_CHARS).collect();
    if chars.iter().all(|c| c.is_whitespace()) {
        return Err(ParseError::Empty);
    }
    let scan = scan_objects(&chars);
    let mut found: Vec<Parsed> = Vec::new();
    let mut first_err: Option<ParseError> = None;
    let mut note = |r: Result<Vec<Parsed>, ParseError>, found: &mut Vec<Parsed>| match r {
        Ok(mut v) => found.append(&mut v),
        Err(e) => {
            if first_err.is_none() {
                first_err = Some(e);
            }
        }
    };
    let mut readable: Vec<Span> = Vec::new();
    for span in &scan.objects {
        let slice = chars.get(span.start..span.end).unwrap_or_default();
        let r = match parse_value(slice) {
            Some(v) => {
                readable.push(*span);
                interpret_top(&v)
            }
            None => Err(ParseError::Malformed),
        };
        note(r, &mut found);
    }
    // Calls are looked for outside readable objects only: `type(4, "{{profile.email}}")`
    // holds a brace pair that is not JSON.
    for r in calls_outside(&chars, &readable) {
        note(r.map(|p| vec![p]), &mut found);
    }
    let mut iter = found.into_iter();
    match iter.next() {
        Some(first) => {
            for other in iter {
                if other.action != first.action {
                    return Err(ParseError::Ambiguous {
                        first: canonical_name(&first.action),
                        second: canonical_name(&other.action),
                    });
                }
            }
            Ok(first)
        }
        None => Err(first_err.unwrap_or(if scan.truncated {
            ParseError::Truncated
        } else {
            ParseError::NoAction
        })),
    }
}

fn canonical_name(a: &Action) -> &'static str {
    a.name()
}

/// Lowercases, turns camelCase and `- . space` into snake_case.
fn normalize_name(s: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in s.trim().chars() {
        if c.is_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.extend(c.to_lowercase());
            prev_lower = false;
        } else if c.is_alphanumeric() {
            out.push(c);
            prev_lower = c.is_lowercase() || c.is_ascii_digit();
        } else if matches!(c, '_' | '-' | ' ' | '.') {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            prev_lower = false;
        } else {
            return String::new();
        }
    }
    out.trim_end_matches('_').to_owned()
}

/// The action spec for a name in any accepted spelling.
fn resolve_action(name: &str) -> Option<&'static ActionSpec> {
    let n = normalize_name(name);
    if n.is_empty() {
        return None;
    }
    let squashed = n.replace('_', "");
    ACTIONS.iter().find(|a| {
        a.name == n
            || a.name.replace('_', "") == squashed
            || a.aliases
                .iter()
                .any(|al| *al == n || al.replace('_', "") == squashed)
    })
}

/// Keys that name the action, in priority order.
const ACTION_KEYS: &[&str] = &[
    "action",
    "action_type",
    "type",
    "command",
    "cmd",
    "function",
];
/// Keys whose object value holds the fields of a non-tool action.
const NESTED_FIELD_KEYS: &[&str] = &["arguments", "args", "params", "parameters", "action_input"];

fn key_is(key: &str, name: &str) -> bool {
    normalize_name(key) == name
}

/// A top-level object, possibly an `{"actions":[...]}` wrapper.
fn interpret_top(v: &LVal) -> Result<Vec<Parsed>, ParseError> {
    let LVal::Obj(pairs) = v else {
        return Err(ParseError::Malformed);
    };
    let list = pairs
        .iter()
        .find(|(k, _)| key_is(k, "actions") || key_is(k, "steps"));
    if let Some((_, LVal::Arr(items))) = list {
        let mut out = Vec::new();
        for item in items {
            out.push(interpret_object(item)?);
        }
        if out.is_empty() {
            return Err(ParseError::MissingAction);
        }
        return Ok(out);
    }
    interpret_object(v).map(|p| vec![p])
}

/// Flattens `{"action": {...}}` and `{"function": "tap", "arguments": {...}}` into one list of
/// pairs, outer keys first.
fn flatten(pairs: &[(String, LVal)]) -> Vec<(String, LVal)> {
    let mut out: Vec<(String, LVal)> = Vec::new();
    let mut nested: Vec<(String, LVal)> = Vec::new();
    for (k, v) in pairs {
        let nk = normalize_name(k);
        match v {
            LVal::Obj(inner) if ACTION_KEYS.contains(&nk.as_str()) => {
                for (ik, iv) in inner {
                    let ink = normalize_name(ik);
                    // `{"action":{"type":"tap"}}`: the inner name key becomes the action key.
                    let names_action =
                        ink == "name" && matches!(iv, LVal::Str(n) if resolve_action(n).is_some());
                    if ACTION_KEYS.contains(&ink.as_str()) || names_action {
                        nested.push((k.clone(), iv.clone()));
                    } else {
                        nested.push((ik.clone(), iv.clone()));
                    }
                }
            }
            _ => out.push((k.clone(), v.clone())),
        }
    }
    out.extend(nested);
    out
}

fn interpret_object(v: &LVal) -> Result<Parsed, ParseError> {
    let LVal::Obj(raw) = v else {
        return Err(ParseError::Malformed);
    };
    let mut pairs = flatten(raw);
    let (spec, name_pair) = find_action(&pairs)?;
    if let Some(i) = name_pair {
        if i < pairs.len() {
            pairs.remove(i);
        }
    }
    if spec.name != "call_tool" {
        // Merge `"arguments": {...}` style nesting for non-tool actions.
        let mut extra = Vec::new();
        for (k, v) in &pairs {
            if let LVal::Obj(inner) = v {
                if NESTED_FIELD_KEYS.contains(&normalize_name(k).as_str()) {
                    extra.extend(inner.iter().cloned());
                }
            }
        }
        pairs.extend(extra);
    }
    let thought = thought_of(&pairs);
    let action = build_action(spec, &pairs)?;
    Ok(Parsed { thought, action })
}

/// Finds the action name; conflicting names are ambiguous. Returns the spec and the index
/// of the pair that named it when that was a `name` key (so it is not read as a field).
fn find_action(
    pairs: &[(String, LVal)],
) -> Result<(&'static ActionSpec, Option<usize>), ParseError> {
    let mut chosen: Option<&'static ActionSpec> = None;
    let mut unknown: Option<String> = None;
    for key in ACTION_KEYS {
        for (_, v) in pairs.iter().filter(|(k, _)| key_is(k, key)) {
            // Only the primary keys report unknown names; `type`/`function` often hold
            // unrelated words (`"type":"button"`).
            let primary = matches!(*key, "action" | "action_type" | "command" | "cmd");
            let LVal::Str(name) = v else {
                if primary && unknown.is_none() {
                    unknown = Some("(not a string)".to_owned());
                }
                continue;
            };
            match resolve_action(name) {
                Some(s) => match chosen {
                    Some(c) if c.name != s.name => {
                        return Err(ParseError::Ambiguous {
                            first: c.name,
                            second: s.name,
                        })
                    }
                    _ => chosen = Some(s),
                },
                None if primary && unknown.is_none() => {
                    unknown = Some(name.chars().take(40).collect());
                }
                None => {}
            }
        }
    }
    if let Some(s) = chosen {
        return Ok((s, None));
    }
    // `{"name":"tap","element":3}`: `name` names the action only when it is one.
    let by_name = pairs.iter().enumerate().find_map(|(i, (k, v))| match v {
        LVal::Str(s) if key_is(k, "name") => resolve_action(s).map(|a| (a, Some(i))),
        _ => None,
    });
    if let Some(found) = by_name {
        return Ok(found);
    }
    Err(match unknown {
        Some(n) => ParseError::UnknownAction(n),
        None => ParseError::MissingAction,
    })
}

fn thought_of(pairs: &[(String, LVal)]) -> Option<String> {
    THOUGHT_KEYS.iter().find_map(|key| {
        pairs.iter().find_map(|(k, v)| match v {
            LVal::Str(s) if key_is(k, key) && !s.trim().is_empty() => {
                Some(s.trim().chars().take(MAX_THOUGHT_KEEP).collect())
            }
            _ => None,
        })
    })
}

/// A field's value, `None` when absent (or null).
///
/// The canonical key wins over aliases (`{"summary":"a","message":"b"}` reads `a`), except
/// for element numbers, where any two different values are a conflict: guessing which
/// element was meant could tap the wrong thing. Repeating one key with different values is
/// always a conflict. Values that do not convert are an error only when nothing converts.
fn take<T: PartialEq>(
    action: &'static str,
    f: &FieldSpec,
    pairs: &[(String, LVal)],
    convert: impl Fn(&LVal) -> Option<T>,
    problem: &'static str,
) -> Result<Option<T>, ParseError> {
    let conflict = ParseError::ConflictingField {
        action,
        field: f.name,
    };
    let mut canonical: Option<T> = None;
    let mut alias: Option<T> = None;
    let mut bad = false;
    for (k, v) in pairs {
        let nk = normalize_name(k);
        let is_canonical = nk == f.name;
        if !(is_canonical || f.aliases.contains(&nk.as_str())) || matches!(v, LVal::Null) {
            continue;
        }
        let Some(t) = convert(v) else {
            bad = true;
            continue;
        };
        let slot = if is_canonical {
            &mut canonical
        } else {
            &mut alias
        };
        match slot {
            Some(prev) if *prev != t => return Err(conflict),
            Some(_) => {}
            None => *slot = Some(t),
        }
    }
    let got = match (canonical, alias) {
        (Some(c), Some(a)) if f.kind == FieldKind::Element && c != a => return Err(conflict),
        (Some(c), _) => Some(c),
        (None, a) => a,
    };
    if got.is_none() && bad {
        return Err(ParseError::InvalidField {
            action,
            field: f.name,
            problem,
        });
    }
    Ok(got)
}

fn to_element(v: &LVal) -> Option<u32> {
    match v {
        LVal::Num(n) if n.fract() == 0.0 && *n >= 0.0 && *n <= f64::from(u32::MAX) => {
            Some(*n as u32)
        }
        LVal::Str(s) => {
            let t = s
                .trim()
                .trim_start_matches(['[', '#', '('])
                .trim_end_matches([']', ')'])
                .trim();
            let t = t.strip_suffix(".0").unwrap_or(t);
            if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            t.parse().ok()
        }
        _ => None,
    }
}

fn to_text(v: &LVal) -> Option<String> {
    match v {
        LVal::Str(s) => Some(s.clone()),
        LVal::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => Some(format!("{}", *n as i64)),
        LVal::Num(n) => Some(n.to_string()),
        _ => None,
    }
}

fn to_bool(v: &LVal) -> Option<bool> {
    match v {
        LVal::Bool(b) => Some(*b),
        LVal::Num(n) if *n == 0.0 => Some(false),
        LVal::Num(n) if *n == 1.0 => Some(true),
        LVal::Str(s) => match s.trim().to_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn to_direction(v: &LVal) -> Option<Direction> {
    let LVal::Str(s) = v else { return None };
    match s.trim().to_lowercase().as_str() {
        "up" | "upward" | "upwards" => Some(Direction::Up),
        "down" | "downward" | "downwards" => Some(Direction::Down),
        "left" | "leftward" | "leftwards" => Some(Direction::Left),
        "right" | "rightward" | "rightwards" => Some(Direction::Right),
        _ => None,
    }
}

fn to_number(v: &LVal) -> Option<f64> {
    match v {
        LVal::Num(n) => Some(*n),
        LVal::Str(s) => s.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
        _ => None,
    }
}

fn to_millis(v: &LVal) -> Option<u64> {
    let n = to_number(v)?;
    (0.0..=86_400_000.0).contains(&n).then(|| n.round() as u64)
}

fn to_object(v: &LVal) -> Option<serde_json::Value> {
    match v {
        LVal::Obj(_) => Some(v.to_json()),
        LVal::Str(s) => {
            let chars: Vec<char> = s.chars().collect();
            match parse_value(&chars)? {
                o @ LVal::Obj(_) => Some(o.to_json()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn field_spec(spec: &'static ActionSpec, name: &str) -> Option<&'static FieldSpec> {
    spec.fields.iter().find(|f| f.name == name)
}

/// Reads one field, erroring when required and missing.
fn get<T: PartialEq>(
    spec: &'static ActionSpec,
    name: &'static str,
    pairs: &[(String, LVal)],
    convert: impl Fn(&LVal) -> Option<T>,
    problem: &'static str,
) -> Result<Option<T>, ParseError> {
    let Some(f) = field_spec(spec, name) else {
        return Ok(None);
    };
    let v = take(spec.name, f, pairs, convert, problem)?;
    if v.is_none() && f.required {
        return Err(ParseError::MissingField {
            action: spec.name,
            field: f.name,
        });
    }
    Ok(v)
}

const ELEMENT_PROBLEM: &str = "must be an element number";
const TEXT_PROBLEM: &str = "must be a string";

fn element(spec: &'static ActionSpec, pairs: &[(String, LVal)]) -> Result<Option<u32>, ParseError> {
    get(spec, "element", pairs, to_element, ELEMENT_PROBLEM)
}

fn required<T>(
    spec: &'static ActionSpec,
    field: &'static str,
    v: Option<T>,
) -> Result<T, ParseError> {
    v.ok_or(ParseError::MissingField {
        action: spec.name,
        field,
    })
}

fn text(
    spec: &'static ActionSpec,
    name: &'static str,
    pairs: &[(String, LVal)],
) -> Result<Option<String>, ParseError> {
    get(spec, name, pairs, to_text, TEXT_PROBLEM)
}

fn build_action(spec: &'static ActionSpec, pairs: &[(String, LVal)]) -> Result<Action, ParseError> {
    let action = match spec.name {
        "tap" => Action::Tap {
            element: required(spec, "element", element(spec, pairs)?)?,
        },
        "long_press" => Action::LongPress {
            element: required(spec, "element", element(spec, pairs)?)?,
            duration_ms: get(
                spec,
                "duration_ms",
                pairs,
                to_millis,
                "must be milliseconds",
            )?
            .unwrap_or(DEFAULT_LONG_PRESS_MS),
        },
        "type" => Action::Type {
            element: element(spec, pairs)?,
            text: required(spec, "text", text(spec, "text", pairs)?)?,
            submit: get(spec, "submit", pairs, to_bool, "must be true or false")?.unwrap_or(false),
        },
        "clear" => Action::Clear {
            element: required(spec, "element", element(spec, pairs)?)?,
        },
        "scroll" | "swipe" => {
            let direction = required(
                spec,
                "direction",
                get(
                    spec,
                    "direction",
                    pairs,
                    to_direction,
                    "must be up, down, left or right",
                )?,
            )?;
            let element = element(spec, pairs)?;
            if spec.name == "scroll" {
                Action::Scroll { direction, element }
            } else {
                Action::Swipe { direction, element }
            }
        }
        "tap_point" => {
            let x = get(spec, "x", pairs, to_number, "must be a number from 0 to 1")?;
            let y = get(spec, "y", pairs, to_number, "must be a number from 0 to 1")?;
            Action::TapPoint {
                x: required(spec, "x", x)?,
                y: required(spec, "y", y)?,
            }
        }
        "press_home" => Action::PressHome,
        "open_app" => Action::OpenApp {
            app: required(spec, "app", text(spec, "app", pairs)?)?,
        },
        "open_url" => Action::OpenUrl {
            url: required(spec, "url", text(spec, "url", pairs)?)?,
        },
        "wait" => Action::Wait {
            ms: wait_ms(spec, pairs)?,
        },
        "call_tool" => Action::CallTool {
            name: required(spec, "name", text(spec, "name", pairs)?)?,
            args: get(spec, "args", pairs, to_object, "must be a JSON object")?
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())),
        },
        "ask_user" => Action::AskUser {
            question: required(spec, "question", text(spec, "question", pairs)?)?,
        },
        "look" => Action::Look,
        "done" => Action::Done {
            summary: text(spec, "summary", pairs)?.unwrap_or_default(),
        },
        "fail" => Action::Fail {
            reason: text(spec, "reason", pairs)?.unwrap_or_default(),
        },
        _ => return Err(ParseError::UnknownAction(spec.name.to_owned())),
    };
    Ok(action)
}

/// `ms` (or its aliases), else `seconds`/`s` × 1000, else the default.
fn wait_ms(spec: &'static ActionSpec, pairs: &[(String, LVal)]) -> Result<u64, ParseError> {
    if let Some(ms) = get(spec, "ms", pairs, to_millis, "must be milliseconds")? {
        return Ok(ms);
    }
    let secs = FieldSpec {
        name: "ms",
        kind: FieldKind::Millis,
        required: false,
        aliases: &["seconds", "secs", "s", "sec"],
    };
    let secs = take(spec.name, &secs, pairs, to_number, "must be milliseconds")?;
    Ok(match secs {
        Some(s) if (0.0..=86_400.0).contains(&s) => (s * 1000.0).round() as u64,
        Some(_) => {
            return Err(ParseError::InvalidField {
                action: spec.name,
                field: "ms",
                problem: "must be milliseconds",
            })
        }
        None => DEFAULT_WAIT_MS,
    })
}

// ---------------------------------------------------------------------------------------
// Call syntax: `tap(17)`, `type(4, "hi")`, `done("sent")`.
// ---------------------------------------------------------------------------------------

/// Parses every `name(...)` call whose name is an action, outside the given object spans.
fn calls_outside(chars: &[char], objects: &[Span]) -> Vec<Result<Parsed, ParseError>> {
    let inside = |i: usize| objects.iter().any(|s| i >= s.start && i < s.end);
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars.get(i).copied().unwrap_or(' ');
        let boundary = i == 0
            || chars
                .get(i.wrapping_sub(1))
                .is_none_or(|p| !(p.is_alphanumeric() || *p == '_'));
        if !(c.is_alphabetic() && boundary) || inside(i) {
            i += 1;
            continue;
        }
        let start = i;
        while chars
            .get(i)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            i += 1;
        }
        if chars.get(i) != Some(&'(') {
            continue;
        }
        let name: String = chars.get(start..i).unwrap_or_default().iter().collect();
        let Some(spec) = resolve_action(&name) else {
            continue;
        };
        match call_args(chars, i) {
            Some((args, end)) => {
                if !(start..end).any(inside) {
                    out.push(call_to_action(spec, &args));
                }
                i = end;
            }
            None => i += 1,
        }
    }
    out
}

/// One call argument: positional or `key=value`.
enum Arg {
    Pos(LVal),
    Kw(String, LVal),
}

/// Parses `( arg, arg, ... )` starting at the `(` at `open`; returns args and the index after `)`.
fn call_args(chars: &[char], open: usize) -> Option<(Vec<Arg>, usize)> {
    let mut i = open + 1;
    let mut args = Vec::new();
    loop {
        while chars.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
        }
        match chars.get(i)? {
            ')' => return Some((args, i + 1)),
            ',' => {
                i += 1;
                continue;
            }
            _ => {}
        }
        // `key=value` / `key: value`.
        let mut j = i;
        while chars
            .get(j)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            j += 1;
        }
        let mut k = j;
        while chars.get(k).is_some_and(|c| *c == ' ') {
            k += 1;
        }
        let kw = j > i && matches!(chars.get(k), Some('=' | ':'));
        let key: Option<String> = kw.then(|| chars.get(i..j).unwrap_or_default().iter().collect());
        if kw {
            i = k + 1;
        }
        let (v, used) = parse_prefix(chars.get(i..)?)?;
        i += used;
        args.push(match key {
            Some(k) => Arg::Kw(k, v),
            None => Arg::Pos(v),
        });
        while chars.get(i).is_some_and(|c| c.is_whitespace()) {
            i += 1;
        }
        match chars.get(i)? {
            ',' => i += 1,
            ')' => return Some((args, i + 1)),
            _ => return None,
        }
    }
}

fn is_element_like(v: &LVal) -> bool {
    to_element(v).is_some()
}

/// Maps call arguments to the object form and interprets it like JSON.
fn call_to_action(spec: &'static ActionSpec, args: &[Arg]) -> Result<Parsed, ParseError> {
    let mut pairs: Vec<(String, LVal)> =
        vec![("action".to_owned(), LVal::Str(spec.name.to_owned()))];
    let positional: Vec<&LVal> = args
        .iter()
        .filter_map(|a| match a {
            Arg::Pos(v) => Some(v),
            Arg::Kw(..) => None,
        })
        .collect();
    let mut put = |k: &str, v: &LVal| pairs.push((k.to_owned(), v.clone()));
    match spec.name {
        // type("hi") / type(4, "hi") / type(4, "hi", true)
        "type" => match positional.as_slice() {
            [t] => put("text", t),
            [e, t, rest @ ..] if is_element_like(e) => {
                put("element", e);
                put("text", t);
                if let Some(s) = rest.first() {
                    put("submit", s);
                }
            }
            [t, s] => {
                put("text", t);
                put("submit", s);
            }
            [_, _, ..] => {
                return Err(ParseError::InvalidField {
                    action: "type",
                    field: "element",
                    problem: ELEMENT_PROBLEM,
                })
            }
            [] => {}
        },
        // scroll("down") / scroll(5, "down") / scroll("down", 5)
        "scroll" | "swipe" => {
            for v in positional.iter().take(2) {
                if to_direction(v).is_some() {
                    put("direction", v);
                } else {
                    put("element", v);
                }
            }
        }
        _ => {
            for (f, v) in spec.fields.iter().zip(positional.iter()) {
                put(f.name, v);
            }
        }
    }
    if positional.len() > spec.fields.len().max(1) {
        return Err(ParseError::InvalidField {
            action: spec.name,
            field: spec.fields.first().map_or("action", |f| f.name),
            problem: "has too many arguments",
        });
    }
    for a in args {
        if let Arg::Kw(k, v) = a {
            pairs.push((k.clone(), v.clone()));
        }
    }
    let action = build_action(spec, &pairs)?;
    Ok(Parsed {
        thought: None,
        action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_lookup_by_alias() {
        assert_eq!(
            resolve_action("longPress").map(|s| s.name),
            Some("long_press")
        );
        assert_eq!(resolve_action("TYPE_TEXT").map(|s| s.name), Some("type"));
        assert_eq!(
            resolve_action("Tap-Point").map(|s| s.name),
            Some("tap_point")
        );
        assert_eq!(resolve_action("click").map(|s| s.name), Some("tap"));
        assert!(resolve_action("explode").is_none());
    }

    #[test]
    fn normalize_handles_odd_input() {
        assert_eq!(normalize_name("openApp"), "open_app");
        assert_eq!(normalize_name("  OPEN APP "), "open_app");
        assert_eq!(normalize_name("tap!"), "");
    }
}
