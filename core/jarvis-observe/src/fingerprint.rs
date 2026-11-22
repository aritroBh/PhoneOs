//! Stable identity hashing for elements (hand-written FNV-1a 64).

use std::collections::BTreeMap;

use jarvis_proto::Role;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Incremental FNV-1a 64 state.
#[derive(Clone, Copy)]
pub(crate) struct Fnv64(u64);

impl Fnv64 {
    pub(crate) fn new() -> Self {
        Fnv64(FNV_OFFSET)
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    /// Writes a field followed by a 0xFF separator (never valid UTF-8, so fields cannot blur).
    pub(crate) fn field(&mut self, s: &str) {
        self.write(s.as_bytes());
        self.write(&[0xFF]);
    }

    pub(crate) fn finish(self) -> u64 {
        self.0
    }
}

/// FNV-1a 64 of a byte string; stable across Rust versions and platforms.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = Fnv64::new();
    h.write(bytes);
    h.finish()
}

/// The identity key of an element: `i:<identifier>` when an identifier exists, else `l:<label>`.
pub fn identity_key(identifier: Option<&str>, label: &str) -> String {
    match identifier {
        Some(id) if !id.is_empty() => format!("i:{id}"),
        _ => format!("l:{label}"),
    }
}

fn base_state(role: &Role, key: &str, ancestors: &[&Role]) -> Fnv64 {
    let mut h = Fnv64::new();
    h.field(role.as_str());
    h.field(key);
    for a in ancestors {
        h.field(a.as_str());
    }
    h
}

fn with_ordinal(mut base: Fnv64, ordinal: u32) -> u64 {
    base.write(&ordinal.to_le_bytes());
    base.finish()
}

/// Fingerprint of a control: hash of role, identity `key` (see [`identity_key`]), the
/// ancestor role path (root first) and the ordinal among elements with identical
/// role/key/path. Independent of frame and screen index.
pub fn fingerprint(role: &Role, key: &str, ancestors: &[&Role], ordinal: u32) -> u64 {
    with_ordinal(base_state(role, key, ancestors), ordinal)
}

/// Hands out fingerprints in document order, counting identical siblings.
#[derive(Default)]
pub(crate) struct FingerprintAssigner {
    seen: BTreeMap<u64, u32>,
}

impl FingerprintAssigner {
    pub(crate) fn assign(&mut self, role: &Role, key: &str, ancestors: &[&Role]) -> u64 {
        let base = base_state(role, key, ancestors);
        let slot = self.seen.entry(base.finish()).or_insert(0);
        let ordinal = *slot;
        *slot = slot.saturating_add(1);
        with_ordinal(base, ordinal)
    }
}
