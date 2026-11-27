//! Second break-it review of the confirm-step fixes (`5e7cebe`). `f…` tests assert what
//! should hold and fail today for a finding; `sound_…` tests pin behaviour that holds and
//! that no earlier test covers.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stderr
)]

mod common;

use std::time::Instant;

use common::*;
use jarvis_remote::{
    clean_name, ClientCredentials, ClientEvent, ClientMessage, CloseReason, ConnId, HubEffect,
    LocalEvent, RemoteClient, RemoteHub, RemotePermissions, Secret32, ServerMessage,
    LOCKOUT_THRESHOLD, MAX_CONNECTIONS,
};

/// Hostile handshakes that never send a byte: `count` of them, opened at `at` (one per
/// call, ids from `next`).
fn flood(rig: &mut Rig, next: &mut ConnId, count: usize, at: u64) {
    for _ in 0..count {
        *next += 1;
        rig.hub.connection_opened(*next, at, &entropy(*next));
    }
}

/// One honest handshake that takes `rtt_ms` between the phone's hello and the client's
/// reply, while a LAN host opens connections at `rate_per_s`. The table is already full
/// of that host's handshakes. Returns whether the honest connection got in.
fn honest_handshake_under_flood(
    rig: &mut Rig,
    client: RemoteClient,
    rtt_ms: u64,
    rate_per_s: u64,
    next: &mut ConnId,
) -> bool {
    flood(rig, next, MAX_CONNECTIONS, rig.now);
    let mut client = client;
    let conn = 1_000_000 + *next;
    let seed = rig.seed();
    let fx = rig.hub.connection_opened(conn, rig.now, &seed);
    let hello = sends(&fx, conn);
    if hello.is_empty() {
        return false;
    }
    // The hostile connections that arrive while the hello and the reply are in flight.
    for ms in 1..=rtt_ms {
        let due = (rate_per_s * ms / 1_000) - (rate_per_s * (ms - 1) / 1_000);
        flood(rig, next, usize::try_from(due).unwrap(), rig.now + ms);
    }
    rig.now += rtt_ms;
    let Ok(events) = client.receive(&hello) else {
        return false;
    };
    let mut back = Vec::new();
    for ev in &events {
        if let ClientEvent::Send { bytes } = ev {
            back.extend(sends(&rig.hub.receive(conn, bytes, rig.now), conn));
        }
    }
    let ok = rig.hub.is_authenticated(conn) && !back.is_empty();
    rig.hub.connection_closed(conn);
    // Let the hostile handshakes of this round time out before the next round.
    rig.now += 20_000;
    rig.hub.tick(rig.now);
    ok
}

// ---- Connection slots (claim 2) ---------------------------------------------------------

/// Finding: the oldest-handshake eviction lets a LAN host that opens connections faster
/// than 8 per round trip evict the owner's own handshake before its reply arrives, every
/// time: neither a connect nor a pairing can finish. 800 connections a second against a
/// 20 ms Wi-Fi round trip (16 arrivals per round trip) is enough.
#[test]
fn f1_owner_handshake_survives_a_connection_flood() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Owner Mac");
    rig.hub.connection_closed(link.conn);
    let mut next: ConnId = 10_000;

    // Measured: for each (round trip, hostile rate), does the owner's connect get in?
    let mut table = Vec::new();
    for rtt in [5u64, 20, 50] {
        for rate in [100u64, 200, 400, 800, 1_600, 3_200] {
            let client = RemoteClient::connect(creds.clone(), &rig.seed()).unwrap();
            let ok = honest_handshake_under_flood(&mut rig, client, rtt, rate, &mut next);
            table.push(format!(
                "rtt {rtt} ms, {rate}/s: {}",
                if ok { "in" } else { "OUT" }
            ));
        }
    }
    eprintln!(
        "owner connect under a handshake flood:\n  {}",
        table.join("\n  ")
    );

    let client = RemoteClient::connect(creds.clone(), &rig.seed()).unwrap();
    let connect_ok = honest_handshake_under_flood(&mut rig, client, 20, 800, &mut next);

    let seed = rig.seed();
    let offer = rig.hub.open_pairing(rig.now, &seed).unwrap();
    let client = RemoteClient::pair(&offer.code, "Owner iPad", &rig.seed()).unwrap();
    let pair_ok = honest_handshake_under_flood(&mut rig, client, 20, 800, &mut next);

    assert!(
        connect_ok && pair_ok,
        "a LAN host opening 800 connections/s (20 ms round trip) evicts the owner's \
         handshake: connect got in: {connect_ok}, pairing got in: {pair_ok}; matrix:\n  {}",
        table.join("\n  ")
    );
}

/// Finding: a device that paired with a glimpsed code and was never confirmed (it holds
/// no permission at all) may open every connection slot with its own key and keep them
/// with pings; authenticated connections are never evicted, so the owner's confirmed
/// controller is refused until the owner finds and removes the pending device.
#[test]
fn f2_a_pending_device_cannot_take_every_slot_from_a_confirmed_one() {
    let mut rig = Rig::new();
    let (link, owner) = rig.pair("Owner Mac");
    rig.hub.connection_closed(link.conn);
    let (first, stranger) = rig.pair_pending("Owner Mac");
    let mut held = vec![first];
    for _ in 1..MAX_CONNECTIONS {
        held.push(rig.connect(&stranger));
    }
    // It keeps them alive (a pending device may ping): the one it still holds.
    let mut alive = 0;
    for l in &mut held {
        if rig.hub.is_authenticated(l.conn) {
            alive += 1;
            let (_, msgs) = rig.request(l, ClientMessage::Ping);
            assert_eq!(msgs, vec![ServerMessage::Pong]);
        }
    }
    assert_eq!(alive, 1, "a pending device holds more than one connection");
    let (link, fx, _) = rig.try_connect(&owner);
    assert!(
        rig.hub.is_authenticated(link.conn),
        "the confirmed owner device was refused ({:?}) while an unconfirmed device holds \
         all {MAX_CONNECTIONS} slots",
        closed(&fx, link.conn)
    );
}

// ---- Store writes that fail (claim 5) ---------------------------------------------------

/// The shell as `RemoteControlService` runs it: what is on disk is the last write that
/// succeeded; a failed write is reported with `store_write_failed`.
struct Disk {
    json: String,
}

impl Disk {
    fn of(hub: &RemoteHub) -> Self {
        Self {
            json: hub.to_json().as_str().to_owned(),
        }
    }

    /// Carries out the StoreChanged in `fx` with a working store.
    fn write_ok(&mut self, hub: &RemoteHub, fx: &[HubEffect]) {
        if fx.contains(&HubEffect::StoreChanged) {
            self.json = hub.to_json().as_str().to_owned();
        }
    }
}

/// Finding: the owner removes a device, that one write fails, and nothing writes the
/// removal again: the store works again at once, the hub keeps ticking, but no
/// `StoreChanged` comes until some unrelated lease runs out. A relaunch in between
/// (the owner closes Jarvis, the phone restarts) brings the removed device back,
/// confirmed, with its key and permissions.
#[test]
fn f3_a_removal_whose_write_failed_is_written_again() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Stolen Mac");
    let mut disk = Disk::of(&rig.hub);
    let fx = rig.local(LocalEvent::DeviceRevoked {
        device_id: creds.device_id,
    });
    assert!(fx.contains(&HubEffect::StoreChanged));
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::Revoked));
    // That write fails.
    rig.hub.store_write_failed();
    // The store works again; the shell keeps ticking for a minute.
    for _ in 0..60 {
        rig.now += 1_000;
        let fx = rig.hub.tick(rig.now);
        disk.write_ok(&rig.hub, &fx);
    }
    let restored = RemoteHub::from_json("Test Phone", &disk.json).unwrap();
    let back: Vec<_> = restored
        .devices()
        .into_iter()
        .filter(|d| d.id == creds.device_id)
        .collect();
    assert!(
        back.is_empty(),
        "the removed device is back after a relaunch: {back:?}"
    );
}

/// Finding (same cause): turning a permission off is lost the same way; after a relaunch
/// the device may approve again.
#[test]
fn f4_a_permission_turned_off_whose_write_failed_is_written_again() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Owner Mac");
    let all = RemotePermissions {
        start_tasks: true,
        see_progress: true,
        see_screen_text: true,
        approve: true,
    };
    rig.set_permissions(&mut link, &creds, all);
    let mut disk = Disk::of(&rig.hub);
    let fx = rig.local(LocalEvent::PermissionsChanged {
        device_id: creds.device_id,
        permissions: RemotePermissions::NONE,
    });
    assert!(fx.contains(&HubEffect::StoreChanged));
    rig.hub.store_write_failed();
    rig.hub.connection_closed(link.conn);
    for _ in 0..60 {
        rig.now += 1_000;
        let fx = rig.hub.tick(rig.now);
        disk.write_ok(&rig.hub, &fx);
    }
    let restored = RemoteHub::from_json("Test Phone", &disk.json).unwrap();
    let perms = restored
        .devices()
        .into_iter()
        .find(|d| d.id == creds.device_id)
        .map(|d| d.permissions);
    assert_eq!(
        perms,
        Some(RemotePermissions::NONE),
        "after a relaunch the device holds the permissions the owner turned off"
    );
}

// ---- Text cleaning and the name migration (claim 4) -------------------------------------

/// Finding: a store from `adfce96` holding one device whose name was only characters the
/// earlier list kept (here U+1D173, a musical format control: the phone showed a blank
/// name) is refused as a whole on the update, so every other pairing is unusable too
/// (remote control stays off with "store problem"). The claim was that names clean under
/// the old list load cleaned instead of failing the load. Such a name is one an
/// unconfirmed device could pick.
#[test]
fn f5_an_old_store_with_a_name_that_cleans_to_nothing_still_loads_the_other_devices() {
    let json = r#"{"schema_version":2,"nonce_lease":64,"request_lease":64,"devices":[
        {"id":"00112233445566778899aabbccddeeff","name":"Owner Mac","key":"1111111111111111111111111111111111111111111111111111111111111111","permissions":{"start_tasks":true,"see_progress":true,"see_screen_text":false,"approve":false},"paired_at_ms":5},
        {"id":"ffeeddccbbaa99887766554433221100","name":"𝅳","key":"2222222222222222222222222222222222222222222222222222222222222222","permissions":{"start_tasks":false,"see_progress":false,"see_screen_text":false,"approve":false},"paired_at_ms":6,"pending_check":"123 456"}
    ]}"#;
    let loaded = RemoteHub::from_json("Test Phone", json);
    let owner_kept = loaded
        .as_ref()
        .is_ok_and(|h| h.devices().iter().any(|d| d.name == "Owner Mac"));
    assert!(
        owner_kept,
        "one pending device with a blank name makes the whole store fail: {:?}",
        loaded.err()
    );
}

/// Finding (low): characters that are not Cf but render as nothing are kept in device
/// names, so a name can still be blank on the phone's pending and device rows, or two
/// names can differ only invisibly. Hangul fillers (Lo), braille blank (So), the
/// combining grapheme joiner, variation selectors and Mongolian free variation selectors
/// (Mn).
#[test]
fn f6_names_that_render_as_nothing_are_refused() {
    let blank = [
        '\u{115F}',
        '\u{1160}',
        '\u{3164}',
        '\u{FFA0}',
        '\u{2800}',
        '\u{034F}',
        '\u{FE0F}',
        '\u{E0100}',
        '\u{180B}',
        '\u{17B4}',
    ];
    let kept: Vec<String> = blank
        .iter()
        .filter(|c| !clean_name(&c.to_string()).is_empty())
        .map(|c| format!("U+{:04X}", u32::from(*c)))
        .collect();
    assert!(
        kept.is_empty(),
        "names of only these invisible characters survive cleaning: {kept:?}"
    );
}

// ---- The connect lockout (claim 1) ------------------------------------------------------

/// Sound: a flood of forged connect requests against a paired device's (clear-text) id
/// never keeps its key out, never grows the lockout once armed, and costs the phone a
/// bounded amount per request. Measured here (debug build): the time per connection
/// opened (hello: one X25519 key), per forged request for a known id (HKDF + HMAC) and
/// for an unknown id (no MAC).
#[test]
fn sound_ten_thousand_forgeries_never_keep_the_key_out() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Owner Mac");
    rig.hub.connection_closed(link.conn);
    let bad = ClientCredentials {
        device_id: creds.device_id,
        device_key: Secret32::from_bytes([9; 32]),
    };
    let unknown = ClientCredentials {
        device_id: jarvis_remote::DeviceId([0x42; 16]),
        device_key: Secret32::from_bytes([9; 32]),
    };
    let mut until = 0;
    let n = 10_000u32;
    let mut opened_ns = 0u128;
    let mut forged_ns = 0u128;
    let mut unknown_ns = 0u128;
    for i in 0..n {
        for (who, sink) in [(&bad, &mut forged_ns), (&unknown, &mut unknown_ns)] {
            let conn = rig.conn_id();
            let seed = rig.seed();
            let t = Instant::now();
            let fx = rig.hub.connection_opened(conn, rig.now, &seed);
            opened_ns += t.elapsed().as_nanos();
            let mut client = RemoteClient::connect(who.clone(), &rig.seed()).unwrap();
            let events = client.receive(&sends(&fx, conn)).unwrap();
            for ev in events {
                if let ClientEvent::Send { bytes } = ev {
                    let t = Instant::now();
                    let fx = rig.hub.receive(conn, &bytes, rig.now);
                    *sink += t.elapsed().as_nanos();
                    assert!(closed(&fx, conn).is_some());
                }
            }
        }
        if i == LOCKOUT_THRESHOLD {
            until = rig.hub.device_locked_until_ms(creds.device_id);
        }
        // Forgeries inside the armed lockout do not grow it (the owner's next connect
        // clears it, at i = 999).
        if i > LOCKOUT_THRESHOLD && i < 999 && rig.now < until {
            assert_eq!(rig.hub.device_locked_until_ms(creds.device_id), until);
        }
        // Every 1000 forgeries (the first time inside the lockout), the owner connects.
        if i % 1_000 == 999 {
            assert!(i > 999 || rig.hub.device_locked_until_ms(creds.device_id) > rig.now);
            let mut owner = rig.connect(&creds);
            let (_, msgs) = rig.request(&mut owner, ClientMessage::Ping);
            assert_eq!(msgs, vec![ServerMessage::Pong]);
            rig.hub.connection_closed(owner.conn);
        }
        rig.now += 1;
    }
    let per = |ns: u128, k: u32| ns / u128::from(k);
    eprintln!(
        "per connection opened: {} ns; per forged request (known id): {} ns; per request \
         for an unknown id: {} ns",
        per(opened_ns, 2 * n),
        per(forged_ns, n),
        per(unknown_ns, n)
    );
}
