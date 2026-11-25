//! Key schedule, handshake MACs and the AEAD of data frames (see the crate docs for the
//! exact inputs). Only vetted RustCrypto primitives: HKDF-SHA256, HMAC-SHA256 (verified
//! in constant time), ChaCha20-Poly1305. The labels are part of the protocol; the
//! known-answer test pins them.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::RunnerError;
use crate::{KEY_LEN, MIN_ENTROPY_LEN, PROTOCOL_VERSION};

/// Length of a connection nonce.
pub const NONCE_LEN: usize = 32;
/// Length of a handshake MAC.
pub const MAC_LEN: usize = 32;
/// Length of the AEAD tag.
pub const TAG_LEN: usize = 16;
/// Protocol magic, in both hellos, the transcript and every AAD.
pub const MAGIC: &[u8; 4] = b"JRUN";

/// Direction byte of client→server frames.
pub const DIR_C2S: u8 = 0x43;
/// Direction byte of server→client frames.
pub const DIR_S2C: u8 = 0x53;
/// Frame kind of data frames (bound into the AAD).
pub const KIND_DATA: u8 = 0x10;

const ENTROPY_SALT: &[u8] = b"jarvis-runner v1 entropy";
const SESSION_SALT: &[u8] = b"jarvis-runner v1";
const SERVER_FINISHED: &[u8] = b"server-finished";
const CLIENT_FINISHED: &[u8] = b"client-finished";

/// Least number of distinct byte values in the entropy (catches zero or constant fills;
/// 32 random bytes have about 30).
const MIN_DISTINCT_ENTROPY_BYTES: usize = 16;

/// The pre-shared key, zeroized on drop, never printed.
#[derive(Clone)]
pub struct Psk(Zeroizing<[u8; KEY_LEN]>);

impl Psk {
    /// Takes a 32-byte key.
    pub fn new(bytes: &[u8]) -> Result<Self, RunnerError> {
        let array: [u8; KEY_LEN] = bytes.try_into().map_err(|_| RunnerError::Key)?;
        Ok(Self(Zeroizing::new(array)))
    }
}

impl std::fmt::Debug for Psk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Psk(<redacted>)")
    }
}

/// Refuses entropy that is too short or obviously not random.
pub fn check_entropy(entropy: &[u8]) -> Result<(), RunnerError> {
    if entropy.len() < MIN_ENTROPY_LEN {
        return Err(RunnerError::Entropy);
    }
    let mut seen = [false; 256];
    for b in entropy {
        if let Some(s) = seen.get_mut(usize::from(*b)) {
            *s = true;
        }
    }
    if seen.iter().filter(|s| **s).count() < MIN_DISTINCT_ENTROPY_BYTES {
        return Err(RunnerError::Entropy);
    }
    Ok(())
}

/// One side's connection nonce from shell entropy (`label` separates the sides, so equal
/// entropy on both sides still gives different nonces).
pub fn nonce_from_entropy(entropy: &[u8], label: &[u8]) -> Result<[u8; NONCE_LEN], RunnerError> {
    check_entropy(entropy)?;
    let mut nonce = [0u8; NONCE_LEN];
    Hkdf::<Sha256>::new(Some(ENTROPY_SALT), entropy)
        .expand(label, &mut nonce)
        .map_err(|_| RunnerError::Entropy)?;
    Ok(nonce)
}

/// The keys of one connection. Zeroized on drop.
pub struct SessionKeys {
    mac: Zeroizing<[u8; 32]>,
    c2s: Zeroizing<[u8; 32]>,
    s2c: Zeroizing<[u8; 32]>,
    transcript: Vec<u8>,
}

impl std::fmt::Debug for SessionKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SessionKeys(<redacted>)")
    }
}

impl SessionKeys {
    /// Derives the keys of the connection with client nonce `nc` and server nonce `ns`.
    pub fn derive(
        psk: &Psk,
        nc: &[u8; NONCE_LEN],
        ns: &[u8; NONCE_LEN],
    ) -> Result<Self, RunnerError> {
        let mut salt = Vec::with_capacity(SESSION_SALT.len() + 2 * NONCE_LEN);
        salt.extend_from_slice(SESSION_SALT);
        salt.extend_from_slice(nc);
        salt.extend_from_slice(ns);
        let hk = Hkdf::<Sha256>::new(Some(&salt), psk.0.as_ref());
        let expand = |info: &[u8]| -> Result<Zeroizing<[u8; 32]>, RunnerError> {
            let mut out = Zeroizing::new([0u8; 32]);
            hk.expand(info, out.as_mut())
                .map_err(|_| RunnerError::Auth)?;
            Ok(out)
        };
        let mut transcript = Vec::with_capacity(MAGIC.len() + 1 + 2 * NONCE_LEN);
        transcript.extend_from_slice(MAGIC);
        transcript.push(PROTOCOL_VERSION);
        transcript.extend_from_slice(nc);
        transcript.extend_from_slice(ns);
        Ok(Self {
            mac: expand(b"mac")?,
            c2s: expand(b"client-to-server")?,
            s2c: expand(b"server-to-client")?,
            transcript,
        })
    }

    fn hmac(&self, label: &[u8]) -> Result<Hmac<Sha256>, RunnerError> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(self.mac.as_ref())
            .map_err(|_| RunnerError::Auth)?;
        mac.update(label);
        mac.update(&self.transcript);
        Ok(mac)
    }

    /// The server's proof of the key.
    pub fn server_mac(&self) -> Result<[u8; MAC_LEN], RunnerError> {
        Ok(self.hmac(SERVER_FINISHED)?.finalize().into_bytes().into())
    }

    /// The client's proof of the key.
    pub fn client_mac(&self) -> Result<[u8; MAC_LEN], RunnerError> {
        Ok(self.hmac(CLIENT_FINISHED)?.finalize().into_bytes().into())
    }

    /// Checks the server's proof (constant time).
    pub fn verify_server(&self, tag: &[u8]) -> Result<(), RunnerError> {
        self.hmac(SERVER_FINISHED)?
            .verify_slice(tag)
            .map_err(|_| RunnerError::Auth)
    }

    /// Checks the client's proof (constant time).
    pub fn verify_client(&self, tag: &[u8]) -> Result<(), RunnerError> {
        self.hmac(CLIENT_FINISHED)?
            .verify_slice(tag)
            .map_err(|_| RunnerError::Auth)
    }

    fn key(&self, direction: u8) -> &[u8; 32] {
        if direction == DIR_C2S {
            &self.c2s
        } else {
            &self.s2c
        }
    }

    /// Seals `plaintext` as data frame `counter` in `direction`; returns ciphertext || tag.
    pub fn seal(
        &self,
        direction: u8,
        counter: u64,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, RunnerError> {
        let cipher =
            ChaCha20Poly1305::new_from_slice(self.key(direction)).map_err(|_| RunnerError::Auth)?;
        let aad = aad(counter);
        cipher
            .encrypt(
                &nonce(direction, counter),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| RunnerError::Auth)
    }

    /// Opens data frame `counter` in `direction`. Any failure is [`RunnerError::Auth`].
    pub fn open(&self, direction: u8, counter: u64, sealed: &[u8]) -> Result<Vec<u8>, RunnerError> {
        if sealed.len() < TAG_LEN {
            return Err(RunnerError::Auth);
        }
        let cipher =
            ChaCha20Poly1305::new_from_slice(self.key(direction)).map_err(|_| RunnerError::Auth)?;
        let aad = aad(counter);
        cipher
            .decrypt(
                &nonce(direction, counter),
                Payload {
                    msg: sealed,
                    aad: &aad,
                },
            )
            .map_err(|_| RunnerError::Auth)
    }
}

fn nonce(direction: u8, counter: u64) -> Nonce {
    let mut n = [0u8; 12];
    let parts = [direction, 0, 0, 0]
        .into_iter()
        .chain(counter.to_be_bytes());
    for (dst, src) in n.iter_mut().zip(parts) {
        *dst = src;
    }
    Nonce::from(n)
}

fn aad(counter: u64) -> [u8; 14] {
    let mut a = [0u8; 14];
    let parts = MAGIC
        .iter()
        .copied()
        .chain([PROTOCOL_VERSION, KIND_DATA])
        .chain(counter.to_be_bytes());
    for (dst, src) in a.iter_mut().zip(parts) {
        *dst = src;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Pins the labels and input order: changing any breaks every runner build in the
    /// field, so it must be a deliberate version bump.
    #[test]
    fn known_answer() {
        let psk = Psk::new(&[7u8; 32]).unwrap();
        let keys = SessionKeys::derive(&psk, &[1u8; 32], &[2u8; 32]).unwrap();
        let s = keys.server_mac().unwrap();
        let c = keys.client_mac().unwrap();
        let sealed = keys.seal(DIR_C2S, 0, b"hi").unwrap();
        let pinned = format!("{}|{}|{}", hex(&s), hex(&c), hex(&sealed));
        // Recomputed from the same inputs every run, and compared with the value pinned
        // when the protocol was written.
        assert_eq!(
            pinned, KNOWN_ANSWER,
            "the runner protocol's key schedule changed"
        );
    }

    // Re-pinned for protocol version 2 (the version byte is in the transcript and AAD).
    const KNOWN_ANSWER: &str = concat!(
        "a894d8ff93d794e1ebdb514fa45ef2c6f267649b10e430817722e1c26c72c79c|",
        "0fac56410318410f419922f85855865d266253643d2c13a214ef0ab2b9f8d295|",
        "b3dd5517e0c9edcdc9f9164e0101f415cf50"
    );

    #[test]
    fn directions_and_counters_separate_frames() {
        let psk = Psk::new(&[9u8; 32]).unwrap();
        let keys = SessionKeys::derive(&psk, &[3u8; 32], &[4u8; 32]).unwrap();
        let sealed = keys.seal(DIR_C2S, 5, b"payload").unwrap();
        assert_eq!(keys.open(DIR_C2S, 5, &sealed).unwrap(), b"payload");
        assert_eq!(keys.open(DIR_S2C, 5, &sealed), Err(RunnerError::Auth));
        assert_eq!(keys.open(DIR_C2S, 6, &sealed), Err(RunnerError::Auth));
        assert_eq!(keys.open(DIR_C2S, 5, &sealed[..3]), Err(RunnerError::Auth));
    }

    #[test]
    fn entropy_is_checked() {
        assert_eq!(check_entropy(&[0u8; 64]), Err(RunnerError::Entropy));
        assert_eq!(check_entropy(&[1u8; 8]), Err(RunnerError::Entropy));
        let good: Vec<u8> = (0u8..32).map(|i| i.wrapping_mul(37)).collect();
        assert!(check_entropy(&good).is_ok());
    }
}
