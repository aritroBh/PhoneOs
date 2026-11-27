//! Everything the Live view draws at one moment, decided here so the view has no logic of
//! its own: which screen is the backdrop, which step (if any) is playing, the cursor's pose
//! and the caption.
//!
//! While following live, the newest step plays to its end even if Jarvis has already moved
//! on (the command ran before the viewer heard of it); after that the cursor idles on the
//! latest screen and the caption says what Jarvis is doing ("Thinking…", "Waiting for your
//! OK · Delete", "Done"). In replay the scrubbed step is shown on the screen it acted on.
//!
//! [`make`] borrows from the timeline and allocates nothing unless the caption is an
//! activity with a name in it ("Using …", "Waiting for your OK · …").

use std::borrow::Cow;

use crate::cursor::{self, Phase, Pose};
use crate::geometry::ViewerPoint;
use crate::replay::ReplayState;
use crate::wire::{Activity, StepState, ViewerOutcome, ViewerScreen, ViewerStep, ViewerTimeline};

/// What the view draws at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene<'a> {
    /// The backdrop's screen.
    pub screen: Option<&'a ViewerScreen>,
    /// The step being shown (`None` while idle with no step playing).
    pub step: Option<&'a ViewerStep>,
    /// The cursor.
    pub pose: Pose,
    /// The caption.
    pub caption: Cow<'a, str>,
    /// The cursor idles (subtle pulse): Jarvis thinks, looks or waits.
    pub is_idle: bool,
    /// The run is over.
    pub is_finished: bool,
    /// In replay: the shown step's position among the kept steps (1-based) and their
    /// count; `None` while following.
    pub position: Option<(usize, usize)>,
}

/// The caption dot's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tone {
    /// Quiet (looking, thinking, a stopped step, a cancelled or interrupted run).
    Neutral,
    /// A step plays or Jarvis acts.
    Acting,
    /// Waiting on the owner (approval, answer, unlock).
    Waiting,
    /// A failed step, a failed or blocked run.
    Error,
    /// The run succeeded.
    Success,
}

impl Scene<'_> {
    /// "Step 3 of 12" in replay, `None` while following.
    pub fn position_text(&self) -> Option<String> {
        self.position.map(|(i, n)| format!("Step {i} of {n}"))
    }

    /// The VoiceOver value of the viewer: the caption, with the position in replay.
    pub fn accessibility_value(&self) -> String {
        let mut out = String::from(self.caption.as_ref());
        if let Some(step) = self.step {
            if step.state.is_failed() {
                out.push_str(", did not work");
            }
            if step.state == StepState::Stopped {
                out.push_str(", stopped");
            }
        }
        if let Some((i, n)) = self.position {
            out.push_str(&format!(", Step {i} of {n}"));
        }
        out
    }

    /// The caption dot's colour for this scene and `activity`.
    pub fn tone(&self, activity: &Activity) -> Tone {
        if let Some(step) = self.step {
            if step.state == StepState::Stopped {
                return Tone::Neutral;
            }
            return if step.state.is_failed() {
                Tone::Error
            } else {
                Tone::Acting
            };
        }
        match activity {
            Activity::WaitingForApproval(_) | Activity::WaitingForAnswer | Activity::Locked => {
                Tone::Waiting
            }
            Activity::Finished(ViewerOutcome::Success) => Tone::Success,
            Activity::Finished(ViewerOutcome::Cancelled | ViewerOutcome::Interrupted) => {
                Tone::Neutral
            }
            Activity::Finished(ViewerOutcome::Failed | ViewerOutcome::Blocked) => Tone::Error,
            Activity::Acting => Tone::Acting,
            Activity::Idle
            | Activity::Looking
            | Activity::Thinking
            | Activity::UsingTool(_)
            | Activity::Unknown(_) => Tone::Neutral,
        }
    }
}

fn is_finished(timeline: &ViewerTimeline) -> bool {
    matches!(timeline.activity, Activity::Finished(_))
}

/// The cursor idling after the newest step.
pub fn idle_pose(timeline: &ViewerTimeline) -> Pose {
    Pose {
        position: cursor::resting_point_after_latest(timeline),
        phase: Phase::Parked,
        progress: 1.0,
    }
}

/// The scene at `now_ms`.
pub fn make<'a>(
    timeline: &'a ViewerTimeline,
    replay: &ReplayState,
    now_ms: f64,
    reduce_motion: bool,
) -> Scene<'a> {
    let finished = is_finished(timeline);
    if !replay.is_following() {
        if let Some(number) = replay.current() {
            if let Some(index) = timeline.step_index(number) {
                if let Some(step) = timeline.steps.get(index) {
                    let pose = cursor::pose_of_step(
                        number,
                        timeline,
                        replay.elapsed_ms(now_ms),
                        replay.origin(),
                        reduce_motion,
                    );
                    return Scene {
                        screen: timeline.screen_for(step),
                        step: Some(step),
                        pose: pose.unwrap_or_else(|| idle_pose(timeline)),
                        caption: Cow::Borrowed(&step.caption),
                        is_idle: false,
                        is_finished: finished,
                        position: Some((index + 1, timeline.steps.len())),
                    };
                }
            }
        }
    }
    // Following: the newest step plays out, then the cursor idles on the latest screen.
    if let Some(step) = timeline.steps.last() {
        let shown = replay.current() == Some(step.number);
        let elapsed = if shown {
            replay.elapsed_ms(now_ms)
        } else {
            f64::INFINITY
        };
        let origin = if shown { replay.origin() } else { None };
        let playing = timeline.activity == Activity::Acting
            || elapsed < cursor::play_ms_of_step(step.number, timeline, origin, reduce_motion);
        if playing {
            let t = if elapsed.is_finite() { elapsed } else { 0.0 };
            let pose = cursor::pose_of_step(step.number, timeline, t, origin, reduce_motion);
            return Scene {
                screen: timeline
                    .screen_for(step)
                    .or_else(|| timeline.current_screen()),
                step: Some(step),
                pose: pose.unwrap_or_else(|| idle_pose(timeline)),
                caption: Cow::Borrowed(&step.caption),
                is_idle: false,
                is_finished: finished,
                position: None,
            };
        }
    }
    let caption = if timeline.activity == Activity::Acting {
        match timeline.steps.last() {
            Some(step) => Cow::Borrowed(step.caption.as_str()),
            None => timeline.activity.caption(),
        }
    } else {
        timeline.activity.caption()
    };
    Scene {
        screen: timeline
            .current_screen()
            .or_else(|| timeline.steps.last().and_then(|s| timeline.screen_for(s))),
        step: None,
        pose: idle_pose(timeline),
        caption,
        is_idle: !finished,
        is_finished: finished,
        position: None,
    }
}

/// `replay` after the timeline changed ([`ReplayState::sync`]): while following, a new step
/// starts from where the cursor is at `now_ms` on the step shown so far, so a step that
/// arrives mid-animation continues the cursor's path instead of making it jump.
pub fn synced(
    replay: &ReplayState,
    timeline: &ViewerTimeline,
    numbers: &[u32],
    now_ms: f64,
    reduce_motion: bool,
) -> ReplayState {
    let mut next = *replay;
    let mut cursor_at: Option<ViewerPoint> = None;
    if replay.is_following() {
        if let Some(shown) = replay.current() {
            if Some(shown) != timeline.steps.last().map(|s| s.number) {
                if let Some(pose) = cursor::pose_of_step(
                    shown,
                    timeline,
                    replay.elapsed_ms(now_ms),
                    replay.origin(),
                    reduce_motion,
                ) {
                    cursor_at = Some(pose.position);
                }
            }
        }
    }
    next.sync(numbers, now_ms, cursor_at);
    next
}

/// Whether the view must redraw every display frame at `now_ms`: replay is playing, the
/// shown step's animation is still in flight, or the cursor pulses while a run is going
/// (not under Reduce Motion). Otherwise nothing on the screen moves.
pub fn needs_frames(
    timeline: &ViewerTimeline,
    replay: &ReplayState,
    now_ms: f64,
    reduce_motion: bool,
) -> bool {
    if replay.is_playing() {
        return true;
    }
    let scene = make(timeline, replay, now_ms, reduce_motion);
    needs_frames_for(&scene, timeline, replay, now_ms, reduce_motion)
}

/// [`needs_frames`] for a scene already made at `now_ms`.
pub fn needs_frames_for(
    scene: &Scene<'_>,
    timeline: &ViewerTimeline,
    replay: &ReplayState,
    now_ms: f64,
    reduce_motion: bool,
) -> bool {
    if replay.is_playing() {
        return true;
    }
    if let (Some(step), Some(shown)) = (scene.step, replay.current()) {
        if shown == step.number {
            let elapsed = replay.elapsed_ms(now_ms);
            let total = cursor::play_ms_of_step(shown, timeline, replay.origin(), reduce_motion);
            if elapsed < total {
                return true;
            }
        }
    }
    // The pulse of the idle cursor while a run is going.
    let running = !timeline.task_id.is_empty() && !is_finished(timeline);
    running && scene.is_idle && !reduce_motion
}
