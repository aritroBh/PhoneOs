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
//! **Rule v3** ([`OCCLUDER_RULE`], eighth review). For a target and a point inside its
//! shown frame, a drawn raw node ([`ScreenTree::nodes`]) whose on-screen frame contains the
//! point and that is not the target nor an ancestor of it contests the point when:
//!
//! 1. it is a **control** ([`takes_taps`]), enabled or disabled, in either document order
//!    (an unlabelled `Cell` is a container, not a control). A control **nested inside the
//!    target** contests too (a card `Button "Wool socks"` holding `Button "Buy now $25.00"`
//!    at its centre: a tap there lands on the nested button, which is never listed), except
//!    the target's own refined inner control: a descendant of the same role whose frame
//!    holds the target's listed frame (1 pt tolerance) and whose label is compatible with
//!    the target's (absent, or equal ignoring case), which is how compaction refines a
//!    wrapper to the control inside it (`jarvis_observe`'s `Target::of`; since the ninth
//!    review only when the wrapper shows no labelled content outside the inner control or
//!    the inner control covers at least 80 % of it, and the refined element's label then
//!    carries the inner control's labels and icon names, so the exempt control is
//!    classified and named in the question). Only controls of the target's own subtree
//!    contest; its labelled content and its other nodes never do;
//! 2. it is part of the **keyboard**: the `Keyboard` node or its subtree, or the input
//!    assistant's (prediction bar) subtree, each clipped to its subtree root's frame, in
//!    either order; not the keyboard window's containers; a node of those subtrees reaching
//!    outside the root's frame is ordinary content (ninth review) and contests by (1) or (3);
//! 3. it is **labelled content drawn later**: not a control, with a non-empty label or
//!    value, after the target's subtree in document order, not inside the target, and not
//!    a container role (window, application, scroll, table, collection and web views, bars,
//!    alerts, sheets).
//!
//! Nothing else contests: unlabelled `Other` nodes, windows, scroll/table/collection/web
//! views, unlabelled cells and bars never cover a point by themselves.
//!
//! * **Bar precedence** (ninth review). iOS draws navigation, tab and bottom tool bars
//!   above the page content that scrolls under them, and that is the one thing a bar's
//!   precedence is for. A *system bar* is a raw `NavigationBar`, `TabBar` or `Toolbar` node
//!   with no web-view, scroll-view, table or collection ancestor (WebKit reports an ARIA
//!   `tablist` as a tab bar; compaction does not list those, see
//!   `jarvis_observe::CompactView::bars`), on a typed tree (the session passes no bars for
//!   runner text), whose shown frame is full width ([`SYSTEM_BAR_MIN_WIDTH`]), anchored to
//!   the top ([`SYSTEM_BAR_TOP_REGION`]) or bottom ([`SYSTEM_BAR_BOTTOM_REGION`]) edge
//!   region and of bounded height ([`SYSTEM_BAR_MAX_TOP_HEIGHT`],
//!   [`SYSTEM_BAR_MAX_BOTTOM_HEIGHT`], never below [`SYSTEM_BAR_MIN_CAP`]; [`system_bar`]).
//!   For a target T inside system bar B, at a point inside B's frame, a node N outside B
//!   that would contest the point is ignored **only if** it is page content scrolled under
//!   B: N has a scrollable ancestor (scroll, table, collection or web view) that is
//!   page-sized on screen (at least [`PAGE_MIN_WIDTH`] of the screen's width and
//!   [`PAGE_MIN_BAR_HEIGHTS`] times B's height tall); N has no modal-container ancestor
//!   (alert, sheet, action sheet, popover, menu, dialog) and is not in a later top-level
//!   window than B; and for a bottom bar that scrollable ancestor comes before B in the
//!   tree (a navigation bar at the top accepts either order: SwiftUI lists the bar first).
//!   Everything else — the bar's own nodes, the keyboard, a menu or banner drawn over the
//!   bar — contests as for any target (the eighth review let everything outside the bar
//!   give way, so a menu's "Delete" over the toolbar's "Compose" took the tap). For any
//!   target outside a system bar, every point inside the bar is unusable (the bar would
//!   take the touch).
//! * The default point (the centre, or the action's own preferred point) is tried first,
//!   then a fixed 5 × 5 grid of inset points of the target's frame ([`GRID`]: fractions
//!   1/6 … 5/6 in each axis, nearest the centre first); the first usable one is used.
//!   Deterministic: the same screen always gives the same point.
//! * When every candidate is unusable the action is not sent ([`Covered`]), naming what
//!   takes the first candidate: another listed element, a bar, the keyboard, a control
//!   nested inside the target, or an unlisted node by its label.
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

/// Least share of the screen's width a `NavigationBar`/`TabBar`/`Toolbar` spans to be a
/// system bar.
pub const SYSTEM_BAR_MIN_WIDTH: f64 = 0.9;
/// A top bar's top edge lies within this share of the screen's height from the top (iOS 26
/// fixtures: navigation bars at y 62 of 956, 6.5 %).
pub const SYSTEM_BAR_TOP_REGION: f64 = 0.15;
/// A bottom bar's bottom edge lies within this share of the screen's height from the
/// bottom (fixtures: the tab bar and the floating toolbar end at 956 of 956).
pub const SYSTEM_BAR_BOTTOM_REGION: f64 = 0.1;
/// Tallest top bar, as a share of the screen's height, at least [`SYSTEM_BAR_MIN_CAP`]
/// points (fixtures: large-title navigation bars 106 pt of 956, 11 %; standard 54 pt).
pub const SYSTEM_BAR_MAX_TOP_HEIGHT: f64 = 0.2;
/// Tallest bottom bar, as a share of the screen's height, at least [`SYSTEM_BAR_MIN_CAP`]
/// points (fixtures: tab bar 83 pt, floating toolbar 86 pt of 956, 9 %).
pub const SYSTEM_BAR_MAX_BOTTOM_HEIGHT: f64 = 0.15;
/// The height cap is never below this many points (a landscape phone is 440 pt high).
pub const SYSTEM_BAR_MIN_CAP: f64 = 100.0;
/// Page content under a bar sits in a scrollable container at least this share of the
/// screen's width wide...
pub const PAGE_MIN_WIDTH: f64 = 0.9;
/// ...and at least this many times the bar's height tall.
pub const PAGE_MIN_BAR_HEIGHTS: f64 = 2.0;

/// Which screen edge a system bar is anchored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// The top (navigation bars): page content may come before or after it in the tree
    /// (SwiftUI lists the bar before its content).
    Top,
    /// The bottom (tab bars, bottom toolbars): page content comes before it.
    Bottom,
}

/// A system bar (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    /// A full-width `NavigationBar`.
    Navigation,
    /// A full-width `TabBar`.
    Tab,
    /// A full-width `Toolbar` (a bottom tool bar; eighth review).
    Tool,
}

impl Bar {
    /// How a refusal names it: "the navigation bar", "the tab bar".
    pub fn name(self) -> &'static str {
        match self {
            Bar::Navigation => "the navigation bar",
            Bar::Tab => "the tab bar",
            Bar::Tool => "the toolbar",
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
    /// A control nested inside the target, which is never listed (its index in the raw
    /// tree; its label, if any, is in [`ScreenTree::nodes`]).
    Inner(u32),
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
    /// Rule v3 (eighth review): [`OccluderRule::Labelled`], and controls nested inside the
    /// target contest too, except its own refined inner control.
    LabelledNested,
}

/// The rule in force: v3, v2 with labelled content plus nested controls (see the module
/// docs and the sweep in `jarvis-observe/tests/ios26.rs`).
pub const OCCLUDER_RULE: OccluderRule = OccluderRule::LabelledNested;

/// How far (points) a refined inner control's frame may differ from the target's listed
/// frame (compaction's nesting tolerance).
const NEST_TOLERANCE: f64 = 1.0;

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

/// The system bar `b` is, and the edge it is anchored to, if it is one on a screen of
/// `size` (ninth review): full width ([`SYSTEM_BAR_MIN_WIDTH`]), anchored to the top or the
/// bottom edge region ([`SYSTEM_BAR_TOP_REGION`], [`SYSTEM_BAR_BOTTOM_REGION`]) and of
/// bounded height ([`SYSTEM_BAR_MAX_TOP_HEIGHT`], [`SYSTEM_BAR_MAX_BOTTOM_HEIGHT`], never
/// below [`SYSTEM_BAR_MIN_CAP`]). A full-screen `Toolbar` is no bar. (Bars inside page
/// content are never in [`ScreenTree::bars`].)
pub fn system_bar(b: &BarNode, size: jarvis_proto::Size) -> Option<(Bar, Edge)> {
    let kind = match b.role {
        Role::NavigationBar => Bar::Navigation,
        Role::TabBar => Bar::Tab,
        Role::Toolbar => Bar::Tool,
        _ => return None,
    };
    let (w, h) = (size.width, size.height);
    if !(w > 0.0 && h > 0.0 && b.frame.width >= SYSTEM_BAR_MIN_WIDTH * w) {
        return None;
    }
    let f = b.frame;
    let top = f.y <= SYSTEM_BAR_TOP_REGION * h
        && f.height <= (SYSTEM_BAR_MAX_TOP_HEIGHT * h).max(SYSTEM_BAR_MIN_CAP);
    let bottom = f.max_y() >= (1.0 - SYSTEM_BAR_BOTTOM_REGION) * h
        && f.height <= (SYSTEM_BAR_MAX_BOTTOM_HEIGHT * h).max(SYSTEM_BAR_MIN_CAP);
    match (top, bottom) {
        (true, false) => Some((kind, Edge::Top)),
        (false, true) => Some((kind, Edge::Bottom)),
        (true, true) if f.center().y < h / 2.0 => Some((kind, Edge::Top)),
        (true, true) => Some((kind, Edge::Bottom)),
        (false, false) => None,
    }
}

/// The system bars of `screen` (none without a tree), with their kinds and edges.
fn system_bars<'t>(
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'t>>,
) -> impl Iterator<Item = (&'t BarNode, Bar, Edge)> {
    let size = screen.size;
    tree.map_or(&[][..], |t| t.bars)
        .iter()
        .filter_map(move |b| system_bar(b, size).map(|(k, e)| (b, k, e)))
}

/// The system bar `el` is inside, if any, with its edge.
fn bar_of<'t>(
    el: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'t>>,
) -> Option<(&'t BarNode, Edge)> {
    let t = tree?;
    system_bars(screen, tree)
        .map(|(b, _, e)| (b, e))
        .find(|(b, _)| t.inside(el, b.node))
}

/// Whether `el` sits in a system bar that also holds a listed control labelled "Back" (a
/// browser's navigation bar; ninth review: its "Forward" is navigation, not a send).
pub(crate) fn in_bar_with_back(
    el: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> bool {
    let (Some(t), Some((b, _))) = (tree, bar_of(el, screen, tree)) else {
        return false;
    };
    screen.elements.iter().any(|e| {
        e.index != el.index
            && takes_taps(&e.role)
            && t.inside(e, b.node)
            && crate::text::tokens(&e.label).iter().any(|w| w == "back")
    })
}

/// The drawn raw node with index `i`, if it is drawn (nodes are in node order).
pub(crate) fn raw_node<'t>(tree: &ScreenTree<'t>, i: u32) -> Option<&'t RawNode> {
    tree.nodes
        .binary_search_by(|n| n.node.cmp(&i))
        .ok()
        .and_then(|k| tree.nodes.get(k))
}

/// Normalised `Other` role name (letters and digits, lower case).
fn other_name(role: &Role) -> Option<String> {
    match role {
        Role::Other(name) => Some(
            name.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect(),
        ),
        _ => None,
    }
}

/// Scroll views, tables, collection views and web views: what page content scrolls in.
fn is_scroller(role: &Role) -> bool {
    match role {
        Role::ScrollView | Role::Table | Role::CollectionView | Role::WebView => true,
        _ => other_name(role).is_some_and(|n| {
            matches!(
                n.as_str(),
                "scrollview" | "table" | "tableview" | "collectionview" | "webview"
            )
        }),
    }
}

/// Modal containers: alerts, sheets, action sheets, popovers, menus, dialogs (whatever the
/// tree types have for them; `MenuItem`/`MenuButton` are controls, not containers).
fn is_modal(role: &Role) -> bool {
    match role {
        Role::Alert | Role::Sheet => true,
        _ => other_name(role).is_some_and(|n| {
            ["alert", "sheet", "popover", "menu", "dialog", "modal"]
                .iter()
                .any(|k| n.contains(k))
                && !["menuitem", "menubutton", "menubaritem"]
                    .iter()
                    .any(|k| n.contains(k))
        }),
    }
}

/// Whether raw node `n`, which contests a point inside system bar `bar` (anchored to
/// `edge`), is page content scrolled under the bar, the one thing a bar's precedence is
/// for (ninth review): `n` has a scrollable ancestor that is page-sized on screen (at least
/// [`PAGE_MIN_WIDTH`] of the screen's width, and [`PAGE_MIN_BAR_HEIGHTS`] times the bar's
/// height tall); `n` has no modal-container ancestor and is not in a later top-level window
/// than the bar; and for a bottom bar that scrollable container comes before the bar in
/// document order (for a top bar either order). Everything else drawn over the bar — a
/// menu, a sheet, a banner, a popover — contests like over any target.
fn scrolled_under(
    n: u32,
    bar: &BarNode,
    edge: Edge,
    screen: &CompactScreen,
    tree: &ScreenTree<'_>,
) -> bool {
    if top_path(n, tree).0 > top_path(bar.node, tree).0 {
        return false;
    }
    let mut page = false;
    let mut at = n;
    let mut steps = 0usize;
    while let Some(p) = usize::try_from(at)
        .ok()
        .and_then(|i| tree.parents.get(i))
        .copied()
        .filter(|p| *p != u32::MAX)
    {
        steps += 1;
        if steps > tree.parents.len() {
            return false;
        }
        at = p;
        let Some(r) = raw_node(tree, p) else {
            continue;
        };
        if is_modal(&r.role) {
            return false;
        }
        if is_scroller(&r.role)
            && r.frame.width >= PAGE_MIN_WIDTH * screen.size.width
            && r.frame.height >= PAGE_MIN_BAR_HEIGHTS * bar.frame.height
            && (edge == Edge::Top || (p < bar.node && !tree.node_inside(bar.node, p)))
        {
            page = true;
        }
    }
    page
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
    // 1. A control, enabled or not, either order (an unlabelled cell is a container).
    let control = takes_taps(&n.role) && !(n.role == Role::Cell && !n.text);
    if let Some(t) = own {
        if t == n.node || tree.node_inside(t, n.node) {
            // The target itself, or an ancestor of it.
            return false;
        }
        if tree.node_inside(n.node, t) {
            // Inside the target: only a nested control contests (rule v3), never the
            // target's own refined inner control.
            return rule == OccluderRule::LabelledNested && control && !refined_inner(n, target);
        }
    }
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

/// Whether raw node `n` (inside the target) is the target's own refined inner control: the
/// same role, a frame holding the target's listed frame (compaction gives a wrapper the
/// frame of the same control nested in it), and a label compatible with the target's.
fn refined_inner(n: &RawNode, target: &Element) -> bool {
    let (f, t) = (n.frame, target.frame);
    let holds = t.x >= f.x - NEST_TOLERANCE
        && t.y >= f.y - NEST_TOLERANCE
        && t.max_x() <= f.max_x() + NEST_TOLERANCE
        && t.max_y() <= f.max_y() + NEST_TOLERANCE;
    let compatible = n.label.as_deref().is_none_or(|l| {
        let (a, b) = (l.trim(), target.label.trim());
        a.is_empty() || b.is_empty() || a.to_lowercase() == b.to_lowercase()
    });
    n.role == target.role && holds && compatible
}

/// What a contesting raw node is, as a cover: the keyboard, a listed control, a control
/// nested inside the target, or a node.
fn cover_of(n: &RawNode, target: &Element, tree: &ScreenTree<'_>) -> CoveredBy {
    if n.keyboard {
        return CoveredBy::Keyboard;
    }
    if tree
        .source(target)
        .is_some_and(|t| tree.node_inside(n.node, t))
    {
        return CoveredBy::Inner(n.node);
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
    blocker_among(p, target, screen, tree, rule, tree.map(|t| t.nodes.iter()))
        .map(|b| b.cover(target, tree))
}

/// What [`blocker`] found, before a contesting raw node is named as a cover (which looks
/// the node up among the listed elements: only the first candidate's cover is ever named).
enum Blocker<'n> {
    /// Named already: a system bar or a listed control.
    Cover(CoveredBy),
    /// The contesting raw node.
    Node(&'n RawNode),
}

impl Blocker<'_> {
    fn cover(self, target: &Element, tree: Option<&ScreenTree<'_>>) -> CoveredBy {
        match (self, tree) {
            (Blocker::Cover(c), _) => c,
            (Blocker::Node(n), Some(t)) => cover_of(n, target, t),
            (Blocker::Node(n), None) => CoveredBy::Node(n.node),
        }
    }
}

/// [`blocker`], looking only at `drawn` among the tree's drawn nodes: every drawn node that
/// could contain `p`, in node order (all of them, or the ones [`near`] a set of points).
/// The rule and its order are [`blocker`]'s: with drawn nodes in the tree, a contesting
/// node is looked for among them (and none found means `p` is clear); without, the listed
/// controls stand in.
fn blocker_among<'n>(
    p: Point,
    target: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    rule: OccluderRule,
    drawn: Option<impl Iterator<Item = &'n RawNode>>,
) -> Option<Blocker<'n>> {
    // Precedence holds only where the bar is drawn: a control the tree puts inside a bar but
    // that lies outside the bar's frame at `p` is ordinary content there.
    let home = bar_of(target, screen, tree).filter(|(h, _)| h.frame.contains(p));
    // A system bar the target is not in takes the point, also where it overlaps the
    // target's own bar (tenth review: two overlapping bars let the tap through).
    if let Some((_, kind, _)) = system_bars(screen, tree)
        .find(|(b, _, _)| b.frame.contains(p) && !tree.is_some_and(|t| t.inside(target, b.node)))
    {
        return Some(Blocker::Cover(CoveredBy::Bar(kind)));
    }
    // Inside its bar, a target gives way to everything but page content scrolled under the
    // bar (ninth review): the bar's own nodes, the keyboard, overlays drawn over the bar.
    let yields = |n: u32, t: &ScreenTree<'_>| {
        home.is_none_or(|(h, edge)| {
            t.node_inside(n, h.node) || !scrolled_under(n, h, edge, screen, t)
        })
    };
    if let (Some(t), Some(drawn)) = (
        tree.filter(|t| !t.nodes.is_empty() && rule != OccluderRule::V0),
        drawn,
    ) {
        let own = t.source(target);
        return drawn
            .filter(|n| n.frame.contains(p) && contests(n, target, own, t, rule))
            .filter(|n| n.keyboard || yields(n.node, t))
            .min_by(|a, b| {
                b.keyboard
                    .cmp(&a.keyboard)
                    .then(a.frame.area().total_cmp(&b.frame.area()))
                    .then(a.node.cmp(&b.node))
            })
            .map(Blocker::Node);
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
                && tree.is_none_or(|t| t.source(e).is_none_or(|n| yields(n, t)))
        })
        .map(|e| Blocker::Cover(CoveredBy::Control(e.index)))
}

/// The drawn nodes of `tree` that could contain any of `points`, in node order: those whose
/// frame reaches the points' bounding box (a node that contains a point reaches every box
/// holding the point, so none is left out).
fn near<'t>(tree: &ScreenTree<'t>, points: &[Point]) -> Vec<&'t RawNode> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let (mut x0, mut x1, mut y0, mut y1) = (first.x, first.x, first.y, first.y);
    for p in points {
        x0 = x0.min(p.x);
        x1 = x1.max(p.x);
        y0 = y0.min(p.y);
        y1 = y1.max(p.y);
    }
    tree.nodes
        .iter()
        .filter(|n| {
            let f = n.frame;
            !(f.x > x1 || f.max_x() <= x0 || f.y > y1 || f.max_y() <= y0)
        })
        .collect()
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
    let mut candidates = std::iter::once(first)
        .chain(grid)
        .filter(|p| f.contains(*p));
    let Some(p0) = candidates.next() else {
        return Err(Covered {
            target: target.index,
            by: CoveredBy::Control(target.index),
        });
    };
    // The first candidate against every drawn node; it is the one a refusal names.
    let all = tree.map(|t| t.nodes.iter());
    let Some(first_blocker) = blocker_among(p0, target, screen, tree, rule, all) else {
        return Ok(p0);
    };
    // The others (up to 25, inside the target's frame) only against the drawn nodes that
    // reach them: the same nodes in the same order as a scan of all would find.
    let rest: Vec<Point> = candidates.collect();
    let reach = tree.map(|t| near(t, &rest)).unwrap_or_default();
    for p in rest {
        let drawn = tree.map(|_| reach.iter().copied());
        if blocker_among(p, target, screen, tree, rule, drawn).is_none() {
            return Ok(p);
        }
    }
    Err(Covered {
        target: target.index,
        by: first_blocker.cover(target, tree),
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
        .map(|(b, _, _)| b)
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

/// The tap-point search as it was before the candidates after the first were checked only
/// against the drawn nodes near them and only the first cover was named: the reference the
/// differential test (`jarvis-observe/tests/perf_equivalence.rs`) holds [`tap_spot_under`]
/// to. Built for tests only (feature `reference`).
#[cfg(feature = "reference")]
pub mod reference {
    use super::*;

    fn blocker(
        p: Point,
        target: &Element,
        screen: &CompactScreen,
        tree: Option<&ScreenTree<'_>>,
        rule: OccluderRule,
    ) -> Option<CoveredBy> {
        let home = bar_of(target, screen, tree).filter(|(h, _)| h.frame.contains(p));
        if let Some((_, kind, _)) = system_bars(screen, tree).find(|(b, _, _)| {
            b.frame.contains(p) && !tree.is_some_and(|t| t.inside(target, b.node))
        }) {
            return Some(CoveredBy::Bar(kind));
        }
        let yields = |n: u32, t: &ScreenTree<'_>| {
            home.is_none_or(|(h, edge)| {
                t.node_inside(n, h.node) || !scrolled_under(n, h, edge, screen, t)
            })
        };
        if let Some(t) = tree.filter(|t| !t.nodes.is_empty() && rule != OccluderRule::V0) {
            let own = t.source(target);
            return t
                .nodes
                .iter()
                .filter(|n| n.frame.contains(p) && contests(n, target, own, t, rule))
                .filter(|n| n.keyboard || yields(n.node, t))
                .min_by(|a, b| {
                    b.keyboard
                        .cmp(&a.keyboard)
                        .then(a.frame.area().total_cmp(&b.frame.area()))
                        .then(a.node.cmp(&b.node))
                })
                .map(|n| cover_of(n, target, t));
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
                    && tree.is_none_or(|t| t.source(e).is_none_or(|n| yields(n, t)))
            })
            .map(|e| CoveredBy::Control(e.index))
    }

    /// [`super::tap_spot_under`] as it was.
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
}
