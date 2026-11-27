//! Sixteenth review, Part A (remote): the confinement question (Rule T) is phone only and
//! the hub refuses a remote approval of it; no lock-screen text reaches a remote device
//! (F9), whatever the shell passes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::{Action, CompactScreen, Element, Phase, Progress, Rect, Role, Size};
use jarvis_remote::{ClientMessage, HubEffect, LocalEvent, RemotePermissions, ServerMessage};
use jarvis_safety::{evaluate_confined, Decision, ScreenSource};

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

fn viewer() -> (Rig, Link) {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    let p = RemotePermissions {
        see_screen_text: true,
        approve: true,
        ..RemotePermissions::NEW_DEVICE
    };
    rig.set_permissions(&mut link, &creds, p);
    rig.request(&mut link, ClientMessage::Subscribe);
    (rig, link)
}

/// A keypad tap after the task read Messages: asked for the flow alone, phone only; a
/// paired device with "approve for you" cannot approve it.
#[test]
fn the_flow_question_is_never_approved_remotely() {
    let s = CompactScreen {
        app_bundle_id: "com.apple.mobilesafari".to_owned(),
        app_name: Some("Safari".to_owned()),
        size: Size::new(393.0, 852.0),
        keyboard_visible: false,
        elements: vec![
            el(1, Role::StaticText, "Enter the code"),
            el(2, Role::Button, "4"),
        ],
        cues: Default::default(),
    };
    let req = match evaluate_confined(
        &jarvis_proto::Policy::default(),
        &Action::Tap { element: 2 },
        &s,
        None,
        ScreenSource::Typed,
        None,
        &[],
        &["Open the page"],
        &["Messages".to_owned()],
    ) {
        Decision::Confirm(r) => r,
        d => panic!("not asked: {d:?}"),
    };
    assert!(req.phone_only);
    let (mut rig, mut conn) = viewer();
    rig.request(
        &mut conn,
        ClientMessage::StartTask {
            text: "open the page".into(),
        },
    );
    let fx = rig.local(LocalEvent::ConfirmRequested { request: req });
    let msgs = deliver(&mut conn, &fx);
    assert!(msgs.iter().any(|m| matches!(
        m,
        ServerMessage::ConfirmPending {
            can_approve: false,
            ..
        }
    )));
    let request_id = rig.hub.pending_request_id().unwrap();
    let (fx, _) = rig.request(
        &mut conn,
        ClientMessage::Decision {
            request_id,
            approved: true,
        },
    );
    assert!(
        !actions(&fx)
            .iter()
            .any(|a| matches!(a, HubEffect::Decide { approved: true, .. })),
        "a remote device approved letting what Jarvis read in Messages reach a page"
    );
}

fn progress(locked: bool, waiting: bool) -> LocalEvent {
    LocalEvent::Progress {
        progress: Progress {
            step: 1,
            phase: if waiting {
                Phase::WaitingForUnlock
            } else {
                Phase::Observing
            },
            waiting_for_unlock: waiting,
            screen_locked: locked,
            ..Progress::default()
        },
        screen_text: Some("Bank: your code is 482913".into()),
    }
}

fn screen_text_of(msgs: &[ServerMessage]) -> Option<String> {
    msgs.iter().find_map(|m| match m {
        ServerMessage::Progress { screen_text, .. } => Some(screen_text.clone()),
        _ => None,
    })?
}

/// F9: the screen text of a locked observation (or while the session waits for an unlock)
/// is dropped by the hub, whatever the shell passed; an unlocked one is sent.
#[test]
fn lock_screen_text_never_reaches_a_remote() {
    for (locked, waiting) in [(true, false), (false, true), (true, true)] {
        let (mut rig, mut link) = viewer();
        let msgs = rig.local_to(&mut link, progress(locked, waiting));
        assert!(
            msgs.iter()
                .any(|m| matches!(m, ServerMessage::Progress { .. })),
            "no progress at all"
        );
        assert_eq!(
            screen_text_of(&msgs),
            None,
            "locked {locked}, waiting {waiting}"
        );
    }
    let (mut rig, mut link) = viewer();
    let msgs = rig.local_to(&mut link, progress(false, false));
    assert!(screen_text_of(&msgs).is_some());
}
