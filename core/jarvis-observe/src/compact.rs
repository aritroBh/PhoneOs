//! Flattening the accessibility tree into the indexed element list.
//!
//! # Visibility: decided per node, from the node's own frame and flag
//!
//! The runner sets `visible` from the node's *own* frame only (non-empty and intersecting the
//! screen, see `UiTreeBuilder.isVisible` on the Swift side). It never looks at ancestors, so
//! `[hidden]` is a statement about the frame, not a verdict on the subtree: SpringBoard wraps
//! its home-screen icons in `Button {{0, 0}, {0, 0}} [hidden]` and the icons inside are on
//! screen. The rule is therefore:
//!
//! * A node is **pruned**, together with its whole subtree, when it is flagged invisible *and*
//!   has a real (non-empty) frame: it is off-screen or hidden by the app, and what it contains
//!   is not reachable (the next home-screen page at x = 447 on a 440-wide screen, the
//!   `left-of-home-scroll-view` at x = -465, a hidden alert).
//! * A node with an **empty frame** and children is *transparent*: a pure grouping node
//!   whatever its role (`Button`, `Cell`, `Other`). It is never listed itself and does not hide
//!   anything; each child is judged by its own frame and flag.
//! * A **leaf** is listed only if its own frame is non-empty, it is not flagged hidden and
//!   enough of it is on screen (see [`compact`]).
//!
//! # Nested controls: the tap lands on the innermost one
//!
//! The agent taps the centre of an element's frame. SwiftUI reports a `Toggle` as a `Switch`
//! spanning the whole row around a `Switch` that is the real control, and a tap on the row's
//! centre toggles nothing. A listed `Button`/`Link`/`Switch`/`Slider`/`Picker`/`Tab` or text
//! input that contains exactly one same-role descendant (nearest one, any depth, ignoring
//! pruned or area-less nodes) keeps its own label/value and takes the frame of that
//! descendant, repeatedly, when the descendant is fully inside (1 pt tolerance) and is the
//! same control: no label, value, identifier or placeholder that differs from the wrapper's
//! (a missing one is compatible, and fills in what the wrapper lacks). Two inner candidates,
//! an inner control sticking out, or an inner control with its own different label (a card
//! `Button "Product"` around a `Button "Add"`) leave the wrapper's frame alone. Cells are not
//! refined: a Cell's frame is the row, and a Cell inside a Cell is a sub-row.
//!
//! Ninth review: a step is taken only when the wrapper shows no labelled content (static
//! texts, labelled images) outside the inner control's frame, or the inner control covers at
//! least 80 % of the wrapper's area. A card `Button "Wool socks"` showing its title and
//! price beside an unlabelled trash button is the card (the trash button then contests the
//! card's tap point as a control nested in it). A refined element's label also carries the
//! labels and image names drawn inside the inner control's frame that it does not already
//! say (`Wool socks, trash`): the tap lands there, so that is what classification and the
//! owner's question read.

use std::collections::{BTreeMap, BTreeSet};
use std::ptr;
use std::rc::Rc;

use jarvis_proto::{
    BarNode, CompactScreen, Element, RawInput, RawNode, Rect, Role, Screen, ScreenCues, UiNode,
};

use crate::fingerprint::{identity_key, FingerprintAssigner};
use crate::layout::{addr, Layout, Quiet, RawIndex};

/// Levels below the root read (`jarvis_proto::MAX_TREE_DEPTH`, the one bound; review pass
/// 30: the intakes allowed 200 and 256 while the walks stopped at 160, silently). A deeper node is
/// not read, and the screen says so (`ScreenCues::truncated`).
pub(crate) const MAX_DEPTH: usize = jarvis_proto::MAX_TREE_DEPTH;
/// Vertical jitter (points) tolerated between elements of the same visual row.
const ROW_TOLERANCE: f64 = 6.0;
/// At most this many child texts are merged into one label.
const MAX_MERGED_PARTS: usize = 12;
/// How far (points) an inner control may stick out of its wrapper and still count as inside.
const NEST_TOLERANCE: f64 = 1.0;

/// An element before filtering by geometry.
struct Cand {
    role: Role,
    label: String,
    value: Option<String>,
    placeholder: Option<String>,
    frame: Rect,
    enabled: bool,
    focused: bool,
    selected: bool,
    secure: bool,
    fingerprint: u64,
    /// Nodes sharing a group are siblings or cell-mates (used to drop duplicate texts).
    group: usize,
    /// The raw node's index in the digest's node order (see [`CompactView::sources`]).
    source: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Skip,
    Container,
    Input,
    Control,
    Text,
    Image,
    Mergeable,
    Leaf,
}

/// A node with no area of its own that still has children: a pure grouping node. SpringBoard
/// wraps its icons in `Button {{0, 0}, {0, 0}} [hidden]`. It says nothing about its children,
/// which are judged by their own frames and flags.
fn is_transparent(node: &UiNode) -> bool {
    node.frame.is_empty() && !node.children.is_empty()
}

/// A node that is positively hidden: flagged invisible while having a real frame (off-screen
/// or hidden by the app). Its whole subtree is out. An invisible node with an empty frame is
/// not hidden in this sense: the runner flags every zero-size node, see [`is_transparent`].
pub(crate) fn is_pruned(node: &UiNode) -> bool {
    !node.visible && !node.frame.is_empty()
}

fn kind(node: &UiNode) -> Kind {
    match &node.role {
        Role::Keyboard | Role::Key => Kind::Skip,
        _ if is_transparent(node) => Kind::Container,
        r if r.is_text_input() => Kind::Input,
        Role::Button | Role::Link | Role::Switch | Role::Slider | Role::Picker | Role::Tab => {
            Kind::Control
        }
        // Probe run 134: an edit menu's `MenuItem` is pressed like a button.
        r if r.is_menu_item() => Kind::Control,
        Role::StaticText => Kind::Text,
        Role::Image => Kind::Image,
        Role::Cell => Kind::Mergeable,
        Role::Other(_) if node.children.is_empty() => Kind::Leaf,
        _ => Kind::Container,
    }
}

/// Collapses runs of whitespace (including newlines) into single spaces.
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_opt(s: &Option<String>) -> String {
    s.as_deref().map(clean).unwrap_or_default()
}

fn non_empty(s: &Option<String>) -> Option<String> {
    let c = clean_opt(s);
    (!c.is_empty()).then_some(c)
}

/// Whether `node` is secure (flag or role): its value is never hashed, so it is never
/// copied anywhere into the compact screen, whatever its role.
fn is_secure(node: &UiNode) -> bool {
    node.secure || node.role == Role::SecureTextField
}

/// `node`'s value when it may be shown (cleaned, not blank): never a secure node's.
fn shown_value(node: &UiNode) -> Option<String> {
    if is_secure(node) {
        None
    } else {
        non_empty(&node.value)
    }
}

/// Indices of `frames` in reading order: rows top to bottom (tolerating jitter), left to
/// right within a row. An element joins the current row when its top edge or its vertical
/// centre is within [`ROW_TOLERANCE`] of the row's first element. Ties keep original order.
fn reading_order(frames: &[Rect]) -> Vec<usize> {
    let top = |i: usize| frames.get(i).map_or(0.0, |f| f.y);
    let cy = |i: usize| frames.get(i).map_or(0.0, |f| f.center().y);
    let x = |i: usize| frames.get(i).map_or(0.0, |f| f.x);
    let mut idx: Vec<usize> = (0..frames.len()).collect();
    idx.sort_by(|&a, &b| {
        top(a)
            .total_cmp(&top(b))
            .then(x(a).total_cmp(&x(b)))
            .then(a.cmp(&b))
    });
    let mut out = Vec::with_capacity(idx.len());
    let mut row: Vec<usize> = Vec::new();
    for i in idx {
        let same_row = row.first().is_some_and(|&a| {
            (top(i) - top(a)).abs() <= ROW_TOLERANCE || (cy(i) - cy(a)).abs() <= ROW_TOLERANCE
        });
        if !row.is_empty() && !same_row {
            flush_row(&mut row, &mut out, x);
        }
        row.push(i);
    }
    flush_row(&mut row, &mut out, x);
    out
}

fn flush_row(row: &mut Vec<usize>, out: &mut Vec<usize>, x: impl Fn(usize) -> f64) {
    row.sort_by(|&a, &b| x(a).total_cmp(&x(b)).then(a.cmp(&b)));
    out.append(row);
}

fn is_live(node: &UiNode) -> bool {
    !is_pruned(node) && kind(node) != Kind::Skip
}

fn is_interactive_kind(node: &UiNode) -> bool {
    matches!(kind(node), Kind::Control | Kind::Input)
}

/// Where a node's subtree lies in the flat pre-order lists of [`Summaries`].
#[derive(Clone, Copy, Default)]
struct Span {
    t0: usize,
    t1: usize,
    g0: usize,
    g1: usize,
    i0: usize,
    i1: usize,
}

/// Per-subtree summaries computed in one pass: texts, labelled images and top-level
/// interactive nodes, in document order, plus each cell/control's range into them.
struct Summaries<'a> {
    texts: Vec<(Rect, String)>,
    images: Vec<(Rect, String)>,
    interactive: Vec<&'a UiNode>,
    spans: BTreeMap<usize, Span>,
}

impl<'a> Summaries<'a> {
    fn build(root: &'a UiNode, layout: &Layout<'_>) -> Self {
        let mut s = Summaries {
            texts: Vec::new(),
            images: Vec::new(),
            interactive: Vec::new(),
            spans: BTreeMap::new(),
        };
        // (node, depth, inside an interactive node, span start, is exit marker)
        let mut stack: Vec<(&'a UiNode, usize, bool, Span, bool)> =
            vec![(root, 0, false, Span::default(), false)];
        while let Some((node, depth, in_inter, start, exit)) = stack.pop() {
            if exit {
                let k = kind(node);
                if matches!(k, Kind::Control | Kind::Input | Kind::Mergeable) {
                    s.spans.insert(
                        addr(node),
                        Span {
                            t1: s.texts.len(),
                            g1: s.images.len(),
                            i1: s.interactive.len(),
                            ..start
                        },
                    );
                }
                continue;
            }
            if is_pruned(node) || depth > MAX_DEPTH || layout.is_excluded(node) {
                continue;
            }
            if layout.is_quiet(node) {
                // Its text is not hashed: never read. Its children may be ordinary content
                // (ninth review: a node reaching outside the keyboard's or the prediction
                // bar's frame).
                for c in node.children.iter().rev() {
                    stack.push((c, depth + 1, in_inter, Span::default(), false));
                }
                continue;
            }
            if !is_live(node) {
                continue;
            }
            let label = non_empty(&node.label).or_else(|| shown_value(node));
            match (&node.role, label) {
                (Role::StaticText, Some(l)) => s.texts.push((node.frame, l)),
                (Role::Image, Some(l)) => s.images.push((node.frame, l)),
                _ => {}
            }
            let inter = is_interactive_kind(node);
            if inter && !in_inter {
                s.interactive.push(node);
            }
            let start = Span {
                t0: s.texts.len(),
                g0: s.images.len(),
                i0: s.interactive.len(),
                ..Span::default()
            };
            stack.push((node, depth, in_inter, start, true));
            for c in node.children.iter().rev() {
                stack.push((c, depth + 1, in_inter || inter, Span::default(), false));
            }
        }
        s
    }

    fn span(&self, node: &UiNode) -> Span {
        self.spans.get(&addr(node)).copied().unwrap_or_default()
    }

    /// Count of top-level interactive descendants and the first one.
    fn interactive_in(&self, sp: Span) -> (usize, Option<&'a UiNode>) {
        let count = sp.i1.saturating_sub(sp.i0);
        (
            count,
            self.interactive.get(sp.i0).copied().filter(|_| count > 0),
        )
    }

    /// The labelled texts and images of the span (frames and words), in document order.
    fn labelled(&self, sp: Span) -> impl Iterator<Item = &(Rect, String)> {
        self.texts
            .get(sp.t0..sp.t1)
            .unwrap_or_default()
            .iter()
            .chain(self.images.get(sp.g0..sp.g1).unwrap_or_default())
    }

    /// Texts in the span in reading order, joined; falls back to labelled images. At most
    /// [`MAX_MERGED_PARTS`] parts, then a trailing `…` marks that more text exists.
    fn merged_text(&self, sp: Span) -> String {
        let texts = self.texts.get(sp.t0..sp.t1).unwrap_or_default();
        let parts = if texts.is_empty() {
            self.images.get(sp.g0..sp.g1).unwrap_or_default()
        } else {
            texts
        };
        let frames: Vec<Rect> = parts.iter().map(|(f, _)| *f).collect();
        let mut ordered: Vec<&str> = Vec::new();
        for i in reading_order(&frames) {
            if let Some((_, t)) = parts.get(i) {
                if ordered.last() != Some(&t.as_str()) {
                    ordered.push(t);
                }
            }
        }
        let more = ordered.len() > MAX_MERGED_PARTS;
        ordered.truncate(MAX_MERGED_PARTS);
        let mut out = ordered.join(", ");
        if more {
            out.push_str(", …");
        }
        out
    }
}

/// What a control is when tapped: its own geometry and state, or those of the same control
/// one level down when the node is only a wrapper around it (see [`Target::of`]).
struct Target {
    frame: Rect,
    /// When the target was refined to an inner control: the labels and image names of the
    /// wrapper's subtree drawn inside the inner control's frame (ninth review: what the tap
    /// lands on says what it does; an unlabelled trash button inside a labelled wrapper).
    extra: Vec<String>,
    /// Label of the inner control, used when the wrapper has none.
    label: Option<String>,
    value: Option<String>,
    placeholder: Option<String>,
    enabled: bool,
    focused: bool,
    selected: bool,
}

/// Two labels/values/identifiers describe the same control unless both are set and differ.
fn compatible(a: &Option<String>, b: &Option<String>) -> bool {
    let (a, b) = (clean_opt(a), clean_opt(b));
    a.is_empty() || b.is_empty() || a.to_lowercase() == b.to_lowercase()
}

/// True when `inner` is the same control as `outer` seen from one level down: same role, and
/// nothing about it says it is something else (a different label, value, identifier or
/// placeholder). A card `Button "Product"` around a `Button "Add"` is two controls.
fn same_control(outer: &UiNode, inner: &UiNode) -> bool {
    compatible(&outer.label, &inner.label)
        && compatible(&outer.value, &inner.value)
        && compatible(&outer.identifier, &inner.identifier)
        && compatible(&outer.placeholder, &inner.placeholder)
}

fn contains_rect(outer: &Rect, inner: &Rect) -> bool {
    inner.x >= outer.x - NEST_TOLERANCE
        && inner.y >= outer.y - NEST_TOLERANCE
        && inner.max_x() <= outer.max_x() + NEST_TOLERANCE
        && inner.max_y() <= outer.max_y() + NEST_TOLERANCE
}

/// The only nearest same-role descendant of `node` (matches are not searched below), or `None`
/// when there is none or more than one. Pruned subtrees and nodes without area do not count.
/// Costs the size of `node`'s subtree minus the part below the match.
fn single_nested<'a>(node: &'a UiNode, layout: &Layout<'_>) -> Option<&'a UiNode> {
    let mut found: Option<&'a UiNode> = None;
    let mut stack: Vec<(&'a UiNode, usize)> = node.children.iter().rev().map(|c| (c, 1)).collect();
    while let Some((n, depth)) = stack.pop() {
        if is_pruned(n) || depth > MAX_DEPTH || layout.is_excluded(n) {
            continue;
        }
        if n.role == node.role && !n.frame.is_empty() && !layout.is_quiet(n) {
            if found.is_some() {
                return None;
            }
            found = Some(n);
            continue;
        }
        stack.extend(n.children.iter().rev().map(|c| (c, depth + 1)));
    }
    found
}

/// Least share of a wrapper's area an inner control must cover to be refined to even when
/// the wrapper shows labelled content outside it (ninth review).
const REFINE_MIN_COVER: f64 = 0.8;

impl Target {
    /// The node itself, unless it wraps a single, fully contained control of the same role that
    /// is compatible with it. SwiftUI reports a `Toggle` as `Switch "Show tips" {{20, 168},
    /// {400, 58}}` (the row) around `Switch {{339, 183}, {63, 28}}` (the actual switch), and
    /// only a tap on the inner one toggles. The target then takes the innermost frame; label
    /// and value stay the outer node's (the inner values are used where the outer has none).
    /// The chain is followed while each step stays unambiguous.
    ///
    /// **Ninth review.** A step is taken only when the wrapper shows no labelled content
    /// (static texts, labelled images) outside the inner control's frame, or the inner
    /// control covers at least [`REFINE_MIN_COVER`] of the wrapper's area: a card `Button
    /// "Wool socks"` showing its title and price beside an unlabelled trash button is the
    /// card, not the trash button (the trash button then contests the card's tap point as a
    /// nested control). When refined, the labels and image names drawn inside the inner
    /// frame are kept in [`Target::extra`] for the element's label.
    fn of(node: &UiNode, layout: &Layout<'_>, sums: &Summaries<'_>) -> Target {
        let mut t = Target {
            extra: Vec::new(),
            frame: node.frame,
            label: None,
            value: None,
            placeholder: None,
            enabled: node.enabled,
            focused: node.focused,
            selected: node.selected,
        };
        // Roles that are rows or containers of other things keep the wrapper's frame: a Cell
        // nested in a Cell is a sub-row, and the outer one is the row the user sees.
        if !matches!(kind(node), Kind::Control | Kind::Input) {
            return t;
        }
        let mut current = node;
        while let Some(inner) = single_nested(current, layout) {
            if !same_control(node, inner) || !contains_rect(&current.frame, &inner.frame) {
                break;
            }
            let covers = current.frame.area() > 0.0
                && inner.frame.area() >= REFINE_MIN_COVER * current.frame.area();
            let content_outside = sums
                .labelled(sums.span(current))
                .any(|(f, _)| !contains_rect(&inner.frame, f));
            if !covers && content_outside {
                break;
            }
            t.frame = inner.frame;
            t.label = t.label.or_else(|| non_empty(&inner.label));
            t.value = t.value.or_else(|| shown_value(inner));
            t.placeholder = t.placeholder.or_else(|| non_empty(&inner.placeholder));
            t.enabled &= inner.enabled;
            t.focused |= inner.focused;
            t.selected |= inner.selected;
            current = inner;
        }
        if !std::ptr::eq(current, node) {
            let f = t.frame;
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for (r, w) in sums.labelled(sums.span(node)) {
                let c = r.center();
                let inside = c.x >= f.x && c.x <= f.max_x() && c.y >= f.y && c.y <= f.max_y();
                if inside && t.extra.len() < MAX_MERGED_PARTS && seen.insert(w.to_lowercase()) {
                    t.extra.push(w.clone());
                }
            }
        }
        t
    }
}

/// `label` with the words of a refined target's inner control ([`Target::extra`]) that it
/// does not already say appended (`Wool socks, trash`).
fn with_inner_words(label: String, extra: &[String]) -> String {
    let mut out = label;
    for w in extra {
        if !out.to_lowercase().contains(&w.to_lowercase()) {
            if !out.is_empty() {
                out.push_str(", ");
            }
            out.push_str(w);
        }
    }
    out
}

#[derive(Clone)]
struct State {
    depth: usize,
    emit: bool,
    group: usize,
    /// Keep `group` for descendants (inside a cell that holds controls).
    sticky: bool,
    /// Static texts below were folded into an unlabeled control's label.
    skip_text: bool,
    inherit: Option<String>,
    /// Nearest enclosing row/container label, folded into fingerprints.
    context: Rc<str>,
}

struct Walker<'a> {
    layout: &'a Layout<'a>,
    index: &'a RawIndex,
    sums: Summaries<'a>,
    scope: Option<&'a UiNode>,
    cands: Vec<Cand>,
    ordinals: FingerprintAssigner,
    path: Vec<&'a Role>,
    next_id: usize,
}

impl<'a> Walker<'a> {
    fn walk(&mut self, node: &'a UiNode, st: &State) {
        if is_pruned(node) || st.depth > MAX_DEPTH || self.layout.is_excluded(node) {
            return;
        }
        if self.layout.is_quiet(node) {
            // Keyboard or prediction-bar text: never listed. A child reaching outside the
            // subtree root's frame is ordinary content (ninth review).
            let child = State {
                depth: st.depth + 1,
                ..st.clone()
            };
            for c in &node.children {
                self.walk(c, &child);
            }
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let emit = st.emit || self.scope.is_some_and(|s| ptr::eq(s, node));
        let st = State { emit, ..st.clone() };
        match kind(node) {
            Kind::Skip => {}
            Kind::Container => {
                let mut inner = st.clone();
                if !matches!(node.role, Role::Application | Role::Window) {
                    if let Some(l) = non_empty(&node.label) {
                        inner.context = Rc::from(l.as_str());
                    }
                }
                self.descend(node, id, &inner);
            }
            Kind::Input => {
                let target = Target::of(node, self.layout, &self.sums);
                let label = non_empty(&node.label)
                    .or_else(|| non_empty(&node.placeholder))
                    .or_else(|| target.label.clone())
                    .or_else(|| target.placeholder.clone())
                    .or_else(|| st.inherit.clone())
                    .or_else(|| non_empty(&node.identifier))
                    .unwrap_or_default();
                self.push(node, label, &st, Some(target));
            }
            Kind::Control => {
                let target = Target::of(node, self.layout, &self.sums);
                let label = non_empty(&node.label)
                    .or_else(|| target.label.clone())
                    .or_else(|| {
                        Some(self.sums.merged_text(self.sums.span(node))).filter(|t| !t.is_empty())
                    })
                    .or_else(|| st.inherit.clone())
                    .or_else(|| non_empty(&node.identifier))
                    .unwrap_or_default();
                let label = with_inner_words(label, &target.extra);
                self.push(node, label, &st, Some(target));
            }
            Kind::Text => {
                if let Some(label) = non_empty(&node.label).or_else(|| shown_value(node)) {
                    if !st.skip_text {
                        self.push(node, label, &st, None);
                    }
                }
            }
            Kind::Image | Kind::Leaf => {
                if let Some(label) = non_empty(&node.label) {
                    self.push(node, label, &st, None);
                }
            }
            Kind::Mergeable => self.mergeable(node, id, &st),
        }
    }

    fn mergeable(&mut self, node: &'a UiNode, id: usize, st: &State) {
        let sp = self.sums.span(node);
        let (count, first) = self.sums.interactive_in(sp);
        let is_cell = node.role == Role::Cell;
        if count == 0 {
            let label = non_empty(&node.label).unwrap_or_else(|| self.sums.merged_text(sp));
            let label = if label.is_empty() {
                non_empty(&node.identifier).unwrap_or_default()
            } else {
                label
            };
            if label.is_empty() && !is_cell {
                self.descend(node, id, st);
            } else if !label.is_empty() {
                self.push(node, label, st, None);
            }
            return;
        }
        // A row holding controls is not itself the target: list the controls, and lend the
        // row's text to a single unlabeled control.
        let merged = self.sums.merged_text(sp);
        let unlabeled = first.is_some_and(|f| {
            non_empty(&f.label).is_none()
                && (!f.role.is_text_input() || non_empty(&f.placeholder).is_none())
        });
        let lend = count == 1 && unlabeled && !merged.is_empty();
        let inner = State {
            sticky: true,
            group: id,
            context: if let Some(l) = non_empty(&node.label) {
                Rc::from(l.as_str())
            } else if merged.is_empty() {
                st.context.clone()
            } else {
                Rc::from(merged.as_str())
            },
            skip_text: st.skip_text || lend,
            inherit: if lend {
                Some(merged)
            } else {
                st.inherit.clone()
            },
            ..st.clone()
        };
        self.descend(node, id, &inner);
    }

    fn descend(&mut self, node: &'a UiNode, id: usize, st: &State) {
        self.path.push(&node.role);
        let child = State {
            depth: st.depth + 1,
            group: if st.sticky { st.group } else { id },
            ..st.clone()
        };
        for c in &node.children {
            self.walk(c, &child);
        }
        self.path.pop();
    }

    fn push(&mut self, node: &UiNode, label: String, st: &State, target: Option<Target>) {
        if !st.emit {
            return;
        }
        let mut key = identity_key(node.identifier.as_deref().filter(|s| !s.is_empty()), &label);
        if !st.context.is_empty() {
            key.push_str("\u{1}c:");
            key.push_str(&st.context);
        }
        let fingerprint = self.ordinals.assign(&node.role, &key, &self.path);
        let secure = is_secure(node);
        let (frame, enabled, focused, selected) = match &target {
            Some(t) => (t.frame, t.enabled, t.focused, t.selected),
            None => (node.frame, node.enabled, node.focused, node.selected),
        };
        let placeholder = non_empty(&node.placeholder)
            .or_else(|| target.as_ref().and_then(|t| t.placeholder.clone()));
        let value = if secure || node.role == Role::StaticText {
            None
        } else {
            non_empty(&node.value)
                .or_else(|| target.as_ref().and_then(|t| t.value.clone()))
                .filter(|v| placeholder.as_ref() != Some(v))
        };
        self.cands.push(Cand {
            role: node.role.clone(),
            label,
            value,
            placeholder: if node.role.is_text_input() {
                placeholder
            } else {
                None
            },
            frame,
            enabled,
            focused,
            selected,
            secure,
            fingerprint,
            group: st.group,
            source: self
                .index
                .by_addr
                .get(&addr(node))
                .copied()
                .unwrap_or(u32::MAX),
        });
    }
}

/// Longest Return-key label kept, in characters (real ones are one or two words).
const MAX_RETURN_KEY_CHARS: usize = 24;

/// Labels iOS gives the keyboard's Return key (`UIReturnKeyType`, English), lowercased: the
/// second way to find the key (a tree whose key carries no type identifier).
const RETURN_KEY_LABELS: &[&str] = &[
    "return",
    "go",
    "google",
    "join",
    "next",
    "route",
    "search",
    "send",
    "yahoo",
    "done",
    "emergency call",
    "continue",
];

/// Audit of real trees (C1): the identifiers iOS gives the Return key — the
/// `UIReturnKeyType` case names, compared whole and case-insensitively — and the type word
/// each stands for (`ScreenCues::return_key_type`). Probe round 3 (run 139, p4b: all twelve
/// `UIReturnKeyType`s captured on iOS 26.5) gave the real identifiers, which are **not** all
/// the type names: `Return`, `Go`, `Google` (label "search"), `Join:` and `Next:` (with a
/// trailing colon), `Route`, `Search`, `Send`, `Yahoo` (label "search"), `Done` and
/// `Emergency call` (with a space; label "Emergency\nCall"); `.continue`'s key has **no**
/// identifier (label "continue"), so it has no entry: its Return is of unknown effect
/// (`ScreenCues::return_key_unknown`). Every entry is evidenced by that capture.
const RETURN_KEY_TYPES: &[(&str, &str)] = &[
    ("return", "return"),
    ("go", "go"),
    ("google", "google"),
    ("join:", "join"),
    ("next:", "next"),
    ("route", "route"),
    ("search", "search"),
    ("send", "send"),
    ("yahoo", "yahoo"),
    ("done", "done"),
    ("emergency call", "emergency call"),
];

/// The Return-key type an identifier names ([`RETURN_KEY_TYPES`]), if any.
fn return_key_type_of(identifier: &str) -> Option<&'static str> {
    let id = identifier.trim().to_ascii_lowercase();
    RETURN_KEY_TYPES
        .iter()
        .find(|(name, _)| *name == id)
        .map(|(_, word)| *word)
}

/// The keyboard's Return key as compaction reads it: its label (bounded) and its type by
/// identifier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ReturnKey {
    /// The key's label, whitespace collapsed, at most [`MAX_RETURN_KEY_CHARS`] characters.
    pub label: Option<String>,
    /// The type its identifier names ([`RETURN_KEY_TYPES`]).
    pub kind: Option<&'static str>,
}

/// `keyboard`'s Return key: the key or button **inside the `Keyboard` subtree** whose
/// identifier names a Return-key type (audit C1: what UIKit reports, whatever the system
/// language), else the one whose whole label is a known Return-key label (no type then).
/// Hidden and area-less keys are ignored; whitespace is collapsed and the label bounded.
/// Keys stay out of the element list either way: this is a fact about the screen, not a
/// control. A button with such an identifier outside the keyboard is never read here.
pub(crate) fn return_key(keyboard: &UiNode) -> Option<ReturnKey> {
    let mut by_label = None;
    let mut stack: Vec<(&UiNode, usize)> = vec![(keyboard, 0)];
    while let Some((n, depth)) = stack.pop() {
        if is_pruned(n) || depth > MAX_DEPTH {
            continue;
        }
        if matches!(n.role, Role::Key | Role::Button) && !n.frame.is_empty() {
            let label = clean_opt(&n.label);
            let label: Option<String> =
                (!label.is_empty()).then(|| label.chars().take(MAX_RETURN_KEY_CHARS).collect());
            if let Some(kind) = return_key_type_of(&clean_opt(&n.identifier)) {
                return Some(ReturnKey {
                    label,
                    kind: Some(kind),
                });
            }
            let known = label
                .as_deref()
                .is_some_and(|l| RETURN_KEY_LABELS.contains(&l.to_lowercase().as_str()));
            if by_label.is_none() && known {
                by_label = Some(ReturnKey { label, kind: None });
            }
        }
        stack.extend(n.children.iter().rev().map(|c| (c, depth + 1)));
    }
    by_label.or_else(|| corner_key(keyboard))
}

/// Probe round 3 (run 139, F3): the key at the keyboard's bottom-right — where the Return
/// key sat in all twelve `UIReturnKeyType` captures and under German (`Button "Öffnen"
/// id=Go {{329.3, 830}, {108.3, 56}}` in `Keyboard {{0, 655}, {440, 243}}`): the labelled key
/// or button of the `Keyboard` subtree whose frame reaches furthest right and down, when its
/// centre lies in the keyboard's bottom-right quarter. Used only to **find** the key whose
/// label the question quotes (no type: [`ReturnKey::kind`] stays `None`, so its Return is of
/// unknown effect and asks); never to exempt anything.
fn corner_key(keyboard: &UiNode) -> Option<ReturnKey> {
    let area = keyboard.frame;
    if area.is_empty() {
        return None;
    }
    let mid = area.center();
    let mut best: Option<(f64, String)> = None;
    let mut stack: Vec<(&UiNode, usize)> = vec![(keyboard, 0)];
    while let Some((n, depth)) = stack.pop() {
        if is_pruned(n) || depth > MAX_DEPTH {
            continue;
        }
        stack.extend(n.children.iter().map(|c| (c, depth + 1)));
        if !matches!(n.role, Role::Key | Role::Button) || n.frame.is_empty() {
            continue;
        }
        let c = n.frame.center();
        let label = clean_opt(&n.label);
        if label.is_empty() || c.x <= mid.x || c.y <= mid.y {
            continue;
        }
        let reach = n.frame.max_x() + n.frame.max_y();
        if best.as_ref().is_none_or(|(r, _)| reach > *r) {
            best = Some((reach, label.chars().take(MAX_RETURN_KEY_CHARS).collect()));
        }
    }
    best.map(|(_, label)| ReturnKey {
        label: Some(label),
        kind: None,
    })
}

fn round(f: &Rect) -> [i64; 4] {
    // Float to int saturates; frames are small screen coordinates.
    [f.x, f.y, f.width, f.height].map(|v| v.round() as i64)
}

/// A compact screen plus where its elements come from in the raw tree. Kept out of the
/// FFI: the agent holds it beside the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactView {
    /// The compact screen ([`compact`]).
    pub screen: CompactScreen,
    /// For each element of `screen` (by position), the index of the raw node it was listed
    /// from, in the digest's node order (depth-first pre-order over the nodes that are not
    /// excluded; see `screen_digest`). An approval pins its target's raw node by it.
    pub sources: Vec<u32>,
    /// For each raw node (by that index), its parent's index (`u32::MAX` for the root).
    pub parents: Vec<u32>,
    /// For each raw node, one past the index of its last descendant: node `j` is inside
    /// node `i`'s subtree when `i < j < ends[i]`.
    pub ends: Vec<u32>,
    /// Every shown `NavigationBar`/`TabBar`/`Toolbar` (eighth review) raw node with its
    /// shown frame, in node order, that has no web-view, scroll-view, table or collection
    /// ancestor (seventh review: WebKit reports an ARIA `tablist` as a tab bar); none while
    /// an alert or sheet is up
    /// (only its subtree is listed then). Which of them take precedence over the content
    /// under them is `jarvis_safety::tap_spot`'s call.
    pub bars: Vec<BarNode>,
    /// Every drawn raw node (no pruned ancestor, within the depth cap, inside the alert
    /// while one is up) with any part of its frame on screen, in node order, with that
    /// on-screen part: what a touch at a point could land on, whatever its role
    /// (`jarvis_safety::tap_spot`'s candidate covers). The `Keyboard` subtree and the input
    /// assistant's are flagged `keyboard`; a node whose text is not hashed carries no label.
    pub nodes: Vec<RawNode>,
    /// Every text input whose text the digest hashes (not excluded, not inside the
    /// keyboard or the input assistant), listed or not, shown or not, in node order (eighth
    /// review): a submit's summary names the fields of its form that are not listed (a
    /// compose view's `To:` scrolled out of view). Secure fields carry no value. While an
    /// alert or sheet is up, only those inside it.
    pub inputs: Vec<RawInput>,
    /// Thirteenth review: one mark per text input that holds text (not inside the keyboard
    /// or the input assistant; behind an alert or sheet too), in node order: a hash of its
    /// value, or for a secure field a hash of its length only (the secret is never kept).
    /// The session compares the marks of consecutive observations of an app, as multisets,
    /// to see text **enter** a field without `TypeText` (paste, AutoFill, dictation, a
    /// keyboard suggestion, drag and drop), whatever the field's label does meanwhile.
    pub input_marks: Vec<u64>,
    /// Seventeenth review (Rule T): every `WebView` raw node that starts a document (by its
    /// index, ascending), shown or not: a web view that is a structural wrapper of the web
    /// view it is nested in — the same frame, and that web view holds no content of its own
    /// beside it (eighteenth review) — is the same document (WebKit wraps a page in two or
    /// three web views of one frame: Safari's real tree) and is left out. A web view nested in
    /// another, or one beside listed native content in an app whose chrome shows no host,
    /// is a place of its own (`jarvis_safety::ScreenPlaces`); the place of an action is
    /// decided by whether its target's raw node lies inside one (`parents`/`ends`).
    pub webviews: Vec<u32>,
}

/// The mark of a text input's content in [`CompactView::input_marks`] (`None`: empty).
fn input_mark(node: &UiNode) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let value = node
        .value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())?;
    if node.placeholder.as_deref().map(str::trim) == Some(value) {
        return None;
    }
    // A fixed-key hasher: the marks of two observations are comparable.
    let mut h = std::hash::DefaultHasher::new();
    if is_secure(node) {
        (0u8, value.chars().count()).hash(&mut h);
    } else {
        (1u8, value.split_whitespace().collect::<Vec<_>>()).hash(&mut h);
    }
    Some(h.finish())
}

/// Flattens a screen to its indexed, ordered, deduplicated actionable/informative elements.
///
/// * Excluded subtrees (SpringBoard's status bar; see
///   `crate::layout`) are never listed.
/// * Invisible, zero-area, off-screen (or mostly off-screen) and keyboard-covered nodes are
///   dropped; partly visible ones keep the visible part of their frame. A hidden node with a
///   real frame takes its subtree with it, a zero-size one (flagged hidden by the runner) does
///   not: its children are judged on their own (see the module docs).
/// * A control wrapping a single compatible control of the same role (a SwiftUI `Toggle` row)
///   is listed with the wrapper's label and value and the inner control's frame.
/// * While an Alert/Sheet is up only its subtree is listed.
/// * Keyboard keys are never listed; `keyboard_visible` is set instead, and the Return
///   key's label goes to `cues.return_key` (it says whether Return sends, searches or goes)
///   and the type its identifier names to `cues.return_key_type` (audit C1: the identifier
///   is the `UIReturnKeyType` name in every language).
/// * Cells (and hittable `Other` rows) take their label from child static texts, and those
///   texts are not listed again. A row holding controls is not listed; its controls are.
/// * Secure fields never carry a value; a value equal to the placeholder is dropped.
pub fn compact(screen: &Screen) -> CompactScreen {
    compact_view(screen).screen
}

/// [`compact`], plus each element's raw node and the raw tree's shape ([`CompactView`]).
pub fn compact_view(screen: &Screen) -> CompactView {
    let layout = Layout::of(screen);
    let index = RawIndex::build(&layout, &screen.root);
    let alert = layout.alert;
    let mut walker = Walker {
        layout: &layout,
        index: &index,
        sums: Summaries::build(&screen.root, &layout),
        scope: alert,
        cands: Vec::new(),
        ordinals: FingerprintAssigner::default(),
        path: Vec::new(),
        next_id: 0,
    };
    let start = State {
        depth: 0,
        emit: alert.is_none(),
        group: 0,
        sticky: false,
        skip_text: false,
        inherit: None,
        context: Rc::from(""),
    };
    walker.walk(&screen.root, &start);
    // Every reached node's fingerprint, listed or not (an input scrolled out of view keeps
    // the fingerprint it had while listed).
    let prints: BTreeMap<u32, u64> = walker
        .cands
        .iter()
        .map(|c| (c.source, c.fingerprint))
        .collect();

    let mut bars = Vec::new();
    let mut nodes = Vec::new();
    let mut inputs = Vec::new();
    let mut input_marks = Vec::new();
    let mut webviews = Vec::new();
    // Probe run 134: the raw subtree ranges of the remote containers (`remote_container`),
    // disjoint once nested ones are folded into their outer one.
    let mut remote: Vec<(u32, u32)> = Vec::new();
    layout.walk(&screen.root, |node, v| {
        let Some(&i) = index.by_addr.get(&addr(node)) else {
            return;
        };
        if jarvis_safety::remote_container(node) && !remote.last().is_some_and(|&(_, e)| i < e) {
            let end = usize::try_from(i)
                .ok()
                .and_then(|k| index.ends.get(k))
                .copied()
                .unwrap_or(i);
            remote.push((i, end));
        }
        // Probe run 134: what a remote container holds is drawn by another process at
        // positions the tree does not give in screen coordinates: it is never a drawn
        // node a point could land on (it neither covers nor is covered).
        let unplaced = remote.last().is_some_and(|&(a, e)| i > a && i < e);
        if node.role == Role::WebView {
            webviews.push((i, node.frame));
        }
        // Thirteenth review: what every text input holds, behind an alert too (an alert
        // coming and going does not make text appear).
        if v.quiet == Quiet::No && node.role.is_text_input() {
            input_marks.extend(input_mark(node));
        }
        // While an alert or sheet is up the fields behind it are not what its buttons send.
        if v.quiet == Quiet::No && v.in_scope && node.role.is_text_input() {
            let placeholder = non_empty(&node.placeholder);
            inputs.push(RawInput {
                node: i,
                role: node.role.clone(),
                label: non_empty(&node.label),
                identifier: non_empty(&node.identifier),
                value: shown_value(node).filter(|v| placeholder.as_ref() != Some(v)),
                placeholder,
                secure: is_secure(node),
                on_screen: v.live && layout.on_screen(node.frame).is_some(),
                fingerprint: prints.get(&i).copied(),
            });
        }
        if v.live && !unplaced {
            // Audit C4: the first shown keyboard is drawn over its whole area (the input
            // host, the row under its keys), so a point there is on the keyboard.
            let drawn = if layout.is_keyboard(node) {
                layout.keyboard_area.and_then(|a| layout.on_screen(a))
            } else {
                layout
                    .on_screen(node.frame)
                    .and_then(|f| v.quiet_clip.map_or(Some(f), |c| f.intersection(&c)))
            };
            if let Some(frame) = drawn {
                let label = if v.quiet == Quiet::No {
                    non_empty(&node.label)
                } else {
                    None
                };
                // Text the node shows (never read inside the keyboard or the assistant, whose
                // nodes count as the keyboard instead).
                let text = v.quiet == Quiet::No
                    && (non_empty(&node.label).is_some() || shown_value(node).is_some());
                nodes.push(RawNode {
                    node: i,
                    role: node.role.clone(),
                    frame,
                    label,
                    text,
                    keyboard: v.quiet != Quiet::No,
                    banner: jarvis_safety::notification_banner(node),
                    grid_cell: jarvis_safety::grid_cell(node),
                    symbol: jarvis_safety::identifier_symbol_risk(node),
                    in_cell: usize::try_from(i)
                        .ok()
                        .and_then(|k| index.in_cell.get(k))
                        .copied()
                        .unwrap_or(false),
                    // Run 161: a list's or a row's whole frame, when it reaches off screen
                    // (a row whose swipe actions show is shifted out of its list).
                    whole: (matches!(node.role, Role::Cell | Role::Table | Role::CollectionView)
                        && node.frame != frame)
                        .then_some(node.frame),
                });
            }
        }
        // A bar inside a web, scroll, table or collection view is page content (WebKit
        // reports an ARIA `tablist` as a tab bar): never listed as a bar.
        if alert.is_some()
            || !v.shown
            || v.in_content
            || !matches!(
                node.role,
                Role::NavigationBar | Role::TabBar | Role::Toolbar
            )
        {
            return;
        }
        if let Some(frame) = layout.shown_frame(node.frame) {
            bars.push(BarNode {
                node: i,
                role: node.role.clone(),
                frame,
            });
        }
    });

    let bounds = screen.bounds();
    // Review pass 28 (M1): every shown keyboard's Return key is read; the cue is the
    // emergency-call key if any keyboard has one, else a send key, else the first found;
    // several keyboards, or typed keys that disagree, make Return of unknown effect.
    let keys: Vec<ReturnKey> = layout
        .keyboards
        .iter()
        .filter_map(|k| return_key(k))
        .collect();
    let pick = |kind: &str| keys.iter().find(|k| k.kind == Some(kind)).cloned();
    let return_key = pick("emergency call")
        .or_else(|| pick("send"))
        .or_else(|| keys.first().cloned())
        .unwrap_or_default();
    let several_keyboards = layout.keyboards.len() > 1
        || layout.odd_keyboards
        || keys
            .iter()
            .filter_map(|k| k.kind)
            .any(|k| Some(k) != return_key.kind);
    let mut kept: Vec<(Cand, Rect)> = walker
        .cands
        .into_iter()
        .filter_map(|c| layout.shown_frame(c.frame).map(|f| (c, f)))
        .collect();

    // Texts duplicating a control's label inside the same group are redundant.
    let control_labels: BTreeSet<(usize, String)> = kept
        .iter()
        .filter(|(c, _)| c.role != Role::StaticText)
        .map(|(c, _)| (c.group, c.label.to_lowercase()))
        .collect();
    let mut seen: BTreeSet<(String, String, String, [i64; 4])> = BTreeSet::new();
    kept.retain(|(c, f)| {
        if c.role == Role::StaticText && control_labels.contains(&(c.group, c.label.to_lowercase()))
        {
            return false;
        }
        seen.insert((
            c.role.as_str().to_owned(),
            c.label.clone(),
            c.value.clone().unwrap_or_default(),
            round(f),
        ))
    });

    let frames: Vec<Rect> = kept.iter().map(|(_, f)| *f).collect();
    let mut slots: Vec<Option<(Cand, Rect)>> = kept.into_iter().map(Some).collect();
    let mut elements = Vec::with_capacity(slots.len());
    let mut sources = Vec::with_capacity(slots.len());
    for i in reading_order(&frames) {
        if let Some((c, frame)) = slots.get_mut(i).and_then(Option::take) {
            sources.push(c.source);
            elements.push(Element {
                index: u32::try_from(elements.len() + 1).unwrap_or(u32::MAX),
                role: c.role,
                label: c.label,
                value: c.value,
                placeholder: c.placeholder,
                frame,
                enabled: c.enabled,
                focused: c.focused,
                selected: c.selected,
                secure: c.secure,
                fingerprint: c.fingerprint,
            });
        }
    }

    let mut screen_out = CompactScreen {
        app_bundle_id: screen.app_bundle_id.clone(),
        app_name: screen.app_name.clone(),
        size: screen.size,
        keyboard_visible: screen.keyboard_visible || layout.keyboard.is_some(),
        elements,
        cues: ScreenCues {
            return_key: return_key.label,
            return_key_type: return_key.kind.map(str::to_owned),
            // Probe round 3 (F3): a keyboard is up whose Return key's type no identifier
            // gives: what Return does is unknown (fail closed).
            return_key_unknown: (layout.keyboard.is_some() && return_key.kind.is_none())
                || several_keyboards,
            ..cues(alert.unwrap_or(&screen.root), bounds, &layout)
        },
    };
    // Probe run 134: the listed elements whose raw node lies inside a remote container have
    // no known position on screen (ascending index: `elements` is in index order).
    // Review pass 30 (M1): a node deeper than the walks read: the screen is incomplete.
    // Review pass 31 (L1): or an intake cut the tree there and said so (`Screen::tree_cut`).
    screen_out.cues.truncated = index.truncated || screen.tree_cut;
    screen_out.cues.unplaced = screen_out
        .elements
        .iter()
        .zip(&sources)
        .filter(|(_, s)| {
            let s = **s;
            let k = remote.partition_point(|&(a, _)| a < s);
            k > 0 && remote.get(k - 1).is_some_and(|&(a, e)| s > a && s < e)
        })
        .map(|(e, _)| e.index)
        .collect();
    // Which listed controls a tap would be refused for (the tap path's own decision).
    screen_out.cues.covered = crate::cover::covered_controls(
        &screen_out,
        &crate::cover::Shape {
            sources: &sources,
            parents: &index.parents,
            ends: &index.ends,
            bars: &bars,
            nodes: &nodes,
        },
    );
    // Eighteenth review (E4): per raw node, whether it is content of its own (a label, a
    // shown value or a control; a web view is not), for the structural-wrapper test. Only
    // a screen with two web views or more needs it (one more walk, O(nodes)).
    let mut content = Vec::new();
    if webviews.len() >= 2 {
        content = vec![false; index.parents.len()];
        layout.walk(&screen.root, |node, _| {
            let Some(&i) = index.by_addr.get(&addr(node)) else {
                return;
            };
            if node.role == Role::WebView {
                return;
            }
            if let Some(c) = usize::try_from(i).ok().and_then(|k| content.get_mut(k)) {
                *c = is_own_content(node);
            }
        });
    }
    let webviews = documents(webviews, &index.ends, &content);
    CompactView {
        screen: screen_out,
        sources,
        parents: index.parents,
        ends: index.ends,
        bars,
        nodes,
        inputs,
        input_marks,
        webviews,
    }
}
/// Whether a raw node (not a web view) is content of its own for the structural-wrapper
/// test: a label, a shown value or a control role. A scroll view's indicator ("Vertical
/// scroll bar, 1 page": WebKit puts two beside the innermost of Safari's wrappers) is the
/// view's chrome, not content.
fn is_own_content(node: &UiNode) -> bool {
    let label = clean_opt(&node.label).to_lowercase();
    let head = label.split(',').next().unwrap_or_default().trim();
    if matches!(head, "vertical scroll bar" | "horizontal scroll bar") {
        return false;
    }
    !label.is_empty() || shown_value(node).is_some() || node.role.is_interactive()
}

/// The web views that start a document (seventeenth review, [`CompactView::webviews`]):
/// `all` sorted by index, minus each one that is a **structural wrapper** of the innermost
/// document web view holding it (eighteenth review, E4): its frame is that web view's frame
/// (within a point) **and** that web view holds no content of its own beside it — no node
/// with a label, a shown value or a control role (`content`, per raw index) in its subtree
/// outside this one's. A same-frame web view beside the page's own content (a full-viewport
/// cross-origin interstitial) is a document of its own. O(web views) after one O(nodes)
/// prefix count.
fn documents(mut all: Vec<(u32, Rect)>, ends: &[u32], content: &[bool]) -> Vec<u32> {
    all.sort_unstable_by_key(|(i, _)| *i);
    if all.len() < 2 {
        return all.into_iter().map(|(i, _)| i).collect();
    }
    let end = |w: u32| {
        usize::try_from(w)
            .ok()
            .and_then(|k| ends.get(k))
            .copied()
            .unwrap_or(0)
    };
    // prefix[k]: content nodes with raw index below k.
    let mut prefix = Vec::with_capacity(content.len() + 1);
    let mut n = 0usize;
    prefix.push(0usize);
    for &c in content {
        n += usize::from(c);
        prefix.push(n);
    }
    let count = |from: u32, to: u32| {
        let at = |k: u32| {
            usize::try_from(k)
                .ok()
                .and_then(|k| prefix.get(k.min(content.len())))
                .copied()
                .unwrap_or(n)
        };
        at(to).saturating_sub(at(from))
    };
    // The parent holds no content outside the child's subtree (the parent's own node
    // excluded), or the child holds none at all (an empty same-frame web view has nothing
    // to tell apart: Safari's innermost wrapper beside the scroll indicators).
    let wrapper = |parent: u32, child: u32| {
        let inside = count(child, end(child));
        inside == 0 || count(parent.saturating_add(1), end(parent)) == inside
    };
    let same = |a: &Rect, b: &Rect| {
        (a.x - b.x).abs() <= 1.0
            && (a.y - b.y).abs() <= 1.0
            && (a.width - b.width).abs() <= 1.0
            && (a.height - b.height).abs() <= 1.0
    };
    let mut open: Vec<(u32, Rect)> = Vec::new();
    let mut out = Vec::new();
    for (i, f) in all {
        while open.last().is_some_and(|&(o, _)| i >= end(o)) {
            open.pop();
        }
        if open
            .last()
            .is_some_and(|&(o, ref of)| same(of, &f) && wrapper(o, i))
        {
            continue;
        }
        open.push((i, f));
        out.push(i);
    }
    out
}

/// `(pages, percent)` of an iOS scroll indicator: label `Vertical scroll bar, N pages`
/// (`1 page`), value `P%`. `None` for anything else.
fn vertical_scroll_bar(node: &UiNode) -> Option<(u32, Option<u32>)> {
    let label = clean_opt(&node.label).to_lowercase();
    let rest = label.strip_prefix("vertical scroll bar,")?.trim();
    let count = rest
        .strip_suffix("pages")
        .or_else(|| rest.strip_suffix("page"))?
        .trim()
        .parse::<u32>()
        .ok()?;
    let percent = clean_opt(&node.value)
        .strip_suffix('%')
        .and_then(|p| p.trim().parse::<f64>().ok())
        .filter(|p| p.is_finite())
        // Float to int saturates; the value is a small percentage.
        .map(|p| p.round().clamp(0.0, 100.0) as u32);
    Some((count, percent))
}

/// `(current, count)` of a page indicator whose value reads `Page N of M` (M >= 2).
fn page_indicator(node: &UiNode) -> Option<(u32, u32)> {
    let value = clean_opt(&node.value).to_lowercase();
    let rest = value.strip_prefix("page ")?;
    let (cur, count) = rest.split_once(" of ")?;
    let cur = cur.trim().parse::<u32>().ok()?;
    let count = count.trim().parse::<u32>().ok()?;
    (count >= 2 && (1..=count).contains(&cur)).then_some((cur, count))
}

/// What the tree says about content that is not listed, from the subtree in front (the
/// alert while one is up).
///
/// Only explicit signals count, never guesses from geometry: UIKit and SwiftUI lists keep
/// just the rows near the viewport in the tree, so "nothing below the screen" proves
/// nothing. iOS reports every scrollable view's indicator as a node labelled
/// `Vertical scroll bar, N pages` with the position as value (`0%` at the top, `100%` at
/// the bottom). With two or more pages, a position below 100% means more below and one
/// above 0% means more above. A single page, a missing or unreadable position, or a bar
/// that is hidden or off screen says nothing. The page indicator is any node whose value
/// reads `Page N of M` (SpringBoard's page control and its search pill carry it). The
/// labels are English; other locales simply get no cue.
fn cues(root: &UiNode, bounds: Rect, layout: &Layout<'_>) -> ScreenCues {
    let mut out = ScreenCues::default();
    let mut stack: Vec<(&UiNode, usize)> = vec![(root, 0)];
    while let Some((n, depth)) = stack.pop() {
        if is_pruned(n) || depth > MAX_DEPTH || layout.is_excluded(n) {
            continue;
        }
        if layout.is_quiet(n) {
            stack.extend(n.children.iter().rev().map(|c| (c, depth + 1)));
            continue;
        }
        let on_screen = !n.frame.is_empty() && (bounds.is_empty() || n.frame.intersects(&bounds));
        if on_screen {
            if let Some((pages, Some(percent))) = vertical_scroll_bar(n) {
                if pages >= 2 {
                    out.more_below |= percent < 100;
                    out.more_above |= percent > 0;
                }
            }
            if out.page.is_none() && !is_secure(n) {
                out.page = page_indicator(n);
            }
        }
        stack.extend(n.children.iter().rev().map(|c| (c, depth + 1)));
    }
    out
}
