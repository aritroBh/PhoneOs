//! Eval tasks: a task text, where to start, and how to tell whether it worked.

use std::fmt::Write as _;

use jarvis_memory::MemoryStore;
use jarvis_proto::{OutcomeStatus, RiskCategory};
use serde::{Deserialize, Serialize};

use crate::error::SimError;
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
    /// Any status; only the success predicate decides.
    Any,
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
    /// The final status the session must reach.
    #[serde(default)]
    pub expect_status: ExpectStatus,
    /// Step budget; also the most steps a passing run may use.
    #[serde(default)]
    pub max_steps: Option<u32>,
    /// The simulated owner.
    #[serde(default)]
    pub human: HumanSpec,
    /// Scripted model outputs used when no real model is given (see `ScriptedModel`).
    #[serde(default)]
    pub script: Vec<String>,
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
    /// Simulated duration in milliseconds.
    pub sim_ms: u64,
    /// Real time spent in model calls, in milliseconds (0 for scripted models).
    pub real_model_ms: u64,
    /// Confirmation categories requested, in order.
    pub confirmations: Vec<String>,
    /// Why the task failed, empty when it passed.
    pub reasons: Vec<String>,
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
    let mut setup = setup.clone();
    if let Some(n) = task.max_steps {
        setup.max_steps = Some(n);
    }
    let mut human = human_for(task, override_approve);
    let report = run_task(
        &setup,
        &task.start_app,
        &task.id,
        &task.task,
        model,
        &mut human,
        MemoryStore::new(),
    )?;
    let mut reasons = Vec::new();
    let status = report.outcome.status;
    let status_ok = match task.expect_status {
        ExpectStatus::Success => status == OutcomeStatus::Success,
        ExpectStatus::Blocked => status == OutcomeStatus::Blocked,
        ExpectStatus::Failed => status == OutcomeStatus::Failed,
        ExpectStatus::Any => true,
    };
    if !status_ok {
        reasons.push(format!(
            "status {} but expected {:?}: {}",
            status_name(status),
            task.expect_status,
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
    if let Some(max) = task.max_steps {
        if report.outcome.steps > max {
            reasons.push(format!("{} steps, more than {max}", report.outcome.steps));
        }
    }
    if let Some(e) = &report.sim_error {
        reasons.push(format!("simulator error: {e}"));
    }
    Ok(TaskResult {
        id: task.id.clone(),
        passed: reasons.is_empty(),
        status: status_name(status),
        steps: report.outcome.steps,
        model_calls: report.counts.model_calls,
        sim_ms: report.outcome.duration_ms,
        real_model_ms: report.model_real_ms,
        confirmations: seen.into_iter().map(category_name).collect(),
        reasons,
    })
}

fn seconds(ms: u64) -> String {
    format!("{}.{}s", ms / 1000, (ms % 1000) / 100)
}

/// Renders results as a fixed-width table with totals.
pub fn format_table(results: &[TaskResult]) -> String {
    let w = results.iter().map(|r| r.id.len()).max().unwrap_or(4).max(4);
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{:<w$}  {:<6}  {:<8}  {:>5}  {:>6}  {:>8}  confirmations",
        "task", "result", "status", "steps", "model", "sim time"
    );
    for r in results {
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
    }
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
    s.push('\n');
    s
}
