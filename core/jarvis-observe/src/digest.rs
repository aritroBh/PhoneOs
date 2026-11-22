//! The exact digest of a raw accessibility tree: what an owner's approval is checked against.
//!
//! An approval is given on a screen; before the approved action runs, the agent looks again
//! and acts only if the fresh tree's digest **equals** the one taken when the question was
//! asked. The digest is taken from the raw tree as the runner reported it, *before*
//! compaction cuts or drops anything (merged-label caps, unlabeled images, placeholders,
//! identifiers, hidden or off-screen nodes, depth cuts), so nothing the compact screen
//! leaves out can change under a standing approval. It is one of four parts of the record
//! (see `jarvis-agent`): the digest (A1), the compact screen's content (A2), the target's
//! raw node (A3, see [`crate::CompactView::sources`]) and the summary (A4).
//!
//! # Input
//!
//! The foreground app's bundle id and the screen's size, then every node that is not
//! excluded (see [`crate::layout`]), in depth-first pre-order: its depth, role, **frame**
//! (clipped to the screen and rounded to whole points; absent when nothing of it is on
//! screen; seventh review), label, value, identifier, placeholder, the `enabled`,
//! `selected` and `secure` flags, the raw `visible` flag, and whether it is **shown**: the
//! exact predicate compaction uses to list a node (no pruned ancestor, within the depth cap,
//! inside the alert while one is up, and enough of its frame on screen and above the
//! keyboard; one function, `Layout::shown_frame`, decides it for both). Every field is
//! length-prefixed, so no two trees share an input.
//!
//! So an approved screen must match in layout too: anything that moves, scrolls, appears,
//! vanishes or changes its shown bit changes the digest (the session's settle retry covers
//! an animation still finishing).
//!
//! **Keyboard text (seventh review).** Inside a node with role `Keyboard` (itself included)
//! only the role, frame, `visible` flag and shown bit are hashed, plus, at the `Keyboard`
//! node, its Return key's label (the cue compaction reads). Inside a node whose identifier
//! is `SystemInputAssistantView` that is in the same top-level window as a `Keyboard` node
//! and has no web-view ancestor (the prediction bar) only that node's own role, frame,
//! identifier, flags and shown bit are hashed (eighth review: nothing inside it, so a
//! different number of candidates after the owner answers does not void an approval; was
//! every inner node's role, frame and flags). Nowhere else does that identifier matter.
//! Prediction candidates, the shift key's state and the dictation and autofill labels
//! change on their own; the keyboard's own structure and geometry are still hashed, so no
//! key can appear, vanish or move there unnoticed, and the prediction bar's frame bounds
//! everything in it (a tap point inside it is refused whatever it holds). Compaction never lists anything in
//! those subtrees, and never reads their text for cues, summaries or classifier context
//! (*not fully hashed ⇒ not in the compact screen*). An app's own accessory bar next to the
//! keyboard is ordinary content: hashed whole and listed.
//!
//! Not part of it:
//!
//! * `focused` and the raw `hittable` flag;
//! * the **value of a secure field** (its presence, role and label are hashed; the value is
//!   never even read into the input); compaction never lists a secure node's value either;
//! * **excluded** subtrees (see [`crate::layout`]): on SpringBoard only, the status bar under
//!   a top-level window;
//! * the keyboard's and the input assistant's text, above;
//! * on SpringBoard only, the badge part after the first comma of an app **icon**'s label
//!   (role `Icon`: `Mail, 3 unread` → `Mail`); other nodes keep their whole label;
//! * the position (`12%`) of iOS's scroll indicators (`Vertical scroll bar, N pages` /
//!   `Horizontal scroll bar, N page`: an `Other` node with a whole percentage as value).
//!   Their label, page count included, stays in.
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

use jarvis_proto::{Rect, Role, Screen, UiNode, SPRINGBOARD_BUNDLE_ID};
use sha2::{Digest, Sha256};

use crate::compact::return_key_label;
use crate::layout::{Layout, Quiet};

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

/// Whether `role` is a SpringBoard app icon (`Icon`, any letter case).
fn is_icon(role: &Role) -> bool {
    matches!(role, Role::Other(name) if name.trim().eq_ignore_ascii_case("icon"))
}

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

/// A frame rounded to whole points (saturating; frames are small screen coordinates).
fn put_frame(sink: &mut impl Sink, frame: Option<Rect>) {
    match frame {
        None => sink.put(&[0]),
        Some(f) => {
            sink.put(&[1]);
            for v in [f.x, f.y, f.width, f.height] {
                // Float to int saturates; NaN becomes 0.
                sink.put(&(v.round() as i64).to_le_bytes());
            }
        }
    }
}

/// Writes the digest input of `screen` (see the module docs) to `sink`.
fn feed(screen: &Screen, sink: &mut impl Sink) {
    sink.put(b"jarvis-screen-digest-v5");
    put_str(sink, screen.app_bundle_id.trim());
    put_frame(sink, Some(screen.bounds()));
    let springboard = screen
        .app_bundle_id
        .trim()
        .eq_ignore_ascii_case(SPRINGBOARD_BUNDLE_ID);
    let layout = Layout::of(screen);
    layout.walk(&screen.root, |node, v| {
        // Inside the prediction bar only its own node is hashed (eighth review): nothing in
        // it, so a different number of candidates does not void an approval. Its nodes are
        // never listed, and a tap point anywhere in its frame is refused.
        if v.quiet == Quiet::Assistant && layout.quiet_root(node) != Some(Quiet::Assistant) {
            return;
        }
        let secure = node.secure || node.role == Role::SecureTextField;
        sink.put(b"N");
        sink.put(&v.depth.to_le_bytes());
        put_str(sink, node.role.as_str());
        put_frame(sink, layout.on_screen(node.frame));
        match v.quiet {
            Quiet::Keyboard => {
                // Structure and geometry only, plus the Return key's label (a cue).
                sink.put(b"K");
                if node.role == Role::Keyboard {
                    put_opt(sink, return_key_label(node).as_deref());
                }
                sink.put(&[u8::from(node.visible), u8::from(v.shown)]);
                return;
            }
            Quiet::Assistant => {
                // The prediction bar's own node: role, frame, identifier and flags.
                sink.put(b"A");
                put_opt(sink, node.identifier.as_deref());
                sink.put(&[
                    u8::from(node.enabled),
                    u8::from(node.selected),
                    u8::from(secure),
                    u8::from(node.visible),
                    u8::from(v.shown),
                ]);
                return;
            }
            Quiet::No => sink.put(b"T"),
        }
        let label = node.label.as_deref().map(|l| {
            if springboard && is_icon(&node.role) {
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
        put_opt(sink, label);
        put_opt(sink, value);
        put_opt(sink, node.identifier.as_deref());
        put_opt(sink, node.placeholder.as_deref());
        sink.put(&[
            u8::from(node.enabled),
            u8::from(node.selected),
            u8::from(secure),
            u8::from(node.visible),
            u8::from(v.shown),
        ]);
    });
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
