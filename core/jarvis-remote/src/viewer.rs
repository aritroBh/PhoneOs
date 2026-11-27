//! The Live view over remote control: the messages that stream the phone's Live view
//! timeline to a paired controller as deltas ([`jarvis_viewer::delta`]), wireframe only.
//!
//! # Messages
//!
//! They travel in the same authenticated frames, JSON tagged with `"type"`, as their own
//! families ([`ViewerRequest`] remote → phone, [`ViewerMessage`] phone → remote) beside
//! [`crate::ClientMessage`] and [`crate::ServerMessage`]. A phone without this feature
//! answers a `ViewerRequest` with `Error { unknown_message }`.
//!
//! ```text
//! remote → phone   {"type":"start_watching"}  {"type":"stop_watching"}  {"type":"viewer_resync"}
//! phone → remote   {"type":"viewer_delta","data":"<base64 delta>"}
//!                  {"type":"viewer_snapshot","data":"<base64 delta>","part":0,"parts":3}
//!                  {"type":"viewer_ended","reason":"stopped"|"permission_revoked"}
//!                  {"type":"viewer_denied","reason":"not_permitted"}
//! ```
//!
//! `data` is one delta's bytes (at most [`MAX_DELTA_BYTES`]) in standard base64; the
//! length is checked before sealing on the phone and before decoding on the remote. A
//! snapshot is a `reset` delta, step chunks and the current screen, each its own
//! `viewer_snapshot` message (`part` of `parts`).
//!
//! # Rules the hub enforces
//!
//! * Watching needs the device's `watch_screen` permission ([`crate::RemoteHub::watch_screen`], set with [`crate::RemoteHub::set_watch_screen`],
//!   off by default, independent of `see_progress`). `start_watching` without it is
//!   answered `viewer_denied { not_permitted }` and audited as `Denied`; starting and
//!   stopping are audited as `Accepted` (`start_watching`, `stop_watching`).
//! * Turning the permission off stops every stream of that device in the same call
//!   (`viewer_ended { permission_revoked }`, audited).
//! * Deltas go only to authenticated connections that asked to watch and hold the
//!   permission. A stream starts with a snapshot; `viewer_resync` asks for another.
//! * Backpressure: per connection the hub keeps at most [`VIEWER_QUEUE_BYTES`] of deltas
//!   not yet handed to the shell, and hands out more only while fewer than
//!   [`VIEWER_WINDOW_BYTES`] it sent are unconfirmed ([`crate::RemoteHub::written`]). On
//!   overflow the queue is dropped and the connection gets one snapshot once there is room
//!   (coalesced): never blocking the run, never growing.
//! * At most one screen per [`MIN_SCREEN_INTERVAL_MS`] per connection; a newer screen
//!   replaces one still waiting (screens are side deltas, so this opens no gap; the
//!   controller shows the replaced one as "not available").
//! * Nothing in the stream changes what may be approved remotely.

use serde::{Deserialize, Serialize};

use crate::codec::MAX_PLAINTEXT_LEN;
use crate::error::MessageError;
use crate::message::{decode as decode_typed, encode as encode_typed, DenyReason};

pub use jarvis_viewer::delta::{MAX_DELTA_BYTES, MAX_SNAPSHOT_BYTES};

/// Unconfirmed viewer bytes per connection before the hub holds deltas back.
pub const VIEWER_WINDOW_BYTES: usize = 128 * 1024;
/// Deltas the hub queues per connection; beyond it the queue is dropped for a snapshot.
pub const VIEWER_QUEUE_BYTES: usize = 256 * 1024;
/// At most one screen per this many milliseconds per connection.
pub const MIN_SCREEN_INTERVAL_MS: u64 = 250;
/// Longest `data` field: base64 of [`MAX_DELTA_BYTES`].
pub const MAX_DELTA_BASE64: usize = MAX_DELTA_BYTES.div_ceil(3) * 4;

// A delta in its envelope always fits one frame (the envelope is under 100 bytes).
const _: () = assert!(MAX_DELTA_BASE64 + 128 <= MAX_PLAINTEXT_LEN);

/// Remote → phone: watching the Live view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ViewerRequest {
    /// Start receiving the Live view (a snapshot, then deltas). Needs `watch_screen`.
    StartWatching,
    /// Stop receiving it.
    StopWatching,
    /// The stream broke (a gap): send a snapshot.
    ViewerResync,
}

impl ViewerRequest {
    /// Every `"type"` tag.
    pub const TYPES: [&'static str; 3] = ["start_watching", "stop_watching", "viewer_resync"];

    /// The `"type"` tag.
    pub fn name(&self) -> &'static str {
        match self {
            ViewerRequest::StartWatching => "start_watching",
            ViewerRequest::StopWatching => "stop_watching",
            ViewerRequest::ViewerResync => "viewer_resync",
        }
    }

    /// Encodes to JSON bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        encode_typed(self)
    }

    /// Decodes JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        decode_typed(bytes, &Self::TYPES)
    }
}

/// Why the phone ended a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerEndReason {
    /// The remote asked to stop.
    Stopped,
    /// The owner turned `watch_screen` off for this device.
    PermissionRevoked,
    /// A reason from a newer phone.
    #[serde(other)]
    Other,
}

/// Phone → remote: the Live view stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ViewerMessage {
    /// One delta (base64).
    ViewerDelta {
        /// The delta's bytes, base64.
        data: String,
    },
    /// One part of a snapshot (base64); part 0 is the `reset`.
    ViewerSnapshot {
        /// The delta's bytes, base64.
        data: String,
        /// Its index.
        part: u32,
        /// How many parts the snapshot has.
        parts: u32,
    },
    /// The stream ended.
    ViewerEnded {
        /// Why.
        reason: ViewerEndReason,
    },
    /// `start_watching` (or `viewer_resync`) was refused.
    ViewerDenied {
        /// Why.
        reason: DenyReason,
    },
}

impl ViewerMessage {
    /// Every `"type"` tag.
    pub const TYPES: [&'static str; 4] = [
        "viewer_delta",
        "viewer_snapshot",
        "viewer_ended",
        "viewer_denied",
    ];

    /// A delta message; `TooLarge` over [`MAX_DELTA_BYTES`].
    pub fn delta(bytes: &[u8]) -> Result<Self, MessageError> {
        if bytes.len() > MAX_DELTA_BYTES {
            return Err(MessageError::TooLarge);
        }
        Ok(ViewerMessage::ViewerDelta {
            data: base64_encode(bytes),
        })
    }

    /// A snapshot part; `TooLarge` over [`MAX_DELTA_BYTES`].
    pub fn snapshot(bytes: &[u8], part: u32, parts: u32) -> Result<Self, MessageError> {
        if bytes.len() > MAX_DELTA_BYTES {
            return Err(MessageError::TooLarge);
        }
        Ok(ViewerMessage::ViewerSnapshot {
            data: base64_encode(bytes),
            part,
            parts,
        })
    }

    /// The delta bytes of a `viewer_delta` or `viewer_snapshot`, length-checked:
    /// `None` for other messages, `Some(Err)` for an oversized or invalid payload.
    pub fn payload(&self) -> Option<Result<Vec<u8>, MessageError>> {
        let data = match self {
            ViewerMessage::ViewerDelta { data } | ViewerMessage::ViewerSnapshot { data, .. } => {
                data
            }
            _ => return None,
        };
        if data.len() > MAX_DELTA_BASE64 {
            return Some(Err(MessageError::TooLarge));
        }
        Some(
            base64_decode(data)
                .filter(|b| b.len() <= MAX_DELTA_BYTES)
                .ok_or_else(|| MessageError::Malformed("bad delta payload".to_owned())),
        )
    }

    /// Encodes to JSON bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        encode_typed(self)
    }

    /// Decodes JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        decode_typed(bytes, &Self::TYPES)
    }
}

/// What the remote's viewer must act on ([`crate::RemoteClient::receive_all`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewerEvent {
    /// A delta, in order: apply it (`jarvis_viewer::LiveView::apply_delta`).
    Delta {
        /// The delta's bytes (at most [`MAX_DELTA_BYTES`]).
        bytes: Vec<u8>,
    },
    /// A snapshot part, in order: apply it like a delta (part 0 resets the view).
    Snapshot {
        /// The delta's bytes.
        bytes: Vec<u8>,
        /// Its index.
        part: u32,
        /// How many parts.
        parts: u32,
    },
    /// The stream broke; the client asked the phone for a snapshot (the request is a
    /// `ClientEvent::Send` among the events). Reported once per gap.
    ResyncRequested,
    /// The phone ended the stream.
    Ended {
        /// Why.
        reason: ViewerEndReason,
    },
    /// The phone refused to stream (e.g. `watch_screen` is off).
    Denied {
        /// Why.
        reason: DenyReason,
    },
    /// The phone streams a newer, incompatible format (its version); watching stopped.
    Incompatible {
        /// The delta format version.
        version: u32,
    },
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk.first().copied().unwrap_or(0);
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        let sym = |shift: u32| {
            char::from(
                B64.get(((n >> shift) & 63) as usize)
                    .copied()
                    .unwrap_or(b'A'),
            )
        };
        out.push(sym(18));
        out.push(sym(12));
        out.push(if chunk.len() > 1 { sym(6) } else { '=' });
        out.push(if chunk.len() > 2 { sym(0) } else { '=' });
    }
    out
}

fn b64_value(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Strict standard base64 with padding; `None` for anything else.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let quads = bytes.len() / 4;
    for (i, q) in bytes.chunks(4).enumerate() {
        let last = i + 1 == quads;
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return None;
        }
        let mut n = 0u32;
        for (j, &c) in q.iter().enumerate() {
            let v = if j >= 4 - pad { 0 } else { b64_value(c)? };
            n = (n << 6) | v;
        }
        let [_, b0, b1, b2] = n.to_be_bytes();
        out.push(b0);
        if pad < 2 {
            out.push(b1);
        }
        if pad < 1 {
            out.push(b2);
        }
        // Non-canonical padding bits are refused.
        if (pad == 1 && b2 != 0) || (pad == 2 && (b1 != 0 || b2 != 0)) {
            return None;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_and_is_strict() {
        for n in 0..70usize {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            let s = base64_encode(&data);
            assert_eq!(base64_decode(&s).unwrap(), data);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_decode("TWE="), Some(b"Ma".to_vec()));
        assert_eq!(base64_decode("TWF="), None, "non-canonical bits");
        assert_eq!(base64_decode("TW=E"), None);
        assert_eq!(base64_decode("TWE"), None);
        assert_eq!(base64_decode("T===").map(|_| ()), None);
        assert_eq!(base64_decode("TWE=TWE="), None, "padding only at the end");
        assert_eq!(base64_decode("TW\u{e9}u"), None);
    }

    #[test]
    fn message_shapes_and_bounds() {
        let m = ViewerMessage::delta(b"{}").unwrap();
        let b = m.to_bytes().unwrap();
        assert_eq!(
            std::str::from_utf8(&b).unwrap(),
            r#"{"type":"viewer_delta","data":"e30="}"#
        );
        assert_eq!(ViewerMessage::from_bytes(&b).unwrap(), m);
        assert_eq!(m.payload(), Some(Ok(b"{}".to_vec())));
        assert_eq!(
            ViewerMessage::delta(&vec![0; MAX_DELTA_BYTES + 1]),
            Err(MessageError::TooLarge)
        );
        // The largest delta still fits one frame.
        let full =
            ViewerMessage::snapshot(&vec![0xFF; MAX_DELTA_BYTES], u32::MAX, u32::MAX).unwrap();
        assert!(full.to_bytes().unwrap().len() <= MAX_PLAINTEXT_LEN);
        let over = ViewerMessage::ViewerDelta {
            data: "A".repeat(MAX_DELTA_BASE64 + 4),
        };
        assert_eq!(over.payload(), Some(Err(MessageError::TooLarge)));
        assert_eq!(
            ViewerRequest::from_bytes(br#"{"type":"start_watching"}"#).unwrap(),
            ViewerRequest::StartWatching
        );
        assert!(ViewerRequest::from_bytes(br#"{"type":"start_watching","x":1}"#).is_err());
        let ended: ViewerMessage =
            ViewerMessage::from_bytes(br#"{"type":"viewer_ended","reason":"solar_flare"}"#)
                .unwrap();
        assert_eq!(
            ended,
            ViewerMessage::ViewerEnded {
                reason: ViewerEndReason::Other
            }
        );
    }
}
