#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Voice break-it fixes at the session level: a task that is not the owner's words
//! (`TaskOrigin::Voice`, `TaskOrigin::Remote`) teaches nothing. What it did is never
//! replayed for the owner's later typed task with the same words; a typed task still
//! teaches.

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{RunnerCommand, TaskOrigin, TaskSpec};

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

fn search_lamps(origin: TaskOrigin) -> Run {
    let field = index_of(&search_screen(true, None), "Search");
    run_spec(
        config(),
        TaskSpec::new("t1", "Search lamps").from_origin(origin),
        MemoryStore::new(),
        search_phone(),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{field},"text":"lamps"}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true, true, true]),
    )
}

#[test]
fn a_task_that_is_not_the_owners_words_teaches_nothing() {
    for origin in [TaskOrigin::Voice, TaskOrigin::Remote] {
        let first = search_lamps(origin);
        // It did the work (the owner approved the typing on the phone)...
        assert!(typed(&first, "lamps"), "{origin:?}: {:?}", first.acts);
        // ...and nothing of it was learned.
        assert!(
            first.memory.flows.flows().is_empty(),
            "{origin:?}: {:#?}",
            first.memory.flows.flows()
        );
        // The owner's own later task with the same words replays nothing: the model
        // decides (here it is done at once, so nothing is typed).
        let later = run(
            config(),
            "Search lamps",
            first.memory,
            search_phone(),
            Script::model(&[r#"{"action":"done","summary":"ok"}"#]),
        );
        assert!(!typed(&later, "lamps"), "{origin:?}: {:?}", later.acts);
    }
}

#[test]
fn a_typed_task_still_teaches() {
    let first = search_lamps(TaskOrigin::Phone);
    assert!(typed(&first, "lamps"));
    assert!(first.memory.flows.find("Search lamps").is_some());
    let later = run(
        config(),
        "Search lamps",
        first.memory,
        search_phone(),
        Script::model(&[r#"{"action":"done","summary":"ok"}"#]),
    );
    assert!(
        typed(&later, "lamps"),
        "the learned flow was not replayed: {:?}",
        later.acts
    );
}
