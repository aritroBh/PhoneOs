//! Pairing offers and pairing codes.
//!
//! A pairing code is the 160-bit [`PairingSecret`] in RFC 4648 base32: exactly 32
//! characters, shown in eight groups of four (`ABCD-EFGH-…`). The QR payload is the same
//! code behind a fixed prefix. There are no short numeric codes: a 6-digit code would be
//! brute-forced offline from one recorded handshake, and the fix for that (a PAKE) is not
//! something to build by hand.
//!
//! The secret is derived from shell entropy with HKDF under its own label, so it never
//! equals (or shares bytes with) anything sent on the wire, even if the shell passes the
//! same entropy elsewhere.

use std::fmt;

use zeroize::Zeroizing;

use crate::codec::{base32_decode, base32_encode, base32_symbols, is_base32_char};
use crate::crypto::{PairingSecret, PAIRING_SECRET_LEN};
use crate::error::CodeError;

/// How long a pairing offer stays valid, in milliseconds (5 minutes).
pub const PAIRING_TTL_MS: u64 = 5 * 60 * 1000;
/// Prefix of the QR payload.
pub const QR_PREFIX: &str = "jarvis-remote:2:";
/// Characters per group in the displayed code.
pub const CODE_GROUP_LEN: usize = 4;
/// Base32 characters in a code (160 bits / 5).
pub const CODE_SYMBOLS: usize = 32;

/// The pairing offer the phone shows: valid once, until `expires_at_ms`.
///
/// Both strings hold the secret and are zeroized on drop; copies the shell makes (for the
/// QR renderer, the label) are outside this crate's control.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingOffer {
    /// Grouped base32 code for typing (`ABCD-EFGH-…`, 39 characters).
    pub code: Zeroizing<String>,
    /// Payload for the QR code (`jarvis-remote:2:` followed by the ungrouped code).
    pub qr_payload: Zeroizing<String>,
    /// When the offer stops being accepted (shell clock, milliseconds).
    pub expires_at_ms: u64,
}

impl fmt::Debug for PairingOffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PairingOffer")
            .field("code", &"<redacted>")
            .field("expires_at_ms", &self.expires_at_ms)
            .finish_non_exhaustive()
    }
}

/// The grouped, typeable form of a secret.
pub fn format_code(secret: &PairingSecret) -> Zeroizing<String> {
    let raw = base32_encode(secret.as_bytes());
    let mut out = Zeroizing::new(String::with_capacity(
        raw.len() + raw.len() / CODE_GROUP_LEN,
    ));
    for (i, c) in raw.chars().enumerate() {
        if i > 0 && i % CODE_GROUP_LEN == 0 {
            out.push('-');
        }
        out.push(c);
    }
    out
}

/// The QR payload for a secret.
pub fn qr_payload(secret: &PairingSecret) -> Zeroizing<String> {
    let raw = base32_encode(secret.as_bytes());
    let mut out = Zeroizing::new(String::with_capacity(QR_PREFIX.len() + raw.len()));
    out.push_str(QR_PREFIX);
    out.push_str(&raw);
    out
}

/// Parses a typed code or a scanned QR payload. Case, spaces and dashes do not matter;
/// `0`, `1` and `8` are read as `O`, `I` and `B`. Exactly [`CODE_SYMBOLS`] symbols are
/// required: trailing characters are an error, not ignored.
pub fn parse_code(input: &str) -> Result<PairingSecret, CodeError> {
    let trimmed = input.trim();
    let body = trimmed.strip_prefix(QR_PREFIX).unwrap_or(trimmed);
    if !body
        .chars()
        .all(|c| c == '-' || c == ' ' || is_base32_char(c))
    {
        return Err(CodeError::InvalidCharacters);
    }
    if base32_symbols(body) != CODE_SYMBOLS {
        return Err(CodeError::WrongLength);
    }
    let bytes = base32_decode(body).ok_or(CodeError::InvalidCharacters)?;
    let arr: [u8; PAIRING_SECRET_LEN] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| CodeError::WrongLength)?;
    Ok(PairingSecret::from_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> PairingSecret {
        PairingSecret::from_bytes([7u8; 20])
    }

    #[test]
    fn code_roundtrips_through_both_forms() {
        let s = secret();
        let code = format_code(&s);
        assert_eq!(code.len(), 39);
        assert_eq!(code.split('-').count(), 8);
        assert_eq!(parse_code(&code).unwrap(), s);
        assert_eq!(parse_code(&code.to_lowercase()).unwrap(), s);
        assert_eq!(parse_code(&qr_payload(&s)).unwrap(), s);
        assert_eq!(parse_code("AAAA"), Err(CodeError::WrongLength));
        assert_eq!(parse_code("ABC!"), Err(CodeError::InvalidCharacters));
    }

    #[test]
    fn trailing_characters_are_rejected() {
        let s = secret();
        let code = format_code(&s);
        // An extra 'A' adds only zero bits; it used to be silently accepted.
        for extra in ["A", "-A", "AAAA", "7"] {
            let long = format!("{}{extra}", code.as_str());
            assert_eq!(parse_code(&long), Err(CodeError::WrongLength), "{long}");
        }
        assert_eq!(
            parse_code(&format!("{}!", code.as_str())),
            Err(CodeError::InvalidCharacters)
        );
        assert_eq!(
            parse_code(&format!("{}AA", qr_payload(&s).as_str())),
            Err(CodeError::WrongLength)
        );
        assert_eq!(parse_code(&format!("  {}\n", code.as_str())).unwrap(), s);
    }
}
