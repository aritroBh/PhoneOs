//! `JarvisViewer`: the Live view's logic (`jarvis-viewer`) for Swift.
//!
//! One `jarvis_viewer::LiveView` behind a `Mutex` (calls from any thread are serialized).
//! Designed for few crossings:
//!
//! - **One call per driver update**: `feed(update:)` takes a [`CoreViewerUpdate`], a
//!   one-to-one mirror of the shell's `DriverUpdate` (plus the prompt text) with the FFI's
//!   own `Effect`, `Event` and `SessionProgress`. Privacy decisions (redaction of every
//!   label, the credential-like field test) happen inside, never one call per label. It
//!   returns what changed, so the shell republishes only then.
//! - **One call per drawn frame**: `scene(query:)` takes a small flat record (time, Reduce
//!   Motion, view size) and returns a small flat record ([`CoreLiveScene`]): the screen and
//!   step to show by id/number, the cursor in screen and view points, the caption, the
//!   tone, the swipe trail and the target outline in view points, and whether frames are
//!   still needed. It syncs the scrubber and advances playback itself.
//! - **On demand**: `timeline()` (steps and screen summaries, no wireframes), `screen(id:)`
//!   (one wireframe; screens never change once made, so the shell caches them by id),
//!   `encode()` (the versioned wire bytes for the remote stream), `replay(action:…)`,
//!   `approval_target()` (the approval sheet's picture of the pending confirmation; never
//!   encoded), fetched when `feed` says it changed.
//!
//! Every method runs inside [`guarded`]; a poisoned mutex (a panic that escaped while the
//! lock was held) resets the viewer to an empty one with the same configuration: losing a
//! display timeline is harmless, showing a half-updated one is not worth the risk.
//!
//! The record and enum names carry a `Core` prefix so they never collide with the shell's
//! own Swift `Viewer*` types while both exist.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use jarvis_proto as p;
use jarvis_viewer::{self as v, LiveView, ReducerConfig};

use crate::convert::event_into_proto;
use crate::error::JarvisError;
use crate::guard::guarded;
use crate::runner::snapshot_to_observation;
use crate::types::{
    Action, AppInfo, ConfirmRequest, Effect, Event, OutcomeStatus, Point, Rect, SessionProgress,
    Size,
};

// ---------------------------------------------------------------------------------------
// Records and enums.
// ---------------------------------------------------------------------------------------

/// How a viewer is set up.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CoreViewerConfig {
    /// Steps kept (clamped to 1…200).
    #[uniffi(default = 200)]
    pub max_steps: u32,
    /// The shell keeps real-screen frames for this run: unlocked screens get a frame key.
    #[uniffi(default = false)]
    pub frames_enabled: bool,
    /// Apps named by their display name in "Open …" captions.
    pub known_apps: Vec<AppInfo>,
}

/// One driver update, one-to-one with the shell's `DriverUpdate` (plus the prompt).
#[allow(clippy::large_enum_variant)]
#[derive(Clone, PartialEq, uniffi::Enum)]
pub enum CoreViewerUpdate {
    /// `.started(spec)`: a run began.
    Started { task_id: String },
    /// The user part of a model request (the coordinator's prompt tap).
    Prompt { text: String },
    /// `.effect(effect, number)`.
    Effect { effect: Effect, number: u64 },
    /// `.progress(progress)`.
    Progress { progress: SessionProgress },
    /// `.confirmationPending(request)`.
    ConfirmationPending { request: ConfirmRequest },
    /// `.questionPending`.
    QuestionPending,
    /// `.awaitingUnlock`.
    AwaitingUnlock,
    /// `.effectCompleted(effect, event)`. A runner-text event is parsed with the session's
    /// parser (see `event_for_viewer`); its raw text goes no further.
    EffectCompleted { effect: Effect, event: Event },
    /// `.finished(result)`: the outcome's status.
    Finished { status: OutcomeStatus },
    /// Anything else (`.lockDetected`, `.unlockWaitEnded`, `.ioFailed`, `.note`, replies).
    Ignored,
}

/// What a `feed` changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct CoreViewerChange {
    /// Steps were added, changed or dropped (or a run started).
    pub steps: bool,
    /// A screen was added or dropped.
    pub screens: bool,
    /// The activity changed.
    pub activity: bool,
    /// Bumped by every update that changed something.
    pub generation: u64,
    /// The approval picture was set, replaced or cleared (`approvalTarget()`).
    #[uniffi(default = false)]
    pub approval: bool,
}

/// What the approval sheet marks on the evaluated screen.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum CoreApprovalMark {
    /// The target's frame, clipped to the screen: screen points and unit coordinates
    /// (0…1 of the screen's width and height).
    Outline { frame: Rect, unit: Rect },
    /// A `tap_point`: screen points and unit coordinates.
    Point { point: Point, unit: Point },
    /// Nothing to mark; the caption says why.
    Nothing,
}

/// The approval sheet's picture of the confirmation pending now (`approvalTarget()`).
/// Never part of the timeline: not encoded, not streamed.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreApprovalTarget {
    /// The observation the confirmation was computed from (a real-screen frame is shown
    /// only under exactly this key).
    pub frame_key: CoreViewerFrameKey,
    /// That screen in the timeline (`screen(id:)` gives its wireframe).
    pub screen_id: u32,
    /// Its size in points, as reported.
    pub screen_size: Size,
    /// The confirmation this picture belongs to.
    pub request: ConfirmRequest,
    pub mark: CoreApprovalMark,
    /// One short line under the picture.
    pub caption: String,
    /// For VoiceOver ("Outlined: Button 'Pay $512.40'").
    pub accessibility_label: String,
}

/// How the viewer tints an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreViewerElementKind {
    Control,
    Input,
    Text,
    Image,
    Container,
    Alert,
    Keyboard,
}

/// One wireframe rectangle (and a step's target).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreViewerElement {
    /// Screen points, clipped to the screen.
    pub frame: Rect,
    /// `Button`, `SecureTextField`, …
    pub role: String,
    pub kind: CoreViewerElementKind,
    /// The redacted label, else placeholder; never the value.
    pub label: Option<String>,
    pub secure: bool,
    pub depth: u32,
}

/// Where a screen came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreViewerSource {
    Tree,
    RunnerText,
    Locked,
    /// From a newer sender (its tag): drawn like runner text.
    Unknown {
        tag: String,
    },
}

/// Names a real-screen frame the shell may hold.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct CoreViewerFrameKey {
    pub task_id: String,
    pub observation: u64,
}

/// A screen without its wireframe (in `timeline()`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreViewerScreenInfo {
    pub id: u32,
    /// As reported.
    pub size: Size,
    /// `size` when usable, else the fallback phone (draw with this).
    pub usable_size: Size,
    /// Width / height of `usable_size`, clamped to 0.3…3.
    pub display_aspect: f64,
    pub app_name: Option<String>,
    pub bundle_id: String,
    /// The app name, else the bundle id.
    pub display_name: String,
    pub source: CoreViewerSource,
    /// Elements in the wireframe (`screen(id:)` returns them).
    pub element_count: u32,
    pub elements_truncated: bool,
    pub frame_key: Option<CoreViewerFrameKey>,
    pub focused_input_secure: Option<bool>,
    /// The wireframe came from the runner's text (`source` is `RunnerText`), not from a typed
    /// tree: say "Layout from the runner's text". Additive (preview review).
    #[uniffi(default = false)]
    pub from_text: bool,
}

/// A screen with its wireframe (`screen(id:)`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreViewerScreen {
    pub info: CoreViewerScreenInfo,
    /// Parents before children: draw in order.
    pub elements: Vec<CoreViewerElement>,
}

/// What a step did.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum CoreViewerGesture {
    Tap {
        point: Point,
    },
    LongPress {
        point: Point,
        duration_ms: u64,
    },
    Swipe {
        from: Point,
        to: Point,
        duration_ms: u64,
    },
    /// Typing; `text` is safe to show, `None` when nothing of it is shown.
    TypeText {
        text: Option<String>,
    },
    Key {
        name: String,
    },
    Launch {
        app: String,
    },
    /// The host of an http(s) URL, `<scheme>:` otherwise, empty when neither.
    OpenUrl {
        host: String,
    },
    Wait {
        ms: u64,
    },
    /// From a newer sender (its kind): no cursor movement.
    Unknown {
        kind: String,
    },
}

/// Where a step stands.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CoreViewerStepState {
    Running,
    Done,
    Failed {
        reason: String,
    },
    /// Stopped while in flight: nothing is claimed about it.
    Stopped,
    Unknown {
        tag: String,
    },
}

/// One executed runner command.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreViewerStep {
    pub number: u32,
    pub core_step: Option<u32>,
    pub screen_id: Option<u32>,
    pub gesture: CoreViewerGesture,
    pub target: Option<CoreViewerElement>,
    pub caption: String,
    pub state: CoreViewerStepState,
    /// "Stopped", the failure's reason, or `None`.
    pub state_title: Option<String>,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreViewerOutcome {
    Success,
    Failed,
    Cancelled,
    Blocked,
    Interrupted,
}

/// What Jarvis is doing now.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CoreViewerActivity {
    Idle,
    Looking,
    Thinking,
    Acting,
    UsingTool { name: String },
    WaitingForApproval { what: String },
    WaitingForAnswer,
    Locked,
    Finished { outcome: CoreViewerOutcome },
    Unknown { tag: String },
}

/// The light timeline: steps and screen summaries, no wireframes.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreViewerTimeline {
    pub task_id: String,
    /// Oldest first; numbers are consecutive.
    pub steps: Vec<CoreViewerStep>,
    pub dropped_steps: u32,
    pub screens: Vec<CoreViewerScreenInfo>,
    pub current_screen_id: Option<u32>,
    pub activity: CoreViewerActivity,
    /// The activity's caption ("Thinking…", "Waiting for your OK · Delete").
    pub activity_caption: String,
    pub generation: u64,
}

/// Replay speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreReplaySpeed {
    Normal,
    Double,
}

/// A scrubber control.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum CoreReplayAction {
    StepBack,
    StepForward,
    TogglePlay,
    ScrubTo {
        number: u32,
    },
    /// The scrubber's value, 0…1.
    ScrubFraction {
        fraction: f64,
    },
    SetSpeed {
        speed: CoreReplaySpeed,
    },
    /// 1× ↔ 2×.
    NextSpeed,
    GoLive,
    /// A fresh scrubber following live (a Live view appeared).
    Reset,
}

/// The scrubber's state, with its texts.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreReplayStatus {
    pub current: Option<u32>,
    pub following: bool,
    pub playing: bool,
    pub speed: CoreReplaySpeed,
    /// "1×" / "2×".
    pub speed_title: String,
    /// The scrubber's value, 0…1 (1 while following).
    pub fraction: f64,
    /// "Live · 3 steps", "Step 2 of 3", "No steps yet".
    pub position_text: String,
    pub step_count: u32,
}

/// The per-frame query.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct CoreSceneQuery {
    /// Any monotonic clock, in milliseconds.
    pub now_ms: f64,
    pub reduce_motion: bool,
    /// The view the screen is drawn into, in points.
    pub view_width: f64,
    pub view_height: f64,
    /// The real-screen frame's pixel size when one is drawn (0 when none): the scene says
    /// where it lands, aspect-fitted into the screen's area.
    #[uniffi(default = 0.0)]
    pub image_width: f64,
    #[uniffi(default = 0.0)]
    pub image_height: f64,
}

/// The cursor's phase in a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreCursorPhase {
    Gliding,
    Pressing,
    Swiping,
    Resting,
    Parked,
}

/// The caption dot's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum CoreSceneTone {
    Neutral,
    Acting,
    Waiting,
    Error,
    Success,
}

/// Everything the Live view draws at one moment.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CoreLiveScene {
    /// The backdrop's screen (`screen(id:)`, cached by id).
    pub screen_id: Option<u32>,
    /// The step shown (in `timeline().steps`), `None` while idle.
    pub step_number: Option<u32>,
    /// The cursor in screen points.
    pub cursor: Point,
    pub phase: CoreCursorPhase,
    /// Progress through `phase`, 0…1.
    pub progress: f64,
    /// The finger is down.
    pub pressed: bool,
    pub caption: String,
    /// The caption, "did not work"/"stopped", and the position in replay.
    pub accessibility_value: String,
    /// The cursor idles (pulses).
    pub is_idle: bool,
    pub is_finished: bool,
    /// In replay: 1-based position of the shown step; `None` while following.
    pub position_index: Option<u32>,
    /// Steps kept.
    pub position_count: u32,
    /// Keep redrawing every display frame.
    pub needs_frames: bool,
    pub tone: CoreSceneTone,
    /// The screen size drawn with (usable) and its clamped aspect ratio.
    pub screen_size: Size,
    pub display_aspect: f64,
    /// Screen → view: view points per screen point, and where the screen's origin lands.
    pub scale: f64,
    pub origin: Point,
    /// The screen's area in the view.
    pub content: Rect,
    /// The cursor's tip in view points.
    pub cursor_in_view: Point,
    /// The shown step's target in view points (clipped), when on screen.
    pub target_in_view: Option<Rect>,
    /// A swipe's trail (start, current end) in view points, while it drags or rests.
    pub trail_start: Option<Point>,
    pub trail_end: Option<Point>,
    /// Where the real-screen frame lands in the view (aspect fit into `content`), when
    /// the query gave a usable image size.
    pub image_in_view: Option<Rect>,
    /// The scrubber, numbers only (`replayStatus()` has the texts).
    pub replay_current: Option<u32>,
    pub following: bool,
    pub playing: bool,
    pub fraction: f64,
    pub generation: u64,
}

// ---------------------------------------------------------------------------------------
// Conversions.
// ---------------------------------------------------------------------------------------

fn pt(p: v::ViewerPoint) -> Point {
    Point { x: p.x, y: p.y }
}

fn rect(r: v::ViewerRect) -> Rect {
    Rect {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

fn size(s: v::ViewerSize) -> Size {
    Size {
        width: s.width,
        height: s.height,
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// An action for the viewer: tool arguments are not read, so bad tool JSON never fails.
fn action_for_viewer(a: Action) -> p::Action {
    match a {
        Action::CallTool { name, .. } => p::Action::CallTool {
            name,
            args: serde_json::Value::Null,
        },
        other => p::Action::try_from(other).unwrap_or(p::Action::Look),
    }
}

/// An effect for the viewer (tool arguments are not read).
fn effect_for_viewer(e: Effect) -> p::Effect {
    match e {
        Effect::CallTool { call } => p::Effect::CallTool(p::ToolCall {
            name: call.name,
            args: serde_json::Value::Null,
        }),
        other => p::Effect::try_from(other).unwrap_or(p::Effect::Observe { screenshot: false }),
    }
}

/// An event for the viewer. A runner-text snapshot is parsed exactly as for the session
/// (`snapshot_to_observation`), so a pending confirmation's target is found on the same
/// tree the core indexed; the viewer draws its wireframe like a typed tree's (labels
/// redacted, never values; `CoreViewerScreenInfo.from_text`) and holds it for approval
/// lookups; the raw text goes no further. A locked snapshot is not parsed.
fn event_for_viewer(e: Event) -> p::Event {
    match e {
        Event::ObservedRunnerText { snapshot } if !snapshot.device_locked => {
            p::Event::ObservedRunnerText(snapshot_to_observation(snapshot).0)
        }
        Event::ObservedRunnerText { snapshot } => {
            let size: p::Size = snapshot.size.into();
            p::Event::ObservedRunnerText(p::Observation {
                screen: p::Screen {
                    app_bundle_id: snapshot.app_bundle_id,
                    app_name: snapshot.app_name,
                    size,
                    root: p::UiNode::new(p::Role::Application, size.to_rect()),
                    keyboard_visible: snapshot.keyboard_visible,
                    orientation: snapshot.orientation.into(),
                },
                screenshot: None,
                device_locked: snapshot.device_locked,
            })
        }
        other => event_into_proto(other).0,
    }
}

fn progress_for_viewer(p_: SessionProgress) -> p::Progress {
    let SessionProgress {
        step,
        phase,
        last_action,
        app,
        model_calls,
        parse_retries,
        waiting_for_unlock,
        locked_wait_ms,
        locked_total_ms,
    } = p_;
    p::Progress {
        step,
        phase: phase.into(),
        last_action: last_action.map(action_for_viewer),
        app,
        model_calls,
        parse_retries,
        waiting_for_unlock,
        locked_wait_ms,
        locked_total_ms,
    }
}

impl From<CoreViewerUpdate> for v::Update {
    fn from(u: CoreViewerUpdate) -> Self {
        match u {
            CoreViewerUpdate::Started { task_id } => v::Update::Started { task_id },
            CoreViewerUpdate::Prompt { text } => v::Update::Prompt(text),
            CoreViewerUpdate::Effect { effect, number } => v::Update::Effect {
                effect: effect_for_viewer(effect),
                number,
            },
            CoreViewerUpdate::Progress { progress } => {
                v::Update::Progress(progress_for_viewer(progress))
            }
            CoreViewerUpdate::ConfirmationPending { request } => {
                v::Update::ConfirmationPending(request.into())
            }
            CoreViewerUpdate::QuestionPending => v::Update::QuestionPending,
            CoreViewerUpdate::AwaitingUnlock => v::Update::AwaitingUnlock,
            CoreViewerUpdate::EffectCompleted { effect, event } => v::Update::EffectCompleted {
                effect: effect_for_viewer(effect),
                event: event_for_viewer(event),
            },
            CoreViewerUpdate::Finished { status } => v::Update::Finished(status.into()),
            CoreViewerUpdate::Ignored => v::Update::Ignored,
        }
    }
}

impl From<CoreViewerConfig> for ReducerConfig {
    fn from(c: CoreViewerConfig) -> Self {
        ReducerConfig {
            max_steps: usize::try_from(c.max_steps).unwrap_or(usize::MAX),
            frames_enabled: c.frames_enabled,
            known_apps: c.known_apps.into_iter().map(Into::into).collect(),
        }
    }
}

fn element_kind(k: v::ElementKind) -> CoreViewerElementKind {
    match k {
        v::ElementKind::Control => CoreViewerElementKind::Control,
        v::ElementKind::Input => CoreViewerElementKind::Input,
        v::ElementKind::Text => CoreViewerElementKind::Text,
        v::ElementKind::Image => CoreViewerElementKind::Image,
        v::ElementKind::Container => CoreViewerElementKind::Container,
        v::ElementKind::Alert => CoreViewerElementKind::Alert,
        v::ElementKind::Keyboard => CoreViewerElementKind::Keyboard,
    }
}

fn element(e: &v::ViewerElement) -> CoreViewerElement {
    CoreViewerElement {
        frame: rect(e.frame),
        role: e.role.clone(),
        kind: element_kind(e.kind),
        label: e.label.clone(),
        secure: e.secure,
        depth: e.depth,
    }
}

fn source(s: &v::ScreenSource) -> CoreViewerSource {
    match s {
        v::ScreenSource::Tree => CoreViewerSource::Tree,
        v::ScreenSource::RunnerText => CoreViewerSource::RunnerText,
        v::ScreenSource::Locked => CoreViewerSource::Locked,
        v::ScreenSource::Unknown(tag) => CoreViewerSource::Unknown { tag: tag.clone() },
    }
}

fn screen_info(s: &v::ViewerScreen) -> CoreViewerScreenInfo {
    CoreViewerScreenInfo {
        id: s.id,
        size: size(s.size),
        usable_size: size(s.size.usable()),
        display_aspect: s.size.display_aspect(),
        app_name: s.app_name.clone(),
        bundle_id: s.bundle_id.clone(),
        display_name: s.display_name().to_owned(),
        source: source(&s.source),
        element_count: count(s.elements.len()),
        elements_truncated: s.elements_truncated,
        frame_key: s.frame_key.as_ref().map(|k| CoreViewerFrameKey {
            task_id: k.task_id.clone(),
            observation: k.observation,
        }),
        focused_input_secure: s.focused_input_secure,
        from_text: s.source == v::ScreenSource::RunnerText,
    }
}

fn gesture(g: &v::Gesture) -> CoreViewerGesture {
    match g {
        v::Gesture::Tap(p) => CoreViewerGesture::Tap { point: pt(*p) },
        v::Gesture::LongPress { point, duration_ms } => CoreViewerGesture::LongPress {
            point: pt(*point),
            duration_ms: *duration_ms,
        },
        v::Gesture::Swipe {
            from,
            to,
            duration_ms,
        } => CoreViewerGesture::Swipe {
            from: pt(*from),
            to: pt(*to),
            duration_ms: *duration_ms,
        },
        v::Gesture::Type(text) => CoreViewerGesture::TypeText { text: text.clone() },
        v::Gesture::Key(name) => CoreViewerGesture::Key { name: name.clone() },
        v::Gesture::Launch(app) => CoreViewerGesture::Launch { app: app.clone() },
        v::Gesture::OpenUrl(host) => CoreViewerGesture::OpenUrl { host: host.clone() },
        v::Gesture::Wait(ms) => CoreViewerGesture::Wait { ms: *ms },
        v::Gesture::Unknown(kind) => CoreViewerGesture::Unknown { kind: kind.clone() },
    }
}

fn approval_target(a: &v::ApprovalTarget) -> CoreApprovalTarget {
    CoreApprovalTarget {
        frame_key: CoreViewerFrameKey {
            task_id: a.frame_key.task_id.clone(),
            observation: a.frame_key.observation,
        },
        screen_id: a.screen_id,
        screen_size: size(a.size),
        request: a.request.clone().into(),
        mark: match a.mark {
            v::ApprovalMark::Outline { frame, unit } => CoreApprovalMark::Outline {
                frame: rect(frame),
                unit: rect(unit),
            },
            v::ApprovalMark::Point { point, unit } => CoreApprovalMark::Point {
                point: pt(point),
                unit: pt(unit),
            },
            v::ApprovalMark::Nothing => CoreApprovalMark::Nothing,
        },
        caption: a.caption.clone(),
        accessibility_label: a.accessibility_label.clone(),
    }
}

fn step_state(s: &v::StepState) -> CoreViewerStepState {
    match s {
        v::StepState::Running => CoreViewerStepState::Running,
        v::StepState::Done => CoreViewerStepState::Done,
        v::StepState::Failed(reason) => CoreViewerStepState::Failed {
            reason: reason.clone(),
        },
        v::StepState::Stopped => CoreViewerStepState::Stopped,
        v::StepState::Unknown(tag) => CoreViewerStepState::Unknown { tag: tag.clone() },
    }
}

fn step(s: &v::ViewerStep) -> CoreViewerStep {
    CoreViewerStep {
        number: s.number,
        core_step: s.core_step,
        screen_id: s.screen_id,
        gesture: gesture(&s.gesture),
        target: s.target.as_ref().map(element),
        caption: s.caption.clone(),
        state: step_state(&s.state),
        state_title: s.state.title().map(str::to_owned),
    }
}

fn outcome(o: v::ViewerOutcome) -> CoreViewerOutcome {
    match o {
        v::ViewerOutcome::Success => CoreViewerOutcome::Success,
        v::ViewerOutcome::Failed => CoreViewerOutcome::Failed,
        v::ViewerOutcome::Cancelled => CoreViewerOutcome::Cancelled,
        v::ViewerOutcome::Blocked => CoreViewerOutcome::Blocked,
        v::ViewerOutcome::Interrupted => CoreViewerOutcome::Interrupted,
    }
}

fn activity(a: &v::Activity) -> CoreViewerActivity {
    match a {
        v::Activity::Idle => CoreViewerActivity::Idle,
        v::Activity::Looking => CoreViewerActivity::Looking,
        v::Activity::Thinking => CoreViewerActivity::Thinking,
        v::Activity::Acting => CoreViewerActivity::Acting,
        v::Activity::UsingTool(name) => CoreViewerActivity::UsingTool { name: name.clone() },
        v::Activity::WaitingForApproval(what) => {
            CoreViewerActivity::WaitingForApproval { what: what.clone() }
        }
        v::Activity::WaitingForAnswer => CoreViewerActivity::WaitingForAnswer,
        v::Activity::Locked => CoreViewerActivity::Locked,
        v::Activity::Finished(o) => CoreViewerActivity::Finished {
            outcome: outcome(*o),
        },
        v::Activity::Unknown(tag) => CoreViewerActivity::Unknown { tag: tag.clone() },
    }
}

fn speed(s: v::Speed) -> CoreReplaySpeed {
    match s {
        v::Speed::Normal => CoreReplaySpeed::Normal,
        v::Speed::Double => CoreReplaySpeed::Double,
    }
}

impl From<CoreReplayAction> for v::ReplayAction {
    fn from(a: CoreReplayAction) -> Self {
        match a {
            CoreReplayAction::StepBack => v::ReplayAction::StepBack,
            CoreReplayAction::StepForward => v::ReplayAction::StepForward,
            CoreReplayAction::TogglePlay => v::ReplayAction::TogglePlay,
            CoreReplayAction::ScrubTo { number } => v::ReplayAction::ScrubTo(number),
            CoreReplayAction::ScrubFraction { fraction } => {
                v::ReplayAction::ScrubFraction(fraction)
            }
            CoreReplayAction::SetSpeed { speed } => v::ReplayAction::SetSpeed(match speed {
                CoreReplaySpeed::Normal => v::Speed::Normal,
                CoreReplaySpeed::Double => v::Speed::Double,
            }),
            CoreReplayAction::NextSpeed => v::ReplayAction::NextSpeed,
            CoreReplayAction::GoLive => v::ReplayAction::GoLive,
            CoreReplayAction::Reset => v::ReplayAction::Reset,
        }
    }
}

fn phase(p_: v::Phase) -> CoreCursorPhase {
    match p_ {
        v::Phase::Gliding => CoreCursorPhase::Gliding,
        v::Phase::Pressing => CoreCursorPhase::Pressing,
        v::Phase::Swiping => CoreCursorPhase::Swiping,
        v::Phase::Resting => CoreCursorPhase::Resting,
        v::Phase::Parked => CoreCursorPhase::Parked,
    }
}

fn tone(t: v::Tone) -> CoreSceneTone {
    match t {
        v::Tone::Neutral => CoreSceneTone::Neutral,
        v::Tone::Acting => CoreSceneTone::Acting,
        v::Tone::Waiting => CoreSceneTone::Waiting,
        v::Tone::Error => CoreSceneTone::Error,
        v::Tone::Success => CoreSceneTone::Success,
    }
}

fn replay_status(live: &LiveView) -> CoreReplayStatus {
    let r = live.replay();
    CoreReplayStatus {
        current: r.current(),
        following: r.is_following(),
        playing: r.is_playing(),
        speed: speed(r.speed()),
        speed_title: r.speed().title().to_owned(),
        fraction: r.fraction(live.numbers()),
        position_text: live.position_text(),
        step_count: count(live.numbers().len()),
    }
}

// ---------------------------------------------------------------------------------------
// The object.
// ---------------------------------------------------------------------------------------

pub(crate) struct Inner {
    pub(crate) live: LiveView,
    config: ReducerConfig,
}

/// Viewers made so far in this process: each gets its own number, so a remote stream
/// (`feedAndStream`) tells a new viewer's run from a continued one (see `live_remote.rs`).
static VIEWERS_MADE: AtomicU64 = AtomicU64::new(0);

/// The Live view of one run. Swift: `JarvisViewer`.
#[derive(uniffi::Object)]
pub struct JarvisViewer {
    inner: Mutex<Inner>,
    /// This viewer's number (`VIEWERS_MADE`).
    pub(crate) instance: u64,
}

fn next_instance() -> u64 {
    VIEWERS_MADE.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
}

impl JarvisViewer {
    /// A viewer with proto configuration (shared by the FFI constructor and Rust tests).
    pub fn with_config(config: ReducerConfig) -> Self {
        JarvisViewer {
            inner: Mutex::new(Inner {
                live: LiveView::new(config.clone()),
                config,
            }),
            instance: next_instance(),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => {
                let mut g = poison.into_inner();
                g.live = LiveView::new(g.config.clone());
                self.inner.clear_poison();
                g
            }
        }
    }

    /// Runs `f` with the viewer locked, inside [`guarded`].
    pub(crate) fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&mut LiveView) -> Result<T, JarvisError>,
    ) -> Result<T, JarvisError> {
        guarded(what, || {
            let mut inner = self.lock();
            f(&mut inner.live)
        })
    }

    /// The timeline with the crate's own types (tests, tools).
    pub fn timeline_proto(&self) -> v::ViewerTimeline {
        self.lock().live.timeline().clone()
    }
}

#[uniffi::export]
impl JarvisViewer {
    /// An empty viewer (a run's updates follow, starting with `.started`).
    #[uniffi::constructor]
    pub fn new(config: CoreViewerConfig) -> Result<Arc<Self>, JarvisError> {
        guarded("JarvisViewer.init", || {
            Ok(Arc::new(Self::with_config(config.into())))
        })
    }

    /// A viewer showing a timeline received as bytes (`encode()` of another viewer, e.g.
    /// on a paired remote controller). Throws `JarvisError.invalidJson` for bytes that are
    /// not a timeline, too large, or from a newer incompatible version (no new error case,
    /// so the shell's exhaustive switches over `JarvisError` keep compiling).
    #[uniffi::constructor]
    pub fn decoded(bytes: Vec<u8>, config: CoreViewerConfig) -> Result<Arc<Self>, JarvisError> {
        guarded("JarvisViewer.decoded", || {
            let timeline =
                v::ViewerTimeline::decode(&bytes).map_err(|e| JarvisError::InvalidJson {
                    field: "JarvisViewer.decoded(bytes:)".to_owned(),
                    message: e.to_string(),
                })?;
            let config: ReducerConfig = config.into();
            Ok(Arc::new(JarvisViewer {
                inner: Mutex::new(Inner {
                    live: LiveView::from_timeline(timeline, config.clone()),
                    config,
                }),
                instance: next_instance(),
            }))
        })
    }

    /// Feeds one driver update. O(1) crossings per update: the privacy rules run inside.
    pub fn feed(&self, update: CoreViewerUpdate) -> Result<CoreViewerChange, JarvisError> {
        self.with("JarvisViewer.feed", |live| {
            let change = live.apply(update.into());
            Ok(CoreViewerChange {
                steps: change.steps,
                screens: change.screens,
                activity: change.activity,
                generation: live.generation(),
                approval: change.approval,
            })
        })
    }

    /// Bumped by every update that changed something (cheap; compare before fetching).
    pub fn generation(&self) -> Result<u64, JarvisError> {
        self.with("JarvisViewer.generation", |live| Ok(live.generation()))
    }

    /// Steps, screen summaries (no wireframes) and the activity.
    pub fn timeline(&self) -> Result<CoreViewerTimeline, JarvisError> {
        self.with("JarvisViewer.timeline", |live| {
            let t = live.timeline();
            Ok(CoreViewerTimeline {
                task_id: t.task_id.clone(),
                steps: t.steps.iter().map(step).collect(),
                dropped_steps: t.dropped_steps,
                screens: t.screens.iter().map(screen_info).collect(),
                current_screen_id: t.current_screen_id,
                activity: activity(&t.activity),
                activity_caption: t.activity.caption().into_owned(),
                generation: live.generation(),
            })
        })
    }

    /// One screen with its wireframe; `None` once no kept step refers to it. A screen never
    /// changes once made, so the shell may cache it by id.
    pub fn screen(&self, id: u32) -> Result<Option<CoreViewerScreen>, JarvisError> {
        self.with("JarvisViewer.screen", |live| {
            Ok(live.timeline().screen(Some(id)).map(|s| CoreViewerScreen {
                info: screen_info(s),
                elements: s.elements.iter().map(element).collect(),
            }))
        })
    }

    /// The approval sheet's picture of the confirmation pending now: the evaluated
    /// observation's key and screen, and the action's target on it. `None` when no
    /// confirmation is pending or no unlocked screen was observed. Never encoded.
    pub fn approval_target(&self) -> Result<Option<CoreApprovalTarget>, JarvisError> {
        self.with("JarvisViewer.approval_target", |live| {
            Ok(live.reducer().approval().map(approval_target))
        })
    }

    /// The timeline's wire form (versioned JSON, see `jarvis_viewer::wire`), for the
    /// remote stream.
    pub fn encode(&self) -> Result<Vec<u8>, JarvisError> {
        self.with("JarvisViewer.encode", |live| Ok(live.timeline().encode()))
    }

    /// A scrubber control at `now_ms`; returns the scrubber's state.
    pub fn replay(
        &self,
        action: CoreReplayAction,
        now_ms: f64,
        reduce_motion: bool,
    ) -> Result<CoreReplayStatus, JarvisError> {
        self.with("JarvisViewer.replay", |live| {
            live.control(action.into(), now_ms, reduce_motion);
            Ok(replay_status(live))
        })
    }

    /// The scrubber's state with its texts.
    pub fn replay_status(&self) -> Result<CoreReplayStatus, JarvisError> {
        self.with("JarvisViewer.replay_status", |live| Ok(replay_status(live)))
    }

    /// What to draw at `query.nowMs` (the view's one call per frame). Syncs the scrubber
    /// with the timeline and advances playback first.
    pub fn scene(&self, query: CoreSceneQuery) -> Result<CoreLiveScene, JarvisError> {
        self.with("JarvisViewer.scene", |live| {
            let generation = live.generation();
            let step_count = count(live.numbers().len());
            let frame = live.frame(query.now_ms, query.reduce_motion);
            let geometry = frame.in_view(v::ViewerSize::new(query.view_width, query.view_height));
            let s = &frame.scene;
            let screen_size = frame.screen_size();
            let scene = CoreLiveScene {
                screen_id: s.screen.map(|x| x.id),
                step_number: s.step.map(|x| x.number),
                cursor: pt(s.pose.position),
                phase: phase(s.pose.phase),
                progress: s.pose.progress,
                pressed: s.pose.is_pressed(),
                caption: s.caption.to_string(),
                accessibility_value: s.accessibility_value(),
                is_idle: s.is_idle,
                is_finished: s.is_finished,
                position_index: s.position.map(|(i, _)| count(i)),
                position_count: step_count,
                needs_frames: frame.needs_frames,
                tone: tone(frame.tone),
                screen_size: size(screen_size),
                display_aspect: screen_size.display_aspect(),
                scale: geometry.mapping.scale,
                origin: pt(geometry.mapping.origin),
                content: rect(geometry.mapping.content_rect()),
                cursor_in_view: pt(geometry.cursor),
                target_in_view: geometry.target.map(rect),
                trail_start: geometry.trail.map(|(a, _)| pt(a)),
                trail_end: geometry.trail.map(|(_, b)| pt(b)),
                image_in_view: {
                    let image = v::ViewerSize::new(query.image_width, query.image_height);
                    image
                        .is_usable()
                        .then(|| rect(geometry.mapping.content_rect().aspect_fit(image)))
                },
                replay_current: None,
                following: false,
                playing: false,
                fraction: 1.0,
                generation,
            };
            let r = *live.replay();
            Ok(CoreLiveScene {
                replay_current: r.current(),
                following: r.is_following(),
                playing: r.is_playing(),
                fraction: r.fraction(live.numbers()),
                ..scene
            })
        })
    }
}

/// The Live view's wire format version (`JarvisViewer.encode()`).
#[uniffi::export]
pub fn viewer_wire_version() -> u32 {
    v::WIRE_VERSION
}
