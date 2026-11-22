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
use jarvis_viewer::{ApprovalMark, FrameKey, Reducer, Update, ViewerRect};

fn request(summary: &str) -> p::ConfirmRequest {
    p::ConfirmRequest {
        category: p::RiskCategory::Payment,
        summary: summary.to_owned(),
        app: "com.example.shop".to_owned(),
    }
}

/// The updates of a confirmation of `action` (the core counted the step when it proposed it).
fn ask(run: &mut Run, action: p::Action, summary: &str) {
    run.number += 1;
    let effect = p::Effect::Confirm(request(summary));
    run.updates.push(Update::Effect {
        effect,
        number: run.number,
    });
    run.updates
        .push(Update::Progress(progress(run.core_step + 1, Some(action))));
    run.updates
        .push(Update::ConfirmationPending(request(summary)));
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
    assert_eq!(a.request, request("Pay $512.40"));
    let frame = rect(16.0, 700.0, 361.0, 50.0);
    assert_eq!(
        a.mark,
        ApprovalMark::Outline {
            frame: frame.into(),
            unit: unit(frame),
        }
    );
    assert_eq!(a.accessibility_label, "Outlined: Button 'Pay $512.40'");
    assert_eq!(a.caption, "Jarvis will tap the outlined control.");
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
        (
            type_action(Some(index_of("Note")), "hello"),
            "Note",
            "type into",
        ),
        // No element: the focused field.
        (type_action(None, "hello"), "Note", "type into"),
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
    // The typing part: the field.
    ask(&mut run, submit.clone(), "s");
    assert!(matches!(
        reduce(&run).approval().unwrap().mark,
        ApprovalMark::Outline { .. }
    ));
    run.add(Update::EffectCompleted {
        effect: p::Effect::Confirm(request("s")),
        event: p::Event::UserDecision { approved: true },
    });
    // The typing runs, the screen is looked at again, then Return is asked.
    run.act_typed(type_text("hi"), submit.clone());
    run.observe(observed_shop());
    run.number += 1;
    run.add(Update::Effect {
        effect: p::Effect::Confirm(request("s")),
        number: run.number,
    });
    run.add(Update::Progress(progress(run.core_step, Some(submit))));
    run.add(Update::ConfirmationPending(request("s")));
    let a = reduce(&run).approval().cloned().unwrap();
    assert_eq!(a.mark, ApprovalMark::Nothing);
    assert!(a.caption.contains("Return"), "{}", a.caption);
    assert!(a.accessibility_label.starts_with("Nothing outlined."));
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
    // Runner text still makes no wireframe.
    assert!(r.timeline().current_screen().unwrap().elements.is_empty());
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
    assert!(!text.contains("\"mark\""), "{text}");
    assert!(!text.contains("observation"), "{text}");
}
