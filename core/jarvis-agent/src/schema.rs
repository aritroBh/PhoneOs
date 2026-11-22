//! The canonical action form, described once and exported as a JSON schema, a GBNF grammar
//! and the one-line examples shown to the model.
//!
//! Canonical form: one flat JSON object, an optional `"thought"` string first, then
//! `"action"` with the action name, then the action's fields in the order of [`ACTIONS`].
//! For example `{"thought":"open Wi-Fi","action":"tap","element":3}`. The parser accepts
//! far more than this (see `parser`), but everything the schema and grammar allow parses to
//! the action it spells.

use serde_json::{json, Map, Value};

/// JSON shape of one action field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldKind {
    /// A 1-based element index (integer).
    Element,
    /// A string.
    Text,
    /// A boolean.
    Bool,
    /// `"up"`, `"down"`, `"left"` or `"right"`.
    Direction,
    /// A number in 0..1.
    Fraction,
    /// A non-negative integer of milliseconds.
    Millis,
    /// A JSON object (tool arguments).
    Object,
}

/// One field of an action.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldSpec {
    /// Canonical key.
    pub name: &'static str,
    /// Value shape.
    pub kind: FieldKind,
    /// Whether the canonical form requires it.
    pub required: bool,
    /// Other keys small models use for it (the parser accepts these too).
    pub aliases: &'static [&'static str],
}

/// One action of the canonical form. Examples use `<placeholder>` text for every free-text
/// field, never a name, contact, app or result: small models copy example strings into their
/// answers ("No contact named ..."), and a placeholder cannot pass for a real answer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ActionSpec {
    /// Canonical name (the `"action"` value), identical to `Action::name()`.
    pub name: &'static str,
    /// Other spellings the parser maps to this action (already normalised to snake_case).
    pub aliases: &'static [&'static str],
    /// Fields in canonical order.
    pub fields: &'static [FieldSpec],
    /// Canonical one-line example shown in the system prompt.
    pub example: &'static str,
}

const fn field(
    name: &'static str,
    kind: FieldKind,
    required: bool,
    aliases: &'static [&'static str],
) -> FieldSpec {
    FieldSpec {
        name,
        kind,
        required,
        aliases,
    }
}

const ELEMENT_ALIASES: &[&str] = &[
    "index",
    "id",
    "target",
    "element_id",
    "element_index",
    "elem",
    "idx",
];
const TEXT_ALIASES: &[&str] = &["input", "value", "content", "string"];
const SUBMIT_ALIASES: &[&str] = &["enter", "press_enter", "press_return", "return"];
const DIRECTION_ALIASES: &[&str] = &["dir", "to", "way"];
const ELEMENT_REQ: FieldSpec = field("element", FieldKind::Element, true, ELEMENT_ALIASES);
const ELEMENT_OPT: FieldSpec = field("element", FieldKind::Element, false, ELEMENT_ALIASES);
const DIRECTION: FieldSpec = field("direction", FieldKind::Direction, true, DIRECTION_ALIASES);

/// Every action, in the order the prompt lists them.
pub(crate) const ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "tap",
        aliases: &["click", "press", "tap_element", "select", "touch"],
        fields: &[ELEMENT_REQ],
        example: r#"{"action":"tap","element":17}"#,
    },
    ActionSpec {
        name: "long_press",
        aliases: &["longpress", "hold", "press_and_hold", "long_tap"],
        fields: &[
            ELEMENT_REQ,
            field(
                "duration_ms",
                FieldKind::Millis,
                false,
                &["duration", "ms", "milliseconds"],
            ),
        ],
        example: r#"{"action":"long_press","element":17,"duration_ms":800}"#,
    },
    ActionSpec {
        name: "type",
        aliases: &[
            "type_text",
            "input",
            "input_text",
            "enter_text",
            "write",
            "fill",
            "typing",
        ],
        fields: &[
            ELEMENT_OPT,
            field("text", FieldKind::Text, true, TEXT_ALIASES),
            field("submit", FieldKind::Bool, false, SUBMIT_ALIASES),
        ],
        example: r#"{"action":"type","element":4,"text":"<text>","submit":false}"#,
    },
    ActionSpec {
        name: "clear",
        aliases: &["clear_text", "erase", "clear_field"],
        fields: &[ELEMENT_REQ],
        example: r#"{"action":"clear","element":4}"#,
    },
    ActionSpec {
        name: "scroll",
        aliases: &["scroll_screen"],
        fields: &[DIRECTION, ELEMENT_OPT],
        example: r#"{"action":"scroll","direction":"down"}"#,
    },
    ActionSpec {
        name: "swipe",
        aliases: &["fling", "drag"],
        fields: &[DIRECTION, ELEMENT_OPT],
        example: r#"{"action":"swipe","direction":"left","element":6}"#,
    },
    ActionSpec {
        name: "tap_point",
        aliases: &[
            "tappoint",
            "tap_xy",
            "click_point",
            "tap_at",
            "tap_coordinates",
        ],
        fields: &[
            field("x", FieldKind::Fraction, true, &[]),
            field("y", FieldKind::Fraction, true, &[]),
        ],
        example: r#"{"action":"tap_point","x":0.5,"y":0.3}"#,
    },
    ActionSpec {
        name: "press_home",
        aliases: &["home", "go_home", "home_button", "press_home_button"],
        fields: &[],
        example: r#"{"action":"press_home"}"#,
    },
    ActionSpec {
        name: "open_app",
        aliases: &["launch", "launch_app", "open", "start_app", "openapp"],
        fields: &[field(
            "app",
            FieldKind::Text,
            true,
            &["app_name", "bundle_id", "name", "application"],
        )],
        example: r#"{"action":"open_app","app":"<app name>"}"#,
    },
    ActionSpec {
        name: "open_url",
        aliases: &["url", "open_link", "go_to_url", "navigate", "browse"],
        fields: &[field(
            "url",
            FieldKind::Text,
            true,
            &["link", "href", "uri"],
        )],
        example: r#"{"action":"open_url","url":"https://example.com"}"#,
    },
    ActionSpec {
        name: "wait",
        aliases: &["sleep", "pause"],
        fields: &[field(
            "ms",
            FieldKind::Millis,
            false,
            &["duration_ms", "duration", "milliseconds", "time"],
        )],
        example: r#"{"action":"wait","ms":1000}"#,
    },
    ActionSpec {
        name: "call_tool",
        aliases: &["tool", "use_tool", "run_tool", "tool_call", "calltool"],
        fields: &[
            field("name", FieldKind::Text, true, &["tool", "tool_name"]),
            field(
                "args",
                FieldKind::Object,
                false,
                &["arguments", "params", "parameters"],
            ),
        ],
        example: r#"{"action":"call_tool","name":"<tool>","args":{"<param>":"<value>"}}"#,
    },
    ActionSpec {
        name: "ask_user",
        aliases: &["ask", "question", "ask_human", "askuser"],
        fields: &[field(
            "question",
            FieldKind::Text,
            true,
            &["text", "message", "prompt", "q"],
        )],
        example: r#"{"action":"ask_user","question":"<question>"}"#,
    },
    ActionSpec {
        name: "look",
        aliases: &["screenshot", "look_at_screen", "see", "observe", "vision"],
        fields: &[],
        example: r#"{"action":"look"}"#,
    },
    ActionSpec {
        name: "done",
        aliases: &[
            "finish",
            "finished",
            "complete",
            "completed",
            "success",
            "end",
        ],
        fields: &[field(
            "summary",
            FieldKind::Text,
            false,
            &["result", "message", "text", "answer", "reason"],
        )],
        example: r#"{"action":"done","summary":"<what was done>"}"#,
    },
    ActionSpec {
        name: "fail",
        aliases: &["failed", "give_up", "abort", "error", "cannot"],
        fields: &[field(
            "reason",
            FieldKind::Text,
            false,
            &["message", "text", "error", "summary"],
        )],
        example: r#"{"action":"fail","reason":"<why it cannot be done>"}"#,
    },
];

/// Keys the parser reads the model's thought from (canonical first).
pub(crate) const THOUGHT_KEYS: &[&str] = &[
    "thought",
    "thoughts",
    "thinking",
    "think",
    "reasoning",
    "rationale",
    "explanation",
    "plan",
];

/// Longest thought the grammar allows, in characters.
pub(crate) const MAX_THOUGHT_CHARS: usize = 200;

/// Every canonical action name, in prompt order.
pub fn action_names() -> Vec<&'static str> {
    ACTIONS.iter().map(|a| a.name).collect()
}

/// The canonical one-line example of every action, in prompt order.
pub fn action_examples() -> Vec<&'static str> {
    ACTIONS.iter().map(|a| a.example).collect()
}

fn field_schema(kind: FieldKind) -> Value {
    match kind {
        FieldKind::Element => json!({"type": "integer", "minimum": 1}),
        FieldKind::Text => json!({"type": "string"}),
        FieldKind::Bool => json!({"type": "boolean"}),
        FieldKind::Direction => json!({"type": "string", "enum": ["up", "down", "left", "right"]}),
        FieldKind::Fraction => json!({"type": "number", "minimum": 0, "maximum": 1}),
        FieldKind::Millis => json!({"type": "integer", "minimum": 0, "maximum": 99_999}),
        FieldKind::Object => json!({"type": "object"}),
    }
}

/// JSON schema (draft 2020-12 subset) of the canonical action object: a `oneOf` with one
/// closed object schema per action. Keys are sorted, so the text is deterministic.
pub fn action_json_schema() -> String {
    let variants: Vec<Value> = ACTIONS
        .iter()
        .map(|a| {
            let mut props = Map::new();
            props.insert(
                "thought".to_owned(),
                json!({"type": "string", "maxLength": MAX_THOUGHT_CHARS}),
            );
            props.insert("action".to_owned(), json!({"const": a.name}));
            let mut required = vec![Value::from("action")];
            for f in a.fields {
                props.insert(f.name.to_owned(), field_schema(f.kind));
                if f.required {
                    required.push(Value::from(f.name));
                }
            }
            json!({
                "type": "object",
                "properties": props,
                "required": required,
                "additionalProperties": false,
            })
        })
        .collect();
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "JarvisAction",
        "oneOf": variants,
    })
    .to_string()
}

fn gbnf_rule_name(action: &str) -> String {
    format!("act-{}", action.replace('_', "-"))
}

fn gbnf_value_rule(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Element => "element",
        FieldKind::Text => "string",
        FieldKind::Bool => "boolean",
        FieldKind::Direction => "direction",
        FieldKind::Fraction => "fraction",
        FieldKind::Millis => "millis",
        FieldKind::Object => "object",
    }
}

/// GBNF grammar (llama.cpp dialect) of the canonical action object, one line, no extra
/// whitespace beyond single optional spaces.
pub fn action_gbnf() -> String {
    let mut out = String::new();
    out.push_str("root ::= \"{\" ws thought? action-body ws \"}\"\n");
    out.push_str(&format!(
        "thought ::= \"\\\"thought\\\"\" ws \":\" ws \"\\\"\" char{{0,{MAX_THOUGHT_CHARS}}} \"\\\"\" ws \",\" ws\n"
    ));
    let alts: Vec<String> = ACTIONS.iter().map(|a| gbnf_rule_name(a.name)).collect();
    out.push_str(&format!("action-body ::= {}\n", alts.join(" | ")));
    for a in ACTIONS {
        let mut rule = format!(
            "{} ::= \"\\\"action\\\"\" ws \":\" ws \"\\\"{}\\\"\"",
            gbnf_rule_name(a.name),
            a.name
        );
        for f in a.fields {
            let part = format!(
                "ws \",\" ws \"\\\"{}\\\"\" ws \":\" ws {}",
                f.name,
                gbnf_value_rule(f.kind)
            );
            if f.required {
                rule.push(' ');
                rule.push_str(&part);
            } else {
                rule.push_str(&format!(" ( {part} )?"));
            }
        }
        out.push_str(&rule);
        out.push('\n');
    }
    out.push_str(concat!(
        "element ::= [1-9] [0-9]{0,4}\n",
        "millis ::= [0-9]{1,5}\n",
        "fraction ::= \"0\" (\".\" [0-9]{1,4})? | \"1\" (\".\" \"0\"{1,4})?\n",
        "boolean ::= \"true\" | \"false\"\n",
        "direction ::= \"\\\"up\\\"\" | \"\\\"down\\\"\" | \"\\\"left\\\"\" | \"\\\"right\\\"\"\n",
        "string ::= \"\\\"\" char* \"\\\"\"\n",
        "char ::= [^\"\\\\\\x7F\\x00-\\x1F] | \"\\\\\" ([\"\\\\/bfnrt] | \"u\" [0-9a-fA-F]{4})\n",
        "object ::= \"{\" ws ( string ws \":\" ws value ( ws \",\" ws string ws \":\" ws value )* )? ws \"}\"\n",
        "array ::= \"[\" ws ( value ( ws \",\" ws value )* )? ws \"]\"\n",
        "value ::= object | array | string | number | boolean | \"null\"\n",
        "number ::= \"-\"? (\"0\" | [1-9] [0-9]{0,15}) (\".\" [0-9]{1,15})?\n",
        "ws ::= \" \"?\n",
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_proto_action_names() {
        use jarvis_proto::{Action, Direction};
        let all = [
            Action::Tap { element: 1 },
            Action::LongPress {
                element: 1,
                duration_ms: 1,
            },
            Action::Type {
                element: None,
                text: String::new(),
                submit: false,
            },
            Action::Clear { element: 1 },
            Action::Scroll {
                direction: Direction::Up,
                element: None,
            },
            Action::Swipe {
                direction: Direction::Up,
                element: None,
            },
            Action::TapPoint { x: 0.0, y: 0.0 },
            Action::PressHome,
            Action::OpenApp { app: String::new() },
            Action::OpenUrl { url: String::new() },
            Action::Wait { ms: 1 },
            Action::CallTool {
                name: String::new(),
                args: Value::Null,
            },
            Action::AskUser {
                question: String::new(),
            },
            Action::Look,
            Action::Done {
                summary: String::new(),
            },
            Action::Fail {
                reason: String::new(),
            },
        ];
        let names: Vec<&str> = all.iter().map(Action::name).collect();
        assert_eq!(names, action_names());
    }

    #[test]
    fn schema_is_valid_json_and_deterministic() {
        let a = action_json_schema();
        let v: Value = serde_json::from_str(&a).unwrap();
        assert_eq!(v["oneOf"].as_array().unwrap().len(), ACTIONS.len());
        assert_eq!(a, action_json_schema());
    }
}
