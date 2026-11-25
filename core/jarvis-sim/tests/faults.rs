//! The session against a phone that locks, cannot be read, or drops typed characters.
//!
//! The phone's action log (`RunReport::commands`, the refusal and stale-act counters) is the
//! ground truth: a command decided before a lock must never run after it without a fresh
//! observation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{Key, OutcomeStatus, RiskCategory, RunnerCommand};
use jarvis_sim::{
    DropChars, Faults, Human, LockSpec, ObserveFault, ObserveMode, RunReport, RunSetup,
    ScriptedModel, SimPhone,
};

fn with_faults(faults: Faults) -> RunSetup {
    RunSetup {
        faults,
        ..setup(ObserveMode::Structured)
    }
}

fn lock_before_act(n: usize, unlock_after_ms: Option<u64>) -> Faults {
    Faults {
        locks: vec![LockSpec {
            before_act: Some(n),
            unlock_after_ms,
            ..LockSpec::default()
        }],
        ..Faults::default()
    }
}

fn run(setup: &RunSetup, app: &str, task: &str, outputs: &[String]) -> RunReport {
    let mut model = ScriptedModel::new(outputs.iter().cloned());
    go_with(
        setup,
        app,
        task,
        &mut model,
        &mut Human::approve_all(),
        MemoryStore::new(),
    )
}

fn taps(r: &RunReport) -> Vec<&RunnerCommand> {
    r.commands
        .iter()
        .filter(|c| matches!(c, RunnerCommand::Tap { .. }))
        .collect()
}

/// Transcript positions of a refused command and of the next command after it: an observe
/// must lie between them.
fn assert_observed_between_refusal_and_next_act(r: &RunReport) {
    let refused = r
        .steps
        .iter()
        .position(|s| s.result.starts_with("REFUSED"))
        .unwrap_or_else(|| panic!("no refusal:\n{}", r.transcript()));
    let next_act = r.steps[refused + 1..]
        .iter()
        .position(|s| s.effect.starts_with("act "))
        .map(|p| p + refused + 1)
        .unwrap_or_else(|| panic!("no act after the refusal:\n{}", r.transcript()));
    let between = &r.steps[refused + 1..next_act];
    assert!(
        between
            .iter()
            .any(|s| s.effect == "observe" && !s.result.contains("locked")),
        "no fresh observation between the refusal and the next command:\n{}",
        r.transcript()
    );
}

#[test]
fn the_stale_act_check_itself_works() {
    // Driven directly: unlock, then act without observing. The phone must notice.
    let mut phone = SimPhone::new().unwrap();
    phone.launch("settings").unwrap();
    phone.lock_now(None);
    let refused = phone.act(&RunnerCommand::Wait { ms: 1 });
    assert!(refused.result.is_device_locked());
    phone.unlock();
    phone.act(&RunnerCommand::Wait { ms: 1 });
    assert_eq!(phone.stale_acts(), 1);
    assert_eq!(phone.locked_refusals(), 1);
    phone.observe_result().unwrap();
    phone.act(&RunnerCommand::Wait { ms: 1 });
    assert_eq!(
        phone.stale_acts(),
        1,
        "after an observation the command is fresh"
    );
}

#[test]
fn lock_mid_task_then_unlock_completes_without_a_stale_action() {
    let r = run(
        &with_faults(lock_before_act(1, Some(120_000))),
        "settings",
        "Turn off Wi-Fi",
        &[
            tap("Cell:Wi-Fi"),
            tap("Switch:Wi-Fi"),
            tap("Switch:Wi-Fi"),
            done("Wi-Fi is off"),
        ],
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert!(r.final_state.check("settings.wifi == false").unwrap());
    // Cell, switch (refused while locked), switch again after a fresh look.
    assert_eq!(taps(&r).len(), 3);
    assert_eq!(r.counts.locked_refusals, 1);
    assert_eq!(r.counts.stale_acts, 0);
    assert_eq!(r.counts.locks, 1);
    assert_eq!(r.counts.unlock_waits, 2, "one timer tick, then the unlock");
    assert_observed_between_refusal_and_next_act(&r);
    // The approval before the lock was for a tap that never ran: asked again.
    assert_eq!(
        categories(&r),
        vec![RiskCategory::SystemSetting, RiskCategory::SystemSetting]
    );
    // Two minutes locked are not active time.
    assert!(r.outcome.duration_ms < 60_000, "{}", r.outcome.duration_ms);
    let entry = r.memory.log.entries().last().unwrap();
    assert_eq!((entry.lock_waits, entry.locked_ms), (1, 120_000));
}

#[test]
fn a_lock_found_by_observation_after_an_action_is_reported_and_nothing_is_learned() {
    // The phone locks right after the first tap: the verifying observation is locked.
    // (2 750 ms: the tap runs at 2 300-2 700 ms, after the fresh look before acting on the
    // 2 s old plan.)
    let faults = Faults {
        locks: vec![LockSpec {
            at_ms: Some(2_750),
            unlock_after_ms: Some(30_000),
            ..LockSpec::default()
        }],
        ..Faults::default()
    };
    let r = run(
        &with_faults(faults),
        "settings",
        "Turn off Wi-Fi",
        &[tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("Wi-Fi is off")],
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert_eq!(r.counts.locked_refusals, 0);
    assert_eq!(r.counts.stale_acts, 0);
    assert!(r.requests.iter().any(|q| q
        .user
        .contains("done, but the phone locked before the result could be checked")));
    assert!(
        r.memory.flows.is_empty(),
        "a recording with a step of unknown result is not learned"
    );
}

#[test]
fn a_lock_during_replay_re_derives_the_step_with_no_model_call() {
    let setup = setup(ObserveMode::Structured);
    let script = [tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("Wi-Fi is off")];
    let first = run(&setup, "settings", "Turn off Wi-Fi", &script);
    assert_eq!(first.memory.flows.len(), 1);
    let faulty = with_faults(lock_before_act(1, Some(60_000)));
    let mut model = ScriptedModel::new(Vec::<String>::new());
    let r = go_with(
        &faulty,
        "settings",
        "Turn off Wi-Fi",
        &mut model,
        &mut Human::approve_all(),
        first.memory,
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert_eq!(r.counts.model_calls, 0);
    assert_eq!(taps(&r).len(), 3);
    assert_eq!(r.counts.stale_acts, 0);
    assert_observed_between_refusal_and_next_act(&r);
    let flow = &r.memory.flows.flows()[0];
    assert_eq!((flow.successes, flow.failures), (2, 0));
}

#[test]
fn a_lock_during_a_pending_confirmation_asks_again_after_the_unlock() {
    let faults = Faults {
        // Effects: observe, infer, observe (fresh look: the 2 s plan is older than the
        // limit), act (tap Mom), observe, infer, observe (fresh look), act (focus), observe
        // (where the focus went), act (type), observe, infer, confirm (Send) <- index 12 with
        // the app already approved (twenty-fourth review: a step that asks gets no fresh look
        // before its question; was 13).
        locks: vec![LockSpec {
            at_effect: Some(12),
            unlock_after_ms: Some(45_000),
            ..LockSpec::default()
        }],
        ..Faults::default()
    };
    let r = run(
        &with_faults(faults),
        "messages",
        "Text Mom that I'm running late",
        &[
            tap("Cell:Mom"),
            type_into("TextView:Message", "running late"),
            tap("Button:Send"),
            done("Sent"),
        ],
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert_eq!(r.confirmations.len(), 2);
    assert!(r.confirmations[0].interrupted);
    assert_eq!(r.confirmations[0].request, r.confirmations[1].request);
    assert!(r.confirmations[1].approved);
    assert_eq!(r.counts.model_calls, 4, "re-asked without a model call");
    assert!(r
        .final_state
        .check("messages.thread[\"Mom\"].last == \"running late\"")
        .unwrap());
}

#[test]
fn a_phone_that_stays_locked_ends_interrupted_and_runs_nothing() {
    let mut setup = with_faults(lock_before_act(0, None));
    setup.max_locked_wait_ms = Some(300_000);
    let r = run(
        &setup,
        "settings",
        "Turn off Wi-Fi",
        &[tap("Cell:Wi-Fi"), done("x")],
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Interrupted,
        "{}",
        r.transcript()
    );
    assert!(r.outcome.summary.contains("stayed locked"));
    assert_eq!(r.counts.locked_refusals, 1);
    assert_eq!(r.counts.unlock_waits, 5);
    assert!(r.final_state.check("settings.wifi == true").unwrap());
    let entry = r.memory.log.entries().last().unwrap();
    assert_eq!(entry.outcome, OutcomeStatus::Interrupted);
    assert!(r.memory.flows.is_empty());
}

#[test]
fn observation_failures_are_retried_then_fail_the_task() {
    let recover = Faults {
        observe_fail: Some(ObserveFault { after: 1, times: 3 }),
        ..Faults::default()
    };
    let r = run(
        &with_faults(recover),
        "settings",
        "Turn off Wi-Fi",
        &[tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("Wi-Fi is off")],
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    assert_eq!(r.counts.observe_failures, 3);

    let broken = Faults {
        observe_fail: Some(ObserveFault {
            after: 1,
            times: 99,
        }),
        ..Faults::default()
    };
    let r = run(
        &with_faults(broken),
        "settings",
        "Turn off Wi-Fi",
        &[tap("Cell:Wi-Fi"), done("x")],
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert!(
        r.outcome
            .summary
            .starts_with("Could not read the screen (4 attempts in a row)"),
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.counts.observe_failures, 4);
    assert_eq!(
        r.memory.log.entries().last().unwrap().outcome,
        OutcomeStatus::Failed,
        "logged as failed, not cancelled"
    );
}

fn drop_every_third(times: usize) -> Faults {
    Faults {
        drop_chars: Some(DropChars { every: 3, times }),
        ..Faults::default()
    }
}

#[test]
fn dropped_characters_are_corrected_on_the_retry() {
    let r = run(
        &with_faults(drop_every_third(1)),
        "messages",
        "Text Mom that I'm running late",
        &[
            tap("Cell:Mom"),
            type_into("TextView:Message", "running late"),
            tap("Button:Send"),
            done("Sent"),
        ],
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    let typed: Vec<&RunnerCommand> = r
        .commands
        .iter()
        .filter(|c| matches!(c, RunnerCommand::TypeText { .. }))
        .collect();
    assert_eq!(typed.len(), 2);
    // "running late" minus every third character is "runig at": 8 characters arrived.
    let deletes = r
        .commands
        .iter()
        .filter(|c| {
            matches!(
                c,
                RunnerCommand::PressKey {
                    key: Key::Delete,
                    ..
                }
            )
        })
        .count();
    assert_eq!(deletes, "running late".chars().count() - 4);
    assert!(r
        .final_state
        .check("messages.thread[\"Mom\"].last == \"running late\"")
        .unwrap());
    assert_eq!(r.counts.model_calls, 4);
}

#[test]
fn characters_dropped_twice_are_reported_to_the_model_which_decides() {
    let r = run(
        &with_faults(drop_every_third(2)),
        "messages",
        "Text Mom that I'm running late",
        &[
            tap("Cell:Mom"),
            type_into("TextView:Message", "running late"),
            r#"{"thought":"t","action":"clear","element":"@TextView:Message"}"#.to_owned(),
            type_into("TextView:Message", "running late"),
            tap("Button:Send"),
            done("Sent"),
        ],
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    let told = &r.requests[2].user;
    assert!(
        told.contains("did not arrive intact (characters were dropped): expected \"running late\", the field shows \"runig at\"; it was typed again once"),
        "{told}"
    );
    assert!(r
        .final_state
        .check("messages.thread[\"Mom\"].last == \"running late\"")
        .unwrap());
    // The mangled text was never sent.
    let thread = r.final_state.eval("messages.thread[\"Mom\"]").unwrap();
    assert!(!thread.to_string().contains("runig"), "{thread}");
}
