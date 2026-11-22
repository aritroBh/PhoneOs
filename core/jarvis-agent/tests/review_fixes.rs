//! Regressions from a safety review of the decision core: approvals are acted on only after
//! a fresh look at the screen, a held-back Return is not pressed on a screen that changed,
//! learned flows and skills cannot carry a drag past the gate, and `Debug` output never
//! shows sensitive values or typed text.

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
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, Key, ModelOutput, Observation, OutcomeStatus, RiskCategory, Role, RunnerCommand,
    RunnerResult, Screen, SkillContext, TaskSpec, TimedEvent, UiNode,
};

const DONE: &str = r#"{"action":"done","summary":"done"}"#;

fn app_root(label: &str, children: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(label)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ])
}

/// A product page with "Buy now" at `buy_at` and another button lower down.
fn shop(buy_at: f64, other: &str) -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        app_root(
            "Shop",
            vec![
                text("Headphones $199.00", rect(16.0, 120.0, 361.0, 30.0)),
                node(Role::Button, "Buy now", rect(16.0, buy_at, 361.0, 50.0)),
                node(Role::Button, other, rect(16.0, 700.0, 361.0, 50.0)),
            ],
        ),
        false,
    )
}

/// A manually driven session with a clock.
struct Driver {
    s: Session,
    now: u64,
}

impl Driver {
    fn new(config: jarvis_proto::SessionConfig, task: &str) -> Self {
        Driver {
            s: Session::new(config, TaskSpec::new("t", task), MemoryStore::new()),
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

    fn model(&mut self, text: &str) -> Effect {
        self.step(Event::ModelOutput(ModelOutput {
            text: text.to_owned(),
        }))
    }
}

fn tap_buy(screen: &Screen) -> String {
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(screen, "Buy now")
    )
}

/// Rule 2: an approval can arrive long after the request; the session must look at the
/// screen again before it acts on the approved control (the coordinates it planned were
/// taken from the screen it asked about).
#[test]
fn an_approval_looks_at_the_screen_again_before_acting() {
    let mut d = Driver::new(config(), "Buy the headphones");
    assert!(matches!(d.step(Event::Start), Effect::Observe { .. }));
    let before = shop(300.0, "Add to wishlist");
    assert!(matches!(d.observed(&before), Effect::Infer(_)));
    let e = d.model(&tap_buy(&before));
    assert!(matches!(e, Effect::Confirm(_)), "{e:?}");
    // Meanwhile the app re-laid out: "Buy now" moved; where it was is now "Subscribe".
    let e = d.step(Event::UserDecision { approved: true });
    match e {
        Effect::Act(cmd) => panic!(
            "approved action sent without re-observing; it taps {:?} on whatever is there now",
            cmd
        ),
        other => assert!(matches!(other, Effect::Observe { .. }), "{other:?}"),
    }
    // The same screen: the approved tap runs.
    assert_eq!(
        d.observed(&before),
        Effect::Act(RunnerCommand::Tap { x: 196.5, y: 325.0 })
    );
}

/// The control is still there but shifted a little (its frame still holds the planned
/// point): it is tapped where it is now, at the re-planned centre.
#[test]
fn an_approved_control_that_shifted_is_tapped_where_it_is_now() {
    let mut d = Driver::new(config(), "Buy the headphones");
    d.step(Event::Start);
    let before = shop(300.0, "Add to wishlist");
    d.observed(&before);
    assert!(matches!(d.model(&tap_buy(&before)), Effect::Confirm(_)));
    assert!(matches!(
        d.step(Event::UserDecision { approved: true }),
        Effect::Observe { .. }
    ));
    let shifted = shop(310.0, "Add to wishlist");
    assert_eq!(
        d.observed(&shifted),
        Effect::Act(RunnerCommand::Tap { x: 196.5, y: 335.0 })
    );
}

/// The approved control moved away (another control is under the planned point): nothing
/// is sent, the model is told, and proposing it again asks again.
#[test]
fn an_approved_control_that_moved_away_is_not_tapped_and_is_asked_again() {
    let mut d = Driver::new(config(), "Buy the headphones");
    d.step(Event::Start);
    let before = shop(300.0, "Add to wishlist");
    d.observed(&before);
    let Effect::Confirm(asked) = d.model(&tap_buy(&before)) else {
        panic!()
    };
    d.step(Event::UserDecision { approved: true });
    // "Buy now" moved down; "Subscribe" is where it was (a different control).
    let moved = screen(
        "com.example.shop",
        "Shop",
        app_root(
            "Shop",
            vec![
                text("Headphones $199.00", rect(16.0, 120.0, 361.0, 30.0)),
                node(Role::Button, "Subscribe", rect(16.0, 300.0, 361.0, 50.0)),
                node(Role::Button, "Buy now", rect(16.0, 500.0, 361.0, 50.0)),
            ],
        ),
        false,
    );
    let Effect::Infer(r) = d.observed(&moved) else {
        panic!("the approved tap must not run on a changed screen")
    };
    assert!(
        r.user
            .contains("the screen changed after the owner approved it"),
        "{}",
        r.user
    );
    // Through the full gate again: asked again, then (approved, same screen) done.
    let Effect::Confirm(again) = d.model(&tap_buy(&moved)) else {
        panic!()
    };
    assert_eq!(again.category, asked.category);
    assert!(matches!(
        d.step(Event::UserDecision { approved: true }),
        Effect::Observe { .. }
    ));
    assert_eq!(
        d.observed(&moved),
        Effect::Act(RunnerCommand::Tap { x: 196.5, y: 525.0 })
    );
}

/// The approved control is gone, or the app changed: nothing is sent.
#[test]
fn an_approved_control_that_is_gone_or_in_another_app_is_not_tapped() {
    for after in [home_screen(), shop(300.0, "Add to wishlist")] {
        let mut d = Driver::new(config(), "Buy the headphones");
        d.step(Event::Start);
        let before = shop(300.0, "Add to wishlist");
        d.observed(&before);
        d.model(&tap_buy(&before));
        d.step(Event::UserDecision { approved: true });
        let mut after = after;
        if after.app_bundle_id == "com.example.shop" {
            // Same layout, but the button now reads differently: not the approved control.
            after.root.children[0].children[1].label = Some("Buy now for $999".to_owned());
        }
        let e = d.observed(&after);
        assert!(matches!(e, Effect::Infer(_)), "{e:?}");
    }
}

/// The approval after a first-use question also looks again, and a screen that turned
/// risky meanwhile is not acted on with only the first-use approval.
#[test]
fn a_first_use_approval_also_looks_again_before_acting() {
    let mut d = Driver::new(first_use_config(), "Add to wishlist");
    d.step(Event::Start);
    let before = shop(300.0, "Add to wishlist");
    d.observed(&before);
    let i = index_of(&before, "Add to wishlist");
    let Effect::Confirm(fu) = d.model(&format!(r#"{{"action":"tap","element":{i}}}"#)) else {
        panic!()
    };
    assert_eq!(fu.category, RiskCategory::FirstUse);
    assert!(matches!(
        d.step(Event::UserDecision { approved: true }),
        Effect::Observe { .. }
    ));
    assert!(matches!(d.observed(&before), Effect::Act(_)));
}

/// An approved `type … submit` is re-checked too: when the field has gone, nothing is typed.
#[test]
fn an_approved_typed_submit_is_rechecked_before_typing() {
    let chat = |focused: bool| {
        screen(
            "com.example.chat",
            "Chat",
            app_root(
                "Chat",
                vec![
                    text("Alex", rect(150.0, 70.0, 93.0, 40.0)),
                    UiNode::new(Role::TextView, rect(16.0, 460.0, 300.0, 40.0))
                        .with_label("Message")
                        .hittable(true)
                        .focused(focused),
                    node(Role::Button, "Send", rect(320.0, 460.0, 60.0, 40.0)),
                ],
            ),
            true,
        )
    };
    let mut d = Driver::new(config(), "Tell Alex I'm late");
    d.step(Event::Start);
    d.observed(&chat(true));
    let Effect::Confirm(c) = d.model(r#"{"action":"type","text":"late","submit":true}"#) else {
        panic!()
    };
    assert_eq!(c.category, RiskCategory::SendMessage);
    assert!(matches!(
        d.step(Event::UserDecision { approved: true }),
        Effect::Observe { .. }
    ));
    // The conversation was closed meanwhile: no field to type into.
    let e = d.observed(&home_screen());
    assert!(matches!(e, Effect::Infer(_)), "{e:?}");
}

/// A screen that keeps changing under every approval cannot keep the session asking: each
/// stale approval counts as a failed step, and the failure budget ends the session.
///
/// Changed in the third review: the two screens used to differ only in where "Buy now"
/// sat; frames are no longer part of what was approved (a list that scrolled keeps the
/// approval, see `an_approval_survives_a_scroll_and_is_tapped_where_it_is_now`), so they
/// now differ in content too.
#[test]
fn approvals_on_a_screen_that_keeps_changing_end_by_the_budget() {
    let a = shop(300.0, "Add to wishlist");
    let b = shop(560.0, "Save for later");
    let mut d = Driver::new(config(), "Buy the headphones");
    let mut e = d.step(Event::Start);
    let mut flip = false;
    let mut current = a.clone();
    let mut acts = 0;
    let mut confirms = 0;
    for _ in 0..200 {
        e = match e {
            Effect::Observe { .. } => {
                current = if flip { b.clone() } else { a.clone() };
                flip = !flip;
                d.observed(&current)
            }
            Effect::Infer(_) => d.model(&tap_buy(&current)),
            Effect::Confirm(_) => {
                confirms += 1;
                d.step(Event::UserDecision { approved: true })
            }
            Effect::Act(_) => {
                acts += 1;
                d.step(Event::ActResult(RunnerResult::ok()))
            }
            Effect::Finished(o) => {
                assert_eq!(o.status, OutcomeStatus::Failed, "{}", o.summary);
                assert_eq!(acts, 0, "nothing approved on another screen ran");
                assert!(confirms >= 2);
                return;
            }
            other => panic!("{other:?}"),
        };
    }
    panic!("the session did not end");
}

/// A held-back Return is not pressed on a screen that changed after typing (the check is
/// unverifiable there); the model is told it was not pressed.
#[test]
fn a_held_back_return_is_not_pressed_after_the_screen_changed() {
    // Typing into the search field jumps straight to a results page without the field.
    let results = screen(
        "com.example.shop",
        "Shop",
        app_root(
            "Shop",
            vec![node(
                Role::Cell,
                "Delete account",
                rect(0.0, 200.0, 393.0, 44.0),
            )],
        ),
        false,
    );
    let phone = Phone::new(
        search_screen(true, None),
        move |cmd, _s: &Screen| match cmd {
            RunnerCommand::TypeText { .. } => Transition::Go(results.clone()),
            _ => Transition::Stay,
        },
    );
    let run = run(
        config(),
        "Search for headphones",
        MemoryStore::new(),
        phone,
        Script::model(&[
            r#"{"action":"type","text":"headphones","submit":true}"#,
            r#"{"action":"fail","reason":"stop"}"#,
        ]),
    );
    assert!(
        run.acts
            .iter()
            .any(|c| matches!(c, RunnerCommand::TypeText { .. })),
        "{:?}",
        run.acts
    );
    assert!(
        !run.acts
            .iter()
            .any(|c| matches!(c, RunnerCommand::PressKey { key: Key::Return })),
        "Return pressed on a screen it was never gated on: {:?}",
        run.acts
    );
    assert!(
        run.requests[1].user.contains("Return was not pressed"),
        "{}",
        run.requests[1].user
    );
}

/// The same check with the field still there and showing the text: Return is pressed as
/// before.
#[test]
fn a_held_back_return_is_pressed_when_the_text_arrived() {
    let phone = Phone::new(search_screen(true, None), |cmd, _s: &Screen| match cmd {
        RunnerCommand::TypeText { text } => Transition::Go(search_screen(true, Some(text))),
        _ => Transition::Stay,
    });
    let run = run(
        config(),
        "Search for headphones",
        MemoryStore::new(),
        phone,
        Script::model(&[
            r#"{"action":"type","text":"headphones","submit":true}"#,
            DONE,
        ]),
    );
    assert!(run
        .acts
        .iter()
        .any(|c| matches!(c, RunnerCommand::PressKey { key: Key::Return })));
}

// ---------------------------------------------------------------------------------------
// Learned flows and skills cannot carry a drag past the gate.
// ---------------------------------------------------------------------------------------

fn checkout_screen() -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        app_root(
            "Shop",
            vec![
                text("Checkout", rect(16.0, 70.0, 200.0, 40.0)),
                text("Total $42.10", rect(16.0, 200.0, 361.0, 30.0)),
                text("Card ending 4242", rect(16.0, 240.0, 361.0, 30.0)),
                node(Role::Button, "Back", rect(8.0, 120.0, 60.0, 44.0)),
                UiNode::new(Role::Slider, rect(16.0, 395.0, 361.0, 44.0))
                    .with_label("Slide to pay")
                    .with_value("0")
                    .hittable(true),
            ],
        ),
        false,
    )
}

fn checkout_phone() -> Phone {
    Phone::new(checkout_screen(), |cmd, _s: &Screen| match cmd {
        RunnerCommand::Swipe { .. } => Transition::Go(screen(
            "com.example.shop",
            "Shop",
            app_root("Shop", vec![text("Paid", rect(16.0, 300.0, 361.0, 40.0))]),
            false,
        )),
        _ => Transition::Stay,
    })
}

/// A memory file (as the shell would load it) whose flow for the task is a drag of the pay
/// slider, phrased as a scroll on it, a swipe on it, or an element-less swipe across it.
fn poisoned_memory(step: serde_json::Value) -> MemoryStore {
    let task = "Pay for the order";
    let flow = serde_json::json!({
        "id": "poison",
        "app_bundle_id": "com.example.shop",
        "intent_key": jarvis_memory::intent_key(task),
        "steps": [step],
        "successes": 5,
        "failures": 0,
    });
    let mut store = serde_json::to_value(MemoryStore::new()).unwrap();
    store["flows"]["flows"] = serde_json::json!([flow]);
    let mem = MemoryStore::from_json(&store.to_string()).unwrap();
    assert_eq!(mem.flows.len(), 1);
    mem
}

#[test]
fn a_poisoned_flow_cannot_drag_the_pay_slider_unasked() {
    let steps = [
        serde_json::json!({"role": "slider", "label": "Slide to pay",
            "app_bundle_id": "com.example.shop",
            "action": {"type": "scroll", "direction": "left"}}),
        serde_json::json!({"role": "slider", "label": "Slide to pay",
            "app_bundle_id": "com.example.shop",
            "action": {"type": "swipe", "direction": "right"}}),
        serde_json::json!({"app_bundle_id": "com.example.shop",
            "action": {"type": "swipe", "direction": "right"}}),
        serde_json::json!({"app_bundle_id": "com.example.shop",
            "action": {"type": "scroll", "direction": "right"}}),
    ];
    for step in steps {
        let memory = poisoned_memory(step.clone());
        let run = run(
            config(),
            "Pay for the order",
            memory,
            checkout_phone(),
            Script::model(&[r#"{"action":"fail","reason":"no"}"#]),
        );
        assert!(run.acts.is_empty(), "{step}: {:?}", run.acts);
        assert_eq!(run.confirms.len(), 1, "{step}");
        assert_eq!(run.confirms[0].category, RiskCategory::Payment, "{step}");
    }
}

/// The role JSON shape the poisoned flow uses really is `Role::Slider` (so the test above
/// replays the step rather than diverging).
#[test]
fn the_poisoned_flow_step_is_replayed_not_skipped() {
    let memory = poisoned_memory(serde_json::json!({"role": "slider",
        "label": "Slide to pay", "app_bundle_id": "com.example.shop",
        "action": {"type": "scroll", "direction": "left"}}));
    let run = run(
        config(),
        "Pay for the order",
        memory,
        checkout_phone(),
        Script::model(&[DONE]).decisions(&[true]),
    );
    assert_eq!(run.requests.len(), 0, "replayed with no model call");
    assert_eq!(run.confirms.len(), 1);
    assert!(matches!(run.acts.as_slice(), [RunnerCommand::Swipe { .. }]));
}

/// A skill confined to the shop app only narrows: a sideways scroll or swipe on (or
/// across) the pay slider inside it still asks.
#[test]
fn a_confined_skill_cannot_carry_a_drag_past_the_gate() {
    let mut c = config();
    c.skill = Some(SkillContext {
        id: "pay".into(),
        name: "Pay".into(),
        instructions: "Slide to pay.".into(),
        apps: vec!["com.example.shop".into()],
        confine_to_apps: true,
        args: BTreeMap::new(),
        tools: Vec::new(),
    });
    let i = index_of(&checkout_screen(), "Slide to pay");
    for a in [
        format!(r#"{{"action":"scroll","direction":"left","element":{i}}}"#),
        format!(r#"{{"action":"swipe","direction":"right","element":{i}}}"#),
        r#"{"action":"swipe","direction":"right"}"#.to_owned(),
    ] {
        let run = run(
            c.clone(),
            "Pay for the order",
            MemoryStore::new(),
            checkout_phone(),
            Script::model(&[&a, DONE]),
        );
        assert!(run.acts.is_empty(), "{a}: {:?}", run.acts);
        assert_eq!(run.confirms.len(), 1, "{a}");
        assert_eq!(run.confirms[0].category, RiskCategory::Payment, "{a}");
    }
}

// ---------------------------------------------------------------------------------------
// Debug output.
// ---------------------------------------------------------------------------------------

/// Rule 6: sensitive profile values must not appear in `Debug` output.
#[test]
fn session_debug_never_shows_a_sensitive_profile_value() {
    let mut memory = MemoryStore::new();
    memory.profile.set("card_number", "4111111111111111", true);
    let s = Session::new(config(), TaskSpec::new("t", "Pay"), memory);
    let dbg = format!("{s:?}");
    assert!(
        !dbg.contains("4111111111111111"),
        "Session Debug output contains a sensitive profile value"
    );
    let _ = OutcomeStatus::Success;
}

/// Nor the text being typed, with a profile value substituted, while the session waits
/// for the runner (the queue and the typed-text check hold it).
#[test]
fn session_debug_never_shows_typed_text_mid_action() {
    let mut memory = MemoryStore::new();
    memory.profile.set("card_number", "4111111111111111", true);
    let field = screen(
        "com.example.shop",
        "Shop",
        app_root(
            "Shop",
            vec![UiNode::new(Role::TextField, rect(16.0, 300.0, 361.0, 44.0))
                .with_label("Gift message")
                .hittable(true)
                .focused(true)],
        ),
        true,
    );
    let mut s = Session::new(config(), TaskSpec::new("t", "Write the note"), memory);
    let mut now = 10;
    let mut step = |s: &mut Session, e: Event| {
        now += 10;
        s.step(TimedEvent::new(now, e))
    };
    step(&mut s, Event::Start);
    step(&mut s, Event::Observed(Observation::new(field.clone())));
    let typed = "hello secret-note-7731 {{profile.card_number}}";
    let e = step(
        &mut s,
        Event::ModelOutput(ModelOutput {
            text: format!(r#"{{"action":"type","text":"{typed}"}}"#),
        }),
    );
    let e = match e {
        Effect::Confirm(_) => step(&mut s, Event::UserDecision { approved: true }),
        other => other,
    };
    let e = match e {
        Effect::Observe { .. } => step(&mut s, Event::Observed(Observation::new(field))),
        other => other,
    };
    assert!(
        matches!(e, Effect::Act(RunnerCommand::TypeText { .. })),
        "{e:?}"
    );
    let dbg = format!("{s:?}");
    for secret in ["4111111111111111", "secret-note-7731"] {
        assert!(!dbg.contains(secret), "{secret} in {dbg}");
    }
    // Nor the effect itself, nor the runner command it carries (only the length shows).
    let effect = format!("{e:?}");
    let Effect::Act(cmd) = &e else { panic!() };
    let command = format!("{cmd:?}");
    for shown in [&effect, &command] {
        for secret in ["4111111111111111", "secret-note-7731", "hello"] {
            assert!(!shown.contains(secret), "{secret} in {shown}");
        }
        assert!(shown.contains("text_chars"), "{shown}");
    }
}

/// The owner's answer to a question is not shown by `Debug` either.
#[test]
fn user_answer_debug_never_shows_the_answer() {
    let e = Event::UserAnswer {
        text: "my door code is 4471".to_owned(),
    };
    let shown = format!("{e:?}");
    assert!(
        !shown.contains("4471") && !shown.contains("door"),
        "{shown}"
    );
    assert!(shown.contains("UserAnswer"), "{shown}");
    let timed = format!("{:?}", TimedEvent::new(5, e));
    assert!(!timed.contains("4471"), "{timed}");
}

// ---------------------------------------------------------------------------------------
// The approval context (second review): an approval holds only while the target, its row,
// the texts just before it, every amount and the classifier's context are unchanged.
// ---------------------------------------------------------------------------------------

/// A flat (web-like) shop list: name/price texts and a "Buy" button per item, with an
/// optional status line far above the list.
fn shop_list_with(status: Option<&str>, items: &[(&str, &str)]) -> Screen {
    let mut children = Vec::new();
    if let Some(s) = status {
        children.push(text(s, rect(16.0, 20.0, 200.0, 20.0)));
    }
    children.push(text("Shop", rect(16.0, 70.0, 200.0, 40.0)));
    for (k, (name, price)) in items.iter().enumerate() {
        let y = 200.0 + 120.0 * k as f64;
        children.push(text(name, rect(16.0, y, 250.0, 30.0)));
        children.push(text(price, rect(16.0, y + 30.0, 250.0, 30.0)));
        children.push(node(Role::Button, "Buy", rect(280.0, y, 90.0, 60.0)));
    }
    screen(
        "com.example.shop",
        "Shop",
        app_root("Shop", children),
        false,
    )
}

fn shop_list(items: &[(&str, &str)]) -> Screen {
    shop_list_with(None, items)
}

/// The `n`th (1-based) element labelled `label`.
fn nth_index(s: &Screen, label: &str, n: usize) -> u32 {
    jarvis_observe::compact(s)
        .elements
        .iter()
        .filter(|e| e.label == label)
        .nth(n - 1)
        .unwrap()
        .index
}

/// Asks to tap element `i` on `before`, approves, and returns the summary asked and the
/// effect on the fresh look `after`.
fn approve_then_look(
    c: jarvis_proto::SessionConfig,
    before: &Screen,
    i: u32,
    after: &Screen,
) -> (String, Effect) {
    let mut d = Driver::new(c, "Buy the socks");
    d.step(Event::Start);
    d.observed(before);
    let Effect::Confirm(asked) = d.model(&format!(r#"{{"action":"tap","element":{i}}}"#)) else {
        panic!("expected a confirmation")
    };
    assert!(matches!(
        d.step(Event::UserDecision { approved: true }),
        Effect::Observe { .. }
    ));
    (asked.summary, d.observed(after))
}

/// Socks approved; the socks row disappears and the TV's "Buy" slides into the same slot
/// (same label, role, fingerprint; its frame holds the planned point). The TV is not
/// bought under the socks approval.
#[test]
fn an_approval_for_one_buy_does_not_buy_its_twin() {
    let before = shop_list(&[("Socks", "$5.00"), ("TV", "$1,500.00")]);
    let i = nth_index(&before, "Buy", 1);
    let (asked, e) = approve_then_look(config(), &before, i, &shop_list(&[("TV", "$1,500.00")]));
    assert_eq!(
        asked,
        "Tap \"Buy\" next to \"Socks $5.00\" in Shop. Amounts on screen: TV $1,500.00; Socks $5.00"
    );
    assert!(
        !matches!(e, Effect::Act(_)),
        "approved '{asked}' but tapped: {e:?}"
    );
}

/// The total next to "Pay" changes from $5 to $500 between ask and act: not paid.
#[test]
fn an_approval_at_one_amount_does_not_pay_another() {
    let pay = |amount: &str| {
        screen(
            "com.example.shop",
            "Shop",
            app_root(
                "Shop",
                vec![
                    text("Checkout", rect(16.0, 70.0, 200.0, 40.0)),
                    text(&format!("Total {amount}"), rect(16.0, 200.0, 361.0, 30.0)),
                    node(Role::Button, "Pay", rect(16.0, 600.0, 361.0, 50.0)),
                ],
            ),
            false,
        )
    };
    let before = pay("$5.00");
    let i = index_of(&before, "Pay");
    let (asked, e) = approve_then_look(config(), &before, i, &pay("$500.00"));
    assert_eq!(asked, "Tap \"Pay\" in Shop. Amounts on screen: Total $5.00");
    assert!(
        !matches!(e, Effect::Act(_)),
        "approved '{asked}' but paid: {e:?}"
    );
    // The same total: paid as approved.
    let (_, e) = approve_then_look(config(), &before, i, &pay("$5.00"));
    assert!(matches!(e, Effect::Act(_)), "{e:?}");
}

/// Step by step: the switch the owner approved turning on is already on at the fresh look;
/// the approved tap would turn it off, so it is not sent.
#[test]
fn an_approved_switch_that_flipped_meanwhile_is_not_tapped() {
    let mut c = config();
    c.policy.mode = jarvis_proto::PolicyMode::StepByStep;
    let before = wifi_screen(false);
    let sw = jarvis_observe::compact(&before)
        .elements
        .iter()
        .find(|e| e.role == Role::Switch)
        .unwrap()
        .index;
    let (asked, e) = approve_then_look(c.clone(), &before, sw, &wifi_screen(true));
    assert_eq!(asked, "Turn on \"Wi-Fi\" in Settings");
    assert!(!matches!(e, Effect::Act(_)), "approved '{asked}': {e:?}");
    // Unchanged: it runs.
    let (_, e) = approve_then_look(c, &before, sw, &wifi_screen(false));
    assert!(matches!(e, Effect::Act(_)), "{e:?}");
}

/// Not brittle: a clock or countdown digit far from the target changing does not void the
/// approval; a text in the target's own row changing does.
///
/// Deliberately changed in the third review: the previous design let *any* unrelated
/// static text far from the target change ("Updated 9:41" → "Updating…" still ran). Under
/// the whole-screen record only digits of free-standing texts may change; a changed word
/// anywhere voids the approval.
#[test]
fn an_approval_survives_far_changes_but_not_changes_in_its_row() {
    let items = [("Socks", "$5.00"), ("TV", "$1,500.00")];
    let before = shop_list_with(Some("Updated 9:41"), &items);
    let tv = nth_index(&before, "Buy", 2);
    for status in ["Updated 9:42", "Updated 10:05"] {
        let after = shop_list_with(Some(status), &items);
        let (asked, e) = approve_then_look(config(), &before, tv, &after);
        assert!(
            matches!(e, Effect::Act(_)),
            "'{status}' far away voided '{asked}': {e:?}"
        );
    }
    let after = shop_list_with(Some("Updating…"), &items);
    let (asked, e) = approve_then_look(config(), &before, tv, &after);
    assert!(
        !matches!(e, Effect::Act(_)),
        "a changed word far away no longer keeps '{asked}': {e:?}"
    );
    let after = shop_list_with(
        Some("Updated 9:41"),
        &[("Socks", "$5.00"), ("TV (used)", "$1,500.00")],
    );
    let (asked, e) = approve_then_look(config(), &before, tv, &after);
    assert!(
        !matches!(e, Effect::Act(_)),
        "the row of '{asked}' changed but it ran: {e:?}"
    );
}

/// A screen that keeps changing after every approval, with a harmless step that works in
/// between (resetting the failure count), ends `Failed` after
/// [`jarvis_agent::MAX_STALE_APPROVALS`] stale approvals instead of asking the owner until
/// the step budget runs out.
#[test]
fn stale_approvals_are_capped_even_with_successes_in_between() {
    // Every look shows a new screen: "Buy now" alternates between two places and the
    // harmless button's caption counts up (so tapping it visibly works).
    let mut d = Driver::new(config(), "Buy the headphones");
    let mut e = d.step(Event::Start);
    let mut looks = 0;
    let mut current = shop(300.0, "Details 0");
    let (mut infers, mut harmless, mut confirms) = (0, 0, 0);
    for _ in 0..200 {
        e = match e {
            Effect::Observe { .. } => {
                looks += 1;
                let at = if looks % 2 == 1 { 300.0 } else { 560.0 };
                current = shop(at, &format!("Details {looks}"));
                d.observed(&current)
            }
            Effect::Infer(_) => {
                infers += 1;
                if infers % 2 == 1 {
                    d.model(&tap_buy(&current))
                } else {
                    let i = index_of(&current, &format!("Details {looks}"));
                    d.model(&format!(r#"{{"action":"tap","element":{i}}}"#))
                }
            }
            Effect::Confirm(c) => {
                assert_eq!(c.category, RiskCategory::Purchase, "{}", c.summary);
                confirms += 1;
                d.step(Event::UserDecision { approved: true })
            }
            Effect::Act(_) => {
                harmless += 1;
                d.step(Event::ActResult(RunnerResult::ok()))
            }
            Effect::Finished(o) => {
                assert_eq!(o.status, OutcomeStatus::Failed, "{}", o.summary);
                assert!(
                    o.summary
                        .contains("the screen changed after the owner approved"),
                    "{}",
                    o.summary
                );
                assert_eq!(confirms, jarvis_agent::MAX_STALE_APPROVALS as usize);
                assert_eq!(harmless, confirms - 1, "only the harmless taps ran");
                return;
            }
            other => panic!("{other:?}"),
        };
    }
    panic!("the session did not end");
}

// ---------------------------------------------------------------------------------------
// The approval covers the whole screen (third review): the fresh screen's content record
// must equal the one the owner approved, digits of free-standing texts excepted.
// ---------------------------------------------------------------------------------------

fn plans(monthly: bool) -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        app_root(
            "Shop",
            vec![
                text("Choose a plan", rect(16.0, 80.0, 361.0, 30.0)),
                node(
                    Role::Button,
                    "Monthly, $4.99",
                    rect(16.0, 150.0, 361.0, 50.0),
                )
                .selected(monthly),
                node(
                    Role::Button,
                    "Yearly, $49.99",
                    rect(16.0, 220.0, 361.0, 50.0),
                )
                .selected(!monthly),
                node(Role::Button, "Subscribe", rect(16.0, 700.0, 361.0, 50.0)),
            ],
        ),
        false,
    )
}

/// Probe 3 (end to end): approved with Monthly selected; at the fresh look Yearly is
/// selected; the approved tap on "Subscribe" is not sent. The question said what was
/// selected.
#[test]
fn a_subscribe_approved_on_one_plan_is_not_sent_on_the_other() {
    let before = plans(true);
    let i = index_of(&before, "Subscribe");
    let (asked, e) = approve_then_look(config(), &before, i, &plans(false));
    assert!(asked.ends_with("Selected: Monthly, $4.99"), "{asked}");
    assert!(
        !matches!(e, Effect::Act(_)),
        "approved '{asked}'; sent {e:?}"
    );
    let (_, e) = approve_then_look(config(), &before, i, &plans(true));
    assert!(matches!(e, Effect::Act(_)), "{e:?}");
}

/// The list scrolled between ask and act (the same content, every frame moved): the
/// approved control is tapped where it is now.
#[test]
fn an_approval_survives_a_scroll_and_is_tapped_where_it_is_now() {
    let before = shop(300.0, "Add to wishlist");
    let i = index_of(&before, "Buy now");
    let scrolled = screen(
        "com.example.shop",
        "Shop",
        app_root(
            "Shop",
            vec![
                text("Headphones $199.00", rect(16.0, 20.0, 361.0, 30.0)),
                node(Role::Button, "Buy now", rect(16.0, 200.0, 361.0, 50.0)),
                node(
                    Role::Button,
                    "Add to wishlist",
                    rect(16.0, 600.0, 361.0, 50.0),
                ),
            ],
        ),
        false,
    );
    let (_, e) = approve_then_look(config(), &before, i, &scrolled);
    assert_eq!(e, Effect::Act(RunnerCommand::Tap { x: 196.5, y: 225.0 }));
}

fn reserved(countdown: &str, stepper: &str, banner: Option<&str>) -> Screen {
    let mut children = vec![
        text("Concert tickets", rect(16.0, 80.0, 361.0, 30.0)),
        text("Total $90.00", rect(16.0, 200.0, 361.0, 30.0)),
        UiNode::new(
            Role::Other("Stepper".into()),
            rect(16.0, 300.0, 200.0, 40.0),
        )
        .with_label("Quantity")
        .with_value(stepper)
        .hittable(true),
        text(countdown, rect(16.0, 640.0, 361.0, 30.0)),
        node(Role::Button, "Pay", rect(16.0, 700.0, 361.0, 50.0)),
    ];
    if let Some(b) = banner {
        children.insert(1, text(b, rect(16.0, 130.0, 361.0, 30.0)));
    }
    screen(
        "com.example.shop",
        "Shop",
        app_root("Shop", children),
        false,
    )
}

/// A countdown right above the target ticking ("Reserved for 9:58" → "9:41") keeps the
/// approval; a stepper's quantity that changed, or a banner that appeared or disappeared,
/// voids it.
#[test]
fn countdowns_tick_but_quantities_and_banners_void_the_approval() {
    let before = reserved("Reserved for 9:58", "1", None);
    let pay = index_of(&before, "Pay");
    let (_, e) = approve_then_look(
        config(),
        &before,
        pay,
        &reserved("Reserved for 9:41", "1", None),
    );
    assert!(matches!(e, Effect::Act(_)), "{e:?}");
    let (_, e) = approve_then_look(
        config(),
        &before,
        pay,
        &reserved("Reserved for 9:58", "2", None),
    );
    assert!(!matches!(e, Effect::Act(_)), "quantity changed: {e:?}");
    let with_banner = reserved("Reserved for 9:58", "1", Some("Prices changed"));
    let (_, e) = approve_then_look(config(), &before, pay, &with_banner);
    assert!(!matches!(e, Effect::Act(_)), "a banner appeared: {e:?}");
    let pay_b = index_of(&with_banner, "Pay");
    let (_, e) = approve_then_look(config(), &with_banner, pay_b, &before);
    assert!(!matches!(e, Effect::Act(_)), "a banner disappeared: {e:?}");
}

/// Probe 3 (timing): a 5,000-element screen goes through observe, gate and ask, and the
/// look after the approval, in well under a few seconds in a debug build (each part was
/// up to ~0.6 s before; the record is linear).
#[test]
fn a_huge_screen_is_gated_and_rechecked_quickly() {
    use std::time::{Duration, Instant};
    let mut children = Vec::new();
    for i in 0..5000u32 {
        let y = 100.0 + f64::from(i % 15) * 40.0;
        if i % 3 == 0 {
            children.push(node(
                Role::Button,
                &format!("Option {i}"),
                rect(16.0, y, 361.0, 36.0),
            ));
        } else {
            children.push(text(
                &format!("Item {i} costs ${i}.00 total"),
                rect(16.0, y, 361.0, 36.0),
            ));
        }
    }
    children.push(node(Role::Button, "Pay", rect(16.0, 760.0, 361.0, 50.0)));
    let s = screen(
        "com.example.shop",
        "Shop",
        app_root("Shop", children),
        false,
    );
    let i = index_of(&s, "Pay");
    let mut d = Driver::new(config(), "Pay");
    d.step(Event::Start);
    let t = Instant::now();
    d.observed(&s);
    let observe = t.elapsed();
    let t = Instant::now();
    let asked = d.model(&format!(r#"{{"action":"tap","element":{i}}}"#));
    let ask = t.elapsed();
    assert!(matches!(asked, Effect::Confirm(_)), "{asked:?}");
    d.step(Event::UserDecision { approved: true });
    let t = Instant::now();
    let acted = d.observed(&s);
    let recheck = t.elapsed();
    assert!(matches!(acted, Effect::Act(_)), "{acted:?}");
    println!("5,000 elements: observe {observe:?}, gate+ask {ask:?}, recheck+act {recheck:?}");
    assert!(ask < Duration::from_millis(1500), "{ask:?}");
    assert!(recheck < Duration::from_millis(3000), "{recheck:?}");
}
