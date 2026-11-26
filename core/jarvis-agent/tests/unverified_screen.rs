#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! A screen read from the third-party runner's text format is not a trusted source (fifth
//! review): the format is ambiguous and on-screen text can forge nodes. Every
//! state-changing runner action on such a screen asks (at least `Unverified`), with a
//! fixed warning first in the summary; waits, reads and plain scrolls do not.

mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::settings_screen;
use jarvis_proto::{
    Effect, Event, ModelOutput, Observation, RiskCategory, RunnerCommand, Screen, TaskSpec,
    TimedEvent,
};
use jarvis_safety::UNVERIFIED_SCREEN_WARNING;

struct D {
    s: Session,
    now: u64,
}

impl D {
    fn new(memory: MemoryStore) -> Self {
        D {
            s: Session::new(config(), TaskSpec::new("t", "Turn off Wi-Fi"), memory),
            now: 1_000,
        }
    }
    fn step(&mut self, e: Event) -> Effect {
        self.now += 50;
        self.s.step(TimedEvent::new(self.now, e))
    }
    fn observe(&mut self, s: &Screen, text: bool) -> Effect {
        let obs = Observation::new(s.clone());
        self.step(if text {
            Event::ObservedRunnerText(obs)
        } else {
            Event::Observed(obs)
        })
    }
    fn model(&mut self, out: &str) -> Effect {
        self.step(Event::ModelOutput(ModelOutput {
            text: out.to_owned(),
        }))
    }
}

/// The settings root as a typed tree and as runner text would give it.
fn start(text: bool, out: &str) -> (D, Effect) {
    let mut d = D::new(MemoryStore::new());
    assert!(matches!(d.step(Event::Start), Effect::Observe { .. }));
    assert!(matches!(
        d.observe(&settings_screen(), text),
        Effect::Infer(_)
    ));
    let e = d.model(out);
    (d, e)
}

fn wifi_row() -> u32 {
    index_of(&settings_screen(), "Wi-Fi, HomeNet")
}

#[test]
fn a_runner_text_screen_asks_before_a_tap_the_typed_tree_does_not() {
    let tap = format!(r#"{{"action":"tap","element":{}}}"#, wifi_row());
    let (_, typed) = start(false, &tap);
    assert!(
        matches!(typed, Effect::Act(RunnerCommand::Tap { .. })),
        "{typed:?}"
    );
    let (mut d, text) = start(true, &tap);
    let Effect::Confirm(req) = text else {
        panic!("expected a confirmation, got {text:?}")
    };
    assert_eq!(req.category, RiskCategory::Unverified);
    assert!(
        req.summary.starts_with(UNVERIFIED_SCREEN_WARNING),
        "{}",
        req.summary
    );
    assert!(
        req.summary.contains("Tap \"Wi-Fi, HomeNet\""),
        "{}",
        req.summary
    );
    // Approved: the look (runner text again) runs it.
    assert!(matches!(
        d.step(Event::phone_decision(true)),
        Effect::Observe { .. }
    ));
    let e = d.observe(&settings_screen(), true);
    assert!(matches!(e, Effect::Act(RunnerCommand::Tap { .. })), "{e:?}");
}

/// An approval given on a runner-text screen does not carry to a typed look of the same
/// screen (or back): the summary differs by the warning, so it is void (fail closed).
#[test]
fn an_approval_does_not_cross_screen_sources() {
    let tap = format!(r#"{{"action":"tap","element":{}}}"#, wifi_row());
    let (mut d, e) = start(true, &tap);
    assert!(matches!(e, Effect::Confirm(_)));
    assert!(matches!(
        d.step(Event::phone_decision(true)),
        Effect::Observe { .. }
    ));
    let e = d.observe(&settings_screen(), false);
    assert!(
        !matches!(e, Effect::Act(RunnerCommand::Tap { .. })),
        "{e:?}"
    );
}

/// A risky action keeps its own category, with the warning in front.
#[test]
fn a_risky_action_on_runner_text_keeps_its_category_with_the_warning() {
    let s = wifi_screen(true);
    let switch = jarvis_observe::compact(&s)
        .elements
        .iter()
        .find(|e| e.role == jarvis_proto::Role::Switch)
        .unwrap()
        .index;
    let mut d = D::new(MemoryStore::new());
    d.step(Event::Start);
    assert!(matches!(d.observe(&s, true), Effect::Infer(_)));
    let e = d.model(&format!(r#"{{"action":"tap","element":{switch}}}"#));
    let Effect::Confirm(req) = e else {
        panic!("{e:?}")
    };
    assert_eq!(req.category, RiskCategory::SystemSetting);
    assert!(req.summary.starts_with(UNVERIFIED_SCREEN_WARNING));
    assert!(
        req.summary.contains("Turn off \"Wi-Fi\""),
        "{}",
        req.summary
    );
}

/// Waiting, reading and scrolling a plain list change nothing and do not ask.
#[test]
fn waits_reads_and_plain_scrolls_do_not_ask_on_runner_text() {
    for out in [
        r#"{"action":"wait","ms":500}"#,
        r#"{"action":"scroll","direction":"down"}"#,
    ] {
        let (_, e) = start(true, out);
        assert!(matches!(e, Effect::Act(_)), "{out}: {e:?}");
    }
    // `look` without a vision model is answered in the history, never asked.
    let (_, e) = start(true, r#"{"action":"look"}"#);
    assert!(matches!(e, Effect::Infer(_)), "{e:?}");
}
