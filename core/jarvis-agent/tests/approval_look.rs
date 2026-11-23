//! The look after an approval in real use: the owner answers inside Jarvis (or comes back
//! via the home screen), so the asked app is brought back once before the look; and a look
//! taken while the screen is still settling gets one short wait and one more look. Nothing
//! is ever acted on unless a look's content record equals the approved one.
//!
//! Effect order: `Observe`, [`Act(LaunchApp asked)`, `Observe`], [`Act(Wait 500)`,
//! `Observe`], then the approved action's commands or (stale) the next decision.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_agent::{Session, APPROVAL_SETTLE_MS, MAX_STALE_APPROVALS};
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, ModelOutput, Observation, OutcomeStatus, Phase, Role, RunnerCommand,
    RunnerResult, Screen, SessionConfig, TaskSpec, TimedEvent, UiNode,
};

const AGENT: &str = "com.example.jarvis";
const SHOP: &str = "com.example.shop";

fn app_root(label: &str, children: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(label)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ])
}

/// A product page with "Buy now" (a purchase: asks) and another button.
fn shop(other: &str) -> Screen {
    screen(
        SHOP,
        "Shop",
        app_root(
            "Shop",
            vec![
                text("Headphones $199.00", rect(16.0, 120.0, 361.0, 30.0)),
                node(Role::Button, "Buy now", rect(16.0, 300.0, 361.0, 50.0)),
                node(Role::Button, other, rect(16.0, 700.0, 361.0, 50.0)),
            ],
        ),
        false,
    )
}

fn asked() -> Screen {
    shop("Add to wishlist")
}

/// The same page with other content: not what the owner approved.
fn changed() -> Screen {
    shop("Save for later")
}

/// Jarvis's own app, where the owner answered the question.
fn jarvis_app() -> Screen {
    screen(
        AGENT,
        "Jarvis",
        app_root(
            "Jarvis",
            vec![
                text("Buy the headphones", rect(16.0, 100.0, 361.0, 30.0)),
                node(Role::Button, "Stop", rect(16.0, 700.0, 361.0, 50.0)),
            ],
        ),
        false,
    )
}

/// The approved tap, carrying what it expects of the phone (the stale-screen guard's
/// layer 2: the app it was re-checked on is in front).
fn tap() -> RunnerCommand {
    RunnerCommand::Tap {
        x: 196.5,
        y: 325.0,
        expect: Some(jarvis_proto::Expect {
            bundle_id: SHOP.to_owned(),
        }),
    }
}

fn launch_shop() -> Effect {
    Effect::Act(RunnerCommand::LaunchApp {
        bundle_id: SHOP.to_owned(),
    })
}

fn settle_wait() -> Effect {
    Effect::Act(RunnerCommand::Wait {
        ms: APPROVAL_SETTLE_MS,
    })
}

fn agent_config() -> SessionConfig {
    let mut c = config();
    c.agent_bundle_id = Some(AGENT.to_owned());
    c
}

struct Driver {
    s: Session,
    now: u64,
}

impl Driver {
    fn new(config: SessionConfig) -> Self {
        Driver {
            s: Session::new(
                config,
                TaskSpec::new("t", "Buy the headphones"),
                MemoryStore::new(),
            ),
            now: 1_000,
        }
    }

    fn step(&mut self, e: Event) -> Effect {
        self.now += 50;
        self.s.step(TimedEvent::new(self.now, e))
    }

    fn observed(&mut self, screen: &Screen) -> Effect {
        self.step(Event::Observed(Observation::new(screen.clone())))
    }

    fn ok(&mut self) -> Effect {
        self.step(Event::ActResult(RunnerResult::ok()))
    }

    fn model(&mut self, text: &str) -> Effect {
        self.step(Event::ModelOutput(ModelOutput {
            text: text.to_owned(),
        }))
    }

    /// Start, ask to buy on `asked()`, approve: the look is outstanding.
    fn approved(config: SessionConfig) -> Self {
        let mut d = Driver::new(config);
        assert!(matches!(d.step(Event::Start), Effect::Observe { .. }));
        assert!(matches!(d.observed(&asked()), Effect::Infer(_)));
        d.ask_and_approve();
        d
    }

    /// From a decision on `asked()`: the model taps "Buy now", the owner approves.
    fn ask_and_approve(&mut self) {
        let e = self.model(&tap_buy());
        assert!(matches!(e, Effect::Confirm(_)), "{e:?}");
        assert_eq!(
            self.step(Event::UserDecision { approved: true }),
            Effect::Observe { screenshot: false }
        );
    }
}

fn tap_buy() -> String {
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&asked(), "Buy now")
    )
}

fn expect_stale(e: &Effect) {
    let Effect::Infer(r) = e else {
        panic!("expected the next decision, got {e:?}")
    };
    assert!(
        r.user
            .contains("the screen changed after the owner approved it"),
        "{}",
        r.user
    );
}

fn locked() -> Event {
    Event::Observed(Observation {
        device_locked: true,
        ..Observation::new(home_screen())
    })
}

fn expect_wait(e: &Effect) {
    let Effect::AwaitUnlock { reason, .. } = e else {
        panic!("expected AwaitUnlock, got {e:?}")
    };
    assert!(reason.contains("approval"), "{reason}");
}

// ---------------------------------------------------------------------------------------
// 1. The agent's own app or the home screen in front at the look.
// ---------------------------------------------------------------------------------------

#[test]
fn approved_in_jarvis_brings_the_app_back_then_acts_on_the_same_screen() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(d.s.progress().phase, Phase::Observing);
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    assert_eq!(d.observed(&asked()), Effect::Act(tap()));
    // The activation is not a step: only the approved tap counts.
    assert_eq!(d.s.progress().step, 1);
}

#[test]
fn the_home_screen_in_front_brings_the_app_back_too() {
    // With or without an agent bundle id configured.
    for c in [config(), agent_config()] {
        let mut d = Driver::approved(c);
        assert_eq!(d.observed(&home_screen()), launch_shop());
        assert_eq!(d.ok(), Effect::Observe { screenshot: false });
        assert_eq!(d.observed(&asked()), Effect::Act(tap()));
    }
}

#[test]
fn without_an_agent_bundle_id_its_app_is_just_another_app() {
    let mut d = Driver::approved(config());
    expect_stale(&d.observed(&jarvis_app()));
}

#[test]
fn after_the_activation_other_content_is_not_acted_on() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    // The app came back with other content: one settle wait, one more look, still other.
    assert_eq!(d.observed(&changed()), settle_wait());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    expect_stale(&d.observed(&changed()));
}

#[test]
fn after_the_activation_a_screen_that_settles_is_acted_on() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&home_screen()), launch_shop());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    // The switch is still finishing: the home screen once more.
    assert_eq!(d.observed(&home_screen()), settle_wait());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    assert_eq!(d.observed(&asked()), Effect::Act(tap()));
}

#[test]
fn still_the_wrong_app_after_the_activation_is_stale_with_no_second_activation() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    assert_eq!(d.observed(&jarvis_app()), settle_wait());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    expect_stale(&d.observed(&jarvis_app()));
}

#[test]
fn a_third_app_in_front_is_stale_at_once_with_no_activation_and_no_wait() {
    let mut d = Driver::approved(agent_config());
    expect_stale(&d.observed(&wifi_screen(false)));
}

#[test]
fn a_third_app_after_the_activation_is_stale_at_once() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    expect_stale(&d.observed(&wifi_screen(false)));
}

#[test]
fn a_denied_app_in_front_after_the_activation_ends_the_session_blocked() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    let Effect::Finished(o) = d.observed(&passwords_screen()) else {
        panic!()
    };
    assert_eq!(o.status, OutcomeStatus::Blocked, "{}", o.summary);
}

#[test]
fn a_failed_activation_still_looks_and_acts_only_on_the_same_screen() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(
        d.step(Event::ActResult(RunnerResult::err("could not launch"))),
        Effect::Observe { screenshot: false }
    );
    assert_eq!(d.observed(&jarvis_app()), settle_wait());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    expect_stale(&d.observed(&jarvis_app()));
}

/// Jarvis itself is the app being driven: the confirmation sheet animating away shows at
/// the first look. No activation (the asked app is in front), one settle wait.
#[test]
fn when_jarvis_is_the_asked_app_the_sheet_settles_without_an_activation() {
    let sheet = |up: bool| {
        let mut children = vec![
            text("Plan: Pro $9.99", rect(16.0, 120.0, 361.0, 30.0)),
            node(Role::Button, "Buy now", rect(16.0, 300.0, 361.0, 50.0)),
        ];
        if up {
            children.push(text("Approve?", rect(16.0, 600.0, 361.0, 30.0)));
        }
        screen(AGENT, "Jarvis", app_root("Jarvis", children), false)
    };
    let mut c = agent_config();
    c.policy.mode = jarvis_proto::PolicyMode::StepByStep;
    let mut d = Driver::new(c);
    d.step(Event::Start);
    assert!(matches!(d.observed(&sheet(false)), Effect::Infer(_)));
    let i = index_of(&sheet(false), "Buy now");
    assert!(matches!(
        d.model(&format!(r#"{{"action":"tap","element":{i}}}"#)),
        Effect::Confirm(_)
    ));
    assert!(matches!(
        d.step(Event::UserDecision { approved: true }),
        Effect::Observe { .. }
    ));
    assert_eq!(d.observed(&sheet(true)), settle_wait());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    // The tap expects Jarvis's own app, where it was asked and re-checked.
    assert_eq!(
        d.observed(&sheet(false)),
        Effect::Act(tap().with_expect(Some(jarvis_proto::Expect {
            bundle_id: AGENT.to_owned(),
        })))
    );
}

// ---------------------------------------------------------------------------------------
// 2. The settle retry.
// ---------------------------------------------------------------------------------------

#[test]
fn a_first_look_that_differs_then_settles_is_acted_on() {
    let mut d = Driver::approved(config());
    assert_eq!(d.observed(&changed()), settle_wait());
    assert_eq!(d.s.progress().phase, Phase::Observing);
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    assert_eq!(d.observed(&asked()), Effect::Act(tap()));
    assert_eq!(d.s.progress().step, 1, "the extra look is not a step");
}

#[test]
fn two_differing_looks_are_stale_once() {
    let mut d = Driver::approved(config());
    assert_eq!(d.observed(&changed()), settle_wait());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    expect_stale(&d.observed(&changed()));
    assert_eq!(d.s.progress().step, 1);
    assert!(format!("{:?}", d.s).contains("stale_approvals: 1"));
}

/// Every stale approval costs one wait and counts once: the session ends after exactly
/// [`MAX_STALE_APPROVALS`] approvals, each with exactly one settle wait.
#[test]
fn the_stale_count_rises_once_per_approval_not_per_look() {
    // Each round asks on a screen of its own (so no stuck rule fires) and shows another
    // content at both looks after the approval.
    let asked_in = |r: u32| shop(&format!("Add {r}"));
    let changed_in = |r: u32| shop(&format!("Later {r}"));
    let mut d = Driver::new(config());
    d.step(Event::Start);
    let mut e = d.observed(&asked_in(1));
    let mut waits = 0;
    for round in 1..=MAX_STALE_APPROVALS {
        assert!(matches!(e, Effect::Infer(_)), "{e:?}");
        d.ask_and_approve();
        assert_eq!(d.observed(&changed_in(round)), settle_wait());
        waits += 1;
        assert_eq!(d.ok(), Effect::Observe { screenshot: false });
        e = d.observed(&changed_in(round));
        if round < MAX_STALE_APPROVALS {
            expect_stale(&e);
            assert!(matches!(
                d.model(r#"{"action":"wait","ms":100}"#),
                Effect::Act(_)
            ));
            assert!(matches!(d.ok(), Effect::Observe { .. }));
            e = d.observed(&asked_in(round + 1));
        }
    }
    let Effect::Finished(o) = e else {
        panic!("{e:?}")
    };
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert!(
        o.summary
            .contains(&format!("after you approved ({MAX_STALE_APPROVALS} times")),
        "{}",
        o.summary
    );
    assert_eq!(waits, MAX_STALE_APPROVALS);
}

#[test]
fn the_full_sequence_is_look_activate_look_wait_look() {
    let mut d = Driver::approved(agent_config());
    let mut effects = vec![d.observed(&jarvis_app())];
    effects.push(d.ok());
    effects.push(d.observed(&changed()));
    effects.push(d.ok());
    effects.push(d.observed(&asked()));
    assert_eq!(
        effects,
        vec![
            launch_shop(),
            Effect::Observe { screenshot: false },
            settle_wait(),
            Effect::Observe { screenshot: false },
            Effect::Act(tap()),
        ]
    );
    // The tap ran; verification and the next decision follow as usual.
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
}

// ---------------------------------------------------------------------------------------
// Budgets are not inflated by the extra looks and still end sessions.
// ---------------------------------------------------------------------------------------

#[test]
fn the_step_budget_counts_the_approved_action_once() {
    let mut c = agent_config();
    c.policy.max_steps = 1;
    let mut d = Driver::approved(c);
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    d.ok();
    assert_eq!(d.observed(&changed()), settle_wait());
    d.ok();
    assert_eq!(d.observed(&asked()), Effect::Act(tap()));
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    let Effect::Finished(o) = d.observed(&asked()) else {
        panic!()
    };
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert!(
        o.summary.contains("Step budget used up (1 steps)"),
        "{}",
        o.summary
    );
}

#[test]
fn the_failure_budget_counts_one_failure_per_stale_approval() {
    let mut c = config();
    c.policy.max_consecutive_failures = 2;
    let mut d = Driver::approved(c);
    assert_eq!(d.observed(&changed()), settle_wait());
    d.ok();
    // One failure so far (not two for two looks): the session goes on.
    expect_stale(&d.observed(&asked_after_change()));
    let e = d.model(&tap_buy_on(&asked_after_change()));
    assert!(matches!(e, Effect::Confirm(_)), "{e:?}");
    assert!(matches!(
        d.step(Event::UserDecision { approved: true }),
        Effect::Observe { .. }
    ));
    assert_eq!(d.observed(&changed()), settle_wait());
    d.ok();
    let Effect::Finished(o) = d.observed(&changed()) else {
        panic!()
    };
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert!(
        o.summary.contains("2 failed actions in a row"),
        "{}",
        o.summary
    );
}

fn asked_after_change() -> Screen {
    shop("Compare")
}

fn tap_buy_on(s: &Screen) -> String {
    format!(r#"{{"action":"tap","element":{}}}"#, index_of(s, "Buy now"))
}

#[test]
fn the_time_budget_ends_the_session_during_the_wait() {
    let mut c = config();
    c.policy.max_duration_ms = 10_000;
    let mut d = Driver::approved(c);
    assert_eq!(d.observed(&changed()), settle_wait());
    d.now += 20_000;
    let Effect::Finished(o) = d.ok() else {
        panic!()
    };
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert_eq!(o.summary, "Time budget used up.");
}

// ---------------------------------------------------------------------------------------
// Lock, cancel and observe failures at every new state.
// ---------------------------------------------------------------------------------------

/// After the unlock on the asked screen: asked again, never acted on the old approval.
fn expect_asked_again(d: &mut Driver) {
    let e = d.observed(&asked());
    assert!(matches!(e, Effect::Confirm(_)), "{e:?}");
}

#[test]
fn a_lock_refusing_the_activation_is_a_lock_during_the_look() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    expect_wait(&d.step(Event::ActResult(RunnerResult::device_locked())));
    expect_wait(&d.step(Event::Tick));
    assert_eq!(d.s.progress().phase, Phase::WaitingForUnlock);
    expect_asked_again(&mut d);
}

#[test]
fn a_lock_event_during_the_activation_drops_its_late_result() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    expect_wait(&d.step(Event::DeviceLocked));
    // The activation's late result is absorbed.
    expect_wait(&d.ok());
    assert_eq!(
        d.step(Event::DeviceUnlocked),
        Effect::Observe { screenshot: false }
    );
    expect_asked_again(&mut d);
}

#[test]
fn a_lock_at_the_look_after_the_activation_is_a_lock_during_the_look() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    expect_wait(&d.step(locked()));
    expect_asked_again(&mut d);
}

#[test]
fn a_lock_during_the_settle_wait_is_a_lock_during_the_look() {
    let mut d = Driver::approved(config());
    assert_eq!(d.observed(&changed()), settle_wait());
    expect_wait(&d.step(Event::ActResult(RunnerResult::device_locked())));
    expect_asked_again(&mut d);

    let mut d = Driver::approved(config());
    assert_eq!(d.observed(&changed()), settle_wait());
    expect_wait(&d.step(Event::DeviceLocked));
    expect_wait(&d.ok());
    // Unlocked on another screen: not asked again, decided afresh.
    let e = d.observed(&changed());
    assert!(matches!(e, Effect::Infer(_)), "{e:?}");
}

#[test]
fn a_lock_at_the_look_after_the_wait_is_a_lock_during_the_look() {
    let mut d = Driver::approved(config());
    assert_eq!(d.observed(&changed()), settle_wait());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    expect_wait(&d.step(locked()));
    expect_asked_again(&mut d);
}

#[test]
fn an_approval_given_before_a_lock_at_any_new_state_is_never_acted_on() {
    // Lock at the activation, the look after it, the wait and the look after it; after
    // the unlock the same screen is shown and nothing approved is sent: the owner is asked.
    for at in 0..4 {
        let mut d = Driver::approved(agent_config());
        let mut e = d.observed(&jarvis_app());
        let mut n = 0;
        loop {
            if n == at {
                expect_wait(&d.step(Event::DeviceLocked));
                break;
            }
            e = match e {
                Effect::Act(_) => d.ok(),
                Effect::Observe { .. } => d.observed(&changed()),
                other => panic!("{other:?}"),
            };
            n += 1;
        }
        assert_eq!(
            d.step(Event::DeviceUnlocked),
            Effect::Observe { screenshot: false }
        );
        expect_asked_again(&mut d);
    }
}

#[test]
fn cancel_is_honoured_at_every_new_state() {
    for at in 0..4 {
        let mut d = Driver::approved(agent_config());
        let mut e = d.observed(&jarvis_app());
        for _ in 0..at {
            e = match e {
                Effect::Act(_) => d.ok(),
                Effect::Observe { .. } => d.observed(&changed()),
                other => panic!("{other:?}"),
            };
        }
        let Effect::Finished(o) = d.step(Event::Cancel) else {
            panic!()
        };
        assert_eq!(o.status, OutcomeStatus::Cancelled);
    }
}

#[test]
fn a_failed_look_after_the_wait_or_the_activation_is_looked_again() {
    let mut d = Driver::approved(agent_config());
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    assert_eq!(
        d.step(Event::ObserveFailed {
            reason: "busy".into()
        }),
        Effect::Observe { screenshot: false }
    );
    assert_eq!(d.observed(&changed()), settle_wait());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    assert_eq!(
        d.step(Event::ObserveFailed {
            reason: "busy".into()
        }),
        Effect::Observe { screenshot: false }
    );
    assert_eq!(d.observed(&asked()), Effect::Act(tap()));
}

#[test]
fn an_observe_failure_during_the_wait_is_a_protocol_error() {
    let mut d = Driver::approved(config());
    assert_eq!(d.observed(&changed()), settle_wait());
    let Effect::Finished(o) = d.step(Event::ObserveFailed {
        reason: "busy".into(),
    }) else {
        panic!()
    };
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert!(o.summary.contains("protocol error"), "{}", o.summary);
}

#[test]
fn a_first_use_approval_brings_the_app_back_too() {
    let mut c = agent_config();
    c.policy.first_use_approval = true;
    let mut d = Driver::new(c);
    d.step(Event::Start);
    d.observed(&asked());
    let i = index_of(&asked(), "Add to wishlist");
    let Effect::Confirm(fu) = d.model(&format!(r#"{{"action":"tap","element":{i}}}"#)) else {
        panic!()
    };
    assert_eq!(fu.category, jarvis_proto::RiskCategory::FirstUse);
    assert!(matches!(
        d.step(Event::UserDecision { approved: true }),
        Effect::Observe { .. }
    ));
    assert_eq!(d.observed(&jarvis_app()), launch_shop());
    assert_eq!(d.ok(), Effect::Observe { screenshot: false });
    assert!(matches!(
        d.observed(&asked()),
        Effect::Act(RunnerCommand::Tap { .. })
    ));
}
