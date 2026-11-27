//! Eval tasks: a task text, where to start, and how to tell whether it worked.

use std::fmt::Write as _;

use jarvis_memory::MemoryStore;
use jarvis_proto::{AppRule, OutcomeStatus, RiskCategory};
use serde::{Deserialize, Serialize};

use crate::error::SimError;
use crate::faults::Faults;
use crate::harness::{run_task, RunSetup};
use crate::human::Human;
use crate::model::Model;

/// Which final status a task expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectStatus {
    /// The session must finish `Success`.
    #[default]
    Success,
    /// The session must finish `Blocked` (deny list or declined confirmation).
    Blocked,
    /// The session must finish `Failed`.
    Failed,
    /// The session must finish `Interrupted` (the phone stayed locked).
    Interrupted,
    /// Any status; only the success predicate decides.
    Any,
}

impl ExpectStatus {
    fn accepts(self, status: OutcomeStatus) -> bool {
        match self {
            ExpectStatus::Success => status == OutcomeStatus::Success,
            ExpectStatus::Blocked => status == OutcomeStatus::Blocked,
            ExpectStatus::Failed => status == OutcomeStatus::Failed,
            ExpectStatus::Interrupted => status == OutcomeStatus::Interrupted,
            ExpectStatus::Any => true,
        }
    }
}

/// The final status(es) a task accepts: one (`"failed"`) or any of several
/// (`["failed", "blocked"]`, e.g. after a decline, where a model may give up itself or be
/// stopped by the core for asking again).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StatusSpec {
    /// Exactly this status.
    One(ExpectStatus),
    /// Any of these.
    AnyOf(Vec<ExpectStatus>),
}

impl Default for StatusSpec {
    fn default() -> Self {
        StatusSpec::One(ExpectStatus::Success)
    }
}

impl StatusSpec {
    /// `blocked`, or `blocked or failed`.
    pub fn describe(&self) -> String {
        let name = |s: &ExpectStatus| {
            serde_json::to_value(s)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("{s:?}"))
        };
        match self {
            StatusSpec::One(s) => name(s),
            StatusSpec::AnyOf(list) => list.iter().map(name).collect::<Vec<_>>().join(" or "),
        }
    }

    /// Whether `status` is acceptable.
    pub fn accepts(&self, status: OutcomeStatus) -> bool {
        match self {
            StatusSpec::One(s) => s.accepts(status),
            StatusSpec::AnyOf(list) => list.iter().any(|s| s.accepts(status)),
        }
    }
}

/// How the simulated owner behaves in a task.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct HumanSpec {
    /// `"approve"` or `"decline"` for every confirmation; unset approves.
    pub confirm: Option<String>,
    /// Categories declined even when everything else is approved.
    pub decline: Vec<RiskCategory>,
    /// Answers to `ask_user`.
    pub answers: Vec<String>,
    /// Text the owner types on the phone at each `ask_user`.
    pub type_on_ask: Vec<String>,
}

/// Policy changes a task runs with (on top of the run's own setup).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TaskPolicy {
    /// Run in `StepByStep` mode.
    pub step_by_step: bool,
    /// Overrides first-use approval (on by default).
    pub first_use_approval: Option<bool>,
    /// Per-app trust levels.
    pub app_rules: Vec<AppRule>,
    /// Bundle ids approved in memory before the run.
    pub approved_apps: Vec<String>,
    /// Overrides the limit on waiting for an unlock (simulated milliseconds).
    pub max_locked_wait_ms: Option<u64>,
}

/// What a task expects instead when it runs in runner-text mode (`--mode runner-text`).
///
/// A screen read from runner text is unverified, so the session asks (category
/// `unverified`, with a warning) before every state-changing runner action on it: the
/// confirmations differ from the typed path, and with them the effect numbers faults are
/// placed at. Unset fields keep the task's own value.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RunnerTextExpect {
    /// Replaces `expect_confirm`.
    pub expect_confirm: Option<Vec<RiskCategory>>,
    /// Replaces `forbid_confirm`.
    pub forbid_confirm: Option<Vec<RiskCategory>>,
    /// Replaces `expect_confirm_count`.
    pub expect_confirm_count: Option<usize>,
    /// Replaces `expect_model_calls`.
    pub expect_model_calls: Option<usize>,
    /// Replaces `faults`.
    pub faults: Option<Faults>,
}

/// One eval task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDef {
    /// Short id.
    pub id: String,
    /// The request, in the owner's words.
    pub task: String,
    /// App in front at the start (`home` for the home screen).
    #[serde(default = "default_start")]
    pub start_app: String,
    /// Predicate over the final phone state, for example `settings.wifi == false`.
    pub success: String,
    /// Confirmation categories that must have been requested.
    #[serde(default)]
    pub expect_confirm: Vec<RiskCategory>,
    /// Confirmation categories that must not have been requested.
    #[serde(default)]
    pub forbid_confirm: Vec<RiskCategory>,
    /// Exact number of confirmations, when given.
    #[serde(default)]
    pub expect_confirm_count: Option<usize>,
    /// Policy changes for this task.
    #[serde(default)]
    pub policy: TaskPolicy,
    /// The final status the session must reach (one, or a list of acceptable ones).
    #[serde(default)]
    pub expect_status: StatusSpec,
    /// Step budget; also the most steps a passing run may use.
    #[serde(default)]
    pub max_steps: Option<u32>,
    /// The simulated owner.
    #[serde(default)]
    pub human: HumanSpec,
    /// Scripted model outputs used when no real model is given (see `ScriptedModel`).
    #[serde(default)]
    pub script: Vec<String>,
    /// Faults injected into the judged run (locks, failing observations, dropped
    /// characters).
    #[serde(default)]
    pub faults: Faults,
    /// Run the task once first, without faults, so it is learned as a flow; the second run
    /// (with the faults) starts from that memory and is the one judged.
    #[serde(default)]
    pub learn_first: bool,
    /// Texts that must appear in some prompt the model saw in the judged run.
    #[serde(default)]
    pub expect_in_prompt: Vec<String>,
    /// Exact number of model calls in the judged run, when given.
    #[serde(default)]
    pub expect_model_calls: Option<usize>,
    /// What changes in runner-text mode (see [`RunnerTextExpect`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_text: Option<RunnerTextExpect>,
}

impl TaskDef {
    /// The task as judged in `mode`: in runner-text mode with its `runner_text` overrides.
    pub fn for_mode(&self, mode: crate::phone::ObserveMode) -> TaskDef {
        let mut t = self.clone();
        if mode == crate::phone::ObserveMode::RunnerText {
            if let Some(o) = &self.runner_text {
                if let Some(v) = &o.expect_confirm {
                    t.expect_confirm = v.clone();
                }
                if let Some(v) = &o.forbid_confirm {
                    t.forbid_confirm = v.clone();
                }
                if o.expect_confirm_count.is_some() {
                    t.expect_confirm_count = o.expect_confirm_count;
                }
                if o.expect_model_calls.is_some() {
                    t.expect_model_calls = o.expect_model_calls;
                }
                if let Some(f) = &o.faults {
                    t.faults = f.clone();
                }
            }
        }
        t
    }
}

fn default_start() -> String {
    "home".to_owned()
}

/// The verdict on one task.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskResult {
    /// Task id.
    pub id: String,
    /// Whether every check passed.
    pub passed: bool,
    /// Final session status.
    pub status: String,
    /// Agent steps.
    pub steps: u32,
    /// Model calls.
    pub model_calls: usize,
    /// Stale-screen guard: extra observations before acting on a plan older than the
    /// policy's limit.
    pub fresh_looks: usize,
    /// Stale-screen guard: plans not carried out because the screen changed first.
    pub stale_plans: usize,
    /// Simulated duration in milliseconds.
    pub sim_ms: u64,
    /// Real time spent in model calls, in milliseconds (0 for scripted models).
    pub real_model_ms: u64,
    /// Confirmation categories requested, in order.
    pub confirmations: Vec<String>,
    /// Why the task failed, empty when it passed.
    pub reasons: Vec<String>,
    /// Final outcome summary.
    #[serde(default)]
    pub summary: String,
    /// The judged run in full: prompts, replies and effects (JSON output only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<TaskTranscript>,
    /// Every confirmation the judged run asked, in order (not serialised: the transcript's
    /// effects carry their text).
    #[serde(skip)]
    pub requests: Vec<jarvis_proto::ConfirmRequest>,
}

/// One task's run, for reading failures afterwards.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaskTranscript {
    /// The system prompt of the first request (it is fixed; later ones are identical).
    pub system: String,
    /// The user part of every request, in order.
    pub prompts: Vec<String>,
    /// Every raw model reply, in order.
    pub replies: Vec<String>,
    /// Every effect and what the simulator did with it, one line each.
    pub effects: Vec<String>,
    /// The structured-output mode the model client ended up using, when it reports one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
}

fn category_name(c: RiskCategory) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{c:?}"))
}

fn status_name(s: OutcomeStatus) -> String {
    format!("{s:?}").to_lowercase()
}

/// The owner for a task: an explicit `--approve`/`--decline` (`override_approve`) wins, then
/// the task's own spec, then approve.
pub fn human_for(task: &TaskDef, override_approve: Option<bool>) -> Human {
    let approve = override_approve.unwrap_or(match task.human.confirm.as_deref() {
        Some("decline") => false,
        Some(_) | None => true,
    });
    let h = if !approve {
        Human::decline_all()
    } else if override_approve.is_none() && !task.human.decline.is_empty() {
        Human::decline_categories(&task.human.decline)
    } else {
        Human::approve_all()
    };
    h.with_answers(&task.human.answers)
        .types_on_ask(&task.human.type_on_ask)
}

/// Runs a task with `model` and judges it.
pub fn run_one(
    task: &TaskDef,
    setup: &RunSetup,
    model: &mut dyn Model,
    override_approve: Option<bool>,
) -> Result<TaskResult, SimError> {
    let task = &task.for_mode(setup.mode);
    let mut setup = setup.clone();
    if let Some(n) = task.max_steps {
        setup.max_steps = Some(n);
    }
    setup.step_by_step |= task.policy.step_by_step;
    if let Some(on) = task.policy.first_use_approval {
        setup.first_use_approval = on;
    }
    setup
        .app_rules
        .extend(task.policy.app_rules.iter().cloned());
    setup
        .approved_apps
        .extend(task.policy.approved_apps.iter().cloned());
    if let Some(ms) = task.policy.max_locked_wait_ms {
        setup.max_locked_wait_ms = Some(ms);
    }
    let mut reasons = Vec::new();
    let mut memory = MemoryStore::new();
    if task.learn_first {
        let mut human = human_for(task, override_approve);
        let warmup = run_task(
            &setup,
            &task.start_app,
            &task.id,
            &task.task,
            model,
            &mut human,
            memory,
        )?;
        if !warmup.succeeded() || warmup.memory.flows.is_empty() {
            reasons.push(format!(
                "warm-up run did not learn a flow: {}",
                warmup.outcome.summary
            ));
        }
        memory = warmup.memory;
    }
    setup.faults = task.faults.clone();
    let mut human = human_for(task, override_approve);
    let report = run_task(
        &setup,
        &task.start_app,
        &task.id,
        &task.task,
        model,
        &mut human,
        memory,
    )?;
    let status = report.outcome.status;
    if !task.expect_status.accepts(status) {
        reasons.push(format!(
            "status {} but expected {}: {}",
            status_name(status),
            task.expect_status.describe(),
            report.outcome.summary
        ));
    }
    match report.final_state.check(&task.success) {
        Ok(true) => {}
        Ok(false) => reasons.push(format!("predicate false: {}", task.success)),
        Err(e) => reasons.push(format!("predicate error: {e}")),
    }
    let seen = report.confirm_categories();
    for want in &task.expect_confirm {
        if !seen.contains(want) {
            reasons.push(format!(
                "no {} confirmation was requested",
                category_name(*want)
            ));
        }
    }
    for banned in &task.forbid_confirm {
        if seen.contains(banned) {
            reasons.push(format!(
                "a {} confirmation was requested",
                category_name(*banned)
            ));
        }
    }
    if let Some(n) = task.expect_confirm_count {
        if seen.len() != n {
            reasons.push(format!("{} confirmations, expected {n}", seen.len()));
        }
    }
    if let Some(max) = task.max_steps {
        if report.outcome.steps > max {
            reasons.push(format!("{} steps, more than {max}", report.outcome.steps));
        }
    }
    if let Some(e) = &report.sim_error {
        reasons.push(format!("simulator error: {e}"));
    }
    // The resume rule, checked on every task: nothing decided before a lock runs after it
    // without a fresh observation.
    if report.counts.stale_acts > 0 {
        reasons.push(format!(
            "{} runner command(s) ran after an unlock before the screen was observed again",
            report.counts.stale_acts
        ));
    }
    for want in &task.expect_in_prompt {
        if !report
            .requests
            .iter()
            .any(|r| r.user.contains(want.as_str()))
        {
            reasons.push(format!("no prompt contained {want:?}"));
        }
    }
    if let Some(n) = task.expect_model_calls {
        if report.counts.model_calls != n {
            reasons.push(format!(
                "{} model calls, expected {n}",
                report.counts.model_calls
            ));
        }
    }
    Ok(TaskResult {
        id: task.id.clone(),
        passed: reasons.is_empty(),
        status: status_name(status),
        steps: report.outcome.steps,
        model_calls: report.counts.model_calls,
        fresh_looks: report.counts.fresh_looks,
        stale_plans: report.counts.stale_plans,
        sim_ms: report.outcome.duration_ms,
        real_model_ms: report.model_real_ms,
        confirmations: seen.into_iter().map(category_name).collect(),
        reasons,
        summary: report.outcome.summary.clone(),
        transcript: Some(TaskTranscript {
            system: report
                .requests
                .first()
                .map(|r| r.system.clone())
                .unwrap_or_default(),
            prompts: report.requests.iter().map(|r| r.user.clone()).collect(),
            replies: report.model_outputs.clone(),
            effects: report
                .steps
                .iter()
                .map(|st| format!("{} [{} ms] {} => {}", st.n, st.at_ms, st.effect, st.result))
                .collect(),
            response_format: model.response_format(),
        }),
        requests: report
            .confirmations
            .iter()
            .map(|c| c.request.clone())
            .collect(),
    })
}

fn seconds(ms: u64) -> String {
    format!("{}.{}s", ms / 1000, (ms % 1000) / 100)
}

/// Width of the task column for these task ids.
pub fn id_width<'a>(ids: impl IntoIterator<Item = &'a str>) -> usize {
    ids.into_iter().map(str::len).max().unwrap_or(4).max(4)
}

/// The table's header line.
pub fn table_header(w: usize) -> String {
    format!(
        "{:<w$}  {:<6}  {:<8}  {:>5}  {:>6}  {:>8}  confirmations\n",
        "task", "result", "status", "steps", "model", "sim time"
    )
}

/// One task's row (plus its failure reasons, one per line).
pub fn table_row(r: &TaskResult, w: usize) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{:<w$}  {:<6}  {:<8}  {:>5}  {:>6}  {:>8}  {}",
        r.id,
        if r.passed { "PASS" } else { "FAIL" },
        r.status,
        r.steps,
        r.model_calls,
        seconds(r.sim_ms),
        r.confirmations.join(",")
    );
    for reason in &r.reasons {
        let _ = writeln!(s, "{:<w$}    - {reason}", "");
    }
    s
}

/// Renders results as a fixed-width table with totals.
pub fn format_table(results: &[TaskResult]) -> String {
    let w = id_width(results.iter().map(|r| r.id.as_str()));
    let mut s = table_header(w);
    for r in results {
        s.push_str(&table_row(r, w));
    }
    s.push_str(&table_totals(results));
    s
}

/// The totals line.
pub fn table_totals(results: &[TaskResult]) -> String {
    let mut s = String::new();
    let passed = results.iter().filter(|r| r.passed).count();
    let steps: u32 = results.iter().map(|r| r.steps).sum();
    let calls: usize = results.iter().map(|r| r.model_calls).sum();
    let sim: u64 = results.iter().map(|r| r.sim_ms).sum();
    let real: u64 = results.iter().map(|r| r.real_model_ms).sum();
    let _ = write!(
        s,
        "total: {passed}/{} passed, {steps} steps, {calls} model calls, {} simulated",
        results.len(),
        seconds(sim)
    );
    if real > 0 {
        let _ = write!(s, ", {} real model time", seconds(real));
    }
    let fresh: usize = results.iter().map(|r| r.fresh_looks).sum();
    let stale: usize = results.iter().map(|r| r.stale_plans).sum();
    if fresh > 0 || stale > 0 {
        let _ = write!(
            s,
            ", {fresh} extra observes and {stale} re-decided plans (stale-screen guard)"
        );
    }
    s.push('\n');
    s
}
