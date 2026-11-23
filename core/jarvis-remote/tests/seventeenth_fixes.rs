//! Seventeenth review (remote): what a paired device receives, per permission, for every
//! event kind. A device without `see_screen_text` gets nothing the model chooses in detail:
//! that a task runs (once, and again only when it starts or stops waiting for an unlock),
//! that it waits for the owner (with the question's category), and how it ended (status).
//! The Live view (captions and the delta stream) has its own permission, `watch_screen`
//! (`viewer_stream.rs::without_the_permission_start_is_denied_and_nothing_is_sent`); no
//! event carries an `ask_user` question to a device (`LocalEvent` has none).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_proto::{Action, OutcomeStatus, Phase, Progress, RiskCategory};
use jarvis_remote::{ClientMessage, LocalEvent, RemotePermissions, ServerMessage};

const SECRET: &str = "482913";

fn rig_with(p: Option<RemotePermissions>) -> (Rig, Link) {
    let mut rig = Rig::new();
    let (mut link, creds) = rig.pair("Mac");
    if let Some(p) = p {
        rig.set_permissions(&mut link, &creds, p);
    }
    rig.request(&mut link, ClientMessage::Subscribe);
    (rig, link)
}

fn progress(step: u32, waiting: bool, hidden: bool) -> LocalEvent {
    LocalEvent::Progress {
        progress: Progress {
            step,
            phase: if waiting {
                Phase::WaitingForUnlock
            } else {
                Phase::Acting
            },
            last_action: Some(Action::OpenApp {
                app: "com.example.bank".into(),
            }),
            app: Some("com.example.bank".into()),
            waiting_for_unlock: waiting,
            screen_hidden: hidden,
            ..Progress::default()
        },
        screen_text: Some(format!("Balance 4210 code {SECRET} Done")),
    }
}

/// Every event kind, for each permission level; the fields each device gets are listed.
#[test]
fn per_permission_matrix() {
    let progress_only = RemotePermissions::NEW_DEVICE;
    let text = RemotePermissions {
        see_screen_text: true,
        ..RemotePermissions::NEW_DEVICE
    };
    let none = RemotePermissions {
        start_tasks: true,
        ..RemotePermissions::NONE
    };
    for (name, perms) in [
        ("none", none),
        ("see_progress", progress_only),
        ("see_screen_text", text),
    ] {
        let (mut rig, mut link) = rig_with(Some(perms));
        let mut got: Vec<ServerMessage> = Vec::new();
        got.extend(rig.local_to(&mut link, started(None)));
        let started_n = got.len();
        // Three ordinary steps, a wait for an unlock, a step after it, a step in Jarvis's
        // own app (hidden).
        for ev in [
            progress(1, false, false),
            progress(2, false, false),
            progress(3, false, false),
            progress(4, true, false),
            progress(5, false, false),
            progress(6, false, true),
        ] {
            got.extend(rig.local_to(&mut link, ev));
        }
        let progresses: Vec<&ServerMessage> = got
            .iter()
            .filter(|m| matches!(m, ServerMessage::Progress { .. }))
            .collect();
        let confirm = rig.local_to(
            &mut link,
            confirm_event(
                RiskCategory::SendMessage,
                &format!("Type \"{SECRET}\" into Code in Bank"),
            ),
        );
        let fin = rig.local_to(
            &mut link,
            finished(
                None,
                OutcomeStatus::Failed,
                &format!("The code was {SECRET}"),
            ),
        );
        match name {
            "none" => {
                // No progress permission: not subscribed; nothing but nothing.
                assert!(progresses.is_empty(), "{got:?}");
                assert!(confirm.is_empty() && fin.is_empty(), "{confirm:?} {fin:?}");
                let _ = started_n;
            }
            "see_progress" => {
                // Running; waiting for an unlock; running again — no step, app, action or
                // text, and not one message per step.
                assert_eq!(
                    progresses,
                    vec![
                        &ServerMessage::Progress {
                            step: 0,
                            phase: Phase::Acting,
                            app: None,
                            last_action: None,
                            screen_text: None,
                            waiting_for_unlock: false
                        },
                        &ServerMessage::Progress {
                            step: 0,
                            phase: Phase::WaitingForUnlock,
                            app: None,
                            last_action: None,
                            screen_text: None,
                            waiting_for_unlock: true
                        },
                        &ServerMessage::Progress {
                            step: 0,
                            phase: Phase::Acting,
                            app: None,
                            last_action: None,
                            screen_text: None,
                            waiting_for_unlock: false
                        },
                    ]
                );
                // The question's category, never its text.
                let [ServerMessage::ConfirmPending {
                    category, summary, ..
                }] = confirm.as_slice()
                else {
                    panic!("{confirm:?}")
                };
                assert_eq!(*category, RiskCategory::SendMessage);
                assert!(
                    !summary.contains(SECRET) && !summary.contains("Bank"),
                    "{summary}"
                );
                // How it ended: the status, a generic summary.
                let [ServerMessage::Finished { status, summary }] = fin.as_slice() else {
                    panic!("{fin:?}")
                };
                assert_eq!(*status, OutcomeStatus::Failed);
                assert!(
                    !summary.contains(SECRET) && !summary.contains("code"),
                    "{summary}"
                );
            }
            _ => {
                // The owner granted the detail: every progress, with step, app and action;
                // the screen text redacted; none for the wait or Jarvis's own app.
                assert_eq!(progresses.len(), 6, "{progresses:?}");
                for (k, m) in progresses.iter().enumerate() {
                    let ServerMessage::Progress {
                        step,
                        app,
                        last_action,
                        screen_text,
                        ..
                    } = m
                    else {
                        unreachable!()
                    };
                    assert_eq!(*step, k as u32 + 1);
                    assert_eq!(app.as_deref(), Some("com.example.bank"));
                    assert_eq!(last_action.as_deref(), Some("open_app"));
                    match k {
                        3 | 5 => assert!(screen_text.is_none(), "{k}: {screen_text:?}"),
                        _ => {
                            let t = screen_text.as_deref().unwrap();
                            assert!(!t.contains(SECRET) && t.contains("Done"), "{t}");
                        }
                    }
                }
                let [ServerMessage::ConfirmPending { summary, .. }] = confirm.as_slice() else {
                    panic!("{confirm:?}")
                };
                assert!(summary.contains("Bank"), "{summary}");
                let [ServerMessage::Finished { summary, .. }] = fin.as_slice() else {
                    panic!("{fin:?}")
                };
                assert!(
                    summary.contains("code") && !summary.contains(SECRET),
                    "{summary}"
                );
            }
        }
    }
}

/// A screen of Jarvis's own app (not driven: a stub for the model) never reaches a device,
/// whatever the shell passes.
#[test]
fn own_app_screen_text_is_never_sent() {
    let (mut rig, mut link) = rig_with(Some(RemotePermissions {
        see_screen_text: true,
        ..RemotePermissions::NEW_DEVICE
    }));
    rig.local_to(&mut link, started(None));
    let msgs = rig.local_to(&mut link, progress(1, false, true));
    assert!(matches!(
        msgs.as_slice(),
        [ServerMessage::Progress {
            screen_text: None,
            ..
        }]
    ));
}

/// A new task starts the coarse state afresh: a device hears "running" again.
#[test]
fn each_task_announces_running_once() {
    let (mut rig, mut link) = rig_with(None);
    for _ in 0..2 {
        rig.local_to(&mut link, started(None));
        let a = rig.local_to(&mut link, progress(1, false, false));
        let b = rig.local_to(&mut link, progress(2, false, false));
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(b.is_empty(), "{b:?}");
        rig.local_to(&mut link, finished(None, OutcomeStatus::Success, "ok"));
    }
}
