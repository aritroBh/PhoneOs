//! The FFI session object driven from Rust: the same end-to-end scenario as the Swift smoke
//! test, memory handling, lifecycle, runner-text parsing and concurrency.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::sync::{Arc, Barrier};
use std::thread;

use common::*;
use jarvis_core_ffi::*;

#[test]
fn end_to_end_then_replay_with_zero_model_calls() {
    let first = drive("");
    let outcome = first.outcome.clone().unwrap();
    assert_eq!(
        outcome.status,
        OutcomeStatus::Success,
        "{}",
        outcome.summary
    );
    assert_eq!(outcome.summary, "Wi-Fi is off");
    assert_eq!(first.final_page, Some(Page::WifiOff));
    assert_eq!(first.infers, 3);
    assert_eq!(first.acts, 2);
    assert!(first.typed_observations >= 1 && first.text_observations >= 2);
    // First use of Settings, then the setting itself.
    assert_eq!(first.confirms.len(), 2, "{:?}", first.confirms);
    assert_eq!(first.confirms[0].category, RiskCategory::FirstUse);
    assert_eq!(
        first.confirms[0].summary,
        "Let Jarvis act in Settings (com.apple.Preferences)?"
    );
    assert_eq!(first.confirms[1].category, RiskCategory::SystemSetting);
    assert_eq!(first.confirms[1].app, SETTINGS);

    let memory: serde_json::Value = serde_json::from_str(&first.memory_json).unwrap();
    assert_eq!(memory["flows"]["flows"].as_array().unwrap().len(), 1);
    assert_eq!(memory["approved_apps"], serde_json::json!([SETTINGS]));

    let second = drive(&first.memory_json);
    let outcome = second.outcome.clone().unwrap();
    assert_eq!(
        outcome.status,
        OutcomeStatus::Success,
        "{}",
        outcome.summary
    );
    assert_eq!(second.infers, 0, "replay must not call the model");
    assert_eq!(second.acts, 2);
    // The approval was remembered; the replayed toggle passes the same risk gate.
    assert_eq!(second.confirms.len(), 1, "replay passes the same gate");
    assert_eq!(second.confirms[0].category, RiskCategory::SystemSetting);
    assert_eq!(second.final_page, Some(Page::WifiOff));
    let memory: serde_json::Value = serde_json::from_str(&second.memory_json).unwrap();
    assert_eq!(memory["flows"]["flows"][0]["successes"], 2);
}

#[test]
fn invalid_memory_json_throws_instead_of_panicking() {
    for bad in ["{", "[1,2]", "not json", r#"{"schema_version": 999}"#] {
        let err = JarvisSession::new(config(), task(), bad.to_owned())
            .err()
            .unwrap_or_else(|| panic!("{bad} was accepted"));
        assert!(
            matches!(err, JarvisError::InvalidMemory { .. }),
            "{bad}: {err:?}"
        );
    }
    // Empty and whitespace-only mean fresh memory.
    assert!(JarvisSession::new(config(), task(), String::new()).is_ok());
    assert!(JarvisSession::new(config(), task(), " \n".to_owned()).is_ok());
}

#[test]
fn finish_is_idempotent_and_step_after_finish_is_sticky() {
    let session = JarvisSession::new(config(), task(), String::new()).unwrap();
    assert!(matches!(
        session.step(1, Event::Start),
        Effect::Observe { screenshot: false }
    ));
    let cancelled = session.step(2, Event::Cancel);
    let Effect::Finished { outcome } = cancelled.clone() else {
        panic!("{cancelled:?}")
    };
    assert_eq!(outcome.status, OutcomeStatus::Cancelled);
    let a = session.finish().unwrap();
    let b = session.finish().unwrap();
    assert_eq!(a, b);
    let log: serde_json::Value = serde_json::from_str(&a).unwrap();
    assert_eq!(
        log["log"]["entries"].as_array().map(Vec::len),
        Some(1),
        "{a}"
    );
    assert_eq!(session.step(3, Event::Start), cancelled);
    assert_eq!(session.progress().unwrap().phase, Phase::Finished);
}

#[test]
fn finish_before_the_session_ended_returns_memory_and_step_reports_protocol_error() {
    let session = JarvisSession::new(config(), task(), String::new()).unwrap();
    session.step(1, Event::Start);
    let json = session.finish().unwrap();
    assert!(jarvis_memory::MemoryStore::from_json(&json).is_ok());
    let Effect::Finished { outcome } = session.step(2, Event::Cancel) else {
        panic!()
    };
    assert_eq!(outcome.status, OutcomeStatus::Failed);
    assert!(
        outcome.summary.contains("after finish"),
        "{}",
        outcome.summary
    );
}

#[test]
fn progress_reports_phase_step_and_app() {
    let session = JarvisSession::new(config(), task(), String::new()).unwrap();
    assert_eq!(session.progress().unwrap().phase, Phase::Idle);
    session.step(1, Event::Start);
    assert_eq!(session.progress().unwrap().phase, Phase::Observing);
    let effect = session.step(
        2,
        Event::Observed {
            observation: ScreenObservation {
                screen: settings_screen(),
                screenshot: None,
            },
        },
    );
    assert!(matches!(effect, Effect::Infer { .. }), "{effect:?}");
    let p = session.progress().unwrap();
    assert_eq!(p.phase, Phase::Thinking);
    assert_eq!(p.app.as_deref(), Some(SETTINGS));
    assert_eq!(p.step, 0);
}

#[test]
fn unparseable_runner_text_is_observed_as_an_empty_screen() {
    let session = JarvisSession::new(config(), task(), String::new()).unwrap();
    session.step(1, Event::Start);
    let effect = session.step(
        2,
        Event::ObservedRunnerText {
            snapshot: snapshot("garbage\n\n<<not a tree>>".to_owned()),
        },
    );
    let Effect::Infer { request } = effect else {
        panic!("{effect:?}")
    };
    assert!(request.user.contains(SETTINGS), "{}", request.user);
    assert!(
        !request.user.contains("[1]"),
        "no elements: {}",
        request.user
    );
    let issue = session.last_observation_issue().unwrap().unwrap();
    assert!(issue.contains("empty screen"), "{issue}");
    assert_eq!(session.progress().unwrap().app.as_deref(), Some(SETTINGS));
}

#[test]
fn unparseable_runner_text_with_vision_asks_for_a_screenshot() {
    let mut c = config();
    c.model.supports_vision = true;
    let session = JarvisSession::new(c, task(), String::new()).unwrap();
    session.step(1, Event::Start);
    let effect = session.step(
        2,
        Event::ObservedRunnerText {
            snapshot: snapshot(String::new()),
        },
    );
    assert_eq!(effect, Effect::Observe { screenshot: true });
}

#[test]
fn runner_text_in_a_denied_app_still_blocks() {
    let session = JarvisSession::new(config(), task(), String::new()).unwrap();
    session.step(1, Event::Start);
    let mut snap = snapshot(String::new());
    snap.app_bundle_id = "com.apple.Passwords".to_owned();
    let Effect::Finished { outcome } =
        session.step(2, Event::ObservedRunnerText { snapshot: snap })
    else {
        panic!()
    };
    assert_eq!(outcome.status, OutcomeStatus::Blocked);
}

#[test]
fn parse_runner_text_to_screen_parses_and_rejects() {
    let screen = api::parse_runner_text_to_screen(snapshot(wifi_text(true))).unwrap();
    assert_eq!(screen.app_bundle_id, SETTINGS);
    assert_eq!(screen.root.role, Role::Application);
    let switch = &screen.root.children[0].children[1].children[0].children[0];
    assert_eq!(switch.role, Role::Switch);
    assert_eq!(switch.value.as_deref(), Some("1"));
    assert_eq!(switch.frame, rect(312.0, 166.0, 51.0, 31.0));
    let err = api::parse_runner_text_to_screen(snapshot("nope".to_owned())).unwrap_err();
    assert!(matches!(err, JarvisError::RunnerText { .. }), "{err:?}");
}

#[test]
fn free_functions_return_the_contract() {
    let schema = api::action_json_schema().unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&schema).is_ok());
    assert_eq!(schema, jarvis_agent::action_json_schema());
    assert!(api::action_gbnf().unwrap().contains("root ::="));
    assert!(api::system_prompt().unwrap().contains("Jarvis"));
    assert_eq!(api::core_version(), env!("CARGO_PKG_VERSION"));
    assert_eq!(
        api::default_policy().unwrap(),
        jarvis_proto::Policy::default().into()
    );
}

#[test]
fn two_threads_stepping_one_session_are_serialized() {
    // Both threads send Start: exactly one is accepted, the other is a protocol error.
    for _ in 0..20 {
        let session = JarvisSession::new(config(), task(), String::new()).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|i| {
                let s = Arc::clone(&session);
                let b = Arc::clone(&barrier);
                thread::spawn(move || {
                    b.wait();
                    s.step(10 + i, Event::Start)
                })
            })
            .collect();
        let effects: Vec<Effect> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let observes = effects
            .iter()
            .filter(|e| matches!(e, Effect::Observe { .. }))
            .count();
        assert_eq!(observes, 1, "{effects:?}");
        let failed = effects.iter().any(|e| {
            matches!(e, Effect::Finished { outcome } if outcome.status == OutcomeStatus::Failed
                && outcome.summary.contains("protocol error"))
        });
        assert!(failed, "{effects:?}");
    }
}

#[test]
fn a_driver_and_a_progress_poller_on_two_threads_do_not_interfere() {
    let session = JarvisSession::new(config(), task(), String::new()).unwrap();
    let poller = {
        let s = Arc::clone(&session);
        thread::spawn(move || {
            let mut polls = 0u32;
            loop {
                let p = s.progress().unwrap();
                polls += 1;
                if p.phase == Phase::Finished {
                    return polls;
                }
                thread::yield_now();
            }
        })
    };
    // Drive the first leg by hand on this thread.
    let mut now = 1;
    let mut effect = session.step(now, Event::Start);
    let mut script = model_script();
    let mut page = Page::Settings;
    while !matches!(effect, Effect::Finished { .. }) {
        now += 1;
        let event = match effect {
            Effect::Observe { .. } => match page {
                Page::Settings => Event::Observed {
                    observation: ScreenObservation {
                        screen: settings_screen(),
                        screenshot: None,
                    },
                },
                _ => Event::ObservedRunnerText {
                    snapshot: snapshot(wifi_text(page == Page::WifiOn)),
                },
            },
            Effect::Infer { request } => {
                let text = match script.pop_front().flatten() {
                    Some((role, label)) => format!(
                        r#"{{"action":"tap","element":{}}}"#,
                        index_in_prompt(&request.user, role, label).unwrap()
                    ),
                    None => r#"{"action":"done","summary":"ok"}"#.to_owned(),
                };
                Event::ModelOutput {
                    output: ModelOutput { text },
                }
            }
            Effect::Act { .. } => {
                page = match page {
                    Page::Settings => Page::WifiOn,
                    _ => Page::WifiOff,
                };
                Event::ActResult {
                    result: RunnerResult {
                        ok: true,
                        error: None,
                    },
                }
            }
            Effect::Confirm { .. } => Event::UserDecision { approved: true },
            other => panic!("{other:?}"),
        };
        effect = session.step(now, event);
    }
    let Effect::Finished { outcome } = effect else {
        unreachable!()
    };
    assert_eq!(
        outcome.status,
        OutcomeStatus::Success,
        "{}",
        outcome.summary
    );
    assert!(poller.join().unwrap() >= 1);
    assert!(session.finish().is_ok());
}

#[test]
fn step_by_step_and_first_use_settings_cross_the_bridge() {
    let defaults = api::default_policy().unwrap();
    assert!(defaults.first_use_approval);
    assert_eq!(defaults.mode, PolicyMode::Guarded);
    assert!(defaults.app_rules.is_empty() && defaults.deny_categories.is_empty());
    for c in [
        RiskCategory::Unverified,
        RiskCategory::Call,
        RiskCategory::FirstUse,
    ] {
        assert!(defaults.confirm.contains(&c), "{c:?}");
    }

    // StepByStep without first-use: the plain row tap asks too (category None).
    let mut c = config();
    c.policy.mode = PolicyMode::StepByStep;
    c.policy.first_use_approval = false;
    let run = drive_with(c, "", |_| true);
    assert_eq!(run.outcome.unwrap().status, OutcomeStatus::Success);
    let cats: Vec<RiskCategory> = run.confirms.iter().map(|r| r.category).collect();
    assert_eq!(cats, vec![RiskCategory::None, RiskCategory::SystemSetting]);

    // A Trusted rule for Settings: no confirmation at all.
    let mut c = config();
    c.policy.app_rules = vec![AppRule {
        bundle_id_prefix: SETTINGS.to_owned(),
        level: TrustLevel::Trusted,
    }];
    let run = drive_with(c, "", |_| false);
    assert_eq!(run.outcome.unwrap().status, OutcomeStatus::Success);
    assert!(run.confirms.is_empty(), "{:?}", run.confirms);
}

#[test]
fn a_declined_first_use_blocks_through_the_bridge() {
    let session = JarvisSession::new(config(), task(), String::new()).unwrap();
    assert!(matches!(
        session.step(1, Event::Start),
        Effect::Observe { .. }
    ));
    let observed = || Event::Observed {
        observation: ScreenObservation {
            screen: settings_screen(),
            screenshot: None,
        },
    };
    let tap_row = |user: &str| {
        let i = index_in_prompt(user, "Cell", "Wi-Fi, HomeNet").unwrap();
        Event::ModelOutput {
            output: ModelOutput {
                text: format!(r#"{{"action":"tap","element":{i}}}"#),
            },
        }
    };
    let Effect::Infer { request } = session.step(2, observed()) else {
        panic!("expected infer")
    };
    let Effect::Confirm { request: ask } = session.step(3, tap_row(&request.user)) else {
        panic!("expected confirm")
    };
    assert_eq!(ask.category, RiskCategory::FirstUse);
    assert_eq!(
        ask.summary,
        "Let Jarvis act in Settings (com.apple.Preferences)?"
    );
    let Effect::Infer { request } = session.step(4, Event::UserDecision { approved: false }) else {
        panic!("expected infer after the decline")
    };
    let progress = session.progress().unwrap();
    assert_eq!(progress.model_calls, 2);
    assert_eq!(progress.parse_retries, 0);
    let Effect::Finished { outcome } = session.step(5, tap_row(&request.user)) else {
        panic!("expected finished")
    };
    assert_eq!(
        outcome.status,
        OutcomeStatus::Blocked,
        "{}",
        outcome.summary
    );
    let memory: serde_json::Value = serde_json::from_str(&session.finish().unwrap()).unwrap();
    assert_eq!(memory["approved_apps"], serde_json::json!([]));
}
