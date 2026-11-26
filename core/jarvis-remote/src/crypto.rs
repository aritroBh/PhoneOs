//! The key schedule, the handshake MACs and the authenticated-encryption channel.
//!
//! Every primitive is a vetted RustCrypto or dalek implementation: HKDF-SHA256 (`hkdf`),
//! HMAC-SHA256 (`hmac`), X25519 (`x25519-dalek`), ChaCha20-Poly1305 (`chacha20poly1305`)
//! and constant-time comparison (`subtle`). Nothing here is a custom primitive; this module
//! only fixes the labels, the inputs and their order. The labels are part of the protocol:
//! changing one breaks every paired device, which the known-answer test catches.
//!
//! ```text
//! -- from shell entropy (E: ≥ 32 CSPRNG bytes; c: persisted counter; t: clock) --
//! S            = HKDF(salt = ENT, ikm = E).expand("pairing-secret", 20)
//! Np           = HKDF(salt = ENT, ikm = E).expand(lp("phone-nonce", c, t), 32)
//! ep           = HKDF(salt = ENT, ikm = E).expand(lp("phone-ephemeral", c, t), 32)
//! Nc, er       = the same with "remote-nonce" / "remote-ephemeral" (no counter, no clock)
//! -- handshake (Ep = X25519(ep), Er = X25519(er), dh = X25519(ep, Er) = X25519(er, Ep)) --
//! pair_key     = HKDF(salt = "jarvis-remote v2 pair", ikm = S).expand("pair-auth", 32)
//! device_id    = HKDF(salt = Np || Nc, ikm = S).expand(lp("device-id"), 16)
//! device_key   = HKDF(salt = Np || Nc, ikm = S || dh).expand(lp("device-key", device_id, name), 32)
//! auth_key     = HKDF(salt = "jarvis-remote v2 auth", ikm = device_key).expand("connect-auth", 32)
//! confirm      = HKDF(salt = ENT, ikm = E).expand(lp("phone-confirm", c, t), 32)   (phone only)
//! commitment   = SHA-256(lp("jarvis-remote v2 confirm-commit", confirm))         (in ServerHello)
//! check        = HKDF(salt = Np || Nc, ikm = device_key || confirm)
//!                    .expand(lp("confirm-check", device_id, Ep, Er, commitment), 8) mod 10^6
//! r2p || p2r   = HKDF(salt = Np || Nc, ikm = device_key || dh)
//!                    .expand(lp("session", device_id, Ep, Er), 64)
//! nonce        = direction (1 byte: 0x52 remote→phone, 0x50 phone→remote) || 0 0 0 || counter (u64 BE)
//! aad          = version || kind || device_id || counter (u64 BE)
//! ```
//!
//! `ENT` is `"jarvis-remote v2 entropy"`. `lp(..)` is a length-prefixed concatenation
//! (u32 BE length before each part), so no two different input lists produce the same
//! bytes. Both ephemeral publics are bound into every handshake MAC, and a
//! non-contributory (low-order) DH result is rejected.
//!
//! Forward secrecy: the session keys depend on `dh`, whose secrets live only for one
//! handshake and are zeroized on drop. A device key stolen later does not decrypt recorded
//! sessions; a pairing secret disclosed later does not yield the device key.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

use crate::codec::{DeviceId, Kind, PROTOCOL_VERSION};

/// Length of a connection nonce, in bytes.
pub const NONCE_LEN: usize = 32;
/// Length of an X25519 public key, in bytes.
pub const PUBLIC_LEN: usize = 32;
/// Length of a handshake MAC (HMAC-SHA256), in bytes.
pub const MAC_LEN: usize = 32;
/// Length of the AEAD tag, in bytes.
pub const TAG_LEN: usize = 16;
/// Length of the pairing secret, in bytes (160 bits).
pub const PAIRING_SECRET_LEN: usize = 20;
/// Least entropy the shell must pass to any entry point, in bytes.
pub const MIN_ENTROPY_LEN: usize = 32;
/// Length of the phone's pairing confirmation value and of its commitment, in bytes.
pub const CONFIRM_LEN: usize = 32;
/// Digits in the pairing check string (shown as two groups of three).
pub const CHECK_DIGITS: usize = 6;

/// Least number of distinct byte values in that entropy (a sanity check, see
/// [`check_entropy`]).
pub const MIN_DISTINCT_ENTROPY_BYTES: usize = 16;

/// Salt for everything derived from shell entropy.
const ENTROPY_SALT: &[u8] = b"jarvis-remote v2 entropy";
/// Salt for the pairing-authentication key.
const PAIR_SALT: &[u8] = b"jarvis-remote v2 pair";
/// Salt for the connect-authentication key.
const AUTH_SALT: &[u8] = b"jarvis-remote v2 auth";
/// Label of the confirmation commitment.
const CONFIRM_COMMIT_LABEL: &[u8] = b"jarvis-remote v2 confirm-commit";

/// Direction byte in the AEAD nonce: remote to phone.
pub const DIR_R2P: u8 = 0x52;
/// Direction byte in the AEAD nonce: phone to remote.
pub const DIR_P2R: u8 = 0x50;

/// A failure inside the cryptographic layer. Deliberately carries no detail: callers turn
/// every variant into the same opaque authentication failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("authentication failed")]
pub struct CryptoError;

/// Shell entropy was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EntropyError {
    /// Fewer than [`MIN_ENTROPY_LEN`] bytes.
    #[error("entropy too short")]
    TooShort,
    /// Fewer than [`MIN_DISTINCT_ENTROPY_BYTES`] distinct byte values (all zero, a
    /// constant fill, a short repeated pattern).
    #[error("entropy is not random")]
    NotRandom,
}

/// Rejects entropy that is obviously not CSPRNG output.
///
/// 32 uniformly random bytes have about 30 distinct values; fewer than 16 has negligible
/// probability, so this never refuses real randomness. It catches zero buffers, constant
/// fills and short repeated patterns. It cannot catch a buffer that is random-looking but
/// reused: the derivations below defend against that separately.
pub fn check_entropy(entropy: &[u8]) -> Result<(), EntropyError> {
    if entropy.len() < MIN_ENTROPY_LEN {
        return Err(EntropyError::TooShort);
    }
    let mut seen = [false; 256];
    for b in entropy {
        if let Some(s) = seen.get_mut(usize::from(*b)) {
            *s = true;
        }
    }
    if seen.iter().filter(|s| **s).count() < MIN_DISTINCT_ENTROPY_BYTES {
        return Err(EntropyError::NotRandom);
    }
    Ok(())
}

/// A 32-byte secret (device key, derived key). Zeroized on drop, compared in constant
/// time, never printed.
#[derive(Clone)]
pub struct Secret32([u8; 32]);

impl Secret32 {
    /// Wraps raw key bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw bytes (for persistence and key derivation only).
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for Secret32 {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl PartialEq for Secret32 {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for Secret32 {}

impl fmt::Debug for Secret32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret32(<redacted>)")
    }
}

/// The pairing secret shown on the phone. Zeroized on drop, never printed.
#[derive(Clone)]
pub struct PairingSecret([u8; PAIRING_SECRET_LEN]);

impl PairingSecret {
    /// Wraps raw secret bytes.
    pub fn from_bytes(bytes: [u8; PAIRING_SECRET_LEN]) -> Self {
        Self(bytes)
    }

    /// The raw bytes (for encoding and key derivation only).
    pub fn as_bytes(&self) -> &[u8; PAIRING_SECRET_LEN] {
        &self.0
    }
}

impl Drop for PairingSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl PartialEq for PairingSecret {
    fn eq(&self, other: &Self) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl Eq for PairingSecret {}

impl fmt::Debug for PairingSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingSecret(<redacted>)")
    }
}

/// One side's per-connection randomness: a public nonce and an ephemeral X25519 key pair.
/// The secret is zeroized on drop.
pub struct Ephemeral {
    /// The nonce sent in clear.
    pub nonce: [u8; NONCE_LEN],
    secret: StaticSecret,
    /// The public key sent in clear.
    pub public: [u8; PUBLIC_LEN],
    /// The phone's confirmation value for the pairing check string: committed to in the
    /// `ServerHello` ([`confirm_commitment`]) and revealed in the `PairAccept`, so neither
    /// side can choose it after seeing the other's values (see [`confirm_check`]).
    pub confirm: [u8; CONFIRM_LEN],
}

impl fmt::Debug for Ephemeral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ephemeral")
            .field("public", &crate::codec::to_hex(&self.public))
            .finish_non_exhaustive()
    }
}

impl Ephemeral {
    fn derive(
        entropy: &[u8],
        nonce_info: &[u8],
        secret_info: &[u8],
        confirm_info: &[u8],
    ) -> Result<Self, CryptoError> {
        let hk = Hkdf::<Sha256>::new(Some(ENTROPY_SALT), entropy);
        let mut nonce = [0u8; NONCE_LEN];
        hk.expand(nonce_info, &mut nonce).map_err(|_| CryptoError)?;
        let mut confirm = [0u8; CONFIRM_LEN];
        hk.expand(confirm_info, &mut confirm)
            .map_err(|_| CryptoError)?;
        let mut sk = Zeroizing::new([0u8; 32]);
        hk.expand(secret_info, sk.as_mut())
            .map_err(|_| CryptoError)?;
        let secret = StaticSecret::from(*sk);
        let public = PublicKey::from(&secret).to_bytes();
        Ok(Self {
            nonce,
            secret,
            public,
            confirm,
        })
    }

    /// The phone's randomness for one connection. `counter` (persisted, never repeated)
    /// and `now_ms` are mixed in, so repeated entropy still yields fresh values.
    pub fn phone(entropy: &[u8], counter: u64, now_ms: u64) -> Result<Self, CryptoError> {
        let (c, t) = (counter.to_be_bytes(), now_ms.to_be_bytes());
        Self::derive(
            entropy,
            &lp(&[b"phone-nonce", &c, &t]),
            &lp(&[b"phone-ephemeral", &c, &t]),
            &lp(&[b"phone-confirm", &c, &t]),
        )
    }

    /// The remote's randomness for one connection.
    pub fn remote(entropy: &[u8]) -> Result<Self, CryptoError> {
        Self::derive(
            entropy,
            &lp(&[b"remote-nonce"]),
            &lp(&[b"remote-ephemeral"]),
            &lp(&[b"remote-confirm"]),
        )
    }

    /// X25519 with the peer's public key. Refuses a non-contributory result (the peer sent
    /// a low-order point, which would make the shared secret predictable).
    pub fn agree(
        &self,
        peer_public: &[u8; PUBLIC_LEN],
    ) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
        let shared = self.secret.diffie_hellman(&PublicKey::from(*peer_public));
        if !shared.was_contributory() {
            return Err(CryptoError);
        }
        Ok(Zeroizing::new(shared.to_bytes()))
    }
}

/// The public values of one handshake, bound into every MAC and key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transcript {
    /// Phone nonce.
    pub np: [u8; NONCE_LEN],
    /// Remote nonce.
    pub nc: [u8; NONCE_LEN],
    /// Phone ephemeral public key.
    pub ep: [u8; PUBLIC_LEN],
    /// Remote ephemeral public key.
    pub er: [u8; PUBLIC_LEN],
}

impl Transcript {
    fn salt(&self) -> [u8; 2 * NONCE_LEN] {
        let mut s = [0u8; 2 * NONCE_LEN];
        for (dst, src) in s.iter_mut().zip(self.np.iter().chain(self.nc.iter())) {
            *dst = *src;
        }
        s
    }
}

/// Derives the pairing secret from shell entropy.
pub fn pairing_secret_from_entropy(entropy: &[u8]) -> Result<PairingSecret, CryptoError> {
    let mut out = [0u8; PAIRING_SECRET_LEN];
    Hkdf::<Sha256>::new(Some(ENTROPY_SALT), entropy)
        .expand(b"pairing-secret", &mut out)
        .map_err(|_| CryptoError)?;
    let secret = PairingSecret(out);
    out.zeroize();
    Ok(secret)
}

/// Length-prefixed concatenation: a u32 BE length before every part.
fn lp(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in parts {
        let len = u32::try_from(p.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(p);
    }
    out
}

fn hkdf32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<Secret32, CryptoError> {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, &mut out)
        .map_err(|_| CryptoError)?;
    let s = Secret32(out);
    out.zeroize();
    Ok(s)
}

fn concat_secret(a: &[u8], b: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut v = Zeroizing::new(Vec::with_capacity(a.len() + b.len()));
    v.extend_from_slice(a);
    v.extend_from_slice(b);
    v
}

fn hmac(key: &Secret32, data: &[u8]) -> Result<[u8; MAC_LEN], CryptoError> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes()).map_err(|_| CryptoError)?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().into())
}

fn hmac_verify(key: &Secret32, data: &[u8], tag: &[u8]) -> Result<(), CryptoError> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes()).map_err(|_| CryptoError)?;
    mac.update(data);
    // `verify_slice` compares in constant time.
    mac.verify_slice(tag).map_err(|_| CryptoError)
}

/// The key that authenticates a pairing handshake, derived from the pairing secret.
pub fn pair_key(secret: &PairingSecret) -> Result<Secret32, CryptoError> {
    hkdf32(PAIR_SALT, secret.as_bytes(), b"pair-auth")
}

/// The device id the phone assigns to a newly paired remote.
pub fn derive_device_id(secret: &PairingSecret, t: &Transcript) -> Result<DeviceId, CryptoError> {
    let mut out = [0u8; 16];
    Hkdf::<Sha256>::new(Some(&t.salt()), secret.as_bytes())
        .expand(&lp(&[b"device-id"]), &mut out)
        .map_err(|_| CryptoError)?;
    Ok(DeviceId(out))
}

/// The long-term key shared by the phone and one paired remote.
pub fn derive_device_key(
    secret: &PairingSecret,
    dh: &[u8; 32],
    t: &Transcript,
    device_id: &DeviceId,
    name: &str,
) -> Result<Secret32, CryptoError> {
    hkdf32(
        &t.salt(),
        &concat_secret(secret.as_bytes(), dh),
        &lp(&[b"device-key", &device_id.0, name.as_bytes()]),
    )
}

/// The key that authenticates a connect handshake, derived from the device key.
pub fn auth_key(device_key: &Secret32) -> Result<Secret32, CryptoError> {
    hkdf32(AUTH_SALT, device_key.as_bytes(), b"connect-auth")
}

/// The per-connection keys `(remote_to_phone, phone_to_remote)`.
pub fn session_keys(
    device_key: &Secret32,
    dh: &[u8; 32],
    device_id: &DeviceId,
    t: &Transcript,
) -> Result<(Secret32, Secret32), CryptoError> {
    let mut okm = Zeroizing::new([0u8; 64]);
    Hkdf::<Sha256>::new(Some(&t.salt()), &concat_secret(device_key.as_bytes(), dh))
        .expand(&lp(&[b"session", &device_id.0, &t.ep, &t.er]), okm.as_mut())
        .map_err(|_| CryptoError)?;
    let mut a = [0u8; 32];
    let mut b = [0u8; 32];
    for (i, byte) in okm.iter().enumerate() {
        if let Some(slot) = a.get_mut(i) {
            *slot = *byte;
        } else if let Some(slot) = b.get_mut(i.saturating_sub(32)) {
            *slot = *byte;
        }
    }
    let keys = (Secret32(a), Secret32(b));
    a.zeroize();
    b.zeroize();
    Ok(keys)
}

fn pair_transcript(
    role: &[u8],
    t: &Transcript,
    name: &str,
    device_id: Option<&DeviceId>,
) -> Vec<u8> {
    let id: &[u8] = device_id.map_or(&[], |d| &d.0);
    lp(&[
        b"jarvis-remote v2 pair",
        role,
        &[PROTOCOL_VERSION],
        &t.np,
        &t.nc,
        &t.ep,
        &t.er,
        name.as_bytes(),
        id,
    ])
}

fn connect_transcript(role: &[u8], device_id: &DeviceId, t: &Transcript) -> Vec<u8> {
    lp(&[
        b"jarvis-remote v2 connect",
        role,
        &[PROTOCOL_VERSION],
        &device_id.0,
        &t.np,
        &t.nc,
        &t.ep,
        &t.er,
    ])
}

/// The remote's pairing proof: it knows the secret, bound to the transcript and its name.
pub fn pair_client_mac(
    pk: &Secret32,
    t: &Transcript,
    name: &str,
) -> Result<[u8; MAC_LEN], CryptoError> {
    hmac(pk, &pair_transcript(b"remote", t, name, None))
}

/// Checks [`pair_client_mac`] in constant time.
pub fn verify_pair_client_mac(
    pk: &Secret32,
    t: &Transcript,
    name: &str,
    tag: &[u8],
) -> Result<(), CryptoError> {
    hmac_verify(pk, &pair_transcript(b"remote", t, name, None), tag)
}

/// The phone's pairing proof, which also authenticates the assigned device id.
pub fn pair_phone_mac(
    pk: &Secret32,
    t: &Transcript,
    name: &str,
    device_id: &DeviceId,
) -> Result<[u8; MAC_LEN], CryptoError> {
    hmac(pk, &pair_transcript(b"phone", t, name, Some(device_id)))
}

/// Checks [`pair_phone_mac`] in constant time.
pub fn verify_pair_phone_mac(
    pk: &Secret32,
    t: &Transcript,
    name: &str,
    device_id: &DeviceId,
    tag: &[u8],
) -> Result<(), CryptoError> {
    hmac_verify(
        pk,
        &pair_transcript(b"phone", t, name, Some(device_id)),
        tag,
    )
}

/// The phone's commitment to its confirmation value, sent in every `ServerHello`:
/// `SHA-256(lp("jarvis-remote v2 confirm-commit", confirm))`.
pub fn confirm_commitment(confirm: &[u8; CONFIRM_LEN]) -> [u8; CONFIRM_LEN] {
    let digest = Sha256::digest(lp(&[CONFIRM_COMMIT_LABEL, confirm]));
    let mut out = [0u8; CONFIRM_LEN];
    for (dst, src) in out.iter_mut().zip(digest.iter()) {
        *dst = *src;
    }
    out
}

/// The pairing check string both sides show after a pair exchange: six decimal digits in
/// two groups ("123 456"), from
/// `HKDF(salt = Np || Nc, ikm = device_key || confirm).expand(lp("confirm-check", device_id,
/// Ep, Er, commitment), 8)` read as a big-endian integer modulo 10^6.
///
/// The device key already depends on the pairing secret, both nonces, the DH result and
/// the name; `confirm` is the phone's value, committed to before the remote chose its own
/// and revealed after it. So a man in the middle (or a second device paired with a
/// glimpsed code) ends up with a different device key on each side, and cannot grind
/// either side's string to match the other: on the phone's side it does not know `confirm`
/// when it chooses its values, on the remote's side it committed to `confirm` before the
/// remote chose its values (and the remote checks the device id the phone assigns).
pub fn confirm_check(
    device_key: &Secret32,
    confirm: &[u8; CONFIRM_LEN],
    device_id: &DeviceId,
    t: &Transcript,
) -> Result<String, CryptoError> {
    let commitment = confirm_commitment(confirm);
    let mut okm = [0u8; 8];
    Hkdf::<Sha256>::new(
        Some(&t.salt()),
        &concat_secret(device_key.as_bytes(), confirm),
    )
    .expand(
        &lp(&[b"confirm-check", &device_id.0, &t.ep, &t.er, &commitment]),
        &mut okm,
    )
    .map_err(|_| CryptoError)?;
    let n = u64::from_be_bytes(okm) % 1_000_000;
    okm.zeroize();
    Ok(format!("{:03} {:03}", n / 1_000, n % 1_000))
}

/// The remote's connect proof (it holds the device key), bound to the transcript.
pub fn connect_client_mac(
    ak: &Secret32,
    device_id: &DeviceId,
    t: &Transcript,
) -> Result<[u8; MAC_LEN], CryptoError> {
    hmac(ak, &connect_transcript(b"remote", device_id, t))
}

/// Checks [`connect_client_mac`] in constant time.
pub fn verify_connect_client_mac(
    ak: &Secret32,
    device_id: &DeviceId,
    t: &Transcript,
    tag: &[u8],
) -> Result<(), CryptoError> {
    hmac_verify(ak, &connect_transcript(b"remote", device_id, t), tag)
}

/// The phone's connect proof (it holds the device key too).
pub fn connect_phone_mac(
    ak: &Secret32,
    device_id: &DeviceId,
    t: &Transcript,
) -> Result<[u8; MAC_LEN], CryptoError> {
    hmac(ak, &connect_transcript(b"phone", device_id, t))
}

/// Checks [`connect_phone_mac`] in constant time.
pub fn verify_connect_phone_mac(
    ak: &Secret32,
    device_id: &DeviceId,
    t: &Transcript,
    tag: &[u8],
) -> Result<(), CryptoError> {
    hmac_verify(ak, &connect_transcript(b"phone", device_id, t), tag)
}

/// Which side of the connection a [`Channel`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The phone (the hub).
    Phone,
    /// The remote device (the client).
    Remote,
}

/// The established, per-connection AEAD state: one key and one strictly increasing
/// counter per direction.
///
/// Gap policy: the transport is an ordered byte stream, so the receiver accepts exactly
/// the next counter value and nothing else. A dropped, replayed or reordered frame is an
/// authentication failure, and every failure is final ([`Channel::open`] leaves the
/// channel unusable). The remote reconnects to resynchronise.
pub struct Channel {
    device_id: DeviceId,
    send_key: Secret32,
    recv_key: Secret32,
    send_dir: u8,
    recv_dir: u8,
    send_ctr: u64,
    recv_ctr: u64,
    failed: bool,
}

impl fmt::Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Channel")
            .field("device_id", &self.device_id)
            .field("send_ctr", &self.send_ctr)
            .field("recv_ctr", &self.recv_ctr)
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

/// Bytes of the data-record header inside the record body (the counter).
pub const COUNTER_LEN: usize = 8;

impl Channel {
    /// Derives both directions' keys for one connection.
    pub fn new(
        side: Side,
        device_key: &Secret32,
        dh: &[u8; 32],
        device_id: DeviceId,
        t: &Transcript,
    ) -> Result<Self, CryptoError> {
        let (r2p, p2r) = session_keys(device_key, dh, &device_id, t)?;
        let (send_key, recv_key, send_dir, recv_dir) = match side {
            Side::Phone => (p2r, r2p, DIR_P2R, DIR_R2P),
            Side::Remote => (r2p, p2r, DIR_R2P, DIR_P2R),
        };
        Ok(Self {
            device_id,
            send_key,
            recv_key,
            send_dir,
            recv_dir,
            send_ctr: 0,
            recv_ctr: 0,
            failed: false,
        })
    }

    /// The device this channel authenticates.
    pub fn device_id(&self) -> DeviceId {
        self.device_id
    }

    fn nonce(dir: u8, ctr: u64) -> Nonce {
        let mut n = [0u8; 12];
        let c = ctr.to_be_bytes();
        for (i, slot) in n.iter_mut().enumerate() {
            *slot = match i {
                0 => dir,
                1..=3 => 0,
                _ => c.get(i - 4).copied().unwrap_or(0),
            };
        }
        Nonce::from(n)
    }

    fn aad(&self, ctr: u64) -> Vec<u8> {
        let mut aad = Vec::with_capacity(2 + 16 + COUNTER_LEN);
        aad.push(PROTOCOL_VERSION);
        aad.push(Kind::Data as u8);
        aad.extend_from_slice(&self.device_id.0);
        aad.extend_from_slice(&ctr.to_be_bytes());
        aad
    }

    /// Encrypts `plaintext` into a data-record body (`counter || ciphertext || tag`).
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if self.failed || self.send_ctr == u64::MAX {
            self.failed = true;
            return Err(CryptoError);
        }
        let ctr = self.send_ctr;
        let cipher =
            ChaCha20Poly1305::new_from_slice(self.send_key.as_bytes()).map_err(|_| CryptoError)?;
        let aad = self.aad(ctr);
        let ct = cipher
            .encrypt(
                &Self::nonce(self.send_dir, ctr),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| CryptoError)?;
        self.send_ctr = ctr.saturating_add(1);
        let mut body = Vec::with_capacity(COUNTER_LEN + ct.len());
        body.extend_from_slice(&ctr.to_be_bytes());
        body.extend_from_slice(&ct);
        Ok(body)
    }

    /// Authenticates and decrypts a data-record body. Accepts only the next expected
    /// counter. Any failure poisons the channel for good.
    pub fn open(&mut self, body: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        if self.failed {
            return Err(CryptoError);
        }
        let result = self.open_inner(body);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn open_inner(&mut self, body: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
        let (ctr_bytes, ct) = body.split_at_checked(COUNTER_LEN).ok_or(CryptoError)?;
        let ctr_arr: [u8; COUNTER_LEN] = ctr_bytes.try_into().map_err(|_| CryptoError)?;
        let ctr = u64::from_be_bytes(ctr_arr);
        // Constant time is not needed for the counter: it is public.
        if ctr != self.recv_ctr || ct.len() < TAG_LEN || self.recv_ctr == u64::MAX {
            return Err(CryptoError);
        }
        let cipher =
            ChaCha20Poly1305::new_from_slice(self.recv_key.as_bytes()).map_err(|_| CryptoError)?;
        let aad = self.aad(ctr);
        let pt = cipher
            .decrypt(
                &Self::nonce(self.recv_dir, ctr),
                Payload { msg: ct, aad: &aad },
            )
            .map_err(|_| CryptoError)?;
        self.recv_ctr = ctr.saturating_add(1);
        Ok(Zeroizing::new(pt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_sanity_check() {
        assert_eq!(check_entropy(&[0u8; 32]), Err(EntropyError::NotRandom));
        assert_eq!(check_entropy(&[7u8; 64]), Err(EntropyError::NotRandom));
        let pattern: Vec<u8> = (0..32).map(|i| (i % 4) as u8).collect();
        assert_eq!(check_entropy(&pattern), Err(EntropyError::NotRandom));
        assert_eq!(check_entropy(&[1, 2, 3]), Err(EntropyError::TooShort));
        let ok: Vec<u8> = (0..32u8).map(|i| i.wrapping_mul(37)).collect();
        assert_eq!(check_entropy(&ok), Ok(()));
    }

    #[test]
    fn repeated_entropy_still_gives_fresh_phone_values() {
        let e: Vec<u8> = (0..32u8).collect();
        let a = Ephemeral::phone(&e, 1, 5).unwrap();
        let b = Ephemeral::phone(&e, 2, 5).unwrap();
        let c = Ephemeral::phone(&e, 1, 6).unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.public, b.public);
        assert_ne!(a.nonce, c.nonce);
        // Nothing derived equals the raw entropy or the pairing secret.
        let s = pairing_secret_from_entropy(&e).unwrap();
        assert_ne!(&a.nonce[..], &e[..]);
        assert_ne!(&a.nonce[..20], &s.as_bytes()[..]);
    }

    #[test]
    fn low_order_peer_key_is_refused() {
        let e: Vec<u8> = (0..32u8).collect();
        let me = Ephemeral::remote(&e).unwrap();
        assert!(me.agree(&[0u8; 32]).is_err());
        let mut one = [0u8; 32];
        one[0] = 1;
        assert!(me.agree(&one).is_err());
    }
}
