//! Adversarial review probes through the FFI objects. Each asserts the SAFE behaviour.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_core_ffi::*;

const KEY: [u8; 32] = [0x11; 32];
/// The shell's clock at every server `receive` in these tests (ms of shared uptime).
const NOW: u64 = 1_000_000;
/// A `not_after_ms` comfortably after [`NOW`].
const FAR: u64 = NOW + 60_000;
const OTHER: [u8; 32] = [0x22; 32];

fn entropy(seed: u8) -> Vec<u8> {
    (0u8..32)
        .map(|i| i.wrapping_mul(37).wrapping_add(seed))
        .collect()
}

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

fn problem(e: &JarvisError) -> Option<RunnerChannelProblem> {
    match e {
        JarvisError::RunnerChannel { problem, .. } => Some(*problem),
        _ => None,
    }
}

/// A squatter (other key): the client's error is Authentication, carries no key or nonce
/// bytes, and the channel is closed afterwards (never usable for a request).
#[test]
fn probe_ffi_squatter_is_authentication_and_closed() {
    let client = JarvisRunnerClientChannel::new(KEY.to_vec(), entropy(1)).unwrap();
    let squatter = JarvisRunnerServerChannel::new(OTHER.to_vec(), entropy(2)).unwrap();
    let hello = client.start().unwrap();
    let sh = sends_s(&squatter.receive(hello, NOW).unwrap());
    let err = client.receive(sh).unwrap_err();
    assert_eq!(problem(&err), Some(RunnerChannelProblem::Authentication));
    let text = format!("{err} {err:?}");
    assert!(!text.contains("17, 17") && !text.contains("0x11") && !text.contains("1111"));
    assert!(!client.is_open().unwrap());
    let r = client
        .request(RunnerWireRequest::LockState, FAR)
        .unwrap_err();
    assert_eq!(problem(&r), Some(RunnerChannelProblem::Closed));
}

/// `expect` crosses the FFI and the wire intact: the follow-up is Swift-only (the client
/// sends nil and `RunnerServer` ignores it).
#[test]
fn probe_ffi_expect_survives_both_ways() {
    let client = JarvisRunnerClientChannel::new(KEY.to_vec(), entropy(3)).unwrap();
    let server = JarvisRunnerServerChannel::new(KEY.to_vec(), entropy(4)).unwrap();
    let sh = sends_s(&server.receive(client.start().unwrap(), NOW).unwrap());
    let ev = client.receive(sh).unwrap();
    server.receive(sends_c(&ev), NOW).unwrap();
    let req = RunnerWireRequest::Perform {
        command: RunnerCommand::Tap { x: 1.0, y: 2.0 },
        expect: Some(CommandExpect {
            bundle_id: "com.apple.Preferences".into(),
        }),
    };
    let out = client.request(req.clone(), FAR).unwrap();
    let events = server.receive(out.bytes, NOW).unwrap();
    assert_eq!(
        events,
        vec![RunnerServerEvent::Request {
            request_id: out.request_id,
            not_after_ms: FAR,
            request: req
        }]
    );
    // Not-now before a reply: refused, channel stays open.
    let e = client
        .request(RunnerWireRequest::LockState, FAR)
        .unwrap_err();
    assert_eq!(problem(&e), Some(RunnerChannelProblem::NotNow));
    assert!(client.is_open().unwrap());
    // A reply of the wrong kind is refused by the server channel (nothing sealed).
    let e = server
        .reply(
            out.request_id,
            RunnerWireReply::LockState {
                state: RunnerWireLockState::Unlocked,
            },
        )
        .unwrap_err();
    assert_eq!(problem(&e), Some(RunnerChannelProblem::NotNow));
}

/// Zero / short entropy through the FFI is refused before anything is sent.
#[test]
fn probe_ffi_entropy_refused() {
    for e in [
        vec![0u8; 32],
        vec![9u8; 64],
        entropy(1)[..16].to_vec(),
        vec![],
    ] {
        let Err(err) = JarvisRunnerClientChannel::new(KEY.to_vec(), e.clone()) else {
            panic!()
        };
        assert_eq!(problem(&err), Some(RunnerChannelProblem::Entropy));
        let Err(err) = JarvisRunnerServerChannel::new(KEY.to_vec(), e) else {
            panic!()
        };
        assert_eq!(problem(&err), Some(RunnerChannelProblem::Entropy));
    }
}
