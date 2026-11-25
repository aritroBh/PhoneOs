//! `JarvisRunnerClientChannel` + `JarvisRunnerServerChannel` through the FFI objects only:
//! a handshake, every request kind with its `expect`, a typed snapshot, a stranger's key,
//! typed errors that never carry key bytes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_core_ffi::*;

fn entropy(seed: u8) -> Vec<u8> {
    (0u8..32)
        .map(|i| i.wrapping_mul(37).wrapping_add(seed))
        .collect()
}

const KEY: [u8; 32] = [0x11; 32];
/// The shell's clock at every server `receive` in these tests (ms of shared uptime).
const NOW: u64 = 1_000_000;
/// A `not_after_ms` comfortably after [`NOW`].
const FAR: u64 = NOW + 60_000;

fn sends_s(events: &[RunnerServerEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| match e {
            RunnerServerEvent::Send { bytes } => Some(bytes.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn sends_c(events: &[RunnerClientEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| match e {
            RunnerClientEvent::Send { bytes } => Some(bytes.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn connect() -> (
    std::sync::Arc<JarvisRunnerClientChannel>,
    std::sync::Arc<JarvisRunnerServerChannel>,
) {
    let client = JarvisRunnerClientChannel::new(KEY.to_vec(), entropy(1)).unwrap();
    let server = JarvisRunnerServerChannel::new(KEY.to_vec(), entropy(2)).unwrap();
    let hello = client.start().unwrap();
    let sh = sends_s(&server.receive(hello, NOW).unwrap());
    let events = client.receive(sh).unwrap();
    assert!(events.contains(&RunnerClientEvent::Authenticated));
    let events = server.receive(sends_c(&events), NOW).unwrap();
    assert_eq!(events, vec![RunnerServerEvent::Authenticated]);
    assert!(client.is_open().unwrap() && server.is_open().unwrap());
    (client, server)
}

fn round_trip(
    client: &JarvisRunnerClientChannel,
    server: &JarvisRunnerServerChannel,
    request: RunnerWireRequest,
    reply: RunnerWireReply,
) -> RunnerWireReply {
    let out = client.request(request.clone(), FAR).unwrap();
    let events = server.receive(out.bytes, NOW).unwrap();
    assert_eq!(
        events,
        vec![RunnerServerEvent::Request {
            request_id: out.request_id,
            not_after_ms: FAR,
            request
        }]
    );
    let bytes = server.reply(out.request_id, reply).unwrap();
    match client.receive(bytes).unwrap().as_slice() {
        [RunnerClientEvent::Reply { request_id, reply }] => {
            assert_eq!(*request_id, out.request_id);
            reply.clone()
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn requests_and_typed_replies_cross_intact() {
    let (client, server) = connect();
    let expect = Some(CommandExpect {
        bundle_id: "com.apple.Preferences".into(),
    });
    for (command, expect) in [
        (RunnerCommand::Tap { x: 1.0, y: 2.0 }, expect.clone()),
        (RunnerCommand::TypeText { text: "hi".into() }, expect),
        (RunnerCommand::Wait { ms: 5 }, None),
    ] {
        let reply = RunnerWireReply::Performed {
            result: RunnerResult {
                ok: false,
                error: Some("stale".into()),
                failure: Some(RunnerFailure::Stale),
            },
        };
        let got = round_trip(
            &client,
            &server,
            RunnerWireRequest::Perform { command, expect },
            reply.clone(),
        );
        assert_eq!(got, reply);
    }
    let root = UiNode {
        role: Role::Application,
        label: Some("Settings".into()),
        value: None,
        identifier: None,
        placeholder: None,
        frame: Rect {
            x: 0.0,
            y: 0.0,
            width: 402.0,
            height: 874.0,
        },
        enabled: true,
        visible: true,
        hittable: true,
        focused: false,
        selected: false,
        secure: false,
        children: vec![UiNode {
            role: Role::Other {
                name: "Custom".into(),
            },
            label: Some("Wi-Fi".into()),
            value: Some("1".into()),
            identifier: Some("wifi".into()),
            placeholder: None,
            frame: Rect {
                x: 0.0,
                y: 100.0,
                width: 402.0,
                height: 44.0,
            },
            enabled: true,
            visible: true,
            hittable: true,
            focused: false,
            selected: true,
            secure: false,
            children: vec![],
        }],
    };
    let snapshot = RunnerWireReply::Snapshot {
        snapshot: RunnerWireSnapshot {
            root,
            app_bundle_id: "com.apple.Preferences".into(),
            app_name: Some("Settings".into()),
            screen_size: Size {
                width: 402.0,
                height: 874.0,
            },
            keyboard_visible: false,
            orientation: Orientation::Portrait,
            device_locked: false,
            lock_state: RunnerWireLockState::Unlocked,
            notes: vec!["n".into()],
            screenshot: Some(RunnerWireScreenshot {
                data: vec![1, 2, 3],
                mime_type: "image/png".into(),
                pixel_width: 3,
                pixel_height: 1,
                scale: Some(3.0),
            }),
        },
    };
    let got = round_trip(
        &client,
        &server,
        RunnerWireRequest::Snapshot {
            include_screenshot: true,
        },
        snapshot.clone(),
    );
    assert_eq!(got, snapshot);
    assert!(
        !format!("{got:?}").contains("[1, 2, 3]"),
        "no pixels in Debug"
    );
    let got = round_trip(
        &client,
        &server,
        RunnerWireRequest::FrontBundleId,
        RunnerWireReply::FrontBundleId { bundle_id: None },
    );
    assert_eq!(got, RunnerWireReply::FrontBundleId { bundle_id: None });
    assert_eq!(runner_protocol_version(), 2);
}

#[test]
fn a_wrong_key_is_an_authentication_problem_and_closes_the_channel() {
    let client = JarvisRunnerClientChannel::new([0x22; 32].to_vec(), entropy(3)).unwrap();
    let server = JarvisRunnerServerChannel::new(KEY.to_vec(), entropy(4)).unwrap();
    let sh = sends_s(&server.receive(client.start().unwrap(), NOW).unwrap());
    let err = client.receive(sh).unwrap_err();
    assert!(matches!(
        err,
        JarvisError::RunnerChannel {
            problem: RunnerChannelProblem::Authentication,
            ..
        }
    ));
    let text = format!("{err} {err:?}");
    assert!(!text.contains("34") && !text.contains("0x22"), "{text}");
    assert!(!client.is_open().unwrap());
    let err = client
        .request(RunnerWireRequest::LockState, FAR)
        .unwrap_err();
    assert!(matches!(
        err,
        JarvisError::RunnerChannel {
            problem: RunnerChannelProblem::Closed,
            ..
        }
    ));
}

#[test]
fn bad_keys_and_entropy_are_typed() {
    let err = JarvisRunnerClientChannel::new(vec![1; 16], entropy(5))
        .err()
        .unwrap();
    assert!(matches!(
        err,
        JarvisError::RunnerChannel {
            problem: RunnerChannelProblem::Key,
            ..
        }
    ));
    let err = JarvisRunnerServerChannel::new(KEY.to_vec(), vec![0; 32])
        .err()
        .unwrap();
    assert!(matches!(
        err,
        JarvisError::RunnerChannel {
            problem: RunnerChannelProblem::Entropy,
            ..
        }
    ));
}

#[test]
fn an_expired_request_is_refused_through_the_ffi() {
    let client = JarvisRunnerClientChannel::new(
        KEY.to_vec(),
        (0u8..32)
            .map(|i| i.wrapping_mul(29).wrapping_add(3))
            .collect(),
    )
    .unwrap();
    let server = JarvisRunnerServerChannel::new(
        KEY.to_vec(),
        (0u8..32)
            .map(|i| i.wrapping_mul(31).wrapping_add(5))
            .collect(),
    )
    .unwrap();
    let sh = sends_s(&server.receive(client.start().unwrap(), NOW).unwrap());
    let events = client.receive(sh).unwrap();
    server.receive(sends_c(&events), NOW).unwrap();
    let out = client
        .request(
            RunnerWireRequest::Perform {
                command: RunnerCommand::Tap { x: 1.0, y: 2.0 },
                expect: None,
            },
            NOW,
        )
        .unwrap();
    let err = server.receive(out.bytes, NOW + 1).unwrap_err();
    assert!(matches!(
        err,
        JarvisError::RunnerChannel {
            problem: RunnerChannelProblem::Expired,
            ..
        }
    ));
    assert!(!server.is_open().unwrap());
}
