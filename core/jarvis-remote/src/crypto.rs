//! The key schedule and the authenticated-encryption channel.
//!
//! Every primitive is a vetted RustCrypto implementation: HKDF-SHA256 (`hkdf`),
//! HMAC-SHA256 (`hmac`), ChaCha20-Poly1305 (`chacha20poly1305`) and constant-time
//! comparison (`subtle`). Nothing here is a custom primitive; this module only fixes the
//! labels, the inputs and their order. The labels are part of the protocol: changing one
//! breaks every paired device, which the known-answer test catches.
//!
//! ```text
//! pair_key     = HKDF(salt = "jarvis-remote v1 pair", ikm = S).expand("pair-auth", 32)
//! device_id    = HKDF(salt = Np || Nc, ikm = S).expand(lp("device-id"), 16)
//! device_key   = HKDF(salt = Np || Nc, ikm = S).expand(lp("device-key", device_id, name), 32)
//! auth_key     = HKDF(salt = "jarvis-remote v1 auth", ikm = device_key).expand("connect-auth", 32)
//! r2p || p2r   = HKDF(salt = Np || Nc, ikm = device_key).expand(lp("session", device_id), 64)
//! nonce        = direction (1 byte) || 0 0 0 || counter (u64 BE)
//! aad          = version || kind || device_id || counter (u64 BE)
//! ```
//!
//! `lp(..)` is a length-prefixed concatenation (u32 BE length before each part), so no two
//! different input lists can produce the same bytes. `S` is the pairing secret, `Np`/`Nc`
//! the phone's and the remote's 32-byte connection nonces.

use std::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::codec::{DeviceId, Kind, PROTOCOL_VERSION};

/// Length of a connection nonce, in bytes.
pub const NONCE_LEN: usize = 32;
/// Length of a handshake MAC (HMAC-SHA256), in bytes.
pub const MAC_LEN: usize = 32;
/// Length of the AEAD tag, in bytes.
pub const TAG_LEN: usize = 16;
/// Length of the pairing secret, in bytes (160 bits).
pub const PAIRING_SECRET_LEN: usize = 20;

/// Salt for the pairing-authentication key.
const PAIR_SALT: &[u8] = b"jarvis-remote v1 pair";
/// Salt for the connect-authentication key.
const AUTH_SALT: &[u8] = b"jarvis-remote v1 auth";

/// Direction byte in the AEAD nonce: remote to phone.
const DIR_R2P: u8 = 0x52;
/// Direction byte in the AEAD nonce: phone to remote.
const DIR_P2R: u8 = 0x50;

/// A failure inside the cryptographic layer. Deliberately carries no detail: callers turn
/// every variant into the same opaque authentication failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("authentication failed")]
pub struct CryptoError;

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

fn salt_nonces(np: &[u8; NONCE_LEN], nc: &[u8; NONCE_LEN]) -> [u8; 2 * NONCE_LEN] {
    let mut s = [0u8; 2 * NONCE_LEN];
    for (dst, src) in s.iter_mut().zip(np.iter().chain(nc.iter())) {
        *dst = *src;
    }
    s
}

fn hkdf32(salt: &[u8], ikm: &[u8], info: &[u8]) -> Result<Secret32, CryptoError> {
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(salt), ikm)
        .expand(info, &mut out)
        .map_err(|_| CryptoError)?;
    Ok(Secret32(out))
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
pub fn derive_device_id(
    secret: &PairingSecret,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
) -> Result<DeviceId, CryptoError> {
    let mut out = [0u8; 16];
    Hkdf::<Sha256>::new(Some(&salt_nonces(np, nc)), secret.as_bytes())
        .expand(&lp(&[b"device-id"]), &mut out)
        .map_err(|_| CryptoError)?;
    Ok(DeviceId(out))
}

/// The long-term key shared by the phone and one paired remote.
pub fn derive_device_key(
    secret: &PairingSecret,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
    device_id: &DeviceId,
    name: &str,
) -> Result<Secret32, CryptoError> {
    hkdf32(
        &salt_nonces(np, nc),
        secret.as_bytes(),
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
    device_id: &DeviceId,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
) -> Result<(Secret32, Secret32), CryptoError> {
    let mut okm = [0u8; 64];
    Hkdf::<Sha256>::new(Some(&salt_nonces(np, nc)), device_key.as_bytes())
        .expand(&lp(&[b"session", &device_id.0]), &mut okm)
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
    okm.zeroize();
    Ok((Secret32(a), Secret32(b)))
}

fn pair_transcript(
    role: &[u8],
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
    name: &str,
    device_id: Option<&DeviceId>,
) -> Vec<u8> {
    let id: &[u8] = device_id.map_or(&[], |d| &d.0);
    lp(&[
        b"jarvis-remote v1 pair",
        role,
        &[PROTOCOL_VERSION],
        np,
        nc,
        name.as_bytes(),
        id,
    ])
}

fn connect_transcript(
    role: &[u8],
    device_id: &DeviceId,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
) -> Vec<u8> {
    lp(&[
        b"jarvis-remote v1 connect",
        role,
        &[PROTOCOL_VERSION],
        &device_id.0,
        np,
        nc,
    ])
}

/// The remote's pairing proof: it knows the secret, bound to both nonces and its name.
pub fn pair_client_mac(
    pair_key: &Secret32,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
    name: &str,
) -> Result<[u8; MAC_LEN], CryptoError> {
    hmac(pair_key, &pair_transcript(b"remote", np, nc, name, None))
}

/// Checks [`pair_client_mac`] in constant time.
pub fn verify_pair_client_mac(
    pair_key: &Secret32,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
    name: &str,
    tag: &[u8],
) -> Result<(), CryptoError> {
    hmac_verify(
        pair_key,
        &pair_transcript(b"remote", np, nc, name, None),
        tag,
    )
}

/// The phone's pairing proof, which also authenticates the assigned device id.
pub fn pair_phone_mac(
    pair_key: &Secret32,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
    name: &str,
    device_id: &DeviceId,
) -> Result<[u8; MAC_LEN], CryptoError> {
    hmac(
        pair_key,
        &pair_transcript(b"phone", np, nc, name, Some(device_id)),
    )
}

/// Checks [`pair_phone_mac`] in constant time.
pub fn verify_pair_phone_mac(
    pair_key: &Secret32,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
    name: &str,
    device_id: &DeviceId,
    tag: &[u8],
) -> Result<(), CryptoError> {
    hmac_verify(
        pair_key,
        &pair_transcript(b"phone", np, nc, name, Some(device_id)),
        tag,
    )
}

/// The remote's connect proof (it holds the device key), bound to both nonces.
pub fn connect_client_mac(
    auth_key: &Secret32,
    device_id: &DeviceId,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
) -> Result<[u8; MAC_LEN], CryptoError> {
    hmac(auth_key, &connect_transcript(b"remote", device_id, np, nc))
}

/// Checks [`connect_client_mac`] in constant time.
pub fn verify_connect_client_mac(
    auth_key: &Secret32,
    device_id: &DeviceId,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
    tag: &[u8],
) -> Result<(), CryptoError> {
    hmac_verify(
        auth_key,
        &connect_transcript(b"remote", device_id, np, nc),
        tag,
    )
}

/// The phone's connect proof (it holds the device key too).
pub fn connect_phone_mac(
    auth_key: &Secret32,
    device_id: &DeviceId,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
) -> Result<[u8; MAC_LEN], CryptoError> {
    hmac(auth_key, &connect_transcript(b"phone", device_id, np, nc))
}

/// Checks [`connect_phone_mac`] in constant time.
pub fn verify_connect_phone_mac(
    auth_key: &Secret32,
    device_id: &DeviceId,
    np: &[u8; NONCE_LEN],
    nc: &[u8; NONCE_LEN],
    tag: &[u8],
) -> Result<(), CryptoError> {
    hmac_verify(
        auth_key,
        &connect_transcript(b"phone", device_id, np, nc),
        tag,
    )
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
        device_id: DeviceId,
        np: &[u8; NONCE_LEN],
        nc: &[u8; NONCE_LEN],
    ) -> Result<Self, CryptoError> {
        let (r2p, p2r) = session_keys(device_key, &device_id, np, nc)?;
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
    pub fn open(&mut self, body: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if self.failed {
            return Err(CryptoError);
        }
        let result = self.open_inner(body);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn open_inner(&mut self, body: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let (ctr_bytes, ct) = body.split_at_checked(COUNTER_LEN).ok_or(CryptoError)?;
        let ctr_arr: [u8; COUNTER_LEN] = ctr_bytes.try_into().map_err(|_| CryptoError)?;
        let ctr = u64::from_be_bytes(ctr_arr);
        // Constant-time is not needed for the counter: it is public.
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
        Ok(pt)
    }
}
