//! In-memory harness: a hub, remote clients and a byte pump between them.

#![allow(dead_code)]

use jarvis_proto::{Action, ConfirmRequest, Outcome, OutcomeStatus, Phase, Progress, RiskCategory};
use jarvis_remote::{
    AuditEvent, ClientCredentials, ClientEvent, ClientMessage, CloseReason, ConnId, HubEffect,
    LocalEvent, RemoteClient, RemoteHub, RemotePermissions, ServerMessage,
};

/// Deterministic, distinct 32-byte "entropy" for tests (never do this in the shell).
pub fn entropy(seed: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    for b in out.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x >> 24) as u8;
    }
    out
}

/// Concatenated `Send` bytes for one connection.
pub fn sends(fx: &[HubEffect], conn: ConnId) -> Vec<u8> {
    fx.iter()
        .filter_map(|e| match e {
            HubEffect::Send { conn_id, bytes } if *conn_id == conn => Some(bytes.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The close reason for a connection, if the effects close it.
pub fn closed(fx: &[HubEffect], conn: ConnId) -> Option<CloseReason> {
    fx.iter().find_map(|e| match e {
        HubEffect::Close { conn_id, reason } if *conn_id == conn => Some(*reason),
        _ => None,
    })
}

/// Audit events in order.
pub fn audits(fx: &[HubEffect]) -> Vec<AuditEvent> {
    fx.iter()
        .filter_map(|e| match e {
            HubEffect::Audit { entry } => Some(entry.event.clone()),
            _ => None,
        })
        .collect()
}

/// Effects the phone would act on (not `Send`, `Close`, `Audit` or `StoreChanged`).
pub fn actions(fx: &[HubEffect]) -> Vec<HubEffect> {
    fx.iter()
        .filter(|e| {
            !matches!(
                e,
                HubEffect::Send { .. }
                    | HubEffect::Close { .. }
                    | HubEffect::Audit { .. }
                    | HubEffect::StoreChanged
            )
        })
        .cloned()
        .collect()
}

/// The task id of the first start effect.
pub fn task_id(fx: &[HubEffect]) -> Option<u64> {
    fx.iter().find_map(|e| match e {
        HubEffect::StartTask { task_id, .. }
        | HubEffect::RunSkill { task_id, .. }
        | HubEffect::RunWorkflow { task_id, .. } => Some(*task_id),
        _ => None,
    })
}

/// Server messages from client events.
pub fn messages(events: &[ClientEvent]) -> Vec<ServerMessage> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::Message(m) => Some(m.clone()),
            _ => None,
        })
        .collect()
}

/// One remote connection.
pub struct Link {
    pub conn: ConnId,
    pub client: RemoteClient,
    /// Every byte the client sent, in order (for replay tests).
    pub sent: Vec<Vec<u8>>,
}

/// The phone plus a clock and id counters.
pub struct Rig {
    pub hub: RemoteHub,
    pub now: u64,
    next_conn: ConnId,
    next_seed: u64,
}

impl Rig {
    pub fn new() -> Self {
        Self::with_hub(RemoteHub::new("Test Phone"))
    }

    pub fn with_hub(hub: RemoteHub) -> Self {
        Self {
            hub,
            now: 1_000_000,
            next_conn: 1,
            next_seed: 1,
        }
    }

    pub fn seed(&mut self) -> [u8; 32] {
        self.next_seed += 1;
        entropy(self.next_seed)
    }

    pub fn conn_id(&mut self) -> ConnId {
        self.next_conn += 1;
        self.next_conn
    }

    /// Opens a connection for `client` and runs the handshake as far as it goes.
    /// Returns the link, all hub effects and all client events (or the client error).
    pub fn open(
        &mut self,
        client: RemoteClient,
    ) -> (
        Link,
        Vec<HubEffect>,
        Result<Vec<ClientEvent>, jarvis_remote::ClientError>,
    ) {
        let conn = self.conn_id();
        let seed = self.seed();
        let mut link = Link {
            conn,
            client,
            sent: Vec::new(),
        };
        let mut all_fx = self.hub.connection_opened(conn, self.now, &seed);
        let mut inbound = sends(&all_fx, conn);
        let mut all_events = Vec::new();
        for _ in 0..4 {
            if inbound.is_empty() {
                break;
            }
            let events = match link.client.receive(&inbound) {
                Ok(e) => e,
                Err(e) => return (link, all_fx, Err(e)),
            };
            inbound.clear();
            for ev in &events {
                if let ClientEvent::Send { bytes } = ev {
                    link.sent.push(bytes.clone());
                    let fx = self.hub.receive(conn, bytes, self.now);
                    inbound.extend(sends(&fx, conn));
                    all_fx.extend(fx);
                }
            }
            all_events.extend(events);
        }
        (link, all_fx, Ok(all_events))
    }

    /// Pairs a new device with a fresh offer; panics unless it works.
    pub fn pair(&mut self, name: &str) -> (Link, ClientCredentials) {
        let seed = self.seed();
        let offer = self.hub.open_pairing(self.now, &seed).unwrap();
        let client = RemoteClient::pair(&offer.code, name, &self.seed()).unwrap();
        let (link, fx, events) = self.open(client);
        let events = events.unwrap();
        assert!(
            fx.iter()
                .any(|e| matches!(e, HubEffect::DevicePaired { .. })),
            "{fx:?}"
        );
        let creds = events
            .iter()
            .find_map(|e| match e {
                ClientEvent::Paired { credentials } => Some(credentials.clone()),
                _ => None,
            })
            .unwrap();
        assert!(events.contains(&ClientEvent::Connected));
        assert!(link.client.is_connected());
        assert!(self.hub.is_authenticated(link.conn));
        (link, creds)
    }

    /// Attempts a connect with these credentials and fresh test entropy.
    pub fn try_connect(
        &mut self,
        creds: &ClientCredentials,
    ) -> (
        Link,
        Vec<HubEffect>,
        Result<Vec<ClientEvent>, jarvis_remote::ClientError>,
    ) {
        let seed = self.seed();
        self.open(RemoteClient::connect(creds.clone(), &seed).unwrap())
    }

    /// Connects with stored credentials; panics unless it works.
    pub fn connect(&mut self, creds: &ClientCredentials) -> Link {
        let client = RemoteClient::connect(creds.clone(), &self.seed()).unwrap();
        let (link, _fx, events) = self.open(client);
        assert!(events.unwrap().contains(&ClientEvent::Connected));
        assert!(self.hub.is_authenticated(link.conn));
        link
    }

    /// Feeds raw bytes from `link` to the hub and the replies back to the client.
    pub fn feed(&mut self, link: &mut Link, bytes: &[u8]) -> (Vec<HubEffect>, Vec<ServerMessage>) {
        let fx = self.hub.receive(link.conn, bytes, self.now);
        let back = sends(&fx, link.conn);
        let msgs = if back.is_empty() {
            Vec::new()
        } else {
            messages(&link.client.receive(&back).unwrap())
        };
        (fx, msgs)
    }

    /// Sends one command and returns the hub effects and the replies.
    pub fn request(
        &mut self,
        link: &mut Link,
        msg: ClientMessage,
    ) -> (Vec<HubEffect>, Vec<ServerMessage>) {
        let bytes = link.client.send(&msg).unwrap();
        link.sent.push(bytes.clone());
        self.feed(link, &bytes)
    }

    /// A local event; returns the effects.
    pub fn local(&mut self, ev: LocalEvent) -> Vec<HubEffect> {
        self.hub.local_event(self.now, ev)
    }

    /// Changes a device's permissions and delivers the announcement to `link`.
    pub fn set_permissions(
        &mut self,
        link: &mut Link,
        creds: &ClientCredentials,
        p: RemotePermissions,
    ) -> Vec<ServerMessage> {
        let fx = self.local(LocalEvent::PermissionsChanged {
            device_id: creds.device_id,
            permissions: p,
        });
        deliver(link, &fx)
    }

    /// A local event, delivered to `link`; returns what it received.
    pub fn local_to(&mut self, link: &mut Link, ev: LocalEvent) -> Vec<ServerMessage> {
        let fx = self.local(ev);
        deliver(link, &fx)
    }

    /// Resolves the pending confirmation (by its real id) and delivers to `link`.
    pub fn resolve_to(&mut self, link: &mut Link, approved: bool) -> Vec<ServerMessage> {
        let request_id = self.hub.pending_request_id().unwrap();
        self.local_to(
            link,
            LocalEvent::ConfirmResolved {
                request_id,
                approved,
            },
        )
    }
}

/// What `link` receives from these hub effects.
pub fn deliver(link: &mut Link, fx: &[HubEffect]) -> Vec<ServerMessage> {
    let back = sends(fx, link.conn);
    if back.is_empty() {
        return Vec::new();
    }
    messages(&link.client.receive(&back).unwrap())
}

pub fn progress_event(step: u32, action: Action, screen_text: Option<&str>) -> LocalEvent {
    LocalEvent::Progress {
        progress: Progress {
            step,
            phase: Phase::Acting,
            last_action: Some(action),
            app: Some("com.apple.MobileSMS".into()),
            model_calls: step,
            ..Progress::default()
        },
        screen_text: screen_text.map(str::to_owned),
    }
}

pub fn confirm_event(category: RiskCategory, summary: &str) -> LocalEvent {
    LocalEvent::ConfirmRequested {
        request: ConfirmRequest {
            category,
            summary: summary.into(),
            app: "com.apple.MobileSMS".into(),
            phone_only: false,
        },
    }
}

pub fn started(task_id: Option<u64>) -> LocalEvent {
    LocalEvent::SessionStarted { task_id }
}

pub fn finished(task_id: Option<u64>, status: OutcomeStatus, summary: &str) -> LocalEvent {
    LocalEvent::SessionFinished {
        task_id,
        outcome: Outcome {
            status,
            summary: summary.into(),
            steps: 3,
            duration_ms: 1234,
        },
    }
}
