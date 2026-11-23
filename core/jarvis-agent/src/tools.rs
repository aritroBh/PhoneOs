//! Tool routing: which tools the model sees, and checking a call against its `ToolSpec`.

use std::collections::BTreeSet;

use jarvis_proto::{ParamKind, ToolSpec};
use serde_json::{Map, Value};

/// Most tools shown in one prompt.
pub(crate) const MAX_TOOLS: usize = 6;

/// Words that carry no meaning for ranking.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "that", "this", "into", "onto", "about", "please", "can",
    "you", "your", "my", "me", "a", "an", "to", "of", "in", "on", "at", "it", "is", "be", "set",
    "get", "make",
];

/// Lowercase alphanumeric words of 3+ characters (split on `_` too), minus stopwords.
fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.chars().count() >= 3 && !STOPWORDS.contains(&w.as_str()))
        .collect()
}

/// Tools for the current step: those for the foreground app or for no app, ranked by how
/// many task words appear in their name/description (ties: cheaper tier, then name), capped
/// at `cap`.
pub(crate) fn relevant<'a>(
    tools: &'a [ToolSpec],
    foreground: &str,
    task: &str,
    cap: usize,
) -> Vec<&'a ToolSpec> {
    let task_words = words(task);
    let mut scored: Vec<(usize, &ToolSpec)> = tools
        .iter()
        .filter(|t| t.app.as_deref().is_none_or(|a| a == foreground))
        .map(|t| {
            let tool_words = words(&format!("{} {}", t.name, t.description));
            (task_words.intersection(&tool_words).count(), t)
        })
        .collect();
    scored.sort_by(|(sa, a), (sb, b)| {
        sb.cmp(sa)
            .then(a.tier.cmp(&b.tier))
            .then(a.name.cmp(&b.name))
    });
    scored.into_iter().take(cap).map(|(_, t)| t).collect()
}

fn kind_name(k: ParamKind) -> &'static str {
    match k {
        ParamKind::String => "string",
        ParamKind::Integer => "integer",
        ParamKind::Number => "number",
        ParamKind::Boolean => "boolean",
    }
}

/// `name(param: type, optional?: type): description` — raw, the caller sanitises.
pub(crate) fn signature(t: &ToolSpec) -> String {
    let params: Vec<String> = t
        .params
        .iter()
        .map(|p| {
            format!(
                "{}{}: {}",
                p.name,
                if p.required { "" } else { "?" },
                kind_name(p.kind)
            )
        })
        .collect();
    format!("{}({}): {}", t.name, params.join(", "), t.description)
}

/// Why a tool call cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolError {
    /// No tool with this name is offered.
    Unknown(String),
    /// `args` is not an object.
    ArgsNotObject,
    /// A required parameter is missing.
    Missing(String),
    /// A parameter has the wrong JSON type.
    WrongType(String, &'static str),
    /// A parameter the tool does not declare.
    Unexpected(String),
}

fn short(s: &str) -> String {
    s.chars().take(40).collect()
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::Unknown(n) => write!(f, "there is no tool \"{}\"", short(n)),
            ToolError::ArgsNotObject => write!(f, "args must be a JSON object"),
            ToolError::Missing(p) => write!(f, "missing required parameter \"{}\"", short(p)),
            ToolError::WrongType(p, k) => write!(f, "parameter \"{}\" must be a {k}", short(p)),
            ToolError::Unexpected(p) => write!(f, "the tool has no parameter \"{}\"", short(p)),
        }
    }
}

/// Coerces a value to the declared kind (numbers written as strings and the like), or `None`.
fn coerce(v: &Value, kind: ParamKind) -> Option<Value> {
    match (kind, v) {
        (ParamKind::String, Value::String(_)) => Some(v.clone()),
        (ParamKind::String, Value::Number(n)) => Some(Value::String(n.to_string())),
        (ParamKind::Integer, Value::Number(n)) if n.is_i64() || n.is_u64() => Some(v.clone()),
        (ParamKind::Integer, Value::Number(n)) => n
            .as_f64()
            .filter(|f| f.fract() == 0.0 && f.abs() < 9.0e15)
            .map(|f| Value::from(f as i64)),
        (ParamKind::Integer, Value::String(s)) => s.trim().parse::<i64>().ok().map(Value::from),
        (ParamKind::Number, Value::Number(_)) => Some(v.clone()),
        (ParamKind::Number, Value::String(s)) => s
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number),
        (ParamKind::Boolean, Value::Bool(_)) => Some(v.clone()),
        (ParamKind::Boolean, Value::String(s)) => match s.trim().to_lowercase().as_str() {
            "true" | "yes" => Some(Value::Bool(true)),
            "false" | "no" => Some(Value::Bool(false)),
            _ => None,
        },
        _ => None,
    }
}

/// Checks a call against the offered tools and returns the spec and normalised arguments.
pub(crate) fn check_call<'a>(
    tools: &'a [ToolSpec],
    name: &str,
    args: &Value,
) -> Result<(&'a ToolSpec, Value), ToolError> {
    let spec = tools
        .iter()
        .find(|t| t.name == name)
        .or_else(|| {
            tools
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(name.trim()))
        })
        .ok_or_else(|| ToolError::Unknown(name.to_owned()))?;
    let empty = Map::new();
    let given = match args {
        Value::Object(m) => m,
        Value::Null => &empty,
        _ => return Err(ToolError::ArgsNotObject),
    };
    if let Some(extra) = given
        .keys()
        .find(|k| !spec.params.iter().any(|p| p.name == **k))
    {
        return Err(ToolError::Unexpected(extra.clone()));
    }
    let mut out = Map::new();
    for p in &spec.params {
        match given.get(&p.name) {
            None | Some(Value::Null) if p.required => {
                return Err(ToolError::Missing(p.name.clone()))
            }
            None | Some(Value::Null) => {}
            Some(v) => {
                let c = coerce(v, p.kind)
                    .ok_or_else(|| ToolError::WrongType(p.name.clone(), kind_name(p.kind)))?;
                out.insert(p.name.clone(), c);
            }
        }
    }
    Ok((spec, Value::Object(out)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvis_proto::{RiskCategory, Tier, ToolParam};
    use serde_json::json;

    fn tool(name: &str, desc: &str, tier: Tier, app: Option<&str>) -> ToolSpec {
        ToolSpec {
            name: name.into(),
            description: desc.into(),
            params: vec![
                ToolParam {
                    name: "title".into(),
                    description: String::new(),
                    kind: ParamKind::String,
                    required: true,
                },
                ToolParam {
                    name: "minutes".into(),
                    description: String::new(),
                    kind: ParamKind::Integer,
                    required: false,
                },
            ],
            tier,
            risk: RiskCategory::None,
            app: app.map(str::to_owned),
        }
    }

    #[test]
    fn narrows_by_app_and_ranks_by_overlap() {
        let tools = vec![
            tool("music_play", "Play a song", Tier::Native, None),
            tool("reminder_add", "Add a reminder", Tier::Native, None),
            tool(
                "notes_append",
                "Append to a note",
                Tier::Shortcut,
                Some("com.apple.mobilenotes"),
            ),
            tool("timer_start", "Start a timer", Tier::Shortcut, None),
        ];
        let r = relevant(
            &tools,
            "com.apple.Preferences",
            "add a reminder to buy milk",
            2,
        );
        let names: Vec<&str> = r.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["reminder_add", "music_play"]);
        let r = relevant(&tools, "com.apple.mobilenotes", "append to note", 10);
        assert_eq!(r.first().unwrap().name, "notes_append");
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn validates_and_coerces_args() {
        let tools = vec![tool("reminder_add", "Add", Tier::Native, None)];
        let (_, args) = check_call(
            &tools,
            "reminder_add",
            &json!({"title": "Milk", "minutes": "5"}),
        )
        .unwrap();
        assert_eq!(args, json!({"title": "Milk", "minutes": 5}));
        assert_eq!(
            check_call(&tools, "nope", &json!({})).unwrap_err(),
            ToolError::Unknown("nope".into())
        );
        assert_eq!(
            check_call(&tools, "reminder_add", &json!({})).unwrap_err(),
            ToolError::Missing("title".into())
        );
        assert_eq!(
            check_call(&tools, "reminder_add", &json!({"title": "x", "when": 1})).unwrap_err(),
            ToolError::Unexpected("when".into())
        );
        assert_eq!(
            check_call(
                &tools,
                "reminder_add",
                &json!({"title": "x", "minutes": "soon"})
            )
            .unwrap_err(),
            ToolError::WrongType("minutes".into(), "integer")
        );
        assert_eq!(
            check_call(&tools, "reminder_add", &json!([1])).unwrap_err(),
            ToolError::ArgsNotObject
        );
    }

    /// Sixteenth review (F7): the gate reads a call's arguments as flat text and numbers;
    /// `check_call` is what keeps them flat. Nested values, undeclared keys and values of the
    /// wrong kind never reach it.
    #[test]
    fn only_declared_flat_parameters_pass() {
        let tools = vec![tool("reminder_add", "Add", Tier::Native, None)];
        for bad in [
            json!({"title": {"a": "x"}}),
            json!({"title": ["x"]}),
            json!({"title": "x", "minutes": [5]}),
            json!({"title": "x", "minutes": {"n": 5}}),
        ] {
            assert!(
                matches!(
                    check_call(&tools, "reminder_add", &bad),
                    Err(ToolError::WrongType(..))
                ),
                "{bad}"
            );
        }
        assert_eq!(
            check_call(
                &tools,
                "reminder_add",
                &json!({"title": "x", "minutes": 5, "extra": {"k": 1}})
            )
            .unwrap_err(),
            ToolError::Unexpected("extra".into())
        );
        // What passes holds the declared parameters only, each a string or a number.
        let (_, args) = check_call(
            &tools,
            "reminder_add",
            &json!({"title": 482913, "minutes": "5"}),
        )
        .unwrap();
        assert_eq!(args, json!({"title": "482913", "minutes": 5}));
    }
}
