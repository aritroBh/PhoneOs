//! What the digest and compaction both see of a raw tree: which subtrees are **excluded**,
//! which node is the overlay in front, and whether a node is **shown**.
//!
//! Both [`crate::screen_digest`] and [`crate::compact`] read a tree through one [`Layout`],
//! so what the compact screen lists and what the digest pins cannot drift apart.
//!
//! # Exclusions
//!
//! Only what screen content cannot imitate is excluded, and an excluded node is invisible
//! everywhere: it is not hashed, never a compact element (so never a target, a source of
//! summary facts or classifier context), and its subtree is skipped by the scroll cues.
//!
//! * **Status bar**, only on SpringBoard (the app id the runner reports): a node with role
//!   `StatusBar` that is a direct child of a top-level `Window` of the root. In any other
//!   app, and anywhere else, a `StatusBar` node is ordinary content.
//!
//! Nothing else is excluded: not by identifier (an app can set any identifier), not by
//! position, and **not the keyboard** (sixth review). An app's input accessory bar lives in
//! the keyboard's window right above the keys, so a rule that left out "the keyboard's
//! window" left out a compose bar, its Send button and an amount; the keyboard, its keys,
//! its prediction bar and everything around it are hashed like any other node. Compaction
//! still keeps the `Keyboard` node's own subtree (the keys) off the compact screen and reads
//! its Return key as a cue; that is the one direction allowed: what is hashed need not be
//! listed, but nothing unhashed is ever listed.
//!
//! # Shown
//!
//! A node is shown when no ancestor (nor itself) is pruned (hidden with a real frame, see
//! `compact`'s module docs), it is within the depth cap, it is inside the alert while one is
//! up, and [`Layout::shown_frame`] keeps enough of its frame (on screen, above the keyboard
//! unless an alert is up). The digest hashes that bit per node; compaction lists an element
//! only when its frame passes the same [`Layout::shown_frame`].

use std::collections::HashSet;

use jarvis_proto::{Rect, Role, Screen, UiNode, SPRINGBOARD_BUNDLE_ID};

use crate::compact::{is_pruned, MAX_DEPTH};

/// An element must keep this share of its area visible...
const MIN_VISIBLE_FRACTION: f64 = 0.3;
/// ...or at least this many points in both directions.
const MIN_VISIBLE_SIDE: f64 = 24.0;

/// The node's address, an identity for the duration of one borrow of the tree.
pub(crate) fn addr(node: &UiNode) -> usize {
    node as *const UiNode as usize
}

/// Whether `role` is the status bar (`StatusBar`, any naming style).
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

/// The excluded subtree roots of `screen` (see the module docs), by address.
fn exclusions(screen: &Screen) -> HashSet<usize> {
    let root = &screen.root;
    let mut out = HashSet::new();
    let springboard = screen
        .app_bundle_id
        .trim()
        .eq_ignore_ascii_case(SPRINGBOARD_BUNDLE_ID);
    if springboard {
        for w in root.children.iter().filter(|w| w.role == Role::Window) {
            for c in w.children.iter().filter(|c| is_status_bar(&c.role)) {
                out.insert(addr(c));
            }
        }
    }
    out
}

/// One screen as both the digest and compaction read it (see the module docs).
pub(crate) struct Layout<'a> {
    excluded: HashSet<usize>,
    /// The topmost shown Alert/Sheet (last in document order), outside excluded subtrees.
    pub alert: Option<&'a UiNode>,
    /// The first shown keyboard: its frame occludes, its Return key is a cue.
    pub keyboard: Option<&'a UiNode>,
    bounds: Rect,
    occluder: Option<Rect>,
}

impl<'a> Layout<'a> {
    /// Reads `screen`'s exclusions and overlays.
    pub fn of(screen: &'a Screen) -> Layout<'a> {
        let excluded = exclusions(screen);
        let mut alert = None;
        let mut keyboard = None;
        // (node, inside an excluded subtree)
        let mut stack = vec![(&screen.root, false)];
        while let Some((n, out)) = stack.pop() {
            if is_pruned(n) {
                continue;
            }
            let out = out || excluded.contains(&addr(n));
            match n.role {
                Role::Alert | Role::Sheet if !out && !n.frame.is_empty() => alert = Some(n),
                Role::Keyboard if keyboard.is_none() && !n.frame.is_empty() => {
                    keyboard = Some(n);
                }
                _ => {}
            }
            stack.extend(n.children.iter().rev().map(|c| (c, out)));
        }
        let occluder = if alert.is_some() {
            None
        } else {
            keyboard.map(|k| k.frame)
        };
        Layout {
            excluded,
            alert,
            keyboard,
            bounds: screen.bounds(),
            occluder,
        }
    }

    /// Whether `node` (and so its whole subtree) is excluded.
    pub fn is_excluded(&self, node: &UiNode) -> bool {
        !self.excluded.is_empty() && self.excluded.contains(&addr(node))
    }

    /// `frame` clipped to the screen and above the keyboard (no keyboard clip while an alert
    /// is up); `None` when too little of it remains: the one geometric test of "shown".
    pub fn shown_frame(&self, frame: Rect) -> Option<Rect> {
        if frame.is_empty() {
            return None;
        }
        let mut vis = if self.bounds.is_empty() {
            frame
        } else {
            frame.intersection(&self.bounds)?
        };
        if let Some(kb) = self.occluder {
            if kb.intersects(&vis) {
                if vis.y >= kb.y {
                    return None;
                }
                vis.height = kb.y - vis.y;
            }
        }
        let fraction = vis.area() / frame.area();
        let big_enough = vis.width >= MIN_VISIBLE_SIDE && vis.height >= MIN_VISIBLE_SIDE;
        (!vis.is_empty() && (fraction >= MIN_VISIBLE_FRACTION || big_enough)).then_some(vis)
    }

    /// Visits every non-excluded node of `root` in depth-first pre-order with its depth and
    /// whether it is shown (see the module docs). Iterative: any depth.
    pub fn walk(&self, root: &'a UiNode, mut f: impl FnMut(&'a UiNode, u32, bool)) {
        // (node, depth, an ancestor is pruned or past the cap, inside the alert)
        let mut stack: Vec<(&'a UiNode, u32, bool, bool)> =
            vec![(root, 0, false, self.alert.is_none())];
        while let Some((node, depth, hidden_above, in_scope)) = stack.pop() {
            if self.is_excluded(node) {
                continue;
            }
            let cut = hidden_above
                || is_pruned(node)
                || usize::try_from(depth).is_ok_and(|d| d > MAX_DEPTH);
            let in_scope = in_scope || self.alert.is_some_and(|a| std::ptr::eq(a, node));
            let shown = !cut && in_scope && self.shown_frame(node.frame).is_some();
            f(node, depth, shown);
            let child = depth.saturating_add(1);
            stack.extend(
                node.children
                    .iter()
                    .rev()
                    .map(|c| (c, child, cut, in_scope)),
            );
        }
    }
}

/// Where each non-excluded raw node sits in the pre-order sequence [`Layout::walk`] visits
/// (the digest's node order): its index by address, and per index its parent's index and
/// one past its last descendant's.
pub(crate) struct RawIndex {
    pub by_addr: std::collections::HashMap<usize, u32>,
    pub parents: Vec<u32>,
    pub ends: Vec<u32>,
}

impl RawIndex {
    /// Indexes `root` as `layout` walks it. Iterative.
    pub fn build(layout: &Layout<'_>, root: &UiNode) -> RawIndex {
        let mut out = RawIndex {
            by_addr: std::collections::HashMap::new(),
            parents: Vec::new(),
            ends: Vec::new(),
        };
        // (node, parent index, exit marker index)
        let mut stack: Vec<(&UiNode, u32, Option<u32>)> = vec![(root, u32::MAX, None)];
        while let Some((node, parent, exit)) = stack.pop() {
            if let Some(i) = exit {
                let end = u32::try_from(out.parents.len()).unwrap_or(u32::MAX);
                if let Some(e) = usize::try_from(i).ok().and_then(|i| out.ends.get_mut(i)) {
                    *e = end;
                }
                continue;
            }
            if layout.is_excluded(node) {
                continue;
            }
            let i = u32::try_from(out.parents.len()).unwrap_or(u32::MAX);
            out.by_addr.insert(addr(node), i);
            out.parents.push(parent);
            out.ends.push(i.saturating_add(1));
            stack.push((node, parent, Some(i)));
            stack.extend(node.children.iter().rev().map(|c| (c, i, None)));
        }
        out
    }
}
