#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Voice, second break-it review, at the session level: what a task that is not the
//! owner's words (`TaskOrigin::Voice`) still writes to memory, and how it replays a flow the
//! owner taught. `f<N>_…` tests FAIL today and pass once fixed; `sound_…` held.

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{OutcomeStatus, RunnerCommand, TaskOrigin, TaskSpec};

fn search_phone() -> Phone {
    Phone::new(search_screen(true, None), |cmd, _| match cmd {
        RunnerCommand::TypeText { text, .. } => Transition::Go(search_screen(true, Some(text))),
        _ => Transition::Stay,
    })
}

fn typed(r: &Run, word: &str) -> bool {
    r.acts
        .iter()
        .any(|c| matches!(c, RunnerCommand::TypeText { text, .. } if text.contains(word)))
}

/// The owner types "Search lamps" on Ask and approves the typing: Jarvis learns the flow.
fn owner_taught() -> MemoryStore {
    let field = index_of(&search_screen(true, None), "Search");
    let first = run_spec(
        config(),
        TaskSpec::new("t1", "Search lamps").from_origin(TaskOrigin::Phone),
        MemoryStore::new(),
        search_phone(),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{field},"text":"lamps"}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true, true, true]),
    );
    assert!(first.memory.flows.find("Search lamps").is_some());
    first.memory
}

/// **f5, medium: tasks the room starts can retire the flow the owner taught.**
///
/// Input: the owner taught "Search lamps" (a typed task, approved). Then, three times (two suffice), a
/// hands-free voice task "Search lamps" (`TaskOrigin::Voice`: the wake word heard a video,
/// or anyone in the room) runs while the screen is not the search screen (the attacker
/// controls the screen and steers the model): the replay diverges and the model fails.
///
/// What happens: `update_memory` records each failed replay on the owner's flow
/// (`flows.record_result(id, false)`) whatever the origin; after three uses with more
/// failures than successes the flow is retired (the learning counts one success, so two
/// voice failures are enough: seen `successes: 1, failures: 2, retired: true`). The flow is
/// slotted ("search {1}"), so this works for any "Search …" the room says. The owner's next
/// typed "Search lamps" is
/// decided by the model from scratch; the learned steps are gone.
///
/// Should: a task that is not the owner's words changes nothing the owner's later tasks
/// rely on — no flow result counted (successes either: they keep a flow alive and refresh
/// `last_used_ms`, which decides eviction), as d79dd37 already did for `upsert`.
///
/// Why: the fix commit says "its result count on a flow it replayed is still kept"; this
/// is what that allows anyone the microphone hears to do to the owner's memory.
#[test]
fn f5_voice_tasks_can_retire_the_owners_learned_flow() {
    let mut memory = owner_taught();
    for i in 0..3 {
        let r = run_spec(
            config(),
            TaskSpec::new(format!("v{i}"), "Search lamps").from_origin(TaskOrigin::Voice),
            memory,
            Phone::still(home_screen()),
            Script::model(&[r#"{"action":"fail","reason":"cannot"}"#]),
        );
        assert_ne!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
        memory = r.memory;
    }
    let flow = memory.flows.flows().first().cloned().expect("the flow");
    assert!(
        memory.flows.find("Search lamps").is_some(),
        "the owner's flow was retired by voice tasks: {flow:?}"
    );
    // The owner's own task still replays what they taught.
    let later = run(
        config(),
        "Search lamps",
        memory,
        search_phone(),
        Script::model(&[r#"{"action":"done","summary":"ok"}"#]).decisions(&[true, true]),
    );
    assert!(typed(&later, "lamps"), "{:?}", later.acts);
}

/// A voice task with the same words replays the owner's flow, but its typing is asked for
/// on the phone: with every question declined, nothing is typed.
#[test]
fn sound_a_voice_replay_of_the_owners_flow_asks_before_typing() {
    let memory = owner_taught();
    let r = run_spec(
        config(),
        TaskSpec::new("v1", "Search lamps").from_origin(TaskOrigin::Voice),
        memory,
        search_phone(),
        Script::model(&[r#"{"action":"done","summary":"ok"}"#]).decisions(&[]),
    );
    assert!(!typed(&r, "lamps"), "{:?}", r.acts);
    assert!(!r.confirms.is_empty(), "nothing was asked: {:?}", r.acts);
}

/// The owner's flow is slotted ("search {1}"): a voice task "Search knives and forks" fills
/// the slot from heard words, and that typing is asked for too.
#[test]
fn sound_a_voice_replay_with_a_heard_slot_value_asks_before_typing() {
    let memory = owner_taught();
    let r = run_spec(
        config(),
        TaskSpec::new("v1", "Search knives and forks").from_origin(TaskOrigin::Voice),
        memory,
        search_phone(),
        Script::model(&[r#"{"action":"done","summary":"ok"}"#]).decisions(&[]),
    );
    assert!(!typed(&r, "knives"), "{:?}", r.acts);
    assert!(!r.confirms.is_empty(), "nothing was asked: {:?}", r.acts);
}
