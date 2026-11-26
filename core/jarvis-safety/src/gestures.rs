//! Gesture geometry: where scrolls and swipes start and end.
//!
//! iOS reserves the screen edges for system gestures: pulling down from the top opens
//! Notification/Control Center, swiping up from the bottom goes home or switches apps, and a
//! swipe from the left edge goes back. Every gesture the agent makes therefore stays inside
//! the *safe area*: below the top [`TOP_RESERVED`] and above the bottom [`BOTTOM_RESERVED`]
//! of the screen height, and [`SIDE_RESERVED`] in from each side.
//!
//! Within the region (the element's frame clipped to the safe area, or the whole safe area)
//! the finger travels through the middle [`TRAVEL_FRACTION`] of the region along the gesture
//! axis, at least [`MIN_TRAVEL`] points when the safe area allows.
//!
//! Direction semantics follow `jarvis_proto::Direction`: for a *scroll* the direction is
//! where the content should move into view (`Down` shows what is below, so the finger moves
//! up); for a *swipe* it is the finger's direction.

use jarvis_proto::{Direction, Point, Rect, Size};
use thiserror::Error;

/// Share of the screen height at the top kept free of gestures (status bar, Notification
/// Center pull-down).
pub const TOP_RESERVED: f64 = 0.08;
/// Share of the screen height at the bottom kept free of gestures (home indicator).
pub const BOTTOM_RESERVED: f64 = 0.10;
/// Share of the screen width kept free on each side (edge-swipe back).
pub const SIDE_RESERVED: f64 = 0.04;
/// Share of the region along the axis the finger travels.
pub const TRAVEL_FRACTION: f64 = 0.6;
/// Minimum travel in points, when the safe area is long enough.
pub const MIN_TRAVEL: f64 = 80.0;
/// Duration of a scroll gesture (slow enough not to fling far).
pub const SCROLL_DURATION_MS: u64 = 400;
/// Duration of a swipe gesture.
pub const SWIPE_DURATION_MS: u64 = 200;

/// Scroll (content direction) or swipe (finger direction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureKind {
    /// Move content so that what lies in `direction` comes into view.
    Scroll,
    /// Move the finger in `direction`.
    Swipe,
}

/// A computed drag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gesture {
    /// Start point in screen points.
    pub from: Point,
    /// End point in screen points.
    pub to: Point,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// Why no gesture could be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum GestureError {
    /// The screen size is zero, negative or not a number.
    #[error("the screen size is unknown")]
    BadScreen,
    /// The element lies entirely in the reserved system-gesture area.
    #[error("the element is at the very edge of the screen, where system gestures live")]
    OutsideSafeArea,
    /// Fourteenth review (Rule H): the drag would start or end on the keyboard (its keys
    /// enter text; a drag from a key can commit a word).
    #[error(
        "the keyboard is up and this drag would start or end on its keys; dismiss the keyboard first"
    )]
    OnKeyboard,
    /// Run 146 (P13): the drag crosses a list row so narrow that the longest sideways travel
    /// kept safe on it ([`ROW_SWIPE_SHARE`] of the row) is shorter than a drag needs to
    /// register ([`MIN_TRAVEL`]): nothing is sent.
    #[error(
        "the drag crosses a list row too narrow to swipe safely: a swipe long enough to register could trigger the row's full-swipe action (such as delete); nothing was sent, use the row's own buttons instead"
    )]
    RowTooNarrow,
}

/// Share of the screen height, from the bottom, that an element-less drag leaves to the
/// keyboard while one is up (fourteenth review, Rule H; generous on purpose: the iPhone
/// keyboard takes about 40 % in portrait).
pub const KEYBOARD_SHARE: f64 = 0.5;

/// The drag the runner makes for a scroll or swipe on `frame` on `screen` (fourteenth
/// review, Rule H): [`gesture`] on the element's frame, or for an element-less drag while a
/// keyboard is up on the part of the screen above [`KEYBOARD_SHARE`] (never on the keys).
/// The classifier reads the same path the commands send.
pub fn drag_gesture(
    kind: GestureKind,
    direction: Direction,
    frame: Option<Rect>,
    screen: &jarvis_proto::CompactScreen,
) -> Result<Gesture, GestureError> {
    let frame = frame.or_else(|| {
        screen.keyboard_visible.then(|| {
            Rect::new(
                0.0,
                0.0,
                screen.size.width,
                screen.size.height * (1.0 - KEYBOARD_SHARE),
            )
        })
    });
    gesture(kind, direction, frame, screen.size)
}

/// The rectangle gestures may use on a screen of `size`.
pub fn safe_area(size: Size) -> Option<Rect> {
    let r = Rect::new(
        size.width * SIDE_RESERVED,
        size.height * TOP_RESERVED,
        size.width * (1.0 - 2.0 * SIDE_RESERVED),
        size.height * (1.0 - TOP_RESERVED - BOTTOM_RESERVED),
    );
    (size.width.is_finite() && size.height.is_finite() && !r.is_empty()).then_some(r)
}

/// The finger direction for a gesture.
pub fn finger_direction(kind: GestureKind, direction: Direction) -> Direction {
    match (kind, direction) {
        (GestureKind::Swipe, d) => d,
        (GestureKind::Scroll, Direction::Up) => Direction::Down,
        (GestureKind::Scroll, Direction::Down) => Direction::Up,
        (GestureKind::Scroll, Direction::Left) => Direction::Right,
        (GestureKind::Scroll, Direction::Right) => Direction::Left,
    }
}

/// Start and end of the travel along one axis: centred on the region, `TRAVEL_FRACTION`
/// of its length, widened to `MIN_TRAVEL` but never past the safe interval.
fn span(region_lo: f64, region_hi: f64, safe_lo: f64, safe_hi: f64) -> (f64, f64) {
    let centre = (region_lo + region_hi) / 2.0;
    let half = ((region_hi - region_lo) * TRAVEL_FRACTION / 2.0).max(MIN_TRAVEL / 2.0);
    let lo = (centre - half).max(safe_lo);
    let hi = (centre + half).min(safe_hi);
    (lo, hi)
}

/// Probe round 3 (Mac run 139, p4d; F1): the longest sideways travel, as a share of the
/// screen width. On the real iOS 26.5 list (a 400-pt row on a 440-pt screen) the core's own
/// row swipe — 240 pt, 200 ms, (340, y) → (100, y), i.e. 240/440 of the width — only revealed
/// the row's actions ("Flag", "Delete"), while a swipe from one edge of the safe area to the
/// other (422.4 → 17.6) **deleted** the row with no tap. Every sideways scroll or swipe Jarvis
/// sends is capped at that measured path (the element-less one was 242.9 pt). Other apps'
/// full-swipe thresholds are not measured: a sideways drag across a row still asks
/// (`jarvis_safety`'s row-swipe rule); this cap only keeps Jarvis's own drag at the
/// measured-safe length. Relative to the width, so it holds in landscape too.
pub const MAX_SIDEWAYS_SHARE: f64 = 240.0 / 440.0;

/// Run 146 (Mac, probe P13; P05b Reminders agrees): the longest sideways travel across a
/// list row, as a share of the **row's** width. On a 400-pt row (`Cell {{20, 278}, {400,
/// 52}}` on a 440-pt screen) a 240-pt drag (0.60 of the row) only revealed the row's actions,
/// and every drag of 264 pt (0.66) or more deleted the row — through the runner and as an
/// XCUITest drag at 200 ms. 0.55 of the row (220 pt there) stays below the length measured
/// to reveal only, 0.11 of the row (17 %) below the measured delete threshold: the margin
/// covers a threshold that is a share of the row (UIKit's full swipe is) on a narrower row,
/// and a drag landing a few points long. A row too narrow for [`MIN_TRAVEL`] within this
/// share (under about 145 pt) gets no drag at all ([`GestureError::RowTooNarrow`]).
pub const ROW_SWIPE_SHARE: f64 = 0.55;

/// `g` with its travel at most `max` long: the start kept (it is where the gate read the
/// drag from), the end moved towards it.
pub fn shortened(g: Gesture, max: f64) -> Gesture {
    let (dx, dy) = (g.to.x - g.from.x, g.to.y - g.from.y);
    let len = dx.hypot(dy);
    if !(max.is_finite() && max >= 0.0 && len > max) {
        return g;
    }
    let k = max / len;
    Gesture {
        to: Point::new(g.from.x + dx * k, g.from.y + dy * k),
        ..g
    }
}

/// `lo..hi` shrunk around its centre to at most `max` long.
fn capped(lo: f64, hi: f64, max: f64) -> (f64, f64) {
    if !(max.is_finite() && max > 0.0) || hi - lo <= max {
        return (lo, hi);
    }
    let centre = (lo + hi) / 2.0;
    (centre - max / 2.0, centre + max / 2.0)
}

/// Computes a scroll or swipe on `frame` (or the whole screen when `None`).
pub fn gesture(
    kind: GestureKind,
    direction: Direction,
    frame: Option<Rect>,
    size: Size,
) -> Result<Gesture, GestureError> {
    let safe = safe_area(size).ok_or(GestureError::BadScreen)?;
    let region = match frame {
        Some(f) => f.intersection(&safe).ok_or(GestureError::OutsideSafeArea)?,
        None => safe,
    };
    let c = region.center();
    let finger = finger_direction(kind, direction);
    let (from, to) = match finger {
        Direction::Up | Direction::Down => {
            let (top, bottom) = span(region.y, region.max_y(), safe.y, safe.max_y());
            if finger == Direction::Up {
                (Point::new(c.x, bottom), Point::new(c.x, top))
            } else {
                (Point::new(c.x, top), Point::new(c.x, bottom))
            }
        }
        Direction::Left | Direction::Right => {
            let (left, right) = span(region.x, region.max_x(), safe.x, safe.max_x());
            // Probe round 3 (F1): never a longer sideways drag than the one measured to
            // reveal a list row's actions without triggering them.
            let (left, right) = capped(left, right, size.width * MAX_SIDEWAYS_SHARE);
            if finger == Direction::Left {
                (Point::new(right, c.y), Point::new(left, c.y))
            } else {
                (Point::new(left, c.y), Point::new(right, c.y))
            }
        }
    };
    let duration_ms = match kind {
        GestureKind::Scroll => SCROLL_DURATION_MS,
        GestureKind::Swipe => SWIPE_DURATION_MS,
    };
    Ok(Gesture {
        from,
        to,
        duration_ms,
    })
}

/// Converts a normalized (0..1) point to screen points.
pub fn denormalize(x: f64, y: f64, size: Size) -> Point {
    Point::new(x * size.width, y * size.height)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const PHONE: Size = Size::new(393.0, 852.0);

    fn inside_safe(p: Point) -> bool {
        let s = safe_area(PHONE).unwrap();
        p.x >= s.x && p.x <= s.max_x() && p.y >= s.y && p.y <= s.max_y()
    }

    #[test]
    fn screen_scroll_down_moves_finger_up_inside_safe_area() {
        let g = gesture(GestureKind::Scroll, Direction::Down, None, PHONE).unwrap();
        assert!(g.from.y > g.to.y, "finger moves up to reveal content below");
        assert_eq!(g.from.x, g.to.x);
        assert!(inside_safe(g.from) && inside_safe(g.to));
        assert!(g.from.y < PHONE.height * 0.9 && g.to.y > PHONE.height * 0.08);
        assert!(g.from.y - g.to.y >= MIN_TRAVEL);
        assert_eq!(g.duration_ms, SCROLL_DURATION_MS);
    }

    #[test]
    fn every_direction_and_kind_stays_safe() {
        let frames = [
            None,
            Some(Rect::new(0.0, 0.0, 393.0, 852.0)),
            Some(Rect::new(16.0, 200.0, 361.0, 44.0)),
            Some(Rect::new(0.0, 700.0, 393.0, 152.0)),
            Some(Rect::new(300.0, 100.0, 93.0, 30.0)),
        ];
        for kind in [GestureKind::Scroll, GestureKind::Swipe] {
            for d in [
                Direction::Up,
                Direction::Down,
                Direction::Left,
                Direction::Right,
            ] {
                for f in frames {
                    let g = gesture(kind, d, f, PHONE).unwrap();
                    assert!(
                        inside_safe(g.from),
                        "{kind:?} {d:?} {f:?} from {:?}",
                        g.from
                    );
                    assert!(inside_safe(g.to), "{kind:?} {d:?} {f:?} to {:?}", g.to);
                    assert_ne!(g.from, g.to);
                }
            }
        }
    }

    #[test]
    fn swipe_left_on_a_cell_stays_in_its_row() {
        let cell = Rect::new(16.0, 300.0, 361.0, 44.0);
        let g = gesture(GestureKind::Swipe, Direction::Left, Some(cell), PHONE).unwrap();
        assert!(g.from.x > g.to.x);
        assert_eq!(g.from.y, cell.center().y);
        assert!(g.from.x <= cell.max_x() && g.to.x >= cell.x);
        assert_eq!(g.duration_ms, SWIPE_DURATION_MS);
    }

    #[test]
    fn short_region_gets_minimum_travel() {
        let small = Rect::new(100.0, 400.0, 200.0, 20.0);
        let g = gesture(GestureKind::Scroll, Direction::Up, Some(small), PHONE).unwrap();
        assert!((g.to.y - g.from.y - MIN_TRAVEL).abs() < 1e-9);
    }

    #[test]
    fn element_in_reserved_area_is_rejected() {
        let status = Rect::new(0.0, 0.0, 393.0, 40.0);
        assert_eq!(
            gesture(GestureKind::Scroll, Direction::Down, Some(status), PHONE),
            Err(GestureError::OutsideSafeArea)
        );
        assert_eq!(
            gesture(GestureKind::Swipe, Direction::Up, None, Size::new(0.0, 0.0)),
            Err(GestureError::BadScreen)
        );
        assert_eq!(
            gesture(
                GestureKind::Swipe,
                Direction::Up,
                None,
                Size::new(f64::NAN, 10.0)
            ),
            Err(GestureError::BadScreen)
        );
    }

    #[test]
    fn denormalize_scales_by_screen() {
        let p = denormalize(0.5, 0.25, PHONE);
        assert_eq!(p, Point::new(196.5, 213.0));
    }
}
