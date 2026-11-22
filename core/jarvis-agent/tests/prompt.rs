//! Prompt, schema and grammar: budget, snapshot, and that every canonical form parses.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_agent::{
    action_examples, action_gbnf, action_json_schema, action_names, parse_model_output,
    system_prompt,
};
use jarvis_memory::MemoryStore;
use jarvis_observe::estimate_tokens;
use jarvis_proto::fixtures::settings_screen;
use jarvis_proto::{ParamKind, RiskCategory, Screen, Tier, ToolParam, ToolSpec};
use serde_json::Value;

fn tokens(r: &jarvis_proto::ModelRequest) -> u32 {
    estimate_tokens(&r.system) + estimate_tokens(&r.user)
}

#[test]
fn prompt_fits_a_4096_model_with_a_300_element_screen() {
    let mut c = config();
    c.tools = (0..12)
        .map(|i| ToolSpec {
            name: format!("tool_{i}"),
            description: "A tool with a long description that takes up some room in the prompt"
                .into(),
            params: vec![ToolParam {
                name: "value".into(),
                description: String::new(),
                kind: ParamKind::String,
                required: true,
            }],
            tier: Tier::Shortcut,
            risk: RiskCategory::None,
            app: None,
        })
        .collect();
    let screen = long_list_screen(300);
    assert!(jarvis_observe::compact(&screen).elements.len() >= 250);
    let outputs: Vec<String> = (1..30)
        .map(|i| format!(r#"{{"action":"wait","ms":{i}}}"#))
        .collect();
    let refs: Vec<&str> = outputs.iter().map(String::as_str).collect();
    let run = run(
        c,
        "Find item 250 in the list",
        MemoryStore::new(),
        Phone::still(screen),
        Script::model(&refs),
    );
    assert!(run.infers() > 10);
    for r in &run.requests {
        assert!(tokens(r) + r.max_tokens <= 4096, "{} tokens", tokens(r));
        assert!(r.user.contains("Task: Find item 250 in the list"));
        assert!(r.system.contains("Rules:"));
        assert!(
            r.user.contains("more elements below; scroll down") || r.user.contains("…\""),
            "screen was degraded to fit"
        );
    }
    // The screen got most of the room, and history survived.
    let last = run.requests.last().unwrap();
    assert!(tokens(last) > 3000, "{}", tokens(last));
    assert!(last.user.contains("Steps so far:"));
}

#[test]
fn tiny_context_drops_history_and_tools_before_the_screen_floor() {
    let mut c = config();
    c.model.context_tokens = 1700;
    c.tools = vec![ToolSpec {
        name: "music_play".into(),
        description: "Play music".into(),
        params: vec![],
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
    }];
    let outputs: Vec<String> = (1..20)
        .map(|i| format!(r#"{{"action":"wait","ms":{i}}}"#))
        .collect();
    let refs: Vec<&str> = outputs.iter().map(String::as_str).collect();
    let run = run(
        c,
        "Find item 3",
        MemoryStore::new(),
        Phone::still(long_list_screen(300)),
        Script::model(&refs),
    );
    for r in &run.requests {
        assert!(tokens(r) + r.max_tokens <= 1700, "{}", tokens(r));
        assert!(r.user.contains("Task: Find item 3"));
    }
}

fn snapshot_run() -> Run {
    let mut c = config();
    c.tools = vec![ToolSpec {
        name: "wifi_set".into(),
        description: "Turn Wi-Fi on or off".into(),
        params: vec![ToolParam {
            name: "on".into(),
            description: String::new(),
            kind: ParamKind::Boolean,
            required: true,
        }],
        tier: Tier::Shortcut,
        risk: RiskCategory::None,
        app: None,
    }];
    let mut memory = MemoryStore::new();
    memory.profile.set("email", "alex@example.com", false);
    let phone = Phone::new(settings_screen(), |cmd, s: &Screen| {
        if taps(cmd, s, "Wi-Fi, HomeNet") {
            Transition::Go(wifi_screen(true))
        } else {
            Transition::Stay
        }
    });
    run(
        c,
        "Turn off Wi-Fi",
        memory,
        phone,
        Script::model(&[r#"{"action":"tap","element":4}"#, r#"{"action":"done"}"#]),
    )
}

#[test]
fn full_prompt_snapshot() {
    let run = snapshot_run();
    let r = &run.requests[1];
    let actual = format!("=== SYSTEM ===\n{}\n=== USER ===\n{}\n", r.system, r.user);
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/snapshots/wifi_prompt.txt"
    );
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots")).unwrap();
        std::fs::write(path, &actual).unwrap();
    }
    let expected =
        std::fs::read_to_string(path).expect("snapshot missing; run with UPDATE_SNAPSHOTS=1");
    assert_eq!(
        actual, expected,
        "prompt changed; review and rerun with UPDATE_SNAPSHOTS=1"
    );
}

#[test]
fn every_prompt_example_parses_to_its_action() {
    let names = action_names();
    for (ex, name) in action_examples().iter().zip(&names) {
        let p = parse_model_output(ex).unwrap_or_else(|e| panic!("{ex}: {e}"));
        assert_eq!(p.action.name(), *name, "{ex}");
        // The canonical serialisation of the parsed action round-trips through the parser.
        let with_thought = ex.replacen('{', "{\"thought\":\"because\",", 1);
        let p2 = parse_model_output(&with_thought).unwrap();
        assert_eq!(p2.action, p.action);
        assert_eq!(p2.thought.as_deref(), Some("because"));
        assert!(system_prompt().contains(ex));
    }
}

#[test]
fn grammar_mentions_every_action_and_field_in_order() {
    let g = action_gbnf();
    for name in action_names() {
        assert!(
            g.contains(&format!("\\\"{name}\\\"")),
            "grammar lacks {name}"
        );
    }
    // Every example's keys appear in its grammar rule in the same order.
    for ex in action_examples() {
        let v: serde_json::Map<String, Value> = serde_json::from_str(ex).unwrap();
        let name = v["action"].as_str().unwrap();
        let rule = g
            .lines()
            .find(|l| l.starts_with(&format!("act-{} ::=", name.replace('_', "-"))))
            .unwrap();
        let mut at = 0;
        for key in keys_in_order(ex) {
            let needle = format!("\\\"{key}\\\"");
            let pos = rule[at..]
                .find(&needle)
                .unwrap_or_else(|| panic!("{name}: {key} out of order in {rule}"));
            at += pos + needle.len();
        }
    }
    assert!(g.starts_with("root ::="));
}

/// Top-level keys of a flat example object, in textual order.
fn keys_in_order(ex: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut depth = 0;
    let mut chars = ex.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            '"' => {
                let end = ex[i + 1..].find('"').unwrap() + i + 1;
                let s = &ex[i + 1..end];
                let after = ex[end + 1..].trim_start();
                if depth == 1 && after.starts_with(':') {
                    keys.push(s.to_owned());
                }
                while chars.peek().is_some_and(|(j, _)| *j <= end) {
                    chars.next();
                }
            }
            _ => {}
        }
    }
    keys
}

/// A validator for the subset of JSON schema `action_json_schema` uses.
fn valid(schema: &Value, v: &Value) -> bool {
    if let Some(alts) = schema.get("oneOf").and_then(Value::as_array) {
        return alts.iter().filter(|s| valid(s, v)).count() == 1;
    }
    if let Some(c) = schema.get("const") {
        return c == v;
    }
    if let Some(e) = schema.get("enum").and_then(Value::as_array) {
        if !e.contains(v) {
            return false;
        }
    }
    let ty = schema.get("type").and_then(Value::as_str);
    let ok_type = match ty {
        Some("object") => v.is_object(),
        Some("string") => v.is_string(),
        Some("integer") => v.is_i64() || v.is_u64(),
        Some("number") => v.is_number(),
        Some("boolean") => v.is_boolean(),
        _ => true,
    };
    if !ok_type {
        return false;
    }
    if let Some(n) = v.as_f64() {
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n < m)
        {
            return false;
        }
        if schema
            .get("maximum")
            .and_then(Value::as_f64)
            .is_some_and(|m| n > m)
        {
            return false;
        }
    }
    if let (Some(obj), Some(props)) = (
        v.as_object(),
        schema.get("properties").and_then(Value::as_object),
    ) {
        for r in schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !obj.contains_key(r.as_str().unwrap()) {
                return false;
            }
        }
        for (k, val) in obj {
            match props.get(k) {
                Some(s) => {
                    if !valid(s, val) {
                        return false;
                    }
                }
                None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    return false
                }
                None => {}
            }
        }
    }
    true
}

#[test]
fn schema_accepts_every_example_exactly_once_and_rejects_junk() {
    let schema: Value = serde_json::from_str(&action_json_schema()).unwrap();
    for ex in action_examples() {
        let v: Value = serde_json::from_str(ex).unwrap();
        assert!(valid(&schema, &v), "{ex}");
    }
    for bad in [
        r#"{"action":"tap"}"#,
        r#"{"action":"tap","element":0}"#,
        r#"{"action":"tap","element":"3"}"#,
        r#"{"action":"fly"}"#,
        r#"{"action":"scroll","direction":"sideways"}"#,
        r#"{"action":"tap","element":3,"extra":1}"#,
        r#"{"action":"tap_point","x":1.5,"y":0.2}"#,
        r#"{"action":"wait","ms":100000}"#,
    ] {
        let v: Value = serde_json::from_str(bad).unwrap();
        assert!(!valid(&schema, &v), "{bad}");
    }
    assert_eq!(
        schema["oneOf"].as_array().unwrap().len(),
        action_names().len()
    );
}

#[test]
fn optional_fields_parse_with_defaults() {
    use jarvis_proto::{Action, Direction};
    let cases = [
        (
            r#"{"action":"long_press","element":2}"#,
            Action::LongPress {
                element: 2,
                duration_ms: 800,
            },
        ),
        (
            r#"{"action":"type","text":"x"}"#,
            Action::Type {
                element: None,
                text: "x".into(),
                submit: false,
            },
        ),
        (
            r#"{"action":"scroll","direction":"up","element":3}"#,
            Action::Scroll {
                direction: Direction::Up,
                element: Some(3),
            },
        ),
        (r#"{"action":"wait"}"#, Action::Wait { ms: 1000 }),
        (
            r#"{"action":"call_tool","name":"t"}"#,
            Action::CallTool {
                name: "t".into(),
                args: serde_json::json!({}),
            },
        ),
        (
            r#"{"action":"done"}"#,
            Action::Done {
                summary: String::new(),
            },
        ),
    ];
    let schema: Value = serde_json::from_str(&action_json_schema()).unwrap();
    for (src, want) in cases {
        assert_eq!(parse_model_output(src).unwrap().action, want, "{src}");
        assert!(valid(&schema, &serde_json::from_str(src).unwrap()), "{src}");
    }
}
