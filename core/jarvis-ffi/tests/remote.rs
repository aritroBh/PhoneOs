//! `JarvisRemoteHub` + `JarvisRemoteClient` in memory, through the FFI objects only:
//! pair, reconnect, start a task, progress, confirmations (a remote decline, a refused
//! approval of a never-remote category), finish, revoke; entropy refusals, persistence,
//! typed errors, and no secret in any `Debug` output, error or audit entry.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use jarvis_core_ffi::*;

/// Test entropy: a splitmix64 stream (looks random to the hub's sanity check). Tests only;
/// the app passes `SecRandomCopyBytes` output.
struct Rng(u64);

impl Rng {
    fn bytes(&mut self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        while out.len() < 32 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            out.extend_from_slice(&(z ^ (z >> 31)).to_le_bytes());
        }
        out
    }
}

/// The shell side of the phone in this test: carries out hub effects in order.
struct Phone {
    hub: Arc<JarvisRemoteHub>,
    /// What the "Keychain" holds; written on every `storeChanged`.
    keychain: String,
    /// Every non-send effect, in order.
    log: Vec<HubEffect>,
    now: u64,
}

impl Phone {
    /// Applies effects; returns the bytes to deliver per connection.
    fn apply(&mut self, fx: Vec<HubEffect>) -> Vec<(u64, Vec<u8>)> {
        let mut out = Vec::new();
        for e in fx {
            match e {
                HubEffect::Send { conn_id, bytes } => out.push((conn_id, bytes)),
                HubEffect::StoreChanged => {
                    self.keychain = self.hub.to_json().unwrap();
                    self.log.push(HubEffect::StoreChanged);
                }
                other => self.log.push(other),
            }
        }
        out
    }

    fn tick(&mut self) -> u64 {
        self.now += 10;
        self.now
    }
}

/// Moves bytes both ways between the hub (connection `conn`) and `client` until quiet;
/// returns the client's non-send events.
fn pump(
    phone: &mut Phone,
    conn: u64,
    client: &JarvisRemoteClient,
    mut to_client: Vec<(u64, Vec<u8>)>,
) -> Vec<ClientEvent> {
    let mut seen = Vec::new();
    for _ in 0..50 {
        let mut to_phone = Vec::new();
        for (c, bytes) in to_client.drain(..) {
            if c != conn {
                continue;
            }
            for ev in client.receive(bytes).unwrap() {
                match ev {
                    ClientEvent::Send { bytes } => to_phone.push(bytes),
                    other => seen.push(other),
                }
            }
        }
        if to_phone.is_empty() {
            return seen;
        }
        for bytes in to_phone {
            let now = phone.tick();
            let fx = phone.hub.receive(conn, bytes, now).unwrap();
            to_client.extend(phone.apply(fx));
        }
    }
    panic!("no quiescence")
}

fn send(
    phone: &mut Phone,
    conn: u64,
    client: &JarvisRemoteClient,
    m: ClientMessage,
) -> Vec<ClientEvent> {
    let bytes = client.send(m).unwrap();
    let now = phone.tick();
    let fx = phone.hub.receive(conn, bytes, now).unwrap();
    let out = phone.apply(fx);
    pump(phone, conn, client, out)
}

fn local(
    phone: &mut Phone,
    conn: u64,
    client: &JarvisRemoteClient,
    e: LocalEvent,
) -> Vec<ClientEvent> {
    let now = phone.tick();
    let fx = phone.hub.local_event(now, e).unwrap();
    let out = phone.apply(fx);
    pump(phone, conn, client, out)
}

fn messages(events: Vec<ClientEvent>) -> Vec<ServerMessage> {
    events
        .into_iter()
        .filter_map(|e| match e {
            ClientEvent::Message { message } => Some(message),
            _ => None,
        })
        .collect()
}

fn progress() -> SessionProgress {
    SessionProgress {
        step: 3,
        phase: Phase::Acting,
        last_action: Some(Action::Type {
            element: Some(2),
            text: "the typed words".into(),
            submit: false,
        }),
        app: Some("com.apple.MobileSMS".into()),
        model_calls: 3,
        parse_retries: 0,
        waiting_for_unlock: false,
        locked_wait_ms: 0,
        locked_total_ms: 0,
        fresh_looks: 0,
        stale_plans: 0,
        screen_locked: false,
        screen_hidden: false,
    }
}

fn node(role: Role, label: &str, value: Option<&str>, secure: bool, y: f64) -> UiNode {
    UiNode {
        role,
        label: Some(label.into()),
        value: value.map(Into::into),
        identifier: None,
        placeholder: None,
        frame: Rect {
            x: 10.0,
            y,
            width: 300.0,
            height: 40.0,
        },
        enabled: true,
        visible: true,
        hittable: true,
        focused: false,
        selected: false,
        secure,
        children: vec![],
    }
}

fn screen() -> Screen {
    let mut root = node(Role::Application, "Messages", None, false, 0.0);
    root.frame = Rect {
        x: 0.0,
        y: 0.0,
        width: 393.0,
        height: 852.0,
    };
    root.children = vec![
        node(
            Role::StaticText,
            "Card 4111 1111 1111 1111",
            None,
            false,
            100.0,
        ),
        node(
            Role::SecureTextField,
            "Password",
            Some("hunter2-Secret!"),
            true,
            200.0,
        ),
        node(Role::Button, "Send", None, false, 300.0),
    ];
    Screen {
        tree_cut: false,
        app_bundle_id: "com.apple.MobileSMS".into(),
        app_name: Some("Messages".into()),
        size: Size {
            width: 393.0,
            height: 852.0,
        },
        root,
        keyboard_visible: false,
        orientation: Orientation::Portrait,
    }
}

fn device_key(store_json: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(store_json).unwrap();
    v["devices"][0]["key"].as_str().unwrap().to_owned()
}

#[test]
fn the_full_protocol_through_the_ffi_objects() {
    let mut rng = Rng(7);
    let mut phone = Phone {
        hub: JarvisRemoteHub::new(String::new(), "Owner's phone".into()).unwrap(),
        keychain: String::new(),
        log: Vec::new(),
        now: 1_000,
    };
    let hub = Arc::clone(&phone.hub);
    let mut debug_dump: Vec<String> = Vec::new();

    // --- pair --------------------------------------------------------------------------
    let offer = hub.open_pairing(phone.now, rng.bytes()).unwrap();
    assert!(hub.is_pairing_open().unwrap());
    assert_eq!(offer.code.len(), 39);
    assert!(offer.qr_payload.starts_with("jarvis-remote:2:"));
    assert_eq!(offer.expires_at_ms, phone.now + 300_000);
    let code = offer.code.clone();
    let code_raw: String = code.chars().filter(|c| *c != '-').collect();
    let offer_debug = format!("{offer:?}");
    assert!(!offer_debug.contains(&code) && !offer_debug.contains(&code_raw[..8]));

    let client = JarvisRemoteClient::pair(code.clone(), "Work Mac".into(), rng.bytes()).unwrap();
    let now = phone.tick();
    let fx = hub.connection_opened(1, now, rng.bytes()).unwrap();
    let out = phone.apply(fx);
    let events = pump(&mut phone, 1, &client, out);
    assert!(client.is_connected().unwrap());
    let credentials = events
        .iter()
        .find_map(|e| match e {
            ClientEvent::Paired {
                credentials_json, ..
            } => Some(credentials_json.clone()),
            _ => None,
        })
        .expect("paired");
    assert!(events.iter().any(|e| matches!(e, ClientEvent::Connected)));
    debug_dump.push(format!("{events:?}"));
    assert!(!debug_dump.last().unwrap().contains(&credentials));
    let welcome = messages(events);
    assert!(
        matches!(&welcome[..], [ServerMessage::Welcome { protocol_version, permissions, phone_name: Some(n), busy: Some(false) }]
            if *protocol_version == remote_protocol_version() && *permissions == new_device_remote_permissions().unwrap() && n == "Owner's phone"),
        "{welcome:?}"
    );
    let devices = hub.devices().unwrap();
    assert_eq!(devices.len(), 1);
    let device_id = devices[0].id.clone();
    assert_eq!(
        client.device_id().unwrap().as_deref(),
        Some(device_id.as_str())
    );
    assert_eq!(devices[0].name, "Work Mac");
    assert_eq!(
        devices[0].permissions,
        new_device_remote_permissions().unwrap()
    );
    assert!(!hub.is_pairing_open().unwrap(), "single use");
    // DevicePaired, then the store was persisted before anything else needed it.
    let paired_at = phone
        .log
        .iter()
        .position(|e| matches!(e, HubEffect::DevicePaired { .. }))
        .unwrap();
    assert!(matches!(phone.log[paired_at + 1], HubEffect::StoreChanged));
    assert!(phone.keychain.contains(&device_id));
    let key = device_key(&phone.keychain);
    assert_eq!(key.len(), 64);
    assert!(credentials.contains(&key));

    // --- reconnect with the stored credentials -----------------------------------------
    let now = phone.tick();
    let fx = hub.connection_closed(1, now).unwrap();
    phone.apply(fx);
    let client = JarvisRemoteClient::connect(credentials.clone(), rng.bytes()).unwrap();
    let now = phone.tick();
    let fx = hub.connection_opened(2, now, rng.bytes()).unwrap();
    let out = phone.apply(fx);
    let events = pump(&mut phone, 2, &client, out);
    assert!(
        events.iter().any(|e| matches!(e, ClientEvent::Connected)),
        "{events:?}"
    );
    assert!(hub.is_authenticated(2).unwrap());
    // Connecting again works as often as needed (a new connection, new keys).
    let now = phone.tick();
    let fx = hub.connection_closed(2, now).unwrap();
    phone.apply(fx);
    let client = JarvisRemoteClient::connect(credentials.clone(), rng.bytes()).unwrap();
    let now = phone.tick();
    let fx = hub.connection_opened(3, now, rng.bytes()).unwrap();
    let out = phone.apply(fx);
    pump(&mut phone, 3, &client, out);
    assert!(client.is_connected().unwrap());

    // --- start a task ------------------------------------------------------------------
    let ev = send(&mut phone, 3, &client, ClientMessage::Subscribe);
    assert!(messages(ev).is_empty());
    phone.log.clear();
    send(
        &mut phone,
        3,
        &client,
        ClientMessage::StartTask {
            text: "Text Sam that I'm late".into(),
        },
    );
    let task_id = phone
        .log
        .iter()
        .find_map(|e| match e {
            HubEffect::StartTask {
                task_id,
                device_id: d,
                text,
            } => {
                assert_eq!(d, &device_id);
                assert_eq!(text, "Text Sam that I'm late");
                Some(*task_id)
            }
            _ => None,
        })
        .expect("startTask effect");
    assert!(hub.is_busy().unwrap());
    // Busy: a second request is denied, never queued.
    let denied = messages(send(
        &mut phone,
        3,
        &client,
        ClientMessage::StartTask { text: "x".into() },
    ));
    assert_eq!(
        denied,
        vec![ServerMessage::Denied {
            reason: DenyReason::Busy
        }]
    );

    let started = messages(local(
        &mut phone,
        3,
        &client,
        LocalEvent::SessionStarted {
            task_id: Some(task_id),
        },
    ));
    assert_eq!(started, vec![ServerMessage::TaskStarted]);

    // --- progress: projected, no screen text without the permission ---------------------
    let p = messages(local(
        &mut phone,
        3,
        &client,
        LocalEvent::Progress {
            progress: progress(),
            screen: Some(screen()),
        },
    ));
    // Seventeenth review: without the screen-text permission only "running" (was: the
    // step, the app and the action kind).
    assert_eq!(
        p,
        vec![ServerMessage::Progress {
            step: 0,
            phase: Phase::Acting,
            app: None,
            last_action: None,
            screen_text: None,
            waiting_for_unlock: false,
        }]
    );
    // Grant screen text and approvals.
    let full = RemotePermissions {
        start_tasks: true,
        see_progress: true,
        see_screen_text: true,
        approve: true,
    };
    let now = phone.tick();
    let fx = hub.set_permissions(now, device_id.clone(), full).unwrap();
    let out = phone.apply(fx);
    assert!(phone
        .log
        .iter()
        .any(|e| matches!(e, HubEffect::StoreChanged)));
    let changed = messages(pump(&mut phone, 3, &client, out));
    assert_eq!(
        changed,
        vec![ServerMessage::PermissionsChanged { permissions: full }]
    );
    let p = messages(local(
        &mut phone,
        3,
        &client,
        LocalEvent::Progress {
            progress: progress(),
            screen: Some(screen()),
        },
    ));
    let [ServerMessage::Progress {
        screen_text: Some(text),
        ..
    }] = &p[..]
    else {
        panic!("{p:?}")
    };
    assert!(text.contains("Send") && text.contains("Password"), "{text}");
    assert!(
        !text.contains("4111 1111 1111 1111") && !text.contains("hunter2"),
        "{text}"
    );
    assert!(!format!("{p:?}").contains("the typed words"));

    // --- a confirmation declined remotely ----------------------------------------------
    let pending = messages(local(
        &mut phone,
        3,
        &client,
        LocalEvent::ConfirmRequested {
            request: ConfirmRequest {
                category: RiskCategory::SendMessage,
                summary: "Send \"I'm late\" to Sam".into(),
                app: "com.apple.MobileSMS".into(),
                phone_only: false,
                rows: Vec::new(),
                stage: ConfirmStage::Whole,
                verb: ConfirmVerb::Other,
                target: None,
                flow_only: false,
            },
        },
    ));
    let request_id = hub.pending_request_id().unwrap().unwrap();
    assert!(
        matches!(&pending[..], [ServerMessage::ConfirmPending { request_id: r, category: RiskCategory::SendMessage, can_approve: true, .. }] if *r == request_id),
        "{pending:?}"
    );
    phone.log.clear();
    send(
        &mut phone,
        3,
        &client,
        ClientMessage::Decision {
            request_id,
            approved: false,
        },
    );
    assert!(
        phone.log.iter().any(|e| *e
            == HubEffect::Decide {
                request_id,
                approved: false,
                category: RiskCategory::SendMessage,
                device_id: device_id.clone()
            }),
        "{:?}",
        phone.log
    );
    let resolved = messages(local(
        &mut phone,
        3,
        &client,
        LocalEvent::ConfirmResolved {
            request_id,
            approved: false,
        },
    ));
    assert_eq!(
        resolved,
        vec![ServerMessage::ConfirmResolved { request_id }]
    );
    assert_eq!(hub.pending_request_id().unwrap(), None);

    // --- approving a never-remote category is refused ----------------------------------
    assert!(never_remote_approve()
        .unwrap()
        .contains(&RiskCategory::Payment));
    assert!(!remotely_approvable(RiskCategory::Payment).unwrap());
    assert!(remotely_approvable(RiskCategory::SendMessage).unwrap());
    let pending = messages(local(
        &mut phone,
        3,
        &client,
        LocalEvent::ConfirmRequested {
            request: ConfirmRequest {
                category: RiskCategory::Payment,
                summary: "Pay 20 EUR".into(),
                app: "com.example.bank".into(),
                phone_only: false,
                rows: Vec::new(),
                stage: ConfirmStage::Whole,
                verb: ConfirmVerb::Other,
                target: None,
                flow_only: false,
            },
        },
    ));
    let pay_id = hub.pending_request_id().unwrap().unwrap();
    assert!(
        matches!(
            &pending[..],
            [ServerMessage::ConfirmPending {
                can_approve: false,
                ..
            }]
        ),
        "{pending:?}"
    );
    phone.log.clear();
    let refused = messages(send(
        &mut phone,
        3,
        &client,
        ClientMessage::Decision {
            request_id: pay_id,
            approved: true,
        },
    ));
    assert_eq!(
        refused,
        vec![ServerMessage::Denied {
            reason: DenyReason::NotRemotelyApprovable
        }]
    );
    assert!(!phone
        .log
        .iter()
        .any(|e| matches!(e, HubEffect::Decide { .. })));
    assert!(phone.log.iter().any(|e| matches!(
        e,
        HubEffect::Audit {
            entry: AuditEntry {
                event: AuditEvent::Denied {
                    reason: DenyReason::NotRemotelyApprovable,
                    ..
                },
                ..
            }
        }
    )));
    assert_eq!(
        hub.pending_request_id().unwrap(),
        Some(pay_id),
        "still pending on the phone"
    );

    // --- finish ------------------------------------------------------------------------
    let finished = messages(local(
        &mut phone,
        3,
        &client,
        LocalEvent::SessionFinished {
            task_id: Some(task_id),
            outcome: Outcome {
                status: OutcomeStatus::Success,
                summary: "Sent".into(),
                steps: 4,
                duration_ms: 9_000,
            },
        },
    ));
    assert_eq!(
        finished,
        vec![ServerMessage::Finished {
            status: OutcomeStatus::Success,
            summary: "Sent".into()
        }]
    );
    assert!(!hub.is_busy().unwrap());

    // --- rename, persistence -----------------------------------------------------------
    let now = phone.tick();
    let fx = hub
        .rename_device(now, device_id.clone(), " Studio \u{202E}Mac ".into())
        .unwrap();
    assert!(fx.contains(&HubEffect::StoreChanged));
    phone.apply(fx);
    assert_eq!(hub.devices().unwrap()[0].name, "Studio Mac");
    let restored = JarvisRemoteHub::new(phone.keychain.clone(), "Owner's phone".into()).unwrap();
    assert_eq!(restored.devices().unwrap(), hub.devices().unwrap());
    assert_eq!(restored.to_json().unwrap(), hub.to_json().unwrap());

    // --- revoke closes its connections -------------------------------------------------
    phone.log.clear();
    let now = phone.tick();
    let fx = hub.revoke_device(now, device_id.clone()).unwrap();
    phone.apply(fx);
    assert!(phone.log.contains(&HubEffect::Close {
        conn_id: 3,
        reason: CloseReason::Revoked
    }));
    assert!(phone
        .log
        .iter()
        .any(|e| matches!(e, HubEffect::StoreChanged)));
    assert!(hub.devices().unwrap().is_empty());
    assert!(
        !phone.keychain.contains(&key),
        "the key left the Keychain copy"
    );
    assert!(matches!(
        hub.revoke_device(phone.now, device_id.clone()).unwrap_err(),
        JarvisError::RemoteUnknownDevice { .. }
    ));
    // The revoked credentials no longer connect.
    let client = JarvisRemoteClient::connect(credentials.clone(), rng.bytes()).unwrap();
    let now = phone.tick();
    let fx = hub.connection_opened(4, now, rng.bytes()).unwrap();
    let mut to_client = phone.apply(fx);
    let hello = client.receive(to_client.remove(0).1).unwrap();
    let ClientEvent::Send { bytes } = &hello[0] else {
        panic!()
    };
    let fx = hub.receive(4, bytes.clone(), phone.tick()).unwrap();
    assert!(fx.contains(&HubEffect::Close {
        conn_id: 4,
        reason: CloseReason::AuthFailed
    }));

    // --- no secret in anything Swift sees except the three secret outputs --------------
    debug_dump.push(format!("{:?}", phone.log));
    debug_dump.push(format!("{:?}", hub.devices().unwrap()));
    debug_dump.push(format!("{offer:?}"));
    let all = debug_dump.join("\n");
    for secret in [
        key.as_str(),
        code.as_str(),
        code_raw.as_str(),
        &code_raw[..16],
    ] {
        assert!(!all.contains(secret), "a secret leaked into Debug output");
    }
}

#[test]
fn weak_entropy_is_a_typed_error_everywhere() {
    let hub = JarvisRemoteHub::new(String::new(), "Phone".into()).unwrap();
    let cases = [
        (vec![0u8; 32], EntropyProblem::NotRandom),
        (
            (0u8..8).cycle().take(64).collect(),
            EntropyProblem::NotRandom,
        ),
        ((0u8..31).collect(), EntropyProblem::TooShort),
        (vec![], EntropyProblem::TooShort),
    ];
    for (bytes, problem) in cases {
        assert_eq!(
            hub.open_pairing(1, bytes.clone()).unwrap_err(),
            JarvisError::RemoteEntropy { problem }
        );
        assert_eq!(
            hub.connection_opened(9, 1, bytes.clone()).unwrap_err(),
            JarvisError::RemoteEntropy { problem }
        );
        assert!(matches!(
            JarvisRemoteClient::pair("AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AAAA".into(), "Mac".into(), bytes)
                .err()
                .unwrap(),
            JarvisError::RemoteEntropy { problem: p } if p == problem
        ));
    }
    assert!(!hub.is_pairing_open().unwrap());
    assert_eq!(hub.connection_count().unwrap(), 0);
    assert_eq!(remote_min_entropy_len(), 32);
}

#[test]
fn pairing_lockout_limit_and_store_errors_are_typed_and_secret_free() {
    let mut rng = Rng(99);
    let hub = JarvisRemoteHub::new(String::new(), "Phone".into()).unwrap();
    let other = JarvisRemoteHub::new(String::new(), "Other".into()).unwrap();
    let mut now = 1_000;
    let mut conn = 0;
    // Five attempts with the wrong code lock pairing out.
    for _ in 0..5 {
        hub.open_pairing(now, rng.bytes()).unwrap();
        let wrong = other.open_pairing(now, rng.bytes()).unwrap();
        let client =
            JarvisRemoteClient::pair(wrong.code.clone(), "Mallory".into(), rng.bytes()).unwrap();
        conn += 1;
        now += 10;
        let fx = hub.connection_opened(conn, now, rng.bytes()).unwrap();
        let Some(bytes) = fx.iter().find_map(|e| match e {
            HubEffect::Send { bytes, .. } => Some(bytes),
            _ => None,
        }) else {
            panic!("{fx:?}")
        };
        let ev = client.receive(bytes.clone()).unwrap();
        let ClientEvent::Send { bytes } = &ev[0] else {
            panic!()
        };
        now += 10;
        let fx = hub.receive(conn, bytes.clone(), now).unwrap();
        assert!(fx.contains(&HubEffect::Close {
            conn_id: conn,
            reason: CloseReason::AuthFailed
        }));
        for e in &fx {
            assert!(!format!("{e:?}").contains(&wrong.code));
        }
    }
    let until = hub.pairing_locked_until_ms().unwrap();
    assert!(until > hub.clock_ms().unwrap());
    let err = hub.open_pairing(now + 1, rng.bytes()).unwrap_err();
    assert_eq!(err, JarvisError::RemotePairingLocked { until_ms: until });
    assert!(
        !hub.is_pairing_open().unwrap(),
        "no offer left open while locked"
    );
    // Over once the lockout ends.
    hub.open_pairing(until + 1, rng.bytes()).unwrap();

    // Eight devices: the limit.
    let devices: Vec<String> = (0..8)
        .map(|i| {
            format!(
                r#"{{"id":"{:032x}","name":"Device {i}","key":"{}","permissions":{{"start_tasks":true}},"paired_at_ms":1}}"#,
                i + 1,
                format!("{:02x}", 0xA0 + i).repeat(32)
            )
        })
        .collect();
    let full = format!(
        r#"{{"schema_version":2,"nonce_lease":0,"devices":[{}]}}"#,
        devices.join(",")
    );
    let hub = JarvisRemoteHub::new(full, "Phone".into()).unwrap();
    // Missing permission fields are off.
    assert_eq!(
        hub.devices().unwrap()[0].permissions,
        RemotePermissions {
            start_tasks: true,
            ..RemotePermissions {
                start_tasks: false,
                see_progress: false,
                see_screen_text: false,
                approve: false
            }
        }
    );
    assert_eq!(
        hub.open_pairing(1, rng.bytes()).unwrap_err(),
        JarvisError::RemoteLimitReached {
            what: "devices".into(),
            max: 8
        }
    );

    // Unknown devices.
    for bad in ["nope", "00000000000000000000000000000099"] {
        let err = hub.rename_device(1, bad.into(), "x".into()).unwrap_err();
        assert!(
            matches!(err, JarvisError::RemoteUnknownDevice { .. }),
            "{err:?}"
        );
        let err = hub
            .set_permissions(1, bad.into(), new_device_remote_permissions().unwrap())
            .unwrap_err();
        assert!(
            matches!(err, JarvisError::RemoteUnknownDevice { .. }),
            "{err:?}"
        );
    }
    assert_eq!(
        hub.rename_device(1, format!("{:032x}", 1), " \u{200B} ".into())
            .unwrap_err(),
        JarvisError::RemoteInvalidName
    );

    // Store errors never echo the input (which holds keys).
    let key = "ab".repeat(32);
    let leaky = [
        format!(r#"{{"schema_version":2,"nonce_lease":"{key}","devices":[]}}"#),
        format!(r#"{{"schema_version":2,"nonce_lease":0,"devices":[{{"id":"{key}"}}]}}"#),
        format!(
            r#"{{"schema_version":2,"nonce_lease":0,"devices":[{{"id":"{:032x}","name":"","key":"{key}","permissions":{{}},"paired_at_ms":1}}]}}"#,
            1
        ),
        format!(r#"{{"schema_version":3,"key":"{key}"}}"#),
        format!("{{{key}"),
    ];
    for json in leaky {
        let err = JarvisRemoteHub::new(json, "Phone".into()).err().unwrap();
        assert!(
            matches!(err, JarvisError::RemoteStoreInvalid { .. }),
            "{err:?}"
        );
        assert!(!format!("{err:?} {err}").contains(&key[..16]), "{err:?}");
        let err = JarvisRemoteClient::connect(format!(r#"{{"device_key":"{key}"}}"#), rng.bytes())
            .err()
            .unwrap();
        assert!(!format!("{err:?} {err}").contains(&key[..16]), "{err:?}");
    }

    // A bad code is typed and not echoed.
    let typo = "ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZZZ-ZZ!Z";
    let err = JarvisRemoteClient::pair(typo.into(), "Mac".into(), rng.bytes())
        .err()
        .unwrap();
    assert!(
        matches!(err, JarvisError::RemoteInvalidCode { .. }),
        "{err:?}"
    );
    assert!(!format!("{err:?} {err}").contains("ZZZZ"));
    let err = JarvisRemoteClient::pair("ABCD".into(), " ".into(), rng.bytes())
        .err()
        .unwrap();
    assert!(
        matches!(err, JarvisError::RemoteInvalidCode { .. }),
        "{err:?}"
    );
}

#[test]
fn client_misuse_and_local_event_errors() {
    let mut rng = Rng(3);
    let hub = JarvisRemoteHub::new(String::new(), "Phone".into()).unwrap();
    let offer = hub.open_pairing(1, rng.bytes()).unwrap();
    let client = JarvisRemoteClient::pair(offer.code.clone(), "Mac".into(), rng.bytes()).unwrap();
    assert_eq!(
        client.send(ClientMessage::Hello).unwrap_err(),
        JarvisError::RemoteNotConnected
    );
    assert_eq!(client.device_id().unwrap(), None);
    assert_eq!(
        client.receive(vec![0, 0, 0, 1, 9]).unwrap_err(),
        JarvisError::RemoteAuthFailed
    );
    assert_eq!(
        client.receive(vec![]).unwrap_err(),
        JarvisError::RemoteAuthFailed,
        "final"
    );
    let err = JarvisRemoteClient::pair(offer.code.clone(), "\u{200B}".into(), rng.bytes())
        .err()
        .unwrap();
    assert_eq!(err, JarvisError::RemoteInvalidName);

    // Local events with a malformed device id or bad tool JSON are typed errors.
    let err = hub
        .local_event(
            2,
            LocalEvent::DeviceRevoked {
                device_id: "zz".into(),
            },
        )
        .unwrap_err();
    assert!(matches!(err, JarvisError::RemoteUnknownDevice { .. }));
    let mut p = progress();
    p.last_action = Some(Action::CallTool {
        name: "t".into(),
        args_json: "{not json".into(),
    });
    let err = hub
        .local_event(
            2,
            LocalEvent::Progress {
                progress: p,
                screen: None,
            },
        )
        .unwrap_err();
    assert!(matches!(err, JarvisError::InvalidJson { .. }), "{err:?}");
    // A catalog and a well-formed unknown device are fine (ignored by the hub).
    let fx = hub
        .local_event(
            3,
            LocalEvent::CatalogChanged {
                skills: vec![CatalogEntry {
                    id: "wifi-off".into(),
                    name: "Wi-Fi off".into(),
                    summary: String::new(),
                }],
                workflows: vec![],
            },
        )
        .unwrap();
    assert!(fx.is_empty());
    let fx = hub
        .local_event(
            4,
            LocalEvent::PermissionsChanged {
                device_id: format!("{:032x}", 5),
                permissions: RemotePermissions {
                    start_tasks: true,
                    see_progress: false,
                    see_screen_text: false,
                    approve: true,
                },
            },
        )
        .unwrap();
    assert!(fx.is_empty());
    assert_eq!(hub.clock_ms().unwrap(), 4);
}

#[cfg(feature = "panic-hooks")]
#[test]
fn a_panic_poisons_the_hub_and_it_is_rebuilt_from_the_keychain() {
    let hub = JarvisRemoteHub::new(String::new(), "Phone".into()).unwrap();
    let saved = hub.to_json().unwrap();
    hub.arm_poison();
    assert!(matches!(
        hub.to_json().unwrap_err(),
        JarvisError::Poisoned { .. }
    ));
    assert!(matches!(
        hub.tick(1).unwrap_err(),
        JarvisError::Poisoned { .. }
    ));
    let again = JarvisRemoteHub::new(saved, "Phone".into()).unwrap();
    assert!(again.devices().unwrap().is_empty());
}
