#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Tenth review fixes in the session: the per-app "Jarvis typed here" state behind the
//! presumption that a tap after typing submits (H1).
mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::compact;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{RiskCategory, Role, RunnerCommand, Screen, UiNode};

fn scr(app: &str, children: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("App")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    screen(app, "App", root, false)
}

/// The reviewer's feedback form: a cell with an unlabelled text view and a self-drawn
/// placeholder text (hidden once filled), a cell with "Continue".
fn feedback(value: Option<&str>) -> Screen {
    let mut tv = UiNode::new(Role::TextView, rect(16.0, 140.0, 361.0, 160.0)).hittable(true);
    let mut row = Vec::new();
    match value {
        Some(v) => tv = tv.with_value(v),
        None => row.push(text(
            "Tell the seller what happened",
            rect(24.0, 148.0, 300.0, 20.0),
        )),
    }
    row.insert(0, tv);
    scr(
        "com.example.shop",
        vec![
            UiNode::new(Role::CollectionView, IPHONE_SIZE.to_rect()).with_children(vec![
                UiNode::new(Role::Cell, rect(0.0, 130.0, 393.0, 180.0)).with_children(row),
                UiNode::new(Role::Cell, rect(0.0, 330.0, 393.0, 50.0)).with_children(vec![node(
                    Role::Button,
                    "Continue",
                    rect(16.0, 333.0, 361.0, 44.0),
                )]),
            ]),
        ],
    )
}

/// End to end: Jarvis types into the text view, the placeholder hides (the field's
/// fingerprint changes), and "Continue" asks as a send showing the text.
#[test]
fn continue_after_typing_into_a_field_whose_placeholder_hid_asks() {
    let before = feedback(None);
    let field = compact(&before)
        .elements
        .iter()
        .find(|e| e.role == Role::TextView)
        .unwrap()
        .index;
    let after = feedback(Some("Item arrived broken"));
    let cont = index_of(&after, "Continue");
    let r = run(
        config(),
        "Report the broken item",
        MemoryStore::new(),
        Phone::new(before, move |c, s| {
            if matches!(c, RunnerCommand::TypeText { .. }) {
                common::Transition::Go(after.clone())
            } else {
                common::Transition::Go(s.clone())
            }
        }),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{field},"text":"Item arrived broken"}}"#),
            &format!(r#"{{"action":"tap","element":{cont}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    let c = r.confirms.last().expect("Continue asked");
    assert_eq!(c.category, RiskCategory::SendMessage, "{c:?}");
    assert!(
        c.summary.contains("Sends:") && c.summary.contains("Item arrived broken"),
        "{}",
        c.summary
    );
}

fn compose(draft: &str) -> Screen {
    scr(
        "com.example.chat",
        vec![
            UiNode::new(Role::TextView, rect(16.0, 700.0, 280.0, 44.0))
                .with_label("Message")
                .with_value(draft)
                .hittable(true),
            node(Role::Button, "Send", rect(320.0, 700.0, 60.0, 44.0)),
        ],
    )
}

/// The next screen: a field the owner filled earlier and a neutral "Done".
fn pinned(app: &str) -> Screen {
    scr(
        app,
        vec![
            UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 44.0))
                .with_label("Note")
                .with_value("Pinned by the owner")
                .hittable(true),
            node(Role::Button, "Done", rect(16.0, 300.0, 361.0, 50.0)),
        ],
    )
}

/// Eleventh review (F2): the app's "typed here" state lasts for the task. An approved send
/// no longer clears it (it cleared it for every form of the app, so text Jarvis typed in
/// another form was submitted unasked): a later "Done" beside a filled field asks, showing
/// the field it would send (was: the approved send cleared the state and "Done" ran
/// unasked; deliberate change, the cost is this question). Without the approved submit (a
/// scroll leads to the same screen) the state stays and "Done" asks, as before.
#[test]
fn an_approved_send_keeps_the_typed_here_state() {
    let message = index_of(&compose(""), "Message");
    let send = index_of(&compose("On my way"), "Send");
    let done = index_of(&pinned("com.example.chat"), "Done");
    // 1. Type, approved Send (carried out: the next screen), Done: asked too (declined).
    let mut typed = false;
    let r = run(
        config(),
        "Tell Alice I'm on my way",
        MemoryStore::new(),
        Phone::new(compose(""), move |c, s| {
            if matches!(c, RunnerCommand::TypeText { .. }) {
                typed = true;
                return common::Transition::Go(compose("On my way"));
            }
            if typed && taps(c, s, "Send") {
                return common::Transition::Go(pinned("com.example.chat"));
            }
            common::Transition::Go(s.clone())
        }),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{message},"text":"On my way"}}"#),
            &format!(r#"{{"action":"tap","element":{send}}}"#),
            &format!(r#"{{"action":"tap","element":{done}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ])
        .decisions(&[true]),
    );
    assert_eq!(r.confirms.len(), 2, "{:#?}", r.confirms);
    assert!(r.confirms[0]
        .summary
        .contains("Sends: Message: \"On my way\""));
    assert_eq!(r.confirms[1].category, RiskCategory::SendMessage);
    assert!(
        r.confirms[1]
            .summary
            .contains("Sends: Note: \"Pinned by the owner\""),
        "{}",
        r.confirms[1].summary
    );
    assert!(
        !r.acts
            .iter()
            .any(|a| taps(a, &pinned("com.example.chat"), "Done")),
        "Done was tapped though the owner declined it"
    );
    // 2. Type, then scroll to the same next screen (no approved submit): Done asks.
    let r = run(
        config(),
        "Tell Alice I'm on my way",
        MemoryStore::new(),
        Phone::new(compose(""), move |c, s| match c {
            RunnerCommand::TypeText { .. } => common::Transition::Go(compose("On my way")),
            RunnerCommand::Swipe { .. } => common::Transition::Go(pinned("com.example.chat")),
            _ => common::Transition::Go(s.clone()),
        }),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{message},"text":"On my way"}}"#),
            r#"{"action":"scroll","direction":"down"}"#,
            &format!(r#"{{"action":"tap","element":{done}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    let c = r
        .confirms
        .last()
        .expect("Done asked while the state is set");
    assert_eq!(c.category, RiskCategory::SendMessage, "{c:?}");
    assert!(
        c.summary.contains("Sends: Note: \"Pinned by the owner\""),
        "{}",
        c.summary
    );
}

/// The state is per app: typing in one app does not make a tap in another app a presumed
/// submit.
#[test]
fn typing_in_one_app_presumes_nothing_in_another() {
    let message = index_of(&compose(""), "Message");
    let done = index_of(&pinned("com.example.shop"), "Done");
    let r = run(
        config(),
        "Note it and finish in the shop",
        MemoryStore::new(),
        Phone::new(compose(""), move |c, s| match c {
            RunnerCommand::TypeText { .. } => common::Transition::Go(compose("On my way")),
            RunnerCommand::LaunchApp { .. } => common::Transition::Go(pinned("com.example.shop")),
            _ => common::Transition::Go(s.clone()),
        }),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{message},"text":"On my way"}}"#),
            r#"{"action":"open_app","app":"com.example.shop"}"#,
            &format!(r#"{{"action":"tap","element":{done}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    assert!(r.confirms.is_empty(), "{:#?}", r.confirms);
    assert!(r
        .acts
        .iter()
        .any(|a| taps(a, &pinned("com.example.shop"), "Done")));
}
