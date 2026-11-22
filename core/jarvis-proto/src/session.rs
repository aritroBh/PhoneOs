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
/// `Observed` is not boxed: one event moves per step, and a plain payload keeps the type
/// simple to mirror across FFI.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Begin the task.
    Start,
    /// Result of `Effect::Observe`.
    Observed(Observation),
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
    /// Elapsed time from `Start`, in milliseconds.
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
}
