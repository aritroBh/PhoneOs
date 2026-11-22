//! The Live view's data and its wire format: what Jarvis did, step by step, on which
//! screen. Plain values with explicit, stable JSON keys (snake_case, enums tagged by
//! `"kind"`), so the timeline is streamed as is to the paired remote controller.
//!
//! Nothing in here is raw: screen text went through the core's redaction, typed text is
//! shown only when the privacy rules allow it ([`crate::privacy`]), links are hosts or
//! schemes only and element values are never kept.
//!
//! # Wire format (version [`WIRE_VERSION`])
//!
//! The JSON shape is the one the Swift `ViewerTimeline` wrote, plus a top-level
//! `"version": 1`. A document without `version` (written by the Swift shell) reads as
//! version 1; a higher version is refused ([`WireError::Version`]): the number only changes
//! for an incompatible change, additions keep it.
//!
//! ```text
//! {"version":1,"task_id":"…","dropped_steps":0,"current_screen_id":3,
//!  "activity":{"kind":"thinking"},
//!  "steps":[{"number":1,"core_step":1,"screen_id":1,
//!            "gesture":{"kind":"tap","point":{"x":100.0,"y":420.0}},
//!            "target":{"frame":{…},"role":"Button","kind":"control","label":"Send","secure":false,"depth":1},
//!            "caption":"Tap “Send”","state":{"kind":"done"}}],
//!  "screens":[{"id":1,"size":{"width":393.0,"height":852.0},"app_name":"Mail",
//!              "bundle_id":"com.apple.mobilemail","source":"tree","elements":[…],
//!              "elements_truncated":false,"frame_key":{"task_id":"…","observation":1},
//!              "focused_input_secure":false}]}
//! ```
//!
//! Optional values are left out when absent. Forward compatible: a gesture, step state,
//! activity or screen source this reader does not know decodes as `Unknown(<its tag>)` and
//! re-encodes with the same tag; an element kind it does not know reads as `container`; a
//! gesture without a `kind` is an error. A non-finite coordinate is written as `null` and
//! reads back as NaN (the Swift encoder refused such a timeline).
//!
//! Bounds, enforced on decode: at most [`MAX_STEPS`] steps, [`MAX_SCREENS`] screens,
//! [`MAX_ELEMENTS`] elements per screen and [`MAX_WIRE_TEXT`] characters per string.

use std::borrow::Cow;

use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::geometry::{ViewerPoint, ViewerRect, ViewerSize};

/// The wire format version this crate writes and the highest it reads.
pub const WIRE_VERSION: u32 = 1;
/// Steps kept at most (the reducer's cap is clamped to this).
pub const MAX_STEPS: usize = 200;
/// Screens a timeline holds at most: one per kept step plus the current one.
pub const MAX_SCREENS: usize = MAX_STEPS + 1;
/// Wireframe rectangles kept per screen.
pub const MAX_ELEMENTS: usize = 400;
/// Longest string accepted on decode, in characters (what the reducer writes is far
/// shorter: labels 40, typed text 32, roles 32, bundle ids 128).
pub const MAX_WIRE_TEXT: usize = 256;

/// Why bytes could not be read as a timeline.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    /// Not JSON of the expected shape (the message says where).
    #[error("malformed timeline: {0}")]
    Malformed(String),
    /// Written by a newer, incompatible sender.
    #[error("timeline version {0} is newer than {WIRE_VERSION}")]
    Version(u32),
    /// Over one of the bounds.
    #[error("timeline too large: {0}")]
    TooLarge(&'static str),
}

/// How the viewer tints an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementKind {
    /// Buttons, links, cells, switches, sliders, pickers, tabs, keys.
    Control,
    /// Text fields, secure fields, text views, search fields.
    Input,
    /// Static text.
    Text,
    /// Images.
    Image,
    /// Bars, tables, scroll and web views and other containers.
    Container,
    /// Alerts and sheets.
    Alert,
    /// The keyboard.
    Keyboard,
}

impl ElementKind {
    /// The coding tag.
    pub fn tag(self) -> &'static str {
        match self {
            ElementKind::Control => "control",
            ElementKind::Input => "input",
            ElementKind::Text => "text",
            ElementKind::Image => "image",
            ElementKind::Container => "container",
            ElementKind::Alert => "alert",
            ElementKind::Keyboard => "keyboard",
        }
    }

    /// Reads a tag; unknown values (from a newer sender) read as `Container`, the quietest
    /// tint.
    pub fn from_tag(tag: &str) -> ElementKind {
        match tag {
            "control" => ElementKind::Control,
            "input" => ElementKind::Input,
            "text" => ElementKind::Text,
            "image" => ElementKind::Image,
            "alert" => ElementKind::Alert,
            "keyboard" => ElementKind::Keyboard,
            _ => ElementKind::Container,
        }
    }
}

impl Serialize for ElementKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.tag())
    }
}

impl<'de> Deserialize<'de> for ElementKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let tag = Cow::<str>::deserialize(d)?;
        Ok(ElementKind::from_tag(&tag))
    }
}

/// One rectangle of the wireframe backdrop (and the target of a gesture).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewerElement {
    /// Frame in screen points, clipped to the screen.
    pub frame: ViewerRect,
    /// The role as the core names it (`Button`, `SecureTextField`, …).
    pub role: String,
    /// The tint.
    pub kind: ElementKind,
    /// The accessibility label, else the placeholder, redacted; never the element's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// A secure (password) field.
    pub secure: bool,
    /// Depth in the tree (0 = top level shown); deeper elements draw on top.
    pub depth: u32,
}

/// Where a screen came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScreenSource {
    /// A typed tree: the wireframe is available.
    Tree,
    /// The runner's text format: no frames, so no wireframe (cursor and caption only).
    RunnerText,
    /// The phone was locked: nothing of the screen is kept.
    Locked,
    /// A source from a newer sender (its tag); drawn like `RunnerText`.
    Unknown(String),
}

impl ScreenSource {
    /// The coding tag.
    pub fn tag(&self) -> &str {
        match self {
            ScreenSource::Tree => "tree",
            ScreenSource::RunnerText => "runner_text",
            ScreenSource::Locked => "locked",
            ScreenSource::Unknown(tag) => tag,
        }
    }

    /// Reads a tag.
    pub fn from_tag(tag: &str) -> ScreenSource {
        match tag {
            "tree" => ScreenSource::Tree,
            "runner_text" => ScreenSource::RunnerText,
            "locked" => ScreenSource::Locked,
            other => ScreenSource::Unknown(other.to_owned()),
        }
    }
}

impl Serialize for ScreenSource {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.tag())
    }
}

impl<'de> Deserialize<'de> for ScreenSource {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let tag = Cow::<str>::deserialize(d)?;
        Ok(ScreenSource::from_tag(&tag))
    }
}

/// Names one observation of one run: the run's task id and the number of the driver
/// effect that observed. The shell stores a real-screen frame under it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameKey {
    /// The run's task id.
    pub task_id: String,
    /// The number of the effect that observed.
    pub observation: u64,
}

/// One observed screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewerScreen {
    /// Unique within a timeline (1, 2, 3… in observation order).
    pub id: u32,
    /// Screen size in points as reported (draw with [`ViewerSize::usable`]).
    pub size: ViewerSize,
    /// The app's name, redacted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    /// The app's bundle id (empty for a locked screen).
    pub bundle_id: String,
    /// Where it came from.
    pub source: ScreenSource,
    /// The wireframe, parents before children (draw in order). Empty for runner text and
    /// locked screens.
    pub elements: Vec<ViewerElement>,
    /// More elements were shown than [`MAX_ELEMENTS`].
    pub elements_truncated: bool,
    /// Where a real screenshot of this observation may be; `None` when the option was off
    /// or the phone was locked. The frame may have been evicted since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_key: Option<FrameKey>,
    /// Whether the focused element is a secure field (`None`: no focused input, or the tree
    /// cannot tell).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_input_secure: Option<bool>,
}

impl ViewerScreen {
    /// The name to show: the app name, else the bundle id.
    pub fn display_name(&self) -> &str {
        match &self.app_name {
            Some(name) if !name.is_empty() => name,
            _ => &self.bundle_id,
        }
    }
}

/// What a step did, in display form.
#[derive(Debug, Clone, PartialEq)]
pub enum Gesture {
    /// A tap.
    Tap(ViewerPoint),
    /// A press and hold.
    LongPress {
        /// Where.
        point: ViewerPoint,
        /// How long.
        duration_ms: u64,
    },
    /// A drag.
    Swipe {
        /// Start.
        from: ViewerPoint,
        /// End.
        to: ViewerPoint,
        /// Duration.
        duration_ms: u64,
    },
    /// Typing; the text is already safe to show (`••••` for masked parts), `None` when it
    /// is not shown at all.
    Type(Option<String>),
    /// A key press (`Return`, `Delete`, …).
    Key(String),
    /// Opening an app (its name, else its bundle id).
    Launch(String),
    /// Opening a URL: the host of an `http`/`https` URL, only `<scheme>:` for any other
    /// scheme, empty when there is neither.
    OpenUrl(String),
    /// A wait.
    Wait(u64),
    /// A gesture from a newer sender (its kind): no cursor movement, a parked step.
    Unknown(String),
}

impl Gesture {
    /// Where the pointer goes down, for pointer gestures.
    pub fn press_point(&self) -> Option<ViewerPoint> {
        match self {
            Gesture::Tap(p) | Gesture::LongPress { point: p, .. } => Some(*p),
            Gesture::Swipe { from, .. } => Some(*from),
            _ => None,
        }
    }

    /// Where the pointer ends up, for pointer gestures.
    pub fn end_point(&self) -> Option<ViewerPoint> {
        match self {
            Gesture::Tap(p) | Gesture::LongPress { point: p, .. } => Some(*p),
            Gesture::Swipe { to, .. } => Some(*to),
            _ => None,
        }
    }

    /// Whether the cursor moves for this gesture.
    pub fn is_pointer(&self) -> bool {
        self.press_point().is_some()
    }

    /// A short stable name (also the coding tag).
    pub fn kind(&self) -> &str {
        match self {
            Gesture::Tap(_) => "tap",
            Gesture::LongPress { .. } => "long_press",
            Gesture::Swipe { .. } => "swipe",
            Gesture::Type(_) => "type",
            Gesture::Key(_) => "key",
            Gesture::Launch(_) => "launch",
            Gesture::OpenUrl(_) => "open_url",
            Gesture::Wait(_) => "wait",
            Gesture::Unknown(kind) => kind,
        }
    }
}

impl Serialize for Gesture {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("kind", self.kind())?;
        match self {
            Gesture::Tap(p) => m.serialize_entry("point", p)?,
            Gesture::LongPress { point, duration_ms } => {
                m.serialize_entry("point", point)?;
                m.serialize_entry("duration_ms", duration_ms)?;
            }
            Gesture::Swipe {
                from,
                to,
                duration_ms,
            } => {
                m.serialize_entry("from", from)?;
                m.serialize_entry("to", to)?;
                m.serialize_entry("duration_ms", duration_ms)?;
            }
            Gesture::Type(text) => {
                if let Some(text) = text {
                    m.serialize_entry("text", text)?;
                }
            }
            Gesture::Key(key) => m.serialize_entry("key", key)?,
            Gesture::Launch(app) => m.serialize_entry("app", app)?,
            Gesture::OpenUrl(host) => m.serialize_entry("host", host)?,
            Gesture::Wait(ms) => m.serialize_entry("ms", ms)?,
            Gesture::Unknown(_) => {}
        }
        m.end()
    }
}

#[derive(Deserialize)]
struct GestureWire {
    kind: String,
    point: Option<ViewerPoint>,
    from: Option<ViewerPoint>,
    to: Option<ViewerPoint>,
    duration_ms: Option<u64>,
    text: Option<String>,
    key: Option<String>,
    app: Option<String>,
    host: Option<String>,
    ms: Option<u64>,
}

fn need<T, E: serde::de::Error>(v: Option<T>, field: &'static str) -> Result<T, E> {
    v.ok_or_else(|| E::missing_field(field))
}

impl<'de> Deserialize<'de> for Gesture {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = GestureWire::deserialize(d)?;
        Ok(match w.kind.as_str() {
            "tap" => Gesture::Tap(need(w.point, "point")?),
            "long_press" => Gesture::LongPress {
                point: need(w.point, "point")?,
                duration_ms: need(w.duration_ms, "duration_ms")?,
            },
            "swipe" => Gesture::Swipe {
                from: need(w.from, "from")?,
                to: need(w.to, "to")?,
                duration_ms: need(w.duration_ms, "duration_ms")?,
            },
            "type" => Gesture::Type(w.text),
            "key" => Gesture::Key(need(w.key, "key")?),
            "launch" => Gesture::Launch(need(w.app, "app")?),
            "open_url" => Gesture::OpenUrl(need(w.host, "host")?),
            "wait" => Gesture::Wait(need(w.ms, "ms")?),
            _ => Gesture::Unknown(w.kind),
        })
    }
}

/// Where a step stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepState {
    /// The command is in flight.
    Running,
    /// The phone did it.
    Done,
    /// It did not work (a fixed, owner-facing reason; never the runner's words).
    Failed(String),
    /// The run was stopped while the command was in flight: whether the phone did it is
    /// not known, so nothing is claimed ("Stopped").
    Stopped,
    /// A state from a newer sender (its tag): neither running nor failed.
    Unknown(String),
}

impl StepState {
    /// Whether it failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, StepState::Failed(_))
    }

    /// A short word for the state ("Stopped" for a step stopped mid-command), `None` when
    /// there is nothing to say (running, done, unknown).
    pub fn title(&self) -> Option<&str> {
        match self {
            StepState::Running | StepState::Done | StepState::Unknown(_) => None,
            StepState::Failed(why) => Some(why),
            StepState::Stopped => Some("Stopped"),
        }
    }

    /// The coding tag.
    pub fn tag(&self) -> &str {
        match self {
            StepState::Running => "running",
            StepState::Done => "done",
            StepState::Failed(_) => "failed",
            StepState::Stopped => "stopped",
            StepState::Unknown(tag) => tag,
        }
    }
}

impl Serialize for StepState {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("kind", self.tag())?;
        if let StepState::Failed(error) = self {
            m.serialize_entry("error", error)?;
        }
        m.end()
    }
}

#[derive(Deserialize)]
struct StateWire {
    kind: String,
    error: Option<String>,
}

impl<'de> Deserialize<'de> for StepState {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = StateWire::deserialize(d)?;
        Ok(match w.kind.as_str() {
            "running" => StepState::Running,
            "done" => StepState::Done,
            "stopped" => StepState::Stopped,
            "failed" => StepState::Failed(w.error.unwrap_or_else(|| "failed".to_owned())),
            _ => StepState::Unknown(w.kind),
        })
    }
}

/// One executed runner command, as the viewer shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewerStep {
    /// 1, 2, 3… in the order the commands were sent; never reused after the cap drops old
    /// steps.
    pub number: u32,
    /// The core's step counter when the command was sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_step: Option<u32>,
    /// The screen it acted on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_id: Option<u32>,
    /// What it did.
    pub gesture: Gesture,
    /// The element under the pointer, for display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ViewerElement>,
    /// "Tap “Send”", "Swipe up", "Type “hello”".
    pub caption: String,
    /// Where it stands.
    pub state: StepState,
}

/// How a run ended, for the viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerOutcome {
    /// Done.
    Success,
    /// Failed.
    Failed,
    /// Stopped by the owner.
    Cancelled,
    /// Stopped by the safety policy.
    Blocked,
    /// The phone stayed locked.
    Interrupted,
}

impl ViewerOutcome {
    /// The caption.
    pub fn title(self) -> &'static str {
        match self {
            ViewerOutcome::Success => "Done",
            ViewerOutcome::Failed => "Failed",
            ViewerOutcome::Cancelled => "Stopped",
            ViewerOutcome::Blocked => "Blocked",
            ViewerOutcome::Interrupted => "Phone stayed locked",
        }
    }
}

impl From<jarvis_proto::OutcomeStatus> for ViewerOutcome {
    fn from(s: jarvis_proto::OutcomeStatus) -> Self {
        use jarvis_proto::OutcomeStatus as S;
        match s {
            S::Success => ViewerOutcome::Success,
            S::Failed => ViewerOutcome::Failed,
            S::Cancelled => ViewerOutcome::Cancelled,
            S::Blocked => ViewerOutcome::Blocked,
            S::Interrupted => ViewerOutcome::Interrupted,
        }
    }
}

/// What Jarvis is doing right now (the caption pill while following live).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// No run yet.
    Idle,
    /// Observing.
    Looking,
    /// The model is thinking.
    Thinking,
    /// A runner command is in flight.
    Acting,
    /// A tool runs (its name, redacted).
    UsingTool(String),
    /// A confirmation waits for the owner; the text names the kind of action.
    WaitingForApproval(String),
    /// A question waits for the owner.
    WaitingForAnswer,
    /// The phone is locked.
    Locked,
    /// The run is over.
    Finished(ViewerOutcome),
    /// An activity from a newer sender (its tag, or `finished` with an outcome this reader
    /// does not know): a neutral caption, the cursor idles.
    Unknown(String),
}

impl Activity {
    /// The pill text (`Acting` shows the step's caption instead).
    pub fn caption(&self) -> Cow<'_, str> {
        match self {
            Activity::Idle => Cow::Borrowed("Standing by"),
            Activity::Looking => Cow::Borrowed("Looking at the screen…"),
            Activity::Thinking => Cow::Borrowed("Thinking…"),
            Activity::Acting => Cow::Borrowed("Acting…"),
            Activity::UsingTool(name) => Cow::Owned(format!("Using {name}")),
            Activity::WaitingForApproval(what) if what.is_empty() => {
                Cow::Borrowed("Waiting for your OK")
            }
            Activity::WaitingForApproval(what) => {
                Cow::Owned(format!("Waiting for your OK · {what}"))
            }
            Activity::WaitingForAnswer => Cow::Borrowed("Waiting for your answer"),
            Activity::Locked => Cow::Borrowed("Unlock to continue"),
            Activity::Finished(outcome) => Cow::Borrowed(outcome.title()),
            Activity::Unknown(_) => Cow::Borrowed("Working…"),
        }
    }

    /// The cursor idles (pulses) instead of acting.
    pub fn is_idle(&self) -> bool {
        !matches!(self, Activity::Acting)
    }

    /// The coding tag.
    pub fn tag(&self) -> &str {
        match self {
            Activity::Idle => "idle",
            Activity::Looking => "looking",
            Activity::Thinking => "thinking",
            Activity::Acting => "acting",
            Activity::UsingTool(_) => "using_tool",
            Activity::WaitingForApproval(_) => "waiting_for_approval",
            Activity::WaitingForAnswer => "waiting_for_answer",
            Activity::Locked => "locked",
            Activity::Finished(_) => "finished",
            Activity::Unknown(tag) => tag,
        }
    }
}

impl Serialize for Activity {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("kind", self.tag())?;
        match self {
            Activity::UsingTool(text) | Activity::WaitingForApproval(text) => {
                m.serialize_entry("text", text)?;
            }
            Activity::Finished(outcome) => m.serialize_entry("outcome", outcome)?,
            _ => {}
        }
        m.end()
    }
}

#[derive(Deserialize)]
struct ActivityWire {
    kind: String,
    text: Option<String>,
    outcome: Option<serde_json::Value>,
}

impl<'de> Deserialize<'de> for Activity {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = ActivityWire::deserialize(d)?;
        Ok(match w.kind.as_str() {
            "idle" => Activity::Idle,
            "looking" => Activity::Looking,
            "thinking" => Activity::Thinking,
            "acting" => Activity::Acting,
            "using_tool" => Activity::UsingTool(need(w.text, "text")?),
            "waiting_for_approval" => Activity::WaitingForApproval(need(w.text, "text")?),
            "waiting_for_answer" => Activity::WaitingForAnswer,
            "locked" => Activity::Locked,
            "finished" => {
                // An outcome this reader does not know reads as an unknown activity.
                match w
                    .outcome
                    .and_then(|v| serde_json::from_value::<ViewerOutcome>(v).ok())
                {
                    Some(outcome) => Activity::Finished(outcome),
                    None => Activity::Unknown("finished".to_owned()),
                }
            }
            _ => Activity::Unknown(w.kind),
        })
    }
}

/// The run as the Live view shows it: steps (newest last, capped), the screens they acted
/// on, the latest screen and what Jarvis is doing now.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewerTimeline {
    /// The run's task id, empty before a run starts.
    pub task_id: String,
    /// Steps, oldest first; at most [`MAX_STEPS`]. Numbers are consecutive.
    pub steps: Vec<ViewerStep>,
    /// Steps dropped from the front by the cap.
    pub dropped_steps: u32,
    /// The screens the kept steps refer to, plus the latest one, in id order.
    pub screens: Vec<ViewerScreen>,
    /// The latest observed screen.
    pub current_screen_id: Option<u32>,
    /// What Jarvis is doing.
    pub activity: Activity,
}

impl Default for ViewerTimeline {
    fn default() -> Self {
        ViewerTimeline {
            task_id: String::new(),
            steps: Vec::new(),
            dropped_steps: 0,
            screens: Vec::new(),
            current_screen_id: None,
            activity: Activity::Idle,
        }
    }
}

impl ViewerTimeline {
    /// The screen with `id`.
    pub fn screen(&self, id: Option<u32>) -> Option<&ViewerScreen> {
        let id = id?;
        // Screens are kept in id order.
        match self.screens.binary_search_by_key(&id, |s| s.id) {
            Ok(i) => self.screens.get(i),
            Err(_) => self.screens.iter().find(|s| s.id == id),
        }
    }

    /// The screen `step` acted on.
    pub fn screen_for(&self, step: &ViewerStep) -> Option<&ViewerScreen> {
        self.screen(step.screen_id)
    }

    /// The latest observed screen.
    pub fn current_screen(&self) -> Option<&ViewerScreen> {
        self.screen(self.current_screen_id)
    }

    /// The index of the step numbered `number` in `steps`.
    pub fn step_index(&self, number: u32) -> Option<usize> {
        let first = self.steps.first()?.number;
        // Numbers are consecutive, so this is the index; checked anyway.
        if let Some(i) = number.checked_sub(first).map(|d| d as usize) {
            if self.steps.get(i).is_some_and(|s| s.number == number) {
                return Some(i);
            }
        }
        self.steps.iter().position(|s| s.number == number)
    }

    /// The step numbered `number`.
    pub fn step(&self, number: Option<u32>) -> Option<&ViewerStep> {
        self.steps.get(self.step_index(number?)?)
    }

    /// The step before the step numbered `number`, `None` for the first kept step.
    pub fn step_before(&self, number: u32) -> Option<&ViewerStep> {
        let i = self.step_index(number)?;
        self.steps.get(i.checked_sub(1)?)
    }

    /// The step numbers kept, oldest first.
    pub fn step_numbers(&self) -> Vec<u32> {
        self.steps.iter().map(|s| s.number).collect()
    }

    /// The wire form (JSON, version [`WIRE_VERSION`]).
    pub fn encode(&self) -> Vec<u8> {
        // Serializing plain data into a Vec cannot fail (map keys are strings).
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// Reads the wire form (any version up to [`WIRE_VERSION`], bounds checked).
    pub fn decode(bytes: &[u8]) -> Result<ViewerTimeline, WireError> {
        let wire: TimelineWire =
            serde_json::from_slice(bytes).map_err(|e| WireError::Malformed(e.to_string()))?;
        wire.into_timeline()
    }

    /// Checks the bounds a decoded timeline must hold.
    pub fn check_bounds(&self) -> Result<(), WireError> {
        if self.steps.len() > MAX_STEPS {
            return Err(WireError::TooLarge("steps"));
        }
        if self.screens.len() > MAX_SCREENS {
            return Err(WireError::TooLarge("screens"));
        }
        let text_ok = |s: &str| s.len() <= MAX_WIRE_TEXT || s.chars().count() <= MAX_WIRE_TEXT;
        let opt_ok = |s: &Option<String>| s.as_deref().is_none_or(text_ok);
        let element_ok = |e: &ViewerElement| text_ok(&e.role) && opt_ok(&e.label);
        if !text_ok(&self.task_id) || !text_ok(self.activity.tag()) {
            return Err(WireError::TooLarge("text"));
        }
        if let Activity::UsingTool(t) | Activity::WaitingForApproval(t) = &self.activity {
            if !text_ok(t) {
                return Err(WireError::TooLarge("text"));
            }
        }
        for step in &self.steps {
            let gesture_ok = match &step.gesture {
                Gesture::Type(t) => opt_ok(t),
                Gesture::Key(t)
                | Gesture::Launch(t)
                | Gesture::OpenUrl(t)
                | Gesture::Unknown(t) => text_ok(t),
                _ => true,
            };
            if !gesture_ok
                || !text_ok(&step.caption)
                || !text_ok(step.state.tag())
                || !step.state.title().is_none_or(text_ok)
                || !step.target.as_ref().is_none_or(element_ok)
            {
                return Err(WireError::TooLarge("text"));
            }
        }
        for screen in &self.screens {
            if screen.elements.len() > MAX_ELEMENTS {
                return Err(WireError::TooLarge("elements"));
            }
            if !opt_ok(&screen.app_name)
                || !text_ok(&screen.bundle_id)
                || !text_ok(screen.source.tag())
                || !screen
                    .frame_key
                    .as_ref()
                    .is_none_or(|k| text_ok(&k.task_id))
                || !screen.elements.iter().all(element_ok)
            {
                return Err(WireError::TooLarge("text"));
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct TimelineWireRef<'a> {
    version: u32,
    task_id: &'a str,
    steps: &'a [ViewerStep],
    dropped_steps: u32,
    screens: &'a [ViewerScreen],
    #[serde(skip_serializing_if = "Option::is_none")]
    current_screen_id: Option<u32>,
    activity: &'a Activity,
}

fn wire_version_1() -> u32 {
    1
}

#[derive(Deserialize)]
struct TimelineWire {
    #[serde(default = "wire_version_1")]
    version: u32,
    task_id: String,
    steps: Vec<ViewerStep>,
    dropped_steps: u32,
    screens: Vec<ViewerScreen>,
    #[serde(default)]
    current_screen_id: Option<u32>,
    activity: Activity,
}

impl TimelineWire {
    fn into_timeline(self) -> Result<ViewerTimeline, WireError> {
        if self.version > WIRE_VERSION {
            return Err(WireError::Version(self.version));
        }
        let t = ViewerTimeline {
            task_id: self.task_id,
            steps: self.steps,
            dropped_steps: self.dropped_steps,
            screens: self.screens,
            current_screen_id: self.current_screen_id,
            activity: self.activity,
        };
        t.check_bounds()?;
        Ok(t)
    }
}

impl Serialize for ViewerTimeline {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        TimelineWireRef {
            version: WIRE_VERSION,
            task_id: &self.task_id,
            steps: &self.steps,
            dropped_steps: self.dropped_steps,
            screens: &self.screens,
            current_screen_id: self.current_screen_id,
            activity: &self.activity,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for ViewerTimeline {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        TimelineWire::deserialize(d)?
            .into_timeline()
            .map_err(D::Error::custom)
    }
}
