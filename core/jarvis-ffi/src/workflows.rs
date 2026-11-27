//! Workflows for Swift: `JarvisWorkflows` (the store), `JarvisWorkflowRun` (one run) and
//! the free functions for schedules and utterance resolution.
//!
//! # How the workflow and skill objects meet
//!
//! Calls that need the skills (`warnings`, `JarvisWorkflowRun.step`, `resolveUtterance`)
//! take the `JarvisSkills` object itself (`Arc<JarvisSkills>`), not its JSON. Passing JSON
//! would re-parse and re-validate the whole skill store (and recompile every trigger) on
//! every step of every run, and could disagree with the store the shell actually edits;
//! the object is the live store, so a skill the owner disables mid-run takes effect at the
//! next skill step, which is what `WorkflowRun` documents. Nothing is retained: the skill
//! store is locked only for the duration of the call.
//!
//! Lock order (no call ever takes them the other way round): a run's lock, then the
//! workflow store's, then the skill store's.
//!
//! Every exported function runs inside [`guarded`]. A poisoned mutex is recovered: the
//! store only changes through whole-value operations that validate first, and a run is
//! restored from a copy taken before the step when that step panics.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard};

use jarvis_skills as sk;
use jarvis_skills::{SkillError, Upsert, WorkflowRun, WorkflowStore};

use crate::error::JarvisError;
use crate::guard::{guarded, panic_message};
use crate::skills::JarvisSkills;
use crate::types::{
    Awaiting, RunLimits, Schedule, ScheduledWorkflow, UtteranceMatch, Weekday, Workflow,
    WorkflowEffect, WorkflowEvent, WorkflowOutcome,
};

/// Maps a workflow or workflow-store error. Schedule errors keep their own case.
pub(crate) fn workflow_error(e: SkillError) -> JarvisError {
    match e {
        SkillError::Schedule(message) => JarvisError::InvalidSchedule { message },
        other => JarvisError::InvalidWorkflow {
            message: other.to_string(),
        },
    }
}

fn schedule_error(e: SkillError) -> JarvisError {
    match e {
        SkillError::Schedule(message) => JarvisError::InvalidSchedule { message },
        other => JarvisError::InvalidSchedule {
            message: other.to_string(),
        },
    }
}

fn lock_recovering<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(poison) => {
            m.clear_poison();
            poison.into_inner()
        }
    }
}

// ---------------------------------------------------------------------------------------
// The store.
// ---------------------------------------------------------------------------------------

struct Inner {
    store: WorkflowStore,
    issues: Vec<String>,
}

/// The owner's workflows. Swift: `JarvisWorkflows`.
#[derive(uniffi::Object)]
pub struct JarvisWorkflows {
    inner: Mutex<Inner>,
}

impl JarvisWorkflows {
    /// Loads a store from snapshot JSON (shared by the FFI constructor and Rust tests).
    pub fn from_json(json: &str) -> Result<Self, JarvisError> {
        let (store, issues) = if json.trim().is_empty() {
            (WorkflowStore::new(), Vec::new())
        } else {
            WorkflowStore::from_json_with_issues(json).map_err(workflow_error)?
        };
        Ok(JarvisWorkflows {
            inner: Mutex::new(Inner { store, issues }),
        })
    }

    fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&mut Inner) -> Result<T, JarvisError>,
    ) -> Result<T, JarvisError> {
        guarded(what, || {
            let mut inner = lock_recovering(&self.inner);
            f(&mut inner)
        })
    }

    /// Runs `f` with the store locked (no guard of its own: callers are guarded).
    pub(crate) fn with_store<T>(&self, f: impl FnOnce(&WorkflowStore) -> T) -> T {
        let inner = lock_recovering(&self.inner);
        f(&inner.store)
    }
}

fn list(ws: Vec<&sk::Workflow>) -> Vec<Workflow> {
    ws.into_iter().cloned().map(Into::into).collect()
}

#[uniffi::export]
impl JarvisWorkflows {
    /// Loads the store from the bytes `toJson()` returned last time (empty for a new
    /// store). A corrupt or newer snapshot throws `JarvisError.invalidWorkflow`; individual
    /// damaged workflows are kept unchanged and listed by `loadIssues()`.
    #[uniffi::constructor]
    pub fn new(json: String) -> Result<Arc<Self>, JarvisError> {
        guarded("JarvisWorkflows.init", || {
            Self::from_json(&json).map(Arc::new)
        })
    }

    /// What loading reported: one line per ignored unknown field and per workflow that
    /// could not be loaded. Unloadable workflows are preserved and written back by
    /// `toJson()` (until `remove(id:)` or an `upsert` with their id replaces them), so
    /// nothing is lost; show these to the owner.
    pub fn load_issues(&self) -> Result<Vec<String>, JarvisError> {
        self.with("load_issues", |i| Ok(i.issues.clone()))
    }

    /// Whether `loadIssues()` is non-empty.
    pub fn has_load_issues(&self) -> Result<bool, JarvisError> {
        self.with("has_load_issues", |i| Ok(!i.issues.is_empty()))
    }

    /// Ids of the entries `loadIssues()` reported as unloadable and kept verbatim (`nil`
    /// for an entry without a readable id). `remove(id:)` discards one; an `upsert` with
    /// its id replaces it.
    pub fn damaged_ids(&self) -> Result<Vec<Option<String>>, JarvisError> {
        self.with("damaged_ids", |i| Ok(i.store.damaged_ids()))
    }

    /// The versioned snapshot to persist (damaged entries last, unchanged).
    pub fn to_json(&self) -> Result<String, JarvisError> {
        self.with("to_json", |i| i.store.to_json().map_err(workflow_error))
    }

    /// All (valid) workflows, ordered by id.
    pub fn list(&self) -> Result<Vec<Workflow>, JarvisError> {
        self.with("list", |i| Ok(list(i.store.list().iter().collect())))
    }

    /// The workflow with this id.
    pub fn get(&self, id: String) -> Result<Option<Workflow>, JarvisError> {
        self.with("get", |i| Ok(i.store.get(&id).cloned().map(Into::into)))
    }

    /// Adds or replaces a workflow at `nowMs`. A new one gets `createdMs = updatedMs =
    /// nowMs` and no runs; an update keeps the stored `createdMs`, `runs`, `lastRunMs` and
    /// `lastOutcome` and sets `updatedMs = nowMs`. Returns true when it was new. Invalid
    /// workflows (id, name, trigger, 1 to 24 steps, texts, waits) and a full store (200)
    /// throw `JarvisError.invalidWorkflow`. Skills named by steps are not checked here:
    /// see `warnings(workflow:skills:)`.
    pub fn upsert(&self, workflow: Workflow, now_ms: u64) -> Result<bool, JarvisError> {
        self.with("upsert", |i| {
            i.store
                .upsert(workflow.into(), now_ms)
                .map(|u| u == Upsert::Inserted)
                .map_err(workflow_error)
        })
    }

    /// Removes a workflow, or an unloadable entry kept with that id; false when there was
    /// neither.
    pub fn remove(&self, id: String) -> Result<bool, JarvisError> {
        self.with("remove", |i| {
            let damaged = i.store.discard_damaged(&id);
            Ok(i.store.remove(&id).is_some() || damaged)
        })
    }

    /// Enables or disables a workflow (an update at `nowMs`). Throws
    /// `JarvisError.invalidWorkflow` for an unknown id.
    pub fn set_enabled(&self, id: String, enabled: bool, now_ms: u64) -> Result<(), JarvisError> {
        self.with("set_enabled", |i| {
            let mut w = i
                .store
                .get(&id)
                .cloned()
                .ok_or_else(|| workflow_error(SkillError::NotFound(id.clone())))?;
            w.enabled = enabled;
            i.store.update(w, now_ms).map_err(workflow_error)
        })
    }

    /// Problems that will fail a step when the workflow runs but do not stop it from being
    /// saved: `runSkill` steps naming a skill that does not exist or is disabled, arguments
    /// the skill does not declare, required parameters left empty, values too long. One
    /// line each, checked against `skills` as it is now.
    pub fn warnings(
        &self,
        workflow: Workflow,
        skills: Arc<JarvisSkills>,
    ) -> Result<Vec<String>, JarvisError> {
        guarded("warnings", || {
            let w: sk::Workflow = workflow.into();
            Ok(skills.with_store(|s| WorkflowStore::warnings(&w, s)))
        })
    }

    /// Enabled workflows whose phrase trigger equals `text` (normalised), ordered by id.
    /// For "what did the owner mean?" use `resolveUtterance`, which also weighs skills.
    pub fn match_phrase(&self, text: String) -> Result<Vec<Workflow>, JarvisError> {
        self.with("match_phrase", |i| Ok(list(i.store.match_phrase(&text))))
    }

    /// Enabled workflows whose external trigger has this key (ASCII case and surrounding
    /// whitespace ignored), ordered by id.
    pub fn for_external(&self, key: String) -> Result<Vec<Workflow>, JarvisError> {
        self.with("for_external", |i| Ok(list(i.store.for_external(&key))))
    }

    /// Enabled scheduled workflows that fired within the last `windowMinutes` minutes of
    /// local time (`weekday`, `minuteOfDay` 0 to 1439), ordered by id. Check every minute
    /// with a window of 1, or after a gap with the minutes since the last check. A minute
    /// of day above 1439 throws `JarvisError.invalidSchedule`.
    pub fn due(
        &self,
        weekday: Weekday,
        minute_of_day: u32,
        window_minutes: u32,
    ) -> Result<Vec<Workflow>, JarvisError> {
        self.with("due", |i| {
            i.store
                .due(weekday.into(), minute_of_day, window_minutes)
                .map(list)
                .map_err(schedule_error)
        })
    }

    /// The enabled scheduled workflow that fires next from local time (`weekday`,
    /// `minuteOfDay`) and the minutes until it does (ties go to the lower id), for
    /// scheduling a wake-up; `nil` when none is scheduled.
    pub fn next_scheduled(
        &self,
        weekday: Weekday,
        minute_of_day: u32,
    ) -> Result<Option<ScheduledWorkflow>, JarvisError> {
        self.with("next_scheduled", |i| {
            let next = i
                .store
                .next_scheduled(weekday.into(), minute_of_day)
                .map_err(schedule_error)?;
            Ok(next.map(|(w, m)| ScheduledWorkflow {
                workflow: w.clone().into(),
                minutes_until: m,
            }))
        })
    }

    /// Records a finished run (`runs`, `lastRunMs`, `lastOutcome`). Call it with the
    /// outcome of the `finished` effect.
    pub fn record_run(
        &self,
        id: String,
        now_ms: u64,
        outcome: WorkflowOutcome,
    ) -> Result<(), JarvisError> {
        self.with("record_run", |i| {
            i.store
                .record_run(&id, now_ms, &outcome.into())
                .map_err(workflow_error)
        })
    }
}

// ---------------------------------------------------------------------------------------
// One run.
// ---------------------------------------------------------------------------------------

/// One run of one workflow. Swift: `JarvisWorkflowRun`. Driven like a session: feed one
/// `WorkflowEvent` at a time, carry out the one `WorkflowEffect` returned.
#[derive(uniffi::Object)]
pub struct JarvisWorkflowRun {
    run: Mutex<WorkflowRun>,
}

impl JarvisWorkflowRun {
    fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&mut WorkflowRun) -> Result<T, JarvisError>,
    ) -> Result<T, JarvisError> {
        guarded(what, || {
            let mut run = lock_recovering(&self.run);
            f(&mut run)
        })
    }

    /// `step` with an optional skill store (none: an empty one).
    fn step_with(
        &self,
        now_ms: u64,
        event: sk::WorkflowEvent,
        skills: Option<&JarvisSkills>,
    ) -> Result<sk::WorkflowEffect, JarvisError> {
        let mut run = lock_recovering(&self.run);
        let backup = run.clone();
        let result = catch_unwind(AssertUnwindSafe(|| match skills {
            Some(s) => s.with_store(|store| run.step(now_ms, event, store)),
            None => run.step(now_ms, event, &sk::SkillStore::new()),
        }));
        match result {
            Ok(Ok(effect)) => Ok(effect),
            Ok(Err(sk::RunError::UnexpectedEvent { got, expected })) => {
                Err(JarvisError::WorkflowProtocol {
                    got: got.to_owned(),
                    expected: expected.to_owned(),
                })
            }
            Err(payload) => {
                // Leave the run exactly as it was before this step.
                *run = backup;
                Err(JarvisError::Internal {
                    message: format!("step: {}", panic_message(payload.as_ref())),
                })
            }
        }
    }
}

#[uniffi::export]
impl JarvisWorkflowRun {
    /// A run of `workflow` (take it from `JarvisWorkflows.get(id:)`) under `limits`
    /// (start from `defaultRunLimits()`). Nothing happens until `step(.start)`; an invalid
    /// or disabled workflow finishes `failed` at `start`.
    #[uniffi::constructor]
    pub fn new(workflow: Workflow, limits: RunLimits) -> Result<Arc<Self>, JarvisError> {
        guarded("JarvisWorkflowRun.init", || {
            Ok(Arc::new(JarvisWorkflowRun {
                run: Mutex::new(WorkflowRun::new(workflow.into(), limits.into())),
            }))
        })
    }

    /// Feeds one event (with the shell's monotonic clock) and returns the next effect.
    /// `skills` is the store as it is now: a `runSkill` step that starts during this call
    /// resolves its skill there (missing or disabled: that step fails with a reason).
    /// An event that does not answer the outstanding effect throws
    /// `JarvisError.workflowProtocol` and leaves the run unchanged. Once finished, every
    /// event returns the same `finished` effect.
    pub fn step(
        &self,
        now_ms: u64,
        event: WorkflowEvent,
        skills: Arc<JarvisSkills>,
    ) -> Result<WorkflowEffect, JarvisError> {
        guarded("step", || {
            self.step_with(now_ms, event.into(), Some(&skills))
                .map(Into::into)
        })
    }

    /// Ends the run: cancels it (status `cancelled`) unless it already finished, and
    /// returns its outcome. Idempotent.
    pub fn finish(&self, now_ms: u64) -> Result<WorkflowOutcome, JarvisError> {
        guarded("finish", || {
            match self.step_with(now_ms, sk::WorkflowEvent::Cancel, None)? {
                sk::WorkflowEffect::Finished { outcome } => Ok(outcome.into()),
                other => Err(JarvisError::Internal {
                    message: format!("finish: cancel returned {other:?}"),
                }),
            }
        })
    }

    /// What the run is waiting for.
    pub fn awaiting(&self) -> Result<Awaiting, JarvisError> {
        self.with("awaiting", |r| Ok(r.awaiting().into()))
    }

    /// Whether the run is over.
    pub fn is_finished(&self) -> Result<bool, JarvisError> {
        self.with("is_finished", |r| Ok(r.outcome().is_some()))
    }

    /// The outstanding effect (the last one returned), e.g. to restore the UI.
    pub fn pending(&self) -> Result<Option<WorkflowEffect>, JarvisError> {
        self.with("pending", |r| Ok(r.pending().cloned().map(Into::into)))
    }

    /// The outcome once finished.
    pub fn outcome(&self) -> Result<Option<WorkflowOutcome>, JarvisError> {
        self.with("outcome", |r| Ok(r.outcome().cloned().map(Into::into)))
    }

    /// 0-based index of the step in progress, if any.
    pub fn current_step(&self) -> Result<Option<u32>, JarvisError> {
        self.with("current_step", |r| {
            Ok(r.current_step()
                .map(|i| u32::try_from(i).unwrap_or(u32::MAX)))
        })
    }

    /// The workflow being run (as it was when the run was created).
    pub fn workflow(&self) -> Result<Workflow, JarvisError> {
        self.with("workflow", |r| Ok(r.workflow().clone().into()))
    }
}

// ---------------------------------------------------------------------------------------
// Free functions.
// ---------------------------------------------------------------------------------------

/// The default run limits: 24 steps, one hour in total, 30 minutes per wait.
#[uniffi::export]
pub fn default_run_limits() -> Result<RunLimits, JarvisError> {
    guarded("default_run_limits", || Ok(sk::RunLimits::default().into()))
}

/// The weekday for 0 (Monday) to 6 (Sunday); above 6 throws `JarvisError.invalidSchedule`.
/// From Swift's `Calendar` (1 = Sunday … 7 = Saturday): `(component + 5) % 7`.
#[uniffi::export]
pub fn weekday_from_index(index: u32) -> Result<Weekday, JarvisError> {
    guarded("weekday_from_index", || {
        sk::Weekday::from_index(index)
            .map(Into::into)
            .map_err(schedule_error)
    })
}

/// Minutes from local time (`weekday`, `minuteOfDay`) until the schedule next fires (0:
/// now). Out-of-range input throws `JarvisError.invalidSchedule`, never clamped.
#[uniffi::export]
pub fn schedule_minutes_until_next(
    schedule: Schedule,
    weekday: Weekday,
    minute_of_day: u32,
) -> Result<u32, JarvisError> {
    guarded("schedule_minutes_until_next", || {
        sk::minutes_until_next(&schedule.into(), weekday.into(), minute_of_day)
            .map_err(schedule_error)
    })
}

/// Minutes since the schedule last fired at or before local time (0: now).
#[uniffi::export]
pub fn schedule_minutes_since_last(
    schedule: Schedule,
    weekday: Weekday,
    minute_of_day: u32,
) -> Result<u32, JarvisError> {
    guarded("schedule_minutes_since_last", || {
        sk::minutes_since_last(&schedule.into(), weekday.into(), minute_of_day)
            .map_err(schedule_error)
    })
}

/// Whether the schedule fired within the last `windowMinutes` minutes, now included.
#[uniffi::export]
pub fn schedule_is_due(
    schedule: Schedule,
    weekday: Weekday,
    minute_of_day: u32,
    window_minutes: u32,
) -> Result<bool, JarvisError> {
    guarded("schedule_is_due", || {
        sk::is_due(
            &schedule.into(),
            weekday.into(),
            minute_of_day,
            window_minutes,
        )
        .map_err(schedule_error)
    })
}

/// "What did the owner mean?" across workflow phrases and skill triggers, with the core's
/// precedence: an exact workflow phrase wins over loose skill matches; two workflows, or a
/// workflow and an exact skill, need the owner's choice; without a workflow a single clear
/// exact skill runs; nothing matched is an ordinary task (`noMatch`).
#[uniffi::export]
pub fn resolve_utterance(
    skills: Arc<JarvisSkills>,
    workflows: Arc<JarvisWorkflows>,
    text: String,
) -> Result<UtteranceMatch, JarvisError> {
    guarded("resolve_utterance", || {
        Ok(workflows
            .with_store(|w| skills.with_store(|s| sk::resolve_utterance(s, w, &text)))
            .into())
    })
}
