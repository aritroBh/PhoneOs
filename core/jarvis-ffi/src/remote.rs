//! Remote control for Swift: `JarvisRemoteHub` (the phone's side) and
//! `JarvisRemoteClient` (a remote device's side, for a Mac app or Swift tests).
//!
//! Both wrap the sans-IO state machines of `jarvis-remote`: Swift moves bytes over its
//! own transport, supplies the clock (`nowMs`, monotonic) and the entropy (`Data` from
//! `SecRandomCopyBytes`, at least 32 fresh bytes per call; Rust has no randomness source of
//! its own), and carries out the returned effects in order.
//!
//! # Secrets across the boundary
//!
//! Exactly three things that cross into Swift are secret, and each is returned by one call:
//! `PairingOffer` (from `openPairing`: the pairing code and QR payload), the hub store
//! JSON (from `toJson`: every device key) and a remote's credentials JSON (in
//! `ClientEvent.paired`). Inside Rust they live in zeroizing buffers; the copies Swift gets
//! are ordinary `String`s that Swift must treat as secrets: Keychain only (store JSON and
//! credentials: `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synchronizable),
//! never logged, never in `UserDefaults`, files, the pasteboard or crash reports. No error,
//! no `Debug` output and no audit entry contains any of them.
//!
//! # Poisoning
//!
//! Every call runs inside the panic guard. A panic inside the hub (or client) makes the
//! object unusable: later calls throw `JarvisError.poisoned`. Recreate the hub from the
//! JSON last written to the Keychain (the nonce lease is safe: the effects of the
//! panicking call never reached Swift).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard};

use jarvis_remote as r;
use jarvis_remote::{RemoteClient, RemoteHub};
use zeroize::Zeroizing;

use crate::convert::device_id_from;
use crate::error::JarvisError;
use crate::guard::{guarded, panic_message};
use crate::types::{
    ClientEvent, ClientMessage, ClientReceived, DeviceInfo, HubEffect, LocalEvent, PairingOffer,
    PendingDevice, RemotePermissions, RiskCategory,
};

/// Maps a store or credentials loading error without echoing any input (the input holds
/// keys, and serde's messages can quote it).
pub(crate) fn persist_error(e: r::PersistError) -> JarvisError {
    let message = match e {
        r::PersistError::Corrupt(_) => "not valid JSON for the schema".to_owned(),
        r::PersistError::UnsupportedVersion(v) => format!("unsupported version {v}"),
        // Fixed texts in `jarvis-remote` ("invalid key", "too many devices", ...).
        r::PersistError::Invalid(m) => m,
    };
    JarvisError::RemoteStoreInvalid { message }
}

fn hub_error(e: r::HubError, device: Option<&r::DeviceId>) -> JarvisError {
    match e {
        r::HubError::TooManyDevices => JarvisError::RemoteLimitReached {
            what: "devices".to_owned(),
            max: u32::try_from(r::MAX_DEVICES).unwrap_or(u32::MAX),
        },
        r::HubError::UnknownDevice => JarvisError::RemoteUnknownDevice {
            device_id: device.map(r::DeviceId::to_hex).unwrap_or_default(),
        },
        r::HubError::Entropy(e) => JarvisError::RemoteEntropy { problem: e.into() },
        r::HubError::Crypto => JarvisError::Internal {
            message: "remote: internal cryptographic error".to_owned(),
        },
        r::HubError::InvalidName => JarvisError::RemoteInvalidName,
    }
}

fn client_error(e: r::ClientError) -> JarvisError {
    match e {
        r::ClientError::Code(c) => JarvisError::RemoteInvalidCode {
            message: c.to_string(),
        },
        r::ClientError::Entropy(e) => JarvisError::RemoteEntropy { problem: e.into() },
        r::ClientError::InvalidName => JarvisError::RemoteInvalidName,
        r::ClientError::Auth => JarvisError::RemoteAuthFailed,
        r::ClientError::NotConnected => JarvisError::RemoteNotConnected,
        r::ClientError::Message(m) => JarvisError::RemoteMessage {
            message: m.to_string(),
        },
    }
}

fn check_entropy(entropy: &[u8]) -> Result<(), JarvisError> {
    r::crypto::check_entropy(entropy).map_err(|e| JarvisError::RemoteEntropy { problem: e.into() })
}

fn effects(fx: Vec<r::HubEffect>) -> Vec<HubEffect> {
    fx.into_iter().map(Into::into).collect()
}

const POISONED: &str =
    "remote object unusable after an internal error; recreate it from the stored JSON";

/// Runs `f` on the object in `slot`, poisoning it (setting `None`) if `f` panics.
fn with_slot<S, T>(
    slot: &Mutex<Option<S>>,
    what: &str,
    f: impl FnOnce(&mut S) -> Result<T, JarvisError>,
) -> Result<T, JarvisError> {
    guarded(what, || {
        let mut guard: MutexGuard<'_, Option<S>> = match slot.lock() {
            Ok(g) => g,
            Err(poison) => {
                slot.clear_poison();
                let mut g = poison.into_inner();
                *g = None;
                g
            }
        };
        let Some(inner) = guard.as_mut() else {
            return Err(JarvisError::Poisoned {
                message: POISONED.to_owned(),
            });
        };
        match catch_unwind(AssertUnwindSafe(|| f(inner))) {
            Ok(r) => r,
            Err(payload) => {
                *guard = None;
                Err(JarvisError::Internal {
                    message: format!("{what}: {}", panic_message(payload.as_ref())),
                })
            }
        }
    })
}

// ---------------------------------------------------------------------------------------
// The phone's hub.
// ---------------------------------------------------------------------------------------

/// The phone's remote-control hub. Swift: `JarvisRemoteHub`.
///
/// The shell's duties (see also docs/ARCHITECTURE.md):
/// - Carry out the returned `[HubEffect]` in order. On `storeChanged`, write `toJson()` to
///   the Keychain before performing any later effect.
/// - Feed every transport event: `connectionOpened`, `receive`, `connectionClosed`, and
///   `tick` every few seconds while the listener runs.
/// - Report what the phone does with `localEvent`: `sessionStarted`/`sessionFinished`
///   with the hub's task id (nil for the owner's own tasks), `progress`,
///   `confirmRequested` then `confirmResolved` with `pendingRequestId()`, and
///   `catalogChanged` whenever the remotely runnable skills or workflows change.
/// - Supply fresh CSPRNG entropy for `openPairing` and `connectionOpened`.
#[derive(uniffi::Object)]
pub struct JarvisRemoteHub {
    hub: Mutex<Option<RemoteHub>>,
}

impl JarvisRemoteHub {
    /// Builds a hub from persisted JSON (empty: no devices). Shared with Rust tests.
    pub fn from_json(store_json: &str, phone_name: &str) -> Result<Self, JarvisError> {
        let hub = if store_json.trim().is_empty() {
            RemoteHub::new(phone_name)
        } else {
            RemoteHub::from_json(phone_name, store_json).map_err(persist_error)?
        };
        Ok(JarvisRemoteHub {
            hub: Mutex::new(Some(hub)),
        })
    }

    pub(crate) fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&mut RemoteHub) -> Result<T, JarvisError>,
    ) -> Result<T, JarvisError> {
        with_slot(&self.hub, what, f)
    }

    /// Test hook: poison the hub as a panic inside it would.
    #[cfg(any(test, feature = "panic-hooks"))]
    pub fn arm_poison(&self) {
        let _ = self.with("debug_poison", |_| -> Result<(), JarvisError> {
            #[allow(clippy::panic)]
            {
                panic!("deliberate panic from the panic-hooks test hook");
            }
        });
    }
}

fn known(hub: &RemoteHub, id: &r::DeviceId) -> Result<(), JarvisError> {
    if hub.devices().iter().any(|d| d.id == *id) {
        Ok(())
    } else {
        Err(hub_error(r::HubError::UnknownDevice, Some(id)))
    }
}

/// A confirmed device, or one waiting for confirmation (what `revokeDevice` removes).
fn known_or_pending(hub: &RemoteHub, id: &r::DeviceId) -> Result<(), JarvisError> {
    if hub.pending_devices().iter().any(|p| p.device.id == *id) {
        Ok(())
    } else {
        known(hub, id)
    }
}

#[uniffi::export]
impl JarvisRemoteHub {
    /// Restores the hub from the JSON `toJson()` returned last time (empty for a phone
    /// with no paired devices). `phoneName` is what remotes are shown (cleaned). A corrupt,
    /// newer or inconsistent store throws `JarvisError.remoteStoreInvalid` (no key material
    /// in the message); keep the Keychain item and report it, do not overwrite it.
    #[uniffi::constructor]
    pub fn new(store_json: String, phone_name: String) -> Result<Arc<Self>, JarvisError> {
        let store_json = Zeroizing::new(store_json);
        guarded("JarvisRemoteHub.init", || {
            Self::from_json(&store_json, &phone_name).map(Arc::new)
        })
    }

    /// SECRET OUTPUT. The persisted state with every device key, for the Keychain only
    /// (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synchronizable). Write it
    /// whenever an effect list contains `storeChanged`, BEFORE carrying out the effects
    /// after it (a new device's first frame and the phone-nonce lease depend on it). Never
    /// log it or keep it in `UserDefaults` or a file.
    pub fn to_json(&self) -> Result<String, JarvisError> {
        self.with("to_json", |h| Ok(h.to_json().as_str().to_owned()))
    }

    /// SECRET OUTPUT. Opens a single-use pairing offer valid for 5 minutes (replacing any
    /// open one); show `code`/`qrPayload` on screen only. `entropy`: at least 32 fresh
    /// bytes from `SecRandomCopyBytes`. Throws `remoteEntropy` (weak or short entropy) or
    /// `remoteLimitReached` (8 devices paired or waiting). Wrong codes are counted per
    /// offer: a new offer starts unlocked, whatever was tried against the last one
    /// (`remotePairingLocked` is no longer thrown here).
    pub fn open_pairing(&self, now_ms: u64, entropy: Vec<u8>) -> Result<PairingOffer, JarvisError> {
        let entropy = Zeroizing::new(entropy);
        self.with("open_pairing", |h| {
            let offer = h
                .open_pairing(now_ms, &entropy)
                .map_err(|e| hub_error(e, None))?;
            Ok(offer.into())
        })
    }

    /// Wrong codes tried against the open offer (0 when none is open). Above zero, the
    /// pairing sheet says that wrong codes are being tried from the network.
    pub fn pairing_failed_attempts(&self) -> Result<u32, JarvisError> {
        self.with("pairing_failed_attempts", |h| {
            Ok(h.pairing_failed_attempts())
        })
    }

    /// Devices paired but not yet confirmed by the owner, in pairing order, with their
    /// check strings. They hold no permission; a pending device not confirmed within 10
    /// minutes is removed by the hub (audited).
    pub fn pending_devices(&self) -> Result<Vec<PendingDevice>, JarvisError> {
        self.with("pending_devices", |h| {
            Ok(h.pending_devices()
                .into_iter()
                .map(|p| PendingDevice {
                    device: p.device.into(),
                    check: p.check,
                    expires_at_ms: p.expires_at_ms,
                })
                .collect())
        })
    }

    /// The owner confirmed a pending device on the phone (after comparing the check
    /// string it shows with the one on the device they paired). It gets exactly the
    /// new-device permissions; its connections are sent a `welcome`. Returns
    /// `deviceConfirmed` and `storeChanged` (plus any timeout effects, like `tick`). Throws
    /// `remoteUnknownDevice` when it is not pending (expired, removed, confirmed).
    pub fn confirm_device(
        &self,
        now_ms: u64,
        device_id: String,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("confirm_device", |h| {
            let id = device_id_from(&device_id)?;
            h.confirm_device(id, now_ms)
                .map(effects)
                .map_err(|e| hub_error(e, Some(&id)))
        })
    }

    /// Withdraws the open pairing offer (the owner closed the pairing screen).
    pub fn cancel_pairing(&self) -> Result<(), JarvisError> {
        self.with("cancel_pairing", |h| {
            h.cancel_pairing();
            Ok(())
        })
    }

    /// Whether a pairing offer is open (it closes when used, expired or cancelled).
    pub fn is_pairing_open(&self) -> Result<bool, JarvisError> {
        self.with("is_pairing_open", |h| Ok(h.pairing_open()))
    }

    /// A transport connection opened (`connId` is the shell's own id, unique while open).
    /// `entropy`: at least 32 fresh CSPRNG bytes for this connection. Weak or short
    /// entropy throws `remoteEntropy` and the hub does not register the connection: close
    /// the transport yourself and fix the entropy source.
    pub fn connection_opened(
        &self,
        conn_id: u64,
        now_ms: u64,
        entropy: Vec<u8>,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        let entropy = Zeroizing::new(entropy);
        self.with("connection_opened", |h| {
            check_entropy(&entropy)?;
            Ok(effects(h.connection_opened(conn_id, now_ms, &entropy)))
        })
    }

    /// Bytes arrived on a connection (any chunking). Unknown connection ids are ignored.
    pub fn receive(
        &self,
        conn_id: u64,
        bytes: Vec<u8>,
        now_ms: u64,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("receive", |h| {
            Ok(effects(h.receive(conn_id, &bytes, now_ms)))
        })
    }

    /// The transport connection closed (idempotent). Applies timeouts first, like `tick`.
    pub fn connection_closed(
        &self,
        conn_id: u64,
        now_ms: u64,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("connection_closed", |h| {
            let mut fx = h.tick(now_ms);
            fx.extend(h.connection_closed(conn_id));
            Ok(effects(fx))
        })
    }

    /// Applies timeouts (handshake, idle, offer expiry) and flushes merged audit entries.
    /// Call every few seconds while the listener runs.
    pub fn tick(&self, now_ms: u64) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("tick", |h| Ok(effects(h.tick(now_ms))))
    }

    /// Something happened on the phone (see `LocalEvent`). Throws `invalidJson` for a
    /// progress whose last action carries invalid tool-argument JSON and
    /// `remoteUnknownDevice` for a device id that is not 32 hex characters (an unknown
    /// but well-formed id is ignored by the hub; `revokeDevice`/`setPermissions` check).
    pub fn local_event(
        &self,
        now_ms: u64,
        event: LocalEvent,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("local_event", |h| {
            let event: r::LocalEvent = event.try_into()?;
            Ok(effects(h.local_event(now_ms, event)))
        })
    }

    /// The paired (confirmed) devices, in pairing order, without keys, with their
    /// `watchScreen` permission.
    pub fn devices(&self) -> Result<Vec<DeviceInfo>, JarvisError> {
        self.with("devices", |h| {
            Ok(h.devices()
                .into_iter()
                .map(|d| {
                    let watch_screen = h.watch_screen(d.id);
                    DeviceInfo {
                        watch_screen,
                        ..d.into()
                    }
                })
                .collect())
        })
    }

    /// Renames a paired device (cleaned like a pairing name). Throws
    /// `remoteUnknownDevice` or `remoteInvalidName` (empty once cleaned). Returns
    /// `storeChanged` when the name changed (plus any timeout effects, like `tick`).
    pub fn rename_device(
        &self,
        now_ms: u64,
        device_id: String,
        name: String,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("rename_device", |h| {
            let id = device_id_from(&device_id)?;
            known(h, &id)?;
            let renamed = h
                .rename_device(id, &name)
                .map_err(|e| hub_error(e, Some(&id)))?;
            let mut fx = h.tick(now_ms);
            fx.extend(renamed);
            Ok(effects(fx))
        })
    }

    /// Changes what a device may do (applies to its next message; its connections are
    /// told). Throws `remoteUnknownDevice`.
    pub fn set_permissions(
        &self,
        now_ms: u64,
        device_id: String,
        permissions: RemotePermissions,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("set_permissions", |h| {
            let id = device_id_from(&device_id)?;
            known(h, &id)?;
            Ok(effects(h.local_event(
                now_ms,
                r::LocalEvent::PermissionsChanged {
                    device_id: id,
                    permissions: permissions.into(),
                },
            )))
        })
    }

    /// Revokes a device (or removes one waiting for confirmation): forgets its key, closes
    /// its connections and cancels the task it requested (never one the owner started).
    /// Throws `remoteUnknownDevice`.
    pub fn revoke_device(
        &self,
        now_ms: u64,
        device_id: String,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("revoke_device", |h| {
            let id = device_id_from(&device_id)?;
            known_or_pending(h, &id)?;
            Ok(effects(h.local_event(
                now_ms,
                r::LocalEvent::DeviceRevoked { device_id: id },
            )))
        })
    }

    /// The hub's monotonic clock as of the last call: what `expiresAtMs`, audit `atMs`
    /// and the lockout ends are measured on (equal to the shell's clock until that steps
    /// back).
    pub fn clock_ms(&self) -> Result<u64, JarvisError> {
        self.with("clock_ms", |h| Ok(h.clock_ms()))
    }

    /// End of the open offer's pairing lockout on the hub clock (0 when no offer is open
    /// or it was never locked; in the past when it is over).
    pub fn pairing_locked_until_ms(&self) -> Result<u64, JarvisError> {
        self.with("pairing_locked_until_ms", |h| {
            Ok(h.pairing_locked_until_ms())
        })
    }

    /// End of the connect lockout for one device on the hub clock (0: none).
    pub fn device_locked_until_ms(&self, device_id: String) -> Result<u64, JarvisError> {
        self.with("device_locked_until_ms", |h| {
            Ok(h.device_locked_until_ms(device_id_from(&device_id)?))
        })
    }

    /// The hub's id for the confirmation now pending. Read it right after
    /// `localEvent(.confirmRequested)` and pass it back in `.confirmResolved`.
    pub fn pending_request_id(&self) -> Result<Option<u64>, JarvisError> {
        self.with("pending_request_id", |h| Ok(h.pending_request_id()))
    }

    /// Whether a task runs, or was requested by a remote and not yet reported finished.
    pub fn is_busy(&self) -> Result<bool, JarvisError> {
        self.with("is_busy", |h| Ok(h.is_busy()))
    }

    /// Open connections the hub knows.
    pub fn connection_count(&self) -> Result<u32, JarvisError> {
        self.with("connection_count", |h| {
            Ok(u32::try_from(h.connection_count()).unwrap_or(u32::MAX))
        })
    }

    /// Whether `connId` finished its handshake.
    pub fn is_authenticated(&self, conn_id: u64) -> Result<bool, JarvisError> {
        self.with("is_authenticated", |h| Ok(h.is_authenticated(conn_id)))
    }

    // ---- Live view over remote control (see `JarvisViewer.feedAndStream`) ------------

    /// The transport finished writing `bytes` of this connection's `send` effects: report
    /// it from the write completion, for every `send` (not only the Live view's). The Live
    /// view is handed out only while fewer than 128 KiB are unconfirmed, so a connection
    /// that stops reading never makes the phone queue more (the hub drops its queue and
    /// sends one snapshot once it reads again). Returns frames that were waiting for room.
    pub fn written(
        &self,
        conn_id: u64,
        bytes: u64,
        now_ms: u64,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("written", |h| {
            let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
            Ok(effects(h.written(conn_id, bytes, now_ms)))
        })
    }

    /// The owner turned "Watch the screen layout" on or off for a device. Off by default;
    /// independent of `RemotePermissions` (it never makes anything approvable). Returns
    /// `storeChanged` and an audit entry when it changed; off also ends that device's
    /// streams in this call (`viewer_ended { permission_revoked }`, audited). Throws
    /// `remoteUnknownDevice`.
    pub fn set_watch_screen(
        &self,
        now_ms: u64,
        device_id: String,
        allowed: bool,
    ) -> Result<Vec<HubEffect>, JarvisError> {
        self.with("set_watch_screen", |h| {
            let id = device_id_from(&device_id)?;
            known(h, &id)?;
            Ok(effects(h.set_watch_screen(id, allowed, now_ms)))
        })
    }

    /// Whether a device may watch the screen layout (`false` for unknown devices).
    pub fn watch_screen(&self, device_id: String) -> Result<bool, JarvisError> {
        self.with("watch_screen", |h| {
            Ok(h.watch_screen(device_id_from(&device_id)?))
        })
    }

    /// Whether connection `connId` receives the Live view now.
    pub fn is_watching(&self, conn_id: u64) -> Result<bool, JarvisError> {
        self.with("is_watching", |h| Ok(h.is_watching(conn_id)))
    }
}

/// Exported test hook (feature `panic-hooks`).
#[cfg(feature = "panic-hooks")]
#[uniffi::export]
impl JarvisRemoteHub {
    /// Panics inside the hub (test builds only): the hub is poisoned afterwards.
    pub fn debug_poison(&self) {
        self.arm_poison();
    }
}

// ---------------------------------------------------------------------------------------
// A remote device.
// ---------------------------------------------------------------------------------------

/// One connection from a remote device to the phone. Swift: `JarvisRemoteClient`
/// (create with `pair(code:deviceName:entropy:)` or `connect(credentialsJson:entropy:)`).
/// Feed every byte from the phone to `receive`, write out every `ClientEvent.send`, and
/// once `connected` arrives encode commands with `send(message:)`.
#[derive(uniffi::Object)]
pub struct JarvisRemoteClient {
    client: Mutex<Option<RemoteClient>>,
}

impl JarvisRemoteClient {
    fn wrap(client: RemoteClient) -> Arc<Self> {
        Arc::new(JarvisRemoteClient {
            client: Mutex::new(Some(client)),
        })
    }

    fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&mut RemoteClient) -> Result<T, JarvisError>,
    ) -> Result<T, JarvisError> {
        with_slot(&self.client, what, f)
    }
}

#[uniffi::export]
impl JarvisRemoteClient {
    /// First contact with the code (typed or scanned) shown on the phone. `deviceName` is
    /// what the phone will list. Throws `remoteInvalidCode`, `remoteInvalidName` or
    /// `remoteEntropy`. Drop the code as soon as this returns.
    #[uniffi::constructor]
    pub fn pair(
        code: String,
        device_name: String,
        entropy: Vec<u8>,
    ) -> Result<Arc<Self>, JarvisError> {
        let code = Zeroizing::new(code);
        let entropy = Zeroizing::new(entropy);
        guarded("JarvisRemoteClient.pair", || {
            RemoteClient::pair(&code, &device_name, &entropy)
                .map(Self::wrap)
                .map_err(client_error)
        })
    }

    /// Reconnects with the credentials JSON from `ClientEvent.paired` (read from the
    /// Keychain). Throws `remoteStoreInvalid` or `remoteEntropy`.
    #[uniffi::constructor]
    pub fn connect(credentials_json: String, entropy: Vec<u8>) -> Result<Arc<Self>, JarvisError> {
        let json = Zeroizing::new(credentials_json);
        let entropy = Zeroizing::new(entropy);
        guarded("JarvisRemoteClient.connect", || {
            let credentials = r::ClientCredentials::from_json(&json).map_err(persist_error)?;
            RemoteClient::connect(credentials, &entropy)
                .map(Self::wrap)
                .map_err(client_error)
        })
    }

    /// Feeds bytes from the phone. A thrown `remoteAuthFailed` is final: close the
    /// transport.
    pub fn receive(&self, bytes: Vec<u8>) -> Result<Vec<ClientEvent>, JarvisError> {
        self.with("receive", |c| {
            c.receive(&bytes)
                .map(|ev| ev.into_iter().map(Into::into).collect())
                .map_err(client_error)
        })
    }

    /// Encodes and seals a command; write the bytes to the phone. Throws
    /// `remoteNotConnected` before `connected`.
    pub fn send(&self, message: ClientMessage) -> Result<Vec<u8>, JarvisError> {
        self.with("send", |c| c.send(&message.into()).map_err(client_error))
    }

    /// Whether commands can be sent.
    pub fn is_connected(&self) -> Result<bool, JarvisError> {
        self.with("is_connected", |c| Ok(c.is_connected()))
    }

    /// The device id this connection authenticated as (after `paired`/`connected`).
    pub fn device_id(&self) -> Result<Option<String>, JarvisError> {
        self.with("device_id", |c| {
            Ok(c.credentials().map(|cr| cr.device_id.to_hex()))
        })
    }

    /// After a pairing on this connection: the check string ("123 456") the phone shows
    /// beside this device. Show it with "Confirm on the phone that it shows …" and keep it
    /// with the pairing (not secret). `nil` for a connect with stored credentials.
    pub fn pairing_check(&self) -> Result<Option<String>, JarvisError> {
        self.with("pairing_check", |c| {
            Ok(c.pairing_check().map(str::to_owned))
        })
    }

    // ---- Live view over remote control -----------------------------------------------

    /// Like `receive`, plus the Live view stream: feed each `RemoteViewerEvent.delta` /
    /// `.snapshot` to the controller's own `JarvisViewer.applyDelta` in order, and write
    /// every `ClientEvent.send` (a resync request the client made by itself on a gap is one
    /// of them). A thrown `remoteAuthFailed` is final: close the transport.
    pub fn receive_all(&self, bytes: Vec<u8>) -> Result<ClientReceived, JarvisError> {
        self.with("receive_all", |c| {
            let r::Received { events, viewer } = c.receive_all(&bytes).map_err(client_error)?;
            Ok(ClientReceived {
                events: events.into_iter().map(Into::into).collect(),
                viewer: viewer.into_iter().map(Into::into).collect(),
            })
        })
    }

    /// Asks the phone for the Live view (a snapshot, then deltas); write the bytes. Needs
    /// the device's "Watch the screen layout" permission on the phone, else
    /// `RemoteViewerEvent.denied` follows. Throws `remoteNotConnected` before `connected`.
    pub fn start_watching(&self) -> Result<Vec<u8>, JarvisError> {
        self.with("start_watching", |c| {
            c.send_viewer(&r::ViewerRequest::StartWatching)
                .map_err(client_error)
        })
    }

    /// Stops the Live view; write the bytes. Frames still on their way are ignored.
    pub fn stop_watching(&self) -> Result<Vec<u8>, JarvisError> {
        self.with("stop_watching", |c| {
            c.send_viewer(&r::ViewerRequest::StopWatching)
                .map_err(client_error)
        })
    }

    /// The controller's view reported `CoreDeltaApplied.needsResync`: the bytes asking the
    /// phone for a snapshot, or `nil` when not watching or one is already on its way (at
    /// most one request per gap).
    pub fn request_resync(&self) -> Result<Option<Vec<u8>>, JarvisError> {
        self.with("request_resync", |c| {
            c.request_resync().map_err(client_error)
        })
    }

    /// Whether this client follows the Live view stream.
    pub fn is_watching(&self) -> Result<bool, JarvisError> {
        self.with("is_watching", |c| Ok(c.is_watching()))
    }
}

// ---------------------------------------------------------------------------------------
// Free functions.
// ---------------------------------------------------------------------------------------

/// The risk categories no remote can ever approve, whatever its `approve` permission
/// says (payment, purchase, credential, account change, delete, unverified). A remote
/// that was shown such a prompt can still decline it.
#[uniffi::export]
pub fn never_remote_approve() -> Result<Vec<RiskCategory>, JarvisError> {
    guarded("never_remote_approve", || {
        Ok(r::NEVER_REMOTE_APPROVE
            .iter()
            .map(|c| (*c).into())
            .collect())
    })
}

/// Whether `category` may be approved remotely at all (not in `neverRemoteApprove()`).
#[uniffi::export]
pub fn remotely_approvable(category: RiskCategory) -> Result<bool, JarvisError> {
    guarded("remotely_approvable", || {
        Ok(r::remotely_approvable(category.into()))
    })
}

/// What a newly paired device gets: start tasks and see progress; no screen text, no
/// approvals. (`RemotePermissions()` is everything off.)
#[uniffi::export]
pub fn new_device_remote_permissions() -> Result<RemotePermissions, JarvisError> {
    guarded("new_device_remote_permissions", || {
        Ok(r::RemotePermissions::NEW_DEVICE.into())
    })
}

/// The fewest entropy bytes the remote calls accept (32).
#[uniffi::export]
pub fn remote_min_entropy_len() -> u32 {
    u32::try_from(r::MIN_ENTROPY_LEN).unwrap_or(u32::MAX)
}

/// The remote protocol version spoken by this core.
#[uniffi::export]
pub fn remote_protocol_version() -> u8 {
    r::PROTOCOL_VERSION
}
