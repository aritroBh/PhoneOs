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
/// `Debug` leaves out the runner commands (they carry substituted profile values) and the
/// screens.
#[derive(Clone, PartialEq)]
pub(crate) struct Pending {
    /// The action as decided: placeholders kept, `open_app` resolved to a bundle id. This is
    /// what history, flows and the log see.
    pub action: Action,
    /// Who chose it.
    pub source: Source,
    /// The screen it was chosen on (original, unsanitised; for verification and recording).
    pub before: CompactScreen,
    /// The prompt-safe copy of `before` (for describing what changed in the history).
    pub before_safe: CompactScreen,
    /// Prompt-safe description for the history.
    pub described: String,
    /// Verification must see a change for the step to count as a success.
    pub expect_change: bool,
    /// How to carry it out.
    pub exec: Exec,
    /// For `type`: the check that the text arrived, run on the next observation.
    pub check: Option<TypeCheck>,
}

/// What a `type` action must leave in its field, checked after the text was typed.
/// `Debug` never shows the typed text or the field's contents.
#[derive(Clone, PartialEq)]
pub(crate) struct TypeCheck {
    /// What was typed (profile placeholders substituted).
    pub typed: String,
    /// Fingerprint of the target field on the screen the action was chosen on.
    pub field: Option<u64>,
    /// The field's value before typing.
    pub prior: Option<String>,
    /// The field's placeholder.
    pub placeholder: Option<String>,
    /// Commands held back until the text is confirmed (`PressKey Return` for `submit`).
    pub then: VecDeque<RunnerCommand>,
    /// Whether the one corrective attempt (delete, type again) has been made.
    pub retried: bool,
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let exec = match &self.exec {
            Exec::Runner(q) => format!("{} runner command(s)", q.len()),
            Exec::Tool(call) => format!("tool {}", call.name),
        };
        f.debug_struct("Pending")
            .field("action", &self.action.name())
            .field("source", &self.source)
            .field("described", &self.described)
            .field("expect_change", &self.expect_change)
            .field("exec", &exec)
            .field("check", &self.check)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for TypeCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeCheck")
            .field("typed_chars", &self.typed.chars().count())
            .field("field", &self.field)
            .field("held_back", &self.then.len())
            .field("retried", &self.retried)
            .finish_non_exhaustive()
    }
}

/// What the session does once a fresh observation is in after an unlock.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Resume {
    /// Decide on the fresh screen (model or learned flow), as after any observation.
    Decide,
    /// A confirmation was pending when the phone locked: ask it again when the fresh screen
    /// is the one it was asked about, otherwise drop it and decide.
    Reconsider(Box<Pending>),
}

/// The effect a lock interrupted, whose late reply is absorbed (once) while waiting.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Late {
    /// `Infer`: the output is discarded (the screen will be read again).
    Model,
    /// `Act`: the result is discarded (the action's outcome is unknown either way).
    Act,
    /// `CallTool`: tools do not need the screen, so a result that still arrives is recorded.
    Tool(Box<Pending>),
    /// `Confirm`: a decline is honoured, an approval is not carried across the lock.
    Decision {
        /// What was asked.
        request: ConfirmRequest,
        /// For a first-use approval: the app.
        approves_app: Option<String>,
        /// History description of the action.
        described: String,
        /// Who chose the action.
        source: Source,
        /// The action (for the log).
        action: Action,
    },
    /// `AskUser`: an answer that arrives is recorded.
    Answer {
        /// History description of the question.
        described: String,
    },
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
    /// The first screen after an unlock, with a confirmation that was pending when the
    /// phone locked (see [`Resume::Reconsider`]).
    Reconsider(Box<Pending>),
    /// A fresh look after the owner approved `Pending`: it runs only if its target is still
    /// the same control (re-planned on the fresh screen). The request is the risk
    /// confirmation that was approved (`None` when only a first-use approval was asked).
    Approved(Box<Pending>, Option<ConfirmRequest>),
    /// One more look at the home screen's search results after typing, before the held-back
    /// Return (which opens the top hit) is pressed.
    Settle(Box<Settle>),
}

/// A typed home-screen search whose held-back Return waits for the results to settle.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Settle {
    /// The typing action (its text already typed and checked).
    pub pending: Box<Pending>,
    /// The held-back commands (`PressKey Return`).
    pub then: VecDeque<RunnerCommand>,
    /// The result labels of the previous look.
    pub labels: Vec<String>,
    /// Looks taken so far for this Return.
    pub looks: u32,
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
        /// The outstanding command is the action's first: if the runner refuses it, nothing
        /// of the action has happened.
        fresh: bool,
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
        /// For a first-use approval: the app it approves (declining denies it for the
        /// session).
        approves_app: Option<String>,
        /// The risk confirmation still to ask once this one is approved.
        then: Option<ConfirmRequest>,
    },
    /// `Effect::AskUser` is outstanding.
    AwaitingAnswer {
        /// History description of the question.
        described: String,
    },
    /// `Effect::AwaitUnlock` is outstanding: the phone is locked.
    AwaitingUnlock {
        /// What to do after the fresh observation.
        resume: Resume,
        /// The interrupted effect, while its reply may still arrive.
        late: Option<Late>,
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
            State::AwaitingUnlock { .. } => "waiting for the phone to be unlocked",
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
            State::AwaitingUnlock { .. } => Phase::WaitingForUnlock,
            State::Finished(_) => Phase::Finished,
        }
    }
}
