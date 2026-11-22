//! Pairing offers and pairing codes.
//!
//! A pairing code is the 160-bit [`PairingSecret`] in RFC 4648 base32: 32 characters,
//! shown in eight groups of four (`ABCD-EFGH-…`). The QR payload is the same code behind
//! a fixed prefix. There are no short numeric codes: a 6-digit code would be brute-forced
//! offline from one recorded handshake, and the fix for that (a PAKE) is not something to
//! build by hand.

use std::fmt;

use crate::codec::{base32_decode, base32_encode};
use crate::crypto::{PairingSecret, PAIRING_SECRET_LEN};
use crate::error::CodeError;

/// How long a pairing offer stays valid, in milliseconds (5 minutes).
pub const PAIRING_TTL_MS: u64 = 5 * 60 * 1000;
/// Prefix of the QR payload.
pub const QR_PREFIX: &str = "jarvis-remote:1:";
/// Characters per group in the displayed code.
pub const CODE_GROUP_LEN: usize = 4;

/// The pairing offer the phone shows: valid once, until `expires_at_ms`.
#[derive(Clone, PartialEq, Eq)]
pub struct PairingOffer {
    /// Grouped base32 code for typing (`ABCD-EFGH-…`, 39 characters).
    pub code: String,
    /// Payload for the QR code (`jarvis-remote:1:` followed by the ungrouped code).
    pub qr_payload: String,
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
pub fn format_code(secret: &PairingSecret) -> String {
    let raw = base32_encode(secret.as_bytes());
    let mut out = String::with_capacity(raw.len() + raw.len() / CODE_GROUP_LEN);
    for (i, c) in raw.chars().enumerate() {
        if i > 0 && i % CODE_GROUP_LEN == 0 {
            out.push('-');
        }
        out.push(c);
    }
    out
}

/// The QR payload for a secret.
pub fn qr_payload(secret: &PairingSecret) -> String {
    format!("{QR_PREFIX}{}", base32_encode(secret.as_bytes()))
}

/// Parses a typed code or a scanned QR payload. Case, spaces and dashes do not matter;
/// `0`, `1` and `8` are read as `O`, `I` and `B`.
pub fn parse_code(input: &str) -> Result<PairingSecret, CodeError> {
    let trimmed = input.trim();
    let body = trimmed.strip_prefix(QR_PREFIX).unwrap_or(trimmed);
    let bytes = base32_decode(body).ok_or(CodeError::InvalidCharacters)?;
    let arr: [u8; PAIRING_SECRET_LEN] = bytes.try_into().map_err(|_| CodeError::WrongLength)?;
    Ok(PairingSecret::from_bytes(arr))
}

/// Builds the secret from the first 20 bytes of shell-supplied entropy.
pub(crate) fn secret_from_entropy(entropy: &[u8; 32]) -> PairingSecret {
    let mut arr = [0u8; PAIRING_SECRET_LEN];
    for (d, s) in arr.iter_mut().zip(entropy.iter()) {
        *d = *s;
    }
    PairingSecret::from_bytes(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_roundtrips_through_both_forms() {
        let s = secret_from_entropy(&[7u8; 32]);
        let code = format_code(&s);
        assert_eq!(code.len(), 39);
        assert_eq!(code.split('-').count(), 8);
        assert_eq!(parse_code(&code).unwrap(), s);
        assert_eq!(parse_code(&code.to_lowercase()).unwrap(), s);
        assert_eq!(parse_code(&qr_payload(&s)).unwrap(), s);
        assert_eq!(parse_code("AAAA"), Err(CodeError::WrongLength));
        assert_eq!(parse_code("ABC!"), Err(CodeError::InvalidCharacters));
    }
}
