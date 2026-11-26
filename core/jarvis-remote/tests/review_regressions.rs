//! Regression tests for the adversarial review of protocol version 1 (findings F01–F11)
//! and for the design changes adopted with version 2 (D1–D9). Each finding's test is a
//! port of the reviewer's reproduction, adapted to the new API, asserting the secure
//! behaviour.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::collections::BTreeMap;
use std::time::Instant;

use common::*;
use jarvis_proto::{Action, OutcomeStatus, RiskCategory};
use jarvis_remote::*;

fn viewer() -> (Rig, Link, ClientCredentials) {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    let p = RemotePermissions {
        see_screen_text: true,
        approve: true,
        ..RemotePermissions::NEW_DEVICE
    };
    rig.set_permissions(&mut link, &creds, p);
    rig.request(&mut link, ClientMessage::Subscribe);
    (rig, link, creds)
}

fn digits_in_clear(text: &str, digits: &str) -> bool {
    let only: String = text.chars().filter(char::is_ascii_digit).collect();
    only.contains(digits)
}

/// Separators that `clean_text` folds into one space.
const SEPARATORS: [&str; 6] = ["\t", "\n", "\u{2028}", "\u{00A0}", " \u{200D}", "\u{200B} "];

// ---- F01: redaction before whitespace normalisation ---------------------------------

#[test]
fn f01_finish_summary_card_number_is_masked_whatever_the_separator() {
    for sep in SEPARATORS {
        let (mut rig, mut link, _) = viewer();
        rig.local_to(&mut link, started(None));
        let summary = format!("Paid with card 4111{sep}1111{sep}1111{sep}1111");
        let msgs = rig.local_to(&mut link, finished(None, OutcomeStatus::Success, &summary));
        let got = msgs
            .iter()
            .find_map(|m| match m {
                ServerMessage::Finished { summary, .. } => Some(summary.clone()),
                _ => None,
            })
            .unwrap();
        assert!(!digits_in_clear(&got, "4111"), "{sep:?}: {got:?}");
        assert!(got.contains("[redacted card]"), "{sep:?}: {got:?}");
    }
}

#[test]
fn f01_confirm_summary_otp_is_masked_whatever_the_separator() {
    for sep in SEPARATORS {
        let (mut rig, mut link, _) = viewer();
        let msgs = rig.local_to(
            &mut link,
            confirm_event(
                RiskCategory::SendMessage,
                &format!("Send 'Your code is 123{sep}456' to Bob"),
            ),
        );
        let got = msgs
            .iter()
            .find_map(|m| match m {
                ServerMessage::ConfirmPending { summary, .. } => Some(summary.clone()),
                _ => None,
            })
            .unwrap();
        assert!(!digits_in_clear(&got, "123456"), "{sep:?}: {got:?}");
        assert!(!got.contains("123"), "{sep:?}: {got:?}");
    }
}

#[test]
fn f01_screen_text_card_number_is_masked_whatever_the_separator() {
    for sep in SEPARATORS {
        let (mut rig, mut link, _) = viewer();
        let text = format!("Card\n4111{sep}1111{sep}1111{sep}1111\nExp 12/29");
        let msgs = rig.local_to(&mut link, progress_event(1, Action::Look, Some(&text)));
        let got = msgs
            .iter()
            .find_map(|m| match m {
                ServerMessage::Progress { screen_text, .. } => screen_text.clone(),
                _ => None,
            })
            .unwrap();
        assert!(!digits_in_clear(&got, "4111"), "{sep:?}: {got:?}");
        assert!(got.contains("Exp"), "{sep:?}: {got:?}");
    }
}

// ---- F02: entropy misuse ------------------------------------------------------------

#[test]
fn f02a_shared_entropy_does_not_leak_the_pairing_secret_in_server_hello() {
    let mut hub = RemoteHub::new("Phone");
    let e = entropy(42);
    let offer = hub.open_pairing(0, &e).unwrap();
    // Shell bug: the same buffer reused as the connection entropy.
    let fx = hub.connection_opened(1, 1, &e);
    let hello = sends(&fx, 1);
    let np = &hello[6..38];
    let ep = &hello[38..70];
    let mut s = [0u8; 20];
    s.copy_from_slice(&np[..20]);
    let stolen = format_code(&PairingSecret::from_bytes(s));
    assert_ne!(stolen.as_str(), offer.code.as_str());
    // Nothing on the wire equals the raw entropy either.
    assert_ne!(np, &e[..]);
    assert_ne!(ep, &e[..]);
    let mut rig = Rig::with_hub(hub);
    rig.now = 2;
    let client = RemoteClient::pair(&stolen, "Attacker", &entropy(7)).unwrap();
    let (_l, fx, _ev) = rig.open(client);
    assert!(
        !fx.iter()
            .any(|e| matches!(e, HubEffect::DevicePaired { .. })),
        "network observer paired using bytes read from ServerHello"
    );
}

#[test]
fn f02b_repeated_connection_entropy_does_not_allow_session_replay() {
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Mac");
    let fixed = entropy(999);
    let fx = rig.hub.connection_opened(50, rig.now, &fixed);
    let mut client = RemoteClient::connect(creds.clone(), &entropy(1234)).unwrap();
    let ev = client.receive(&sends(&fx, 50)).unwrap();
    let mut recorded: Vec<u8> = Vec::new();
    for e in &ev {
        if let ClientEvent::Send { bytes } = e {
            recorded.extend(bytes);
        }
    }
    let fx = rig.hub.receive(50, &recorded, rig.now);
    client.receive(&sends(&fx, 50)).unwrap();
    let cmd = client
        .send(&ClientMessage::StartTask {
            text: "Send Bob 'yes'".into(),
        })
        .unwrap();
    recorded.extend(&cmd);
    let fx = rig.hub.receive(50, &cmd, rig.now);
    let id = task_id(&fx);
    assert!(id.is_some());
    rig.hub.connection_closed(50);
    rig.local(finished(id, OutcomeStatus::Success, "done"));
    // Later: the shell hands out the same entropy again, and even the same clock.
    rig.hub.connection_opened(51, rig.now, &fixed);
    let fx = rig.hub.receive(51, &recorded, rig.now);
    assert!(actions(&fx).is_empty(), "recorded session replayed: {fx:?}");
    assert!(!rig.hub.is_authenticated(51));
}

#[test]
fn f02b_repeated_connect_request_is_refused_as_a_replay_and_not_counted() {
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Mac");
    let link = rig.connect(&creds);
    let request = link.sent[0].clone();
    for i in 0..LOCKOUT_THRESHOLD + 2 {
        let conn = 700 + u64::from(i);
        let seed = rig.seed();
        rig.hub.connection_opened(conn, rig.now, &seed);
        let fx = rig.hub.receive(conn, &request, rig.now);
        assert_eq!(closed(&fx, conn), Some(CloseReason::Replayed));
    }
    // Replays are not MAC failures: the device is not locked out by them.
    assert_eq!(rig.hub.device_locked_until_ms(creds.device_id), 0);
    rig.connect(&creds);
}

#[test]
fn f02c_weak_entropy_is_refused_with_a_typed_error() {
    let mut hub = RemoteHub::new("Phone");
    assert_eq!(
        hub.open_pairing(0, &[0u8; 32]).unwrap_err(),
        HubError::Entropy(EntropyError::NotRandom)
    );
    assert_eq!(
        hub.open_pairing(0, &[0xa5u8; 64]).unwrap_err(),
        HubError::Entropy(EntropyError::NotRandom)
    );
    assert_eq!(
        hub.open_pairing(0, &entropy(1)[..16]).unwrap_err(),
        HubError::Entropy(EntropyError::TooShort)
    );
    let fx = hub.connection_opened(1, 0, &[0u8; 32]);
    assert_eq!(closed(&fx, 1), Some(CloseReason::BadEntropy));
    assert_eq!(hub.connection_count(), 0);
    assert!(matches!(
        RemoteClient::pair(
            &format_code(&PairingSecret::from_bytes([1; 20])),
            "Mac",
            &[0; 32]
        ),
        Err(ClientError::Entropy(EntropyError::NotRandom))
    ));
    // Longer entropy is fine.
    assert!(hub
        .open_pairing(0, &[entropy(1), entropy(2)].concat())
        .is_ok());
}

#[test]
fn f02d_repeated_entropy_never_repeats_the_phone_nonce_or_key() {
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Mac");
    let np = entropy(31337);
    let fx = rig.hub.connection_opened(60, rig.now, &np);
    let mut client = RemoteClient::connect(creds.clone(), &entropy(555)).unwrap();
    let hello1 = sends(&fx, 60);
    let ev = client.receive(&hello1).unwrap();
    let mut rec = Vec::new();
    for e in &ev {
        if let ClientEvent::Send { bytes } = e {
            rec.extend(bytes);
        }
    }
    let fx = rig.hub.receive(60, &rec, rig.now);
    client.receive(&sends(&fx, 60)).unwrap();
    let hello = client.send(&ClientMessage::Hello).unwrap();
    rec.extend(&hello);
    let reply1 = sends(&rig.hub.receive(60, &hello, rig.now), 60);
    assert!(!reply1.is_empty());
    rig.hub.connection_closed(60);
    rig.local(started(None));
    // Same entropy, same clock: a different nonce and ephemeral key.
    let fx = rig.hub.connection_opened(61, rig.now, &np);
    let hello2 = sends(&fx, 61);
    assert_ne!(hello1[6..38], hello2[6..38]);
    assert_ne!(hello1[38..70], hello2[38..70]);
    // The replayed handshake fails, so the phone encrypts nothing under old keys.
    let fx = rig.hub.receive(61, &rec, rig.now);
    assert!(sends(&fx, 61).is_empty(), "{fx:?}");
    assert!(!rig.hub.is_authenticated(61));
}

// ---- F03: lockout -------------------------------------------------------------------

fn garbage(hub: &mut RemoteHub, conn: u64, now: u64) {
    hub.connection_opened(conn, now, &entropy(conn + 10_000));
    // A record with an unknown version: never a MAC check, so never counted.
    hub.receive(conn, &[0, 0, 0, 2, 9, 2], now);
}

fn wrong_pair(rig: &mut Rig) {
    let code = format_code(&PairingSecret::from_bytes([0x33; 20]));
    let seed = rig.seed();
    let _ = rig.open(RemoteClient::pair(&code, "x", &seed).unwrap());
}

#[test]
fn f03_attempts_during_a_lockout_do_not_extend_it() {
    let mut rig = Rig::new();
    rig.hub.open_pairing(rig.now, &entropy(5)).unwrap();
    for _ in 0..LOCKOUT_THRESHOLD {
        wrong_pair(&mut rig);
    }
    let first = rig.hub.pairing_locked_until_ms();
    assert!(first > rig.now);
    rig.now += 1;
    for i in 0..7 {
        garbage(&mut rig.hub, 200 + i, rig.now);
        wrong_pair(&mut rig);
    }
    assert_eq!(rig.hub.pairing_locked_until_ms(), first);
}

#[test]
fn f03_garbage_never_locks_anyone_out() {
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Owner Mac");
    let mut now = rig.now;
    for i in 0..12u64 {
        garbage(&mut rig.hub, 300 + i, now);
    }
    let mut refused = 0;
    for k in 0..36u64 {
        now += 10 * 60_000;
        garbage(&mut rig.hub, 1000 + k, now);
        rig.now = now + 60_000;
        let (link, fx, _) = rig.try_connect(&creds);
        if closed(&fx, link.conn) == Some(CloseReason::LockedOut) {
            refused += 1;
        }
        assert!(rig.hub.is_authenticated(link.conn));
        rig.hub.connection_closed(link.conn);
    }
    assert_eq!(refused, 0);
    assert_eq!(rig.hub.pairing_locked_until_ms(), 0);
    assert_eq!(rig.hub.device_locked_until_ms(creds.device_id), 0);
}

// ---- F04: audit flood ---------------------------------------------------------------

#[test]
fn f04_unauthenticated_refusals_are_merged_per_window() {
    let mut hub = RemoteHub::new("Phone");
    let now = 1_000_000;
    for c in 0..MAX_CONNECTIONS as u64 {
        hub.connection_opened(c, now, &entropy(c + 1));
    }
    let mut audit = 0usize;
    for c in 100..100_100u64 {
        for e in hub.connection_opened(c, now, &entropy(c)) {
            if matches!(e, HubEffect::Audit { .. }) {
                audit += 1;
            }
        }
    }
    assert!(
        audit <= 10,
        "unbounded pre-auth audit growth: {audit} entries"
    );
    // The window's summary carries the count.
    let fx = hub.tick(now + PREAUTH_AUDIT_WINDOW_MS);
    let summary: Vec<_> = audits(&fx)
        .into_iter()
        .filter_map(|a| match a {
            AuditEvent::PreAuthSuppressed { count, .. } => Some(count),
            _ => None,
        })
        .collect();
    // 100 000 refusals, one logged in full and the rest counted. (The 8 handshake
    // timeouts this tick causes open the next window.)
    assert_eq!(summary, vec![100_000 - 1]);
}

// ---- F05: unsanitised remote text ---------------------------------------------------

#[test]
fn f05a_unknown_type_is_cleaned_before_the_audit_log() {
    let mut rig = Rig::new();
    let (mut link, _c) = rig.pair("Mac");
    let raw = "{\"type\":\"\\u202e\\u0007\\u001b[2Jdeggap revoked\\u202c\"}";
    let bytes = link.client.send_raw(raw.as_bytes()).unwrap();
    let (fx, _) = rig.feed(&mut link, &bytes);
    let req = audits(&fx)
        .into_iter()
        .find_map(|a| match a {
            AuditEvent::Rejected { request, .. } => request,
            _ => None,
        })
        .unwrap();
    assert!(
        !req.chars()
            .any(|c| c.is_control() || ('\u{202a}'..='\u{202e}').contains(&c)),
        "{req:?}"
    );
    assert!(req.chars().count() <= 64);
}

#[test]
fn f05b_run_skill_args_are_strings_cleaned_and_capped() {
    let mut rig = Rig::new();
    let (mut link, _c) = rig.pair("Mac");
    rig.local(LocalEvent::CatalogChanged {
        skills: vec![CatalogEntry {
            id: "text_mom".into(),
            name: "Text Mom".into(),
            summary: "".into(),
        }],
        workflows: vec![],
    });
    let mut args = BTreeMap::new();
    args.insert(
        "message".to_string(),
        format!("hi\u{202e}\u{0}{}", "x".repeat(60_000)),
    );
    let (fx, _) = rig.request(
        &mut link,
        ClientMessage::RunSkill {
            id: "text_mom".into(),
            args,
        },
    );
    let got = actions(&fx)
        .into_iter()
        .find_map(|a| match a {
            HubEffect::RunSkill { args, .. } => Some(args),
            _ => None,
        })
        .unwrap();
    let v = &got["message"];
    assert!(v.chars().count() <= MAX_SKILL_ARG_VALUE_CHARS);
    assert!(v.chars().count() <= MAX_TASK_TEXT_CHARS);
    assert!(!v.contains('\u{202e}') && !v.contains('\0'));
    // Too many characters in all is refused outright.
    rig.local_to(
        &mut link,
        finished(task_id(&fx), OutcomeStatus::Success, ""),
    );
    let big: BTreeMap<String, String> = (0..3)
        .map(|i| (format!("k{i}"), "v".repeat(2_000)))
        .collect();
    let (fx, msgs) = rig.request(
        &mut link,
        ClientMessage::RunSkill {
            id: "text_mom".into(),
            args: big,
        },
    );
    assert!(actions(&fx).is_empty());
    assert_eq!(
        msgs,
        vec![ServerMessage::Error {
            code: ErrorCode::InvalidRequest
        }]
    );
}

// ---- F06: lifecycle identity --------------------------------------------------------

#[test]
fn f06_owners_local_task_is_not_attributed_to_a_remote_and_keeps_the_hub_busy() {
    let mut rig = Rig::new();
    let (mut a, ca) = rig.pair("A");
    let (mut b, _cb) = rig.pair("B");
    rig.set_permissions(
        &mut a,
        &ca,
        RemotePermissions {
            see_progress: false,
            ..RemotePermissions::NEW_DEVICE
        },
    );
    let (fx, _) = rig.request(
        &mut a,
        ClientMessage::StartTask {
            text: "Open Notes".into(),
        },
    );
    let a_task = task_id(&fx).unwrap();
    // The owner starts a task on the phone first; the shell reports it as local.
    let to_a = rig.local_to(&mut a, started(None));
    // The shell refuses A's request (agent busy) and reports it as finished.
    let to_a_finish = rig.local_to(
        &mut a,
        finished(Some(a_task), OutcomeStatus::Failed, "busy"),
    );
    assert!(
        to_a.is_empty(),
        "A was sent the owner's TaskStarted: {to_a:?}"
    );
    assert!(matches!(
        to_a_finish[..],
        [ServerMessage::Finished {
            status: OutcomeStatus::Failed,
            ..
        }]
    ));
    assert!(rig.hub.is_busy(), "the owner's task still runs");
    let (fx, msgs) = rig.request(
        &mut b,
        ClientMessage::StartTask {
            text: "Call Mom".into(),
        },
    );
    assert!(actions(&fx).is_empty());
    assert_eq!(
        msgs,
        vec![ServerMessage::Denied {
            reason: DenyReason::Busy
        }]
    );
}

#[test]
fn f06_revoking_a_device_never_cancels_the_owners_task() {
    let mut rig = Rig::new();
    let (mut a, ca) = rig.pair("A");
    let (fx, _) = rig.request(
        &mut a,
        ClientMessage::StartTask {
            text: "Open Notes".into(),
        },
    );
    let a_task = task_id(&fx).unwrap();
    // The owner's local task wins the race and is reported first.
    rig.local(started(None));
    let fx = rig.local(LocalEvent::DeviceRevoked {
        device_id: ca.device_id,
    });
    let cancels = actions(&fx);
    // Only A's own (never started) request is withdrawn, by its id.
    assert_eq!(
        cancels,
        vec![HubEffect::CancelTask {
            device_id: ca.device_id,
            task_id: Some(a_task),
        }]
    );
    assert!(!cancels
        .iter()
        .any(|c| matches!(c, HubEffect::CancelTask { task_id: None, .. })));
    // And with no request outstanding, nothing at all.
    let (mut b, cb) = rig.pair("B");
    rig.local(finished(None, OutcomeStatus::Success, ""));
    rig.local(finished(Some(a_task), OutcomeStatus::Cancelled, ""));
    rig.local(started(None));
    let _ = rig.request(&mut b, ClientMessage::Ping);
    let fx = rig.local(LocalEvent::DeviceRevoked {
        device_id: cb.device_id,
    });
    assert!(actions(&fx).is_empty(), "{fx:?}");
}

#[test]
fn f06_late_confirm_resolved_does_not_clear_a_newer_request() {
    let mut rig = Rig::new();
    let (mut a, ca) = rig.pair("A");
    rig.set_permissions(
        &mut a,
        &ca,
        RemotePermissions {
            approve: true,
            ..RemotePermissions::NEW_DEVICE
        },
    );
    rig.request(&mut a, ClientMessage::Subscribe);
    rig.local_to(
        &mut a,
        confirm_event(RiskCategory::FirstUse, "Let Jarvis act in Bank?"),
    );
    let first_use = rig.hub.pending_request_id().unwrap();
    rig.request(
        &mut a,
        ClientMessage::Decision {
            request_id: first_use,
            approved: true,
        },
    );
    rig.local_to(&mut a, confirm_event(RiskCategory::Payment, "Pay $500"));
    let payment_id = rig.hub.pending_request_id().unwrap();
    // The shell reports the FirstUse resolution late.
    let got = rig.local_to(
        &mut a,
        LocalEvent::ConfirmResolved {
            request_id: first_use,
            approved: true,
        },
    );
    assert!(got.is_empty(), "{got:?}");
    assert_eq!(rig.hub.pending_request_id(), Some(payment_id));
}

// ---- F07: clock stepping backwards --------------------------------------------------

#[test]
fn f07_offer_and_half_open_connections_expire_after_a_backwards_step() {
    let mut hub = RemoteHub::new("Phone");
    let offer = hub.open_pairing(10_000_000, &entropy(1)).unwrap();
    hub.connection_opened(1, 10_000_000, &entropy(2));
    let mut now = 10_000_000 - 3_600_000;
    for _ in 0..59 {
        now += 60_000;
        hub.tick(now);
    }
    assert!(!hub.pairing_open());
    assert_eq!(hub.connection_count(), 0);
    let mut rig = Rig::with_hub(hub);
    rig.now = now;
    let (_l, fx, _) = rig.open(RemoteClient::pair(&offer.code, "Late", &entropy(3)).unwrap());
    assert!(!fx
        .iter()
        .any(|e| matches!(e, HubEffect::DevicePaired { .. })));
}

#[test]
fn f07_backwards_step_never_extends_a_deadline_either() {
    let mut hub = RemoteHub::new("Phone");
    let offer = hub.open_pairing(5_000_000, &entropy(1)).unwrap();
    // Forward to one second before expiry, then a step back: still one second left.
    hub.tick(5_000_000 + PAIRING_TTL_MS - 1_000);
    hub.tick(1_000);
    assert!(hub.pairing_open());
    hub.tick(2_000);
    assert!(!hub.pairing_open());
    assert_eq!(offer.expires_at_ms, 5_000_000 + PAIRING_TTL_MS);
}

#[test]
fn f07_rate_window_resets_and_cancel_gets_through_after_a_backwards_step() {
    let mut rig = Rig::new();
    let (mut link, _c) = rig.pair("Mac");
    rig.now += 5_000;
    let (fx, _) = rig.request(&mut link, ClientMessage::StartTask { text: "x".into() });
    let id = task_id(&fx);
    rig.local_to(&mut link, started(id));
    for _ in 0..RATE_MAX_MESSAGES {
        rig.request(&mut link, ClientMessage::Ping);
    }
    rig.now -= 2_000;
    let mut dropped = 0;
    for _ in 0..10 {
        rig.now += 100;
        let (fx, _) = rig.request(&mut link, ClientMessage::Cancel);
        if !actions(&fx)
            .iter()
            .any(|a| matches!(a, HubEffect::CancelTask { .. }))
        {
            dropped += 1;
        }
    }
    assert_eq!(dropped, 0);
    // The interval spanning the backward step cannot be measured, so the hub's clock
    // has advanced 900 ms here; one more real second later the window has reset.
    rig.now += 100;
    let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
}

// ---- F08: quadratic drain -----------------------------------------------------------

#[test]
fn f08_many_small_frames_in_one_call_cost_linear_time() {
    let mut rig = Rig::new();
    let (mut link, _c) = rig.pair("Mac");
    let n = 2_000usize;
    let frames: Vec<Vec<u8>> = (0..n).map(|_| link.client.send_raw(b"").unwrap()).collect();
    let batch: Vec<u8> = frames.concat();
    let t = Instant::now();
    let fx = rig.hub.receive(link.conn, &batch, rig.now);
    let one_call = t.elapsed();
    assert!(closed(&fx, link.conn).is_none());
    let mut rig2 = Rig::new();
    let (mut link2, _c2) = rig2.pair("Mac");
    let frames2: Vec<Vec<u8>> = (0..n)
        .map(|_| link2.client.send_raw(b"").unwrap())
        .collect();
    let t = Instant::now();
    for f in &frames2 {
        rig2.hub.receive(link2.conn, f, rig2.now);
    }
    let per_call = t.elapsed();
    assert!(
        one_call < per_call * 3,
        "batched {one_call:?} vs per-frame {per_call:?}"
    );
    // The reader alone: a full buffer of empty records, many times over.
    let mut r = codec::RecordReader::new();
    let rec = codec::encode_record(codec::Kind::Data, b"");
    let mut big = Vec::new();
    while big.len() + rec.len() <= MAX_BUFFERED_BYTES {
        big.extend(&rec);
    }
    let mut k = 0;
    for _ in 0..16 {
        r.push(&big).unwrap();
        while let Ok(Some(_)) = r.next_record() {
            k += 1;
        }
    }
    assert_eq!(k, 16 * (big.len() / rec.len()));
    assert_eq!(r.buffered(), 0);
}

// ---- F09: permissions deserialise fail-closed ---------------------------------------

#[test]
fn f09_misspelled_or_missing_permission_fields_never_grant_access() {
    let id = "00112233445566778899aabbccddeeff";
    let key = "11".repeat(32);
    let store = |perms: &str| {
        format!(
            r#"{{"schema_version":2,"nonce_lease":0,"devices":[{{"id":"{id}","name":"Old iPad","key":"{key}","permissions":{perms},"paired_at_ms":1}}]}}"#
        )
    };
    // A misspelling is an error, not silently ignored.
    assert!(matches!(
        DeviceStore::from_json(&store(r#"{"startTasks":false,"seeProgress":false}"#)),
        Err(PersistError::Corrupt(_))
    ));
    // A missing field is false.
    let s = DeviceStore::from_json(&store(r#"{"see_progress":true}"#)).unwrap();
    assert_eq!(
        s.devices[0].permissions,
        RemotePermissions {
            see_progress: true,
            ..RemotePermissions::NONE
        }
    );
    let ev: LocalEvent = serde_json::from_str(&format!(
        r#"{{"type":"permissions_changed","device_id":"{id}","permissions":{{}}}}"#
    ))
    .unwrap();
    let LocalEvent::PermissionsChanged { permissions, .. } = ev else {
        panic!()
    };
    assert!(permissions.is_none());
    assert_eq!(RemotePermissions::default(), RemotePermissions::NONE);
}

// ---- F10: provisional pairing -------------------------------------------------------

#[test]
fn f10_dropped_pair_accept_leaves_no_orphans_and_burns_no_slots() {
    let mut rig = Rig::new();
    for _ in 0..(MAX_DEVICES * 2) {
        let seed = rig.seed();
        let offer = rig.hub.open_pairing(rig.now, &seed).unwrap();
        let conn = rig.conn_id();
        let (now, seed) = (rig.now, rig.seed());
        let fx = rig.hub.connection_opened(conn, now, &seed);
        let mut client = RemoteClient::pair(&offer.code, "MacBook", &rig.seed()).unwrap();
        let ev = client.receive(&sends(&fx, conn)).unwrap();
        for e in ev {
            if let ClientEvent::Send { bytes } = e {
                let fx = rig.hub.receive(conn, &bytes, rig.now);
                assert!(!fx.iter().any(|e| matches!(
                    e,
                    HubEffect::DevicePaired { .. } | HubEffect::StoreChanged
                )));
            }
        }
        // The attacker drops the PairAccept; the provisional device is not persisted.
        assert!(client.credentials().is_none());
        assert!(!rig.hub.to_json().contains("MacBook"));
        let fx = rig.hub.connection_closed(conn);
        assert!(audits(&fx).contains(&AuditEvent::PairingAbandoned));
    }
    assert!(rig.hub.devices().is_empty());
    // Slots are all still free.
    for i in 0..MAX_DEVICES {
        rig.pair(&format!("Device {i}"));
    }
}

#[test]
fn f10_device_is_kept_once_its_first_frame_arrives() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(1)).unwrap();
    let mut client = RemoteClient::pair(&offer.code, "Mac", &entropy(2)).unwrap();
    let fx = rig.hub.connection_opened(9, rig.now, &entropy(3));
    let ev = client.receive(&sends(&fx, 9)).unwrap();
    let ClientEvent::Send { bytes } = &ev[0] else {
        panic!()
    };
    let fx = rig.hub.receive(9, bytes, rig.now);
    assert!(rig.hub.devices().is_empty(), "provisional until confirmed");
    // A provisional pairing reserves its slot.
    assert!(rig.hub.is_authenticated(9));
    let ev = client.receive(&sends(&fx, 9)).unwrap();
    // PairAccept → Paired, Connected, and the automatic Hello.
    let hello = ev
        .iter()
        .find_map(|e| match e {
            ClientEvent::Send { bytes } => Some(bytes.clone()),
            _ => None,
        })
        .unwrap();
    let fx = rig.hub.receive(9, &hello, rig.now);
    let i_paired = fx
        .iter()
        .position(|e| matches!(e, HubEffect::DevicePaired { .. }))
        .unwrap();
    let i_store = fx
        .iter()
        .position(|e| matches!(e, HubEffect::StoreChanged))
        .unwrap();
    assert!(i_paired < i_store);
    // Kept, and persisted, as pending: confirmed only by the owner on the phone.
    assert!(rig.hub.devices().is_empty());
    assert_eq!(rig.hub.pending_devices().len(), 1);
    assert!(rig.hub.to_json().contains("\"name\":\"Mac\""));
    assert!(rig.hub.to_json().contains("\"pending_check\""));
}

// ---- F11: with_store validation -----------------------------------------------------

#[test]
fn f11_with_store_validates_like_from_json() {
    let rec = |k: u8| DeviceRecord {
        id: DeviceId([1; 16]),
        name: "Mac".into(),
        key: Secret32::from_bytes([k; 32]),
        permissions: RemotePermissions::NEW_DEVICE,
        paired_at_ms: 0,
    };
    let dup = DeviceStore {
        devices: vec![rec(1), rec(2)],
        nonce_lease: 0,
    };
    assert!(matches!(
        RemoteHub::with_store("Phone", dup),
        Err(PersistError::Invalid(_))
    ));
    let bad_name = DeviceStore {
        devices: vec![DeviceRecord {
            name: "Mac\u{202E}".into(),
            ..rec(1)
        }],
        nonce_lease: 0,
    };
    assert!(RemoteHub::with_store("Phone", bad_name).is_err());
    let ok = DeviceStore {
        devices: vec![rec(1)],
        nonce_lease: 0,
    };
    let hub = RemoteHub::with_store("Phone", ok).unwrap();
    assert!(RemoteHub::from_json("Phone", &hub.to_json()).is_ok());
}

// ---- Design changes -----------------------------------------------------------------

#[test]
fn d1_forward_secrecy_a_stolen_device_key_does_not_open_recorded_frames() {
    use jarvis_remote::crypto::{Channel, Ephemeral, Side, Transcript};
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Mac");
    // Record a session's public values and one frame.
    let fx = rig.hub.connection_opened(40, rig.now, &entropy(4000));
    let hello = sends(&fx, 40);
    let mut client = RemoteClient::connect(creds.clone(), &entropy(4001)).unwrap();
    let ev = client.receive(&hello).unwrap();
    let ClientEvent::Send { bytes: request } = ev[0].clone() else {
        panic!()
    };
    let fx = rig.hub.receive(40, &request, rig.now);
    client.receive(&sends(&fx, 40)).unwrap();
    let frame = client
        .send(&ClientMessage::StartTask {
            text: "secret plan".into(),
        })
        .unwrap();
    // Later the attacker steals the device key and knows every public value.
    let t = Transcript {
        np: hello[6..38].try_into().unwrap(),
        ep: hello[38..70].try_into().unwrap(),
        nc: request[22..54].try_into().unwrap(),
        er: request[54..86].try_into().unwrap(),
    };
    let body = &frame[6..];
    // Without either ephemeral secret, the best it can do is a DH with its own key.
    let mallory = Ephemeral::remote(&entropy(666)).unwrap();
    for dh in [
        *mallory.agree(&t.ep).unwrap(),
        *mallory.agree(&t.er).unwrap(),
        [0u8; 32],
    ] {
        let mut ch =
            Channel::new(Side::Phone, &creds.device_key, &dh, creds.device_id, &t).unwrap();
        assert!(ch.open(body).is_err());
    }
}

#[test]
fn d3_declining_needs_no_approve_permission_but_needs_the_prompt() {
    let mut rig = Rig::new();
    let (mut shown, shown_creds) = rig.pair("Shown");
    let (mut unshown, _) = rig.pair("Unshown");
    rig.request(&mut shown, ClientMessage::Subscribe);
    rig.local_to(&mut shown, confirm_event(RiskCategory::Payment, "Pay $5"));
    let request_id = rig.hub.pending_request_id().unwrap();
    // The device that was not shown the prompt cannot decline it.
    let (fx, msgs) = rig.request(
        &mut unshown,
        ClientMessage::Decision {
            request_id,
            approved: false,
        },
    );
    assert!(actions(&fx).is_empty());
    assert_eq!(
        msgs,
        vec![ServerMessage::Denied {
            reason: DenyReason::NotPermitted
        }]
    );
    // A new device (no approve) that was shown it can.
    let (fx, _) = rig.request(
        &mut shown,
        ClientMessage::Decision {
            request_id,
            approved: false,
        },
    );
    assert_eq!(
        actions(&fx),
        vec![HubEffect::Decide {
            request_id,
            approved: false,
            category: RiskCategory::Payment,
            device_id: shown_creds.device_id,
        }]
    );
}

#[test]
fn d4_cancel_is_never_rate_limited_away() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    rig.now += RATE_WINDOW_MS;
    let (fx, _) = rig.request(&mut link, ClientMessage::StartTask { text: "x".into() });
    let id = task_id(&fx);
    let mut burst = Vec::new();
    for _ in 0..RATE_MAX_MESSAGES + 5 {
        burst.extend(link.client.send(&ClientMessage::Ping).unwrap());
    }
    burst.extend(link.client.send(&ClientMessage::Cancel).unwrap());
    let (fx, _) = rig.feed(&mut link, &burst);
    assert_eq!(
        actions(&fx),
        vec![HubEffect::CancelTask {
            device_id: creds.device_id,
            task_id: id,
        }]
    );
}

#[test]
fn d6_a_device_with_everything_off_learns_nothing_from_welcome() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    rig.set_permissions(&mut link, &creds, RemotePermissions::NONE);
    rig.local(started(None));
    let (_, msgs) = rig.request(&mut link, ClientMessage::Hello);
    assert_eq!(
        msgs,
        vec![ServerMessage::Welcome {
            protocol_version: PROTOCOL_VERSION,
            permissions: RemotePermissions::NONE,
            phone_name: None,
            busy: None,
        }]
    );
    let (_, msgs) = rig.request(&mut link, ClientMessage::ListSkills);
    assert_eq!(
        msgs,
        vec![ServerMessage::Denied {
            reason: DenyReason::NotPermitted
        }]
    );
}

#[test]
fn d7_trailing_characters_in_a_code_are_rejected() {
    let code = format_code(&PairingSecret::from_bytes([7; 20]));
    assert!(RemoteClient::pair(&code, "Mac", &entropy(1)).is_ok());
    for extra in ["A", "-AAAA", "2"] {
        assert!(matches!(
            RemoteClient::pair(&format!("{}{extra}", code.as_str()), "Mac", &entropy(1)),
            Err(ClientError::Code(CodeError::WrongLength))
        ));
    }
}

#[test]
fn nonce_counter_is_leased_and_persisted_before_use() {
    let mut hub = RemoteHub::new("Phone");
    let fx = hub.connection_opened(1, 0, &entropy(1));
    let store = fx.iter().position(|e| matches!(e, HubEffect::StoreChanged));
    let send = fx.iter().position(|e| matches!(e, HubEffect::Send { .. }));
    assert!(
        store.unwrap() < send.unwrap(),
        "persist before the first nonce is used"
    );
    assert_eq!(hub.store().nonce_lease, NONCE_LEASE_BLOCK);
    // No further persist until the block is used up.
    for c in 2..=NONCE_LEASE_BLOCK {
        hub.connection_closed(c - 1);
        let fx = hub.connection_opened(c, 0, &entropy(c));
        assert!(!fx.contains(&HubEffect::StoreChanged), "{c}");
    }
    hub.connection_closed(NONCE_LEASE_BLOCK);
    let fx = hub.connection_opened(0, 0, &entropy(5));
    assert!(fx.contains(&HubEffect::StoreChanged));
    // A restored hub starts above everything it may have used.
    let restored = RemoteHub::from_json("Phone", &hub.to_json()).unwrap();
    assert_eq!(restored.store().nonce_lease, 2 * NONCE_LEASE_BLOCK);
    let mut restored = restored;
    let fx = restored.connection_opened(1, 0, &entropy(1));
    assert!(fx.contains(&HubEffect::StoreChanged));
    assert_eq!(restored.store().nonce_lease, 3 * NONCE_LEASE_BLOCK);
}

// ---- Properties the review found sound in version 1, kept ---------------------------

#[test]
fn z_debug_output_never_contains_secrets() {
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Mac");
    let offer = rig.hub.open_pairing(rig.now, &entropy(77)).unwrap();
    let conn = rig.conn_id();
    rig.hub.connection_opened(conn, rig.now, &entropy(78));
    let key_hex = codec::to_hex(creds.device_key.as_bytes());
    let code = offer.code.replace('-', "");
    for s in [
        format!("{:?}", rig.hub),
        format!("{:?}", rig.hub.store()),
        format!("{creds:?}"),
        format!("{offer:?}"),
    ] {
        assert!(!s.contains(&key_hex) && !s.contains(&code) && !s.to_uppercase().contains(&code));
    }
}

#[test]
fn z_plaintext_size_boundary() {
    let mut rig = Rig::new();
    let (mut link, _c) = rig.pair("Mac");
    let exact = vec![b' '; MAX_PLAINTEXT_LEN];
    let b = link.client.send_raw(&exact).unwrap();
    let (fx, _) = rig.feed(&mut link, &b);
    assert!(closed(&fx, link.conn).is_none());
    let over = vec![b' '; MAX_PLAINTEXT_LEN + 1];
    let b = link.client.send_raw(&over).unwrap();
    let fx = rig.hub.receive(link.conn, &b, rig.now);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::TooLarge));
}

#[test]
fn z_forbidden_categories_are_never_approved_by_any_path() {
    let mut rig = Rig::new();
    let (mut a, ca) = rig.pair("A");
    let all = RemotePermissions {
        approve: true,
        see_screen_text: true,
        ..RemotePermissions::NEW_DEVICE
    };
    rig.set_permissions(&mut a, &ca, all);
    for cat in NEVER_REMOTE_APPROVE {
        rig.local_to(&mut a, confirm_event(cat, "x"));
        let id = rig.hub.pending_request_id().unwrap();
        // Permissions flip mid-request, re-subscribe, repeated approvals.
        rig.set_permissions(&mut a, &ca, all);
        rig.request(&mut a, ClientMessage::Subscribe);
        for _ in 0..3 {
            let (fx, _) = rig.request(
                &mut a,
                ClientMessage::Decision {
                    request_id: id,
                    approved: true,
                },
            );
            assert!(
                !actions(&fx)
                    .iter()
                    .any(|e| matches!(e, HubEffect::Decide { approved: true, .. })),
                "{cat:?}"
            );
        }
        rig.resolve_to(&mut a, false);
    }
}

#[test]
fn z_key_under_another_id_and_mac_reflection_fail() {
    let mut rig = Rig::new();
    let (_a, ca) = rig.pair("A");
    let (_b, cb) = rig.pair("B");
    let forged = ClientCredentials {
        device_id: cb.device_id,
        device_key: ca.device_key.clone(),
    };
    let (link, _fx, ev) = rig.try_connect(&forged);
    assert!(ev.is_ok() && !rig.hub.is_authenticated(link.conn));
    // Reflection: the phone's ConnectAccept MAC sent back as the remote's MAC, on a new
    // connection opened with the same entropy.
    let np = entropy(4242);
    let fx = rig.hub.connection_opened(900, rig.now, &np);
    let mut c = RemoteClient::connect(ca.clone(), &entropy(4343)).unwrap();
    let ev = c.receive(&sends(&fx, 900)).unwrap();
    let req = match &ev[0] {
        ClientEvent::Send { bytes } => bytes.clone(),
        _ => unreachable!(),
    };
    let fx = rig.hub.receive(900, &req, rig.now);
    let accept = sends(&fx, 900);
    let phone_mac = &accept[6..38];
    rig.hub.connection_opened(901, rig.now, &np);
    let mut forged = req.clone();
    let mac_at = req.len() - 32;
    forged[mac_at..].copy_from_slice(phone_mac);
    let fx = rig.hub.receive(901, &forged, rig.now);
    assert!(matches!(
        closed(&fx, 901),
        Some(CloseReason::AuthFailed | CloseReason::Replayed)
    ));
    assert!(!rig.hub.is_authenticated(901));
}

/// A corrupt store or credentials file is reported by kind and position only: serde_json's
/// own message quotes the value it choked on, and in these files that can be a key.
#[test]
fn corrupt_persistence_errors_never_quote_the_input() {
    use jarvis_remote::{ClientCredentials, DeviceStore};
    const MARK: &str = "5ec2e7c0ffee5ec2e7c0ffee";
    let inputs = [
        format!(r#"{{"schema_version":2,"devices":"{MARK}"}}"#),
        format!(r#"{{"schema_version":2,"devices":[{{"id":7,"key":"{MARK}"}}]}}"#),
        format!(r#"{{"schema_version":2,"devices":[], "{MARK}"#),
        format!(r#""{MARK}""#),
        format!(r#"{{"schema_version":"{MARK}"}}"#),
    ];
    for input in &inputs {
        for error in [
            DeviceStore::from_json(input).err(),
            ClientCredentials::from_json(input).err(),
        ]
        .into_iter()
        .flatten()
        {
            let shown = format!("{error} {error:?}");
            assert!(!shown.contains(MARK), "the error quotes the input: {shown}");
        }
    }
    // Still useful: the kind and the place.
    let error = DeviceStore::from_json("{not json").unwrap_err();
    assert_eq!(
        error.to_string(),
        "corrupt device store: not JSON at line 1 column 2"
    );
}
