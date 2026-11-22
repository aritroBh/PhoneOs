//! End to end: a workflow run by `jarvis_skills::WorkflowRun` over the simulated phone,
//! each `StartSession` an ordinary agent `Session` with the scripted model, including a
//! skill confined to Messages whose script tries to leave for the Shop.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::collections::BTreeMap;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::{OutcomeStatus, RiskCategory, RunnerCommand, TaskSpec};
use jarvis_sim::{run_with, Human, ObserveMode, RunReport, RunSetup, ScriptedModel, SimPhone};
use jarvis_skills::{
    OnFailure, RunLimits, Skill, SkillParam, SkillStore, Step, Trigger, Workflow, WorkflowEffect,
    WorkflowEvent, WorkflowOutcome, WorkflowRun, WorkflowStatus,
};

const MESSAGES: &str = "com.apple.MobileSMS";
const SHOP: &str = "com.example.shop";

fn skills() -> SkillStore {
    let mut s = SkillStore::new();
    s.add(Skill {
        id: "text-someone".into(),
        name: "Text someone".into(),
        triggers: vec!["text {contact} {message}".into()],
        instructions: "Use the existing thread in Messages.".into(),
        apps: vec![MESSAGES.into()],
        confine_to_apps: true,
        params: vec![
            SkillParam {
                name: "contact".into(),
                description: "Who".into(),
                required: true,
            },
            SkillParam {
                name: "message".into(),
                description: "What".into(),
                required: true,
            },
        ],
        ..Skill::default()
    })
    .unwrap();
    s
}

fn workflow(on_failure: OnFailure) -> Workflow {
    Workflow {
        id: "heading-home".into(),
        name: "Heading home".into(),
        trigger: Trigger::External {
            key: "left.office".into(),
        },
        on_failure,
        steps: vec![
            Step::RunSkill {
                skill_id: "text-someone".into(),
                args: [("contact", "Mom"), ("message", "running late")]
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                    .collect::<BTreeMap<_, _>>(),
            },
            Step::RunTask {
                text: "Turn off Wi-Fi".into(),
            },
            Step::Notify {
                text: "Mom knows, and Wi-Fi is off.".into(),
            },
        ],
        ..Workflow::default()
    }
}

/// What the shell did for one workflow run.
struct Driven {
    outcome: WorkflowOutcome,
    sessions: Vec<RunReport>,
    notifications: Vec<String>,
    phone: SimPhone,
}

/// The shell loop: workflow effects to sessions on one phone (memory carried over, nothing
/// approved at first), the owner approving every confirmation, notifications recorded.
fn drive(wf: Workflow, scripts: Vec<Vec<String>>, mode: ObserveMode) -> Driven {
    let setup = RunSetup {
        mode,
        ..RunSetup::default()
    };
    let mut phone = setup.new_phone("messages").unwrap();
    let mut memory = MemoryStore::new();
    let mut scripts = scripts.into_iter();
    let mut sessions = Vec::new();
    let mut notifications = Vec::new();
    let mut run = WorkflowRun::new(wf, &skills(), RunLimits::default());
    let mut event = WorkflowEvent::Start;
    for _ in 0..50 {
        let now = phone.now_ms();
        match run.step(now, event).unwrap() {
            WorkflowEffect::StartSession { task, skill } => {
                let mut config = setup.session_config(&phone);
                config.skill = skill;
                let id = format!("wf-{}", sessions.len());
                let session = Session::new(config, TaskSpec::new(id, task), memory.clone());
                let mut model = ScriptedModel::new(scripts.next().unwrap_or_default());
                let report = run_with(
                    session,
                    &mut phone,
                    &mut model,
                    &mut Human::approve_all(),
                    &setup.options,
                );
                memory = report.memory.clone();
                event = WorkflowEvent::SessionFinished {
                    success: report.outcome.status == OutcomeStatus::Success,
                    summary: report.outcome.summary.clone(),
                };
                sessions.push(report);
            }
            WorkflowEffect::Notify { text } => {
                notifications.push(text);
                event = WorkflowEvent::Notified;
            }
            WorkflowEffect::AskOwner { .. } => {
                event = WorkflowEvent::OwnerAnswered {
                    yes: true,
                    text: String::new(),
                }
            }
            WorkflowEffect::Wait { until_ms } => {
                phone.advance(until_ms.saturating_sub(phone.now_ms()));
                event = WorkflowEvent::WaitElapsed;
            }
            WorkflowEffect::Finished { outcome } => {
                return Driven {
                    outcome,
                    sessions,
                    notifications,
                    phone,
                };
            }
        }
    }
    panic!("workflow did not finish");
}

fn text_mom_escaping_once() -> Vec<String> {
    vec![
        open_app("Shop"),
        tap("Cell:Mom"),
        type_into("TextView:Message", "running late"),
        tap("Button:Send"),
        done("Sent"),
    ]
}

fn wifi_off() -> Vec<String> {
    vec![
        open_app("Settings"),
        tap("Cell:Wi-Fi"),
        tap("Switch:Wi-Fi"),
        done("Wi-Fi is off"),
    ]
}

fn launched(r: &RunReport, bundle: &str) -> bool {
    r.commands
        .iter()
        .any(|c| matches!(c, RunnerCommand::LaunchApp { bundle_id } if bundle_id == bundle))
}

#[test]
fn three_step_workflow_with_a_confined_skill_runs_end_to_end() {
    for mode in MODES {
        let d = drive(
            workflow(OnFailure::Stop),
            vec![text_mom_escaping_once(), wifi_off()],
            mode,
        );
        let transcripts: String = d.sessions.iter().map(RunReport::transcript).collect();
        assert_eq!(
            d.outcome.status,
            WorkflowStatus::Succeeded,
            "{mode:?}: {}\n{transcripts}",
            d.outcome.summary
        );
        assert_eq!(d.outcome.steps_run, 3);
        assert_eq!(d.sessions.len(), 2);
        assert_eq!(
            d.notifications,
            vec!["Mom knows, and Wi-Fi is off.".to_owned()]
        );

        // Step 1: the skill session. Its task is the trigger filled with the arguments,
        // its prompt carries the guidance, and the escape to the Shop was refused.
        let text = &d.sessions[0];
        assert!(text.requests[0]
            .user
            .starts_with("Task: text Mom running late\n"));
        assert!(text.requests[0]
            .user
            .contains("| Use the existing thread in Messages."));
        assert!(text.requests[0].user.contains("Work only in: Messages"));
        assert!(
            text.requests[1].user.contains("works only in its own apps"),
            "{}",
            text.requests[1].user
        );
        assert!(!launched(text, SHOP), "{}", text.transcript());
        // The skill skipped neither first-use approval of Messages nor the send
        // confirmation.
        assert_eq!(
            text.confirm_categories(),
            vec![RiskCategory::FirstUse, RiskCategory::SendMessage]
        );
        assert_eq!(text.confirmations[0].request.app, MESSAGES);

        // Step 2: an ordinary, unconfined session that may open Settings.
        let wifi = &d.sessions[1];
        assert!(!wifi.requests[0].user.contains("Owner's guidance"));
        assert!(launched(wifi, "com.apple.Preferences"));
        assert_eq!(
            wifi.confirm_categories(),
            vec![RiskCategory::FirstUse, RiskCategory::SystemSetting]
        );
        // Approvals carry over in memory like for any two sessions.
        assert!(wifi.memory.is_app_approved(MESSAGES));
        assert!(wifi.memory.is_app_approved("com.apple.Preferences"));

        let state = d.phone.snapshot();
        assert!(state
            .check(r#"messages.thread["Mom"].last == "running late""#)
            .unwrap());
        assert!(state.check("settings.wifi == false").unwrap());
    }
}

#[test]
fn a_skill_that_keeps_trying_to_escape_is_blocked_and_the_workflow_stops() {
    let escape_twice = vec![open_app("Shop"), open_app("Shop"), done("Bought")];
    let d = drive(
        workflow(OnFailure::Stop),
        vec![escape_twice, wifi_off()],
        ObserveMode::Structured,
    );
    assert_eq!(d.outcome.status, WorkflowStatus::Failed);
    assert_eq!(d.outcome.failed_step, Some(0));
    assert_eq!(d.outcome.steps_run, 1);
    assert!(
        d.outcome.summary.contains("works only in its own apps"),
        "{}",
        d.outcome.summary
    );
    assert_eq!(d.sessions.len(), 1, "the Wi-Fi step never ran");
    assert_eq!(d.sessions[0].outcome.status, OutcomeStatus::Blocked);
    assert!(!launched(&d.sessions[0], SHOP));
    assert!(d.notifications.is_empty());
    assert!(d.phone.snapshot().check("settings.wifi == true").unwrap());

    // With Continue, the later steps still run.
    let escape_twice = vec![open_app("Shop"), open_app("Shop"), done("Bought")];
    let d = drive(
        workflow(OnFailure::Continue),
        vec![escape_twice, wifi_off()],
        ObserveMode::Structured,
    );
    assert_eq!(d.outcome.status, WorkflowStatus::CompletedWithFailures);
    assert_eq!(d.outcome.failed_step, Some(0));
    assert_eq!(d.sessions.len(), 2);
    assert!(d.phone.snapshot().check("settings.wifi == false").unwrap());
}
