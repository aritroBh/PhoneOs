//! Screen hashing (stuck detection) and before/after diffs.

use std::collections::BTreeSet;

use jarvis_proto::{CompactScreen, Element};
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

fn is_time_token(token: &str) -> bool {
    let t = token.to_ascii_lowercase();
    let t = ["a.m.", "p.m.", "am", "pm"]
        .iter()
        .find_map(|s| t.strip_suffix(s).map(str::to_owned))
        .unwrap_or(t);
    let parts: Vec<&str> = t.split(':').collect();
    let digits =
        |s: &str, lens: &[usize]| lens.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    match parts.as_slice() {
        [h, m] => digits(h, &[1, 2]) && digits(m, &[2]),
        [h, m, s] => digits(h, &[1, 2]) && digits(m, &[2]) && digits(s, &[2]),
        _ => false,
    }
}

fn is_percent_token(token: &str) -> bool {
    token
        .strip_suffix('%')
        .is_some_and(|n| !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()))
}

fn is_meridiem(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "am" | "pm" | "a.m." | "p.m."
    )
}

/// Replaces clock times (`9:41`, `10:02 PM`) and percentages (`85%`) with `#` so that
/// ticking clocks and battery levels do not count as screen changes.
fn mask_volatile(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut tokens = s.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        if is_time_token(tok) {
            if tokens.peek().is_some_and(|n| is_meridiem(n)) {
                tokens.next();
            }
            out.push("#");
        } else if is_percent_token(tok) {
            out.push("#");
        } else {
            out.push(tok);
        }
    }
    out.join(" ")
}

/// Hash of what a screen means, for stuck detection and verification.
///
/// Covers the app, keyboard visibility and, per element in order: role, label, value,
/// enabled and selected. It ignores frames, indices, focus, and clock times / percentages
/// inside labels and values.
pub fn screen_hash(screen: &CompactScreen) -> u64 {
    let mut h = Fnv64::new();
    h.field(&screen.app_bundle_id);
    h.write(&[u8::from(screen.keyboard_visible)]);
    for e in &screen.elements {
        h.field(e.role.as_str());
        h.field(&mask_volatile(&e.label));
        match &e.value {
            Some(v) => {
                h.write(&[1]);
                h.field(&mask_volatile(v));
            }
            None => h.write(&[0]),
        }
        h.write(&[u8::from(e.enabled), u8::from(e.selected)]);
    }
    h.finish()
}

/// Compares two screens by element fingerprint.
pub fn diff(before: &CompactScreen, after: &CompactScreen) -> ScreenDiff {
    let before_fp: BTreeSet<u64> = before.elements.iter().map(|e| e.fingerprint).collect();
    let after_fp: BTreeSet<u64> = after.elements.iter().map(|e| e.fingerprint).collect();
    let appeared: Vec<Element> = after
        .elements
        .iter()
        .filter(|e| !before_fp.contains(&e.fingerprint))
        .cloned()
        .collect();
    let disappeared: Vec<Element> = before
        .elements
        .iter()
        .filter(|e| !after_fp.contains(&e.fingerprint))
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
