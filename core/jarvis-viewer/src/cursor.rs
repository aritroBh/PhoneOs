//! Where the Live view's cursor is at a moment of a step: the one source of truth for the
//! animation (the view only asks for a pose every frame).
//!
//! A pointer step glides from where the cursor rested to the press point (ease-in-out, the
//! duration grows with the distance and is clamped), then presses (tap ripple, long-press
//! ring filling over the hold) or drags along the swipe, and rests on the end point. Other
//! gestures (typing, keys, opening an app or URL, waiting) leave the cursor parked where it
//! was. All points are clamped onto the screen; NaN and negative times are handled.

use crate::geometry::{smax, smin, ViewerPoint, ViewerSize};
use crate::wire::{Gesture, ViewerTimeline};

/// Glide duration bounds, in ms.
pub const GLIDE_RANGE_MS: (f64, f64) = (180.0, 700.0);
/// Glide milliseconds per point of distance.
pub const GLIDE_MS_PER_POINT: f64 = 1.1;
/// A tap's press (the ripple).
pub const TAP_MS: f64 = 280.0;
/// A long press shows its real hold, clamped to this range.
pub const LONG_PRESS_RANGE_MS: (f64, f64) = (300.0, 2_500.0);
/// A swipe shows its real duration, clamped to this range.
pub const SWIPE_RANGE_MS: (f64, f64) = (200.0, 1_500.0);
/// How long a non-pointer step plays (its caption is the show).
pub const PARKED_MS: f64 = 700.0;
/// A wait plays its real length, clamped to this range.
pub const WAIT_RANGE_MS: (f64, f64) = (400.0, 1_500.0);
/// Pause after a step before replay moves to the next, so the result can be seen.
pub const SETTLE_MS: f64 = 450.0;

/// The cursor's phase within a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Moving to the press point.
    Gliding,
    /// Finger down on the point (tap ripple / long-press ring); progress runs 0…1.
    Pressing,
    /// Dragging from the press point to the end point; progress runs 0…1.
    Swiping,
    /// Done; resting on the end point.
    Resting,
    /// A non-pointer step: the cursor stays where it was.
    Parked,
}

/// The cursor at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// In screen points, on the screen.
    pub position: ViewerPoint,
    /// The phase.
    pub phase: Phase,
    /// Progress through `phase`, 0…1.
    pub progress: f64,
}

impl Pose {
    /// The finger is down.
    pub fn is_pressed(&self) -> bool {
        matches!(self.phase, Phase::Pressing | Phase::Swiping)
    }
}

/// How long a gesture plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    /// Moving to the press point.
    pub glide_ms: f64,
    /// The press, drag or parked time.
    pub action_ms: f64,
}

impl Timing {
    /// Both.
    pub fn total_ms(&self) -> f64 {
        self.glide_ms + self.action_ms
    }
}

/// Ease-in-out (smoothstep): 0 → 0, 1 → 1, monotone, flat at both ends. Clamped.
pub fn ease(t: f64) -> f64 {
    if !t.is_finite() {
        return 0.0;
    }
    let x = smin(smax(t, 0.0), 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Glide duration for a distance in points.
pub fn glide_ms(distance: f64) -> f64 {
    let d = if distance.is_finite() {
        smax(distance, 0.0)
    } else {
        0.0
    };
    smin(
        smax(d * GLIDE_MS_PER_POINT, GLIDE_RANGE_MS.0),
        GLIDE_RANGE_MS.1,
    )
}

fn clamped(value: f64, range: (f64, f64)) -> f64 {
    smin(
        smax(if value.is_finite() { value } else { range.0 }, range.0),
        range.1,
    )
}

#[allow(clippy::cast_precision_loss)]
fn ms(v: u64) -> f64 {
    v as f64
}

/// How long `gesture` plays when the cursor starts at `rest`. Reduce Motion: no glide.
pub fn timing(
    gesture: &Gesture,
    rest: ViewerPoint,
    screen: ViewerSize,
    reduce_motion: bool,
) -> Timing {
    let start = screen.clamp(rest);
    let glide_to = |p: ViewerPoint| {
        if reduce_motion {
            0.0
        } else {
            glide_ms(start.distance(screen.clamp(p)))
        }
    };
    match gesture {
        Gesture::Tap(p) => Timing {
            glide_ms: glide_to(*p),
            action_ms: TAP_MS,
        },
        Gesture::LongPress { point, duration_ms } => Timing {
            glide_ms: glide_to(*point),
            action_ms: clamped(ms(*duration_ms), LONG_PRESS_RANGE_MS),
        },
        Gesture::Swipe {
            from, duration_ms, ..
        } => Timing {
            glide_ms: glide_to(*from),
            action_ms: clamped(ms(*duration_ms), SWIPE_RANGE_MS),
        },
        Gesture::Wait(w) => Timing {
            glide_ms: 0.0,
            action_ms: clamped(ms(*w), WAIT_RANGE_MS),
        },
        Gesture::Type(_)
        | Gesture::Key(_)
        | Gesture::Launch(_)
        | Gesture::OpenUrl(_)
        | Gesture::Unknown(_) => Timing {
            glide_ms: 0.0,
            action_ms: PARKED_MS,
        },
    }
}

/// How long replay shows a step before moving on (its timing plus [`SETTLE_MS`]).
pub fn play_ms(
    gesture: &Gesture,
    rest: ViewerPoint,
    screen: ViewerSize,
    reduce_motion: bool,
) -> f64 {
    timing(gesture, rest, screen, reduce_motion).total_ms() + SETTLE_MS
}

fn fraction(t: f64, total: f64) -> f64 {
    if total.is_nan() || total <= 0.0 || !t.is_finite() {
        return 1.0;
    }
    smin(smax(t / total, 0.0), 1.0)
}

/// The cursor `t` milliseconds into `gesture`, starting from `rest`.
pub fn pose(
    gesture: &Gesture,
    rest: ViewerPoint,
    screen: ViewerSize,
    t: f64,
    reduce_motion: bool,
) -> Pose {
    let time = if t.is_finite() { smax(t, 0.0) } else { 0.0 };
    let start = screen.clamp(rest);
    let timing = timing(gesture, rest, screen, reduce_motion);
    let (Some(press_raw), Some(end_raw)) = (gesture.press_point(), gesture.end_point()) else {
        return Pose {
            position: start,
            phase: Phase::Parked,
            progress: fraction(time, timing.action_ms),
        };
    };
    let press = screen.clamp(press_raw);
    let end = screen.clamp(end_raw);
    if time < timing.glide_ms {
        return Pose {
            position: start.interpolated(press, ease(time / timing.glide_ms)),
            phase: Phase::Gliding,
            progress: time / timing.glide_ms,
        };
    }
    let in_action = time - timing.glide_ms;
    if in_action < timing.action_ms {
        let p = fraction(in_action, timing.action_ms);
        if matches!(gesture, Gesture::Swipe { .. }) {
            // Reduce Motion: the cursor does not travel; the trail still shows the drag.
            let position = if reduce_motion {
                end
            } else {
                press.interpolated(end, ease(p))
            };
            return Pose {
                position,
                phase: Phase::Swiping,
                progress: p,
            };
        }
        return Pose {
            position: press,
            phase: Phase::Pressing,
            progress: p,
        };
    }
    Pose {
        position: end,
        phase: Phase::Resting,
        progress: 1.0,
    }
}

/// Where the cursor rests after `gesture` started from `rest`.
pub fn resting_point_after(
    gesture: &Gesture,
    rest: ViewerPoint,
    screen: ViewerSize,
) -> ViewerPoint {
    match gesture.end_point() {
        Some(end) => screen.clamp(end),
        None => screen.clamp(rest),
    }
}

fn screen_size(timeline: &ViewerTimeline, screen_id: Option<u32>) -> Option<ViewerSize> {
    timeline.screen(screen_id).map(|s| s.size)
}

/// Where the cursor rests before step `number` of `timeline`: the end of the latest pointer
/// step before it, else the centre of the step's screen (the first step glides in from the
/// middle).
pub fn resting_point_before(number: u32, timeline: &ViewerTimeline) -> ViewerPoint {
    let Some(index) = timeline.step_index(number) else {
        return timeline
            .current_screen()
            .map_or(ViewerSize::FALLBACK_PHONE, |s| s.size)
            .center();
    };
    let size = timeline
        .steps
        .get(index)
        .and_then(|s| screen_size(timeline, s.screen_id))
        .unwrap_or(ViewerSize::FALLBACK_PHONE);
    for step in timeline.steps.get(..index).unwrap_or_default().iter().rev() {
        if let Some(end) = step.gesture.end_point() {
            let step_size = screen_size(timeline, step.screen_id).unwrap_or(size);
            return step_size.clamp(end);
        }
    }
    size.center()
}

/// Where the cursor rests after the newest step (or the centre of the current screen).
pub fn resting_point_after_latest(timeline: &ViewerTimeline) -> ViewerPoint {
    let size = timeline
        .current_screen()
        .map_or(ViewerSize::FALLBACK_PHONE, |s| s.size);
    for step in timeline.steps.iter().rev() {
        if let Some(end) = step.gesture.end_point() {
            return screen_size(timeline, step.screen_id)
                .unwrap_or(size)
                .clamp(end);
        }
    }
    size.center()
}

/// The pose of step `number` of `timeline` at `t` ms (the view's one call per frame).
/// `origin` is where the cursor starts (`ReplayState::origin`: where it was when a live
/// step interrupted the one before); `None` starts from [`resting_point_before`].
pub fn pose_of_step(
    number: u32,
    timeline: &ViewerTimeline,
    t: f64,
    origin: Option<ViewerPoint>,
    reduce_motion: bool,
) -> Option<Pose> {
    let step = timeline.step(Some(number))?;
    let size = screen_size(timeline, step.screen_id).unwrap_or(ViewerSize::FALLBACK_PHONE);
    let rest = origin.unwrap_or_else(|| resting_point_before(number, timeline));
    Some(pose(&step.gesture, rest, size, t, reduce_motion))
}

/// How long replay shows step `number` of `timeline` (starting at `origin`, see
/// [`pose_of_step`]).
pub fn play_ms_of_step(
    number: u32,
    timeline: &ViewerTimeline,
    origin: Option<ViewerPoint>,
    reduce_motion: bool,
) -> f64 {
    let Some(step) = timeline.step(Some(number)) else {
        return SETTLE_MS;
    };
    let size = screen_size(timeline, step.screen_id).unwrap_or(ViewerSize::FALLBACK_PHONE);
    let rest = origin.unwrap_or_else(|| resting_point_before(number, timeline));
    play_ms(&step.gesture, rest, size, reduce_motion)
}
