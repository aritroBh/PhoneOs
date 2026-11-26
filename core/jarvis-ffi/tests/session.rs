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
                device_locked: false,
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
fn redact_screen_text_is_the_cores_redaction() {
    for text in [
        "Your verification code is 482913.",
        "Apple ID Code: 771305. Don't share it",
        "Visa 4111 1111 1111 1111",
        "Card: 5555 5555 5555 4444 exp 12/29",
        "IBAN: GB82 WEST 1234 5698 7654 32",
        "DE89 3704 0044 0532 0130 00",
        "key sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N here",
        "AKIAIOSFODNN7EXAMPLE",
        "Wi-Fi password: Tr0ub4dor&3",
        "a\u{200B}b",
    ] {
        let shown = api::redact_screen_text(text.to_owned()).unwrap();
        assert_eq!(shown, jarvis_safety::redact(text), "{text}");
        assert_ne!(shown, text, "{text}");
    }
    for text in [
        "Send",
        "Wi-Fi",
        "Call me at +1 415-555-2671",
        "Order #12345 has shipped",
        "",
    ] {
        assert_eq!(api::redact_screen_text(text.to_owned()).unwrap(), text);
    }
}

#[test]
fn field_value_is_hidden_follows_the_core() {
    let hidden = |role: Role,
                  label: Option<&str>,
                  placeholder: Option<&str>,
                  id: Option<&str>,
                  secure: bool| {
        api::field_value_is_hidden(
            role,
            label.map(str::to_owned),
            placeholder.map(str::to_owned),
            id.map(str::to_owned),
            secure,
        )
        .unwrap()
    };
    // Secure by flag or role.
    assert!(hidden(Role::TextField, Some("Name"), None, None, true));
    assert!(hidden(Role::SecureTextField, None, None, None, false));
    // Credential-like by label, placeholder or identifier.
    for label in [
        "Verification code",
        "Card number",
        "CVV",
        "IBAN",
        "Password",
        "PIN",
    ] {
        assert!(
            hidden(Role::TextField, Some(label), None, None, false),
            "{label}"
        );
        assert!(
            hidden(Role::TextField, None, Some(label), None, false),
            "{label}"
        );
        assert!(
            hidden(Role::TextField, None, None, Some(label), false),
            "{label}"
        );
    }
    assert!(hidden(
        Role::TextField,
        Some("Note"),
        Some("Card number"),
        None,
        false
    ));
    // Plain fields and non-entry roles keep their value.
    assert!(!hidden(
        Role::TextField,
        Some("Email"),
        Some("you@example.com"),
        None,
        false
    ));
    assert!(!hidden(
        Role::SearchField,
        Some("Search"),
        None,
        None,
        false
    ));
    assert!(!hidden(Role::TextField, None, None, None, false));
    assert!(!hidden(Role::Button, Some("Password"), None, None, false));
    // The same answer as the core's own test on an element.
    let el = jarvis_proto::Element {
        index: 1,
        role: jarvis_proto::Role::TextField,
        label: "One-time code".to_owned(),
        value: Some("1".to_owned()),
        placeholder: None,
        frame: jarvis_proto::Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        enabled: true,
        focused: false,
        selected: false,
        secure: false,
        fingerprint: 0,
    };
    assert_eq!(
        hidden(Role::TextField, Some("One-time code"), None, None, false),
        jarvis_safety::credential_like_field(&el)
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
                        device_locked: false,
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
                        failure: None,
                    },
                }
            }
            Effect::Confirm { .. } => Event::UserDecision {
                approved: true,
                by_phone: true,
            },
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

    // A Trusted rule for Settings: nothing is asked on the typed screen. Sixth review: the
    // Wi-Fi page here is read from runner text, and a text screen always confirms whatever
    // the trust level, so the switch tap there is asked (was: no confirmation at all;
    // deliberate change).
    let mut c = config();
    c.policy.app_rules = vec![AppRule {
        bundle_id_prefix: SETTINGS.to_owned(),
        level: TrustLevel::Trusted,
        name: Some("Settings".to_owned()),
    }];
    let run = drive_with(c, "", |_| true);
    assert_eq!(run.outcome.unwrap().status, OutcomeStatus::Success);
    assert_eq!(run.confirms.len(), 1, "{:?}", run.confirms);
    assert!(run.confirms[0]
        .summary
        .starts_with(jarvis_safety::UNVERIFIED_SCREEN_WARNING));
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
            device_locked: false,
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
    let Effect::Infer { request } = session.step(
        4,
        Event::UserDecision {
            approved: false,
            by_phone: true,
        },
    ) else {
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

#[test]
fn a_locked_phone_waits_resumes_by_observing_and_can_time_out() {
    let mut c = config();
    c.policy.max_locked_wait_ms = 120_000;
    let session = JarvisSession::new(c.clone(), task(), String::new()).unwrap();
    session.step(1, Event::Start);
    // The device path reports the lock on the runner snapshot.
    let mut locked = snapshot(String::new());
    locked.device_locked = true;
    let effect = session.step(2, Event::ObservedRunnerText { snapshot: locked });
    assert!(
        matches!(&effect, Effect::AwaitUnlock { waited_ms: 0, reason } if reason.contains("Unlock")),
        "{effect:?}"
    );
    assert_eq!(
        session.last_observation_issue().unwrap(),
        None,
        "an empty lock-screen tree is not a parse issue"
    );
    let p = session.progress().unwrap();
    assert!(p.waiting_for_unlock);
    assert_eq!(p.phase, Phase::WaitingForUnlock);
    assert!(matches!(
        session.step(60_002, Event::Tick),
        Effect::AwaitUnlock {
            waited_ms: 60_000,
            ..
        }
    ));
    assert_eq!(session.progress().unwrap().locked_wait_ms, 60_000);
    assert!(matches!(
        session.step(70_000, Event::DeviceUnlocked),
        Effect::Observe { screenshot: false }
    ));
    // An observation that failed is asked for again.
    assert!(matches!(
        session.step(
            70_001,
            Event::ObserveFailed {
                reason: "timeout".to_owned()
            }
        ),
        Effect::Observe { screenshot: false }
    ));
    let effect = session.step(
        70_002,
        Event::Observed {
            observation: ScreenObservation {
                screen: settings_screen(),
                screenshot: None,
                device_locked: false,
            },
        },
    );
    assert!(matches!(effect, Effect::Infer { .. }), "{effect:?}");
    assert_eq!(session.progress().unwrap().locked_total_ms, 69_998);

    // A command refused as locked, then the wait runs out.
    let session = JarvisSession::new(c, task(), String::new()).unwrap();
    session.step(1, Event::Start);
    let Effect::Infer { request } = session.step(
        2,
        Event::Observed {
            observation: ScreenObservation {
                screen: settings_screen(),
                screenshot: None,
                device_locked: false,
            },
        },
    ) else {
        panic!()
    };
    let i = index_in_prompt(&request.user, "Cell", "Wi-Fi, HomeNet").unwrap();
    let effect = session.step(
        3,
        Event::ModelOutput {
            output: ModelOutput {
                text: format!(r#"{{"action":"tap","element":{i}}}"#),
            },
        },
    );
    // First use of Settings: approve, a fresh look (observe-after-approval), then the tap
    // is refused.
    assert!(matches!(effect, Effect::Confirm { .. }), "{effect:?}");
    assert!(matches!(
        session.step(
            4,
            Event::UserDecision {
                approved: true,
                by_phone: true
            }
        ),
        Effect::Observe { .. }
    ));
    assert!(matches!(
        session.step(
            4,
            Event::Observed {
                observation: ScreenObservation {
                    screen: settings_screen(),
                    screenshot: None,
                    device_locked: false,
                },
            },
        ),
        Effect::Act { .. }
    ));
    let refused = RunnerResult {
        ok: false,
        error: Some("screen_locked".to_owned()),
        failure: Some(RunnerFailure::DeviceLocked),
    };
    assert!(matches!(
        session.step(5, Event::ActResult { result: refused }),
        Effect::AwaitUnlock { .. }
    ));
    let Effect::Finished { outcome } = session.step(200_000, Event::Tick) else {
        panic!()
    };
    assert_eq!(outcome.status, OutcomeStatus::Interrupted);
    assert!(
        outcome.summary.contains("stayed locked"),
        "{}",
        outcome.summary
    );
    let memory = session.finish().unwrap();
    assert!(memory.contains(r#""outcome":"interrupted""#), "{memory}");
    assert!(memory.contains(r#""lock_waits":1"#), "{memory}");
}

/// Review pass 29 (depth): a tree is at most `jarvis_runner::MAX_TREE_DEPTH` (160) deep when
/// it reaches the FFI (the runner wire refuses deeper ones; the in-runner copy and
/// `UiTreeBuilder` stop at 200 levels; the deepest real capture is 35). At that depth the
/// session's step — the FFI conversion, the compaction, the clones — runs on a 512 KB
/// thread stack, an iOS secondary thread's.
#[test]
fn a_tree_at_the_wire_depth_cap_is_stepped_on_a_small_stack() {
    let worker = thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let full = rect(0.0, 0.0, 393.0, 852.0);
            let mut node = ui(
                Role::Button,
                Some("Deep"),
                rect(16.0, 100.0, 200.0, 44.0),
                true,
                vec![],
            );
            for _ in 0..jarvis_runner::MAX_TREE_DEPTH - 2 {
                node = ui(
                    Role::Other {
                        name: "Other".into(),
                    },
                    None,
                    full,
                    false,
                    vec![node],
                );
            }
            let root = ui(Role::Application, Some("Settings"), full, false, vec![node]);
            let mut screen = settings_screen();
            screen.root = root;
            let session = JarvisSession::new(config(), task(), String::new()).unwrap();
            session.step(1, Event::Start);
            let effect = session.step(
                2,
                Event::Observed {
                    observation: ScreenObservation {
                        screen,
                        screenshot: None,
                        device_locked: false,
                    },
                },
            );
            matches!(effect, Effect::Infer { .. })
        })
        .unwrap();
    assert!(worker.join().unwrap());
}

/// Review pass 31 (L1): the FFI conversion is an intake: a tree from the shell deeper than
/// `jarvis_proto::MAX_TREE_DEPTH` is cut there and the screen flagged (even when the shell
/// did not flag it), never an error and never a silent cut; at the bound nothing is flagged.
#[test]
fn the_ffi_conversion_cuts_a_deep_tree_and_flags_it() {
    let deep = |levels: usize| {
        let full = rect(0.0, 0.0, 393.0, 852.0);
        let mut node = ui(
            Role::Button,
            Some("Deep"),
            rect(16.0, 100.0, 200.0, 44.0),
            true,
            vec![],
        );
        for _ in 0..levels - 2 {
            node = ui(
                Role::Other {
                    name: "Other".into(),
                },
                None,
                full,
                false,
                vec![node],
            );
        }
        let mut screen = settings_screen();
        screen.root = ui(Role::Application, Some("Settings"), full, false, vec![node]);
        screen
    };
    let levels = |n: &jarvis_proto::UiNode| {
        let mut best = 0;
        let mut stack = vec![(n, 1usize)];
        while let Some((e, d)) = stack.pop() {
            best = best.max(d);
            stack.extend(e.children.iter().map(|c| (c, d + 1)));
        }
        best
    };
    let at: jarvis_proto::Screen = deep(jarvis_proto::MAX_TREE_DEPTH).into();
    assert!(!at.tree_cut);
    assert_eq!(levels(&at.root), jarvis_proto::MAX_TREE_DEPTH);
    let over: jarvis_proto::Screen = deep(300).into();
    assert!(over.tree_cut);
    assert_eq!(levels(&over.root), jarvis_proto::MAX_TREE_DEPTH);
    let mut flagged = deep(3);
    flagged.tree_cut = true;
    let flagged: jarvis_proto::Screen = flagged.into();
    assert!(flagged.tree_cut, "the shell's flag is carried");
}
