//! The loop that turns the session's effects into simulated IO and back into events.

use std::fmt::Write as _;

use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::{
    ConfirmRequest, Effect, Event, ModelOutput, ModelRequest, Outcome, OutcomeStatus, RiskCategory,
    RunnerCommand, TimedEvent, ToolResult,
};

use crate::human::Human;
use crate::model::Model;
use crate::phone::{SimObservation, SimPhone, SimSnapshot};

/// Limits and fault injection for one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// Hard cap on effect/event round trips.
    pub max_iterations: usize,
    /// Deliver `Event::Cancel` instead of the reply to the n-th effect (0-based).
    pub cancel_at_effect: Option<usize>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_iterations: 2_000,
            cancel_at_effect: None,
        }
    }
}

/// One effect and what the simulator did with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptStep {
    /// 1-based position.
    pub n: usize,
    /// Simulated time when the effect was produced.
    pub at_ms: u64,
    /// The effect.
    pub effect: String,
    /// What the simulator did and what came back.
    pub result: String,
}

/// Counters for a run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunCounts {
    /// Effects the session produced.
    pub effects: usize,
    /// Observations taken.
    pub observes: usize,
    /// Model calls made.
    pub model_calls: usize,
    /// Model calls that failed.
    pub model_failures: usize,
    /// Re-inferences after unparsable output (the session's `Progress::parse_retries`).
    pub parse_retries: usize,
    /// Runner commands sent.
    pub acts: usize,
    /// Runner commands that failed.
    pub act_failures: usize,
    /// Confirmation requests.
    pub confirmations: usize,
    /// Confirmations the owner approved.
    pub approvals: usize,
    /// Questions asked of the owner.
    pub questions: usize,
    /// `AwaitUnlock` effects (each one answered by a timer tick or the unlock).
    pub unlock_waits: usize,
    /// Times the phone locked.
    pub locks: usize,
    /// Runner commands refused because the phone was locked.
    pub locked_refusals: usize,
    /// Runner commands carried out after an unlock without a fresh observation first.
    /// The core's resume rule says this must be 0.
    pub stale_acts: usize,
    /// Observations that failed (`ObserveFailed`).
    pub observe_failures: usize,
}

/// A confirmation request and the owner's decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmRecord {
    /// What was asked.
    pub request: ConfirmRequest,
    /// The decision.
    pub approved: bool,
    /// The phone locked while this was asked, so no decision was given (`approved` is
    /// false); the session asks again after the unlock when it is still relevant.
    pub interrupted: bool,
}

/// Everything a run produced.
#[derive(Debug, Clone)]
pub struct RunReport {
    /// How the session ended.
    pub outcome: Outcome,
    /// True when the iteration cap ended the run instead of the session.
    pub capped: bool,
    /// Set when the simulator itself failed (not the agent).
    pub sim_error: Option<String>,
    /// One entry per effect.
    pub steps: Vec<TranscriptStep>,
    /// Counters.
    pub counts: RunCounts,
    /// Confirmations in order.
    pub confirmations: Vec<ConfirmRecord>,
    /// Questions asked of the owner.
    pub questions: Vec<String>,
    /// Every runner command sent.
    pub commands: Vec<RunnerCommand>,
    /// Every model request.
    pub requests: Vec<ModelRequest>,
    /// Every raw model output.
    pub model_outputs: Vec<String>,
    /// The phone at the end.
    pub final_state: SimSnapshot,
    /// Memory after the session (flows, log).
    pub memory: MemoryStore,
    /// Real milliseconds spent in model calls, when the model reports them.
    pub model_real_ms: u64,
}

impl RunReport {
    /// Whether the session ended `Success`.
    pub fn succeeded(&self) -> bool {
        self.outcome.status == OutcomeStatus::Success
    }

    /// Categories of the confirmations that were requested, in order.
    pub fn confirm_categories(&self) -> Vec<RiskCategory> {
        self.confirmations
            .iter()
            .map(|c| c.request.category)
            .collect()
    }

    /// The readable transcript, one block per effect.
    pub fn transcript(&self) -> String {
        let mut s = String::new();
        for st in &self.steps {
            let _ = writeln!(s, "{:>3}. [{:>7} ms] {}", st.n, st.at_ms, st.effect);
            for line in st.result.lines() {
                let _ = writeln!(s, "         {line}");
            }
        }
        let _ = writeln!(
            s,
            "outcome: {:?} after {} steps, {} ms: {}",
            self.outcome.status, self.outcome.steps, self.outcome.duration_ms, self.outcome.summary
        );
        if self.capped {
            let _ = writeln!(s, "(the driver iteration cap ended this run)");
        }
        if let Some(e) = &self.sim_error {
            let _ = writeln!(s, "simulator error: {e}");
        }
        s
    }
}

fn one_line(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut out: String = flat.chars().take(max).collect();
    if flat.chars().count() > max {
        out.push('…');
    }
    out
}

fn describe_command(c: &RunnerCommand) -> String {
    match c {
        RunnerCommand::Tap { x, y } => format!("tap ({x:.0}, {y:.0})"),
        RunnerCommand::LongPress { x, y, duration_ms } => {
            format!("long_press ({x:.0}, {y:.0}) {duration_ms} ms")
        }
        RunnerCommand::Swipe { from, to, .. } => format!(
            "swipe ({:.0}, {:.0}) -> ({:.0}, {:.0})",
            from.x, from.y, to.x, to.y
        ),
        RunnerCommand::TypeText { text } => format!("type_text {:?}", one_line(text, 60)),
        RunnerCommand::PressKey { key } => format!("press_key {key:?}"),
        RunnerCommand::LaunchApp { bundle_id } => format!("launch_app {bundle_id}"),
        RunnerCommand::OpenUrl { url } => format!("open_url {url}"),
        RunnerCommand::Wait { ms } => format!("wait {ms} ms"),
    }
}

/// Drives `session` against the simulated phone, model and owner until it finishes.
pub fn run(
    session: Session,
    phone: &mut SimPhone,
    model: &mut dyn Model,
    human: &mut Human,
) -> RunReport {
    run_with(session, phone, model, human, &RunOptions::default())
}

/// [`run`] with explicit limits and fault injection.
pub fn run_with(
    mut session: Session,
    phone: &mut SimPhone,
    model: &mut dyn Model,
    human: &mut Human,
    opts: &RunOptions,
) -> RunReport {
    let mut report = RunReport {
        outcome: Outcome {
            status: OutcomeStatus::Failed,
            summary: String::new(),
            steps: 0,
            duration_ms: 0,
        },
        capped: false,
        sim_error: None,
        steps: Vec::new(),
        counts: RunCounts::default(),
        confirmations: Vec::new(),
        questions: Vec::new(),
        commands: Vec::new(),
        requests: Vec::new(),
        model_outputs: Vec::new(),
        final_state: phone.snapshot(),
        memory: MemoryStore::new(),
        model_real_ms: 0,
    };
    let mut event = Event::Start;
    let mut finished = false;
    for i in 0..opts.max_iterations {
        if opts.cancel_at_effect == Some(i) {
            event = Event::Cancel;
        }
        let at_ms = phone.now_ms();
        let effect = session.step(TimedEvent::new(at_ms, event));
        report.counts.effects += 1;
        let costs = phone.costs();
        let interrupts = !matches!(effect, Effect::Finished(_) | Effect::AwaitUnlock { .. });
        if interrupts && phone.lock_at_effect(i) {
            // The lock notification arrives while this effect is pending: the shell
            // abandons it and tells the session.
            report.steps.push(TranscriptStep {
                n: report.steps.len() + 1,
                at_ms,
                effect: effect.name().to_owned(),
                result: "the phone locked before this was done: device_locked".to_owned(),
            });
            if let Effect::Confirm(request) = effect {
                report.counts.confirmations += 1;
                report.confirmations.push(ConfirmRecord {
                    request,
                    approved: false,
                    interrupted: true,
                });
            }
            event = Event::DeviceLocked;
            continue;
        }
        let (label, result, next) = match effect {
            Effect::AwaitUnlock { reason, waited_ms } => {
                report.counts.unlock_waits += 1;
                let poll = phone.faults().poll_ms();
                let label = format!("await_unlock (waited {waited_ms} ms): {reason}");
                if phone.wait_for_unlock(poll) {
                    (
                        label,
                        format!("owner unlocks at {} ms", phone.now_ms()),
                        Some(Event::DeviceUnlocked),
                    )
                } else {
                    (
                        label,
                        format!("still locked at {} ms: tick", phone.now_ms()),
                        Some(Event::Tick),
                    )
                }
            }
            Effect::Observe { screenshot } => {
                report.counts.observes += 1;
                phone.advance(costs.observe_ms);
                match phone.observe_result() {
                    Ok(SimObservation::Locked(obs)) => (
                        "observe".to_owned(),
                        "the phone is locked: screen_locked".to_owned(),
                        Some(Event::Observed(obs)),
                    ),
                    Ok(SimObservation::Failed(reason)) => {
                        report.counts.observe_failures += 1;
                        (
                            "observe".to_owned(),
                            format!("FAILED: {reason}"),
                            Some(Event::ObserveFailed { reason }),
                        )
                    }
                    Ok(SimObservation::Screen(obs)) => {
                        let snap = phone.snapshot();
                        let kb = if obs.screen.keyboard_visible {
                            ", keyboard up"
                        } else {
                            ""
                        };
                        let shot = if screenshot {
                            " (screenshot requested: none available in the simulator)"
                        } else {
                            ""
                        };
                        (
                            format!("observe{shot}"),
                            format!(
                                "{}/{}{}: {} nodes{kb} [{}]",
                                snap.app,
                                snap.screen,
                                snap.alert.map_or(String::new(), |a| format!(" alert {a}")),
                                obs.screen.root.count(),
                                phone.mode().name(),
                            ),
                            Some(Event::Observed(obs)),
                        )
                    }
                    Err(e) => {
                        report.sim_error = Some(e.to_string());
                        ("observe".to_owned(), format!("FAILED: {e}"), None)
                    }
                }
            }
            Effect::Infer(req) => {
                report.counts.model_calls += 1;
                phone.advance(costs.infer_ms);
                let kind = format!("{:?}", req.kind).to_lowercase();
                let label = format!(
                    "infer ({kind}, ~{} prompt chars)",
                    req.system.len() + req.user.len()
                );
                let answer = model.generate(&req);
                if let Some(ms) = model.take_real_ms() {
                    report.model_real_ms = report.model_real_ms.saturating_add(ms);
                }
                let note = model
                    .take_note()
                    .map_or(String::new(), |n| format!(" ({n})"));
                report.requests.push(req);
                match answer {
                    Ok(out) => {
                        report.model_outputs.push(out.text.clone());
                        (
                            label,
                            format!("model{note}: {}", one_line(&out.text, 200)),
                            Some(Event::ModelOutput(ModelOutput { text: out.text })),
                        )
                    }
                    Err(e) => {
                        report.counts.model_failures += 1;
                        (
                            label,
                            format!("model FAILED{note}: {e}"),
                            Some(Event::ModelFailed {
                                error: e.to_string(),
                            }),
                        )
                    }
                }
            }
            Effect::Act(cmd) => {
                report.counts.acts += 1;
                phone.advance(costs.act_ms);
                let out = phone.act(&cmd);
                if !out.result.ok {
                    report.counts.act_failures += 1;
                }
                let label = format!("act {}", describe_command(&cmd));
                report.commands.push(cmd);
                (label, out.note, Some(Event::ActResult(out.result)))
            }
            Effect::CallTool(call) => {
                phone.advance(costs.tool_ms);
                (
                    format!("call_tool {}", call.name),
                    "the simulator has no tools".to_owned(),
                    Some(Event::ToolResult(ToolResult {
                        ok: false,
                        output: String::new(),
                        error: Some("no tools are available in the simulator".to_owned()),
                    })),
                )
            }
            Effect::Confirm(req) => {
                report.counts.confirmations += 1;
                phone.advance(costs.user_ms);
                let approved = human.confirm(&req);
                if approved {
                    report.counts.approvals += 1;
                }
                let label = format!(
                    "confirm [{:?}] {} (app {})",
                    req.category, req.summary, req.app
                );
                report.confirmations.push(ConfirmRecord {
                    request: req,
                    approved,
                    interrupted: false,
                });
                (
                    label,
                    if approved {
                        "owner approves".to_owned()
                    } else {
                        "owner declines".to_owned()
                    },
                    Some(Event::UserDecision { approved }),
                )
            }
            Effect::AskUser { question } => {
                report.counts.questions += 1;
                phone.advance(costs.user_ms);
                let (reply, typed) = human.answer(&question);
                let mut result = String::new();
                if let Some(t) = &typed {
                    let out = phone.owner_types(t);
                    let _ = write!(result, "owner types on the phone: {}; ", out.note);
                }
                let _ = write!(result, "owner answers {reply:?}");
                let label = format!("ask_user {}", one_line(&question, 160));
                report.questions.push(question);
                (label, result, Some(Event::UserAnswer { text: reply }))
            }
            Effect::Finished(outcome) => {
                let line = format!("{:?}: {}", outcome.status, outcome.summary);
                report.outcome = outcome;
                finished = true;
                ("finished".to_owned(), line, None)
            }
        };
        report.steps.push(TranscriptStep {
            n: report.steps.len() + 1,
            at_ms,
            effect: label,
            result,
        });
        match next {
            Some(e) => event = e,
            None => break,
        }
    }
    if !finished {
        if let Some(e) = &report.sim_error {
            report.outcome = Outcome {
                status: OutcomeStatus::Failed,
                summary: format!("simulator error: {e}"),
                steps: 0,
                duration_ms: 0,
            };
        } else {
            report.capped = true;
            report.outcome.summary = format!(
                "driver stopped after {} iterations without a Finished effect",
                opts.max_iterations
            );
        }
    }
    report.final_state = phone.snapshot();
    report.counts.locks = phone.lock_count();
    report.counts.locked_refusals = phone.locked_refusals();
    report.counts.stale_acts = phone.stale_acts();
    let progress = session.progress();
    report.counts.parse_retries = usize::try_from(progress.parse_retries).unwrap_or(usize::MAX);
    report.memory = session.into_memory();
    report
}
