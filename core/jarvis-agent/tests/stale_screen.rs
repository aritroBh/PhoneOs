#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Stale-screen guard: the gate decides on the fresh look (layer 1), and the runner's
//! refusal when another app is in front (layer 2, `RunnerFailure::Stale`).
mod common;

use common::*;
use jarvis_agent::{Session, MAX_REPLANS};
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, ModelOutput, Observation, Role, RunnerCommand, RunnerResult, Screen, TaskSpec,
    TimedEvent, UiNode,
};

const SHOP: &str = "com.example.shop";

/// A form: a "Note" field (empty or holding `note`), a "Continue" button, and a neutral
/// "Help" button.
fn form(note: Option<&str>) -> Screen {
    let mut field = UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 44.0))
        .with_label("Note")
        .hittable(true);
    if let Some(v) = note {
        field = field.with_value(v);
    }
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Shop")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![
                field,
                node(Role::Button, "Continue", rect(16.0, 300.0, 361.0, 50.0)),
                node(Role::Button, "Help", rect(16.0, 700.0, 361.0, 50.0)),
            ])]);
    screen(SHOP, "Shop", root, false)
}

struct Driver {
    s: Session,
    now: u64,
}

impl Driver {
    fn new() -> Driver {
        Driver {
            s: Session::new(
                config(),
                TaskSpec::new("t", "Finish the form"),
                MemoryStore::new(),
            ),
            now: 1_000,
        }
    }
    fn at(&mut self, dt: u64, e: Event) -> Effect {
        self.now += dt;
        self.s.step(TimedEvent::new(self.now, e))
    }
    fn observed(&mut self, dt: u64, screen: &Screen) -> Effect {
        self.at(dt, Event::Observed(Observation::new(screen.clone())))
    }
    fn model(&mut self, dt: u64, text: &str) -> Effect {
        self.at(
            dt,
            Event::ModelOutput(ModelOutput {
                text: text.to_owned(),
            }),
        )
    }
    fn tap(&mut self, dt: u64, screen: &Screen, label: &str) -> Effect {
        let i = index_of(screen, label);
        self.model(dt, &format!(r#"{{"action":"tap","element":{i}}}"#))
    }
    fn ok(&mut self) -> Effect {
        self.at(50, Event::ActResult(RunnerResult::ok()))
    }
}

/// Jarvis acts once ("Help"), so text that enters a field afterwards sets the app's
/// "typed here" state (thirteenth review). Returns the driver waiting for the model's next
/// reply on the empty form.
fn acted_once() -> Driver {
    let mut d = Driver::new();
    assert!(matches!(d.at(0, Event::Start), Effect::Observe { .. }));
    assert!(matches!(d.observed(50, &form(None)), Effect::Infer(_)));
    assert!(matches!(
        d.tap(100, &form(None), "Help"),
        Effect::Act(RunnerCommand::Tap { .. })
    ));
    assert!(matches!(d.ok(), Effect::Observe { .. }));
    // Run 146: "Help" changed nothing on the first look: one settle wait, one more look.
    assert_eq!(
        d.observed(50, &form(None)),
        Effect::Act(RunnerCommand::Wait {
            ms: jarvis_agent::VERIFY_SETTLE_MS
        })
    );
    assert!(matches!(d.ok(), Effect::Observe { .. }));
    assert!(matches!(d.observed(50, &form(None)), Effect::Infer(_)));
    d
}

#[test]
fn the_gate_decides_on_the_fresh_look_and_asks_when_the_form_now_holds_text() {
    // The model chose "Continue" on the empty form, 3 s ago; meanwhile the page filled the
    // field (AutoFill, a paste, a script). The control is the same, so the plan holds, but
    // "Continue" now presumably submits text that entered after Jarvis acted: the gate,
    // run on the fresh look, asks.
    let mut d = acted_once();
    assert_eq!(
        d.tap(3_000, &form(None), "Continue"),
        Effect::Observe { screenshot: false },
        "a 3 s old plan looks again first"
    );
    let e = d.observed(50, &form(Some("my home address")));
    assert!(
        matches!(e, Effect::Confirm(_)),
        "the gate must decide on the fresh screen (the form now holds text): {e:?}"
    );
    assert_eq!(d.s.progress().fresh_looks, 1);
    assert_eq!(d.s.progress().stale_plans, 0);
}

#[test]
fn a_young_plan_is_gated_on_its_own_look() {
    // The same choice made quickly: no extra look; the empty form is what the gate read.
    let mut d = acted_once();
    let e = d.tap(200, &form(None), "Continue");
    assert!(matches!(e, Effect::Act(RunnerCommand::Tap { .. })), "{e:?}");
    assert_eq!(d.s.progress().fresh_looks, 0);
}

#[test]
fn a_runner_stale_refusal_refunds_the_step_and_looks_again() {
    let mut d = acted_once();
    let steps = d.s.progress().step;
    // A young plan: sent at once, with the app it was planned on.
    let e = d.tap(200, &form(None), "Help");
    let Effect::Act(cmd) = e else { panic!("{e:?}") };
    assert_eq!(cmd.expect().map(|x| x.bundle_id.as_str()), Some(SHOP));
    // The runner reads the home screen in front: refused, nothing happened.
    let e = d.at(
        50,
        Event::ActResult(RunnerResult::stale("com.apple.springboard")),
    );
    assert_eq!(e, Effect::Observe { screenshot: false });
    assert_eq!(d.s.progress().step, steps, "the refused step is refunded");
    assert_eq!(d.s.progress().stale_plans, 1);
    let e = d.observed(50, &form(None));
    let Effect::Infer(req) = e else {
        panic!("{e:?}")
    };
    assert!(
        req.user.contains("another app came to the front"),
        "{}",
        req.user
    );
}

#[test]
fn runner_stale_refusals_are_bounded_then_a_failed_step() {
    let mut d = acted_once();
    let steps = d.s.progress().step;
    for i in 0..=MAX_REPLANS {
        assert!(matches!(
            d.tap(200, &form(None), "Help"),
            Effect::Act(RunnerCommand::Tap { .. })
        ));
        let e = d.at(
            50,
            Event::ActResult(RunnerResult::stale("com.apple.springboard")),
        );
        assert_eq!(e, Effect::Observe { screenshot: false });
        let refunded = i < MAX_REPLANS;
        assert_eq!(
            d.s.progress().step,
            if refunded { steps } else { steps + 1 },
            "refusal {i}"
        );
        assert!(matches!(d.observed(50, &form(None)), Effect::Infer(_)));
    }
}

#[test]
fn an_old_plan_in_an_app_that_left_the_front_is_not_sent_and_decided_again() {
    let mut d = acted_once();
    assert_eq!(
        d.tap(20_000, &form(None), "Continue"),
        Effect::Observe { screenshot: false }
    );
    // Run 101: the app left the foreground while the model thought.
    let e = d.observed(50, &home_screen());
    let Effect::Infer(req) = e else {
        panic!("not acted on: {e:?}")
    };
    assert!(
        req.user
            .contains("The screen changed before the action (another app is in front)"),
        "{}",
        req.user
    );
    assert_eq!(d.s.progress().stale_plans, 1);
}
