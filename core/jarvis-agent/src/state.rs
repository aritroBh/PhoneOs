//! The session's explicit states and the data each one waits with.

use std::collections::VecDeque;
use std::sync::Arc;

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

/// Which part of a model step a [`Pending`] carries out. A `type … submit` is two parts
/// (sixth review): the typing, gated as a plain `type`, and, once the typed text checks out,
/// pressing Return in that field, gated on the screen as it is then as its own action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    /// The whole action (everything but a `type … submit`).
    Whole,
    /// The typing of a `type … submit`, without Return.
    Typing,
    /// Return after the typing of a `type … submit`, gated on the post-typing screen.
    Return,
}

/// An action that passed parsing, validation and safety, on its way to being executed.
/// `Debug` leaves out the runner commands (they carry substituted profile values) and the
/// screens.
#[derive(Clone, PartialEq)]
pub(crate) struct Pending {
    /// The action as decided: placeholders kept, `open_app` resolved to a bundle id. This is
    /// what history, flows and the log see (for both parts of a `type … submit`).
    pub action: Action,
    /// What was gated and what an approval is bound to (placeholders kept): `action`, but
    /// for the typing part of a `type … submit` the same `type` without `submit`, and for
    /// its Return part a `type` with `submit` naming the field on the post-typing screen
    /// (`bound`), whose text is already typed.
    pub gate: Action,
    /// Which part of the model step this is.
    pub stage: Stage,
    /// Who chose it.
    pub source: Source,
    /// The screen the model step was chosen on (original, unsanitised; for verification and
    /// recording).
    pub before: Arc<CompactScreen>,
    /// The prompt-safe copy of `before` (for describing what changed in the history).
    pub before_safe: Arc<CompactScreen>,
    /// The screen `gate` was gated on, which an approval is bound to: `before`, but the
    /// post-typing screen for the Return part of a `type … submit`.
    pub bound: Arc<CompactScreen>,
    /// The prompt-safe copy of `bound` (for the note when an approval no longer holds).
    pub bound_safe: Arc<CompactScreen>,
    /// The exact digest of the raw tree `bound` was compacted from: an approval holds only
    /// on a fresh look with the same digest.
    pub before_digest: jarvis_observe::ScreenDigest,
    /// The raw node (index in the digest's node order) the target was listed from, if the
    /// action has a target: an approval holds only on the same node at the same position.
    pub before_target_source: Option<u32>,
    /// Prompt-safe description for the history.
    pub described: String,
    /// Verification must see a change for the step to count as a success.
    pub expect_change: bool,
    /// How to carry it out.
    pub exec: Exec,
    /// For `type`: the check that the text arrived, run on the next observation.
    pub check: Option<TypeCheck>,
    /// Fifteenth review (R1): the typing part of a `type … submit` was asked as one question
    /// that also said what its Return does — that question's category and whether it was
    /// phone only. Its approval covers the Return of the same plan, unless the Return asks
    /// for more on the screen after the typing (a riskier category, or phone only when the
    /// typing was not).
    pub covers_return: Option<(jarvis_proto::RiskCategory, bool, String)>,
    /// Sixteenth review (Rule T): the flow this action's question asks about — contexts read
    /// in this task that the owner has not allowed into the one the input goes to. Its note
    /// is part of the question (and of the summary an approval is checked against).
    pub flow: Option<FlowAsk>,
    /// Seventeenth review: the places this action's input goes to (empty for an action that
    /// is no input: `wait`, `look`, the text of `done`/`fail`/`ask_user`). A recorded step
    /// stores them (`FlowStep::place`).
    pub places: Vec<jarvis_safety::FlowContext>,
}

/// Sixteenth review (Rule T): a flow an action's question asks the owner to allow.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlowAsk {
    /// Where the input goes (seventeenth review: every place it may reach — usually one).
    pub targets: Vec<jarvis_safety::FlowContext>,
    /// Twentieth review (R20-A): the destinations the question names (with more than one
    /// destination it names them instead of "here"; as many as fit): an approval allows the
    /// named sources into these only.
    pub dests: Vec<jarvis_safety::FlowContext>,
    /// Twenty-third review (R23): what an approval keeps, per named (destination, source)
    /// pair — an observe-kind pair and a pair of the action's own input are kept apart, so
    /// the two cannot be confused (twentieth review: one `Keep` per question).
    pub grants: Vec<FlowGrant>,
    /// The contexts read in this task (or held on the clipboard, for a paste) not yet
    /// allowed into `targets` that the question names: exactly what its approval allows.
    pub sources: Vec<jarvis_safety::FlowContext>,
    /// Seventeenth review: more sources were unallowed than the question could name; after
    /// its approval the action is decided again (the next question names the rest).
    pub partial: bool,
    /// The question's note naming them.
    pub note: jarvis_proto::QuestionRow,
    /// The question is asked for the flow alone (the action would otherwise run unasked):
    /// its decline blocks this flow only, never a risk category.
    pub only: bool,
}

/// Twentieth review (R20-A): what approving a flow question keeps. A kept approval never
/// covers a stronger kind of input than the one the owner was shown, and only the places the
/// question named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keep {
    /// Twenty-third review (R23): shown for a destination that only sees the step (a wait, a
    /// look, `press_home`, or a place on the screen besides the action's own destinations):
    /// an observe-only grant for the pair, for the task. It never covers a scroll or a tap.
    Observe,
    /// Shown for a scroll: a scroll-only grant for each named pair, for the task.
    Scroll,
    /// Shown for a full input with exactly one decidable destination: a full grant, for the
    /// task.
    Full,
    /// A swipe, an undecidable destination, or any other input with more than one own
    /// destination: the named pairs for this action only (a typing and its Return are one
    /// action).
    Action,
}

/// Twenty-third review (R23): one (destination, source) pair a flow question named, and
/// what its approval keeps for it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlowGrant {
    /// Where the input goes (or who sees it).
    pub dest: jarvis_safety::FlowContext,
    /// What was read.
    pub source: jarvis_safety::FlowContext,
    /// What approving keeps for this pair.
    pub keep: Keep,
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
    /// Whether the one corrective attempt (delete, type again) has been made.
    pub retried: bool,
    /// The field's value at the previous mismatching reading of this verification (`None`
    /// before the first): a correction is sent only when two readings in a row are equal.
    pub last: Option<String>,
    /// Extra looks taken so far for this verification (at most
    /// [`crate::session::MAX_TYPING_LOOKS`]); they are not steps.
    pub looks: u32,
}

impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let exec = match &self.exec {
            Exec::Runner(q) => format!("{} runner command(s)", q.len()),
            Exec::Tool(call) => format!("tool {}", call.name),
        };
        f.debug_struct("Pending")
            .field("action", &self.action.name())
            .field("stage", &self.stage)
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
            .field("retried", &self.retried)
            .field("looks", &self.looks)
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
        /// Which part of the model step was asked about (a declined Return leaves the text
        /// typed).
        stage: Stage,
        /// The flow the question asked about (Rule T), when it did.
        flow: Option<Box<FlowAsk>>,
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
    /// A fresh look after the owner approved an action: it runs only if the approval's
    /// four records hold (raw digest, compact content, the target's raw node, the identical
    /// summary; see [`ApprovalLook`] and `Session::recheck_approved`).
    Approved(Box<ApprovalLook>),
    /// One more look at the home screen's search results after typing, before the held-back
    /// Return (which opens the top hit) is pressed.
    Settle(Box<Settle>),
    /// Stale-screen guard: a fresh look before acting on a plan older than
    /// `Policy::max_plan_age_ms`; the action runs (through the full gate, on the fresh
    /// screen) only if the same app is in front and its target is the same control (see
    /// `Session::plan_holds`).
    Replan(Box<PlanBasis>),
    /// Fifteenth review (F5): the look after a typing's (or a clear's) focusing tap, before
    /// its `TypeText` (or Deletes): sent only if the planned field holds the focus (see
    /// `Session::after_focus_tap`).
    Focus(Box<FocusCheck>),
}

/// The rest of a typing's (or a clear's) commands, held after its focusing tap until a look
/// shows where the focus went.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FocusCheck {
    /// The commands still to send (the `TypeText`, or the Deletes, first).
    pub queue: VecDeque<RunnerCommand>,
    /// The action.
    pub pending: Box<Pending>,
}

/// A screen-touching action the model (or a flow) chose, held while the session looks
/// again because the observation it was planned on is older than `Policy::max_plan_age_ms`.
/// `Debug` shows the action's name only.
#[derive(Clone)]
pub(crate) struct PlanBasis {
    /// The action as chosen (element numbers of `seen`).
    pub action: Action,
    /// Who chose it.
    pub source: Source,
    /// The observation it was chosen on.
    pub seen: crate::session::Current,
}

impl PartialEq for PlanBasis {
    fn eq(&self, other: &Self) -> bool {
        self.action == other.action
            && self.source == other.source
            && self.seen.same_observation(&other.seen)
    }
}

impl std::fmt::Debug for PlanBasis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanBasis")
            .field("action", &self.action.name())
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

/// The look after an approval, with what it has used of its two retries: one activation
/// of the app the question was asked in (when the agent's own app or the home screen is in
/// front) and one settle wait before a last look. Neither is a step of the task.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ApprovalLook {
    /// The approved action, as planned on the screen it was asked about.
    pub pending: Box<Pending>,
    /// The risk confirmation that was approved (`None` when only a first-use approval was
    /// asked).
    pub approved: Option<ConfirmRequest>,
    /// The asked app was brought back to the front once.
    pub activated: bool,
    /// The one settle wait was taken.
    pub settled: bool,
}

/// A typed home-screen search whose Return (gated on its own) waits for the results to
/// settle.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Settle {
    /// The typing part (its text already typed and checked).
    pub pending: Box<Pending>,
    /// Fingerprint of the field the text went into (where Return is pressed).
    pub field: Option<u64>,
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
    /// `Effect::Act` is outstanding for the look after an approval: the activation of the
    /// app the question was asked in, or the settle wait. An observation follows either way.
    AwaitingRelook {
        /// The look it prepares.
        look: Box<ApprovalLook>,
    },
    /// `Effect::Act` is outstanding for the settle wait before another look at a typed field
    /// that did not match yet (see `Session::verify_typing`). A `Verify` observation follows.
    AwaitingTypeWait {
        /// The typing under verification (its check holds the readings so far).
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
        /// For a first-use approval: the app it approves (declining denies it for the
        /// session).
        approves_app: Option<String>,
        /// The risk confirmation still to ask once this one is approved.
        then: Option<ConfirmRequest>,
        /// Sixteenth review: when `then` is a first-use approval asked after the flow
        /// question (Rule T asks the flow first), the app it approves.
        then_app: Option<String>,
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
            State::AwaitingAct { .. }
            | State::AwaitingRelook { .. }
            | State::AwaitingTypeWait { .. } => "waiting for the runner",
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
            State::AwaitingTypeWait { .. } => Phase::Verifying,
            State::AwaitingObservation { .. } | State::AwaitingRelook { .. } => Phase::Observing,
            State::AwaitingModel { .. } => Phase::Thinking,
            State::AwaitingAct { .. } => Phase::Acting,
            State::AwaitingTool { .. } => Phase::CallingTool,
            State::AwaitingConfirm { .. } | State::AwaitingAnswer { .. } => Phase::WaitingForUser,
            State::AwaitingUnlock { .. } => Phase::WaitingForUnlock,
            State::Finished(_) => Phase::Finished,
        }
    }
}
