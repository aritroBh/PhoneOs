//! The phone's side: [`RemoteHub`], a sans-IO state machine.
//!
//! The shell feeds it what happens (a connection opened, bytes arrived, the agent made
//! progress, the owner revoked a device) and carries out the [`HubEffect`]s it returns,
//! **in order** (in particular, persist on [`HubEffect::StoreChanged`] before sending the
//! bytes that follow it). The hub never reads a clock or a random source: `now_ms`
//! (monotonic milliseconds) and `entropy` (at least 32 bytes from the OS CSPRNG, e.g.
//! `SecRandomCopyBytes`, fresh for every call) arrive as arguments.
//!
//! # Clock
//!
//! Every check uses the hub's own monotonic clock, never the raw `now_ms`. It starts at
//! the first `now_ms`, advances by every forward step of `now_ms`, and ignores a backward
//! step (re-anchoring on the new value). So it never runs backwards (and is never below
//! a time already seen), and a shell clock that steps back does not stop time either: a
//! pairing offer, a handshake, an idle connection, a lockout and a rate window all keep
//! counting real elapsed time. All timestamps the hub reports (`expires_at_ms`,
//! `at_ms`, lockout ends) are on this clock; they equal the shell's until its clock steps
//! back.
//!
//! # Entropy misuse
//!
//! The shell is still responsible for fresh CSPRNG entropy, but mistakes are contained:
//! obviously non-random entropy is refused ([`crate::crypto::check_entropy`]); the
//! pairing secret, the phone nonce and the phone's ephemeral key are derived from it with
//! HKDF under separate labels, so nothing on the wire reveals the secret; a persisted
//! counter and the clock are mixed into every phone nonce, so repeated entropy never
//! repeats a nonce or an ephemeral key; and a recently seen `(device id, remote nonce)`
//! is refused, so a recorded `ConnectRequest` cannot be replayed.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use jarvis_proto::{ConfirmRequest, Outcome, OutcomeStatus, Progress, RiskCategory};
use jarvis_viewer::delta::DeltaClass;
use jarvis_viewer::{DeltaEncoder, ViewerTimeline};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::codec::PROTOCOL_VERSION;
use crate::codec::{encode_record, DeviceId, Kind, Record, RecordError, RecordReader};
use crate::crypto::{
    auth_key, check_entropy, connect_phone_mac, derive_device_id, derive_device_key, pair_key,
    pair_phone_mac, pairing_secret_from_entropy, verify_connect_client_mac, verify_pair_client_mac,
    Channel, Ephemeral, PairingSecret, Side, Transcript, MAC_LEN, NONCE_LEN, PUBLIC_LEN,
};
use crate::error::{HubError, MessageError, PersistError};
use crate::message::{CatalogEntry, ClientMessage, DenyReason, ErrorCode, ServerMessage};
use crate::pairing::{format_code, qr_payload, PairingOffer, PAIRING_TTL_MS};
use crate::permissions::{remotely_approvable, RemotePermissions};
use crate::store::{
    clean_name, DeviceInfo, DeviceRecord, DeviceStore, MAX_DEVICES, MAX_DEVICE_NAME_BYTES,
};
use crate::text::{clean_text, redact_text};
use crate::viewer::{
    ViewerEndReason, ViewerMessage, ViewerRequest, MIN_SCREEN_INTERVAL_MS, VIEWER_QUEUE_BYTES,
    VIEWER_WINDOW_BYTES,
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
/// Messages accepted per connection per window; the rest are dropped (except `Cancel`).
pub const RATE_MAX_MESSAGES: u32 = 20;
/// Failed MAC checks (per device for connects, one counter for pairing) before a lockout.
pub const LOCKOUT_THRESHOLD: u32 = 5;
/// First lockout, in milliseconds; doubles with every further failure.
pub const LOCKOUT_BASE_MS: u64 = 5_000;
/// Longest lockout, in milliseconds.
pub const LOCKOUT_MAX_MS: u64 = 10 * 60_000;
/// A failure counter with no new failure for this long starts again from zero.
pub const FAILURE_MEMORY_MS: u64 = 15 * 60_000;
/// Pre-authentication audit entries are merged per window of this length: the first
/// event in a window is logged in full, the rest are counted in one summary entry.
pub const PREAUTH_AUDIT_WINDOW_MS: u64 = 60_000;
/// `(device id, remote nonce)` pairs remembered to refuse replayed connect requests.
pub const RECENT_CONNECTS: usize = 1_024;
/// The phone-nonce counter is leased in blocks of this size (one persist per block).
pub const NONCE_LEASE_BLOCK: u64 = 64;
/// Longest accepted task text, in characters.
pub const MAX_TASK_TEXT_CHARS: usize = 2_000;
/// Longest accepted skill or workflow id, in characters.
pub const MAX_ID_CHARS: usize = 128;
/// Most skill arguments.
pub const MAX_SKILL_ARGS: usize = 16;
/// Longest skill argument name, in characters.
pub const MAX_SKILL_ARG_KEY_CHARS: usize = 64;
/// Longest skill argument value, in characters (longer values are cut).
pub const MAX_SKILL_ARG_VALUE_CHARS: usize = 2_000;
/// Most characters across all skill argument values.
pub const MAX_SKILL_ARGS_TOTAL_CHARS: usize = 4_000;
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
    /// purpose: wrong code, expired code, unknown device, bad MAC, bit flips and garbage
    /// all look the same.
    AuthFailed,
    /// A connect request repeated a recently seen `(device id, remote nonce)`.
    Replayed,
    /// Handshakes for this device (or pairing) are locked out after repeated failures.
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
    /// The shell passed entropy that is obviously not random.
    BadEntropy,
    /// The peer closed the connection.
    PeerClosed,
    /// An internal error (key schedule or counter exhaustion).
    Internal,
}

/// One entry of the phone's remote-control audit log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// The hub's monotonic clock (see the module docs).
    pub at_ms: u64,
    /// Connection, when one is involved.
    pub conn_id: Option<ConnId>,
    /// Authenticated (or claimed) device, when known.
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
    /// A device was paired (it proved it holds the key on its first frame).
    Paired {
        /// Its name.
        name: String,
    },
    /// Pairing completed cryptographically but the connection closed before the remote
    /// confirmed it; the device was not added.
    PairingAbandoned,
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
    /// A pairing or connect MAC check failed.
    HandshakeFailed {
        /// Failures counted so far (for this device, or for pairing).
        failures: u32,
    },
    /// Further pre-authentication events in the last window, merged into one entry.
    PreAuthSuppressed {
        /// How many were not logged individually.
        count: u32,
        /// Start of the window.
        since_ms: u64,
    },
    /// Handshakes (for `device_id`, or pairing when it is absent) are locked out.
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
        /// The message type (cleaned), when it could be read.
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
    /// Persist [`RemoteHub::to_json`] to the Keychain now, before carrying out the
    /// effects that follow (they may depend on it: a new device, a nonce lease).
    StoreChanged,
    /// Start a free-text task on behalf of a device. Report back with
    /// [`LocalEvent::SessionStarted`] / [`LocalEvent::SessionFinished`] carrying
    /// `task_id` (`SessionFinished` alone if it cannot start).
    StartTask {
        /// Hub-issued id for this request.
        task_id: u64,
        /// Requesting device.
        device_id: DeviceId,
        /// Cleaned task text.
        text: String,
    },
    /// Run a listed skill on behalf of a device. Report back as for `StartTask`.
    RunSkill {
        /// Hub-issued id for this request.
        task_id: u64,
        /// Requesting device.
        device_id: DeviceId,
        /// Skill id (present in the catalog).
        id: String,
        /// Cleaned, capped arguments.
        args: BTreeMap<String, String>,
    },
    /// Run a listed workflow on behalf of a device. Report back as for `StartTask`.
    RunWorkflow {
        /// Hub-issued id for this request.
        task_id: u64,
        /// Requesting device.
        device_id: DeviceId,
        /// Workflow id (present in the catalog).
        id: String,
    },
    /// Cancel one task: a remote asked, or the device that requested it was revoked.
    /// Cancel only if `task_id` names what is running (`None`: the task the owner
    /// started on the phone, which only an explicit remote `Cancel` can name).
    CancelTask {
        /// Requesting (or revoked) device.
        device_id: DeviceId,
        /// The task to cancel.
        task_id: Option<u64>,
    },
    /// Answer the pending confirmation. Feed the session `UserDecision { approved }` only
    /// if `request_id` is still the confirmation on screen and `category` matches it.
    Decide {
        /// Hub request id (see [`RemoteHub::pending_request_id`]).
        request_id: u64,
        /// The answer.
        approved: bool,
        /// The category the hub checked, to re-check against the prompt on screen.
        category: RiskCategory,
        /// Deciding device, for the task log.
        device_id: DeviceId,
    },
    /// A device was paired (followed by [`HubEffect::StoreChanged`]).
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
    /// A session started. `task_id` is the id from the `StartTask`/`RunSkill`/
    /// `RunWorkflow` effect it carries out, or `None` for a task the owner started on the
    /// phone (no remote origin; revoking a device never cancels it).
    SessionStarted {
        /// Hub task id, or `None` for a local task.
        task_id: Option<u64>,
    },
    /// The session made progress.
    Progress {
        /// The session's progress snapshot.
        progress: Progress,
        /// Screen text for remotes with `see_screen_text`. Build it from
        /// `jarvis_safety::redact_screen` output (secure fields already empty); the hub
        /// redacts it again before sending.
        screen_text: Option<String>,
    },
    /// The session asks for a confirmation. The hub assigns it an id; read it with
    /// [`RemoteHub::pending_request_id`] and pass it back in `ConfirmResolved`.
    ConfirmRequested {
        /// The request.
        request: ConfirmRequest,
    },
    /// A confirmation was answered (on the phone or after a `Decide`). Ignored unless
    /// `request_id` is the one pending, so a late answer cannot clear a newer prompt.
    ConfirmResolved {
        /// The hub request id it answers.
        request_id: u64,
        /// The answer.
        approved: bool,
    },
    /// A session ended, or a requested start failed. Ignored unless `task_id` matches
    /// the running task or the requested one.
    SessionFinished {
        /// Hub task id, or `None` for a local task.
        task_id: Option<u64>,
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

#[derive(Debug, Default, Clone, Copy)]
struct Lockout {
    failures: u32,
    last_failure_ms: u64,
    locked_until_ms: u64,
}

impl Lockout {
    fn locked(&self, now: u64) -> bool {
        now < self.locked_until_ms
    }

    /// Counts one failed MAC check; returns the new lockout end if one was armed.
    fn fail(&mut self, now: u64) -> Option<u64> {
        if self.failures > 0 && now.saturating_sub(self.last_failure_ms) >= FAILURE_MEMORY_MS {
            self.failures = 0;
        }
        self.failures = self.failures.saturating_add(1);
        self.last_failure_ms = now;
        if self.failures < LOCKOUT_THRESHOLD {
            return None;
        }
        let shift = (self.failures - LOCKOUT_THRESHOLD).min(16);
        let ms = LOCKOUT_BASE_MS
            .saturating_mul(1u64 << shift)
            .min(LOCKOUT_MAX_MS);
        self.locked_until_ms = now.saturating_add(ms);
        Some(self.locked_until_ms)
    }
}

#[derive(Debug)]
enum Phase {
    Handshake {
        /// Taken (and dropped, zeroizing it) by the first handshake record.
        eph: Option<Box<Ephemeral>>,
    },
    Established {
        channel: Box<Channel>,
        device_id: DeviceId,
        /// A device paired on this connection, not yet confirmed by a data frame.
        provisional: Option<Box<DeviceRecord>>,
        subscribed: bool,
        window_start_ms: u64,
        window_count: u32,
        /// The Live view stream, while this connection watches.
        watch: Option<Box<Watch>>,
        /// Bytes of `Send` effects the shell has not yet reported written.
        in_flight: usize,
    },
}

/// One connection's Live view stream (see [`crate::viewer`]).
#[derive(Debug, Default)]
struct Watch {
    /// The next thing to send is a snapshot (start, resync, overflow).
    needs_snapshot: bool,
    /// `viewer_delta` plaintexts, sealed when handed out (in order).
    queue: VecDeque<Vec<u8>>,
    queued_bytes: usize,
    /// A screen held back by the rate limit (a newer one replaces it).
    pending_screen: Option<Vec<u8>>,
    last_screen_ms: Option<u64>,
}

impl Watch {
    /// Queues a message; on overflow drops the queue and asks for a snapshot instead.
    fn enqueue(&mut self, msg: Vec<u8>) {
        if self.needs_snapshot {
            return;
        }
        if self.queued_bytes.saturating_add(msg.len()) > VIEWER_QUEUE_BYTES {
            self.resync();
            return;
        }
        self.queued_bytes += msg.len();
        self.queue.push_back(msg);
    }

    /// Offers one delta of the live stream.
    fn offer(&mut self, class: DeltaClass, msg: &[u8], now: u64) {
        if self.needs_snapshot {
            return;
        }
        match class {
            // A held-back screen of the old generation must not follow the new reset.
            DeltaClass::Reset => {
                self.pending_screen = None;
                self.enqueue(msg.to_vec());
            }
            DeltaClass::Side => {
                if self.pending_screen.is_none() && self.screen_due(now) {
                    self.last_screen_ms = Some(now);
                    self.enqueue(msg.to_vec());
                } else {
                    self.pending_screen = Some(msg.to_vec());
                }
            }
            _ => self.enqueue(msg.to_vec()),
        }
    }

    fn screen_due(&self, now: u64) -> bool {
        self.last_screen_ms
            .is_none_or(|l| now.saturating_sub(l) >= MIN_SCREEN_INTERVAL_MS)
    }

    /// Drops what is queued: the next thing sent is a snapshot (coalesced).
    fn resync(&mut self) {
        self.queue.clear();
        self.queued_bytes = 0;
        self.pending_screen = None;
        self.needs_snapshot = true;
    }
}

/// What the pump does next for one watching connection.
enum PumpStep {
    Snapshot,
    Send(Vec<u8>),
    Done,
}

#[derive(Debug)]
struct Conn {
    opened_at_ms: u64,
    last_rx_ms: u64,
    reader: RecordReader,
    phase: Phase,
}

#[derive(Debug, Clone, Copy)]
struct Requested {
    task_id: u64,
    origin: DeviceId,
}

#[derive(Debug, Clone, Copy)]
struct Running {
    task_id: Option<u64>,
    origin: Option<DeviceId>,
}

#[derive(Debug, Clone)]
struct Pending {
    id: u64,
    category: RiskCategory,
    /// Thirteenth review: the request is `phone_only` (a presumed submit, a link the owner
    /// did not give): no remote may approve it, whatever its category.
    phone_only: bool,
    summary: String,
    decided: bool,
    /// Devices that were sent this `ConfirmPending` (they may decline it).
    notified: BTreeSet<DeviceId>,
}

#[derive(Debug, Default)]
struct PreAuthAudit {
    started: bool,
    window_start_ms: u64,
    suppressed: u32,
}

/// The phone's remote-control hub. See the crate docs for the protocol and the rules.
#[derive(Debug)]
pub struct RemoteHub {
    phone_name: String,
    devices: Vec<DeviceRecord>,
    offer: Option<Offer>,
    conns: BTreeMap<ConnId, Conn>,
    pair_lockout: Lockout,
    device_lockouts: BTreeMap<DeviceId, Lockout>,
    requested: Option<Requested>,
    running: Option<Running>,
    pending: Option<Pending>,
    next_request_id: u64,
    next_task_id: u64,
    skills: Vec<CatalogEntry>,
    workflows: Vec<CatalogEntry>,
    /// The hub's monotonic clock (see the module docs).
    last_now_ms: u64,
    /// The raw `now_ms` the clock was last anchored to (`None` before the first call).
    clock_anchor: Option<u64>,
    nonce_counter: u64,
    nonce_lease: u64,
    recent_connects: VecDeque<(DeviceId, [u8; NONCE_LEN])>,
    preauth: PreAuthAudit,
    /// Devices holding the `watch_screen` permission.
    watch_allowed: BTreeSet<DeviceId>,
    /// The Live view stream's encoder (fed by [`RemoteHub::viewer_update`]).
    viewer: DeltaEncoder,
}

impl RemoteHub {
    /// A hub with no paired devices.
    pub fn new(phone_name: &str) -> Self {
        Self::build(phone_name, DeviceStore::default())
    }

    /// A hub with previously paired devices. Validated like [`DeviceStore::from_json`].
    pub fn with_store(phone_name: &str, store: DeviceStore) -> Result<Self, PersistError> {
        store.validate()?;
        Ok(Self::build(phone_name, store))
    }

    fn build(phone_name: &str, store: DeviceStore) -> Self {
        Self {
            phone_name: clean_name(phone_name),
            devices: store.devices,
            offer: None,
            conns: BTreeMap::new(),
            pair_lockout: Lockout::default(),
            device_lockouts: BTreeMap::new(),
            requested: None,
            running: None,
            pending: None,
            next_request_id: 1,
            next_task_id: 1,
            skills: Vec::new(),
            workflows: Vec::new(),
            last_now_ms: 0,
            clock_anchor: None,
            // Everything below the lease may have been used before the restart.
            nonce_counter: store.nonce_lease,
            nonce_lease: store.nonce_lease,
            recent_connects: VecDeque::new(),
            preauth: PreAuthAudit::default(),
            watch_allowed: BTreeSet::new(),
            viewer: DeltaEncoder::new(),
        }
    }

    /// Restores a hub from [`RemoteHub::to_json`] output.
    pub fn from_json(phone_name: &str, json: &str) -> Result<Self, PersistError> {
        let (store, watch) = DeviceStore::from_json_with_watch(json)?;
        let mut hub = Self::with_store(phone_name, store)?;
        hub.watch_allowed = watch;
        Ok(hub)
    }

    /// The persisted state, keys included, for the Keychain (see [`DeviceStore`]), with
    /// the `watch_screen` permissions. Devices whose pairing is not yet confirmed are not
    /// included.
    pub fn to_json(&self) -> Zeroizing<String> {
        self.store().to_json_with_watch(&self.watch_allowed)
    }

    /// Whether a device holds the `watch_screen` permission: it may receive the Live view
    /// (wireframes, cursor and captions; never pixels). Off by default and for devices
    /// stored before it existed; independent of `see_progress`. Kept beside
    /// [`RemotePermissions`]; change it with [`RemoteHub::set_watch_screen`].
    pub fn watch_screen(&self, device_id: DeviceId) -> bool {
        self.watch_allowed.contains(&device_id)
    }

    /// The owner turned the `watch_screen` permission on or off for a device (persist on
    /// the returned `StoreChanged`). Off stops that device's streams in this call. An
    /// unknown device or no change returns nothing.
    pub fn set_watch_screen(
        &mut self,
        device_id: DeviceId,
        allowed: bool,
        now_ms: u64,
    ) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        self.expire(now_ms, &mut fx);
        self.watch_screen_changed(device_id, allowed, &mut fx);
        fx
    }

    /// Whether `conn_id` currently receives the Live view.
    pub fn is_watching(&self, conn_id: ConnId) -> bool {
        self.watch_ref(conn_id).is_some()
    }

    /// The Live view changed: one call per driver update on the phone, after feeding it to
    /// the view (`jarvis_viewer::LiveView`), with the view's `runs()` and `timeline()`
    /// (the reducer's post-privacy state, never raw updates). The hub diffs it into deltas
    /// and queues them for every connection that watches with the permission. Never
    /// blocks and never holds more than [`VIEWER_QUEUE_BYTES`] per connection (see
    /// [`crate::viewer`]).
    pub fn viewer_update(
        &mut self,
        now_ms: u64,
        run: u64,
        timeline: &ViewerTimeline,
    ) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        let now = self.expire(now_ms, &mut fx);
        let deltas = self.viewer.feed(run, timeline);
        let live: Vec<ConnId> = self
            .watcher_ids()
            .into_iter()
            .filter(|id| self.watch_ref(*id).is_some_and(|w| !w.needs_snapshot))
            .collect();
        if !deltas.is_empty() && !live.is_empty() {
            let mut encoded = Vec::with_capacity(deltas.len());
            let mut broken = false;
            for d in &deltas {
                match ViewerMessage::delta(&d.encode()).and_then(|m| m.to_bytes()) {
                    Ok(msg) => encoded.push((d.header().class, msg)),
                    // Not expected for the encoder's deltas; leaving one out would open a
                    // gap, so resync instead.
                    Err(_) => broken = true,
                }
            }
            for id in live {
                if let Some(w) = self.watch_mut(id) {
                    if broken {
                        w.resync();
                    }
                    for (class, msg) in &encoded {
                        w.offer(*class, msg, now);
                    }
                }
            }
        }
        self.pump_all(now, &mut fx);
        fx
    }

    /// The transport wrote `bytes` of this connection's `Send` effects (report it from the
    /// write completion). The Live view is handed out only while fewer than
    /// [`VIEWER_WINDOW_BYTES`] are unconfirmed; over-reporting only loosens that window.
    pub fn written(&mut self, conn_id: ConnId, bytes: usize, now_ms: u64) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        let now = self.expire(now_ms, &mut fx);
        if let Some(Conn {
            phase: Phase::Established { in_flight, .. },
            ..
        }) = self.conns.get_mut(&conn_id)
        {
            *in_flight = in_flight.saturating_sub(bytes);
        }
        self.pump(conn_id, now, &mut fx);
        fx
    }

    /// A copy of the persisted state, keys included.
    pub fn store(&self) -> DeviceStore {
        DeviceStore {
            devices: self.devices.clone(),
            nonce_lease: self.nonce_lease,
        }
    }

    /// The paired (confirmed) devices without keys.
    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.devices.iter().map(DeviceRecord::info).collect()
    }

    /// The id of the confirmation currently pending, if any.
    pub fn pending_request_id(&self) -> Option<u64> {
        self.pending.as_ref().map(|p| p.id)
    }

    /// Whether a task is running, or was requested and not yet reported finished.
    pub fn is_busy(&self) -> bool {
        self.running.is_some() || self.requested.is_some()
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

    /// The hub's monotonic clock as of the last call (see the module docs): the clock
    /// that `expires_at_ms`, `at_ms` and the lockout ends are on. 0 before the first call.
    pub fn clock_ms(&self) -> u64 {
        self.last_now_ms
    }

    /// Renames a paired device (the owner edits the name on the phone). The name is
    /// cleaned like a pairing name ([`clean_name`]); a name that is empty once cleaned is
    /// [`HubError::InvalidName`]. Returns [`HubEffect::StoreChanged`] when the name
    /// changed, nothing when it is already that name.
    pub fn rename_device(
        &mut self,
        device_id: DeviceId,
        name: &str,
    ) -> Result<Vec<HubEffect>, HubError> {
        let name = clean_name(name);
        if name.is_empty() {
            return Err(HubError::InvalidName);
        }
        let dev = self
            .devices
            .iter_mut()
            .find(|d| d.id == device_id)
            .ok_or(HubError::UnknownDevice)?;
        if dev.name == name {
            return Ok(Vec::new());
        }
        dev.name = name;
        Ok(vec![HubEffect::StoreChanged])
    }

    /// End of the pairing lockout (0 when none has happened).
    pub fn pairing_locked_until_ms(&self) -> u64 {
        self.pair_lockout.locked_until_ms
    }

    /// End of the connect lockout for one device (0 when none has happened).
    pub fn device_locked_until_ms(&self, device_id: DeviceId) -> u64 {
        self.device_lockouts
            .get(&device_id)
            .map_or(0, |l| l.locked_until_ms)
    }

    /// Creates a single-use pairing offer, valid for [`PAIRING_TTL_MS`]. The secret is
    /// derived from `entropy` (≥ 32 CSPRNG bytes). Replaces any open offer.
    pub fn open_pairing(&mut self, now_ms: u64, entropy: &[u8]) -> Result<PairingOffer, HubError> {
        let now = self.clock(now_ms);
        check_entropy(entropy)?;
        if self.devices.len() + self.provisional_count() >= MAX_DEVICES {
            return Err(HubError::TooManyDevices);
        }
        let secret = pairing_secret_from_entropy(entropy).map_err(|_| HubError::Crypto)?;
        let expires_at_ms = now.saturating_add(PAIRING_TTL_MS);
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

    /// A transport connection opened. `entropy` (≥ 32 CSPRNG bytes) seeds the phone's
    /// nonce and ephemeral key for this connection.
    pub fn connection_opened(
        &mut self,
        conn_id: ConnId,
        now_ms: u64,
        entropy: &[u8],
    ) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        let now = self.expire(now_ms, &mut fx);
        if self.conns.contains_key(&conn_id) {
            self.close(conn_id, CloseReason::DuplicateConnection, &mut fx);
            return fx;
        }
        if self.conns.len() >= MAX_CONNECTIONS {
            fx.push(HubEffect::Close {
                conn_id,
                reason: CloseReason::TooManyConnections,
            });
            self.audit_preauth(
                Some(conn_id),
                None,
                AuditEvent::ConnectionRefused {
                    reason: CloseReason::TooManyConnections,
                },
                &mut fx,
            );
            return fx;
        }
        if check_entropy(entropy).is_err() {
            fx.push(HubEffect::Close {
                conn_id,
                reason: CloseReason::BadEntropy,
            });
            self.audit(
                Some(conn_id),
                None,
                AuditEvent::ConnectionRefused {
                    reason: CloseReason::BadEntropy,
                },
                &mut fx,
            );
            return fx;
        }
        // Lease the counter before using it, so a restart never reuses one.
        if self.nonce_counter >= self.nonce_lease {
            self.nonce_lease = self.nonce_counter.saturating_add(NONCE_LEASE_BLOCK);
            fx.push(HubEffect::StoreChanged);
        }
        let counter = self.nonce_counter;
        self.nonce_counter = self.nonce_counter.saturating_add(1);
        let Ok(eph) = Ephemeral::phone(entropy, counter, now) else {
            fx.push(HubEffect::Close {
                conn_id,
                reason: CloseReason::Internal,
            });
            return fx;
        };
        let mut hello = Vec::with_capacity(NONCE_LEN + PUBLIC_LEN);
        hello.extend_from_slice(&eph.nonce);
        hello.extend_from_slice(&eph.public);
        self.conns.insert(
            conn_id,
            Conn {
                opened_at_ms: now,
                last_rx_ms: now,
                reader: RecordReader::new(),
                phase: Phase::Handshake {
                    eph: Some(Box::new(eph)),
                },
            },
        );
        fx.push(HubEffect::Send {
            conn_id,
            bytes: encode_record(Kind::ServerHello, &hello),
        });
        fx
    }

    /// The transport connection closed. Idempotent. A device paired on this connection
    /// but not yet confirmed is dropped.
    pub fn connection_closed(&mut self, conn_id: ConnId) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        if let Some(conn) = self.conns.remove(&conn_id) {
            if let Phase::Established {
                device_id,
                provisional,
                ..
            } = conn.phase
            {
                if provisional.is_some() {
                    self.audit(
                        Some(conn_id),
                        Some(device_id),
                        AuditEvent::PairingAbandoned,
                        &mut fx,
                    );
                }
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

    /// Applies timeouts: handshake and idle limits, pairing-offer expiry, and flushes the
    /// pre-authentication audit summary. Call it every few seconds; every other entry
    /// point applies it too.
    pub fn tick(&mut self, now_ms: u64) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        let now = self.expire(now_ms, &mut fx);
        // Screens held back by the Live view's rate limit.
        self.pump_all(now, &mut fx);
        fx
    }

    /// Bytes arrived on a connection. Unknown connection ids are ignored.
    pub fn receive(&mut self, conn_id: ConnId, bytes: &[u8], now_ms: u64) -> Vec<HubEffect> {
        let mut fx = Vec::new();
        let now = self.expire(now_ms, &mut fx);
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
                    Ok(Some(record)) => self.handle_record(conn_id, record, now, &mut fx),
                    Ok(None) => break,
                    Err(e) => {
                        let reason = match e {
                            RecordError::TooLarge | RecordError::BufferFull => {
                                CloseReason::TooLarge
                            }
                            RecordError::Malformed => CloseReason::AuthFailed,
                        };
                        self.close(conn_id, reason, &mut fx);
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
            LocalEvent::SessionStarted { task_id } => self.session_started(task_id, &mut fx),
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
                                .map(|t| redact_text(t, MAX_SCREEN_TEXT_CHARS))
                        } else {
                            None
                        },
                        waiting_for_unlock: progress.waiting_for_unlock,
                    })
                });
            }
            LocalEvent::ConfirmRequested { request } => {
                let id = self.next_request_id;
                self.next_request_id = self.next_request_id.saturating_add(1);
                self.pending = Some(Pending {
                    id,
                    category: request.category,
                    phone_only: request.phone_only,
                    summary: request.summary,
                    decided: false,
                    notified: BTreeSet::new(),
                });
                self.broadcast_pending(&mut fx);
            }
            LocalEvent::ConfirmResolved { request_id, .. } => {
                if self.pending.as_ref().is_some_and(|p| p.id == request_id) {
                    if let Some(p) = self.pending.take() {
                        self.broadcast(&mut fx, |_, c| {
                            (c.subscribed || c.device_id.is_some_and(|d| p.notified.contains(&d)))
                                .then_some(ServerMessage::ConfirmResolved { request_id: p.id })
                        });
                    }
                }
            }
            LocalEvent::SessionFinished { task_id, outcome } => {
                self.session_finished(task_id, &outcome, &mut fx);
            }
            LocalEvent::DeviceRevoked { device_id } => self.revoke(device_id, &mut fx),
            LocalEvent::PermissionsChanged {
                device_id,
                permissions,
            } => self.permissions_changed(device_id, permissions, &mut fx),
            LocalEvent::CatalogChanged { skills, workflows } => {
                self.skills = clean_catalog(skills);
                self.workflows = clean_catalog(workflows);
            }
        }
        fx
    }

    // ---- lifecycle --------------------------------------------------------------------

    fn session_started(&mut self, task_id: Option<u64>, fx: &mut Vec<HubEffect>) {
        let origin = match (task_id, self.requested) {
            (Some(id), Some(r)) if r.task_id == id => {
                self.requested = None;
                Some(r.origin)
            }
            _ => None,
        };
        self.running = Some(Running { task_id, origin });
        self.broadcast(fx, |hub, c| {
            hub.permissions(c.device_id)?;
            (c.subscribed || (origin.is_some() && c.device_id == origin))
                .then_some(ServerMessage::TaskStarted)
        });
    }

    fn session_finished(
        &mut self,
        task_id: Option<u64>,
        outcome: &Outcome,
        fx: &mut Vec<HubEffect>,
    ) {
        let origin = if self.running.is_some_and(|r| r.task_id == task_id) {
            let origin = self.running.take().and_then(|r| r.origin);
            self.pending = None;
            origin
        } else if let (Some(id), Some(r)) = (task_id, self.requested) {
            if r.task_id != id {
                return;
            }
            self.requested = None;
            Some(r.origin)
        } else {
            return;
        };
        self.broadcast(fx, |hub, c| {
            let p = hub.permissions(c.device_id)?;
            (c.subscribed || (origin.is_some() && c.device_id == origin)).then_some(())?;
            let summary = if p.see_screen_text {
                redact_text(&outcome.summary, MAX_SUMMARY_CHARS)
            } else {
                generic_finish(outcome.status).to_owned()
            };
            Some(ServerMessage::Finished {
                status: outcome.status,
                summary,
            })
        });
    }

    fn permissions_changed(
        &mut self,
        device_id: DeviceId,
        permissions: RemotePermissions,
        fx: &mut Vec<HubEffect>,
    ) {
        let Some(dev) = self.devices.iter_mut().find(|d| d.id == device_id) else {
            return;
        };
        dev.permissions = permissions;
        fx.push(HubEffect::StoreChanged);
        self.audit(
            None,
            Some(device_id),
            AuditEvent::PermissionsChanged { permissions },
            fx,
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
        self.broadcast(fx, |_, c| {
            (c.device_id == Some(device_id))
                .then_some(ServerMessage::PermissionsChanged { permissions })
        });
    }

    fn revoke(&mut self, device_id: DeviceId, fx: &mut Vec<HubEffect>) {
        let before = self.devices.len();
        // Dropping the record zeroizes its key.
        self.devices.retain(|d| d.id != device_id);
        if self.devices.len() == before {
            return;
        }
        self.device_lockouts.remove(&device_id);
        self.recent_connects.retain(|(d, _)| *d != device_id);
        self.watch_allowed.remove(&device_id);
        fx.push(HubEffect::StoreChanged);
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
        // Stop what this device asked for, and nothing else.
        if let Some(r) = self.running.filter(|r| r.origin == Some(device_id)) {
            fx.push(HubEffect::CancelTask {
                device_id,
                task_id: r.task_id,
            });
        }
        if let Some(r) = self.requested.filter(|r| r.origin == device_id) {
            fx.push(HubEffect::CancelTask {
                device_id,
                task_id: Some(r.task_id),
            });
        }
    }

    // ---- internals -------------------------------------------------------------------

    /// Advances the hub's monotonic clock with `now_ms` and returns it.
    fn clock(&mut self, now_ms: u64) -> u64 {
        match self.clock_anchor {
            None => self.last_now_ms = now_ms,
            Some(anchor) => {
                // Forward steps count; a backward step only moves the anchor.
                self.last_now_ms = self
                    .last_now_ms
                    .saturating_add(now_ms.saturating_sub(anchor));
            }
        }
        self.clock_anchor = Some(now_ms);
        self.last_now_ms
    }

    fn provisional_count(&self) -> usize {
        self.conns
            .values()
            .filter(|c| {
                matches!(
                    c.phase,
                    Phase::Established {
                        provisional: Some(_),
                        ..
                    }
                )
            })
            .count()
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

    /// Network-driven events before authentication: the first per window is logged, the
    /// rest are counted into one [`AuditEvent::PreAuthSuppressed`] entry.
    fn audit_preauth(
        &mut self,
        conn_id: Option<ConnId>,
        device_id: Option<DeviceId>,
        event: AuditEvent,
        fx: &mut Vec<HubEffect>,
    ) {
        self.flush_preauth(fx);
        if !self.preauth.started {
            self.preauth.started = true;
            self.preauth.window_start_ms = self.last_now_ms;
            self.preauth.suppressed = 0;
            self.audit(conn_id, device_id, event, fx);
        } else {
            self.preauth.suppressed = self.preauth.suppressed.saturating_add(1);
        }
    }

    /// Closes the pre-auth window when it has run out, emitting its summary.
    fn flush_preauth(&mut self, fx: &mut Vec<HubEffect>) {
        if self.preauth.started
            && self
                .last_now_ms
                .saturating_sub(self.preauth.window_start_ms)
                >= PREAUTH_AUDIT_WINDOW_MS
        {
            if self.preauth.suppressed > 0 {
                self.audit(
                    None,
                    None,
                    AuditEvent::PreAuthSuppressed {
                        count: self.preauth.suppressed,
                        since_ms: self.preauth.window_start_ms,
                    },
                    fx,
                );
            }
            self.preauth = PreAuthAudit::default();
        }
    }

    /// Applies the clock; returns the effective (highest seen) time.
    fn expire(&mut self, now_ms: u64, fx: &mut Vec<HubEffect>) -> u64 {
        let now = self.clock(now_ms);
        self.flush_preauth(fx);
        if self.offer.as_ref().is_some_and(|o| now >= o.expires_at_ms) {
            self.offer = None;
            self.audit(None, None, AuditEvent::PairingExpired, fx);
        }
        let expired: Vec<(ConnId, CloseReason)> = self
            .conns
            .iter()
            .filter_map(|(id, c)| match c.phase {
                Phase::Handshake { .. }
                    if now.saturating_sub(c.opened_at_ms) >= HANDSHAKE_TIMEOUT_MS =>
                {
                    Some((*id, CloseReason::HandshakeTimeout))
                }
                Phase::Established { .. }
                    if now.saturating_sub(c.last_rx_ms) >= IDLE_TIMEOUT_MS =>
                {
                    Some((*id, CloseReason::IdleTimeout))
                }
                _ => None,
            })
            .collect();
        for (id, reason) in expired {
            self.close(id, reason, fx);
        }
        now
    }

    /// Forgets the connection and emits `Close` plus an audit entry (merged per window
    /// for connections that never authenticated).
    fn close(&mut self, conn_id: ConnId, reason: CloseReason, fx: &mut Vec<HubEffect>) {
        let Some(conn) = self.conns.remove(&conn_id) else {
            return;
        };
        fx.push(HubEffect::Close { conn_id, reason });
        match conn.phase {
            Phase::Established {
                device_id,
                provisional,
                ..
            } => {
                if provisional.is_some() {
                    self.audit(
                        Some(conn_id),
                        Some(device_id),
                        AuditEvent::PairingAbandoned,
                        fx,
                    );
                }
                self.audit(
                    Some(conn_id),
                    Some(device_id),
                    AuditEvent::Closed { reason },
                    fx,
                );
            }
            Phase::Handshake { .. } => {
                self.audit_preauth(Some(conn_id), None, AuditEvent::Closed { reason }, fx);
            }
        }
    }

    fn handle_record(
        &mut self,
        conn_id: ConnId,
        record: Record,
        now: u64,
        fx: &mut Vec<HubEffect>,
    ) {
        let Some(conn) = self.conns.get(&conn_id) else {
            return;
        };
        let kind = Kind::from_byte(record.kind);
        if record.version != PROTOCOL_VERSION || kind.is_none() {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        }
        match (&conn.phase, kind) {
            (Phase::Handshake { .. }, Some(Kind::PairRequest)) => {
                self.handle_pair(conn_id, &record.payload, now, fx);
            }
            (Phase::Handshake { .. }, Some(Kind::ConnectRequest)) => {
                self.handle_connect(conn_id, &record.payload, now, fx);
            }
            (Phase::Established { .. }, Some(Kind::Data)) => {
                self.handle_data(conn_id, &record.payload, now, fx);
            }
            _ => self.close(conn_id, CloseReason::AuthFailed, fx),
        }
    }

    /// Takes the phone's ephemeral out of a handshake-phase connection (it is used once).
    fn take_ephemeral(&mut self, conn_id: ConnId) -> Option<Box<Ephemeral>> {
        match &mut self.conns.get_mut(&conn_id)?.phase {
            Phase::Handshake { eph } => eph.take(),
            Phase::Established { .. } => None,
        }
    }

    fn handshake_mac_failed(
        &mut self,
        conn_id: ConnId,
        device: Option<DeviceId>,
        now: u64,
        fx: &mut Vec<HubEffect>,
    ) {
        let lockout = match device {
            Some(d) => self.device_lockouts.entry(d).or_default(),
            None => &mut self.pair_lockout,
        };
        let armed = lockout.fail(now);
        let failures = lockout.failures;
        self.audit_preauth(
            Some(conn_id),
            device,
            AuditEvent::HandshakeFailed { failures },
            fx,
        );
        if let Some(until_ms) = armed {
            // Bounded: while locked, attempts are refused before any check is counted.
            self.audit(
                Some(conn_id),
                device,
                AuditEvent::LockedOut { until_ms },
                fx,
            );
        }
        self.close(conn_id, CloseReason::AuthFailed, fx);
    }

    fn handle_pair(&mut self, conn_id: ConnId, payload: &[u8], now: u64, fx: &mut Vec<HubEffect>) {
        // Lockout is decided before anything is checked or counted.
        if self.pair_lockout.locked(now) {
            self.close(conn_id, CloseReason::LockedOut, fx);
            return;
        }
        let Some((nc, er, name, mac)) = parse_pair_request(payload) else {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        let secret = match &self.offer {
            Some(o) if now < o.expires_at_ms => o.secret.clone(),
            _ => {
                // No MAC was checked: nothing to count.
                self.close(conn_id, CloseReason::AuthFailed, fx);
                return;
            }
        };
        let Some(eph) = self.take_ephemeral(conn_id) else {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        let t = Transcript {
            np: eph.nonce,
            nc,
            ep: eph.public,
            er,
        };
        let Ok(pk) = pair_key(&secret) else {
            self.close(conn_id, CloseReason::Internal, fx);
            return;
        };
        if verify_pair_client_mac(&pk, &t, &name, &mac).is_err() {
            self.handshake_mac_failed(conn_id, None, now, fx);
            return;
        }
        // Authenticated: the offer is used up whatever happens next.
        self.offer = None;
        self.pair_lockout = Lockout::default();
        if self.devices.len() + self.provisional_count() >= MAX_DEVICES {
            self.close(conn_id, CloseReason::TooManyDevices, fx);
            return;
        }
        let derived = eph.agree(&er).and_then(|dh| {
            let id = derive_device_id(&secret, &t)?;
            let key = derive_device_key(&secret, &dh, &t, &id, &name)?;
            let channel = Channel::new(Side::Phone, &key, &dh, id, &t)?;
            let mac = pair_phone_mac(&pk, &t, &name, &id)?;
            Ok((id, key, channel, mac))
        });
        drop(eph);
        let Ok((device_id, key, channel, phone_mac)) = derived else {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        if self.devices.iter().any(|d| d.id == device_id) {
            self.close(conn_id, CloseReason::Internal, fx);
            return;
        }
        let record = DeviceRecord {
            id: device_id,
            name,
            key,
            permissions: RemotePermissions::NEW_DEVICE,
            paired_at_ms: now,
        };
        let mut body = Vec::with_capacity(16 + MAC_LEN);
        body.extend_from_slice(&device_id.0);
        body.extend_from_slice(&phone_mac);
        self.establish(conn_id, channel, device_id, Some(Box::new(record)), now);
        fx.push(HubEffect::Send {
            conn_id,
            bytes: encode_record(Kind::PairAccept, &body),
        });
    }

    fn handle_connect(
        &mut self,
        conn_id: ConnId,
        payload: &[u8],
        now: u64,
        fx: &mut Vec<HubEffect>,
    ) {
        let Some((device_id, nc, er, mac)) = parse_connect_request(payload) else {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        let Some(dev) = self.devices.iter().find(|d| d.id == device_id) else {
            // Unknown ids are not counted against anyone.
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        let key = dev.key.clone();
        if self
            .device_lockouts
            .get(&device_id)
            .is_some_and(|l| l.locked(now))
        {
            self.close(conn_id, CloseReason::LockedOut, fx);
            return;
        }
        if self
            .recent_connects
            .iter()
            .any(|(d, n)| *d == device_id && *n == nc)
        {
            self.close(conn_id, CloseReason::Replayed, fx);
            return;
        }
        let Some(eph) = self.take_ephemeral(conn_id) else {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        let t = Transcript {
            np: eph.nonce,
            nc,
            ep: eph.public,
            er,
        };
        let Ok(ak) = auth_key(&key) else {
            self.close(conn_id, CloseReason::Internal, fx);
            return;
        };
        if verify_connect_client_mac(&ak, &device_id, &t, &mac).is_err() {
            self.handshake_mac_failed(conn_id, Some(device_id), now, fx);
            return;
        }
        self.device_lockouts.remove(&device_id);
        if self.recent_connects.len() >= RECENT_CONNECTS {
            self.recent_connects.pop_front();
        }
        self.recent_connects.push_back((device_id, nc));
        let established = eph.agree(&er).and_then(|dh| {
            let reply = connect_phone_mac(&ak, &device_id, &t)?;
            let channel = Channel::new(Side::Phone, &key, &dh, device_id, &t)?;
            Ok((reply, channel))
        });
        drop(eph);
        let Ok((reply, channel)) = established else {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        self.establish(conn_id, channel, device_id, None, now);
        fx.push(HubEffect::Send {
            conn_id,
            bytes: encode_record(Kind::ConnectAccept, &reply),
        });
        self.audit(Some(conn_id), Some(device_id), AuditEvent::Connected, fx);
    }

    fn establish(
        &mut self,
        conn_id: ConnId,
        channel: Channel,
        device_id: DeviceId,
        provisional: Option<Box<DeviceRecord>>,
        now: u64,
    ) {
        if let Some(conn) = self.conns.get_mut(&conn_id) {
            conn.last_rx_ms = now;
            conn.phase = Phase::Established {
                channel: Box::new(channel),
                device_id,
                provisional,
                subscribed: false,
                window_start_ms: now,
                window_count: 0,
                watch: None,
                in_flight: 0,
            };
        }
    }

    /// Adds a device paired on this connection once it has proved it holds the key.
    fn confirm_provisional(&mut self, conn_id: ConnId, fx: &mut Vec<HubEffect>) -> bool {
        let Some(Conn {
            phase: Phase::Established { provisional, .. },
            ..
        }) = self.conns.get_mut(&conn_id)
        else {
            return true;
        };
        let Some(record) = provisional.take() else {
            return true;
        };
        if self.devices.len() >= MAX_DEVICES || self.devices.iter().any(|d| d.id == record.id) {
            self.close(conn_id, CloseReason::TooManyDevices, fx);
            return false;
        }
        let info = record.info();
        let name = record.name.clone();
        self.devices.push(*record);
        fx.push(HubEffect::DevicePaired {
            device: info.clone(),
        });
        fx.push(HubEffect::StoreChanged);
        self.audit(
            Some(conn_id),
            Some(info.id),
            AuditEvent::Paired { name },
            fx,
        );
        self.audit(Some(conn_id), Some(info.id), AuditEvent::Connected, fx);
        true
    }

    fn handle_data(&mut self, conn_id: ConnId, payload: &[u8], now: u64, fx: &mut Vec<HubEffect>) {
        let Some(conn) = self.conns.get_mut(&conn_id) else {
            return;
        };
        let Phase::Established {
            channel, device_id, ..
        } = &mut conn.phase
        else {
            return;
        };
        let device_id = *device_id;
        let Ok(plaintext) = channel.open(payload) else {
            self.close(conn_id, CloseReason::AuthFailed, fx);
            return;
        };
        conn.last_rx_ms = now;
        if !self.confirm_provisional(conn_id, fx) {
            return;
        }
        if self.permissions(Some(device_id)).is_none() {
            self.close(conn_id, CloseReason::Revoked, fx);
            return;
        }
        let over_limit = self.count_message(conn_id, now);
        let decoded = ClientMessage::from_bytes(&plaintext);
        if let Some(count) = over_limit {
            // Cancel is never dropped while there is something to cancel.
            let cancel = matches!(decoded, Ok(ClientMessage::Cancel)) && self.is_busy();
            if !cancel {
                // One reply and one audit entry per window; the rest are dropped silently.
                if count == RATE_MAX_MESSAGES + 1 {
                    self.reject(conn_id, device_id, None, ErrorCode::RateLimited, fx);
                }
                return;
            }
        }
        match decoded {
            Ok(msg) => self.handle_message(conn_id, device_id, msg, fx),
            Err(MessageError::UnknownType(t)) if ViewerRequest::TYPES.contains(&t.as_str()) => {
                match ViewerRequest::from_bytes(&plaintext) {
                    Ok(req) => self.handle_viewer(conn_id, device_id, req, now, fx),
                    Err(_) => self.reject(conn_id, device_id, None, ErrorCode::Malformed, fx),
                }
            }
            Err(MessageError::UnknownType(t)) => {
                self.reject(conn_id, device_id, Some(t), ErrorCode::UnknownMessage, fx);
            }
            Err(MessageError::Malformed(_) | MessageError::TooLarge) => {
                self.reject(conn_id, device_id, None, ErrorCode::Malformed, fx);
            }
        }
    }

    /// Counts one message in the connection's rate window; returns the count when it is
    /// over the limit.
    fn count_message(&mut self, conn_id: ConnId, now: u64) -> Option<u32> {
        let Some(Conn {
            phase:
                Phase::Established {
                    window_start_ms,
                    window_count,
                    ..
                },
            ..
        }) = self.conns.get_mut(&conn_id)
        else {
            return None;
        };
        if now.saturating_sub(*window_start_ms) >= RATE_WINDOW_MS {
            *window_start_ms = now;
            *window_count = 0;
        }
        *window_count = window_count.saturating_add(1);
        (*window_count > RATE_MAX_MESSAGES).then_some(*window_count)
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
                let reveal = !perms.is_none();
                let welcome = ServerMessage::Welcome {
                    protocol_version: PROTOCOL_VERSION,
                    permissions: perms,
                    phone_name: reveal.then(|| self.phone_name.clone()),
                    busy: reveal.then(|| self.is_busy()),
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
                let task_id = self.request_task(device_id);
                let detail = redact_text(&text, MAX_SUMMARY_CHARS);
                fx.push(HubEffect::StartTask {
                    task_id,
                    device_id,
                    text,
                });
                self.accept(conn_id, device_id, request, Some(detail), fx);
            }
            ClientMessage::RunSkill { id, args } => {
                if let Err(r) = self.may_start(perms) {
                    return self.deny(conn_id, device_id, request, r, fx);
                }
                if !self.skills.iter().any(|s| s.id == id) {
                    return self.deny(conn_id, device_id, request, DenyReason::UnknownSkill, fx);
                }
                let Some(args) = clean_args(args) else {
                    return self.reject(
                        conn_id,
                        device_id,
                        Some(request.to_owned()),
                        ErrorCode::InvalidRequest,
                        fx,
                    );
                };
                let task_id = self.request_task(device_id);
                fx.push(HubEffect::RunSkill {
                    task_id,
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
                let task_id = self.request_task(device_id);
                fx.push(HubEffect::RunWorkflow {
                    task_id,
                    device_id,
                    id: id.clone(),
                });
                self.accept(conn_id, device_id, request, Some(id), fx);
            }
            ClientMessage::Cancel => {
                let task_id = match (self.running, self.requested) {
                    (Some(r), _) => r.task_id,
                    (None, Some(r)) => Some(r.task_id),
                    (None, None) => {
                        return self.deny(conn_id, device_id, request, DenyReason::NotRunning, fx)
                    }
                };
                fx.push(HubEffect::CancelTask { device_id, task_id });
                self.accept(conn_id, device_id, request, None, fx);
            }
            ClientMessage::Decision {
                request_id,
                approved,
            } => self.decide(conn_id, device_id, perms, request_id, approved, fx),
            ClientMessage::Subscribe => {
                if !perms.see_progress {
                    return self.deny(conn_id, device_id, request, DenyReason::NotPermitted, fx);
                }
                self.set_subscribed(conn_id, true);
                if let Some(msg) = self.pending_message(perms) {
                    self.send_pending(conn_id, device_id, &msg, fx);
                }
            }
            ClientMessage::Unsubscribe => self.set_subscribed(conn_id, false),
            ClientMessage::ListSkills => {
                if !perms.start_tasks {
                    return self.deny(conn_id, device_id, request, DenyReason::NotPermitted, fx);
                }
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

    fn decide(
        &mut self,
        conn_id: ConnId,
        device_id: DeviceId,
        perms: RemotePermissions,
        request_id: u64,
        approved: bool,
        fx: &mut Vec<HubEffect>,
    ) {
        let request = "decision";
        if approved && !perms.approve {
            return self.deny(conn_id, device_id, request, DenyReason::NotPermitted, fx);
        }
        let Some(p) = self
            .pending
            .as_ref()
            .filter(|p| p.id == request_id && !p.decided)
        else {
            return self.deny(conn_id, device_id, request, DenyReason::StaleRequest, fx);
        };
        let category = p.category;
        if approved && (p.phone_only || !remotely_approvable(category)) {
            return self.deny(
                conn_id,
                device_id,
                request,
                DenyReason::NotRemotelyApprovable,
                fx,
            );
        }
        if !approved && !p.notified.contains(&device_id) {
            // Declining needs no permission, but only a device that was shown the prompt.
            return self.deny(conn_id, device_id, request, DenyReason::NotPermitted, fx);
        }
        if let Some(p) = self.pending.as_mut() {
            p.decided = true;
        }
        fx.push(HubEffect::Decide {
            request_id,
            approved,
            category,
            device_id,
        });
        let detail = format!(
            "request {request_id} {}",
            if approved { "approved" } else { "declined" }
        );
        self.accept(conn_id, device_id, request, Some(detail), fx);
    }

    fn request_task(&mut self, origin: DeviceId) -> u64 {
        let task_id = self.next_task_id;
        self.next_task_id = self.next_task_id.saturating_add(1);
        self.requested = Some(Requested { task_id, origin });
        task_id
    }

    fn may_start(&self, perms: RemotePermissions) -> Result<(), DenyReason> {
        if !perms.start_tasks {
            Err(DenyReason::NotPermitted)
        } else if self.is_busy() {
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
        self.seal_and_send(conn_id, &bytes, fx);
    }

    /// Seals plaintext for one connection, emits it and counts it as in flight.
    fn seal_and_send(&mut self, conn_id: ConnId, plaintext: &[u8], fx: &mut Vec<HubEffect>) {
        let Some(Conn {
            phase: Phase::Established {
                channel, in_flight, ..
            },
            ..
        }) = self.conns.get_mut(&conn_id)
        else {
            return;
        };
        match channel.seal(plaintext) {
            Ok(body) => {
                let bytes = encode_record(Kind::Data, &body);
                *in_flight = in_flight.saturating_add(bytes.len());
                fx.push(HubEffect::Send { conn_id, bytes });
            }
            Err(_) => self.close(conn_id, CloseReason::Internal, fx),
        }
    }

    fn send_viewer(&mut self, conn_id: ConnId, msg: &ViewerMessage, fx: &mut Vec<HubEffect>) {
        if let Ok(bytes) = msg.to_bytes() {
            self.seal_and_send(conn_id, &bytes, fx);
        }
    }

    // ---- Live view stream -----------------------------------------------------------

    fn watch_ref(&self, conn_id: ConnId) -> Option<&Watch> {
        match &self.conns.get(&conn_id)?.phase {
            Phase::Established { watch, .. } => watch.as_deref(),
            Phase::Handshake { .. } => None,
        }
    }

    fn watch_mut(&mut self, conn_id: ConnId) -> Option<&mut Watch> {
        match &mut self.conns.get_mut(&conn_id)?.phase {
            Phase::Established { watch, .. } => watch.as_deref_mut(),
            Phase::Handshake { .. } => None,
        }
    }

    /// Connections watching whose device still holds the permission.
    fn watcher_ids(&self) -> Vec<ConnId> {
        self.conns
            .iter()
            .filter_map(|(id, c)| match &c.phase {
                Phase::Established {
                    device_id,
                    watch: Some(_),
                    ..
                } if self.watch_allowed.contains(device_id) => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn pump_all(&mut self, now: u64, fx: &mut Vec<HubEffect>) {
        for id in self.watcher_ids() {
            self.pump(id, now, fx);
        }
    }

    /// Hands Live view messages to the shell while the window allows: a snapshot first
    /// when one is due, then a held-back screen once it is due, then deltas in order.
    fn pump(&mut self, conn_id: ConnId, now: u64, fx: &mut Vec<HubEffect>) {
        loop {
            let step = {
                let Some(Conn {
                    phase:
                        Phase::Established {
                            watch: Some(w),
                            in_flight,
                            device_id,
                            ..
                        },
                    ..
                }) = self.conns.get_mut(&conn_id)
                else {
                    return;
                };
                if !self.watch_allowed.contains(device_id) || *in_flight >= VIEWER_WINDOW_BYTES {
                    return;
                }
                if !w.needs_snapshot && w.screen_due(now) {
                    if let Some(screen) = w.pending_screen.take() {
                        w.last_screen_ms = Some(now);
                        w.enqueue(screen);
                    }
                }
                if w.needs_snapshot {
                    PumpStep::Snapshot
                } else if let Some(msg) = w.queue.pop_front() {
                    w.queued_bytes = w.queued_bytes.saturating_sub(msg.len());
                    PumpStep::Send(msg)
                } else {
                    PumpStep::Done
                }
            };
            match step {
                PumpStep::Done => return,
                PumpStep::Send(msg) => self.seal_and_send(conn_id, &msg, fx),
                PumpStep::Snapshot => {
                    let parts = self.viewer.snapshot();
                    let n = u32::try_from(parts.len()).unwrap_or(u32::MAX);
                    let msgs: Vec<Vec<u8>> = parts
                        .iter()
                        .zip(0u32..)
                        .filter_map(|(d, i)| {
                            ViewerMessage::snapshot(&d.encode(), i, n)
                                .and_then(|m| m.to_bytes())
                                .ok()
                        })
                        .collect();
                    if let Some(w) = self.watch_mut(conn_id) {
                        w.queue.clear();
                        w.queued_bytes = 0;
                        w.pending_screen = None;
                        w.needs_snapshot = false;
                        // The snapshot carries the current screen.
                        w.last_screen_ms = Some(now);
                    }
                    for msg in msgs {
                        self.seal_and_send(conn_id, &msg, fx);
                    }
                }
            }
        }
    }

    fn handle_viewer(
        &mut self,
        conn_id: ConnId,
        device_id: DeviceId,
        req: ViewerRequest,
        now: u64,
        fx: &mut Vec<HubEffect>,
    ) {
        let request = req.name();
        let allowed = self.watch_allowed.contains(&device_id);
        match req {
            ViewerRequest::StartWatching => {
                if !allowed {
                    return self.viewer_deny(conn_id, device_id, request, fx);
                }
                if let Some(Conn {
                    phase: Phase::Established { watch, .. },
                    ..
                }) = self.conns.get_mut(&conn_id)
                {
                    // A repeated start restarts the stream with a snapshot.
                    *watch = Some(Box::new(Watch {
                        needs_snapshot: true,
                        ..Watch::default()
                    }));
                }
                self.accept(conn_id, device_id, request, None, fx);
                self.pump(conn_id, now, fx);
            }
            ViewerRequest::StopWatching => {
                let was = match self.conns.get_mut(&conn_id) {
                    Some(Conn {
                        phase: Phase::Established { watch, .. },
                        ..
                    }) => watch.take().is_some(),
                    _ => false,
                };
                self.send_viewer(
                    conn_id,
                    &ViewerMessage::ViewerEnded {
                        reason: ViewerEndReason::Stopped,
                    },
                    fx,
                );
                if was {
                    self.accept(conn_id, device_id, request, None, fx);
                }
            }
            ViewerRequest::ViewerResync => {
                if !allowed {
                    return self.viewer_deny(conn_id, device_id, request, fx);
                }
                // Coalesced: one snapshot however often it is asked for.
                if let Some(w) = self.watch_mut(conn_id) {
                    w.resync();
                }
                self.pump(conn_id, now, fx);
            }
        }
    }

    /// `viewer_denied { not_permitted }`, audited like every refusal.
    fn viewer_deny(
        &mut self,
        conn_id: ConnId,
        device_id: DeviceId,
        request: &str,
        fx: &mut Vec<HubEffect>,
    ) {
        let reason = DenyReason::NotPermitted;
        self.send_viewer(conn_id, &ViewerMessage::ViewerDenied { reason }, fx);
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

    fn watch_screen_changed(
        &mut self,
        device_id: DeviceId,
        allowed: bool,
        fx: &mut Vec<HubEffect>,
    ) {
        if !self.devices.iter().any(|d| d.id == device_id) {
            return;
        }
        let changed = if allowed {
            self.watch_allowed.insert(device_id)
        } else {
            self.watch_allowed.remove(&device_id)
        };
        if !changed {
            return;
        }
        fx.push(HubEffect::StoreChanged);
        self.audit(
            None,
            Some(device_id),
            AuditEvent::Accepted {
                request: "watch_screen_permission".to_owned(),
                detail: Some(if allowed { "on" } else { "off" }.to_owned()),
            },
            fx,
        );
        if allowed {
            return;
        }
        // Stop every stream of this device now, in this call.
        let stopped: Vec<ConnId> = self
            .conns
            .iter_mut()
            .filter_map(|(id, c)| match &mut c.phase {
                Phase::Established {
                    device_id: d,
                    watch,
                    ..
                } if *d == device_id => watch.take().map(|_| *id),
                _ => None,
            })
            .collect();
        for id in stopped {
            self.send_viewer(
                id,
                &ViewerMessage::ViewerEnded {
                    reason: ViewerEndReason::PermissionRevoked,
                },
                fx,
            );
            self.audit(
                Some(id),
                Some(device_id),
                AuditEvent::Accepted {
                    request: "stop_watching".to_owned(),
                    detail: Some("permission revoked".to_owned()),
                },
                fx,
            );
        }
    }

    /// Sends a `ConfirmPending` and records that the device was shown it.
    fn send_pending(
        &mut self,
        conn_id: ConnId,
        device_id: DeviceId,
        msg: &ServerMessage,
        fx: &mut Vec<HubEffect>,
    ) {
        if let Some(p) = self.pending.as_mut() {
            p.notified.insert(device_id);
        }
        self.send(conn_id, msg, fx);
    }

    fn views(&self) -> Vec<(ConnId, ConnView)> {
        self.conns
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
            .collect()
    }

    /// Sends to every authenticated connection the closure picks, with a message built
    /// for that connection's device.
    fn broadcast(
        &mut self,
        fx: &mut Vec<HubEffect>,
        build: impl Fn(&RemoteHub, ConnView) -> Option<ServerMessage>,
    ) {
        for (id, view) in self.views() {
            if let Some(msg) = build(self, view) {
                self.send(id, &msg, fx);
            }
        }
    }

    fn pending_message(&self, perms: RemotePermissions) -> Option<ServerMessage> {
        let p = self.pending.as_ref().filter(|p| !p.decided)?;
        let can_approve = !p.phone_only && perms.can_approve(p.category);
        let summary = if perms.see_screen_text || can_approve {
            redact_text(&p.summary, MAX_SUMMARY_CHARS)
        } else {
            format!("Waiting for confirmation ({})", category_name(p.category))
        };
        Some(ServerMessage::ConfirmPending {
            request_id: p.id,
            category: p.category,
            summary,
            can_approve,
        })
    }

    fn broadcast_pending(&mut self, fx: &mut Vec<HubEffect>) {
        let Some((category, phone_only)) =
            self.pending.as_ref().map(|p| (p.category, p.phone_only))
        else {
            return;
        };
        for (id, view) in self.views() {
            let Some(device_id) = view.device_id else {
                continue;
            };
            let Some(perms) = self.permissions(Some(device_id)) else {
                continue;
            };
            if !(view.subscribed || (!phone_only && perms.can_approve(category))) {
                continue;
            }
            if let Some(msg) = self.pending_message(perms) {
                self.send_pending(id, device_id, &msg, fx);
            }
        }
    }
}

/// What a broadcast closure sees of one authenticated connection.
#[derive(Debug, Clone, Copy)]
struct ConnView {
    device_id: Option<DeviceId>,
    subscribed: bool,
}

type PairRequest = ([u8; NONCE_LEN], [u8; PUBLIC_LEN], String, [u8; MAC_LEN]);

fn parse_pair_request(payload: &[u8]) -> Option<PairRequest> {
    let (nc, rest) = payload.split_at_checked(NONCE_LEN)?;
    let (er, rest) = rest.split_at_checked(PUBLIC_LEN)?;
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
    Some((
        nc.try_into().ok()?,
        er.try_into().ok()?,
        name.to_owned(),
        mac.try_into().ok()?,
    ))
}

type ConnectRequest = (DeviceId, [u8; NONCE_LEN], [u8; PUBLIC_LEN], [u8; MAC_LEN]);

fn parse_connect_request(payload: &[u8]) -> Option<ConnectRequest> {
    let (id, rest) = payload.split_at_checked(16)?;
    let (nc, rest) = rest.split_at_checked(NONCE_LEN)?;
    let (er, mac) = rest.split_at_checked(PUBLIC_LEN)?;
    if mac.len() != MAC_LEN {
        return None;
    }
    Some((
        DeviceId(id.try_into().ok()?),
        nc.try_into().ok()?,
        er.try_into().ok()?,
        mac.try_into().ok()?,
    ))
}

/// Cleans and caps skill arguments; `None` when there are too many, a name is empty or
/// collides after cleaning, or the total is too long.
fn clean_args(args: BTreeMap<String, String>) -> Option<BTreeMap<String, String>> {
    if args.len() > MAX_SKILL_ARGS {
        return None;
    }
    let mut out = BTreeMap::new();
    let mut total = 0usize;
    for (k, v) in args {
        let key = clean_text(&k, MAX_SKILL_ARG_KEY_CHARS, false);
        if key.is_empty() || out.contains_key(&key) {
            return None;
        }
        let value = clean_text(&v, MAX_SKILL_ARG_VALUE_CHARS, true);
        total = total.saturating_add(value.chars().count());
        out.insert(key, value);
    }
    (total <= MAX_SKILL_ARGS_TOTAL_CHARS).then_some(out)
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
        OutcomeStatus::Interrupted => "Task interrupted (the phone stayed locked)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockout_backoff_doubles_and_caps() {
        let mut l = Lockout::default();
        for i in 0..LOCKOUT_THRESHOLD - 1 {
            assert_eq!(l.fail(1_000 + u64::from(i)), None);
        }
        assert_eq!(l.fail(1_004), Some(1_004 + LOCKOUT_BASE_MS));
        assert_eq!(l.fail(2_000), Some(2_000 + 2 * LOCKOUT_BASE_MS));
        for _ in 0..40 {
            l.fail(3_000);
        }
        assert_eq!(l.locked_until_ms, 3_000 + LOCKOUT_MAX_MS);
        // Forgotten after a quiet period.
        l.fail(3_000 + FAILURE_MEMORY_MS);
        assert_eq!(l.failures, 1);
    }

    #[test]
    fn skill_args_are_cleaned_and_bounded() {
        let mut a = BTreeMap::new();
        a.insert(
            "msg\u{202E}".to_owned(),
            format!("hi\u{0}\u{202E}{}", "x".repeat(5_000)),
        );
        let out = clean_args(a).unwrap();
        let v = &out["msg"];
        assert_eq!(v.chars().count(), MAX_SKILL_ARG_VALUE_CHARS);
        assert!(!v.contains('\u{0}') && !v.contains('\u{202E}'));
        let many: BTreeMap<String, String> = (0..=MAX_SKILL_ARGS)
            .map(|i| (format!("k{i}"), "v".into()))
            .collect();
        assert!(clean_args(many).is_none());
        let big: BTreeMap<String, String> = (0..3)
            .map(|i| (format!("k{i}"), "v".repeat(2_000)))
            .collect();
        assert!(clean_args(big).is_none());
        let mut clash = BTreeMap::new();
        clash.insert("a".to_owned(), "1".to_owned());
        clash.insert("a\u{200B}".to_owned(), "2".to_owned());
        assert!(clean_args(clash).is_none());
    }

    #[test]
    fn rename_device_cleans_and_persists() {
        let id = DeviceId([1; 16]);
        let store = DeviceStore {
            devices: vec![DeviceRecord {
                id,
                name: "Mac".into(),
                key: crate::crypto::Secret32::from_bytes([9; 32]),
                permissions: RemotePermissions::NEW_DEVICE,
                paired_at_ms: 1,
            }],
            nonce_lease: 0,
        };
        let mut hub = RemoteHub::with_store("Phone", store).unwrap();
        assert_eq!(hub.clock_ms(), 0);
        assert_eq!(
            hub.rename_device(id, "  Work\u{202E} Mac \n"),
            Ok(vec![HubEffect::StoreChanged])
        );
        assert_eq!(hub.devices()[0].name, "Work Mac");
        assert_eq!(hub.rename_device(id, "Work Mac"), Ok(vec![]));
        assert_eq!(
            hub.rename_device(id, " \u{200B} "),
            Err(HubError::InvalidName)
        );
        assert_eq!(
            hub.rename_device(DeviceId([2; 16]), "x"),
            Err(HubError::UnknownDevice)
        );
        hub.tick(42);
        assert_eq!(hub.clock_ms(), 42);
    }
}
