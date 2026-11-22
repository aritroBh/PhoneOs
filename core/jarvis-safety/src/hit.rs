//! Where a tap lands: every point Jarvis taps or presses must land on the control it names.
//!
//! Frames are not part of an approval's binding (content may scroll), and UIKit delivers a
//! touch to the frontmost view under the finger, which the accessibility tree does not say.
//! So a point is only used when no *other* control could receive it (sixth review):
//!
//! * The point lies in the target's shown frame, and in the shown frame of no other shown,
//!   enabled control that takes taps ([`takes_taps`]: buttons, links, cells, switches,
//!   sliders, pickers, tabs, text inputs, and `Other` roles named like a control: icons,
//!   menu items, checkboxes, steppers…) unless that control is an ancestor or a descendant
//!   of the target in the raw tree. Document order excuses nothing: an overlap is a
//!   conflict whichever of the two is drawn on top. Containers (`Other` overlays, scroll
//!   views, bars), static texts and images never count.
//! * **Bar precedence.** The one drawing order the rule relies on: iOS draws navigation and
//!   tab bars above scrolling content (rows scroll under the translucent bars). A *system
//!   bar* is a raw `NavigationBar` or `TabBar` node whose shown frame spans at least
//!   [`SYSTEM_BAR_MIN_WIDTH`] of the screen's width, on a typed tree (the session passes no
//!   bars for runner text, whose roles are forgeable; see [`ScreenTree::bars`]). `Toolbar`
//!   is not one (web content makes toolbars through ARIA), nor is a narrower bar. For a
//!   target inside a system bar, only other controls inside that same bar contest its
//!   points (everything else is under the bar). For any other target, every point inside
//!   any system bar is unusable (the bar would take the touch), on top of the rule above: a
//!   row partly under the tab bar is tapped in its part outside the bar, a row wholly under
//!   one is refused as covered by the bar. Nothing is assumed about alerts and sheets (no
//!   bars are listed while one is up).
//! * The default point (the centre, or the action's own preferred point) is tried first,
//!   then a fixed 5 × 5 grid of inset points of the target's frame ([`GRID`]: fractions
//!   1/6 … 5/6 in each axis, nearest the centre first), in a fixed order; the first usable
//!   one is used. A free strip of a control is found when it is deeper than a sixth of the
//!   control's side (7.3 pt of a 44 pt button). Deterministic: the same screen always gives
//!   the same point, so the look after an approval re-derives it exactly.
//! * When every candidate is unusable the action is not sent ([`Covered`]): the agent
//!   refuses it ("not done: "X" is covered by "Y"") and the model chooses something else.
//! * A raw point (`tap_point`) resolves to the smallest control containing it (only the
//!   controls of the system bar it is in, when it is in one); a second control containing
//!   it that is neither its ancestor nor its descendant makes it ambiguous, handled the
//!   same way.
//!
//! Ancestry comes from the raw tree ([`ScreenTree`]); without one (hand-built screens) no
//! two elements are related and there are no bars. Compaction never lists a control
//! together with a control inside it (a row holding controls lists the controls), so in
//! practice related pairs are rare; the rule keeps them allowed.

use jarvis_proto::{BarNode, CompactScreen, Element, Point, Role};

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
    /// Another control (compact index).
    Control(u32),
    /// A system bar drawn over the target (the target is not inside it).
    Bar(Bar),
}

/// A tap that would not land on its control alone: the target (compact index) and what
/// takes the first candidate point (or, for a raw point, the other control containing it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Covered {
    /// The control the action names (or the one a raw point resolves to).
    pub target: u32,
    /// Another control, or a system bar, that could receive the touch.
    pub by: CoveredBy,
}

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

/// Whether an element of `role` takes taps (and so can cover another control's point): the
/// classifier's pressable roles ([`is_pressable`]) that are controls rather than
/// containers, plus text inputs. Containers (bars, scroll and table views, windows,
/// alerts, the keyboard) and `Other` roles not named like a control (full-screen overlays
/// are common in real trees) never count, nor do static texts and images.
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
        (Some(sa), Some(sb)) => sa == sb || tree.inside(a, sb) || tree.inside(b, sa),
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

/// What would take a touch at `p` meant for `target` (see the module docs), if anything: a
/// system bar over a target outside it, else the first other control (screen order) that
/// takes taps, is enabled, contains `p`, is not related to `target`, and (for a target
/// inside a system bar) is inside that same bar.
fn blocker(
    p: Point,
    target: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Option<CoveredBy> {
    // Precedence holds only where the bar is drawn: a control the tree puts inside a bar but
    // that lies outside the bar's frame at `p` is ordinary content there.
    let home = bar_of(target, screen, tree).filter(|h| h.frame.contains(p));
    if home.is_none() {
        if let Some((_, kind)) = system_bars(screen, tree).find(|(b, _)| b.frame.contains(p)) {
            return Some(CoveredBy::Bar(kind));
        }
    }
    screen
        .elements
        .iter()
        .find(|e| {
            e.index != target.index
                && e.enabled
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
        match blocker(p, target, screen, tree) {
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
    match blocker(p, target, screen, tree) {
        None => Ok(()),
        Some(by) => Err(Covered {
            target: target.index,
            by,
        }),
    }
}

/// The control a raw screen point (in points) lands on: the smallest control that takes
/// taps and contains it (`Ok(None)` when there is none); inside a system bar, only that
/// bar's controls count (the bar is on top of the rest). Another control containing the
/// point that is neither its ancestor nor its descendant makes it ambiguous: `Err`.
pub fn control_at<'a>(
    p: Point,
    screen: &'a CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Result<Option<&'a Element>, Covered> {
    let over = system_bars(screen, tree)
        .map(|(b, _)| b)
        .find(|b| b.frame.contains(p));
    let Some(smallest) = screen
        .elements
        .iter()
        .filter(|e| takes_taps(&e.role) && e.enabled && e.frame.contains(p))
        .filter(|e| over.is_none_or(|b| tree.is_some_and(|t| t.inside(e, b.node))))
        .min_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
    else {
        return Ok(None);
    };
    match blocker(p, smallest, screen, tree) {
        None => Ok(Some(smallest)),
        Some(by) => Err(Covered {
            target: smallest.index,
            by,
        }),
    }
}
