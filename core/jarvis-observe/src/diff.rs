//! Screen hashing (stuck detection) and before/after diffs.

use std::collections::BTreeSet;

use jarvis_proto::{CompactScreen, Element, Role};
use serde::{Deserialize, Serialize};

use crate::fingerprint::Fnv64;

/// What changed between two compact screens.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ScreenDiff {
    /// True when anything meaningful differs (see [`screen_hash`]) or elements came or went.
    pub changed: bool,
    /// Elements of `after` whose fingerprint is not in `before`.
    pub appeared: Vec<Element>,
    /// Elements of `before` whose fingerprint is not in `after`.
    pub disappeared: Vec<Element>,
    /// The foreground app differs.
    pub app_changed: bool,
}

/// Status-bar band: elements centred above this y (points) are clock/battery/signal chrome.
const STATUS_BAR_BOTTOM: f64 = 54.0;

/// True for status-bar chrome (static text, images and `Other` leaves in the top band),
/// whose clock and battery text must not count as screen changes. Controls, inputs and
/// everything below the band are always significant.
fn is_volatile(e: &Element) -> bool {
    matches!(e.role, Role::StaticText | Role::Image | Role::Other(_))
        && e.frame.center().y < STATUS_BAR_BOTTOM
}

/// Hash of what a screen means, for stuck detection and verification.
///
/// Covers the app, keyboard visibility and, per element in order: role, label, value,
/// enabled and selected. It ignores frames, indices, focus and status-bar chrome (clock,
/// battery); values of sliders, fields, switches and ordinary labels always count.
pub fn screen_hash(screen: &CompactScreen) -> u64 {
    let mut h = Fnv64::new();
    h.field(&screen.app_bundle_id);
    h.write(&[u8::from(screen.keyboard_visible)]);
    for e in screen.elements.iter().filter(|e| !is_volatile(e)) {
        h.field(e.role.as_str());
        h.field(&e.label);
        match &e.value {
            Some(v) => {
                h.write(&[1]);
                h.field(v);
            }
            None => h.write(&[0]),
        }
        h.write(&[u8::from(e.enabled), u8::from(e.selected)]);
    }
    h.finish()
}

/// Compares two screens by element fingerprint.
pub fn diff(before: &CompactScreen, after: &CompactScreen) -> ScreenDiff {
    let before_fp: BTreeSet<u64> = before
        .elements
        .iter()
        .filter(|e| !is_volatile(e))
        .map(|e| e.fingerprint)
        .collect();
    let after_fp: BTreeSet<u64> = after
        .elements
        .iter()
        .filter(|e| !is_volatile(e))
        .map(|e| e.fingerprint)
        .collect();
    let appeared: Vec<Element> = after
        .elements
        .iter()
        .filter(|e| !is_volatile(e) && !before_fp.contains(&e.fingerprint))
        .cloned()
        .collect();
    let disappeared: Vec<Element> = before
        .elements
        .iter()
        .filter(|e| !is_volatile(e) && !after_fp.contains(&e.fingerprint))
        .cloned()
        .collect();
    let app_changed = before.app_bundle_id != after.app_bundle_id;
    let changed = app_changed
        || !appeared.is_empty()
        || !disappeared.is_empty()
        || screen_hash(before) != screen_hash(after);
    ScreenDiff {
        changed,
        appeared,
        disappeared,
        app_changed,
    }
}
