//! Where a tap lands: every point Jarvis taps or presses must land on the control it names.
//!
//! **Why the rule is what it is.** The accessibility tree is not the touch hierarchy.
//! Occlusion read from the tree can only catch covers that are *in* the tree, and a hostile
//! page can catch taps with elements that have no accessibility presence at all, or simply
//! mislabel a button. So occlusion is never the defence against hostile content (that is
//! the app's trust tier and the approval question). Its job is honest screens: no tap for
//! "Cancel" is sent when a sheet, a banner, the keyboard or another control visibly sits on
//! that point. A node contests a point when it carries evidence of being something the owner
//! would see there; bare containers never do (real SwiftUI and UIKit trees are full of
//! full-screen pass-through `Other` nodes after the content: a rule that counted them
//! refused 92 of 108 controls on the ten iOS 26 fixtures).
//!
//! **Rule v2** ([`OCCLUDER_RULE`]). For a target and a point inside its shown frame, a
//! drawn raw node ([`ScreenTree::nodes`]) whose on-screen frame contains the point and that
//! is neither an ancestor nor a descendant of the target contests the point when:
//!
//! 1. it is a **control** ([`takes_taps`]), enabled or disabled, in either document order
//!    (an unlabelled `Cell` is a container, not a control);
//! 2. it is part of the **keyboard**: the `Keyboard` node or its subtree, or the input
//!    assistant's (prediction bar) subtree, each clipped to its subtree root's frame, in
//!    either order; not the keyboard window's containers;
//! 3. it is **labelled content drawn later**: not a control, with a non-empty label or
//!    value, after the target's subtree in document order, and not a container role
//!    (window, application, scroll, table, collection and web views, bars, alerts, sheets).
//!
//! Nothing else contests: unlabelled `Other` nodes, windows, scroll/table/collection/web
//! views, unlabelled cells and bars never cover a point by themselves.
//!
//! * **Bar precedence.** iOS draws navigation and tab bars above scrolling content. A
//!   *system bar* is a raw `NavigationBar` or `TabBar` node with no web-view, scroll-view,
//!   table or collection ancestor (WebKit reports an ARIA `tablist` as a tab bar; compaction
//!   does not list those, see `jarvis_observe::CompactView::bars`) whose shown frame spans at
//!   least [`SYSTEM_BAR_MIN_WIDTH`] of the screen's width, on a typed tree (the session passes
//!   no bars for runner text). For a target inside a system bar, at points inside the bar's
//!   frame, only nodes inside that same bar (and the keyboard) contest. For any other
//!   target, every point inside any system bar is unusable (the bar would take the touch).
//! * The default point (the centre, or the action's own preferred point) is tried first,
//!   then a fixed 5 × 5 grid of inset points of the target's frame ([`GRID`]: fractions
//!   1/6 … 5/6 in each axis, nearest the centre first); the first usable one is used.
//!   Deterministic: the same screen always gives the same point.
//! * When every candidate is unusable the action is not sent ([`Covered`]), naming what
//!   takes the first candidate: another listed element, a bar, the keyboard, or an unlisted
//!   node by its label.
//! * A raw point (`tap_point`, [`control_at`]) on the keyboard is refused in every app
//!   ([`PointError::Keyboard`]: Jarvis does not tap keys; `type` enters text). Otherwise it
//!   resolves to the smallest listed control containing it (inside a system bar, that bar's
//!   controls), contested by the same rule; a point on no listed control is the session's
//!   to gate.
//!
//! Without the raw tree's nodes (hand-built screens) the listed controls stand in, enabled
//! or not, either order. The rules the fixture sweep measured side by side are the other
//! [`OccluderRule`] variants ([`tap_spot_under`]); acting always uses [`OCCLUDER_RULE`].

use jarvis_proto::{BarNode, CompactScreen, Element, Point, RawNode, Role};

use crate::classify::is_pressable;
use crate::evaluate::ScreenTree;

/// The fallback candidates, as fractions of the target's frame (x, y), after the default
/// point: a 5 × 5 grid at sixths (1/6 … 5/6) in each axis, nearest the centre first: the
/// centre, the inner ring (edge middles, then corners), then the outer ring (edge middles,
/// the points beside them, then corners).
pub const GRID: [(f64, f64); 25] = {
    const A: f64 = 1.0 / 6.0;
    const B: f64 = 2.0 / 6.0;
    const C: f64 = 0.5;
    const D: f64 = 4.0 / 6.0;
    const E: f64 = 5.0 / 6.0;
    [
        (C, C),
        // Inner ring.
        (C, B),
        (C, D),
        (B, C),
        (D, C),
        (B, B),
        (D, B),
        (B, D),
        (D, D),
        // Outer ring.
        (C, A),
        (C, E),
        (A, C),
        (E, C),
        (B, A),
        (D, A),
        (B, E),
        (D, E),
        (A, B),
        (E, B),
        (A, D),
        (E, D),
        (A, A),
        (E, A),
        (A, E),
        (E, E),
    ]
};

/// Least share of the screen's width a `NavigationBar`/`TabBar` spans to be a system bar.
pub const SYSTEM_BAR_MIN_WIDTH: f64 = 0.9;

/// A system bar (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    /// A full-width `NavigationBar`.
    Navigation,
    /// A full-width `TabBar`.
    Tab,
}

impl Bar {
    /// How a refusal names it: "the navigation bar", "the tab bar".
    pub fn name(self) -> &'static str {
        match self {
            Bar::Navigation => "the navigation bar",
            Bar::Tab => "the tab bar",
        }
    }
}

/// What would take a touch meant for the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoveredBy {
    /// Another listed control (compact index).
    Control(u32),
    /// A system bar drawn over the target (the target is not inside it).
    Bar(Bar),
    /// The keyboard: a key, its prediction bar, or anything else in its window.
    Keyboard,
    /// Another raw node that is not a listed control (its index in the raw tree; its
    /// label, if any, is in [`ScreenTree::nodes`]).
    Node(u32),
}

/// A tap that would not land on its control alone: the target (compact index) and what
/// takes the first candidate point (or, for a raw point, the other node containing it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Covered {
    /// The control the action names (or the one a raw point resolves to).
    pub target: u32,
    /// Another control or node, a system bar, or the keyboard that could receive the touch.
    pub by: CoveredBy,
}

/// Why a raw point (`tap_point`) is not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointError {
    /// The point is on the keyboard (a key, or the prediction bar): Jarvis does not tap
    /// keys; text goes in with `type`.
    Keyboard,
    /// It resolves to a listed control, but another node could take the touch.
    Covered(Covered),
}

/// Which drawn raw nodes contest a point (see the module docs). Only [`OCCLUDER_RULE`] is
/// used to act; the others exist so the fixture sweep can measure them side by side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccluderRule {
    /// The sixth review's rule: other *listed* controls that are enabled, either order.
    V0,
    /// Controls (enabled or not, either order) and the keyboard (either order) only.
    Controls,
    /// [`OccluderRule::Controls`] plus labelled content drawn after the target.
    Labelled,
    /// [`OccluderRule::Labelled`], except a sibling of the target whose frame lies within
    /// the target's (the target's own visible label).
    LabelledNotOwnLabel,
    /// [`OccluderRule::LabelledNotOwnLabel`], and labelled content only from another
    /// top-level window, or a later subtree of the target's window.
    LabelledOtherSubtree,
}

/// The rule in force: v2 with labelled content (see the module docs and the sweep in
/// `jarvis-observe/tests/ios26.rs`).
pub const OCCLUDER_RULE: OccluderRule = OccluderRule::Labelled;

/// `Other` role names that are controls (lower-case, without separators).
const CONTROL_OTHER_NAMES: &[&str] = &[
    "icon",
    "menuitem",
    "menubutton",
    "menubaritem",
    "checkbox",
    "radiobutton",
    "toggle",
    "stepper",
    "segmentedcontrol",
    "popupbutton",
    "disclosuretriangle",
    "colorwell",
    "incrementarrow",
    "decrementarrow",
    "ratingindicator",
    "datepicker",
    "pickerwheel",
];

/// Whether an element of `role` takes taps: the classifier's pressable roles
/// ([`is_pressable`]) that are controls rather than containers, plus text inputs.
/// Containers (bars, scroll and table views, windows, alerts, the keyboard) and `Other`
/// roles not named like a control are not controls, nor are static texts and images; they
/// still cover a point as raw nodes (see the module docs).
pub fn takes_taps(role: &Role) -> bool {
    match role {
        Role::Button
        | Role::Link
        | Role::Cell
        | Role::Switch
        | Role::Slider
        | Role::Picker
        | Role::Tab
        | Role::TextField
        | Role::SecureTextField
        | Role::TextView
        | Role::SearchField => true,
        Role::Other(name) => {
            let n: String = name
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            is_pressable(role) && CONTROL_OTHER_NAMES.contains(&n.as_str())
        }
        _ => false,
    }
}

/// Whether `a` and `b` are the same raw node or one holds the other in the raw tree.
fn related(a: &Element, b: &Element, tree: Option<&ScreenTree<'_>>) -> bool {
    if a.index == b.index {
        return true;
    }
    let Some(tree) = tree else {
        return false;
    };
    match (tree.source(a), tree.source(b)) {
        (Some(sa), Some(sb)) => tree.related_nodes(sa, sb),
        _ => false,
    }
}

/// The system bar `b` is, if it is one on a screen `width` points wide.
fn system_bar(b: &BarNode, width: f64) -> Option<Bar> {
    let kind = match b.role {
        Role::NavigationBar => Bar::Navigation,
        Role::TabBar => Bar::Tab,
        _ => return None,
    };
    (width > 0.0 && b.frame.width >= SYSTEM_BAR_MIN_WIDTH * width).then_some(kind)
}

/// The system bars of `screen` (none without a tree), with their kinds.
fn system_bars<'t>(
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'t>>,
) -> impl Iterator<Item = (&'t BarNode, Bar)> {
    let width = screen.size.width;
    tree.map_or(&[][..], |t| t.bars)
        .iter()
        .filter_map(move |b| system_bar(b, width).map(|k| (b, k)))
}

/// The system bar `el` is inside, if any.
fn bar_of<'t>(
    el: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'t>>,
) -> Option<&'t BarNode> {
    let t = tree?;
    system_bars(screen, tree)
        .map(|(b, _)| b)
        .find(|b| t.inside(el, b.node))
}

/// Roles that never contest a point by themselves (containers): windows, the app, scroll,
/// table, collection and web views, bars, alerts and sheets, the keyboard node itself
/// (its keys do, as the keyboard).
fn is_container(role: &Role) -> bool {
    match role {
        Role::Application
        | Role::Window
        | Role::ScrollView
        | Role::Table
        | Role::CollectionView
        | Role::WebView
        | Role::NavigationBar
        | Role::TabBar
        | Role::Toolbar
        | Role::Alert
        | Role::Sheet => true,
        Role::Other(name) => {
            let n: String = name
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect();
            matches!(
                n.as_str(),
                "webview" | "scrollview" | "table" | "tableview" | "collectionview" | "window"
            )
        }
        _ => false,
    }
}

/// The top-level window (a child of the root) and that window's child holding raw node `n`.
fn top_path(n: u32, tree: &ScreenTree<'_>) -> (Option<u32>, Option<u32>) {
    let mut chain = vec![n];
    let mut at = n;
    while let Some(p) = usize::try_from(at)
        .ok()
        .and_then(|i| tree.parents.get(i))
        .copied()
        .filter(|p| *p != u32::MAX)
    {
        if chain.len() > tree.parents.len() {
            break;
        }
        chain.push(p);
        at = p;
    }
    // chain: n … root; the window is the one below the root.
    let len = chain.len();
    let window = len.checked_sub(2).and_then(|i| chain.get(i)).copied();
    let child = len.checked_sub(3).and_then(|i| chain.get(i)).copied();
    (window, child)
}

/// Whether raw node `n` contests a point for `target` (compact element, raw node `own`)
/// under `rule` (see the module docs).
fn contests(
    n: &RawNode,
    target: &Element,
    own: Option<u32>,
    tree: &ScreenTree<'_>,
    rule: OccluderRule,
) -> bool {
    if own.is_some_and(|t| tree.related_nodes(t, n.node)) {
        return false;
    }
    // 1. A control, enabled or not, either order (an unlabelled cell is a container).
    let control = takes_taps(&n.role) && !(n.role == Role::Cell && !n.text);
    // 2. The keyboard: its subtree and the prediction bar's, either order.
    if control || n.keyboard {
        return true;
    }
    if matches!(rule, OccluderRule::V0 | OccluderRule::Controls) {
        return false;
    }
    // 3. Labelled content drawn after the target's subtree.
    let after = own.is_none_or(|t| {
        let end = usize::try_from(t)
            .ok()
            .and_then(|i| tree.ends.get(i))
            .copied()
            .unwrap_or(0);
        n.node >= end
    });
    if !n.text || is_container(&n.role) || n.role == Role::Keyboard || !after {
        return false;
    }
    let own_label = own.is_some_and(|t| {
        let parent = |i: u32| {
            usize::try_from(i)
                .ok()
                .and_then(|i| tree.parents.get(i))
                .copied()
        };
        let f = target.frame;
        let inside = n.frame.x >= f.x - 1.0
            && n.frame.y >= f.y - 1.0
            && n.frame.max_x() <= f.max_x() + 1.0
            && n.frame.max_y() <= f.max_y() + 1.0;
        parent(t) == parent(n.node) && inside
    });
    match rule {
        OccluderRule::LabelledNotOwnLabel => !own_label,
        OccluderRule::LabelledOtherSubtree => {
            !own_label && own.is_none_or(|t| top_path(t, tree) != top_path(n.node, tree))
        }
        _ => true,
    }
}

/// What a contesting raw node is, as a cover: the keyboard, a listed control, or a node.
fn cover_of(n: &RawNode, tree: &ScreenTree<'_>) -> CoveredBy {
    if n.keyboard {
        return CoveredBy::Keyboard;
    }
    match tree.element_of(n.node) {
        Some(i) => CoveredBy::Control(i),
        None => CoveredBy::Node(n.node),
    }
}

/// What would take a touch at `p` meant for `target` (see the module docs), if anything: a
/// system bar over a target outside it; else, with the raw tree's drawn nodes, the first
/// contesting node containing `p` (keyboard nodes first, then the smallest); without them
/// (a hand-built screen), the first other listed control containing `p` that is not
/// related to `target`. For a target inside a system bar at `p`, only nodes inside that
/// same bar (and the keyboard) contest.
fn blocker(
    p: Point,
    target: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    rule: OccluderRule,
) -> Option<CoveredBy> {
    // Precedence holds only where the bar is drawn: a control the tree puts inside a bar but
    // that lies outside the bar's frame at `p` is ordinary content there.
    let home = bar_of(target, screen, tree).filter(|h| h.frame.contains(p));
    if home.is_none() {
        if let Some((_, kind)) = system_bars(screen, tree).find(|(b, _)| b.frame.contains(p)) {
            return Some(CoveredBy::Bar(kind));
        }
    }
    if let Some(t) = tree.filter(|t| !t.nodes.is_empty() && rule != OccluderRule::V0) {
        let own = t.source(target);
        return t
            .nodes
            .iter()
            .filter(|n| n.frame.contains(p) && contests(n, target, own, t, rule))
            .filter(|n| n.keyboard || home.is_none_or(|h| t.node_inside(n.node, h.node)))
            .min_by(|a, b| {
                b.keyboard
                    .cmp(&a.keyboard)
                    .then(a.frame.area().total_cmp(&b.frame.area()))
                    .then(a.node.cmp(&b.node))
            })
            .map(|n| cover_of(n, t));
    }
    screen
        .elements
        .iter()
        .find(|e| {
            e.index != target.index
                && (e.enabled || rule != OccluderRule::V0)
                && takes_taps(&e.role)
                && e.frame.contains(p)
                && !related(target, e, tree)
                && home.is_none_or(|h| tree.is_some_and(|t| t.inside(e, h.node)))
        })
        .map(|e| CoveredBy::Control(e.index))
}

/// The point to tap or press `target` at (see the module docs): `preferred` (the action's
/// own default, e.g. the trailing edge a `clear` taps; the centre when `None`) when it is
/// inside the target's frame and usable, else the first usable point of [`GRID`].
/// `Err(Covered)` when every candidate is taken, naming what takes the first one.
pub fn tap_spot(
    target: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    preferred: Option<Point>,
) -> Result<Point, Covered> {
    tap_spot_under(OCCLUDER_RULE, target, screen, tree, preferred)
}

/// [`tap_spot`] under another [`OccluderRule`], for measuring rules side by side (the
/// fixture sweep). Acting always uses [`OCCLUDER_RULE`].
#[doc(hidden)]
pub fn tap_spot_under(
    rule: OccluderRule,
    target: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    preferred: Option<Point>,
) -> Result<Point, Covered> {
    let f = target.frame;
    let first = preferred
        .filter(|p| f.contains(*p))
        .unwrap_or_else(|| f.center());
    let grid = GRID
        .iter()
        .map(|(fx, fy)| Point::new(f.x + f.width * fx, f.y + f.height * fy));
    let mut first_blocker: Option<CoveredBy> = None;
    for p in std::iter::once(first).chain(grid) {
        if !f.contains(p) {
            continue;
        }
        match blocker(p, target, screen, tree, rule) {
            None => return Ok(p),
            Some(b) => {
                first_blocker.get_or_insert(b);
            }
        }
    }
    Err(Covered {
        target: target.index,
        by: first_blocker.unwrap_or(CoveredBy::Control(target.index)),
    })
}

/// Whether the drag or press starting at `p` on `target` is uncontested (no point is
/// moved: a gesture's path is what the classifier read). Bar precedence applies as for
/// [`tap_spot`].
pub fn point_is_clear(
    p: Point,
    target: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Result<(), Covered> {
    match blocker(p, target, screen, tree, OCCLUDER_RULE) {
        None => Ok(()),
        Some(by) => Err(Covered {
            target: target.index,
            by,
        }),
    }
}

/// The listed control a raw screen point (in points) lands on, if any.
///
/// A point on the keyboard (a drawn node of the `Keyboard` subtree or of the prediction
/// bar's) is [`PointError::Keyboard`], in every app. Otherwise the smallest listed control
/// that takes taps and contains `p` (inside a system bar, only that bar's controls: the bar
/// is on top of the rest) is the target, contested by [`tap_spot`]'s rule
/// ([`PointError::Covered`]); `Ok(None)` when no listed control contains it (the session
/// gates such a point as it always did).
pub fn control_at<'a>(
    p: Point,
    screen: &'a CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Result<Option<&'a Element>, PointError> {
    if tree.is_some_and(|t| t.nodes.iter().any(|n| n.keyboard && n.frame.contains(p))) {
        return Err(PointError::Keyboard);
    }
    let over = system_bars(screen, tree)
        .map(|(b, _)| b)
        .find(|b| b.frame.contains(p));
    let Some(smallest) = screen
        .elements
        .iter()
        .filter(|e| takes_taps(&e.role) && e.frame.contains(p))
        .filter(|e| over.is_none_or(|b| tree.is_some_and(|t| t.inside(e, b.node))))
        .min_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
    else {
        return Ok(None);
    };
    match blocker(p, smallest, screen, tree, OCCLUDER_RULE) {
        None => Ok(Some(smallest)),
        Some(by) => Err(PointError::Covered(Covered {
            target: smallest.index,
            by,
        })),
    }
}
