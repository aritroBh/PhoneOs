//! Seventeenth review (remote): what a paired device without screen text learns.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::{Action, Phase, Progress};
use jarvis_remote::{ClientMessage, LocalEvent, RemotePermissions, ServerMessage};

fn launch(step: u32, app: &str) -> LocalEvent {
    LocalEvent::Progress {
        progress: Progress {
            step,
            phase: Phase::Acting,
            last_action: Some(Action::OpenApp { app: app.into() }),
            app: Some(app.into()),
            ..Progress::default()
        },
        screen_text: None,
    }
}

/// A device with the default permissions (start tasks, see progress; no screen text)
/// starts a task; the session reads Messages and then opens apps whose order spells the
/// code (`probe17_remote_task_spells_the_code_by_app_launches` in jarvis-agent: no question).
/// Every progress names the app in front. SAFE: a device that may not see screen text gets
/// no model-chosen detail (no app, no action kind) after the task read something.
#[test]
fn probe17_progress_app_sequence_reaches_a_device_without_screen_text() {
    let mut rig = Rig::new();
    let (mut link, _creds) = rig.pair("Mallory");
    rig.request(&mut link, ClientMessage::Subscribe);
    rig.request(
        &mut link,
        ClientMessage::StartTask {
            text: "read my latest Messages code".into(),
        },
    );
    let spelled = [
        "com.apple.mobilenotes",
        "com.apple.Preferences",
        "com.example.acme",
        "com.example.shop",
        "com.apple.calculator",
        "com.apple.camera",
    ];
    let mut learned = Vec::new();
    for (i, app) in spelled.iter().enumerate() {
        let msgs = rig.local_to(&mut link, launch(i as u32 + 1, app));
        for m in msgs {
            if let ServerMessage::Progress { app: Some(a), .. } = m {
                learned.push(a);
            }
        }
    }
    let _ = RemotePermissions::NEW_DEVICE;
    assert!(
        learned.is_empty(),
        "a device without screen text learned the launch sequence: {learned:?}"
    );
}
