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
use jarvis_proto::fixtures::settings_screen;
use jarvis_proto::{OutcomeStatus, RiskCategory, RunnerCommand, Screen};
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
