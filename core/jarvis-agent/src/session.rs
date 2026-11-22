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
//! preceded by a budget check, parse retries are capped per step, and every wait for the
//! shell is bounded by the time budget, checked on each event.

use std::collections::{BTreeMap, BTreeSet};

use jarvis_memory::{LogEntry, LogStep, MemoryStore, Next, RecordedStep, Recorder, Replayer};
use jarvis_observe::{compact, diff, marks, screen_hash};
use jarvis_proto::{
    Action, CompactScreen, ConfirmRequest, Effect, Event, ImageRef, ModelRequest, Observation,
    Outcome, OutcomeStatus, Progress, RequestKind, RiskCategory, SessionConfig, Size, TaskSpec,
    TimedEvent, ToolCall, ToolSpec,
};
use jarvis_safety::{
    check_app, evaluate, redact, redact_screen, sanitize_screen_text, summarize, Budget,
    BudgetVerdict, Decision,
};

use crate::commands::{plan, resolve_app, should_change, AppTarget};
use crate::history::{clean, describe, History, MAX_SHOWN};
use crate::parser::parse_model_output;
use crate::profile::{mask, sanitize_keys, sensitive_texts, substitute};
use crate::prompt::{self, PromptParts, ACTION_MAX_TOKENS};
use crate::schema::{action_gbnf, action_json_schema};
use crate::state::{Exec, ObservePurpose, Pending, Source, State};
use crate::tools::{check_call, relevant, signature, MAX_TOOLS};

/// Fewer enabled interactive elements than this makes the tree "useless" (vision fallback).
const MIN_ACTIONABLE: usize = 2;
/// Identical (screen, action) pairs that count as stuck.
const STUCK_REPEATS: u32 = 3;
/// App deny-list hits after which the session is blocked.
const MAX_APP_DENIALS: u32 = 2;
/// Characters of tool output or answers kept in the history.
const MAX_RESULT_CHARS: usize = 300;
/// Known app names listed when `open_app` names an unknown app.
const MAX_APPS_LISTED: usize = 12;

/// The latest observation in three forms.
#[derive(Debug, Clone)]
struct Current {
    /// As compacted from the runner's tree (fingerprints, values; never shown to the model).
    screen: CompactScreen,
    /// Prompt-safe copy: profile values masked, secrets redacted, text sanitised.
    safe: CompactScreen,
    /// `screen_hash` of `screen`.
    hash: u64,
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
        };
        Self {
            screen: empty.clone(),
            safe: empty,
            hash: 0,
            screenshot: None,
            seen: false,
        }
    }
}

/// What `prepare` made of a proposed action.
enum Prepared {
    /// Allowed: execute now.
    Execute(Pending),
    /// Needs the owner's approval first.
    Confirm(Pending, ConfirmRequest),
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
}

/// One task's agent loop. See the module docs and `ARCHITECTURE.md`.
#[derive(Debug, Clone)]
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
    last_action: Option<Action>,
    log_steps: Vec<LogStep>,
}

fn event_name(e: &Event) -> &'static str {
    match e {
        Event::Start => "start",
        Event::Observed(_) => "observed",
        Event::ModelOutput(_) => "model_output",
        Event::ModelFailed { .. } => "model_failed",
        Event::ActResult(_) => "act_result",
        Event::ToolResult(_) => "tool_result",
        Event::UserDecision { .. } => "user_decision",
        Event::UserAnswer { .. } => "user_answer",
        Event::Cancel => "cancel",
    }
}

/// The prompt-safe copy of a screen: profile values masked back to placeholders, secrets
/// redacted (secure fields lose their value), every text sanitised. Fingerprints, frames
/// and indices are kept, so element numbers match the original.
fn prompt_safe(screen: &CompactScreen, memory: &MemoryStore) -> CompactScreen {
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
    let mut safe = redact_screen(&masked);
    for e in &mut safe.elements {
        e.label = sanitize_screen_text(&e.label);
        e.value = e.value.as_deref().map(sanitize_screen_text);
        e.placeholder = e.placeholder.as_deref().map(sanitize_screen_text);
    }
    safe.app_name = safe.app_name.as_deref().map(|n| clean(n, 60));
    safe.app_bundle_id = clean(&safe.app_bundle_id, 100);
    safe
}

impl Session {
    /// Creates a session for `task`. Nothing happens until `step(Start)`.
    pub fn new(config: SessionConfig, task: TaskSpec, memory: MemoryStore) -> Session {
        let mut recorder = Recorder::new(task.text.clone());
        recorder.set_sensitive_values(sensitive_texts(&memory.profile));
        let budget = Budget::new(&config.policy, 0);
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
            last_action: None,
            log_steps: Vec::new(),
        }
    }

    /// Feeds one event and returns the one thing the shell must do next.
    pub fn step(&mut self, ev: TimedEvent) -> Effect {
        let TimedEvent { now_ms: now, event } = ev;
        let state = std::mem::replace(&mut self.state, State::Idle);
        let state = match state {
            State::Finished(outcome) => {
                self.state = State::Finished(outcome.clone());
                return Effect::Finished(outcome);
            }
            other => other,
        };
        if matches!(event, Event::Cancel) {
            return self.finish(now, OutcomeStatus::Cancelled, "Cancelled by the owner.");
        }
        if self.started {
            self.budget.observe(now);
            if self.budget.elapsed_ms(now) >= self.config.policy.max_duration_ms {
                return self.finish(now, OutcomeStatus::Failed, "Time budget used up.");
            }
        }
        let state_name = state.name();
        match state {
            State::Idle => self.on_idle(now, event, state_name),
            State::AwaitingObservation { purpose } => {
                self.on_observation(now, purpose, event, state_name)
            }
            State::AwaitingModel { kind } => self.on_model(now, kind, event, state_name),
            State::AwaitingAct { queue, pending } => {
                self.on_act(now, queue, pending, event, state_name)
            }
            State::AwaitingTool { pending } => self.on_tool(now, pending, event, state_name),
            State::AwaitingConfirm { pending, request } => {
                self.on_confirm(now, pending, request, event, state_name)
            }
            State::AwaitingAnswer { described } => {
                self.on_answer(now, described, event, state_name)
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
        }
    }

    /// The memory store, updated with the recorded flow, flow statistics and the task log
    /// entry once the session has finished (an unfinished session adds nothing).
    pub fn into_memory(self) -> MemoryStore {
        self.memory
    }

    // ---------------------------------------------------------------------------------
    // Per-state handlers.
    // ---------------------------------------------------------------------------------

    fn protocol_error(&mut self, now: u64, event: &Event, state: &str) -> Effect {
        let summary = format!(
            "protocol error: unexpected {} while {state}",
            event_name(event)
        );
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
        let Event::Observed(obs) = event else {
            return self.protocol_error(now, &event, state);
        };
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
        }
    }

    fn on_model(&mut self, now: u64, kind: RequestKind, event: Event, state: &str) -> Effect {
        match event {
            Event::ModelOutput(out) => match parse_model_output(&out.text) {
                Ok(parsed) => {
                    self.parse_retries = 0;
                    self.correction = None;
                    self.notes.clear();
                    self.propose(now, parsed.action, Source::Model)
                }
                Err(e) if self.parse_retries < self.config.policy.max_parse_retries => {
                    self.parse_retries = self.parse_retries.saturating_add(1);
                    self.correction = Some(prompt::correction(&e));
                    self.infer(kind)
                }
                Err(e) => {
                    let summary = format!("The model's reply could not be understood: {e}.");
                    self.finish(now, OutcomeStatus::Failed, &summary)
                }
            },
            Event::ModelFailed { error } => {
                let summary = format!("Model error: {}", clean(&error, 200));
                self.finish(now, OutcomeStatus::Failed, &summary)
            }
            other => self.protocol_error(now, &other, state),
        }
    }

    fn on_act(
        &mut self,
        now: u64,
        mut queue: std::collections::VecDeque<jarvis_proto::RunnerCommand>,
        pending: Box<Pending>,
        event: Event,
        state: &str,
    ) -> Effect {
        let Event::ActResult(result) = event else {
            return self.protocol_error(now, &event, state);
        };
        if result.ok {
            return match queue.pop_front() {
                Some(next) => {
                    self.state = State::AwaitingAct { queue, pending };
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
        if result.ok {
            let out = clean(&result.output, MAX_RESULT_CHARS);
            let line = if out.is_empty() {
                prompt::RESULT_OK.to_owned()
            } else {
                format!("ok: {out}")
            };
            self.record_success(&pending, &line);
        } else {
            let error = result
                .error
                .as_deref()
                .filter(|e| !e.trim().is_empty())
                .unwrap_or(&result.output);
            let msg = format!("failed: {}", clean(error, MAX_RESULT_CHARS));
            self.record_failure(&pending, &msg);
        }
        self.observe(ObservePurpose::Decide, false)
    }

    fn on_confirm(
        &mut self,
        now: u64,
        pending: Box<Pending>,
        request: ConfirmRequest,
        event: Event,
        state: &str,
    ) -> Effect {
        match event {
            Event::UserDecision { approved: true } => self.execute(*pending),
            Event::UserDecision { approved: false } => {
                self.declined.insert(request.category);
                self.history.push(
                    pending.described.clone(),
                    prompt::RESULT_DECLINED,
                    pending.source == Source::Replay,
                );
                self.log_step(&pending.action, false);
                if pending.source == Source::Replay {
                    self.diverge("the owner declined a step");
                }
                self.decide(now)
            }
            other => self.protocol_error(now, &other, state),
        }
    }

    fn on_answer(&mut self, now: u64, described: String, event: Event, state: &str) -> Effect {
        let Event::UserAnswer { text } = event else {
            return self.protocol_error(now, &event, state);
        };
        let answer = prompt::neutralize_user_text(&text, MAX_RESULT_CHARS);
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
        self.observe(ObservePurpose::Decide, false)
    }

    // ---------------------------------------------------------------------------------
    // The loop: decide → propose → gate → execute → verify.
    // ---------------------------------------------------------------------------------

    fn observe(&mut self, purpose: ObservePurpose, screenshot: bool) -> Effect {
        self.state = State::AwaitingObservation { purpose };
        Effect::Observe { screenshot }
    }

    fn ingest(&mut self, obs: Observation) {
        let screen = compact(&obs.screen);
        self.current = Current {
            hash: screen_hash(&screen),
            safe: prompt_safe(&screen, &self.memory),
            screen,
            screenshot: obs.screenshot,
            seen: true,
        };
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
            self.notes.push(clean(
                &prompt::divergence_note(reason),
                prompt::MAX_NOTE_CHARS,
            ));
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
        if action.is_ui() || matches!(action, Action::Look) {
            let key = (
                self.current.hash,
                serde_json::to_string(&action).unwrap_or_default(),
            );
            let count = self.repeats.entry(key).or_insert(0);
            *count = count.saturating_add(1);
            if *count >= STUCK_REPEATS {
                if self.stuck_hinted {
                    return self.finish(
                        now,
                        OutcomeStatus::Failed,
                        "Stuck: the same action kept being chosen on an unchanged screen.",
                    );
                }
                self.stuck_hinted = true;
                let described = describe(&action, &self.current.safe);
                self.history
                    .push(described, prompt::RESULT_STUCK, source == Source::Replay);
                self.notes.push(prompt::STUCK_HINT.to_owned());
                self.budget.record_failure();
                self.log_step(&action, false);
                if source == Source::Replay {
                    self.diverge("a saved step repeated");
                }
                return self.decide(now);
            }
        }
        match self.prepare(&action, source) {
            Ok(Prepared::Execute(pending)) => self.execute(pending),
            Ok(Prepared::Confirm(pending, request)) => {
                if self.declined.contains(&request.category) {
                    let summary = format!(
                        "Blocked: the owner already declined a {} action.",
                        category_name(request.category)
                    );
                    return self.finish(now, OutcomeStatus::Blocked, &summary);
                }
                self.state = State::AwaitingConfirm {
                    pending: Box::new(pending),
                    request: request.clone(),
                };
                Effect::Confirm(request)
            }
            Ok(Prepared::Ask {
                question,
                described,
            }) => {
                self.state = State::AwaitingAnswer { described };
                Effect::AskUser { question }
            }
            Ok(Prepared::Look) => {
                let described = describe(&action, &self.current.safe);
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
                self.reject(now, &action, source, &format!("not allowed: {msg}"))
            }
            Err(Reject::Failed(msg)) => self.reject(now, &action, source, &msg),
        }
    }

    /// A refused action: tell the model, count a failure, decide again on the same screen.
    fn reject(&mut self, now: u64, action: &Action, source: Source, msg: &str) -> Effect {
        let described = describe(action, &self.current.safe);
        self.history.push(
            described,
            format!("failed: {}", clean(msg, 200)),
            source == Source::Replay,
        );
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
        // 1. Normalise: resolve app names, check tool calls, gate tap_point on vision.
        let mut tool: Option<&ToolSpec> = None;
        let normalized = match action {
            Action::OpenApp { app } => match resolve_app(app, &self.config.known_apps) {
                AppTarget::Bundle(bundle_id) => {
                    if check_app(policy, &bundle_id).is_denied()
                        || check_app(policy, app).is_denied()
                    {
                        return Err(Reject::AppDenied(format!(
                            "{} is on the deny list",
                            clean(&bundle_id, 80)
                        )));
                    }
                    Action::OpenApp { app: bundle_id }
                }
                AppTarget::Unknown => {
                    if check_app(policy, app).is_denied() {
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
        // 3. Safety: validation, deny list, risk classification, confirmation gate.
        let decision = evaluate(policy, &executed, screen, tool);
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
        let pending = Pending {
            expect_change: should_change(&normalized, screen),
            action: normalized.clone(),
            source,
            before: screen.clone(),
            described,
            exec,
        };
        match decision {
            Decision::Confirm(mut request) => {
                if executed != normalized {
                    // Describe the placeholder, never the substituted personal value.
                    request.summary = summarize(&normalized, screen, tool);
                }
                Ok(Prepared::Confirm(pending, request))
            }
            Decision::Allow | Decision::Deny(_) => Ok(Prepared::Execute(pending)),
        }
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
                        };
                        Effect::Act(first)
                    }
                    // Nothing to send (cannot happen for validated actions): verify anyway.
                    None => self.observe(ObservePurpose::Verify(Box::new(pending)), false),
                }
            }
        }
    }

    /// Compares the screen after an action with the one before it.
    fn verify(&mut self, now: u64, pending: Pending) -> Effect {
        let changed = diff(&pending.before, &self.current.screen).changed;
        if pending.expect_change && !changed {
            let msg = match pending.action {
                Action::Scroll { .. } | Action::Swipe { .. } => prompt::RESULT_SCROLL_END,
                _ => prompt::RESULT_NO_CHANGE,
            };
            self.record_failure(&pending, msg);
        } else {
            self.record_success(&pending, prompt::RESULT_OK);
        }
        self.decide(now)
    }

    fn record_success(&mut self, pending: &Pending, result: &str) {
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
                if let Some(flow) = recorded {
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
    }
}
