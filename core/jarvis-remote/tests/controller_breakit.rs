//! Break-it tests for the controller screen (Shell commit a129f14), at the core level:
//! the controller's `RemoteClient` facing a rogue phone that holds the device key (a
//! compromised phone, or anyone who got that key), and the phone facing a network attacker
//! on the local Wi-Fi. Tests named `finding_…` fail for the reason in their name; tests
//! named `sound_…` pin a property that was found to hold.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::collapsible_match,
    clippy::manual_is_multiple_of
)]

mod common;

use common::*;
use jarvis_proto::{OutcomeStatus, RiskCategory};
use jarvis_remote::codec::{encode_record, Kind};
use jarvis_remote::crypto::{
    auth_key, connect_phone_mac, pair_client_mac, pair_key, Channel, Ephemeral, Side, Transcript,
};
use jarvis_remote::{
    ClientCredentials, ClientEvent, ClientMessage, CloseReason, HubEffect, LocalEvent,
    RemoteClient, RemoteHub, RemotePermissions, ServerMessage, LOCKOUT_THRESHOLD,
    MAX_DEVICE_NAME_BYTES, MAX_SUMMARY_CHARS, PAIRING_TTL_MS,
};

/// A phone that knows a device's key and speaks the protocol itself: it can send any
/// `ServerMessage` (or any bytes) the real hub would never send.
struct RoguePhone {
    channel: Channel,
}

impl RoguePhone {
    /// Runs the connect handshake with `client` (which must be fresh) and returns the
    /// rogue phone and the client, authenticated.
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
        // device_id (16) || nc (32) || er (32) || mac (32), after the 6-byte header.
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

    fn frame(&mut self, plaintext: &[u8]) -> Vec<u8> {
        encode_record(Kind::Data, &self.channel.seal(plaintext).unwrap())
    }

    fn message(&mut self, msg: &ServerMessage) -> Vec<u8> {
        let bytes = msg.to_bytes().unwrap();
        self.frame(&bytes)
    }
}

fn paired_creds() -> (Rig, ClientCredentials) {
    let mut rig = Rig::new();
    let (_link, creds) = rig.pair("Controller");
    (rig, creds)
}

fn has_control_or_bidi(s: &str) -> bool {
    s.chars().any(|c| {
        c.is_control() || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
    })
}

// ---- A rogue phone against the controller ---------------------------------------------

#[test]
fn finding_rogue_phone_strings_reach_the_controller_unsanitized_and_unbounded() {
    let (_rig, creds) = paired_creds();
    let (mut phone, mut client) = RoguePhone::connect(&creds, 100);
    // A name that imitates the controller's own lines, with a newline and a bidi
    // override, and far longer than any name the phone may have (64 bytes).
    let spoof = format!(
        "Answer on the phone\nThis phone shares nothing with this device.\u{202E}enohp{}",
        "x".repeat(60_000)
    );
    let welcome = ServerMessage::Welcome {
        protocol_version: jarvis_remote::PROTOCOL_VERSION,
        permissions: RemotePermissions {
            start_tasks: true,
            see_progress: true,
            see_screen_text: false,
            approve: false,
        },
        phone_name: Some(spoof.clone()),
        busy: Some(false),
    };
    let finished = ServerMessage::Finished {
        status: OutcomeStatus::Success,
        summary: format!("Approved from this device\n\u{202E}{}", "y".repeat(60_000)),
    };
    let mut bytes = phone.message(&welcome);
    bytes.extend(phone.message(&finished));
    let got = messages(&client.receive(&bytes).unwrap());
    let mut problems = Vec::new();
    for m in &got {
        match m {
            ServerMessage::Welcome {
                phone_name: Some(n),
                ..
            } => {
                if n.len() > MAX_DEVICE_NAME_BYTES || has_control_or_bidi(n) {
                    problems.push(format!(
                        "phone name of {} bytes, controls/bidi: {}",
                        n.len(),
                        has_control_or_bidi(n)
                    ));
                }
            }
            ServerMessage::Finished { summary, .. } => {
                if summary.chars().count() > MAX_SUMMARY_CHARS || has_control_or_bidi(summary) {
                    problems.push(format!(
                        "summary of {} chars, controls/bidi: {}",
                        summary.chars().count(),
                        has_control_or_bidi(summary)
                    ));
                }
            }
            _ => {}
        }
    }
    assert_eq!(got.len(), 2, "{got:?}");
    // The controller shows the name on its status card and in the stored pairing (the
    // Keychain), and puts summaries into its timeline as "Done: <summary>" verbatim.
    assert!(
        problems.is_empty(),
        "the client passes the phone's text through: {problems:?}"
    );
}

#[test]
fn finding_rogue_phone_offers_remote_approval_of_a_never_remote_category() {
    let (_rig, creds) = paired_creds();
    let (mut phone, mut client) = RoguePhone::connect(&creds, 200);
    let bytes = phone.message(&ServerMessage::ConfirmPending {
        request_id: 1,
        category: RiskCategory::Payment,
        summary: "Pay $5".into(),
        can_approve: true,
    });
    let got = messages(&client.receive(&bytes).unwrap());
    // The controller model takes `can_approve` as given and shows Approve for a payment.
    // Defence in depth: the client could clamp it with `remotely_approvable`.
    let offered = got.iter().any(|m| {
        matches!(m, ServerMessage::ConfirmPending { category, can_approve: true, .. }
            if !jarvis_remote::remotely_approvable(*category))
    });
    assert!(
        !offered,
        "a payment arrives at the controller as approvable: {got:?}"
    );
}

#[test]
fn sound_deeply_nested_json_from_the_phone_does_not_overflow_a_small_stack() {
    let (_rig, creds) = paired_creds();
    // iOS secondary threads have 512 KiB of stack; the main thread has more.
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || {
            let (mut phone, mut client) = RoguePhone::connect(&creds, 300);
            let depth = 30_000;
            let mut nested = String::from("{\"type\":\"welcome\",\"x\":");
            nested.push_str(&"[".repeat(depth));
            nested.push_str(&"]".repeat(depth));
            nested.push('}');
            let bytes = phone.frame(nested.as_bytes());
            let events = client.receive(&bytes).unwrap();
            assert!(
                events
                    .iter()
                    .all(|e| matches!(e, ClientEvent::Undecodable(_))),
                "{events:?}"
            );
            // The connection survives an undecodable message.
            let ok = phone.message(&ServerMessage::Pong);
            assert_eq!(
                messages(&client.receive(&ok).unwrap()),
                vec![ServerMessage::Pong]
            );
        })
        .unwrap();
    handle.join().expect("the client crashed on nested JSON");
}

#[test]
fn sound_viewer_delta_bombs_do_not_overflow_or_grow_the_controller() {
    use jarvis_viewer::delta::DeltaDecoder;
    use jarvis_viewer::wire::{ViewerTimeline, MAX_STEPS};
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let mut decoder = DeltaDecoder::new();
            let mut timeline = ViewerTimeline::default();
            // Deep nesting inside a delta (within the 48 000-byte limit).
            let depth = 20_000;
            let nested = format!(
                "{{\"v\":1,\"gen\":1,\"seq\":0,\"kind\":\"reset\",\"x\":{}{}}}",
                "[".repeat(depth),
                "]".repeat(depth)
            );
            let _ = decoder.apply(&mut timeline, nested.as_bytes());
            // Endless step_added deltas: the timeline stays capped.
            let reset = br#"{"v":1,"gen":1,"seq":0,"kind":"reset","task_id":"t"}"#;
            let _ = decoder.apply(&mut timeline, reset);
            let mut accepted = 0;
            for n in 1..2_000u32 {
                let d = format!(
                    r#"{{"v":1,"gen":1,"seq":{n},"kind":"step_added","step":{{"number":{n},"gesture":{{"kind":"wait","ms":1}},"caption":"c","state":{{"kind":"done"}}}}}}"#
                );
                let before = timeline.steps.last().map(|s| s.number);
                let _ = decoder.apply(&mut timeline, d.as_bytes());
                if timeline.steps.last().map(|s| s.number) != before {
                    accepted += 1;
                }
            }
            assert!(accepted > MAX_STEPS, "the flood was not applied ({accepted}): the test does not test the cap");
            assert!(timeline.steps.len() <= MAX_STEPS, "{}", timeline.steps.len());
        })
        .unwrap();
    handle
        .join()
        .expect("the viewer decoder crashed on a delta bomb");
}

#[test]
fn sound_a_rogue_phone_cannot_make_the_client_send_anything_by_itself() {
    // Whatever the phone sends, the client's only automatic outputs are a resync request
    // for the Live view and the handshake: it never answers a question or starts a task.
    let (_rig, creds) = paired_creds();
    let (mut phone, mut client) = RoguePhone::connect(&creds, 400);
    let mut bytes = Vec::new();
    for m in [
        ServerMessage::ConfirmPending {
            request_id: 9,
            category: RiskCategory::FirstUse,
            summary: "Approve?".into(),
            can_approve: true,
        },
        ServerMessage::TaskStarted,
        ServerMessage::Finished {
            status: OutcomeStatus::Success,
            summary: "x".into(),
        },
    ] {
        bytes.extend(phone.message(&m));
    }
    let events = client.receive(&bytes).unwrap();
    assert!(
        !events.iter().any(|e| matches!(e, ClientEvent::Send { .. })),
        "{events:?}"
    );
}

// ---- The phone after a restart ------------------------------------------------------

#[test]
fn finding_request_ids_restart_after_a_phone_restart_so_a_stale_answer_hits_a_new_question() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Controller");
    rig.set_permissions(
        &mut link,
        &creds,
        RemotePermissions {
            start_tasks: true,
            see_progress: true,
            see_screen_text: true,
            approve: true,
        },
    );
    rig.request(&mut link, ClientMessage::Subscribe);
    let shown = rig.local_to(
        &mut link,
        confirm_event(RiskCategory::FirstUse, "Let Jarvis act in Notes?"),
    );
    let old_id = shown
        .iter()
        .find_map(|m| match m {
            ServerMessage::ConfirmPending { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .expect("the question");
    // Jarvis is closed on the phone and opened again: the hub is rebuilt from its store.
    // The controller (RemoteControllerModel) keeps showing the old card across the
    // reconnect (see ControllerBreakitTests.findingStaleQuestionSurvivesReconnect).
    let json = rig.hub.to_json();
    let mut rig = Rig::with_hub(RemoteHub::from_json("Test Phone", &json).unwrap());
    let mut link = rig.connect(&creds);
    rig.request(&mut link, ClientMessage::Subscribe);
    let shown = rig.local_to(
        &mut link,
        confirm_event(RiskCategory::FirstUse, "Let Jarvis act in Mail?"),
    );
    let new_id = shown
        .iter()
        .find_map(|m| match m {
            ServerMessage::ConfirmPending { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .expect("the new question");
    // The stale card's answer, sent with the old id, decides the NEW question.
    let (fx, _) = rig.request(
        &mut link,
        ClientMessage::Decision {
            request_id: old_id,
            approved: true,
        },
    );
    let decided = fx
        .iter()
        .any(|e| matches!(e, HubEffect::Decide { approved: true, .. }));
    assert!(
        new_id != old_id && !decided,
        "request id {old_id} was reused after the restart; the old answer approved the new question"
    );
}

// ---- A network attacker against pairing ---------------------------------------------

/// A pair request with a wrong code (a correct format, a MAC under a random key).
fn bad_pair_request(hello: &[u8], seed: u64) -> Vec<u8> {
    let mine = Ephemeral::remote(&entropy(seed)).unwrap();
    let t = Transcript {
        np: hello[6..38].try_into().unwrap(),
        ep: hello[38..70].try_into().unwrap(),
        nc: mine.nonce,
        er: mine.public,
    };
    let wrong = jarvis_remote::parse_code(&"A".repeat(32)).unwrap();
    let mac = pair_client_mac(&pair_key(&wrong).unwrap(), &t, "x").unwrap();
    let mut body = Vec::new();
    body.extend_from_slice(&mine.nonce);
    body.extend_from_slice(&mine.public);
    body.push(1);
    body.push(b'x');
    body.extend_from_slice(&mac);
    encode_record(Kind::PairRequest, &body)
}

#[test]
fn finding_a_codeless_lan_attacker_keeps_the_owner_from_pairing_for_the_whole_offer() {
    let mut rig = Rig::new();
    let start = rig.now;
    let seed = rig.seed();
    let offer = rig.hub.open_pairing(rig.now, &seed).unwrap();
    let mut owner_paired = false;
    let mut attacker_seed = 10_000;
    let mut locked_until = 0;
    // Every second for the offer's lifetime: the attacker (no code, no device) sends a
    // wrong pair request whenever the pairing is not locked; the owner tries the right
    // code every 20 s.
    while rig.now < start + PAIRING_TTL_MS - 1 {
        if rig.now >= locked_until {
            for _ in 0..LOCKOUT_THRESHOLD {
                let conn = rig.conn_id();
                let fx = rig
                    .hub
                    .connection_opened(conn, rig.now, &entropy(attacker_seed));
                attacker_seed += 1;
                let hello = sends(&fx, conn);
                let fx = rig
                    .hub
                    .receive(conn, &bad_pair_request(&hello, attacker_seed), rig.now);
                attacker_seed += 1;
                if closed(&fx, conn) == Some(CloseReason::LockedOut) {
                    break;
                }
                if rig.hub.pairing_locked_until_ms() > rig.now {
                    break;
                }
            }
            locked_until = rig.hub.pairing_locked_until_ms();
        }
        if (rig.now - start) % 20_000 == 0 {
            let client = RemoteClient::pair(&offer.code, "Owner", &rig.seed()).unwrap();
            let (_l, _fx, events) = rig.open(client);
            if events
                .map(|e| e.contains(&ClientEvent::Connected))
                .unwrap_or(false)
            {
                owner_paired = true;
                break;
            }
        }
        rig.now += 1_000;
        let _ = rig.hub.tick(rig.now);
    }
    assert!(
        owner_paired,
        "a LAN host without the code kept pairing locked for the whole 5-minute offer (lockout until {locked_until})"
    );
}

#[test]
fn sound_the_code_never_crosses_the_wire_and_a_relay_without_it_learns_no_key() {
    // A pairing request carries the remote's nonce, ephemeral key, name and a MAC keyed by
    // HKDF(code): no byte of the code. The phone's accept is a MAC too. An attacker that
    // relays both directions cannot derive the device key (it needs the code and one of
    // the X25519 secrets).
    let mut rig = Rig::new();
    let seed = rig.seed();
    let offer = rig.hub.open_pairing(rig.now, &seed).unwrap();
    let raw: String = offer.code.chars().filter(|c| *c != '-').collect();
    let secret = jarvis_remote::parse_code(&offer.code).unwrap();
    let client = RemoteClient::pair(&offer.code, "Owner", &rig.seed()).unwrap();
    let (link, fx, events) = rig.open(client);
    let events = events.unwrap();
    assert!(events.contains(&ClientEvent::Connected));
    let mut wire: Vec<u8> = sends(&fx, link.conn);
    for s in &link.sent {
        wire.extend_from_slice(s);
    }
    let hay = String::from_utf8_lossy(&wire).to_string();
    assert!(!hay.contains(&raw[..8]));
    let secret_bytes = secret.as_bytes();
    assert!(!wire.windows(8).any(|w| w == &secret_bytes[..8]));
    let _ = LocalEvent::ConfirmResolved {
        request_id: 0,
        approved: false,
    };
}
