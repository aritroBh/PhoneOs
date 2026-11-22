//! Error types.

/// A pairing code could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CodeError {
    /// A character outside the base32 alphabet (after the usual misreadings).
    #[error("the code contains characters that are not part of a pairing code")]
    InvalidCharacters,
    /// The code does not decode to exactly 160 bits.
    #[error("the code has the wrong length")]
    WrongLength,
}

/// A JSON message inside an authenticated frame could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MessageError {
    /// Not valid UTF-8 JSON, not an object, no string `"type"`, or the fields do not fit
    /// the type.
    #[error("malformed message: {0}")]
    Malformed(String),
    /// A well-formed message whose `"type"` this version does not know.
    #[error("unknown message type {0:?}")]
    UnknownType(String),
    /// Larger than [`crate::MAX_PLAINTEXT_LEN`] once encoded.
    #[error("message too large")]
    TooLarge,
}

/// The persisted device list could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PersistError {
    /// Not valid JSON for the schema.
    #[error("corrupt device store: {0}")]
    Corrupt(String),
    /// Written by a newer (or unknown) schema version.
    #[error("unsupported device store version {0}")]
    UnsupportedVersion(u32),
    /// Valid JSON with invalid content (duplicate ids, too many devices, bad key).
    #[error("invalid device store: {0}")]
    Invalid(String),
}

/// A hub operation the shell asked for could not be done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HubError {
    /// Already [`crate::MAX_DEVICES`] paired devices; revoke one first.
    #[error("too many paired devices")]
    TooManyDevices,
    /// No such paired device.
    #[error("unknown device")]
    UnknownDevice,
    /// The key schedule failed (not expected to happen).
    #[error("internal cryptographic error")]
    Crypto,
}

/// What went wrong on the remote's side of a connection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    /// The pairing code is not a valid code.
    #[error(transparent)]
    Code(#[from] CodeError),
    /// The device name is empty or longer than [`crate::MAX_DEVICE_NAME_BYTES`].
    #[error("invalid device name")]
    InvalidName,
    /// The phone could not be authenticated, a frame failed authentication, or the phone
    /// sent something out of order. Deliberately opaque. The connection is unusable.
    #[error("authentication failed")]
    Auth,
    /// `send` was called before the connection was established, or after it failed.
    #[error("not connected")]
    NotConnected,
    /// An authenticated message could not be decoded.
    #[error(transparent)]
    Message(#[from] MessageError),
}
