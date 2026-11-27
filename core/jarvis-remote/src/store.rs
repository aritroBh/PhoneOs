//! Paired devices and their persistence.
//!
//! The JSON produced by [`DeviceStore::to_json`] contains every device's long-term key in
//! hex. Anyone holding it can impersonate those devices. The shell must store it in the
//! Keychain (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synced: the hub must
//! still read it while the phone is locked) and nowhere else: not in the app's documents,
//! not in logs, not in backups. The returned string is
//! zeroized on drop; serde's intermediate buffers while encoding and decoding are not
//! (serde_json offers no way to do that), nor are copies the shell makes.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::codec::{from_hex, to_hex_secret, DeviceId};
use crate::crypto::Secret32;
use crate::error::PersistError;
use crate::permissions::RemotePermissions;
use crate::text::{is_blank, is_format};

/// Most paired devices the phone keeps.
pub const MAX_DEVICES: usize = 8;
/// Longest device name, in bytes of UTF-8.
pub const MAX_DEVICE_NAME_BYTES: usize = 64;
/// Largest lease (request ids and connection counters) a store may hold. Leases grow by
/// one small block per persist, so a real store stays far below this (2^53, about 10^16);
/// a store above it is refused at load like any corrupt one, and a hub whose ids reach it
/// stops handing out ids rather than repeating one.
pub const MAX_LEASE: u64 = 1 << 53;
/// Current schema version of the persisted device list. Version 2 added `nonce_lease`;
/// version 1 is not read (it was never shipped). Later additions are optional fields
/// written only when used (`watch_screen`, `pending_check`, `request_lease`), so a store
/// that does not use them is byte for byte what older versions wrote.
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

/// A device paired but not yet confirmed by the owner on the phone (see
/// [`crate::RemoteHub::confirm_device`]): it holds no permission at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRecord {
    /// The device (its permissions are all off while pending).
    pub record: DeviceRecord,
    /// The check string both sides showed after the pair exchange ("123 456").
    pub check: String,
}

/// Everything [`crate::RemoteHub::to_json`] persists, as [`DeviceStore::from_json_full`]
/// reads it back.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StoredHub {
    /// Confirmed devices and the nonce lease.
    pub store: DeviceStore,
    /// Devices holding the `watch_screen` permission.
    pub watch: BTreeSet<DeviceId>,
    /// Devices waiting for the owner's confirmation.
    pub pending: Vec<PendingRecord>,
    /// Every confirmation request id below this value may already have been used; a
    /// restored hub starts numbering here (0 in stores written before it existed).
    pub request_lease: u64,
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
    /// See [`StoredHub::request_lease`]. Absent in stores written before it existed.
    #[serde(default, skip_serializing_if = "is_zero")]
    request_lease: u64,
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
    /// The `watch_screen` permission (Live view over remote control). Absent in records
    /// written before it existed, which load with it off; written only when on, so a
    /// store without it is byte for byte what older versions wrote.
    #[serde(default, skip_serializing_if = "is_false")]
    watch_screen: bool,
    /// Present while the device waits for the owner's confirmation on the phone: its
    /// check string. Absent in records written before the confirm step existed, which
    /// load as confirmed (they were paired under the rules of their time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_check: Option<String>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Whether `s` is a check string as [`crate::crypto::confirm_check`] formats it.
fn is_check_string(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7
        && b.iter().enumerate().all(|(i, c)| {
            if i == 3 {
                *c == b' '
            } else {
                c.is_ascii_digit()
            }
        })
}

impl DeviceStore {
    /// Serializes the store, keys included (see the module docs: Keychain only).
    pub fn to_json(&self) -> Zeroizing<String> {
        self.to_json_with_watch(&BTreeSet::new())
    }

    /// [`DeviceStore::to_json`] plus the devices holding the `watch_screen` permission
    /// (kept beside [`RemotePermissions`], see [`crate::RemoteHub::watch_screen`]).
    pub fn to_json_with_watch(&self, watch_screen: &BTreeSet<DeviceId>) -> Zeroizing<String> {
        Self::to_json_full(&StoredHub {
            store: self.clone(),
            watch: watch_screen.clone(),
            pending: Vec::new(),
            request_lease: 0,
        })
    }

    /// Serializes everything the hub persists (see [`StoredHub`]), keys included.
    /// Pending devices follow the confirmed ones and carry their check string.
    pub fn to_json_full(hub: &StoredHub) -> Zeroizing<String> {
        let json = |d: &DeviceRecord, pending_check: Option<String>| DeviceJson {
            id: d.id,
            name: d.name.clone(),
            key: to_hex_secret(d.key.as_bytes()),
            permissions: d.permissions,
            paired_at_ms: d.paired_at_ms,
            watch_screen: pending_check.is_none() && hub.watch.contains(&d.id),
            pending_check,
        };
        let doc = StoreJson {
            schema_version: DEVICE_STORE_VERSION,
            nonce_lease: hub.store.nonce_lease,
            request_lease: hub.request_lease,
            devices: hub
                .store
                .devices
                .iter()
                .map(|d| json(d, None))
                .chain(
                    hub.pending
                        .iter()
                        .map(|p| json(&p.record, Some(p.check.clone()))),
                )
                .collect(),
        };
        Zeroizing::new(serde_json::to_string(&doc).unwrap_or_else(|_| String::from("{}")))
    }

    /// Loads a store written by [`DeviceStore::to_json`]. Corrupt, unknown-version or
    /// inconsistent input is an error; nothing is partially loaded.
    pub fn from_json(json: &str) -> Result<Self, PersistError> {
        Self::from_json_with_watch(json).map(|(store, _)| store)
    }

    /// [`DeviceStore::from_json`] plus the devices holding the `watch_screen` permission
    /// (none for records written before it existed). Devices still waiting for the
    /// owner's confirmation are left out (never loaded as confirmed).
    pub fn from_json_with_watch(json: &str) -> Result<(Self, BTreeSet<DeviceId>), PersistError> {
        Self::from_json_full(json).map(|h| (h.store, h.watch))
    }

    /// Loads everything [`DeviceStore::to_json_full`] wrote. Records without a
    /// `pending_check` (every record written before the confirm step) are confirmed
    /// devices; records with one stay pending. Validated like [`DeviceStore::from_json`],
    /// across confirmed and pending devices together.
    pub fn from_json_full(json: &str) -> Result<StoredHub, PersistError> {
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
        if doc.request_lease > MAX_LEASE {
            return Err(PersistError::Invalid("unreasonable lease".into()));
        }
        let mut devices: Vec<DeviceRecord> = Vec::with_capacity(doc.devices.len());
        let mut pending: Vec<PendingRecord> = Vec::new();
        let mut watch = BTreeSet::new();
        for d in doc.devices {
            let key_bytes: [u8; 32] = from_hex(&d.key)
                .and_then(|b| b.as_slice().try_into().ok())
                .ok_or_else(|| PersistError::Invalid("invalid key".into()))?;
            let record = DeviceRecord {
                id: d.id,
                name: migrated_name(d.name, &d.id),
                key: Secret32::from_bytes(key_bytes),
                permissions: d.permissions,
                paired_at_ms: d.paired_at_ms,
            };
            match d.pending_check {
                Some(check) => {
                    if !is_check_string(&check) {
                        return Err(PersistError::Invalid("invalid check string".into()));
                    }
                    // A pending device holds nothing, whatever the record says.
                    pending.push(PendingRecord {
                        record: DeviceRecord {
                            permissions: RemotePermissions::NONE,
                            ..record
                        },
                        check,
                    });
                }
                None => {
                    if d.watch_screen {
                        watch.insert(d.id);
                    }
                    devices.push(record);
                }
            }
        }
        let store = Self {
            devices,
            nonce_lease: doc.nonce_lease,
        };
        store.validate()?;
        let all = Self {
            devices: store
                .devices
                .iter()
                .cloned()
                .chain(pending.iter().map(|p| p.record.clone()))
                .collect(),
            nonce_lease: 0,
        };
        all.validate()?;
        Ok(StoredHub {
            store,
            watch,
            pending,
            request_lease: doc.request_lease,
        })
    }

    /// Checks the invariants [`DeviceStore::from_json`] enforces: at most
    /// [`MAX_DEVICES`], unique ids, clean names within [`MAX_DEVICE_NAME_BYTES`], a lease
    /// of at most [`MAX_LEASE`].
    pub fn validate(&self) -> Result<(), PersistError> {
        if self.nonce_lease > MAX_LEASE {
            return Err(PersistError::Invalid("unreasonable lease".into()));
        }
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
/// trimmed, cut to [`MAX_DEVICE_NAME_BYTES`] on a character boundary. A name with no
/// visible character (only blanks: whitespace, fillers, variation selectors, the braille
/// blank, every default-ignorable code point) cleans to the empty name, which pairing and
/// renaming refuse.
pub fn clean_name(name: &str) -> String {
    let out = clean_name_with(name, is_format);
    if out.chars().all(is_blank) {
        return String::new();
    }
    out
}

fn clean_name_with(name: &str, is_format: fn(char) -> bool) -> String {
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

/// The format characters names were cleaned of before the full Cf table (`adfce96` and
/// earlier). Only for [`migrated_name`].
fn legacy_is_format(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{206F}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}'
        | '\u{E0000}'..='\u{E007F}')
}

/// A stored name that was clean under the earlier, shorter list of format characters is
/// cleaned with the full table instead of failing the store's validation (which would
/// drop every pairing on the update). One that cleans to nothing (it was only characters
/// that draw nothing) becomes [`placeholder_name`]: the device keeps its key and
/// permissions, and the other devices load (second break-it review F5). Any other name is
/// returned as it is and validated.
fn migrated_name(name: String, id: &DeviceId) -> String {
    // An empty name was never written (it was always refused): it stays refused.
    if !name.is_empty() && clean_name_with(&name, legacy_is_format) == name {
        let cleaned = clean_name(&name);
        if cleaned.is_empty() {
            return placeholder_name(id);
        }
        return cleaned;
    }
    name
}

/// The name a stored device whose name draws nothing loads with: "Device " and the first
/// six hex digits of its id ("Device ab12cd"). The owner can rename it.
pub fn placeholder_name(id: &DeviceId) -> String {
    let hex = id.to_hex();
    format!("Device {}", hex.get(..6).unwrap_or(&hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_stored_under_the_earlier_list_loads_cleaned() {
        let json = r#"{"schema_version":2,"nonce_lease":0,"devices":[{"id":"00112233445566778899aabbccddeeff","name":"Owner\ud834\udd73 Mac","key":"0000000000000000000000000000000000000000000000000000000000000000","permissions":{"start_tasks":true,"see_progress":true,"see_screen_text":false,"approve":false},"paired_at_ms":5}]}"#;
        let store = DeviceStore::from_json(json).unwrap();
        assert_eq!(store.devices[0].name, "Owner Mac");
        // Names the earlier list refused stay refused.
        let dirty = json.replace("Owner\\ud834\\udd73 Mac", "Owner\\u202e Mac");
        assert_ne!(dirty, json);
        assert!(DeviceStore::from_json(&dirty).is_err());
        // A name of nothing but newly listed characters loads with a placeholder derived
        // from the id (second break-it review F5; it was refused, and the whole store with
        // it).
        let empty = json.replace("Owner\\ud834\\udd73 Mac", "\\ud834\\udd73");
        assert_eq!(
            DeviceStore::from_json(&empty).unwrap().devices[0].name,
            "Device 001122"
        );
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Mac\u{202E}Book \n Pro "), "MacBook Pro");
        assert_eq!(clean_name(&"é".repeat(100)).len(), 64);
        assert_eq!(clean_name("\u{200B}"), "");
    }
}
