//! The approval sheet's picture (`Reducer::approval`): bound to the evaluated observation,
//! the target's frame for each action, nothing (with a caption) where no box would be
//! right, cleared by the next update, and never in the encoded timeline.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

mod common;

use common::*;
use jarvis_proto as p;
use jarvis_viewer::{
    ApprovalMark, FrameKey, Reducer, ScreenSource, Update, ViewerRect, MAX_ELEMENTS,
};

/// A request stating nothing structured (as decoded from older JSON): no stage, kind or
/// target.
fn request(summary: &str) -> p::ConfirmRequest {
    p::ConfirmRequest {
        category: p::RiskCategory::Payment,
        summary: summary.to_owned(),
        app: "com.example.shop".to_owned(),
        phone_only: false,
        ..Default::default()
    }
}

/// The request the core sends when it asks about `action` (part `stage`) on the shop screen:
/// its kind and the target the session resolves (`Session::stamped`: the element the action
/// names, or the focused field for `type` without one, its frame, the screen's digest; a
/// `tap_point`'s point in screen points).
fn request_for(action: &p::Action, stage: p::ConfirmStage, summary: &str) -> p::ConfirmRequest {
    let screen = shop_screen();
    let compact = jarvis_observe::compact(&screen);
    let digest = jarvis_observe::screen_digest(&screen).to_hex();
    let index = match action {
        p::Action::Type { element: None, .. } => compact
            .elements
            .iter()
            .find(|e| e.focused && e.role.is_text_input())
            .map(|e| e.index),
        p::Action::TapPoint { .. } => None,
        other => other.element(),
    };
    let target = match action {
        p::Action::TapPoint { x, y } => Some(p::ConfirmTarget {
            point: Some(p::Point::new(x * PHONE.width, y * PHONE.height)),
            digest,
            ..p::ConfirmTarget::default()
        }),
        _ => index.map(|i| p::ConfirmTarget {
            element: Some(i),
            frame: compact.get(i).map(|e| e.frame),
            digest,
            ..p::ConfirmTarget::default()
        }),
    }
    .map(Box::new);
    p::ConfirmRequest {
        stage,
        verb: jarvis_safety::verb_of(action),
        target,
        ..request(summary)
    }
}

/// The updates of a confirmation of `action` (the core counted the step when it proposed it).
fn ask(run: &mut Run, action: p::Action, summary: &str) {
    ask_part(run, action, p::ConfirmStage::Whole, summary);
}

/// [`ask`] for part `stage` of the action.
fn ask_part(run: &mut Run, action: p::Action, stage: p::ConfirmStage, summary: &str) {
    run.number += 1;
    let request = request_for(&action, stage, summary);
    let effect = p::Effect::Confirm(request.clone());
    run.updates.push(Update::Effect {
        effect,
        number: run.number,
    });
    run.updates
        .push(Update::Progress(progress(run.core_step + 1, Some(action))));
    run.updates.push(Update::ConfirmationPending(request));
}

fn shop_tree() -> p::UiNode {
    let full = rect(0.0, 0.0, PHONE.width, PHONE.height);
    node(p::Role::Application, Some("Shop"), full)
        .children(vec![node(p::Role::Window, None, full).children(vec![
            node(
                p::Role::TextField,
                Some("Note"),
                rect(16.0, 200.0, 361.0, 44.0),
            )
            .focused(true),
            node(
                p::Role::Button,
                Some("Pay $512.40"),
                rect(16.0, 700.0, 361.0, 50.0),
            ),
        ])])
        .build()
}

fn shop_screen() -> p::Screen {
    p::Screen {
        tree_cut: false,
        app_bundle_id: "com.example.shop".to_owned(),
        app_name: Some("Shop".to_owned()),
        size: PHONE,
        root: shop_tree(),
        keyboard_visible: false,
        orientation: p::Orientation::Portrait,
    }
}

/// The index the core gives the element labelled `label` on the shop screen.
fn index_of(label: &str) -> u32 {
    jarvis_observe::compact(&shop_screen())
        .elements
        .iter()
        .find(|e| e.label == label)
        .map(|e| e.index)
        .unwrap()
}

fn observed_shop() -> p::Event {
    p::Event::Observed(p::Observation {
        screen: shop_screen(),
        screenshot: None,
        device_locked: false,
    })
}

fn reduce(run: &Run) -> Reducer {
    run.reducer(config(200, false))
}

fn unit(frame: p::Rect) -> ViewerRect {
    ViewerRect::new(
        frame.x / PHONE.width,
        frame.y / PHONE.height,
        frame.width / PHONE.width,
        frame.height / PHONE.height,
    )
}

#[test]
fn a_tap_outlines_its_element_on_the_evaluated_observation() {
    let mut run = Run::new("task-1");
    run.observe(observed_shop());
    let evaluated = run.number;
    run.add(Update::Effect {
        effect: infer(),
        number: run.number + 1,
    });
    run.number += 1;
    ask(
        &mut run,
        p::Action::Tap {
            element: index_of("Pay $512.40"),
        },
        "Pay $512.40",
    );
    let r = reduce(&run);
    let a = r
        .approval()
        .expect("a picture while the confirmation is pending");
    assert_eq!(
        a.frame_key,
        FrameKey {
            task_id: "task-1".to_owned(),
            observation: evaluated,
        }
    );
    assert_eq!(Some(a.screen_id), r.timeline().current_screen_id);
    assert_eq!(
        a.request,
        request_for(
            &p::Action::Tap {
                element: index_of("Pay $512.40"),
            },
            p::ConfirmStage::Whole,
            "Pay $512.40"
        )
    );
    let frame = rect(16.0, 700.0, 361.0, 50.0);
    assert_eq!(
        a.mark,
        ApprovalMark::Outline {
            frame: frame.into(),
            unit: unit(frame),
        }
    );
    assert_eq!(a.accessibility_label, "Outlined: Button 'Pay $512.40'");
    // Preview review: the caption says where, and that the picture is from when Jarvis asked.
    assert_eq!(
        a.caption,
        "The outline is where Jarvis will tap. The picture is from when Jarvis asked and may have changed."
    );
}

#[test]
fn long_press_type_and_focused_typing_outline_their_targets() {
    for (action, label, caption) in [
        (
            p::Action::LongPress {
                element: index_of("Pay $512.40"),
                duration_ms: 800,
            },
            "Pay $512.40",
            "press and hold",
        ),
        // Preview review: "… where Jarvis will type." (was "… type into the outlined field").
        (
            type_action(Some(index_of("Note")), "hello"),
            "Note",
            "will type.",
        ),
        // No element: the focused field.
        (type_action(None, "hello"), "Note", "will type."),
    ] {
        let mut run = Run::new("t");
        run.observe(observed_shop());
        ask(&mut run, action, "s");
        let r = reduce(&run);
        let a = r.approval().unwrap();
        let ApprovalMark::Outline { frame, .. } = a.mark else {
            panic!("{a:?}")
        };
        let expected = if label == "Note" {
            rect(16.0, 200.0, 361.0, 44.0)
        } else {
            rect(16.0, 700.0, 361.0, 50.0)
        };
        assert_eq!(frame, expected.into());
        assert!(a.caption.contains(caption), "{}", a.caption);
        assert!(a.accessibility_label.contains(label));
    }
}

#[test]
fn tap_point_marks_the_point_and_off_screen_marks_nothing() {
    let mut run = Run::new("t");
    run.observe(observed_shop());
    ask(&mut run, p::Action::TapPoint { x: 0.5, y: 0.25 }, "s");
    let r = reduce(&run);
    let a = r.approval().unwrap();
    let ApprovalMark::Point { point, unit } = a.mark else {
        panic!("{a:?}")
    };
    assert!(close(point.x, PHONE.width / 2.0, 1e-9) && close(point.y, PHONE.height / 4.0, 1e-9));
    assert!(close(unit.x, 0.5, 1e-12) && close(unit.y, 0.25, 1e-12));
    assert!(a
        .accessibility_label
        .starts_with("Marked: a point 50% across and 25% down"));

    for action in [
        p::Action::TapPoint { x: 1.5, y: 0.5 },
        p::Action::TapPoint {
            x: f64::NAN,
            y: 0.5,
        },
    ] {
        let mut run = Run::new("t");
        run.observe(observed_shop());
        ask(&mut run, action, "s");
        let a = reduce(&run).approval().cloned().unwrap();
        assert_eq!(a.mark, ApprovalMark::Nothing);
        assert!(
            a.caption.contains("not on the visible screen"),
            "{}",
            a.caption
        );
    }
    // An element the screen does not list: no box, a caption.
    let mut run = Run::new("t");
    run.observe(observed_shop());
    ask(&mut run, p::Action::Tap { element: 99 }, "s");
    let a = reduce(&run).approval().cloned().unwrap();
    assert_eq!(a.mark, ApprovalMark::Nothing);
    assert!(a.caption.contains("could not find"), "{}", a.caption);
}

#[test]
fn the_return_part_of_a_submit_outlines_nothing() {
    let mut run = Run::new("t");
    run.observe(observed_shop());
    let submit = p::Action::Type {
        element: Some(index_of("Note")),
        text: "hi".to_owned(),
        submit: true,
    };
    // The part asked is stated by the core in the request (`stage`), never read from the
    // question's text or the step history.
    let typing = "Type \"hi\" into TextField \"Note\" in Shop";
    let ret = "Press Return in TextField \"Note\" in Shop";
    // The typing part: the field.
    ask_part(&mut run, submit.clone(), p::ConfirmStage::Typing, typing);
    assert!(matches!(
        reduce(&run).approval().unwrap().mark,
        ApprovalMark::Outline { .. }
    ));
    run.add(Update::EffectCompleted {
        effect: p::Effect::Confirm(request_for(&submit, p::ConfirmStage::Typing, typing)),
        event: p::Event::UserDecision { approved: true },
    });
    // The typing runs, the screen is looked at again, then Return is asked.
    run.act_typed(type_text("hi"), submit.clone());
    run.observe(observed_shop());
    let after_typing = run.clone();
    run.number += 1;
    let asked_return = request_for(&submit, p::ConfirmStage::Return, ret);
    run.add(Update::Effect {
        effect: p::Effect::Confirm(asked_return.clone()),
        number: run.number,
    });
    run.add(Update::Progress(progress(
        run.core_step,
        Some(submit.clone()),
    )));
    run.add(Update::ConfirmationPending(asked_return));
    let a = reduce(&run).approval().cloned().unwrap();
    assert_eq!(a.mark, ApprovalMark::Nothing);
    assert!(a.caption.contains("Return"), "{}", a.caption);
    assert!(a.accessibility_label.starts_with("Nothing outlined."));

    // A question that states no part, kind or target (decoded from older JSON; here the
    // one-letter summary the old test used, or a first-use question): nothing marked, the
    // neutral caption, no specific claim.
    for summary in ["s", "Let Jarvis open the top search result for \"hi\"?"] {
        let mut run = after_typing.clone();
        run.number += 1;
        run.add(Update::Effect {
            effect: p::Effect::Confirm(request(summary)),
            number: run.number,
        });
        run.add(Update::Progress(progress(
            run.core_step,
            Some(submit.clone()),
        )));
        run.add(Update::ConfirmationPending(request(summary)));
        let a = reduce(&run).approval().cloned().unwrap();
        assert_eq!(a.mark, ApprovalMark::Nothing);
        assert_eq!(
            a.caption,
            "Jarvis could not mark the target; read the question."
        );
        assert!(!a.caption.contains("Return") && !a.caption.contains("will type"));
    }
    // Behind the runner-text warning the stated part still decides.
    let mut run = Run::new("t");
    run.observe(observed_shop());
    let warned = format!("{}{typing}", jarvis_safety::UNVERIFIED_SCREEN_WARNING);
    ask_part(&mut run, submit, p::ConfirmStage::Typing, &warned);
    assert!(matches!(
        reduce(&run).approval().unwrap().mark,
        ApprovalMark::Outline { .. }
    ));
}

#[test]
fn app_level_actions_outline_nothing() {
    for action in [
        p::Action::PressHome,
        p::Action::OpenApp {
            app: "Mail".to_owned(),
        },
        p::Action::OpenUrl {
            url: "https://example.com/pay?card=1".to_owned(),
        },
        p::Action::Swipe {
            direction: p::Direction::Up,
            element: None,
        },
    ] {
        let mut run = Run::new("t");
        run.observe(observed_shop());
        ask(&mut run, action, "s");
        let a = reduce(&run).approval().cloned().unwrap();
        assert_eq!(a.mark, ApprovalMark::Nothing, "{a:?}");
        assert!(!a.caption.contains("card"), "{}", a.caption);
    }
}

#[test]
fn every_other_update_clears_it_and_a_locked_screen_has_none() {
    let mut run = Run::new("t");
    run.observe(observed_shop());
    ask(&mut run, p::Action::Tap { element: 1 }, "s");
    let mut r = reduce(&run);
    assert!(r.approval().is_some());
    let change = r.apply(Update::EffectCompleted {
        effect: p::Effect::Confirm(request("s")),
        event: p::Event::UserDecision { approved: false },
    });
    assert!(change.approval && change.any());
    assert!(r.approval().is_none());
    for clear in [
        Update::Finished(p::OutcomeStatus::Cancelled),
        Update::AwaitingUnlock,
        Update::Ignored,
        Update::QuestionPending,
    ] {
        let mut r = reduce(&run);
        r.apply(clear);
        assert!(r.approval().is_none());
    }
    // The phone locked: nothing evaluated to show.
    let mut run = Run::new("t");
    run.observe(observed_full(
        shop_tree(),
        PHONE,
        true,
        Some("Shop"),
        "com.example.shop",
    ));
    ask(&mut run, p::Action::Tap { element: 1 }, "s");
    assert!(reduce(&run).approval().is_none());
}

#[test]
fn runner_text_takes_labels_from_the_prompt_only() {
    let mut run = Run::new("t");
    run.observe(p::Event::ObservedRunnerText(p::Observation {
        screen: shop_screen(),
        screenshot: None,
        device_locked: false,
    }));
    let index = index_of("Pay $512.40");
    run.prompt(&format!(
        "<<<SCREEN_DATA>>>\n[{index}] Button \"Pay now\"\n<<<END_SCREEN_DATA>>>"
    ));
    ask(&mut run, p::Action::Tap { element: index }, "s");
    let r = reduce(&run);
    let a = r.approval().unwrap();
    assert!(matches!(a.mark, ApprovalMark::Outline { .. }));
    assert_eq!(a.accessibility_label, "Outlined: Button 'Pay now'");
    // Preview review: runner text now makes a wireframe like a typed tree (was: none), and
    // its source says where the layout came from.
    let current = r.timeline().current_screen().unwrap();
    assert_eq!(current.source, ScreenSource::RunnerText);
    assert_eq!(current.elements.len(), 2, "{:?}", current.elements);
}

/// The runner's text as LoopCtrl writes it, parsed with the session's parser.
fn parsed(text: &str, locked: bool) -> p::Event {
    let root = jarvis_observe::parse_runner_text(text).unwrap().root;
    p::Event::ObservedRunnerText(p::Observation {
        screen: p::Screen {
            tree_cut: false,
            app_bundle_id: "com.example.shop".to_owned(),
            app_name: Some("Shop".to_owned()),
            size: PHONE,
            root,
            keyboard_visible: false,
            orientation: p::Orientation::Portrait,
        },
        screenshot: None,
        device_locked: locked,
    })
}

#[test]
fn runner_text_is_drawn_like_a_typed_tree_and_keeps_no_values_or_secrets() {
    let text = "Application, label: 'Shop', {{0, 0}, {393, 852}}\n  \
                Window, {{0, 0}, {393, 852}}\n    \
                TextField, label: 'Card 4111 1111 1111 1111', value: 'hunter2-value', id: 'card', {{16, 200}, {361, 44}}\n    \
                SecureTextField, label: 'Password', value: 'p4ssw0rd-secret', {{16, 260}, {361, 44}}\n    \
                Button, label: 'Pay $512.40', {{16, 700}, {361, 50}}\n    \
                Button, label: 'No frame here'\n    \
                StaticText, label: 'Zero', {{10, 10}, {0, 0}}\n";
    let mut run = Run::new("t");
    run.observe(parsed(text, false));
    let r = reduce(&run);
    let screen = r.timeline().current_screen().unwrap();
    assert_eq!(screen.source, ScreenSource::RunnerText);
    let labels: Vec<Option<&str>> = screen.elements.iter().map(|e| e.label.as_deref()).collect();
    // A line without a frame is no node; a zero-area frame is not drawn.
    assert_eq!(screen.elements.len(), 3, "{labels:?}");
    assert!(labels.contains(&Some("Pay $512.40")), "{labels:?}");
    assert!(screen
        .elements
        .iter()
        .any(|e| e.secure && e.role == "SecureTextField"));
    let encoded = json(r.timeline());
    for secret in [
        "4111 1111 1111 1111",
        "1111 1111",
        "hunter2-value",
        "p4ssw0rd-secret",
        "No frame here",
    ] {
        assert!(!encoded.contains(secret), "{secret} in {encoded}");
    }
    // A locked runner-text observation keeps nothing of the screen.
    let mut run = Run::new("t");
    run.observe(parsed(text, true));
    let r = reduce(&run);
    let screen = r.timeline().current_screen().unwrap();
    assert_eq!(screen.source, ScreenSource::Locked);
    assert!(screen.elements.is_empty());
    assert!(!json(r.timeline()).contains("Pay"));
    // The element cap holds for runner text as for a typed tree.
    let mut wide = String::from("Application, label: 'Shop', {{0, 0}, {393, 852}}\n");
    for i in 0..(MAX_ELEMENTS + 50) {
        wide.push_str(&format!(
            "  Button, label: 'B{i}', {{{{0, {}}}, {{10, 1}}}}\n",
            i % 800
        ));
    }
    let mut run = Run::new("t");
    run.observe(parsed(&wide, false));
    let r = reduce(&run);
    let screen = r.timeline().current_screen().unwrap();
    assert_eq!(screen.elements.len(), MAX_ELEMENTS);
    assert!(screen.elements_truncated);
}

#[test]
fn the_picture_is_never_encoded() {
    let mut run = Run::new("t");
    run.observe(observed_shop());
    ask(&mut run, p::Action::Tap { element: 1 }, "Pay $512.40");
    let r = reduce(&run);
    assert!(r.approval().is_some());
    let text = json(r.timeline());
    assert!(!text.contains("Outlined"), "{text}");
    assert!(!text.contains("Jarvis will tap"), "{text}");
    assert!(!text.contains("The outline is where"), "{text}");
    assert!(!text.contains("from when Jarvis asked"), "{text}");
    assert!(!text.contains("\"mark\""), "{text}");
    assert!(!text.contains("observation"), "{text}");
}

/// The updates of a confirmation `request` of `action`, sent as is.
fn ask_request(run: &mut Run, action: p::Action, request: p::ConfirmRequest) {
    run.number += 1;
    run.updates.push(Update::Effect {
        effect: p::Effect::Confirm(request.clone()),
        number: run.number,
    });
    run.updates
        .push(Update::Progress(progress(run.core_step + 1, Some(action))));
    run.updates.push(Update::ConfirmationPending(request));
}

/// What is asked is stated by the core (`stage`, `verb`, `target`); a target that does not
/// match the evaluated screen marks nothing, with the neutral caption.
#[test]
fn a_target_that_does_not_match_the_evaluated_screen_marks_nothing() {
    let tap = p::Action::Tap {
        element: index_of("Pay $512.40"),
    };
    let base = request_for(&tap, p::ConfirmStage::Whole, "Pay");
    let edit = |f: &dyn Fn(&mut p::ConfirmTarget)| {
        let mut r = base.clone();
        if let Some(t) = r.target.as_mut() {
            f(t);
        }
        r
    };
    let mut no_target = base.clone();
    no_target.target = None;
    for request in [
        edit(&|t| t.digest = "00".repeat(32)),
        edit(&|t| t.digest.clear()),
        edit(&|t| t.frame = Some(rect(16.0, 650.0, 361.0, 50.0))),
        edit(&|t| t.frame = None),
        no_target,
    ] {
        let mut run = Run::new("t");
        run.observe(observed_shop());
        ask_request(&mut run, tap.clone(), request.clone());
        let a = reduce(&run).approval().cloned().unwrap();
        assert_eq!(a.mark, ApprovalMark::Nothing, "{request:?}");
        assert_eq!(
            a.caption,
            "Jarvis could not mark the target; read the question."
        );
    }
    // The same request with its target intact outlines the button.
    let mut run = Run::new("t");
    run.observe(observed_shop());
    ask_request(&mut run, tap, base);
    assert!(matches!(
        reduce(&run).approval().unwrap().mark,
        ApprovalMark::Outline { .. }
    ));
}

/// The step history decides nothing: a typing request outlines the field the core named,
/// whatever the last proposed action says.
#[test]
fn the_request_not_the_last_action_decides_the_mark() {
    let typing = p::Action::Type {
        element: Some(index_of("Note")),
        text: "hi".to_owned(),
        submit: true,
    };
    let request = request_for(&typing, p::ConfirmStage::Typing, "Type");
    let mut run = Run::new("t");
    run.observe(observed_shop());
    ask_request(&mut run, p::Action::PressHome, request);
    let a = reduce(&run).approval().cloned().unwrap();
    let ApprovalMark::Outline { frame, .. } = a.mark else {
        panic!("{a:?}")
    };
    assert_eq!(frame, rect(16.0, 200.0, 361.0, 44.0).into());
}
