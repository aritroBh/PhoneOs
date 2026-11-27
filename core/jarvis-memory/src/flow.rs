//! The learned-flow data model: a recorded sequence of UI steps keyed by intent and app.

use std::collections::BTreeMap;

use jarvis_proto::{CompactScreen, Direction, Role};
use serde::{Deserialize, Serialize};

/// Maximum number of steps in a recordable flow.
pub const MAX_STEPS: usize = 64;

/// Slot number to the text bound for it from the task (see [`crate::intent::match_slotted`]).
pub type Bindings = BTreeMap<u32, String>;

/// FNV-1a 64-bit hash, hand-written so it is stable across Rust versions.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Deterministic flow id: FNV-1a 64 of intent key and app, as 16 hex digits.
pub fn flow_id(intent_key: &str, app_bundle_id: &str) -> String {
    let mut bytes = Vec::with_capacity(intent_key.len() + app_bundle_id.len() + 1);
    bytes.extend_from_slice(intent_key.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(app_bundle_id.as_bytes());
    format!("{:016x}", fnv1a64(&bytes))
}

/// Diagnostic signature of a screen: hash of the app and the element fingerprints in order.
/// Informational only; replay never gates on it because screens carry volatile content.
pub fn screen_signature(screen: &CompactScreen) -> u64 {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(screen.app_bundle_id.as_bytes());
    for e in &screen.elements {
        bytes.push(0);
        bytes.extend_from_slice(&e.fingerprint.to_le_bytes());
    }
    fnv1a64(&bytes)
}

/// Text to type: a literal, or a slot filled from the task at replay time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FlowText {
    /// Fixed text.
    Literal {
        /// The text.
        text: String,
    },
    /// Text bound from the task, by slot number.
    Slot {
        /// Slot number (`{n}` in the intent key).
        slot: u32,
    },
    /// A profile field, by key: replayed as the `{{profile.KEY}}` placeholder, which the
    /// agent substitutes at execution time. The value is never stored.
    Profile {
        /// Profile key as the model wrote it (aliases resolve at substitution).
        key: String,
    },
}

/// What a recorded step does. Mirrors the UI subset of `Action`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FlowAction {
    /// Tap the step's target.
    Tap,
    /// Press and hold the step's target.
    LongPress {
        /// Hold duration in milliseconds.
        duration_ms: u64,
    },
    /// Type into the step's target, or the focused field when it has none.
    Type {
        /// What to type.
        text: FlowText,
        /// Press Return afterwards.
        #[serde(default)]
        submit: bool,
    },
    /// Clear the step's target.
    Clear,
    /// Scroll the step's target, or the screen when it has none.
    Scroll {
        /// Scroll direction.
        direction: Direction,
    },
    /// Swipe on the step's target, or across the screen when it has none.
    Swipe {
        /// Finger direction.
        direction: Direction,
    },
    /// Go to the home screen.
    PressHome,
    /// Launch an app by bundle id or display name.
    OpenApp {
        /// Bundle id or display name.
        app: String,
    },
    /// Open a URL.
    OpenUrl {
        /// The URL.
        url: String,
    },
    /// Wait for the UI to settle.
    Wait {
        /// Milliseconds.
        ms: u64,
    },
    /// Stop replaying here and give control back to the model (secure or sensitive input).
    HandBack {
        /// Why control is handed back.
        reason: String,
    },
}

impl FlowAction {
    /// True when the step cannot run without a resolved target control.
    pub fn needs_target(&self) -> bool {
        matches!(
            self,
            FlowAction::Tap | FlowAction::LongPress { .. } | FlowAction::Clear
        )
    }

    /// True when the step acts on the contents of the foreground app, so the expected app must
    /// be in front. App-switching and waiting steps are exempt.
    pub fn acts_on_screen(&self) -> bool {
        !matches!(
            self,
            FlowAction::PressHome
                | FlowAction::OpenApp { .. }
                | FlowAction::OpenUrl { .. }
                | FlowAction::Wait { .. }
                | FlowAction::HandBack { .. }
        )
    }
}

/// One recorded step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowStep {
    /// Fingerprint of the target control, if the step has one.
    #[serde(default)]
    pub fingerprint: Option<u64>,
    /// Role of the target control.
    #[serde(default)]
    pub role: Option<Role>,
    /// Label of the target control (empty when none).
    #[serde(default)]
    pub label: String,
    /// Foreground app the step was recorded in.
    #[serde(default)]
    pub app_bundle_id: String,
    /// The action.
    pub action: FlowAction,
    /// Diagnostic [`screen_signature`] of the screen before the step.
    #[serde(default)]
    pub screen_hash_before: u64,
    /// Seventeenth review: the place the step's input went to when it was recorded (the
    /// app and the site, as `jarvis_safety::FlowContext::key` writes it; several joined
    /// with `|`). `None` for steps recorded before it was stored: such a step is replayed
    /// as an input into a place it may not have been recorded in (Rule T applies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
}

/// A learned flow: the UI steps that completed a task once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flow {
    /// Deterministic id, see [`flow_id`].
    pub id: String,
    /// App the work happens in (see `Recorder::finish`).
    #[serde(default)]
    pub app_bundle_id: String,
    /// Normalised task text, possibly with `{n}` slot placeholders.
    pub intent_key: String,
    /// Recorded steps.
    #[serde(default)]
    pub steps: Vec<FlowStep>,
    /// Runs that completed the task (the recording run counts as one).
    #[serde(default)]
    pub successes: u32,
    /// Runs that failed or diverged into failure.
    #[serde(default)]
    pub failures: u32,
    /// Last time the flow was recorded or used, in the shell's milliseconds.
    #[serde(default)]
    pub last_used_ms: u64,
    /// Set once the flow has been retired; retired flows are never replayed.
    #[serde(default)]
    pub retired: bool,
}

impl Flow {
    /// Total recorded runs of this flow.
    pub fn uses(&self) -> u32 {
        self.successes.saturating_add(self.failures)
    }

    /// True when the retirement rule applies: at least 3 uses and more failures than successes.
    pub fn should_retire(&self) -> bool {
        self.uses() >= 3 && self.failures > self.successes
    }
}
