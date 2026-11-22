//! Wire records, identifiers and text encodings (hex, grouped base32).
//!
//! The transport is an ordered byte stream (TCP, or WebSocket messages concatenated). It
//! carries records:
//!
//! ```text
//! record  = length (u32 BE, of what follows) || version (u8) || kind (u8) || payload
//! ```
//!
//! `length` is at least 2 and at most [`MAX_RECORD_LEN`]; a larger value is rejected as
//! soon as the four length bytes arrive, before anything is buffered.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::crypto::{COUNTER_LEN, TAG_LEN};

/// Protocol version carried in every record and bound into every MAC and AEAD tag.
pub const PROTOCOL_VERSION: u8 = 1;
/// Largest JSON message inside one data record, in bytes.
pub const MAX_PLAINTEXT_LEN: usize = 64 * 1024;
/// Largest record body (version, kind, payload), in bytes.
pub const MAX_RECORD_LEN: usize = 2 + COUNTER_LEN + MAX_PLAINTEXT_LEN + TAG_LEN;
/// Largest number of not-yet-complete bytes buffered per connection.
pub const MAX_BUFFERED_BYTES: usize = 4 + MAX_RECORD_LEN;

/// Record kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// Phone → remote, plaintext: `phone_nonce (32)`.
    ServerHello = 0x01,
    /// Remote → phone, plaintext: `remote_nonce (32) || name_len (u8) || name || mac (32)`.
    PairRequest = 0x02,
    /// Phone → remote, plaintext: `device_id (16) || mac (32)`.
    PairAccept = 0x03,
    /// Remote → phone, plaintext: `device_id (16) || remote_nonce (32) || mac (32)`.
    ConnectRequest = 0x04,
    /// Phone → remote, plaintext: `mac (32)`.
    ConnectAccept = 0x05,
    /// Either direction, authenticated: `counter (u64 BE) || ciphertext || tag (16)`.
    Data = 0x10,
}

impl Kind {
    /// Parses a kind byte.
    pub fn from_byte(b: u8) -> Option<Kind> {
        match b {
            0x01 => Some(Kind::ServerHello),
            0x02 => Some(Kind::PairRequest),
            0x03 => Some(Kind::PairAccept),
            0x04 => Some(Kind::ConnectRequest),
            0x05 => Some(Kind::ConnectAccept),
            0x10 => Some(Kind::Data),
            _ => None,
        }
    }
}

/// A complete record taken off the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Protocol version byte.
    pub version: u8,
    /// Kind byte (not yet validated).
    pub kind: u8,
    /// Payload after version and kind.
    pub payload: Vec<u8>,
}

/// Why the record layer refused input. The hub reports all of them to the peer the same
/// way (it closes), and they never carry secret-dependent detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecordError {
    /// The length prefix is larger than [`MAX_RECORD_LEN`].
    #[error("record too large")]
    TooLarge,
    /// The length prefix is too small to hold version and kind.
    #[error("malformed record")]
    Malformed,
    /// More than [`MAX_BUFFERED_BYTES`] would be buffered.
    #[error("receive buffer full")]
    BufferFull,
}

/// Encodes one record.
pub fn encode_record(kind: Kind, payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(2 + payload.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(6 + payload.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.push(PROTOCOL_VERSION);
    out.push(kind as u8);
    out.extend_from_slice(payload);
    out
}

/// Reassembles records from a byte stream, with a bounded buffer.
#[derive(Debug, Default, Clone)]
pub struct RecordReader {
    buf: Vec<u8>,
}

impl RecordReader {
    /// An empty reader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bytes currently buffered.
    pub fn buffered(&self) -> usize {
        self.buf.len()
    }

    /// Room left before [`RecordError::BufferFull`].
    pub fn room(&self) -> usize {
        MAX_BUFFERED_BYTES.saturating_sub(self.buf.len())
    }

    /// Appends bytes; fails without appending when they do not fit.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), RecordError> {
        if bytes.len() > self.room() {
            return Err(RecordError::BufferFull);
        }
        self.buf.extend_from_slice(bytes);
        Ok(())
    }

    /// Takes the next complete record, if one is buffered. Validates the length prefix as
    /// soon as it is complete.
    pub fn next_record(&mut self) -> Result<Option<Record>, RecordError> {
        let Some(len_bytes) = self.buf.get(..4) else {
            return Ok(None);
        };
        let len_arr: [u8; 4] = len_bytes.try_into().map_err(|_| RecordError::Malformed)?;
        let len =
            usize::try_from(u32::from_be_bytes(len_arr)).map_err(|_| RecordError::TooLarge)?;
        if len > MAX_RECORD_LEN {
            return Err(RecordError::TooLarge);
        }
        if len < 2 {
            return Err(RecordError::Malformed);
        }
        let Some(body) = self.buf.get(4..4 + len) else {
            return Ok(None);
        };
        let version = body.first().copied().ok_or(RecordError::Malformed)?;
        let kind = body.get(1).copied().ok_or(RecordError::Malformed)?;
        let payload = body.get(2..).unwrap_or_default().to_vec();
        self.buf.drain(..4 + len);
        Ok(Some(Record {
            version,
            kind,
            payload,
        }))
    }
}

/// Identifier of a paired remote device: 16 bytes, shown as 32 lowercase hex digits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(pub [u8; 16]);

impl DeviceId {
    /// Lowercase hex form.
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    /// Parses the hex form.
    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes = from_hex(s)?;
        Some(DeviceId(bytes.try_into().ok()?))
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({})", self.to_hex())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for DeviceId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        DeviceId::from_hex(&s).ok_or_else(|| serde::de::Error::custom("invalid device id"))
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Lowercase hex encoding.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(
            HEX.get(usize::from(b >> 4)).copied().unwrap_or(b'0'),
        ));
        s.push(char::from(
            HEX.get(usize::from(b & 0x0f)).copied().unwrap_or(b'0'),
        ));
    }
    s
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Decodes hex (either case); `None` on odd length or a non-hex digit.
pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return None;
    }
    b.as_chunks::<2>()
        .0
        .iter()
        .map(|[hi, lo]| Some((hex_val(*hi)? << 4) | hex_val(*lo)?))
        .collect()
}

/// RFC 4648 base32 alphabet (no padding). It has no `0`, `1`, `8` or `9`.
const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Base32 encoding without padding.
pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for b in bytes {
        acc = (acc << 8) | u32::from(*b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = usize::try_from((acc >> bits) & 31).unwrap_or(0);
            out.push(char::from(B32.get(idx).copied().unwrap_or(b'A')));
        }
        acc &= (1 << bits) - 1;
    }
    if bits > 0 {
        let idx = usize::try_from((acc << (5 - bits)) & 31).unwrap_or(0);
        out.push(char::from(B32.get(idx).copied().unwrap_or(b'A')));
    }
    out
}

fn b32_val(c: char) -> Option<u32> {
    let c = c.to_ascii_uppercase();
    // Common misreadings of a typed code.
    let c = match c {
        '0' => 'O',
        '1' => 'I',
        '8' => 'B',
        other => other,
    };
    B32.iter()
        .position(|&x| char::from(x) == c)
        .and_then(|p| u32::try_from(p).ok())
}

/// Decodes base32, ignoring case, spaces and dashes and reading `0`/`1`/`8` as `O`/`I`/`B`.
/// `None` on any other character or non-zero trailing bits.
pub fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for c in s.chars() {
        if c == '-' || c == ' ' {
            continue;
        }
        acc = (acc << 5) | b32_val(c)?;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).ok()?);
        }
        acc &= (1 << bits) - 1;
    }
    if acc != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let b = [0u8, 1, 0xab, 0xff];
        assert_eq!(to_hex(&b), "0001abff");
        assert_eq!(from_hex("0001ABff").unwrap(), b);
        assert!(from_hex("abc").is_none());
        assert!(from_hex("zz").is_none());
    }

    #[test]
    fn base32_roundtrip_and_misreadings() {
        // RFC 4648 test vector.
        assert_eq!(base32_encode(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32_decode("mzxw-6ytb-oi").unwrap(), b"foobar");
        let bytes: Vec<u8> = (0u8..20).collect();
        let enc = base32_encode(&bytes);
        assert_eq!(enc.len(), 32);
        assert_eq!(base32_decode(&enc).unwrap(), bytes);
        assert_eq!(base32_decode("MZXW6YTB0I").unwrap(), b"foobar");
        assert!(base32_decode("MZXW6YTBO!").is_none());
    }

    #[test]
    fn reader_reassembles_split_records() {
        let mut bytes = encode_record(Kind::Data, b"hello");
        bytes.extend(encode_record(Kind::ServerHello, b""));
        let mut r = RecordReader::new();
        for b in &bytes {
            r.push(&[*b]).unwrap();
        }
        let a = r.next_record().unwrap().unwrap();
        assert_eq!(a.kind, Kind::Data as u8);
        assert_eq!(a.payload, b"hello");
        let b = r.next_record().unwrap().unwrap();
        assert_eq!(b.kind, Kind::ServerHello as u8);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn reader_rejects_bad_lengths_early() {
        let mut r = RecordReader::new();
        r.push(&u32::MAX.to_be_bytes()).unwrap();
        assert_eq!(r.next_record(), Err(RecordError::TooLarge));
        let mut r = RecordReader::new();
        r.push(&1u32.to_be_bytes()).unwrap();
        assert_eq!(r.next_record(), Err(RecordError::Malformed));
    }
}
