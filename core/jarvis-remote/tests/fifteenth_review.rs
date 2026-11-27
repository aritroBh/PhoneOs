//! Fifteenth review probes (remote): each asserts the SAFE behaviour.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::{Action, CompactScreen, Element, Policy, Rect, Role};
use jarvis_remote::{ClientMessage, HubEffect, LocalEvent, RemotePermissions};
use jarvis_safety::{evaluate_owned, Decision, ScreenSource};

fn el(index: u32, role: Role, label: &str) -> Element {
    Element {
        index,
        role,
        label: label.to_owned(),
        value: None,
        placeholder: None,
        frame: Rect::new(16.0, 60.0 * f64::from(index), 361.0, 44.0),
        enabled: true,
        focused: false,
        selected: false,
        secure: false,
        fingerprint: u64::from(index),
    }
}

/// A wallet's review screen: amounts as static texts, final button "Send". The gate reads
/// it as a `SendMessage` from the button's own words (Rule C: READ, not phone only), so a
/// paired device with the approve permission approves moving 0.5 ETH ($1,712).
#[test]
fn probe15_remote_device_approves_a_wallet_send() {
    let s = CompactScreen {
        app_bundle_id: "com.example.wallet".to_owned(),
        app_name: Some("Wallet".to_owned()),
        size: jarvis_proto::Size::new(393.0, 852.0),
        keyboard_visible: false,
        elements: vec![
            el(1, Role::StaticText, "Review"),
            el(2, Role::StaticText, "To"),
            el(
                3,
                Role::StaticText,
                "0x71C7656EC7ab88b098defB751B7401B5f6d8976F",
            ),
            el(4, Role::StaticText, "Amount"),
            el(5, Role::StaticText, "0.5 ETH"),
            el(6, Role::StaticText, "≈ $1,712.40"),
            el(7, Role::StaticText, "Network fee"),
            el(8, Role::StaticText, "$1.20"),
            el(9, Role::Button, "Send"),
        ],
        cues: Default::default(),
    };
    let req = match evaluate_owned(
        &Policy::default(),
        &Action::Tap { element: 9 },
        &s,
        None,
        ScreenSource::Typed,
        None,
        &[],
        &["Check my wallet balance"],
    ) {
        Decision::Confirm(r) => r,
        d => panic!("not asked: {d:?}"),
    };
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
            text: "check my wallet balance".into(),
        },
    );
    let fx = rig.local(LocalEvent::ConfirmRequested { request: req });
    let _ = deliver(&mut conn, &fx);
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
        "a remote device approved sending 0.5 ETH ($1,712) from a wallet"
    );
}
