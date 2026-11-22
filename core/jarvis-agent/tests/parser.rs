//! Table-driven parser tests over realistic small-model outputs.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_agent::{parse_model_output, ParseError};
use jarvis_proto::{Action, Direction};
use serde_json::json;

fn tap(e: u32) -> Action {
    Action::Tap { element: e }
}

fn ty(e: Option<u32>, t: &str, submit: bool) -> Action {
    Action::Type {
        element: e,
        text: t.into(),
        submit,
    }
}

#[test]
fn accepted_outputs() {
    let cases: Vec<(&str, Action)> = vec![
        // Canonical.
        (
            r#"{"thought":"open wifi","action":"tap","element":4}"#,
            tap(4),
        ),
        // Code fences and prose around.
        (
            "```json\n{\"action\": \"tap\", \"element\": 4}\n```",
            tap(4),
        ),
        (
            "Sure! Here is my answer:\n{\"action\":\"tap\",\"element\":4}\nLet me know.",
            tap(4),
        ),
        ("```\n{'action': 'tap', 'element': 4}\n```", tap(4)),
        // Single quotes, trailing commas, unquoted keys and values.
        (
            "{'thought': \"it's the Wi-Fi row\", 'action': 'tap', 'element': 4,}",
            tap(4),
        ),
        ("{action: tap, element: 4}", tap(4)),
        ("{\"action\" = \"tap\", \"element\" = 4}", tap(4)),
        // Numbers as strings and odd number forms.
        (r#"{"action":"tap","element":"4"}"#, tap(4)),
        (r#"{"action":"tap","element":"[4]"}"#, tap(4)),
        (r##"{"action":"tap","element":"#4"}"##, tap(4)),
        (r#"{"action":"tap","element":4.0}"#, tap(4)),
        // Alternate keys.
        (r#"{"action":"tap","index":4}"#, tap(4)),
        (r#"{"action":"tap","id":4}"#, tap(4)),
        (r#"{"action":"tap","target":"4"}"#, tap(4)),
        (r#"{"action_type":"tap","element":4}"#, tap(4)),
        (r#"{"type":"tap","element":4}"#, tap(4)),
        (r#"{"name":"tap","element":4}"#, tap(4)),
        (r#"{"command":"TAP","element_id":4}"#, tap(4)),
        // Alternate spellings.
        (r#"{"action":"click","element":4}"#, tap(4)),
        (r#"{"action":"Tap","element":4}"#, tap(4)),
        (
            r#"{"action":"longPress","element":4}"#,
            Action::LongPress {
                element: 4,
                duration_ms: 800,
            },
        ),
        (
            r#"{"action":"long-press","element":4,"duration":1200}"#,
            Action::LongPress {
                element: 4,
                duration_ms: 1200,
            },
        ),
        // Nested forms.
        (r#"{"action":{"type":"tap","element":4}}"#, tap(4)),
        (r#"{"function":"tap","arguments":{"element":4}}"#, tap(4)),
        (
            r#"{"thought":"x","actions":[{"action":"tap","element":4}]}"#,
            tap(4),
        ),
        // Type variants.
        (
            r#"{"action":"type","element":2,"text":"hello","submit":true}"#,
            ty(Some(2), "hello", true),
        ),
        (
            r#"{"action":"type","index":2,"input":"hello"}"#,
            ty(Some(2), "hello", false),
        ),
        (
            r#"{"action":"type_text","value":"hello","submit":"true"}"#,
            ty(None, "hello", true),
        ),
        (
            r#"{"action":"input","text":12345}"#,
            ty(None, "12345", false),
        ),
        (
            r#"{"action":"type","text":"line\"quote\" and \\ slash"}"#,
            ty(None, "line\"quote\" and \\ slash", false),
        ),
        (
            r#"{"action":"type","text":"{{profile.email}}"}"#,
            ty(None, "{{profile.email}}", false),
        ),
        (
            r#"{"action":"type","text":"caf\u00e9"}"#,
            ty(None, "café", false),
        ),
        (
            r#"{"action":"type","element":2,"text":"hi","enter":1}"#,
            ty(Some(2), "hi", true),
        ),
        // Call syntax.
        ("tap(17)", tap(17)),
        ("Action: tap(17)", tap(17)),
        ("I will tap(17) now.", tap(17)),
        ("type(4, \"hi\")", ty(Some(4), "hi", false)),
        ("type(4, \"hi\", true)", ty(Some(4), "hi", true)),
        ("type(\"hi\")", ty(None, "hi", false)),
        ("type(\"hi\", submit=true)", ty(None, "hi", true)),
        (
            "type(4, \"{{profile.email}}\")",
            ty(Some(4), "{{profile.email}}", false),
        ),
        (
            "done(\"sent\")",
            Action::Done {
                summary: "sent".into(),
            },
        ),
        (
            "done()",
            Action::Done {
                summary: String::new(),
            },
        ),
        (
            "fail(reason=\"no signal\")",
            Action::Fail {
                reason: "no signal".into(),
            },
        ),
        (
            "scroll(down)",
            Action::Scroll {
                direction: Direction::Down,
                element: None,
            },
        ),
        (
            "scroll(\"up\", 3)",
            Action::Scroll {
                direction: Direction::Up,
                element: Some(3),
            },
        ),
        (
            "swipe(5, left)",
            Action::Swipe {
                direction: Direction::Left,
                element: Some(5),
            },
        ),
        ("press_home()", Action::PressHome),
        ("look()", Action::Look),
        (
            "open_app(\"Settings\")",
            Action::OpenApp {
                app: "Settings".into(),
            },
        ),
        ("wait(500)", Action::Wait { ms: 500 }),
        ("tap_point(0.5, 0.25)", Action::TapPoint { x: 0.5, y: 0.25 }),
        // Other actions with aliases.
        (
            r#"{"action":"scroll","direction":"DOWN"}"#,
            Action::Scroll {
                direction: Direction::Down,
                element: None,
            },
        ),
        (
            r#"{"action":"swipe","dir":"leftwards","index":5}"#,
            Action::Swipe {
                direction: Direction::Left,
                element: Some(5),
            },
        ),
        (
            r#"{"action":"tap_point","x":"0.5","y":0.75}"#,
            Action::TapPoint { x: 0.5, y: 0.75 },
        ),
        (r#"{"action":"home"}"#, Action::PressHome),
        (
            r#"{"action":"open_app","name":"Mail"}"#,
            Action::OpenApp { app: "Mail".into() },
        ),
        (
            r#"{"action":"launch","app_name":"com.apple.mobilemail"}"#,
            Action::OpenApp {
                app: "com.apple.mobilemail".into(),
            },
        ),
        (
            r#"{"action":"open_url","link":"https://example.com"}"#,
            Action::OpenUrl {
                url: "https://example.com".into(),
            },
        ),
        (
            r#"{"action":"wait","seconds":2}"#,
            Action::Wait { ms: 2000 },
        ),
        (r#"{"action":"wait","ms":"300"}"#, Action::Wait { ms: 300 }),
        (
            r#"{"action":"call_tool","name":"reminder_add","args":{"title":"Milk","n":2}}"#,
            Action::CallTool {
                name: "reminder_add".into(),
                args: json!({"title": "Milk", "n": 2}),
            },
        ),
        (
            r#"{"action":"call_tool","tool":"weather","arguments":"{\"city\": \"Paris\"}"}"#,
            Action::CallTool {
                name: "weather".into(),
                args: json!({"city": "Paris"}),
            },
        ),
        (
            r#"{"action":{"type":"call_tool","name":"weather"}}"#,
            Action::CallTool {
                name: "weather".into(),
                args: json!({}),
            },
        ),
        (
            r#"{"action":"ask_user","question":"Which Alex?"}"#,
            Action::AskUser {
                question: "Which Alex?".into(),
            },
        ),
        (
            r#"{"action":"ask","message":"Which Alex?"}"#,
            Action::AskUser {
                question: "Which Alex?".into(),
            },
        ),
        (r#"{"action":"look"}"#, Action::Look),
        (r#"{"action":"screenshot"}"#, Action::Look),
        (
            r#"{"action":"done","summary":"Wi-Fi is off"}"#,
            Action::Done {
                summary: "Wi-Fi is off".into(),
            },
        ),
        (
            r#"{"action":"finish","result":"ok"}"#,
            Action::Done {
                summary: "ok".into(),
            },
        ),
        (
            r#"{"action":"done","summary":"a","message":"b"}"#,
            Action::Done {
                summary: "a".into(),
            },
        ),
        (
            r#"{"action":"fail","reason":"no contact"}"#,
            Action::Fail {
                reason: "no contact".into(),
            },
        ),
        (
            r#"{"action":"give_up"}"#,
            Action::Fail {
                reason: String::new(),
            },
        ),
        // Proto wire form.
        (
            r#"{"type":"type","element":2,"text":"x"}"#,
            ty(Some(2), "x", false),
        ),
        // The same action twice is fine.
        (
            "{\"action\":\"tap\",\"element\":4}\n{\"action\":\"tap\",\"element\":4}",
            tap(4),
        ),
        ("tap(4) {\"action\":\"tap\",\"element\":4}", tap(4)),
        // Unrelated `type` words are ignored.
        (r#"{"action":"tap","element":4,"type":"button"}"#, tap(4)),
        // Braces inside strings do not confuse the scanner.
        (
            r#"{"thought":"the {menu} is } here","action":"tap","element":4}"#,
            tap(4),
        ),
        // Python-ish literals.
        (
            "{'action': 'type', 'text': 'x', 'submit': True, 'element': None}",
            ty(None, "x", true),
        ),
    ];
    for (src, want) in cases {
        match parse_model_output(src) {
            Ok(p) => assert_eq!(p.action, want, "input: {src}"),
            Err(e) => panic!("input {src:?} failed: {e}"),
        }
    }
}

#[test]
fn thoughts_are_captured() {
    let p = parse_model_output(r#"{"reasoning":"  look for Send  ","action":"tap","element":8}"#)
        .unwrap();
    assert_eq!(p.thought.as_deref(), Some("look for Send"));
    assert_eq!(parse_model_output("tap(1)").unwrap().thought, None);
}

#[test]
fn rejected_outputs() {
    let cases: Vec<(&str, ParseError)> = vec![
        ("", ParseError::Empty),
        ("   \n\t", ParseError::Empty),
        ("I would tap the Wi-Fi row.", ParseError::NoAction),
        (
            "{\"thought\":\"I need to think about this for a wh",
            ParseError::Truncated,
        ),
        ("{\"action\":\"tap\",\"element\":4", ParseError::Truncated),
        ("{\"action\": tap, element: [}", ParseError::Malformed),
        (r#"{"thought":"hmm"}"#, ParseError::MissingAction),
        (
            r#"{"action":"explode","element":4}"#,
            ParseError::UnknownAction("explode".into()),
        ),
        (
            r#"{"action":"tap"}"#,
            ParseError::MissingField {
                action: "tap",
                field: "element",
            },
        ),
        (
            r#"{"action":"type","element":3}"#,
            ParseError::MissingField {
                action: "type",
                field: "text",
            },
        ),
        (
            r#"{"action":"scroll"}"#,
            ParseError::MissingField {
                action: "scroll",
                field: "direction",
            },
        ),
        (
            r#"{"action":"open_app"}"#,
            ParseError::MissingField {
                action: "open_app",
                field: "app",
            },
        ),
        (
            r#"{"action":"ask_user"}"#,
            ParseError::MissingField {
                action: "ask_user",
                field: "question",
            },
        ),
        (
            r#"{"action":"tap","element":"the send button"}"#,
            ParseError::InvalidField {
                action: "tap",
                field: "element",
                problem: "must be an element number",
            },
        ),
        (
            r#"{"action":"tap","element":-3}"#,
            ParseError::InvalidField {
                action: "tap",
                field: "element",
                problem: "must be an element number",
            },
        ),
        (
            r#"{"action":"tap","element":1e300}"#,
            ParseError::InvalidField {
                action: "tap",
                field: "element",
                problem: "must be an element number",
            },
        ),
        (
            r#"{"action":"scroll","direction":"sideways"}"#,
            ParseError::InvalidField {
                action: "scroll",
                field: "direction",
                problem: "must be up, down, left or right",
            },
        ),
        (
            r#"{"action":"type","text":"x","submit":"maybe"}"#,
            ParseError::InvalidField {
                action: "type",
                field: "submit",
                problem: "must be true or false",
            },
        ),
        (
            r#"{"action":"tap","element":4,"index":5}"#,
            ParseError::ConflictingField {
                action: "tap",
                field: "element",
            },
        ),
        (
            r#"{"action":"tap","element":4,"element":5}"#,
            ParseError::ConflictingField {
                action: "tap",
                field: "element",
            },
        ),
        // Two different actions: never guess.
        (
            "{\"action\":\"tap\",\"element\":4}\n{\"action\":\"tap\",\"element\":5}",
            ParseError::Ambiguous {
                first: "tap",
                second: "tap",
            },
        ),
        (
            "{\"action\":\"tap\",\"element\":4} then done(\"ok\")",
            ParseError::Ambiguous {
                first: "tap",
                second: "done",
            },
        ),
        (
            r#"{"action":"tap","type":"done","element":4}"#,
            ParseError::Ambiguous {
                first: "tap",
                second: "done",
            },
        ),
        (
            r#"{"actions":[{"action":"tap","element":1},{"action":"tap","element":2}]}"#,
            ParseError::Ambiguous {
                first: "tap",
                second: "tap",
            },
        ),
        (
            "done(\"ok\") fail(\"no\")",
            ParseError::Ambiguous {
                first: "done",
                second: "fail",
            },
        ),
        (
            "tap(\"send\")",
            ParseError::InvalidField {
                action: "tap",
                field: "element",
                problem: "must be an element number",
            },
        ),
        (
            "done(\"a\", \"b\")",
            ParseError::InvalidField {
                action: "done",
                field: "summary",
                problem: "has too many arguments",
            },
        ),
    ];
    for (src, want) in cases {
        assert_eq!(
            parse_model_output(src).map(|p| p.action),
            Err(want),
            "input: {src:?}"
        );
    }
}

#[test]
fn errors_read_well_as_corrections() {
    let e = parse_model_output(r#"{"action":"tap"}"#).unwrap_err();
    assert_eq!(e.to_string(), "tap needs \"element\"");
    let e = parse_model_output("tap(1) tap(2)").unwrap_err();
    assert_eq!(
        e.to_string(),
        "two different actions (tap and tap); give exactly one"
    );
}

#[test]
fn pathological_inputs_do_not_panic() {
    let long = "a".repeat(100_000);
    let deep = "{\"a\":".repeat(5_000);
    let quotes = "'\"".repeat(10_000);
    let calls = "tap(".repeat(5_000);
    for s in [
        long.as_str(),
        deep.as_str(),
        quotes.as_str(),
        calls.as_str(),
        "\u{0}",
        "{\"\\u",
        "{\"\\ud800\\u",
        "{'a':'\\",
        "))))((((",
        "tap(1",
        "type(,,,)",
        "{\"action\":\"type\",\"text\":\"\\uZZZZ\"}",
        "{\"action\":\"wait\",\"ms\":-5}",
        "{\"action\":\"wait\",\"seconds\":1e309}",
        "\u{feff}{\"action\":\"look\"}",
    ] {
        let _ = parse_model_output(s);
    }
    assert_eq!(
        parse_model_output("\u{feff}{\"action\":\"look\"}")
            .unwrap()
            .action,
        Action::Look
    );
}
