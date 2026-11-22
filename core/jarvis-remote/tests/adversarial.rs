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
    HubEffect, LocalEvent, RemoteClient, RemoteHub, ServerMessage, LOCKOUT_BASE_MS,
    LOCKOUT_THRESHOLD, MAX_RECORD_LEN, PAIRING_TTL_MS, RATE_MAX_MESSAGES, RATE_WINDOW_MS,
};

/// A hub restored from a store with one paired device, plus that device's credentials.
fn paired_store() -> (String, ClientCredentials) {
    let mut rig = Rig::new();
    let (_, creds) = rig.pair("Mac");
    (rig.hub.to_json(), creds)
}

fn fresh(json: &str) -> Rig {
    Rig::with_hub(RemoteHub::from_json("Test Phone", json).unwrap())
}

/// Nothing the phone would act on and no reply.
fn inert(fx: &[HubEffect], conn: u64) {
    assert!(actions(fx).is_empty(), "{fx:?}");
    assert!(sends(fx, conn).is_empty(), "{fx:?}");
}

#[test]
fn every_single_bit_flip_of_a_data_frame_is_rejected() {
    let (json, creds) = paired_store();
    // Learn the frame once to know its length.
    let frame_len = {
        let mut rig = fresh(&json);
        let mut link = rig.connect(&creds);
        link.client
            .send(&ClientMessage::StartTask { text: "x".into() })
            .unwrap()
            .len()
    };
    for bit in 0..frame_len * 8 {
        let mut rig = fresh(&json);
        let mut link = rig.connect(&creds);
        let mut frame = link
            .client
            .send(&ClientMessage::StartTask { text: "x".into() })
            .unwrap();
        frame[bit / 8] ^= 1 << (bit % 8);
        let fx = rig.hub.receive(link.conn, &frame, rig.now);
        inert(&fx, link.conn);
        if bit / 8 >= 4 || closed(&fx, link.conn).is_some() {
            // Anything past the length prefix (and any length that is complete) closes.
            assert!(closed(&fx, link.conn).is_some(), "bit {bit}: {fx:?}");
            assert!(!rig.hub.is_authenticated(link.conn));
        } else {
            // A longer length prefix: the hub waits for bytes that will never authenticate.
            let pad = vec![0u8; MAX_RECORD_LEN];
            let fx = rig.hub.receive(link.conn, &pad, rig.now);
            inert(&fx, link.conn);
            assert!(closed(&fx, link.conn).is_some(), "bit {bit}: {fx:?}");
        }
        assert!(!rig.hub.is_busy());
    }
}

#[test]
fn every_single_bit_flip_of_a_handshake_is_rejected() {
    let (json, creds) = paired_store();
    let request = {
        let mut rig = fresh(&json);
        rig.connect(&creds).sent[0].clone()
    };
    for bit in 0..request.len() * 8 {
        let mut rig = fresh(&json);
        let mut client = RemoteClient::connect(creds.clone(), entropy(500));
        let conn = 77;
        let fx = rig.hub.connection_opened(conn, rig.now, entropy(501));
        let ev = client.receive(&sends(&fx, conn)).unwrap();
        let ClientEvent::Send { bytes } = &ev[0] else {
            panic!()
        };
        assert_eq!(bytes.len(), request.len());
        let mut bytes = bytes.clone();
        bytes[bit / 8] ^= 1 << (bit % 8);
        let mut fx = rig.hub.receive(conn, &bytes, rig.now);
        if bit / 8 < 4 && closed(&fx, conn).is_none() {
            fx = rig.hub.receive(conn, &vec![0u8; MAX_RECORD_LEN], rig.now);
        }
        assert!(closed(&fx, conn).is_some(), "bit {bit}");
        assert!(!rig.hub.is_authenticated(conn));
        assert!(!fx.iter().any(|e| matches!(
            e,
            HubEffect::Send { conn_id, .. } if *conn_id == conn
        )));
    }
}

#[test]
fn every_single_bit_flip_of_a_phone_frame_is_rejected_by_the_client() {
    let (json, creds) = paired_store();
    let mut rig = fresh(&json);
    let mut link = rig.connect(&creds);
    let bytes = link.client.send(&ClientMessage::Hello).unwrap();
    let fx = rig.hub.receive(link.conn, &bytes, rig.now);
    let reply = sends(&fx, link.conn);
    for bit in 0..reply.len() * 8 {
        let mut rig = fresh(&json);
        let mut link = rig.connect(&creds);
        let bytes = link.client.send(&ClientMessage::Hello).unwrap();
        let fx = rig.hub.receive(link.conn, &bytes, rig.now);
        let mut reply = sends(&fx, link.conn);
        reply[bit / 8] ^= 1 << (bit % 8);
        match link.client.receive(&reply) {
            Err(ClientError::Auth) => {}
            // A longer length prefix: still waiting, nothing delivered.
            Ok(ev) => {
                assert!(bit / 8 < 4 && ev.is_empty(), "bit {bit}: {ev:?}");
                assert!(link.client.receive(&vec![0u8; MAX_RECORD_LEN]).is_err());
            }
            Err(e) => panic!("bit {bit}: {e:?}"),
        }
        assert!(link.client.send(&ClientMessage::Ping).is_err());
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
    rig.local(LocalEvent::SessionFinished {
        outcome: jarvis_proto::Outcome {
            status: jarvis_proto::OutcomeStatus::Success,
            summary: String::new(),
            steps: 1,
            duration_ms: 1,
        },
    });
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
    // And the connection is gone: the in-order frame is not accepted afterwards.
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
    // Same device, same counter position, different connection keys.
    let frame = a
        .client
        .send(&ClientMessage::StartTask { text: "x".into() })
        .unwrap();
    let fx = rig.hub.receive(b.conn, &frame, rig.now);
    inert(&fx, b.conn);
    assert_eq!(closed(&fx, b.conn), Some(CloseReason::AuthFailed));
    // Different device.
    let frame = c_link.client.send(&ClientMessage::Ping).unwrap();
    let (_, msgs) = rig.feed(&mut a, &frame);
    assert!(msgs.is_empty());
    assert!(!rig.hub.is_authenticated(a.conn));
    // The victim connections are unaffected by the stolen frame being spent elsewhere.
    assert!(rig.hub.is_authenticated(c_link.conn));
}

#[test]
fn recorded_session_cannot_be_replayed_into_a_fresh_connection() {
    let (json, creds) = paired_store();
    let mut rig = fresh(&json);
    let mut link = rig.connect(&creds);
    rig.request(
        &mut link,
        ClientMessage::StartTask {
            text: "transfer".into(),
        },
    );
    rig.request(&mut link, ClientMessage::Ping);
    let recorded: Vec<u8> = link.sent.concat();

    // Replayed whole into a new connection (new phone nonce): the handshake fails.
    let mut rig = fresh(&json);
    let conn = 500;
    rig.hub.connection_opened(conn, rig.now, entropy(9_999));
    let fx = rig.hub.receive(conn, &recorded, rig.now);
    inert(&fx, conn);
    assert_eq!(closed(&fx, conn), Some(CloseReason::AuthFailed));

    // Data frames replayed after a genuine fresh handshake: different session keys.
    // (The test rig's entropy is deterministic; skip ahead so the new connection gets
    // fresh nonces, as the shell's CSPRNG would give it.)
    let mut rig = fresh(&json);
    for _ in 0..10 {
        rig.seed();
    }
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
    let client = RemoteClient::pair(code, "Attacker", entropy(rig.now)).unwrap();
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
    let offer = rig.hub.open_pairing(rig.now, entropy(1)).unwrap();
    let wrong = jarvis_remote::format_code(&jarvis_remote::PairingSecret::from_bytes([0x5a; 20]));
    assert_ne!(wrong, offer.code);
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
    // Malformed codes are refused locally.
    assert!(RemoteClient::pair("1234-56", "x", entropy(3)).is_err());
    assert!(matches!(
        RemoteClient::pair(&offer.code, " \u{200B} ", entropy(3)),
        Err(ClientError::InvalidName)
    ));
}

#[test]
fn expired_code_is_rejected() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, entropy(1)).unwrap();
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
    let offer = rig.hub.open_pairing(rig.now, entropy(1)).unwrap();
    let mut client = RemoteClient::pair(&offer.code, "Mac", entropy(2)).unwrap();
    let fx = rig.hub.connection_opened(5, rig.now, entropy(3));
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
    let offer = rig.hub.open_pairing(rig.now, entropy(1)).unwrap();
    let (fx, _, _) = try_pair(&mut rig, &offer.code);
    assert!(paired(&fx));
    let (fx, _, conn) = try_pair(&mut rig, &offer.code);
    assert!(!paired(&fx));
    assert_eq!(closed(&fx, conn), Some(CloseReason::AuthFailed));
    assert_eq!(rig.hub.devices().len(), 1);

    let offer = rig.hub.open_pairing(rig.now, entropy(2)).unwrap();
    rig.hub.cancel_pairing();
    let (fx, _, _) = try_pair(&mut rig, &offer.code);
    assert!(!paired(&fx));
    // A newer offer replaces an older one.
    let old = rig.hub.open_pairing(rig.now, entropy(3)).unwrap();
    let new = rig.hub.open_pairing(rig.now, entropy(4)).unwrap();
    let (fx, _, _) = try_pair(&mut rig, &old.code);
    assert!(!paired(&fx));
    let (fx, _, _) = try_pair(&mut rig, &new.code);
    assert!(paired(&fx));
}

#[test]
fn handshake_lockout_after_failures_and_recovery_after_backoff() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, entropy(1)).unwrap();
    let wrong = jarvis_remote::format_code(&jarvis_remote::PairingSecret::from_bytes([1; 20]));
    for i in 1..=LOCKOUT_THRESHOLD {
        let (fx, _, _) = try_pair(&mut rig, &wrong);
        assert!(audits(&fx).contains(&AuditEvent::HandshakeFailed { failures: i }));
        rig.now += 10;
    }
    let until = rig.hub.locked_until_ms();
    assert_eq!(until, rig.now - 10 + LOCKOUT_BASE_MS);
    // During the lockout even the right code is refused, without being checked.
    let (fx, _, conn) = try_pair(&mut rig, &offer.code);
    assert!(!paired(&fx));
    assert_eq!(closed(&fx, conn), Some(CloseReason::LockedOut));
    assert!(audits(&fx).contains(&AuditEvent::ConnectionRefused {
        reason: CloseReason::LockedOut
    }));
    // So is a known device.
    let mut other = Rig::new();
    let (_, creds) = other.pair("Mac");
    let mut locked = Rig::with_hub(RemoteHub::from_json("P", &other.hub.to_json()).unwrap());
    for _ in 0..LOCKOUT_THRESHOLD {
        let (_, fx, _) = locked.open(RemoteClient::connect(
            ClientCredentials {
                device_id: creds.device_id,
                device_key: jarvis_remote::Secret32::from_bytes([9; 32]),
            },
            entropy(8),
        ));
        assert!(audits(&fx)
            .iter()
            .any(|a| matches!(a, AuditEvent::HandshakeFailed { .. })));
    }
    let (link, fx, _) = locked.open(RemoteClient::connect(creds.clone(), entropy(9)));
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::LockedOut));
    locked.now = locked.hub.locked_until_ms();
    locked.connect(&creds);

    // One more failure after the threshold doubles the backoff.
    rig.now = until;
    let (_, _, _) = try_pair(&mut rig, &wrong);
    assert_eq!(rig.hub.locked_until_ms(), rig.now + 2 * LOCKOUT_BASE_MS);
    // After the backoff the right code works, and success resets the counter.
    rig.now = rig.hub.locked_until_ms();
    let (fx, _, _) = try_pair(&mut rig, &offer.code);
    assert!(paired(&fx));
    let (fx, _, _) = try_pair(&mut rig, &wrong);
    assert!(audits(&fx).contains(&AuditEvent::HandshakeFailed { failures: 1 }));
}

#[test]
fn oversized_frame_is_rejected_before_buffering() {
    let mut rig = Rig::new();
    let (link, _) = rig.pair("Mac");
    let len = u32::try_from(MAX_RECORD_LEN + 1).unwrap();
    let fx = rig.hub.receive(link.conn, &len.to_be_bytes(), rig.now);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::TooLarge));
    // Pre-auth too.
    rig.hub.connection_opened(50, rig.now, entropy(50));
    let fx = rig.hub.receive(50, &u32::MAX.to_be_bytes(), rig.now);
    assert_eq!(closed(&fx, 50), Some(CloseReason::TooLarge));
    // A sealed message over the plaintext limit cannot even be built.
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
    // The next frame completes the length with the wrong bytes: rejected.
    let next = link.client.send(&ClientMessage::Ping).unwrap();
    let fx = rig.hub.receive(link.conn, &next, rig.now);
    inert(&fx, link.conn);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    // A frame truncated to a valid shorter length (tag cut off) is rejected too.
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let mut frame = link.client.send(&ClientMessage::Ping).unwrap();
    frame.truncate(frame.len() - 5);
    let new_len = u32::try_from(frame.len() - 4).unwrap();
    frame[..4].copy_from_slice(&new_len.to_be_bytes());
    let fx = rig.hub.receive(link.conn, &frame, rig.now);
    inert(&fx, link.conn);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    // A truncated handshake times out.
    rig.hub.connection_opened(60, rig.now, entropy(60));
    assert!(rig
        .hub
        .receive(60, &[0, 0, 0, 90, 1, 4, 7], rig.now)
        .is_empty());
    rig.now += jarvis_remote::HANDSHAKE_TIMEOUT_MS;
    let fx = rig.hub.tick(rig.now);
    assert_eq!(closed(&fx, 60), Some(CloseReason::HandshakeTimeout));
}

#[test]
fn garbage_bytes_close_the_connection() {
    let mut rig = Rig::new();
    let (link, _) = rig.pair("Mac");
    let garbage = b"GET / HTTP/1.1\r\nHost: phone\r\n\r\n";
    let fx = rig.hub.receive(link.conn, garbage, rig.now);
    inert(&fx, link.conn);
    assert!(closed(&fx, link.conn).is_some());
    rig.hub.connection_opened(70, rig.now, entropy(70));
    let fx = rig.hub.receive(70, garbage, rig.now);
    inert(&fx, 70);
    assert!(closed(&fx, 70).is_some());
    // Well-formed records of the wrong kind or version, pre- and post-auth.
    rig.hub.connection_opened(71, rig.now, entropy(71));
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
    rig.hub.connection_opened(72, rig.now, entropy(72));
    let mut wrong_version = encode_record(Kind::ConnectRequest, &[0; 80]);
    wrong_version[4] = 2;
    let fx = rig.hub.receive(72, &wrong_version, rig.now);
    assert_eq!(closed(&fx, 72), Some(CloseReason::AuthFailed));
}

#[test]
fn unknown_and_malformed_messages_get_typed_errors_and_keep_the_connection() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let cases: [(&[u8], ErrorCode); 4] = [
        (br#"{"type":"format_disk"}"#, ErrorCode::UnknownMessage),
        (br#"{"type":"start_task"}"#, ErrorCode::Malformed),
        (br#"{"type":"ping","admin":true}"#, ErrorCode::Malformed),
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
    // The remote verifies the phone's MAC: a forged PairAccept is an error.
    let mut client = RemoteClient::pair(
        &jarvis_remote::format_code(&jarvis_remote::PairingSecret::from_bytes([3; 20])),
        "Mac",
        entropy(1),
    )
    .unwrap();
    let ev = client
        .receive(&encode_record(Kind::ServerHello, &[7; 32]))
        .unwrap();
    assert_eq!(ev.len(), 1);
    let forged = encode_record(Kind::PairAccept, &[0x41; 48]);
    assert_eq!(client.receive(&forged), Err(ClientError::Auth));
    assert!(!client.is_connected());
    assert_eq!(
        client.send(&ClientMessage::Ping),
        Err(ClientError::NotConnected)
    );

    let (_, creds) = paired_store();
    let mut client = RemoteClient::connect(creds, entropy(2));
    client
        .receive(&encode_record(Kind::ServerHello, &[7; 32]))
        .unwrap();
    assert_eq!(
        client.receive(&encode_record(Kind::ConnectAccept, &[0; 32])),
        Err(ClientError::Auth)
    );
    // Out-of-order records from the phone are refused too.
    let (_, creds) = paired_store();
    let mut client = RemoteClient::connect(creds, entropy(3));
    assert_eq!(
        client.receive(&encode_record(Kind::Data, &[0; 30])),
        Err(ClientError::Auth)
    );
}

#[test]
fn unknown_device_and_wrong_key_fail_like_a_wrong_code() {
    let (json, creds) = paired_store();
    let mut rig = fresh(&json);
    let impostor = ClientCredentials {
        device_id: jarvis_remote::DeviceId([0xee; 16]),
        device_key: creds.device_key.clone(),
    };
    let (link, fx, _) = rig.open(RemoteClient::connect(impostor, entropy(4)));
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    let wrong_key = ClientCredentials {
        device_id: creds.device_id,
        device_key: jarvis_remote::Secret32::from_bytes([0; 32]),
    };
    let (link, fx, _) = rig.open(RemoteClient::connect(wrong_key, entropy(5)));
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    assert!(audits(&fx).contains(&AuditEvent::HandshakeFailed { failures: 2 }));
}
