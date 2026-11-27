#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 39 through the session: a control whose drawing the tables cannot read (an
//! intruder inside a Latin word: `Deǀete Account`, `Ca∣∣`, also as a text) is asked on the
//! phone only at every trust level, a paired device's "yes" does not press it, it is
//! refused while the owner denies any category, and an approval never covers another
//! press of it — for a tap, a long press, a `tap_point` and the Return of a field whose
//! label cannot be read. The reviewer's own tests are in `jarvis-safety`.

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text};
use jarvis_proto::{
    Answerer, AppRule, Policy, RiskCategory, Role, RunnerCommand, Screen, TrustLevel, UiNode,
};

const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const LOOK: &str = r#"{"action":"look"}"#;
const DELETE: &str = "De\u{1C0}ete Account";
const CALL: &str = "Ca\u{2223}\u{2223} support";

/// A shop screen with a disguised delete button, a disguised call text and a plain one.
fn shop() -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, rect(0.0, 0.0, 393.0, 852.0))
            .with_label("Shop")
            .with_children(vec![UiNode::new(
                Role::Window,
                rect(0.0, 0.0, 393.0, 852.0),
            )
            .with_children(vec![
                text("Account", rect(16.0, 120.0, 361.0, 30.0)),
                text(CALL, rect(16.0, 400.0, 361.0, 30.0)),
                node(Role::Button, DELETE, rect(16.0, 600.0, 361.0, 50.0)),
                node(Role::Button, "Next", rect(16.0, 700.0, 361.0, 44.0)),
            ])]),
        false,
    )
}

/// A screen with a focused field whose label cannot be read.
fn form() -> Screen {
    let field = UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 40.0))
        .with_label("Pass\u{2223}word")
        .focused(true)
        .hittable(true);
    screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, rect(0.0, 0.0, 393.0, 852.0))
            .with_label("Shop")
            .with_children(vec![UiNode::new(
                Role::Window,
                rect(0.0, 0.0, 393.0, 852.0),
            )
            .with_children(vec![field])]),
        true,
    )
}

fn policy(level: Option<TrustLevel>, deny: &[RiskCategory]) -> jarvis_proto::SessionConfig {
    let mut c = vision_config();
    c.policy = Policy {
        first_use_approval: false,
        app_rules: level
            .map(|l| vec![AppRule::new("com.example.shop", l)])
            .unwrap_or_default(),
        deny_categories: deny.to_vec(),
        ..Policy::default()
    };
    c
}

const TIERS: [Option<TrustLevel>; 4] = [
    None,
    Some(TrustLevel::Trusted),
    Some(TrustLevel::Guarded),
    Some(TrustLevel::StepByStep),
];

/// The ways to press the element labelled `label` on `s`: tap, long press, `tap_point`.
fn presses(s: &Screen, label: &str) -> Vec<(&'static str, String)> {
    let c = jarvis_observe::compact(s);
    let e = c.elements.iter().find(|e| e.label == label).unwrap();
    let (x, y) = (
        (e.frame.x + e.frame.width / 2.0) / c.size.width,
        (e.frame.y + e.frame.height / 2.0) / c.size.height,
    );
    vec![
        (
            "tap",
            format!(r#"{{"action":"tap","element":{}}}"#, e.index),
        ),
        (
            "long press",
            format!(
                r#"{{"action":"long_press","element":{},"duration_ms":800}}"#,
                e.index
            ),
        ),
        (
            "tap_point",
            format!(r#"{{"action":"tap_point","x":{x:.4},"y":{y:.4}}}"#),
        ),
    ]
}

fn pressed(r: &Run, s: &Screen, label: &str) -> bool {
    r.acts.iter().any(|a| {
        taps(a, s, label)
            && matches!(
                a,
                RunnerCommand::Tap { .. } | RunnerCommand::LongPress { .. }
            )
    })
}

/// Every tier: asked once, phone only; approved on the phone it is pressed.
#[test]
fn asked_phone_only_at_every_tier_and_pressed_when_the_phone_approves() {
    let s = shop();
    for label in [DELETE, CALL] {
        for (how, out) in presses(&s, label) {
            for level in TIERS {
                let r = run(
                    policy(level, &[]),
                    "Tidy up my account",
                    MemoryStore::new(),
                    Phone::still(s.clone()),
                    Script::model(&[LOOK, &out, DONE])
                        .decisions(&[true])
                        .answered_by(Answerer::Phone),
                );
                assert_eq!(r.confirms.len(), 1, "{label:?} {how} {level:?}");
                assert!(r.confirms[0].phone_only, "{label:?} {how} {level:?}");
                assert!(pressed(&r, &s, label), "{label:?} {how} {level:?}");
            }
        }
    }
}

/// A paired device's "yes" never presses it (the request is phone only).
#[test]
fn a_remote_yes_does_not_press_it() {
    let s = shop();
    for label in [DELETE, CALL] {
        for (how, out) in presses(&s, label) {
            let r = run(
                policy(Some(TrustLevel::Trusted), &[]),
                "Tidy up my account",
                MemoryStore::new(),
                Phone::still(s.clone()),
                Script::model(&[LOOK, &out, DONE])
                    .decisions(&[true])
                    .answered_by(Answerer::Remote),
            );
            assert!(!pressed(&r, &s, label), "{label:?} {how}");
        }
    }
}

/// While the owner denies any category (here `Call` for the delete button and `Delete` for
/// the call text: neither is what the label reads as), it is refused without a question,
/// in a Trusted app too.
#[test]
fn refused_under_any_deny_at_every_tier() {
    let s = shop();
    for (label, deny) in [(DELETE, RiskCategory::Call), (CALL, RiskCategory::Delete)] {
        for (how, out) in presses(&s, label) {
            for level in TIERS {
                let r = run(
                    policy(level, &[deny]),
                    "Tidy up my account",
                    MemoryStore::new(),
                    Phone::still(s.clone()),
                    Script::model(&[LOOK, &out, DONE]).decisions(&[true, true]),
                );
                assert!(r.confirms.is_empty(), "{label:?} {how} {level:?}");
                assert!(!pressed(&r, &s, label), "{label:?} {how} {level:?}");
            }
        }
    }
}

/// An approval is never kept: pressing it a second time asks again; the plain button
/// beside it is pressed unasked.
#[test]
fn an_approval_is_not_kept_for_the_next_press() {
    let s = shop();
    let next = presses(&s, "Next");
    for (how, out) in presses(&s, DELETE) {
        let r = run(
            policy(Some(TrustLevel::Trusted), &[]),
            "Tidy up my account",
            MemoryStore::new(),
            Phone::still(s.clone()),
            Script::model(&[LOOK, &out, LOOK, &out, &next[0].1, DONE])
                .decisions(&[true, true])
                .answered_by(Answerer::Phone),
        );
        assert_eq!(r.confirms.len(), 2, "{how}");
        assert!(r.confirms.iter().all(|c| c.phone_only), "{how}");
        assert!(pressed(&r, &s, "Next"), "{how}");
    }
}

/// Typing into a field whose label cannot be read, with Return: asked phone only in a
/// Trusted app, the Return asked again (never covered by the typing's approval); refused
/// under any deny.
#[test]
fn typing_with_return_into_a_field_that_cannot_be_read() {
    let s = form();
    let idx = index_of(&s, "Pass\u{2223}word");
    let out = format!(r#"{{"action":"type","element":{idx},"text":"hello","submit":true}}"#);
    let r = run(
        policy(Some(TrustLevel::Trusted), &[]),
        "Fill in the form",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[&out, DONE])
            .decisions(&[true, true])
            .answered_by(Answerer::Phone),
    );
    let returns = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::PressKey { .. }))
        .count();
    let typed = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::TypeText { .. }))
        .count();
    // Asked for the typing, typed; asked again for the Return, pressed.
    assert_eq!(
        (r.confirms.len(), typed, returns),
        (2, 1, 1),
        "{:?} {:?}",
        r.confirms,
        r.acts
    );
    assert!(r.confirms.iter().all(|c| c.phone_only));
    let denied = run(
        policy(Some(TrustLevel::Trusted), &[RiskCategory::Call]),
        "Fill in the form",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[&out, DONE]).decisions(&[true, true]),
    );
    assert!(
        denied.confirms.is_empty(),
        "{:?} {:?}",
        denied.confirms,
        denied.acts
    );
    assert!(!denied
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::TypeText { .. })));
}
