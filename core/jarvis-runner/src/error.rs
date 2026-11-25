//! Errors of the runner channel.

/// Why a message could not be encoded or decoded. Carries no input bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MessageError {
    /// The bytes end before the message does.
    #[error("truncated")]
    Truncated,
    /// Bytes remain after the message.
    #[error("trailing bytes")]
    Trailing,
    /// An unknown kind, tag or enum value.
    #[error("unknown {0}")]
    Unknown(&'static str),
    /// A string is not UTF-8.
    #[error("invalid UTF-8")]
    Utf8,
    /// A number is NaN or infinite.
    #[error("non-finite number")]
    NonFinite,
    /// A limit of the protocol was exceeded (named).
    #[error("{0} over the limit")]
    Limit(&'static str),
}

/// A failure of a channel. Every variant except [`RunnerError::TooLarge`],
/// [`RunnerError::State`] and [`RunnerError::Entropy`]/[`RunnerError::Key`] (refused before
/// anything was sent) closes the channel: later calls return [`RunnerError::Closed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RunnerError {
    /// The shell's entropy was refused (too short, or obviously not random).
    #[error("entropy refused")]
    Entropy,
    /// The key is not [`crate::KEY_LEN`] bytes.
    #[error("the key must be 32 bytes")]
    Key,
    /// The peer could not prove the key, or a frame failed authentication (a wrong key,
    /// tampering, a replayed, reordered or cross-direction frame). Deliberately carries no
    /// detail.
    #[error("authentication failed")]
    Auth,
    /// A frame or handshake message is malformed (length, kind, magic, version).
    #[error("malformed frame: {0}")]
    Frame(&'static str),
    /// An authenticated message could not be decoded, or breaks the request/reply order.
    #[error("protocol error: {0}")]
    Protocol(&'static str),
    /// The outgoing message is too large or not encodable; nothing was sent and the
    /// channel stays open.
    #[error("message not sent: {0}")]
    TooLarge(MessageError),
    /// The call is not valid now (not authenticated yet, a request already outstanding);
    /// nothing was sent and the channel stays open.
    #[error("not now: {0}")]
    State(&'static str),
    /// A request arrived after its `not_after_ms` (held back, or the client already gave
    /// up on it). Nothing was carried out.
    #[error("request expired")]
    Expired,
    /// The channel failed earlier and is unusable.
    #[error("channel closed")]
    Closed,
}
