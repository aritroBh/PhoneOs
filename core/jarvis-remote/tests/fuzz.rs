//! Deterministic pseudo-random fuzzing: no input may panic the hub or the client, and the
//! hub's limits hold throughout. A tiny xorshift generator keeps it reproducible.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::{Action, OutcomeStatus, RiskCategory};
use jarvis_remote::{
    codec::{encode_record, Kind},
    CatalogEntry, ClientCredentials, ClientEvent, ClientMessage, DeviceStore, HubEffect,
    LocalEvent, RemoteClient, RemoteHub, RemotePermissions, ServerMessage, MAX_CONNECTIONS,
    MAX_DEVICES, MAX_RECORD_LEN,
};

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
    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }
    fn bytes(&mut self, max: u64) -> Vec<u8> {
        let n = self.below(max + 1);
        (0..n).map(|_| self.next() as u8).collect()
    }
    fn entropy(&mut self) -> [u8; 32] {
        core::array::from_fn(|_| self.next() as u8)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

fn random_message(rng: &mut Rng, pending: Option<u64>) -> ClientMessage {
    match rng.below(11) {
        0 => ClientMessage::Hello,
        1 => ClientMessage::StartTask {
            text: String::from_utf8_lossy(&rng.bytes(40)).into_owned(),
        },
        2 => ClientMessage::RunSkill {
            id: rng.pick(&["timer", "nope", ""]).to_string(),
            args: Default::default(),
        },
        3 => ClientMessage::RunWorkflow {
            id: rng.pick(&["morning", "x"]).to_string(),
        },
        4 => ClientMessage::Cancel,
        5 => ClientMessage::Decision {
            request_id: if rng.chance(2) {
                pending.unwrap_or(0)
            } else {
                rng.next()
            },
            approved: rng.chance(2),
        },
        6 => ClientMessage::Subscribe,
        7 => ClientMessage::Unsubscribe,
        8 => ClientMessage::ListSkills,
        _ => ClientMessage::Ping,
    }
}

fn random_local(
    rng: &mut Rng,
    device: Option<ClientCredentials>,
    task: Option<u64>,
    pending: Option<u64>,
) -> LocalEvent {
    let task_id = if rng.chance(3) {
        None
    } else {
        task.or(Some(rng.next()))
    };
    let cats = RiskCategory::ALL;
    match rng.below(9) {
        0 => started(task_id),
        1 => progress_event(
            rng.below(50) as u32,
            Action::Type {
                element: None,
                text: "hunter2".into(),
                submit: false,
            },
            Some(&String::from_utf8_lossy(&rng.bytes(300))),
        ),
        2 => confirm_event(*rng.pick(&cats), "Tap \"Send\""),
        3 => LocalEvent::ConfirmResolved {
            request_id: if rng.chance(4) {
                rng.next()
            } else {
                pending.unwrap_or(0)
            },
            approved: rng.chance(2),
        },
        4 => finished(
            task_id,
            *rng.pick(&[
                OutcomeStatus::Success,
                OutcomeStatus::Failed,
                OutcomeStatus::Cancelled,
                OutcomeStatus::Blocked,
                OutcomeStatus::Interrupted,
            ]),
            "done 4111 1111 1111 1111",
        ),
        5 => match device {
            Some(d) if rng.chance(4) => LocalEvent::DeviceRevoked {
                device_id: d.device_id,
            },
            _ => started(task_id),
        },
        6 => match device {
            Some(d) => LocalEvent::PermissionsChanged {
                device_id: d.device_id,
                permissions: RemotePermissions {
                    start_tasks: rng.chance(2),
                    see_progress: rng.chance(2),
                    see_screen_text: rng.chance(2),
                    approve: rng.chance(2),
                },
            },
            None => started(None),
        },
        _ => LocalEvent::CatalogChanged {
            skills: vec![CatalogEntry {
                id: "timer".into(),
                name: "Timer".into(),
                summary: String::from_utf8_lossy(&rng.bytes(30)).into_owned(),
            }],
            workflows: vec![CatalogEntry {
                id: "morning".into(),
                name: "Morning".into(),
                summary: String::new(),
            }],
        },
    }
}

fn mutate(rng: &mut Rng, mut frame: Vec<u8>) -> Vec<u8> {
    match rng.below(5) {
        0 if !frame.is_empty() => {
            let i = rng.below(frame.len() as u64) as usize;
            frame[i] ^= 1 << rng.below(8);
        }
        1 => frame.truncate(rng.below(frame.len() as u64 + 1) as usize),
        2 => frame.extend(rng.bytes(20)),
        3 if !frame.is_empty() => {
            let i = rng.below(frame.len() as u64) as usize;
            frame[i] = rng.next() as u8;
        }
        _ => frame = rng.bytes(64),
    }
    frame
}

fn check_effects(hub: &RemoteHub, fx: &[HubEffect]) {
    assert!(hub.connection_count() <= MAX_CONNECTIONS);
    assert!(hub.devices().len() <= MAX_DEVICES);
    for e in fx {
        match e {
            HubEffect::Send { bytes, .. } => assert!(bytes.len() <= MAX_RECORD_LEN + 4),
            HubEffect::Close { conn_id, .. } => assert!(!hub.is_authenticated(*conn_id)),
            _ => {}
        }
    }
}

/// The legitimate remote that the fuzzer keeps (re)connecting while noise flies.
struct Legit {
    conn: u64,
    client: Option<RemoteClient>,
    creds: Option<ClientCredentials>,
}

fn reconnect(rng: &mut Rng, hub: &mut RemoteHub, now: u64, legit: &mut Legit) {
    let conn = 1_000 + rng.below(4);
    if hub.is_authenticated(conn) {
        return;
    }
    hub.connection_closed(conn);
    let known = legit
        .creds
        .as_ref()
        .is_some_and(|c| hub.devices().iter().any(|d| d.id == c.device_id));
    let mut client = if known {
        RemoteClient::connect(legit.creds.clone().unwrap(), &rng.entropy()).unwrap()
    } else {
        let Ok(offer) = hub.open_pairing(now, &rng.entropy()) else {
            return;
        };
        RemoteClient::pair(&offer.code, "Fuzz", &rng.entropy()).unwrap()
    };
    let fx = hub.connection_opened(conn, now, &rng.entropy());
    check_effects(hub, &fx);
    let mut inbound = sends(&fx, conn);
    for _ in 0..3 {
        let Ok(events) = client.receive(&inbound) else {
            return;
        };
        inbound.clear();
        for ev in events {
            match ev {
                ClientEvent::Send { bytes } => {
                    let fx = hub.receive(conn, &bytes, now);
                    check_effects(hub, &fx);
                    inbound.extend(sends(&fx, conn));
                }
                ClientEvent::Paired { credentials } => legit.creds = Some(credentials),
                _ => {}
            }
        }
    }
    if client.is_connected() {
        legit.conn = conn;
        legit.client = Some(client);
    }
}

#[test]
fn hub_survives_arbitrary_input() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut hub = RemoteHub::new("Fuzz Phone");
    let mut now: u64 = 10_000;
    let mut legit = Legit {
        conn: 0,
        client: None,
        creds: None,
    };
    let mut accepted_commands = 0usize;
    let mut last_task: Option<u64> = None;
    // Opening a connection costs an X25519 key generation (slow in debug builds), so
    // opens are rarer than the other operations.
    for _ in 0..12_000 {
        let op = match rng.below(20) {
            0 => 0,
            1 => 1,
            2..=4 => 2,
            5 => 3,
            6 | 7 => 4,
            8 => 5,
            _ => 9,
        };
        let fx = match op {
            0 => {
                let id = rng.below(12);
                // Sometimes the shell passes broken entropy.
                let e: Vec<u8> = if rng.chance(10) {
                    vec![0u8; 32]
                } else {
                    rng.entropy().to_vec()
                };
                hub.connection_opened(id, now, &e)
            }
            1 => hub.connection_closed(rng.below(12)),
            2 => {
                let id = rng.below(12);
                let mut bytes = rng.bytes(120);
                if rng.chance(2) {
                    let kind = *rng.pick(&[1u8, 2, 3, 4, 5, 16, 0, 255]);
                    let payload = rng.bytes(100);
                    bytes = encode_record(Kind::Data, &payload);
                    bytes[5] = kind;
                }
                hub.receive(id, &bytes, now)
            }
            3 => {
                now = if rng.chance(20) {
                    now.saturating_sub(rng.below(5_000))
                } else {
                    now + rng.below(30_000)
                };
                hub.tick(now)
            }
            4 => {
                let device = legit.creds.clone();
                let pending = hub.pending_request_id();
                hub.local_event(now, random_local(&mut rng, device, last_task, pending))
            }
            5 => {
                if rng.chance(3) {
                    let _ = hub.open_pairing(now, &rng.entropy());
                } else if rng.chance(5) {
                    hub.cancel_pairing();
                }
                Vec::new()
            }
            _ => {
                // The legitimate remote: valid commands, sometimes mangled in flight.
                if legit.client.as_ref().is_none_or(|c| !c.is_connected())
                    || !hub.is_authenticated(legit.conn)
                {
                    reconnect(&mut rng, &mut hub, now, &mut legit);
                    Vec::new()
                } else {
                    let msg = random_message(&mut rng, hub.pending_request_id());
                    let client = legit.client.as_mut().unwrap();
                    let mut frame = client.send(&msg).unwrap();
                    // Rare: a mangled frame closes the link, and reconnecting is X25519 work.
                    if rng.chance(40) {
                        frame = mutate(&mut rng, frame);
                    }
                    let fx = hub.receive(legit.conn, &frame, now);
                    accepted_commands += actions(&fx).len();
                    last_task = task_id(&fx).or(last_task);
                    fx
                }
            }
        };
        check_effects(&hub, &fx);
        // Everything the hub sends to the legitimate remote reaches it, or its counter
        // falls behind (which, correctly, ends that connection).
        if let Some(client) = legit.client.as_mut() {
            let back = sends(&fx, legit.conn);
            if !back.is_empty() {
                let _ = client.receive(&back);
            }
        }
    }
    // The fuzzer did reach the interesting states.
    assert!(accepted_commands > 50, "{accepted_commands}");
}

#[test]
fn client_survives_arbitrary_input() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (_, creds) = {
        let mut rig = Rig::new();
        rig.pair("Mac")
    };
    // Fewer iterations than the hub fuzz: every client does an X25519 key generation.
    for i in 0..1_000 {
        let mut client = if i % 2 == 0 {
            RemoteClient::connect(creds.clone(), &rng.entropy()).unwrap()
        } else {
            RemoteClient::pair(
                &jarvis_remote::format_code(&jarvis_remote::PairingSecret::from_bytes(
                    core::array::from_fn(|_| rng.next() as u8),
                )),
                "Mac",
                &rng.entropy(),
            )
            .unwrap()
        };
        if rng.chance(2) {
            let mut hello = rng.entropy().to_vec();
            hello.extend(rng.entropy());
            let _ = client.receive(&encode_record(Kind::ServerHello, &hello));
        }
        for _ in 0..3 {
            let mut bytes = rng.bytes(80);
            if rng.chance(2) {
                let kind = *rng.pick(&[1u8, 3, 5, 16, 9]);
                bytes = encode_record(Kind::Data, &rng.bytes(60));
                bytes[5] = kind;
            }
            let _ = client.receive(&bytes);
        }
        assert!(!client.is_connected());
    }
}

#[test]
fn decoders_survive_arbitrary_input() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let samples = [
        r#"{"type":"start_task","text":"x"}"#,
        r#"{"type":"decision","request_id":1,"approved":true}"#,
        r#"{"type":"run_skill","id":"a","args":{"k":[1,{"z":null}]}}"#,
        r#"{"schema_version":2,"nonce_lease":1024,"devices":[{"id":"00112233445566778899aabbccddeeff","name":"Mac","key":"0000000000000000000000000000000000000000000000000000000000000000","permissions":{"start_tasks":true,"see_progress":true,"see_screen_text":false,"approve":false},"paired_at_ms":5}]}"#,
    ];
    for _ in 0..20_000 {
        let base = rng.pick(&samples).as_bytes().to_vec();
        let bytes = mutate(&mut rng, base);
        let _ = ClientMessage::from_bytes(&bytes);
        let _ = ServerMessage::from_bytes(&bytes);
        let text = String::from_utf8_lossy(&bytes);
        let _ = DeviceStore::from_json(&text);
        let _ = ClientCredentials::from_json(&text);
        let _ = jarvis_remote::parse_code(&text);
    }
    // The unmutated store sample loads.
    assert_eq!(DeviceStore::from_json(samples[3]).unwrap().devices.len(), 1);
}
