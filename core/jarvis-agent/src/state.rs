//! The session's explicit states and the data each one waits with.

use std::collections::VecDeque;

use jarvis_proto::{
    Action, CompactScreen, ConfirmRequest, Outcome, Phase, RequestKind, RunnerCommand, ToolCall,
};

/// Who chose an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// The model.
    Model,
    /// A learned flow being replayed.
    Replay,
}

/// How a decided action will be carried out.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Exec {
    /// Runner commands, sent one per `Effect::Act`.
    Runner(VecDeque<RunnerCommand>),
    /// A tool call.
    Tool(ToolCall),
}

/// An action that passed parsing, validation and safety, on its way to being executed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pending {
    /// The action as decided: placeholders kept, `open_app` resolved to a bundle id. This is
    /// what history, flows and the log see.
    pub action: Action,
    /// Who chose it.
    pub source: Source,
    /// The screen it was chosen on (original, unsanitised; for verification and recording).
    pub before: CompactScreen,
    /// Prompt-safe description for the history.
    pub described: String,
    /// Verification must see a change for the step to count as a success.
    pub expect_change: bool,
    /// How to carry it out.
    pub exec: Exec,
}

/// Why the session is waiting for an observation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ObservePurpose {
    /// A fresh screen to decide the next action on.
    Decide,
    /// A screenshot for a vision request.
    Vision,
    /// The screen after executing `Pending`, to verify it.
    Verify(Box<Pending>),
}

/// The session state machine. Exactly one effect is outstanding in every non-final state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum State {
    /// Created; waiting for `Start`.
    Idle,
    /// `Effect::Observe` is outstanding.
    AwaitingObservation {
        /// What the observation is for.
        purpose: ObservePurpose,
    },
    /// `Effect::Infer` is outstanding.
    AwaitingModel {
        /// Kind of the request that was sent (retries reuse it).
        kind: RequestKind,
    },
    /// `Effect::Act` is outstanding; `queue` holds the commands still to send.
    AwaitingAct {
        /// Remaining commands of this action.
        queue: VecDeque<RunnerCommand>,
        /// The action being executed.
        pending: Box<Pending>,
    },
    /// `Effect::CallTool` is outstanding.
    AwaitingTool {
        /// The call being executed.
        pending: Box<Pending>,
    },
    /// `Effect::Confirm` is outstanding.
    AwaitingConfirm {
        /// The action waiting for approval.
        pending: Box<Pending>,
        /// What the user was asked.
        request: ConfirmRequest,
    },
    /// `Effect::AskUser` is outstanding.
    AwaitingAnswer {
        /// History description of the question.
        described: String,
    },
    /// Over; every further step returns this outcome.
    Finished(Outcome),
}

impl State {
    /// Short name for protocol-error messages.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            State::Idle => "not started",
            State::AwaitingObservation { .. } => "waiting for an observation",
            State::AwaitingModel { .. } => "waiting for the model",
            State::AwaitingAct { .. } => "waiting for the runner",
            State::AwaitingTool { .. } => "waiting for a tool result",
            State::AwaitingConfirm { .. } => "waiting for a confirmation",
            State::AwaitingAnswer { .. } => "waiting for an answer",
            State::Finished(_) => "finished",
        }
    }

    /// The UI phase.
    pub(crate) fn phase(&self) -> Phase {
        match self {
            State::Idle => Phase::Idle,
            State::AwaitingObservation {
                purpose: ObservePurpose::Verify(_),
            } => Phase::Verifying,
            State::AwaitingObservation { .. } => Phase::Observing,
            State::AwaitingModel { .. } => Phase::Thinking,
            State::AwaitingAct { .. } => Phase::Acting,
            State::AwaitingTool { .. } => Phase::CallingTool,
            State::AwaitingConfirm { .. } | State::AwaitingAnswer { .. } => Phase::WaitingForUser,
            State::Finished(_) => Phase::Finished,
        }
    }
}
