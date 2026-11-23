//! Fourteenth review probes (remote): each asserts the SAFE behaviour.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::{Action, CompactScreen, ConfirmRequest, Element, Policy, Rect, Role};
use jarvis_remote::{ClientMessage, HubEffect, LocalEvent, RemotePermissions};
use jarvis_safety::{evaluate_owned, Decision, ScreenSource, TypedText};

fn el(index: u32, role: Role, label: &str, value: Option<&str>) -> Element {
    Element {
        index,
        role,
        label: label.to_owned(),
        value: value.map(str::to_owned),
        placeholder: None,
        frame: Rect::new(16.0, 60.0 * f64::from(index), 361.0, 44.0),
        enabled: true,
        focused: false,
        selected: false,
        secure: false,
        fingerprint: u64::from(index),
    }
}

fn screen(elements: Vec<Element>) -> CompactScreen {
    CompactScreen {
        app_bundle_id: "com.example.forum".to_owned(),
        app_name: Some("Forum".to_owned()),
        size: jarvis_proto::Size::new(393.0, 852.0),
        keyboard_visible: false,
        elements,
        cues: Default::default(),
    }
}

/// What the real gate asks for `action`.
fn ask(s: &CompactScreen, action: &Action, typed: &[TypedText]) -> ConfirmRequest {
    match evaluate_owned(
        &Policy::default(),
        action,
        s,
        None,
        ScreenSource::Typed,
        None,
        typed,
        &["Finish verifying on this page"],
    ) {
        Decision::Confirm(r) => r,
        d => panic!("not asked: {d:?}"),
    }
}

/// A P2P money form whose payee field is not under a strong head and whose amount is a bare
/// number, submitted by a button labelled "Send" (Venmo/Cash App style), after Jarvis typed
/// the memo. The gate calls it a `SendMessage` and, since the label "sends", not phone
/// only: a paired device with the approve permission approves moving $900.
#[test]
fn probe14_money_form_with_send_button_is_remotely_approvable() {
    let form = screen(vec![
        el(1, Role::TextField, "Who are you paying?", Some("Mallory")),
        el(2, Role::TextField, "How much?", Some("900")),
        el(3, Role::TextField, "What's it for?", Some("rent")),
        el(4, Role::Button, "Send", None),
    ]);
    let state = [TypedText {
        field: 3,
        text: "rent".to_owned(),
    }];
    let req = ask(&form, &Action::Tap { element: 4 }, &state);
    eprintln!("{req:?}");

    let mut rig = Rig::new();
    let (mut conn, creds) = rig.pair("Mac");
    rig.set_permissions(
        &mut conn,
        &creds,
        RemotePermissions {
            approve: true,
            see_screen_text: true,
            ..RemotePermissions::NEW_DEVICE
        },
    );
    rig.request(&mut conn, ClientMessage::Subscribe);
    rig.request(
        &mut conn,
        ClientMessage::StartTask {
            text: "pay rent".into(),
        },
    );
    let fx = rig.local(LocalEvent::ConfirmRequested { request: req });
    let msgs = deliver(&mut conn, &fx);
    eprintln!("{msgs:?}");
    let request_id = rig.hub.pending_request_id().unwrap();
    let (fx, msgs) = rig.request(
        &mut conn,
        ClientMessage::Decision {
            request_id,
            approved: true,
        },
    );
    eprintln!("{msgs:?} {:?}", actions(&fx));
    assert!(
        !actions(&fx)
            .iter()
            .any(|a| matches!(a, HubEffect::Decide { approved: true, .. })),
        "a remote device approved sending $900 to a payee the gate could not verify"
    );
}
