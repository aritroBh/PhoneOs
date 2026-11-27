//! Review pass 35, fixes (F4): an answer given by a remote device never becomes a kept
//! approval. A first use a paired device approved holds for that task only and is not
//! written to memory, whoever started the task; a remote "yes" to what only the phone may
//! approve counts as a "no".

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{Answerer, Event, RiskCategory, RunnerCommand, Screen, TaskOrigin, TaskSpec};

const SHOP: &str = "com.example.shop";

fn wishlist_phone() -> Phone {
    Phone::new(product_screen(), |cmd: &RunnerCommand, s: &Screen| {
        if taps(cmd, s, "Add to wishlist") {
            Transition::Go(product_screen())
        } else {
            Transition::Stay
        }
    })
}

fn wish() -> String {
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&product_screen(), "Add to wishlist")
    )
}

fn first_uses(r: &Run) -> usize {
    r.confirms
        .iter()
        .filter(|c| c.category == RiskCategory::FirstUse)
        .count()
}

/// A remote first-use approval covers the rest of the task (asked once for two taps) and
/// is not kept: the next task, the owner's own on the phone included, asks again. The same
/// whether the task was started on the phone or by the device.
#[test]
fn a_remote_first_use_holds_for_the_task_only() {
    for origin in [TaskOrigin::Phone, TaskOrigin::Remote] {
        let w = wish();
        let more = format!(
            r#"{{"action":"tap","element":{}}}"#,
            index_of(&product_screen(), "Wireless Headphones")
        );
        let r = run_spec(
            first_use_config(),
            TaskSpec::new("t1", "Save the headphones and go on").from_origin(origin),
            MemoryStore::new(),
            wishlist_phone(),
            Script::model(&[&w, &more, r#"{"action":"done","summary":"ok"}"#])
                .decisions(&[true])
                .answered_by(Answerer::Remote),
        );
        // Both taps were sent (the second, "nothing changed", needed no new question).
        let taps_sent = r
            .acts
            .iter()
            .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
            .count();
        assert_eq!(taps_sent, 2, "{origin:?}: {:?}", r.acts);
        assert_eq!(first_uses(&r), 1, "{origin:?}: {:?}", r.confirms);
        assert!(
            !r.memory.is_app_approved(SHOP),
            "{origin:?}: kept in memory"
        );
        // The next task, on the phone, asks again; a phone "yes" is kept.
        let next = run_spec(
            first_use_config(),
            TaskSpec::new("t2", "Save the headphones"),
            r.memory.clone(),
            wishlist_phone(),
            Script::model(&[&w, r#"{"action":"done","summary":"ok"}"#]).decisions(&[true]),
        );
        assert_eq!(
            first_uses(&next),
            1,
            "{origin:?}: the next task did not ask"
        );
        assert!(next.memory.is_app_approved(SHOP));
    }
}

/// A first use the owner approved on the phone is kept, also in a task a device started.
#[test]
fn a_phone_first_use_is_kept() {
    let w = wish();
    let r = run_spec(
        first_use_config(),
        TaskSpec::new("t1", "Save the headphones").from_origin(TaskOrigin::Remote),
        MemoryStore::new(),
        wishlist_phone(),
        Script::model(&[&w, r#"{"action":"done","summary":"ok"}"#])
            .decisions(&[true])
            .answered_by(Answerer::Phone),
    );
    assert_eq!(first_uses(&r), 1);
    assert!(r.memory.is_app_approved(SHOP));
}

/// A remote "yes" to a question only the phone may approve (a purchase) is a "no": the
/// action is not sent.
#[test]
fn a_remote_yes_to_a_phone_only_question_is_a_no() {
    let buy = format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&product_screen(), "Buy now")
    );
    let mut memory = MemoryStore::new();
    memory.approve_app(SHOP);
    let r = run_spec(
        first_use_config(),
        TaskSpec::new("t1", "Buy the headphones"),
        memory,
        Phone::new(product_screen(), |cmd: &RunnerCommand, s: &Screen| {
            if taps(cmd, s, "Buy now") {
                Transition::Go(bought_screen())
            } else {
                Transition::Stay
            }
        }),
        Script::model(&[&buy, r#"{"action":"done","summary":"bought"}"#])
            .decisions(&[true])
            .answered_by(Answerer::Remote),
    );
    assert!(
        r.confirms
            .iter()
            .any(|c| jarvis_safety::NEVER_REMOTE_APPROVE.contains(&c.category) || c.phone_only),
        "{:?}",
        r.confirms
    );
    let bought = r.acts.iter().any(|a| taps(a, &product_screen(), "Buy now"));
    assert!(!bought, "a remote yes bought: {:?}", r.acts);
}

/// An answer whose source the shell does not give is remote (JSON without `by`).
#[test]
fn an_answer_of_unknown_source_is_remote() {
    let e: Event = serde_json::from_str(r#"{"type":"user_decision","approved":true}"#).unwrap();
    assert_eq!(e, Event::remote_decision(true));
}
