//! Safety behaviour: confirmations, deny list, prompt injection, stuck detection, budgets.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{keyboard, node, rect, screen, settings_screen, text, IPHONE_SIZE};
use jarvis_proto::{
    AppRule, Effect, Event, Key, OutcomeStatus, RiskCategory, Role, RunnerCommand, Screen,
    TrustLevel, UiNode,
};
use jarvis_safety::{SCREEN_BLOCK_END, SCREEN_BLOCK_START};

fn shop_phone() -> Phone {
    Phone::new(product_screen(), |cmd, s: &Screen| {
        if taps(cmd, s, "Buy now") {
            Transition::Go(bought_screen())
        } else if taps(cmd, s, "Add to wishlist") {
            Transition::Go(home_screen())
        } else {
            Transition::Stay
        }
    })
}

fn buy() -> String {
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&product_screen(), "Buy now")
    )
}

fn wishlist() -> String {
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&product_screen(), "Add to wishlist")
    )
}

#[test]
fn confirmation_approved_executes() {
    let b = buy();
    let run = run(
        config(),
        "Buy the headphones",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&b, r#"{"action":"done","summary":"bought"}"#]).decisions(&[true]),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Success);
    assert_eq!(run.confirms.len(), 1);
    let c = &run.confirms[0];
    assert!(
        matches!(c.category, RiskCategory::Purchase | RiskCategory::Payment),
        "{c:?}"
    );
    assert_eq!(c.app, "com.example.shop");
    assert!(c.summary.contains("Buy now"), "{}", c.summary);
    assert_eq!(run.acts.len(), 1);
    // Confirm comes before the act.
    let names: Vec<&str> = run.effects.iter().map(|e| e.name()).collect();
    let ci = names.iter().position(|n| *n == "confirm").unwrap();
    let ai = names.iter().position(|n| *n == "act").unwrap();
    assert!(ci < ai);
}

#[test]
fn confirmation_declined_then_continue() {
    let (b, w) = (buy(), wishlist());
    let run = run(
        config(),
        "Buy the headphones or save them",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&b, &w, r#"{"action":"done","summary":"saved to wishlist"}"#])
            .decisions(&[false]),
    );
    assert_eq!(
        run.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run.outcome.summary
    );
    assert_eq!(run.confirms.len(), 1);
    assert!(run
        .acts
        .iter()
        .all(|c| !taps(c, &product_screen(), "Buy now")));
    let u = &run.requests[1].user;
    assert!(u.contains("-> not done: the owner declined"), "{u}");
}

#[test]
fn declined_twice_same_category_blocks() {
    let b = buy();
    let run = run(
        config(),
        "Buy the headphones",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&b, &b, r#"{"action":"done","summary":"x"}"#]).decisions(&[false, true]),
    );
    assert_eq!(
        run.outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        run.outcome.summary
    );
    assert_eq!(run.confirms.len(), 1, "the owner is not asked twice");
    assert!(run.acts.is_empty());
    assert!(run.outcome.summary.contains("declined"));
    assert_eq!(run.memory.log.entries()[0].outcome, OutcomeStatus::Blocked);
}

#[test]
fn open_app_into_denied_app_is_refused_then_blocked() {
    let run = run(
        config(),
        "Show my saved passwords",
        MemoryStore::new(),
        Phone::still(home_screen()),
        Script::model(&[
            r#"{"action":"open_app","app":"Passwords"}"#,
            r#"{"action":"open_app","app":"com.apple.Passwords"}"#,
            r#"{"action":"done","summary":"x"}"#,
        ]),
    );
    assert!(
        run.acts.is_empty(),
        "LaunchApp must never be sent: {:?}",
        run.acts
    );
    assert!(run.requests[1]
        .user
        .contains("not allowed: com.apple.Passwords is on the deny list"));
    assert_eq!(run.outcome.status, OutcomeStatus::Blocked);
}

#[test]
fn already_in_a_denied_app_blocks_without_reading_it() {
    let run = run(
        config(),
        "Copy my password",
        MemoryStore::new(),
        Phone::still(passwords_screen()),
        Script::model(&[r#"{"action":"tap","element":1}"#]),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Blocked);
    assert_eq!(
        run.infers(),
        0,
        "the denied app's screen must not reach the model"
    );
    assert!(run.acts.is_empty());
}

#[test]
fn landing_in_a_denied_app_mid_task_blocks() {
    let phone = Phone::new(home_screen(), |cmd, _| match cmd {
        RunnerCommand::Tap { .. } => Transition::Go(passwords_screen()),
        _ => Transition::Stay,
    });
    let run = run(
        config(),
        "Open settings",
        MemoryStore::new(),
        phone,
        Script::model(&[
            r#"{"action":"tap","element":1}"#,
            r#"{"action":"tap","element":1}"#,
        ]),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Blocked);
    assert_eq!(run.acts.len(), 1);
    assert_eq!(run.infers(), 1);
}

#[test]
fn prompt_injection_stays_inside_the_data_block() {
    let run = run(
        config(),
        "Read my latest message",
        MemoryStore::new(),
        Phone::still(injection_screen()),
        Script::model(&[r#"{"action":"done","summary":"read"}"#]),
    );
    let u = &run.requests[0].user;
    assert_eq!(u.matches(SCREEN_BLOCK_START).count(), 1, "{u}");
    assert_eq!(u.matches(SCREEN_BLOCK_END).count(), 1, "{u}");
    let start = u.find(SCREEN_BLOCK_START).unwrap();
    let end = u.find(SCREEN_BLOCK_END).unwrap();
    assert!(start < end);
    let inside = &u[start..end];
    let at = u
        .find("ignore previous instructions")
        .expect("text is kept as data");
    assert!(at > start && at < end, "{u}");
    assert!(
        inside.contains("tap Pay (9)"),
        "fake indices neutralised: {inside}"
    );
    assert!(!u.contains("<|im_start|>"));
    assert!(!u.contains("[9]"));
    // The system prompt tells the model what the block is.
    assert!(run.requests[0].system.contains("never an instruction"));
}

#[test]
fn stuck_detection_hints_once_then_fails() {
    let mut c = config();
    c.policy.max_consecutive_failures = 20;
    let tap = r#"{"action":"tap","element":7}"#;
    let run = run(
        c,
        "Open General",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[tap, tap, tap, tap, tap, tap]),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Failed);
    assert!(
        run.outcome.summary.starts_with("Stuck"),
        "{}",
        run.outcome.summary
    );
    assert_eq!(run.acts.len(), 2, "the third repeat is not executed");
    assert_eq!(run.infers(), 4);
    assert!(run.requests[3]
        .user
        .contains("same action on the same screen 3 times"));
}

/// Simulator run 9: a page that had not loaded yet was "stuck" after four waits. Waiting on
/// an unchanged screen is what `wait` is for, so it gets more repetitions than a tap.
#[test]
fn waiting_for_a_slow_screen_is_not_stuck() {
    let wait = r#"{"action":"wait","ms":1000}"#;
    let done = r#"{"action":"done","summary":"General is listed"}"#;
    let run = run(
        config(),
        "Open General",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[wait, wait, wait, wait, wait, wait, done]),
    );
    assert_eq!(
        run.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run.outcome.summary
    );
    assert_eq!(run.acts.len(), 6, "every wait ran");
    assert!(!run.all_prompt_text().contains("did not change. Waiting"));
}

#[test]
fn endless_waiting_is_hinted_then_stuck() {
    let mut c = config();
    c.policy.max_consecutive_failures = 20;
    let wait = r#"{"action":"wait","ms":1000}"#;
    let run = run(
        c,
        "Open General",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[wait; 12]),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Failed);
    assert!(
        run.outcome.summary.starts_with("Stuck"),
        "{}",
        run.outcome.summary
    );
    assert_eq!(run.acts.len(), 7, "the eighth wait is not executed");
    assert_eq!(run.infers(), 9);
    assert!(
        run.requests[8]
            .user
            .contains("Waiting longer will not help"),
        "{}",
        run.requests[8].user
    );
}

#[test]
fn step_budget_ends_the_session() {
    let mut c = config();
    c.policy.max_steps = 3;
    let run = run(
        c,
        "Wait for it",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[
            r#"{"action":"wait","ms":100}"#,
            r#"{"action":"wait","ms":200}"#,
            r#"{"action":"wait","ms":300}"#,
            r#"{"action":"wait","ms":400}"#,
        ]),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Failed);
    assert!(
        run.outcome.summary.contains("Step budget"),
        "{}",
        run.outcome.summary
    );
    assert_eq!(run.outcome.steps, 3);
    assert_eq!(run.infers(), 3);
}

#[test]
fn time_budget_ends_the_session() {
    let mut c = config();
    c.policy.max_duration_ms = 400;
    let waits: Vec<String> = (1..50)
        .map(|i| format!(r#"{{"action":"wait","ms":{i}}}"#))
        .collect();
    let refs: Vec<&str> = waits.iter().map(String::as_str).collect();
    let run = run(
        c,
        "Wait",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&refs),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Failed);
    assert!(
        run.outcome.summary.contains("Time budget"),
        "{}",
        run.outcome.summary
    );
    assert!(run.outcome.duration_ms >= 400);
    assert!(run.infers() < 10);
}

#[test]
fn consecutive_failures_end_the_session() {
    let run = run(
        config(),
        "Tap things",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[
            r#"{"action":"tap","element":99}"#,
            r#"{"action":"tap","element":98}"#,
            r#"{"action":"tap","element":97}"#,
            r#"{"action":"tap","element":96}"#,
            r#"{"action":"tap","element":95}"#,
        ]),
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Failed);
    assert!(
        run.outcome.summary.contains("4 failed actions in a row"),
        "{}",
        run.outcome.summary
    );
    assert_eq!(run.infers(), 4);
}

#[test]
fn replayed_steps_go_through_the_same_gate() {
    // Learn "buy" with approval, then replay it: the replayed tap must be confirmed again,
    // and a decline stops it.
    let b = buy();
    let first = run(
        config(),
        "Buy the headphones",
        MemoryStore::new(),
        shop_phone(),
        Script::model(&[&b, r#"{"action":"done","summary":"bought"}"#]).decisions(&[true]),
    );
    assert_eq!(first.memory.flows.len(), 1);
    let second = run(
        config(),
        "Buy the headphones",
        first.memory,
        shop_phone(),
        Script::model(&[&b]).decisions(&[false]),
    );
    assert_eq!(
        second.confirms.len(),
        1,
        "replay is confirmed like a model action"
    );
    assert!(second.acts.is_empty());
    // After the decline the model takes over; asking for the same category again blocks.
    assert_eq!(second.outcome.status, OutcomeStatus::Blocked);
    assert!(second.requests[0]
        .user
        .contains("(saved) -> not done: the owner declined"));
}

#[test]
fn secrets_on_screen_are_redacted_from_prompts_and_log() {
    use jarvis_proto::fixtures::{node, rect, screen};
    use jarvis_proto::{Role, UiNode};
    let card = "4111 1111 1111 1111";
    let s = screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, rect(0.0, 0.0, 393.0, 852.0)).with_children(vec![
            node(
                Role::Cell,
                &format!("Card {card}"),
                rect(16.0, 200.0, 361.0, 44.0),
            ),
            UiNode::new(Role::SecureTextField, rect(16.0, 260.0, 361.0, 44.0))
                .with_label("Password")
                .with_value("hunter2-secret")
                .secure(true)
                .hittable(true),
        ]),
        false,
    );
    let run = run(
        config(),
        "Pay with card 4111 1111 1111 1111",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[r#"{"action":"fail","reason":"card 4111 1111 1111 1111 declined"}"#]),
    );
    let all = run.all_prompt_text();
    assert!(!all.contains("hunter2-secret"));
    assert!(
        !all.contains("4111 1111 1111 1111\""),
        "screen copy is redacted: {all}"
    );
    assert!(all.contains("redacted card"));
    let log = &run.memory.log.entries()[0];
    assert!(!log.task.contains("4111 1111"), "{}", log.task);
    assert!(!log.summary.contains("4111 1111"), "{}", log.summary);
}

// ---------------------------------------------------------------------------------------
// Gestures, the keyboard's Return key and denied apps' icons (regressions from a review:
// each of these ran unasked before).
// ---------------------------------------------------------------------------------------

fn app_root(label: &str, children: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(label)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ])
}

/// A checkout screen with a total, a card line and a "Slide to pay" slider across the
/// middle of the screen.
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

fn paid_screen() -> Screen {
    screen(
        "com.example.shop",
        "Shop",
        app_root(
            "Shop",
            vec![text("Payment complete", rect(16.0, 300.0, 361.0, 40.0))],
        ),
        false,
    )
}

/// Any drag pays.
fn checkout_phone() -> Phone {
    Phone::new(checkout_screen(), |cmd, _s: &Screen| match cmd {
        RunnerCommand::Swipe { .. } => Transition::Go(paid_screen()),
        _ => Transition::Stay,
    })
}

/// Rule 1: a risky control reached by a gesture must ask. Control: `swipe` on the slider
/// asks (classifier sees Payment).
#[test]
fn swipe_on_a_pay_slider_asks() {
    let i = index_of(&checkout_screen(), "Slide to pay");
    let a = format!(r#"{{"action":"swipe","direction":"right","element":{i}}}"#);
    let run = run(
        config(),
        "Pay for the order",
        MemoryStore::new(),
        checkout_phone(),
        Script::model(&[&a, r#"{"action":"done","summary":"paid"}"#]),
    );
    assert!(!run.confirms.is_empty(), "swipe on the pay slider must ask");
    assert!(
        run.acts.is_empty(),
        "declined: nothing may run: {:?}",
        run.acts
    );
}

/// Rule 1: the same drag expressed as a sideways `scroll` in the slider moves the finger
/// right across "Slide to pay" and must ask as well.
#[test]
fn a_sideways_scroll_on_a_pay_slider_asks_like_the_swipe_it_becomes() {
    let i = index_of(&checkout_screen(), "Slide to pay");
    let a = format!(r#"{{"action":"scroll","direction":"left","element":{i}}}"#);
    let run = run(
        config(),
        "Pay for the order",
        MemoryStore::new(),
        checkout_phone(),
        Script::model(&[&a, r#"{"action":"done","summary":"paid"}"#]),
    );
    assert!(
        !run.confirms.is_empty() || run.acts.is_empty(),
        "scroll left in the pay slider dragged it with no confirmation: acts {:?}, outcome {:?}",
        run.acts,
        run.outcome
    );
    assert_eq!(run.confirms[0].category, RiskCategory::Payment);
}

/// A vertical scroll whose element is the slider itself is a drag on it too.
#[test]
fn a_vertical_scroll_on_a_pay_slider_asks() {
    let i = index_of(&checkout_screen(), "Slide to pay");
    let a = format!(r#"{{"action":"scroll","direction":"down","element":{i}}}"#);
    let run = run(
        config(),
        "Pay for the order",
        MemoryStore::new(),
        checkout_phone(),
        Script::model(&[&a, r#"{"action":"done","summary":"paid"}"#]),
    );
    assert_eq!(run.confirms.len(), 1, "{:?}", run.acts);
    assert_eq!(run.confirms[0].category, RiskCategory::Payment);
    assert!(run.acts.is_empty());
}

/// Rule 1: an element-less sideways swipe runs through the middle of the screen, across
/// the pay slider, and must ask on a checkout screen.
#[test]
fn an_elementless_swipe_across_a_pay_slider_asks() {
    let a = r#"{"action":"swipe","direction":"right"}"#;
    let run = run(
        config(),
        "Pay for the order",
        MemoryStore::new(),
        checkout_phone(),
        Script::model(&[a, r#"{"action":"done","summary":"paid"}"#]).decisions(&[true]),
    );
    // The gesture really crosses the slider.
    if let Some(RunnerCommand::Swipe { from, to, .. }) = run.acts.first() {
        assert!(
            from.y > 395.0 && from.y < 439.0 && to.x > from.x,
            "{from:?} {to:?}"
        );
    }
    assert!(
        !run.confirms.is_empty() || run.acts.is_empty(),
        "element-less swipe across the pay slider ran unasked: acts {:?}, outcome {:?}",
        run.acts,
        run.outcome
    );
    assert_eq!(run.confirms[0].category, RiskCategory::Payment);
}

/// The same element-less drag as a sideways `scroll` (finger moving the other way).
#[test]
fn an_elementless_sideways_scroll_across_a_pay_slider_asks() {
    let a = r#"{"action":"scroll","direction":"right"}"#;
    let run = run(
        config(),
        "Pay for the order",
        MemoryStore::new(),
        checkout_phone(),
        Script::model(&[a, r#"{"action":"done","summary":"paid"}"#]),
    );
    assert_eq!(run.confirms.len(), 1);
    assert_eq!(run.confirms[0].category, RiskCategory::Payment);
    assert!(run.acts.is_empty(), "{:?}", run.acts);
}

/// Scrolling a plain list never asks (it would make every scroll a question).
#[test]
fn a_plain_vertical_scroll_of_a_list_does_not_ask() {
    let run = run(
        config(),
        "Find the last item",
        MemoryStore::new(),
        Phone::still(long_list_screen(30)),
        Script::model(&[
            r#"{"action":"scroll","direction":"down"}"#,
            r#"{"action":"swipe","direction":"up"}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ]),
    );
    assert!(run.confirms.is_empty(), "{:?}", run.confirms);
    assert_eq!(run.acts.len(), 2);
}

/// Rule 2: after the owner declines the payment swipe, the same drag phrased as a
/// sideways scroll must not run silently.
#[test]
fn a_declined_payment_is_not_redone_as_a_scroll() {
    let i = index_of(&checkout_screen(), "Slide to pay");
    let swipe = format!(r#"{{"action":"swipe","direction":"right","element":{i}}}"#);
    let scroll = format!(r#"{{"action":"scroll","direction":"left","element":{i}}}"#);
    let run = run(
        config(),
        "Pay for the order",
        MemoryStore::new(),
        checkout_phone(),
        Script::model(&[&swipe, &scroll, r#"{"action":"done","summary":"paid"}"#])
            .decisions(&[false]),
    );
    assert_eq!(run.confirms.len(), 1);
    assert!(
        run.acts.is_empty(),
        "owner said no to the payment, yet the slider was dragged: acts {:?}, outcome {:?}",
        run.acts,
        run.outcome
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Blocked);
}

/// ... and the other way round: a declined tap-free drag (element-less swipe) is not redone
/// as a scroll of the slider.
#[test]
fn a_declined_elementless_swipe_is_not_redone_as_a_scroll_on_the_slider() {
    let i = index_of(&checkout_screen(), "Slide to pay");
    let scroll = format!(r#"{{"action":"scroll","direction":"up","element":{i}}}"#);
    let run = run(
        config(),
        "Pay for the order",
        MemoryStore::new(),
        checkout_phone(),
        Script::model(&[
            r#"{"action":"swipe","direction":"right"}"#,
            &scroll,
            r#"{"action":"done","summary":"paid"}"#,
        ])
        .decisions(&[false]),
    );
    assert_eq!(run.confirms.len(), 1);
    assert!(run.acts.is_empty(), "{:?}", run.acts);
    assert_eq!(run.outcome.status, OutcomeStatus::Blocked);
}

/// A chat app that sends on Return: focused text field, no Send button, the keyboard's
/// return key reads `return_label` (a full keyboard subtree in the runner's tree).
fn chat_screen(value: Option<&str>, return_label: &str) -> Screen {
    let mut field = UiNode::new(Role::TextField, rect(16.0, 460.0, 361.0, 40.0))
        .with_label("Write something")
        .with_identifier("composer")
        .hittable(true)
        .focused(true);
    if let Some(v) = value {
        field = field.with_value(v);
    }
    screen(
        "com.example.chat",
        "Chat",
        app_root(
            "Chat",
            vec![
                text("Alex", rect(150.0, 70.0, 93.0, 40.0)),
                text("Are you coming?", rect(16.0, 300.0, 200.0, 40.0)),
                field,
                keyboard(return_label),
            ],
        ),
        true,
    )
}

fn chat_run(return_label: &'static str) -> Run {
    let phone = Phone::new(
        chat_screen(None, return_label),
        move |cmd, _s: &Screen| match cmd {
            RunnerCommand::TypeText { text } => {
                Transition::Go(chat_screen(Some(text), return_label))
            }
            RunnerCommand::PressKey { .. } => Transition::Go(chat_screen(None, return_label)),
            _ => Transition::Stay,
        },
    );
    run(
        config(),
        "Tell Alex I'm on my way",
        MemoryStore::new(),
        phone,
        Script::model(&[
            r#"{"action":"type","text":"on my way","submit":true}"#,
            r#"{"action":"done","summary":"sent"}"#,
        ]),
    )
}

fn pressed_return(run: &Run) -> bool {
    run.acts
        .iter()
        .any(|c| matches!(c, RunnerCommand::PressKey { key: Key::Return }))
}

/// Rule 1: Return with a "send" return key sends. The keyboard is a subtree of the real
/// tree; the observe → compact → session pipeline must carry the key's label to the gate.
#[test]
fn return_on_a_send_key_asks_end_to_end() {
    let run = chat_run("send");
    assert!(
        !run.confirms.is_empty() || !pressed_return(&run),
        "Return pressed on a send-on-return composer with no confirmation; acts {:?}",
        run.acts
    );
    assert_eq!(run.confirms[0].category, RiskCategory::SendMessage);
    assert!(!pressed_return(&run), "declined: nothing sent");
    // The model sees the key's label (a cue, not an element: numbering is unchanged).
    let user = &run.requests[0].user;
    assert!(
        user.contains("Keyboard: visible (Return key: \"send\")"),
        "{user}"
    );
    assert!(!user.contains("Button \"send\""), "{user}");
}

/// A plain "return" key keeps today's behaviour: nothing to ask in a plain composer.
#[test]
fn return_on_a_plain_return_key_does_not_ask() {
    let run = chat_run("return");
    assert!(run.confirms.is_empty(), "{:?}", run.confirms);
    assert!(pressed_return(&run), "{:?}", run.acts);
    assert!(!run.requests[0].user.contains("Return key:"));
}

/// UIKit names the key by its identifier `Return`; its label may be capitalised.
#[test]
fn a_return_key_found_by_its_identifier_asks_under_its_category() {
    let pay_screen = |value: Option<&str>| {
        let mut s = chat_screen(value, "Pay");
        // The fixture keyboard's last child is the return key: give it UIKit's identifier.
        let window = &mut s.root.children[0];
        let kb = window.children.last_mut().unwrap();
        let key = kb.children.last_mut().unwrap();
        key.identifier = Some("Return".to_owned());
        s
    };
    let phone = Phone::new(pay_screen(None), move |cmd, _s: &Screen| match cmd {
        RunnerCommand::TypeText { text } => Transition::Go(pay_screen(Some(text))),
        _ => Transition::Stay,
    });
    let run = run(
        config(),
        "Pay Alex",
        MemoryStore::new(),
        phone,
        Script::model(&[
            r#"{"action":"type","text":"20","submit":true}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ]),
    );
    assert_eq!(run.confirms.len(), 1);
    assert_eq!(run.confirms[0].category, RiskCategory::Payment);
    assert!(!pressed_return(&run));
}

fn bank_home() -> Screen {
    screen(
        "com.apple.springboard",
        "Home",
        app_root(
            "Home",
            vec![
                node(Role::Button, "ExampleBank", rect(20.0, 100.0, 64.0, 64.0)),
                node(Role::Button, "Settings", rect(110.0, 100.0, 64.0, 64.0)),
            ],
        ),
        false,
    )
}

fn bank_screen() -> Screen {
    screen(
        "com.examplebank.mobile",
        "ExampleBank",
        app_root(
            "ExampleBank",
            vec![node(
                Role::Button,
                "Transfer",
                rect(16.0, 300.0, 361.0, 50.0),
            )],
        ),
        false,
    )
}

fn bank_icon_run(cfg: jarvis_proto::SessionConfig, label: &str) -> Run {
    let mut home = bank_home();
    home.root.children[0].children[0].label = Some(label.to_owned());
    let i = index_of(&home, label);
    let h2 = home.clone();
    let wanted = label.to_owned();
    let phone = Phone::new(home, move |cmd, _s| {
        if taps(cmd, &h2, &wanted) {
            Transition::Go(bank_screen())
        } else {
            Transition::Stay
        }
    });
    let a = format!(r#"{{"action":"tap","element":{i}}}"#);
    run(
        cfg,
        "Check my balance",
        MemoryStore::new(),
        phone,
        Script::model(&[&a, &a, r#"{"action":"done","summary":"ok"}"#]),
    )
}

/// Rule 3: a denied app must never be opened, also from its home-screen icon. A denied
/// bank whose bundle id ends in a generic word (`com.examplebank.mobile`) and whose icon
/// says "ExampleBank" is recognised by name.
#[test]
fn a_denied_app_cannot_be_launched_from_its_home_screen_icon() {
    let mut cfg = config();
    cfg.policy
        .denied_apps
        .push("com.examplebank.mobile".to_owned());
    let run = bank_icon_run(cfg, "ExampleBank");
    assert!(
        run.acts.is_empty(),
        "the denied bank app was launched from its icon: acts {:?}, final app {}, outcome {:?}",
        run.acts,
        run.screen.app_bundle_id,
        run.outcome
    );
    assert_eq!(run.outcome.status, OutcomeStatus::Blocked);
}

/// The display name comes from `known_apps` when the bundle id is known: an icon named
/// unlike any part of the bundle id ("My Bank") is refused by name.
#[test]
fn a_denied_known_apps_display_name_is_refused_on_the_home_screen() {
    let mut cfg = config();
    cfg.policy
        .denied_apps
        .push("com.examplebank.mobile".to_owned());
    cfg.known_apps.push(jarvis_proto::AppInfo::new(
        "com.examplebank.mobile",
        "My Bank",
    ));
    for label in ["My Bank", "mybank", "MY-BANK!", "My Bank, 3 notifications"] {
        let run = bank_icon_run(cfg.clone(), label);
        assert!(run.acts.is_empty(), "{label}: {:?}", run.acts);
        assert_eq!(run.outcome.status, OutcomeStatus::Blocked, "{label}");
    }
    // open_app by that name is refused the same way.
    let run = run(
        cfg,
        "Check my balance",
        MemoryStore::new(),
        Phone::still(bank_home()),
        Script::model(&[
            r#"{"action":"open_app","app":"my bank"}"#,
            r#"{"action":"open_app","app":"My Bank"}"#,
        ]),
    );
    assert!(run.acts.is_empty(), "{:?}", run.acts);
    assert_eq!(run.outcome.status, OutcomeStatus::Blocked);
}

/// A `Denied` app rule carrying the owner-visible name (what the shell's settings pass)
/// refuses the icon and the app-switcher card with that name.
#[test]
fn a_denied_rule_with_a_display_name_refuses_icons_and_switcher_cards_by_name() {
    let mut cfg = config();
    cfg.policy
        .app_rules
        .push(AppRule::new("com.othercorp.x7", TrustLevel::Denied).with_name("Fortune Vault"));
    let by_icon = bank_icon_run(cfg.clone(), "Fortune Vault");
    assert!(by_icon.acts.is_empty(), "{:?}", by_icon.acts);
    assert_eq!(by_icon.outcome.status, OutcomeStatus::Blocked);
    // The app switcher (SpringBoard) shows the same name on the app's card.
    let switcher = screen(
        "com.apple.springboard",
        "Home",
        app_root(
            "Home",
            vec![node(
                Role::Other("card".into()),
                "fortune vault.",
                rect(40.0, 200.0, 300.0, 400.0),
            )
            .hittable(true)],
        ),
        false,
    );
    let i = index_of(&switcher, "fortune vault.");
    let a = format!(r#"{{"action":"tap","element":{i}}}"#);
    let run = run(
        cfg,
        "Check my balance",
        MemoryStore::new(),
        Phone::new(switcher, |_, _| Transition::Go(bank_screen())),
        Script::model(&[&a, &a]),
    );
    assert!(run.acts.is_empty(), "{:?}", run.acts);
    assert_eq!(run.outcome.status, OutcomeStatus::Blocked);
}

/// Names that are not denied still launch (no new refusals for ordinary icons).
#[test]
fn other_icons_still_open() {
    let mut cfg = config();
    cfg.policy
        .denied_apps
        .push("com.examplebank.mobile".to_owned());
    let mut home = bank_home();
    home.root.children[0].children[0].label = Some("Example".to_owned());
    let i = index_of(&home, "Settings");
    let run = run(
        cfg,
        "Open settings",
        MemoryStore::new(),
        Phone::new(home, |_, _| Transition::Go(settings_screen())),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{i}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ]),
    );
    assert_eq!(run.acts.len(), 1);
    assert_eq!(
        run.outcome.status,
        OutcomeStatus::Success,
        "{}",
        run.outcome.summary
    );
}

// ---------------------------------------------------------------------------------------
// Spotlight and the home screen (second review): a denied app cannot be opened by
// searching for it, by Return on a search that shows it, or by tapping a result or widget
// that names it.
// ---------------------------------------------------------------------------------------

fn spotlight_root(children: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Home")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ])
}

/// The home screen's search with `query` typed and `results` listed below the field.
fn spotlight(query: Option<&str>, results: &[&str]) -> Screen {
    let mut field = UiNode::new(Role::SearchField, rect(16.0, 120.0, 361.0, 36.0))
        .with_label("Search")
        .hittable(true)
        .focused(true);
    if let Some(q) = query {
        field = field.with_value(q);
    }
    let mut children = vec![field];
    for (k, r) in results.iter().enumerate() {
        children.push(node(
            Role::Button,
            r,
            rect(16.0, 200.0 + 60.0 * k as f64, 361.0, 50.0),
        ));
    }
    screen(
        "com.apple.springboard",
        "Home",
        spotlight_root(children),
        true,
    )
}

fn deny_bank(c: &mut jarvis_proto::SessionConfig, name: &str) {
    c.policy
        .app_rules
        .push(AppRule::new("com.examplebank.mobile", TrustLevel::Denied).with_name(name));
}

fn returned(acts: &[RunnerCommand]) -> bool {
    acts.iter()
        .any(|a| matches!(a, RunnerCommand::PressKey { key: Key::Return }))
}

fn typed_anything(acts: &[RunnerCommand]) -> bool {
    acts.iter()
        .any(|a| matches!(a, RunnerCommand::TypeText { .. }))
}

/// Typing a denied app's name (or the name and "app") into the home screen's search, with
/// or without Return, is refused before anything is typed. (Third review: a start of the
/// name, "Exa", or text containing it, "open ExampleBank now", is no longer refused here;
/// the results check holds their Return back, see
/// `spotlight_prefix_searches_are_typed_but_return_waits_for_the_results`.)
#[test]
fn spotlight_search_for_a_denied_app_is_refused_before_typing() {
    for (text, submit) in [
        ("ExampleBank", true),
        ("examplebank", false),
        ("Example Bank", true),
        ("examplebank app", true),
    ] {
        let mut c = config();
        deny_bank(&mut c, "ExampleBank");
        let r = run(
            c,
            "Check my bank balance",
            MemoryStore::new(),
            Phone::still(spotlight(None, &[])),
            Script::model(&[
                &format!(r#"{{"action":"type","text":"{text}","submit":{submit}}}"#),
                &format!(r#"{{"action":"type","text":"{text}","submit":{submit}}}"#),
            ]),
        );
        assert!(r.acts.is_empty(), "{text}: {:?}", r.acts);
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Blocked,
            "{text}: {}",
            r.outcome.summary
        );
    }
}

/// A harmless search whose results show a denied app: the text is typed, but Return
/// (which opens the top hit) is not pressed.
#[test]
fn spotlight_return_is_not_pressed_when_a_result_names_a_denied_app() {
    let mut c = config();
    deny_bank(&mut c, "ExampleBank");
    let phone = Phone::new(spotlight(None, &[]), |cmd, _s: &Screen| match cmd {
        RunnerCommand::TypeText { .. } => Transition::Go(spotlight(
            Some("balance"),
            &["ExampleBank", "Balance Sheets"],
        )),
        _ => Transition::Stay,
    });
    let r = run(
        c,
        "Check my balance",
        MemoryStore::new(),
        phone,
        Script::model(&[
            r#"{"action":"type","text":"balance","submit":true}"#,
            r#"{"action":"fail","reason":"blocked"}"#,
        ]),
    );
    assert!(typed_anything(&r.acts), "{:?}", r.acts);
    assert!(!returned(&r.acts), "{:?}", r.acts);
    assert!(
        r.all_prompt_text().contains("Return was not pressed"),
        "{}",
        r.last_user()
    );
}

/// An app result or icon of a denied app ("ExampleBank", "ExampleBank, 2 notifications")
/// and a widget of it ("Example Bank Balance", value "Widget") are not tapped. (Third
/// review: a label that merely contains the name, "Open ExampleBank", is no longer refused;
/// see `ordinary_home_screen_labels_that_mention_a_denied_app_are_tapped`.)
#[test]
fn a_home_screen_label_that_names_a_denied_app_is_not_tapped() {
    for (label, widget) in [
        ("ExampleBank", false),
        ("ExampleBank, 2 notifications", false),
        ("Example Bank Balance", true),
    ] {
        let mut c = config();
        deny_bank(&mut c, "ExampleBank");
        let mut home = spotlight(None, &[label]);
        if widget {
            home.root.children[0].children[1].value = Some("Widget".to_owned());
        }
        let i = index_of(&home, label);
        let tap = format!(r#"{{"action":"tap","element":{i}}}"#);
        let r = run(
            c,
            "Check my balance",
            MemoryStore::new(),
            Phone::still(home),
            Script::model(&[&tap, &tap]),
        );
        assert!(r.acts.is_empty(), "{label}: {:?}", r.acts);
        assert_eq!(r.outcome.status, OutcomeStatus::Blocked, "{label}");
    }
}

/// Unrelated searches still work, and a name that only shares a word with a denied one is
/// not refused: "weather", and "Example Mail" while "Example Bank" is denied. A label that
/// only starts like a denied name ("Mail" while "Mailchimp" is denied) is tapped, and
/// (third review: the prefix rule is gone) typing "Mail" with Return works while no
/// Mailchimp result is on the screen.
#[test]
fn spotlight_rules_leave_unrelated_apps_alone() {
    for (deny, text) in [("ExampleBank", "weather"), ("Example Bank", "Example Mail")] {
        let mut c = config();
        deny_bank(&mut c, deny);
        let r = run(
            c,
            "Search",
            MemoryStore::new(),
            Phone::still(spotlight(None, &[])),
            Script::model(&[
                &format!(r#"{{"action":"type","text":"{text}","submit":true}}"#),
                r#"{"action":"done","summary":"searched"}"#,
            ]),
        );
        assert!(typed_anything(&r.acts), "{text}: {:?}", r.acts);
        assert!(returned(&r.acts), "{text}: {:?}", r.acts);
    }
    let mut c = config();
    c.policy
        .app_rules
        .push(AppRule::new("com.mailchimp.app", TrustLevel::Denied).with_name("Mailchimp"));
    c.known_apps
        .push(jarvis_proto::AppInfo::new("com.apple.mobilemail", "Mail"));
    let home = spotlight(None, &["Mail"]);
    let i = index_of(&home, "Mail");
    let r = run(
        c.clone(),
        "Open Mail",
        MemoryStore::new(),
        Phone::still(home),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{i}}}"#),
            r#"{"action":"done","summary":"opened"}"#,
        ]),
    );
    assert_eq!(r.acts.len(), 1, "{:?}", r.acts);
    let r = run(
        c,
        "Open Mail",
        MemoryStore::new(),
        Phone::still(spotlight(None, &[])),
        Script::model(&[
            r#"{"action":"type","text":"Mail","submit":true}"#,
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    assert!(typed_anything(&r.acts), "{:?}", r.acts);
    assert!(returned(&r.acts), "{:?}", r.acts);
}

/// End to end: a page that puts 200 texts across the screen's middle before a "Slide to
/// pay" slider cannot get an element-less swipe across the slider sent unasked.
#[test]
fn padding_cannot_hide_a_pay_slider_from_an_elementless_swipe() {
    let mut children = vec![
        text("Checkout", rect(16.0, 70.0, 200.0, 40.0)),
        text("Total $42.10", rect(16.0, 200.0, 361.0, 30.0)),
    ];
    for i in 0..200 {
        children.push(text(
            &format!("Note line {i}"),
            rect(16.0, 400.0, 361.0, 30.0),
        ));
    }
    children.push(
        UiNode::new(Role::Slider, rect(16.0, 395.0, 361.0, 44.0))
            .with_label("Slide to pay")
            .with_value("0")
            .hittable(true),
    );
    let page = screen("com.example.shop", "Shop", spotlight_root(children), false);
    let r = run(
        config(),
        "Look around",
        MemoryStore::new(),
        Phone::still(page),
        Script::model(&[
            r#"{"action":"swipe","direction":"right"}"#,
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    assert!(r.acts.is_empty(), "{:?}", r.acts);
    assert_eq!(r.confirms.len(), 1, "{:?}", r.confirms);
    assert_eq!(r.confirms[0].category, RiskCategory::Payment);
}

// ---------------------------------------------------------------------------------------
// Spotlight (third review): every way to launch from the home screen goes through one
// gate, the results are checked (twice) before Return, and ordinary searches work.
// ---------------------------------------------------------------------------------------

/// Spotlight with "wa" typed, Wallet as the top hit, and the keyboard with its Return key.
fn spotlight_with_key(key_label: &str) -> Screen {
    let field = UiNode::new(Role::SearchField, rect(16.0, 120.0, 361.0, 36.0))
        .with_label("Search")
        .with_value("wa")
        .hittable(true)
        .focused(true);
    let keyboard = UiNode::new(Role::Keyboard, rect(0.0, 560.0, 393.0, 292.0)).with_children(vec![
        node(Role::Key, "w", rect(0.0, 600.0, 39.0, 44.0)),
        node(Role::Key, key_label, rect(300.0, 800.0, 90.0, 44.0)).with_identifier("Return"),
    ]);
    screen(
        "com.apple.springboard",
        "Home",
        spotlight_root(vec![
            field,
            node(Role::Button, "Wallet", rect(16.0, 200.0, 361.0, 50.0)),
            keyboard,
        ]),
        true,
    )
}

fn tap_point_on_return() -> String {
    format!(
        r#"{{"action":"tap_point","x":{},"y":{}}}"#,
        345.0 / 393.0,
        822.0 / IPHONE_SIZE.height
    )
}

/// Probe: a vision model taps the keyboard's Return key by point while Spotlight's top hit
/// is Wallet (denied by default). Keys are not listed, so the tap used to resolve to no
/// element and was sent with no check. It is refused (not a deny strike: the model is told
/// which app), with the default policy too.
#[test]
fn a_tap_point_on_the_return_key_never_opens_a_denied_top_hit() {
    for key in ["go", "search", "open"] {
        let r = run(
            vision_config(),
            "Show my cards",
            MemoryStore::new(),
            Phone::still(spotlight_with_key(key)),
            Script::model(&[
                r#"{"action":"look"}"#,
                &tap_point_on_return(),
                r#"{"action":"fail","reason":"x"}"#,
            ]),
        );
        assert!(r.acts.is_empty(), "key '{key}': {:?}", r.acts);
        assert!(
            r.all_prompt_text()
                .contains("could open Wallet, which is on the deny list"),
            "{}",
            r.last_user()
        );
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Failed,
            "{}",
            r.outcome.summary
        );
    }
    let mut c = first_use_config();
    c.model.supports_vision = true;
    let r = run(
        c,
        "Show my cards",
        MemoryStore::new(),
        Phone::still(spotlight_with_key("go")),
        Script::model(&[
            r#"{"action":"look"}"#,
            &tap_point_on_return(),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    assert!(
        r.acts.is_empty(),
        "acts {:?}; confirms {:?}",
        r.acts,
        r.confirms
    );
}

/// A tap on the home screen that hits no listed element (or the keyboard) is never sent
/// unasked, even with first-use approval off; declined, nothing is sent.
#[test]
fn an_unresolved_home_screen_tap_is_always_asked() {
    let mut search = spotlight_with_key("go");
    // No denied result: "Weather" instead of "Wallet".
    search.root.children[0].children[1].label = Some("Weather".to_owned());
    for (s, point) in [
        (search, tap_point_on_return()),
        (
            spotlight(None, &[]),
            r#"{"action":"tap_point","x":0.5,"y":0.05}"#.to_owned(),
        ),
    ] {
        let r = run(
            vision_config(),
            "Check the weather",
            MemoryStore::new(),
            Phone::still(s),
            Script::model(&[
                r#"{"action":"look"}"#,
                &point,
                r#"{"action":"fail","reason":"x"}"#,
            ]),
        );
        assert!(r.acts.is_empty(), "{:?}", r.acts);
        assert_eq!(r.confirms.len(), 1, "{:?}", r.confirms);
        assert_eq!(r.confirms[0].category, RiskCategory::FirstUse);
        assert!(
            r.confirms[0].summary.starts_with("Let Jarvis open ")
                && r.confirms[0].summary.contains(" from the home screen?"),
            "{}",
            r.confirms[0].summary
        );
    }
}

/// Drives a home-screen search for `text` with Return: after `TypeText` each observation
/// shows the next screen of `looks` (the last one repeats). Returns the runner commands and
/// the final outcome summary.
fn search_with_results(text: &str, looks: Vec<Screen>) -> (Vec<RunnerCommand>, String) {
    let mut s = jarvis_agent::Session::new(
        config(),
        jarvis_proto::TaskSpec::new("t", "Search"),
        MemoryStore::new(),
    );
    let mut now = 1_000;
    let mut typed = false;
    let mut shown = 0usize;
    let mut acts = Vec::new();
    let mut e = s.step(jarvis_proto::TimedEvent::new(now, Event::Start));
    let mut script = vec![
        format!(r#"{{"action":"type","text":"{text}","submit":true}}"#),
        r#"{"action":"fail","reason":"stop"}"#.to_owned(),
    ]
    .into_iter();
    for _ in 0..100 {
        now += 50;
        let ev = match e {
            Effect::Observe { .. } => {
                let screen = if typed {
                    let k = shown.min(looks.len() - 1);
                    shown += 1;
                    looks[k].clone()
                } else {
                    spotlight(None, &[])
                };
                Event::Observed(jarvis_proto::Observation::new(screen))
            }
            Effect::Infer(_) => Event::ModelOutput(jarvis_proto::ModelOutput {
                text: script.next().unwrap_or_default(),
            }),
            Effect::Act(cmd) => {
                typed |= matches!(cmd, RunnerCommand::TypeText { .. });
                acts.push(cmd);
                Event::ActResult(jarvis_proto::RunnerResult::ok())
            }
            Effect::Confirm(_) => Event::UserDecision { approved: true },
            Effect::Finished(o) => return (acts, o.summary),
            other => panic!("{other:?}"),
        };
        e = s.step(jarvis_proto::TimedEvent::new(now, ev));
    }
    panic!("the session did not end");
}

/// Results can load after the first look: the held-back Return waits for two looks in a
/// row with the same results. Wallet appearing on the second look is seen, and Return is
/// not pressed; harmless results that settle are.
#[test]
fn spotlight_return_waits_for_results_that_load_late() {
    let empty = spotlight(Some("wa"), &[]);
    let wallet = spotlight(Some("wa"), &["Wallet", "Walmart"]);
    let (acts, _) = search_with_results("wa", vec![empty.clone(), wallet.clone(), wallet]);
    assert!(typed_anything(&acts), "{acts:?}");
    assert!(!returned(&acts), "{acts:?}");
    let weather = spotlight(Some("wea"), &["Weather"]);
    let (acts, _) = search_with_results("wea", vec![empty.clone(), weather.clone(), weather]);
    assert!(returned(&acts), "{acts:?}");
    // Results that never settle: Return is not pressed.
    let looks: Vec<Screen> = (0..8)
        .map(|k| {
            spotlight(
                Some("we"),
                &[&format!(
                    "Result {}",
                    ["a", "b", "c", "d", "e", "f", "g", "h"][k]
                )],
            )
        })
        .collect();
    let (acts, _) = search_with_results("we", looks);
    assert!(!returned(&acts), "{acts:?}");
}

/// Third review: under the default deny list (Passwords, Wallet), ordinary searches whose
/// results merely mention a denied app ("leather wallet", "Wallpaper, Settings", "Wi-Fi
/// Passwords, Settings") are typed and submitted, and two of them do not end the session.
#[test]
fn ordinary_searches_under_the_default_deny_list_still_work() {
    let p = jarvis_proto::Policy {
        first_use_approval: false,
        ..jarvis_proto::Policy::default()
    };
    for (text, results) in [
        ("wallpaper", vec!["Wallpaper, Settings", "leather wallet"]),
        (
            "password",
            vec!["Wi-Fi Passwords, Settings", "password manager"],
        ),
        ("wal", vec!["Buy a new wallet, Reminders", "Walmart"]),
        ("pass", vec!["passport photo", "Passbook tips"]),
    ] {
        let mut c = config();
        c.policy = p.clone();
        let after = spotlight(Some(text), &results);
        let phone = Phone::new(spotlight(None, &[]), move |cmd, _s: &Screen| match cmd {
            RunnerCommand::TypeText { .. } => Transition::Go(after.clone()),
            _ => Transition::Stay,
        });
        let r = run(
            c,
            "Search",
            MemoryStore::new(),
            phone,
            Script::model(&[
                &format!(r#"{{"action":"type","text":"{text}","submit":true}}"#),
                r#"{"action":"done","summary":"searched"}"#,
            ]),
        );
        assert!(typed_anything(&r.acts), "{text}: {:?}", r.acts);
        assert!(returned(&r.acts), "{text}: {:?}", r.acts);
        assert_eq!(
            r.outcome.status,
            OutcomeStatus::Success,
            "{text}: {}",
            r.outcome.summary
        );
    }
}

/// A start of a denied name is typed now; its Return is held while the denied app is among
/// the results, the model is told which app, and that is no deny strike (two such searches
/// do not end the session `Blocked`).
#[test]
fn spotlight_prefix_searches_are_typed_but_return_waits_for_the_results() {
    for text in ["Exa", "open ExampleBank now"] {
        let mut c = config();
        deny_bank(&mut c, "ExampleBank");
        let after = spotlight(Some(text), &["ExampleBank", "Example Mail"]);
        let phone = Phone::new(spotlight(None, &[]), move |cmd, _s: &Screen| match cmd {
            RunnerCommand::TypeText { .. } => Transition::Go(after.clone()),
            _ => Transition::Stay,
        });
        let typing = format!(r#"{{"action":"type","text":"{text}","submit":true}}"#);
        let r = run(
            c,
            "Check my bank",
            MemoryStore::new(),
            phone,
            Script::model(&[&typing, &typing, r#"{"action":"fail","reason":"x"}"#]),
        );
        assert!(typed_anything(&r.acts), "{text}: {:?}", r.acts);
        assert!(!returned(&r.acts), "{text}: {:?}", r.acts);
        assert!(
            r.all_prompt_text()
                .contains("could open ExampleBank, which is on the deny list"),
            "{}",
            r.last_user()
        );
        assert_ne!(
            r.outcome.status,
            OutcomeStatus::Blocked,
            "{}",
            r.outcome.summary
        );
    }
}

/// Labels that merely mention a denied app (a web suggestion, another app's content) are
/// tapped like any unknown label.
#[test]
fn ordinary_home_screen_labels_that_mention_a_denied_app_are_tapped() {
    for label in ["Open ExampleBank", "ExampleBank login help, Safari"] {
        let mut c = config();
        deny_bank(&mut c, "ExampleBank");
        let home = spotlight(None, &[label]);
        let i = index_of(&home, label);
        let r = run(
            c,
            "Find help",
            MemoryStore::new(),
            Phone::still(home),
            Script::model(&[
                &format!(r#"{{"action":"tap","element":{i}}}"#),
                r#"{"action":"done","summary":"ok"}"#,
            ]),
        );
        assert_eq!(
            r.acts.len(),
            1,
            "{label}: {:?} {}",
            r.acts,
            r.all_prompt_text()
        );
    }
}
