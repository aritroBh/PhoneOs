//! Paired devices and their persistence.
//!
//! The JSON produced by [`DeviceStore::to_json`] contains every device's long-term key in
//! hex. Anyone holding it can impersonate those devices. The shell must store it in the
//! Keychain (`kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, not synced) and nowhere
//! else: not in the app's documents, not in logs, not in backups.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::codec::{from_hex, to_hex, DeviceId};
use crate::crypto::Secret32;
use crate::error::PersistError;
use crate::permissions::RemotePermissions;

/// Most paired devices the phone keeps.
pub const MAX_DEVICES: usize = 8;
/// Longest device name, in bytes of UTF-8.
pub const MAX_DEVICE_NAME_BYTES: usize = 64;
/// Current schema version of the persisted device list.
pub const DEVICE_STORE_VERSION: u32 = 1;

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

/// The persisted list of paired devices, in pairing order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceStore {
    /// The devices.
    pub devices: Vec<DeviceRecord>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreJson {
    schema_version: u32,
    devices: Vec<DeviceJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceJson {
    id: DeviceId,
    name: String,
    key: String,
    permissions: RemotePermissions,
    paired_at_ms: u64,
}

impl DeviceStore {
    /// Serializes the list, keys included (see the module docs: Keychain only).
    pub fn to_json(&self) -> String {
        let doc = StoreJson {
            schema_version: DEVICE_STORE_VERSION,
            devices: self
                .devices
                .iter()
                .map(|d| DeviceJson {
                    id: d.id,
                    name: d.name.clone(),
                    key: to_hex(d.key.as_bytes()),
                    permissions: d.permissions,
                    paired_at_ms: d.paired_at_ms,
                })
                .collect(),
        };
        serde_json::to_string(&doc).unwrap_or_else(|_| String::from("{}"))
    }

    /// Loads a list written by [`DeviceStore::to_json`]. Corrupt, unknown-version or
    /// inconsistent input is an error; nothing is partially loaded.
    pub fn from_json(json: &str) -> Result<Self, PersistError> {
        let probe: serde_json::Value =
            serde_json::from_str(json).map_err(|e| PersistError::Corrupt(e.to_string()))?;
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
            serde_json::from_value(probe).map_err(|e| PersistError::Corrupt(e.to_string()))?;
        if doc.devices.len() > MAX_DEVICES {
            return Err(PersistError::Invalid("too many devices".into()));
        }
        let mut devices: Vec<DeviceRecord> = Vec::with_capacity(doc.devices.len());
        for d in doc.devices {
            if devices.iter().any(|x| x.id == d.id) {
                return Err(PersistError::Invalid("duplicate device id".into()));
            }
            if d.name.len() > MAX_DEVICE_NAME_BYTES || d.name != clean_name(&d.name) {
                return Err(PersistError::Invalid("invalid device name".into()));
            }
            let key_bytes: [u8; 32] = from_hex(&d.key)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| PersistError::Invalid("invalid key".into()))?;
            devices.push(DeviceRecord {
                id: d.id,
                name: d.name,
                key: Secret32::from_bytes(key_bytes),
                permissions: d.permissions,
                paired_at_ms: d.paired_at_ms,
            });
        }
        Ok(Self { devices })
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

/// Zero-width, bidi and similar invisible format characters.
pub(crate) fn is_format(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}'
        | '\u{E0000}'..='\u{E007F}')
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
