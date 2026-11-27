//! Adversarial review probes (not product tests). Each asserts the SAFE behaviour.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::time::Instant;

use jarvis_proto::{Rect, Role, RunnerCommand, UiNode};
use jarvis_runner::channel::{CLIENT_FINISH_LEN, CLIENT_HELLO_LEN, SERVER_HELLO_LEN};
use jarvis_runner::message::{decode_reply, encode_reply};
use jarvis_runner::{
    ClientEvent, LockState, MessageError, Reply, Request, RunnerClientChannel, RunnerError,
    RunnerServerChannel, ServerEvent, MAX_CLIENT_FRAME, MAX_TREE_NODES,
};

const KEY: [u8; 32] = [0x5a; 32];

/// The shell's clock at every server `receive` in these tests (ms of shared uptime).
const NOW: u64 = 1_000_000;
/// A `not_after_ms` comfortably after [`NOW`].
const FAR: u64 = NOW + 60_000;

fn entropy(seed: u64) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..32)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn sends_s(e: &[ServerEvent]) -> Vec<u8> {
    e.iter()
        .filter_map(|e| match e {
            ServerEvent::Send(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn sends_c(e: &[ClientEvent]) -> Vec<u8> {
    e.iter()
        .filter_map(|e| match e {
            ClientEvent::Send(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn connect_with(ce: &[u8], se: &[u8]) -> (RunnerClientChannel, RunnerServerChannel, Vec<u8>) {
    let mut c = RunnerClientChannel::new(&KEY, ce).unwrap();
    let mut s = RunnerServerChannel::new(&KEY, se).unwrap();
    let hello = c.start().unwrap();
    let sh = sends_s(&s.receive(&hello, NOW).unwrap());
    let fin = sends_c(&c.receive(&sh).unwrap());
    assert_eq!(
        s.receive(&fin, NOW).unwrap(),
        vec![ServerEvent::Authenticated]
    );
    let mut c2s = hello;
    c2s.extend(fin);
    (c, s, c2s)
}

fn connect(seed: u64) -> (RunnerClientChannel, RunnerServerChannel) {
    let (c, s, _) = connect_with(&entropy(seed), &entropy(seed + 7_777));
    (c, s)
}

fn tap() -> Request {
    Request::Perform {
        command: RunnerCommand::Tap {
            x: 10.0,
            y: 20.0,
            expect: None,
        },
    }
}

// ---------------------------------------------------------------------------------------
// 1. Handshake.
// ---------------------------------------------------------------------------------------

/// The client's own hello bounced back to it is not a server hello.
#[test]
fn probe_reflection_client_hello_bounced_to_client() {
    let mut c = RunnerClientChannel::new(&KEY, &entropy(1)).unwrap();
    let hello = c.start().unwrap();
    assert!(c.receive(&hello).is_err());
    assert!(!c.is_open());
}

/// A server hello fed to a server, and a server's mac_s offered as a client finish.
#[test]
fn probe_reflection_server_proof_as_client_proof() {
    let mut c = RunnerClientChannel::new(&KEY, &entropy(2)).unwrap();
    let mut s = RunnerServerChannel::new(&KEY, &entropy(3)).unwrap();
    let hello = c.start().unwrap();
    let sh = sends_s(&s.receive(&hello, NOW).unwrap());
    // mac_s is the last 32 bytes of the server hello.
    let mac_s = &sh[sh.len() - 32..];
    let mut fin = vec![0, 0, 0, u8::try_from(CLIENT_FINISH_LEN).unwrap(), 0x03];
    fin.extend_from_slice(mac_s);
    assert_eq!(s.receive(&fin, NOW), Err(RunnerError::Auth));
    // Server hello into a fresh server.
    let mut s2 = RunnerServerChannel::new(&KEY, &entropy(4)).unwrap();
    assert!(s2.receive(&sh, NOW).is_err());
}

/// Same nonce reflected: a client hello whose nonce equals the server's own nonce and a
/// relay that hands one server's hello to two clients sharing entropy.
#[test]
fn probe_server_proof_not_reusable_across_clients() {
    // Two clients with different entropy; the relay forwards client A's hello to the
    // server and gives the server hello to client B.
    let mut a = RunnerClientChannel::new(&KEY, &entropy(10)).unwrap();
    let mut b = RunnerClientChannel::new(&KEY, &entropy(11)).unwrap();
    let mut s = RunnerServerChannel::new(&KEY, &entropy(12)).unwrap();
    let ha = a.start().unwrap();
    b.start().unwrap();
    let sh = sends_s(&s.receive(&ha, NOW).unwrap());
    assert_eq!(b.receive(&sh), Err(RunnerError::Auth));
}

/// Version byte downgrade/upgrade in either hello is refused (never negotiated).
#[test]
fn probe_version_byte_is_not_negotiable() {
    // Review pass 31: version 3 is current; 2 is refused too.
    for v in [0u8, 1, 2, 4, 0xff] {
        let mut c = RunnerClientChannel::new(&KEY, &entropy(20)).unwrap();
        let mut s = RunnerServerChannel::new(&KEY, &entropy(21)).unwrap();
        let mut hello = c.start().unwrap();
        hello[4 + 1 + 4] = v;
        assert!(s.receive(&hello, NOW).is_err(), "server took version {v}");
        let mut c = RunnerClientChannel::new(&KEY, &entropy(20)).unwrap();
        let mut s = RunnerServerChannel::new(&KEY, &entropy(21)).unwrap();
        let hello = c.start().unwrap();
        let mut sh = sends_s(&s.receive(&hello, NOW).unwrap());
        sh[4 + 1 + 4] = v;
        assert!(c.receive(&sh).is_err(), "client took version {v}");
    }
}

/// Poor entropy: short, zero, constant, or a short period is refused; side labels make
/// equal entropy on both sides harmless.
#[test]
fn probe_poor_entropy_refused() {
    assert_eq!(
        RunnerClientChannel::new(&KEY, &entropy(1)[..31]).unwrap_err(),
        RunnerError::Entropy
    );
    assert_eq!(
        RunnerServerChannel::new(&KEY, &[0u8; 32]).unwrap_err(),
        RunnerError::Entropy
    );
    assert_eq!(
        RunnerServerChannel::new(&KEY, &[7u8; 4096]).unwrap_err(),
        RunnerError::Entropy
    );
    // Period 2 and period 8.
    let p2: Vec<u8> = (0..64).map(|i| if i % 2 == 0 { 1 } else { 2 }).collect();
    assert!(RunnerClientChannel::new(&KEY, &p2).is_err());
    let p8: Vec<u8> = (0..64).map(|i| (i % 8) as u8).collect();
    assert!(RunnerClientChannel::new(&KEY, &p8).is_err());
    // Key length.
    assert_eq!(
        RunnerClientChannel::new(&KEY[..31], &entropy(1)).unwrap_err(),
        RunnerError::Key
    );
    // Equal entropy on both sides still completes and gives distinct direction keys.
    let e = entropy(99);
    let (mut c, mut s, _) = connect_with(&e, &e);
    let (id, bytes) = c.request(&Request::LockState, FAR).unwrap();
    // A c2s frame reflected back to the client is refused.
    assert!(c.receive(&bytes).is_err());
    let _ = (id, &mut s);
}

/// Nonce reuse by ONE side (client reconnects with the same entropy, or server does) still
/// changes the session keys: a request recorded in connection 1 fails in connection 2.
#[test]
fn probe_one_sided_nonce_reuse_changes_keys_both_orders() {
    for (c1, s1, c2, s2) in [(1u64, 2u64, 1u64, 3u64), (1, 2, 4, 2)] {
        let (mut ca, mut sa, _) = connect_with(&entropy(c1), &entropy(s1));
        let (_, req_a) = ca.request(&tap(), FAR).unwrap();
        sa.receive(&req_a, NOW).unwrap();
        let (_, mut sb, _) = connect_with(&entropy(c2), &entropy(s2));
        assert_eq!(
            sb.receive(&req_a, NOW),
            Err(RunnerError::Auth),
            "{c1},{s1} -> {c2},{s2}"
        );
    }
}

/// Client releases nothing before the server proof; server accepts nothing before the
/// client proof (a data frame between hello and finish).
#[test]
fn probe_nothing_released_or_acted_on_before_proofs() {
    let mut c = RunnerClientChannel::new(&KEY, &entropy(30)).unwrap();
    assert!(c.request(&tap(), FAR).is_err());
    c.start().unwrap();
    assert!(matches!(c.request(&tap(), FAR), Err(RunnerError::State(_))));
    // Server: hello, then a real sealed request frame from another session in place of the
    // finish: refused (wrong size for the handshake limit).
    let (mut oc, mut os) = connect(31);
    let (_, req) = oc.request(&tap(), FAR).unwrap();
    let _ = &mut os;
    let mut c = RunnerClientChannel::new(&KEY, &entropy(32)).unwrap();
    let mut s = RunnerServerChannel::new(&KEY, &entropy(33)).unwrap();
    let h = c.start().unwrap();
    s.receive(&h, NOW).unwrap();
    assert!(s.receive(&req, NOW).is_err());
    assert!(!s.is_open());
}

/// The client refuses a server hello before start(); a second hello mid-session is refused
/// on both sides.
#[test]
fn probe_state_confusion() {
    let mut c = RunnerClientChannel::new(&KEY, &entropy(40)).unwrap();
    let mut other_c = RunnerClientChannel::new(&KEY, &entropy(41)).unwrap();
    let mut s = RunnerServerChannel::new(&KEY, &entropy(42)).unwrap();
    let sh = sends_s(&s.receive(&other_c.start().unwrap(), NOW).unwrap());
    assert!(c.receive(&sh).is_err());
    assert!(c.start().is_err());

    let (mut c, mut s, c2s) = connect_with(&entropy(43), &entropy(44));
    let hello = &c2s[..4 + CLIENT_HELLO_LEN];
    assert!(s.receive(hello, NOW).is_err());
    assert!(!s.is_open());
    // Client after auth: a fresh server hello for its own nonce from a second server.
    let mut s2 = RunnerServerChannel::new(&KEY, &entropy(45)).unwrap();
    let sh2 = sends_s(&s2.receive(hello, NOW).unwrap());
    assert_eq!(sh2.len(), 4 + SERVER_HELLO_LEN);
    assert!(c.receive(&sh2).is_err());
    assert!(!c.is_open());
}

// ---------------------------------------------------------------------------------------
// 2. Record layer.
// ---------------------------------------------------------------------------------------

/// A relay drops reply N; reply N+1 (sealed under counter N+1) can never be accepted —
/// in fact the client never asks again on that channel, but even a forced later frame
/// fails.
#[test]
fn probe_dropped_frame_makes_later_frames_fail() {
    let (mut c, mut s) = connect(50);
    let (id1, r1) = c.request(&Request::LockState, FAR).unwrap();
    s.receive(&r1, NOW).unwrap();
    let _dropped = s.reply(id1, &Reply::LockState(LockState::Locked)).unwrap();
    // Client cannot send another request while one is outstanding.
    assert!(matches!(
        c.request(&Request::LockState, FAR),
        Err(RunnerError::State(_))
    ));
    // Server-side: a frame with counter 1 after counter 0 was dropped is refused.
    let (mut c, mut s) = connect(51);
    let (_, r1) = c.request(&Request::LockState, FAR).unwrap();
    // Drop r1; force the client to make another (simulate by a second client channel with
    // the same keys is impossible); instead rewrite r1's counter to 1.
    let mut forged = r1.clone();
    forged[4 + 1 + 7] = 1;
    assert_eq!(s.receive(&forged, NOW), Err(RunnerError::Auth));
}

/// Trailing byte after the tag, a length prefix that covers one byte less/more, and a
/// counter field that disagrees with the AEAD nonce.
#[test]
fn probe_record_malleability() {
    let (mut c, _s) = connect(60);
    let (_, r) = c.request(&Request::LockState, FAR).unwrap();
    // +1 trailing byte inside the frame.
    let (_, mut s) = connect_pair_same(60);
    let mut t = r.clone();
    t.push(0);
    let len = u32::from_be_bytes([t[0], t[1], t[2], t[3]]) + 1;
    t[..4].copy_from_slice(&len.to_be_bytes());
    assert_eq!(s.receive(&t, NOW), Err(RunnerError::Auth));
    // Truncated by one byte.
    let (_, mut s) = connect_pair_same(60);
    let mut t = r[..r.len() - 1].to_vec();
    let len = u32::from_be_bytes([t[0], t[1], t[2], t[3]]) - 1;
    t[..4].copy_from_slice(&len.to_be_bytes());
    assert_eq!(s.receive(&t, NOW), Err(RunnerError::Auth));
    // Kind byte changed.
    let (_, mut s) = connect_pair_same(60);
    let mut t = r.clone();
    t[4] = 0x11;
    assert!(s.receive(&t, NOW).is_err());
    // Genuine frame still works on a fresh copy.
    let (_, mut s) = connect_pair_same(60);
    assert!(s.receive(&r, NOW).is_ok());
}

fn connect_pair_same(seed: u64) -> (RunnerClientChannel, RunnerServerChannel) {
    connect(seed)
}

// ---------------------------------------------------------------------------------------
// 3. Request/reply binding.
// ---------------------------------------------------------------------------------------

/// A reply for an earlier request id, a duplicate, and an unsolicited reply all close the
/// client (ids under the AEAD; a relay cannot change them anyway).
#[test]
fn probe_reply_binding() {
    let (mut c, mut s) = connect(70);
    let (id1, r1) = c.request(&Request::LockState, FAR).unwrap();
    s.receive(&r1, NOW).unwrap();
    let rep1 = s
        .reply(id1, &Reply::LockState(LockState::Unlocked))
        .unwrap();
    assert_eq!(c.receive(&rep1).unwrap().len(), 1);
    // Duplicate delivery of the same bytes.
    assert!(c.receive(&rep1).is_err());
    assert!(!c.is_open());

    // Wrong kind for a snapshot request (a LockState reply) is refused even with the
    // right id: the server channel refuses to seal it.
    let (mut c, mut s) = connect(71);
    let (id, r) = c
        .request(
            &Request::Snapshot {
                include_screenshot: false,
            },
            FAR,
        )
        .unwrap();
    s.receive(&r, NOW).unwrap();
    assert!(matches!(
        s.reply(id, &Reply::LockState(LockState::Unlocked)),
        Err(RunnerError::State(_))
    ));
    // Replying to a request id other than the outstanding one is refused.
    assert!(s
        .reply(
            id + 1,
            &Reply::Error {
                message: "x".into()
            }
        )
        .is_err());
}

/// Server request ids strictly increase; the server refuses a second request while one is
/// outstanding even when both arrive in one read (and none of them is surfaced).
#[test]
fn probe_server_pipelined_requests_refused_none_surfaced() {
    let (mut c, mut s) = connect(80);
    let (id, r1) = c.request(&Request::LockState, FAR).unwrap();
    // Force a second request by sealing through a cloned scenario: deliver r1 twice in one
    // buffer: the second is a replay (counter 0) -> Auth; no event escapes.
    let mut both = r1.clone();
    both.extend(&r1);
    assert!(s.receive(&both, NOW).is_err());
    let _ = id;
}

// ---------------------------------------------------------------------------------------
// 4. Framing / codec limits.
// ---------------------------------------------------------------------------------------

/// Pre-auth length limits refuse on the 4-byte header alone, on both sides.
#[test]
fn probe_preauth_limits() {
    let mut s = RunnerServerChannel::new(&KEY, &entropy(90)).unwrap();
    let hdr = u32::try_from(CLIENT_HELLO_LEN + 1).unwrap().to_be_bytes();
    assert!(s.receive(&hdr, NOW).is_err());
    let mut c = RunnerClientChannel::new(&KEY, &entropy(91)).unwrap();
    c.start().unwrap();
    let hdr = u32::try_from(SERVER_HELLO_LEN + 1).unwrap().to_be_bytes();
    assert!(c.receive(&hdr).is_err());
    // A header claiming exactly the limit, then one byte at a time: no event, no error.
    let mut s = RunnerServerChannel::new(&KEY, &entropy(92)).unwrap();
    let hdr = u32::try_from(CLIENT_HELLO_LEN).unwrap().to_be_bytes();
    assert_eq!(s.receive(&hdr, NOW).unwrap(), vec![]);
    for _ in 0..CLIENT_HELLO_LEN - 1 {
        assert_eq!(s.receive(&[0], NOW).unwrap(), vec![]);
    }
    // Post-auth: one byte over the client frame limit on the header alone.
    let (_, mut s) = connect(93);
    let hdr = u32::try_from(MAX_CLIENT_FRAME + 1).unwrap().to_be_bytes();
    assert!(s.receive(&hdr, NOW).is_err());
}

fn encode_chain(depth: usize) -> Vec<u8> {
    // A REP_SNAPSHOT body by hand: reuse the codec on a small snapshot, then splice in a
    // hand-built chain tree. Simpler: build the UiNode chain and encode (write side refuses
    // past the limit, so build at the limit and patch child counts).
    let mut node = UiNode::new(Role::Button, Rect::new(0.0, 0.0, 1.0, 1.0));
    for _ in 1..depth {
        let mut p = UiNode::new(Role::Other("x".into()), Rect::new(0.0, 0.0, 1.0, 1.0));
        p.children = vec![node];
        node = p;
    }
    let snap = jarvis_runner::Snapshot {
        tree_cut: false,
        root: node,
        app_bundle_id: "a".into(),
        app_name: None,
        screen_size: jarvis_proto::Size::new(1.0, 1.0),
        keyboard_visible: false,
        orientation: jarvis_proto::Orientation::Portrait,
        device_locked: false,
        lock_state: LockState::Unknown,
        notes: vec![],
        screenshot: None,
    };
    encode_reply(1, &Reply::Snapshot(Box::new(snap))).unwrap()
}

/// Depth at the limit decodes; 100k flat nodes decode in linear time; one over the node
/// limit is refused; a hostile child count is refused without allocating.
#[test]
fn probe_decoder_limits_and_speed() {
    let at_limit = encode_chain(jarvis_runner::MAX_TREE_DEPTH);
    assert!(decode_reply(&at_limit).is_ok());

    let mut root = UiNode::new(Role::Application, Rect::new(0.0, 0.0, 1.0, 1.0));
    root.children = (0..MAX_TREE_NODES - 1)
        .map(|_| UiNode::new(Role::Button, Rect::new(0.0, 0.0, 1.0, 1.0)))
        .collect();
    let snap = jarvis_runner::Snapshot {
        tree_cut: false,
        root: root.clone(),
        app_bundle_id: "a".into(),
        app_name: None,
        screen_size: jarvis_proto::Size::new(1.0, 1.0),
        keyboard_visible: false,
        orientation: jarvis_proto::Orientation::Portrait,
        device_locked: false,
        lock_state: LockState::Unknown,
        notes: vec![],
        screenshot: None,
    };
    let bytes = encode_reply(1, &Reply::Snapshot(Box::new(snap.clone()))).unwrap();
    assert!(bytes.len() < jarvis_runner::MAX_SERVER_FRAME);
    let t = Instant::now();
    assert!(decode_reply(&bytes).is_ok());
    assert!(
        t.elapsed().as_secs() < 5,
        "decode too slow: {:?}",
        t.elapsed()
    );

    // One more node: patch the root's child count +1 and append one leaf.
    let leaf_len = 42;
    let mut over = bytes.clone();
    over.extend_from_slice(&bytes[bytes.len() - leaf_len..]);
    // Root child count sits right before the first child: find it as the u32
    // (MAX_TREE_NODES - 1) after the root node header.
    let needle = u32::try_from(MAX_TREE_NODES - 1).unwrap().to_be_bytes();
    let pos = over
        .windows(4)
        .position(|w| w == needle)
        .expect("root child count");
    over[pos..pos + 4].copy_from_slice(&u32::try_from(MAX_TREE_NODES).unwrap().to_be_bytes());
    assert_eq!(decode_reply(&over), Err(MessageError::Limit("tree nodes")));

    // Hostile child count: u32::MAX children with few bytes left.
    let mut hostile = bytes[..pos + 4].to_vec();
    hostile[pos..pos + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    hostile.extend_from_slice(&[0u8; 100]);
    assert_eq!(decode_reply(&hostile), Err(MessageError::Truncated));
}

/// Worst case for the closing loop: a "comb" (each node has 2 children: a leaf and the
/// rest) at the depth limit, repeated — still linear.
#[test]
fn probe_decoder_no_quadratic_on_deep_wide_mix() {
    // Stacked chains at the depth limit under one root: ~100k nodes (pass 30: the limit is
    // 160, was 256; 620 chains keep the node count).
    let chain = |d: usize| {
        let mut n = UiNode::new(Role::Button, Rect::new(0.0, 0.0, 1.0, 1.0));
        for _ in 1..d {
            let mut p = UiNode::new(Role::Cell, Rect::new(0.0, 0.0, 1.0, 1.0));
            p.children = vec![n];
            n = p;
        }
        n
    };
    let mut root = UiNode::new(Role::Application, Rect::new(0.0, 0.0, 1.0, 1.0));
    root.children = (0..620)
        .map(|_| chain(jarvis_runner::MAX_TREE_DEPTH - 1))
        .collect();
    let snap = jarvis_runner::Snapshot {
        tree_cut: false,
        root,
        app_bundle_id: "a".into(),
        app_name: None,
        screen_size: jarvis_proto::Size::new(1.0, 1.0),
        keyboard_visible: false,
        orientation: jarvis_proto::Orientation::Portrait,
        device_locked: false,
        lock_state: LockState::Unknown,
        notes: vec![],
        screenshot: None,
    };
    let bytes = encode_reply(1, &Reply::Snapshot(Box::new(snap))).unwrap();
    let t = Instant::now();
    assert!(decode_reply(&bytes).is_ok());
    assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
}

/// Hostile counts on strings/notes/screenshot are validated against remaining bytes.
#[test]
fn probe_counts_validated_before_allocation() {
    // REP_FRONT with Some + string length u32::MAX.
    let mut b = vec![0x86];
    b.extend_from_slice(&1u64.to_be_bytes());
    b.push(1);
    b.extend_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(decode_reply(&b), Err(MessageError::Truncated));
    // REP_ERROR with length 1 but 0 bytes.
    let mut b = vec![0xEF];
    b.extend_from_slice(&1u64.to_be_bytes());
    b.extend_from_slice(&1u32.to_be_bytes());
    assert_eq!(decode_reply(&b), Err(MessageError::Truncated));
}
