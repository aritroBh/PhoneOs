//! The Live view's scrubber: which step is shown, live or not, playing or paused, at what
//! speed, and when the shown step's animation started. Pure: every change takes the step
//! numbers the timeline holds now (oldest first) and the current time in milliseconds (any
//! monotonic clock).
//!
//! - **Live** (the default): follows the newest step; each new step starts its animation.
//! - Stepping, scrubbing or playing leaves live. New steps arriving then never move the
//!   shown step; if the cap drops the shown step, the oldest kept step is shown.
//! - [`ReplayState::go_live`] returns to following.

use crate::geometry::{smax, smin, ViewerPoint};

/// Replay speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Speed {
    /// 1×.
    #[default]
    Normal,
    /// 2×.
    Double,
}

impl Speed {
    /// The factor.
    pub fn factor(self) -> f64 {
        match self {
            Speed::Normal => 1.0,
            Speed::Double => 2.0,
        }
    }

    /// "1×" / "2×".
    pub fn title(self) -> &'static str {
        match self {
            Speed::Normal => "1×",
            Speed::Double => "2×",
        }
    }

    /// The speed the speed button switches to.
    pub fn next(self) -> Speed {
        match self {
            Speed::Normal => Speed::Double,
            Speed::Double => Speed::Normal,
        }
    }
}

/// The scrubber's state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplayState {
    current: Option<u32>,
    following: bool,
    playing: bool,
    speed: Speed,
    started_at_ms: f64,
    origin: Option<ViewerPoint>,
}

impl Default for ReplayState {
    fn default() -> Self {
        ReplayState {
            current: None,
            following: true,
            playing: false,
            speed: Speed::Normal,
            started_at_ms: 0.0,
            origin: None,
        }
    }
}

impl ReplayState {
    /// Following live, nothing shown yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The shown step's number, `None` while there are no steps.
    pub fn current(&self) -> Option<u32> {
        self.current
    }

    /// Following the newest step.
    pub fn is_following(&self) -> bool {
        self.following
    }

    /// Replay is playing (never while following).
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// The replay speed.
    pub fn speed(&self) -> Speed {
        self.speed
    }

    /// When the shown step's animation started (ms).
    pub fn started_at_ms(&self) -> f64 {
        self.started_at_ms
    }

    /// Where the cursor starts the shown step: set when, following live, a new step arrived
    /// while the cursor was still moving (it continues from there instead of jumping);
    /// `None` starts from the end of the previous pointer step.
    pub fn origin(&self) -> Option<ViewerPoint> {
        self.origin
    }

    /// Milliseconds into the shown step's animation at `now_ms` (scaled by the speed in
    /// replay).
    pub fn elapsed_ms(&self, now_ms: f64) -> f64 {
        let elapsed = if now_ms.is_finite() && self.started_at_ms.is_finite() {
            smax(0.0, now_ms - self.started_at_ms)
        } else {
            0.0
        };
        elapsed
            * if self.following {
                1.0
            } else {
                self.speed.factor()
            }
    }

    /// The timeline changed (a step was added, finished or dropped). While following, a new
    /// step starts from `cursor` (where the cursor is now, see `scene::synced`) when given.
    pub fn sync(&mut self, numbers: &[u32], now_ms: f64, cursor: Option<ViewerPoint>) {
        let (Some(&first), Some(&last)) = (numbers.first(), numbers.last()) else {
            self.current = None;
            self.playing = false;
            self.origin = None;
            return;
        };
        if self.following {
            if self.current != Some(last) {
                self.show(last, now_ms, cursor);
            }
            return;
        }
        match self.current {
            None => self.show(first, now_ms, None),
            // Dropped by the cap: the oldest kept step.
            Some(shown) if !numbers.contains(&shown) => self.show(first, now_ms, None),
            Some(_) => {}
        }
    }

    /// One step back (leaves live, pauses).
    pub fn step_back(&mut self, numbers: &[u32], now_ms: f64) {
        let Some(i) = self.index_in(numbers) else {
            return;
        };
        self.following = false;
        self.playing = false;
        if let Some(&n) = numbers.get(i.saturating_sub(1)) {
            self.show(n, now_ms, None);
        }
    }

    /// One step forward (leaves live, pauses). At the newest step it stays there.
    pub fn step_forward(&mut self, numbers: &[u32], now_ms: f64) {
        let Some(i) = self.index_in(numbers) else {
            return;
        };
        self.following = false;
        self.playing = false;
        if let Some(&n) = numbers.get((i + 1).min(numbers.len() - 1)) {
            self.show(n, now_ms, None);
        }
    }

    /// Shows the step numbered `number` (leaves live, pauses). Unknown numbers are ignored.
    pub fn scrub_to(&mut self, number: u32, numbers: &[u32], now_ms: f64) {
        if !numbers.contains(&number) {
            return;
        }
        self.following = false;
        self.playing = false;
        self.show(number, now_ms, None);
    }

    /// Shows the step at `fraction` (0…1) of the kept steps (the scrubber's value).
    pub fn scrub_fraction(&mut self, fraction: f64, numbers: &[u32], now_ms: f64) {
        if numbers.is_empty() {
            return;
        }
        let f = if fraction.is_finite() {
            smin(smax(fraction, 0.0), 1.0)
        } else {
            0.0
        };
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let i = (f * (numbers.len() - 1) as f64).round() as usize;
        if let Some(&n) = numbers.get(i.min(numbers.len() - 1)) {
            self.scrub_to(n, numbers, now_ms);
        }
    }

    /// The scrubber's value for the shown step (0…1; 1 while following).
    pub fn fraction(&self, numbers: &[u32]) -> f64 {
        if numbers.len() <= 1 {
            return 1.0;
        }
        let Some(i) = self.index_in(numbers) else {
            return 1.0;
        };
        #[allow(clippy::cast_precision_loss)]
        let v = i as f64 / (numbers.len() - 1) as f64;
        v
    }

    /// Play / pause. Playing leaves live; at the newest step it starts again from the
    /// oldest.
    pub fn toggle_play(&mut self, numbers: &[u32], now_ms: f64) {
        let Some(&first) = numbers.first() else {
            return;
        };
        if self.playing {
            self.playing = false;
            return;
        }
        self.following = false;
        self.playing = true;
        match self.current {
            None => self.show(first, now_ms, None),
            Some(c) if Some(&c) == numbers.last() => self.show(first, now_ms, None),
            Some(c) => self.show(c, now_ms, None),
        }
    }

    /// Changes the speed, keeping the animation where it is.
    pub fn set_speed(&mut self, speed: Speed, now_ms: f64) {
        if speed == self.speed {
            return;
        }
        if !self.following {
            let elapsed = self.elapsed_ms(now_ms);
            self.started_at_ms = now_ms - elapsed / speed.factor();
        }
        self.speed = speed;
    }

    /// Back to following the newest step.
    pub fn go_live(&mut self, numbers: &[u32], now_ms: f64) {
        self.following = true;
        self.playing = false;
        match numbers.last() {
            Some(&last) => {
                if self.current != Some(last) {
                    self.show(last, now_ms, None);
                }
            }
            None => self.current = None,
        }
    }

    /// While playing: moves to the next step once the shown one has played for
    /// `play_ms(current)` milliseconds; stops at the newest step. Returns whether the shown
    /// step changed.
    pub fn advance(
        &mut self,
        numbers: &[u32],
        now_ms: f64,
        play_ms: impl FnOnce(u32) -> f64,
    ) -> bool {
        if !self.playing {
            return false;
        }
        let Some(shown) = self.current else {
            return false;
        };
        let Some(i) = numbers.iter().position(|&n| n == shown) else {
            return false;
        };
        // `elapsed_ms` is never NaN.
        if self.elapsed_ms(now_ms) < smax(0.0, play_ms(shown)) {
            return false;
        }
        if let Some(&next) = numbers.get(i + 1) {
            self.show(next, now_ms, None);
            return true;
        }
        self.playing = false;
        false
    }

    fn index_in(&self, numbers: &[u32]) -> Option<usize> {
        if numbers.is_empty() {
            return None;
        }
        if let Some(i) = self
            .current
            .and_then(|c| numbers.iter().position(|&n| n == c))
        {
            return Some(i);
        }
        Some(numbers.len() - 1)
    }

    fn show(&mut self, number: u32, now_ms: f64, cursor: Option<ViewerPoint>) {
        self.current = Some(number);
        self.started_at_ms = if now_ms.is_finite() { now_ms } else { 0.0 };
        self.origin = cursor.filter(ViewerPoint::is_finite);
    }
}
