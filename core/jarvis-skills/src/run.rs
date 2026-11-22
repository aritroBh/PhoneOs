//! The sans-IO workflow runner: one run of one workflow, driven by the shell one event at
//! a time, like the agent's `Session`.
//!
//! ```text
//! run.step(now, Start)                      -> StartSession { task, skill }
//! (shell runs an ordinary agent session under the owner's Policy)
//! run.step(now, SessionFinished { .. })     -> AskOwner { question }
//! run.step(now, OwnerAnswered { yes, .. })  -> Notify { text }
//! run.step(now, Notified)                   -> Wait { until_ms }
//! run.step(now, WaitElapsed)                -> Finished { outcome }
//! ```
//!
//! # Policy
//!
//! A workflow never widens policy (see [`crate::workflow`]). Each `StartSession` is an
//! ordinary session the shell creates with the owner's `Policy`; its confirmations go to the
//! owner. The runner only sequences steps; it never sees or answers a confirmation.
//!
//! # Protocol
//!
//! Exactly one effect is outstanding at a time and each has one answer: `StartSession` ->
//! `SessionFinished`, `AskOwner` -> `OwnerAnswered`, `Notify` -> `Notified`, `Wait` ->
//! `WaitElapsed`. `Cancel` is accepted in every state. Any other event is a
//! [`RunError::UnexpectedEvent`]: the run is left exactly as it was (still waiting for the
//! right answer), never panics. `WaitElapsed` before `until_ms` repeats the `Wait`. Once
//! finished, every event returns the same `Finished` effect.
//!
//! # Limits
//!
//! [`RunLimits`] caps the steps a run may have, the total time from `Start` (checked on
//! every event; a session answered after the deadline still finishes the run `Failed`)
//! and any single wait. A `RunSkill` whose skill is missing, disabled or lacks a required
//! argument fails that step with a reason; `on_failure` decides what happens next.

use std::collections::BTreeMap;

use jarvis_proto::SkillContext;
use serde::{Deserialize, Serialize};

use crate::skill::{Skill, MAX_ARG_CHARS};
use crate::store::SkillStore;
use crate::text::truncate_chars;
use crate::workflow::{
    Condition, OnFailure, Step, Workflow, WorkflowOutcome, WorkflowStatus, MAX_WORKFLOW_STEPS,
};

/// Longest summary kept from a session or put in an outcome, in characters.
const MAX_SUMMARY_CHARS: usize = 300;

/// Limits for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunLimits {
    /// Most steps a workflow may have to run (at most [`MAX_WORKFLOW_STEPS`] in any case).
    pub max_steps: u32,
    /// Longest run from `Start`, in the shell's milliseconds.
    pub max_total_ms: u64,
    /// Longest single wait, in milliseconds.
    pub max_wait_ms: u64,
}

impl Default for RunLimits {
    /// 24 steps, one hour in total, 15 minutes per wait.
    fn default() -> Self {
        Self {
            max_steps: 24,
            max_total_ms: 3_600_000,
            max_wait_ms: 900_000,
        }
    }
}

/// An input to the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowEvent {
    /// Begin.
    Start,
    /// The session from `StartSession` ended (`success` is its `Success` status).
    SessionFinished {
        /// Whether the session's outcome was `Success`.
        success: bool,
        /// Its summary.
        summary: String,
    },
    /// The owner answered `AskOwner`.
    OwnerAnswered {
        /// Yes or no.
        yes: bool,
        /// What they said, if anything.
        #[serde(default)]
        text: String,
    },
    /// The notification from `Notify` was posted.
    Notified,
    /// The wait from `Wait` is over.
    WaitElapsed,
    /// The owner stopped the run.
    Cancel,
}

impl WorkflowEvent {
    /// The snake_case name, identical to the JSON `"type"` tag.
    pub fn name(&self) -> &'static str {
        match self {
            WorkflowEvent::Start => "start",
            WorkflowEvent::SessionFinished { .. } => "session_finished",
            WorkflowEvent::OwnerAnswered { .. } => "owner_answered",
            WorkflowEvent::Notified => "notified",
            WorkflowEvent::WaitElapsed => "wait_elapsed",
            WorkflowEvent::Cancel => "cancel",
        }
    }
}

/// The one thing the shell must do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkflowEffect {
    /// Run an ordinary agent session for `task`, with `skill` as `SessionConfig::skill`,
    /// under the owner's policy; answer with `SessionFinished`.
    StartSession {
        /// The task text.
        task: String,
        /// The skill context, for `RunSkill` steps.
        skill: Option<SkillContext>,
    },
    /// Ask the owner a yes/no question; answer with `OwnerAnswered`.
    AskOwner {
        /// The question.
        question: String,
    },
    /// Post a notification; answer with `Notified`.
    Notify {
        /// The text.
        text: String,
    },
    /// Answer with `WaitElapsed` once the shell's clock reaches `until_ms`.
    Wait {
        /// Deadline in the shell's milliseconds.
        until_ms: u64,
    },
    /// The run is over.
    Finished {
        /// How it went.
        outcome: WorkflowOutcome,
    },
}

/// What the runner is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Awaiting {
    /// `Start`.
    Start,
    /// `SessionFinished`.
    Session,
    /// `OwnerAnswered`.
    Answer,
    /// `Notified`.
    Notified,
    /// `WaitElapsed`.
    WaitElapsed,
    /// Nothing: the run is over.
    Nothing,
}

impl Awaiting {
    fn event_name(self) -> &'static str {
        match self {
            Awaiting::Start => "start",
            Awaiting::Session => "session_finished",
            Awaiting::Answer => "owner_answered",
            Awaiting::Notified => "notified",
            Awaiting::WaitElapsed => "wait_elapsed",
            Awaiting::Nothing => "nothing",
        }
    }
}

/// A protocol error: the event did not answer the outstanding effect. The run is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    /// The event does not answer what the run is waiting for.
    #[error("protocol error: unexpected {got} while waiting for {expected}")]
    UnexpectedEvent {
        /// The event's name.
        got: &'static str,
        /// The event the run is waiting for.
        expected: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Idle,
    Session { step: usize },
    Answer { step: usize },
    Notified { step: usize },
    Waiting { step: usize, until_ms: u64 },
    Finished(WorkflowOutcome),
}

/// One run of a workflow. See the module docs.
#[derive(Debug, Clone)]
pub struct WorkflowRun {
    workflow: Workflow,
    /// The skills the workflow's `RunSkill` steps name, as they were when the run was
    /// created (missing ones are absent).
    skills: BTreeMap<String, Skill>,
    limits: RunLimits,
    state: State,
    started_ms: u64,
    steps_run: u32,
    failed_step: Option<u32>,
    failures: u32,
    last_session: Option<(bool, String)>,
    last_answer: Option<bool>,
    last_effect: Option<WorkflowEffect>,
}

impl WorkflowRun {
    /// A run of `workflow`, resolving its skills from `skills` now. Nothing happens until
    /// `step(Start)`. An invalid or disabled workflow finishes `Failed` at `Start`.
    pub fn new(workflow: Workflow, skills: &SkillStore, limits: RunLimits) -> WorkflowRun {
        let resolved = workflow
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::RunSkill { skill_id, .. } => skills.get(skill_id),
                _ => None,
            })
            .map(|s| (s.id.clone(), s.clone()))
            .collect();
        WorkflowRun {
            workflow,
            skills: resolved,
            limits,
            state: State::Idle,
            started_ms: 0,
            steps_run: 0,
            failed_step: None,
            failures: 0,
            last_session: None,
            last_answer: None,
            last_effect: None,
        }
    }

    /// The workflow being run.
    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    /// What the run is waiting for.
    pub fn awaiting(&self) -> Awaiting {
        match self.state {
            State::Idle => Awaiting::Start,
            State::Session { .. } => Awaiting::Session,
            State::Answer { .. } => Awaiting::Answer,
            State::Notified { .. } => Awaiting::Notified,
            State::Waiting { .. } => Awaiting::WaitElapsed,
            State::Finished(_) => Awaiting::Nothing,
        }
    }

    /// The outstanding effect (the last one returned), if any.
    pub fn pending(&self) -> Option<&WorkflowEffect> {
        self.last_effect.as_ref()
    }

    /// The outcome once finished.
    pub fn outcome(&self) -> Option<&WorkflowOutcome> {
        match &self.state {
            State::Finished(o) => Some(o),
            _ => None,
        }
    }

    /// 0-based index of the step in progress, if any.
    pub fn current_step(&self) -> Option<usize> {
        match self.state {
            State::Session { step }
            | State::Answer { step }
            | State::Notified { step }
            | State::Waiting { step, .. } => Some(step),
            State::Idle | State::Finished(_) => None,
        }
    }

    /// Feeds one event; returns the next effect, or a protocol error that leaves the run
    /// unchanged.
    pub fn step(&mut self, now_ms: u64, event: WorkflowEvent) -> Result<WorkflowEffect, RunError> {
        let effect = self.dispatch(now_ms, event)?;
        self.last_effect = Some(effect.clone());
        Ok(effect)
    }

    fn unexpected(&self, event: &WorkflowEvent) -> RunError {
        RunError::UnexpectedEvent {
            got: event.name(),
            expected: self.awaiting().event_name(),
        }
    }

    fn dispatch(&mut self, now: u64, event: WorkflowEvent) -> Result<WorkflowEffect, RunError> {
        if let State::Finished(o) = &self.state {
            return Ok(WorkflowEffect::Finished { outcome: o.clone() });
        }
        if event == WorkflowEvent::Cancel {
            return Ok(self.finish(WorkflowStatus::Cancelled, "Cancelled by the owner."));
        }
        let answers = matches!(
            (&self.state, &event),
            (State::Idle, WorkflowEvent::Start)
                | (State::Session { .. }, WorkflowEvent::SessionFinished { .. })
                | (State::Answer { .. }, WorkflowEvent::OwnerAnswered { .. })
                | (State::Notified { .. }, WorkflowEvent::Notified)
                | (State::Waiting { .. }, WorkflowEvent::WaitElapsed)
        );
        if !answers {
            return Err(self.unexpected(&event));
        }
        if self.state != State::Idle && self.over_time(now) {
            return Ok(self.finish(WorkflowStatus::Failed, "The workflow ran out of time."));
        }
        match (self.state.clone(), event) {
            (State::Idle, WorkflowEvent::Start) => Ok(self.start(now)),
            (State::Session { step }, WorkflowEvent::SessionFinished { success, summary }) => {
                let summary = truncate_chars(summary.trim(), MAX_SUMMARY_CHARS).to_owned();
                self.last_session = Some((success, summary.clone()));
                if success {
                    Ok(self.advance(now, step + 1))
                } else {
                    let reason = if summary.is_empty() {
                        "the task did not succeed".to_owned()
                    } else {
                        summary
                    };
                    Ok(self.step_failed(now, step, &reason))
                }
            }
            (State::Answer { step }, WorkflowEvent::OwnerAnswered { yes, .. }) => {
                self.last_answer = Some(yes);
                Ok(self.advance(now, step + 1))
            }
            (State::Notified { step }, WorkflowEvent::Notified) => Ok(self.advance(now, step + 1)),
            (State::Waiting { step, until_ms }, WorkflowEvent::WaitElapsed) => {
                if now < until_ms {
                    Ok(WorkflowEffect::Wait { until_ms })
                } else {
                    Ok(self.advance(now, step + 1))
                }
            }
            (_, other) => Err(self.unexpected(&other)),
        }
    }

    fn over_time(&self, now: u64) -> bool {
        now.saturating_sub(self.started_ms) > self.limits.max_total_ms
    }

    fn start(&mut self, now: u64) -> WorkflowEffect {
        self.started_ms = now;
        if !self.workflow.enabled {
            return self.finish(WorkflowStatus::Failed, "The workflow is disabled.");
        }
        if let Err(e) = self.workflow.validate() {
            return self.finish(
                WorkflowStatus::Failed,
                &format!("The workflow is not valid: {e}."),
            );
        }
        let cap = usize::try_from(self.limits.max_steps)
            .unwrap_or(MAX_WORKFLOW_STEPS)
            .min(MAX_WORKFLOW_STEPS);
        if self.workflow.steps.len() > cap {
            return self.finish(
                WorkflowStatus::Failed,
                &format!("The workflow has more than {cap} steps."),
            );
        }
        self.advance(now, 0)
    }

    /// Runs steps from `index` until one needs the shell (or the run ends).
    fn advance(&mut self, now: u64, mut index: usize) -> WorkflowEffect {
        loop {
            let Some(step) = self.workflow.steps.get(index).cloned() else {
                return self.complete();
            };
            self.steps_run = self.steps_run.saturating_add(1);
            match step {
                Step::RunSkill { skill_id, args } => match self.skill_session(&skill_id, args) {
                    Ok(effect) => {
                        self.state = State::Session { step: index };
                        return effect;
                    }
                    Err(reason) => match self.fail_step(index, &reason) {
                        Some(done) => return done,
                        None => index += 1,
                    },
                },
                Step::RunTask { text } => {
                    self.state = State::Session { step: index };
                    return WorkflowEffect::StartSession {
                        task: text,
                        skill: None,
                    };
                }
                Step::AskOwner { question } => {
                    self.state = State::Answer { step: index };
                    return WorkflowEffect::AskOwner { question };
                }
                Step::Notify { text } => {
                    self.state = State::Notified { step: index };
                    return WorkflowEffect::Notify { text };
                }
                Step::Wait { ms } => {
                    let until_ms = now.saturating_add(ms);
                    let deadline = self.started_ms.saturating_add(self.limits.max_total_ms);
                    let reason = if ms > self.limits.max_wait_ms {
                        Some(format!(
                            "a wait of {ms} ms is longer than the {} ms allowed",
                            self.limits.max_wait_ms
                        ))
                    } else if until_ms > deadline {
                        Some("the wait would run past the workflow's time budget".to_owned())
                    } else {
                        None
                    };
                    match reason {
                        None => {
                            self.state = State::Waiting {
                                step: index,
                                until_ms,
                            };
                            return WorkflowEffect::Wait { until_ms };
                        }
                        Some(r) => match self.fail_step(index, &r) {
                            Some(done) => return done,
                            None => index += 1,
                        },
                    }
                }
                Step::StopUnless { condition } => {
                    if self.holds(&condition) {
                        index += 1;
                    } else {
                        let summary = format!(
                            "Stopped at step {}: {} did not hold.",
                            index + 1,
                            describe_condition(&condition)
                        );
                        return self.finish(WorkflowStatus::Stopped, &summary);
                    }
                }
            }
        }
    }

    /// The `StartSession` for a skill step, or why it cannot run.
    fn skill_session(
        &self,
        skill_id: &str,
        args: BTreeMap<String, String>,
    ) -> Result<WorkflowEffect, String> {
        let skill = self
            .skills
            .get(skill_id)
            .ok_or_else(|| format!("the skill \"{skill_id}\" does not exist"))?;
        if !skill.enabled {
            return Err(format!("the skill \"{}\" is disabled", skill.name));
        }
        let args = skill.declared_args(args);
        let missing = skill.missing_params(&args);
        if !missing.is_empty() {
            return Err(format!(
                "the skill \"{}\" needs {}",
                skill.name,
                missing.join(", ")
            ));
        }
        let task = skill.task_text(&args);
        Ok(WorkflowEffect::StartSession {
            task: truncate_chars(&task, MAX_ARG_CHARS * 4).to_owned(),
            skill: Some(skill.context(args)),
        })
    }

    /// Records a failed step after the shell answered it, then stops or continues.
    fn step_failed(&mut self, now: u64, index: usize, reason: &str) -> WorkflowEffect {
        match self.fail_step(index, reason) {
            Some(done) => done,
            None => self.advance(now, index + 1),
        }
    }

    /// Records a failure; returns the `Finished` effect when the run stops.
    fn fail_step(&mut self, index: usize, reason: &str) -> Option<WorkflowEffect> {
        self.failures = self.failures.saturating_add(1);
        let i = u32::try_from(index).unwrap_or(u32::MAX);
        if self.failed_step.is_none() {
            self.failed_step = Some(i);
        }
        match self.workflow.on_failure {
            OnFailure::Stop => {
                let summary = format!("Step {} failed: {reason}.", index + 1);
                Some(self.finish(WorkflowStatus::Failed, &summary))
            }
            OnFailure::Continue => None,
        }
    }

    fn holds(&self, condition: &Condition) -> bool {
        match condition {
            Condition::LastSucceeded => self.last_session.as_ref().is_some_and(|(ok, _)| *ok),
            Condition::LastSummaryContains { text } => {
                let needle = text.trim().to_lowercase();
                !needle.is_empty()
                    && self
                        .last_session
                        .as_ref()
                        .is_some_and(|(_, s)| s.to_lowercase().contains(&needle))
            }
            Condition::OwnerSaidYes => self.last_answer == Some(true),
        }
    }

    fn complete(&mut self) -> WorkflowEffect {
        if self.failures > 0 {
            let summary = format!(
                "Finished with {} failed step{}.",
                self.failures,
                if self.failures == 1 { "" } else { "s" }
            );
            self.finish(WorkflowStatus::CompletedWithFailures, &summary)
        } else {
            let summary = match &self.last_session {
                Some((_, s)) if !s.is_empty() => format!("Done. {s}"),
                _ => "Done.".to_owned(),
            };
            self.finish(WorkflowStatus::Succeeded, &summary)
        }
    }

    fn finish(&mut self, status: WorkflowStatus, summary: &str) -> WorkflowEffect {
        let outcome = WorkflowOutcome {
            status,
            steps_run: self.steps_run,
            summary: truncate_chars(summary, MAX_SUMMARY_CHARS).to_owned(),
            failed_step: self.failed_step,
        };
        self.state = State::Finished(outcome.clone());
        WorkflowEffect::Finished { outcome }
    }
}

fn describe_condition(c: &Condition) -> String {
    match c {
        Condition::LastSucceeded => "\"the last task succeeded\"".to_owned(),
        Condition::LastSummaryContains { text } => {
            format!("\"the last result mentions {}\"", truncate_chars(text, 60))
        }
        Condition::OwnerSaidYes => "\"the owner said yes\"".to_owned(),
    }
}
