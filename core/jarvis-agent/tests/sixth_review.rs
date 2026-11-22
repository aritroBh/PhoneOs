#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixth review: every tap lands on the control it names; Return after typing is its own
//! gated action; nothing in or near the keyboard is excluded from the digest; runner-text
//! screens always confirm; summaries name fields and show values only when safe; `type`
//! without an element needs a listed focused field. The review's probes are kept here as
//! named regression tests (adapted where the specification defines different behaviour).

mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact, screen_digest};
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    AppRule, Effect, Event, Key, ModelOutput, Observation, OutcomeStatus, Point, Policy,
    RiskCategory, Role, RunnerCommand, RunnerResult, Screen, SessionConfig, TaskSpec, TimedEvent,
    TrustLevel, UiNode,
};

const BANK: &str = "com.example.bank";

fn app_root_of(children: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Bank")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ])
}

fn scr(children: Vec<UiNode>) -> Screen {
    screen(BANK, "Bank", app_root_of(children), false)
}

/// Steps a session by hand.
struct D {
    s: Session,
    now: u64,
}

impl D {
    fn new() -> Self {
        Self::with(config())
    }
    fn with(config: SessionConfig) -> Self {
        D {
            s: Session::new(
                config,
                TaskSpec::new("t", "Pay my bill"),
                MemoryStore::new(),
            ),
            now: 1_000,
        }
    }
    fn step(&mut self, e: Event) -> Effect {
        self.now += 50;
        self.s.step(TimedEvent::new(self.now, e))
    }
    fn observed(&mut self, s: &Screen) -> Effect {
        self.step(Event::Observed(Observation::new(s.clone())))
    }
    fn start(&mut self, s: &Screen, action: &str) -> Effect {
        assert!(matches!(self.step(Event::Start), Effect::Observe { .. }));
        assert!(matches!(self.observed(s), Effect::Infer(_)));
        self.model(action)
    }
    fn model(&mut self, action: &str) -> Effect {
        self.step(Event::ModelOutput(ModelOutput {
            text: action.to_owned(),
        }))
    }
    fn ask(&mut self, asked: &Screen, action: &str) -> String {
        let e = self.start(asked, action);
        let Effect::Confirm(req) = e else {
            panic!("expected a confirmation, got {e:?}")
        };
        req.summary
    }
    fn approve(&mut self) {
        assert_eq!(
            self.step(Event::UserDecision { approved: true }),
            Effect::Observe { screenshot: false }
        );
    }
    fn ok(&mut self) -> Effect {
        self.step(Event::ActResult(RunnerResult::ok()))
    }
    fn look(&mut self, fresh: &Screen) -> Effect {
        let e = self.observed(fresh);
        if let Effect::Act(RunnerCommand::Wait { .. }) = e {
            let r = self.ok();
            assert!(matches!(r, Effect::Observe { .. }), "{r:?}");
            return self.observed(fresh);
        }
        e
    }
}

// ---------------------------------------------------------------------------------------
// 1. Every tap lands on the control it names.
// ---------------------------------------------------------------------------------------

fn overlap(mallory_y: f64) -> Screen {
    scr(vec![
        node(
            Role::Button,
            "Pay $40.00 to Electricity Co",
            rect(16.0, 300.0, 361.0, 50.0),
        )
        .with_identifier("pay-electricity"),
        node(
            Role::Button,
            "Pay $4,000.00 to Mallory Ltd",
            rect(16.0, mallory_y, 361.0, 50.0),
        )
        .with_identifier("pay-mallory"),
    ])
}

/// Probe: frames are not hashed, so a shown button that moves over the approved one leaves
/// digest, compact content, source and summary equal. The tap point, derived again on the
/// fresh look, is covered: nothing is sent.
#[test]
fn an_overlap_appearing_on_the_fresh_look_sends_nothing() {
    let asked = overlap(600.0);
    let fresh = overlap(302.0);
    assert_eq!(screen_digest(&asked), screen_digest(&fresh), "frames only");
    let mut d = D::new();
    let summary = d.ask(&asked, r#"{"action":"tap","element":1}"#);
    d.approve();
    let e = d.look(&fresh);
    assert!(
        !matches!(e, Effect::Act(RunnerCommand::Tap { .. })),
        "approved '{summary}', tapped while covered: {e:?}"
    );
    assert!(matches!(e, Effect::Infer(_)), "{e:?}");
}

/// Probe (adapted: the specification lets the session refuse instead of asking, and this
/// implementation refuses): the same overlap already present when the action is proposed.
/// Nothing is asked or sent; the model is told which control covers which.
#[test]
fn an_overlap_at_planning_time_is_refused_naming_both() {
    let s = overlap(302.0);
    let mut d = D::new();
    let e = d.start(&s, r#"{"action":"tap","element":1}"#);
    let Effect::Infer(req) = e else {
        panic!("refused and decided again, got {e:?}")
    };
    assert!(
        req.user.contains("not done: element 1 (Pay $40.00 to Electricity Co) is covered by element 2 (Pay $4,000.00 to Mallory Ltd)"),
        "{}",
        req.user
    );
    // Either order is a conflict.
    let mut d = D::new();
    let e = d.start(&s, r#"{"action":"tap","element":2}"#);
    assert!(matches!(e, Effect::Infer(_)), "{e:?}");
}

/// A partial overlap over the centre: another point inside the target is used, and it is
/// inside no other control.
#[test]
fn a_partial_overlap_taps_another_point_of_the_target() {
    let s = scr(vec![
        node(Role::Button, "Continue", rect(16.0, 300.0, 361.0, 50.0)),
        node(Role::Button, "Info", rect(150.0, 290.0, 100.0, 40.0)),
    ]);
    let tap = format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&s, "Continue")
    );
    let run = run(
        config(),
        "Continue",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[&tap, r#"{"action":"fail"}"#]),
    );
    let tap = run.acts.first().unwrap();
    assert!(taps(tap, &s, "Continue"), "{tap:?}");
    assert!(!taps(tap, &s, "Info"), "{tap:?}");
    assert_ne!(
        point_of(tap),
        Some(Point::new(196.5, 325.0)),
        "not the centre"
    );
}

/// Nested pressables: a button inside a cell (compaction lists the button, not the row),
/// a switch inside a full-width cell: the control is tapped where it is.
#[test]
fn nested_controls_are_tapped_normally() {
    let s = scr(vec![
        UiNode::new(Role::Cell, rect(0.0, 200.0, 393.0, 52.0)).with_children(vec![
            text("Wi-Fi", rect(16.0, 210.0, 100.0, 30.0)),
            UiNode::new(Role::Switch, rect(320.0, 210.0, 51.0, 31.0))
                .with_value("0")
                .hittable(true),
        ]),
        UiNode::new(Role::Cell, rect(0.0, 260.0, 393.0, 52.0)).with_children(vec![node(
            Role::Button,
            "Details",
            rect(300.0, 270.0, 80.0, 30.0),
        )]),
    ]);
    let c = compact(&s);
    let switch = c.elements.iter().find(|e| e.role == Role::Switch).unwrap();
    let details = index_of(&s, "Details");
    let run = run(
        config(),
        "Wi-Fi",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{}}}"#, switch.index),
            &format!(r#"{{"action":"tap","element":{details}}}"#),
            r#"{"action":"fail"}"#,
        ]),
    );
    assert_eq!(
        point_of(&run.acts[0]),
        Some(switch.frame.center()),
        "the switch itself"
    );
    assert!(taps(&run.acts[1], &s, "Details"));
}

/// A list scrolled under the navigation bar: `Item 1` lies under the back button, `Item 0`
/// wholly under the bar.
fn scrolled_under_bar() -> Screen {
    scr(vec![
        node(Role::Button, "Item 1", rect(0.0, 40.0, 393.0, 80.0)),
        node(Role::Button, "Item 0", rect(100.0, 62.0, 200.0, 38.0)),
        UiNode::new(Role::NavigationBar, rect(0.0, 59.0, 393.0, 44.0))
            .with_identifier("Inbox")
            .with_children(vec![node(
                Role::Button,
                "Back",
                rect(16.0, 59.0, 44.0, 44.0),
            )
            .with_identifier("BackButton")]),
    ])
}

/// Bar precedence on a typed tree: the back button is tapped at its centre although a row
/// has scrolled under it; the row is tapped outside the bar; a row wholly under the bar is
/// refused, naming the bar.
#[test]
fn a_typed_tree_bar_control_is_tapped_over_rows_under_it() {
    let s = scrolled_under_bar();
    let c = compact(&s);
    let back = c.elements.iter().find(|e| e.label == "Back").unwrap();
    let back_tap = format!(r#"{{"action":"tap","element":{}}}"#, back.index);
    let row_tap = format!(r#"{{"action":"tap","element":{}}}"#, index_of(&s, "Item 1"));
    let run = run(
        config(),
        "Back",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[&back_tap, &row_tap, r#"{"action":"fail"}"#]),
    );
    assert_eq!(point_of(&run.acts[0]), Some(back.frame.center()));
    let p = point_of(&run.acts[1]).unwrap();
    assert!(taps(&run.acts[1], &s, "Item 1"), "{p:?}");
    assert!(!(59.0..103.0).contains(&p.y), "outside the bar: {p:?}");

    let mut d = D::new();
    let e = d.start(
        &s,
        &format!(r#"{{"action":"tap","element":{}}}"#, index_of(&s, "Item 0")),
    );
    let Effect::Infer(req) = e else {
        panic!("refused and decided again, got {e:?}")
    };
    assert!(
        req.user
            .contains("not done: element 3 (Item 0) is covered by the navigation bar"),
        "{}",
        req.user
    );
}

/// The same screen read from runner text (roles forgeable): no bar precedence, the back
/// button is contested by the row under it as before, and nothing is tapped.
#[test]
fn a_runner_text_bar_takes_no_precedence() {
    let s = scrolled_under_bar();
    let c = compact(&s);
    let back = c.elements.iter().find(|e| e.label == "Back").unwrap();
    let mut d = D::new();
    d.step(Event::Start);
    let e = d.step(Event::ObservedRunnerText(Observation::new(s.clone())));
    assert!(matches!(e, Effect::Infer(_)), "{e:?}");
    let mut e = d.model(&format!(r#"{{"action":"tap","element":{}}}"#, back.index));
    if let Effect::Confirm(_) = e {
        d.approve();
        e = d.step(Event::ObservedRunnerText(Observation::new(s.clone())));
    }
    assert!(
        !matches!(e, Effect::Act(RunnerCommand::Tap { .. })),
        "tapped a contested point on runner text: {e:?}"
    );
    let Effect::Infer(req) = e else {
        panic!("refused and decided again, got {e:?}")
    };
    assert!(
        req.user.contains(&format!(
            "not done: element {} (Back) is covered by element",
            back.index
        )),
        "{}",
        req.user
    );
}

// ---------------------------------------------------------------------------------------
// 2. Return after typing is its own gated action.
// ---------------------------------------------------------------------------------------

fn chat(to: &str, value: Option<&str>) -> Screen {
    let mut field = UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 44.0))
        .with_label("Message")
        .with_identifier("messageBodyField")
        .hittable(true)
        .focused(true);
    if let Some(v) = value {
        field = field.with_value(v);
    }
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Chat")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![
                text(to, rect(16.0, 80.0, 300.0, 40.0)),
                text("Gift $40.00", rect(16.0, 130.0, 300.0, 30.0)),
                field,
                jarvis_proto::fixtures::keyboard("send"),
            ])]);
    screen("com.example.chat", "Chat", root, true)
}

fn sent() -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Chat")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![
                text("To: alice@example.com", rect(16.0, 80.0, 300.0, 40.0)),
                text("Delivered", rect(16.0, 300.0, 300.0, 30.0)),
            ])]);
    screen("com.example.chat", "Chat", root, false)
}

/// The chat phone: typing shows the text in the field, Return sends.
fn chat_phone() -> Phone {
    Phone::new(
        chat("To: alice@example.com", None),
        |cmd, s: &Screen| match cmd {
            RunnerCommand::TypeText { text } => {
                let to = compact(s).elements[0].label.clone();
                Transition::Go(chat(&to, Some(text)))
            }
            RunnerCommand::PressKey { key: Key::Return } => Transition::Go(sent()),
            _ => Transition::Stay,
        },
    )
}

const SEND: &str = r#"{"action":"type","element":3,"text":"On my way","submit":true}"#;

/// Probe: after the typing, the session only checked that the text arrived and pressed
/// Return on a screen whose recipient had changed. Now the Return is gated on that screen:
/// asked, not pressed.
#[test]
fn return_after_typing_is_gated_on_the_post_typing_screen() {
    let asked = chat("To: Alice", None);
    let mut d = D::new();
    let summary = d.ask(
        &asked,
        r#"{"action":"type","element":3,"text":"I owe you the code 4242","submit":true}"#,
    );
    // Typing a code is a credential: the typing itself is asked first.
    assert!(summary.starts_with("Type"), "{summary}");
    d.approve();
    let e = d.look(&asked);
    assert!(
        matches!(e, Effect::Act(RunnerCommand::TypeText { .. })),
        "{e:?}"
    );
    let e = d.ok();
    assert!(matches!(e, Effect::Observe { .. }), "{e:?}");
    let changed = chat(
        "To: Mallory (unknown number)",
        Some("I owe you the code 4242"),
    );
    let e = d.observed(&changed);
    assert!(
        !matches!(e, Effect::Act(RunnerCommand::PressKey { .. })),
        "Return pressed with 'To: Mallory' on screen: {e:?}"
    );
    let Effect::Confirm(req) = e else {
        panic!("{e:?}")
    };
    // Gated on this screen: the typed code is a secret and is never quoted.
    assert!(
        req.summary
            .starts_with("Press Return (\"send\") in \"Message\" after typing a secret"),
        "{}",
        req.summary
    );
}

/// Typing harmless, Return risky: one question, asked after the typing, naming the typed
/// text, the field and the screen's recipient and amount. The effect sequence of a risky
/// submit.
#[test]
fn a_risky_send_asks_once_after_typing() {
    let run = run(
        config(),
        "Tell Alice I'm on my way",
        MemoryStore::new(),
        chat_phone(),
        Script::model(&[SEND, r#"{"action":"done","summary":"sent"}"#]).decisions(&[true]),
    );
    assert_eq!(
        run.outcome.status,
        OutcomeStatus::Success,
        "{:?}",
        run.outcome
    );
    assert_eq!(run.confirms.len(), 1, "{:?}", run.confirms);
    let c = &run.confirms[0];
    // "send" on a screen with an amount reads as a payment.
    assert_eq!(c.category, RiskCategory::Payment);
    assert_eq!(
        c.summary,
        "Press Return (\"send\") in \"Message\" after typing \"On my way\" in Chat. Amounts on screen: Gift $40.00. To/Account: To: alice@example.com"
    );
    let kinds: Vec<&str> = run
        .effects
        .iter()
        .map(|e| match e {
            Effect::Observe { .. } => "observe",
            Effect::Infer(_) => "infer",
            Effect::Act(RunnerCommand::TypeText { .. }) => "type",
            Effect::Act(RunnerCommand::PressKey { key: Key::Return }) => "return",
            Effect::Act(_) => "act",
            Effect::Confirm(_) => "confirm",
            Effect::Finished(_) => "finished",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "observe", "infer", "type", "observe", "confirm", "observe", "return", "observe",
            "infer", "finished"
        ]
    );
    // One model step, one history line.
    assert_eq!(run.outcome.steps, 2);
    let last = run.last_user();
    assert_eq!(last.matches("type [3]").count(), 1, "{last}");
    assert!(last.contains("-> ok"), "{last}");
}

/// The screen changes between the Return question and the look: Return is not pressed.
#[test]
fn a_screen_change_between_the_return_question_and_the_look_voids_it() {
    let mut d = D::new();
    let e = d.start(&chat("To: alice@example.com", None), SEND);
    assert!(
        matches!(e, Effect::Act(RunnerCommand::TypeText { .. })),
        "{e:?}"
    );
    assert!(matches!(d.ok(), Effect::Observe { .. }));
    let typed = chat("To: alice@example.com", Some("On my way"));
    assert!(matches!(d.observed(&typed), Effect::Confirm(_)));
    d.approve();
    let e = d.look(&chat("To: mallory@example.com", Some("On my way")));
    assert!(
        !matches!(e, Effect::Act(RunnerCommand::PressKey { .. })),
        "{e:?}"
    );
    let Effect::Infer(req) = e else {
        panic!("{e:?}")
    };
    assert!(
        req.user.contains("not done: the screen changed"),
        "{}",
        req.user
    );
    // The same screen: pressed.
    let mut d = D::new();
    d.start(&chat("To: alice@example.com", None), SEND);
    d.ok();
    assert!(matches!(d.observed(&typed), Effect::Confirm(_)));
    d.approve();
    assert_eq!(
        d.look(&typed),
        Effect::Act(RunnerCommand::PressKey { key: Key::Return })
    );
}

/// A decline leaves the text typed, sends nothing and tells the model "typed, not
/// submitted".
#[test]
fn a_declined_return_leaves_the_text_and_tells_the_model() {
    let run = run(
        config(),
        "Tell Alice I'm on my way",
        MemoryStore::new(),
        chat_phone(),
        Script::model(&[SEND, r#"{"action":"fail","reason":"declined"}"#]).decisions(&[false]),
    );
    assert_eq!(run.confirms.len(), 1);
    assert!(run
        .acts
        .iter()
        .all(|a| !matches!(a, RunnerCommand::PressKey { .. })));
    assert!(run
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::TypeText { .. })));
    let field = compact(&run.screen)
        .elements
        .into_iter()
        .find(|e| e.label == "Message")
        .unwrap();
    assert_eq!(field.value.as_deref(), Some("On my way"));
    assert!(
        run.last_user().contains("not done: typed, not submitted"),
        "{}",
        run.last_user()
    );
}

/// A policy that does not ask for messages: Return is pressed at once after the typing.
#[test]
fn with_a_no_ask_policy_return_is_pressed_at_once() {
    let mut c = config();
    c.policy
        .confirm
        .retain(|k| !matches!(k, RiskCategory::SendMessage | RiskCategory::Payment));
    let run = run(
        c,
        "Tell Alice I'm on my way",
        MemoryStore::new(),
        chat_phone(),
        Script::model(&[SEND, r#"{"action":"done","summary":"sent"}"#]),
    );
    assert!(run.confirms.is_empty(), "{:?}", run.confirms);
    assert_eq!(
        run.acts,
        [
            RunnerCommand::TypeText {
                text: "On my way".to_owned()
            },
            RunnerCommand::PressKey { key: Key::Return }
        ]
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Success);
}

/// A learned flow with a submit replays in the same two parts: no model call, the Return
/// asked after the typing.
#[test]
fn a_learned_submit_replays_in_two_gated_parts() {
    let first = run(
        config(),
        "Tell Alice I'm on my way",
        MemoryStore::new(),
        chat_phone(),
        Script::model(&[SEND, r#"{"action":"done","summary":"sent"}"#]).decisions(&[true]),
    );
    assert_eq!(first.outcome.status, OutcomeStatus::Success);
    let replay = run(
        config(),
        "Tell Alice I'm on my way",
        first.memory,
        chat_phone(),
        Script::model(&[]).decisions(&[true]),
    );
    assert_eq!(
        replay.outcome.status,
        OutcomeStatus::Success,
        "{:?}",
        replay.outcome
    );
    assert_eq!(replay.infers(), 0);
    assert_eq!(replay.confirms.len(), 1);
    assert!(replay.confirms[0].summary.starts_with("Press Return"));
    let ty = replay
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Act(RunnerCommand::TypeText { .. })))
        .unwrap();
    let ask = replay
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Confirm(_)))
        .unwrap();
    assert!(ty < ask);
}

/// A lock while the Return question is open: asked again on the same screen after the
/// unlock (an approval is never carried across); a changed screen gets no Return and the
/// model is told. A cancel finishes with nothing pressed.
#[test]
fn a_lock_or_cancel_between_typing_and_the_return_question() {
    let typed = chat("To: alice@example.com", Some("On my way"));
    let to_question = |d: &mut D| {
        d.start(&chat("To: alice@example.com", None), SEND);
        d.ok();
        assert!(matches!(d.observed(&typed), Effect::Confirm(_)));
    };
    // Same screen after the unlock: asked again, then pressed.
    let mut d = D::new();
    to_question(&mut d);
    assert!(matches!(
        d.step(Event::DeviceLocked),
        Effect::AwaitUnlock { .. }
    ));
    assert!(matches!(
        d.step(Event::DeviceUnlocked),
        Effect::Observe { .. }
    ));
    assert!(matches!(d.observed(&typed), Effect::Confirm(_)));
    d.approve();
    assert_eq!(
        d.look(&typed),
        Effect::Act(RunnerCommand::PressKey { key: Key::Return })
    );
    // Changed screen after the unlock: no Return, the model is told.
    let mut d = D::new();
    to_question(&mut d);
    d.step(Event::DeviceLocked);
    d.step(Event::DeviceUnlocked);
    let e = d.observed(&chat("To: mallory@example.com", Some("On my way")));
    let Effect::Infer(req) = e else {
        panic!("{e:?}")
    };
    assert!(
        req.user.contains("the phone locked before Return"),
        "{}",
        req.user
    );
    // Cancel.
    let mut d = D::new();
    to_question(&mut d);
    assert!(matches!(
        d.step(Event::Cancel),
        Effect::Finished(o) if o.status == OutcomeStatus::Cancelled
    ));
}

// ---------------------------------------------------------------------------------------
// 3. No keyboard exclusion in the digest.
// ---------------------------------------------------------------------------------------

fn kb_window(accessory_text: &str) -> UiNode {
    let kb = UiNode::new(Role::Keyboard, rect(0.0, 560.0, 393.0, 292.0)).with_children(vec![
        node(Role::Key, "q", rect(3.0, 570.0, 37.0, 46.0)),
        node(Role::Button, "send", rect(297.0, 790.0, 93.0, 46.0)).with_identifier("Return"),
    ]);
    UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
        Role::Other("Other".into()),
        rect(0.0, 470.0, 393.0, 382.0),
    )
    .with_children(vec![
        // The app's input accessory bar: 46 pt tall, right above the 44 pt prediction bar.
        UiNode::new(Role::Other("Other".into()), rect(0.0, 470.0, 393.0, 46.0)).with_children(
            vec![
                text(accessory_text, rect(16.0, 478.0, 280.0, 30.0)),
                node(Role::Button, "Send", rect(310.0, 474.0, 70.0, 38.0)),
            ],
        ),
        node(Role::Button, "Passwords", rect(30.0, 516.0, 330.0, 44.0)),
        kb,
    ])])
}

fn transfer(accessory_text: &str, note: &str) -> Screen {
    let app = UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![
        text("Send money", rect(16.0, 80.0, 300.0, 40.0)),
        UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 44.0))
            .with_label("Note")
            .with_value(note)
            .hittable(true)
            .focused(true),
    ]);
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Bank")
        .with_children(vec![app, kb_window(accessory_text)]);
    screen(BANK, "Bank", root, true)
}

/// Probe: an app's input accessory bar in the keyboard's window was left out of both the
/// digest and the compact screen. Now it is hashed and listed (its Send button can be
/// tapped, its amount read); the keys are not listed, the Return key is still a cue.
#[test]
fn an_accessory_bar_is_hashed_and_listed() {
    let a = transfer("Send $40.00 to Alice", "rent");
    let b = transfer("Send $4,000.00 to Mallory", "rent");
    let ca = compact(&a);
    let labels: Vec<&str> = ca.elements.iter().map(|e| e.label.as_str()).collect();
    assert!(labels.contains(&"Send $40.00 to Alice"), "{labels:?}");
    assert!(labels.contains(&"Send"), "{labels:?}");
    assert!(labels.contains(&"Passwords"), "{labels:?}");
    assert!(!labels.contains(&"q"), "{labels:?}");
    assert_eq!(ca.cues.return_key.as_deref(), Some("send"));
    assert_ne!(screen_digest(&a), screen_digest(&b));
}

/// Probe (adapted: the Return is now asked after the typing, on the post-typing screen):
/// the accessory's amount changing between that question and the look voids it.
#[test]
fn an_accessory_amount_change_voids_the_return() {
    let mut d = D::new();
    let e = d.start(
        &transfer("Send $40.00 to Alice", "rent"),
        r#"{"action":"type","element":2,"text":" thanks","submit":true}"#,
    );
    assert!(
        matches!(e, Effect::Act(RunnerCommand::TypeText { .. })),
        "{e:?}"
    );
    assert!(matches!(d.ok(), Effect::Observe { .. }));
    let asked = transfer("Send $40.00 to Alice", "rent thanks");
    let Effect::Confirm(req) = d.observed(&asked) else {
        panic!("the Return is asked")
    };
    assert!(req.summary.contains("$40.00"), "{}", req.summary);
    d.approve();
    let e = d.look(&transfer("Send $4,000.00 to Mallory", "rent thanks"));
    assert!(!matches!(e, Effect::Act(_)), "{e:?}");
}

/// Probe: summaries are deterministic (the same screen, the same summary, every time).
#[test]
fn summaries_are_deterministic() {
    let s = overlap(600.0);
    let mut first: Option<String> = None;
    for _ in 0..1000 {
        let mut d = D::new();
        let summary = d.ask(&s, r#"{"action":"tap","element":1}"#);
        d.approve();
        let e = d.look(&s);
        assert!(matches!(e, Effect::Act(RunnerCommand::Tap { .. })), "{e:?}");
        match &first {
            None => first = Some(summary),
            Some(f) => assert_eq!(f, &summary),
        }
    }
}

// ---------------------------------------------------------------------------------------
// 4. Runner-text screens always confirm.
// ---------------------------------------------------------------------------------------

/// A runner-text screen in an app the owner marked Trusted asks; the typed tree does not.
#[test]
fn a_runner_text_screen_in_a_trusted_app_asks() {
    let mut c = config();
    c.policy
        .app_rules
        .push(AppRule::new(BANK, TrustLevel::Trusted));
    let s = scr(vec![
        text("Profile", rect(16.0, 80.0, 300.0, 40.0)),
        node(Role::Button, "Next", rect(16.0, 300.0, 361.0, 50.0)),
    ]);
    let next = format!(r#"{{"action":"tap","element":{}}}"#, index_of(&s, "Next"));
    for runner_text in [true, false] {
        let mut d = D::with(c.clone());
        d.step(Event::Start);
        let obs = Observation::new(s.clone());
        let e = if runner_text {
            d.step(Event::ObservedRunnerText(obs))
        } else {
            d.step(Event::Observed(obs))
        };
        assert!(matches!(e, Effect::Infer(_)));
        let e = d.model(&next);
        assert_eq!(matches!(e, Effect::Confirm(_)), runner_text, "{e:?}");
    }
}

// ---------------------------------------------------------------------------------------
// 5. Summaries name fields; they show values only when safe.
// ---------------------------------------------------------------------------------------

fn compose(value: Option<&str>) -> Screen {
    let mut field = UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 44.0))
        .with_label("Message")
        .hittable(true);
    if let Some(v) = value {
        field = field.with_value(v);
    }
    scr(vec![
        text("To: Alice", rect(16.0, 80.0, 300.0, 40.0)),
        field,
        node(Role::Button, "Send", rect(16.0, 300.0, 361.0, 50.0)),
    ])
}

/// A message Jarvis typed is shown before Send; one already in the field is "(filled)".
#[test]
fn a_typed_message_is_shown_before_send_and_a_prefilled_one_is_not() {
    let phone = || {
        Phone::new(compose(None), |cmd, _s: &Screen| match cmd {
            RunnerCommand::TypeText { text } => Transition::Go(compose(Some(text))),
            _ => Transition::Stay,
        })
    };
    let typed = run(
        config(),
        "Tell Alice",
        MemoryStore::new(),
        phone(),
        Script::model(&[
            r#"{"action":"type","element":2,"text":"On my way"}"#,
            r#"{"action":"tap","element":3}"#,
            r#"{"action":"fail"}"#,
        ]),
    );
    assert_eq!(typed.confirms.len(), 1);
    assert!(
        typed.confirms[0].summary.contains("Message: On my way"),
        "{}",
        typed.confirms[0].summary
    );
    let prefilled = run(
        config(),
        "Tell Alice",
        MemoryStore::new(),
        Phone::still(compose(Some("Wire the money to Mallory"))),
        Script::model(&[r#"{"action":"tap","element":3}"#, r#"{"action":"fail"}"#]),
    );
    let sum = &prefilled.confirms[0].summary;
    assert!(sum.contains("Message (filled)"), "{sum}");
    assert!(!sum.contains("Mallory"), "{sum}");
}

/// Nothing from the probe's list appears in a summary, a decline's text or the outcome.
#[test]
fn protected_values_reach_no_summary_decline_or_outcome() {
    let fields = [
        ("CSC", "123"),
        ("Security number", "4417"),
        ("Memorable word", "zebra42"),
        ("Answer", "Fluffy"),
        ("Mother's maiden name", "Smithers"),
        ("Date of birth", "04/07/1981"),
        ("Tax ID", "12-3456789"),
        ("Driver license", "D1234567"),
        ("Passport number", "X12345678"),
        ("Field 2", "482913"),
    ];
    let mut children = Vec::new();
    for (k, (name, value)) in fields.iter().enumerate() {
        children.push(
            UiNode::new(
                Role::TextField,
                rect(16.0, 60.0 + 40.0 * k as f64, 361.0, 36.0),
            )
            .with_label(*name)
            .with_value(*value)
            .hittable(true),
        );
    }
    children.push(text("Fee $25.00", rect(16.0, 500.0, 361.0, 30.0)));
    children.push(node(
        Role::Button,
        "Pay now",
        rect(16.0, 700.0, 361.0, 50.0),
    ));
    let s = scr(children);
    let pay = format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&s, "Pay now")
    );
    let r = run(
        config(),
        "Pay",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[&pay, r#"{"action":"done","summary":"paid"}"#]),
    );
    assert_eq!(r.confirms.len(), 1);
    let texts = format!(
        "{}\n{}\n{}",
        r.confirms[0].summary,
        r.outcome.summary,
        r.all_prompt_text()
    );
    for (_, value) in fields.iter().filter(|(n, _)| *n != "Field 2") {
        assert!(!texts.contains(value), "{value}: {texts}");
    }
    // A neutral field's value may reach the prompt (the model sees the screen), never the
    // summary or the outcome.
    let owner = format!("{}\n{}", r.confirms[0].summary, r.outcome.summary);
    assert!(!owner.contains("482913"), "{owner}");
    assert_eq!(r.outcome.status, OutcomeStatus::Blocked);
}

// ---------------------------------------------------------------------------------------
// 6. `type` with no element needs a listed focused field.
// ---------------------------------------------------------------------------------------

#[test]
fn type_without_an_element_needs_a_listed_focused_field() {
    let unfocused = compose(None);
    let r = run(
        config(),
        "Write",
        MemoryStore::new(),
        Phone::still(unfocused),
        Script::model(&[
            r#"{"action":"type","text":"hello"}"#,
            r#"{"action":"fail"}"#,
        ]),
    );
    assert!(r.acts.is_empty(), "{:?}", r.acts);
    assert!(
        r.last_user()
            .contains("failed: no field is focused; tap a field first"),
        "{}",
        r.last_user()
    );
    let focused = chat("To: alice@example.com", None);
    let r = run(
        config(),
        "Write",
        MemoryStore::new(),
        Phone::still(focused),
        Script::model(&[
            r#"{"action":"type","text":"hello"}"#,
            r#"{"action":"fail"}"#,
        ]),
    );
    assert_eq!(
        r.acts.first(),
        Some(&RunnerCommand::TypeText {
            text: "hello".to_owned()
        })
    );
}

#[allow(dead_code)]
fn unused(_: Policy) {}
