//! Remote control for Jarvis: pairing, authenticated frames and the hub state machine.
//!
//! The owner starts a task on the phone from another of their devices (a Mac, a browser on
//! the same network), watches it run, stops it, and, only if they switch it on, answers
//! confirmations remotely. The phone stays the authority: this crate decides what a remote
//! may do; the Swift shell only moves bytes over a local-network listener and carries out
//! the [`HubEffect`]s.
//!
//! # Threat model
//!
//! * **The network is hostile.** The transport is plain TCP/WebSocket on a local network.
//!   An attacker can read, drop, replay, reorder and inject bytes, and can open
//!   connections to the listener. Nothing pre-authentication is trusted, and every
//!   authenticated byte is under an AEAD tag.
//! * **The attacker has no paired device key.** Getting one requires the pairing secret,
//!   which is only shown on the phone's screen, is 160 bits, expires after 5 minutes and
//!   is single use.
//! * **A paired device may be lost.** Revocation ([`LocalEvent::DeviceRevoked`]) forgets
//!   its key and closes its connections in the same call; if it started the running task,
//!   that task is cancelled.
//! * **Remote control is never a way around the safety policy.** A remote can only ask
//!   for what the phone's agent would do anyway; every action still goes through the
//!   session's policy. A remote can only do what its [`RemotePermissions`] allow, and by
//!   default it cannot approve anything. Categories in [`NEVER_REMOTE_APPROVE`] (payment,
//!   purchase, credential, account change, delete) can never be approved remotely,
//!   whatever the toggle says.
//! * **Out of scope.** A compromised phone or a compromised paired device that is not yet
//!   revoked (it can do what its permissions allow until then); traffic analysis (an
//!   observer sees connection timing and frame sizes); denial of service (an attacker on
//!   the network can drop packets anyway, so the handshake lockout, which an attacker can
//!   trigger, adds no new capability); a shoulder-surfed pairing code used within its
//!   5 minutes; forward secrecy (see below).
//!
//! # Protocol (version 1)
//!
//! ```text
//! records    len u32 BE | version u8 | kind u8 | payload        (len ≤ MAX_RECORD_LEN)
//! open       phone → ServerHello { Np }                         Np: 32 bytes from the shell's CSPRNG
//! pair       remote → PairRequest { Nc, name, HMAC(pair_key, "remote"‖v‖Np‖Nc‖name) }
//!            phone checks the open offer (unexpired, unused) and the MAC in constant time
//!            phone → PairAccept { device_id, HMAC(pair_key, "phone"‖v‖Np‖Nc‖name‖device_id) }
//!            both: device_key = HKDF(Np‖Nc, S, "device-key"‖device_id‖name); offer consumed
//! connect    remote → ConnectRequest { device_id, Nc, HMAC(auth_key, "remote"‖v‖id‖Np‖Nc) }
//!            phone → ConnectAccept { HMAC(auth_key, "phone"‖v‖id‖Np‖Nc) }
//! keys       r2p ‖ p2r = HKDF(Np‖Nc, device_key, "session"‖device_id)   (fresh per connection)
//! data       counter u64 | ChaCha20-Poly1305(key_dir, nonce = dir‖0‖counter,
//!                                           aad = version‖kind‖device_id‖counter, JSON)
//! counters   start at 0 per direction; exactly the next value is accepted (no gaps:
//!            the stream is ordered, so a gap means tampering); any failure closes
//! failures   wrong code, expired/used offer, unknown device, bad MAC, bit flip, replay,
//!            reorder: all the same opaque close (CloseReason::AuthFailed)
//! lockout    ≥ 5 failed handshakes → none accepted for 5 s, doubling, max 10 min;
//!            forgotten after 15 quiet minutes; reset by a successful handshake
//! messages   JSON tagged with "type": ClientMessage (remote → phone), ServerMessage back
//! ```
//!
//! See [`crypto`] for the exact key schedule. Because both nonces feed the session keys,
//! a recorded session cannot be replayed into a new connection, and a frame from one
//! connection cannot be injected into another.
//!
//! # Rules the hub enforces
//!
//! * One task at a time: `StartTask`/`RunSkill`/`RunWorkflow` while busy is
//!   `Denied { busy }`, never queued. Skill and workflow ids must be in the catalog the
//!   shell supplied.
//! * A `Decision` is honoured only from a device with `approve`, only for the currently
//!   pending request id, only once, and only for a category not in
//!   [`NEVER_REMOTE_APPROVE`]. [`HubEffect::Decide`] names the deciding device.
//! * `Cancel` is allowed for every paired device: stopping is always safe.
//! * Limits: [`MAX_DEVICES`] paired devices, [`MAX_CONNECTIONS`] connections,
//!   [`MAX_BUFFERED_BYTES`] buffered per connection, [`MAX_PLAINTEXT_LEN`] per message,
//!   [`RATE_MAX_MESSAGES`] per [`RATE_WINDOW_MS`] per connection, a
//!   [`HANDSHAKE_TIMEOUT_MS`] and an [`IDLE_TIMEOUT_MS`].
//! * Every refused request yields a [`HubEffect::Audit`] entry (rate-limited messages:
//!   one entry per window).
//!
//! # What remotes see
//!
//! Progress is projected, not forwarded: step, phase, foreground app and the *kind* of the
//! last action (`"type"`, never the typed text). Free text from the phone (screen text,
//! the confirmation summary, the finish summary) is sent only to devices with
//! `see_screen_text` (the confirmation summary also to a device that may decide it) and
//! always passes through `jarvis_safety::redact` plus control/bidi stripping first;
//! others get a generic line. The shell must build screen text from
//! `jarvis_safety::redact_screen` output, which empties secure fields. Using the safety
//! crate's redaction here is deliberate: it is pure, deterministic, already reviewed for
//! the prompt path, and a second pass costs nothing; the crate stays independent of the
//! agent and skills crates.
//!
//! # Limits of this design: read before trusting it
//!
//! This protocol was assembled from standard primitives (HKDF-SHA256, HMAC-SHA256,
//! ChaCha20-Poly1305) by an automated agent. It has **not** been reviewed by a
//! cryptographer or audited. A human security review should look first at:
//!
//! 1. **No forward secrecy.** Session keys come from the long-term device key and public
//!    nonces. Whoever later steals a device key (from the Keychain or the lost device)
//!    and has recorded traffic can decrypt those past sessions. An ephemeral X25519
//!    exchange would fix this; it was left out to keep to the primitives listed above.
//! 2. **Pairing relies on secret entropy alone.** A recorded pairing handshake lets an
//!    attacker test guesses of the secret offline; 160 bits makes that infeasible, but
//!    only while the code stays long and random. Never shorten it.
//! 3. **The phone is not authenticated before the remote proves itself.** During pairing
//!    the remote sends its MAC first; a fake phone learns nothing usable (it cannot test
//!    guesses faster than offline anyway), but check this reasoning.
//! 4. **The key schedule and transcripts** in [`crypto`] (labels, length-prefixing,
//!    what is bound where), and the nonce construction (direction byte + counter under
//!    per-direction keys).
//! 5. **Entropy is the shell's job.** A weak or repeated `entropy` argument breaks
//!    everything; the shell must use the OS CSPRNG for every call.
//! 6. **The lockout is global**, so a network attacker can keep the owner's devices from
//!    connecting (accepted as equivalent to dropping packets).
//! 7. **Key storage.** [`RemoteHub::to_json`] contains every device key in hex; the shell
//!    must keep it in the Keychain only.
//!
//! # Judgement calls
//!
//! Decisions the brief left open, each worth a second opinion:
//!
//! * The pairing secret is 160 bits, typed as 32 base32 characters in groups of four;
//!   `0`/`1`/`8` are read as `O`/`I`/`B`. There is no checksum, so a typo is a failed
//!   handshake (and counts towards the lockout).
//! * The device id is derived from the secret and both nonces rather than drawn from extra
//!   entropy; a collision with an existing device is refused.
//! * Records are length-prefixed on a byte stream (works over TCP and over WebSocket
//!   messages alike). Counters allow no gaps; any failure is final for the connection.
//! * The handshake lockout is global and counts pairing and connect failures alike;
//!   attempts during a lockout are refused unchecked and not counted.
//! * Only the five categories in [`NEVER_REMOTE_APPROVE`] are excluded, as briefed. Notably
//!   `Unverified` (a control the classifier could not read, which might be a foreign
//!   "Pay") and `FirstUse` stay remotely approvable with `approve` on; adding them to the
//!   list is the conservative alternative.
//! * A `Decision` needs `approve` whether it approves or declines; a remote without it can
//!   still `Cancel`. `Cancel` needs no permission at all.
//! * Revoking the device that started the running task also cancels that task.
//! * Events reach subscribed connections, plus `TaskStarted`/`Finished` the device that
//!   started the task and `ConfirmPending`/`ConfirmResolved` devices that may decide.
//! * Unknown message types get `Error { unknown_message }` and keep the connection: the
//!   frame was authentic, and a newer remote should degrade, not disconnect.
//! * Rate limiting drops excess messages with one `Error` and one audit entry per window
//!   rather than closing.
//! * The `Skills` reply drops entries from the end until it fits one frame.
//! * A reused connection id (a shell bug) closes the old connection and refuses the new.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod client;
pub mod codec;
pub mod crypto;
pub mod error;
pub mod hub;
pub mod message;
pub mod pairing;
pub mod permissions;
pub mod store;

pub use client::{ClientCredentials, ClientEvent, RemoteClient};
pub use codec::{
    DeviceId, MAX_BUFFERED_BYTES, MAX_PLAINTEXT_LEN, MAX_RECORD_LEN, PROTOCOL_VERSION,
};
pub use crypto::{PairingSecret, Secret32};
pub use error::{ClientError, CodeError, HubError, MessageError, PersistError};
pub use hub::{
    AuditEntry, AuditEvent, CloseReason, ConnId, HubEffect, LocalEvent, RemoteHub,
    FAILURE_MEMORY_MS, HANDSHAKE_TIMEOUT_MS, IDLE_TIMEOUT_MS, LOCKOUT_BASE_MS, LOCKOUT_MAX_MS,
    LOCKOUT_THRESHOLD, MAX_CATALOG_ENTRIES, MAX_CONNECTIONS, MAX_ID_CHARS, MAX_SCREEN_TEXT_CHARS,
    MAX_SUMMARY_CHARS, MAX_TASK_TEXT_CHARS, RATE_MAX_MESSAGES, RATE_WINDOW_MS,
};
pub use message::{CatalogEntry, ClientMessage, DenyReason, ErrorCode, ServerMessage};
pub use pairing::{format_code, parse_code, qr_payload, PairingOffer, PAIRING_TTL_MS, QR_PREFIX};
pub use permissions::{remotely_decidable, RemotePermissions, NEVER_REMOTE_APPROVE};
pub use store::{
    clean_name, DeviceInfo, DeviceRecord, DeviceStore, DEVICE_STORE_VERSION, MAX_DEVICES,
    MAX_DEVICE_NAME_BYTES,
};
