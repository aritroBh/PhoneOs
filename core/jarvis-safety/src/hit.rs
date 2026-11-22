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
//! * The default point (the centre, or the action's own preferred point) is tried first,
//!   then a fixed 3 × 3 grid of inset points of the target's frame ([`GRID`]), in a fixed
//!   order; the first uncontested one is used. Deterministic: the same screen always gives
//!   the same point, so the look after an approval re-derives it exactly.
//! * When every candidate is contested the action is not sent ([`Covered`]): the agent
//!   refuses it ("not done: "X" is covered by "Y"") and the model chooses something else.
//! * A raw point (`tap_point`) resolves to the smallest control containing it; a second
//!   control containing it that is neither its ancestor nor its descendant makes it
//!   ambiguous, handled the same way.
//!
//! Ancestry comes from the raw tree ([`ScreenTree`]); without one (hand-built screens) no
//! two elements are related. Compaction never lists a control together with a control
//! inside it (a row holding controls lists the controls), so in practice related pairs are
//! rare; the rule keeps them allowed.

use jarvis_proto::{CompactScreen, Element, Point, Role};

use crate::classify::is_pressable;
use crate::evaluate::ScreenTree;

/// The fallback candidates, as fractions of the target's frame (x, y), after the default
/// point: the centre, then the middles of the four edges' inset lines, then the corners'.
pub const GRID: [(f64, f64); 9] = [
    (0.5, 0.5),
    (0.5, 0.25),
    (0.5, 0.75),
    (0.25, 0.5),
    (0.75, 0.5),
    (0.25, 0.25),
    (0.75, 0.25),
    (0.25, 0.75),
    (0.75, 0.75),
];

/// A tap that would not land on its control alone: the target (compact index) and the
/// first other control covering the point (or, for a raw point, the other control
/// containing it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Covered {
    /// The control the action names (or the one a raw point resolves to).
    pub target: u32,
    /// Another control that could receive the touch.
    pub by: u32,
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

/// The first other control (screen order) that takes taps, is enabled, contains `p` and is
/// not related to `target`.
pub fn covering<'a>(
    p: Point,
    target: &Element,
    screen: &'a CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Option<&'a Element> {
    screen.elements.iter().find(|e| {
        e.index != target.index
            && e.enabled
            && takes_taps(&e.role)
            && e.frame.contains(p)
            && !related(target, e, tree)
    })
}

/// The point to tap or press `target` at (see the module docs): `preferred` (the action's
/// own default, e.g. the trailing edge a `clear` taps; the centre when `None`) when it is
/// inside the target's frame and uncontested, else the first uncontested point of [`GRID`].
/// `Err(Covered)` when every candidate is contested.
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
    let mut blocker: Option<u32> = None;
    for p in std::iter::once(first).chain(grid) {
        if !f.contains(p) {
            continue;
        }
        match covering(p, target, screen, tree) {
            None => return Ok(p),
            Some(e) => {
                blocker.get_or_insert(e.index);
            }
        }
    }
    Err(Covered {
        target: target.index,
        by: blocker.unwrap_or(target.index),
    })
}

/// Whether the drag or press starting at `p` on `target` is uncontested (no point is
/// moved: a gesture's path is what the classifier read).
pub fn point_is_clear(
    p: Point,
    target: &Element,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Result<(), Covered> {
    match covering(p, target, screen, tree) {
        None => Ok(()),
        Some(e) => Err(Covered {
            target: target.index,
            by: e.index,
        }),
    }
}

/// The control a raw screen point (in points) lands on: the smallest control that takes
/// taps and contains it (`Ok(None)` when there is none). Another control containing the
/// point that is neither its ancestor nor its descendant makes it ambiguous: `Err`.
pub fn control_at<'a>(
    p: Point,
    screen: &'a CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Result<Option<&'a Element>, Covered> {
    let Some(smallest) = screen
        .elements
        .iter()
        .filter(|e| takes_taps(&e.role) && e.enabled && e.frame.contains(p))
        .min_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
    else {
        return Ok(None);
    };
    match covering(p, smallest, screen, tree) {
        None => Ok(Some(smallest)),
        Some(other) => Err(Covered {
            target: smallest.index,
            by: other.index,
        }),
    }
}
