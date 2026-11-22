//! The Live view as a stream of deltas, for a paired remote controller: a [`DeltaEncoder`]
//! on the phone watches the reducer's timeline and turns each change into a few small
//! [`ViewerDelta`]s; a [`DeltaDecoder`] on the controller applies them to its own
//! [`ViewerTimeline`], which the existing cursor, replay and scene code draw unchanged.
//!
//! # What goes in
//!
//! Only the reducer's **post-privacy** timeline ([`crate::Reducer::timeline`]), never the
//! driver's raw updates (those carry unredacted screen text and typed values). The encoder
//! sends nothing the phone's own timeline does not keep, and less: real-screen frame keys
//! are dropped (no pixels and no reference to pixels in this format), every string is cut
//! to [`MAX_WIRE_TEXT`] characters, and a wireframe that does not fit one delta is sent
//! with fewer elements. A locked observation is already an empty `locked` screen in the
//! timeline, so the stream follows the phone: nothing of a locked screen is sent.
//!
//! # Wire format (version [`WIRE_VERSION`], shared with [`crate::wire`])
//!
//! One JSON object per delta: `{"v":1,"gen":2,"seq":17,"kind":"step_added","step":{…}}`.
//! Steps, screens and activities use the [`crate::wire`] shapes. Kinds:
//!
//! | kind | fields | sequence |
//! |---|---|---|
//! | `reset` | `task_id`, `dropped_steps`, `current_screen_id`, `screen_size`, `activity` | starts a stream at `seq` |
//! | `snapshot_steps` | `steps` (oldest first, consecutive) | `seq` = the reset's, only right after it |
//! | `step_added` | `step` | advances |
//! | `step_changed` | `number` + changed fields among `core_step`, `gesture`, `target`, `caption`, `state` | advances |
//! | `step_replaced` | `step` (a change a field list cannot express) | advances |
//! | `steps_dropped` | `dropped_steps` (the phone's cap evicted up to this number) | advances |
//! | `current_screen` | `screen_id` (absent: none) | advances |
//! | `screen` | `screen` (sent at most once per id and stream) | side: `seq` ≤ the last one |
//! | `activity` | `activity` (a finished activity is the run's end) | advances |
//!
//! An **advancing** delta must carry exactly the previous `seq` + 1. A **side** delta
//! (`screen`, `snapshot_steps`) carries the `seq` of the advancing delta before it and
//! never advances, so the phone may delay a screen (rate limit) or drop it (a newer screen
//! replaced it before it was sent) without opening a gap; a dropped screen is simply "not
//! available" on the controller. `gen` changes when the phone's run restarts; every
//! generation starts with a `reset`.
//!
//! Lenient like [`crate::wire`]: unknown fields are ignored; an unknown kind is skipped
//! (advancing when its `seq` is the next one, a side delta when it is not newer); a
//! missing `v` reads as 1; a higher `v` is [`DeltaError::Version`] (the decoder stops: a
//! resync would bring the same version again).
//!
//! # Bounds
//!
//! * Every encoded delta is at most [`MAX_DELTA_BYTES`]; the decoder refuses larger input
//!   before parsing it.
//! * A snapshot ([`DeltaEncoder::snapshot`]) is a `reset`, up to [`SNAPSHOT_STEPS`] of the
//!   newest steps in `snapshot_steps` chunks, and the current screen: at most
//!   [`MAX_SNAPSHOT_BYTES`] in all (older steps are left out until it fits). Screens of
//!   older steps are not sent; the controller draws them as "not available".
//! * The decoder's timeline holds at most [`MAX_STEPS`] steps and only the screens its
//!   steps or the current screen refer to (at most [`MAX_SCREENS`], each at most
//!   [`MAX_ELEMENTS`] elements and [`MAX_DELTA_BYTES`] encoded), whatever the peer sends.
//!
//! # Resync
//!
//! The decoder reports [`Applied::NeedsResync`] once for a gap, a generation change
//! without a `reset`, an undecodable or oversized delta, or a delta that does not fit the
//! timeline (a step number out of order); then it skips everything until the next `reset`.
//! The phone answers a resync request with a snapshot. A reconnect starts with one too.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::geometry::ViewerSize;
use crate::reducer::Change;
use crate::wire::{
    activity_ok, screen_ok, step_ok, text_ok, Activity, ElementKind, Gesture, ScreenSource,
    StepState, ViewerElement, ViewerScreen, ViewerStep, ViewerTimeline, MAX_ELEMENTS, MAX_SCREENS,
    MAX_STEPS, MAX_WIRE_TEXT, WIRE_VERSION,
};

/// Largest encoded delta, in bytes. Chosen so a delta still fits one encrypted remote
/// frame (64 KiB of plaintext) after base64 (4/3) and the message envelope: 48 000 → 64 000
/// bytes of base64. That holds a wireframe of about 350 elements (≈ 135 bytes each); a
/// screen with more is sent with fewer elements, most relevant first, and flagged
/// truncated.
pub const MAX_DELTA_BYTES: usize = 48_000;
/// Largest snapshot (all its deltas together), in bytes: a reconnect or resync costs at
/// most this much, about a second on a slow local link.
pub const MAX_SNAPSHOT_BYTES: usize = 256 * 1024;
/// Newest steps a snapshot carries at most. A step encodes to 150–400 bytes, so 100 steps
/// plus a full-size current screen stay far below [`MAX_SNAPSHOT_BYTES`]; the byte bound
/// is enforced anyway, by leaving older steps out.
pub const SNAPSHOT_STEPS: usize = 100;

/// Why delta bytes could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeltaError {
    /// Larger than [`MAX_DELTA_BYTES`].
    #[error("delta too large")]
    TooLarge,
    /// Not JSON of the expected shape.
    #[error("malformed delta: {0}")]
    Malformed(String),
    /// Written by a newer, incompatible sender.
    #[error("delta version {0} is newer than {WIRE_VERSION}")]
    Version(u32),
}

/// The fields of a step that changed (each `Some` is the new value).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StepChange {
    /// The step's number.
    pub number: u32,
    /// New core step counter.
    pub core_step: Option<u32>,
    /// New gesture (a typing step's text resolved).
    pub gesture: Option<Gesture>,
    /// New target element.
    pub target: Option<ViewerElement>,
    /// New caption.
    pub caption: Option<String>,
    /// New state.
    pub state: Option<StepState>,
}

/// What one delta does.
#[derive(Debug, Clone, PartialEq)]
pub enum DeltaBody {
    /// Starts the stream (a new run, or a snapshot): the controller's timeline becomes
    /// empty with these values.
    Reset {
        /// The run's task id.
        task_id: String,
        /// Steps before the first one that will follow.
        dropped_steps: u32,
        /// The latest observed screen (its `screen` delta follows).
        current_screen_id: Option<u32>,
        /// The current screen's size, when known.
        screen_size: Option<ViewerSize>,
        /// What Jarvis is doing.
        activity: Activity,
    },
    /// Steps of a snapshot, oldest first (side delta right after its `reset`).
    SnapshotSteps(Vec<ViewerStep>),
    /// A new step.
    StepAdded(ViewerStep),
    /// Some fields of a step changed.
    StepChanged(StepChange),
    /// A step changed in a way a field list cannot express.
    StepReplaced(ViewerStep),
    /// The phone's cap dropped every step up to this number.
    StepsDropped(u32),
    /// The latest observed screen changed.
    CurrentScreen(Option<u32>),
    /// A screen's wireframe (side delta; sent at most once per id and stream).
    Screen(ViewerScreen),
    /// What Jarvis is doing changed (a finished activity ends the run).
    Activity(Activity),
    /// A kind from a newer sender (its tag): skipped.
    Unknown(String),
}

impl DeltaBody {
    /// The coding tag.
    pub fn kind(&self) -> &str {
        match self {
            DeltaBody::Reset { .. } => "reset",
            DeltaBody::SnapshotSteps(_) => "snapshot_steps",
            DeltaBody::StepAdded(_) => "step_added",
            DeltaBody::StepChanged(_) => "step_changed",
            DeltaBody::StepReplaced(_) => "step_replaced",
            DeltaBody::StepsDropped(_) => "steps_dropped",
            DeltaBody::CurrentScreen(_) => "current_screen",
            DeltaBody::Screen(_) => "screen",
            DeltaBody::Activity(_) => "activity",
            DeltaBody::Unknown(kind) => kind,
        }
    }
}

/// How a delta takes part in the sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeltaClass {
    /// `reset`: starts a stream.
    Reset,
    /// Must carry the previous `seq` + 1.
    Advancing,
    /// `snapshot_steps`: the reset's `seq`, only right after it.
    SnapshotSteps,
    /// `screen`: any `seq` not newer than the last one.
    Side,
    /// An unknown kind: advancing if its `seq` is the next one, skipped if not newer.
    Unknown,
}

impl DeltaClass {
    /// The class of a kind tag.
    pub fn of_kind(kind: &str) -> DeltaClass {
        match kind {
            "reset" => DeltaClass::Reset,
            "snapshot_steps" => DeltaClass::SnapshotSteps,
            "screen" => DeltaClass::Side,
            "step_added" | "step_changed" | "step_replaced" | "steps_dropped"
            | "current_screen" | "activity" => DeltaClass::Advancing,
            _ => DeltaClass::Unknown,
        }
    }
}

/// One delta: its stream position and what it does.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewerDelta {
    /// The stream generation (changes when the phone's run restarts).
    pub generation: u64,
    /// The sequence number (see the module docs).
    pub seq: u64,
    /// What it does.
    pub body: DeltaBody,
}

/// A delta's stream position, read without decoding its body (what a relay checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeltaHeader {
    /// The generation.
    pub generation: u64,
    /// The sequence number.
    pub seq: u64,
    /// How it takes part in the sequence.
    pub class: DeltaClass,
}

fn wire_version_1() -> u32 {
    1
}

#[derive(Deserialize)]
struct HeaderWire {
    #[serde(default = "wire_version_1")]
    v: u32,
    gen: u64,
    seq: u64,
    kind: String,
}

/// Reads a delta's header (version, generation, sequence, kind) without decoding the rest.
pub fn peek(bytes: &[u8]) -> Result<DeltaHeader, DeltaError> {
    if bytes.len() > MAX_DELTA_BYTES {
        return Err(DeltaError::TooLarge);
    }
    let h: HeaderWire =
        serde_json::from_slice(bytes).map_err(|e| DeltaError::Malformed(e.to_string()))?;
    if h.v > WIRE_VERSION {
        return Err(DeltaError::Version(h.v));
    }
    Ok(DeltaHeader {
        generation: h.gen,
        seq: h.seq,
        class: DeltaClass::of_kind(&h.kind),
    })
}

#[derive(Serialize)]
struct DeltaOut<'a> {
    v: u32,
    gen: u64,
    seq: u64,
    kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dropped_steps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_screen_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screen_size: Option<ViewerSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity: Option<&'a Activity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    steps: Option<&'a [ViewerStep]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    step: Option<&'a ViewerStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    core_step: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gesture: Option<&'a Gesture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'a ViewerElement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    caption: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state: Option<&'a StepState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screen_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screen: Option<&'a ViewerScreen>,
}

impl ViewerDelta {
    /// The header.
    pub fn header(&self) -> DeltaHeader {
        DeltaHeader {
            generation: self.generation,
            seq: self.seq,
            class: DeltaClass::of_kind(self.body.kind()),
        }
    }

    /// The wire form (JSON). Deltas made by [`DeltaEncoder`] are at most
    /// [`MAX_DELTA_BYTES`].
    pub fn encode(&self) -> Vec<u8> {
        let mut out = DeltaOut {
            v: WIRE_VERSION,
            gen: self.generation,
            seq: self.seq,
            kind: self.body.kind(),
            task_id: None,
            dropped_steps: None,
            current_screen_id: None,
            screen_size: None,
            activity: None,
            steps: None,
            step: None,
            number: None,
            core_step: None,
            gesture: None,
            target: None,
            caption: None,
            state: None,
            screen_id: None,
            screen: None,
        };
        match &self.body {
            DeltaBody::Reset {
                task_id,
                dropped_steps,
                current_screen_id,
                screen_size,
                activity,
            } => {
                out.task_id = Some(task_id);
                out.dropped_steps = Some(*dropped_steps);
                out.current_screen_id = *current_screen_id;
                out.screen_size = *screen_size;
                out.activity = Some(activity);
            }
            DeltaBody::SnapshotSteps(steps) => out.steps = Some(steps),
            DeltaBody::StepAdded(step) | DeltaBody::StepReplaced(step) => out.step = Some(step),
            DeltaBody::StepChanged(c) => {
                out.number = Some(c.number);
                out.core_step = c.core_step;
                out.gesture = c.gesture.as_ref();
                out.target = c.target.as_ref();
                out.caption = c.caption.as_deref();
                out.state = c.state.as_ref();
            }
            DeltaBody::StepsDropped(n) => out.dropped_steps = Some(*n),
            DeltaBody::CurrentScreen(id) => out.screen_id = *id,
            DeltaBody::Screen(screen) => out.screen = Some(screen),
            DeltaBody::Activity(activity) => out.activity = Some(activity),
            DeltaBody::Unknown(_) => {}
        }
        // Serializing plain data into a Vec cannot fail (map keys are strings).
        serde_json::to_vec(&out).unwrap_or_default()
    }

    /// Reads the wire form (bounded: larger than [`MAX_DELTA_BYTES`] is refused unread).
    pub fn decode(bytes: &[u8]) -> Result<ViewerDelta, DeltaError> {
        if bytes.len() > MAX_DELTA_BYTES {
            return Err(DeltaError::TooLarge);
        }
        let malformed = |e: serde_json::Error| DeltaError::Malformed(e.to_string());
        let mut map: Map<String, Value> = serde_json::from_slice(bytes).map_err(malformed)?;
        let version = match map.get("v") {
            None => 1,
            Some(v) => v
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| DeltaError::Malformed("bad version".to_owned()))?,
        };
        if version > WIRE_VERSION {
            return Err(DeltaError::Version(version));
        }
        let generation = take_u64(&map, "gen")?;
        let seq = take_u64(&map, "seq")?;
        let kind = match map.get("kind") {
            Some(Value::String(k)) => k.clone(),
            _ => return Err(DeltaError::Malformed("missing kind".to_owned())),
        };
        let mut field = |name: &str| map.remove(name).filter(|v| !v.is_null());
        fn need<T: serde::de::DeserializeOwned>(
            v: Option<Value>,
            name: &str,
        ) -> Result<T, DeltaError> {
            let v = v.ok_or_else(|| DeltaError::Malformed(format!("missing {name}")))?;
            serde_json::from_value(v).map_err(|e| DeltaError::Malformed(e.to_string()))
        }
        fn opt<T: serde::de::DeserializeOwned>(v: Option<Value>) -> Result<Option<T>, DeltaError> {
            v.map(|v| serde_json::from_value(v).map_err(|e| DeltaError::Malformed(e.to_string())))
                .transpose()
        }
        let body = match kind.as_str() {
            "reset" => DeltaBody::Reset {
                task_id: need(field("task_id"), "task_id")?,
                dropped_steps: opt(field("dropped_steps"))?.unwrap_or(0),
                current_screen_id: opt(field("current_screen_id"))?,
                screen_size: opt(field("screen_size"))?,
                activity: opt(field("activity"))?.unwrap_or(Activity::Idle),
            },
            "snapshot_steps" => DeltaBody::SnapshotSteps(need(field("steps"), "steps")?),
            "step_added" => DeltaBody::StepAdded(need(field("step"), "step")?),
            "step_replaced" => DeltaBody::StepReplaced(need(field("step"), "step")?),
            "step_changed" => DeltaBody::StepChanged(StepChange {
                number: need(field("number"), "number")?,
                core_step: opt(field("core_step"))?,
                gesture: opt(field("gesture"))?,
                target: opt(field("target"))?,
                caption: opt(field("caption"))?,
                state: opt(field("state"))?,
            }),
            "steps_dropped" => {
                DeltaBody::StepsDropped(need(field("dropped_steps"), "dropped_steps")?)
            }
            "current_screen" => DeltaBody::CurrentScreen(opt(field("screen_id"))?),
            "screen" => DeltaBody::Screen(need(field("screen"), "screen")?),
            "activity" => DeltaBody::Activity(need(field("activity"), "activity")?),
            _ => DeltaBody::Unknown(kind),
        };
        Ok(ViewerDelta {
            generation,
            seq,
            body,
        })
    }
}

fn take_u64(map: &Map<String, Value>, name: &str) -> Result<u64, DeltaError> {
    map.get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| DeltaError::Malformed(format!("missing {name}")))
}

// MARK: - Sanitizing (what the encoder sends of the phone's timeline)

fn cap(s: &str) -> String {
    if text_ok(s) {
        return s.to_owned();
    }
    s.chars().take(MAX_WIRE_TEXT).collect()
}

fn cap_opt(s: &Option<String>) -> Option<String> {
    s.as_deref().map(cap)
}

fn sanitize_element(e: &ViewerElement) -> ViewerElement {
    ViewerElement {
        frame: e.frame,
        role: cap(&e.role),
        kind: e.kind,
        label: cap_opt(&e.label),
        secure: e.secure,
        depth: e.depth,
    }
}

fn sanitize_gesture(g: &Gesture) -> Gesture {
    match g {
        Gesture::Type(t) => Gesture::Type(cap_opt(t)),
        Gesture::Key(t) => Gesture::Key(cap(t)),
        Gesture::Launch(t) => Gesture::Launch(cap(t)),
        Gesture::OpenUrl(t) => Gesture::OpenUrl(cap(t)),
        Gesture::Unknown(t) => Gesture::Unknown(cap(t)),
        other => other.clone(),
    }
}

fn sanitize_state(s: &StepState) -> StepState {
    match s {
        StepState::Failed(t) => StepState::Failed(cap(t)),
        StepState::Unknown(t) => StepState::Unknown(cap(t)),
        other => other.clone(),
    }
}

fn sanitize_activity(a: &Activity) -> Activity {
    match a {
        Activity::UsingTool(t) => Activity::UsingTool(cap(t)),
        Activity::WaitingForApproval(t) => Activity::WaitingForApproval(cap(t)),
        Activity::Unknown(t) => Activity::Unknown(cap(t)),
        other => other.clone(),
    }
}

/// A step as the stream sends it: every string cut to [`MAX_WIRE_TEXT`] characters (so a
/// step always fits one delta).
pub fn wire_step(step: &ViewerStep) -> ViewerStep {
    ViewerStep {
        number: step.number,
        core_step: step.core_step,
        screen_id: step.screen_id,
        gesture: sanitize_gesture(&step.gesture),
        target: step.target.as_ref().map(sanitize_element),
        caption: cap(&step.caption),
        state: sanitize_state(&step.state),
    }
}

/// How relevant an element is when a wireframe must be cut (lower first).
fn kind_rank(kind: ElementKind) -> u8 {
    match kind {
        ElementKind::Alert => 0,
        ElementKind::Input => 1,
        ElementKind::Control => 2,
        ElementKind::Keyboard => 3,
        ElementKind::Container => 4,
        ElementKind::Text => 5,
        ElementKind::Image => 6,
    }
}

fn area(e: &ViewerElement) -> f64 {
    let a = e.frame.width * e.frame.height;
    if a.is_finite() {
        a.max(0.0)
    } else {
        0.0
    }
}

fn encoded_len<T: Serialize>(v: &T) -> usize {
    serde_json::to_vec(v).map_or(usize::MAX, |b| b.len())
}

/// A screen as the stream sends it: no frame key (no pixels, no reference to pixels),
/// strings cut to [`MAX_WIRE_TEXT`], at most [`MAX_ELEMENTS`] elements, and, when its
/// `screen` delta would exceed [`MAX_DELTA_BYTES`], only the most relevant elements that
/// fit (alerts, inputs, controls, keyboard, containers, text, images; larger first), kept
/// in drawing order and flagged `elements_truncated`.
pub fn wire_screen(screen: &ViewerScreen) -> ViewerScreen {
    let mut elements: Vec<ViewerElement> = screen
        .elements
        .iter()
        .take(MAX_ELEMENTS)
        .map(sanitize_element)
        .collect();
    let mut truncated = screen.elements_truncated || screen.elements.len() > MAX_ELEMENTS;
    let mut out = ViewerScreen {
        id: screen.id,
        size: screen.size,
        app_name: cap_opt(&screen.app_name),
        bundle_id: cap(&screen.bundle_id),
        source: match &screen.source {
            ScreenSource::Unknown(t) => ScreenSource::Unknown(cap(t)),
            other => other.clone(),
        },
        elements: Vec::new(),
        elements_truncated: true,
        frame_key: None,
        focused_input_secure: screen.focused_input_secure,
    };
    // The header with the widest generation and sequence numbers.
    let probe = |s: &ViewerScreen| {
        ViewerDelta {
            generation: u64::MAX,
            seq: u64::MAX,
            body: DeltaBody::Screen(s.clone()),
        }
        .encode()
        .len()
    };
    let header = probe(&out);
    let sizes: Vec<usize> = elements.iter().map(|e| encoded_len(e) + 1).collect();
    let total: usize = header.saturating_add(sizes.iter().sum::<usize>());
    if total > MAX_DELTA_BYTES {
        let budget = MAX_DELTA_BYTES.saturating_sub(header);
        let mut order: Vec<usize> = (0..elements.len()).collect();
        let rank: Vec<(u8, f64)> = elements
            .iter()
            .map(|e| (kind_rank(e.kind), area(e)))
            .collect();
        let key = |i: usize| rank.get(i).copied().unwrap_or((u8::MAX, 0.0));
        order.sort_by(|&a, &b| {
            let ((ka, aa), (kb, ab)) = (key(a), key(b));
            ka.cmp(&kb)
                .then_with(|| ab.partial_cmp(&aa).unwrap_or(Ordering::Equal))
                .then_with(|| a.cmp(&b))
        });
        let mut keep = vec![false; elements.len()];
        let mut used = 0usize;
        for i in order {
            let size = sizes.get(i).copied().unwrap_or(usize::MAX);
            if used.saturating_add(size) <= budget {
                used += size;
                if let Some(k) = keep.get_mut(i) {
                    *k = true;
                }
            }
        }
        let mut i = 0;
        elements.retain(|_| {
            let k = keep.get(i).copied().unwrap_or(false);
            i += 1;
            k
        });
        truncated = true;
    }
    out.elements = elements;
    out.elements_truncated = truncated;
    // Belt and braces: never above the bound, whatever the arithmetic above missed.
    while probe(&out) > MAX_DELTA_BYTES && !out.elements.is_empty() {
        out.elements.pop();
        out.elements_truncated = true;
    }
    out
}

/// The timeline a controller rebuilds from a complete stream: frame keys dropped, strings
/// capped, wireframes cut like [`wire_screen`]. (Coordinates are as the JSON wire format
/// writes them; compare encodings.)
pub fn wire_timeline(t: &ViewerTimeline) -> ViewerTimeline {
    ViewerTimeline {
        task_id: cap(&t.task_id),
        steps: t.steps.iter().map(wire_step).collect(),
        dropped_steps: t.dropped_steps,
        screens: t.screens.iter().map(wire_screen).collect(),
        current_screen_id: t.current_screen_id,
        activity: sanitize_activity(&t.activity),
    }
}

/// Steps equal as the wire would carry them (a NaN coordinate equals itself).
fn same_step(a: &ViewerStep, b: &ViewerStep) -> bool {
    a == b || wire_bytes(a) == wire_bytes(b)
}

fn wire_bytes<T: Serialize>(v: &T) -> Vec<u8> {
    serde_json::to_vec(v).unwrap_or_default()
}

fn same_gesture(a: &Gesture, b: &Gesture) -> bool {
    a == b || wire_bytes(a) == wire_bytes(b)
}

fn same_element(a: &Option<ViewerElement>, b: &Option<ViewerElement>) -> bool {
    a == b || wire_bytes(a) == wire_bytes(b)
}

// MARK: - Encoder

/// The phone's side: watches the timeline, emits deltas, builds snapshots. Holds a copy of
/// the steps (as last seen) and the current screen (as sent), nothing else.
#[derive(Debug, Clone)]
pub struct DeltaEncoder {
    started: bool,
    generation: u64,
    seq: u64,
    run: u64,
    task_id: String,
    steps: Vec<ViewerStep>,
    dropped_steps: u32,
    current_screen_id: Option<u32>,
    /// The current screen as sent (for snapshots).
    current_screen: Option<ViewerScreen>,
    /// The highest screen id sent in this generation.
    last_screen_sent: Option<u32>,
    activity: Activity,
}

impl Default for DeltaEncoder {
    fn default() -> Self {
        DeltaEncoder {
            started: false,
            generation: 0,
            seq: 0,
            run: 0,
            task_id: String::new(),
            steps: Vec::new(),
            dropped_steps: 0,
            current_screen_id: None,
            current_screen: None,
            last_screen_sent: None,
            activity: Activity::Idle,
        }
    }
}

impl DeltaEncoder {
    /// An encoder that has seen nothing (generation 0, sequence 0).
    pub fn new() -> Self {
        Self::default()
    }

    /// The current generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The sequence number of the last advancing delta (or reset).
    pub fn seq(&self) -> u64 {
        self.seq
    }

    fn next(&mut self, body: DeltaBody) -> ViewerDelta {
        self.seq = self.seq.saturating_add(1);
        ViewerDelta {
            generation: self.generation,
            seq: self.seq,
            body,
        }
    }

    fn side(&self, body: DeltaBody) -> ViewerDelta {
        ViewerDelta {
            generation: self.generation,
            seq: self.seq,
            body,
        }
    }

    /// Compares `timeline` (the reducer's, after its privacy rules) with what was seen
    /// last and returns the deltas that bring a controller from there to here, in the order
    /// to apply them. `run` counts the runs the phone's view has started (bump it on
    /// `Update::Started`, see [`crate::LiveView::runs`]): a new run starts a new generation
    /// with a `reset`. A timeline that cannot follow from the last one (an earlier screen
    /// or step number, a different task id) does too.
    pub fn feed(&mut self, run: u64, timeline: &ViewerTimeline) -> Vec<ViewerDelta> {
        let mut out = Vec::new();
        let last_number = |steps: &[ViewerStep]| steps.last().map(|s| s.number);
        let restart = !self.started
            || run != self.run
            || timeline.task_id != self.task_id
            || timeline.dropped_steps < self.dropped_steps
            || last_number(&timeline.steps) < last_number(&self.steps)
            || timeline.current_screen_id < self.current_screen_id;
        if restart {
            self.started = true;
            self.run = run;
            self.generation = self.generation.saturating_add(1);
            self.task_id = timeline.task_id.clone();
            self.steps.clear();
            self.dropped_steps = 0;
            self.current_screen_id = None;
            self.current_screen = None;
            self.last_screen_sent = None;
            self.activity = Activity::Idle;
            let reset = DeltaBody::Reset {
                task_id: cap(&self.task_id),
                dropped_steps: 0,
                current_screen_id: None,
                screen_size: None,
                activity: Activity::Idle,
            };
            out.push(self.next(reset));
        }
        // 1. Evictions by the phone's cap.
        if timeline.dropped_steps > self.dropped_steps {
            self.dropped_steps = timeline.dropped_steps;
            let d = self.dropped_steps;
            self.steps.retain(|s| s.number > d);
            out.push(self.next(DeltaBody::StepsDropped(d)));
        }
        // 2. Steps added or changed (before the screens, so a screen that arrives refers to
        //    a step or the current screen the controller already has).
        let known_last = last_number(&self.steps);
        for step in &timeline.steps {
            let is_new = known_last.is_none_or(|n| step.number > n);
            if is_new {
                let body = DeltaBody::StepAdded(wire_step(step));
                out.push(self.next(body));
                self.steps.push(step.clone());
                continue;
            }
            let Some(i) = self.steps.iter().position(|s| s.number == step.number) else {
                continue;
            };
            let Some(old) = self.steps.get(i) else {
                continue;
            };
            if same_step(old, step) {
                continue;
            }
            let body = step_change(old, step);
            out.push(self.next(body));
            if let Some(slot) = self.steps.get_mut(i) {
                *slot = step.clone();
            }
        }
        // Steps the phone no longer has (a shorter list without a cap eviction): forget.
        let kept_first = timeline.steps.first().map(|s| s.number);
        self.steps
            .retain(|s| kept_first.is_some_and(|f| s.number >= f));
        // 3. The current screen, then new screens.
        if timeline.current_screen_id != self.current_screen_id {
            self.current_screen_id = timeline.current_screen_id;
            self.current_screen = None;
            out.push(self.next(DeltaBody::CurrentScreen(self.current_screen_id)));
        }
        for screen in &timeline.screens {
            if self.last_screen_sent.is_some_and(|last| screen.id <= last) {
                continue;
            }
            self.last_screen_sent = Some(screen.id);
            let wire = wire_screen(screen);
            if Some(screen.id) == self.current_screen_id {
                self.current_screen = Some(wire.clone());
            }
            out.push(self.side(DeltaBody::Screen(wire)));
        }
        if self.current_screen.is_none() {
            self.current_screen = timeline.current_screen().map(wire_screen);
        }
        // 4. The activity.
        if timeline.activity != self.activity {
            self.activity = timeline.activity.clone();
            out.push(self.next(DeltaBody::Activity(sanitize_activity(&self.activity))));
        }
        out
    }

    /// A snapshot of what was last fed, at the current position: a `reset`, the newest
    /// steps (at most [`SNAPSHOT_STEPS`]) in `snapshot_steps` chunks and the current
    /// screen. Every delta is at most [`MAX_DELTA_BYTES`], all together at most
    /// [`MAX_SNAPSHOT_BYTES`]. Deltas fed afterwards continue it.
    pub fn snapshot(&self) -> Vec<ViewerDelta> {
        let screen = self.current_screen.as_ref().map(|s| {
            let d = self.side(DeltaBody::Screen(s.clone()));
            let len = d.encode().len();
            (d, len)
        });
        let screen_len = screen.as_ref().map_or(0, |(_, l)| *l);
        let wire_steps: Vec<ViewerStep> = self
            .steps
            .iter()
            .rev()
            .take(SNAPSHOT_STEPS)
            .rev()
            .map(wire_step)
            .collect();
        let sizes: Vec<usize> = wire_steps.iter().map(|s| encoded_len(s) + 1).collect();
        let mut skip = 0usize;
        loop {
            let kept = wire_steps.get(skip..).unwrap_or(&[]);
            let kept_sizes = sizes.get(skip..).unwrap_or(&[]);
            let mut deltas = Vec::new();
            let dropped = kept
                .first()
                .map_or(self.dropped_steps, |s| s.number.saturating_sub(1));
            deltas.push(self.side(DeltaBody::Reset {
                task_id: cap(&self.task_id),
                dropped_steps: dropped,
                current_screen_id: self.current_screen_id,
                screen_size: self.current_screen.as_ref().map(|s| s.size),
                activity: sanitize_activity(&self.activity),
            }));
            let chunk_header = self
                .side(DeltaBody::SnapshotSteps(Vec::new()))
                .encode()
                .len();
            let mut chunk: Vec<ViewerStep> = Vec::new();
            let mut chunk_len = chunk_header;
            for (step, size) in kept.iter().zip(kept_sizes) {
                if !chunk.is_empty() && chunk_len.saturating_add(*size) > MAX_DELTA_BYTES {
                    deltas.push(self.side(DeltaBody::SnapshotSteps(std::mem::take(&mut chunk))));
                    chunk_len = chunk_header;
                }
                chunk.push(step.clone());
                chunk_len = chunk_len.saturating_add(*size);
            }
            if !chunk.is_empty() {
                deltas.push(self.side(DeltaBody::SnapshotSteps(chunk)));
            }
            let total: usize = deltas
                .iter()
                .map(|d| d.encode().len())
                .sum::<usize>()
                .saturating_add(screen_len);
            if total <= MAX_SNAPSHOT_BYTES || kept.is_empty() {
                if let Some((d, _)) = screen {
                    deltas.push(d);
                }
                return deltas;
            }
            // Leave older steps out until it fits (about a tenth at a time).
            skip += (kept.len() / 10).max(1);
        }
    }
}

/// The delta for a step that changed from `old` to `new`.
fn step_change(old: &ViewerStep, new: &ViewerStep) -> DeltaBody {
    let inexpressible = old.number != new.number
        || old.screen_id != new.screen_id
        || (old.core_step.is_some() && new.core_step.is_none())
        || (old.target.is_some() && new.target.is_none());
    if inexpressible {
        return DeltaBody::StepReplaced(wire_step(new));
    }
    let w = wire_step(new);
    DeltaBody::StepChanged(StepChange {
        number: new.number,
        core_step: (old.core_step != new.core_step)
            .then_some(new.core_step)
            .flatten(),
        gesture: (!same_gesture(&old.gesture, &new.gesture)).then_some(w.gesture),
        target: (!same_element(&old.target, &new.target))
            .then_some(w.target)
            .flatten(),
        caption: (old.caption != new.caption).then_some(w.caption),
        state: (old.state != new.state).then_some(w.state),
    })
}

// MARK: - Stream position (shared by the decoder and a relay such as the remote client)

/// Why a stream needs a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResyncReason {
    /// A sequence number was skipped, or a delta came before any reset.
    Gap,
    /// A different generation without a reset.
    Generation,
    /// Bytes that do not decode (or exceed [`MAX_DELTA_BYTES`]).
    Undecodable,
    /// A delta that does not fit the timeline (a step number out of order, a value over a
    /// bound).
    Invalid,
}

/// What [`StreamCursor::admit`] decided about one delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Admit {
    /// Apply it.
    Apply,
    /// Skip it quietly (an unknown kind, or anything while a resync is awaited).
    Skip,
    /// The stream broke: request a snapshot (reported once; then everything but a reset
    /// is skipped).
    Resync(ResyncReason),
}

/// Where a stream stands: the generation and sequence number of the last delta applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StreamCursor {
    generation: Option<u64>,
    seq: u64,
    in_snapshot: bool,
    broken: bool,
}

impl StreamCursor {
    /// A cursor waiting for a reset.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a snapshot is awaited (a gap was seen, or no reset yet).
    pub fn needs_resync(&self) -> bool {
        self.broken || self.generation.is_none()
    }

    /// The position: generation and sequence number.
    pub fn position(&self) -> Option<(u64, u64)> {
        self.generation.map(|g| (g, self.seq))
    }

    /// Marks the stream broken (e.g. a delta that did not fit the timeline). Returns
    /// whether it was not broken before.
    pub fn break_stream(&mut self) -> bool {
        let first = !self.broken;
        self.broken = true;
        first
    }

    fn fail(&mut self, reason: ResyncReason) -> Admit {
        if self.break_stream() {
            Admit::Resync(reason)
        } else {
            Admit::Skip
        }
    }

    /// Checks one delta's position and advances the cursor when it is applied.
    pub fn admit(&mut self, h: &DeltaHeader) -> Admit {
        if h.class == DeltaClass::Reset {
            self.generation = Some(h.generation);
            self.seq = h.seq;
            self.in_snapshot = true;
            self.broken = false;
            return Admit::Apply;
        }
        if self.broken {
            return Admit::Skip;
        }
        let Some(generation) = self.generation else {
            return self.fail(ResyncReason::Gap);
        };
        if h.generation != generation {
            return self.fail(ResyncReason::Generation);
        }
        let next = self.seq.checked_add(1);
        match h.class {
            DeltaClass::Reset => Admit::Apply,
            DeltaClass::Advancing => {
                if Some(h.seq) == next {
                    self.seq = h.seq;
                    self.in_snapshot = false;
                    Admit::Apply
                } else {
                    self.fail(ResyncReason::Gap)
                }
            }
            DeltaClass::SnapshotSteps => {
                if h.seq == self.seq && self.in_snapshot {
                    Admit::Apply
                } else {
                    self.fail(ResyncReason::Gap)
                }
            }
            DeltaClass::Side => {
                if h.seq <= self.seq {
                    Admit::Apply
                } else {
                    self.fail(ResyncReason::Gap)
                }
            }
            DeltaClass::Unknown => {
                if Some(h.seq) == next {
                    self.seq = h.seq;
                    self.in_snapshot = false;
                    Admit::Skip
                } else if h.seq <= self.seq {
                    Admit::Skip
                } else {
                    self.fail(ResyncReason::Gap)
                }
            }
        }
    }
}

// MARK: - Decoder

/// What applying one delta did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Applied; what changed.
    Changed(Change),
    /// Nothing to do (an unknown kind, a screen already held or no longer referred to,
    /// anything while a resync is awaited).
    Unchanged,
    /// The stream broke: ask the phone for a snapshot. Reported once per break.
    NeedsResync(ResyncReason),
    /// Written by a newer, incompatible phone (its version): stop watching.
    Incompatible(u32),
}

/// The controller's side: applies deltas to a timeline. Treats the phone as untrusted
/// input: never panics, and the timeline stays within the bounds in the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DeltaDecoder {
    cursor: StreamCursor,
    incompatible: Option<u32>,
}

impl DeltaDecoder {
    /// A decoder waiting for a reset.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a snapshot is awaited.
    pub fn needs_resync(&self) -> bool {
        self.cursor.needs_resync()
    }

    /// The stream position.
    pub fn cursor(&self) -> &StreamCursor {
        &self.cursor
    }

    /// Applies delta bytes to `timeline`.
    pub fn apply(&mut self, timeline: &mut ViewerTimeline, bytes: &[u8]) -> Applied {
        if self.incompatible.is_some() {
            return Applied::Unchanged;
        }
        match ViewerDelta::decode(bytes) {
            Ok(delta) => self.apply_delta(timeline, delta),
            Err(DeltaError::Version(v)) => {
                self.incompatible = Some(v);
                Applied::Incompatible(v)
            }
            Err(DeltaError::TooLarge | DeltaError::Malformed(_)) => {
                if self.cursor.break_stream() {
                    Applied::NeedsResync(ResyncReason::Undecodable)
                } else {
                    Applied::Unchanged
                }
            }
        }
    }

    /// Applies a decoded delta to `timeline`.
    pub fn apply_delta(&mut self, timeline: &mut ViewerTimeline, delta: ViewerDelta) -> Applied {
        if self.incompatible.is_some() {
            return Applied::Unchanged;
        }
        match self.cursor.admit(&delta.header()) {
            Admit::Apply => {}
            Admit::Skip => return Applied::Unchanged,
            Admit::Resync(reason) => return Applied::NeedsResync(reason),
        }
        match apply_body(timeline, delta.body) {
            Some(change) if change.any() => Applied::Changed(change),
            Some(_) => Applied::Unchanged,
            None => {
                if self.cursor.break_stream() {
                    Applied::NeedsResync(ResyncReason::Invalid)
                } else {
                    Applied::Unchanged
                }
            }
        }
    }
}

/// Keeps the screens the kept steps refer to and the current one (as the reducer does).
fn prune(t: &mut ViewerTimeline) -> bool {
    let before = t.screens.len();
    let mut used: Vec<u32> = t.steps.iter().filter_map(|s| s.screen_id).collect();
    if let Some(current) = t.current_screen_id {
        used.push(current);
    }
    used.sort_unstable();
    used.dedup();
    t.screens.retain(|s| used.binary_search(&s.id).is_ok());
    t.screens.len() != before
}

/// The number the next step must have.
fn next_number(t: &ViewerTimeline) -> Option<u32> {
    match t.steps.last() {
        Some(s) => s.number.checked_add(1),
        None => t.dropped_steps.checked_add(1),
    }
}

/// Appends a step (its number must be the next one); evicts beyond [`MAX_STEPS`].
fn push_step(t: &mut ViewerTimeline, step: ViewerStep) -> Option<bool> {
    if Some(step.number) != next_number(t) || !step_ok(&step) {
        return None;
    }
    t.steps.push(step);
    let mut screens = false;
    if t.steps.len() > MAX_STEPS {
        let drop = t.steps.len() - MAX_STEPS;
        t.steps.drain(..drop);
        t.dropped_steps = t
            .steps
            .first()
            .map_or(t.dropped_steps, |s| s.number.saturating_sub(1));
        screens = prune(t);
    }
    Some(screens)
}

/// Applies one body; `None` when it does not fit the timeline.
fn apply_body(t: &mut ViewerTimeline, body: DeltaBody) -> Option<Change> {
    let mut change = Change::default();
    match body {
        DeltaBody::Reset {
            task_id,
            dropped_steps,
            current_screen_id,
            activity,
            ..
        } => {
            if !text_ok(&task_id) || !activity_ok(&activity) {
                return None;
            }
            *t = ViewerTimeline {
                task_id,
                steps: Vec::new(),
                dropped_steps,
                screens: Vec::new(),
                current_screen_id,
                activity,
            };
            change = Change {
                steps: true,
                screens: true,
                activity: true,
                ..Change::default()
            };
        }
        DeltaBody::SnapshotSteps(steps) => {
            if steps.len() > MAX_STEPS {
                return None;
            }
            for step in steps {
                change.screens |= push_step(t, step)?;
                change.steps = true;
            }
        }
        DeltaBody::StepAdded(step) => {
            change.screens = push_step(t, step)?;
            change.steps = true;
        }
        DeltaBody::StepChanged(c) => {
            let Some(i) = t.step_index(c.number) else {
                // A step older than what this controller holds (left out of a snapshot).
                return (c.number <= t.dropped_steps).then_some(change);
            };
            let step = t.steps.get_mut(i)?;
            let mut new = step.clone();
            if let Some(v) = c.core_step {
                new.core_step = Some(v);
            }
            if let Some(v) = c.gesture {
                new.gesture = v;
            }
            if let Some(v) = c.target {
                new.target = Some(v);
            }
            if let Some(v) = c.caption {
                new.caption = v;
            }
            if let Some(v) = c.state {
                new.state = v;
            }
            if !step_ok(&new) {
                return None;
            }
            *step = new;
            change.steps = true;
        }
        DeltaBody::StepReplaced(new) => {
            let Some(i) = t.step_index(new.number) else {
                return (new.number <= t.dropped_steps).then_some(change);
            };
            if !step_ok(&new) {
                return None;
            }
            *t.steps.get_mut(i)? = new;
            change.steps = true;
            change.screens = prune(t);
        }
        DeltaBody::StepsDropped(d) => {
            let before = (t.steps.len(), t.dropped_steps);
            t.steps.retain(|s| s.number > d);
            // A list emptied by the cap continues after the last dropped number.
            t.dropped_steps = t.dropped_steps.max(d);
            change.steps = (t.steps.len(), t.dropped_steps) != before;
            change.screens = prune(t);
        }
        DeltaBody::CurrentScreen(id) => {
            change.screens = t.current_screen_id != id;
            t.current_screen_id = id;
            change.screens |= prune(t);
        }
        DeltaBody::Screen(screen) => {
            if screen.elements.len() > MAX_ELEMENTS || !screen_ok(&screen) {
                return None;
            }
            let referenced = t.current_screen_id == Some(screen.id)
                || t.steps.iter().any(|s| s.screen_id == Some(screen.id));
            if !referenced {
                return Some(change);
            }
            match t.screens.binary_search_by_key(&screen.id, |s| s.id) {
                Ok(_) => {}
                Err(i) => {
                    if t.screens.len() >= MAX_SCREENS {
                        return None;
                    }
                    t.screens.insert(i, screen);
                    change.screens = true;
                }
            }
        }
        DeltaBody::Activity(activity) => {
            if !activity_ok(&activity) {
                return None;
            }
            change.activity = t.activity != activity;
            t.activity = activity;
        }
        DeltaBody::Unknown(_) => {}
    }
    Some(change)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_kinds_and_fields_are_skipped() {
        let mut t = ViewerTimeline::default();
        let mut d = DeltaDecoder::new();
        let reset = br#"{"gen":1,"seq":1,"kind":"reset","task_id":"t","future":[1,2]}"#;
        assert!(matches!(d.apply(&mut t, reset), Applied::Changed(_)));
        let unknown = br#"{"v":1,"gen":1,"seq":2,"kind":"hologram","x":{}}"#;
        assert_eq!(d.apply(&mut t, unknown), Applied::Unchanged);
        let act = br#"{"gen":1,"seq":3,"kind":"activity","activity":{"kind":"thinking"}}"#;
        assert!(matches!(d.apply(&mut t, act), Applied::Changed(_)));
        assert_eq!(t.activity, Activity::Thinking);
        let newer = br#"{"v":2,"gen":1,"seq":4,"kind":"activity"}"#;
        assert_eq!(d.apply(&mut t, newer), Applied::Incompatible(2));
    }

    #[test]
    fn a_gap_is_reported_once() {
        let mut t = ViewerTimeline::default();
        let mut d = DeltaDecoder::new();
        let act = |seq: u64| {
            format!(r#"{{"gen":1,"seq":{seq},"kind":"activity","activity":{{"kind":"looking"}}}}"#)
        };
        assert_eq!(
            d.apply(&mut t, act(2).as_bytes()),
            Applied::NeedsResync(ResyncReason::Gap)
        );
        assert_eq!(d.apply(&mut t, act(3).as_bytes()), Applied::Unchanged);
        assert!(d.needs_resync());
    }
}
