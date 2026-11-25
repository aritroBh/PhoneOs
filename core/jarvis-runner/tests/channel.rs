//! The protocol proof: handshake, frames, messages and every attack the loopback channel
//! must survive, with no I/O (the two channels talk through byte vectors).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_proto::{
    Expect, Key, Orientation, Point, Rect, Role, RunnerCommand, RunnerFailure, RunnerResult, Size,
    UiNode,
};
use jarvis_runner::channel::{CLIENT_FINISH_LEN, CLIENT_HELLO_LEN, SERVER_HELLO_LEN};
use jarvis_runner::message::{decode_reply, decode_request, encode_reply, encode_request};
use jarvis_runner::{
    ClientEvent, LockState, MessageError, Reply, Request, RunnerClientChannel, RunnerError,
    RunnerServerChannel, Screenshot, ServerEvent, Snapshot, MAX_CLIENT_FRAME, MAX_SERVER_FRAME,
    PROTOCOL_VERSION,
};

const KEY: [u8; 32] = [0x5a; 32];
const OTHER_KEY: [u8; 32] = [0xa5; 32];

/// The shell's clock at every server `receive` in these tests (ms of shared uptime).
const NOW: u64 = 1_000_000;
/// A `not_after_ms` comfortably after [`NOW`].
const FAR: u64 = NOW + 60_000;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| (self.next() >> 24) as u8).collect()
    }
}

/// 32 bytes of test entropy, different per seed.
fn entropy(seed: u64) -> Vec<u8> {
    Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1).bytes(32)
}

/// The bytes a `Send` event carries.
fn sends_s(events: &[ServerEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| match e {
            ServerEvent::Send(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn sends_c(events: &[ClientEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::Send(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// A full handshake; also returns every byte each side wrote (for replay tests).
fn connect_recording(seed: u64) -> (RunnerClientChannel, RunnerServerChannel, Vec<u8>, Vec<u8>) {
    let mut client = RunnerClientChannel::new(&KEY, &entropy(seed)).unwrap();
    let mut server = RunnerServerChannel::new(&KEY, &entropy(seed + 1_000)).unwrap();
    let hello = client.start().unwrap();
    let s_events = server.receive(&hello, NOW).unwrap();
    let server_hello = sends_s(&s_events);
    let c_events = client.receive(&server_hello).unwrap();
    assert!(c_events.contains(&ClientEvent::Authenticated));
    let finish = sends_c(&c_events);
    let s_events = server.receive(&finish, NOW).unwrap();
    assert_eq!(s_events, vec![ServerEvent::Authenticated]);
    assert!(client.is_open() && server.is_open());
    let mut c2s = hello;
    c2s.extend(finish);
    (client, server, c2s, server_hello)
}

fn connect(seed: u64) -> (RunnerClientChannel, RunnerServerChannel) {
    let (c, s, _, _) = connect_recording(seed);
    (c, s)
}

/// One request through both channels; returns the reply the client got.
fn exchange(
    client: &mut RunnerClientChannel,
    server: &mut RunnerServerChannel,
    request: &Request,
    reply: &Reply,
) -> Reply {
    let (id, bytes) = client.request(request, FAR).unwrap();
    let events = server.receive(&bytes, NOW).unwrap();
    assert_eq!(
        events,
        vec![ServerEvent::Request {
            id,
            not_after_ms: FAR,
            request: request.clone()
        }]
    );
    let out = server.reply(id, reply).unwrap();
    let events = client.receive(&out).unwrap();
    match events.as_slice() {
        [ClientEvent::Reply { id: got, reply }] => {
            assert_eq!(*got, id);
            reply.clone()
        }
        other => panic!("unexpected events {other:?}"),
    }
}

fn node(role: Role, label: &str, n: u32) -> UiNode {
    let f = f64::from(n);
    UiNode {
        role,
        label: Some(format!(
            "{label} {n} ✓ \"quoted\", {{{{1, 2}}, {{3, 4}}}}\nnext line"
        )),
        value: (n.is_multiple_of(3)).then(|| format!("value {n}")),
        identifier: (n.is_multiple_of(5)).then(|| format!("id.{n}")),
        placeholder: (n.is_multiple_of(7)).then(String::new),
        frame: Rect::new(f, f * 2.0, 10.5, 44.0),
        enabled: n.is_multiple_of(2),
        visible: !n.is_multiple_of(3),
        hittable: n.is_multiple_of(4),
        focused: n.is_multiple_of(11),
        selected: n.is_multiple_of(13),
        secure: n.is_multiple_of(17),
        children: vec![],
    }
}

/// A tree of `count` nodes: a few deep chains and wide lists.
fn big_tree(count: u32) -> UiNode {
    let mut root = node(Role::Application, "root", 0);
    let mut n = 1;
    let mut section = 0;
    while n < count {
        section += 1;
        let mut table = node(Role::Table, "table", n);
        n += 1;
        let mut i = 0;
        while i < 200 && n < count {
            let role = if i % 9 == 0 {
                Role::Other(format!("Type{i}"))
            } else {
                Role::Cell
            };
            let mut cell = node(role, "cell", n);
            n += 1;
            if n < count {
                cell.children.push(node(Role::StaticText, "text", n));
                n += 1;
            }
            table.children.push(cell);
            i += 1;
        }
        // Every tenth section is nested 100 deep.
        if section % 10 == 0 {
            let mut deep = node(Role::Other("Deep".into()), "deep", n);
            for d in 0..100 {
                let mut parent = node(Role::Window, "level", d);
                parent.children.push(deep);
                deep = parent;
            }
            table.children.push(deep);
        }
        root.children.push(table);
    }
    root
}

fn count_nodes(n: &UiNode) -> usize {
    let mut stack = vec![n];
    let mut c = 0;
    while let Some(x) = stack.pop() {
        c += 1;
        stack.extend(x.children.iter());
    }
    c
}

fn screenshot(len: usize) -> Screenshot {
    Screenshot {
        data: Rng(42).bytes(len),
        mime_type: "image/png".into(),
        pixel_width: 1320,
        pixel_height: 2868,
        scale: Some(3.0),
    }
}

fn snapshot(root: UiNode, shot: Option<Screenshot>) -> Snapshot {
    Snapshot {
        root,
        app_bundle_id: "dev.jarvis.app".into(),
        app_name: Some("Jarvis".into()),
        screen_size: Size::new(440.0, 956.0),
        keyboard_visible: true,
        orientation: Orientation::LandscapeLeft,
        device_locked: false,
        lock_state: LockState::Unlocked,
        notes: vec!["a note".into(), String::new()],
        screenshot: shot,
    }
}

fn every_command() -> Vec<RunnerCommand> {
    let expect = Some(Expect {
        bundle_id: "com.apple.Preferences".into(),
    });
    vec![
        RunnerCommand::Tap {
            x: 1.5,
            y: -2.0,
            expect: expect.clone(),
        },
        RunnerCommand::Tap {
            x: 0.0,
            y: 0.0,
            expect: None,
        },
        RunnerCommand::LongPress {
            x: 3.0,
            y: 4.0,
            duration_ms: 900,
            expect: expect.clone(),
        },
        RunnerCommand::Swipe {
            from: Point::new(1.0, 2.0),
            to: Point::new(3.0, 4.0),
            duration_ms: 250,
            expect: None,
        },
        RunnerCommand::TypeText {
            text: "héllo\n\"world\"".into(),
            expect: expect.clone(),
        },
        RunnerCommand::PressKey {
            key: Key::Return,
            expect: None,
        },
        RunnerCommand::PressKey {
            key: Key::Home,
            expect,
        },
        RunnerCommand::LaunchApp {
            bundle_id: "com.apple.mobilenotes".into(),
        },
        RunnerCommand::OpenUrl {
            url: "https://example.com/?q=1".into(),
        },
        RunnerCommand::Wait { ms: u64::MAX },
    ]
}

// ---------------------------------------------------------------------------------------
// Round trips.
// ---------------------------------------------------------------------------------------

#[test]
fn every_message_round_trips_through_the_channels() {
    let (mut client, mut server) = connect(1);
    let cases: Vec<(Request, Reply)> = vec![
        (
            Request::Hello {
                protocol_version: PROTOCOL_VERSION,
            },
            Reply::Hello {
                protocol_version: PROTOCOL_VERSION,
                runner: "JarvisRunner test".into(),
            },
        ),
        (
            Request::Snapshot {
                include_screenshot: false,
            },
            Reply::Snapshot(Box::new(snapshot(big_tree(50), None))),
        ),
        (
            Request::Snapshot {
                include_screenshot: true,
            },
            Reply::Snapshot(Box::new(snapshot(big_tree(10), Some(screenshot(1000))))),
        ),
        (Request::LockState, Reply::LockState(LockState::Locked)),
        (Request::LockState, Reply::LockState(LockState::Unlocked)),
        (Request::LockState, Reply::LockState(LockState::Unknown)),
        (Request::ViewerScreenshot, Reply::ViewerScreenshot(None)),
        (
            Request::ViewerScreenshot,
            Reply::ViewerScreenshot(Some(Screenshot {
                scale: None,
                ..screenshot(10)
            })),
        ),
        (Request::FrontBundleId, Reply::FrontBundleId(None)),
        (
            Request::FrontBundleId,
            Reply::FrontBundleId(Some("com.apple.springboard".into())),
        ),
        (
            Request::LockState,
            Reply::Error {
                message: "the runner could not read the lock state".into(),
            },
        ),
    ];
    let results = [
        RunnerResult {
            ok: true,
            error: None,
            failure: None,
        },
        RunnerResult {
            ok: false,
            error: Some("locked".into()),
            failure: Some(RunnerFailure::DeviceLocked),
        },
        RunnerResult {
            ok: false,
            error: Some("stale".into()),
            failure: Some(RunnerFailure::Stale),
        },
        RunnerResult {
            ok: false,
            error: None,
            failure: Some(RunnerFailure::Other),
        },
    ];
    let mut all = cases;
    for (i, command) in every_command().into_iter().enumerate() {
        all.push((
            Request::Perform { command },
            Reply::Performed(results[i % results.len()].clone()),
        ));
    }
    for (request, reply) in &all {
        assert_eq!(&exchange(&mut client, &mut server, request, reply), reply);
    }
}

#[test]
fn a_20k_node_tree_and_a_large_screenshot_round_trip() {
    let tree = big_tree(20_000);
    assert!(count_nodes(&tree) >= 20_000);
    let shot = screenshot(8 * 1024 * 1024);
    let reply = Reply::Snapshot(Box::new(snapshot(tree, Some(shot))));
    let (mut client, mut server) = connect(2);
    let got = exchange(
        &mut client,
        &mut server,
        &Request::Snapshot {
            include_screenshot: true,
        },
        &reply,
    );
    assert_eq!(got, reply);
}

#[test]
fn byte_at_a_time_delivery_equals_whole_buffer_delivery() {
    let reply = Reply::Snapshot(Box::new(snapshot(big_tree(300), Some(screenshot(2000)))));
    // Whole buffers.
    let (mut c1, mut s1, c2s, s2c) = connect_recording(3);
    let (id, req) = c1
        .request(
            &Request::Snapshot {
                include_screenshot: true,
            },
            FAR,
        )
        .unwrap();
    s1.receive(&req, NOW).unwrap();
    let rep = s1.reply(id, &reply).unwrap();
    let whole = c1.receive(&rep).unwrap();

    // The same bytes one at a time, both directions, handshake included.
    let mut client = RunnerClientChannel::new(&KEY, &entropy(3)).unwrap();
    let mut server = RunnerServerChannel::new(&KEY, &entropy(1_003)).unwrap();
    let hello = client.start().unwrap();
    let mut s_events = Vec::new();
    for b in &hello {
        s_events.extend(server.receive(std::slice::from_ref(b), NOW).unwrap());
    }
    let server_hello = sends_s(&s_events);
    assert_eq!(server_hello, s2c, "the same entropy gives the same hello");
    let mut c_events = Vec::new();
    for b in &server_hello {
        c_events.extend(client.receive(std::slice::from_ref(b)).unwrap());
    }
    let finish = sends_c(&c_events);
    assert_eq!([hello.as_slice(), finish.as_slice()].concat(), c2s);
    for b in &finish {
        server.receive(std::slice::from_ref(b), NOW).unwrap();
    }
    let (id2, req2) = client
        .request(
            &Request::Snapshot {
                include_screenshot: true,
            },
            FAR,
        )
        .unwrap();
    assert_eq!((id2, &req2), (id, &req));
    let mut got = Vec::new();
    for b in &req2 {
        got.extend(server.receive(std::slice::from_ref(b), NOW).unwrap());
    }
    assert_eq!(got.len(), 1);
    let rep2 = server.reply(id2, &reply).unwrap();
    assert_eq!(rep2, rep);
    let mut events = Vec::new();
    for b in &rep2 {
        events.extend(client.receive(std::slice::from_ref(b)).unwrap());
    }
    assert_eq!(events, whole);
}

#[test]
fn the_codec_refuses_truncation_and_trailing_bytes_everywhere() {
    let request = encode_request(
        7,
        FAR,
        &Request::Perform {
            command: every_command()[4].clone(),
        },
    )
    .unwrap();
    let reply = encode_reply(
        7,
        &Reply::Snapshot(Box::new(snapshot(big_tree(40), Some(screenshot(64))))),
    )
    .unwrap();
    for cut in 0..request.len() {
        assert!(decode_request(&request[..cut]).is_err(), "cut at {cut}");
    }
    for cut in 0..reply.len() {
        assert!(decode_reply(&reply[..cut]).is_err(), "cut at {cut}");
    }
    let mut longer = request.clone();
    longer.push(0);
    assert_eq!(decode_request(&longer), Err(MessageError::Trailing));
    let mut longer = reply.clone();
    longer.push(0);
    assert_eq!(decode_reply(&longer), Err(MessageError::Trailing));
    // Unknown kinds, and a request where a reply is expected.
    assert!(decode_request(&[0x7f, 0, 0, 0, 0, 0, 0, 0, 1]).is_err());
    assert!(decode_reply(&request).is_err());
    assert!(decode_request(&reply).is_err());
}

#[test]
fn non_finite_coordinates_are_never_sent_or_accepted() {
    let (mut client, _server) = connect(4);
    let bad = Request::Perform {
        command: RunnerCommand::Tap {
            x: f64::NAN,
            y: 1.0,
            expect: None,
        },
    };
    assert_eq!(
        client.request(&bad, FAR),
        Err(RunnerError::TooLarge(MessageError::NonFinite))
    );
    // Refused before sealing: the channel is still usable.
    assert!(client.is_open() && !client.has_pending_request());
    let mut tree = big_tree(5);
    tree.children[0].frame.width = f64::INFINITY;
    let (mut client, mut server) = connect(5);
    let got = exchange(
        &mut client,
        &mut server,
        &Request::Snapshot {
            include_screenshot: false,
        },
        &Reply::Snapshot(Box::new(snapshot(tree, None))),
    );
    // The server could not encode it, so the client hears an error, never a guess.
    assert!(matches!(got, Reply::Error { message } if message.contains("non-finite")));
}

// ---------------------------------------------------------------------------------------
// Frames.
// ---------------------------------------------------------------------------------------

#[test]
fn oversized_frames_fail_on_their_length_alone() {
    // Before authentication: anything longer than a hello.
    let mut server = RunnerServerChannel::new(&KEY, &entropy(6)).unwrap();
    let len = u32::try_from(CLIENT_HELLO_LEN + 1).unwrap().to_be_bytes();
    assert!(matches!(
        server.receive(&len, NOW),
        Err(RunnerError::Frame(_))
    ));
    assert_eq!(server.receive(&[0], NOW), Err(RunnerError::Closed));

    let mut client = RunnerClientChannel::new(&KEY, &entropy(7)).unwrap();
    client.start().unwrap();
    let len = u32::try_from(SERVER_HELLO_LEN + 1).unwrap().to_be_bytes();
    assert!(matches!(client.receive(&len), Err(RunnerError::Frame(_))));
    assert_eq!(client.receive(&[0]), Err(RunnerError::Closed));

    // After authentication: over the data limits.
    let (mut client, mut server) = connect(8);
    let len = u32::try_from(MAX_CLIENT_FRAME + 1).unwrap().to_be_bytes();
    assert!(matches!(
        server.receive(&len, NOW),
        Err(RunnerError::Frame(_))
    ));
    let len = u32::try_from(MAX_SERVER_FRAME + 1).unwrap().to_be_bytes();
    assert!(matches!(client.receive(&len), Err(RunnerError::Frame(_))));
    // Zero-length frames.
    let (mut client, mut server) = connect(9);
    assert!(server.receive(&[0, 0, 0, 0], NOW).is_err());
    assert!(client.receive(&[0, 0, 0, 0]).is_err());
}

#[test]
fn a_request_too_large_for_the_protocol_is_refused_before_sending() {
    let (mut client, _server) = connect(10);
    let huge = Request::Perform {
        command: RunnerCommand::TypeText {
            text: "x".repeat(MAX_CLIENT_FRAME),
            expect: None,
        },
    };
    assert!(matches!(
        client.request(&huge, FAR),
        Err(RunnerError::TooLarge(_))
    ));
    assert!(client.is_open() && !client.has_pending_request());
}

#[test]
fn a_reply_too_large_for_the_protocol_becomes_an_error_reply() {
    let (mut client, mut server) = connect(11);
    let shot = screenshot(jarvis_runner::MAX_SCREENSHOT_BYTES + 1);
    let got = exchange(
        &mut client,
        &mut server,
        &Request::ViewerScreenshot,
        &Reply::ViewerScreenshot(Some(shot)),
    );
    assert!(matches!(got, Reply::Error { .. }));
}

#[test]
fn malformed_handshake_frames_close_the_channel() {
    let good_hello = RunnerClientChannel::new(&KEY, &entropy(12))
        .unwrap()
        .start()
        .unwrap();
    // Wrong kind, magic, version, length.
    let mut variants = Vec::new();
    for (i, delta) in [(4usize, 1u8), (5, 1), (9, 1)] {
        let mut v = good_hello.clone();
        v[i] ^= delta;
        variants.push(v);
    }
    let mut short = good_hello.clone();
    short.truncate(good_hello.len() - 1);
    short[3] -= 1;
    variants.push(short);
    for v in variants {
        let mut server = RunnerServerChannel::new(&KEY, &entropy(13)).unwrap();
        assert!(server.receive(&v, NOW).is_err());
        assert!(!server.is_open());
        assert_eq!(server.receive(&good_hello, NOW), Err(RunnerError::Closed));
    }
    // A data frame instead of a hello.
    let (mut client, _) = connect(14);
    let (_, data) = client.request(&Request::LockState, FAR).unwrap();
    let mut server = RunnerServerChannel::new(&KEY, &entropy(15)).unwrap();
    assert!(server.receive(&data, NOW).is_err());
    // The server never speaks first; a client hearing it before start fails.
    let mut client = RunnerClientChannel::new(&KEY, &entropy(16)).unwrap();
    assert!(client.receive(&good_hello).is_err());
}

// ---------------------------------------------------------------------------------------
// Authentication.
// ---------------------------------------------------------------------------------------

/// A stranger without the key reaches the runner: it never gets past the handshake, and
/// no request is ever delivered.
#[test]
fn a_client_with_the_wrong_key_gets_nothing() {
    let mut stranger = RunnerClientChannel::new(&OTHER_KEY, &entropy(20)).unwrap();
    let mut server = RunnerServerChannel::new(&KEY, &entropy(21)).unwrap();
    let hello = stranger.start().unwrap();
    let s_events = server.receive(&hello, NOW).unwrap();
    // The server's hello: the stranger cannot verify it.
    assert_eq!(
        stranger.receive(&sends_s(&s_events)),
        Err(RunnerError::Auth)
    );
    assert!(!stranger.is_open());
    // Forging a finish from the transcript fails too.
    for guess in [[0u8; 32], [0xff; 32]] {
        let mut server = RunnerServerChannel::new(&KEY, &entropy(22)).unwrap();
        server.receive(&hello, NOW).unwrap();
        let mut finish = vec![0, 0, 0, u8::try_from(CLIENT_FINISH_LEN).unwrap(), 0x03];
        finish.extend_from_slice(&guess);
        assert_eq!(server.receive(&finish, NOW), Err(RunnerError::Auth));
        assert!(!server.is_open());
    }
}

/// Another app binds the port first and serves "typed" trees: the client never
/// authenticates it, never sends it a request, and never yields a reply.
#[test]
fn a_squatting_server_is_never_trusted() {
    let mut client = RunnerClientChannel::new(&KEY, &entropy(30)).unwrap();
    let mut squatter = RunnerServerChannel::new(&OTHER_KEY, &entropy(31)).unwrap();
    let hello = client.start().unwrap();
    let forged = sends_s(&squatter.receive(&hello, NOW).unwrap());
    assert_eq!(client.receive(&forged), Err(RunnerError::Auth));
    assert!(!client.is_open());
    assert_eq!(
        client.request(
            &Request::Snapshot {
                include_screenshot: false
            },
            FAR
        ),
        Err(RunnerError::Closed)
    );
    // A squatter replaying a real server's hello from an earlier connection fails as well:
    // the MAC covers this client's fresh nonce.
    let (_, _, _, old_server_hello) = connect_recording(32);
    let mut client = RunnerClientChannel::new(&KEY, &entropy(33)).unwrap();
    client.start().unwrap();
    assert_eq!(client.receive(&old_server_hello), Err(RunnerError::Auth));
    // And a squatter that skips the handshake and sends (real, sealed) data frames is
    // refused.
    let (mut c, mut s) = connect(34);
    let (id, req) = c.request(&Request::LockState, FAR).unwrap();
    s.receive(&req, NOW).unwrap();
    let data = s.reply(id, &Reply::LockState(LockState::Unlocked)).unwrap();
    let mut client = RunnerClientChannel::new(&KEY, &entropy(36)).unwrap();
    client.start().unwrap();
    assert!(client.receive(&data).is_err());
    assert!(!client.is_open());
}

#[test]
fn handshake_tampering_is_detected() {
    // Flip each byte of the server hello after its length: the client refuses all.
    let (_, _, c2s, s2c) = connect_recording(50);
    for i in 4..s2c.len() {
        let mut client = RunnerClientChannel::new(&KEY, &entropy(50)).unwrap();
        client.start().unwrap();
        let mut t = s2c.clone();
        t[i] ^= 0x01;
        assert!(client.receive(&t).is_err(), "byte {i}");
        assert!(!client.is_open());
    }
    // Flip each byte of the client hello (after its length): the server's hello then fails
    // the client's check, or the server refuses it outright.
    let hello_len = 4 + CLIENT_HELLO_LEN;
    for i in 4..hello_len {
        let mut client = RunnerClientChannel::new(&KEY, &entropy(50)).unwrap();
        let mut server = RunnerServerChannel::new(&KEY, &entropy(1_050)).unwrap();
        client.start().unwrap();
        let mut t = c2s[..hello_len].to_vec();
        t[i] ^= 0x01;
        match server.receive(&t, NOW) {
            Err(_) => assert!(!server.is_open()),
            Ok(events) => {
                assert_eq!(client.receive(&sends_s(&events)), Err(RunnerError::Auth));
            }
        }
    }
    // Flip each byte of the client finish: the server refuses.
    for i in hello_len + 4..c2s.len() {
        let mut server = RunnerServerChannel::new(&KEY, &entropy(1_050)).unwrap();
        server.receive(&c2s[..hello_len], NOW).unwrap();
        let mut t = c2s[hello_len..].to_vec();
        t[i - hello_len] ^= 0x01;
        assert!(server.receive(&t, NOW).is_err(), "byte {i}");
        assert!(!server.is_open());
    }
}

// ---------------------------------------------------------------------------------------
// Replay, reorder, cross-direction.
// ---------------------------------------------------------------------------------------

#[test]
fn a_replayed_request_is_refused() {
    let (mut client, mut server) = connect(60);
    let (id, bytes) = client.request(&Request::LockState, FAR).unwrap();
    server.receive(&bytes, NOW).unwrap();
    let out = server
        .reply(id, &Reply::LockState(LockState::Unlocked))
        .unwrap();
    client.receive(&out).unwrap();
    assert_eq!(server.receive(&bytes, NOW), Err(RunnerError::Auth));
    assert!(!server.is_open());
}

#[test]
fn a_replayed_reply_is_refused() {
    let (mut client, mut server) = connect(61);
    let (id, bytes) = client.request(&Request::LockState, FAR).unwrap();
    server.receive(&bytes, NOW).unwrap();
    let out = server
        .reply(id, &Reply::LockState(LockState::Unlocked))
        .unwrap();
    client.receive(&out).unwrap();
    let (_, bytes) = client.request(&Request::LockState, FAR).unwrap();
    server.receive(&bytes, NOW).unwrap();
    // The old "unlocked" reply played again in place of the fresh one.
    assert_eq!(client.receive(&out), Err(RunnerError::Auth));
}

#[test]
fn reordered_frames_are_refused() {
    // Two sealed requests from a client that sends without waiting (forced by two
    // clients with the same entropy and keys is not possible; use a recorded session).
    let (mut client, mut server) = connect(62);
    let (id1, r1) = client.request(&Request::LockState, FAR).unwrap();
    // Hold r1 back; answer would be needed before r2, so record r2 from a twin channel.
    let (mut twin_c, _twin_s) = connect(62);
    let (_, t1) = twin_c.request(&Request::LockState, FAR).unwrap();
    assert_eq!(t1, r1);
    // The twin's second request after a reply is frame counter 1.
    let mut twin_s = connect(62).1;
    twin_s.receive(&t1, NOW).unwrap();
    let rep = twin_s
        .reply(id1, &Reply::LockState(LockState::Locked))
        .unwrap();
    twin_c.receive(&rep).unwrap();
    let (_, r2) = twin_c.request(&Request::FrontBundleId, FAR).unwrap();
    // Delivered out of order: counter 1 before counter 0.
    assert_eq!(server.receive(&r2, NOW), Err(RunnerError::Auth));
    // A dropped frame (skip counter 0) is the same failure.
    let (_, mut server) = connect(62);
    assert_eq!(server.receive(&r2, NOW), Err(RunnerError::Auth));
}

#[test]
fn a_frame_sent_back_in_the_other_direction_is_refused() {
    let (mut client, mut server) = connect(63);
    let (id, bytes) = client.request(&Request::LockState, FAR).unwrap();
    server.receive(&bytes, NOW).unwrap();
    // The client's own request reflected to it as if it were the reply (counter 0 both
    // ways): the direction keys differ.
    assert_eq!(client.receive(&bytes), Err(RunnerError::Auth));
    // And a server reply played to a server expecting the client's frame 0 (a twin
    // connection with the same keys): the direction keys differ.
    let out = server
        .reply(id, &Reply::LockState(LockState::Locked))
        .unwrap();
    let (_, mut twin_server) = connect(63);
    assert_eq!(twin_server.receive(&out, NOW), Err(RunnerError::Auth));
}

#[test]
fn a_recorded_session_replayed_into_a_new_connection_fails() {
    // Record a whole session: hello, finish, a request.
    let (mut client, mut server, c2s, _) = connect_recording(70);
    let (id, req) = client
        .request(
            &Request::Perform {
                command: RunnerCommand::Tap {
                    x: 10.0,
                    y: 20.0,
                    expect: None,
                },
            },
            FAR,
        )
        .unwrap();
    server.receive(&req, NOW).unwrap();
    let _ = id;
    let mut recording = c2s;
    recording.extend(req);
    // Played into a fresh server (fresh nonce): it answers the hello, but the finish fails,
    // so the tap is never delivered.
    let mut fresh = RunnerServerChannel::new(&KEY, &entropy(71)).unwrap();
    let result = fresh.receive(&recording, NOW);
    assert_eq!(result, Err(RunnerError::Auth));
    assert!(!fresh.is_open());
    // Byte by byte, no request event appears either.
    let mut fresh = RunnerServerChannel::new(&KEY, &entropy(72)).unwrap();
    for b in &recording {
        match fresh.receive(std::slice::from_ref(b), NOW) {
            Ok(events) => assert!(events.iter().all(|e| matches!(e, ServerEvent::Send(_)))),
            Err(_) => break,
        }
    }
    assert!(!fresh.is_open());
}

// ---------------------------------------------------------------------------------------
// Request/reply order.
// ---------------------------------------------------------------------------------------

#[test]
fn only_one_request_is_outstanding_and_replies_must_match() {
    let (mut client, mut server) = connect(80);
    assert_eq!(
        server.reply(1, &Reply::LockState(LockState::Locked)),
        Err(RunnerError::State("no request is outstanding"))
    );
    let (id, bytes) = client.request(&Request::LockState, FAR).unwrap();
    assert_eq!(
        client.request(&Request::LockState, FAR),
        Err(RunnerError::State("a request is outstanding"))
    );
    server.receive(&bytes, NOW).unwrap();
    assert_eq!(
        server.reply(id + 1, &Reply::LockState(LockState::Locked)),
        Err(RunnerError::State("not the outstanding request"))
    );
    assert_eq!(
        server.reply(id, &Reply::FrontBundleId(None)),
        Err(RunnerError::State("the reply answers another request"))
    );
    // Neither refusal burned a counter: the right reply still goes through.
    let out = server
        .reply(id, &Reply::LockState(LockState::Locked))
        .unwrap();
    assert_eq!(
        client.receive(&out).unwrap(),
        vec![ClientEvent::Reply {
            id,
            reply: Reply::LockState(LockState::Locked)
        }]
    );
    // Before the handshake ends nothing can be requested.
    let mut client = RunnerClientChannel::new(&KEY, &entropy(81)).unwrap();
    assert_eq!(
        client.request(&Request::LockState, FAR),
        Err(RunnerError::State("not authenticated"))
    );
    client.start().unwrap();
    assert_eq!(client.start(), Err(RunnerError::State("already started")));
}

#[test]
fn keys_and_entropy_are_checked() {
    assert_eq!(
        RunnerClientChannel::new(&[1u8; 31], &entropy(1)).unwrap_err(),
        RunnerError::Key
    );
    assert_eq!(
        RunnerServerChannel::new(&KEY, &[0u8; 32]).unwrap_err(),
        RunnerError::Entropy
    );
    assert_eq!(
        RunnerClientChannel::new(&KEY, &[7u8; 16]).unwrap_err(),
        RunnerError::Entropy
    );
}

// ---------------------------------------------------------------------------------------
// Random input.
// ---------------------------------------------------------------------------------------

/// Random bytes in random splits, fed to fresh and to authenticated channels of both
/// kinds: never a panic, work bounded by the input, and never an authenticated event or a
/// message from bytes made without the key.
#[test]
fn random_bytes_never_panic_and_never_yield_a_message() {
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let started = std::time::Instant::now();
    for round in 0..3_000u64 {
        let len = usize::try_from(rng.below(600)).unwrap();
        let mut bytes = rng.bytes(len);
        // Often start with a plausible length header and kind, to get past framing.
        if round % 3 == 0 && bytes.len() > 5 {
            let body = u32::try_from(bytes.len() - 4).unwrap().to_be_bytes();
            bytes[..4].copy_from_slice(&body);
            bytes[4] = [0x01, 0x02, 0x03, 0x10][usize::try_from(round % 4).unwrap()];
        }
        let mut splits = Vec::new();
        let mut rest = bytes.as_slice();
        while !rest.is_empty() {
            let n = usize::try_from(1 + rng.below(64)).unwrap().min(rest.len());
            splits.push(&rest[..n]);
            rest = &rest[n..];
        }

        let mut fresh_server = RunnerServerChannel::new(&KEY, &entropy(round + 1)).unwrap();
        let mut fresh_client = RunnerClientChannel::new(&KEY, &entropy(round + 2)).unwrap();
        fresh_client.start().unwrap();
        let (mut open_client, mut open_server) = connect(round + 3);
        let (_, _) = open_client.request(&Request::LockState, FAR).unwrap();
        for chunk in &splits {
            if let Ok(events) = fresh_server.receive(chunk, NOW) {
                assert!(events.iter().all(|e| matches!(e, ServerEvent::Send(_))));
            }
            if let Ok(events) = fresh_client.receive(chunk) {
                assert!(events.is_empty(), "{events:?}");
            }
            if let Ok(events) = open_server.receive(chunk, NOW) {
                assert!(events.is_empty(), "{events:?}");
            }
            if let Ok(events) = open_client.receive(chunk) {
                assert!(events.is_empty(), "{events:?}");
            }
        }
        assert!(!fresh_server.is_open() && !fresh_client.is_open());
        // Decoders on their own, too.
        let _ = decode_request(&bytes);
        let _ = decode_reply(&bytes);
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(60));
}

/// Random corruption of a real session's bytes (one bit somewhere): the receiving side
/// either fails or yields exactly what was sent.
#[test]
fn bit_flips_in_a_real_session_are_caught() {
    let mut rng = Rng(77);
    let reply = Reply::Snapshot(Box::new(snapshot(big_tree(30), Some(screenshot(100)))));
    for round in 0..300u64 {
        let (mut client, mut server) = connect(round + 500);
        let (id, req) = client
            .request(
                &Request::Snapshot {
                    include_screenshot: true,
                },
                FAR,
            )
            .unwrap();
        server.receive(&req, NOW).unwrap();
        let mut out = server.reply(id, &reply).unwrap();
        let i = usize::try_from(rng.below(out.len() as u64)).unwrap();
        out[i] ^= 1 << rng.below(8);
        match client.receive(&out) {
            Ok(events) => assert!(events.is_empty(), "a corrupted reply was accepted"),
            Err(_) => assert!(!client.is_open()),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Expiry (protocol version 2): `not_after_ms` under the AEAD, checked by the server.
// ---------------------------------------------------------------------------------------

fn tap_request() -> Request {
    Request::Perform {
        command: RunnerCommand::Tap {
            x: 3.0,
            y: 4.0,
            expect: Some(Expect {
                bundle_id: "com.apple.Preferences".into(),
            }),
        },
    }
}

#[test]
fn an_expired_request_is_refused_and_never_surfaced() {
    let (mut client, mut server) = connect(301);
    let (_, bytes) = client.request(&tap_request(), NOW - 1).unwrap();
    assert_eq!(server.receive(&bytes, NOW), Err(RunnerError::Expired));
    assert!(!server.is_open() && !server.has_pending_request());
    // The channel stays closed: nothing later is acted on either.
    assert_eq!(server.receive(&bytes, NOW), Err(RunnerError::Closed));
}

#[test]
fn the_expiry_boundary_is_inclusive() {
    // Received exactly at not_after: still accepted.
    let (mut client, mut server) = connect(302);
    let (id, bytes) = client.request(&tap_request(), NOW).unwrap();
    assert_eq!(
        server.receive(&bytes, NOW).unwrap(),
        vec![ServerEvent::Request {
            id,
            not_after_ms: NOW,
            request: tap_request()
        }]
    );
    // One millisecond later: refused.
    let (mut client, mut server) = connect(303);
    let (_, bytes) = client.request(&tap_request(), NOW).unwrap();
    assert_eq!(server.receive(&bytes, NOW + 1), Err(RunnerError::Expired));
}

#[test]
fn a_clock_that_goes_backwards_cannot_revive_an_expired_request() {
    let (mut client, mut server) = connect(304);
    // An earlier request was received late in the clock's life ...
    let (id, bytes) = client.request(&Request::LockState, NOW + 10_000).unwrap();
    assert_eq!(server.receive(&bytes, NOW + 5_000).unwrap().len(), 1);
    let out = server
        .reply(id, &Reply::LockState(LockState::Unlocked))
        .unwrap();
    client.receive(&out).unwrap();
    // ... then the shell's reading jumps back to before this request's expiry: the
    // server keeps its latest reading, so the request is still expired.
    let (_, bytes) = client.request(&tap_request(), NOW + 1_000).unwrap();
    assert_eq!(server.receive(&bytes, NOW), Err(RunnerError::Expired));
}

#[test]
fn a_perform_held_back_and_delivered_late_is_refused() {
    let (mut client, mut server) = connect(305);
    // The client gives the tap 2 s (its own deadline); a relay holds the bytes for 3 s.
    let sent_at = NOW;
    let (_, bytes) = client.request(&tap_request(), sent_at + 2_000).unwrap();
    // Delivered byte by byte after the hold: no event ever, and the channel closes.
    let mut events = Vec::new();
    let mut result = Ok(());
    for b in &bytes {
        match server.receive(std::slice::from_ref(b), sent_at + 3_000) {
            Ok(e) => events.extend(e),
            Err(e) => {
                result = Err(e);
                break;
            }
        }
    }
    assert!(
        events.is_empty(),
        "a held-back perform was surfaced: {events:?}"
    );
    assert_eq!(result, Err(RunnerError::Expired));
    assert!(!server.is_open());
}

#[test]
fn the_expiry_is_authenticated() {
    // Changing any byte after the frame header (counter, sealed not_after, tag) fails.
    let (mut client, _) = connect(306);
    let (_, bytes) = client.request(&tap_request(), NOW - 1).unwrap();
    for i in 4 + 1 + 8..bytes.len() {
        let (_, mut server) = connect(306);
        let mut t = bytes.clone();
        t[i] ^= 0x80;
        assert_eq!(server.receive(&t, NOW), Err(RunnerError::Auth), "byte {i}");
    }
}

#[test]
fn the_expiry_is_in_the_request_codec() {
    let bytes = encode_request(9, 0x0102_0304_0506_0708, &Request::LockState).unwrap();
    assert_eq!(bytes.len(), 1 + 8 + 8);
    assert_eq!(&bytes[9..17], &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        decode_request(&bytes),
        Ok((9, 0x0102_0304_0506_0708, Request::LockState))
    );
    // A version-1 request (no expiry) does not decode as one.
    assert!(decode_request(&bytes[..9]).is_err());
}
