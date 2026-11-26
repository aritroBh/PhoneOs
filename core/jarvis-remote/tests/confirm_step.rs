//! The confirm step: a device paired with the code holds nothing until the owner confirms
//! it on the phone, after comparing the check string both sides show. Also the per-offer
//! pairing lockout and the request-id lease that came with the controller review.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::{OutcomeStatus, RiskCategory};
use jarvis_remote::codec::{encode_record, Kind, RecordReader};
use jarvis_remote::crypto::confirm_check;
use jarvis_remote::{
    AuditEvent, ClientEvent, ClientMessage, CloseReason, DenyReason, HubEffect, HubError,
    LocalEvent, RemoteClient, RemoteHub, RemotePermissions, Secret32, ServerMessage, ViewerRequest,
    LOCKOUT_BASE_MS, LOCKOUT_THRESHOLD, MAX_DEVICES, PAIR_LOCKOUT_BUDGET_MS,
    PENDING_CONFIRM_TTL_MS,
};

/// Every request a device can make, with a task running and a question pending.
fn every_request(request_id: u64) -> Vec<ClientMessage> {
    vec![
        ClientMessage::StartTask {
            text: "Turn off Wi-Fi".into(),
        },
        ClientMessage::RunSkill {
            id: "s".into(),
            args: Default::default(),
        },
        ClientMessage::RunWorkflow { id: "w".into() },
        ClientMessage::Cancel,
        ClientMessage::Decision {
            request_id,
            approved: true,
        },
        ClientMessage::Decision {
            request_id,
            approved: false,
        },
        ClientMessage::Subscribe,
        ClientMessage::Unsubscribe,
        ClientMessage::ListSkills,
    ]
}

fn catalog(rig: &mut Rig) {
    let entry = |id: &str| jarvis_remote::CatalogEntry {
        id: id.into(),
        name: id.into(),
        summary: String::new(),
    };
    rig.local(LocalEvent::CatalogChanged {
        skills: vec![entry("s")],
        workflows: vec![entry("w")],
    });
}

#[test]
fn a_pending_device_is_told_only_that_it_waits_and_every_function_is_refused() {
    let mut rig = Rig::new();
    catalog(&mut rig);
    let (mut link, creds) = rig.pair_pending("Glimpsed");
    assert!(rig.hub.devices().is_empty());
    assert_eq!(rig.hub.pending_devices().len(), 1);
    // The automatic Hello was answered with the waiting notice only (no phone name).
    let (_, msgs) = rig.request(&mut link, ClientMessage::Hello);
    assert_eq!(msgs, vec![ServerMessage::AwaitingConfirmation]);
    let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);

    // A task the owner runs, with a question pending.
    let fx = rig.local(started(None));
    assert!(sends(&fx, link.conn).is_empty(), "{fx:?}");
    let fx = rig.local(confirm_event(RiskCategory::SendMessage, "Send it?"));
    assert!(
        sends(&fx, link.conn).is_empty(),
        "the question reached a pending device"
    );
    let request_id = rig.hub.pending_request_id().unwrap();
    for msg in every_request(request_id) {
        rig.now += 100;
        let (fx, msgs) = rig.request(&mut link, msg.clone());
        assert!(actions(&fx).is_empty(), "{msg:?} was carried out: {fx:?}");
        assert_eq!(
            msgs,
            vec![ServerMessage::Denied {
                reason: DenyReason::NotPermitted
            }],
            "{msg:?}"
        );
        assert!(audits(&fx)
            .iter()
            .any(|a| matches!(a, AuditEvent::Denied { .. })));
    }
    // Progress, the answer and the finish go nowhere near it.
    for ev in [
        progress_event(1, jarvis_proto::Action::Look, Some("secret screen")),
        LocalEvent::ConfirmResolved {
            request_id,
            approved: false,
        },
        finished(None, OutcomeStatus::Success, "done"),
    ] {
        let fx = rig.local(ev);
        assert!(sends(&fx, link.conn).is_empty(), "{fx:?}");
    }
    // No Live view either.
    let bytes = link
        .client
        .send_viewer(&ViewerRequest::StartWatching)
        .unwrap();
    let fx = rig.hub.receive(link.conn, &bytes, rig.now);
    assert!(!rig.hub.is_watching(link.conn));
    let back = sends(&fx, link.conn);
    let got = link.client.receive_all(&back).unwrap();
    assert!(matches!(
        got.viewer[..],
        [jarvis_remote::ViewerEvent::Denied {
            reason: DenyReason::NotPermitted
        }]
    ));
    // Still pending, with no permission at all; a reconnect is told the same.
    assert!(rig.hub.devices().is_empty());
    assert_eq!(
        rig.hub.pending_devices()[0].device.permissions,
        RemotePermissions::NONE
    );
    let mut again = rig.connect(&creds);
    let (_, msgs) = rig.request(&mut again, ClientMessage::Hello);
    assert_eq!(msgs, vec![ServerMessage::AwaitingConfirmation]);
}

#[test]
fn a_pending_device_cannot_confirm_itself_by_any_message() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair_pending("Self-confirmer");
    let raw: Vec<&[u8]> = vec![
        br#"{"type":"confirm_device"}"#,
        br#"{"type":"confirm"}"#,
        br#"{"type":"welcome","protocol_version":2,"permissions":{"start_tasks":true,"see_progress":true,"see_screen_text":true,"approve":true},"phone_name":null,"busy":null}"#,
        br#"{"type":"permissions_changed","permissions":{"approve":true}}"#,
        br#"{"type":"hello","confirmed":true}"#,
        b"not json",
        br#"{"type":"viewer_resync"}"#,
        br#"{"type":"stop_watching"}"#,
    ];
    for bytes in raw {
        rig.now += 100;
        let frame = link.client.send_raw(bytes).unwrap();
        let (fx, _) = rig.feed(&mut link, &frame);
        assert!(actions(&fx).is_empty(), "{fx:?}");
        assert!(!fx.contains(&HubEffect::StoreChanged));
    }
    for msg in every_request(1)
        .into_iter()
        .chain([ClientMessage::Hello, ClientMessage::Ping])
    {
        rig.now += 100;
        rig.request(&mut link, msg);
    }
    assert!(rig.hub.devices().is_empty());
    assert_eq!(rig.hub.pending_devices().len(), 1);
    assert_eq!(
        rig.hub.pending_devices()[0].device.permissions,
        RemotePermissions::NONE
    );
}

#[test]
fn confirming_grants_exactly_the_new_device_permissions_and_says_welcome() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair_pending("Owner Mac");
    let fx = rig.hub.confirm_device(creds.device_id, rig.now).unwrap();
    let i_confirmed = fx
        .iter()
        .position(|e| matches!(e, HubEffect::DeviceConfirmed { .. }))
        .unwrap();
    let i_store = fx
        .iter()
        .position(|e| matches!(e, HubEffect::StoreChanged))
        .unwrap();
    let i_send = fx
        .iter()
        .position(|e| matches!(e, HubEffect::Send { .. }))
        .unwrap();
    assert!(
        i_confirmed < i_store && i_store < i_send,
        "persist before telling"
    );
    assert_eq!(
        deliver(&mut link, &fx),
        vec![ServerMessage::Welcome {
            protocol_version: jarvis_remote::PROTOCOL_VERSION,
            permissions: RemotePermissions::NEW_DEVICE,
            phone_name: Some("Test Phone".into()),
            busy: Some(false),
        }]
    );
    assert!(rig.hub.pending_devices().is_empty());
    assert_eq!(
        rig.hub.devices()[0].permissions,
        RemotePermissions::NEW_DEVICE
    );
    assert!(!rig.hub.watch_screen(creds.device_id));
    // Now it may start a task.
    let (fx, _) = rig.request(
        &mut link,
        ClientMessage::StartTask {
            text: "Turn off Wi-Fi".into(),
        },
    );
    assert!(task_id(&fx).is_some());
    // Confirming twice, or an unknown device, is refused.
    assert_eq!(
        rig.hub.confirm_device(creds.device_id, rig.now),
        Err(HubError::UnknownDevice)
    );
}

#[test]
fn removing_a_pending_device_or_letting_it_expire_deletes_it() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair_pending("Removed");
    let fx = rig.local(LocalEvent::DeviceRevoked {
        device_id: creds.device_id,
    });
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::Revoked));
    // Told so in an authenticated frame first (a closed connection proves nothing).
    assert_eq!(deliver(&mut link, &fx), vec![ServerMessage::Removed]);
    assert!(fx.contains(&HubEffect::StoreChanged));
    assert!(rig.hub.pending_devices().is_empty());
    assert!(!rig.hub.to_json().contains("Removed"));
    let (l, fx, _) = rig.try_connect(&creds);
    assert_eq!(closed(&fx, l.conn), Some(CloseReason::AuthFailed));

    let (mut link, creds) = rig.pair_pending("Forgotten");
    let expires = rig.hub.pending_devices()[0].expires_at_ms.unwrap();
    assert_eq!(expires, rig.now + PENDING_CONFIRM_TTL_MS);
    // Kept alive with pings until just before the deadline: still pending.
    while rig.now + 60_000 < expires {
        rig.now += 60_000;
        let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
        assert_eq!(msgs, vec![ServerMessage::Pong]);
    }
    rig.now = expires - 1;
    assert!(rig
        .hub
        .tick(rig.now)
        .iter()
        .all(|e| !matches!(e, HubEffect::Close { .. })));
    assert_eq!(rig.hub.pending_devices().len(), 1);
    rig.now = expires;
    let fx = rig.hub.tick(rig.now);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::Revoked));
    assert_eq!(deliver(&mut link, &fx), vec![ServerMessage::Removed]);
    assert!(fx.contains(&HubEffect::StoreChanged));
    assert!(audits(&fx).contains(&AuditEvent::PendingExpired));
    assert!(rig.hub.pending_devices().is_empty());
    assert_eq!(
        rig.hub.confirm_device(creds.device_id, rig.now),
        Err(HubError::UnknownDevice)
    );
    let (l, fx, _) = rig.try_connect(&creds);
    assert_eq!(closed(&fx, l.conn), Some(CloseReason::AuthFailed));
}

#[test]
fn both_ends_show_the_same_check_string_and_another_device_shows_another() {
    let mut rig = Rig::new();
    let (a, a_creds) = rig.pair_pending("First");
    let (b, b_creds) = rig.pair_pending("Second");
    let pending = rig.hub.pending_devices();
    let check = |id| {
        pending
            .iter()
            .find(|p| p.device.id == id)
            .unwrap()
            .check
            .clone()
    };
    let a_check = a.client.pairing_check().unwrap().to_owned();
    let b_check = b.client.pairing_check().unwrap().to_owned();
    assert_eq!(a_check, check(a_creds.device_id));
    assert_eq!(b_check, check(b_creds.device_id));
    assert_ne!(a_check, b_check);
    // Six digits in two groups.
    assert_eq!(a_check.len(), 7);
    assert!(a_check.chars().enumerate().all(|(i, c)| if i == 3 {
        c == ' '
    } else {
        c.is_ascii_digit()
    }));
    // The derivation depends on the device key (and nothing the phone could leave out).
    let t = jarvis_remote::crypto::Transcript {
        np: [1; 32],
        nc: [2; 32],
        ep: [3; 32],
        er: [4; 32],
    };
    let k1 = Secret32::from_bytes([5; 32]);
    let k2 = Secret32::from_bytes([6; 32]);
    let id = a_creds.device_id;
    let one = confirm_check(&k1, &[7; 32], &id, &t).unwrap();
    assert_eq!(one, confirm_check(&k1, &[7; 32], &id, &t).unwrap());
    assert_ne!(one, confirm_check(&k2, &[7; 32], &id, &t).unwrap());
    assert_ne!(one, confirm_check(&k1, &[8; 32], &id, &t).unwrap());
    // A connect with stored credentials has no check string.
    let link = rig.connect(&a_creds);
    assert_eq!(link.client.pairing_check(), None);
}

#[test]
fn a_man_in_the_middle_with_the_code_shows_a_different_string_on_each_side() {
    // The attacker saw the code: it pairs with the phone itself, and plays the phone for
    // the owner's device (a second hub with the same offer).
    let mut phone = Rig::new();
    let offer = phone.hub.open_pairing(phone.now, &entropy(4242)).unwrap();
    let mut fake = Rig::with_hub(RemoteHub::new("Test Phone"));
    let fake_offer = fake.hub.open_pairing(fake.now, &entropy(4242)).unwrap();
    assert_eq!(offer.code, fake_offer.code);
    let (to_phone, _, ev) =
        phone.open(RemoteClient::pair(&offer.code, "Owner Mac", &entropy(9)).unwrap());
    assert!(ev.unwrap().contains(&ClientEvent::Connected));
    let (owner, _, ev) =
        fake.open(RemoteClient::pair(&offer.code, "Owner Mac", &entropy(10)).unwrap());
    assert!(ev.unwrap().contains(&ClientEvent::Connected));
    let on_phone = phone.hub.pending_devices()[0].check.clone();
    assert_eq!(to_phone.client.pairing_check().unwrap(), on_phone);
    assert_ne!(owner.client.pairing_check().unwrap(), on_phone);
}

#[test]
fn the_code_pairs_one_device_only_and_a_second_attempt_fails() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(5)).unwrap();
    let (_l, _, ev) = rig.open(RemoteClient::pair(&offer.code, "First", &entropy(6)).unwrap());
    assert!(ev.unwrap().contains(&ClientEvent::Connected));
    let (l, fx, ev) = rig.open(RemoteClient::pair(&offer.code, "Second", &entropy(7)).unwrap());
    assert_eq!(closed(&fx, l.conn), Some(CloseReason::AuthFailed));
    assert!(ev.is_err() || !l.client.is_connected());
    assert_eq!(rig.hub.pending_devices().len(), 1);
    assert_eq!(rig.hub.pending_devices()[0].device.name, "First");
}

#[test]
fn a_pending_device_survives_a_restart_still_pending_and_old_stores_load_confirmed() {
    let mut rig = Rig::new();
    let (_owner, owner_creds) = rig.pair("Owner Mac");
    let (_link, creds) = rig.pair_pending("Pending Mac");
    let check = rig.hub.pending_devices()[0].check.clone();
    let json = rig.hub.to_json();
    let mut rig = Rig::with_hub(RemoteHub::from_json("Test Phone", &json).unwrap());
    assert_eq!(rig.hub.devices().len(), 1);
    assert_eq!(rig.hub.devices()[0].id, owner_creds.device_id);
    let pending = rig.hub.pending_devices();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].check, check);
    assert_eq!(pending[0].device.permissions, RemotePermissions::NONE);
    // The expiry is set at the first call after the restore: 10 minutes after the pairing
    // (break-it review F3; it used to be 10 minutes after that call).
    assert_eq!(pending[0].expires_at_ms, None);
    let paired_at = pending[0].device.paired_at_ms;
    rig.hub.tick(rig.now);
    assert!(rig.now >= paired_at);
    assert_eq!(
        rig.hub.pending_devices()[0].expires_at_ms,
        Some(paired_at + PENDING_CONFIRM_TTL_MS)
    );
    let mut link = rig.connect(&creds);
    let (_, msgs) = rig.request(&mut link, ClientMessage::Hello);
    assert_eq!(msgs, vec![ServerMessage::AwaitingConfirmation]);
    let (_, msgs) = rig.request(&mut link, ClientMessage::Subscribe);
    assert_eq!(
        msgs,
        vec![ServerMessage::Denied {
            reason: DenyReason::NotPermitted
        }]
    );
    // The store API that predates the confirm step never loads it as a device.
    let store = jarvis_remote::DeviceStore::from_json(&json).unwrap();
    assert_eq!(store.devices.len(), 1);
    // A forged record that claims permissions while pending still holds none.
    let forged = json.replace(
        r#""permissions":{"start_tasks":false,"see_progress":false,"see_screen_text":false,"approve":false},"paired_at_ms""#,
        r#""permissions":{"start_tasks":true,"see_progress":true,"see_screen_text":true,"approve":true},"paired_at_ms""#,
    );
    assert_ne!(forged.as_str(), json.as_str());
    let hub = RemoteHub::from_json("Test Phone", &forged).unwrap();
    assert_eq!(
        hub.pending_devices()[0].device.permissions,
        RemotePermissions::NONE
    );
    // Confirmed after the restart: the new-device permissions.
    let fx = rig.hub.confirm_device(creds.device_id, rig.now).unwrap();
    assert!(deliver(&mut link, &fx)
        .iter()
        .any(|m| matches!(m, ServerMessage::Welcome { permissions, .. } if *permissions == RemotePermissions::NEW_DEVICE)));

    // Migration: a store written before the confirm step (no pending_check) loads every
    // device as confirmed, with its permissions.
    let old = r#"{"schema_version":2,"nonce_lease":64,"devices":[{"id":"0102030405060708090a0b0c0d0e0f10","name":"Old Mac","key":"0909090909090909090909090909090909090909090909090909090909090909","permissions":{"start_tasks":true,"see_progress":true,"see_screen_text":false,"approve":false},"paired_at_ms":5}]}"#;
    let hub = RemoteHub::from_json("Test Phone", old).unwrap();
    assert!(hub.pending_devices().is_empty());
    assert_eq!(hub.devices().len(), 1);
    assert_eq!(hub.devices()[0].permissions, RemotePermissions::NEW_DEVICE);
    // And writes it back byte for byte.
    assert_eq!(hub.to_json().as_str(), old);
    // A broken check string is refused, not loaded.
    let bad = json.replace(
        &format!("\"pending_check\":\"{check}\""),
        "\"pending_check\":\"12a 456\"",
    );
    assert!(RemoteHub::from_json("Test Phone", &bad).is_err());
}

#[test]
fn pending_devices_count_against_the_device_limit() {
    let mut rig = Rig::new();
    for i in 0..MAX_DEVICES {
        rig.pair_pending(&format!("Device {i}"));
    }
    assert_eq!(rig.hub.pending_devices().len(), MAX_DEVICES);
    assert_eq!(
        rig.hub.open_pairing(rig.now, &entropy(99)).unwrap_err(),
        HubError::TooManyDevices
    );
}

#[test]
fn a_hello_without_the_commitment_or_a_wrong_reveal_fails_the_pairing() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(5)).unwrap();
    let mut client = RemoteClient::pair(&offer.code, "Mac", &entropy(6)).unwrap();
    let fx = rig.hub.connection_opened(3, rig.now, &entropy(7));
    let hello = sends(&fx, 3);
    assert_eq!(
        hello.len(),
        6 + 96,
        "Np, Ep and the commitment while an offer is open"
    );
    let ev = client.receive(&hello).unwrap();
    let ClientEvent::Send { bytes } = &ev[0] else {
        panic!()
    };
    let fx = rig.hub.receive(3, bytes, rig.now);
    let mut accept = sends(&fx, 3);
    // Flip one bit of the revealed value: the client refuses the pairing.
    let last = accept.len() - 1;
    accept[last] ^= 1;
    assert!(client.receive(&accept).is_err());
    assert!(!client.is_connected());

    // The same exchange with the commitment cut from the hello (an older phone, or a
    // host in between that drops it): the client sends its request but refuses the
    // accept.
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(5)).unwrap();
    let mut client = RemoteClient::pair(&offer.code, "Mac", &entropy(6)).unwrap();
    let fx = rig.hub.connection_opened(3, rig.now, &entropy(7));
    let hello = sends(&fx, 3);
    let mut reader = RecordReader::new();
    reader.push(&hello).unwrap();
    let record = reader.next_record().unwrap().unwrap();
    let short = encode_record(Kind::ServerHello, &record.payload[..64]);
    let ev = client.receive(&short).unwrap();
    let ClientEvent::Send { bytes } = &ev[0] else {
        panic!()
    };
    let fx = rig.hub.receive(3, bytes, rig.now);
    assert!(client.receive(&sends(&fx, 3)).is_err());

    // Outside an offer the hello is what it always was (Np, Ep).
    let mut rig = Rig::new();
    let fx = rig.hub.connection_opened(4, rig.now, &entropy(8));
    assert_eq!(sends(&fx, 4).len(), 6 + 64);
}

#[test]
fn wrong_codes_are_counted_per_offer_and_the_lockout_is_bounded() {
    let mut rig = Rig::new();
    rig.hub.open_pairing(rig.now, &entropy(1)).unwrap();
    let wrong = jarvis_remote::format_code(&jarvis_remote::PairingSecret::from_bytes([0x33; 20]));
    for _ in 0..LOCKOUT_THRESHOLD {
        let seed = rig.seed();
        let _ = rig.open(RemoteClient::pair(&wrong, "x", &seed).unwrap());
    }
    assert_eq!(rig.hub.pairing_failed_attempts(), LOCKOUT_THRESHOLD);
    assert_eq!(rig.hub.pairing_locked_until_ms(), rig.now + LOCKOUT_BASE_MS);
    // A new offer the owner opens starts clean.
    let offer = rig.hub.open_pairing(rig.now, &entropy(2)).unwrap();
    assert_eq!(rig.hub.pairing_failed_attempts(), 0);
    assert_eq!(rig.hub.pairing_locked_until_ms(), 0);
    let (_l, fx, _) = rig.open(RemoteClient::pair(&offer.code, "Owner", &entropy(3)).unwrap());
    assert!(fx
        .iter()
        .any(|e| matches!(e, HubEffect::DevicePaired { .. })));

    // However long an attacker keeps trying, one offer is locked at most the budget.
    let mut rig = Rig::new();
    let start = rig.now;
    rig.hub.open_pairing(rig.now, &entropy(4)).unwrap();
    let mut locked = 0;
    while rig.now < start + 4 * 60_000 {
        if rig.hub.pairing_locked_until_ms() > rig.now {
            locked += 1_000;
        } else {
            let seed = rig.seed();
            let _ = rig.open(RemoteClient::pair(&wrong, "x", &seed).unwrap());
        }
        rig.now += 1_000;
    }
    assert!(locked <= PAIR_LOCKOUT_BUDGET_MS, "{locked}");
    assert!(rig.hub.pairing_failed_attempts() > 20);
}

#[test]
fn request_ids_are_leased_so_a_restart_never_reuses_one() {
    let mut rig = Rig::new();
    let fx = rig.local(confirm_event(RiskCategory::FirstUse, "First?"));
    assert!(
        fx.contains(&HubEffect::StoreChanged),
        "the lease is persisted first"
    );
    let first = rig.hub.pending_request_id().unwrap();
    let fx = rig.local(confirm_event(RiskCategory::FirstUse, "Second?"));
    assert!(
        !fx.contains(&HubEffect::StoreChanged),
        "one persist per block"
    );
    let json = rig.hub.to_json();
    let mut rig = Rig::with_hub(RemoteHub::from_json("Test Phone", &json).unwrap());
    rig.local(confirm_event(RiskCategory::FirstUse, "After the restart?"));
    let after = rig.hub.pending_request_id().unwrap();
    assert!(after > first + 1, "{after} after {first}");
}
