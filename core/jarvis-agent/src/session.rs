//! The session: one task, driven by the shell one event at a time.
//!
//! `step` takes the current [`State`] out, dispatches to one handler per state, and every
//! handler leaves a new state behind with exactly one effect outstanding. An event that does
//! not belong to the current state finishes the session `Failed` with a `protocol error`
//! summary; `Cancel` is honoured in every unfinished state; once finished, every step returns
//! the same `Finished` effect.
//!
//! Budget accounting (what guarantees termination): every action taken from the model or a
//! learned flow counts one step before anything else happens to it, every model request is
//! preceded by a budget check, parse retries are capped per step, model failures are retried
//! at most [`MAX_MODEL_RETRIES`] times in a row (each counting as a failed action), and every
//! wait for the shell is bounded by the time budget, checked on each event.
//!
//! First-use approval: before the first state-changing action inside an app that is neither
//! approved in memory, approved earlier in this session, `Trusted`, SpringBoard nor the
//! agent's own app, the session asks `FirstUse` ("Let Jarvis act in Shop
//! (com.example.shop)?"). When the same action also needs a risk confirmation, that is asked
//! second, as its own prompt (a first Pay tap in a new app asks twice). A declined app is off
//! limits for the rest of the session: any later action inside it, `open_app` into it, or a
//! tool acting on it finishes the session `Blocked`. Learned-flow steps take exactly the same
//! path as model actions.
//!
//! Approvals are not acted on blind: the owner may answer long after the question, so once
//! the last confirmation of a runner action is approved the session observes again and runs
//! the action only if the four records of the approval hold on the fresh screen: the raw
//! tree's exact digest with every node's shown bit (A1, `jarvis_observe::screen_digest`: no
//! tolerance; a screen with live-updating text cannot be confirmed, fail closed), the
//! compact screen's content (A2), the target's raw node (A3) and the identical summary in
//! the same category (A4); the action is re-planned on the fresh screen, see
//! `recheck_approved`; otherwise the model is told it was not done and the action goes
//! through the full gate again.
//!
//! Screen source: an observation parsed from the third-party runner's text format
//! (`Event::ObservedRunnerText`) is unverified: every state-changing runner action on it
//! asks (at least `Unverified`, the summary starting with a fixed warning), learned flows
//! included (`jarvis_safety::ScreenSource`). Within one approval, and never as a step,
//! the look may bring the asked app back once (when the agent's own app or the home screen
//! is in front) and wait [`APPROVAL_SETTLE_MS`] once for a settling screen before a last
//! look: `Observe`, (`LaunchApp`, `Observe`), (`Wait`, `Observe`) (see `act_approved`).
//!
//! Skills: with `SessionConfig::skill` the prompt carries the owner's guidance, and a skill
//! with `confine_to_apps` refuses actions outside its apps (`jarvis_safety::check_confinement`;
//! refused like a deny-list hit, so a second attempt finishes `Blocked`). A skill only
//! narrows: deny list, trust levels, first-use approval and confirmations apply unchanged.
//!
//! Locked phone: a passcode-locked iPhone cannot be driven (black screenshots, dead taps, no
//! app brought forward) and no runner can enter the passcode. **The core never tries to
//! unlock the phone and never handles a passcode.** It learns of a lock from a locked
//! observation, a command refused with `RunnerFailure::DeviceLocked`, or
//! `Event::DeviceLocked`; it then emits `Effect::AwaitUnlock` (re-emitted on `Tick`) and
//! takes no model call, action, step or failure while waiting. The resume rule is
//! **re-observe before acting**: nothing decided before the lock is executed after it
//! without a fresh observation. Per state when the lock arrives:
//!
//! | Waiting for | What happens to it |
//! |---|---|
//! | an observation (decide / vision) | dropped; after the unlock the fresh screen is decided on |
//! | an observation to verify an action | the action ran but its result is unknown: the model is told, the step is not learned |
//! | the model | the request is dropped (a late reply is ignored) and made again on the fresh screen |
//! | the runner, nothing of the action done yet | the action is dropped unexecuted (its step refunded) and decided again |
//! | the runner, part of the action done | as for verification: result unknown, told, not learned |
//! | a tool | a result that still arrives while locked is recorded; otherwise its result is unknown |
//! | a confirmation (or the look after an approval, its activation or settle wait) | kept: asked again after the unlock when the screen is unchanged, else dropped; a decline sent while locked is honoured, an approval is not (the owner approves a screen they can see) |
//! | an answer | an answer sent while locked is recorded; otherwise the question is dropped and decided again |
//!
//! A learned-flow step is never carried across a lock: it is derived again from the fresh
//! screen, and a step whose result the lock made unknown ends the replay (the model takes
//! over). A session in which a lock left any action's result unknown never saves a learned
//! flow. Locked time does not count against `max_duration_ms`; waiting longer than
//! `max_locked_wait_ms` finishes `Interrupted`.
//!
//! Typed text is checked: after a `type`, the next observation must show the text in the
//! field (`crate::typing::check_typed`). On a mismatch the session deletes what arrived and
//! types once more; a second mismatch is reported to the model as a failed step. A `submit`
//! is pressed only after the text checks out.
//!
//! Small models need more than "ok": every step that worked says what visibly changed
//! (`crate::changes`), the same action repeated on the same control on a changing screen is
//! warned about and finally stopped ([`RepeatRun`]), and `done` is not taken as a success
//! right after the owner declined something or a step failed, with nothing working since.
//! A tap on the home screen launches what it hits, so the icon gets `open_app`'s checks
//! before the tap is sent (`crate::commands::home_launch`); a tap Jarvis cannot name (no
//! listed element, or the keyboard) is always asked, and no launch-by-Return happens while
//! a denied app is among Spotlight's results (looked at twice, see `on_settle`).

use std::collections::{BTreeMap, BTreeSet};

use jarvis_memory::{LogEntry, LogStep, MemoryStore, Next, RecordedStep, Recorder, Replayer};
use jarvis_observe::{compact_view, diff, marks, screen_digest, screen_hash, ScreenDigest};
use std::collections::VecDeque;

use jarvis_proto::{
    Action, AppInfo, CompactScreen, ConfirmRequest, Effect, Element, Event, ImageRef, Key,
    ModelRequest, Observation, Outcome, OutcomeStatus, Point, Progress, RequestKind, RiskCategory,
    RunnerCommand, SessionConfig, Size, TaskSpec, TimedEvent, ToolCall, ToolResult, ToolSpec,
};
use jarvis_safety::gestures::denormalize;
use jarvis_safety::{
    approval_context, check_app, check_app_name, check_confinement, evaluate_from, first_use_app,
    first_use_request, is_springboard, label_names_denied_app, redact, redact_screen,
    sanitize_screen_text, search_names_denied_app, summary_for, widget_names_denied_app, Budget,
    BudgetVerdict, Decision, ScreenSource, ScreenTree, Verdict,
};

use crate::changes::describe_change;
use crate::commands::{
    home_launch, is_widget, plan, resolve_app, should_change, AppTarget, HomeLaunch,
    MAX_CLEAR_DELETES,
};
use crate::history::{clean, describe, History, MAX_SHOWN};
use crate::parser::{parse_model_output, ParseError};
use crate::profile::{mask, sanitize_keys, sensitive_texts, substitute};
use crate::prompt::{self, PromptParts, ACTION_MAX_TOKENS};
use crate::schema::{action_gbnf, action_json_schema};
use crate::state::{
    ApprovalLook, Exec, Late, ObservePurpose, Pending, Resume, Settle, Source, State, TypeCheck,
};
use crate::tools::{check_call, relevant, signature, MAX_TOOLS};
use crate::typing::{check_typed, correction_deletes, TypedVerdict};

/// Fewer enabled interactive elements than this makes the tree "useless" (vision fallback).
const MIN_ACTIONABLE: usize = 2;
/// Identical (screen, action) pairs that count as stuck.
const STUCK_REPEATS: u32 = 3;
/// The same for `wait`. Waiting is the one action whose point is that the screen has not
/// changed yet (a page still loading, a sheet still animating), so it gets more room; the
/// step and time budgets still bound it.
const STUCK_WAIT_REPEATS: u32 = 8;
/// The same action on the same control, working each time, this many times in a row on a
/// changing screen draws the first warning ([`RepeatRun`]).
const REPEAT_WARN: u32 = 3;
/// ... and this many the second, last warning; one more proposal finishes `Failed`.
const REPEAT_FINAL: u32 = 5;
/// Two screens share a layout when at least this share of the smaller one's element
/// fingerprints is in the other (a counter or a typed value changing keeps the layout; a
/// new page of a wizard does not).
const SAME_LAYOUT: f64 = 0.75;
/// App deny-list hits after which the session is blocked.
const MAX_APP_DENIALS: u32 = 2;
/// Approved actions not carried out because the screen changed after the approval, after
/// which the session ends `Failed`: a screen that keeps changing must not keep the owner
/// answering prompts until the step budget runs out (a harmless step between prompts
/// resets the failure count, so the failure budget alone does not bound it).
pub const MAX_STALE_APPROVALS: u32 = 3;
/// The settle wait before the last look after an approval, when a look showed a screen
/// other than the approved one (a sheet still animating away, a keyboard dismissing, an app
/// switch finishing).
pub const APPROVAL_SETTLE_MS: u64 = 500;
/// Most looks at the home screen's search results before a held-back Return: it is pressed
/// once two looks in a row show the same results, and not at all if they keep changing.
pub const MAX_SETTLE_LOOKS: u32 = 3;
/// Characters of tool output or answers kept in the history.
const MAX_RESULT_CHARS: usize = 300;
/// Known app names listed when `open_app` names an unknown app.
const MAX_APPS_LISTED: usize = 12;
/// Times a failed model request is re-sent before the session fails.
pub const MAX_MODEL_RETRIES: u32 = 2;
/// Characters of the keyboard's Return-key label shown to the model.
const MAX_RETURN_KEY_CHARS: usize = 24;
/// Characters of typed text or field content quoted when typing did not arrive intact.
const MAX_TYPED_QUOTE: usize = 60;
/// `AwaitUnlock` reason while nothing needs the owner's attention after the unlock.
const UNLOCK_REASON: &str = "The phone is locked. Unlock it to let Jarvis continue.";
/// `AwaitUnlock` reason while a confirmation is held.
const UNLOCK_REASON_CONFIRM: &str =
    "The phone is locked. Unlock it to continue; Jarvis will ask for your approval again.";

/// The latest observation in three forms.
#[derive(Debug, Clone)]
struct Current {
    /// As compacted from the runner's tree (fingerprints, values; never shown to the model).
    screen: CompactScreen,
    /// Prompt-safe copy: profile values masked, secrets redacted, text sanitised.
    safe: CompactScreen,
    /// `screen` with the owner's profile values masked (nothing else changed): what
    /// confirmation summaries are built from.
    masked: CompactScreen,
    /// Per element of `screen` (by position), the raw node it was listed from, and the raw
    /// tree's shape (see `jarvis_observe::CompactView`): an approval pins its target's raw
    /// node; summaries prefer the target's own container.
    sources: Vec<u32>,
    parents: Vec<u32>,
    ends: Vec<u32>,
    /// Where the tree came from: a typed tree, or runner text (unverified, see
    /// `jarvis_safety::ScreenSource`).
    source: ScreenSource,
    /// `screen_hash` of `screen`.
    hash: u64,
    /// The exact digest of the raw tree `screen` was compacted from (see
    /// [`jarvis_observe::screen_digest`]): what an approval is checked against. Kept here,
    /// beside the screen, so no FFI-visible type carries it.
    digest: ScreenDigest,
    /// Screenshot handle, when this observation has one.
    screenshot: Option<ImageRef>,
    /// Whether anything has been observed yet.
    seen: bool,
}

impl Default for Current {
    fn default() -> Self {
        let empty = CompactScreen {
            app_bundle_id: String::new(),
            app_name: None,
            size: Size::new(0.0, 0.0),
            keyboard_visible: false,
            elements: Vec::new(),
            cues: Default::default(),
        };
        Self {
            screen: empty.clone(),
            masked: empty.clone(),
            safe: empty,
            sources: Vec::new(),
            parents: Vec::new(),
            ends: Vec::new(),
            source: ScreenSource::Typed,
            hash: 0,
            digest: ScreenDigest::default(),
            screenshot: None,
            seen: false,
        }
    }
}

/// A run of the same action on the same control, each one carried out and changing the
/// screen ("Add to Cart" tapped again and again, the same text typed into the same field).
///
/// The unchanged-screen stuck rule cannot see these loops because every step changes
/// something. The rule: an action counts when it is a tap, long press, clear, tap_point or
/// type (same text) chosen by the model, on the control with the same fingerprint, on a
/// screen with the same layout as the previous one ([`SAME_LAYOUT`]), and it worked.
/// Anything else in between (another action, a failure, a decline) ends the run. Scrolls
/// and swipes never count (scrolling a list is repetition by design; the end of a list
/// shows as "nothing changed"), and neither do adjusters: sliders, pickers, controls named
/// like a stepper or pager (`+`, `Increment`, `Next`, `Show more`...) and controls whose
/// own value changed (a stepper reaching a value). Typing different text, or into another
/// field, is a different action. At [`REPEAT_WARN`] the model is told what happened since
/// the first time, at [`REPEAT_FINAL`] it is warned that one more ends the task, and the
/// next proposal of it finishes `Failed` as stuck.
#[derive(Debug, Clone)]
struct RepeatRun {
    /// The action and its control (see [`repeat_key`]).
    key: String,
    /// Element fingerprints of the screen the last repetition was chosen on.
    layout: BTreeSet<u64>,
    /// The prompt-safe screen the first repetition was chosen on.
    first: CompactScreen,
    /// Repetitions carried out so far.
    count: u32,
    /// Warnings given (0, 1 or 2).
    warnings: u32,
}

/// Labels of controls that step a value or a page; pressing them repeatedly is their use.
const ADJUSTER_LABELS: &[&str] = &[
    "+",
    "-",
    "\u{2212}",
    "plus",
    "minus",
    "increment",
    "decrement",
    "increase",
    "decrease",
    "add one",
    "remove one",
    "next",
    "previous",
    "next page",
    "previous page",
    "page down",
    "page up",
    "more",
    "show more",
    "load more",
    "see more",
    "zoom in",
    "zoom out",
    "volume up",
    "volume down",
];

/// The run key of `action` on `screen`: action name, the target control's fingerprint and,
/// for `type`, the text and submit flag. `None` for actions that never count.
fn repeat_key(action: &Action, screen: &CompactScreen) -> Option<String> {
    let target = |i: u32| screen.get(i).filter(|e| !is_adjuster(e));
    match action {
        Action::Tap { element } | Action::LongPress { element, .. } | Action::Clear { element } => {
            target(*element).map(|e| format!("{}#{}", action.name(), e.fingerprint))
        }
        Action::Type {
            element,
            text,
            submit,
        } => {
            let field = match element {
                Some(i) => target(*i),
                None => screen.elements.iter().find(|e| e.focused),
            };
            field.map(|e| format!("type#{}#{submit}#{text}", e.fingerprint))
        }
        Action::TapPoint { x, y } => Some(format!("tap_point#{x:.3}#{y:.3}")),
        _ => None,
    }
}

/// Sliders, pickers and controls labelled like a stepper or pager.
fn is_adjuster(e: &jarvis_proto::Element) -> bool {
    if matches!(
        e.role,
        jarvis_proto::Role::Slider | jarvis_proto::Role::Picker
    ) {
        return true;
    }
    let label: String = e
        .label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    ADJUSTER_LABELS.contains(&label.as_str())
}

fn layout(screen: &CompactScreen) -> BTreeSet<u64> {
    screen.elements.iter().map(|e| e.fingerprint).collect()
}

/// Whether two element sets share a layout (see [`SAME_LAYOUT`]).
fn same_layout(a: &BTreeSet<u64>, b: &BTreeSet<u64>) -> bool {
    let smaller = a.len().min(b.len());
    if smaller == 0 {
        return a.is_empty() && b.is_empty();
    }
    let shared = a.intersection(b).count();
    // Element counts are far below 2^52, so the conversions are exact.
    #[allow(clippy::cast_precision_loss)]
    let share = shared as f64 / smaller as f64;
    share >= SAME_LAYOUT
}

/// What `prepare` made of a proposed action.
enum Prepared {
    /// Allowed: execute now.
    Execute(Pending),
    /// Needs the owner's approval first.
    Confirm(Pending, ConfirmRequest),
    /// Needs the owner to approve acting in `app` first, then (if `then`) the risk
    /// confirmation.
    FirstUse {
        /// The action.
        pending: Pending,
        /// The app to approve (remembered for the session when approved); `None` for a
        /// one-off approval that names no app (a home-screen tap on an unknown icon).
        app: Option<String>,
        /// The `FirstUse` request.
        request: ConfirmRequest,
        /// The risk confirmation that follows an approval.
        then: Option<ConfirmRequest>,
    },
    /// Ask the owner a question.
    Ask {
        /// Cleaned question for the shell.
        question: String,
        /// History description.
        described: String,
    },
    /// Take a screenshot step.
    Look,
}

/// Why `prepare` refused a proposed action.
enum Reject {
    /// Not executed; the model is told and it counts as a failure.
    Failed(String),
    /// The action targets a deny-listed app.
    AppDenied(String),
    /// The action targets an app the owner declined in this session: finish `Blocked`.
    Blocked(String),
}

/// One task's agent loop. See the module docs and `ARCHITECTURE.md`.
///
/// `Debug` is safe for logs: it shows the task id, the state, counters and the memory store
/// (whose profile hides sensitive values), never screens, history, typed text or the task
/// text.
#[derive(Clone)]
pub struct Session {
    config: SessionConfig,
    task: TaskSpec,
    memory: MemoryStore,
    state: State,
    budget: Budget,
    started: bool,
    started_ms: u64,
    history: History,
    notes: Vec<String>,
    correction: Option<String>,
    current: Current,
    recorder: Recorder,
    replay: Option<Replayer>,
    replayed_flow: Option<String>,
    replay_diverged: bool,
    declined: BTreeSet<RiskCategory>,
    app_denials: u32,
    repeats: BTreeMap<(u64, String), u32>,
    stuck_hinted: bool,
    vision_hash: Option<u64>,
    last_request_vision: bool,
    parse_retries: u32,
    total_parse_retries: u32,
    model_calls: u32,
    model_failures: u32,
    last_request: Option<ModelRequest>,
    approved_apps: BTreeSet<String>,
    declined_apps: BTreeSet<String>,
    last_action: Option<Action>,
    log_steps: Vec<LogStep>,
    /// The owner's guidance block when the session runs a skill (built once).
    guidance: Option<String>,
    /// `known_apps` plus the named app rules whose prefix is a whole bundle id: the apps
    /// whose display names the session can resolve (icons, `open_app`, deny checks).
    apps: Vec<AppInfo>,
    /// Stuck-detection key of the most recently proposed action (to undo on a refund).
    last_repeat_key: Option<(u64, String)>,
    /// Observations in a row that failed (`Event::ObserveFailed`).
    observe_failures: u32,
    /// The current run of one repeated action on a changing screen.
    repeat: Option<RepeatRun>,
    /// What the owner declined, while no action has worked since (a `done` now is not a
    /// success).
    declined_open: Option<String>,
    /// The latest step's failure, while no step has worked since.
    last_failed: Option<String>,
    /// The one "reply was cut off" retry of this decision was used.
    truncation_retried: bool,
    /// The current wait for an unlock so far (forward time only).
    lock_wait_ms: u64,
    /// Clock reading the lock wait was last advanced to.
    lock_last_ms: u64,
    /// Every wait for an unlock in this session, summed.
    locked_total_ms: u64,
    /// Waits for an unlock in this session.
    lock_waits: u32,
    /// A lock left some action's result unknown: the recording is not trustworthy, so no
    /// learned flow is saved from this session.
    lock_uncertain: bool,
    /// Approved actions not carried out because the screen changed after the approval
    /// (see [`MAX_STALE_APPROVALS`]).
    stale_approvals: u32,
    /// Where the observation being handled came from (`Observed` or `ObservedRunnerText`).
    incoming_source: ScreenSource,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("task_id", &self.task.id)
            .field("state", &self.state.name())
            .field("steps", &self.budget.steps())
            .field("model_calls", &self.model_calls)
            .field("app", &self.current.screen.app_bundle_id)
            .field("declined", &self.declined)
            .field("approved_apps", &self.approved_apps)
            .field("stale_approvals", &self.stale_approvals)
            .field("declined_apps", &self.declined_apps)
            .field("replaying", &self.replay.is_some())
            .field("memory", &self.memory)
            .finish_non_exhaustive()
    }
}

/// How the session learned of a lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockSignal {
    /// `Event::DeviceLocked` (or `DeviceUnlocked` out of turn): the outstanding effect's
    /// reply may still arrive.
    Event,
    /// The reply itself said so: a locked observation or a refused command.
    Reply,
}

/// The display form of a duration: whole minutes from one minute on, else seconds.
fn minutes(ms: u64) -> String {
    if ms >= 60_000 {
        format!("{} min", ms / 60_000)
    } else {
        format!("{} s", ms / 1_000)
    }
}

/// `screen` with the owner's profile values masked back to placeholders (and secure fields'
/// values dropped); nothing else changed, so element numbers, frames and states match.
fn profile_masked(screen: &CompactScreen, memory: &MemoryStore) -> CompactScreen {
    let profile = &memory.profile;
    let mut masked = screen.clone();
    for e in &mut masked.elements {
        e.label = mask(&e.label, profile);
        e.value = if e.secure {
            None
        } else {
            e.value.as_deref().map(|v| mask(v, profile))
        };
        e.placeholder = e.placeholder.as_deref().map(|p| mask(p, profile));
    }
    masked.cues.return_key = masked.cues.return_key.as_deref().map(|k| mask(k, profile));
    masked
}

/// The prompt-safe copy of a screen: profile values masked back to placeholders, secrets
/// redacted (secure fields lose their value), every text sanitised. Fingerprints, frames
/// and indices are kept, so element numbers match the original.
fn prompt_safe(screen: &CompactScreen, memory: &MemoryStore) -> CompactScreen {
    let profile = &memory.profile;
    let masked = profile_masked(screen, memory);
    let mut safe = redact_screen(&masked);
    for e in &mut safe.elements {
        e.label = sanitize_screen_text(&e.label);
        e.value = e.value.as_deref().map(sanitize_screen_text);
        e.placeholder = e.placeholder.as_deref().map(sanitize_screen_text);
    }
    safe.app_name = safe.app_name.as_deref().map(|n| clean(n, 60));
    safe.app_bundle_id = clean(&safe.app_bundle_id, 100);
    // The keyboard's Return-key label is screen text too.
    safe.cues.return_key = safe
        .cues
        .return_key
        .as_deref()
        .map(|k| clean(&mask(k, profile), MAX_RETURN_KEY_CHARS))
        .filter(|k| !k.is_empty());
    safe
}

impl Session {
    /// Creates a session for `task`. Nothing happens until `step(Start)`.
    pub fn new(config: SessionConfig, task: TaskSpec, memory: MemoryStore) -> Session {
        let mut recorder = Recorder::new(task.text.clone());
        recorder.set_sensitive_values(sensitive_texts(&memory.profile));
        let budget = Budget::new(&config.policy, 0);
        let apps = named_apps(&config);
        let guidance = config
            .skill
            .as_ref()
            .and_then(|s| prompt::guidance(s, &config.known_apps));
        Session {
            config,
            task,
            memory,
            state: State::Idle,
            budget,
            started: false,
            started_ms: 0,
            history: History::default(),
            notes: Vec::new(),
            correction: None,
            current: Current::default(),
            recorder,
            replay: None,
            replayed_flow: None,
            replay_diverged: false,
            declined: BTreeSet::new(),
            app_denials: 0,
            repeats: BTreeMap::new(),
            stuck_hinted: false,
            vision_hash: None,
            last_request_vision: false,
            parse_retries: 0,
            total_parse_retries: 0,
            model_calls: 0,
            model_failures: 0,
            last_request: None,
            approved_apps: BTreeSet::new(),
            declined_apps: BTreeSet::new(),
            last_action: None,
            log_steps: Vec::new(),
            guidance,
            apps,
            last_repeat_key: None,
            observe_failures: 0,
            repeat: None,
            declined_open: None,
            last_failed: None,
            truncation_retried: false,
            lock_wait_ms: 0,
            lock_last_ms: 0,
            locked_total_ms: 0,
            lock_waits: 0,
            lock_uncertain: false,
            stale_approvals: 0,
            incoming_source: ScreenSource::Typed,
        }
    }

    /// Feeds one event and returns the one thing the shell must do next.
    pub fn step(&mut self, ev: TimedEvent) -> Effect {
        let TimedEvent { now_ms: now, event } = ev;
        // An observation parsed from runner text is handled like any other, but the screen
        // it carries is marked unverified (see `ingest`).
        let event = match event {
            Event::ObservedRunnerText(obs) => {
                self.incoming_source = ScreenSource::RunnerText;
                Event::Observed(obs)
            }
            Event::Observed(obs) => {
                self.incoming_source = ScreenSource::Typed;
                Event::Observed(obs)
            }
            other => other,
        };
        let state = std::mem::replace(&mut self.state, State::Idle);
        let state = match state {
            State::Finished(outcome) => {
                self.state = State::Finished(outcome.clone());
                return Effect::Finished(outcome);
            }
            other => other,
        };
        let locked = matches!(state, State::AwaitingUnlock { .. });
        if self.started && locked {
            // Waiting for an unlock is not active time: the budget skips it, the lock
            // clock counts it (forward steps only, like the budget).
            self.budget.skip(now);
            let delta = now.saturating_sub(self.lock_last_ms);
            self.lock_wait_ms = self.lock_wait_ms.saturating_add(delta);
            self.locked_total_ms = self.locked_total_ms.saturating_add(delta);
            self.lock_last_ms = now;
        }
        if matches!(event, Event::Cancel) {
            return self.finish(now, OutcomeStatus::Cancelled, "Cancelled by the owner.");
        }
        if self.started && !locked {
            self.budget.observe(now);
            if self.budget.elapsed_ms(now) >= self.config.policy.max_duration_ms {
                return self.finish(now, OutcomeStatus::Failed, "Time budget used up.");
            }
        }
        let state_name = state.name();
        if self.started && !locked {
            match event {
                Event::DeviceLocked => return self.interrupt(now, state, LockSignal::Event, false),
                Event::DeviceUnlocked => {
                    return self.interrupt(now, state, LockSignal::Event, true)
                }
                _ => {}
            }
        }
        match state {
            State::Idle => self.on_idle(now, event, state_name),
            State::AwaitingObservation { purpose } => {
                self.on_observation(now, purpose, event, state_name)
            }
            State::AwaitingModel { kind } => self.on_model(now, kind, event, state_name),
            State::AwaitingAct {
                queue,
                pending,
                fresh,
            } => self.on_act(now, queue, pending, fresh, event, state_name),
            State::AwaitingRelook { look } => self.on_relook(now, look, event, state_name),
            State::AwaitingTool { pending } => self.on_tool(now, pending, event, state_name),
            State::AwaitingConfirm {
                pending,
                request,
                approves_app,
                then,
            } => self.on_confirm(now, pending, request, approves_app, then, event, state_name),
            State::AwaitingAnswer { described } => {
                self.on_answer(now, described, event, state_name)
            }
            State::AwaitingUnlock { resume, late } => {
                self.on_locked(now, resume, late, event, state_name)
            }
            State::Finished(outcome) => {
                self.state = State::Finished(outcome.clone());
                Effect::Finished(outcome)
            }
        }
    }

    /// Progress for the UI.
    pub fn progress(&self) -> Progress {
        Progress {
            step: self.budget.steps(),
            phase: self.state.phase(),
            last_action: self.last_action.clone(),
            app: self
                .current
                .seen
                .then(|| self.current.screen.app_bundle_id.clone()),
            model_calls: self.model_calls,
            parse_retries: self.total_parse_retries,
            waiting_for_unlock: matches!(self.state, State::AwaitingUnlock { .. }),
            locked_wait_ms: if matches!(self.state, State::AwaitingUnlock { .. }) {
                self.lock_wait_ms
            } else {
                0
            },
            locked_total_ms: self.locked_total_ms,
        }
    }

    /// The memory store, updated with the recorded flow, flow statistics, the task log entry
    /// and the apps approved in this session once the session has finished (an unfinished
    /// session adds nothing).
    pub fn into_memory(self) -> MemoryStore {
        self.memory
    }

    // ---------------------------------------------------------------------------------
    // Per-state handlers.
    // ---------------------------------------------------------------------------------

    fn protocol_error(&mut self, now: u64, event: &Event, state: &str) -> Effect {
        let summary = format!("protocol error: unexpected {} while {state}", event.name());
        self.finish(now, OutcomeStatus::Failed, &summary)
    }

    fn on_idle(&mut self, now: u64, event: Event, state: &str) -> Effect {
        match event {
            Event::Start => {
                self.started = true;
                self.started_ms = now;
                self.budget = Budget::new(&self.config.policy, now);
                if let Some((flow, bindings)) = self.memory.flows.find(&self.task.text) {
                    self.replayed_flow = Some(flow.id.clone());
                    self.replay = Some(Replayer::new(flow, bindings));
                }
                self.observe(ObservePurpose::Decide, false)
            }
            other => self.protocol_error(now, &other, state),
        }
    }

    fn on_observation(
        &mut self,
        now: u64,
        purpose: ObservePurpose,
        event: Event,
        state: &str,
    ) -> Effect {
        let obs = match event {
            Event::Observed(obs) => obs,
            Event::ObserveFailed { reason } => return self.observe_failed(now, purpose, &reason),
            other => return self.protocol_error(now, &other, state),
        };
        if obs.device_locked {
            let state = State::AwaitingObservation { purpose };
            return self.interrupt(now, state, LockSignal::Reply, false);
        }
        self.observe_failures = 0;
        self.ingest(obs);
        let app = self.current.screen.app_bundle_id.clone();
        if check_app(&self.config.policy, &app).is_denied() {
            let summary = format!(
                "Stopped: {} is on the deny list, so Jarvis will not operate it.",
                clean(&app, 100)
            );
            return self.finish(now, OutcomeStatus::Blocked, &summary);
        }
        match purpose {
            ObservePurpose::Decide => self.decide(now),
            ObservePurpose::Vision => self.infer(RequestKind::Vision),
            ObservePurpose::Verify(pending) => self.verify(now, *pending),
            ObservePurpose::Reconsider(pending) => self.reconsider(now, *pending),
            ObservePurpose::Approved(look) => self.act_approved(now, *look),
            ObservePurpose::Settle(settle) => self.on_settle(now, *settle),
        }
    }

    /// `ObserveFailed`: observe again until the failure budget is used up.
    fn observe_failed(&mut self, now: u64, purpose: ObservePurpose, reason: &str) -> Effect {
        self.observe_failures = self.observe_failures.saturating_add(1);
        if self.observe_failures >= self.config.policy.max_consecutive_failures.max(1) {
            let summary = format!(
                "Could not read the screen ({} attempts in a row): {}",
                self.observe_failures,
                clean(reason, 160)
            );
            return self.finish(now, OutcomeStatus::Failed, &summary);
        }
        let screenshot = matches!(purpose, ObservePurpose::Vision);
        self.observe(purpose, screenshot)
    }

    fn on_model(&mut self, now: u64, kind: RequestKind, event: Event, state: &str) -> Effect {
        match event {
            Event::ModelOutput(out) => match parse_model_output(&out.text) {
                Ok(parsed) => {
                    self.model_failures = 0;
                    self.parse_retries = 0;
                    self.truncation_retried = false;
                    self.correction = None;
                    self.notes.clear();
                    self.propose(now, parsed.action, Source::Model)
                }
                Err(ParseError::Truncated) if !self.truncation_retried => {
                    // Cut off at the token cap: almost always a rambling thought. One extra
                    // retry asks for a shorter one; it does not use up a parse retry.
                    self.model_failures = 0;
                    self.truncation_retried = true;
                    self.total_parse_retries = self.total_parse_retries.saturating_add(1);
                    self.correction = Some(prompt::truncated_note());
                    self.infer(kind)
                }
                Err(e) if self.parse_retries < self.config.policy.max_parse_retries => {
                    self.model_failures = 0;
                    self.parse_retries = self.parse_retries.saturating_add(1);
                    self.total_parse_retries = self.total_parse_retries.saturating_add(1);
                    self.correction = Some(prompt::correction(&e));
                    self.infer(kind)
                }
                Err(e) => {
                    let summary = format!("The model's reply could not be understood: {e}.");
                    self.finish(now, OutcomeStatus::Failed, &summary)
                }
            },
            Event::ModelFailed { error } => {
                // The runtime may fail transiently (model still loading, memory pressure):
                // re-send the same request a bounded number of times.
                self.model_failures = self.model_failures.saturating_add(1);
                self.budget.record_failure();
                let retry = self
                    .last_request
                    .clone()
                    .filter(|_| self.model_failures <= MAX_MODEL_RETRIES)
                    .filter(|_| self.budget.check(now) == BudgetVerdict::Ok);
                match retry {
                    Some(request) => {
                        self.model_calls = self.model_calls.saturating_add(1);
                        self.state = State::AwaitingModel { kind };
                        Effect::Infer(request)
                    }
                    None => {
                        let summary = format!("Model error: {}", clean(&error, 200));
                        self.finish(now, OutcomeStatus::Failed, &summary)
                    }
                }
            }
            other => self.protocol_error(now, &other, state),
        }
    }

    fn on_act(
        &mut self,
        now: u64,
        mut queue: VecDeque<RunnerCommand>,
        pending: Box<Pending>,
        fresh: bool,
        event: Event,
        state: &str,
    ) -> Effect {
        let Event::ActResult(result) = event else {
            return self.protocol_error(now, &event, state);
        };
        if result.is_device_locked() {
            let state = State::AwaitingAct {
                queue,
                pending,
                fresh,
            };
            return self.interrupt(now, state, LockSignal::Reply, false);
        }
        if result.ok {
            return match queue.pop_front() {
                Some(next) => {
                    self.state = State::AwaitingAct {
                        queue,
                        pending,
                        fresh: false,
                    };
                    Effect::Act(next)
                }
                None => self.observe(ObservePurpose::Verify(pending), false),
            };
        }
        // A failed command aborts the rest of this action's commands.
        let error = result
            .error
            .as_deref()
            .unwrap_or("the runner could not do it");
        let msg = format!("failed: {}", clean(error, 120));
        self.record_failure(&pending, &msg);
        self.observe(ObservePurpose::Decide, false)
    }

    fn on_tool(&mut self, now: u64, pending: Box<Pending>, event: Event, state: &str) -> Effect {
        let Event::ToolResult(result) = event else {
            return self.protocol_error(now, &event, state);
        };
        self.record_tool(&pending, &result);
        self.observe(ObservePurpose::Decide, false)
    }

    /// Records a tool's result in the history, budget, log and recording.
    fn record_tool(&mut self, pending: &Pending, result: &ToolResult) {
        if result.ok {
            let out = self.safe_text(&result.output, MAX_RESULT_CHARS);
            let line = if out.is_empty() {
                prompt::RESULT_OK.to_owned()
            } else {
                format!("ok: {out}")
            };
            self.record_success(pending, &line);
        } else {
            let error = result
                .error
                .as_deref()
                .filter(|e| !e.trim().is_empty())
                .unwrap_or(&result.output);
            let msg = format!("failed: {}", self.safe_text(error, MAX_RESULT_CHARS));
            self.record_failure(pending, &msg);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_confirm(
        &mut self,
        now: u64,
        pending: Box<Pending>,
        request: ConfirmRequest,
        approves_app: Option<String>,
        then: Option<ConfirmRequest>,
        event: Event,
        state: &str,
    ) -> Effect {
        match event {
            Event::UserDecision { approved: true } => {
                if let Some(app) = approves_app {
                    self.approved_apps.insert(app);
                }
                match then {
                    Some(next) => self.ask_confirm(now, *pending, next, None, None),
                    None => self.after_approval(*pending, request),
                }
            }
            Event::UserDecision { approved: false } => {
                self.apply_decline(
                    &request,
                    approves_app,
                    pending.described.clone(),
                    &pending.action,
                    pending.source,
                );
                self.decide(now)
            }
            other => self.protocol_error(now, &other, state),
        }
    }

    /// Records the owner's "no": a declined app is off limits, a declined risk category
    /// finishes the session if asked again.
    fn apply_decline(
        &mut self,
        request: &ConfirmRequest,
        approves_app: Option<String>,
        described: String,
        action: &Action,
        source: Source,
    ) {
        self.repeat = None;
        self.declined_open = Some(format!("\"{}\"", clean(&request.summary, 120)));
        let result = match approves_app {
            Some(app) => {
                self.declined_apps.insert(app);
                prompt::RESULT_APP_DECLINED
            }
            None => {
                // A step-by-step confirmation of a harmless-looking action says nothing
                // about the next one; risky categories are remembered.
                if request.category != RiskCategory::None {
                    self.declined.insert(request.category);
                }
                prompt::RESULT_DECLINED
            }
        };
        self.history
            .push(described, result, source == Source::Replay);
        self.log_step(action, false);
        if source == Source::Replay {
            self.diverge("the owner declined a step");
        }
    }

    fn on_answer(&mut self, now: u64, described: String, event: Event, state: &str) -> Effect {
        let Event::UserAnswer { text } = event else {
            return self.protocol_error(now, &event, state);
        };
        self.record_answer(described, &text);
        self.observe(ObservePurpose::Decide, false)
    }

    /// Records the owner's answer to a question.
    fn record_answer(&mut self, described: String, text: &str) {
        let answer = prompt::neutralize_user_text(text, MAX_RESULT_CHARS);
        self.history
            .push(described, format!("owner answered: {answer}"), false);
        self.budget.record_success();
        let question = Action::AskUser {
            question: String::new(),
        };
        self.recorder.record(RecordedStep {
            action: question.clone(),
            element: None,
            screen: &self.current.screen,
        });
        self.log_step(&question, true);
    }

    // ---------------------------------------------------------------------------------
    // Locked phone (see the module docs for the per-state table).
    // ---------------------------------------------------------------------------------

    /// The phone locked while `state` was outstanding: set it aside and wait for the
    /// owner. With `unlocked` the unlock is already known (an out-of-turn
    /// `DeviceUnlocked`), so the session observes at once.
    fn interrupt(&mut self, now: u64, state: State, signal: LockSignal, unlocked: bool) -> Effect {
        let by_event = signal == LockSignal::Event;
        let (resume, late) = match state {
            State::AwaitingObservation { purpose } => match purpose {
                ObservePurpose::Decide | ObservePurpose::Vision => (Resume::Decide, None),
                ObservePurpose::Reconsider(pending) => (Resume::Reconsider(pending), None),
                ObservePurpose::Approved(look) => (self.approval_interrupted(look.pending), None),
                ObservePurpose::Verify(pending) => {
                    self.unknown_result(&pending, prompt::RESULT_LOCK_UNVERIFIED);
                    (Resume::Decide, None)
                }
                // The text was typed; its Return was not pressed and is not pressed after
                // the unlock.
                ObservePurpose::Settle(settle) => {
                    self.record_failure(&settle.pending, prompt::RESULT_RETURN_HELD);
                    (Resume::Decide, None)
                }
            },
            State::AwaitingModel { .. } => (Resume::Decide, by_event.then_some(Late::Model)),
            State::AwaitingAct { pending, fresh, .. } => {
                if fresh && !by_event {
                    // The runner refused the action's first command: nothing happened.
                    self.set_aside(&pending, prompt::RESULT_LOCK_NOT_DONE);
                    (Resume::Decide, None)
                } else {
                    self.unknown_result(&pending, prompt::RESULT_LOCK_UNKNOWN);
                    (Resume::Decide, by_event.then_some(Late::Act))
                }
            }
            // The activation or settle wait before a look after an approval: as a lock
            // during that look (neither is part of the action; its late result is dropped).
            State::AwaitingRelook { look } => (
                self.approval_interrupted(look.pending),
                by_event.then_some(Late::Act),
            ),
            State::AwaitingTool { pending } => (Resume::Decide, Some(Late::Tool(pending))),
            State::AwaitingConfirm {
                pending,
                request,
                approves_app,
                ..
            } => {
                let late = Late::Decision {
                    request,
                    approves_app,
                    described: pending.described.clone(),
                    source: pending.source,
                    action: pending.action.clone(),
                };
                if pending.source == Source::Replay {
                    // A flow step is derived again from the fresh screen.
                    self.set_aside(&pending, prompt::RESULT_LOCK_NOT_DONE);
                    (Resume::Decide, Some(late))
                } else {
                    (Resume::Reconsider(pending), Some(late))
                }
            }
            State::AwaitingAnswer { described } => {
                (Resume::Decide, Some(Late::Answer { described }))
            }
            // Not reached: `step` routes these elsewhere. Keep the session consistent.
            State::Idle | State::AwaitingUnlock { .. } | State::Finished(_) => {
                (Resume::Decide, None)
            }
        };
        self.lock_waits = self.lock_waits.saturating_add(1);
        self.lock_wait_ms = 0;
        self.lock_last_ms = now;
        if unlocked {
            self.end_wait(late);
            self.resume_observe(resume)
        } else {
            self.await_unlock(now, resume, late)
        }
    }

    /// Emits `AwaitUnlock`, or finishes `Interrupted` once the wait is too long.
    fn await_unlock(&mut self, now: u64, resume: Resume, late: Option<Late>) -> Effect {
        let max = self.config.policy.max_locked_wait_ms;
        if self.lock_wait_ms >= max {
            let summary = format!(
                "Stopped: the phone stayed locked for {} (the limit is {}). Nothing went wrong with the task; unlock the phone and run it again.",
                minutes(self.lock_wait_ms),
                minutes(max)
            );
            return self.finish(now, OutcomeStatus::Interrupted, &summary);
        }
        let reason = match resume {
            Resume::Reconsider(_) => UNLOCK_REASON_CONFIRM,
            Resume::Decide => UNLOCK_REASON,
        };
        self.state = State::AwaitingUnlock { resume, late };
        Effect::AwaitUnlock {
            reason: reason.to_owned(),
            waited_ms: self.lock_wait_ms,
        }
    }

    fn on_locked(
        &mut self,
        now: u64,
        mut resume: Resume,
        late: Option<Late>,
        event: Event,
        state: &str,
    ) -> Effect {
        match event {
            Event::Tick | Event::DeviceLocked | Event::ObserveFailed { .. } => {
                self.await_unlock(now, resume, late)
            }
            Event::Observed(obs) if obs.device_locked => self.await_unlock(now, resume, late),
            Event::DeviceUnlocked => {
                self.end_wait(late);
                self.resume_observe(resume)
            }
            Event::Observed(obs) => {
                // A fresh, unlocked observation: exactly what the resume waits for.
                self.end_wait(late);
                let purpose = match resume {
                    Resume::Decide => ObservePurpose::Decide,
                    Resume::Reconsider(pending) => ObservePurpose::Reconsider(pending),
                };
                self.on_observation(now, purpose, Event::Observed(obs), state)
            }
            Event::ModelOutput(_) | Event::ModelFailed { .. }
                if matches!(late, Some(Late::Model)) =>
            {
                self.await_unlock(now, resume, None)
            }
            Event::ActResult(_) if matches!(late, Some(Late::Act)) => {
                self.await_unlock(now, resume, None)
            }
            Event::ToolResult(result) => match late {
                Some(Late::Tool(pending)) => {
                    // Tools do not need the screen: the result stands.
                    self.record_tool(&pending, &result);
                    self.await_unlock(now, resume, None)
                }
                _ => self.protocol_error(now, &Event::ToolResult(result), state),
            },
            Event::UserDecision { approved } => match late {
                Some(Late::Decision {
                    request,
                    approves_app,
                    described,
                    source,
                    action,
                }) => {
                    // A "no" is always safe to honour. A "yes" is not carried across the
                    // lock: the owner approves a screen they can see, so it is asked again.
                    if !approved {
                        self.apply_decline(&request, approves_app, described, &action, source);
                        resume = Resume::Decide;
                    }
                    self.await_unlock(now, resume, None)
                }
                _ => self.protocol_error(now, &Event::UserDecision { approved }, state),
            },
            Event::UserAnswer { text } => match late {
                Some(Late::Answer { described }) => {
                    self.record_answer(described, &text);
                    self.await_unlock(now, resume, None)
                }
                _ => self.protocol_error(now, &Event::UserAnswer { text }, state),
            },
            other => self.protocol_error(now, &other, state),
        }
    }

    /// The wait is over: what the interrupted effect never answered is told to the model.
    fn end_wait(&mut self, late: Option<Late>) {
        self.lock_wait_ms = 0;
        match late {
            Some(Late::Tool(pending)) => {
                self.unknown_result(&pending, prompt::RESULT_LOCK_TOOL_UNKNOWN);
            }
            Some(Late::Answer { described }) => {
                self.history
                    .push(described, prompt::RESULT_LOCK_UNANSWERED, false);
            }
            Some(Late::Model | Late::Act | Late::Decision { .. }) | None => {}
        }
    }

    /// After an unlock: observe before anything else.
    fn resume_observe(&mut self, resume: Resume) -> Effect {
        match resume {
            Resume::Decide => self.observe(ObservePurpose::Decide, false),
            Resume::Reconsider(pending) => self.observe(ObservePurpose::Reconsider(pending), false),
        }
    }

    /// A confirmation that was pending when the phone locked, on the first screen after the
    /// unlock: asked again (through the full gate) when the screen is unchanged, else
    /// dropped and decided again.
    fn reconsider(&mut self, now: u64, pending: Pending) -> Effect {
        if screen_hash(&pending.before) == self.current.hash {
            let prepared = self.prepare(&pending.action, pending.source);
            return self.dispatch(now, &pending.action, pending.source, prepared);
        }
        self.set_aside(&pending, prompt::RESULT_LOCK_SCREEN_CHANGED);
        self.decide(now)
    }

    /// A lock during the look after an approval (or the activation or wait before it). An
    /// approval is never carried across a lock: asked again on the same screen, else
    /// decided again (a flow step is derived again).
    fn approval_interrupted(&mut self, pending: Box<Pending>) -> Resume {
        if pending.source == Source::Replay {
            self.set_aside(&pending, prompt::RESULT_LOCK_NOT_DONE);
            Resume::Decide
        } else {
            Resume::Reconsider(pending)
        }
    }

    /// The owner approved `pending` (the last of its confirmations). The answer may come
    /// long after the question, so nothing planned on the screen it was asked about is sent
    /// blind: tools (which do not use the screen) run now, runner actions look at the screen
    /// once more first ([`Self::act_approved`]).
    fn after_approval(&mut self, pending: Pending, request: ConfirmRequest) -> Effect {
        if matches!(pending.exec, Exec::Tool(_)) {
            return self.execute(pending);
        }
        let approved = (request.category != RiskCategory::FirstUse).then_some(request);
        let look = ApprovalLook {
            pending: Box::new(pending),
            approved,
            activated: false,
            settled: false,
        };
        self.observe(ObservePurpose::Approved(Box::new(look)), false)
    }

    /// A fresh look after an approval: run the approved action when the screen's record
    /// equals the approved one, re-planned on this screen ([`Self::recheck_approved`]).
    /// Otherwise, within one approval and never as a step of the task:
    ///
    /// - the agent's own app or the home screen in front while the question was asked in
    ///   another app (the owner answered in Jarvis, or came back via the home screen):
    ///   bring the asked app back once (`LaunchApp`, as `open_app` sends it for an approved
    ///   app; no gate, no first use) and look again;
    /// - the asked app (or, before the activation, the agent's own app or the home screen)
    ///   in front: wait [`APPROVAL_SETTLE_MS`] once (the screen may still be settling) and
    ///   look again;
    /// - else (another app in front, or both retries used): stale.
    ///
    /// So at most: look, [activate, look], wait, look. Nothing is acted on unless a look
    /// equals the record.
    fn act_approved(&mut self, now: u64, mut look: ApprovalLook) -> Effect {
        if let Some(fresh) = self.recheck_approved(&look.pending, look.approved.as_ref()) {
            return self.execute(fresh);
        }
        let asked = look.pending.before.app_bundle_id.trim().to_owned();
        let front = self.current.screen.app_bundle_id.trim().to_owned();
        let same_app = front.eq_ignore_ascii_case(&asked);
        let between = is_springboard(&front) || self.is_agent_app(&front);
        if !same_app && !between {
            return self.approval_stale(now, &look.pending);
        }
        if !same_app && !look.activated && !is_springboard(&asked) && !self.is_agent_app(&asked) {
            // The asked app passed the deny list when the question was asked (a denied app
            // in front ends the session); checked again before it is brought forward.
            if asked.is_empty()
                || check_app(&self.config.policy, &asked).is_denied()
                || self.app_declined(&asked)
            {
                return self.approval_stale(now, &look.pending);
            }
            look.activated = true;
            self.state = State::AwaitingRelook {
                look: Box::new(look),
            };
            return Effect::Act(RunnerCommand::LaunchApp { bundle_id: asked });
        }
        if !look.settled {
            look.settled = true;
            self.state = State::AwaitingRelook {
                look: Box::new(look),
            };
            return Effect::Act(RunnerCommand::Wait {
                ms: APPROVAL_SETTLE_MS,
            });
        }
        self.approval_stale(now, &look.pending)
    }

    /// The runner's reply to the activation or settle wait before a look after an approval:
    /// look (a failed command changes nothing: the look decides). A refusal because the
    /// phone is locked is a lock during that look.
    fn on_relook(
        &mut self,
        now: u64,
        look: Box<ApprovalLook>,
        event: Event,
        state: &str,
    ) -> Effect {
        let Event::ActResult(result) = event else {
            return self.protocol_error(now, &event, state);
        };
        if result.is_device_locked() {
            let state = State::AwaitingRelook { look };
            return self.interrupt(now, state, LockSignal::Reply, false);
        }
        self.observe(ObservePurpose::Approved(look), false)
    }

    /// An approved action not carried out because the screen changed after the approval:
    /// the model is told and it counts as a failed step (not refunded), so a screen that
    /// keeps changing cannot keep the session asking forever; after
    /// [`MAX_STALE_APPROVALS`] the session ends `Failed`. Otherwise decide again (through
    /// the full gate, which asks again if the action is proposed again).
    fn approval_stale(&mut self, now: u64, pending: &Pending) -> Effect {
        // What changed, in words (from the prompt-safe screens; cleaned and bounded).
        let change = approval_context(&pending.action, &pending.before_safe)
            .change_note(&pending.action, &self.current.safe)
            .unwrap_or_else(|| prompt::CHANGE_UNSEEN.to_owned());
        self.record_failure(pending, &prompt::approval_stale(&change));
        self.stale_approvals = self.stale_approvals.saturating_add(1);
        if self.stale_approvals >= MAX_STALE_APPROVALS {
            let summary = prompt::stale_approvals_summary(self.stale_approvals, &change);
            return self.finish(now, OutcomeStatus::Failed, &summary);
        }
        self.decide(now)
    }

    /// `pending` re-planned on the current screen, when it is still the action the owner
    /// approved: (A1) the exact digest of the fresh raw tree **equals** the one taken when
    /// the question was asked (`pending.before_digest`; see [`jarvis_observe::screen_digest`]:
    /// every node's role, texts, identifier, placeholder, states and shown bit; no
    /// tolerance, so live-updating text voids the approval); (A2) the compact screen's
    /// content is equal, frames only ignored, with the target at the same position
    /// (`jarvis_safety::ApprovalContext::holds_on`); (A3) that element comes from the same
    /// raw node (`pending.before_target_source`), so a twin cannot take its place; (A4) the
    /// gate on the fresh screen asks for nothing new: no denial, and the same category with
    /// the identical summary recomputed for this target ([`Self::risk_summary`]); the
    /// target's fresh frame contains the re-planned point; and on the home screen a launch
    /// that cannot be named still finds no denied app among the results. Frames are not
    /// compared, so a scroll that keeps every node's shown bit keeps the approval and the
    /// action is re-planned where its target is now. `None` otherwise.
    fn recheck_approved(
        &self,
        pending: &Pending,
        approved: Option<&ConfirmRequest>,
    ) -> Option<Pending> {
        let screen = &self.current.screen;
        let action = pending.action.clone();
        // A1: the raw digest; A2: the compact content (and the target's position, the app,
        // and the Return key for a submit); A3: the target is the same raw node.
        if self.current.digest != pending.before_digest
            || !approval_context(&action, &pending.before).holds_on(&action, screen)
            || self.target_source(&action) != pending.before_target_source
        {
            return None;
        }
        if is_springboard(&screen.app_bundle_id)
            && matches!(
                home_launch(&action, screen, &self.apps),
                Some(HomeLaunch::Unresolved { .. })
            )
            && self.denied_search_result().is_some()
        {
            return None;
        }
        let executed = match &action {
            Action::Type {
                element,
                text,
                submit,
            } => Action::Type {
                element: *element,
                text: substitute(text, &self.memory.profile).ok()?,
                submit: *submit,
            },
            other => other.clone(),
        };
        // A4 and the gate: nothing new is asked, and an approved risk is the same category
        // with the identical summary, recomputed for this target on this screen.
        match evaluate_from(
            &self.config.policy,
            &executed,
            screen,
            None,
            self.current.source,
        ) {
            Decision::Allow => {
                if approved.is_some() {
                    return None;
                }
            }
            Decision::Deny(_) => return None,
            Decision::Confirm(request) => {
                let same = approved.is_some_and(|a| {
                    a.category == request.category
                        && a.summary == self.risk_summary(&action, request.category)
                });
                if !same {
                    return None;
                }
            }
        }
        let queue = plan(&executed, screen).ok()?;
        let exec = Exec::Runner(queue);
        if let (Some(target), Some(p)) = (action_target(&action, screen), planned_point(&exec)) {
            if !target.frame.contains(p) {
                return None;
            }
        }
        let Exec::Runner(queue) = exec else {
            return None;
        };
        let (queue, check) = type_check(&executed, screen, queue);
        Some(Pending {
            expect_change: should_change(&action, screen),
            described: describe(&action, &self.current.safe),
            before_target_source: self.target_source(&action),
            action,
            source: pending.source,
            before: screen.clone(),
            before_safe: self.current.safe.clone(),
            before_digest: self.current.digest,
            exec: Exec::Runner(queue),
            check,
        })
    }

    /// An action dropped before any of it ran: its step is refunded (it will be decided
    /// again) and the model, if it chose it, is told why.
    fn set_aside(&mut self, pending: &Pending, msg: &str) {
        self.budget.refund_step();
        if let Some(key) = self.last_repeat_key.take() {
            if let Some(count) = self.repeats.get_mut(&key) {
                *count = count.saturating_sub(1);
            }
        }
        if pending.source == Source::Model {
            self.history.push(pending.described.clone(), msg, false);
        }
    }

    /// An action whose result the lock made unknown: told, not learned, not a failure; a
    /// flow step ends the replay.
    fn unknown_result(&mut self, pending: &Pending, msg: &str) {
        self.history.push(
            pending.described.clone(),
            msg,
            pending.source == Source::Replay,
        );
        self.log_step(&pending.action, false);
        self.lock_uncertain = true;
        if pending.source == Source::Replay {
            self.diverge("the phone locked during a saved step");
        }
    }

    // ---------------------------------------------------------------------------------
    // The loop: decide → propose → gate → execute → verify.
    // ---------------------------------------------------------------------------------

    fn observe(&mut self, purpose: ObservePurpose, screenshot: bool) -> Effect {
        self.state = State::AwaitingObservation { purpose };
        Effect::Observe { screenshot }
    }

    fn ingest(&mut self, obs: Observation) {
        // The digest is taken from the raw tree before compaction cuts or drops anything.
        let digest = screen_digest(&obs.screen);
        let view = compact_view(&obs.screen);
        let screen = view.screen;
        self.current = Current {
            hash: screen_hash(&screen),
            digest,
            safe: prompt_safe(&screen, &self.memory),
            masked: profile_masked(&screen, &self.memory),
            sources: view.sources,
            parents: view.parents,
            ends: view.ends,
            source: self.incoming_source,
            screen,
            screenshot: obs.screenshot,
            seen: true,
        };
    }

    /// The raw node (index in the digest's order) of `action`'s target on the current
    /// screen, if it has one.
    fn target_source(&self, action: &Action) -> Option<u32> {
        let target = action_target(action, &self.current.screen)?;
        let pos = usize::try_from(target.index).ok()?.checked_sub(1)?;
        self.current.sources.get(pos).copied()
    }

    /// The confirmation summary of `action` (as decided: placeholders kept) in `category`
    /// on the current screen: built from the profile-masked screen, with the raw tree for
    /// `under "…"` and the unverified-screen warning on runner text. The same function
    /// builds it when asked and when an approval is checked (A4: it must be identical).
    fn risk_summary(&self, action: &Action, category: RiskCategory) -> String {
        let tree = ScreenTree {
            sources: &self.current.sources,
            parents: &self.current.parents,
            ends: &self.current.ends,
        };
        summary_for(
            action,
            &self.current.masked,
            category,
            self.current.source,
            Some(&tree),
        )
    }

    /// Finishes when a budget is exhausted.
    fn budget_stop(&mut self, now: u64) -> Option<Effect> {
        let summary = match self.budget.check(now) {
            BudgetVerdict::Ok => return None,
            BudgetVerdict::StepsExceeded => {
                format!("Step budget used up ({} steps).", self.budget.steps())
            }
            BudgetVerdict::TimeExceeded => "Time budget used up.".to_owned(),
            BudgetVerdict::TooManyFailures => format!(
                "Gave up after {} failed actions in a row.",
                self.budget.consecutive_failures()
            ),
        };
        Some(self.finish(now, OutcomeStatus::Failed, &summary))
    }

    /// Chooses the next action on the current screen: learned flow, vision, or the model.
    fn decide(&mut self, now: u64) -> Effect {
        if let Some(stop) = self.budget_stop(now) {
            return stop;
        }
        if let Some(replayer) = &self.replay {
            match replayer.next(&self.current.screen) {
                Next::Action(action) => return self.propose(now, action, Source::Replay),
                Next::Complete => {
                    return self.finish(now, OutcomeStatus::Success, "Done (saved routine).")
                }
                Next::Diverged { reason } => self.diverge(&reason.to_string()),
            }
        }
        if self.wants_vision() {
            self.vision_hash = Some(self.current.hash);
            return self.observe(ObservePurpose::Vision, true);
        }
        self.infer(RequestKind::Text)
    }

    /// The tree is too thin to act on, the model can see, and this screen has not had a
    /// screenshot yet.
    fn wants_vision(&self) -> bool {
        let actionable = self
            .current
            .screen
            .elements
            .iter()
            .filter(|e| e.enabled && e.role.is_interactive())
            .count();
        self.config.model.supports_vision
            && actionable < MIN_ACTIONABLE
            && self.vision_hash != Some(self.current.hash)
    }

    /// Hands control from a learned flow to the model.
    fn diverge(&mut self, reason: &str) {
        if self.replay.take().is_some() {
            self.replay_diverged = true;
            let note = self.safe_text(&prompt::divergence_note(reason), prompt::MAX_NOTE_CHARS);
            self.notes.push(note);
        }
    }

    fn infer(&mut self, kind: RequestKind) -> Effect {
        let screenshot = match kind {
            RequestKind::Vision => self.current.screenshot.clone(),
            RequestKind::Text => None,
        };
        let vision = screenshot.is_some();
        let request = self.build_request(screenshot);
        self.last_request_vision = vision;
        self.model_calls = self.model_calls.saturating_add(1);
        self.last_request = Some(request.clone());
        self.state = State::AwaitingModel { kind: request.kind };
        Effect::Infer(request)
    }

    fn build_request(&self, screenshot: Option<ImageRef>) -> ModelRequest {
        let cur = &self.current;
        let tools: Vec<String> = relevant(
            &self.config.tools,
            &cur.screen.app_bundle_id,
            &self.task.text,
            MAX_TOOLS,
        )
        .into_iter()
        .map(|t| clean(&signature(t), 160))
        .collect();
        let mut notes = self.notes.clone();
        if let Some(c) = &self.correction {
            notes.push(c.clone());
        }
        let vision = screenshot.is_some();
        let parts = PromptParts {
            task: &self.task.text,
            tools,
            profile_keys: sanitize_keys(&self.memory.profile),
            history: self.history.recent(MAX_SHOWN),
            notes,
            screen: &cur.safe,
            vision,
            guidance: self.guidance.as_deref(),
        };
        let (system, user) = prompt::build(&parts, &self.config.model);
        let grammar = self.config.model.supports_grammar;
        ModelRequest {
            system,
            user,
            marks: if vision {
                marks(&cur.screen)
            } else {
                Vec::new()
            },
            image: screenshot,
            max_tokens: ACTION_MAX_TOKENS,
            grammar: grammar.then(action_gbnf),
            json_schema: grammar.then(action_json_schema),
            kind: if vision {
                RequestKind::Vision
            } else {
                RequestKind::Text
            },
        }
    }

    /// Takes one decided action (model or flow) through budget, stuck detection, safety and
    /// into execution. Every action counts one step first.
    fn propose(&mut self, now: u64, action: Action, source: Source) -> Effect {
        self.budget.record_step(now);
        self.last_action = Some(action.clone());
        match &action {
            Action::Done { summary } => {
                let s = if summary.trim().is_empty() {
                    "Done."
                } else {
                    summary
                };
                // The session does not judge whether a summary is true. It only refuses to
                // call it a success when nothing has worked since the owner said no, or
                // since the last step failed.
                if let Some(what) = self.declined_open.clone() {
                    let summary = format!(
                        "Not done: the owner declined {what}, and nothing was done after that. (The agent said: {})",
                        clean(s, 120)
                    );
                    return self.finish(now, OutcomeStatus::Blocked, &summary);
                }
                if let Some(failed) = self.last_failed.clone() {
                    let summary = format!(
                        "Not confirmed: the last step failed ({failed}), and nothing worked after it. (The agent said: {})",
                        clean(s, 120)
                    );
                    return self.finish(now, OutcomeStatus::Failed, &summary);
                }
                return self.finish(now, OutcomeStatus::Success, s);
            }
            Action::Fail { reason } => {
                let s = if reason.trim().is_empty() {
                    "The agent gave up."
                } else {
                    reason
                };
                return self.finish(now, OutcomeStatus::Failed, s);
            }
            _ => {}
        }
        if let Some(stop) = self.repeat_check(now, &action, source) {
            return stop;
        }
        self.last_repeat_key = None;
        if action.is_ui() || matches!(action, Action::Look) {
            let key = (
                self.current.hash,
                serde_json::to_string(&action).unwrap_or_default(),
            );
            self.last_repeat_key = Some(key.clone());
            let count = self.repeats.entry(key).or_insert(0);
            *count = count.saturating_add(1);
            let waiting = matches!(action, Action::Wait { .. });
            let limit = if waiting {
                STUCK_WAIT_REPEATS
            } else {
                STUCK_REPEATS
            };
            if *count >= limit {
                if self.stuck_hinted {
                    return self.finish(
                        now,
                        OutcomeStatus::Failed,
                        "Stuck: the same action kept being chosen on an unchanged screen.",
                    );
                }
                self.stuck_hinted = true;
                let described = describe(&action, &self.current.safe);
                self.last_failed = Some(format!("{described} -> {}", prompt::RESULT_STUCK));
                self.history
                    .push(described, prompt::RESULT_STUCK, source == Source::Replay);
                self.notes.push(
                    if waiting {
                        prompt::STUCK_WAIT_HINT
                    } else {
                        prompt::STUCK_HINT
                    }
                    .to_owned(),
                );
                self.budget.record_failure();
                self.log_step(&action, false);
                if source == Source::Replay {
                    self.diverge("a saved step repeated");
                }
                return self.decide(now);
            }
        }
        let prepared = self.prepare(&action, source);
        self.dispatch(now, &action, source, prepared)
    }

    /// Carries out what `prepare` decided for `action`.
    fn dispatch(
        &mut self,
        now: u64,
        action: &Action,
        source: Source,
        prepared: Result<Prepared, Reject>,
    ) -> Effect {
        match prepared {
            Ok(Prepared::Execute(pending)) => self.execute(pending),
            Ok(Prepared::Confirm(pending, request)) => {
                self.ask_confirm(now, pending, request, None, None)
            }
            Ok(Prepared::FirstUse {
                pending,
                app,
                request,
                then,
            }) => {
                if let Some(next) = then
                    .as_ref()
                    .filter(|n| self.declined.contains(&n.category))
                {
                    let summary = format!(
                        "Blocked: the owner already declined a {} action.",
                        category_name(next.category)
                    );
                    return self.finish(now, OutcomeStatus::Blocked, &summary);
                }
                self.ask_confirm(now, pending, request, app, then)
            }
            Ok(Prepared::Ask {
                question,
                described,
            }) => {
                self.state = State::AwaitingAnswer { described };
                Effect::AskUser { question }
            }
            Ok(Prepared::Look) => {
                let described = describe(action, &self.current.safe);
                if self.config.model.supports_vision {
                    self.history.push(described, prompt::RESULT_LOOKED, false);
                    self.vision_hash = Some(self.current.hash);
                    self.observe(ObservePurpose::Vision, true)
                } else {
                    self.history
                        .push(described, prompt::RESULT_NO_VISION, false);
                    self.decide(now)
                }
            }
            Err(Reject::AppDenied(msg)) => {
                self.app_denials = self.app_denials.saturating_add(1);
                if self.app_denials >= MAX_APP_DENIALS {
                    let summary = format!("Blocked: {msg}.");
                    return self.finish(now, OutcomeStatus::Blocked, &summary);
                }
                self.reject(now, action, source, &format!("not allowed: {msg}"))
            }
            Err(Reject::Failed(msg)) => self.reject(now, action, source, &msg),
            Err(Reject::Blocked(msg)) => {
                self.log_step(action, false);
                if source == Source::Replay {
                    self.diverge("a saved step needs an app the owner declined");
                }
                let summary = format!("Blocked: {msg}.");
                self.finish(now, OutcomeStatus::Blocked, &summary)
            }
        }
    }

    /// Emits `Effect::Confirm(request)`, or finishes `Blocked` when the owner already
    /// declined this risk category in this session.
    fn ask_confirm(
        &mut self,
        now: u64,
        pending: Pending,
        request: ConfirmRequest,
        approves_app: Option<String>,
        then: Option<ConfirmRequest>,
    ) -> Effect {
        if approves_app.is_none() && self.declined.contains(&request.category) {
            let summary = format!(
                "Blocked: the owner already declined a {} action.",
                category_name(request.category)
            );
            return self.finish(now, OutcomeStatus::Blocked, &summary);
        }
        self.state = State::AwaitingConfirm {
            pending: Box::new(pending),
            request: request.clone(),
            approves_app,
            then,
        };
        Effect::Confirm(request)
    }

    /// Whether the owner approved acting in `app` (in memory or in this session), or it is
    /// the agent's own app.
    fn app_approved(&self, app: &str) -> bool {
        self.memory.is_app_approved(app)
            || self.approved_apps.contains(app)
            || self.is_agent_app(app)
    }

    /// Whether `app` is the agent's own app (`SessionConfig::agent_bundle_id`).
    fn is_agent_app(&self, app: &str) -> bool {
        self.config.agent_bundle_id.as_deref().is_some_and(|own| {
            !own.trim().is_empty() && own.trim().eq_ignore_ascii_case(app.trim())
        })
    }

    /// Whether the owner declined first-use approval for `app` in this session (bundle ids
    /// compared ignoring ASCII case and surrounding whitespace).
    fn app_declined(&self, app: &str) -> bool {
        let app = app.trim();
        self.declined_apps
            .iter()
            .any(|d| d.trim().eq_ignore_ascii_case(app))
    }

    /// The display name of `bundle_id`: the screen's, when it is in front, else from
    /// `known_apps`.
    fn app_name(&self, bundle_id: &str) -> Option<String> {
        if self.current.screen.app_bundle_id == bundle_id {
            if let Some(n) = &self.current.screen.app_name {
                return Some(n.clone());
            }
        }
        self.apps
            .iter()
            .find(|a| a.bundle_id == bundle_id)
            .map(|a| a.name.clone())
    }

    /// The "declined" message for `app`.
    fn declined_message(&self, app: &str) -> String {
        let name = self.app_name(app).unwrap_or_else(|| app.to_owned());
        format!("the owner did not let Jarvis act in {}", clean(&name, 60))
    }

    /// A refused action: tell the model, count a failure, decide again on the same screen.
    fn reject(&mut self, now: u64, action: &Action, source: Source, msg: &str) -> Effect {
        let described = describe(action, &self.current.safe);
        let result = format!("failed: {}", clean(msg, 200));
        self.repeat = None;
        self.last_failed = Some(format!("{described} -> {}", clean(&result, 120)));
        self.history
            .push(described, result, source == Source::Replay);
        self.budget.record_failure();
        self.log_step(action, false);
        if source == Source::Replay {
            self.diverge("a saved step is not possible here");
        }
        self.decide(now)
    }

    /// Normalises, validates and gates an action. Nothing reaches the shell without passing
    /// `jarvis_safety::evaluate` here.
    fn prepare(&self, action: &Action, source: Source) -> Result<Prepared, Reject> {
        let screen = &self.current.screen;
        let policy = &self.config.policy;
        // 0. Nothing happens inside an app the owner declined in this session, not even
        //    reading it closer.
        if uses_foreground_app(action) && self.app_declined(&screen.app_bundle_id) {
            return Err(Reject::Blocked(
                self.declined_message(&screen.app_bundle_id),
            ));
        }
        // 1. Normalise: resolve app names, check tool calls, gate tap_point on vision.
        let mut tool: Option<&ToolSpec> = None;
        let normalized = match action {
            Action::OpenApp { app } => match resolve_app(app, &self.apps) {
                AppTarget::Bundle(bundle_id) => {
                    if check_app(policy, &bundle_id).is_denied()
                        || check_app_name(policy, &self.apps, app).is_denied()
                    {
                        return Err(Reject::AppDenied(format!(
                            "{} is on the deny list",
                            clean(&bundle_id, 80)
                        )));
                    }
                    if self.app_declined(&bundle_id) {
                        return Err(Reject::Blocked(self.declined_message(&bundle_id)));
                    }
                    Action::OpenApp { app: bundle_id }
                }
                AppTarget::Unknown => {
                    if check_app_name(policy, &self.apps, app).is_denied() {
                        return Err(Reject::AppDenied(format!(
                            "{} is on the deny list",
                            clean(app, 80)
                        )));
                    }
                    return Err(Reject::Failed(self.unknown_app_message(app)));
                }
            },
            Action::CallTool { name, args } => match check_call(&self.config.tools, name, args) {
                Ok((spec, args)) => {
                    if let Some(app) = &spec.app {
                        if self.app_declined(app) {
                            return Err(Reject::Blocked(self.declined_message(app)));
                        }
                        if check_app(policy, app).is_denied() {
                            return Err(Reject::AppDenied(format!(
                                "tool {} acts on {}, which is on the deny list",
                                clean(&spec.name, 40),
                                clean(app, 80)
                            )));
                        }
                    }
                    tool = Some(spec);
                    Action::CallTool {
                        name: spec.name.clone(),
                        args,
                    }
                }
                Err(e) => {
                    let names: Vec<String> = self
                        .config
                        .tools
                        .iter()
                        .take(MAX_TOOLS * 2)
                        .map(|t| clean(&t.name, 40))
                        .collect();
                    return Err(Reject::Failed(format!(
                        "{e}; tools: {}",
                        if names.is_empty() {
                            "none".to_owned()
                        } else {
                            names.join(", ")
                        }
                    )));
                }
            },
            Action::TapPoint { .. } if !(self.last_request_vision && source == Source::Model) => {
                return Err(Reject::Failed(
                    "tap_point needs a screenshot; use the element numbers or look first"
                        .to_owned(),
                ));
            }
            other => other.clone(),
        };
        // 1a. A tap on the home screen launches what it hits: the icon gets the checks
        //     `open_app` gets, before the tap is sent (see `home_launch`).
        let home = if is_springboard(&screen.app_bundle_id) {
            home_launch(&normalized, screen, &self.apps)
        } else {
            None
        };
        match &home {
            Some(HomeLaunch::App {
                bundle_id,
                label,
                widget,
            }) => {
                // The known app's bundle id, or the label as an app result, icon or widget
                // of a denied app (not merely a label that mentions one).
                if check_app(policy, bundle_id).is_denied() || self.names_denied_app(label, *widget)
                {
                    return Err(Reject::AppDenied(denied_launch_message(label)));
                }
                if self.app_declined(bundle_id) {
                    return Err(Reject::Blocked(self.declined_message(bundle_id)));
                }
            }
            Some(HomeLaunch::Unknown { label, widget })
                if self.names_denied_app(label, *widget) =>
            {
                return Err(Reject::AppDenied(denied_launch_message(label)));
            }
            Some(HomeLaunch::Unresolved { keyboard, .. }) => {
                // Launch-by-Return (or by a tap Jarvis cannot name): not while a denied app
                // is among the results. A result on the list is not a deny strike.
                if let Some(name) = self.denied_search_result() {
                    let how = if *keyboard {
                        "a tap on the keyboard (its Return key opens the top search result)"
                    } else {
                        "a tap on no listed element"
                    };
                    return Err(Reject::Failed(format!(
                        "not done: {how} could open {name}, which is on the deny list and among the results"
                    )));
                }
            }
            Some(HomeLaunch::Unknown { .. } | HomeLaunch::Search { .. }) | None => {}
        }
        // 1b. Skill confinement: a skill with `confine_to_apps` keeps the session in its
        //     apps. It only narrows; every check below still applies.
        if let Verdict::Deny(reason) =
            check_confinement(self.config.skill.as_ref(), &normalized, screen, tool)
        {
            return Err(Reject::AppDenied(reason));
        }
        // Jarvis never types into password fields (validate_action would reject it every
        // time): hand that one field to the owner instead of counting failures.
        if let Action::Type { element, .. } = &normalized {
            let target = match element {
                Some(i) => screen.get(*i),
                None => screen.elements.iter().find(|e| e.focused),
            };
            if target.is_some_and(|e| e.secure) {
                let app = self
                    .current
                    .safe
                    .app_name
                    .clone()
                    .unwrap_or_else(|| self.current.safe.app_bundle_id.clone());
                return Ok(Prepared::Ask {
                    question: format!(
                        "Please enter the password in {app} yourself (or use AutoFill), then tell me to continue."
                    ),
                    described: "type into a password field (handed to the owner)".to_owned(),
                });
            }
        }
        // 2. Substitute profile placeholders into what will actually be typed.
        let executed = match &normalized {
            Action::Type {
                element,
                text,
                submit,
            } => match substitute(text, &self.memory.profile) {
                Ok(t) => Action::Type {
                    element: *element,
                    text: t,
                    submit: *submit,
                },
                Err(e) => return Err(Reject::Failed(e.to_string())),
            },
            other => other.clone(),
        };
        // 2a. Typing on the home screen searches (Spotlight), and Spotlight opens its top
        //     hit on Return: text that *is* a denied app's name (or the name and "app") is
        //     refused before it is typed (see `search_names_denied_app`); anything else is
        //     typed, and its Return waits for the results check (`on_settle`).
        if let Action::Type { text, .. } = &executed {
            if is_springboard(&screen.app_bundle_id)
                && search_names_denied_app(policy, &self.apps, text).is_some()
            {
                return Err(Reject::AppDenied(format!(
                    "searching the home screen for \"{}\" would open {}, which is on the deny list",
                    clean(text, 60),
                    clean(text.trim(), 60)
                )));
            }
        }
        // 3. Safety: validation, deny list, risk classification, confirmation gate. A screen
        //    read from runner text is unverified: every state-changing runner action on it
        //    is at least `Unverified` (asks), with a warning in its summary.
        let decision = evaluate_from(policy, &executed, screen, tool, self.current.source);
        if let Decision::Deny(reason) = &decision {
            return Err(Reject::Failed(format!("not allowed: {reason}")));
        }
        let described = describe(&normalized, &self.current.safe);
        // 4. How to carry it out.
        let exec = match &executed {
            Action::AskUser { question } => {
                return Ok(Prepared::Ask {
                    question: clean(question, 300),
                    described,
                })
            }
            Action::Look => return Ok(Prepared::Look),
            Action::CallTool { name, args } => Exec::Tool(ToolCall {
                name: name.clone(),
                args: args.clone(),
            }),
            ui => Exec::Runner(plan(ui, screen).map_err(|e| Reject::Failed(e.to_string()))?),
        };
        let (exec, check) = match exec {
            Exec::Runner(queue) => {
                let (queue, check) = type_check(&executed, screen, queue);
                (Exec::Runner(queue), check)
            }
            tool => (tool, None),
        };
        let pending = Pending {
            expect_change: should_change(&normalized, screen),
            action: normalized.clone(),
            source,
            before: screen.clone(),
            before_safe: self.current.safe.clone(),
            before_digest: self.current.digest,
            before_target_source: self.target_source(&normalized),
            described,
            exec,
            check,
        };
        let risk = match decision {
            Decision::Confirm(mut request) => {
                // Describe the placeholder, never the substituted personal value, from the
                // profile-masked screen (see `risk_summary`).
                request.summary = self.risk_summary(&normalized, request.category);
                Some(request)
            }
            Decision::Allow | Decision::Deny(_) => None,
        };
        // 5. First-use approval of the app the action acts in (asked before the risk). A
        //    home-screen tap that may launch an app Jarvis cannot name asks a one-off
        //    approval naming what was tapped, never silently.
        let unknown_launch = match &home {
            Some(HomeLaunch::Unknown { label, .. }) => {
                Some((format!("\"{}\"", clean(label, 60)), false))
            }
            Some(HomeLaunch::Search { text }) => Some((
                format!("the top search result for \"{}\"", clean(text, 60)),
                false,
            )),
            // Never sent unasked, whatever the first-use setting.
            Some(HomeLaunch::Unresolved { x, y, keyboard }) => Some((
                if *keyboard {
                    format!(
                        "the top search result (a tap on the keyboard at {:.0}%, {:.0}%)",
                        x * 100.0,
                        y * 100.0
                    )
                } else {
                    format!(
                        "whatever is at {:.0}%, {:.0}% of the screen",
                        x * 100.0,
                        y * 100.0
                    )
                },
                true,
            )),
            Some(HomeLaunch::App { .. }) | None => None,
        };
        if let Some((what, _)) =
            unknown_launch.filter(|(_, always)| *always || policy.first_use_approval)
        {
            if policy.denies_category(RiskCategory::FirstUse) {
                return Err(Reject::AppDenied(format!(
                    "{what} on the home screen is not a known app, and new apps are not allowed"
                )));
            }
            let request = ConfirmRequest {
                category: RiskCategory::FirstUse,
                summary: clean(
                    &format!(
                        "Let Jarvis open {what} from the home screen? It is not one of the known apps."
                    ),
                    200,
                ),
                app: screen.app_bundle_id.clone(),
            };
            return Ok(Prepared::FirstUse {
                pending,
                app: None,
                request,
                then: risk,
            });
        }
        if let Some(app) =
            first_use_app(policy, &executed, screen, tool).filter(|a| !self.app_approved(a))
        {
            if policy.denies_category(RiskCategory::FirstUse) {
                return Err(Reject::AppDenied(format!(
                    "{} has not been approved for Jarvis",
                    clean(&app, 80)
                )));
            }
            let request = first_use_request(&app, self.app_name(&app).as_deref());
            return Ok(Prepared::FirstUse {
                pending,
                app: Some(app),
                request,
                then: risk,
            });
        }
        Ok(match risk {
            Some(request) => Prepared::Confirm(pending, request),
            None => Prepared::Execute(pending),
        })
    }

    fn unknown_app_message(&self, app: &str) -> String {
        let names: Vec<String> = self
            .config
            .known_apps
            .iter()
            .take(MAX_APPS_LISTED)
            .map(|a| clean(&a.name, 30))
            .collect();
        if names.is_empty() {
            format!("unknown app \"{}\"", clean(app, 40))
        } else {
            format!(
                "unknown app \"{}\"; known apps: {}",
                clean(app, 40),
                names.join(", ")
            )
        }
    }

    /// Sends the first command (or the tool call) of an approved action.
    fn execute(&mut self, mut pending: Pending) -> Effect {
        match &mut pending.exec {
            Exec::Tool(call) => {
                let call = call.clone();
                self.state = State::AwaitingTool {
                    pending: Box::new(pending),
                };
                Effect::CallTool(call)
            }
            Exec::Runner(queue) => {
                let mut queue = std::mem::take(queue);
                match queue.pop_front() {
                    Some(first) => {
                        self.state = State::AwaitingAct {
                            queue,
                            pending: Box::new(pending),
                            fresh: true,
                        };
                        Effect::Act(first)
                    }
                    // Nothing to send (cannot happen for validated actions): verify anyway.
                    None => self.observe(ObservePurpose::Verify(Box::new(pending)), false),
                }
            }
        }
    }

    /// Sends the follow-up commands of an action already under way.
    fn continue_act(&mut self, mut queue: VecDeque<RunnerCommand>, pending: Pending) -> Effect {
        match queue.pop_front() {
            Some(first) => {
                self.state = State::AwaitingAct {
                    queue,
                    pending: Box::new(pending),
                    fresh: false,
                };
                Effect::Act(first)
            }
            None => self.observe(ObservePurpose::Verify(Box::new(pending)), false),
        }
    }

    /// Checks that typed text arrived; corrects once, then tells the model.
    fn verify_typing(&mut self, now: u64, mut pending: Pending, mut check: TypeCheck) -> Effect {
        let screen = &self.current.screen;
        let field = check
            .field
            .and_then(|fp| screen.find_by_fingerprint(fp))
            .or_else(|| {
                screen
                    .elements
                    .iter()
                    .find(|e| e.focused && e.role.is_text_input())
            })
            .filter(|e| !e.secure);
        let after = field.and_then(|e| e.value.clone());
        let placeholder = check
            .placeholder
            .clone()
            .or_else(|| field.and_then(|e| e.placeholder.clone()));
        let verdict = match field {
            Some(_) => check_typed(
                &check.typed,
                check.prior.as_deref(),
                after.as_deref(),
                placeholder.as_deref(),
            ),
            None => TypedVerdict::Unverifiable,
        };
        // The field the text went into is still on screen: found by its fingerprint (or,
        // when the check had none, the focused input), in the same app.
        let same_field = screen
            .app_bundle_id
            .eq_ignore_ascii_case(&pending.before.app_bundle_id)
            && match check.field {
                Some(fp) => screen.find_by_fingerprint(fp).is_some(),
                None => field.is_some(),
            };
        let (kind, found) = match verdict {
            TypedVerdict::Unverifiable if !check.then.is_empty() && !same_field => {
                // The screen changed after typing: the held-back Return was gated on the
                // screen it was asked about, not this one. It is not pressed.
                self.record_failure(&pending, prompt::RESULT_RETURN_HELD);
                return self.decide(now);
            }
            TypedVerdict::Match | TypedVerdict::Unverifiable => {
                let then = std::mem::take(&mut check.then);
                if then.is_empty() {
                    return self.verify(now, pending);
                }
                if is_springboard(&self.current.screen.app_bundle_id) {
                    // Return on the home screen opens Spotlight's top hit: look once more
                    // so results that load late are seen, then press it only while no
                    // denied app is among them.
                    let labels = self.result_labels();
                    return self.observe(
                        ObservePurpose::Settle(Box::new(Settle {
                            pending: Box::new(pending),
                            then,
                            labels,
                            looks: 1,
                        })),
                        false,
                    );
                }
                return self.continue_act(then, pending);
            }
            TypedVerdict::Mismatch { kind, found } => (kind, found),
        };
        if !check.retried {
            let deletes = after.as_deref().and_then(|a| {
                correction_deletes(
                    check.prior.as_deref(),
                    a,
                    placeholder.as_deref(),
                    MAX_CLEAR_DELETES,
                )
            });
            if let Some(n) = deletes {
                let mut queue: VecDeque<RunnerCommand> = (0..n)
                    .map(|_| RunnerCommand::PressKey { key: Key::Delete })
                    .collect();
                queue.push_back(RunnerCommand::TypeText {
                    text: check.typed.clone(),
                });
                check.retried = true;
                pending.check = Some(check);
                return self.continue_act(queue, pending);
            }
        }
        let shown = match &pending.action {
            Action::Type { text, .. } => text.clone(),
            _ => String::new(),
        };
        // Quote the field only when the typed text was the model's own (no profile value
        // substituted), so a mangled personal value is never echoed into a prompt.
        let field_text = if shown == check.typed {
            format!(
                ", the field shows \"{}\"",
                self.safe_text(&found, MAX_TYPED_QUOTE)
            )
        } else {
            String::new()
        };
        let msg = format!(
            "failed: the typed text did not arrive intact ({}): expected \"{}\"{field_text}{}",
            kind.describe(),
            self.safe_text(&shown, MAX_TYPED_QUOTE),
            if check.retried {
                "; it was typed again once"
            } else {
                ""
            }
        );
        self.record_failure(&pending, &msg);
        self.decide(now)
    }

    /// Whether the home-screen label `label` is a denied app's app result or icon (see
    /// `label_names_denied_app`), or, for a widget, belongs to one
    /// (`widget_names_denied_app`).
    fn names_denied_app(&self, label: &str, widget: bool) -> bool {
        let policy = &self.config.policy;
        label_names_denied_app(policy, &self.apps, label).is_some()
            || (widget && widget_names_denied_app(policy, &self.apps, label).is_some())
    }

    /// On the home screen (Spotlight, App Library, widgets): the display name of a denied
    /// app among the app results, icons and widgets on the screen, which Return (opening
    /// the top hit) or a tap Jarvis cannot name could open; `None` elsewhere or when there
    /// is none. Web suggestions and other apps' content that merely mention the name do
    /// not count.
    fn denied_search_result(&self) -> Option<String> {
        let screen = &self.current.screen;
        if !is_springboard(&screen.app_bundle_id) {
            return None;
        }
        screen
            .elements
            .iter()
            .filter(|e| !e.role.is_text_input())
            .find(|e| self.names_denied_app(&e.label, is_widget(e)))
            .map(|e| app_display_name(&e.label))
    }

    /// The labels of the search results on the home screen (everything but the text
    /// inputs), to tell whether the results have settled.
    fn result_labels(&self) -> Vec<String> {
        self.current
            .screen
            .elements
            .iter()
            .filter(|e| !e.role.is_text_input())
            .map(|e| format!("{}|{}", e.role.as_str(), e.label))
            .collect()
    }

    /// A held-back Return on the home screen, once the results have settled: pressed
    /// unless a denied app is among them. That is not a deny strike (the result list is
    /// not an attempt to open it): the step fails and the model is told which app.
    fn press_held_return(
        &mut self,
        now: u64,
        pending: Pending,
        then: VecDeque<RunnerCommand>,
    ) -> Effect {
        if let Some(name) = self.denied_search_result() {
            let msg = format!(
                "not done: Return was not pressed: it could open {name}, which is on the deny list and among the results"
            );
            self.record_failure(&pending, &msg);
            return self.decide(now);
        }
        self.continue_act(then, pending)
    }

    /// The look after typing a search on the home screen, before its held-back Return:
    /// results can load after the first look, so the Return waits until two looks in a row
    /// show the same result labels (at most [`MAX_SETTLE_LOOKS`] more looks).
    fn on_settle(&mut self, now: u64, settle: Settle) -> Effect {
        let Settle {
            pending,
            then,
            labels,
            looks,
        } = settle;
        let fresh = self.result_labels();
        let still_home = is_springboard(&self.current.screen.app_bundle_id);
        if still_home && fresh == labels {
            return self.press_held_return(now, *pending, then);
        }
        if !still_home || looks >= MAX_SETTLE_LOOKS {
            let msg = if still_home {
                "not done: Return was not pressed: the search results kept changing"
            } else {
                prompt::RESULT_RETURN_HELD
            };
            self.record_failure(&pending, msg);
            return self.decide(now);
        }
        self.observe(
            ObservePurpose::Settle(Box::new(Settle {
                pending,
                then,
                labels: fresh,
                looks: looks.saturating_add(1),
            })),
            false,
        )
    }

    /// Compares the screen after an action with the one before it.
    fn verify(&mut self, now: u64, mut pending: Pending) -> Effect {
        if let Some(check) = pending.check.take() {
            return self.verify_typing(now, pending, check);
        }
        let changed = diff(&pending.before, &self.current.screen).changed;
        if pending.expect_change && !changed {
            let msg = match pending.action {
                Action::Scroll { .. } | Action::Swipe { .. } => prompt::RESULT_SCROLL_END,
                _ => prompt::RESULT_NO_CHANGE,
            };
            self.record_failure(&pending, msg);
        } else {
            let change = describe_change(&pending.action, &pending.before_safe, &self.current.safe);
            let note = self.count_repeat(&pending);
            let result = match note {
                Some(times) => format!(
                    "{}; {change} (done {times} times in a row)",
                    prompt::RESULT_OK
                ),
                None => format!("{}; {change}", prompt::RESULT_OK),
            };
            self.record_success(&pending, &result);
        }
        self.decide(now)
    }

    /// Ends the session when the model proposes an action it was told, twice, it keeps
    /// repeating; `None` otherwise. Any other action ends the run.
    fn repeat_check(&mut self, now: u64, action: &Action, source: Source) -> Option<Effect> {
        let key = (source == Source::Model)
            .then(|| repeat_key(action, &self.current.screen))
            .flatten();
        let run = self.repeat.as_ref()?;
        let same = key.as_deref() == Some(run.key.as_str())
            && same_layout(&run.layout, &layout(&self.current.screen));
        if !same {
            self.repeat = None;
            return None;
        }
        if run.warnings < 2 {
            return None;
        }
        let described = describe(action, &self.current.safe);
        let summary = format!(
            "Stuck: {described} was chosen again after {} repetitions and two warnings.",
            run.count
        );
        self.log_step(action, false);
        Some(self.finish(now, OutcomeStatus::Failed, &summary))
    }

    /// Counts a repetition that worked (see [`RepeatRun`]) and leaves a warning note at
    /// [`REPEAT_WARN`] and [`REPEAT_FINAL`]. Returns the run length when a warning was
    /// given.
    fn count_repeat(&mut self, pending: &Pending) -> Option<u32> {
        let key = (pending.source == Source::Model)
            .then(|| repeat_key(&pending.action, &pending.before))
            .flatten();
        let Some(key) = key else {
            self.repeat = None;
            return None;
        };
        // A control whose own value changed is an adjuster (a stepper reaching a value).
        let own_value_changed = pending
            .action
            .element()
            .and_then(|i| pending.before.get(i))
            .filter(|e| !e.role.is_text_input())
            .and_then(|e| {
                self.current
                    .screen
                    .find_by_fingerprint(e.fingerprint)
                    .map(|now| now.value != e.value)
            })
            .unwrap_or(false);
        if own_value_changed {
            self.repeat = None;
            return None;
        }
        let here = layout(&pending.before);
        let run = match self.repeat.take() {
            Some(mut run) if run.key == key && same_layout(&run.layout, &here) => {
                run.count = run.count.saturating_add(1);
                run.layout = here;
                run
            }
            _ => RepeatRun {
                key,
                layout: here,
                first: pending.before_safe.clone(),
                count: 1,
                warnings: 0,
            },
        };
        let warn = match (run.count, run.warnings) {
            (c, 0) if c >= REPEAT_WARN => Some(false),
            (c, 1) if c >= REPEAT_FINAL => Some(true),
            _ => None,
        };
        let count = run.count;
        let mut run = run;
        let given = warn.map(|last| {
            run.warnings = run.warnings.saturating_add(1);
            let since = describe_change(&pending.action, &run.first, &self.current.safe);
            let note = if last {
                prompt::repeat_final_note(&pending.described, count, &since)
            } else {
                prompt::repeat_note(&pending.described, count, &since)
            };
            self.notes.push(note);
            count
        });
        self.repeat = Some(run);
        given
    }

    fn record_success(&mut self, pending: &Pending, result: &str) {
        self.declined_open = None;
        self.last_failed = None;
        self.history.push(
            pending.described.clone(),
            result,
            pending.source == Source::Replay,
        );
        self.budget.record_success();
        self.log_step(&pending.action, true);
        let element = pending.action.element().and_then(|i| pending.before.get(i));
        self.recorder.record(RecordedStep {
            action: pending.action.clone(),
            element,
            screen: &pending.before,
        });
        if pending.source == Source::Replay {
            if let Some(r) = &mut self.replay {
                r.advance();
            }
        }
    }

    fn record_failure(&mut self, pending: &Pending, result: &str) {
        self.repeat = None;
        // Reaching the end of a list is information, not a failed attempt.
        if result != prompt::RESULT_SCROLL_END {
            self.last_failed = Some(format!("{} -> {}", pending.described, clean(result, 120)));
        }
        self.history.push(
            pending.described.clone(),
            result,
            pending.source == Source::Replay,
        );
        self.budget.record_failure();
        self.log_step(&pending.action, false);
        if pending.source == Source::Replay {
            self.diverge("a saved step did not work");
        }
    }

    fn log_step(&mut self, action: &Action, ok: bool) {
        self.log_steps.push(LogStep {
            action: action.name().to_owned(),
            app: self.current.screen.app_bundle_id.clone(),
            ok,
        });
    }

    /// Text safe for a prompt from a source the screen sanitiser did not see (tool output,
    /// flow labels): profile values masked, secrets redacted, sanitised.
    fn safe_text(&self, text: &str, max_chars: usize) -> String {
        clean(&mask(text, &self.memory.profile), max_chars)
    }

    /// Text safe for the persistent log: profile values masked, secrets redacted.
    fn log_text(&self, text: &str) -> String {
        redact(&mask(text, &self.memory.profile))
    }

    fn finish(&mut self, now: u64, status: OutcomeStatus, summary: &str) -> Effect {
        let outcome = Outcome {
            status,
            summary: clean(summary, 300),
            steps: self.budget.steps(),
            duration_ms: if self.started {
                self.budget.elapsed_ms(now)
            } else {
                0
            },
        };
        if self.started {
            self.update_memory(now, &outcome);
        }
        self.state = State::Finished(outcome.clone());
        Effect::Finished(outcome)
    }

    /// Stores the learned flow, flow statistics and the log entry.
    fn update_memory(&mut self, now: u64, outcome: &Outcome) {
        for app in &self.approved_apps {
            self.memory.approve_app(app);
        }
        let success = outcome.status == OutcomeStatus::Success;
        let failed = matches!(
            outcome.status,
            OutcomeStatus::Failed | OutcomeStatus::Blocked
        );
        let recorder = std::mem::replace(&mut self.recorder, Recorder::new(String::new()));
        let recorded = if success {
            recorder.finish(true, now)
        } else {
            None
        };
        match (&self.replayed_flow, self.replay_diverged) {
            (Some(id), false) if success => {
                self.memory.flows.record_result(id, true, now);
            }
            (Some(id), _) if failed => {
                self.memory.flows.record_result(id, false, now);
            }
            _ => {
                // A lock that left a step's result unknown makes the recording unreliable
                // (it may lack a step that did happen): learn nothing from this session.
                if let Some(flow) = recorded.filter(|_| !self.lock_uncertain) {
                    self.memory.flows.upsert(flow);
                }
            }
        }
        let entry = LogEntry {
            task: self.log_text(&self.task.text),
            steps: std::mem::take(&mut self.log_steps),
            outcome: outcome.status,
            summary: self.log_text(&outcome.summary),
            started_ms: self.started_ms,
            finished_ms: now.max(self.started_ms),
            lock_waits: self.lock_waits,
            locked_ms: self.locked_total_ms,
        };
        self.memory.log.push(entry);
    }
}

fn category_name(c: RiskCategory) -> &'static str {
    match c {
        RiskCategory::None => "harmless",
        RiskCategory::SendMessage => "send-message",
        RiskCategory::Payment => "payment",
        RiskCategory::Purchase => "purchase",
        RiskCategory::Delete => "delete",
        RiskCategory::PublicPost => "public-post",
        RiskCategory::AccountChange => "account-change",
        RiskCategory::SystemSetting => "system-setting",
        RiskCategory::Credential => "credential",
        RiskCategory::Unverified => "unverified",
        RiskCategory::Call => "call",
        RiskCategory::FirstUse => "first-use",
    }
}

/// `config.known_apps`, then each app rule that carries a display name and whose prefix is a
/// whole bundle id (at least two dot-separated parts, no wildcard) and is not known yet.
fn named_apps(config: &SessionConfig) -> Vec<AppInfo> {
    let mut apps = config.known_apps.clone();
    for rule in &config.policy.app_rules {
        let Some(name) = rule
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
        else {
            continue;
        };
        let id = rule.bundle_id_prefix.trim();
        let whole =
            id.split('.').count() >= 2 && id.split('.').all(|p| !p.is_empty()) && !id.contains('*');
        if whole
            && !apps
                .iter()
                .any(|a| a.bundle_id.trim().eq_ignore_ascii_case(id))
        {
            apps.push(AppInfo::new(id, name));
        }
    }
    apps
}

/// Whether `action` touches or reads the foreground app (leaving it does not).
fn uses_foreground_app(action: &Action) -> bool {
    matches!(
        action,
        Action::Tap { .. }
            | Action::LongPress { .. }
            | Action::Type { .. }
            | Action::Clear { .. }
            | Action::Scroll { .. }
            | Action::Swipe { .. }
            | Action::TapPoint { .. }
            | Action::Look
    )
}

/// The control `action` acts on, on `screen`: its element; for `type` without one, the
/// focused text input; for `tap_point`, the smallest element under the point.
fn action_target<'a>(action: &Action, screen: &'a CompactScreen) -> Option<&'a Element> {
    match action {
        Action::Type { element: None, .. } => screen
            .elements
            .iter()
            .find(|e| e.focused && e.role.is_text_input()),
        Action::TapPoint { x, y } => {
            let p = denormalize(*x, *y, screen.size);
            screen
                .elements
                .iter()
                .filter(|e| e.frame.contains(p))
                .min_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
        }
        other => other.element().and_then(|i| screen.get(i)),
    }
}

/// The first screen point the planned commands touch (a tap, long press or drag start).
fn planned_point(exec: &Exec) -> Option<Point> {
    let Exec::Runner(queue) = exec else {
        return None;
    };
    queue.iter().find_map(|c| match c {
        RunnerCommand::Tap { x, y } | RunnerCommand::LongPress { x, y, .. } => {
            Some(Point::new(*x, *y))
        }
        RunnerCommand::Swipe { from, .. } => Some(*from),
        _ => None,
    })
}

/// For a `type` with text into a field that is not secure: the runner queue up to and
/// including `TypeText`, and the check (holding back what follows, i.e. `Return`).
fn type_check(
    executed: &Action,
    screen: &CompactScreen,
    mut queue: VecDeque<RunnerCommand>,
) -> (VecDeque<RunnerCommand>, Option<TypeCheck>) {
    let Action::Type { element, text, .. } = executed else {
        return (queue, None);
    };
    let target = match element {
        Some(i) => screen.get(*i),
        None => screen.elements.iter().find(|e| e.focused),
    };
    let Some(at) = queue
        .iter()
        .position(|c| matches!(c, RunnerCommand::TypeText { .. }))
    else {
        return (queue, None);
    };
    if text.is_empty() || target.is_some_and(|e| e.secure) {
        return (queue, None);
    }
    let then = queue.split_off(at.saturating_add(1));
    let check = TypeCheck {
        typed: text.clone(),
        field: target.map(|e| e.fingerprint),
        prior: target.and_then(|e| e.value.clone()),
        placeholder: target.and_then(|e| e.placeholder.clone()),
        then,
        retried: false,
    };
    (queue, Some(check))
}

/// The name a home-screen label shows for its app: the label, or its part before the
/// first comma ("Wallet, 2 notifications"), cleaned for a message.
fn app_display_name(label: &str) -> String {
    let head = label.split(',').next().unwrap_or(label).trim();
    clean(if head.is_empty() { label } else { head }, 40)
}

/// The refusal for a home-screen tap on a denied app's icon, result or widget, naming the
/// app so the model does not try variants.
fn denied_launch_message(label: &str) -> String {
    let name = app_display_name(label);
    let shown = clean(label, 60);
    if shown == name {
        format!("{name} is on the deny list")
    } else {
        format!("\"{shown}\" would open {name}, which is on the deny list")
    }
}
