//! The real protocol, end to end in memory: pairing, reconnecting, tasks, progress,
//! confirmations, permissions, skills, persistence and revocation.

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
    AuditEvent, CatalogEntry, ClientEvent, ClientMessage, CloseReason, DenyReason, DeviceStore,
    HubEffect, HubError, LocalEvent, PersistError, RemoteClient, RemoteHub, RemotePermissions,
    ServerMessage, HANDSHAKE_TIMEOUT_MS, IDLE_TIMEOUT_MS, MAX_CONNECTIONS, MAX_DEVICES,
};

fn denied(msgs: &[ServerMessage]) -> Option<DenyReason> {
    msgs.iter().find_map(|m| match m {
        ServerMessage::Denied { reason } => Some(*reason),
        _ => None,
    })
}

fn audited_denial(fx: &[HubEffect], want: DenyReason) -> bool {
    audits(fx)
        .iter()
        .any(|a| matches!(a, AuditEvent::Denied { reason, .. } if *reason == want))
}

#[test]
fn pair_connect_start_progress_confirm_finish() {
    let mut rig = Rig::new();
    let (first, creds) = rig.pair("Owner's Mac");
    drop(first);
    // Reconnect on a new connection with the stored credentials.
    let mut link = rig.connect(&creds);

    let (_, msgs) = rig.request(&mut link, ClientMessage::Hello);
    assert_eq!(
        msgs,
        vec![ServerMessage::Welcome {
            phone_name: "Test Phone".into(),
            permissions: RemotePermissions::default(),
            busy: false,
        }]
    );
    let (_, msgs) = rig.request(&mut link, ClientMessage::Subscribe);
    assert!(msgs.is_empty());

    let (fx, msgs) = rig.request(
        &mut link,
        ClientMessage::StartTask {
            text: "Text Sam\u{0} that I'm late".into(),
        },
    );
    assert!(msgs.is_empty());
    assert_eq!(
        actions(&fx),
        vec![HubEffect::StartTask {
            device_id: creds.device_id,
            text: "Text Sam that I'm late".into(),
        }]
    );
    assert!(rig.hub.is_busy());

    let fx = rig.local(LocalEvent::SessionStarted);
    assert_eq!(deliver(&mut link, &fx), vec![ServerMessage::TaskStarted]);

    let fx = rig.local(progress_event(
        2,
        Action::Type {
            element: Some(3),
            text: "my secret text".into(),
            submit: false,
        },
        Some("Messages\nSam: are you coming?"),
    ));
    let msgs = deliver(&mut link, &fx);
    assert_eq!(
        msgs,
        vec![ServerMessage::Progress {
            step: 2,
            phase: jarvis_proto::Phase::Acting,
            app: Some("com.apple.MobileSMS".into()),
            last_action: Some("type".into()),
            screen_text: None,
        }]
    );
    // The typed text never leaves the phone.
    let wire = sends(&fx, link.conn);
    assert!(!String::from_utf8_lossy(&wire).contains("secret"));

    let fx = rig.local(confirm_event(
        RiskCategory::SendMessage,
        "Tap \"Send\" in Messages",
    ));
    let msgs = deliver(&mut link, &fx);
    let request_id = rig.hub.pending_request_id().unwrap();
    assert_eq!(
        msgs,
        vec![ServerMessage::ConfirmPending {
            request_id,
            category: RiskCategory::SendMessage,
            summary: "Waiting for confirmation (send message)".into(),
            can_decide: false,
        }]
    );
    // Default permissions cannot decide.
    let (fx, msgs) = rig.request(
        &mut link,
        ClientMessage::Decision {
            request_id,
            approved: true,
        },
    );
    assert_eq!(denied(&msgs), Some(DenyReason::NotPermitted));
    assert!(actions(&fx).is_empty());
    assert!(audited_denial(&fx, DenyReason::NotPermitted));

    let fx = rig.local(LocalEvent::ConfirmResolved { approved: true });
    assert_eq!(
        deliver(&mut link, &fx),
        vec![ServerMessage::ConfirmResolved { request_id }]
    );
    let fx = rig.local(finished_event(
        OutcomeStatus::Success,
        "Sent \"I'm late\" to Sam",
    ));
    assert_eq!(
        deliver(&mut link, &fx),
        vec![ServerMessage::Finished {
            status: OutcomeStatus::Success,
            summary: "Task completed".into(),
        }]
    );
    assert!(!rig.hub.is_busy());
    let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
}

#[test]
fn approve_on_permitted_category_is_honoured_exactly_once() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    rig.set_permissions(
        &mut link,
        &creds,
        RemotePermissions {
            approve: true,
            see_screen_text: true,
            ..RemotePermissions::default()
        },
    );
    // The permission change is announced on the open connection.
    let (_, msgs) = rig.request(&mut link, ClientMessage::Subscribe);
    assert!(msgs.is_empty());
    rig.request(&mut link, ClientMessage::StartTask { text: "go".into() });
    rig.local_to(&mut link, LocalEvent::SessionStarted);

    let fx = rig.local(confirm_event(
        RiskCategory::SendMessage,
        "Type \"card 4111 1111 1111 1111\" into Message in Messages",
    ));
    let msgs = deliver(&mut link, &fx);
    let request_id = rig.hub.pending_request_id().unwrap();
    let ServerMessage::ConfirmPending {
        summary,
        can_decide,
        ..
    } = &msgs[0]
    else {
        panic!("{msgs:?}")
    };
    assert!(*can_decide);
    assert!(summary.contains("[redacted card]"), "{summary}");
    assert!(!summary.contains("4111"));

    let (fx, msgs) = rig.request(
        &mut link,
        ClientMessage::Decision {
            request_id,
            approved: true,
        },
    );
    assert!(msgs.is_empty());
    assert_eq!(
        actions(&fx),
        vec![HubEffect::Decide {
            request_id,
            approved: true,
            device_id: creds.device_id,
        }]
    );
    assert!(audits(&fx)
        .iter()
        .any(|a| matches!(a, AuditEvent::Accepted { request, .. } if request == "decision")));

    // Same id again: already decided.
    let (fx, msgs) = rig.request(
        &mut link,
        ClientMessage::Decision {
            request_id,
            approved: false,
        },
    );
    assert_eq!(denied(&msgs), Some(DenyReason::StaleRequest));
    assert!(actions(&fx).is_empty());
    assert!(audited_denial(&fx, DenyReason::StaleRequest));

    // After the phone resolves it, still stale; unknown ids too.
    rig.local_to(&mut link, LocalEvent::ConfirmResolved { approved: true });
    for id in [request_id, request_id + 1, 0, u64::MAX] {
        let (fx, msgs) = rig.request(
            &mut link,
            ClientMessage::Decision {
                request_id: id,
                approved: true,
            },
        );
        assert_eq!(denied(&msgs), Some(DenyReason::StaleRequest));
        assert!(actions(&fx).is_empty());
    }

    // A new request gets a new id; the old id does not answer it.
    rig.local_to(&mut link, confirm_event(RiskCategory::Call, "Call Sam"));
    let new_id = rig.hub.pending_request_id().unwrap();
    assert_ne!(new_id, request_id);
    let (_, msgs) = rig.request(
        &mut link,
        ClientMessage::Decision {
            request_id,
            approved: true,
        },
    );
    assert_eq!(denied(&msgs), Some(DenyReason::StaleRequest));
    let (fx, _) = rig.request(
        &mut link,
        ClientMessage::Decision {
            request_id: new_id,
            approved: false,
        },
    );
    assert_eq!(
        actions(&fx),
        vec![HubEffect::Decide {
            request_id: new_id,
            approved: false,
            device_id: creds.device_id,
        }]
    );
}

#[test]
fn forbidden_categories_are_denied_even_with_approve() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    rig.set_permissions(
        &mut link,
        &creds,
        RemotePermissions {
            approve: true,
            ..RemotePermissions::default()
        },
    );
    rig.request(&mut link, ClientMessage::Subscribe);
    rig.request(&mut link, ClientMessage::StartTask { text: "buy".into() });
    for category in jarvis_remote::NEVER_REMOTE_APPROVE {
        let fx = rig.local(confirm_event(category, "Tap \"Pay\" in Shop"));
        let msgs = deliver(&mut link, &fx);
        let request_id = rig.hub.pending_request_id().unwrap();
        assert!(matches!(
            msgs[0],
            ServerMessage::ConfirmPending {
                can_decide: false,
                ..
            }
        ));
        let (fx, msgs) = rig.request(
            &mut link,
            ClientMessage::Decision {
                request_id,
                approved: true,
            },
        );
        assert_eq!(
            denied(&msgs),
            Some(DenyReason::NotRemotelyApprovable),
            "{category:?}"
        );
        assert!(actions(&fx).is_empty());
        assert!(audited_denial(&fx, DenyReason::NotRemotelyApprovable));
        rig.local_to(&mut link, LocalEvent::ConfirmResolved { approved: false });
    }
}

#[test]
fn screen_text_needs_the_permission_and_is_redacted() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    rig.request(&mut link, ClientMessage::Subscribe);
    rig.local_to(&mut link, LocalEvent::SessionStarted);
    let text = "Your verification code is 482913\u{202E}\nDone";
    let fx = rig.local(progress_event(1, Action::Look, Some(text)));
    assert!(matches!(
        &deliver(&mut link, &fx)[0],
        ServerMessage::Progress {
            screen_text: None,
            ..
        }
    ));
    let fx = rig.local(LocalEvent::PermissionsChanged {
        device_id: creds.device_id,
        permissions: RemotePermissions {
            see_screen_text: true,
            ..RemotePermissions::default()
        },
    });
    assert!(matches!(
        &deliver(&mut link, &fx)[0],
        ServerMessage::PermissionsChanged { permissions } if permissions.see_screen_text
    ));
    let fx = rig.local(progress_event(2, Action::Look, Some(text)));
    let msgs = deliver(&mut link, &fx);
    let ServerMessage::Progress {
        screen_text: Some(t),
        ..
    } = &msgs[0]
    else {
        panic!("{msgs:?}")
    };
    assert!(!t.contains("482913"), "{t}");
    assert!(!t.contains('\u{202E}'));
    assert!(t.contains("Done"));
    // The finish summary is forwarded too, redacted.
    let fx = rig.local(finished_event(
        OutcomeStatus::Success,
        "Signed in with code 482913",
    ));
    let msgs = deliver(&mut link, &fx);
    let ServerMessage::Finished { summary, .. } = &msgs[0] else {
        panic!("{msgs:?}")
    };
    assert!(summary.starts_with("Signed in"), "{summary}");
    assert!(!summary.contains("482913"));
}

#[test]
fn unsubscribed_or_unpermitted_devices_see_nothing() {
    let mut rig = Rig::new();
    let (mut watcher, _) = rig.pair("Watcher");
    let (mut quiet, quiet_creds) = rig.pair("Quiet");
    rig.request(&mut watcher, ClientMessage::Subscribe);
    rig.set_permissions(
        &mut quiet,
        &quiet_creds,
        RemotePermissions {
            see_progress: false,
            ..RemotePermissions::default()
        },
    );
    let (fx, msgs) = rig.request(&mut quiet, ClientMessage::Subscribe);
    assert_eq!(denied(&msgs), Some(DenyReason::NotPermitted));
    assert!(audited_denial(&fx, DenyReason::NotPermitted));
    let fx = rig.local(progress_event(1, Action::PressHome, None));
    assert_eq!(deliver(&mut watcher, &fx).len(), 1);
    assert!(deliver(&mut quiet, &fx).is_empty());
    // Unsubscribing stops the stream.
    rig.request(&mut watcher, ClientMessage::Unsubscribe);
    let fx = rig.local(progress_event(2, Action::PressHome, None));
    assert!(deliver(&mut watcher, &fx).is_empty());
}

#[test]
fn one_task_at_a_time_across_devices() {
    let mut rig = Rig::new();
    let (mut a, _) = rig.pair("A");
    let (mut b, _) = rig.pair("B");
    let (fx, _) = rig.request(&mut a, ClientMessage::StartTask { text: "one".into() });
    assert_eq!(actions(&fx).len(), 1);
    for link in [&mut a, &mut b] {
        let (fx, msgs) = rig.request(link, ClientMessage::StartTask { text: "two".into() });
        assert_eq!(denied(&msgs), Some(DenyReason::Busy));
        assert!(actions(&fx).is_empty());
        assert!(audited_denial(&fx, DenyReason::Busy));
    }
    // A task started on the phone itself also makes the hub busy.
    rig.local_to(&mut a, finished_event(OutcomeStatus::Success, "done"));
    rig.local(LocalEvent::SessionStarted);
    let (_, msgs) = rig.request(
        &mut b,
        ClientMessage::StartTask {
            text: "three".into(),
        },
    );
    assert_eq!(denied(&msgs), Some(DenyReason::Busy));
    let (_, msgs) = rig.request(&mut b, ClientMessage::Hello);
    assert!(matches!(msgs[0], ServerMessage::Welcome { busy: true, .. }));
    rig.local(finished_event(OutcomeStatus::Cancelled, "stopped"));
    let (fx, _) = rig.request(
        &mut b,
        ClientMessage::StartTask {
            text: "four".into(),
        },
    );
    assert_eq!(actions(&fx).len(), 1);
}

#[test]
fn device_without_start_tasks_is_refused() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Viewer");
    rig.set_permissions(
        &mut link,
        &creds,
        RemotePermissions {
            start_tasks: false,
            ..RemotePermissions::default()
        },
    );
    rig.local(LocalEvent::CatalogChanged {
        skills: vec![CatalogEntry {
            id: "wifi.on".into(),
            name: "Wi-Fi on".into(),
            summary: "Turns Wi-Fi on".into(),
        }],
        workflows: vec![CatalogEntry {
            id: "morning".into(),
            name: "Morning".into(),
            summary: "".into(),
        }],
    });
    for msg in [
        ClientMessage::StartTask { text: "x".into() },
        ClientMessage::RunSkill {
            id: "wifi.on".into(),
            args: Default::default(),
        },
        ClientMessage::RunWorkflow {
            id: "morning".into(),
        },
    ] {
        let (fx, msgs) = rig.request(&mut link, msg);
        assert_eq!(denied(&msgs), Some(DenyReason::NotPermitted));
        assert!(actions(&fx).is_empty());
        assert!(audited_denial(&fx, DenyReason::NotPermitted));
    }
    assert!(!rig.hub.is_busy());
}

#[test]
fn skills_are_listed_and_run_by_known_id_only() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    let skill = CatalogEntry {
        id: "timer.start".into(),
        name: "Start timer".into(),
        summary: "Starts a timer".into(),
    };
    let flow = CatalogEntry {
        id: "leave-home".into(),
        name: "Leaving home".into(),
        summary: "Lights off, music on".into(),
    };
    rig.local(LocalEvent::CatalogChanged {
        skills: vec![skill.clone()],
        workflows: vec![flow.clone()],
    });
    let (_, msgs) = rig.request(&mut link, ClientMessage::ListSkills);
    assert_eq!(
        msgs,
        vec![ServerMessage::Skills {
            skills: vec![skill],
            workflows: vec![flow],
        }]
    );
    let (fx, msgs) = rig.request(
        &mut link,
        ClientMessage::RunSkill {
            id: "rm.rf".into(),
            args: Default::default(),
        },
    );
    assert_eq!(denied(&msgs), Some(DenyReason::UnknownSkill));
    assert!(audited_denial(&fx, DenyReason::UnknownSkill));
    let (_, msgs) = rig.request(
        &mut link,
        ClientMessage::RunWorkflow {
            id: "timer.start".into(),
        },
    );
    assert_eq!(denied(&msgs), Some(DenyReason::UnknownSkill));

    let mut args = std::collections::BTreeMap::new();
    args.insert("minutes".to_owned(), serde_json::json!(5));
    let (fx, _) = rig.request(
        &mut link,
        ClientMessage::RunSkill {
            id: "timer.start".into(),
            args: args.clone(),
        },
    );
    assert_eq!(
        actions(&fx),
        vec![HubEffect::RunSkill {
            device_id: creds.device_id,
            id: "timer.start".into(),
            args,
        }]
    );
    rig.local_to(&mut link, finished_event(OutcomeStatus::Success, "ok"));
    let (fx, _) = rig.request(
        &mut link,
        ClientMessage::RunWorkflow {
            id: "leave-home".into(),
        },
    );
    assert_eq!(
        actions(&fx),
        vec![HubEffect::RunWorkflow {
            device_id: creds.device_id,
            id: "leave-home".into(),
        }]
    );
}

#[test]
fn cancel_needs_a_running_task_and_names_the_device() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    let (fx, msgs) = rig.request(&mut link, ClientMessage::Cancel);
    assert_eq!(denied(&msgs), Some(DenyReason::NotRunning));
    assert!(audited_denial(&fx, DenyReason::NotRunning));
    rig.local(LocalEvent::SessionStarted);
    let (fx, _) = rig.request(&mut link, ClientMessage::Cancel);
    assert_eq!(
        actions(&fx),
        vec![HubEffect::CancelTask {
            device_id: creds.device_id
        }]
    );
}

#[test]
fn invalid_task_text_is_rejected() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    for text in ["".to_owned(), " \u{200B}\n ".to_owned(), "x".repeat(2_001)] {
        let (fx, msgs) = rig.request(&mut link, ClientMessage::StartTask { text });
        assert_eq!(
            msgs,
            vec![ServerMessage::Error {
                code: jarvis_remote::ErrorCode::InvalidRequest
            }]
        );
        assert!(actions(&fx).is_empty());
        assert!(audits(&fx)
            .iter()
            .any(|a| matches!(a, AuditEvent::Rejected { .. })));
    }
}

#[test]
fn revoking_a_device_mid_session_closes_it_and_its_key_stops_working() {
    let mut rig = Rig::new();
    let (mut lost, creds) = rig.pair("Lost phone");
    let (mut other, _) = rig.pair("Mac");
    let mut second = rig.connect(&creds);
    rig.request(&mut lost, ClientMessage::StartTask { text: "go".into() });
    rig.local(LocalEvent::SessionStarted);

    let fx = rig.local(LocalEvent::DeviceRevoked {
        device_id: creds.device_id,
    });
    assert_eq!(closed(&fx, lost.conn), Some(CloseReason::Revoked));
    assert_eq!(closed(&fx, second.conn), Some(CloseReason::Revoked));
    assert_eq!(closed(&fx, other.conn), None);
    assert_eq!(
        actions(&fx),
        vec![HubEffect::CancelTask {
            device_id: creds.device_id
        }]
    );
    assert!(audits(&fx).contains(&AuditEvent::Revoked));
    assert!(!rig.hub.is_authenticated(lost.conn));
    assert_eq!(rig.hub.devices().len(), 1);

    // Bytes still in flight on the closed connection are ignored.
    let bytes = lost.client.send(&ClientMessage::Ping).unwrap();
    assert!(rig.hub.receive(lost.conn, &bytes, rig.now).is_empty());
    let bytes = second.client.send(&ClientMessage::Ping).unwrap();
    assert!(rig.hub.receive(second.conn, &bytes, rig.now).is_empty());

    // The old key cannot reconnect.
    let (link, fx, _) = rig.open(RemoteClient::connect(creds.clone(), entropy(99)));
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::AuthFailed));
    assert!(!link.client.is_connected());

    // The other device is unaffected.
    let (_, msgs) = rig.request(&mut other, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
}

#[test]
fn persistence_roundtrips_and_rejects_corrupt_input() {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    rig.set_permissions(
        &mut link,
        &creds,
        RemotePermissions {
            approve: true,
            ..RemotePermissions::default()
        },
    );
    let json = rig.hub.to_json();
    assert!(json.contains("\"schema_version\":1"));
    let restored = RemoteHub::from_json("Test Phone", &json).unwrap();
    assert_eq!(restored.devices(), rig.hub.devices());
    assert_eq!(restored.store(), rig.hub.store());
    let mut rig2 = Rig::with_hub(restored);
    let mut link = rig2.connect(&creds);
    let (_, msgs) = rig2.request(&mut link, ClientMessage::Hello);
    assert!(matches!(
        &msgs[0],
        ServerMessage::Welcome { permissions, .. } if permissions.approve
    ));

    // Client credentials round-trip too, and the Debug output hides the key.
    let cj = creds.to_json();
    assert_eq!(
        jarvis_remote::ClientCredentials::from_json(&cj).unwrap(),
        creds
    );
    assert!(format!("{creds:?}").contains("redacted"));
    assert!(!format!("{:?}", rig.hub.store()).contains(&cj[cj.len() - 20..cj.len() - 2]));

    assert!(matches!(
        DeviceStore::from_json("{not json"),
        Err(PersistError::Corrupt(_))
    ));
    assert!(matches!(
        DeviceStore::from_json(r#"{"devices":[]}"#),
        Err(PersistError::Corrupt(_))
    ));
    assert_eq!(
        DeviceStore::from_json(r#"{"schema_version":2,"devices":[]}"#),
        Err(PersistError::UnsupportedVersion(2))
    );
    let bad_key = json.replacen("\"key\":\"", "\"key\":\"zz", 1);
    assert!(matches!(
        DeviceStore::from_json(&bad_key),
        Err(PersistError::Invalid(_))
    ));
    let store = rig.hub.store();
    let mut dup = store.clone();
    dup.devices.push(store.devices[0].clone());
    assert!(matches!(
        DeviceStore::from_json(&dup.to_json()),
        Err(PersistError::Invalid(_))
    ));
    assert_eq!(
        DeviceStore::from_json(r#"{"schema_version":1,"devices":[]}"#).unwrap(),
        DeviceStore::default()
    );
}

#[test]
fn device_and_connection_limits() {
    let mut rig = Rig::new();
    let mut links = Vec::new();
    for i in 0..MAX_DEVICES {
        links.push(rig.pair(&format!("Device {i}")));
    }
    assert_eq!(
        rig.hub.open_pairing(rig.now, entropy(5)).unwrap_err(),
        HubError::TooManyDevices
    );
    assert_eq!(rig.hub.connection_count(), MAX_CONNECTIONS);
    let fx = rig.hub.connection_opened(999, rig.now, entropy(6));
    assert_eq!(closed(&fx, 999), Some(CloseReason::TooManyConnections));
    assert_eq!(rig.hub.connection_count(), MAX_CONNECTIONS);
    // Closing one frees a slot.
    rig.hub.connection_closed(links[0].0.conn);
    let fx = rig.hub.connection_opened(999, rig.now, entropy(6));
    assert_eq!(closed(&fx, 999), None);
}

#[test]
fn timeouts_close_idle_and_half_open_connections() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let half_open = rig.conn_id();
    rig.hub.connection_opened(half_open, rig.now, entropy(42));

    rig.now += HANDSHAKE_TIMEOUT_MS;
    let fx = rig.hub.tick(rig.now);
    assert_eq!(closed(&fx, half_open), Some(CloseReason::HandshakeTimeout));
    assert_eq!(closed(&fx, link.conn), None);

    // Pings keep the authenticated connection alive.
    rig.now += IDLE_TIMEOUT_MS - HANDSHAKE_TIMEOUT_MS - 1;
    let (_, msgs) = rig.request(&mut link, ClientMessage::Ping);
    assert_eq!(msgs, vec![ServerMessage::Pong]);
    rig.now += IDLE_TIMEOUT_MS - 1;
    assert!(rig.hub.tick(rig.now).is_empty());
    rig.now += 1;
    let fx = rig.hub.tick(rig.now);
    assert_eq!(closed(&fx, link.conn), Some(CloseReason::IdleTimeout));
}

#[test]
fn split_and_coalesced_bytes_are_reassembled() {
    let mut rig = Rig::new();
    let (mut link, _) = rig.pair("Mac");
    let mut all = Vec::new();
    for _ in 0..3 {
        all.extend(link.client.send(&ClientMessage::Ping).unwrap());
    }
    let mut pongs = 0;
    for b in &all {
        let (_, msgs) = rig.feed(&mut link, std::slice::from_ref(b));
        pongs += msgs.len();
    }
    assert_eq!(pongs, 3);
    let mut all = Vec::new();
    for _ in 0..3 {
        all.extend(link.client.send(&ClientMessage::Ping).unwrap());
    }
    let (_, msgs) = rig.feed(&mut link, &all);
    assert_eq!(msgs, vec![ServerMessage::Pong; 3]);
}

#[test]
fn qr_payload_pairs_too_and_events_are_announced() {
    let mut rig = Rig::new();
    let offer = rig.hub.open_pairing(rig.now, entropy(1234)).unwrap();
    assert!(offer.qr_payload.starts_with("jarvis-remote:1:"));
    assert!(!format!("{offer:?}").contains(&offer.code));
    let client = RemoteClient::pair(&offer.qr_payload, "  Browser\n", entropy(77)).unwrap();
    let (_, fx, events) = rig.open(client);
    let events = events.unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, ClientEvent::Paired { .. })));
    let paired = fx
        .iter()
        .find_map(|e| match e {
            HubEffect::DevicePaired { device } => Some(device.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(paired.name, "Browser");
    assert_eq!(paired.permissions, RemotePermissions::default());
    assert!(audits(&fx).contains(&AuditEvent::Paired {
        name: "Browser".into()
    }));
    assert!(!rig.hub.pairing_open());
}
