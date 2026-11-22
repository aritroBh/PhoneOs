//! The structural policy end to end on the simulated phone: first-use approval from a fresh
//! store, StepByStep, per-app trust levels, unverified (non-English) controls and calls.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{AppRule, OutcomeStatus, RiskCategory, RunnerCommand, TrustLevel};
use jarvis_sim::{Human, ObserveMode, RunSetup, ScriptedModel};

const MSG: &str = "com.apple.MobileSMS";
const TASK: &str = "Text Mom that I'm running late";

fn text_mom() -> Vec<String> {
    vec![
        tap("Cell:Mom"),
        type_into("TextView:Message", "running late"),
        tap("Button:Send"),
        done("Sent"),
    ]
}

fn run(
    setup: &RunSetup,
    app: &str,
    task: &str,
    outputs: &[String],
    human: &mut Human,
) -> jarvis_sim::RunReport {
    let mut model = ScriptedModel::new(outputs.iter().cloned());
    go_with(setup, app, task, &mut model, human, MemoryStore::new())
}

#[test]
fn a_fresh_store_asks_once_per_app_then_remembers() {
    for mode in MODES {
        let r = run(
            &fresh_setup(mode),
            "messages",
            TASK,
            &text_mom(),
            &mut Human::approve_all(),
        );
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Success,
            "{}",
            r.transcript()
        );
        assert_eq!(
            categories(&r),
            vec![RiskCategory::FirstUse, RiskCategory::SendMessage]
        );
        assert_eq!(
            r.confirmations[0].request.summary,
            "Let Jarvis act in Messages (com.apple.MobileSMS)?"
        );
        // Nothing ran before the app was approved.
        let confirm = r
            .steps
            .iter()
            .position(|s| s.effect.contains("[FirstUse]"))
            .unwrap();
        let first_act = r
            .steps
            .iter()
            .position(|s| s.effect.starts_with("act"))
            .unwrap();
        assert!(confirm < first_act);
        assert!(r.memory.is_app_approved(MSG));
        // The saved memory carries the approval: a second session does not ask again.
        let mem = MemoryStore::from_json(&r.memory.to_json().unwrap()).unwrap();
        let mut model = ScriptedModel::new(Vec::<String>::new());
        let again = go_with(
            &fresh_setup(mode),
            "messages",
            TASK,
            &mut model,
            &mut Human::approve_all(),
            mem,
        );
        assert_eq!(again.outcome.status, OutcomeStatus::Success);
        assert_eq!(categories(&again), vec![RiskCategory::SendMessage]);
    }
}

#[test]
fn a_declined_first_use_ends_blocked_and_nothing_happens_in_the_app() {
    for mode in MODES {
        let r = run(
            &fresh_setup(mode),
            "messages",
            TASK,
            &[tap("Cell:Mom"), tap("Cell:Mom"), done("x")],
            &mut Human::decline_categories(&[RiskCategory::FirstUse]),
        );
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Blocked,
            "{}",
            r.transcript()
        );
        assert_eq!(categories(&r), vec![RiskCategory::FirstUse]);
        assert!(r.commands.is_empty(), "{:?}", r.commands);
        assert_eq!(r.final_state.screen, "list");
        assert!(!r.memory.is_app_approved(MSG));
    }
}

#[test]
fn a_replayed_flow_cannot_skip_a_declined_first_use() {
    // Learn with everything approved, then drop the approvals (a reset) and replay.
    let r = run(
        &fresh_setup(ObserveMode::Structured),
        "messages",
        TASK,
        &text_mom(),
        &mut Human::approve_all(),
    );
    let mut mem = r.memory.clone();
    assert_eq!(mem.flows.len(), 1);
    mem.approved_apps.clear();
    let mut model = ScriptedModel::new([tap("Cell:Mom")]);
    let replay = go_with(
        &fresh_setup(ObserveMode::Structured),
        "messages",
        TASK,
        &mut model,
        &mut Human::decline_categories(&[RiskCategory::FirstUse]),
        mem,
    );
    assert_eq!(
        replay.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        replay.transcript()
    );
    assert_eq!(categories(&replay), vec![RiskCategory::FirstUse]);
    assert!(replay.commands.is_empty());
    assert!(replay
        .final_state
        .check(r#"messages.thread["Mom"].last == "See you at six""#)
        .unwrap());
}

#[test]
fn step_by_step_asks_before_every_state_change() {
    let mut setup = setup(ObserveMode::Structured);
    setup.step_by_step = true;
    let r = run(
        &setup,
        "notes",
        "Delete the note called Old receipts",
        &[
            scroll_down(),
            tap("Cell:Old receipts"),
            tap("Button:Delete"),
            tap("Button:Delete"),
            done("Deleted"),
        ],
        &mut Human::approve_all(),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    // The plain scroll ran without asking; the row tap asked as a step; both deletes asked
    // with their category.
    assert_eq!(
        categories(&r),
        vec![
            RiskCategory::None,
            RiskCategory::Delete,
            RiskCategory::Delete
        ]
    );
    // Declining a step does not run it, and the next step is asked again.
    let r = run(
        &setup,
        "settings",
        "Turn off Wi-Fi",
        &[
            tap("Cell:Wi-Fi"),
            tap("Cell:Wi-Fi"),
            tap("Switch:Wi-Fi"),
            done("off"),
        ],
        &mut Human::scripted(&[false, true, true]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert_eq!(
        categories(&r),
        vec![
            RiskCategory::None,
            RiskCategory::None,
            RiskCategory::SystemSetting
        ]
    );
    assert!(r.final_state.check("settings.wifi == false").unwrap());
}

#[test]
fn a_trusted_settings_app_toggles_without_any_confirmation() {
    let mut setup = fresh_setup(ObserveMode::Structured);
    setup.app_rules = vec![AppRule::new("com.apple.Preferences", TrustLevel::Trusted)];
    let r = run(
        &setup,
        "settings",
        "Turn off Wi-Fi",
        &[tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("off")],
        &mut Human::decline_all(),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert!(r.confirmations.is_empty());
    assert!(r.final_state.check("settings.wifi == false").unwrap());
}

#[test]
fn a_denied_app_rule_keeps_the_agent_out() {
    let mut setup = fresh_setup(ObserveMode::Structured);
    setup.app_rules = vec![AppRule::new("com.apple.mobilenotes", TrustLevel::Denied)];
    let r = run(
        &setup,
        "home",
        "Open Notes",
        &[open_app("Notes"), open_app("Notes")],
        &mut Human::approve_all(),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        r.transcript()
    );
    assert!(r.commands.is_empty());
    assert!(r.confirmations.is_empty());
    // Starting inside it blocks before the model is asked.
    let r = run(
        &setup,
        "notes",
        "Read my notes",
        &[done("x")],
        &mut Human::approve_all(),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Blocked);
    assert_eq!(r.counts.model_calls, 0);
}

#[test]
fn a_german_pay_button_is_unverified_and_asks() {
    let r = run(
        &setup(ObserveMode::Structured),
        "laden",
        "Pay for the headphones",
        &[tap("Button:Bezahlen"), fail("declined")],
        &mut Human::decline_all(),
    );
    assert_eq!(categories(&r), vec![RiskCategory::Unverified]);
    assert!(r.confirmations[0].request.summary.contains("Bezahlen"));
    assert!(r.final_state.check("laden.paid == false").unwrap());
    assert!(r.commands.is_empty());
}

#[test]
fn tapping_a_phone_favorite_needs_a_call_confirmation() {
    for mode in MODES {
        let r = run(
            &setup(mode),
            "dialer",
            "Call Mom",
            &[tap("Cell:Mom"), done("calling")],
            &mut Human::approve_all(),
        );
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Success,
            "{}",
            r.transcript()
        );
        assert_eq!(categories(&r), vec![RiskCategory::Call]);
        assert!(r.final_state.check("dialer.calling == \"Mom\"").unwrap());
        // Declined: no call is placed.
        let r = run(
            &setup(mode),
            "dialer",
            "Call Mom",
            &[tap("Cell:Mom"), fail("declined")],
            &mut Human::decline_all(),
        );
        assert_eq!(categories(&r), vec![RiskCategory::Call]);
        assert!(r.final_state.check("dialer.calling == \"\"").unwrap());
        assert!(!r
            .commands
            .iter()
            .any(|c| matches!(c, RunnerCommand::Tap { .. })));
    }
}

#[test]
fn parse_retries_come_from_the_session_counter() {
    let r = run(
        &setup(ObserveMode::Structured),
        "settings",
        "Turn off Wi-Fi",
        &[
            "this is not json".to_owned(),
            tap("Cell:Wi-Fi"),
            tap("Switch:Wi-Fi"),
            done("off"),
        ],
        &mut Human::approve_all(),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert_eq!(r.counts.parse_retries, 1);
    assert_eq!(r.counts.model_calls, 4);
}
