//! The fixes for the confirm step's break-it review (`confirm_breakit.rs`): what the
//! reviewer's tests do not pin on their own.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::RiskCategory;
use jarvis_remote::{
    ClientCredentials, ClientMessage, CloseReason, DenyReason, HubEffect, LocalEvent, RemoteHub,
    RemotePermissions, Secret32, ServerMessage, LOCKOUT_THRESHOLD, MAX_CONNECTIONS, MAX_HANDSHAKES,
    MAX_LEASE, PENDING_CONFIRM_TTL_MS, REQUEST_LEASE_BLOCK,
};

const ALL: RemotePermissions = RemotePermissions {
    start_tasks: true,
    see_progress: true,
    see_screen_text: true,
    approve: true,
};

/// A confirmed device that may approve and follows progress.
fn approver(rig: &mut Rig) -> (Link, ClientCredentials) {
    let (mut link, creds) = rig.pair("Owner Mac");
    rig.set_permissions(&mut link, &creds, ALL);
    let (_, msgs) = rig.request(&mut link, ClientMessage::Subscribe);
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::Denied { .. })),
        "{msgs:?}"
    );
    (link, creds)
}

fn confirm_pending_ids(msgs: &[ServerMessage]) -> Vec<u64> {
    msgs.iter()
        .filter_map(|m| match m {
            ServerMessage::ConfirmPending { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .collect()
}

fn forged(creds: &ClientCredentials) -> ClientCredentials {
    ClientCredentials {
        device_id: creds.device_id,
        device_key: Secret32::from_bytes([7; 32]),
    }
}

// ---- F1 ------------------------------------------------------------------------------

#[test]
fn a_locked_device_still_refuses_forgeries_without_counting_them_and_lets_its_key_in() {
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Owner Mac");
    let bad = forged(&creds);
    for _ in 0..LOCKOUT_THRESHOLD {
        let (link, fx, _) = rig.try_connect(&bad);
        assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    }
    let until = rig.hub.device_locked_until_ms(creds.device_id);
    assert!(until > rig.now);
    // Forgeries during the lockout: refused as locked out, not counted (the lockout does
    // not grow).
    for _ in 0..20 {
        let (link, fx, _) = rig.try_connect(&bad);
        assert_eq!(closed(&fx, link.conn), Some(CloseReason::LockedOut));
        assert!(!rig.hub.is_authenticated(link.conn));
    }
    assert_eq!(rig.hub.device_locked_until_ms(creds.device_id), until);
    // The device's own key: in, at once.
    let mut link = rig.connect(&creds);
    let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
}

// ---- F2 ------------------------------------------------------------------------------

#[test]
fn a_full_table_evicts_a_handshake_and_never_an_authenticated_connection() {
    let mut rig = Rig::new();
    let mut links = Vec::new();
    for i in 0..MAX_CONNECTIONS - 2 {
        links.push(rig.pair(&format!("Device {i}")));
    }
    // Two unfinished handshakes: 10 (older) and 11.
    rig.hub.connection_opened(10, rig.now, &entropy(10));
    rig.now += 1;
    rig.hub.connection_opened(11, rig.now, &entropy(11));
    assert_eq!(rig.hub.connection_count(), MAX_CONNECTIONS);
    // Second break-it review F1: handshakes have their own MAX_HANDSHAKES places, so a new
    // connection is admitted beside them (this asserted that it took the oldest
    // handshake's place in a shared table of MAX_CONNECTIONS).
    let fx = rig.hub.connection_opened(12, rig.now, &entropy(12));
    assert_eq!(closed(&fx, 10), None);
    assert_eq!(closed(&fx, 12), None);
    assert!(
        !sends(&fx, 12).is_empty(),
        "no hello for the new connection"
    );
    assert_eq!(rig.hub.connection_count(), MAX_CONNECTIONS + 1);
    // A stream of strangers only ever replaces handshakes, once their places are full.
    let mut replaced = 0;
    for c in 100..300u64 {
        let fx = rig.hub.connection_opened(c, rig.now, &entropy(c));
        assert!(rig.hub.connection_count() <= MAX_CONNECTIONS - 2 + MAX_HANDSHAKES);
        replaced += fx
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    HubEffect::Close {
                        reason: CloseReason::TooManyConnections,
                        ..
                    }
                )
            })
            .count();
        for (link, _) in &links {
            assert_eq!(
                closed(&fx, link.conn),
                None,
                "an authenticated connection was evicted"
            );
        }
    }
    // 3 handshakes were open, 200 more arrived: all but the first MAX_HANDSHAKES - 3 of
    // them took a handshake's place.
    assert_eq!(replaced, 200 - (MAX_HANDSHAKES - 3));
    for (link, _) in &links {
        assert!(rig.hub.is_authenticated(link.conn));
    }
    // The owner's connect gets in.
    let creds = links[0].1.clone();
    let mut owner = rig.connect(&creds);
    let (_, msgs) = rig.request(&mut owner, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
    // A table of authenticated connections only refuses the newcomer, as before.
    let creds = links[1].1.clone();
    let _more = rig.connect(&creds);
    let fx = rig.hub.connection_opened(999, rig.now, &entropy(999));
    assert_eq!(closed(&fx, 999), Some(CloseReason::TooManyConnections));
    for (link, _) in &links {
        assert!(rig.hub.is_authenticated(link.conn));
    }
}

// ---- F3 ------------------------------------------------------------------------------

#[test]
fn a_restored_pending_device_keeps_its_pairing_deadline_unless_the_clock_went_back() {
    let mut rig = Rig::new();
    let (_link, _creds) = rig.pair_pending("Glimpsed");
    let paired_at = rig.hub.pending_devices()[0].device.paired_at_ms;
    let json = rig.hub.to_json();
    // Restored 3 minutes later: 7 minutes left, not 10.
    let mut hub = RemoteHub::from_json("Test Phone", &json).unwrap();
    hub.tick(paired_at + 3 * 60_000);
    assert_eq!(
        hub.pending_devices()[0].expires_at_ms,
        Some(paired_at + PENDING_CONFIRM_TTL_MS)
    );
    hub.tick(paired_at + PENDING_CONFIRM_TTL_MS - 1);
    assert_eq!(hub.pending_devices().len(), 1);
    let fx = hub.tick(paired_at + PENDING_CONFIRM_TTL_MS);
    assert!(hub.pending_devices().is_empty());
    assert!(fx.contains(&HubEffect::StoreChanged));
    // Restored with a clock earlier than the pairing time: the 10 minutes run from now.
    let mut hub = RemoteHub::from_json("Test Phone", &json).unwrap();
    let early = paired_at / 2;
    hub.tick(early);
    assert_eq!(
        hub.pending_devices()[0].expires_at_ms,
        Some(early + PENDING_CONFIRM_TTL_MS)
    );
}

// ---- F5 ------------------------------------------------------------------------------

#[test]
fn unreasonable_leases_are_refused_at_load() {
    for (nonce, request) in [
        (MAX_LEASE + 1, 0),
        (u64::MAX, 0),
        (0, MAX_LEASE + 1),
        (0, u64::MAX - REQUEST_LEASE_BLOCK),
    ] {
        let json = format!(
            r#"{{"schema_version":2,"nonce_lease":{nonce},"request_lease":{request},"devices":[]}}"#
        );
        assert!(
            RemoteHub::from_json("Test Phone", &json).is_err(),
            "loaded {json}"
        );
    }
    let json = format!(
        r#"{{"schema_version":2,"nonce_lease":{MAX_LEASE},"request_lease":{MAX_LEASE},"devices":[]}}"#
    );
    assert!(RemoteHub::from_json("Test Phone", &json).is_ok());
}

#[test]
fn ids_that_run_out_leave_questions_to_the_phone_and_never_repeat() {
    let json = format!(
        r#"{{"schema_version":2,"nonce_lease":0,"request_lease":{},"devices":[]}}"#,
        MAX_LEASE - 1
    );
    let mut rig = Rig::with_hub(RemoteHub::from_json("Test Phone", &json).unwrap());
    let (mut link, _creds) = approver(&mut rig);
    // The last id is offered.
    let msgs = rig.local_to(&mut link, confirm_event(RiskCategory::FirstUse, "Last?"));
    assert_eq!(confirm_pending_ids(&msgs), vec![MAX_LEASE - 1]);
    rig.resolve_to(&mut link, false);
    assert!(rig
        .hub
        .to_json()
        .contains(&format!("\"request_lease\":{MAX_LEASE}")));
    // Then the phone still asks, but no device is shown the question or may answer it.
    for _ in 0..3 {
        let msgs = rig.local_to(&mut link, confirm_event(RiskCategory::FirstUse, "After?"));
        assert!(confirm_pending_ids(&msgs).is_empty(), "{msgs:?}");
        let id = rig.hub.pending_request_id().unwrap();
        let (fx, msgs) = rig.request(
            &mut link,
            ClientMessage::Decision {
                request_id: id,
                approved: true,
            },
        );
        assert!(actions(&fx).is_empty(), "{fx:?}");
        assert_eq!(
            msgs,
            vec![ServerMessage::Denied {
                reason: DenyReason::StaleRequest
            }]
        );
        rig.resolve_to(&mut link, false);
    }
    // The stored hub still loads.
    assert!(RemoteHub::from_json("Test Phone", &rig.hub.to_json()).is_ok());
}

// ---- A store write that fails --------------------------------------------------------

#[test]
fn after_a_failed_store_write_no_id_from_the_unsaved_lease_reaches_a_device() {
    let mut rig = Rig::new();
    let (link, creds) = approver(&mut rig);
    // The first question leases a block; the write of that lease fails. The shell sends
    // nothing after a failed write and closes every connection (`RemoteControlService`).
    let fx = rig.local(confirm_event(RiskCategory::FirstUse, "First?"));
    assert!(fx.contains(&HubEffect::StoreChanged));
    let first = rig.hub.pending_request_id().unwrap();
    rig.hub.store_write_failed();
    rig.hub.connection_closed(link.conn);
    // The device reconnects: the pending question is not offered again and cannot be
    // answered.
    let mut link = rig.connect(&creds);
    let (_, msgs) = rig.request(&mut link, ClientMessage::Hello);
    assert!(confirm_pending_ids(&msgs).is_empty(), "{msgs:?}");
    let (_, msgs) = rig.request(&mut link, ClientMessage::Subscribe);
    assert!(confirm_pending_ids(&msgs).is_empty(), "{msgs:?}");
    let (fx, msgs) = rig.request(
        &mut link,
        ClientMessage::Decision {
            request_id: first,
            approved: true,
        },
    );
    assert!(actions(&fx).is_empty());
    assert_eq!(
        msgs,
        vec![ServerMessage::Denied {
            reason: DenyReason::StaleRequest
        }]
    );
    let fx = rig.local(LocalEvent::ConfirmResolved {
        request_id: first,
        approved: false,
    });
    assert!(
        deliver(&mut link, &fx).is_empty(),
        "the phone-only question was announced"
    );
    // The next question leases again, before its id is sent: StoreChanged comes first.
    let fx = rig.local(confirm_event(RiskCategory::FirstUse, "Second?"));
    let store = fx
        .iter()
        .position(|e| *e == HubEffect::StoreChanged)
        .expect("leased again");
    let send = fx
        .iter()
        .position(|e| matches!(e, HubEffect::Send { .. }))
        .expect("offered");
    assert!(store < send, "{fx:?}");
    let second = rig.hub.pending_request_id().unwrap();
    assert!(second > first);
    assert_eq!(confirm_pending_ids(&deliver(&mut link, &fx)), vec![second]);
    // That write succeeds: a hub restored from it starts above every id handed out.
    let json = rig.hub.to_json();
    let mut again = Rig::with_hub(RemoteHub::from_json("Test Phone", &json).unwrap());
    again.local(confirm_event(RiskCategory::FirstUse, "After the restart?"));
    assert!(again.hub.pending_request_id().unwrap() > second);
}

#[test]
fn a_failed_store_write_also_leases_the_connection_counter_again() {
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Owner Mac");
    rig.hub.store_write_failed();
    let fx = rig.hub.connection_opened(500, rig.now, &entropy(500));
    let store = fx.iter().position(|e| *e == HubEffect::StoreChanged);
    let hello = fx.iter().position(|e| matches!(e, HubEffect::Send { .. }));
    assert!(store.is_some() && store < hello, "{fx:?}");
    rig.hub.connection_closed(500);
    rig.connect(&creds);
}
