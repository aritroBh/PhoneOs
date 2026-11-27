//! Known-answer test for protocol version 2: entropy derivations, X25519, the key
//! schedule, the handshake MACs and frames in both directions.
//!
//! The expected values were produced by this crate and checked against an independent
//! implementation (Python: `hmac`/`hashlib` for HKDF and HMAC, the `cryptography` package
//! for X25519 and ChaCha20-Poly1305) written from the description in `crypto.rs`; all 19
//! matched. If this test fails, the wire protocol changed: every paired device would stop
//! working.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_remote::codec::{from_hex, to_hex};
use jarvis_remote::crypto::*;

const NAME: &str = "Test Mac";

struct Fixture {
    phone: Ephemeral,
    remote: Ephemeral,
    t: Transcript,
    s: PairingSecret,
}

fn fixture() -> Fixture {
    let phone_entropy: Vec<u8> = (0x40u8..0x60).collect();
    let remote_entropy: Vec<u8> = (0x80u8..0xa0).collect();
    let phone = Ephemeral::phone(&phone_entropy, 7, 1000).unwrap();
    let remote = Ephemeral::remote(&remote_entropy).unwrap();
    let t = Transcript {
        np: phone.nonce,
        nc: remote.nonce,
        ep: phone.public,
        er: remote.public,
    };
    Fixture {
        phone,
        remote,
        t,
        s: PairingSecret::from_bytes(core::array::from_fn(|i| i as u8)),
    }
}

#[test]
fn entropy_derivations_and_x25519_known_answers() {
    let phone_entropy: Vec<u8> = (0x40u8..0x60).collect();
    let se = pairing_secret_from_entropy(&phone_entropy).unwrap();
    assert_eq!(
        to_hex(se.as_bytes()),
        "c5fb185ccdf12d44dbb57d29bd140a300f58132a"
    );
    let f = fixture();
    assert_eq!(
        to_hex(&f.phone.nonce),
        "87d9f39f51e962aab53b5e1c955176d3a7561a0308e99e86ae4d9382764ef68d"
    );
    assert_eq!(
        to_hex(&f.phone.public),
        "c0b632573dba7473ff3e6115488cd8c15e04e80349e9a22ccbc0eff830389b15"
    );
    assert_eq!(
        to_hex(&f.remote.nonce),
        "cfa582db51ab9d1831ba257ff83fa785eeab9a80574e51ae3061dd01857df609"
    );
    assert_eq!(
        to_hex(&f.remote.public),
        "f0b83de25fbe3cf6746e2b42b1274eee8af8e8c460996723de3bef97e0c2a443"
    );
    let dh = f.phone.agree(&f.remote.public).unwrap();
    assert_eq!(*dh, *f.remote.agree(&f.phone.public).unwrap());
    assert_eq!(
        to_hex(&*dh),
        "9cb4693e7696b3b11e0d0d17ab5074f447778e1ee46eb58ffff4c217fe58124f"
    );
    // The secret shares nothing with the public nonce derived from the same entropy.
    assert!(!to_hex(&f.phone.nonce).contains(&to_hex(&se.as_bytes()[..4])));
}

#[test]
fn key_schedule_known_answers() {
    let f = fixture();
    let dh = f.phone.agree(&f.remote.public).unwrap();
    let pk = pair_key(&f.s).unwrap();
    assert_eq!(
        to_hex(pk.as_bytes()),
        "c0da69c681c81fbc2db4241d4310432660020a962427165ea3b6d06a8c385b87"
    );
    let id = derive_device_id(&f.s, &f.t).unwrap();
    assert_eq!(id.to_hex(), "8d813a2b3d5cbf946f462d2f6d11fbe1");
    let dk = derive_device_key(&f.s, &dh, &f.t, &id, NAME).unwrap();
    assert_eq!(
        to_hex(dk.as_bytes()),
        "be86b968a9e5a1edc745734dc880e6798aa119d887a04d85c607a8accd84bab4"
    );
    let ak = auth_key(&dk).unwrap();
    assert_eq!(
        to_hex(ak.as_bytes()),
        "962aeb955141315c15b06375c5c839c7d97b3847f71a240b56688a15fc2c3015"
    );
    let (r2p, p2r) = session_keys(&dk, &dh, &id, &f.t).unwrap();
    assert_eq!(
        to_hex(r2p.as_bytes()),
        "9892e4efa156a49d541adaa172cac4f915b569f47fc06ac875bd2a0fd2731de5"
    );
    assert_eq!(
        to_hex(p2r.as_bytes()),
        "8eab86be35d54dfbb53572b37a8a024cbab5d7ac3394b70ff77b319c37fb6440"
    );
    // Forward secrecy, structurally: without the DH output the session keys differ.
    let (r2p_no_dh, _) = session_keys(&dk, &[0u8; 32], &id, &f.t).unwrap();
    assert_ne!(r2p_no_dh, r2p);
}

#[test]
fn handshake_mac_known_answers() {
    let f = fixture();
    let dh = f.phone.agree(&f.remote.public).unwrap();
    let pk = pair_key(&f.s).unwrap();
    let id = derive_device_id(&f.s, &f.t).unwrap();
    let ak = auth_key(&derive_device_key(&f.s, &dh, &f.t, &id, NAME).unwrap()).unwrap();
    let pcm = pair_client_mac(&pk, &f.t, NAME).unwrap();
    assert_eq!(
        to_hex(&pcm),
        "76b4b6a8287d8aa81b9bd1222f3e8272ac7480a64ed917a2a26d6c25d37c3c82"
    );
    assert!(verify_pair_client_mac(&pk, &f.t, NAME, &pcm).is_ok());
    assert!(verify_pair_client_mac(&pk, &f.t, "Test Mad", &pcm).is_err());
    let swapped = Transcript {
        ep: f.t.er,
        er: f.t.ep,
        ..f.t
    };
    assert!(verify_pair_client_mac(&pk, &swapped, NAME, &pcm).is_err());
    let ppm = pair_phone_mac(&pk, &f.t, NAME, &id).unwrap();
    assert_eq!(
        to_hex(&ppm),
        "bf17b9932fea3017419b12d08aab6883ed2e1c0fbbb2f7df4b643f8ed22161b9"
    );
    assert!(verify_pair_phone_mac(&pk, &f.t, NAME, &id, &ppm).is_ok());
    let ccm = connect_client_mac(&ak, &id, &f.t).unwrap();
    assert_eq!(
        to_hex(&ccm),
        "1f05146113b0259dec892ba48695a6835326e50b083c2217516a118a38a36dd5"
    );
    let cpm = connect_phone_mac(&ak, &id, &f.t).unwrap();
    assert_eq!(
        to_hex(&cpm),
        "f386da4a8c27d4fc0e76025b720ff373b2ef2572b07402bec552d50b08f80a49"
    );
    assert!(verify_connect_phone_mac(&ak, &id, &f.t, &cpm).is_ok());
    // The two roles' MACs differ, so one cannot be reflected as the other.
    assert_ne!(pcm, ppm);
    assert_ne!(ccm, cpm);
    assert!(verify_connect_client_mac(&ak, &id, &f.t, &cpm).is_err());
}

#[test]
fn frame_known_answers_in_both_directions() {
    let f = fixture();
    let dh = f.phone.agree(&f.remote.public).unwrap();
    let id = derive_device_id(&f.s, &f.t).unwrap();
    let dk = derive_device_key(&f.s, &dh, &f.t, &id, NAME).unwrap();
    let mut remote = Channel::new(Side::Remote, &dk, &dh, id, &f.t).unwrap();
    let mut phone = Channel::new(Side::Phone, &dk, &dh, id, &f.t).unwrap();
    let f0 = remote.seal(br#"{"type":"ping"}"#).unwrap();
    let f1 = remote.seal(br#"{"type":"ping"}"#).unwrap();
    assert_eq!(
        to_hex(&f0),
        "0000000000000000dfcda4cc57a9a958f5e05648c9774b17d8badc9afe9e9848660476ce37b234"
    );
    assert_eq!(
        to_hex(&f1),
        "000000000000000180f1a1f8837ebc3887df8a065e94efecc40bb3ec09ffa96637627d8111c033"
    );
    // Phone → remote, direction byte 0x50.
    let g0 = phone.seal(br#"{"type":"pong"}"#).unwrap();
    assert_eq!(
        to_hex(&g0),
        "00000000000000002cba29c427bd3069908a44d871a8679a901242b2a14b80cd7cda7eba9ee135"
    );
    assert_eq!(DIR_P2R, 0x50);
    assert_eq!(DIR_R2P, 0x52);
    assert_eq!(phone.open(&f0).unwrap().as_slice(), br#"{"type":"ping"}"#);
    assert_eq!(phone.open(&f1).unwrap().as_slice(), br#"{"type":"ping"}"#);
    assert_eq!(remote.open(&g0).unwrap().as_slice(), br#"{"type":"pong"}"#);
    // A phone frame does not open as a remote frame (separate keys and directions).
    let mut phone2 = Channel::new(Side::Phone, &dk, &dh, id, &f.t).unwrap();
    assert!(phone2.open(&g0).is_err());
    // And a failure is final, even for the right frame afterwards.
    let mut phone3 = Channel::new(Side::Phone, &dk, &dh, id, &f.t).unwrap();
    assert!(phone3.open(&from_hex("00").unwrap()).is_err());
    assert!(phone3.open(&f0).is_err());
}
