//! The exact digest of a raw accessibility tree: what an owner's approval is checked against.
//!
//! An approval is given on a screen; before the approved action runs, the agent looks again
//! and acts only if the fresh tree's digest **equals** the one taken when the question was
//! asked. The digest is taken from the raw tree as the runner reported it, *before*
//! compaction cuts or drops anything (merged-label caps, unlabeled images, placeholders,
//! identifiers, hidden or off-screen nodes, depth cuts), so nothing the compact screen
//! leaves out can change under a standing approval.
//!
//! # Input
//!
//! The foreground app's bundle id, then every node in depth-first pre-order: its depth,
//! role, label, value, identifier, placeholder, and the `enabled`, `selected` and `secure`
//! flags. Every field is length-prefixed, so no two trees share an input.
//!
//! Not part of it:
//!
//! * **frames** (content may scroll; the agent re-plans the point on the fresh screen), and
//!   the geometric flags derived from them (`visible`, `hittable`) and `focused`;
//! * the **value of a secure field** (its presence, role and label are hashed; the value is
//!   never even read into the input);
//! * the subtree of the system **status bar** (`Other("StatusBar")`, XCUIElement type 25):
//!   its clock, battery and signal change on their own;
//! * the subtree of the **keyboard** (`Keyboard`), and of the keyboard's input-assistant bar
//!   (identifier `SystemInputAssistantView`: typing predictions, the autofill bar) while a
//!   keyboard is in the tree: keys and predictions change on their own. The keyboard's
//!   Return-key label (what Return does: send, search, go) stays in, as one entry;
//! * on SpringBoard only, the badge part of an icon's label after its first comma
//!   (`Mail, 3 unread` → `Mail`): the app's name stays in;
//! * the position (`12%`) of iOS's scroll indicators (`Vertical scroll bar, N pages` /
//!   `Horizontal scroll bar, N page`: an `Other` node with a whole percentage as value).
//!   Their label, page count included, stays in. UIKit and SwiftUI lists keep only the rows
//!   near the viewport in the tree, so a list that scrolled usually differs in its rows
//!   anyway and voids the approval (fail closed).
//!
//! Exclusions go by role (status bar, keyboard) or by iOS's fixed identifier/label
//! (input-assistant bar, scroll indicator), never by position on the screen.
//!
//! **Consequence, deliberately fail-closed:** a screen with live-updating text inside the
//! app (an ETA, a countdown, a ticking clock) never shows the same digest twice, so an
//! approval on it cannot be confirmed and is not acted on.
//!
//! # Hash
//!
//! SHA-256 (the workspace's `sha2`), kept whole: a page cannot be crafted to collide with
//! another. Linear in the size of the tree; the traversal is iterative (any depth).

use std::fmt;

use jarvis_proto::{Role, Screen, UiNode, SPRINGBOARD_BUNDLE_ID};
use sha2::{Digest, Sha256};

use crate::compact::return_key_label;

/// The exact digest of a raw screen (see the module docs). `Debug` shows a short hex prefix.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ScreenDigest([u8; 32]);

impl ScreenDigest {
    /// The 32 digest bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ScreenDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ScreenDigest(")?;
        for b in self.0.iter().take(8) {
            write!(f, "{b:02x}")?;
        }
        write!(f, "…)")
    }
}

/// Where the digest input goes: the hasher, or a buffer (for inspection in tests).
trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for Sha256 {
    fn put(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

fn put_len(sink: &mut impl Sink, len: usize) {
    sink.put(&u64::try_from(len).unwrap_or(u64::MAX).to_le_bytes());
}

fn put_str(sink: &mut impl Sink, s: &str) {
    put_len(sink, s.len());
    sink.put(s.as_bytes());
}

fn put_opt(sink: &mut impl Sink, s: Option<&str>) {
    match s {
        None => sink.put(&[0]),
        Some(s) => {
            sink.put(&[1]);
            put_str(sink, s);
        }
    }
}

/// Whether `role` is the system status bar (`Other("StatusBar")`, any naming style).
fn is_status_bar(role: &Role) -> bool {
    match role {
        Role::Other(name) => {
            let n: String = name
                .chars()
                .filter(|c| !matches!(c, '_' | '-' | ' '))
                .flat_map(char::to_lowercase)
                .collect();
            n == "statusbar"
        }
        _ => false,
    }
}

/// iOS's identifier of the bar above the keyboard (typing predictions, autofill).
const INPUT_ASSISTANT_ID: &str = "SystemInputAssistantView";

/// Whether `node` is one of iOS's scroll indicators, whose value (the position) is left out.
fn is_scroll_indicator(node: &UiNode) -> bool {
    if !matches!(node.role, Role::Other(_)) {
        return false;
    }
    let label = node.label.as_deref().unwrap_or("").trim().to_lowercase();
    let Some(rest) = label
        .strip_prefix("vertical scroll bar,")
        .or_else(|| label.strip_prefix("horizontal scroll bar,"))
    else {
        return false;
    };
    let pages = rest
        .trim()
        .strip_suffix("pages")
        .or_else(|| rest.trim().strip_suffix("page"));
    let percent = node
        .value
        .as_deref()
        .map(str::trim)
        .and_then(|v| v.strip_suffix('%'))
        .is_some_and(|p| !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()));
    pages.is_some_and(|p| !p.trim().is_empty() && p.trim().chars().all(|c| c.is_ascii_digit()))
        && percent
}

/// Writes the digest input of `screen` (see the module docs) to `sink`.
fn feed(screen: &Screen, sink: &mut impl Sink) {
    sink.put(b"jarvis-screen-digest-v1");
    put_str(sink, screen.app_bundle_id.trim());
    let springboard = screen
        .app_bundle_id
        .trim()
        .eq_ignore_ascii_case(SPRINGBOARD_BUNDLE_ID);
    let keyboard_up = screen.root.iter().any(|n| n.role == Role::Keyboard);
    let mut stack: Vec<(&UiNode, u32)> = vec![(&screen.root, 0)];
    while let Some((node, depth)) = stack.pop() {
        if is_status_bar(&node.role) {
            continue;
        }
        if node.role == Role::Keyboard {
            // One entry for the keyboard: what its Return key does.
            sink.put(b"K");
            sink.put(&depth.to_le_bytes());
            put_opt(sink, return_key_label(node).as_deref());
            continue;
        }
        if keyboard_up && node.identifier.as_deref() == Some(INPUT_ASSISTANT_ID) {
            continue;
        }
        let secure = node.secure || node.role == Role::SecureTextField;
        let label = node.label.as_deref().map(|l| {
            if springboard && node.role != Role::StaticText {
                l.split_once(',').map_or(l, |(name, _badge)| name)
            } else {
                l
            }
        });
        let value = if secure || is_scroll_indicator(node) {
            None
        } else {
            node.value.as_deref()
        };
        sink.put(b"N");
        sink.put(&depth.to_le_bytes());
        put_str(sink, node.role.as_str());
        put_opt(sink, label);
        put_opt(sink, value);
        put_opt(sink, node.identifier.as_deref());
        put_opt(sink, node.placeholder.as_deref());
        sink.put(&[
            u8::from(node.enabled),
            u8::from(node.selected),
            u8::from(secure),
        ]);
        let child = depth.saturating_add(1);
        stack.extend(node.children.iter().rev().map(|c| (c, child)));
    }
}

/// The exact digest of `screen`'s raw tree (see the module docs).
pub fn screen_digest(screen: &Screen) -> ScreenDigest {
    let mut h = Sha256::new();
    feed(screen, &mut h);
    ScreenDigest(h.finalize().into())
}

/// The bytes [`screen_digest`] hashes, for inspection (tests and audits): it never holds
/// a secure field's value.
pub fn screen_digest_input(screen: &Screen) -> Vec<u8> {
    let mut out = Vec::new();
    feed(screen, &mut out);
    out
}
