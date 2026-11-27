//! The model side of the simulator: a small trait and a scripted implementation.

use std::collections::{BTreeSet, VecDeque};

use jarvis_proto::{ModelOutput, ModelRequest};
use serde_json::Value;
use thiserror::Error;

/// Why a model call failed. The driver turns every variant into `Event::ModelFailed`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModelError {
    /// A scripted model ran out of outputs.
    #[error("scripted model exhausted: all {served} outputs were used (call {call})")]
    Exhausted {
        /// Outputs served so far.
        served: usize,
        /// 1-based number of the call that found the queue empty.
        call: usize,
    },
    /// A fault injected by the test or CLI.
    #[error("injected model failure on call {0}")]
    Injected(usize),
    /// A scripted `@label` reference did not match anything on the screen.
    #[error("{0}")]
    Script(String),
    /// HTTP transport or status error, including timeouts.
    #[error("{0}")]
    Http(String),
}

/// Anything that can answer a [`ModelRequest`].
pub trait Model {
    /// Produces the model's raw text for `req`.
    fn generate(&mut self, req: &ModelRequest) -> Result<ModelOutput, ModelError>;

    /// A short name for transcripts.
    fn name(&self) -> String {
        "model".to_owned()
    }

    /// A note about the last call for the transcript (for example "vision sent as text").
    fn take_note(&mut self) -> Option<String> {
        None
    }

    /// Real elapsed milliseconds of the last call, for reporting only.
    fn take_real_ms(&mut self) -> Option<u64> {
        None
    }

    /// The structured-output mode in use (`json_schema`, `json_object`, `none`), for models
    /// that negotiate one with a server; `None` otherwise.
    fn response_format(&self) -> Option<String> {
        None
    }
}

/// A queue of fixed outputs.
///
/// Outputs are served in order, one per call. A JSON output whose `"element"` is a string
/// starting with `@` is resolved against the screen in the request, so scripts do not
/// depend on element numbers:
///
/// * `"@Wi-Fi"` the first element whose label is `Wi-Fi` (exact, then case-insensitive,
///   then containing);
/// * `"@Switch:Wi-Fi"` restricts the match to one role;
/// * `"@Cell:Mom#2"` picks the second match.
///
/// An output may also be a condition on the prompt, for a model that reacts to what it is
/// told: `{"if_prompt": "TEXT", "then": OUTPUT, "else": OUTPUT}`. When the request's user
/// text contains `TEXT`, `then` is served and the entry is used up; otherwise `else` is
/// served and the entry stays queued, so it is asked again on the next call (a model that
/// keeps doing the same thing until it sees `TEXT`). `then` and `else` are outputs like any
/// other (`@` references work).
#[derive(Debug, Clone, Default)]
pub struct ScriptedModel {
    outputs: VecDeque<String>,
    served: usize,
    calls: usize,
    fail_at: BTreeSet<usize>,
    requests: Vec<ModelRequest>,
}

impl ScriptedModel {
    /// A model that will answer with these outputs in order.
    pub fn new<I, S>(outputs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            outputs: outputs.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// One output per non-empty line; lines starting with `#` are comments.
    pub fn from_jsonl(text: &str) -> Self {
        Self::new(
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_owned),
        )
    }

    /// Makes the `n`-th call (0-based) fail once without consuming an output.
    pub fn fail_at(mut self, n: usize) -> Self {
        self.fail_at.insert(n);
        self
    }

    /// Number of `generate` calls so far, failed ones included.
    pub fn calls(&self) -> usize {
        self.calls
    }

    /// Outputs not yet served.
    pub fn remaining(&self) -> usize {
        self.outputs.len()
    }

    /// Every request received, in order.
    pub fn requests(&self) -> &[ModelRequest] {
        &self.requests
    }
}

impl Model for ScriptedModel {
    fn generate(&mut self, req: &ModelRequest) -> Result<ModelOutput, ModelError> {
        let n = self.calls;
        self.calls += 1;
        self.requests.push(req.clone());
        if self.fail_at.remove(&n) {
            return Err(ModelError::Injected(n + 1));
        }
        let entry = self.outputs.front().ok_or(ModelError::Exhausted {
            served: self.served,
            call: n + 1,
        })?;
        let (chosen, used_up) = match conditional(entry) {
            Some((needle, then, otherwise)) => {
                if req.user.contains(&needle) {
                    (then, true)
                } else {
                    (otherwise, false)
                }
            }
            None => (entry.clone(), true),
        };
        // An output whose `@reference` matches nothing stays queued: the session retries a
        // failed request unchanged, and the retry fails the same way.
        let text = resolve_labels(&chosen, &req.user)?;
        if used_up {
            self.outputs.pop_front();
        }
        self.served += 1;
        Ok(ModelOutput { text })
    }

    fn name(&self) -> String {
        format!("scripted ({} outputs left)", self.outputs.len())
    }
}

/// One `[N] Role "label" ...` line of the rendered screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenLine {
    /// Element number.
    pub index: u32,
    /// Role name as rendered (`Button`).
    pub role: String,
    /// Label, unescaped.
    pub label: String,
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Reads the element lines out of a prompt's screen block.
pub fn parse_screen_lines(user: &str) -> Vec<ScreenLine> {
    user.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix('[')?;
            let (num, rest) = rest.split_once("] ")?;
            let index: u32 = num.parse().ok()?;
            let (role, rest) = match rest.split_once(' ') {
                Some((r, rest)) => (r, rest),
                None => (rest, ""),
            };
            let label = match rest.strip_prefix('"') {
                Some(body) => {
                    let mut end = None;
                    let mut escaped = false;
                    for (i, c) in body.char_indices() {
                        if escaped {
                            escaped = false;
                        } else if c == '\\' {
                            escaped = true;
                        } else if c == '"' {
                            end = Some(i);
                            break;
                        }
                    }
                    unescape(body.get(..end?)?)
                }
                None => String::new(),
            };
            Some(ScreenLine {
                index,
                role: role.to_owned(),
                label,
            })
        })
        .collect()
}

/// Resolves an `@label` reference to an element number.
pub fn resolve_reference(spec: &str, lines: &[ScreenLine]) -> Result<u32, ModelError> {
    let (body, nth) = match spec.rsplit_once('#') {
        Some((b, n)) if n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() => {
            (b, n.parse::<usize>().unwrap_or(1).max(1))
        }
        _ => (spec, 1),
    };
    let (role, label) = match body.split_once(':') {
        Some((r, l))
            if !r.is_empty()
                && r.chars().all(|c| c.is_ascii_alphanumeric())
                && lines.iter().any(|x| x.role.eq_ignore_ascii_case(r)) =>
        {
            (Some(r), l)
        }
        _ => (None, body),
    };
    let candidates: Vec<&ScreenLine> = lines
        .iter()
        .filter(|l| role.is_none_or(|r| l.role.eq_ignore_ascii_case(r)))
        .collect();
    let want = label.to_lowercase();
    let passes: [&dyn Fn(&ScreenLine) -> bool; 3] = [
        &|l| l.label == label,
        &|l| l.label.to_lowercase() == want,
        &|l| l.label.to_lowercase().contains(&want),
    ];
    for pass in passes {
        if let Some(l) = candidates.iter().filter(|l| pass(l)).nth(nth - 1) {
            return Ok(l.index);
        }
    }
    let seen: Vec<String> = lines
        .iter()
        .take(10)
        .map(|l| format!("{} \"{}\"", l.role, l.label))
        .collect();
    Err(ModelError::Script(format!(
        "script reference @{spec} matches nothing on the screen (first elements: {})",
        seen.join(", ")
    )))
}

/// `(needle, then, else)` of a conditional script entry (see [`ScriptedModel`]).
fn conditional(entry: &str) -> Option<(String, String, String)> {
    let v: Value = serde_json::from_str(entry).ok()?;
    let obj = v.as_object()?;
    let needle = obj.get("if_prompt")?.as_str()?.to_owned();
    let out = |k: &str| match obj.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    Some((needle, out("then"), out("else")))
}

/// Rewrites `"element":"@..."` in a JSON output; any other text passes through untouched.
fn resolve_labels(output: &str, user: &str) -> Result<String, ModelError> {
    if !output.contains("\"@") {
        return Ok(output.to_owned());
    }
    let Ok(Value::Object(mut obj)) = serde_json::from_str::<Value>(output) else {
        return Ok(output.to_owned());
    };
    let Some(Value::String(spec)) = obj.get("element").cloned() else {
        return Ok(output.to_owned());
    };
    let Some(spec) = spec.strip_prefix('@') else {
        return Ok(output.to_owned());
    };
    let index = resolve_reference(spec, &parse_screen_lines(user))?;
    obj.insert("element".to_owned(), Value::from(index));
    Ok(Value::Object(obj).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: &str = "Task: x\nSteps so far:\n1. tap [3] -> ok\n[1] NavigationBar \"Wi-Fi\"\n[2] Switch \"Wi-Fi\" value=\"1\"\n[3] Cell \"Wi-Fi: \\\"Home\\\"\"\n[4] Cell \"Wi-Fi extra\"";

    fn lines() -> Vec<ScreenLine> {
        parse_screen_lines(SCREEN)
    }

    #[test]
    fn parses_only_element_lines_and_unescapes_labels() {
        let l = lines();
        assert_eq!(l.len(), 4);
        assert_eq!(l[2].label, "Wi-Fi: \"Home\"");
        assert_eq!(l[1].role, "Switch");
    }

    #[test]
    fn references_resolve_by_label_role_and_position() {
        let l = lines();
        assert_eq!(resolve_reference("Wi-Fi", &l).unwrap(), 1);
        assert_eq!(resolve_reference("Switch:Wi-Fi", &l).unwrap(), 2);
        assert_eq!(resolve_reference("Wi-Fi#2", &l).unwrap(), 2);
        assert_eq!(resolve_reference("wi-fi extra", &l).unwrap(), 4);
        assert_eq!(resolve_reference("Cell:Wi-Fi", &l).unwrap(), 3);
        let e = resolve_reference("Nope", &l).unwrap_err();
        assert!(e.to_string().contains("@Nope"), "{e}");
    }

    #[test]
    fn scripted_model_serves_in_order_then_errors_clearly() {
        let mut m = ScriptedModel::new(["a", "{\"action\":\"tap\",\"element\":\"@Switch:Wi-Fi\"}"]);
        let req = ModelRequest {
            user: SCREEN.to_owned(),
            ..ModelRequest::default()
        };
        assert_eq!(m.generate(&req).unwrap().text, "a");
        let second = m.generate(&req).unwrap().text;
        assert!(second.contains("\"element\":2"), "{second}");
        let err = m.generate(&req).unwrap_err();
        assert_eq!(err, ModelError::Exhausted { served: 2, call: 3 });
        assert!(err.to_string().contains("exhausted"));
        assert_eq!(m.calls(), 3);
        assert_eq!(m.requests().len(), 3);
    }

    #[test]
    fn conditional_entries_repeat_until_the_prompt_shows_the_text() {
        let entry = r#"{"if_prompt":"changed:","then":"{\"action\":\"done\"}","else":{"action":"tap","element":"@Switch:Wi-Fi"}}"#;
        let mut m = ScriptedModel::new([entry, "last"]);
        let plain = ModelRequest {
            user: SCREEN.to_owned(),
            ..ModelRequest::default()
        };
        for _ in 0..3 {
            let out = m.generate(&plain).unwrap().text;
            assert!(out.contains("\"element\":2"), "{out}");
        }
        assert_eq!(m.remaining(), 2);
        let told = ModelRequest {
            user: format!("{SCREEN}\n1. tap [2] -> ok; changed: x"),
            ..ModelRequest::default()
        };
        assert_eq!(m.generate(&told).unwrap().text, "{\"action\":\"done\"}");
        assert_eq!(m.generate(&told).unwrap().text, "last");
    }

    #[test]
    fn injected_failure_does_not_consume_an_output() {
        let mut m = ScriptedModel::new(["x"]).fail_at(0);
        let req = ModelRequest::default();
        assert_eq!(m.generate(&req).unwrap_err(), ModelError::Injected(1));
        assert_eq!(m.generate(&req).unwrap().text, "x");
    }

    #[test]
    fn jsonl_skips_blank_and_comment_lines() {
        let m = ScriptedModel::from_jsonl("# c\n\n{\"a\":1}\n  \nfoo\n");
        assert_eq!(m.remaining(), 2);
    }
}
