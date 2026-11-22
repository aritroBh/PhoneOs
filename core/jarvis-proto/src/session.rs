//! Session IO: the events the shell feeds the core and the effects the core asks for.

use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::model::{ModelOutput, ModelRequest};
use crate::policy::RiskCategory;
use crate::runner::{RunnerCommand, RunnerResult};
use crate::tool::{ToolCall, ToolResult};
use crate::ui::Observation;

/// One input to the session. JSON is internally tagged with `"type"`; newtype variants
/// flatten their payload's fields next to the tag (`{"type":"observed","screen":...}`).
///
/// Lock awareness: the shell may learn that the phone locked from an observation
/// (`Observation::device_locked`), from a refused command (`RunnerResult::device_locked`),
/// or between effects (`DeviceLocked`). The core never tries to unlock the phone and never
/// handles a passcode: it waits (`Effect::AwaitUnlock`) and, once unlocked, observes again
/// before doing anything.
///
/// `Observed` is not boxed: one event moves per step, and a plain payload keeps the type
/// simple to mirror across FFI.
///
/// `Debug` never shows the owner's answer text (`UserAnswer`), only its length.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Begin the task.
    Start,
    /// Result of `Effect::Observe`: a typed tree (Jarvis's own runner).
    Observed(Observation),
    /// Result of `Effect::Observe` whose tree was parsed from the third-party runner's text
    /// format. That format is ambiguous and forgeable by on-screen text, so the session
    /// treats such a screen as unverified: every state-changing runner action on it asks,
    /// with a warning (see `jarvis_safety::ScreenSource`). Answers `Observe` exactly like
    /// `Observed` otherwise.
    ObservedRunnerText(Observation),
    /// Result of `Effect::Infer`.
    ModelOutput(ModelOutput),
    /// `Effect::Infer` failed in the runtime.
    ModelFailed {
        /// Error text.
        error: String,
    },
    /// Result of `Effect::Act`.
    ActResult(RunnerResult),
    /// Result of `Effect::CallTool`.
    ToolResult(ToolResult),
    /// The user's answer to `Effect::Confirm`.
    UserDecision {
        /// True to proceed.
        approved: bool,
    },
    /// The user's answer to `Effect::AskUser`.
    UserAnswer {
        /// Answer text.
        text: String,
    },
    /// The user stopped the task (kill switch).
    Cancel,
    /// The shell learned that the phone locked (lock notification, runner `screen_locked`).
    /// Valid in every running state: whatever was outstanding is set aside and the session
    /// waits for the owner to unlock (`Effect::AwaitUnlock`).
    DeviceLocked,
    /// The owner unlocked the phone. Answers `Effect::AwaitUnlock`; the session observes
    /// again before acting. Sent while the session was not waiting, it is treated as a lock
    /// the session missed, immediately followed by the unlock.
    DeviceUnlocked,
    /// Answers `Effect::AwaitUnlock` when the phone is still locked: the shell's timer
    /// fired. The session re-emits `AwaitUnlock` with the updated wait, or finishes
    /// `Interrupted` once `Policy::max_locked_wait_ms` has passed.
    Tick,
    /// `Effect::Observe` could not produce a snapshot even after the shell's own retries
    /// (not because the phone is locked: send `DeviceLocked` or a locked observation for
    /// that). Counts towards `Policy::max_consecutive_failures`; the session asks to observe
    /// again until that budget is used up, then finishes `Failed`.
    ObserveFailed {
        /// What went wrong, for the log.
        reason: String,
    },
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Start => f.write_str("Start"),
            Event::Observed(o) => f.debug_tuple("Observed").field(o).finish(),
            Event::ObservedRunnerText(o) => f.debug_tuple("ObservedRunnerText").field(o).finish(),
            Event::ModelOutput(o) => f.debug_tuple("ModelOutput").field(o).finish(),
            Event::ModelFailed { error } => {
                f.debug_struct("ModelFailed").field("error", error).finish()
            }
            Event::ActResult(r) => f.debug_tuple("ActResult").field(r).finish(),
            Event::ToolResult(r) => f.debug_tuple("ToolResult").field(r).finish(),
            Event::UserDecision { approved } => f
                .debug_struct("UserDecision")
                .field("approved", approved)
                .finish(),
            Event::UserAnswer { text } => f
                .debug_struct("UserAnswer")
                .field("text_chars", &text.chars().count())
                .finish_non_exhaustive(),
            Event::Cancel => f.write_str("Cancel"),
            Event::DeviceLocked => f.write_str("DeviceLocked"),
            Event::DeviceUnlocked => f.write_str("DeviceUnlocked"),
            Event::Tick => f.write_str("Tick"),
            Event::ObserveFailed { reason } => f
                .debug_struct("ObserveFailed")
                .field("reason", reason)
                .finish(),
        }
    }
}

impl Event {
    /// The snake_case event name, identical to the JSON `"type"` tag.
    pub fn name(&self) -> &'static str {
        match self {
            Event::Start => "start",
            Event::Observed(_) => "observed",
            Event::ObservedRunnerText(_) => "observed_runner_text",
            Event::ModelOutput(_) => "model_output",
            Event::ModelFailed { .. } => "model_failed",
            Event::ActResult(_) => "act_result",
            Event::ToolResult(_) => "tool_result",
            Event::UserDecision { .. } => "user_decision",
            Event::UserAnswer { .. } => "user_answer",
            Event::Cancel => "cancel",
            Event::DeviceLocked => "device_locked",
            Event::DeviceUnlocked => "device_unlocked",
            Event::Tick => "tick",
            Event::ObserveFailed { .. } => "observe_failed",
        }
    }
}

/// An [`Event`] stamped with the shell's monotonic clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedEvent {
    /// Monotonic milliseconds from the shell; the core never reads a clock.
    pub now_ms: u64,
    /// The event.
    pub event: Event,
}

impl TimedEvent {
    /// Stamps an event.
    pub fn new(now_ms: u64, event: Event) -> Self {
        Self { now_ms, event }
    }
}

/// What the core asks the user to approve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmRequest {
    /// Why confirmation is needed.
    pub category: RiskCategory,
    /// Human-readable description of the pending action.
    pub summary: String,
    /// Bundle id (or tool app) the action affects.
    pub app: String,
}

/// Final status of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    /// The task was completed.
    Success,
    /// The agent gave up or hit an error or budget.
    Failed,
    /// The user cancelled.
    Cancelled,
    /// Safety policy (deny list or declined confirmation) stopped the task.
    Blocked,
    /// The phone became unusable for the agent (it stayed locked longer than
    /// `Policy::max_locked_wait_ms`). Nothing is known to be wrong with the task or a learned
    /// flow; running it again once the phone is unlocked is reasonable.
    Interrupted,
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// Final status.
    pub status: OutcomeStatus,
    /// Human-readable summary.
    pub summary: String,
    /// Steps taken.
    pub steps: u32,
    /// Active time from `Start`, in milliseconds: time spent waiting for the owner to
    /// unlock the phone is not included (see `Progress::locked_total_ms`).
    pub duration_ms: u64,
}

/// One output of the session: the single thing the shell must do next.
///
/// JSON is internally tagged with `"type"`. Because [`RunnerCommand`] is itself tagged
/// with `"type"`, `Act` nests its command under a `"command"` key
/// (`{"type":"act","command":{"type":"tap","x":1.0,"y":2.0}}`); every other newtype
/// variant flattens its payload next to the tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Snapshot the UI tree, plus a screenshot when `screenshot` is true.
    Observe {
        /// Whether to capture a screenshot.
        screenshot: bool,
    },
    /// Run the model.
    Infer(ModelRequest),
    /// Send a command to the runner.
    Act(#[serde(with = "act_command")] RunnerCommand),
    /// Run a tool.
    CallTool(ToolCall),
    /// Ask the user to approve a risky action.
    Confirm(ConfirmRequest),
    /// Ask the user a question.
    AskUser {
        /// The question.
        question: String,
    },
    /// The session is over.
    Finished(Outcome),
    /// The phone is locked: show "Unlock to continue" (notification, Live Activity). The
    /// core never unlocks the phone or handles a passcode. Answer with `DeviceUnlocked` once
    /// the owner unlocks, `Tick` when a timer fires and it is still locked, or a fresh
    /// observation (`Observed`, locked or not). While waiting no model call, action or step
    /// is taken and no failure is counted.
    AwaitUnlock {
        /// Why the session is waiting, for the owner.
        reason: String,
        /// How long the session has waited so far, in milliseconds.
        waited_ms: u64,
    },
}

impl Effect {
    /// The snake_case effect name, identical to the JSON `"type"` tag.
    pub fn name(&self) -> &'static str {
        match self {
            Effect::Observe { .. } => "observe",
            Effect::Infer(_) => "infer",
            Effect::Act(_) => "act",
            Effect::CallTool(_) => "call_tool",
            Effect::Confirm(_) => "confirm",
            Effect::AskUser { .. } => "ask_user",
            Effect::Finished(_) => "finished",
            Effect::AwaitUnlock { .. } => "await_unlock",
        }
    }

    /// True for `Finished`.
    pub fn is_finished(&self) -> bool {
        matches!(self, Effect::Finished(_))
    }
}

/// Serializes `Effect::Act`'s command as `{"command": ...}` to avoid a duplicate `"type"` key.
mod act_command {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::runner::RunnerCommand;

    #[derive(Serialize)]
    struct Ref<'a> {
        command: &'a RunnerCommand,
    }

    #[derive(Deserialize)]
    struct Owned {
        command: RunnerCommand,
    }

    pub(super) fn serialize<S: Serializer>(c: &RunnerCommand, s: S) -> Result<S::Ok, S::Error> {
        Ref { command: c }.serialize(s)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<RunnerCommand, D::Error> {
        Owned::deserialize(d).map(|o| o.command)
    }
}

/// Coarse phase of the session loop, for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Created, `Start` not yet received.
    #[default]
    Idle,
    /// Waiting for an observation.
    Observing,
    /// Waiting for the model.
    Thinking,
    /// Waiting for the runner.
    Acting,
    /// Waiting for the post-action observation.
    Verifying,
    /// Waiting for a tool result.
    CallingTool,
    /// Waiting for the user to confirm or answer.
    WaitingForUser,
    /// Finished.
    Finished,
    /// Waiting for the owner to unlock the phone.
    WaitingForUnlock,
}

/// Progress snapshot read from the session (`Session::progress`) for the UI.
///
/// Not named as a proto type in the spec; placed here so `jarvis-agent`, `jarvis-ffi` and
/// `jarvis-sim` share one definition.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Progress {
    /// Steps taken so far.
    pub step: u32,
    /// Current phase.
    pub phase: Phase,
    /// Most recent action chosen.
    #[serde(default)]
    pub last_action: Option<Action>,
    /// Bundle id of the foreground app at the last observation.
    #[serde(default)]
    pub app: Option<String>,
    /// Model requests sent so far (retries included).
    #[serde(default)]
    pub model_calls: u32,
    /// Re-inferences after unparsable model output so far.
    #[serde(default)]
    pub parse_retries: u32,
    /// Whether the session is waiting for the owner to unlock the phone.
    #[serde(default)]
    pub waiting_for_unlock: bool,
    /// How long the current wait for an unlock has lasted, in milliseconds (0 when not
    /// waiting), as of the last event.
    #[serde(default)]
    pub locked_wait_ms: u64,
    /// Total time this session has spent waiting for unlocks, in milliseconds.
    #[serde(default)]
    pub locked_total_ms: u64,
}
