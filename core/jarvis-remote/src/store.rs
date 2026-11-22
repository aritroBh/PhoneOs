//! Paired devices and their persistence.
//!
//! The JSON produced by [`DeviceStore::to_json`] contains every device's long-term key in
//! hex. Anyone holding it can impersonate those devices. The shell must store it in the
//! Keychain (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synced: the hub must
//! still read it while the phone is locked) and nowhere else: not in the app's documents,
//! not in logs, not in backups. The returned string is
//! zeroized on drop; serde's intermediate buffers while encoding and decoding are not
//! (serde_json offers no way to do that), nor are copies the shell makes.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::codec::{from_hex, to_hex_secret, DeviceId};
use crate::crypto::Secret32;
use crate::error::PersistError;
use crate::permissions::RemotePermissions;
use crate::text::is_format;

/// Most paired devices the phone keeps.
pub const MAX_DEVICES: usize = 8;
/// Longest device name, in bytes of UTF-8.
pub const MAX_DEVICE_NAME_BYTES: usize = 64;
/// Current schema version of the persisted device list. Version 2 added `nonce_lease`;
/// version 1 is not read (it was never shipped).
pub const DEVICE_STORE_VERSION: u32 = 2;

/// One paired remote device.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    /// Id assigned at pairing.
    pub id: DeviceId,
    /// Name the remote gave at pairing (cleaned).
    pub name: String,
    /// Long-term key shared with the remote.
    pub key: Secret32,
    /// What it may do.
    pub permissions: RemotePermissions,
    /// When it was paired (shell clock).
    pub paired_at_ms: u64,
}

impl fmt::Debug for DeviceRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceRecord")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("key", &self.key)
            .field("permissions", &self.permissions)
            .field("paired_at_ms", &self.paired_at_ms)
            .finish()
    }
}

/// A paired device without its key, for the shell's UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Id assigned at pairing.
    pub id: DeviceId,
    /// Name the remote gave at pairing.
    pub name: String,
    /// What it may do.
    pub permissions: RemotePermissions,
    /// When it was paired.
    pub paired_at_ms: u64,
}

impl DeviceRecord {
    /// The key-less view.
    pub fn info(&self) -> DeviceInfo {
        DeviceInfo {
            id: self.id,
            name: self.name.clone(),
            permissions: self.permissions,
            paired_at_ms: self.paired_at_ms,
        }
    }
}

/// The persisted hub state: paired devices in pairing order, and the phone-nonce lease.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceStore {
    /// The devices.
    pub devices: Vec<DeviceRecord>,
    /// Every connection counter below this value may already have been used; a restored
    /// hub starts counting here. See [`crate::RemoteHub`] on the nonce counter.
    pub nonce_lease: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreJson {
    schema_version: u32,
    nonce_lease: u64,
    devices: Vec<DeviceJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceJson {
    id: DeviceId,
    name: String,
    key: Zeroizing<String>,
    permissions: RemotePermissions,
    paired_at_ms: u64,
}

impl DeviceStore {
    /// Serializes the store, keys included (see the module docs: Keychain only).
    pub fn to_json(&self) -> Zeroizing<String> {
        let doc = StoreJson {
            schema_version: DEVICE_STORE_VERSION,
            nonce_lease: self.nonce_lease,
            devices: self
                .devices
                .iter()
                .map(|d| DeviceJson {
                    id: d.id,
                    name: d.name.clone(),
                    key: to_hex_secret(d.key.as_bytes()),
                    permissions: d.permissions,
                    paired_at_ms: d.paired_at_ms,
                })
                .collect(),
        };
        Zeroizing::new(serde_json::to_string(&doc).unwrap_or_else(|_| String::from("{}")))
    }

    /// Loads a store written by [`DeviceStore::to_json`]. Corrupt, unknown-version or
    /// inconsistent input is an error; nothing is partially loaded.
    pub fn from_json(json: &str) -> Result<Self, PersistError> {
        let probe: serde_json::Value =
            serde_json::from_str(json).map_err(|e| PersistError::corrupt(&e))?;
        let version = probe
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| PersistError::Corrupt("missing schema_version".into()))?;
        if version != u64::from(DEVICE_STORE_VERSION) {
            return Err(PersistError::UnsupportedVersion(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        let doc: StoreJson =
            serde_json::from_value(probe).map_err(|e| PersistError::corrupt(&e))?;
        let mut devices: Vec<DeviceRecord> = Vec::with_capacity(doc.devices.len());
        for d in doc.devices {
            let key_bytes: [u8; 32] = from_hex(&d.key)
                .and_then(|b| b.as_slice().try_into().ok())
                .ok_or_else(|| PersistError::Invalid("invalid key".into()))?;
            devices.push(DeviceRecord {
                id: d.id,
                name: d.name,
                key: Secret32::from_bytes(key_bytes),
                permissions: d.permissions,
                paired_at_ms: d.paired_at_ms,
            });
        }
        let store = Self {
            devices,
            nonce_lease: doc.nonce_lease,
        };
        store.validate()?;
        Ok(store)
    }

    /// Checks the invariants [`DeviceStore::from_json`] enforces: at most
    /// [`MAX_DEVICES`], unique ids, clean names within [`MAX_DEVICE_NAME_BYTES`].
    pub fn validate(&self) -> Result<(), PersistError> {
        if self.devices.len() > MAX_DEVICES {
            return Err(PersistError::Invalid("too many devices".into()));
        }
        for (i, d) in self.devices.iter().enumerate() {
            if self.devices.iter().take(i).any(|x| x.id == d.id) {
                return Err(PersistError::Invalid("duplicate device id".into()));
            }
            if d.name.is_empty()
                || d.name.len() > MAX_DEVICE_NAME_BYTES
                || d.name != clean_name(&d.name)
            {
                return Err(PersistError::Invalid("invalid device name".into()));
            }
        }
        Ok(())
    }
}

/// Cleans a device name: control and format characters removed, whitespace collapsed,
/// trimmed, cut to [`MAX_DEVICE_NAME_BYTES`] on a character boundary.
pub fn clean_name(name: &str) -> String {
    let mut out = String::new();
    let mut space = false;
    for c in name.chars() {
        if c.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if c.is_control() || is_format(c) {
            continue;
        }
        let extra = c.len_utf8() + usize::from(space);
        if out.len() + extra > MAX_DEVICE_NAME_BYTES {
            break;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Mac\u{202E}Book \n Pro "), "MacBook Pro");
        assert_eq!(clean_name(&"é".repeat(100)).len(), 64);
        assert_eq!(clean_name("\u{200B}"), "");
    }
}
