//! The phone's side: [`RemoteHub`], a sans-IO state machine.
//!
//! The shell feeds it what happens (a connection opened, bytes arrived, the agent made
//! progress, the owner revoked a device) and carries out the [`HubEffect`]s it returns,
//! in order. The hub never reads a clock or a random source: `now_ms` (monotonic
//! milliseconds) and 32-byte `entropy` (from the OS CSPRNG, e.g. `SecRandomCopyBytes`)
//! arrive as arguments.

use std::collections::BTreeMap;

use jarvis_proto::{ConfirmRequest, Outcome, OutcomeStatus, Progress, RiskCategory};
use serde::{Deserialize, Serialize};

use crate::codec::PROTOCOL_VERSION;
use crate::codec::{encode_record, DeviceId, Kind, Record, RecordError, RecordReader};
use crate::crypto::{
    auth_key, connect_phone_mac, derive_device_id, derive_device_key, pair_key, pair_phone_mac,
    verify_connect_client_mac, verify_pair_client_mac, Channel, PairingSecret, Side, MAC_LEN,
    NONCE_LEN,
};
use crate::error::{HubError, MessageError, PersistError};
use crate::message::{CatalogEntry, ClientMessage, DenyReason, ErrorCode, ServerMessage};
use crate::pairing::{format_code, qr_payload, secret_from_entropy, PairingOffer, PAIRING_TTL_MS};
use crate::permissions::{remotely_decidable, RemotePermissions};
use crate::store::{
    clean_name, is_format, DeviceInfo, DeviceRecord, DeviceStore, MAX_DEVICES,
    MAX_DEVICE_NAME_BYTES,
};

/// The shell's identifier for one transport connection.
pub type ConnId = u64;

/// Most simultaneous connections (authenticated or not).
pub const MAX_CONNECTIONS: usize = 8;
/// A connection must finish its handshake this soon after opening, in milliseconds.
pub const HANDSHAKE_TIMEOUT_MS: u64 = 10_000;
/// An authenticated connection that sends nothing for this long is closed (clients ping).
pub const IDLE_TIMEOUT_MS: u64 = 120_000;
/// Length of a rate-limit window, in milliseconds.
pub const RATE_WINDOW_MS: u64 = 1_000;
/// Messages accepted per connection per window; the rest are dropped.
pub const RATE_MAX_MESSAGES: u32 = 20;
/// Failed handshakes (across all connections) before the lockout starts.
pub const LOCKOUT_THRESHOLD: u32 = 5;
/// First lockout, in milliseconds; doubles with every further failure.
pub const LOCKOUT_BASE_MS: u64 = 5_000;
/// Longest lockout, in milliseconds.
pub const LOCKOUT_MAX_MS: u64 = 10 * 60_000;
/// A failure counter with no new failure for this long starts again from zero.
pub const FAILURE_MEMORY_MS: u64 = 15 * 60_000;
/// Longest accepted task text, in characters.
pub const MAX_TASK_TEXT_CHARS: usize = 2_000;
/// Longest accepted skill or workflow id, in characters.
pub const MAX_ID_CHARS: usize = 128;
/// Longest screen text forwarded in one progress event, in characters.
pub const MAX_SCREEN_TEXT_CHARS: usize = 4_000;
/// Longest forwarded summary, in characters.
pub const MAX_SUMMARY_CHARS: usize = 500;
/// Most catalog entries kept per list.
pub const MAX_CATALOG_ENTRIES: usize = 64;

/// Why the hub closed a connection. For the shell and the audit log only: the remote is
/// told nothing, it just sees the connection close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    /// A handshake or frame failed authentication, or arrived out of order. Opaque on
    /// purpose: wrong code, expired code, unknown device, bad MAC, replay and bit flips
    /// all look the same.
    AuthFailed,
    /// Handshakes are locked out after repeated failures.
    LockedOut,
    /// The handshake did not finish within [`HANDSHAKE_TIMEOUT_MS`].
    HandshakeTimeout,
    /// Nothing received for [`IDLE_TIMEOUT_MS`].
    IdleTimeout,
    /// [`MAX_CONNECTIONS`] already open.
    TooManyConnections,
    /// The shell reused a connection id that is still open.
    DuplicateConnection,
    /// A record was larger than allowed or the buffer limit was hit.
    TooLarge,
    /// The device was revoked.
    Revoked,
    /// Pairing succeeded cryptographically but [`MAX_DEVICES`] are already paired.
    TooManyDevices,
    /// The peer closed the connection.
    PeerClosed,
    /// An internal error (key schedule or counter exhaustion).
    Internal,
}

/// One entry of the phone's remote-control audit log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Shell clock.
    pub at_ms: u64,
    /// Connection, when one is involved.
    pub conn_id: Option<ConnId>,
    /// Authenticated device, when known.
    pub device_id: Option<DeviceId>,
    /// What happened.
    pub event: AuditEvent,
}

/// What an [`AuditEntry`] records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditEvent {
    /// The pairing offer expired unused.
    PairingExpired,
    /// A device was paired.
    Paired {
        /// Its name.
        name: String,
    },
    /// A device authenticated on a connection.
    Connected,
    /// A connection was closed by the hub or the peer.
    Closed {
        /// Why.
        reason: CloseReason,
    },
    /// A connection was refused before any handshake work.
    ConnectionRefused {
        /// Why.
        reason: CloseReason,
    },
    /// A pairing or connect handshake failed.
    HandshakeFailed {
        /// Failures counted so far.
        failures: u32,
    },
    /// Handshakes are locked out until `until_ms`.
    LockedOut {
        /// End of the lockout (shell clock).
        until_ms: u64,
    },
    /// A request was carried out.
    Accepted {
        /// The message type.
        request: String,
        /// Redacted detail (task text, skill id, decision).
        detail: Option<String>,
    },
    /// A request was refused by policy.
    Denied {
        /// The message type.
        request: String,
        /// Why.
        reason: DenyReason,
    },
    /// An authenticated message could not be served.
    Rejected {
        /// The message type, when it could be read.
        request: Option<String>,
        /// Why.
        code: ErrorCode,
    },
    /// The owner revoked a device.
    Revoked,
    /// The owner changed a device's permissions.
    PermissionsChanged {
        /// The new permissions.
        permissions: RemotePermissions,
    },
}

/// What the shell must do. Carry them out in order.
#[derive(Debug, Clone, PartialEq)]
pub enum HubEffect {
    /// Write `bytes` to the connection.
    Send {
        /// Connection.
        conn_id: ConnId,
        /// Bytes to write.
        bytes: Vec<u8>,
    },
    /// Close the connection. The hub has already forgotten it.
    Close {
        /// Connection.
        conn_id: ConnId,
        /// Why (for logs; nothing is sent to the remote).
        reason: CloseReason,
    },
    /// Start a free-text task on behalf of a device. Report back with
    /// [`LocalEvent::SessionStarted`] (or [`LocalEvent::SessionFinished`] if it cannot
    /// start).
    StartTask {
        /// Requesting device.
        device_id: DeviceId,
        /// Cleaned task text.
        text: String,
    },
    /// Run a listed skill on behalf of a device.
    RunSkill {
        /// Requesting device.
        device_id: DeviceId,
        /// Skill id (present in the catalog).
        id: String,
        /// Arguments as sent.
        args: BTreeMap<String, serde_json::Value>,
    },
    /// Run a listed workflow on behalf of a device.
    RunWorkflow {
        /// Requesting device.
        device_id: DeviceId,
        /// Workflow id (present in the catalog).
        id: String,
    },
    /// Cancel the running task (a remote asked, or the device that started it was
    /// revoked).
    CancelTask {
        /// Requesting (or revoked) device.
        device_id: DeviceId,
    },
    /// Answer the pending confirmation. Feed the session `UserDecision { approved }` only
    /// if `request_id` is still the confirmation on screen.
    Decide {
        /// Hub request id (see [`RemoteHub::pending_request_id`]).
        request_id: u64,
        /// The answer.
        approved: bool,
        /// Deciding device, for the task log.
        device_id: DeviceId,
    },
    /// A device was paired; persist [`RemoteHub::to_json`] to the Keychain.
    DevicePaired {
        /// The new device.
        device: DeviceInfo,
    },
    /// Append to the audit log.
    Audit {
        /// The entry.
        entry: AuditEntry,
    },
}

/// What happened on the phone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalEvent {
    /// A session started (remotely requested or started on the phone).
    SessionStarted,
    /// The session made progress.
    Progress {
        /// The session's progress snapshot.
        progress: Progress,
        /// Screen text for remotes with `see_screen_text`. Build it from
        /// `jarvis_safety::redact_screen` output (secure fields already empty); the hub
        /// redacts it again before sending.
        screen_text: Option<String>,
    },
    /// The session asks for a confirmation.
    ConfirmRequested {
        /// The request.
        request: ConfirmRequest,
    },
    /// The pending confirmation was answered (on the phone or after a `Decide`).
    ConfirmResolved {
        /// The answer.
        approved: bool,
    },
    /// The session ended (or a requested start failed).
    SessionFinished {
        /// How it ended.
        outcome: Outcome,
    },
    /// The owner revoked a device: its key is forgotten and its connections close now.
    DeviceRevoked {
        /// The device.
        device_id: DeviceId,
    },
    /// The owner changed a device's permissions.
    PermissionsChanged {
        /// The device.
        device_id: DeviceId,
        /// The new permissions.
        permissions: RemotePermissions,
    },
    /// The skills and workflows remotes may list and run.
    CatalogChanged {
        /// Skills.
        skills: Vec<CatalogEntry>,
        /// Workflows.
        workflows: Vec<CatalogEntry>,
    },
}

#[derive(Debug)]
struct Offer {
    secret: PairingSecret,
    expires_at_ms: u64,
}

#[derive(Debug)]
enum Phase {
    Handshake {
        np: [u8; NONCE_LEN],
    },
    Established {
        channel: Box<Channel>,
        device_id: DeviceId,
        subscribed: bool,
        window_start_ms: u64,
        window_count: u32,
    },
}

#[derive(Debug)]
struct Conn {
    opened_at_ms: u64,
    last_rx_ms: u64,
    reader: RecordReader,
    phase: Phase,
}

#[derive(Debug, Clone, Copy)]
struct Task {
    origin: Option<DeviceId>,
}

#[derive(Debug, Clone)]
struct Pending {
    id: u64,
    category: RiskCategory,
    summary: String,
    decided: bool,
}

/// The phone's remote-control hub. See the crate docs for the protocol and the rules.
#[derive(Debug)]
pub struct RemoteHub {
    phone_name: String,
    devices: Vec<DeviceRecord>,
    offer: Option<Offer>,
    conns: BTreeMap<ConnId, Conn>,
    failures: u32,
    last_failure_ms: u64,
    locked_until_ms: u64,
    task: Option<Task>,
    pending: Option<Pending>,
    next_request_id: u64,
    skills: Vec<CatalogEntry>,
    workflows: Vec<CatalogEntry>,
    last_now_ms: u64,
}

impl RemoteHub {
    /// A hub with no paired devices.
    pub fn new(phone_name: &str) -> Self {
        Self::with_store(phone_name, DeviceStore::default())
    }

    /// A hub with previously paired devices.
    pub fn with_store(phone_name: &str, store: DeviceStore) -> Self {
        let mut devices = store.devices;
        devices.truncate(MAX_DEVICES);
        Self {
            phone_name: clean_name(phone_name),
            devices,
            offer: None,
            conns: BTreeMap::new(),
            failures: 0,
            last_failure_ms: 0,
            locked_until_ms: 0,
            task: None,
            pending: None,
            next_request_id: 1,
            skills: Vec::new(),
            workflows: Vec::new(),
            last_now_ms: 0,
        }
    }

    /// Restores a hub from [`RemoteHub::to_json`] output.
    pub fn from_json(phone_name: &str, json: &str) -> Result<Self, PersistError> {
        Ok(Self::with_store(phone_name, DeviceStore::from_json(json)?))
    }

    /// The paired devices, keys included, for the Keychain (see [`DeviceStore`]).
    pub fn to_json(&self) -> String {
        self.store().to_json()
    }

    /// A copy of the paired-device list, keys included.
    pub fn store(&self) -> DeviceStore {
        DeviceStore {
            devices: self.devices.clone(),
        }
    }

    /// The paired devices without keys.
    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.devices.iter().map(DeviceRecord::info).collect()
    }

    /// The id of the confirmation remotes may currently answer, if any.
    pub fn pending_request_id(&self) -> Option<u64> {
        self.pending.as_ref().map(|p| p.id)
    }

    /// Whether a task is running (or was requested and not yet reported finished).
    pub fn is_busy(&self) -> bool {
        self.task.is_some()
    }

    /// Open connections.
    pub fn connection_count(&self) -> usize {
        self.conns.len()
    }

    /// Whether `conn_id` has completed a handshake.
    pub fn is_authenticated(&self, conn_id: ConnId) -> bool {
        matches!(
            self.conns.get(&conn_id).map(|c| &c.phase),
            Some(Phase::Established { .. })
        )
    }

    /// End of the current handshake lockout (0 when none has happened).
    pub fn locked_until_ms(&self) -> u64 {
        self.locked_until_ms
    }

    /// Creates a single-use pairing offer from 32 bytes of CSPRNG output (160 bits are
    /// used), valid for [`PAIRING_TTL_MS`]. Replaces any open offer.
    pub fn open_pairing(
        &mut self,
        now_ms: u64,
        entropy: [u8; 32],
    ) -> Result<PairingOffer, HubError> {
        self.observe_clock(now_ms);
        if self.devices.len() >= MAX_DEVICES {
            return Err(HubError::TooManyDevices);
        }
        let secret = secret_from_entropy(&entropy);
        let expires_at_ms = now_ms.saturating_add(PAIRING_TTL_MS);
        let offer = PairingOffer {
            code: format_code(&secret),
            qr_payload: qr_payload(&secret),
            expires_at_ms,
        };
        self.offer = Some(Offer {
            secret,
            expires_at_ms,
        });
        Ok(offer)
    }

    /// Withdraws the open pairing offer.
    pub fn cancel_pairing(&mut self) {
        self.offer = None;
    }

    /// Whether a pairing offer is open.
    pub fn pairing_open(&self) -> bool {
        self.offer.is_some()
    }

    /// A transport connection opened. `entropy` (CSPRNG) becomes the phone's nonce for
    /// this connection.
    pub fn connection_opened(
        &mut self,
        conn_id: ConnId,
        now_ms: u64,
        entropy: [u8; 32],
    ) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        self.expire(now_ms, &mut fx);
        if self.conns.remove(&conn_id).is_some() {
            self.close_effect(conn_id, None, CloseReason::DuplicateConnection, &mut fx);
            return fx;
        }
        if self.conns.len() >= MAX_CONNECTIONS {
            fx.push(HubEffect::Close {
                conn_id,
                reason: CloseReason::TooManyConnections,
            });
            self.audit(
                Some(conn_id),
                None,
                AuditEvent::ConnectionRefused {
                    reason: CloseReason::TooManyConnections,
                },
                &mut fx,
            );
            return fx;
        }
        self.conns.insert(
            conn_id,
            Conn {
                opened_at_ms: now_ms,
                last_rx_ms: now_ms,
                reader: RecordReader::new(),
                phase: Phase::Handshake { np: entropy },
            },
        );
        fx.push(HubEffect::Send {
            conn_id,
            bytes: encode_record(Kind::ServerHello, &entropy),
        });
        fx
    }

    /// The transport connection closed. Idempotent.
    pub fn connection_closed(&mut self, conn_id: ConnId) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        if let Some(conn) = self.conns.remove(&conn_id) {
            if let Phase::Established { device_id, .. } = conn.phase {
                self.audit(
                    Some(conn_id),
                    Some(device_id),
                    AuditEvent::Closed {
                        reason: CloseReason::PeerClosed,
                    },
                    &mut fx,
                );
            }
        }
        fx
    }

    /// Applies timeouts: handshake and idle limits, pairing-offer expiry. Call it every
    /// few seconds; every other entry point applies it too.
    pub fn tick(&mut self, now_ms: u64) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        self.expire(now_ms, &mut fx);
        fx
    }

    /// Bytes arrived on a connection. Unknown connection ids are ignored.
    pub fn receive(&mut self, conn_id: ConnId, bytes: &[u8], now_ms: u64) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        self.expire(now_ms, &mut fx);
        let mut rest = bytes;
        loop {
            let Some(conn) = self.conns.get_mut(&conn_id) else {
                return fx;
            };
            let take = conn.reader.room().min(rest.len());
            let (chunk, tail) = rest.split_at_checked(take).unwrap_or((rest, &[]));
            if conn.reader.push(chunk).is_err() || (take == 0 && !rest.is_empty()) {
                self.close(conn_id, CloseReason::TooLarge, &mut fx);
                return fx;
            }
            rest = tail;
            loop {
                let Some(conn) = self.conns.get_mut(&conn_id) else {
                    return fx;
                };
                match conn.reader.next_record() {
                    Ok(Some(record)) => self.handle_record(conn_id, record, now_ms, &mut fx),
                    Ok(None) => break,
                    Err(e) => {
                        let reason = match e {
                            RecordError::TooLarge | RecordError::BufferFull => {
                                CloseReason::TooLarge
                            }
                            RecordError::Malformed => CloseReason::AuthFailed,
                        };
                        self.fail(conn_id, reason, now_ms, &mut fx);
                        return fx;
                    }
                }
            }
            if rest.is_empty() {
                return fx;
            }
        }
    }

    /// Something happened on the phone.
    pub fn local_event(&mut self, now_ms: u64, event: LocalEvent) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        self.expire(now_ms, &mut fx);
        match event {
            LocalEvent::SessionStarted => {
                let origin = self.task.and_then(|t| t.origin);
                self.task = Some(Task { origin });
                self.broadcast(&mut fx, |hub, c| {
                    (c.subscribed || c.device_id == origin).then_some(())?;
                    hub.permissions(c.device_id)?;
                    Some(ServerMessage::TaskStarted)
                });
            }
            LocalEvent::Progress {
                progress,
                screen_text,
            } => {
                self.broadcast(&mut fx, |hub, c| {
                    let p = hub.permissions(c.device_id)?;
                    (c.subscribed && p.see_progress).then_some(())?;
                    Some(ServerMessage::Progress {
                        step: progress.step,
                        phase: progress.phase,
                        app: progress.app.as_deref().map(|a| clean_text(a, 256, false)),
                        last_action: progress.last_action.as_ref().map(|a| a.name().to_owned()),
                        screen_text: if p.see_screen_text {
                            screen_text
                                .as_deref()
                                .map(|t| redact_text(t, MAX_SCREEN_TEXT_CHARS, true))
                        } else {
                            None
                        },
                    })
                });
            }
            LocalEvent::ConfirmRequested { request } => {
                let id = self.next_request_id;
                self.next_request_id = self.next_request_id.saturating_add(1);
                self.pending = Some(Pending {
                    id,
                    category: request.category,
                    summary: request.summary,
                    decided: false,
                });
                self.broadcast_pending(&mut fx);
            }
            LocalEvent::ConfirmResolved { .. } => {
                if let Some(p) = self.pending.take() {
                    self.broadcast(&mut fx, |hub, c| {
                        let perms = hub.permissions(c.device_id)?;
                        (c.subscribed || perms.can_decide(p.category)).then_some(())?;
                        Some(ServerMessage::ConfirmResolved { request_id: p.id })
                    });
                }
            }
            LocalEvent::SessionFinished { outcome } => {
                let origin = self.task.take().and_then(|t| t.origin);
                self.pending = None;
                self.broadcast(&mut fx, |hub, c| {
                    let p = hub.permissions(c.device_id)?;
                    (c.subscribed || c.device_id == origin).then_some(())?;
                    let summary = if p.see_screen_text {
                        redact_text(&outcome.summary, MAX_SUMMARY_CHARS, false)
                    } else {
                        generic_finish(outcome.status).to_owned()
                    };
                    Some(ServerMessage::Finished {
                        status: outcome.status,
                        summary,
                    })
                });
            }
            LocalEvent::DeviceRevoked { device_id } => self.revoke(device_id, &mut fx),
            LocalEvent::PermissionsChanged {
                device_id,
                permissions,
            } => {
                let Some(dev) = self.devices.iter_mut().find(|d| d.id == device_id) else {
                    return fx;
                };
                dev.permissions = permissions;
                self.audit(
                    None,
                    Some(device_id),
                    AuditEvent::PermissionsChanged { permissions },
                    &mut fx,
                );
                for conn in self.conns.values_mut() {
                    if let Phase::Established {
                        device_id: d,
                        subscribed,
                        ..
                    } = &mut conn.phase
                    {
                        if *d == device_id && !permissions.see_progress {
                            *subscribed = false;
                        }
                    }
                }
                self.broadcast(&mut fx, |_, c| {
                    (c.device_id == Some(device_id))
                        .then_some(ServerMessage::PermissionsChanged { permissions })
                });
            }
            LocalEvent::CatalogChanged { skills, workflows } => {
                self.skills = clean_catalog(skills);
                self.workflows = clean_catalog(workflows);
            }
        }
        fx
    }

    // ---- internals -------------------------------------------------------------------

    fn observe_clock(&mut self, now_ms: u64) {
        self.last_now_ms = self.last_now_ms.max(now_ms);
    }

    fn permissions(&self, device_id: Option<DeviceId>) -> Option<RemotePermissions> {
        let id = device_id?;
        self.devices
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.permissions)
    }

    fn audit(
        &self,
        conn_id: Option<ConnId>,
        device_id: Option<DeviceId>,
        event: AuditEvent,
        fx: &mut Vec<HubEffect>,
    ) {
        fx.push(HubEffect::Audit {
            entry: AuditEntry {
                at_ms: self.last_now_ms,
                conn_id,
                device_id,
                event,
            },
        });
    }

    fn expire(&mut self, now_ms: u64, fx: &mut Vec<HubEffect>) {
        self.observe_clock(now_ms);
        if self
            .offer
            .as_ref()
            .is_some_and(|o| now_ms >= o.expires_at_ms)
        {
            self.offer = None;
            self.audit(None, None, AuditEvent::PairingExpired, fx);
        }
        let expired: Vec<(ConnId, CloseReason)> = self
            .conns
            .iter()
            .filter_map(|(id, c)| match c.phase {
                Phase::Handshake { .. }
                    if now_ms.saturating_sub(c.opened_at_ms) >= HANDSHAKE_TIMEOUT_MS =>
                {
                    Some((*id, CloseReason::HandshakeTimeout))
                }
                Phase::Established { .. }
                    if now_ms.saturating_sub(c.last_rx_ms) >= IDLE_TIMEOUT_MS =>
                {
                    Some((*id, CloseReason::IdleTimeout))
                }
                _ => None,
            })
            .collect();
        for (id, reason) in expired {
            self.close(id, reason, fx);
        }
    }

    /// Forgets the connection and emits `Close` plus an audit entry.
    fn close(&mut self, conn_id: ConnId, reason: CloseReason, fx: &mut Vec<HubEffect>) {
        let device = self.conns.remove(&conn_id).and_then(|c| match c.phase {
            Phase::Established { device_id, .. } => Some(device_id),
            Phase::Handshake { .. } => None,
        });
        self.close_effect(conn_id, device, reason, fx);
    }

    fn close_effect(
        &self,
        conn_id: ConnId,
        device: Option<DeviceId>,
        reason: CloseReason,
        fx: &mut Vec<HubEffect>,
    ) {
        fx.push(HubEffect::Close { conn_id, reason });
        self.audit(Some(conn_id), device, AuditEvent::Closed { reason }, fx);
    }

    /// A failure on a connection: counted towards the lockout while in the handshake.
    fn fail(&mut self, conn_id: ConnId, reason: CloseReason, now_ms: u64, fx: &mut Vec<HubEffect>) {
        let in_handshake = matches!(
            self.conns.get(&conn_id).map(|c| &c.phase),
            Some(Phase::Handshake { .. })
        );
        if in_handshake {
            self.count_failure(conn_id, now_ms, fx);
        }
        self.close(conn_id, reason, fx);
    }

    fn count_failure(&mut self, conn_id: ConnId, now_ms: u64, fx: &mut Vec<HubEffect>) {
        if self.failures > 0 && now_ms.saturating_sub(self.last_failure_ms) >= FAILURE_MEMORY_MS {
            self.failures = 0;
        }
        self.failures = self.failures.saturating_add(1);
        self.last_failure_ms = now_ms;
        self.audit(
            Some(conn_id),
            None,
            AuditEvent::HandshakeFailed {
                failures: self.failures,
            },
            fx,
        );
        if self.failures >= LOCKOUT_THRESHOLD {
            let shift = (self.failures - LOCKOUT_THRESHOLD).min(16);
            let ms = LOCKOUT_BASE_MS
                .saturating_mul(1u64 << shift)
                .min(LOCKOUT_MAX_MS);
            self.locked_until_ms = now_ms.saturating_add(ms);
            self.audit(
                Some(conn_id),
                None,
                AuditEvent::LockedOut {
                    until_ms: self.locked_until_ms,
                },
                fx,
            );
        }
    }

    fn handle_record(
        &mut self,
        conn_id: ConnId,
        record: Record,
        now_ms: u64,
        fx: &mut Vec<HubEffect>,
    ) {
        let Some(conn) = self.conns.get(&conn_id) else {
            return;
        };
        let kind = Kind::from_byte(record.kind);
        if record.version != PROTOCOL_VERSION || kind.is_none() {
            self.fail(conn_id, CloseReason::AuthFailed, now_ms, fx);
            return;
        }
        match (&conn.phase, kind) {
            (Phase::Handshake { np }, Some(Kind::PairRequest)) => {
                let np = *np;
                if self.check_lockout(conn_id, now_ms, fx) {
                    self.handle_pair(conn_id, np, &record.payload, now_ms, fx);
                }
            }
            (Phase::Handshake { np }, Some(Kind::ConnectRequest)) => {
                let np = *np;
                if self.check_lockout(conn_id, now_ms, fx) {
                    self.handle_connect(conn_id, np, &record.payload, now_ms, fx);
                }
            }
            (Phase::Established { .. }, Some(Kind::Data)) => {
                self.handle_data(conn_id, &record.payload, now_ms, fx);
            }
            _ => self.fail(conn_id, CloseReason::AuthFailed, now_ms, fx),
        }
    }

    /// True when handshakes are allowed; otherwise closes the connection.
    fn check_lockout(&mut self, conn_id: ConnId, now_ms: u64, fx: &mut Vec<HubEffect>) -> bool {
        if now_ms < self.locked_until_ms {
            self.conns.remove(&conn_id);
            fx.push(HubEffect::Close {
                conn_id,
                reason: CloseReason::LockedOut,
            });
            self.audit(
                Some(conn_id),
                None,
                AuditEvent::ConnectionRefused {
                    reason: CloseReason::LockedOut,
                },
                fx,
            );
            return false;
        }
        true
    }

    fn handle_pair(
        &mut self,
        conn_id: ConnId,
        np: [u8; NONCE_LEN],
        payload: &[u8],
        now_ms: u64,
        fx: &mut Vec<HubEffect>,
    ) {
        let Some((nc, name, mac)) = parse_pair_request(payload) else {
            self.fail(conn_id, CloseReason::AuthFailed, now_ms, fx);
            return;
        };
        let secret = match &self.offer {
            Some(o) if now_ms < o.expires_at_ms => o.secret.clone(),
            _ => {
                self.fail(conn_id, CloseReason::AuthFailed, now_ms, fx);
                return;
            }
        };
        let Ok(pk) = pair_key(&secret) else {
            self.close(conn_id, CloseReason::Internal, fx);
            return;
        };
        if verify_pair_client_mac(&pk, &np, &nc, &name, &mac).is_err() {
            self.fail(conn_id, CloseReason::AuthFailed, now_ms, fx);
            return;
        }
        // Authenticated: the offer is used up whatever happens next.
        self.offer = None;
        self.failures = 0;
        if self.devices.len() >= MAX_DEVICES {
            self.close(conn_id, CloseReason::TooManyDevices, fx);
            return;
        }
        let derived = derive_device_id(&secret, &np, &nc).and_then(|id| {
            let key = derive_device_key(&secret, &np, &nc, &id, &name)?;
            let channel = Channel::new(Side::Phone, &key, id, &np, &nc)?;
            let mac = pair_phone_mac(&pk, &np, &nc, &name, &id)?;
            Ok((id, key, channel, mac))
        });
        let Ok((device_id, key, channel, phone_mac)) = derived else {
            self.close(conn_id, CloseReason::Internal, fx);
            return;
        };
        if self.devices.iter().any(|d| d.id == device_id) {
            self.close(conn_id, CloseReason::Internal, fx);
            return;
        }
        let record = DeviceRecord {
            id: device_id,
            name: name.clone(),
            key,
            permissions: RemotePermissions::default(),
            paired_at_ms: now_ms,
        };
        let info = record.info();
        self.devices.push(record);
        let mut body = Vec::with_capacity(16 + MAC_LEN);
        body.extend_from_slice(&device_id.0);
        body.extend_from_slice(&phone_mac);
        self.establish(conn_id, channel, device_id, now_ms);
        fx.push(HubEffect::Send {
            conn_id,
            bytes: encode_record(Kind::PairAccept, &body),
        });
        fx.push(HubEffect::DevicePaired { device: info });
        self.audit(
            Some(conn_id),
            Some(device_id),
            AuditEvent::Paired { name },
            fx,
        );
        self.audit(Some(conn_id), Some(device_id), AuditEvent::Connected, fx);
    }

    fn handle_connect(
        &mut self,
        conn_id: ConnId,
        np: [u8; NONCE_LEN],
        payload: &[u8],
        now_ms: u64,
        fx: &mut Vec<HubEffect>,
    ) {
        let Some((device_id, nc, mac)) = parse_connect_request(payload) else {
            self.fail(conn_id, CloseReason::AuthFailed, now_ms, fx);
            return;
        };
        let Some(dev) = self.devices.iter().find(|d| d.id == device_id) else {
            self.fail(conn_id, CloseReason::AuthFailed, now_ms, fx);
            return;
        };
        let verified = auth_key(&dev.key).and_then(|ak| {
            verify_connect_client_mac(&ak, &device_id, &np, &nc, &mac)?;
            let reply = connect_phone_mac(&ak, &device_id, &np, &nc)?;
            let channel = Channel::new(Side::Phone, &dev.key, device_id, &np, &nc)?;
            Ok((reply, channel))
        });
        let Ok((reply, channel)) = verified else {
            self.fail(conn_id, CloseReason::AuthFailed, now_ms, fx);
            return;
        };
        self.failures = 0;
        self.establish(conn_id, channel, device_id, now_ms);
        fx.push(HubEffect::Send {
            conn_id,
            bytes: encode_record(Kind::ConnectAccept, &reply),
        });
        self.audit(Some(conn_id), Some(device_id), AuditEvent::Connected, fx);
    }

    fn establish(&mut self, conn_id: ConnId, channel: Channel, device_id: DeviceId, now_ms: u64) {
        if let Some(conn) = self.conns.get_mut(&conn_id) {
            conn.last_rx_ms = now_ms;
            conn.phase = Phase::Established {
                channel: Box::new(channel),
                device_id,
                subscribed: false,
                window_start_ms: now_ms,
                window_count: 0,
            };
        }
    }

    fn handle_data(
        &mut self,
        conn_id: ConnId,
        payload: &[u8],
        now_ms: u64,
        fx: &mut Vec<HubEffect>,
    ) {
        let Some(conn) = self.conns.get_mut(&conn_id) else {
            return;
        };
        let Phase::Established {
            channel,
            device_id,
            window_start_ms,
            window_count,
            ..
        } = &mut conn.phase
        else {
            return;
        };
        let device_id = *device_id;
        let Ok(plaintext) = channel.open(payload) else {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        conn.last_rx_ms = now_ms;
        if now_ms.saturating_sub(*window_start_ms) >= RATE_WINDOW_MS {
            *window_start_ms = now_ms;
            *window_count = 0;
        }
        *window_count = window_count.saturating_add(1);
        let count = *window_count;
        if count > RATE_MAX_MESSAGES {
            // One reply and one audit entry per window; the rest are dropped silently.
            if count == RATE_MAX_MESSAGES + 1 {
                self.reject(conn_id, device_id, None, ErrorCode::RateLimited, fx);
            }
            return;
        }
        if self.permissions(Some(device_id)).is_none() {
            self.close(conn_id, CloseReason::Revoked, fx);
            return;
        }
        match ClientMessage::from_bytes(&plaintext) {
            Ok(msg) => self.handle_message(conn_id, device_id, msg, fx),
            Err(MessageError::UnknownType(t)) => {
                self.reject(conn_id, device_id, Some(t), ErrorCode::UnknownMessage, fx);
            }
            Err(MessageError::Malformed(_) | MessageError::TooLarge) => {
                self.reject(conn_id, device_id, None, ErrorCode::Malformed, fx);
            }
        }
    }

    fn handle_message(
        &mut self,
        conn_id: ConnId,
        device_id: DeviceId,
        msg: ClientMessage,
        fx: &mut Vec<HubEffect>,
    ) {
        let Some(perms) = self.permissions(Some(device_id)) else {
            return;
        };
        let request = msg.name();
        match msg {
            ClientMessage::Hello => {
                let welcome = ServerMessage::Welcome {
                    phone_name: self.phone_name.clone(),
                    permissions: perms,
                    busy: self.is_busy(),
                };
                self.send(conn_id, &welcome, fx);
            }
            ClientMessage::Ping => self.send(conn_id, &ServerMessage::Pong, fx),
            ClientMessage::StartTask { text } => {
                if let Err(r) = self.may_start(perms) {
                    return self.deny(conn_id, device_id, request, r, fx);
                }
                let text = clean_text(&text, usize::MAX, false);
                let len = text.chars().count();
                if len == 0 || len > MAX_TASK_TEXT_CHARS {
                    return self.reject(
                        conn_id,
                        device_id,
                        Some(request.to_owned()),
                        ErrorCode::InvalidRequest,
                        fx,
                    );
                }
                self.task = Some(Task {
                    origin: Some(device_id),
                });
                let detail = redact_text(&text, MAX_SUMMARY_CHARS, false);
                fx.push(HubEffect::StartTask { device_id, text });
                self.accept(conn_id, device_id, request, Some(detail), fx);
            }
            ClientMessage::RunSkill { id, args } => {
                if let Err(r) = self.may_start(perms) {
                    return self.deny(conn_id, device_id, request, r, fx);
                }
                if !self.skills.iter().any(|s| s.id == id) {
                    return self.deny(conn_id, device_id, request, DenyReason::UnknownSkill, fx);
                }
                self.task = Some(Task {
                    origin: Some(device_id),
                });
                fx.push(HubEffect::RunSkill {
                    device_id,
                    id: id.clone(),
                    args,
                });
                self.accept(conn_id, device_id, request, Some(id), fx);
            }
            ClientMessage::RunWorkflow { id } => {
                if let Err(r) = self.may_start(perms) {
                    return self.deny(conn_id, device_id, request, r, fx);
                }
                if !self.workflows.iter().any(|s| s.id == id) {
                    return self.deny(conn_id, device_id, request, DenyReason::UnknownSkill, fx);
                }
                self.task = Some(Task {
                    origin: Some(device_id),
                });
                fx.push(HubEffect::RunWorkflow {
                    device_id,
                    id: id.clone(),
                });
                self.accept(conn_id, device_id, request, Some(id), fx);
            }
            ClientMessage::Cancel => {
                if self.task.is_none() {
                    return self.deny(conn_id, device_id, request, DenyReason::NotRunning, fx);
                }
                fx.push(HubEffect::CancelTask { device_id });
                self.accept(conn_id, device_id, request, None, fx);
            }
            ClientMessage::Decision {
                request_id,
                approved,
            } => {
                if !perms.approve {
                    return self.deny(conn_id, device_id, request, DenyReason::NotPermitted, fx);
                }
                let Some(p) = self
                    .pending
                    .as_mut()
                    .filter(|p| p.id == request_id && !p.decided)
                else {
                    return self.deny(conn_id, device_id, request, DenyReason::StaleRequest, fx);
                };
                if !remotely_decidable(p.category) {
                    return self.deny(
                        conn_id,
                        device_id,
                        request,
                        DenyReason::NotRemotelyApprovable,
                        fx,
                    );
                }
                p.decided = true;
                fx.push(HubEffect::Decide {
                    request_id,
                    approved,
                    device_id,
                });
                let detail = format!(
                    "request {request_id} {}",
                    if approved { "approved" } else { "declined" }
                );
                self.accept(conn_id, device_id, request, Some(detail), fx);
            }
            ClientMessage::Subscribe => {
                if !perms.see_progress {
                    return self.deny(conn_id, device_id, request, DenyReason::NotPermitted, fx);
                }
                self.set_subscribed(conn_id, true);
                if let Some(msg) = self.pending_message(perms) {
                    self.send(conn_id, &msg, fx);
                }
            }
            ClientMessage::Unsubscribe => self.set_subscribed(conn_id, false),
            ClientMessage::ListSkills => {
                let mut skills = self.skills.clone();
                let mut workflows = self.workflows.clone();
                // Drop entries from the end until the reply fits in one frame.
                loop {
                    let msg = ServerMessage::Skills {
                        skills: skills.clone(),
                        workflows: workflows.clone(),
                    };
                    if msg.to_bytes().is_ok() || (skills.is_empty() && workflows.is_empty()) {
                        self.send(conn_id, &msg, fx);
                        break;
                    }
                    if skills.len() >= workflows.len() {
                        skills.pop();
                    } else {
                        workflows.pop();
                    }
                }
            }
        }
    }

    fn may_start(&self, perms: RemotePermissions) -> Result<(), DenyReason> {
        if !perms.start_tasks {
            Err(DenyReason::NotPermitted)
        } else if self.task.is_some() {
            Err(DenyReason::Busy)
        } else {
            Ok(())
        }
    }

    fn set_subscribed(&mut self, conn_id: ConnId, value: bool) {
        if let Some(Conn {
            phase: Phase::Established { subscribed, .. },
            ..
        }) = self.conns.get_mut(&conn_id)
        {
            *subscribed = value;
        }
    }

    fn accept(
        &self,
        conn_id: ConnId,
        device_id: DeviceId,
        request: &str,
        detail: Option<String>,
        fx: &mut Vec<HubEffect>,
    ) {
        self.audit(
            Some(conn_id),
            Some(device_id),
            AuditEvent::Accepted {
                request: request.to_owned(),
                detail,
            },
            fx,
        );
    }

    fn deny(
        &mut self,
        conn_id: ConnId,
        device_id: DeviceId,
        request: &str,
        reason: DenyReason,
        fx: &mut Vec<HubEffect>,
    ) {
        self.send(conn_id, &ServerMessage::Denied { reason }, fx);
        self.audit(
            Some(conn_id),
            Some(device_id),
            AuditEvent::Denied {
                request: request.to_owned(),
                reason,
            },
            fx,
        );
    }

    fn reject(
        &mut self,
        conn_id: ConnId,
        device_id: DeviceId,
        request: Option<String>,
        code: ErrorCode,
        fx: &mut Vec<HubEffect>,
    ) {
        self.send(conn_id, &ServerMessage::Error { code }, fx);
        self.audit(
            Some(conn_id),
            Some(device_id),
            AuditEvent::Rejected { request, code },
            fx,
        );
    }

    /// Seals `msg` for one connection. A message too large for a frame becomes
    /// `Error { too_large }`.
    fn send(&mut self, conn_id: ConnId, msg: &ServerMessage, fx: &mut Vec<HubEffect>) {
        let bytes = match msg.to_bytes() {
            Ok(b) => b,
            Err(_) => match (ServerMessage::Error {
                code: ErrorCode::TooLarge,
            })
            .to_bytes()
            {
                Ok(b) => b,
                Err(_) => return,
            },
        };
        let Some(Conn {
            phase: Phase::Established { channel, .. },
            ..
        }) = self.conns.get_mut(&conn_id)
        else {
            return;
        };
        match channel.seal(&bytes) {
            Ok(body) => fx.push(HubEffect::Send {
                conn_id,
                bytes: encode_record(Kind::Data, &body),
            }),
            Err(_) => self.close(conn_id, CloseReason::Internal, fx),
        }
    }

    /// Sends to every authenticated connection the closure picks, with a message built
    /// for that connection's device.
    fn broadcast(
        &mut self,
        fx: &mut Vec<HubEffect>,
        build: impl Fn(&RemoteHub, ConnView) -> Option<ServerMessage>,
    ) {
        let views: Vec<(ConnId, ConnView)> = self
            .conns
            .iter()
            .filter_map(|(id, c)| match &c.phase {
                Phase::Established {
                    device_id,
                    subscribed,
                    ..
                } => Some((
                    *id,
                    ConnView {
                        device_id: Some(*device_id),
                        subscribed: *subscribed,
                    },
                )),
                Phase::Handshake { .. } => None,
            })
            .collect();
        for (id, view) in views {
            if let Some(msg) = build(self, view) {
                self.send(id, &msg, fx);
            }
        }
    }

    fn pending_message(&self, perms: RemotePermissions) -> Option<ServerMessage> {
        let p = self.pending.as_ref().filter(|p| !p.decided)?;
        let can_decide = perms.can_decide(p.category);
        let summary = if perms.see_screen_text || can_decide {
            redact_text(&p.summary, MAX_SUMMARY_CHARS, false)
        } else {
            format!("Waiting for confirmation ({})", category_name(p.category))
        };
        Some(ServerMessage::ConfirmPending {
            request_id: p.id,
            category: p.category,
            summary,
            can_decide,
        })
    }

    fn broadcast_pending(&mut self, fx: &mut Vec<HubEffect>) {
        let Some(category) = self.pending.as_ref().map(|p| p.category) else {
            return;
        };
        self.broadcast(fx, |hub, c| {
            let perms = hub.permissions(c.device_id)?;
            (c.subscribed || perms.can_decide(category)).then_some(())?;
            hub.pending_message(perms)
        });
    }

    fn revoke(&mut self, device_id: DeviceId, fx: &mut Vec<HubEffect>) {
        let before = self.devices.len();
        // Dropping the record zeroizes its key.
        self.devices.retain(|d| d.id != device_id);
        if self.devices.len() == before {
            return;
        }
        self.audit(None, Some(device_id), AuditEvent::Revoked, fx);
        let ids: Vec<ConnId> = self
            .conns
            .iter()
            .filter(|(_, c)| {
                matches!(c.phase, Phase::Established { device_id: d, .. } if d == device_id)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.close(id, CloseReason::Revoked, fx);
        }
        if self.task.is_some_and(|t| t.origin == Some(device_id)) {
            fx.push(HubEffect::CancelTask { device_id });
        }
    }
}

/// What a broadcast closure sees of one authenticated connection.
#[derive(Debug, Clone, Copy)]
struct ConnView {
    device_id: Option<DeviceId>,
    subscribed: bool,
}

fn parse_pair_request(payload: &[u8]) -> Option<([u8; NONCE_LEN], String, [u8; MAC_LEN])> {
    let (nc, rest) = payload.split_at_checked(NONCE_LEN)?;
    let (len, rest) = rest.split_first()?;
    let len = usize::from(*len);
    if len > MAX_DEVICE_NAME_BYTES {
        return None;
    }
    let (name, mac) = rest.split_at_checked(len)?;
    if mac.len() != MAC_LEN {
        return None;
    }
    let name = std::str::from_utf8(name).ok()?;
    if name.is_empty() || name != clean_name(name) {
        return None;
    }
    Some((nc.try_into().ok()?, name.to_owned(), mac.try_into().ok()?))
}

fn parse_connect_request(payload: &[u8]) -> Option<(DeviceId, [u8; NONCE_LEN], [u8; MAC_LEN])> {
    let (id, rest) = payload.split_at_checked(16)?;
    let (nc, mac) = rest.split_at_checked(NONCE_LEN)?;
    if mac.len() != MAC_LEN {
        return None;
    }
    Some((
        DeviceId(id.try_into().ok()?),
        nc.try_into().ok()?,
        mac.try_into().ok()?,
    ))
}

/// Removes control and invisible format characters and caps the length in characters.
/// With `keep_newlines`, `\n` survives; otherwise every whitespace run becomes one space.
pub(crate) fn clean_text(text: &str, max_chars: usize, keep_newlines: bool) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    let mut pending_space = false;
    for c in text.chars() {
        if count >= max_chars {
            break;
        }
        if c == '\n' && keep_newlines {
            out.push('\n');
            count += 1;
            pending_space = false;
            continue;
        }
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if c.is_control() || is_format(c) {
            continue;
        }
        if pending_space && !out.ends_with('\n') {
            out.push(' ');
            count += 1;
            if count >= max_chars {
                break;
            }
        }
        pending_space = false;
        out.push(c);
        count += 1;
    }
    out
}

/// [`jarvis_safety::redact`] followed by [`clean_text`].
fn redact_text(text: &str, max_chars: usize, keep_newlines: bool) -> String {
    clean_text(&jarvis_safety::redact(text), max_chars, keep_newlines)
}

fn clean_catalog(entries: Vec<CatalogEntry>) -> Vec<CatalogEntry> {
    entries
        .into_iter()
        .filter(|e| !e.id.is_empty() && e.id.chars().count() <= MAX_ID_CHARS)
        .take(MAX_CATALOG_ENTRIES)
        .map(|e| CatalogEntry {
            id: e.id,
            name: clean_text(&e.name, 64, false),
            summary: clean_text(&e.summary, 200, false),
        })
        .collect()
}

fn category_name(c: RiskCategory) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.replace('_', " ")))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn generic_finish(status: OutcomeStatus) -> &'static str {
    match status {
        OutcomeStatus::Success => "Task completed",
        OutcomeStatus::Failed => "Task failed",
        OutcomeStatus::Cancelled => "Task cancelled",
        OutcomeStatus::Blocked => "Task stopped by the safety policy",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_strips_controls_and_caps() {
        assert_eq!(clean_text("a\u{0}b\u{202E}c  d\n e", 100, false), "abc d e");
        assert_eq!(clean_text("a\n\nb", 100, true), "a\n\nb");
        assert_eq!(clean_text("abcdef", 3, false), "abc");
    }

    #[test]
    fn lockout_backoff_doubles_and_caps() {
        let mut hub = RemoteHub::new("Phone");
        let mut fx = Vec::new();
        for i in 0..LOCKOUT_THRESHOLD {
            hub.count_failure(1, 1_000 + u64::from(i), &mut fx);
        }
        let first = hub.locked_until_ms - 1_004;
        assert_eq!(first, LOCKOUT_BASE_MS);
        hub.count_failure(1, 2_000, &mut fx);
        assert_eq!(hub.locked_until_ms - 2_000, 2 * LOCKOUT_BASE_MS);
        for _ in 0..40 {
            hub.count_failure(1, 3_000, &mut fx);
        }
        assert_eq!(hub.locked_until_ms - 3_000, LOCKOUT_MAX_MS);
        // Forgotten after a quiet period.
        hub.count_failure(1, 3_000 + FAILURE_MEMORY_MS, &mut fx);
        assert_eq!(hub.failures, 1);
    }
}
