//! Stale-screen guard against the simulated phone: the screen changes while the model
//! thinks (an alert appears over the target; the app goes to the background), and the tap
//! planned on the old screen is not sent. With the default 2 s model latency the session
//! looks again before acting (layer 1); with a latency under the limit the tap is sent and
//! the phone's runner refuses it because another app is in front (layer 2).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{OutcomeStatus, RunnerCommand};
use jarvis_sim::{
    ClockCosts, Faults, Human, ObserveMode, RunReport, RunSetup, ScreenChange, ScriptedModel,
};

fn with(change: ScreenChange, infer_ms: u64) -> RunSetup {
    RunSetup {
        faults: Faults {
            screen_changes: vec![change],
            ..Faults::default()
        },
        costs: ClockCosts {
            infer_ms,
            ..ClockCosts::default()
        },
        ..setup(ObserveMode::Structured)
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

fn taps(r: &RunReport) -> Vec<(f64, f64)> {
    r.commands
        .iter()
        .filter_map(|c| match c {
            RunnerCommand::Tap { x, y, .. } => Some((*x, *y)),
            _ => None,
        })
        .collect()
}

#[test]
fn an_alert_over_the_target_during_model_latency_voids_the_tap() {
    // "Trip ideas" (the fourth note) lies under the alert's buttons once it shows.
    let change = ScreenChange {
        during_infer: 0,
        alert: Some("delete_note".to_owned()),
        home: false,
    };
    let r = run(
        &with(change, 2_000),
        "notes",
        "Open my trip ideas note",
        &[
            tap("Cell:Trip ideas"),
            tap("Button:Cancel"),
            tap("Cell:Trip ideas"),
            done("Opened"),
        ],
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.transcript()
    );
    // The first tap command the phone got is the alert's Cancel (its first button, at
    // y 400-444), never the row planned on the screen without the alert: under the alert
    // that tap would have pressed "Delete".
    let first = taps(&r)[0];
    assert!(
        (400.0..444.0).contains(&first.1),
        "{first:?}\n{}",
        r.transcript()
    );
    assert_eq!(r.counts.stale_plans, 1, "{}", r.transcript());
    assert!(r.final_state.check("notes.open == \"Trip ideas\"").unwrap());
    assert!(r.final_state.check("notes.last_deleted == \"\"").unwrap());
    assert_eq!(r.counts.stale_refusals, 0);
}

#[test]
fn the_app_going_to_the_background_during_model_latency_voids_the_tap() {
    // Run 101: the observation came from one app, the app left the front, the tap would
    // have landed on the home screen.
    let change = ScreenChange {
        during_infer: 0,
        alert: None,
        home: true,
    };
    let r = run(
        &with(change, 2_000),
        "settings",
        "Turn off Wi-Fi",
        &[
            tap("Cell:Wi-Fi"),
            open_app("Settings"),
            tap("Cell:Wi-Fi"),
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
    assert!(
        matches!(r.commands[0], RunnerCommand::LaunchApp { .. }),
        "nothing was tapped on the home screen: {:?}\n{}",
        r.commands[0],
        r.transcript()
    );
    assert_eq!(r.counts.stale_plans, 1);
    assert!(r.final_state.check("settings.wifi == false").unwrap());
}

#[test]
fn under_the_age_limit_the_runner_refuses_a_tap_whose_app_left_the_front() {
    // A fast model (500 ms): no second look, the tap goes out carrying the app it was
    // planned on, and the phone's runner refuses it (layer 2); the step is refunded and
    // decided again on a fresh observation.
    let change = ScreenChange {
        during_infer: 0,
        alert: None,
        home: true,
    };
    let r = run(
        &with(change, 500),
        "settings",
        "Turn off Wi-Fi",
        &[
            tap("Cell:Wi-Fi"),
            open_app("Settings"),
            tap("Cell:Wi-Fi"),
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
    assert_eq!(r.counts.stale_refusals, 1, "{}", r.transcript());
    assert_eq!(r.counts.fresh_looks, 0);
    assert_eq!(
        r.commands[0].expect().map(|e| e.bundle_id.as_str()),
        Some("com.apple.Preferences")
    );
    assert!(r.final_state.check("settings.wifi == false").unwrap());
    // Refunded: the refused tap is not a step.
    assert_eq!(r.outcome.steps, 4, "{}", r.transcript());
}

#[test]
fn nothing_changing_costs_one_look_per_slow_step_and_nothing_per_fast_one() {
    let script = [tap("Cell:Wi-Fi"), tap("Switch:Wi-Fi"), done("Wi-Fi is off")];
    let slow = run(
        &with(ScreenChange::default(), 2_000),
        "settings",
        "Turn off Wi-Fi",
        &script,
    );
    let fast = run(
        &with(ScreenChange::default(), 500),
        "settings",
        "Turn off Wi-Fi",
        &script,
    );
    for r in [&slow, &fast] {
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Success,
            "{}",
            r.transcript()
        );
        assert_eq!(taps(r).len(), 2);
        assert_eq!(r.counts.stale_plans, 0);
    }
    // Twenty-fourth review (R24): only a slow step that would be sent unasked gets the fresh
    // look (the Wi-Fi row); the switch asks (a system setting) on the screen it was planned
    // on, and the look after its approval re-checks the screen (was: 2 fresh looks).
    assert_eq!(slow.counts.fresh_looks, 1, "{}", slow.transcript());
    assert_eq!(fast.counts.fresh_looks, 0);
    assert_eq!(slow.counts.observes, fast.counts.observes + 1);
}
