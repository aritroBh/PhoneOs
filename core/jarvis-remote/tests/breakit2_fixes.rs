//! The fixes for the second break-it review (`remote_breakit2.rs`), beyond what its own
//! tests pin: the dirty store, the per-device connection caps and blank names.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_remote::{
    ClientError, ClientMessage, CloseReason, HubEffect, HubError, LocalEvent, RemoteClient,
    RemoteHub, ServerMessage, MAX_CONNECTIONS_PER_DEVICE,
};

#[test]
fn a_failed_write_closes_every_connection_and_is_asked_for_again_until_one_succeeds() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Owner Mac");
    rig.hub.connection_opened(500, rig.now, &entropy(500));
    assert_eq!(rig.hub.connection_count(), 2);
    let fx = rig.local(LocalEvent::PermissionsChanged {
        device_id: creds.device_id,
        permissions: jarvis_remote::RemotePermissions::NONE,
    });
    assert!(fx.contains(&HubEffect::StoreChanged));
    // That write fails: every connection closes, the hub forgets them.
    let fx = rig.hub.store_write_failed();
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::Internal));
    assert_eq!(closed(&fx, 500), Some(CloseReason::Internal));
    assert_eq!(rig.hub.connection_count(), 0);
    assert!(rig.hub.store_dirty());
    // The next call asks for the write first; it fails again, and again it is asked for.
    rig.now += 1_000;
    let fx = rig.hub.tick(rig.now);
    assert_eq!(fx.first(), Some(&HubEffect::StoreChanged), "{fx:?}");
    assert!(!rig.hub.store_dirty());
    rig.hub.store_write_failed();
    // A connection while the store is dirty: the write comes before its hello (the shell
    // sends nothing after a write that fails, and the failure closes it).
    let fx = rig.hub.connection_opened(501, rig.now, &entropy(501));
    let store = fx.iter().position(|e| *e == HubEffect::StoreChanged);
    let hello = fx.iter().position(|e| matches!(e, HubEffect::Send { .. }));
    assert!(store.is_some() && store < hello, "{fx:?}");
    let fx = rig.hub.store_write_failed();
    assert_eq!(closed(&fx, 501), Some(CloseReason::Internal));
    // Once a write goes through, nothing more is asked for, and devices connect again.
    rig.now += 1_000;
    assert!(rig.hub.tick(rig.now).contains(&HubEffect::StoreChanged));
    rig.now += 1_000;
    assert!(!rig.hub.tick(rig.now).contains(&HubEffect::StoreChanged));
    let restored = RemoteHub::from_json("Test Phone", &rig.hub.to_json()).unwrap();
    assert!(restored.devices()[0].permissions.is_none());
    let mut link = rig.connect(&creds);
    let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
}

#[test]
fn a_confirming_failure_dropped_with_an_error_is_written_by_the_next_call() {
    let mut rig = Rig::new();
    let (_l, _owner) = rig.pair("Owner Mac");
    let (_p, pending) = rig.pair_pending("Other Mac");
    // The pending device runs out of time; the owner taps Confirm just after: refused,
    // and the removal (asked for by the expiry inside that call) is asked for again.
    rig.now += jarvis_remote::PENDING_CONFIRM_TTL_MS + 1;
    assert_eq!(
        rig.hub.confirm_device(pending.device_id, rig.now),
        Err(HubError::UnknownDevice)
    );
    assert!(rig.hub.store_dirty());
    assert!(rig.hub.tick(rig.now).contains(&HubEffect::StoreChanged));
}

#[test]
fn a_device_holds_at_most_its_cap_and_replaces_only_its_own_connections() {
    let mut rig = Rig::new();
    let (other, other_creds) = rig.pair("Other Mac");
    let (first, creds) = rig.pair("Owner Mac");
    let mut links = vec![first];
    for _ in 1..MAX_CONNECTIONS_PER_DEVICE {
        links.push(rig.connect(&creds));
    }
    for l in &links {
        assert!(rig.hub.is_authenticated(l.conn));
    }
    // One more: its own oldest goes, nothing else.
    let (newer, fx, _) = rig.try_connect(&creds);
    assert!(rig.hub.is_authenticated(newer.conn));
    assert_eq!(
        closed(&fx, links[0].conn),
        Some(CloseReason::TooManyConnections)
    );
    for l in &links[1..] {
        assert!(rig.hub.is_authenticated(l.conn));
    }
    assert!(rig.hub.is_authenticated(other.conn));
    assert_eq!(closed(&fx, other.conn), None);
    // The other device keeps its own.
    let again = rig.connect(&other_creds);
    assert!(rig.hub.is_authenticated(other.conn) && rig.hub.is_authenticated(again.conn));
}

#[test]
fn names_that_draw_nothing_are_refused_at_rename_and_at_pairing() {
    let mut rig = Rig::new();
    let (_l, creds) = rig.pair("Owner Mac");
    let offer = rig.hub.open_pairing(rig.now, &entropy(77)).unwrap();
    for blank in [
        "\u{3164}",
        "\u{2800} \u{FE0F}",
        "\u{034F}\u{180B}",
        "\u{E0100}",
    ] {
        assert_eq!(
            rig.hub.rename_device(creds.device_id, blank),
            Err(HubError::InvalidName),
            "{blank:?}"
        );
        assert!(matches!(
            RemoteClient::pair(&offer.code, blank, &entropy(9)),
            Err(ClientError::InvalidName)
        ));
    }
    // Blank characters beside visible ones are kept (an emoji's variation selector).
    assert_eq!(
        rig.hub
            .rename_device(creds.device_id, "\u{2764}\u{FE0F} Mac"),
        Ok(vec![HubEffect::StoreChanged])
    );
}
