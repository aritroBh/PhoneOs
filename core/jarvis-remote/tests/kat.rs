//! Known-answer test for the key schedule, the handshake MACs and the frame format.
//!
//! The expected values were produced by this crate and checked against an independent
//! implementation (Python: `hmac`/`hashlib` for HKDF and HMAC, the `cryptography`
//! package for ChaCha20-Poly1305) written from the description in `crypto.rs`. If this
//! test fails, the wire protocol changed: every paired device would stop working.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_remote::codec::{from_hex, to_hex};
use jarvis_remote::crypto::*;

const NAME: &str = "Test Mac";
const NP: [u8; 32] = [0x11; 32];
const NC: [u8; 32] = [0x22; 32];

fn secret() -> PairingSecret {
    PairingSecret::from_bytes(core::array::from_fn(|i| i as u8))
}

#[test]
fn key_schedule_known_answers() {
    let s = secret();
    let pk = pair_key(&s).unwrap();
    assert_eq!(
        to_hex(pk.as_bytes()),
        "6db7ace8c75d6181c49328f3a401ee7e825124bdcecec424a6b7685a24f7e755"
    );
    let id = derive_device_id(&s, &NP, &NC).unwrap();
    assert_eq!(id.to_hex(), "02e34c519d81580fedb7d79b937a53a3");
    let dk = derive_device_key(&s, &NP, &NC, &id, NAME).unwrap();
    assert_eq!(
        to_hex(dk.as_bytes()),
        "1919bcbae907f3defa8922d89bdf73bbdcd33584985438b9121ab29ccb7236bf"
    );
    let ak = auth_key(&dk).unwrap();
    assert_eq!(
        to_hex(ak.as_bytes()),
        "cd6fd810ca7c060b6194a744e86d31277f2ea511d85a789ce22f6c0b4597dd22"
    );
    let (r2p, p2r) = session_keys(&dk, &id, &NP, &NC).unwrap();
    assert_eq!(
        to_hex(r2p.as_bytes()),
        "77b7f0276be899cef08a17ffd870e10b379bde9a57db1cd62346f4b2a140ea1c"
    );
    assert_eq!(
        to_hex(p2r.as_bytes()),
        "88552e37874bd623b40ea6d14dd38e327094ac32b24d0b92c5d33805aae729ab"
    );
}

#[test]
fn handshake_mac_known_answers() {
    let s = secret();
    let pk = pair_key(&s).unwrap();
    let id = derive_device_id(&s, &NP, &NC).unwrap();
    let ak = auth_key(&derive_device_key(&s, &NP, &NC, &id, NAME).unwrap()).unwrap();
    let pcm = pair_client_mac(&pk, &NP, &NC, NAME).unwrap();
    assert_eq!(
        to_hex(&pcm),
        "cf4ebed32a6b716a0ccf7e166927e850a56a067e46cf4036c3dfe1bca270967e"
    );
    assert!(verify_pair_client_mac(&pk, &NP, &NC, NAME, &pcm).is_ok());
    assert!(verify_pair_client_mac(&pk, &NP, &NC, "Test Mad", &pcm).is_err());
    let ppm = pair_phone_mac(&pk, &NP, &NC, NAME, &id).unwrap();
    assert_eq!(
        to_hex(&ppm),
        "9a2d19e66a2abb016fe52a1c4989a5906dfd1a2930a15a88cf7cac09b681b327"
    );
    assert!(verify_pair_phone_mac(&pk, &NP, &NC, NAME, &id, &ppm).is_ok());
    let ccm = connect_client_mac(&ak, &id, &NP, &NC).unwrap();
    assert_eq!(
        to_hex(&ccm),
        "a50892030d8c12d942ae05e277802e2abdb5046ef1ff5444818cc6ba1f72b12e"
    );
    assert!(verify_connect_client_mac(&ak, &id, &NC, &NP, &ccm).is_err());
    let cpm = connect_phone_mac(&ak, &id, &NP, &NC).unwrap();
    assert_eq!(
        to_hex(&cpm),
        "1bad85b54810f208f154897acf2ce7fbbe73b3ee79d7ae897e282bcf6346ce70"
    );
    // The two roles' MACs differ, so one cannot be reflected as the other.
    assert_ne!(pcm, ppm);
    assert_ne!(ccm, cpm);
}

#[test]
fn frame_known_answers() {
    let s = secret();
    let id = derive_device_id(&s, &NP, &NC).unwrap();
    let dk = derive_device_key(&s, &NP, &NC, &id, NAME).unwrap();
    let mut remote = Channel::new(Side::Remote, &dk, id, &NP, &NC).unwrap();
    let mut phone = Channel::new(Side::Phone, &dk, id, &NP, &NC).unwrap();
    let f0 = remote.seal(br#"{"type":"ping"}"#).unwrap();
    let f1 = remote.seal(br#"{"type":"ping"}"#).unwrap();
    assert_eq!(
        to_hex(&f0),
        "00000000000000003479c27a0203de568f20dfe8731785d33f4cc9de2d674c2256a268180ed646"
    );
    assert_eq!(
        to_hex(&f1),
        "0000000000000001a1a5530351b0cf14384fc23c71e776e8f22c9965c1dccaeda3110f95b9189d"
    );
    assert_eq!(phone.open(&f0).unwrap(), br#"{"type":"ping"}"#);
    assert_eq!(phone.open(&f1).unwrap(), br#"{"type":"ping"}"#);
    // A frame sealed by the phone does not open as one from the remote (separate keys).
    let mut phone2 = Channel::new(Side::Phone, &dk, id, &NP, &NC).unwrap();
    let echo = phone2.seal(br#"{"type":"ping"}"#).unwrap();
    let mut phone3 = Channel::new(Side::Phone, &dk, id, &NP, &NC).unwrap();
    assert!(phone3.open(&echo).is_err());
    // And a failure is final, even for the right frame afterwards.
    let mut phone4 = Channel::new(Side::Phone, &dk, id, &NP, &NC).unwrap();
    assert!(phone4.open(&from_hex("00").unwrap()).is_err());
    assert!(phone4.open(&f0).is_err());
}
