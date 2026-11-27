#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Voice break-it tests at the session level (1073262 + b3dbc41): a task the microphone
//! started without a touch (`TaskOrigin::Voice`) takes the non-owner branch. Everything
//! tried here held: `sound_…` tests only.

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{RunnerCommand, TaskOrigin, TaskSpec};

fn opened(r: &Run) -> bool {
    r.acts
        .iter()
        .any(|c| matches!(c, RunnerCommand::OpenUrl { .. }))
}

/// A link spoken in a voice request is not "the owner gave this link": the model opening
/// it asks (and the owner's "no" keeps it closed), where the same words typed open it.
#[test]
fn sound_a_link_in_a_voice_request_is_not_owner_given() {
    let task = "Open https://bank.com and check my balance";
    let script = || {
        Script::model(&[
            r#"{"action":"open_url","url":"https://bank.com"}"#,
            r#"{"action":"fail","reason":"stop"}"#,
        ])
    };
    let voice = run_spec(
        config(),
        TaskSpec::new("t", task).from_origin(TaskOrigin::Voice),
        MemoryStore::new(),
        Phone::still(home_screen()),
        script(),
    );
    assert!(!opened(&voice), "{:?}", voice.acts);
    assert!(!voice.confirms.is_empty(), "{:#?}", voice.confirms);
    let phone = run(
        config(),
        task,
        MemoryStore::new(),
        Phone::still(home_screen()),
        script(),
    );
    assert!(opened(&phone), "{:?} {:#?}", phone.acts, phone.confirms);
}

/// The voice task's text reaches the model only as the task (the prompt carries it like a
/// typed one) and the run is stopped like any other: nothing of the origin changes the
/// prompt the model is shown.
#[test]
fn sound_the_voice_text_is_only_the_task_in_the_prompt() {
    let task = "Turn off Wi-Fi";
    let script = || Script::model(&[r#"{"action":"fail","reason":"stop"}"#]);
    let voice = run_spec(
        config(),
        TaskSpec::new("t", task).from_origin(TaskOrigin::Voice),
        MemoryStore::new(),
        Phone::still(wifi_screen(true)),
        script(),
    );
    let phone = run(
        config(),
        task,
        MemoryStore::new(),
        Phone::still(wifi_screen(true)),
        script(),
    );
    assert_eq!(voice.all_prompt_text(), phone.all_prompt_text());
    assert_eq!(voice.all_prompt_text().matches(task).count(), 1);
}
