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
//! the keyboard's window right above the keys; it is hashed and listed like any content.
//!
//! # Keyboard
//!
//! Two kinds of subtree have their *text* left out of the digest (seventh review), while
//! their structure, frames and shown bits are hashed: a node with role `Keyboard`, and a
//! node whose identifier is `SystemInputAssistantView` that is in a top-level window (a
//! child of the root) holding a `Keyboard` node and has no web-view ancestor (the
//! prediction bar). Compaction lists nothing inside them and reads no text from them
//! (only the Return key's label, which the digest hashes): the one direction allowed is
//! "hashed need not be listed", never "listed but not hashed". The nodes of those two kinds
//! of subtree are the keyboard for the tap-point rule (`CompactView::nodes`); the keyboard
//! window's other nodes (its full-screen containers) are not.
//!
//! **The keyboard's area (audit of real trees, C4)** is larger than the `Keyboard` node:
//! real trees draw the Emoji and Dictate buttons below it, inside the input host
//! (`inputView`). The first shown keyboard's area (`keyboard_area`: the node, the strip from
//! its top to the screen's bottom, every input host holding it) is what occludes listing,
//! and the `Keyboard` node's drawn frame in `CompactView::nodes`, so a point anywhere in it
//! is the keyboard for the tap-point rule.
//!
//! **Only inside the root's frame (ninth review).** The reduced hashing covers a node of
//! such a subtree only when its frame lies inside the subtree root's own frame (1 pt
//! tolerance; only the part of its frame on screen counts, so a node with nothing on screen
//! draws nothing and counts as inside). Any node of
//! the subtree reaching outside it is **ordinary content**: hashed fully, listed, and
//! contesting taps like any node (an app's accessory view can carry the identifier
//! `SystemInputAssistantView` and draw a control over the app's Send button). The test is
//! per node: an escaped node's children inside the root's frame stay reduced.
//!
//! # Shown
//!
//! A node is shown when no ancestor (nor itself) is pruned (hidden with a real frame, see
//! `compact`'s module docs), it is within the depth cap, it is inside the alert while one is
//! up, and [`Layout::shown_frame`] keeps enough of its frame (on screen, above the keyboard
//! unless an alert is up). The digest hashes that bit per node; compaction lists an element
//! only when its frame passes the same [`Layout::shown_frame`].

use std::collections::{HashMap, HashSet};

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

/// The identifier iOS gives the input assistant (the prediction bar above the keyboard).
pub(crate) const INPUT_ASSISTANT_ID: &str = "SystemInputAssistantView";

/// How much of a node's text the digest takes (see the module docs, *Keyboard*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quiet {
    /// All of it.
    No,
    /// Inside a `Keyboard`-role node (itself included): role, frame and shown bit only.
    Keyboard,
    /// Inside the input assistant next to a keyboard: no labels, values or placeholders.
    Assistant,
}

/// What [`Layout::walk`] knows of a node besides the node itself.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Visit {
    /// Depth from the root.
    pub depth: u32,
    /// The node is shown (see the module docs).
    pub shown: bool,
    /// The node is drawn: no pruned ancestor (nor itself), within the depth cap, inside the
    /// alert while one is up. Its frame may still lie off screen.
    pub live: bool,
    /// Inside the alert while one is up (always, without one).
    pub in_scope: bool,
    /// How much of its text is hashed (and so may be listed).
    pub quiet: Quiet,
    /// Inside a keyboard or input-assistant subtree: that subtree root's frame, which
    /// clips where its nodes are drawn (the prediction bar's scroll content reports a
    /// screen-high frame but is drawn only inside the 44 pt bar).
    pub quiet_clip: Option<Rect>,
    /// It has an ancestor that is a web view, scroll view, table or collection view.
    pub in_content: bool,
}

/// Whether `role` is a web view, scroll view, table or collection view.
fn is_content_role(role: &Role) -> bool {
    match role {
        Role::WebView | Role::ScrollView | Role::Table | Role::CollectionView => true,
        Role::Other(name) => {
            let n: String = name
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            matches!(
                n.as_str(),
                "webview" | "scrollview" | "table" | "tableview" | "collectionview"
            )
        }
        _ => false,
    }
}

/// The quiet subtree roots (keyboards and qualifying input assistants), by address (see the
/// module docs, *Keyboard*).
fn keyboard_parts(screen: &Screen, excluded: &HashSet<usize>) -> HashMap<usize, Quiet> {
    let mut quiet = HashMap::new();
    for w in &screen.root.children {
        if excluded.contains(&addr(w)) || is_pruned(w) {
            continue;
        }
        let mut keyboards = Vec::new();
        let mut assistants = Vec::new();
        // (node, depth, a web view above it)
        let mut stack: Vec<(&UiNode, usize, bool)> = vec![(w, 1, false)];
        while let Some((n, depth, web)) = stack.pop() {
            if is_pruned(n) || depth > MAX_DEPTH || excluded.contains(&addr(n)) {
                continue;
            }
            if n.role == Role::Keyboard {
                keyboards.push(addr(n));
            }
            if !web && n.identifier.as_deref() == Some(INPUT_ASSISTANT_ID) {
                assistants.push(addr(n));
            }
            let web = web || is_web(&n.role);
            stack.extend(n.children.iter().rev().map(|c| (c, depth + 1, web)));
        }
        if keyboards.is_empty() {
            continue;
        }
        for a in assistants {
            quiet.insert(a, Quiet::Assistant);
        }
        for k in keyboards {
            quiet.insert(k, Quiet::Keyboard);
        }
    }
    quiet
}

/// Whether `role` is a web view.
fn is_web(role: &Role) -> bool {
    match role {
        Role::WebView => true,
        Role::Other(name) => {
            let n: String = name
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            n == "webview"
        }
        _ => false,
    }
}

/// How far (points) a node of a keyboard or input-assistant subtree may reach outside the
/// subtree root's frame and still count as inside it.
const QUIET_TOLERANCE: f64 = 1.0;

/// Whether the on-screen part of `inner` (clipped to `bounds`, unless they are empty) lies
/// inside `outer` (within [`QUIET_TOLERANCE`]); a frame with nothing on screen draws
/// nothing and counts as inside (a prediction bar's candidates scrolled off the edge).
fn quiet_inside(outer: &Rect, inner: &Rect, bounds: &Rect) -> bool {
    let inner = if inner.is_empty() {
        None
    } else if bounds.is_empty() {
        Some(*inner)
    } else {
        inner.intersection(bounds)
    };
    let Some(inner) = inner.filter(|f| !f.is_empty()) else {
        return true;
    };
    inner.x >= outer.x - QUIET_TOLERANCE
        && inner.y >= outer.y - QUIET_TOLERANCE
        && inner.max_x() <= outer.max_x() + QUIET_TOLERANCE
        && inner.max_y() <= outer.max_y() + QUIET_TOLERANCE
}

/// Every node whose text is left out (see the module docs, *Keyboard*): the nodes of the
/// quiet subtrees (`roots`) that lie inside their outermost root's frame, with their kind
/// (a keyboard inside the assistant is still a keyboard). Nodes reaching outside are left
/// out of the map: they are ordinary content.
fn quiet_nodes(
    screen: &Screen,
    roots: &HashMap<usize, Quiet>,
    excluded: &HashSet<usize>,
) -> HashMap<usize, Quiet> {
    let mut out = HashMap::new();
    if roots.is_empty() {
        return out;
    }
    let bounds = screen.bounds();
    // (node, inherited kind, outermost root frame)
    let mut stack: Vec<(&UiNode, Quiet, Option<Rect>)> = vec![(&screen.root, Quiet::No, None)];
    while let Some((n, kind, clip)) = stack.pop() {
        if excluded.contains(&addr(n)) {
            continue;
        }
        let (kind, clip) = match roots.get(&addr(n)) {
            Some(q) => {
                let k = if kind == Quiet::Keyboard { kind } else { *q };
                (k, clip.or(Some(n.frame)))
            }
            None => (kind, clip),
        };
        if kind != Quiet::No {
            let root = roots.contains_key(&addr(n));
            if root || clip.is_some_and(|c| quiet_inside(&c, &n.frame, &bounds)) {
                out.insert(addr(n), kind);
            }
        }
        stack.extend(n.children.iter().rev().map(|c| (c, kind, clip)));
    }
    out
}

/// Audit of real trees (C4): the identifier iOS gives the keyboard window's input host,
/// which holds the keys, the row under them (`Button "Emoji"`, `Button "Dictate"
/// id=dictation`, outside the `Keyboard` node) and the prediction bar: in every real
/// keyboard capture of iOS 26.5 (`login_keyboard`, Safari's form, Spotlight, the edit
/// menus).
pub(crate) const INPUT_HOST_ID: &str = "inputView";

/// How far (points) the input host's frame may fall short of the keyboard's and still hold
/// it.
const HOST_TOLERANCE: f64 = 1.0;

/// The smallest rectangle holding `a` and `b`.
fn bounding(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect::new(
        x,
        y,
        a.max_x().max(b.max_x()) - x,
        a.max_y().max(b.max_y()) - y,
    )
}

/// Audit of real trees (C4): where the keyboard `k` takes touches. Real trees put the
/// keys' bottom row outside the `Keyboard` node (`Keyboard {{0, 655}, {440, 243}}` ends at
/// y 898; Emoji and Dictate sit at y 887–956 in a sibling), inside the input host
/// (`Other id=inputView {{0, 611}, {440, 345}}`). The area is the smallest rectangle holding
/// the `Keyboard` node's frame, the strip from its top to the screen's bottom (full width),
/// and every drawn input host ([`INPUT_HOST_ID`], not excluded) whose frame holds the
/// keyboard's — clipped to the screen. The identifier only ever enlarges the area (an app
/// can set any identifier: at worst more of its screen counts as keyboard, fail closed).
fn keyboard_area(screen: &Screen, k: &UiNode, excluded: &HashSet<usize>) -> Rect {
    let kb = k.frame;
    let bounds = screen.bounds();
    let mut area = kb;
    if !bounds.is_empty() && kb.y < bounds.max_y() {
        let top = kb.y.max(bounds.y);
        area = bounding(
            area,
            Rect::new(bounds.x, top, bounds.width, bounds.max_y() - top),
        );
    }
    let holds = |f: &Rect| {
        f.x <= kb.x + HOST_TOLERANCE
            && f.y <= kb.y + HOST_TOLERANCE
            && f.max_x() >= kb.max_x() - HOST_TOLERANCE
            && f.max_y() >= kb.max_y() - HOST_TOLERANCE
    };
    let mut stack: Vec<(&UiNode, usize)> = vec![(&screen.root, 0)];
    while let Some((n, depth)) = stack.pop() {
        if is_pruned(n) || depth > MAX_DEPTH || excluded.contains(&addr(n)) {
            continue;
        }
        if n.identifier.as_deref().map(str::trim) == Some(INPUT_HOST_ID)
            && !n.frame.is_empty()
            && holds(&n.frame)
        {
            area = bounding(area, n.frame);
        }
        stack.extend(n.children.iter().map(|c| (c, depth + 1)));
    }
    if bounds.is_empty() {
        area
    } else {
        area.intersection(&bounds).unwrap_or(area)
    }
}

/// One screen as both the digest and compaction read it (see the module docs).
pub(crate) struct Layout<'a> {
    excluded: HashSet<usize>,
    /// Roots of the subtrees whose text is not hashed (keyboards, the input assistant).
    quiet: HashMap<usize, Quiet>,
    /// Every node whose text is not hashed: the quiet subtrees' nodes inside their root's
    /// frame (ninth review; see the module docs).
    quiet_nodes: HashMap<usize, Quiet>,
    /// The topmost shown Alert/Sheet (last in document order), outside excluded subtrees.
    pub alert: Option<&'a UiNode>,
    /// The first shown keyboard: its area occludes, its Return key is a cue.
    pub keyboard: Option<&'a UiNode>,
    /// Audit of real trees (C4): where the keyboard takes touches — larger than the
    /// `Keyboard` node (see [`keyboard_area`]).
    pub keyboard_area: Option<Rect>,
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
                Role::Keyboard if !out && keyboard.is_none() && !n.frame.is_empty() => {
                    keyboard = Some(n);
                }
                _ => {}
            }
            stack.extend(n.children.iter().rev().map(|c| (c, out)));
        }
        let keyboard_area = keyboard.map(|k| keyboard_area(screen, k, &excluded));
        let occluder = if alert.is_some() { None } else { keyboard_area };
        let quiet = keyboard_parts(screen, &excluded);
        let quiet_nodes = quiet_nodes(screen, &quiet, &excluded);
        Layout {
            excluded,
            quiet,
            quiet_nodes,
            alert,
            keyboard,
            keyboard_area,
            bounds: screen.bounds(),
            occluder,
        }
    }

    /// Whether `node` is the first shown keyboard ([`Layout::keyboard`]), whose drawn frame
    /// is the keyboard's whole area ([`Layout::keyboard_area`]).
    pub fn is_keyboard(&self, node: &UiNode) -> bool {
        self.keyboard.is_some_and(|k| std::ptr::eq(k, node))
    }

    /// Whether `node` (and so its whole subtree) is excluded.
    pub fn is_excluded(&self, node: &UiNode) -> bool {
        !self.excluded.is_empty() && self.excluded.contains(&addr(node))
    }

    /// Whether `node` is the root of a subtree whose text is not hashed, and how.
    pub fn quiet_root(&self, node: &UiNode) -> Option<Quiet> {
        if self.quiet.is_empty() {
            return None;
        }
        self.quiet.get(&addr(node)).copied()
    }

    /// How much of `node`'s own text is hashed (and so may be listed): [`Quiet::No`] for
    /// ordinary content, including a node of a quiet subtree that reaches outside its
    /// root's frame (ninth review).
    pub fn quiet_of(&self, node: &UiNode) -> Quiet {
        if self.quiet_nodes.is_empty() {
            return Quiet::No;
        }
        self.quiet_nodes
            .get(&addr(node))
            .copied()
            .unwrap_or(Quiet::No)
    }

    /// Whether `node`'s own text is left out (see [`Layout::quiet_of`]): it is never
    /// listed nor read, but its children may be ordinary content.
    pub fn is_quiet(&self, node: &UiNode) -> bool {
        self.quiet_of(node) != Quiet::No
    }

    /// `frame` clipped to the screen (`None` when nothing of it is on screen): where a
    /// node is drawn, whether or not it is shown.
    pub fn on_screen(&self, frame: Rect) -> Option<Rect> {
        if frame.is_empty() {
            return None;
        }
        if self.bounds.is_empty() {
            Some(frame)
        } else {
            frame.intersection(&self.bounds)
        }
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

    /// Visits every non-excluded node of `root` in depth-first pre-order with what is known
    /// of it ([`Visit`]: depth, shown, drawn, quiet, keyboard window). Iterative: any depth.
    pub fn walk(&self, root: &'a UiNode, mut f: impl FnMut(&'a UiNode, Visit)) {
        self.walk_pruning(root, |n, v| {
            f(n, v);
            true
        });
    }

    /// [`Layout::walk`], but a node's children are visited only when `f` returns `true` for
    /// it (the digest hashes a canonical run of order-free subtrees in one record).
    pub fn walk_pruning(&self, root: &'a UiNode, mut f: impl FnMut(&'a UiNode, Visit) -> bool) {
        struct Frame<'n> {
            node: &'n UiNode,
            depth: u32,
            /// An ancestor is pruned or past the cap.
            hidden_above: bool,
            in_scope: bool,
            quiet_clip: Option<Rect>,
            in_content: bool,
        }
        let mut stack: Vec<Frame<'a>> = vec![Frame {
            node: root,
            depth: 0,
            hidden_above: false,
            in_scope: self.alert.is_none(),
            quiet_clip: None,
            in_content: false,
        }];
        while let Some(fr) = stack.pop() {
            let node = fr.node;
            if self.is_excluded(node) {
                continue;
            }
            let cut = fr.hidden_above
                || is_pruned(node)
                || usize::try_from(fr.depth).is_ok_and(|d| d > MAX_DEPTH);
            let in_scope = fr.in_scope || self.alert.is_some_and(|a| std::ptr::eq(a, node));
            let shown = !cut && in_scope && self.shown_frame(node.frame).is_some();
            // The node's own text: reduced only inside its quiet root's frame (ninth review).
            let quiet = self.quiet_of(node);
            // The outermost quiet root clips the subtree's quiet nodes; an escaped node is
            // drawn where its frame says.
            let sub_clip = fr
                .quiet_clip
                .or_else(|| self.quiet_root(node).map(|_| node.frame));
            let quiet_clip = sub_clip.filter(|_| quiet != Quiet::No);
            let descend = f(
                node,
                Visit {
                    depth: fr.depth,
                    shown,
                    live: !cut && in_scope,
                    in_scope,
                    quiet,
                    quiet_clip,
                    in_content: fr.in_content,
                },
            );
            if !descend {
                continue;
            }
            let child = fr.depth.saturating_add(1);
            let in_content = fr.in_content || is_content_role(&node.role);
            stack.extend(node.children.iter().rev().map(|c| Frame {
                node: c,
                depth: child,
                hidden_above: cut,
                in_scope,
                quiet_clip: sub_clip,
                in_content,
            }));
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
