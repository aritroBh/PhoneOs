//! Break-it tests for the confirm step and the controller review fixes (`adfce96`).
//! Tests named `f<n>_…` assert what should hold and fail today for the reason in their
//! name; tests named `sound_…` pin a property that was attacked and found to hold.

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
use jarvis_remote::crypto::{
    auth_key, confirm_commitment, connect_phone_mac, Channel, Ephemeral, Side, Transcript,
};
use jarvis_remote::{
    ClientCredentials, ClientEvent, ClientMessage, CloseReason, DenyReason, HubEffect, HubError,
    RemoteClient, RemoteHub, ServerMessage, MAX_CONNECTIONS, PAIRING_TTL_MS,
    PENDING_CONFIRM_TTL_MS,
};

// ---- helpers -------------------------------------------------------------------------

/// A ConnectRequest for `device` with a MAC nobody can verify: what any host on the
/// Wi-Fi can send once it has seen the device id (it travels in clear, in every
/// ConnectRequest and in the PairAccept).
fn forged_connect(rig: &mut Rig, device: &ClientCredentials, salt: u64) -> Vec<HubEffect> {
    let conn = rig.conn_id();
    let seed = rig.seed();
    let mut fx = rig.hub.connection_opened(conn, rig.now, &seed);
    let mut body = Vec::with_capacity(16 + 32 + 32 + 32);
    body.extend_from_slice(&device.device_id.0);
    body.extend_from_slice(&entropy(salt));
    body.extend_from_slice(&entropy(salt ^ 0x5555));
    body.extend_from_slice(&[0u8; 32]);
    fx.extend(
        rig.hub
            .receive(conn, &encode_record(Kind::ConnectRequest, &body), rig.now),
    );
    fx
}

/// The distinct values, in first-seen order.
fn distinct<T: PartialEq + Clone>(all: &[T]) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();
    for v in all {
        if !out.contains(v) {
            out.push(v.clone());
        }
    }
    out
}

/// The payload of the first record in `bytes`.
fn first_payload(bytes: &[u8]) -> Vec<u8> {
    let mut reader = RecordReader::new();
    reader.push(bytes).unwrap();
    reader.next_record().unwrap().unwrap().payload
}

/// A phone that holds a device's key and speaks the protocol itself (a fake phone: a
/// man in the middle with the code, or a compromised phone).
struct RoguePhone {
    channel: Channel,
}

impl RoguePhone {
    fn connect(creds: &ClientCredentials, seed: u64) -> (RoguePhone, RemoteClient) {
        let mut client = RemoteClient::connect(creds.clone(), &entropy(seed)).unwrap();
        let eph = Ephemeral::phone(&entropy(seed + 1), 7, 0).unwrap();
        let mut hello = Vec::new();
        hello.extend_from_slice(&eph.nonce);
        hello.extend_from_slice(&eph.public);
        let events = client
            .receive(&encode_record(Kind::ServerHello, &hello))
            .unwrap();
        let ClientEvent::Send { bytes: request } = events[0].clone() else {
            panic!("{events:?}")
        };
        let t = Transcript {
            np: eph.nonce,
            ep: eph.public,
            nc: request[22..54].try_into().unwrap(),
            er: request[54..86].try_into().unwrap(),
        };
        let ak = auth_key(&creds.device_key).unwrap();
        let mac = connect_phone_mac(&ak, &creds.device_id, &t).unwrap();
        let dh = eph.agree(&t.er).unwrap();
        let channel =
            Channel::new(Side::Phone, &creds.device_key, &dh, creds.device_id, &t).unwrap();
        let events = client
            .receive(&encode_record(Kind::ConnectAccept, &mac))
            .unwrap();
        assert!(events.contains(&ClientEvent::Connected), "{events:?}");
        (RoguePhone { channel }, client)
    }

    fn message(&mut self, msg: &ServerMessage) -> Vec<u8> {
        let bytes = msg.to_bytes().unwrap();
        encode_record(Kind::Data, &self.channel.seal(&bytes).unwrap())
    }
}

// ---- findings ------------------------------------------------------------------------

/// F1. The connect lockout is per device id, and the id travels in clear. A host on the
/// Wi-Fi without any key sends one forged ConnectRequest whenever the lockout ends: the
/// lockout doubles to 10 minutes and is re-armed for ever, so the paired controller is
/// refused on every attempt (and stops retrying after its 10 slow retries). The key is
/// 256 bits: the lockout protects nothing and is a weapon. The pairing side was fixed
/// for exactly this reason (per-offer budget); the connect side was not.
#[test]
fn f1_a_codeless_lan_host_keeps_a_paired_controller_out_indefinitely() {
    let mut rig = Rig::new();
    let (link, creds) = rig.pair("Owner Mac");
    rig.hub.connection_closed(link.conn);
    let start = rig.now;
    let mut salt = 1_000u64;
    let mut successes = 0u32;
    let mut refused = Vec::new();
    // The attacker probes once a second (a probe while locked is not even counted); the
    // owner's controller tries once a minute, from one minute in, for 30 minutes.
    let end = start + 31 * 60_000;
    while rig.now < end {
        salt += 1;
        let _ = forged_connect(&mut rig, &creds, salt);
        rig.now += 500;
        if rig.now >= start + 60_000 && (rig.now - start) % 60_000 == 500 {
            let (l, fx, ev) = rig.try_connect(&creds);
            if ev
                .as_ref()
                .is_ok_and(|e| e.contains(&ClientEvent::Connected))
            {
                successes += 1;
                rig.hub.connection_closed(l.conn);
            } else {
                refused.push(closed(&fx, l.conn));
            }
        }
        rig.now += 500;
    }
    assert!(
        successes > 0,
        "the owner's device never got in during 30 minutes: {} attempts refused \
         (reasons {:?}), device locked until +{} ms",
        refused.len(),
        distinct(&refused),
        rig.hub
            .device_locked_until_ms(creds.device_id)
            .saturating_sub(rig.now)
    );
}

/// F2. A host on the Wi-Fi without the code fills the hub's connection table with
/// handshakes that never finish (each lives 10 s; it opens a new one as one times out).
/// The owner's pairing is refused `TooManyConnections` for the whole offer (and the
/// same holds for every connect). Nothing about it needs the code or a key.
#[test]
fn f2_a_codeless_lan_host_holds_every_connection_slot_and_blocks_pairing() {
    let mut rig = Rig::new();
    let start = rig.now;
    let offer = rig.hub.open_pairing(rig.now, &entropy(77)).unwrap();
    let mut paired = false;
    let mut reasons = Vec::new();
    while rig.now < start + PAIRING_TTL_MS - 1_000 && !paired {
        // Whole seconds: the attacker tops the table up again.
        rig.hub.tick(rig.now);
        while rig.hub.connection_count() < MAX_CONNECTIONS {
            let conn = rig.conn_id();
            let seed = rig.seed();
            rig.hub.connection_opened(conn, rig.now, &seed);
        }
        rig.now += 500;
        // Every 15 s the owner's controller tries the code shown on the phone.
        if (rig.now - start) % 15_000 == 500 {
            let seed = rig.seed();
            let client = RemoteClient::pair(&offer.code, "Owner Mac", &seed).unwrap();
            let (l, fx, _) = rig.open(client);
            paired = fx
                .iter()
                .any(|e| matches!(e, HubEffect::DevicePaired { .. }));
            reasons.push(closed(&fx, l.conn));
        }
        rig.now += 500;
    }
    assert!(
        paired,
        "the owner could not pair during the whole offer: {} attempts, closed with {:?}",
        reasons.len(),
        distinct(&reasons)
    );
}

/// F3. A pending device gets a fresh 10 minutes at every restore of the hub, although
/// the hub stored when it was paired and the shell's clock is the wall clock
/// (`RemoteControlService` passes `Date()`). Jarvis relaunching on the phone (iOS ends
/// apps in the background) keeps a device paired with a glimpsed code waiting for a
/// Confirm far beyond the promised 10 minutes.
#[test]
fn f3_a_pending_device_outlives_its_ten_minutes_across_a_restart() {
    let mut rig = Rig::new();
    let (_link, creds) = rig.pair_pending("Glimpsed");
    let paired_at = rig.hub.pending_devices()[0].device.paired_at_ms;
    let json = rig.hub.to_json();
    // The phone restarts Jarvis 11 minutes after the pairing (same wall clock).
    let later = paired_at + PENDING_CONFIRM_TTL_MS + 60_000;
    let mut hub = RemoteHub::from_json("Test Phone", &json).unwrap();
    hub.tick(later);
    let left: Vec<_> = hub
        .pending_devices()
        .iter()
        .map(|p| (p.device.name.clone(), p.expires_at_ms))
        .collect();
    let confirm = hub.confirm_device(creds.device_id, later).map(|_| ());
    assert!(
        left.is_empty() && confirm == Err(HubError::UnknownDevice),
        "paired at {paired_at}, now {later} (+{} min): still pending {left:?}, confirm {confirm:?}",
        (later - paired_at) / 60_000
    );
}

/// F4. The text the controller shows is cleaned of "zero-width, bidi and similar
/// invisible format characters", but `is_format` is a hand list that misses Unicode
/// format (Cf) characters that render as nothing: musical formatting U+1D173..U+1D17A,
/// shorthand format controls U+1BCA0..U+1BCA3, Egyptian format controls U+13430... A
/// fake phone (or a phone name) can hide characters in what the owner reads.
#[test]
fn f4_invisible_format_characters_survive_the_clients_cleaning() {
    let mut rig = Rig::new();
    let (_link, creds) = rig.pair("Owner Mac");
    let (mut phone, mut client) = RoguePhone::connect(&creds, 500);
    let hidden = [
        '\u{1D173}',
        '\u{1D17A}',
        '\u{1BCA0}',
        '\u{1BCA3}',
        '\u{13430}',
    ];
    let summary: String = format!("Do{}ne", hidden.iter().collect::<String>());
    let bytes = phone.message(&ServerMessage::Finished {
        status: OutcomeStatus::Success,
        summary,
    });
    let got = messages(&client.receive(&bytes).unwrap());
    let ServerMessage::Finished { summary, .. } = &got[0] else {
        panic!("{got:?}")
    };
    let kept: Vec<String> = summary
        .chars()
        .filter(|c| hidden.contains(c))
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect();
    assert!(
        kept.is_empty(),
        "the summary shown is {summary:?} ({} chars for \"Done\"), invisible kept: {kept:?}",
        summary.chars().count()
    );
    // The same list cleans device names on the phone: two pending rows can differ only
    // by invisible characters.
    let name = jarvis_remote::clean_name("Owner\u{1D173} Mac");
    assert_eq!(name, "Owner Mac", "cleaned name {name:?}");
}

/// F5. The request-id lease saturates: a store whose lease is at (or near) `u64::MAX`
/// loads, and then every question gets the same id, so an answer meant for one can be
/// applied to the next. A store like that should be refused, or the ids should not
/// repeat.
#[test]
fn f5_request_ids_repeat_once_the_lease_saturates() {
    let json = format!(
        r#"{{"schema_version":2,"nonce_lease":0,"request_lease":{},"devices":[]}}"#,
        u64::MAX
    );
    let Ok(hub) = RemoteHub::from_json("Test Phone", &json) else {
        return; // Refused: what should happen.
    };
    let mut rig = Rig::with_hub(hub);
    rig.local(confirm_event(RiskCategory::FirstUse, "First?"));
    let first = rig.hub.pending_request_id().unwrap();
    rig.local(jarvis_remote::LocalEvent::ConfirmResolved {
        request_id: first,
        approved: false,
    });
    rig.local(confirm_event(RiskCategory::FirstUse, "Second?"));
    let second = rig.hub.pending_request_id().unwrap();
    assert_ne!(first, second, "two questions share request id {first}");
}

// ---- sound ---------------------------------------------------------------------------

/// The phone's confirmation value is fresh for every connection of one offer, and it is
/// revealed only to a peer that proved the code: an attacker cannot learn it on one
/// handshake and grind its own values on the next.
#[test]
fn sound_the_phone_commitment_is_fresh_per_connection_and_revealed_only_after_the_code() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, &entropy(31)).unwrap();
    let mut commitments = Vec::new();
    for conn in 10..14 {
        let hello = first_payload(&sends(
            &rig.hub.connection_opened(conn, rig.now, &entropy(conn)),
            conn,
        ));
        assert_eq!(hello.len(), 96);
        commitments.push(hello[64..].to_vec());
    }
    commitments.sort();
    commitments.dedup();
    assert_eq!(
        commitments.len(),
        4,
        "a commitment repeated within one offer"
    );
    // A wrong code on connection 10: no PairAccept, nothing revealed.
    let wrong = jarvis_remote::format_code(&jarvis_remote::PairingSecret::from_bytes([9; 20]));
    let mut bad = RemoteClient::pair(&wrong, "x", &entropy(900)).unwrap();
    let hello = sends(&rig.hub.connection_opened(20, rig.now, &entropy(901)), 20);
    let ev = bad.receive(&hello).unwrap();
    let ClientEvent::Send { bytes } = &ev[0] else {
        panic!("{ev:?}")
    };
    let fx = rig.hub.receive(20, bytes, rig.now);
    assert!(sends(&fx, 20).is_empty(), "something was sent: {fx:?}");
    assert_eq!(closed(&fx, 20), Some(CloseReason::AuthFailed));
    // The right code on another connection: the value revealed is the one committed to
    // in that connection's hello.
    let mut good = RemoteClient::pair(&offer.code, "Owner Mac", &entropy(902)).unwrap();
    let hello = sends(&rig.hub.connection_opened(21, rig.now, &entropy(903)), 21);
    let commitment: [u8; 32] = first_payload(&hello)[64..].try_into().unwrap();
    let ev = good.receive(&hello).unwrap();
    let ClientEvent::Send { bytes } = &ev[0] else {
        panic!("{ev:?}")
    };
    let accept = first_payload(&sends(&rig.hub.receive(21, bytes, rig.now), 21));
    let confirm: [u8; 32] = accept[48..80].try_into().unwrap();
    assert_eq!(confirm_commitment(&confirm), commitment);
}

/// A pending device's replies say nothing about the phone: the same bytes whether a task
/// runs and a question waits or the phone is idle (Cancel does not say "not running",
/// a decision does not say "stale").
#[test]
fn sound_a_pending_device_gets_the_same_replies_idle_and_busy() {
    let replies = |busy: bool| {
        let mut rig = Rig::new();
        let (mut link, _creds) = rig.pair_pending("Glimpsed");
        if busy {
            rig.local(started(None));
            rig.local(confirm_event(RiskCategory::SendMessage, "Send it?"));
        }
        let mut out = Vec::new();
        for msg in [
            ClientMessage::Cancel,
            ClientMessage::StartTask { text: "x".into() },
            ClientMessage::Decision {
                request_id: 1,
                approved: false,
            },
            ClientMessage::Decision {
                request_id: 1,
                approved: true,
            },
            ClientMessage::Hello,
        ] {
            rig.now += 100;
            let (fx, msgs) = rig.request(&mut link, msg);
            let sizes: Vec<usize> = fx
                .iter()
                .filter_map(|e| match e {
                    HubEffect::Send { bytes, .. } => Some(bytes.len()),
                    _ => None,
                })
                .collect();
            out.push((msgs, sizes));
        }
        out
    };
    let idle = replies(false);
    let busy = replies(true);
    assert_eq!(idle, busy);
    assert_eq!(
        idle[0].0,
        vec![ServerMessage::Denied {
            reason: DenyReason::NotPermitted
        }]
    );
}

/// A forged store that marks a pending device as allowed to watch the screen: the
/// permission is not loaded, and confirming the device does not turn it on.
#[test]
fn sound_a_pending_record_cannot_carry_the_watch_permission_into_confirmation() {
    let mut rig = Rig::new();
    let (_link, creds) = rig.pair_pending("Glimpsed");
    let json = rig.hub.to_json();
    let forged = json.replace(
        "\"pending_check\"",
        "\"watch_screen\":true,\"pending_check\"",
    );
    assert_ne!(forged.as_str(), json.as_str());
    let mut hub = RemoteHub::from_json("Test Phone", &forged).unwrap();
    assert!(!hub.watch_screen(creds.device_id));
    hub.confirm_device(creds.device_id, rig.now).unwrap();
    assert!(!hub.watch_screen(creds.device_id));
}

/// Every truncation of a store holding a pending device is refused (no panic), and none
/// loads that device as confirmed.
#[test]
fn sound_a_truncated_store_never_loads_a_pending_device_as_confirmed() {
    let mut rig = Rig::new();
    rig.pair("Owner Mac");
    let (_link, creds) = rig.pair_pending("Glimpsed");
    let json = rig.hub.to_json();
    for cut in 0..json.len() {
        let Some(prefix) = json.get(..cut) else {
            continue;
        };
        if let Ok(hub) = RemoteHub::from_json("Test Phone", prefix) {
            assert!(
                hub.devices().iter().all(|d| d.id != creds.device_id),
                "cut at {cut} loads the pending device as confirmed"
            );
        }
    }
}
