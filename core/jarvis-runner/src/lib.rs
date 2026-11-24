//! Jarvis's own runner protocol: the loopback channel between the Jarvis app (client) and
//! the Jarvis XCUITest runner process (server) that reads typed UI trees and performs
//! commands on the phone.
//!
//! Sans-IO: the shell moves bytes over its own transport (a TCP connection on 127.0.0.1),
//! supplies fresh CSPRNG entropy for each connection, and carries out what the channels
//! return. Nothing here does I/O, spawns threads, reads a clock or panics on any input.
//!
//! # Threat model
//!
//! * **Loopback is shared.** Every app installed on the phone can connect to a loopback
//!   port, and can bind the runner's port first. So (a) a stranger may send bytes to the
//!   runner, and (b) a stranger may pose as the runner and serve forged "typed" trees to
//!   Jarvis. Both must fail: the typed tree is what the core's safety gate trusts.
//! * **Both sides hold one 32-byte pre-shared key** (provisioned by the shell; see
//!   `docs/JARVIS-RUNNER.md`). Whoever lacks it can connect, but learns nothing and makes
//!   neither side act: the server proves the key before the client sends any request, the
//!   client proves it before the server accepts one, and every later byte is under an AEAD
//!   tag.
//! * **Recordings are useless.** Keys are derived per connection from a nonce of each
//!   side, so a recorded session replayed into a new connection fails at its first MAC.
//!   Inside a connection every frame carries a strict per-direction counter: replayed,
//!   reordered, dropped-then-resumed and cross-direction frames all fail.
//! * **Out of scope.** A compromised phone, a stolen key, and denial of service (a
//!   stranger can hold the one connection slot until the shell's handshake deadline
//!   drops it, or squat the port; the client then reports the runner absent).
//!
//! # Wire format
//!
//! ```text
//! frame        = length (u32 BE, of body) || body          1 <= length <= limit
//! ClientHello  = 0x01 || "JRUN" || version || Nc (32)
//! ServerHello  = 0x02 || "JRUN" || version || Ns (32) || mac_s (32)
//! ClientFinish = 0x03 || mac_c (32)
//! Data         = 0x10 || counter (u64 BE) || ChaCha20-Poly1305(plaintext) || tag (16)
//! ```
//!
//! Key schedule (HKDF-SHA256, HMAC-SHA256, labels are part of the protocol):
//!
//! ```text
//! th    = "JRUN" || version || Nc || Ns
//! prk   = HKDF-Extract(salt = "jarvis-runner v1" || Nc || Ns, ikm = psk)
//! k_mac = Expand(prk, "mac"); k_c2s = Expand(prk, "client-to-server"); k_s2c = Expand(prk, "server-to-client")
//! mac_s = HMAC(k_mac, "server-finished" || th);   mac_c = HMAC(k_mac, "client-finished" || th)
//! nonce = direction (0x43 client→server, 0x53 server→client) || 0 0 0 || counter (u64 BE)
//! aad   = "JRUN" || version || 0x10 || counter (u64 BE)
//! ```
//!
//! Before authentication a side accepts only frames of the exact handshake size, so a
//! stranger cannot make it buffer more than a few dozen bytes. The plaintext of a data
//! frame is one message ([`message`]): `kind || request id (u64 BE) || body`, a compact
//! binary encoding of exactly the `PhoneRunner` surface over the `jarvis-proto` types.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod channel;
pub mod crypto;
pub mod error;
pub mod frame;
pub mod message;
mod wire;

pub use channel::{ClientEvent, RunnerClientChannel, RunnerServerChannel, ServerEvent};
pub use error::{MessageError, RunnerError};
pub use message::{LockState, Reply, ReplyKind, Request, Screenshot, Snapshot};

/// Protocol version, carried in both hellos and bound into every MAC and AEAD tag.
pub const PROTOCOL_VERSION: u8 = 1;
/// Length of the pre-shared key, in bytes.
pub const KEY_LEN: usize = 32;
/// Least entropy the shell passes per connection, in bytes.
pub const MIN_ENTROPY_LEN: usize = 32;
/// Largest client→server frame body (requests are small; a long `type_text` fits).
pub const MAX_CLIENT_FRAME: usize = 256 * 1024;
/// Largest server→client frame body (a snapshot with a full-resolution PNG fits).
pub const MAX_SERVER_FRAME: usize = 16 * 1024 * 1024;
/// Largest screenshot carried, in bytes.
pub const MAX_SCREENSHOT_BYTES: usize = 15 * 1024 * 1024;
/// Most nodes in one tree.
pub const MAX_TREE_NODES: usize = 100_000;
/// Deepest tree (the root is depth 1). The runner copies at most 200 levels.
pub const MAX_TREE_DEPTH: usize = 256;
/// Most notes in one snapshot.
pub const MAX_NOTES: usize = 256;
