//! [`LiveView`]: the reducer, the scrubber and the per-frame scene behind one value, so a
//! shell needs one call per driver update and one per drawn frame.
//!
//! The scrubber follows the timeline lazily: the first query after the step numbers
//! changed re-syncs it at that query's time (a step arriving mid-animation continues from
//! where the cursor is, see [`scene::synced`]); a new run (`Update::Started`) starts a
//! fresh scrubber that follows live.

use crate::cursor;
use crate::geometry::{ViewerMapping, ViewerPoint, ViewerRect, ViewerSize};
use crate::reducer::{Change, Reducer, ReducerConfig, Update};
use crate::replay::{ReplayState, Speed};
use crate::scene::{self, Scene, Tone};
use crate::wire::{Gesture, ViewerTimeline};

/// A scrubber control.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplayAction {
    /// One step back (leaves live, pauses).
    StepBack,
    /// One step forward (leaves live, pauses).
    StepForward,
    /// Play / pause.
    TogglePlay,
    /// Show the step with this number.
    ScrubTo(u32),
    /// Show the step at this fraction (0…1) of the kept steps.
    ScrubFraction(f64),
    /// Set the speed.
    SetSpeed(Speed),
    /// Switch 1× ↔ 2×.
    NextSpeed,
    /// Back to following the newest step.
    GoLive,
    /// A fresh scrubber following live (a Live view appeared).
    Reset,
}

/// One drawn frame: the scene plus what the view needs to schedule and tint it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame<'a> {
    /// What to draw.
    pub scene: Scene<'a>,
    /// Whether the view must keep redrawing every display frame.
    pub needs_frames: bool,
    /// The caption dot's colour.
    pub tone: Tone,
}

/// The scene's geometry in a view of a given size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewGeometry {
    /// Screen points → view points.
    pub mapping: ViewerMapping,
    /// The cursor's tip, in view points.
    pub cursor: ViewerPoint,
    /// The shown step's target, in view points (clipped), when it has one on screen.
    pub target: Option<ViewerRect>,
    /// A swipe's trail (start, current end) while it drags or rests, in view points.
    pub trail: Option<(ViewerPoint, ViewerPoint)>,
}

impl Frame<'_> {
    /// The screen size the backdrop is drawn with.
    pub fn screen_size(&self) -> ViewerSize {
        self.scene
            .screen
            .map_or(ViewerSize::FALLBACK_PHONE, |s| s.size.usable())
    }

    /// The geometry in a view of `view` points.
    pub fn in_view(&self, view: ViewerSize) -> ViewGeometry {
        let mapping = ViewerMapping::new(self.screen_size(), view);
        let pose = self.scene.pose;
        let cursor = mapping.to_view(pose.position);
        let target = self
            .scene
            .step
            .and_then(|s| s.target.as_ref())
            .and_then(|t| mapping.rect_to_view(&t.frame));
        let trail = match self.scene.step.map(|s| &s.gesture) {
            Some(Gesture::Swipe { from, to, .. })
                if matches!(pose.phase, cursor::Phase::Swiping | cursor::Phase::Resting) =>
            {
                let end = if pose.phase == cursor::Phase::Resting {
                    mapping.to_view(*to)
                } else {
                    cursor
                };
                Some((mapping.to_view(*from), end))
            }
            _ => None,
        };
        ViewGeometry {
            mapping,
            cursor,
            target,
            trail,
        }
    }
}

/// The Live view's state for one run.
#[derive(Debug, Clone)]
pub struct LiveView {
    reducer: Reducer,
    replay: ReplayState,
    numbers: Vec<u32>,
    /// Bumped whenever the step numbers change.
    numbers_generation: u64,
    /// `numbers_generation` the scrubber was last synced to.
    synced_generation: Option<u64>,
    /// A new run started: the next query starts a fresh scrubber.
    fresh: bool,
    /// Bumped by every update that changed something.
    generation: u64,
}

impl Default for LiveView {
    fn default() -> Self {
        LiveView::new(ReducerConfig::default())
    }
}

impl LiveView {
    /// An empty Live view.
    pub fn new(config: ReducerConfig) -> Self {
        Self::with_reducer(Reducer::new(config))
    }

    /// A Live view of a decoded timeline (a remote controller's copy).
    pub fn from_timeline(timeline: ViewerTimeline, config: ReducerConfig) -> Self {
        Self::with_reducer(Reducer::resume(timeline, config))
    }

    fn with_reducer(reducer: Reducer) -> Self {
        let numbers = reducer.timeline().step_numbers();
        LiveView {
            reducer,
            replay: ReplayState::new(),
            numbers,
            numbers_generation: 0,
            synced_generation: None,
            fresh: true,
            generation: 0,
        }
    }

    /// The timeline so far.
    pub fn timeline(&self) -> &ViewerTimeline {
        self.reducer.timeline()
    }

    /// The reducer.
    pub fn reducer(&self) -> &Reducer {
        &self.reducer
    }

    /// The scrubber, as of the last query or control.
    pub fn replay(&self) -> &ReplayState {
        &self.replay
    }

    /// The kept step numbers, oldest first.
    pub fn numbers(&self) -> &[u32] {
        &self.numbers
    }

    /// Bumped by every update that changed something.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Feeds one driver update.
    pub fn apply(&mut self, update: Update) -> Change {
        let started = matches!(update, Update::Started { .. });
        let change = self.reducer.apply(update);
        if started {
            self.fresh = true;
        }
        if change.steps {
            let steps = &self.reducer.timeline().steps;
            let same = steps.len() == self.numbers.len()
                && steps.first().map(|s| s.number) == self.numbers.first().copied()
                && steps.last().map(|s| s.number) == self.numbers.last().copied();
            if !same {
                self.numbers.clear();
                self.numbers.extend(steps.iter().map(|s| s.number));
                self.numbers_generation += 1;
            }
        }
        if change.any() || started {
            self.generation += 1;
        }
        change
    }

    /// Brings the scrubber up to date at `now_ms` and advances playback.
    fn prepare(&mut self, now_ms: f64, reduce_motion: bool) {
        let timeline = self.reducer.timeline();
        if self.fresh {
            self.fresh = false;
            self.replay = ReplayState::new();
            self.replay.sync(&self.numbers, now_ms, None);
            self.synced_generation = Some(self.numbers_generation);
        } else if self.synced_generation != Some(self.numbers_generation) {
            self.replay =
                scene::synced(&self.replay, timeline, &self.numbers, now_ms, reduce_motion);
            self.synced_generation = Some(self.numbers_generation);
        }
        if self.replay.is_playing() {
            self.replay.advance(&self.numbers, now_ms, |n| {
                cursor::play_ms_of_step(n, timeline, None, reduce_motion)
            });
        }
    }

    /// Applies a scrubber control at `now_ms`.
    pub fn control(&mut self, action: ReplayAction, now_ms: f64, reduce_motion: bool) {
        if action == ReplayAction::Reset {
            self.fresh = true;
        }
        self.prepare(now_ms, reduce_motion);
        let numbers = &self.numbers;
        let r = &mut self.replay;
        match action {
            ReplayAction::StepBack => r.step_back(numbers, now_ms),
            ReplayAction::StepForward => r.step_forward(numbers, now_ms),
            ReplayAction::TogglePlay => r.toggle_play(numbers, now_ms),
            ReplayAction::ScrubTo(n) => r.scrub_to(n, numbers, now_ms),
            ReplayAction::ScrubFraction(f) => r.scrub_fraction(f, numbers, now_ms),
            ReplayAction::SetSpeed(s) => r.set_speed(s, now_ms),
            ReplayAction::NextSpeed => r.set_speed(r.speed().next(), now_ms),
            ReplayAction::GoLive => r.go_live(numbers, now_ms),
            ReplayAction::Reset => {}
        }
    }

    /// The frame to draw at `now_ms` (the view's one call per frame). Syncs the scrubber
    /// and advances playback first.
    pub fn frame(&mut self, now_ms: f64, reduce_motion: bool) -> Frame<'_> {
        self.prepare(now_ms, reduce_motion);
        let timeline = self.reducer.timeline();
        let scene = scene::make(timeline, &self.replay, now_ms, reduce_motion);
        let needs_frames =
            scene::needs_frames_for(&scene, timeline, &self.replay, now_ms, reduce_motion);
        let tone = scene.tone(&timeline.activity);
        Frame {
            scene,
            needs_frames,
            tone,
        }
    }

    /// "Live · 3 steps", "Step 2 of 3", "No steps yet": the scrubber's position.
    pub fn position_text(&self) -> String {
        let count = self.numbers.len();
        if count == 0 {
            return "No steps yet".to_owned();
        }
        if self.replay.is_following() {
            return format!(
                "Live · {count} {}",
                if count == 1 { "step" } else { "steps" }
            );
        }
        let index = self
            .replay
            .current()
            .and_then(|c| self.numbers.iter().position(|&n| n == c))
            .unwrap_or(count - 1);
        format!("Step {} of {count}", index + 1)
    }
}
