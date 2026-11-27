//! A confirmation's structured question (rows, stage, kind, target) never changes what a
//! remote controller receives: the same `ConfirmPending` as for the text alone, with no
//! layout (frames, points, the screen digest) and no picture.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto as p;
use jarvis_remote::*;

fn viewer() -> (Rig, Link) {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    let perms = RemotePermissions {
        see_screen_text: true,
        approve: true,
        ..RemotePermissions::NEW_DEVICE
    };
    rig.set_permissions(&mut link, &creds, perms);
    rig.request(&mut link, ClientMessage::Subscribe);
    (rig, link)
}

fn row(kind: p::QuestionRowKind, text: &str) -> p::QuestionRow {
    p::QuestionRow {
        kind,
        text: text.to_owned(),
    }
}

#[test]
fn a_structured_question_reaches_the_remote_exactly_as_its_text_did() {
    let summary =
        "Tap 'Pay $5.00' in Shop. Amounts on screen: Total $5.00. Screen says: Pay to: Mallory";
    let plain = confirm_event(p::RiskCategory::Payment, summary);
    let LocalEvent::ConfirmRequested { request } = plain.clone() else {
        panic!("not a confirmation")
    };
    let mut rich = request;
    rich.rows = vec![
        row(p::QuestionRowKind::Action, "Tap 'Pay $5.00' in Shop"),
        row(
            p::QuestionRowKind::Amount,
            ". Amounts on screen: Total $5.00",
        ),
        row(
            p::QuestionRowKind::ScreenSays,
            ". Screen says: Pay to: Mallory",
        ),
    ];
    assert_eq!(p::render_rows(&rich.rows), summary);
    rich.stage = p::ConfirmStage::Whole;
    rich.verb = p::ConfirmVerb::Tap;
    rich.target = Some(Box::new(p::ConfirmTarget {
        element: Some(7),
        node: Some(31),
        frame: Some(p::Rect::new(16.0, 700.0, 361.0, 50.0)),
        point: Some(p::Point::new(196.5, 725.0)),
        digest: "ab".repeat(32),
    }));
    let (mut rig_a, mut link_a) = viewer();
    let (mut rig_b, mut link_b) = viewer();
    let before = rig_a.local_to(&mut link_a, plain);
    let after = rig_b.local_to(&mut link_b, LocalEvent::ConfirmRequested { request: rich });
    assert_eq!(format!("{before:?}"), format!("{after:?}"));
    assert!(before.iter().any(
        |m| matches!(m, ServerMessage::ConfirmPending { summary: s, .. } if s.contains("Mallory"))
    ));
    let wire = format!("{after:?}");
    for leak in [
        "361",
        "725",
        "abab",
        "frame",
        "digest",
        "ScreenSays",
        "rows",
    ] {
        assert!(!wire.contains(leak), "{leak} in {wire}");
    }
}
