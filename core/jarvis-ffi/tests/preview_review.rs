//! Review probes (approval preview): a real `JarvisSession` driven through the FFI, every
//! update forwarded to a `JarvisViewer` in the driver's order, and the picture the viewer
//! computes for each confirmation compared with what the session actually does after the
//! owner approves. Each test asserts the SAFE behaviour.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

mod common;

use common::*;
use jarvis_core_ffi::*;

const SHOP: &str = "com.example.shop";

fn node(role: Role, label: Option<&str>, frame: Rect, children: Vec<UiNode>) -> UiNode {
    ui(role, label, frame, true, children)
}

fn screen_of(children: Vec<UiNode>) -> Screen {
    let full = rect(0.0, 0.0, 393.0, 852.0);
    Screen {
        app_bundle_id: SHOP.to_owned(),
        app_name: Some("Shop".to_owned()),
        size: size(),
        root: node(
            Role::Application,
            Some("Shop"),
            full,
            vec![node(Role::Window, None, full, children)],
        ),
        keyboard_visible: false,
        orientation: Orientation::Portrait,
    }
}

fn observed(screen: Screen) -> Event {
    Event::Observed {
        observation: ScreenObservation {
            screen,
            screenshot: None,
            device_locked: false,
        },
    }
}

fn ok() -> RunnerResult {
    RunnerResult {
        ok: true,
        error: None,
        failure: None,
    }
}

fn locked() -> RunnerResult {
    RunnerResult {
        ok: false,
        error: Some("locked".to_owned()),
        failure: Some(RunnerFailure::DeviceLocked),
    }
}

/// Every-action supervision, no first-use question: every state-changing action asks.
fn step_by_step() -> SessionConfig {
    let mut c = api::default_session_config().unwrap();
    c.policy.mode = PolicyMode::StepByStep;
    c.policy.first_use_approval = false;
    c.known_apps = vec![AppInfo {
        bundle_id: SHOP.to_owned(),
        name: "Shop".to_owned(),
    }];
    c
}

#[derive(Debug)]
struct Asked {
    request: ConfirmRequest,
    target: Option<CoreApprovalTarget>,
    /// The runner commands sent after this approval, up to the next question.
    acts_after: Vec<RunnerCommand>,
}

#[derive(Debug, Default)]
struct Trace {
    asked: Vec<Asked>,
    acts: Vec<RunnerCommand>,
    outcome: Option<Outcome>,
}

/// Drives a session: `observe(n)` answers the n-th observation, `model(prompt, n)` the
/// n-th inference, `act(cmd, n)` the n-th runner command; every confirmation is approved.
fn drive(
    config: SessionConfig,
    mut observe: impl FnMut(usize) -> Event,
    mut model: impl FnMut(&str, usize) -> String,
    mut act: impl FnMut(&RunnerCommand, usize) -> RunnerResult,
) -> Trace {
    let session = JarvisSession::new(
        config,
        TaskSpec {
            id: "probe".to_owned(),
            text: "probe".to_owned(),
        },
        String::new(),
    )
    .unwrap();
    let viewer = JarvisViewer::new(CoreViewerConfig {
        max_steps: 200,
        frames_enabled: true,
        known_apps: vec![],
    })
    .unwrap();
    let feed = |u: CoreViewerUpdate| {
        viewer.feed(u).unwrap();
    };
    feed(CoreViewerUpdate::Started {
        task_id: "probe".to_owned(),
    });
    let mut trace = Trace::default();
    let (mut n_obs, mut n_inf, mut n_act) = (0, 0, 0);
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for number in 1..400u64 {
        let effect = session.step(now, event);
        now += 50;
        feed(CoreViewerUpdate::Effect {
            effect: effect.clone(),
            number,
        });
        feed(CoreViewerUpdate::Progress {
            progress: session.progress().unwrap(),
        });
        event = match &effect {
            Effect::Observe { screenshot } => {
                n_obs += 1;
                let mut e = observe(n_obs);
                if let (true, Event::Observed { observation }) = (*screenshot, &mut e) {
                    observation.screenshot = Some(ImageRef {
                        id: format!("shot-{n_obs}"),
                        width: 393,
                        height: 852,
                    });
                }
                e
            }
            Effect::Infer { request } => {
                feed(CoreViewerUpdate::Prompt {
                    text: request.user.clone(),
                });
                n_inf += 1;
                Event::ModelOutput {
                    output: ModelOutput {
                        text: model(&request.user, n_inf),
                    },
                }
            }
            Effect::Act { command, .. } => {
                n_act += 1;
                trace.acts.push(command.clone());
                if let Some(last) = trace.asked.last_mut() {
                    last.acts_after.push(command.clone());
                }
                Event::ActResult {
                    result: act(command, n_act),
                }
            }
            Effect::Confirm { request } => {
                feed(CoreViewerUpdate::ConfirmationPending {
                    request: request.clone(),
                });
                trace.asked.push(Asked {
                    request: request.clone(),
                    target: viewer.approval_target().unwrap(),
                    acts_after: vec![],
                });
                Event::UserDecision { approved: true }
            }
            Effect::AwaitUnlock { .. } => {
                feed(CoreViewerUpdate::AwaitingUnlock);
                Event::DeviceUnlocked
            }
            Effect::Finished { outcome } => {
                feed(CoreViewerUpdate::Finished {
                    status: outcome.status,
                });
                trace.outcome = Some(outcome.clone());
                return trace;
            }
            other => panic!("unexpected {other:?}"),
        };
        feed(CoreViewerUpdate::EffectCompleted {
            effect,
            event: event.clone(),
        });
    }
    panic!("session did not finish");
}

fn outline_frame(t: &CoreApprovalTarget) -> Rect {
    match &t.mark {
        CoreApprovalMark::Outline { frame, .. } => *frame,
        other => panic!("no outline: {other:?} ({})", t.caption),
    }
}

fn inside(r: &Rect, x: f64, y: f64) -> bool {
    x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
}

// ---------------------------------------------------------------------------------------
// P1: `type` without an element. The session types into the first focused TEXT INPUT
// (`prepare`, `type_check`); the viewer outlines the first focused element of ANY role
// (`approval.rs` `focused_index`).
// ---------------------------------------------------------------------------------------

fn two_focused() -> Screen {
    let mut button = node(
        Role::Button,
        Some("Close account"),
        rect(16.0, 120.0, 361.0, 50.0),
        vec![],
    );
    button.focused = true;
    let mut field = node(
        Role::TextField,
        Some("Gift message"),
        rect(16.0, 600.0, 361.0, 44.0),
        vec![],
    );
    field.focused = true;
    screen_of(vec![button, field])
}

#[test]
fn probe_focused_type_outlines_the_field_the_session_types_into() {
    let trace = drive(
        step_by_step(),
        |_| observed(two_focused()),
        |_, n| match n {
            1 => r#"{"action":"type","text":"Happy birthday"}"#.to_owned(),
            _ => r#"{"action":"fail","reason":"x"}"#.to_owned(),
        },
        |_, _| ok(),
    );
    let asked = trace.asked.first().expect("typing asks under step-by-step");
    eprintln!("request: {:?}", asked.request);
    eprintln!("target: {:?}", asked.target);
    eprintln!("acts after approval: {:?}", asked.acts_after);
    let target = asked.target.as_ref().expect("a picture");
    // The session types into the focused text field (no tap: the OS focus takes the text).
    assert!(
        asked.request.summary.contains("Gift message"),
        "the core's question names the field: {}",
        asked.request.summary
    );
    assert_eq!(
        outline_frame(target),
        rect(16.0, 600.0, 361.0, 44.0),
        "the picture must outline the field the text goes into, not another focused element ({})",
        target.accessibility_label
    );
}

// ---------------------------------------------------------------------------------------
// P1: the "Return part" heuristic. A typing step refused by the lock is set aside and its
// step number refunded; the next proposal reuses that number, so its typing question is
// taken for the Return part of a submit (`reducer.rs` `make_approval`).
// ---------------------------------------------------------------------------------------

fn note_screen(value: Option<&str>) -> Screen {
    let mut field = node(
        Role::TextField,
        Some("Message"),
        rect(16.0, 600.0, 361.0, 44.0),
        vec![],
    );
    field.focused = true;
    field.value = value.map(str::to_owned);
    screen_of(vec![field])
}

#[test]
fn probe_typing_question_after_a_lock_refund_is_not_shown_as_return() {
    let trace = drive(
        step_by_step(),
        |_| observed(note_screen(None)),
        |_, n| match n {
            1 => r#"{"action":"type","text":"first"}"#.to_owned(),
            2 => r#"{"action":"type","text":"second","submit":true}"#.to_owned(),
            _ => r#"{"action":"fail","reason":"x"}"#.to_owned(),
        },
        // The first command (TypeText "first") is refused: the phone locked.
        |_, n| if n == 1 { locked() } else { ok() },
    );
    for a in &trace.asked {
        eprintln!(
            "asked {:?}\n  mark {:?}\n  caption {:?}\n  acts after {:?}",
            a.request.summary,
            a.target.as_ref().map(|t| &t.mark),
            a.target.as_ref().map(|t| &t.caption),
            a.acts_after
        );
    }
    // The second proposal's typing question (its first command after approval is
    // TypeText "second", nothing has been typed for it yet).
    let typing = trace
        .asked
        .iter()
        .find(|a| {
            matches!(a.acts_after.first(), Some(RunnerCommand::TypeText { text }) if text == "second")
        })
        .expect("the second typing is asked and then typed");
    let target = typing.target.as_ref().expect("a picture");
    assert!(
        !target.caption.contains("Return"),
        "a typing question must not be pictured as the Return part: {:?}",
        target.caption
    );
    assert_eq!(outline_frame(target), rect(16.0, 600.0, 361.0, 44.0));
}

// ---------------------------------------------------------------------------------------
// Holds checks: the tap the session sends after approval lies in the outlined frame, for a
// covered centre (grid point) and a partly off-screen control; a tap_point is marked where
// it is tapped.
// ---------------------------------------------------------------------------------------

#[test]
fn probe_tap_point_after_approval_is_inside_the_outline() {
    // "Pay" at the bottom edge, partly off screen; a "Coupon" button sits on its centre.
    let screen = screen_of(vec![
        node(
            Role::Button,
            Some("Pay $512.40"),
            rect(16.0, 800.0, 361.0, 100.0),
            vec![],
        ),
        node(
            Role::Button,
            Some("Coupon"),
            rect(150.0, 815.0, 100.0, 25.0),
            vec![],
        ),
    ]);
    let s2 = screen.clone();
    let trace = drive(
        step_by_step(),
        move |_| observed(s2.clone()),
        |user, n| match n {
            1 => {
                let i = index_in_prompt(user, "Button", "Pay $512.40")
                    .unwrap_or_else(|| panic!("{user}"));
                format!(r#"{{"action":"tap","element":{i}}}"#)
            }
            _ => r#"{"action":"fail","reason":"x"}"#.to_owned(),
        },
        |_, _| ok(),
    );
    let asked = trace.asked.first().expect("asked");
    let target = asked.target.as_ref().unwrap();
    let frame = outline_frame(target);
    let Some(RunnerCommand::Tap { x, y }) = asked.acts_after.first() else {
        panic!("no tap after approval: {:?}", asked.acts_after)
    };
    eprintln!("outline {frame:?} tap ({x}, {y})");
    assert!(inside(&frame, *x, *y), "tap ({x}, {y}) outside {frame:?}");
}

#[test]
fn probe_tap_point_is_marked_where_it_is_tapped() {
    let mut c = step_by_step();
    c.model.supports_vision = true;
    let screen = screen_of(vec![node(
        Role::Button,
        Some("Pay $512.40"),
        rect(16.0, 700.0, 361.0, 50.0),
        vec![],
    )]);
    let (x, y) = (0.4, 725.0 / 852.0);
    let trace = drive(
        c,
        move |_| observed(screen.clone()),
        move |_, n| match n {
            1 => r#"{"action":"look"}"#.to_owned(),
            2 => format!(r#"{{"action":"tap_point","x":{x},"y":{y}}}"#),
            _ => r#"{"action":"fail","reason":"x"}"#.to_owned(),
        },
        |_, _| ok(),
    );
    eprintln!("outcome {:?} acts {:?}", trace.outcome, trace.acts);
    let asked = trace.asked.first().expect("asked");
    let t = asked.target.as_ref().unwrap();
    let Some(RunnerCommand::Tap { x: tx, y: ty }) = asked.acts_after.first() else {
        panic!("no tap: {:?}", asked.acts_after)
    };
    eprintln!(
        "mark {:?} tap ({tx}, {ty}) a11y {:?}",
        t.mark, t.accessibility_label
    );
    match &t.mark {
        CoreApprovalMark::Point { point, .. } => {
            assert!((point.x - tx).abs() < 1e-6 && (point.y - ty).abs() < 1e-6);
        }
        other => panic!("{other:?}"),
    }
}
