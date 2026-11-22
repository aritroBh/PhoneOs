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
    AppRule, Key, OutcomeStatus, RiskCategory, Role, RunnerCommand, Screen, TrustLevel, UiNode,
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
