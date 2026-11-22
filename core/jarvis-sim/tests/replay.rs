//! Learned flows: run once with the model, save memory, run again with no model calls.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{OutcomeStatus, RiskCategory};
use jarvis_sim::{Human, ObserveMode, ScriptedModel};
use serde_json::json;

const TASK: &str = "Text Mom that I'm running late";

fn script() -> Vec<String> {
    vec![
        tap("Cell:Mom"),
        type_into("TextView:Message", "running late"),
        tap("Button:Send"),
        done("Sent"),
    ]
}

/// Runs the task once and returns the memory after a JSON save/load, as the shell would.
fn learn(mode: ObserveMode) -> MemoryStore {
    let (r, m) = go(mode, "messages", TASK, &script(), &mut Human::approve_all());
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert_eq!(m.calls(), 4);
    let json = r.memory.to_json().unwrap();
    let mem = MemoryStore::from_json(&json).unwrap();
    assert_eq!(mem.flows.len(), 1, "one flow was recorded");
    mem
}

#[test]
fn second_run_replays_with_zero_model_calls_and_the_same_final_state() {
    for mode in MODES {
        let (first, _) = go(mode, "messages", TASK, &script(), &mut Human::approve_all());
        let mem = MemoryStore::from_json(&first.memory.to_json().unwrap()).unwrap();
        let mut model = ScriptedModel::new(Vec::<String>::new());
        let second = go_with(
            &setup(mode),
            "messages",
            TASK,
            &mut model,
            &mut Human::approve_all(),
            mem,
        );
        assert_eq!(
            second.outcome.status,
            OutcomeStatus::Success,
            "{mode:?}\n{}",
            second.transcript()
        );
        assert_eq!(model.calls(), 0, "{mode:?}");
        assert_eq!(second.counts.model_calls, 0);
        assert!(second.requests.is_empty());
        assert!(
            second.outcome.summary.contains("saved routine"),
            "{}",
            second.outcome.summary
        );
        // Same final state as the model-driven run.
        assert_eq!(second.final_state.states, first.final_state.states);
        assert_eq!(second.final_state.screen, first.final_state.screen);
        assert!(second
            .final_state
            .check(r#"messages.thread["Mom"].last == "running late""#)
            .unwrap());
        // A replay is still gated: the send was confirmed again, and on runner text every
        // replayed step that changes something asks again (fifth review: a replay never
        // skips the unverified-screen question).
        assert_eq!(
            categories(&second),
            per_mode(
                mode,
                vec![RiskCategory::SendMessage],
                vec![
                    RiskCategory::Unverified,
                    RiskCategory::Unverified,
                    RiskCategory::SendMessage
                ]
            )
        );
        assert_text_warnings(&second, mode);
        // The flow's statistics and the log recorded both runs.
        assert!(second.memory.flows.flows()[0].successes >= 2);
        assert_eq!(second.memory.log.entries().len(), 2);
    }
}

#[test]
fn replay_with_a_declined_confirmation_does_not_send() {
    let mem = learn(ObserveMode::Structured);
    let mut model = ScriptedModel::new(Vec::<String>::new());
    let r = go_with(
        &setup(ObserveMode::Structured),
        "messages",
        TASK,
        &mut model,
        &mut Human::decline_all(),
        mem,
    );
    assert_eq!(r.confirmations.len(), 1);
    assert!(!r.confirmations[0].approved);
    assert!(r
        .final_state
        .check(r#"messages.thread["Mom"].last == "See you at six""#)
        .unwrap());
    assert_ne!(r.outcome.status, OutcomeStatus::Success);
}

#[test]
fn a_different_screen_hands_control_back_to_the_model() {
    for mode in MODES {
        let mem = learn(mode);
        let setup = setup(mode);
        // Mom is now saved as "Mother": the saved first step cannot find its control.
        let mut phone = setup.new_phone("messages").unwrap();
        phone
            .set_state("messages", "contacts", json!(["Alex", "Mother", "Dad"]))
            .unwrap();
        phone
            .set_state("messages", "thread.Mother", json!(["Hi"]))
            .unwrap();
        let mut model = ScriptedModel::new([
            tap("Cell:Mother"),
            type_into("TextView:Message", "running late"),
            tap("Button:Send"),
            done("Sent"),
        ]);
        let r = run_phone(
            &setup,
            &mut phone,
            TASK,
            &mut model,
            &mut Human::approve_all(),
            mem,
        );
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Success,
            "{mode:?}\n{}",
            r.transcript()
        );
        assert_eq!(model.calls(), 4, "the model drove the whole task");
        assert!(
            r.requests[0].user.contains("saved routine"),
            "{}",
            r.requests[0].user
        );
        assert!(r
            .final_state
            .check(r#"messages.thread["Mother"].last == "running late""#)
            .unwrap());
    }
}

#[test]
fn a_replayed_step_that_fails_hands_over_mid_flow() {
    for mode in MODES {
        let mem = learn(mode);
        let setup = setup(mode);
        let mut phone = setup.new_phone("messages").unwrap();
        // Act 0 taps Mom, act 1 taps the field, act 2 types: make the typing fail.
        phone.fail_act_at(2);
        let mut model = ScriptedModel::new([
            type_into("TextView:Message", "running late"),
            tap("Button:Send"),
            done("Sent"),
        ]);
        let r = run_phone(
            &setup,
            &mut phone,
            TASK,
            &mut model,
            &mut Human::approve_all(),
            mem,
        );
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Success,
            "{mode:?}\n{}",
            r.transcript()
        );
        assert_eq!(
            model.calls(),
            3,
            "only the steps after the failure used the model"
        );
        assert!(
            r.requests[0].user.contains("(saved)"),
            "{}",
            r.requests[0].user
        );
        assert!(r
            .final_state
            .check(r#"messages.thread["Mom"].last == "running late""#)
            .unwrap());
    }
}

#[test]
fn a_different_task_text_does_not_replay() {
    let mem = learn(ObserveMode::Structured);
    let mut model = ScriptedModel::new([tap("Cell:Alex"), done("Opened Alex")]);
    let r = go_with(
        &setup(ObserveMode::Structured),
        "messages",
        "Open the conversation with Alex",
        &mut model,
        &mut Human::approve_all(),
        mem,
    );
    assert_eq!(model.calls(), 2);
    assert_eq!(r.outcome.status, OutcomeStatus::Success);
    assert_eq!(r.memory.flows.len(), 2);
}
