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

#[test]
fn record_four_steps_and_replay_on_same_screens_without_divergence() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    assert_eq!(flow.steps.len(), 4);
    assert_eq!(flow.app_bundle_id, MSG);
    assert_eq!(flow.successes, 1);
    assert_eq!(flow.id, flow_id(&flow.intent_key, MSG));
    let actions = replay_all(flow, Bindings::new());
    assert_eq!(
        actions,
        vec![
            Action::OpenApp {
                app: "Messages".into()
            },
            Action::Tap { element: 2 },
            Action::Type {
                element: Some(2),
                text: "zzz".into(),
                submit: false
            },
            Action::Tap { element: 3 },
        ]
    );
}

#[test]
fn replay_uses_current_indices_when_elements_shifted() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance(); // skip OpenApp
                  // A banner pushes everything down by one: Mom is now index 3.
    let mut shifted = inbox();
    for e in &mut shifted.elements {
        e.index += 1;
    }
    shifted
        .elements
        .insert(0, el(1, Role::StaticText, "Banner", 999));
    assert_eq!(rp.next(&shifted), Next::Action(Action::Tap { element: 3 }));
}

#[test]
fn falls_back_to_unique_role_and_label_when_fingerprint_changed() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    let mut s = inbox();
    s.elements[1].fingerprint = 5555; // app update changed the hash
    assert_eq!(rp.next(&s), Next::Action(Action::Tap { element: 2 }));
}

#[test]
fn diverges_on_missing_control() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    let mut s = inbox();
    s.elements.remove(1); // Mom is gone
    match rp.next(&s) {
        Next::Diverged {
            reason: Divergence::ControlMissing { label },
        } => assert_eq!(label, "Mom"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn diverges_on_wrong_app() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    let mut s = inbox();
    s.app_bundle_id = "com.whatsapp".into();
    assert!(matches!(
        rp.next(&s),
        Next::Diverged {
            reason: Divergence::AppMismatch { .. }
        }
    ));
}

#[test]
fn open_app_step_runs_from_any_app() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let rp = Replayer::new(flow, Bindings::new());
    let mut elsewhere = inbox();
    elsewhere.app_bundle_id = "com.apple.notes".into();
    assert!(matches!(
        rp.next(&elsewhere),
        Next::Action(Action::OpenApp { .. })
    ));
}

#[test]
fn ambiguous_fallback_diverges_instead_of_guessing() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    let mut s = inbox();
    s.elements[1].fingerprint = 5555;
    s.elements.push(el(5, Role::Cell, "Mom", 5556)); // two "Mom" cells, no fingerprint hit
    assert!(matches!(
        rp.next(&s),
        Next::Diverged {
            reason: Divergence::Ambiguous { matches: 2, .. }
        }
    ));
}

#[test]
fn duplicate_fingerprints_are_ambiguous() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    let mut s = inbox();
    s.elements.push(el(5, Role::Cell, "Mom (work)", 101));
    assert!(matches!(
        rp.next(&s),
        Next::Diverged {
            reason: Divergence::Ambiguous { .. }
        }
    ));
}

#[test]
fn fingerprint_with_different_role_is_not_trusted() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    let mut s = inbox();
    s.elements[1].role = Role::Image;
    s.elements[1].label = "Photo".into();
    assert!(matches!(rp.next(&s), Next::Diverged { .. }));
}

#[test]
fn disabled_control_diverges() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    let mut s = inbox();
    s.elements[1].enabled = false;
    assert!(matches!(
        rp.next(&s),
        Next::Diverged {
            reason: Divergence::ControlDisabled { .. }
        }
    ));
}

#[test]
fn next_is_pure_until_advance() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let rp = Replayer::new(flow, Bindings::new());
    let h = home();
    assert_eq!(rp.next(&h), rp.next(&h));
    assert_eq!(rp.position(), 0);
}

#[test]
fn advance_past_end_stays_complete() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    for _ in 0..10 {
        rp.advance();
    }
    assert_eq!(rp.position(), 4);
    assert_eq!(rp.next(&home()), Next::Complete);
}

#[test]
fn replay_refuses_to_type_into_secure_field_even_if_recorded_plain() {
    let flow = record_text_flow("text mom running late", "zzz").unwrap();
    let mut rp = Replayer::new(flow, Bindings::new());
    rp.advance();
    rp.advance();
    let mut t = thread();
    t.elements[1].secure = true;
    assert!(matches!(
        rp.next(&t),
        Next::Diverged {
            reason: Divergence::SecureField
        }
    ));
}

#[test]
fn swapped_same_fingerprint_buttons_never_tap_the_wrong_one() {
    let a = screen(
        "app",
        vec![
            el(1, Role::Button, "Cancel", 77),
            el(2, Role::Button, "Delete account", 77),
        ],
    );
    let mut rec = Recorder::new("cancel it");
    rec.record(RecordedStep {
        action: Action::Tap { element: 1 },
        element: a.get(1),
        screen: &a,
    });
    let flow = rec.finish(true, 1).unwrap();
    let rp = Replayer::new(flow.clone(), Bindings::new());
    let swapped = screen(
        "app",
        vec![
            el(1, Role::Button, "Delete account", 77),
            el(2, Role::Button, "Cancel", 77),
        ],
    );
    assert!(matches!(
        rp.next(&swapped),
        Next::Diverged {
            reason: Divergence::Ambiguous { .. }
        }
    ));
    // A single element with the fingerprint but another label also diverges.
    let relabelled = screen("app", vec![el(1, Role::Button, "Delete account", 77)]);
    assert!(matches!(rp.next(&relabelled), Next::Diverged { .. }));
    // Label case/whitespace differences are fine.
    let ok = screen("app", vec![el(4, Role::Button, " cancel ", 77)]);
    assert_eq!(rp.next(&ok), Next::Action(Action::Tap { element: 4 }));
}

#[test]
fn fallback_refuses_generic_or_tiny_labels() {
    for label in ["OK", "Delete", "Send", "x"] {
        let a = screen("app", vec![el(1, Role::Button, label, 1)]);
        let mut rec = Recorder::new("do thing");
        rec.record(RecordedStep {
            action: Action::Tap { element: 1 },
            element: a.get(1),
            screen: &a,
        });
        let rp = Replayer::new(rec.finish(true, 1).unwrap(), Bindings::new());
        let changed = screen("app", vec![el(1, Role::Button, label, 2)]);
        assert!(
            matches!(rp.next(&changed), Next::Diverged { .. }),
            "{label}"
        );
        assert!(
            matches!(rp.next(&a), Next::Action(_)),
            "fingerprint still works: {label}"
        );
    }
}
