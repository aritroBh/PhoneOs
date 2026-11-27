//! An attacker on the network: tampering, replay, reordering, injection, guessing,
//! garbage and floods. Every case must end without the attacker's bytes being acted on.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_remote::{
    codec::{encode_record, Kind},
    AuditEvent, ClientCredentials, ClientError, ClientEvent, ClientMessage, CloseReason, ErrorCode,
    HubEffect, RemoteClient, RemoteHub, ServerMessage, LOCKOUT_BASE_MS, LOCKOUT_THRESHOLD,
    MAX_RECORD_LEN, PAIRING_TTL_MS, RATE_MAX_MESSAGES, RATE_WINDOW_MS,
};

/// A hub restored from a store with one paired device, plus that device's credentials.
fn paired_store() -> (String, ClientCredentials) {
    let mut rig = Rig::new();
    let (_, creds) = rig.pair("Mac");
    (rig.hub.to_json().to_string(), creds)
}

/// A hub restored from `json`, with test entropy advanced by `skip` so that two rigs
/// restored from the same snapshot do not hand out identical randomness (the shell's
/// CSPRNG would not).
fn fresh(json: &str, skip: u64) -> Rig {
    let mut rig = Rig::with_hub(RemoteHub::from_json("Test Phone", json).unwrap());
    for _ in 0..skip {
        rig.seed();
    }
    rig
}

/// Nothing the phone would act on and no reply.
fn inert(fx: &[HubEffect], conn: u64) {
    assert!(actions(fx).is_empty(), "{fx:?}");
    assert!(sends(fx, conn).is_empty(), "{fx:?}");
}

fn wrong_code() -> String {
    jarvis_remote::format_code(&jarvis_remote::PairingSecret::from_bytes([0x5a; 20])).to_string()
}

/// One deterministic connection, recorded: everything here is a pure function of the
/// snapshot, the entropy and the clock, so each flip below replays the recorded side
/// instead of redoing its X25519 work (debug-build scalar multiplication is slow).
struct Recording {
    json: String,
    creds: ClientCredentials,
    hello: Vec<u8>,
    request: Vec<u8>,
    accept: Vec<u8>,
    frame: Vec<u8>,
    reply: Vec<u8>,
}

const HUB_SEED: u64 = 501;
const CLIENT_SEED: u64 = 500;
const CONN: u64 = 77;

fn record() -> Recording {
    let (json, creds) = paired_store();
    let mut hub = RemoteHub::from_json("Test Phone", &json).unwrap();
    let mut client = RemoteClient::connect(creds.clone(), &entropy(CLIENT_SEED)).unwrap();
    let hello = sends(&hub.connection_opened(CONN, 1, &entropy(HUB_SEED)), CONN);
    let ev = client.receive(&hello).unwrap();
    let ClientEvent::Send { bytes: request } = ev[0].clone() else {
        panic!()
    };
    let accept = sends(&hub.receive(CONN, &request, 1), CONN);
    assert!(client
        .receive(&accept)
        .unwrap()
        .contains(&ClientEvent::Connected));
    let frame = client.send(&ClientMessage::Ping).unwrap();
    let reply = sends(&hub.receive(CONN, &frame, 1), CONN);
    assert!(!reply.is_empty());
    Recording {
        json,
        creds,
        hello,
        request,
        accept,
        frame,
        reply,
    }
}

/// A hub in the recorded state just after `ConnectAccept` (no client work needed).
fn recorded_hub(r: &Recording) -> RemoteHub {
    let mut hub = RemoteHub::from_json("Test Phone", &r.json).unwrap();
    assert_eq!(
        sends(&hub.connection_opened(CONN, 1, &entropy(HUB_SEED)), CONN),
        r.hello
    );
    assert_eq!(sends(&hub.receive(CONN, &r.request, 1), CONN), r.accept);
    hub
}

/// Feeds a flipped record; a longer length prefix is completed with zeros. Returns all
/// effects.
fn feed_flipped(hub: &mut RemoteHub, bytes: &[u8]) -> Vec<HubEffect> {
    let mut fx = hub.receive(CONN, bytes, 1);
    if closed(&fx, CONN).is_none() {
        fx.extend(hub.receive(CONN, &vec![0u8; MAX_RECORD_LEN], 1));
    }
    fx
}

#[test]
fn every_single_bit_flip_of_a_data_frame_is_rejected() {
    let r = record();
    // Sanity: the unflipped frame is accepted by a recorded hub.
    let mut hub = recorded_hub(&r);
    assert_eq!(sends(&hub.receive(CONN, &r.frame, 1), CONN), r.reply);
    for bit in 0..r.frame.len() * 8 {
        let mut hub = recorded_hub(&r);
        let mut frame = r.frame.clone();
        frame[bit / 8] ^= 1 << (bit % 8);
        let fx = feed_flipped(&mut hub, &frame);
        inert(&fx, CONN);
        assert!(closed(&fx, CONN).is_some(), "bit {bit}: {fx:?}");
        assert!(!hub.is_authenticated(CONN));
    }
}

#[test]
fn every_single_bit_flip_of_a_handshake_is_rejected() {
    let r = record();
    for bit in 0..r.request.len() * 8 {
        let mut hub = RemoteHub::from_json("Test Phone", &r.json).unwrap();
        hub.connection_opened(CONN, 1, &entropy(HUB_SEED));
        let mut bytes = r.request.clone();
        bytes[bit / 8] ^= 1 << (bit % 8);
        let fx = feed_flipped(&mut hub, &bytes);
        assert!(closed(&fx, CONN).is_some(), "bit {bit}");
        assert!(!hub.is_authenticated(CONN));
        assert!(sends(&fx, CONN).is_empty(), "bit {bit}");
    }
}

#[test]
fn every_single_bit_flip_of_the_server_hello_breaks_the_handshake() {
    let r = record();
    for bit in 0..r.hello.len() * 8 {
        let mut client = RemoteClient::connect(r.creds.clone(), &entropy(CLIENT_SEED)).unwrap();
        let mut hello = r.hello.clone();
        hello[bit / 8] ^= 1 << (bit % 8);
        // The client refuses the hello, or sends a request the phone refuses.
        if let Ok(ev) = client.receive(&hello) {
            for e in ev {
                if let ClientEvent::Send { bytes } = e {
                    let mut hub = RemoteHub::from_json("Test Phone", &r.json).unwrap();
                    hub.connection_opened(CONN, 1, &entropy(HUB_SEED));
                    let fx = hub.receive(CONN, &bytes, 1);
                    assert!(!hub.is_authenticated(CONN), "bit {bit}: {fx:?}");
                }
            }
        }
        assert!(!client.is_connected(), "bit {bit}");
    }
}

#[test]
fn every_single_bit_flip_of_a_phone_frame_is_rejected_by_the_client() {
    let r = record();
    for bit in 0..r.reply.len() * 8 {
        let mut client = RemoteClient::connect(r.creds.clone(), &entropy(CLIENT_SEED)).unwrap();
        client.receive(&r.hello).unwrap();
        client.receive(&r.accept).unwrap();
        assert_eq!(client.send(&ClientMessage::Ping).unwrap(), r.frame);
        let mut reply = r.reply.clone();
        reply[bit / 8] ^= 1 << (bit % 8);
        match client.receive(&reply) {
            Err(ClientError::Auth) => {}
            Ok(ev) => {
                assert!(bit / 8 < 4 && ev.is_empty(), "bit {bit}: {ev:?}");
                assert!(client.receive(&vec![0u8; MAX_RECORD_LEN]).is_err());
            }
            Err(e) => panic!("bit {bit}: {e:?}"),
        }
        assert!(client.send(&ClientMessage::Ping).is_err());
    }
}

#[test]
fn replayed_frame_is_rejected() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let frame = link
        .client
        .send(&ClientMessage::StartTask {
            text: "once".into(),
        })
        .unwrap();
    let (fx, _) = rig.feed(&mut link, &frame);
    assert_eq!(actions(&fx).len(), 1);
    let fx = rig.local(finished(
        task_id(&fx),
        jarvis_proto::OutcomeStatus::Success,
        "",
    ));
    deliver(&mut link, &fx);
    let fx = rig.hub.receive(link.conn, &frame, rig.now);
    inert(&fx, link.conn);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    assert!(audits(&fx).contains(&AuditEvent::Closed {
        reason: CloseReason::AuthFailed
    }));
}

#[test]
fn reordered_frames_are_rejected() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let first = link.client.send(&ClientMessage::Ping).unwrap();
    let second = link
        .client
        .send(&ClientMessage::StartTask { text: "x".into() })
        .unwrap();
    let fx = rig.hub.receive(link.conn, &second, rig.now);
    inert(&fx, link.conn);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    assert!(rig.hub.receive(link.conn, &first, rig.now).is_empty());
}

#[test]
fn dropped_frame_is_detected_as_a_gap() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let _lost = link.client.send(&ClientMessage::Ping).unwrap();
    let next = link.client.send(&ClientMessage::Ping).unwrap();
    let fx = rig.hub.receive(link.conn, &next, rig.now);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
}

#[test]
fn frame_from_one_connection_is_rejected_on_another() {
    let mut rig = Rig::new();
    let (mut a, creds) = rig.pair("Mac");
    let b = rig.connect(&creds);
    let (mut c_link, _) = rig.pair("Other");
    let frame = a
        .client
        .send(&ClientMessage::StartTask { text: "x".into() })
        .unwrap();
    let fx = rig.hub.receive(b.conn, &frame, rig.now);
    inert(&fx, b.conn);
    assert_eq!(closed(&fx, b.conn), Some(CloseReason::AuthFailed));
    let frame = c_link.client.send(&ClientMessage::Ping).unwrap();
    let (_, msgs) = rig.feed(&mut a, &frame);
    assert!(msgs.is_empty());
    assert!(!rig.hub.is_authenticated(a.conn));
    assert!(rig.hub.is_authenticated(c_link.conn));
}

#[test]
fn recorded_session_cannot_be_replayed_into_a_fresh_connection() {
    let (json, creds) = paired_store();
    let mut rig = fresh(&json, 0);
    let mut link = rig.connect(&creds);
    rig.request(
        &mut link,
        ClientMessage::StartTask {
            text: "transfer".into(),
        },
    );
    rig.request(&mut link, ClientMessage::Ping);
    let recorded: Vec<u8> = link.sent.concat();

    // Replayed whole into a new connection: the handshake fails.
    let mut rig = fresh(&json, 10);
    let conn = 500;
    rig.hub.connection_opened(conn, rig.now, &entropy(9_999));
    let fx = rig.hub.receive(conn, &recorded, rig.now);
    inert(&fx, conn);
    assert_eq!(closed(&fx, conn), Some(CloseReason::AuthFailed));

    // Data frames replayed after a genuine fresh handshake: different session keys.
    let mut rig = fresh(&json, 10);
    let fresh_link = rig.connect(&creds);
    let data: Vec<u8> = link.sent[1..].concat();
    let fx = rig.hub.receive(fresh_link.conn, &data, rig.now);
    inert(&fx, fresh_link.conn);
    assert_eq!(closed(&fx, fresh_link.conn), Some(CloseReason::AuthFailed));
    assert!(!rig.hub.is_busy());
}

fn try_pair(
    rig: &mut Rig,
    code: &str,
) -> (Vec<HubEffect>, Result<Vec<ClientEvent>, ClientError>, u64) {
    let client = RemoteClient::pair(code, "Attacker", &entropy(rig.now ^ 0xabcdef)).unwrap();
    let (link, fx, events) = rig.open(client);
    (fx, events, link.conn)
}

fn paired(fx: &[HubEffect]) -> bool {
    fx.iter()
        .any(|e| matches!(e, HubEffect::DevicePaired { .. }))
}

#[test]
fn wrong_pairing_code_fails_opaquely() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(1)).unwrap();
    let wrong = wrong_code();
    assert_ne!(wrong, offer.code.as_str());
    let (fx, events, conn) = try_pair(&mut rig, &wrong);
    assert!(!paired(&fx));
    assert_eq!(closed(&fx, conn), Some(CloseReason::AuthFailed));
    assert!(audits(&fx).contains(&AuditEvent::HandshakeFailed { failures: 1 }));
    assert!(!events.unwrap().contains(&ClientEvent::Connected));
    assert!(rig.hub.devices().is_empty());
    // The offer survives a wrong guess (160 bits cannot be guessed) and still works.
    assert!(rig.hub.pairing_open());
    let (fx, _, _) = try_pair(&mut rig, &offer.code);
    assert!(paired(&fx));
    // Malformed codes and names are refused locally.
    assert!(RemoteClient::pair("1234-56", "x", &entropy(3)).is_err());
    assert!(matches!(
        RemoteClient::pair(&offer.code, " \u{200B} ", &entropy(3)),
        Err(ClientError::InvalidName)
    ));
}

#[test]
fn expired_code_is_rejected() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(1)).unwrap();
    rig.now += PAIRING_TTL_MS;
    let (fx, _, conn) = try_pair(&mut rig, &offer.code);
    assert!(!paired(&fx));
    assert!(audits(&fx).contains(&AuditEvent::PairingExpired));
    assert_eq!(closed(&fx, conn), Some(CloseReason::AuthFailed));
    assert!(!rig.hub.pairing_open());
}

#[test]
fn code_expiring_between_hello_and_request_is_rejected() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(1)).unwrap();
    let mut client = RemoteClient::pair(&offer.code, "Mac", &entropy(2)).unwrap();
    let fx = rig.hub.connection_opened(5, rig.now, &entropy(3));
    let ev = client.receive(&sends(&fx, 5)).unwrap();
    let ClientEvent::Send { bytes } = &ev[0] else {
        panic!()
    };
    let fx = rig.hub.receive(5, bytes, rig.now + PAIRING_TTL_MS);
    assert!(!paired(&fx));
    assert!(closed(&fx, 5).is_some());
}

#[test]
fn reused_and_cancelled_codes_are_rejected() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(1)).unwrap();
    let (fx, _, _) = try_pair(&mut rig, &offer.code);
    assert!(paired(&fx));
    let (fx, _, conn) = try_pair(&mut rig, &offer.code);
    assert!(!paired(&fx));
    assert_eq!(closed(&fx, conn), Some(CloseReason::AuthFailed));
    // The one device paired with this code waits for the owner's confirmation.
    assert_eq!(rig.hub.devices().len() + rig.hub.pending_devices().len(), 1);
    assert_eq!(rig.hub.pending_devices().len(), 1);

    let offer = rig.hub.open_pairing(rig.now, &entropy(2)).unwrap();
    rig.hub.cancel_pairing();
    let (fx, _, _) = try_pair(&mut rig, &offer.code);
    assert!(!paired(&fx));
    let old = rig.hub.open_pairing(rig.now, &entropy(3)).unwrap();
    let new = rig.hub.open_pairing(rig.now, &entropy(4)).unwrap();
    let (fx, _, _) = try_pair(&mut rig, &old.code);
    assert!(!paired(&fx));
    let (fx, _, _) = try_pair(&mut rig, &new.code);
    assert!(paired(&fx));
}

#[test]
fn pairing_lockout_after_failed_macs_and_recovery_after_backoff() {
    let mut rig = Rig::new();
    let (mut owner, owner_creds) = rig.pair("Owner");
    let offer = rig.hub.open_pairing(rig.now, &entropy(1)).unwrap();
    let wrong = wrong_code();
    for _ in 0..LOCKOUT_THRESHOLD {
        let _ = try_pair(&mut rig, &wrong);
        rig.now += 10;
    }
    let until = rig.hub.pairing_locked_until_ms();
    assert_eq!(until, rig.now - 10 + LOCKOUT_BASE_MS);
    // During the lockout even the right code is refused, without being checked.
    let (fx, _, conn) = try_pair(&mut rig, &offer.code);
    assert!(!paired(&fx));
    assert_eq!(closed(&fx, conn), Some(CloseReason::LockedOut));
    // Further attempts during the lockout do not extend it.
    let _ = try_pair(&mut rig, &wrong);
    assert_eq!(rig.hub.pairing_locked_until_ms(), until);
    // A paired device is not affected by the pairing lockout.
    rig.connect(&owner_creds);
    let (_, msgs) = rig.request(&mut owner, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);

    // One more failure after the backoff doubles it.
    rig.now = until;
    let _ = try_pair(&mut rig, &wrong);
    assert_eq!(
        rig.hub.pairing_locked_until_ms(),
        rig.now + 2 * LOCKOUT_BASE_MS
    );
    // After the backoff the right code works, and success resets the counter.
    rig.now = rig.hub.pairing_locked_until_ms();
    let (fx, _, _) = try_pair(&mut rig, &offer.code);
    assert!(paired(&fx));
    let offer = rig.hub.open_pairing(rig.now, &entropy(77)).unwrap();
    for _ in 0..LOCKOUT_THRESHOLD - 1 {
        let _ = try_pair(&mut rig, &wrong);
    }
    let (fx, _, _) = try_pair(&mut rig, &offer.code);
    assert!(paired(&fx), "4 failures after a reset do not lock");
}

#[test]
fn connect_lockout_is_per_device() {
    let mut rig = Rig::new();
    let (_, victim) = rig.pair("Victim");
    let (_, bystander) = rig.pair("Bystander");
    let forged = ClientCredentials {
        device_id: victim.device_id,
        device_key: jarvis_remote::Secret32::from_bytes([9; 32]),
    };
    for _ in 0..LOCKOUT_THRESHOLD {
        let (link, fx, _) = rig.try_connect(&forged);
        assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    }
    let until = rig.hub.device_locked_until_ms(victim.device_id);
    assert!(until > rig.now);
    // During the lockout a forged request is refused as locked out (and not counted)...
    let (link, fx, _) = rig.try_connect(&forged);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::LockedOut));
    assert_eq!(rig.hub.device_locked_until_ms(victim.device_id), until);
    // ...but the device's own key is never refused because of someone else's failures
    // (confirm-step break-it review F1; this used to assert `LockedOut` for it).
    let (link, _, _) = rig.try_connect(&victim);
    assert!(rig.hub.is_authenticated(link.conn));
    rig.hub.connection_closed(link.conn);
    // Other devices, and unknown ids, are not affected or counted.
    rig.connect(&bystander);
    let unknown = ClientCredentials {
        device_id: jarvis_remote::DeviceId([0xee; 16]),
        device_key: jarvis_remote::Secret32::from_bytes([9; 32]),
    };
    for _ in 0..20 {
        let _ = rig.try_connect(&unknown);
    }
    assert_eq!(rig.hub.device_locked_until_ms(unknown.device_id), 0);
    rig.now = until;
    rig.connect(&victim);
}

#[test]
fn oversized_frame_is_rejected_before_buffering() {
    let mut rig = Rig::new();
    let (link, _) = rig.pair("Mac");
    let len = u32::try_from(MAX_RECORD_LEN + 1).unwrap();
    let fx = rig.hub.receive(link.conn, &len.to_be_bytes(), rig.now);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::TooLarge));
    rig.hub.connection_opened(50, rig.now, &entropy(50));
    let fx = rig.hub.receive(50, &u32::MAX.to_be_bytes(), rig.now);
    assert_eq!(closed(&fx, 50), Some(CloseReason::TooLarge));
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let huge = ClientMessage::StartTask {
        text: "a".repeat(jarvis_remote::MAX_PLAINTEXT_LEN),
    };
    assert!(link.client.send(&huge).is_err());
}

#[test]
fn truncated_frame_is_never_accepted() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let frame = link
        .client
        .send(&ClientMessage::StartTask { text: "x".into() })
        .unwrap();
    let cut = &frame[..frame.len() - 1];
    let fx = rig.hub.receive(link.conn, cut, rig.now);
    assert!(fx.is_empty(), "incomplete record waits: {fx:?}");
    let next = link.client.send(&ClientMessage::Ping).unwrap();
    let fx = rig.hub.receive(link.conn, &next, rig.now);
    inert(&fx, link.conn);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let mut frame = link.client.send(&ClientMessage::Ping).unwrap();
    frame.truncate(frame.len() - 5);
    let new_len = u32::try_from(frame.len() - 4).unwrap();
    frame[..4].copy_from_slice(&new_len.to_be_bytes());
    let fx = rig.hub.receive(link.conn, &frame, rig.now);
    inert(&fx, link.conn);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    rig.hub.connection_opened(60, rig.now, &entropy(60));
    assert!(rig
        .hub
        .receive(60, &[0, 0, 0, 90, 2, 4, 7], rig.now)
        .is_empty());
    rig.now += jarvis_remote::HANDSHAKE_TIMEOUT_MS;
    let fx = rig.hub.tick(rig.now);
    assert_eq!(closed(&fx, 60), Some(CloseReason::HandshakeTimeout));
}

#[test]
fn garbage_and_version_1_records_close_the_connection() {
    let mut rig = Rig::new();
    let (link, _) = rig.pair("Mac");
    let garbage = b"GET / HTTP/1.1\r\nHost: phone\r\n\r\n";
    let fx = rig.hub.receive(link.conn, garbage, rig.now);
    inert(&fx, link.conn);
    assert!(closed(&fx, link.conn).is_some());
    rig.hub.connection_opened(70, rig.now, &entropy(70));
    let fx = rig.hub.receive(70, garbage, rig.now);
    inert(&fx, 70);
    assert!(closed(&fx, 70).is_some());
    rig.hub.connection_opened(71, rig.now, &entropy(71));
    let fx = rig
        .hub
        .receive(71, &encode_record(Kind::Data, &[0; 40]), rig.now);
    assert_eq!(closed(&fx, 71), Some(CloseReason::AuthFailed));
    let (link, _) = rig.pair("Mac 2");
    let fx = rig.hub.receive(
        link.conn,
        &encode_record(Kind::PairRequest, &[0; 80]),
        rig.now,
    );
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    // A version-1 record (no downgrade).
    rig.hub.connection_opened(72, rig.now, &entropy(72));
    let mut v1 = encode_record(Kind::ConnectRequest, &[0; 112]);
    v1[4] = 1;
    let fx = rig.hub.receive(72, &v1, rig.now);
    assert_eq!(closed(&fx, 72), Some(CloseReason::AuthFailed));
}

#[test]
fn unknown_and_malformed_messages_get_typed_errors_and_keep_the_connection() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let cases: [(&[u8], ErrorCode); 5] = [
        (br#"{"type":"format_disk"}"#, ErrorCode::UnknownMessage),
        (br#"{"type":"start_task"}"#, ErrorCode::Malformed),
        (br#"{"type":"ping","admin":true}"#, ErrorCode::Malformed),
        (
            br#"{"type":"run_skill","id":"a","args":{"n":5}}"#,
            ErrorCode::Malformed,
        ),
        (b"\xff\xfe not json", ErrorCode::Malformed),
    ];
    for (raw, code) in cases {
        let bytes = link.client.send_raw(raw).unwrap();
        let (fx, msgs) = rig.feed(&mut link, &bytes);
        assert_eq!(msgs, vec![ServerMessage::Error { code }]);
        assert!(actions(&fx).is_empty());
        assert!(audits(&fx)
            .iter()
            .any(|a| matches!(a, AuditEvent::Rejected { code: c, .. } if *c == code)));
    }
    let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
}

#[test]
fn flood_is_rate_limited_per_connection() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    rig.now += RATE_WINDOW_MS;
    let mut burst = Vec::new();
    let n = RATE_MAX_MESSAGES as usize + 10;
    for _ in 0..n {
        burst.extend(link.client.send(&ClientMessage::Ping).unwrap());
    }
    let (fx, msgs) = rig.feed(&mut link, &burst);
    let pongs = msgs.iter().filter(|m| **m == ServerMessage::Pong).count();
    assert_eq!(pongs, RATE_MAX_MESSAGES as usize);
    assert_eq!(
        msgs.iter()
            .filter(|m| **m
                == ServerMessage::Error {
                    code: ErrorCode::RateLimited
                })
            .count(),
        1
    );
    assert_eq!(
        audits(&fx)
            .iter()
            .filter(|a| matches!(
                a,
                AuditEvent::Rejected {
                    code: ErrorCode::RateLimited,
                    ..
                }
            ))
            .count(),
        1
    );
    rig.now += RATE_WINDOW_MS;
    let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
}

#[test]
fn fake_phone_cannot_complete_pairing_or_connect() {
    let hello = encode_record(Kind::ServerHello, &entropy(7).repeat(2));
    let mut client = RemoteClient::pair(&wrong_code(), "Mac", &entropy(1)).unwrap();
    let ev = client.receive(&hello).unwrap();
    assert_eq!(ev.len(), 1);
    let forged = encode_record(Kind::PairAccept, &[0x41; 48]);
    assert_eq!(client.receive(&forged), Err(ClientError::Auth));
    assert!(!client.is_connected());
    assert_eq!(
        client.send(&ClientMessage::Ping),
        Err(ClientError::NotConnected)
    );

    let (_, creds) = paired_store();
    let mut client = RemoteClient::connect(creds.clone(), &entropy(2)).unwrap();
    client.receive(&hello).unwrap();
    assert_eq!(
        client.receive(&encode_record(Kind::ConnectAccept, &[0; 32])),
        Err(ClientError::Auth)
    );
    let mut client = RemoteClient::connect(creds.clone(), &entropy(3)).unwrap();
    assert_eq!(
        client.receive(&encode_record(Kind::Data, &[0; 30])),
        Err(ClientError::Auth)
    );
    // A low-order phone key is refused by the client before it sends anything.
    let mut client = RemoteClient::connect(creds, &entropy(4)).unwrap();
    let mut bad = entropy(7).to_vec();
    bad.extend([0u8; 32]);
    assert_eq!(
        client.receive(&encode_record(Kind::ServerHello, &bad)),
        Err(ClientError::Auth)
    );
}

#[test]
fn unknown_device_and_wrong_key_fail_like_a_wrong_code() {
    let (json, creds) = paired_store();
    let mut rig = fresh(&json, 0);
    let impostor = ClientCredentials {
        device_id: jarvis_remote::DeviceId([0xee; 16]),
        device_key: creds.device_key.clone(),
    };
    let (link, fx, _) = rig.open(RemoteClient::connect(impostor, &entropy(4)).unwrap());
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    let wrong_key = ClientCredentials {
        device_id: creds.device_id,
        device_key: jarvis_remote::Secret32::from_bytes([0; 32]),
    };
    let (link, fx, _) = rig.open(RemoteClient::connect(wrong_key, &entropy(5)).unwrap());
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
}
