//! JSON wire-format tests: every variant of the tagged enums round-trips, and the shapes the
//! Swift shell depends on are pinned.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_proto::fixtures;
use jarvis_proto::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

/// Round-trips through a JSON string and returns the intermediate value.
fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(v: &T) -> Value {
    let s = serde_json::to_string(v).unwrap();
    let back: T = serde_json::from_str(&s).unwrap_or_else(|e| panic!("{s}: {e}"));
    assert_eq!(&back, v, "round trip changed value via {s}");
    serde_json::from_str(&s).unwrap()
}

fn all_actions() -> Vec<Action> {
    vec![
        Action::Tap { element: 17 },
        Action::LongPress {
            element: 3,
            duration_ms: 800,
        },
        Action::Type {
            element: Some(4),
            text: "alex@example.com".into(),
            submit: false,
        },
        Action::Type {
            element: None,
            text: "See you at 7 \u{1f44b}".into(),
            submit: true,
        },
        Action::Clear { element: 4 },
        Action::Scroll {
            direction: Direction::Down,
            element: None,
        },
        Action::Scroll {
            direction: Direction::Up,
            element: Some(2),
        },
        Action::Swipe {
            direction: Direction::Left,
            element: Some(9),
        },
        Action::Swipe {
            direction: Direction::Right,
            element: None,
        },
        Action::TapPoint { x: 0.25, y: 0.75 },
        Action::PressHome,
        Action::OpenApp {
            app: "Messages".into(),
        },
        Action::OpenUrl {
            url: "https://example.com/a?b=c".into(),
        },
        Action::Wait { ms: 500 },
        Action::CallTool {
            name: "create_reminder".into(),
            args: json!({"title": "Buy milk", "due": "2026-11-23T09:00", "priority": 1}),
        },
        Action::AskUser {
            question: "Which Alex?".into(),
        },
        Action::Look,
        Action::Done {
            summary: "Sent".into(),
        },
        Action::Fail {
            reason: "Face ID required".into(),
        },
    ]
}

fn all_runner_commands() -> Vec<RunnerCommand> {
    vec![
        RunnerCommand::Tap { x: 196.5, y: 426.0 },
        RunnerCommand::LongPress {
            x: 10.0,
            y: 20.0,
            duration_ms: 1000,
        },
        RunnerCommand::Swipe {
            from: Point::new(196.5, 700.0),
            to: Point::new(196.5, 200.0),
            duration_ms: 300,
        },
        RunnerCommand::TypeText {
            text: "hello\nworld".into(),
        },
        RunnerCommand::PressKey { key: Key::Return },
        RunnerCommand::PressKey { key: Key::Delete },
        RunnerCommand::PressKey { key: Key::Tab },
        RunnerCommand::PressKey { key: Key::Space },
        RunnerCommand::PressKey { key: Key::Escape },
        RunnerCommand::PressKey { key: Key::Home },
        RunnerCommand::LaunchApp {
            bundle_id: "com.apple.Preferences".into(),
        },
        RunnerCommand::OpenUrl {
            url: "shortcuts://run-shortcut?name=Home".into(),
        },
        RunnerCommand::Wait { ms: 250 },
    ]
}

#[test]
fn every_action_round_trips_with_name_as_tag() {
    for a in all_actions() {
        let v = round_trip(&a);
        assert_eq!(v["type"], a.name(), "{v}");
    }
}

#[test]
fn every_runner_command_round_trips_with_name_as_tag() {
    for c in all_runner_commands() {
        let v = round_trip(&c);
        assert_eq!(v["type"], c.name(), "{v}");
    }
}

#[test]
fn action_wire_shapes() {
    assert_eq!(
        serde_json::to_value(Action::Tap { element: 17 }).unwrap(),
        json!({"type": "tap", "element": 17})
    );
    assert_eq!(
        serde_json::to_value(Action::PressHome).unwrap(),
        json!({"type": "press_home"})
    );
    // Optional fields may be omitted on input.
    let a: Action = serde_json::from_value(json!({"type": "type", "text": "hi"})).unwrap();
    assert_eq!(
        a,
        Action::Type {
            element: None,
            text: "hi".into(),
            submit: false
        }
    );
    let a: Action = serde_json::from_value(json!({"type": "scroll", "direction": "down"})).unwrap();
    assert_eq!(a.element(), None);
    assert!(serde_json::from_value::<Action>(json!({"type": "teleport"})).is_err());
}

#[test]
fn action_helpers() {
    assert_eq!(Action::Clear { element: 4 }.element(), Some(4));
    assert!(Action::Done {
        summary: "x".into()
    }
    .is_terminal());
    assert!(!Action::Look.is_terminal());
    assert!(Action::PressHome.is_ui());
    assert!(!Action::AskUser {
        question: "?".into()
    }
    .is_ui());
}

fn all_events() -> Vec<Event> {
    vec![
        Event::Start,
        Event::Observed(fixtures::observation(fixtures::login_screen())),
        Event::Observed(Observation {
            screen: fixtures::settings_screen(),
            screenshot: Some(ImageRef {
                id: "shot-1".into(),
                width: 1179,
                height: 2556,
            }),
        }),
        Event::ModelOutput(ModelOutput {
            text: r#"{"thought":"tap send","action":"tap","element":12}"#.into(),
        }),
        Event::ModelFailed {
            error: "out of memory".into(),
        },
        Event::ActResult(RunnerResult::ok()),
        Event::ActResult(RunnerResult::err("element not hittable")),
        Event::ToolResult(ToolResult {
            ok: true,
            output: "Reminder created".into(),
            error: None,
        }),
        Event::UserDecision { approved: true },
        Event::UserDecision { approved: false },
        Event::UserAnswer {
            text: "Alex Kim".into(),
        },
        Event::Cancel,
    ]
}

fn all_effects() -> Vec<Effect> {
    let compose = fixtures::compose_screen();
    vec![
        Effect::Observe { screenshot: false },
        Effect::Observe { screenshot: true },
        Effect::Infer(ModelRequest {
            system: "You operate an iPhone.".into(),
            user: "Task: text Alex".into(),
            image: None,
            marks: vec![],
            max_tokens: 256,
            grammar: Some("root ::= \"{\" ws \"}\"".into()),
            json_schema: None,
            kind: RequestKind::Text,
        }),
        Effect::Infer(ModelRequest {
            system: "s".into(),
            user: "u".into(),
            image: Some(ImageRef {
                id: "shot-2".into(),
                width: 1179,
                height: 2556,
            }),
            marks: vec![Mark {
                index: 1,
                frame: compose.root.frame,
            }],
            max_tokens: 128,
            grammar: None,
            json_schema: Some("{}".into()),
            kind: RequestKind::Vision,
        }),
    ]
    .into_iter()
    .chain(all_runner_commands().into_iter().map(Effect::Act))
    .chain([
        Effect::CallTool(ToolCall {
            name: "create_event".into(),
            args: json!({"title": "Dinner", "start": "19:00"}),
        }),
        Effect::Confirm(ConfirmRequest {
            category: RiskCategory::SendMessage,
            summary: "Send \"See you at 7\" to Alex".into(),
            app: "com.apple.MobileSMS".into(),
        }),
        Effect::AskUser {
            question: "Which account?".into(),
        },
        Effect::Finished(Outcome {
            status: OutcomeStatus::Success,
            summary: "Message sent".into(),
            steps: 6,
            duration_ms: 18_400,
        }),
        Effect::Finished(Outcome {
            status: OutcomeStatus::Failed,
            summary: "stuck".into(),
            steps: 40,
            duration_ms: 1,
        }),
        Effect::Finished(Outcome {
            status: OutcomeStatus::Cancelled,
            summary: "".into(),
            steps: 0,
            duration_ms: 0,
        }),
        Effect::Finished(Outcome {
            status: OutcomeStatus::Blocked,
            summary: "denied app".into(),
            steps: 1,
            duration_ms: 5,
        }),
    ])
    .collect()
}

#[test]
fn every_event_round_trips() {
    let tags: Vec<String> = all_events()
        .iter()
        .map(|e| round_trip(e)["type"].as_str().unwrap().to_owned())
        .collect();
    for t in [
        "start",
        "observed",
        "model_output",
        "model_failed",
        "act_result",
        "tool_result",
        "user_decision",
        "user_answer",
        "cancel",
    ] {
        assert!(tags.iter().any(|x| x == t), "missing {t}");
    }
}

#[test]
fn timed_event_round_trips_and_flattens_newtype_payloads() {
    let ev = TimedEvent::new(1_234, Event::ActResult(RunnerResult::err("timeout")));
    let v = round_trip(&ev);
    assert_eq!(
        v,
        json!({"now_ms": 1234, "event": {"type": "act_result", "ok": false, "error": "timeout"}})
    );
    let v = round_trip(&TimedEvent::new(0, Event::Start));
    assert_eq!(v, json!({"now_ms": 0, "event": {"type": "start"}}));
}

#[test]
fn every_effect_round_trips_with_name_as_tag() {
    for e in all_effects() {
        let v = round_trip(&e);
        assert_eq!(v["type"], e.name(), "{v}");
        assert_eq!(e.is_finished(), e.name() == "finished");
    }
}

#[test]
fn act_effect_nests_runner_command() {
    let v = serde_json::to_value(Effect::Act(RunnerCommand::Tap { x: 1.5, y: 2.0 })).unwrap();
    assert_eq!(
        v,
        json!({"type": "act", "command": {"type": "tap", "x": 1.5, "y": 2.0}})
    );
    let e: Effect = serde_json::from_value(
        json!({"type": "act", "command": {"type": "press_key", "key": "return"}}),
    )
    .unwrap();
    assert_eq!(e, Effect::Act(RunnerCommand::PressKey { key: Key::Return }));
}

#[test]
fn unit_enums_are_snake_case_strings() {
    assert_eq!(
        serde_json::to_value(RiskCategory::ALL).unwrap(),
        json!([
            "none",
            "send_message",
            "payment",
            "purchase",
            "delete",
            "public_post",
            "account_change",
            "system_setting",
            "credential",
            "unverified",
            "call",
            "first_use"
        ])
    );
    assert_eq!(
        serde_json::to_value(PolicyMode::StepByStep).unwrap(),
        json!("step_by_step")
    );
    assert_eq!(
        serde_json::to_value(TrustLevel::Denied).unwrap(),
        json!("denied")
    );
    assert_eq!(
        serde_json::to_value(Tier::Shortcut).unwrap(),
        json!("shortcut")
    );
    assert_eq!(
        serde_json::to_value(RequestKind::Vision).unwrap(),
        json!("vision")
    );
    assert_eq!(
        serde_json::to_value(Orientation::LandscapeLeft).unwrap(),
        json!("landscape_left")
    );
    assert_eq!(
        serde_json::to_value(Phase::WaitingForUser).unwrap(),
        json!("waiting_for_user")
    );
    assert_eq!(
        serde_json::to_value(ParamKind::Boolean).unwrap(),
        json!("boolean")
    );
}

#[test]
fn session_config_and_progress_round_trip() {
    let cfg = SessionConfig {
        policy: Policy::default(),
        tools: vec![ToolSpec {
            name: "send_message".into(),
            description: "Send an iMessage".into(),
            params: vec![
                ToolParam {
                    name: "to".into(),
                    description: "Recipient".into(),
                    kind: ParamKind::String,
                    required: true,
                },
                ToolParam {
                    name: "body".into(),
                    description: "Text".into(),
                    kind: ParamKind::String,
                    required: true,
                },
            ],
            tier: Tier::Shortcut,
            risk: RiskCategory::SendMessage,
            app: Some("com.apple.MobileSMS".into()),
        }],
        model: ModelProfile {
            context_tokens: 8192,
            supports_vision: true,
            supports_grammar: true,
        },
        known_apps: vec![AppInfo::new("com.apple.MobileSMS", "Messages")],
        agent_bundle_id: Some("com.example.jarvis".into()),
    };
    round_trip(&cfg);
    // Everything in SessionConfig is defaulted, so an empty object is a valid config.
    let empty: SessionConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, SessionConfig::default());

    round_trip(&TaskSpec::new("t-1", "Turn on airplane mode"));
    round_trip(&Progress {
        step: 3,
        phase: Phase::Acting,
        last_action: Some(Action::Tap { element: 2 }),
        app: Some("com.apple.Preferences".into()),
        model_calls: 5,
        parse_retries: 1,
    });
    round_trip(&Progress::default());
    // Progress JSON from before the counters existed still loads.
    let old: Progress =
        serde_json::from_str(r#"{"step":2,"phase":"acting","last_action":null,"app":null}"#)
            .unwrap();
    assert_eq!((old.model_calls, old.parse_retries), (0, 0));
}

#[test]
fn policy_default_is_conservative() {
    let p = Policy::default();
    assert_eq!(p.max_steps, 40);
    assert_eq!(p.max_duration_ms, 600_000);
    assert_eq!(p.max_consecutive_failures, 4);
    assert_eq!(p.max_parse_retries, 2);
    for c in RiskCategory::ALL {
        assert_eq!(p.requires_confirmation(c), c != RiskCategory::None, "{c:?}");
    }
    assert!(p.denied_apps.iter().any(|a| a == "com.apple.Passwords"));
    assert!(p.denied_apps.iter().any(|a| a == "com.apple.Passbook"));
    // A partial policy from JSON keeps the conservative defaults for missing fields.
    let partial: Policy = serde_json::from_str(r#"{"max_steps": 10}"#).unwrap();
    assert_eq!(partial.max_steps, 10);
    assert_eq!(partial.confirm, p.confirm);
    assert_eq!(partial.denied_apps, p.denied_apps);
    assert!(partial.first_use_approval);
    assert_eq!(partial.mode, PolicyMode::Guarded);
    round_trip(&p);
    assert!(p.first_use_approval);
    assert_eq!(p.mode, PolicyMode::Guarded);
    assert!(p.deny_categories.is_empty() && p.app_rules.is_empty());
    assert!(!p.denies_category(RiskCategory::Payment));
}

#[test]
fn policy_with_the_new_fields_round_trips() {
    let p = Policy {
        deny_categories: vec![RiskCategory::Payment, RiskCategory::None],
        mode: PolicyMode::StepByStep,
        app_rules: vec![
            AppRule::new("com.example", TrustLevel::Denied),
            AppRule::new("com.apple.Preferences", TrustLevel::Trusted),
        ],
        first_use_approval: false,
        confirm: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    round_trip(&p);
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["mode"], "step_by_step");
    assert_eq!(
        v["app_rules"][1],
        json!({"bundle_id_prefix": "com.apple.Preferences", "level": "trusted"})
    );
    assert!(p.denies_category(RiskCategory::Payment));
    assert!(
        !p.denies_category(RiskCategory::None),
        "None is never denied"
    );
    // A current-format confirm list is taken as written (no migration).
    assert_eq!(
        serde_json::from_value::<Policy>(v).unwrap().confirm,
        vec![RiskCategory::Delete]
    );
}

/// A policy exactly as the shell stored it before the structural controls existed: it must
/// load, keep every old setting, and behave as before (unreadable taps used to classify as
/// `payment`, calls as `send_message`).
#[test]
fn policy_json_from_before_the_structural_controls_is_migrated() {
    let old = r#"{
        "denied_apps": ["com.apple.Passwords", "com.examplebank"],
        "confirm": ["send_message", "payment", "delete"],
        "max_steps": 25,
        "max_duration_ms": 120000,
        "max_consecutive_failures": 3,
        "max_parse_retries": 1
    }"#;
    let p: Policy = serde_json::from_str(old).unwrap();
    assert_eq!(
        p.denied_apps,
        vec!["com.apple.Passwords", "com.examplebank"]
    );
    assert_eq!(
        p.confirm,
        vec![
            RiskCategory::SendMessage,
            RiskCategory::Payment,
            RiskCategory::Delete,
            RiskCategory::Unverified,
            RiskCategory::Call,
            RiskCategory::FirstUse,
        ]
    );
    assert_eq!((p.max_steps, p.max_duration_ms), (25, 120_000));
    assert_eq!((p.max_consecutive_failures, p.max_parse_retries), (3, 1));
    assert_eq!(p.mode, PolicyMode::Guarded);
    assert!(p.first_use_approval);
    assert!(p.app_rules.is_empty() && p.deny_categories.is_empty());
    // A list without payment/send_message gains only first_use.
    let p: Policy = serde_json::from_str(r#"{"confirm": ["delete"]}"#).unwrap();
    assert_eq!(
        p.confirm,
        vec![RiskCategory::Delete, RiskCategory::FirstUse]
    );
    // Saving and loading again is stable (the saved form is current).
    let again: Policy = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(again, p);
    // A whole SessionConfig from before loads too.
    let cfg: SessionConfig = serde_json::from_str(&format!(
        r#"{{"policy": {old}, "tools": [], "known_apps": []}}"#
    ))
    .unwrap();
    assert_eq!(cfg.agent_bundle_id, None);
    assert!(cfg.policy.confirm.contains(&RiskCategory::Unverified));
}
