//! A locked phone, an unreadable screen and typed text that does not arrive.
//!
//! One test (at least) per state the session can be in when the lock arrives, the time
//! accounting (budget, wait limit, a clock that stands still or jumps), learned flows and
//! locks, observation failures, and typed-text verification inside the session.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{observation, settings_screen};
use jarvis_proto::{
    Effect, Event, Key, ModelOutput, ModelRequest, Observation, OutcomeStatus, ParamKind, Phase,
    RiskCategory, RunnerCommand, RunnerResult, Screen, SessionConfig, TaskSpec, Tier, TimedEvent,
    ToolParam, ToolResult, ToolSpec,
};

const TAP_WIFI_CELL: &str = r#"{"action":"tap","element":4}"#;
/// The Airplane Mode switch: a system setting, so it asks first.
const TAP_AIRPLANE: &str = r#"{"action":"tap","element":3}"#;

fn ev(now: u64, e: Event) -> TimedEvent {
    TimedEvent::new(now, e)
}

fn observed(screen: Screen) -> Event {
    Event::Observed(observation(screen))
}

fn locked_observation() -> Event {
    Event::Observed(Observation {
        device_locked: true,
        ..observation(home_screen())
    })
}

fn model(text: &str) -> Event {
    Event::ModelOutput(ModelOutput { text: text.into() })
}

fn note_tool() -> ToolSpec {
    ToolSpec {
        name: "note".into(),
        description: "Write a note".into(),
        params: vec![ToolParam {
            name: "text".into(),
            description: String::new(),
            kind: ParamKind::String,
            required: true,
        }],
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
    }
}

fn session_with(c: SessionConfig) -> Session {
    Session::new(c, TaskSpec::new("t", "Turn off Wi-Fi"), MemoryStore::new())
}

fn session() -> Session {
    let mut c = config();
    c.tools = vec![note_tool()];
    session_with(c)
}

/// Drives a fresh session to its first `Infer` on the Settings screen.
fn thinking(s: &mut Session) -> ModelRequest {
    assert_eq!(
        s.step(ev(10, Event::Start)),
        Effect::Observe { screenshot: false }
    );
    match s.step(ev(20, observed(settings_screen()))) {
        Effect::Infer(r) => r,
        other => panic!("{other:?}"),
    }
}

fn expect_infer(e: Effect) -> ModelRequest {
    match e {
        Effect::Infer(r) => r,
        other => panic!("expected Infer, got {other:?}"),
    }
}

fn expect_wait(e: &Effect) -> (String, u64) {
    match e {
        Effect::AwaitUnlock { reason, waited_ms } => (reason.clone(), *waited_ms),
        other => panic!("expected AwaitUnlock, got {other:?}"),
    }
}

fn finished(e: Effect) -> jarvis_proto::Outcome {
    match e {
        Effect::Finished(o) => o,
        other => panic!("expected Finished, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------
// One test per state the lock can arrive in.
// ---------------------------------------------------------------------------------------

#[test]
fn locked_observation_waits_then_observes_again_before_anything_else() {
    let mut s = session();
    assert_eq!(
        s.step(ev(10, Event::Start)),
        Effect::Observe { screenshot: false }
    );
    let e = s.step(ev(20, locked_observation()));
    let (reason, waited) = expect_wait(&e);
    assert!(reason.contains("Unlock"), "{reason}");
    assert_eq!(waited, 0);
    let p = s.progress();
    assert!(p.waiting_for_unlock);
    assert_eq!(p.phase, Phase::WaitingForUnlock);
    assert_eq!(p.app, None, "the lock screen is not ingested");
    // A timer tick re-emits the wait with the time so far.
    let (_, waited) = expect_wait(&s.step(ev(60_020, Event::Tick)));
    assert_eq!(waited, 60_000);
    assert_eq!(s.progress().locked_wait_ms, 60_000);
    // Still locked when polled: still waiting, no failure counted.
    expect_wait(&s.step(ev(70_020, locked_observation())));
    // Unlocked: observe first.
    assert_eq!(
        s.step(ev(80_020, Event::DeviceUnlocked)),
        Effect::Observe { screenshot: false }
    );
    let p = s.progress();
    assert!(!p.waiting_for_unlock);
    assert_eq!(p.locked_wait_ms, 0);
    assert_eq!(p.locked_total_ms, 80_000);
    expect_infer(s.step(ev(80_030, observed(settings_screen()))));
    let p = s.progress();
    assert_eq!(p.model_calls, 1);
    assert_eq!(p.step, 0);
}

#[test]
fn lock_while_thinking_drops_the_request_and_asks_again_on_the_fresh_screen() {
    let mut s = session();
    let first = thinking(&mut s);
    expect_wait(&s.step(ev(30, Event::DeviceLocked)));
    // The model's reply arrives late: it was decided on the old screen, so it is dropped.
    expect_wait(&s.step(ev(40, model(TAP_WIFI_CELL))));
    assert_eq!(s.progress().step, 0);
    assert_eq!(
        s.step(ev(50, Event::DeviceUnlocked)),
        Effect::Observe { screenshot: false }
    );
    let second = expect_infer(s.step(ev(60, observed(wifi_screen(true)))));
    assert_ne!(
        first.user, second.user,
        "asked again about the fresh screen"
    );
    assert!(second.user.contains("Switch \"Wi-Fi\""), "{}", second.user);
    assert_eq!(s.progress().model_calls, 2);
}

#[test]
fn a_late_model_failure_is_absorbed_too_but_only_once() {
    let mut s = session();
    thinking(&mut s);
    expect_wait(&s.step(ev(30, Event::DeviceLocked)));
    expect_wait(&s.step(ev(
        40,
        Event::ModelFailed {
            error: "cancelled".into(),
        },
    )));
    let o = finished(s.step(ev(50, model(TAP_WIFI_CELL))));
    assert!(o.summary.starts_with("protocol error"), "{}", o.summary);
}

#[test]
fn a_refused_first_command_sets_the_action_aside_and_refunds_its_step() {
    let mut s = session();
    thinking(&mut s);
    assert!(matches!(
        s.step(ev(30, model(TAP_WIFI_CELL))),
        Effect::Act(_)
    ));
    assert_eq!(s.progress().step, 1);
    expect_wait(&s.step(ev(40, Event::ActResult(RunnerResult::device_locked()))));
    assert_eq!(
        s.progress().step,
        0,
        "nothing happened, the step is refunded"
    );
    assert_eq!(
        s.step(ev(50, Event::DeviceUnlocked)),
        Effect::Observe { screenshot: false }
    );
    let r = expect_infer(s.step(ev(60, observed(settings_screen()))));
    assert!(
        r.user
            .contains("not done: the phone locked first; the screen was read again"),
        "{}",
        r.user
    );
    // The same tap again is a fresh decision, not a repeat towards stuck detection.
    for t in 0..2u64 {
        assert!(matches!(
            s.step(ev(70 + t * 10, model(TAP_WIFI_CELL))),
            Effect::Act(_)
        ));
        s.step(ev(71 + t * 10, Event::ActResult(RunnerResult::ok())));
        expect_infer(s.step(ev(72 + t * 10, observed(settings_screen()))));
    }
}

/// Settings search: the field is focused and typing replaces its value.
fn search_session() -> Session {
    Session::new(
        config(),
        TaskSpec::new("t", "Search the shop for milk"),
        MemoryStore::new(),
    )
}

#[test]
fn lock_part_way_through_an_action_leaves_its_result_unknown_and_learns_nothing() {
    let mut s = search_session();
    s.step(ev(10, Event::Start));
    expect_infer(s.step(ev(20, observed(search_screen(false, None)))));
    let idx = index_of(&search_screen(false, None), "Search");
    let typed = format!(r#"{{"action":"type","element":{idx},"text":"milk"}}"#);
    assert!(matches!(
        s.step(ev(30, model(&typed))),
        Effect::Act(RunnerCommand::Tap { .. })
    ));
    assert_eq!(
        s.step(ev(40, Event::ActResult(RunnerResult::ok()))),
        Effect::Act(RunnerCommand::TypeText {
            text: "milk".into()
        })
    );
    // The tap ran, the typing was refused: the action is half done.
    expect_wait(&s.step(ev(50, Event::ActResult(RunnerResult::device_locked()))));
    assert_eq!(s.progress().step, 1, "part of it ran: no refund");
    s.step(ev(60, Event::DeviceUnlocked));
    let r = expect_infer(s.step(ev(70, observed(search_screen(true, None)))));
    assert!(
        r.user.contains("it may or may not have happened"),
        "{}",
        r.user
    );
    // The model finishes the job; the session succeeds but its recording is not trusted.
    let retype = format!(r#"{{"action":"type","element":{idx},"text":"milk"}}"#);
    assert!(matches!(s.step(ev(80, model(&retype))), Effect::Act(_)));
    assert!(matches!(
        s.step(ev(90, Event::ActResult(RunnerResult::ok()))),
        Effect::Observe { .. }
    ));
    expect_infer(s.step(ev(100, observed(search_screen(true, Some("milk"))))));
    let o = finished(s.step(ev(110, model(r#"{"action":"done","summary":"typed"}"#))));
    assert_eq!(o.status, OutcomeStatus::Success);
    let mem = s.into_memory();
    assert!(
        mem.flows.is_empty(),
        "a lock-interrupted recording is not learned"
    );
    let entry = mem.log.entries().last().unwrap();
    assert_eq!(entry.lock_waits, 1);
    assert_eq!(entry.locked_ms, 10);
}

#[test]
fn device_locked_while_acting_absorbs_one_late_result() {
    let mut s = session();
    thinking(&mut s);
    assert!(matches!(
        s.step(ev(30, model(TAP_WIFI_CELL))),
        Effect::Act(_)
    ));
    expect_wait(&s.step(ev(40, Event::DeviceLocked)));
    // The runner's reply to the abandoned command: absorbed, never acted on.
    expect_wait(&s.step(ev(45, Event::ActResult(RunnerResult::ok()))));
    s.step(ev(50, Event::DeviceUnlocked));
    let r = expect_infer(s.step(ev(60, observed(wifi_screen(true)))));
    assert!(
        r.user.contains("may or may not have happened"),
        "{}",
        r.user
    );
}

#[test]
fn lock_while_verifying_tells_the_model_the_result_was_not_checked() {
    let mut s = session();
    thinking(&mut s);
    s.step(ev(30, model(TAP_WIFI_CELL)));
    assert_eq!(
        s.step(ev(40, Event::ActResult(RunnerResult::ok()))),
        Effect::Observe { screenshot: false }
    );
    assert_eq!(s.progress().phase, Phase::Verifying);
    expect_wait(&s.step(ev(50, locked_observation())));
    // An unlocked observation sent straight away is the fresh one.
    let r = expect_infer(s.step(ev(60, observed(wifi_screen(true)))));
    assert!(
        r.user
            .contains("done, but the phone locked before the result could be checked"),
        "{}",
        r.user
    );
}

#[test]
fn a_pending_confirmation_is_asked_again_after_the_unlock_on_the_same_screen() {
    let mut s = session();
    thinking(&mut s);
    let Effect::Confirm(asked) = s.step(ev(30, model(TAP_AIRPLANE))) else {
        panic!()
    };
    assert_eq!(asked.category, RiskCategory::SystemSetting);
    let (reason, _) = expect_wait(&s.step(ev(40, Event::DeviceLocked)));
    assert!(reason.contains("approval"), "{reason}");
    // An approval given while locked is not carried across the lock.
    expect_wait(&s.step(ev(50, Event::UserDecision { approved: true })));
    assert_eq!(
        s.step(ev(60, Event::DeviceUnlocked)),
        Effect::Observe { screenshot: false }
    );
    // Same screen: the same question again, with no model call in between.
    assert_eq!(
        s.step(ev(70, observed(settings_screen()))),
        Effect::Confirm(asked)
    );
    assert_eq!(s.progress().model_calls, 1);
    assert_eq!(s.progress().step, 1);
    assert!(matches!(
        s.step(ev(80, Event::UserDecision { approved: true })),
        Effect::Act(RunnerCommand::Tap { .. })
    ));
}

#[test]
fn a_pending_confirmation_is_dropped_when_the_screen_changed() {
    let mut s = session();
    thinking(&mut s);
    assert!(matches!(
        s.step(ev(30, model(TAP_AIRPLANE))),
        Effect::Confirm(_)
    ));
    expect_wait(&s.step(ev(40, Event::DeviceLocked)));
    s.step(ev(50, Event::DeviceUnlocked));
    let r = expect_infer(s.step(ev(60, observed(wifi_screen(true)))));
    assert!(r.user.contains("and the screen changed"), "{}", r.user);
    assert_eq!(
        s.progress().step,
        0,
        "the dropped action's step is refunded"
    );
}

#[test]
fn a_decline_sent_while_locked_is_honoured() {
    let mut s = session();
    thinking(&mut s);
    s.step(ev(30, model(TAP_AIRPLANE)));
    expect_wait(&s.step(ev(40, Event::DeviceLocked)));
    expect_wait(&s.step(ev(50, Event::UserDecision { approved: false })));
    s.step(ev(60, Event::DeviceUnlocked));
    // Not asked again: decided afresh, and the model knows the owner said no.
    let r = expect_infer(s.step(ev(70, observed(settings_screen()))));
    assert!(r.user.contains("declined"), "{}", r.user);
    let o = finished(s.step(ev(80, model(TAP_AIRPLANE))));
    assert_eq!(o.status, OutcomeStatus::Blocked, "{}", o.summary);
}

#[test]
fn an_approval_given_before_the_lock_is_asked_again_for_the_unexecuted_action() {
    let mut s = session();
    thinking(&mut s);
    s.step(ev(30, model(TAP_AIRPLANE)));
    assert!(matches!(
        s.step(ev(40, Event::UserDecision { approved: true })),
        Effect::Act(_)
    ));
    // The approved tap never ran: the phone locked first.
    expect_wait(&s.step(ev(50, Event::ActResult(RunnerResult::device_locked()))));
    s.step(ev(60, Event::DeviceUnlocked));
    expect_infer(s.step(ev(70, observed(settings_screen()))));
    assert!(
        matches!(s.step(ev(80, model(TAP_AIRPLANE))), Effect::Confirm(r) if r.category == RiskCategory::SystemSetting),
        "the owner approved a screen state; after a lock they are asked again"
    );
}

#[test]
fn a_question_answered_while_locked_is_kept_an_unanswered_one_is_dropped() {
    let ask = r#"{"action":"ask_user","question":"Which network?"}"#;
    // Answered from the lock screen.
    let mut s = session();
    thinking(&mut s);
    assert!(matches!(s.step(ev(30, model(ask))), Effect::AskUser { .. }));
    expect_wait(&s.step(ev(40, Event::DeviceLocked)));
    expect_wait(&s.step(ev(
        50,
        Event::UserAnswer {
            text: "HomeNet".into(),
        },
    )));
    s.step(ev(60, Event::DeviceUnlocked));
    let r = expect_infer(s.step(ev(70, observed(settings_screen()))));
    assert!(r.user.contains("owner answered: HomeNet"), "{}", r.user);
    // Not answered before the unlock.
    let mut s = session();
    thinking(&mut s);
    s.step(ev(30, model(ask)));
    expect_wait(&s.step(ev(40, Event::DeviceLocked)));
    s.step(ev(60, Event::DeviceUnlocked));
    let r = expect_infer(s.step(ev(70, observed(settings_screen()))));
    assert!(
        r.user.contains("not answered: the phone locked"),
        "{}",
        r.user
    );
}

#[test]
fn a_tool_result_while_locked_is_recorded_a_missing_one_is_unknown() {
    let call = r#"{"action":"call_tool","name":"note","args":{"text":"hi"}}"#;
    let mut s = session();
    thinking(&mut s);
    assert!(matches!(s.step(ev(30, model(call))), Effect::CallTool(_)));
    expect_wait(&s.step(ev(40, Event::DeviceLocked)));
    expect_wait(&s.step(ev(
        50,
        Event::ToolResult(ToolResult {
            ok: true,
            output: "saved".into(),
            error: None,
        }),
    )));
    s.step(ev(60, Event::DeviceUnlocked));
    let r = expect_infer(s.step(ev(70, observed(settings_screen()))));
    assert!(r.user.contains("ok: saved"), "{}", r.user);

    let mut s = session();
    thinking(&mut s);
    s.step(ev(30, model(call)));
    expect_wait(&s.step(ev(40, Event::DeviceLocked)));
    s.step(ev(60, Event::DeviceUnlocked));
    let r = expect_infer(s.step(ev(70, observed(settings_screen()))));
    assert!(
        r.user.contains("before the tool's result came back"),
        "{}",
        r.user
    );
    // A result arriving after the unlock is out of protocol.
    let mut s2 = session();
    thinking(&mut s2);
    s2.step(ev(30, model(call)));
    s2.step(ev(40, Event::DeviceLocked));
    s2.step(ev(50, Event::DeviceUnlocked));
    let o = finished(s2.step(ev(
        60,
        Event::ToolResult(ToolResult {
            ok: true,
            output: String::new(),
            error: None,
        }),
    )));
    assert!(o.summary.starts_with("protocol error"));
}

#[test]
fn an_unlock_the_session_never_heard_locked_still_re_observes_first() {
    let mut s = session();
    thinking(&mut s);
    assert!(matches!(
        s.step(ev(30, model(TAP_WIFI_CELL))),
        Effect::Act(_)
    ));
    // Lock and unlock both happened while the tap was outstanding.
    assert_eq!(
        s.step(ev(40, Event::DeviceUnlocked)),
        Effect::Observe { screenshot: false }
    );
    let r = expect_infer(s.step(ev(50, observed(settings_screen()))));
    assert!(
        r.user.contains("may or may not have happened"),
        "{}",
        r.user
    );
}

#[test]
fn lock_events_are_protocol_errors_before_start_and_ticks_outside_a_wait() {
    let mut s = session();
    let o = finished(s.step(ev(1, Event::DeviceLocked)));
    assert!(o.summary.starts_with("protocol error"), "{}", o.summary);
    let mut s = session();
    thinking(&mut s);
    let o = finished(s.step(ev(30, Event::Tick)));
    assert_eq!(
        o.summary,
        "protocol error: unexpected tick while waiting for the model"
    );
}

#[test]
fn cancel_while_locked_cancels() {
    let mut s = session();
    thinking(&mut s);
    s.step(ev(30, Event::DeviceLocked));
    let o = finished(s.step(ev(40, Event::Cancel)));
    assert_eq!(o.status, OutcomeStatus::Cancelled);
}

// ---------------------------------------------------------------------------------------
// Time: the wait limit, the time budget, clocks that stand still or jump.
// ---------------------------------------------------------------------------------------

fn short_wait_config(max_locked_wait_ms: u64) -> SessionConfig {
    let mut c = config();
    c.policy.max_locked_wait_ms = max_locked_wait_ms;
    c
}

#[test]
fn staying_locked_past_the_limit_finishes_interrupted() {
    let mut s = session_with(short_wait_config(600_000));
    thinking(&mut s);
    expect_wait(&s.step(ev(1_000, Event::DeviceLocked)));
    let (_, w) = expect_wait(&s.step(ev(241_000, Event::Tick)));
    assert_eq!(w, 240_000);
    let (_, w) = expect_wait(&s.step(ev(481_000, Event::Tick)));
    assert_eq!(w, 480_000);
    let o = finished(s.step(ev(721_000, Event::Tick)));
    assert_eq!(o.status, OutcomeStatus::Interrupted);
    assert!(
        o.summary.contains("stayed locked for 12 min"),
        "{}",
        o.summary
    );
    assert!(o.summary.contains("limit is 10 min"), "{}", o.summary);
    assert_eq!(o.duration_ms, 990, "locked time is not active time");
    let mem = s.into_memory();
    let entry = mem.log.entries().last().unwrap();
    assert_eq!(entry.outcome, OutcomeStatus::Interrupted);
    assert_eq!(entry.lock_waits, 1);
    assert_eq!(entry.locked_ms, 720_000);
    assert!(mem.flows.is_empty());
}

#[test]
fn the_default_wait_limit_is_thirty_minutes() {
    let mut s = session();
    thinking(&mut s);
    s.step(ev(20, Event::DeviceLocked));
    expect_wait(&s.step(ev(1_799_999 + 20, Event::Tick)));
    let o = finished(s.step(ev(1_800_000 + 20, Event::Tick)));
    assert_eq!(o.status, OutcomeStatus::Interrupted);
}

#[test]
fn locked_time_does_not_count_against_the_time_budget() {
    let mut c = config();
    c.policy.max_duration_ms = 60_000;
    let mut s = session_with(c);
    s.step(ev(0, Event::Start));
    expect_infer(s.step(ev(10_000, observed(settings_screen()))));
    expect_wait(&s.step(ev(20_000, Event::DeviceLocked)));
    // Sixteen minutes locked, far beyond the one-minute time budget.
    expect_wait(&s.step(ev(1_000_000, Event::Tick)));
    s.step(ev(1_500_000, Event::DeviceUnlocked));
    expect_infer(s.step(ev(1_510_000, observed(settings_screen()))));
    let p = s.progress();
    assert_eq!(p.locked_total_ms, 1_480_000);
    let o = finished(s.step(ev(1_520_000, model(r#"{"action":"done","summary":"ok"}"#))));
    assert_eq!(o.status, OutcomeStatus::Success, "{}", o.summary);
    // 20 s before the lock + 20 s after it.
    assert_eq!(o.duration_ms, 40_000);
    // Active time still runs out: 30 s more is past the budget.
    let mut c = config();
    c.policy.max_duration_ms = 60_000;
    let mut s = session_with(c);
    s.step(ev(0, Event::Start));
    expect_infer(s.step(ev(10_000, observed(settings_screen()))));
    s.step(ev(20_000, Event::DeviceLocked));
    s.step(ev(500_000, Event::DeviceUnlocked));
    let o = finished(s.step(ev(541_000, observed(settings_screen()))));
    assert_eq!(o.summary, "Time budget used up.");
}

#[test]
fn a_clock_that_stands_still_never_times_out_the_wait() {
    let mut s = session_with(short_wait_config(1_000));
    thinking(&mut s);
    s.step(ev(500, Event::DeviceLocked));
    for _ in 0..1_000 {
        let (_, w) = expect_wait(&s.step(ev(500, Event::Tick)));
        assert_eq!(w, 0);
    }
    assert_eq!(
        s.step(ev(500, Event::DeviceUnlocked)),
        Effect::Observe { screenshot: false }
    );
}

#[test]
fn a_clock_that_jumps_back_buys_nothing_and_a_jump_forward_times_out() {
    let mut s = session_with(short_wait_config(600_000));
    thinking(&mut s);
    s.step(ev(100_000, Event::DeviceLocked));
    let (_, w) = expect_wait(&s.step(ev(400_000, Event::Tick)));
    assert_eq!(w, 300_000);
    // Backwards: no time is added, the reading re-anchors.
    let (_, w) = expect_wait(&s.step(ev(50, Event::Tick)));
    assert_eq!(w, 300_000);
    let (_, w) = expect_wait(&s.step(ev(200_050, Event::Tick)));
    assert_eq!(w, 500_000);
    // A jump far forward ends the wait.
    let o = finished(s.step(ev(u64::MAX, Event::Tick)));
    assert_eq!(o.status, OutcomeStatus::Interrupted);
    assert!(s.progress().locked_total_ms >= 600_000);
}

#[test]
fn a_zero_wait_limit_finishes_at_once() {
    let mut s = session_with(short_wait_config(0));
    thinking(&mut s);
    let o = finished(s.step(ev(30, Event::DeviceLocked)));
    assert_eq!(o.status, OutcomeStatus::Interrupted);
}

// ---------------------------------------------------------------------------------------
// Observation failures.
// ---------------------------------------------------------------------------------------

fn observe_failed() -> Event {
    Event::ObserveFailed {
        reason: "snapshot timed out".into(),
    }
}

#[test]
fn a_failed_observation_is_asked_again_until_the_failure_budget_is_used_up() {
    let mut s = session();
    s.step(ev(1, Event::Start));
    for i in 0..3u64 {
        assert_eq!(
            s.step(ev(2 + i, observe_failed())),
            Effect::Observe { screenshot: false },
            "attempt {i}"
        );
    }
    let o = finished(s.step(ev(9, observe_failed())));
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert_eq!(
        o.summary,
        "Could not read the screen (4 attempts in a row): snapshot timed out"
    );
    let mem = s.into_memory();
    assert_eq!(
        mem.log.entries().last().unwrap().outcome,
        OutcomeStatus::Failed
    );
}

#[test]
fn a_successful_observation_resets_the_count() {
    let mut s = session();
    s.step(ev(1, Event::Start));
    s.step(ev(2, observe_failed()));
    s.step(ev(3, observe_failed()));
    s.step(ev(4, observe_failed()));
    expect_infer(s.step(ev(5, observed(settings_screen()))));
    s.step(ev(6, model(TAP_WIFI_CELL)));
    s.step(ev(7, Event::ActResult(RunnerResult::ok())));
    for i in 0..3u64 {
        assert_eq!(
            s.step(ev(8 + i, observe_failed())),
            Effect::Observe { screenshot: false }
        );
    }
    expect_infer(s.step(ev(20, observed(wifi_screen(true)))));
    assert_eq!(s.progress().step, 1, "observation failures take no steps");
}

#[test]
fn a_failed_screenshot_is_asked_again_with_a_screenshot() {
    let mut s = session_with(vision_config());
    s.step(ev(1, Event::Start));
    assert_eq!(
        s.step(ev(2, observed(canvas_screen()))),
        Effect::Observe { screenshot: true }
    );
    assert_eq!(
        s.step(ev(3, observe_failed())),
        Effect::Observe { screenshot: true }
    );
}

#[test]
fn failed_polls_while_locked_are_not_observation_failures() {
    let mut c = config();
    c.policy.max_consecutive_failures = 2;
    let mut s = session_with(c);
    s.step(ev(1, Event::Start));
    expect_wait(&s.step(ev(2, locked_observation())));
    for i in 0..5u64 {
        expect_wait(&s.step(ev(3 + i, observe_failed())));
    }
    assert_eq!(
        s.step(ev(10, Event::DeviceUnlocked)),
        Effect::Observe { screenshot: false }
    );
    assert_eq!(
        s.step(ev(11, observe_failed())),
        Effect::Observe { screenshot: false },
        "one real failure so far"
    );
    expect_infer(s.step(ev(12, observed(settings_screen()))));
}

// ---------------------------------------------------------------------------------------
// Learned flows and locks.
// ---------------------------------------------------------------------------------------

fn wifi_phone() -> Phone {
    Phone::new(settings_screen(), |cmd, s: &Screen| {
        if taps(cmd, s, "Wi-Fi, HomeNet") {
            return Transition::Go(wifi_screen(true));
        }
        if taps(cmd, s, "Wi-Fi") {
            return Transition::Go(wifi_screen(false));
        }
        Transition::Stay
    })
}

/// The Wi-Fi switch's index on the Wi-Fi page.
fn switch_index() -> u32 {
    jarvis_observe::compact(&wifi_screen(true))
        .elements
        .iter()
        .find(|e| e.role == jarvis_proto::Role::Switch)
        .unwrap()
        .index
}

/// Memory holding a learned "Turn off Wi-Fi" flow (cell, then switch).
fn learned() -> MemoryStore {
    let switch = switch_index();
    let tap_switch = format!(r#"{{"action":"tap","element":{switch}}}"#);
    let r = run(
        config(),
        "Turn off Wi-Fi",
        MemoryStore::new(),
        wifi_phone(),
        Script::model(&[TAP_WIFI_CELL, &tap_switch, "done()"]).decisions(&[true, true]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    assert_eq!(r.memory.flows.len(), 1);
    r.memory
}

#[test]
fn a_replay_step_refused_by_the_lock_is_derived_again_with_no_model_call() {
    let memory = learned();
    let mut s = Session::new(config(), TaskSpec::new("t", "Turn off Wi-Fi"), memory);
    s.step(ev(10, Event::Start));
    let Effect::Act(tap_cell) = s.step(ev(20, observed(settings_screen()))) else {
        panic!("replay acts without the model")
    };
    expect_wait(&s.step(ev(30, Event::ActResult(RunnerResult::device_locked()))));
    s.step(ev(40, Event::DeviceUnlocked));
    // Derived again from the fresh screen: the same tap, still no model call.
    assert_eq!(
        s.step(ev(50, observed(settings_screen()))),
        Effect::Act(tap_cell)
    );
    s.step(ev(60, Event::ActResult(RunnerResult::ok())));
    let e = s.step(ev(70, observed(wifi_screen(true))));
    // The switch needs approval; the lock comes while it is asked.
    assert!(
        matches!(&e, Effect::Confirm(r) if r.category == RiskCategory::SystemSetting),
        "{e:?}"
    );
    expect_wait(&s.step(ev(80, Event::DeviceLocked)));
    s.step(ev(90, Event::DeviceUnlocked));
    // Asked again (a flow step is derived again, through the same gate).
    let again = s.step(ev(100, observed(wifi_screen(true))));
    assert_eq!(again, e);
    assert!(matches!(
        s.step(ev(110, Event::UserDecision { approved: true })),
        Effect::Act(_)
    ));
    s.step(ev(120, Event::ActResult(RunnerResult::ok())));
    let o = finished(s.step(ev(130, observed(wifi_screen(false)))));
    assert_eq!(o.status, OutcomeStatus::Success, "{}", o.summary);
    assert_eq!(o.steps, 2, "steps set aside by the lock are refunded");
    assert_eq!(s.progress().model_calls, 0);
    let mem = s.into_memory();
    assert_eq!(mem.flows.flows()[0].successes, 2, "the flow still worked");
}

#[test]
fn a_replay_step_whose_result_the_lock_made_unknown_hands_over_to_the_model() {
    let memory = learned();
    let mut s = Session::new(config(), TaskSpec::new("t", "Turn off Wi-Fi"), memory);
    s.step(ev(10, Event::Start));
    assert!(matches!(
        s.step(ev(20, observed(settings_screen()))),
        Effect::Act(_)
    ));
    s.step(ev(30, Event::ActResult(RunnerResult::ok())));
    // Locked before the tap could be verified: did it happen? Not replayed blindly.
    expect_wait(&s.step(ev(40, locked_observation())));
    s.step(ev(50, Event::DeviceUnlocked));
    let r = expect_infer(s.step(ev(60, observed(wifi_screen(true)))));
    assert!(
        r.user.contains("phone locked during a saved step"),
        "{}",
        r.user
    );
    let switch = switch_index();
    assert!(matches!(
        s.step(ev(
            70,
            model(&format!(r#"{{"action":"tap","element":{switch}}}"#))
        )),
        Effect::Confirm(_)
    ));
    s.step(ev(80, Event::UserDecision { approved: true }));
    s.step(ev(90, Event::ActResult(RunnerResult::ok())));
    expect_infer(s.step(ev(100, observed(wifi_screen(false)))));
    let o = finished(s.step(ev(110, model("done()"))));
    assert_eq!(o.status, OutcomeStatus::Success);
    let mem = s.into_memory();
    assert_eq!(mem.flows.len(), 1, "no second, lock-tainted flow");
    let flow = &mem.flows.flows()[0];
    assert_eq!((flow.successes, flow.failures), (1, 0));
}

#[test]
fn an_interrupted_replay_does_not_count_against_the_flow() {
    let memory = learned();
    let mut c = config();
    c.policy.max_locked_wait_ms = 1_000;
    let mut s = Session::new(c, TaskSpec::new("t", "Turn off Wi-Fi"), memory);
    s.step(ev(10, Event::Start));
    s.step(ev(20, observed(settings_screen())));
    s.step(ev(30, Event::ActResult(RunnerResult::device_locked())));
    let o = finished(s.step(ev(5_000, Event::Tick)));
    assert_eq!(o.status, OutcomeStatus::Interrupted);
    let mem = s.into_memory();
    let flow = &mem.flows.flows()[0];
    assert_eq!((flow.successes, flow.failures, flow.retired), (1, 0, false));
}

// ---------------------------------------------------------------------------------------
// Typed-text verification in the session.
// ---------------------------------------------------------------------------------------

/// A focused search field whose typing drops every second character for the first
/// `drops` `TypeText` commands; Delete removes one character, Return searches.
fn dropping_phone(drops: usize) -> Phone {
    let mut value = String::new();
    let mut left = drops;
    Phone::new(search_screen(true, None), move |cmd, _s: &Screen| {
        match cmd {
            RunnerCommand::TypeText { text } => {
                if left > 0 {
                    left -= 1;
                    value.extend(
                        text.chars()
                            .enumerate()
                            .filter(|(i, _)| i % 2 == 0)
                            .map(|(_, c)| c),
                    );
                } else {
                    value.push_str(text);
                }
            }
            RunnerCommand::PressKey { key: Key::Delete } => {
                value.pop();
            }
            RunnerCommand::PressKey { key: Key::Return } => {
                return Transition::Go(search_screen(false, Some(&format!("{value}!"))));
            }
            _ => return Transition::Stay,
        }
        let shown = (!value.is_empty()).then_some(value.as_str());
        Transition::Go(search_screen(true, shown))
    })
}

fn type_milk() -> String {
    let idx = index_of(&search_screen(true, None), "Search");
    format!(r#"{{"action":"type","element":{idx},"text":"milk","submit":true}}"#)
}

#[test]
fn dropped_characters_are_deleted_and_typed_once_more_before_submitting() {
    let out = type_milk();
    let r = run(
        config(),
        "Search for milk",
        MemoryStore::new(),
        dropping_phone(1),
        Script::model(&[&out, "done()"]),
    );
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{}",
        r.outcome.summary
    );
    let del = RunnerCommand::PressKey { key: Key::Delete };
    assert_eq!(
        r.acts,
        vec![
            RunnerCommand::TypeText {
                text: "milk".into()
            },
            // "ml" arrived: two characters deleted, typed again.
            del.clone(),
            del,
            RunnerCommand::TypeText {
                text: "milk".into()
            },
            RunnerCommand::PressKey { key: Key::Return },
        ]
    );
    assert_eq!(r.infers(), 2);
    assert!(!r.all_prompt_text().contains("did not arrive"));
}

#[test]
fn characters_dropped_twice_are_reported_to_the_model_and_never_submitted() {
    let out = type_milk();
    let r = run(
        config(),
        "Search for milk",
        MemoryStore::new(),
        dropping_phone(2),
        Script::model(&[&out, r#"{"action":"fail","reason":"typing is broken"}"#]),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Failed);
    assert!(
        !r.acts
            .contains(&RunnerCommand::PressKey { key: Key::Return }),
        "wrong text is never submitted: {:?}",
        r.acts
    );
    let types = r
        .acts
        .iter()
        .filter(|c| matches!(c, RunnerCommand::TypeText { .. }))
        .count();
    assert_eq!(types, 2, "one corrective attempt, no loop");
    let told = r.last_user();
    assert!(
        told.contains(
            "failed: the typed text did not arrive intact (characters were dropped): expected \"milk\", the field shows \"ml\"; it was typed again once"
        ),
        "{told}"
    );
}

#[test]
fn a_mangled_personal_value_is_not_echoed_to_the_model() {
    let mut memory = MemoryStore::new();
    memory.profile.set("email", "sam.secret@example.org", false);
    let idx = index_of(&search_screen(true, None), "Search");
    let out = format!(r#"{{"action":"type","element":{idx},"text":"{{{{profile.email}}}}"}}"#);
    let r = run(
        config(),
        "Enter my email",
        memory,
        dropping_phone(2),
        Script::model(&[&out, "fail()"]),
    );
    let told = r.last_user();
    assert!(
        told.contains("expected \"{{profile.email}}\"; it was typed again once"),
        "{told}"
    );
    assert!(
        !r.all_prompt_text().contains("sm.ert"),
        "no fragment of the value"
    );
}

#[test]
fn intact_typing_adds_no_effects_without_submit() {
    let idx = index_of(&search_screen(true, None), "Search");
    let out = format!(r#"{{"action":"type","element":{idx},"text":"milk"}}"#);
    let r = run(
        config(),
        "type milk",
        MemoryStore::new(),
        dropping_phone(0),
        Script::model(&[&out, "done()"]),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Success);
    let names: Vec<&str> = r.effects.iter().map(Effect::name).collect();
    assert_eq!(
        names,
        ["observe", "infer", "act", "observe", "infer", "finished"]
    );
}
