#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;
use common::*;
use jarvis_memory::*;
use jarvis_proto::{Action, Role};
use serde_json::json;

fn rec_step(rec: &mut Recorder, action: Action, idx: Option<u32>, s: &jarvis_proto::CompactScreen) {
    rec.record(RecordedStep {
        action,
        element: idx.and_then(|i| s.get(i)),
        screen: s,
    });
}

#[test]
fn secure_field_typing_becomes_hand_back_and_text_is_never_stored() {
    let mut login = screen(
        "com.example.acme",
        vec![
            el(1, Role::TextField, "Email", 11),
            el(2, Role::SecureTextField, "Password", 12),
            el(3, Role::Button, "Sign in", 13),
        ],
    );
    login.elements[1].secure = true;
    let mut rec = Recorder::new("log in to acme");
    rec_step(
        &mut rec,
        Action::Type {
            element: Some(1),
            text: "alex@example.com".into(),
            submit: false,
        },
        Some(1),
        &login,
    );
    rec_step(
        &mut rec,
        Action::Type {
            element: Some(2),
            text: "hunter2hunter2".into(),
            submit: false,
        },
        Some(2),
        &login,
    );
    rec_step(&mut rec, Action::Tap { element: 3 }, Some(3), &login); // ignored: flow ended at hand back
    let flow = rec.finish(true, 1).unwrap();
    assert_eq!(flow.steps.len(), 2);
    assert!(matches!(flow.steps[1].action, FlowAction::HandBack { .. }));
    assert!(!serde_json::to_string(&flow).unwrap().contains("hunter2"));

    let mut rp = Replayer::new(flow, Bindings::new());
    assert!(matches!(rp.next(&login), Next::Action(Action::Type { .. })));
    rp.advance();
    assert!(matches!(
        rp.next(&login),
        Next::Diverged {
            reason: Divergence::HandBack { .. }
        }
    ));
}

#[test]
fn typing_into_focused_secure_field_without_element_is_also_hand_back() {
    let mut login = screen(
        "com.example.acme",
        vec![el(1, Role::SecureTextField, "Password", 12)],
    );
    login.elements[0].secure = true;
    login.elements[0].focused = true;
    let mut rec = Recorder::new("log in");
    rec_step(&mut rec, Action::Tap { element: 1 }, Some(1), &login);
    rec_step(
        &mut rec,
        Action::Type {
            element: None,
            text: "pw".into(),
            submit: true,
        },
        None,
        &login,
    );
    let flow = rec.finish(true, 1).unwrap();
    assert!(matches!(flow.steps[1].action, FlowAction::HandBack { .. }));
}

#[test]
fn flow_starting_with_hand_back_is_dropped() {
    let mut s = screen("a", vec![el(1, Role::SecureTextField, "Password", 1)]);
    s.elements[0].secure = true;
    let mut rec = Recorder::new("enter password");
    rec_step(
        &mut rec,
        Action::Type {
            element: Some(1),
            text: "x".into(),
            submit: false,
        },
        Some(1),
        &s,
    );
    assert!(rec.finish(true, 1).is_none());
}

#[test]
fn sensitive_profile_values_are_handed_back() {
    let t = thread();
    let mut rec = Recorder::new("send my phone number to mom");
    rec.set_sensitive_values(vec!["555-0100".into()]);
    rec_step(&mut rec, Action::Tap { element: 3 }, Some(3), &t);
    rec_step(
        &mut rec,
        Action::Type {
            element: Some(2),
            text: " 555-0100 ".into(),
            submit: false,
        },
        Some(2),
        &t,
    );
    let flow = rec.finish(true, 1).unwrap();
    assert!(matches!(flow.steps[1].action, FlowAction::HandBack { .. }));
    assert!(!serde_json::to_string(&flow).unwrap().contains("555-0100"));
}

#[test]
fn non_recordable_sessions() {
    let i = inbox();
    let cases: Vec<Action> = vec![
        Action::AskUser {
            question: "which Mom?".into(),
        },
        Action::CallTool {
            name: "send_sms".into(),
            args: json!({}),
        },
        Action::TapPoint { x: 0.5, y: 0.5 },
    ];
    for bad in cases {
        let mut rec = Recorder::new("text mom hi");
        rec_step(&mut rec, Action::Tap { element: 2 }, Some(2), &i);
        rec_step(&mut rec, bad.clone(), None, &i);
        rec_step(&mut rec, Action::Tap { element: 3 }, Some(3), &i);
        assert!(rec.non_recordable_reason().is_some(), "{bad:?}");
        assert!(rec.finish(true, 1).is_none(), "{bad:?}");
    }
}

#[test]
fn unresolvable_or_mismatched_element_is_non_recordable() {
    let i = inbox();
    let mut rec = Recorder::new("x y");
    rec_step(&mut rec, Action::Tap { element: 9 }, None, &i);
    assert!(rec.finish(true, 1).is_none());
    let mut rec = Recorder::new("x y");
    rec.record(RecordedStep {
        action: Action::Tap { element: 2 },
        element: i.get(3),
        screen: &i,
    });
    assert!(rec.finish(true, 1).is_none());
}

#[test]
fn failed_empty_or_wait_only_sessions_yield_nothing() {
    let i = inbox();
    let mut rec = Recorder::new("text mom hi");
    rec_step(&mut rec, Action::Tap { element: 2 }, Some(2), &i);
    assert!(rec.finish(false, 1).is_none());
    assert!(Recorder::new("text mom hi").finish(true, 1).is_none());
    let mut rec = Recorder::new("wait");
    rec_step(&mut rec, Action::Wait { ms: 100 }, None, &i);
    rec_step(
        &mut rec,
        Action::Done {
            summary: "ok".into(),
        },
        None,
        &i,
    );
    assert!(rec.finish(true, 1).is_none());
    // Filler-only task text has no intent.
    let mut rec = Recorder::new("please");
    rec_step(&mut rec, Action::Tap { element: 2 }, Some(2), &i);
    assert!(rec.finish(true, 1).is_none());
}

#[test]
fn look_and_terminal_actions_are_ignored() {
    let i = inbox();
    let mut rec = Recorder::new("open mom");
    rec_step(&mut rec, Action::Look, None, &i);
    rec_step(&mut rec, Action::Tap { element: 2 }, Some(2), &i);
    rec_step(
        &mut rec,
        Action::Done {
            summary: "done".into(),
        },
        None,
        &i,
    );
    assert_eq!(rec.finish(true, 7).unwrap().steps.len(), 1);
}

#[test]
fn type_without_element_is_pinned_to_the_focused_field() {
    let t = thread();
    let mut rec = Recorder::new("say hello");
    rec_step(
        &mut rec,
        Action::Type {
            element: None,
            text: "hello".into(),
            submit: true,
        },
        None,
        &t,
    );
    let flow = rec.finish(true, 1).unwrap();
    assert_eq!(flow.steps[0].fingerprint, Some(201));
    assert_eq!(flow.intent_key, "say {1}");
    let rp = Replayer::new(flow, Bindings::from([(1, "hello".to_owned())]));
    // Even if indices differ, the focused field's current index is used.
    let mut shifted = thread();
    shifted.elements.insert(0, el(9, Role::StaticText, "x", 1));
    for (n, e) in shifted.elements.iter_mut().enumerate() {
        e.index = n as u32 + 1;
    }
    assert_eq!(
        rp.next(&shifted),
        Next::Action(Action::Type {
            element: Some(3),
            text: "hello".into(),
            submit: true
        })
    );
}

#[test]
fn replay_of_type_without_focus_diverges() {
    let mut rec = Recorder::new("say hello");
    let s = screen("a", vec![el(1, Role::StaticText, "Hello", 1)]);
    rec_step(
        &mut rec,
        Action::Type {
            element: None,
            text: "hello".into(),
            submit: false,
        },
        None,
        &s,
    );
    let flow = rec.finish(true, 1).unwrap();
    assert!(flow.steps[0].fingerprint.is_none());
    let rp = Replayer::new(flow, Bindings::new());
    assert!(matches!(
        rp.next(&s),
        Next::Diverged {
            reason: Divergence::NoFocusedField
        }
    ));
}

#[test]
fn unbound_slot_diverges() {
    let flow = record_text_flow("text mom running late", "running late").unwrap();
    assert_eq!(flow.intent_key, "text mom {1}");
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    rp.advance();
    assert!(matches!(
        rp.next(&thread()),
        Next::Diverged {
            reason: Divergence::UnboundSlot { slot: 1 }
        }
    ));
}

#[test]
fn flow_json_roundtrip_and_ids_are_stable() {
    let flow = record_text_flow("text mom running late", "running late").unwrap();
    let back: Flow = serde_json::from_str(&serde_json::to_string(&flow).unwrap()).unwrap();
    assert_eq!(back, flow);
    assert_eq!(flow_id("a", "b"), flow_id("a", "b"));
    assert_ne!(flow_id("a", "b"), flow_id("b", "a"));
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(flow_id("x", "y").len(), 16);
}

#[test]
fn typing_after_tapping_a_secure_field_without_focus_is_never_stored() {
    let mut login = screen(
        "com.example.acme",
        vec![
            el(1, Role::TextField, "Email", 11),
            el(2, Role::SecureTextField, "Password", 12),
        ],
    );
    login.elements[1].secure = true;
    let mut rec = Recorder::new("log in");
    rec_step(&mut rec, Action::Tap { element: 2 }, Some(2), &login);
    rec_step(
        &mut rec,
        Action::Type {
            element: None,
            text: "hunter2".into(),
            submit: true,
        },
        None,
        &login,
    );
    let flow = rec.finish(true, 1).unwrap();
    assert!(matches!(flow.steps[1].action, FlowAction::HandBack { .. }));
    let mut m = MemoryStore::new();
    m.flows.upsert(flow);
    assert!(!m.to_json().unwrap().contains("hunter2"));

    // Even without a preceding tap: secure field on screen and nothing focused.
    let mut rec = Recorder::new("log in");
    rec_step(&mut rec, Action::Tap { element: 1 }, Some(1), &login);
    rec_step(
        &mut rec,
        Action::Type {
            element: None,
            text: "hunter3".into(),
            submit: true,
        },
        None,
        &login,
    );
    let flow = rec.finish(true, 1).unwrap();
    assert!(!serde_json::to_string(&flow).unwrap().contains("hunter3"));
}

#[test]
fn sensitive_values_match_by_normalised_containment() {
    let t = thread();
    let mut rec = Recorder::new("pay");
    rec.set_sensitive_values(vec!["4111111111111111".into()]);
    rec_step(&mut rec, Action::Tap { element: 3 }, Some(3), &t);
    rec_step(
        &mut rec,
        Action::Type {
            element: Some(2),
            text: "4111 1111-1111 1111".into(),
            submit: false,
        },
        Some(2),
        &t,
    );
    let flow = rec.finish(true, 1).unwrap();
    assert!(matches!(flow.steps[1].action, FlowAction::HandBack { .. }));
    assert!(!serde_json::to_string(&flow).unwrap().contains("4111"));
}

#[test]
fn profile_placeholders_are_stored_as_profile_slots() {
    for (typed, key) in [
        ("{{profile.phone}}", "phone"),
        (" {{ Profile.E-mail }} ", "E-mail"),
    ] {
        let t = thread();
        let mut rec = Recorder::new("send my phone number to mom");
        // The value is registered as sensitive; the placeholder never needs to be.
        rec.set_sensitive_values(vec!["555-0100".into()]);
        rec_step(&mut rec, Action::Tap { element: 3 }, Some(3), &t);
        rec_step(
            &mut rec,
            Action::Type {
                element: Some(2),
                text: typed.into(),
                submit: true,
            },
            Some(2),
            &t,
        );
        let flow = rec.finish(true, 1).unwrap();
        assert_eq!(
            flow.steps[1].action,
            FlowAction::Type {
                text: FlowText::Profile { key: key.into() },
                submit: true
            },
            "{typed}"
        );
        let json = serde_json::to_string(&flow).unwrap();
        assert!(!json.contains("555-0100"));
        // Replay hands the placeholder (never a value) to the agent.
        let mut rp = Replayer::new(flow, Bindings::new());
        rp.advance();
        match rp.next(&t) {
            Next::Action(Action::Type { text, submit, .. }) => {
                assert_eq!(text, profile_placeholder(key));
                assert!(submit);
            }
            other => panic!("{other:?}"),
        }
    }
    // Text around a placeholder is not a profile slot; it is kept literally (placeholder
    // included, value excluded).
    assert_eq!(placeholder_key("Call me at {{profile.phone}}"), None);
    assert_eq!(placeholder_key("{{profile.}}"), None);
    assert_eq!(placeholder_key("{{profile.a}}{{profile.b}}"), None);
    assert_eq!(placeholder_key("{{other.phone}}"), None);
    // Typing the raw sensitive value is still handed back.
    let t = thread();
    let mut rec = Recorder::new("send my phone number to mom");
    rec.set_sensitive_values(vec!["555-0100".into()]);
    rec_step(&mut rec, Action::Tap { element: 3 }, Some(3), &t);
    rec_step(
        &mut rec,
        Action::Type {
            element: Some(2),
            text: "my number is 555-0100".into(),
            submit: false,
        },
        Some(2),
        &t,
    );
    let flow = rec.finish(true, 1).unwrap();
    assert!(matches!(flow.steps[1].action, FlowAction::HandBack { .. }));
}
