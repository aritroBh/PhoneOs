//! Remote control for Jarvis: pairing, authenticated frames and the hub state machine.
//!
//! The owner starts a task on the phone from another of their devices (a Mac, a browser on
//! the same network), watches it run, stops it, and, only if they switch it on, approves
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
//!   its key and closes its connections in the same call, and cancels the tasks *that
//!   device* requested (never one the owner started on the phone).
//! * **Remote control is never a way around the safety policy.** A remote can only ask
//!   for what the phone's agent would do anyway; every action still goes through the
//!   session's policy. A remote can only do what its [`RemotePermissions`] allow, and a
//!   new device cannot approve anything. Categories in [`NEVER_REMOTE_APPROVE`] (payment,
//!   purchase, credential, account change, delete, unverified) can never be approved
//!   remotely, whatever the toggle says.
//! * **The shell may misuse entropy.** Mistakes are contained rather than fatal; see
//!   [`hub`] ("Entropy misuse").
//! * **Out of scope.** A compromised phone or a compromised paired device that is not yet
//!   revoked (it can do what its permissions allow until then); traffic analysis (an
//!   observer sees connection timing and frame sizes); denial of service (an attacker on
//!   the network can drop packets, fill the connection slots with half-open connections,
//!   or lock out pairing and a known device id with wrong MACs; none of that adds a
//!   capability beyond dropping packets). A shoulder-surfed pairing code used within its
//!   5 minutes pairs a device, but that device holds nothing until the owner confirms it
//!   on the phone after comparing its check string (see the protocol below).
//!
//! # Protocol (version 2)
//!
//! ```text
//! records    len u32 BE | version u8 | kind u8 | payload        (len ≤ MAX_RECORD_LEN)
//! open       phone → ServerHello { Np, Ep [, C] }    derived from entropy + counter + clock;
//!            C = SHA-256(lp(label, confirm)), only while a pairing offer is open
//! pair       remote → PairRequest { Nc, Er, name, HMAC(pair_key, "remote"‖v‖Np‖Nc‖Ep‖Er‖name) }
//!            phone checks the open offer (unexpired, unused) and the MAC in constant time
//!            phone → PairAccept { device_id, HMAC(pair_key, "phone"‖…‖name‖device_id), confirm }
//!            both: dh = X25519; device_key = HKDF(Np‖Nc, S‖dh, "device-key"‖device_id‖name)
//!            remote checks H(confirm) = C and device_id = HKDF(Np‖Nc, S, "device-id")
//!            both: check = HKDF(Np‖Nc, device_key‖confirm, "confirm-check"‖id‖Ep‖Er‖C) mod 10^6
//!            the device is kept, PENDING, once its first authenticated frame arrives
//!            (client: Hello → AwaitingConfirmation); only the owner's confirm_device on the
//!            phone gives it the new-device permissions
//! connect    remote → ConnectRequest { device_id, Nc, Er, HMAC(auth_key, "remote"‖v‖id‖Np‖Nc‖Ep‖Er) }
//!            phone refuses a recently seen (device_id, Nc); phone → ConnectAccept { HMAC(…"phone"…) }
//! keys       r2p ‖ p2r = HKDF(Np‖Nc, device_key‖dh, "session"‖device_id‖Ep‖Er)  (forward secret)
//! data       counter u64 | ChaCha20-Poly1305(key_dir, nonce = dir‖0‖counter,
//!                                           aad = version‖kind‖device_id‖counter, JSON)
//! counters   start at 0 per direction; exactly the next value is accepted (no gaps:
//!            the stream is ordered, so a gap means tampering); any failure closes
//! failures   wrong code, expired/used offer, unknown device, bad MAC, bit flip, replay,
//!            reorder, garbage: the same opaque close (CloseReason::AuthFailed)
//! lockout    ≥ 5 failed MAC checks (pairing: per offer, a new offer starts clean;
//!            connect: per claimed known device) → that handshake refused for 5 s,
//!            doubling, max 10 min (pairing: max 30 s, and at most 60 s in all per offer);
//!            forgotten after 15 quiet minutes; refusals during it are not counted. A
//!            connect that proves the device key is accepted even while that device is
//!            locked (the id travels in clear: the lockout must not be a weapon)
//! messages   JSON tagged with "type": ClientMessage (remote → phone), ServerMessage back
//! ```
//!
//! See [`crypto`] for the exact key schedule. Both nonces and both ephemeral keys feed the
//! session keys, so a recorded session cannot be replayed into a new connection, a frame
//! from one connection cannot be injected into another, and a device key stolen later
//! does not decrypt recorded traffic.
//!
//! # Rules the hub enforces
//!
//! * One task at a time: `StartTask`/`RunSkill`/`RunWorkflow` while busy is
//!   `Denied { busy }`, never queued. Skill and workflow ids must be in the catalog the
//!   shell supplied; skill arguments are strings, cleaned and capped. Every request gets a
//!   hub task id, which the shell echoes in `SessionStarted`/`SessionFinished`; a session
//!   without one is the owner's own.
//! * Approving needs `approve`, the currently pending request id, a category outside
//!   [`NEVER_REMOTE_APPROVE`], and happens once. Declining needs no permission, only that
//!   the device was sent that `ConfirmPending`. [`HubEffect::Decide`] names the deciding
//!   device and the category it checked.
//! * `Cancel` is allowed for every paired device (stopping is always safe) and is exempt
//!   from rate-limit dropping while something runs.
//! * `ListSkills` needs `start_tasks`; a device with every permission off learns from
//!   `Welcome` only the protocol version and its own permissions.
//! * Limits: [`MAX_DEVICES`] paired devices, [`MAX_CONNECTIONS`] connections (a full
//!   table closes its oldest unfinished handshake to admit a new connection; authenticated
//!   connections are never evicted),
//!   [`MAX_BUFFERED_BYTES`] buffered per connection, [`MAX_PLAINTEXT_LEN`] per message,
//!   [`RATE_MAX_MESSAGES`] per [`RATE_WINDOW_MS`] per connection, a
//!   [`HANDSHAKE_TIMEOUT_MS`] and an [`IDLE_TIMEOUT_MS`]. Every time check uses the
//!   hub's own monotonic clock, which a backwards step of the shell's clock neither
//!   rewinds nor stops (see [`hub`]).
//! * Every refused request yields a [`HubEffect::Audit`] entry. Rate-limited messages get
//!   one entry per window; network-driven events before authentication are merged into
//!   one detailed entry plus one [`AuditEvent::PreAuthSuppressed`] count per
//!   [`PREAUTH_AUDIT_WINDOW_MS`], so an attacker cannot grow the log without bound.
//!
//! # What remotes see
//!
//! Progress is projected, not forwarded: step, phase, foreground app, the *kind* of the
//! last action (`"type"`, never the typed text) and whether the task waits for an unlock.
//! Free text from the phone (screen text, the confirmation summary, the finish summary) is
//! sent only to devices with `see_screen_text` (the confirmation summary also to a device
//! that may approve it); others get a generic line. Free text is first normalised to one
//! line (controls, bidi and zero-width characters removed, every whitespace run including
//! tabs, newlines, NBSP and U+2028 made one space), then passed through
//! `jarvis_safety::redact`, then capped, so a secret split by odd whitespace is seen in
//! its canonical form and no cut can hide one. The shell must build screen text from
//! `jarvis_safety::redact_screen` output, which empties secure fields. Reusing the safety
//! crate's redaction is deliberate: it is pure, deterministic, already reviewed for the
//! prompt path, and a second pass costs nothing.
//!
//! # Secrets in memory
//!
//! Keys, the pairing secret, ephemeral X25519 secrets, DH outputs, derived buffers,
//! decoded hex/base32 buffers, decrypted frames, the pairing code and QR strings and the
//! persisted JSON are zeroized on drop (`zeroize`, and `ZeroizeOnDrop` inside
//! chacha20poly1305 and x25519-dalek). What cannot be: serde_json's intermediate buffers
//! and `Value`s while encoding or decoding the store, credentials and messages; copies
//! the shell makes; and anything the allocator moved before it was wrapped.
//!
//! # Limits of this design: read before trusting it
//!
//! This protocol was assembled from standard primitives (HKDF-SHA256, HMAC-SHA256, X25519,
//! ChaCha20-Poly1305) by an automated agent and revised after one adversarial review. It
//! has **not** been reviewed by a cryptographer or audited. A human security review should
//! look first at:
//!
//! 1. **Pairing relies on secret entropy alone** for authentication. A recorded pairing
//!    handshake lets an attacker test guesses of the secret offline; 160 bits makes that
//!    infeasible, but only while the code stays long and random. Never shorten it. An
//!    active attacker who sees the code within its 5 minutes can pair, or sit between the
//!    owner's device and the phone; the confirm step (both sides show a six-digit check
//!    string the owner compares, from a commitment scheme neither side can grind) makes
//!    that visible: a second device is a second pending entry, a man in the middle shows
//!    different digits on each side (1 in 10^6 to match by chance).
//! 2. **The key schedule and transcripts** in [`crypto`] (labels, length-prefixing, what
//!    is bound where, DH mixing), and the nonce construction (direction byte + counter
//!    under per-direction keys).
//! 3. **Entropy is still the shell's job.** The hub contains mistakes (see [`hub`]), but
//!    a predictable CSPRNG would still break pairing secrecy and forward secrecy.
//! 4. **Lockouts** can be triggered by anyone for pairing and for a known device id
//!    (device ids travel in clear): a deliberate availability-for-safety trade.
//! 5. **Key storage.** [`RemoteHub::to_json`] contains every device key in hex; the shell
//!    must keep it in the Keychain only, and persist it on [`HubEffect::StoreChanged`]
//!    (the phone-nonce counter lease depends on it).
//!
//! # Judgement calls
//!
//! Decisions the brief left open, each worth a second opinion:
//!
//! * The pairing secret is 160 bits, typed as exactly 32 base32 characters in groups of
//!   four; `0`/`1`/`8` are read as `O`/`I`/`B`. There is no checksum, so a typo is a
//!   failed MAC check (and counts towards the pairing lockout).
//! * The device id is derived from the secret and both nonces rather than drawn from extra
//!   entropy; a collision with an existing device is refused.
//! * Records are length-prefixed on a byte stream (works over TCP and over WebSocket
//!   messages alike). Counters allow no gaps; any failure is final for the connection.
//! * The entropy sanity check requires 16 distinct byte values in ≥ 32 bytes: it never
//!   rejects real randomness and catches zero/constant/short-pattern buffers, nothing more.
//! * The phone-nonce counter is leased in blocks of [`NONCE_LEASE_BLOCK`] and persisted
//!   via [`HubEffect::StoreChanged`]; up to one block is skipped after a restart.
//! * Screen text is flattened to one line before redaction (safer, less readable).
//! * Revoking a device cancels both its running task and a task it requested that has not
//!   started yet.
//! * Unknown message types get `Error { unknown_message }` and keep the connection: the
//!   frame was authentic, and a newer remote should degrade, not disconnect.
//! * Rate limiting drops excess messages with one `Error` and one audit entry per window
//!   rather than closing. Over-limit skill arguments are refused, an over-long value is
//!   cut.
//! * The hub assigns confirmation ids; the shell reads [`RemoteHub::pending_request_id`]
//!   after `ConfirmRequested` and echoes it in `ConfirmResolved`.
//! * New pairings get [`RemotePermissions::NEW_DEVICE`] (start tasks, see progress) once
//!   the owner confirms them on the phone ([`RemoteHub::confirm_device`]); until then they
//!   hold nothing, and one not confirmed within [`PENDING_CONFIRM_TTL_MS`] is removed.
//!   Everything else defaults to off.
//! * Confirmation request ids are leased in blocks of [`REQUEST_LEASE_BLOCK`] and
//!   persisted, so they never repeat across a hub rebuild (an old card cannot name a new
//!   question).
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
mod text;
pub mod viewer;

pub use client::{ClientCredentials, ClientEvent, Received, RemoteClient, CREDENTIALS_VERSION};
pub use codec::{
    DeviceId, MAX_BUFFERED_BYTES, MAX_PLAINTEXT_LEN, MAX_RECORD_LEN, PROTOCOL_VERSION,
};
pub use crypto::{PairingSecret, Secret32, MIN_ENTROPY_LEN};
pub use error::{ClientError, CodeError, EntropyError, HubError, MessageError, PersistError};
pub use hub::{
    AuditEntry, AuditEvent, CloseReason, ConnId, HubEffect, LocalEvent, PendingDeviceInfo,
    RemoteHub, FAILURE_MEMORY_MS, HANDSHAKE_TIMEOUT_MS, IDLE_TIMEOUT_MS, LOCKOUT_BASE_MS,
    LOCKOUT_MAX_MS, LOCKOUT_THRESHOLD, MAX_CATALOG_ENTRIES, MAX_CONNECTIONS, MAX_ID_CHARS,
    MAX_SCREEN_TEXT_CHARS, MAX_SKILL_ARGS, MAX_SKILL_ARGS_TOTAL_CHARS, MAX_SKILL_ARG_KEY_CHARS,
    MAX_SKILL_ARG_VALUE_CHARS, MAX_SUMMARY_CHARS, MAX_TASK_TEXT_CHARS, NONCE_LEASE_BLOCK,
    PAIR_LOCKOUT_BUDGET_MS, PAIR_LOCKOUT_MAX_MS, PENDING_CONFIRM_TTL_MS, PREAUTH_AUDIT_WINDOW_MS,
    RATE_MAX_MESSAGES, RATE_WINDOW_MS, RECENT_CONNECTS, REQUEST_LEASE_BLOCK,
};
pub use message::{CatalogEntry, ClientMessage, DenyReason, ErrorCode, ServerMessage};
pub use pairing::{
    format_code, parse_code, qr_payload, PairingOffer, CODE_SYMBOLS, PAIRING_TTL_MS, QR_PREFIX,
};
pub use permissions::{remotely_approvable, RemotePermissions, NEVER_REMOTE_APPROVE};
pub use store::{
    clean_name, DeviceInfo, DeviceRecord, DeviceStore, PendingRecord, StoredHub,
    DEVICE_STORE_VERSION, MAX_DEVICES, MAX_DEVICE_NAME_BYTES, MAX_LEASE,
};
pub use viewer::{
    ViewerEndReason, ViewerEvent, ViewerMessage, ViewerRequest, MAX_DELTA_BASE64, MAX_DELTA_BYTES,
    MAX_SNAPSHOT_BYTES, MIN_SCREEN_INTERVAL_MS, VIEWER_QUEUE_BYTES, VIEWER_WINDOW_BYTES,
};
