//! Thirteenth review (M1, M3), through the real gate and the real hub: a presumed submit
//! (a neutral "Continue" after text entered the form) and a link the owner did not give
//! are `phone_only` — a paired device with the approve permission is shown them as not
//! approvable and is refused when it approves, whatever the category says; it may decline.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::{
    Action, CompactScreen, ConfirmRequest, Element, Policy, Rect, RiskCategory, Role,
};
use jarvis_remote::{
    ClientMessage, DenyReason, HubEffect, LocalEvent, RemotePermissions, ServerMessage,
};
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

#[test]
fn phone_only_requests_cannot_be_approved_remotely() {
    let form = screen(vec![
        el(1, Role::TextField, "Your note", Some("Code 482913")),
        el(2, Role::Button, "Continue", None),
        el(3, Role::Button, "Send", None),
    ]);
    let state = [TypedText {
        field: 0,
        text: String::new(),
    }];
    let presumed = ask(&form, &Action::Tap { element: 2 }, &state);
    assert_eq!(presumed.category, RiskCategory::SendMessage);
    assert!(presumed.phone_only);
    let link = ask(
        &form,
        &Action::OpenUrl {
            url: "https://collector.example/c?v=482913".to_owned(),
        },
        &[],
    );
    assert_eq!(link.category, RiskCategory::SendMessage);
    assert!(link.phone_only);
    // A send by its own label stays remotely approvable.
    let own = ask(&form, &Action::Tap { element: 3 }, &state);
    assert!(!own.phone_only);

    let mut rig = Rig::new();
    let (mut link_conn, creds) = rig.pair("Mac");
    rig.set_permissions(
        &mut link_conn,
        &creds,
        RemotePermissions {
            approve: true,
            see_screen_text: true,
            ..RemotePermissions::NEW_DEVICE
        },
    );
    rig.request(&mut link_conn, ClientMessage::Subscribe);
    rig.request(
        &mut link_conn,
        ClientMessage::StartTask {
            text: "verify".into(),
        },
    );
    for request in [presumed, link] {
        let fx = rig.local(LocalEvent::ConfirmRequested {
            request: request.clone(),
        });
        let msgs = deliver(&mut link_conn, &fx);
        let request_id = rig.hub.pending_request_id().unwrap();
        assert!(
            matches!(
                msgs[0],
                ServerMessage::ConfirmPending {
                    can_approve: false,
                    category: RiskCategory::SendMessage,
                    ..
                }
            ),
            "{msgs:?}"
        );
        let (fx, msgs) = rig.request(
            &mut link_conn,
            ClientMessage::Decision {
                request_id,
                approved: true,
            },
        );
        assert!(
            msgs.iter().any(|m| matches!(
                m,
                ServerMessage::Denied {
                    reason: DenyReason::NotRemotelyApprovable
                }
            )),
            "{msgs:?}"
        );
        assert!(actions(&fx).is_empty(), "{:?}", actions(&fx));
        // Declining stays possible.
        let (fx, _) = rig.request(
            &mut link_conn,
            ClientMessage::Decision {
                request_id,
                approved: false,
            },
        );
        assert!(matches!(
            actions(&fx)[..],
            [HubEffect::Decide {
                approved: false,
                ..
            }]
        ));
        rig.resolve_to(&mut link_conn, false);
    }
    // The same category without the flag is approvable: the flag decides.
    let mut plain = own;
    plain.phone_only = false;
    let fx = rig.local(LocalEvent::ConfirmRequested { request: plain });
    let msgs = deliver(&mut link_conn, &fx);
    assert!(matches!(
        msgs[0],
        ServerMessage::ConfirmPending {
            can_approve: true,
            ..
        }
    ));
}
