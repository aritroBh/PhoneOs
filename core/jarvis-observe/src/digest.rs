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
//! keyboard is ordinary content: hashed whole and listed. **Ninth review:** the reduced
//! hashing covers only the nodes of those subtrees whose on-screen frame lies inside the
//! subtree root's own frame; a node reaching outside it (an accessory view carrying the
//! assistant's identifier and drawing a control over the app's Send button) is ordinary
//! content — hashed whole, listed and contesting taps (see `crate::layout`).
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
//!   Their label, page count included, stays in;
//! * the **order** within a run of consecutive **order-free** sibling subtrees (v6, below).
//!
//! # Order (v6)
//!
//! XCUITest does not keep the order of the children of a hidden, area-less node: on the
//! iOS 26.5 simulator home screen (CI run of df4a416, `testSpringboardUnknownIconAsksOnce`)
//! two consecutive snapshots of the same screen (274 nodes, same frames, same compact
//! screen) reported the children of `Button "Utilities folder" {{0, 0}, {0, 0}} [hidden]`
//! as `Button "Passwords"`, `Button "Shortcuts"` (both `{{0, 0}, {0, 0}} [hidden]`, no
//! children) and then the other way round. The v5 digest hashed raw order, so the approval
//! was voided and the owner was asked the same first-use question twice.
//!
//! A subtree is **order-free** when every node in it (excluded subtrees aside) is flagged
//! invisible, has no area (or a non-finite origin) and nothing of it on screen (a NaN
//! origin with a size is clamped onto the screen and drawn: it keeps its order), so it is
//! neither drawn nor shown, is
//! ordinary content (not in the keyboard or the input assistant, nor the root of either),
//! and is none of what compaction reads in document order: a text input or a secure node
//! (the fields a submit sends, `CompactView::inputs`), a `StaticText` or `Image` with a label
//! or a value (their words go into an enclosing control's or cell's label in document
//! order), a keyboard or a key. Within one parent, each **maximal run of consecutive**
//! order-free children (excluded children count as absent) is hashed as one record: `R`,
//! the depth, the count and the members' subtree hashes **sorted** (a multiset: a
//! duplicate counts). A subtree hash is SHA-256 of the node's role, frame and content
//! fields exactly as in its node record, its child count and its children's subtree
//! hashes, sorted (every child of an order-free node is order-free). The root is never in
//! a run. Consequences:
//!
//! * a reorder **inside** a run keeps the digest;
//! * any hashed field of any node in a run changing, a node added, removed or duplicated,
//!   a member moving to another parent or depth changes it;
//! * a member moving across a sibling that is not order-free (shown, framed, a text input,
//!   a labelled text…) changes it: runs never span such a sibling;
//! * a node of the subtree becoming visible or getting an area takes the whole subtree
//!   out of the rule (its record is then the ordinary, order-sensitive one).
//!
//! **Audit (why the order carries no meaning there).** Every consumer of raw or compact
//! order was checked (`compact`, `layout`, `render`, this module; `jarvis-safety`'s `hit`,
//! `classify`, `evaluate`, `approval`; the session's re-checks A2–A4 and its tap-point
//! re-derivation). An order-free node is never listed: compaction keeps an element only
//! when `Layout::shown_frame` keeps its (possibly refined) frame, which needs an area, and
//! a refinement only steps to a nested control with an area. It is never a cover
//! (`CompactView::nodes` needs a frame on screen), a bar (needs shown), a scroll or page cue
//! (needs a frame on screen), an alert or the keyboard (`Layout::of` needs an area). It
//! never lends words (no labelled text or image is order-free) and is never an input (no
//! text input is). It does take part in counts that do not depend on order: an enclosing
//! cell's interactive count and its first interactive child are only used when the count
//! is 1; fingerprint ordinals count earlier same-key nodes, and a run's multiset stays
//! between the same neighbours; group ids and raw-node indices outside the run are
//! unchanged because the run keeps its size. So a reorder inside a run leaves the whole
//! `CompactView` equal (screen, sources, bars, nodes, inputs; `parents`/`ends` differ only
//! at indices inside the run, which no listed element, cover, bar or input refers to), and
//! `jarvis-safety` (classification, covers, "later in document order", forms, `Sends:`,
//! facts) reads only those. A2 (compact record), A3 (target's raw node) and A4 (summary)
//! are therefore unchanged too; the tests `order_free_reorders_leave_the_compact_view_equal`
//! (`tests/digest_agreement.rs`) check it on random trees.
//!
//! **Consequence, deliberately fail-closed:** a screen with live-updating text inside the
//! app (an ETA, a countdown, a ticking clock) never shows the same digest twice, so an
//! approval on it cannot be confirmed and is not acted on.
//!
//! # Hash
//!
//! SHA-256 (the workspace's `sha2`), kept whole: a page cannot be crafted to collide with
//! another. Linear in the size of the tree, plus sorting within each order-free run
//! (`k log k` for `k` siblings); both traversals are iterative (any depth).

use std::collections::HashMap;
use std::fmt;

use jarvis_proto::{Rect, Role, Screen, UiNode, SPRINGBOARD_BUNDLE_ID};
use sha2::{Digest, Sha256};

use crate::compact::return_key_label;
use crate::layout::{addr, Layout, Quiet};

/// The version tag the digest input starts with. v6 (CI run of df4a416): order-free runs of
/// hidden, area-less sibling subtrees are hashed as a sorted multiset (module docs,
/// *Order*). v5: the prediction bar's inner nodes left out (eighth review).
const DIGEST_VERSION: &[u8] = b"jarvis-screen-digest-v6";

/// The exact digest of a raw screen (see the module docs). `Debug` shows a short hex prefix.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ScreenDigest([u8; 32]);

impl ScreenDigest {
    /// The 32 digest bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The digest as 64 lower-case hex digits (how a confirmation names the screen it was
    /// asked on, `jarvis_proto::ConfirmTarget::digest`).
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
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

/// A node's text fields and flags (the `T` part of its record): label (an icon's badge cut
/// on SpringBoard), value (never a secure field's, nor a scroll indicator's position),
/// identifier, placeholder, `enabled`, `selected`, `secure`, `visible` and `shown`.
fn put_content(sink: &mut impl Sink, node: &UiNode, springboard: bool, shown: bool) {
    let secure = node.secure || node.role == Role::SecureTextField;
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
        u8::from(shown),
    ]);
}

/// Whether `node` by itself may belong to an order-free subtree (see the module docs,
/// *Order*): flagged invisible, with no area (or a non-finite origin) and nothing of it on
/// screen, so neither drawn nor shown; ordinary content (not in or at the root of the keyboard or the input assistant);
/// and nothing compaction reads in document order: not a text input or a secure node (the
/// fields a submit sends, `CompactView::inputs`), not a `StaticText` or `Image` carrying a
/// label or a value (their words go into an enclosing control's or cell's label in document
/// order), not a keyboard or a key. Fail closed: every other node keeps its order.
fn order_free_node(node: &UiNode, layout: &Layout<'_>) -> bool {
    let f = node.frame;
    !node.visible
        && (f.is_empty() || !f.x.is_finite() || !f.y.is_finite())
        && layout.on_screen(f).is_none()
        && layout.shown_frame(f).is_none()
        && layout.quiet_of(node) == Quiet::No
        && layout.quiet_root(node).is_none()
        && !node.secure
        && !node.role.is_text_input()
        && !matches!(
            node.role,
            Role::Keyboard | Role::Key | Role::SecureTextField
        )
        && !(matches!(node.role, Role::StaticText | Role::Image)
            && (node.label.is_some() || node.value.is_some()))
}

/// Where a maximal run of order-free sibling subtrees is hashed (see the module docs,
/// *Order*): at its first member, the run's subtree hashes, sorted, are
/// `hashes[start..start + len]`; at its other members, nothing.
#[derive(Clone, Copy)]
enum RunAt {
    First { start: usize, len: usize },
    Rest,
}

/// The order-free runs of `screen` (see the module docs, *Order*), by member address, and
/// their sorted subtree hashes. One iterative post-order pass. An order-free subtree's hash
/// is SHA-256 of `S`, its role, frame and content (as in its node record, `shown` false),
/// its child count and its children's hashes sorted. Nothing is kept for a node outside an
/// order-free subtree; one 32-byte hash per run member.
fn order_free_runs(
    screen: &Screen,
    layout: &Layout<'_>,
    springboard: bool,
) -> (HashMap<usize, RunAt>, Vec<[u8; 32]>) {
    struct Open<'n> {
        node: &'n UiNode,
        /// Next child to visit.
        next: usize,
        /// The subtree is order-free as far as seen.
        free: bool,
        /// Where its order-free children's entries start in `pending`.
        start: usize,
        /// Its position among its parent's non-excluded children.
        pos: usize,
        /// Non-excluded children seen.
        kids: usize,
    }
    let mut runs: HashMap<usize, RunAt> = HashMap::new();
    let mut hashes: Vec<[u8; 32]> = Vec::new();
    // Order-free subtrees whose parent is still open: (address, position, hash).
    let mut pending: Vec<(usize, usize, [u8; 32])> = Vec::new();
    let mut stack = vec![Open {
        node: &screen.root,
        next: 0,
        free: false,
        start: 0,
        pos: 0,
        kids: 0,
    }];
    while let Some(top) = stack.last_mut() {
        if let Some(child) = top.node.children.get(top.next) {
            top.next += 1;
            if layout.is_excluded(child) {
                // Neither hashed nor listed: as if absent.
                continue;
            }
            let pos = top.kids;
            top.kids += 1;
            let start = pending.len();
            stack.push(Open {
                node: child,
                next: 0,
                free: order_free_node(child, layout),
                start,
                pos,
                kids: 0,
            });
            continue;
        }
        let Some(done) = stack.pop() else {
            break;
        };
        let parent = stack.last_mut();
        if done.free && parent.is_some() {
            // Every child is order-free: their order carries no meaning.
            let kids = pending.get_mut(done.start..).unwrap_or_default();
            kids.sort_unstable_by_key(|k| k.2);
            let mut h = Sha256::new();
            h.put(b"S");
            put_str(&mut h, done.node.role.as_str());
            put_frame(&mut h, layout.on_screen(done.node.frame));
            put_content(&mut h, done.node, springboard, false);
            put_len(&mut h, kids.len());
            for k in kids.iter() {
                h.put(&k.2);
            }
            pending.truncate(done.start);
            pending.push((addr(done.node), done.pos, h.finalize().into()));
            continue;
        }
        if let Some(p) = parent {
            p.free = false;
        }
        // Its order-free children are maximal: each run of consecutive ones is one record.
        let kids = pending.get(done.start..).unwrap_or_default();
        let mut i = 0;
        while let Some(first) = kids.get(i) {
            let mut j = i + 1;
            while kids
                .get(j)
                .zip(kids.get(j - 1))
                .is_some_and(|(b, a)| b.1 == a.1 + 1)
            {
                j += 1;
            }
            let start = hashes.len();
            hashes.extend(kids.get(i..j).unwrap_or_default().iter().map(|k| k.2));
            hashes.get_mut(start..).unwrap_or_default().sort_unstable();
            runs.insert(first.0, RunAt::First { start, len: j - i });
            for k in kids.get(i + 1..j).unwrap_or_default() {
                runs.insert(k.0, RunAt::Rest);
            }
            i = j;
        }
        pending.truncate(done.start);
    }
    (runs, hashes)
}

/// Writes the digest input of `screen` (see the module docs) to `sink`.
fn feed(screen: &Screen, sink: &mut impl Sink) {
    sink.put(DIGEST_VERSION);
    put_str(sink, screen.app_bundle_id.trim());
    put_frame(sink, Some(screen.bounds()));
    let springboard = screen
        .app_bundle_id
        .trim()
        .eq_ignore_ascii_case(SPRINGBOARD_BUNDLE_ID);
    let layout = Layout::of(screen);
    let (runs, run_hashes) = order_free_runs(screen, &layout, springboard);
    layout.walk_pruning(&screen.root, |node, v| {
        match runs.get(&addr(node)) {
            Some(RunAt::First { start, len }) => {
                // A maximal run of order-free sibling subtrees: their hashes, sorted.
                sink.put(b"R");
                sink.put(&v.depth.to_le_bytes());
                put_len(sink, *len);
                for h in run_hashes.get(*start..start + len).unwrap_or_default() {
                    sink.put(h);
                }
                return false;
            }
            Some(RunAt::Rest) => return false,
            None => {}
        }
        // Inside the prediction bar only its own node is hashed (eighth review): nothing in
        // it, so a different number of candidates does not void an approval. Its nodes are
        // never listed, and a tap point anywhere in its frame is refused.
        if v.quiet == Quiet::Assistant && layout.quiet_root(node) != Some(Quiet::Assistant) {
            return true;
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
                return true;
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
                return true;
            }
            Quiet::No => sink.put(b"T"),
        }
        put_content(sink, node, springboard, v.shown);
        true
    });
}

/// The exact digest of `screen`'s raw tree (see the module docs).
pub fn screen_digest(screen: &Screen) -> ScreenDigest {
    let mut h = Sha256::new();
    feed(screen, &mut h);
    ScreenDigest(h.finalize().into())
}

/// How common the order rule is on `screen`, for audits (see the module docs, *Order*):
/// the runs of order-free siblings with two or more members, their members, and the nodes
/// in those members' subtrees (the nodes whose sibling order is not hashed).
#[doc(hidden)]
pub fn order_free_census(screen: &Screen) -> (usize, usize, usize) {
    let springboard = screen
        .app_bundle_id
        .trim()
        .eq_ignore_ascii_case(SPRINGBOARD_BUNDLE_ID);
    let layout = Layout::of(screen);
    let (runs, _) = order_free_runs(screen, &layout, springboard);
    let (mut count, mut members, mut nodes) = (0, 0, 0);
    let mut in_run = 0usize;
    layout.walk_pruning(&screen.root, |node, _| match runs.get(&addr(node)) {
        Some(RunAt::First { len, .. }) => {
            in_run = *len;
            if *len > 1 {
                count += 1;
                members += len;
                nodes += node.count();
            }
            false
        }
        Some(RunAt::Rest) => {
            if in_run > 1 {
                nodes += node.count();
            }
            false
        }
        None => true,
    });
    (count, members, nodes)
}

/// The bytes [`screen_digest`] hashes, for inspection (tests and audits): it never holds
/// a secure field's value.
pub fn screen_digest_input(screen: &Screen) -> Vec<u8> {
    let mut out = Vec::new();
    feed(screen, &mut out);
    out
}
