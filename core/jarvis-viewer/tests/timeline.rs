//! Ported from `Shell/Tests/JarvisKitTests/LiveViewTimelineTests.swift`: the reducer over
//! synthetic runs, its privacy rules for typed text, the encoding for the remote stream and
//! the scene the view draws. (The demo run through the coordinator and the driver's frame
//! option stay in Swift: they drive the shell's `SessionDriver`.)

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
use jarvis_viewer::cursor;
use jarvis_viewer::privacy;
use jarvis_viewer::scene;
use jarvis_viewer::{
    Activity, ElementKind, FrameKey, Gesture, ReplayState, ScreenSource, StepState, Update,
    ViewerMapping, ViewerOutcome, ViewerPoint, ViewerRect, ViewerScreen, ViewerSize, ViewerStep,
    ViewerTimeline, MAX_ELEMENTS,
};

fn pt(x: f64, y: f64) -> ViewerPoint {
    ViewerPoint::new(x, y)
}

fn swipe(fx: f64, fy: f64, tx: f64, ty: f64, ms: u64) -> p::RunnerCommand {
    p::RunnerCommand::Swipe {
        from: p::Point::new(fx, fy),
        to: p::Point::new(tx, ty),
        duration_ms: ms,
    }
}

// MARK: - The reducer

#[test]
fn every_gesture() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    run.act(p::RunnerCommand::LongPress {
        x: 100.0,
        y: 220.0,
        duration_ms: 900,
    });
    run.act(swipe(200.0, 700.0, 200.0, 200.0, 300));
    run.act(swipe(200.0, 200.0, 210.0, 700.0, 300));
    run.act(swipe(300.0, 500.0, 20.0, 480.0, 300));
    run.act(swipe(20.0, 500.0, 300.0, 520.0, 300));
    run.act(swipe(20.0, 500.0, 20.0, 500.0, 300));
    run.act(p::RunnerCommand::PressKey {
        key: p::Key::Return,
    });
    run.act(p::RunnerCommand::PressKey { key: p::Key::Home });
    run.act(p::RunnerCommand::LaunchApp {
        bundle_id: "com.apple.Preferences".into(),
    });
    run.act(p::RunnerCommand::LaunchApp {
        bundle_id: "com.example.unknown".into(),
    });
    run.act(p::RunnerCommand::OpenUrl {
        url: "https://user:pw@Example.com:8443/path/secret?q=token#frag".into(),
    });
    run.act(p::RunnerCommand::OpenUrl {
        url: "tel:+15551234567".into(),
    });
    run.act(p::RunnerCommand::Wait { ms: 300 });
    run.act(p::RunnerCommand::Wait { ms: 1_500 });
    run.act(p::RunnerCommand::Wait { ms: 2_000 });
    let t = run.timeline();
    let sw = |fx, fy, tx, ty| Gesture::Swipe {
        from: pt(fx, fy),
        to: pt(tx, ty),
        duration_ms: 300,
    };
    let expected: Vec<(Gesture, &str)> = vec![
        (Gesture::Tap(pt(100.0, 420.0)), "Tap \u{201C}Send\u{201D}"),
        (
            Gesture::LongPress {
                point: pt(100.0, 220.0),
                duration_ms: 900,
            },
            "Press and hold \u{201C}Email\u{201D}",
        ),
        (sw(200.0, 700.0, 200.0, 200.0), "Swipe up"),
        (sw(200.0, 200.0, 210.0, 700.0), "Swipe down"),
        (sw(300.0, 500.0, 20.0, 480.0), "Swipe left"),
        (sw(20.0, 500.0, 300.0, 520.0), "Swipe right"),
        (sw(20.0, 500.0, 20.0, 500.0), "Swipe"),
        (Gesture::Key("Return".into()), "Press Return"),
        (Gesture::Key("Home".into()), "Go home"),
        (Gesture::Launch("Settings".into()), "Open Settings"),
        (
            Gesture::Launch("com.example.unknown".into()),
            "Open com.example.unknown",
        ),
        (Gesture::OpenUrl("example.com".into()), "Open example.com"),
        (Gesture::OpenUrl("tel:".into()), "Open a tel: link"),
        (Gesture::Wait(300), "Wait 300 ms"),
        (Gesture::Wait(1_500), "Wait 1.5 s"),
        (Gesture::Wait(2_000), "Wait 2 s"),
    ];
    assert_eq!(t.steps.len(), expected.len());
    for (step, (gesture, caption)) in t.steps.iter().zip(expected) {
        assert_eq!(step.gesture, gesture);
        assert_eq!(step.caption, caption);
    }
    assert!(t.steps.iter().all(|s| s.state == StepState::Done));
    assert!(t.steps.iter().all(|s| s.screen_id == t.current_screen_id));
    assert_eq!(
        t.steps.iter().map(|s| s.core_step).collect::<Vec<_>>(),
        (1..=16).map(Some).collect::<Vec<_>>()
    );
    let encoded = json(&t);
    assert!(
        !encoded.contains("secret")
            && !encoded.contains("token")
            && !encoded.contains("pw")
            && !encoded.contains("5551234567")
    );
}

#[test]
fn targets() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(None)));
    run.act(tap(180.0, 415.0)); // on the label inside the button
    run.act(tap(5.0, 600.0)); // on the window only
    let t = run.timeline();
    assert_eq!(t.steps[0].target.as_ref().unwrap().role, "StaticText");
    assert_eq!(t.steps[0].target.as_ref().unwrap().depth, 1);
    assert!(t.steps[1].target.is_none());
    assert_eq!(t.steps[1].caption, "Tap");
}

#[test]
fn failed_act() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(Some("email"))));
    run.act_with(
        type_text("abc"),
        Some(type_action(None, "abc")),
        p::RunnerResult::err("could not type 'abc' into field"),
    );
    run.act_with(tap(100.0, 420.0), None, p::RunnerResult::device_locked());
    run.add(Update::Effect {
        effect: p::Effect::Act(tap(1.0, 1.0)),
        number: 99,
    });
    run.add(Update::Finished(p::OutcomeStatus::Success));
    let t = run.timeline();
    assert_eq!(
        t.steps[0].state,
        StepState::Failed("The phone did not do it".into())
    );
    assert_eq!(
        t.steps[1].state,
        StepState::Failed("The phone was locked".into())
    );
    assert_eq!(
        t.steps[2].state,
        StepState::Failed("Not finished".into()),
        "a command still running when the run ends"
    );
    assert!(!json(&t).contains("could not type"));
}

#[test]
fn lock_mid_run() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    run.add(Update::Effect {
        effect: p::Effect::AwaitUnlock {
            reason: "locked".into(),
            waited_ms: 0,
        },
        number: 50,
    });
    run.add(Update::AwaitingUnlock);
    let mid = run.reducer(config(200, true));
    assert_eq!(mid.timeline().activity, Activity::Locked);
    assert_eq!(
        scene::make(mid.timeline(), &ReplayState::new(), 100_000.0, false).caption,
        "Unlock to continue"
    );
    let lock_root = node(
        p::Role::Application,
        Some("SpringBoard"),
        rect(0.0, 0.0, 393.0, 852.0),
    )
    .children(vec![node(
        p::Role::StaticText,
        Some("Enter passcode 1234"),
        rect(10.0, 10.0, 200.0, 40.0),
    )])
    .build();
    let before_lock = run.updates.len();
    run.observe(observed_full(
        lock_root.clone(),
        PHONE,
        true,
        Some("Mail"),
        "com.apple.mobilemail",
    ));
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    let reducer = run.reducer(config(200, true));
    let t = reducer.timeline();
    assert!(
        !t.screens.iter().any(|s| s.source == ScreenSource::Locked),
        "the lock screen is not kept once no step refers to it"
    );
    let mut at_lock = jarvis_viewer::Reducer::new(config(200, true));
    for u in &run.updates[..before_lock] {
        at_lock.apply(u.clone());
    }
    // The locked observation itself, applied alone.
    at_lock.apply(Update::Effect {
        effect: p::Effect::Observe { screenshot: false },
        number: 60,
    });
    at_lock.apply(Update::EffectCompleted {
        effect: p::Effect::Observe { screenshot: false },
        event: observed_full(lock_root, PHONE, true, Some("Mail"), "com.apple.mobilemail"),
    });
    let screen = at_lock.timeline().current_screen().unwrap();
    assert_eq!(screen.source, ScreenSource::Locked);
    assert!(screen.elements.is_empty());
    assert!(
        screen.frame_key.is_none(),
        "no frame for a locked observation"
    );
    assert!(!json(at_lock.timeline()).contains("passcode"));
    assert!(t.steps.len() == 2 && t.steps.iter().all(|s| s.state == StepState::Done));
    assert_ne!(t.steps[1].screen_id, t.steps[0].screen_id);
}

#[test]
fn runner_text() {
    let mut run = Run::new("task-1");
    run.observe(observed_text("com.apple.Preferences", "Settings", false));
    run.act(tap(100.0, 220.0));
    run.act_typed(type_text("hello"), type_action(None, "hello"));
    let r = run.reducer(config(200, true));
    let t = r.timeline();
    let screen = t.current_screen().unwrap();
    assert_eq!(screen.source, ScreenSource::RunnerText);
    assert!(screen.elements.is_empty());
    assert_eq!(
        screen.frame_key,
        Some(FrameKey {
            task_id: "task-1".into(),
            observation: 1
        })
    );
    assert!(t.steps[0].target.is_none() && t.steps[0].caption == "Tap");
    assert_eq!(
        t.steps[1].caption, "Type text",
        "no tree, no prompt: the field cannot be checked, so the text is hidden"
    );
}

#[test]
fn degenerate() {
    let mut run = Run::new("task-1");
    let zero = p::Size::new(0.0, 0.0);
    run.observe(observed_full(
        login_tree_sized(None, zero),
        zero,
        false,
        Some("Mail"),
        "com.apple.mobilemail",
    ));
    run.act(tap(-50.0, 5_000.0));
    run.act(tap(f64::NAN, 3.0));
    run.act(swipe(f64::INFINITY, 0.0, 0.0, f64::NAN, 10));
    run.act(tap(100.0, 420.0));
    let t = run.timeline();
    assert_eq!(t.steps.len(), 4);
    assert_eq!(
        t.steps[0].gesture,
        Gesture::Tap(pt(-50.0, 5_000.0)),
        "the command's point is kept as sent"
    );
    assert!(t.steps[0].target.is_none());
    assert!(t.steps[1].target.is_none() && t.steps[1].caption == "Tap");
    assert_eq!(t.steps[2].caption, "Swipe");
    assert_eq!(
        t.steps[3].caption, "Tap \u{201C}Send\u{201D}",
        "a zero-size screen uses the fallback phone's bounds"
    );
    for step in &t.steps {
        for ms in [0.0, 50.0, 500.0, 5_000.0] {
            let pose = cursor::pose_of_step(step.number, &t, ms, None, false).unwrap();
            let bounds = ViewerSize::FALLBACK_PHONE;
            assert!(pose.position.is_finite());
            assert!(
                pose.position.x >= 0.0
                    && pose.position.x <= bounds.width
                    && pose.position.y >= 0.0
                    && pose.position.y <= bounds.height
            );
        }
    }
}

#[test]
fn landscape() {
    let size = p::Size::new(852.0, 393.0);
    let root = node(
        p::Role::Application,
        Some("Maps"),
        rect(0.0, 0.0, 852.0, 393.0),
    )
    .children(vec![node(
        p::Role::Button,
        Some("Directions"),
        rect(700.0, 300.0, 120.0, 60.0),
    )])
    .build();
    let mut run = Run::new("task-1");
    run.observe(observed_full(
        root,
        size,
        false,
        Some("Maps"),
        "com.apple.Maps",
    ));
    run.act(tap(760.0, 330.0));
    let t = run.timeline();
    let screen = t.current_screen().unwrap();
    assert!(screen.size == ViewerSize::new(852.0, 393.0) && screen.size.is_landscape());
    assert_eq!(t.steps[0].caption, "Tap \u{201C}Directions\u{201D}");
    let end = cursor::pose_of_step(1, &t, 1e9, None, false).unwrap();
    assert_eq!(
        end.position,
        pt(760.0, 330.0),
        "not clamped by a portrait size"
    );
    let m = ViewerMapping::new(screen.size, ViewerSize::new(300.0, 600.0));
    assert!(m.content_rect().width == 300.0 && m.content_rect().height < 300.0);
}

#[test]
fn cap() {
    let mut run = Run::new("task-1");
    for i in 0..1_000u32 {
        if i % 3 == 0 {
            run.observe(observed(login_tree(None)));
        }
        run.act(tap(f64::from(i % 393), 420.0));
    }
    let t = run.timeline();
    assert_eq!(t.steps.len(), 200);
    assert_eq!(t.dropped_steps, 800);
    assert!(t.steps.first().unwrap().number == 801 && t.steps.last().unwrap().number == 1_000);
    assert_eq!(t.step_numbers(), (801..=1_000).collect::<Vec<u32>>());
    assert!(t.screens.len() <= 201);
    let used: Vec<u32> = t.steps.iter().filter_map(|s| s.screen_id).collect();
    assert!(t
        .screens
        .iter()
        .all(|s| used.contains(&s.id) || Some(s.id) == t.current_screen_id));
    assert!(used.iter().all(|id| t.screens.iter().any(|s| s.id == *id)));
    assert_eq!(t.step(Some(900)).unwrap().number, 900);
    assert!(t.step(Some(10)).is_none());
    let small = run.reducer(config(0, false));
    assert_eq!(small.timeline().steps.len(), 1, "the cap is at least 1");
}

#[test]
fn wireframe() {
    let mut children = vec![
        node(p::Role::Button, Some("Zero"), rect(10.0, 10.0, 0.0, 20.0)),
        node(
            p::Role::Button,
            Some("Off"),
            rect(-500.0, 10.0, 100.0, 20.0),
        ),
        node(
            p::Role::Button,
            Some("Hidden"),
            rect(10.0, 10.0, 100.0, 20.0),
        )
        .visible(false),
        node(
            p::Role::Button,
            Some("NaN"),
            rect(f64::NAN, 10.0, 100.0, 20.0),
        ),
        node(
            p::Role::Button,
            Some("Half"),
            rect(-50.0, 10.0, 100.0, 20.0),
        ),
        node(p::Role::SecureTextField, None, PASSWORD_FRAME)
            .value(SECRET_VALUE)
            .placeholder("Password")
            .secure(true),
        node(
            p::Role::Other("Pager".into()),
            Some("Page 1"),
            rect(0.0, 800.0, 393.0, 20.0),
        ),
    ];
    for i in 0..500u32 {
        children.push(
            node(
                p::Role::Cell,
                Some(&format!("Row {i}")),
                rect(0.0, f64::from(i), 393.0, 1.0),
            )
            .value(&format!("value-{i}")),
        );
    }
    let full = rect(0.0, 0.0, 393.0, 852.0);
    let root = node(p::Role::Application, Some("App"), full)
        .children(vec![node(p::Role::Window, None, full).children(children)])
        .build();
    let mut run = Run::new("task-1");
    run.observe(observed(root));
    let t = run.timeline();
    let screen = t.current_screen().unwrap();
    assert_eq!(screen.elements.len(), MAX_ELEMENTS);
    assert!(screen.elements_truncated);
    let labels: Vec<&str> = screen
        .elements
        .iter()
        .filter_map(|e| e.label.as_deref())
        .collect();
    for absent in ["Zero", "Off", "Hidden", "NaN"] {
        assert!(!labels.contains(&absent));
    }
    assert_eq!(screen.elements[0].label.as_deref(), Some("Half"));
    assert_eq!(
        screen.elements[0].frame,
        ViewerRect::new(0.0, 10.0, 50.0, 20.0),
        "clipped to the screen"
    );
    let secure = screen.elements.iter().find(|e| e.secure).unwrap();
    assert!(secure.label.as_deref() == Some("Password") && secure.kind == ElementKind::Input);
    assert!(screen
        .elements
        .iter()
        .any(|e| e.role == "Pager" && e.kind == ElementKind::Container));
    assert!(!screen
        .elements
        .iter()
        .any(|e| e.role == "Application" || e.role == "Window"));
    let encoded = json(&t);
    assert!(!encoded.contains(SECRET_VALUE));
    assert!(!encoded.contains("value-"), "element values are never kept");
}

#[test]
fn pathological_tree() {
    // As deep as the shell ever builds a tree (`UiTreeBuilder.maxDepth`, 200) and far wider
    // than the viewer keeps.
    let leaf = node(p::Role::Button, Some("Leaf"), rect(10.0, 10.0, 10.0, 10.0));
    let mut inner = vec![leaf];
    for i in 0..5_000u32 {
        inner.push(node(
            p::Role::Button,
            Some(&format!("B{i}")),
            rect(
                100.0 + f64::from(i % 20) * 10.0,
                100.0 + f64::from(i / 20),
                8.0,
                1.0,
            ),
        ));
    }
    let full = rect(0.0, 0.0, 393.0, 852.0);
    let mut deep = node(p::Role::Other("Group".into()), Some("Inner"), full).children(inner);
    for i in 1..200 {
        deep =
            node(p::Role::Other("Group".into()), Some(&format!("G{i}")), full).children(vec![deep]);
    }
    let mut run = Run::new("task-1");
    run.observe(observed(deep.build()));
    run.act(tap(15.0, 15.0));
    let t = run.timeline();
    assert_eq!(t.current_screen().unwrap().elements.len(), MAX_ELEMENTS);
    assert_eq!(
        t.steps[0].target.as_ref().unwrap().label.as_deref(),
        Some("Leaf")
    );
}

#[test]
fn activity() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(None)));
    let mut t = run.reduce();
    assert_eq!(t.timeline().activity, Activity::Looking);
    t.apply(Update::Effect {
        effect: infer(),
        number: 9,
    });
    assert_eq!(t.timeline().activity, Activity::Thinking);
    assert_eq!(
        scene::make(t.timeline(), &ReplayState::new(), 0.0, false).caption,
        "Thinking…"
    );
    t.apply(Update::ConfirmationPending(p::ConfirmRequest {
        category: p::RiskCategory::Delete,
        summary: "Delete the account".into(),
        app: "Mail".into(),
    }));
    assert_eq!(
        t.timeline().activity,
        Activity::WaitingForApproval("Delete".into())
    );
    assert_eq!(
        t.timeline().activity.caption(),
        "Waiting for your OK · Delete"
    );
    t.apply(Update::QuestionPending);
    assert_eq!(t.timeline().activity.caption(), "Waiting for your answer");
    t.apply(Update::Finished(p::OutcomeStatus::Success));
    assert_eq!(
        t.timeline().activity,
        Activity::Finished(ViewerOutcome::Success)
    );
    let s = scene::make(t.timeline(), &ReplayState::new(), 0.0, false);
    assert!(s.caption == "Done" && s.is_finished && !s.is_idle);
}

#[test]
fn restart() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    let mut t = run.reducer(config(7, true));
    t.apply(Update::Started {
        task_id: "task-2".into(),
    });
    assert!(
        t.timeline().steps.is_empty()
            && t.timeline().screens.is_empty()
            && t.timeline().task_id == "task-2"
    );
    assert!(t.config().max_steps == 7 && t.config().frames_enabled);
}

// MARK: - Typed text

fn typed(run: &Run) -> (ViewerStep, String) {
    let t = run.timeline();
    let step = t
        .steps
        .iter()
        .rev()
        .find(|s| matches!(s.gesture, Gesture::Type(_)))
        .unwrap()
        .clone();
    (step, json(&t))
}

#[test]
fn typing_plain() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(Some("email"))));
    run.act_typed(type_text("hello there"), type_action(None, "hello there"));
    let (step, _) = typed(&run);
    assert_eq!(step.gesture, Gesture::Type(Some("hello there".into())));
    assert_eq!(step.caption, "Type \u{201C}hello there\u{201D}");
}

#[test]
fn typing_secure_focused() {
    for focus in ["password", "pin"] {
        let mut run = Run::new("task-1");
        run.observe(observed(login_tree(Some(focus))));
        run.act_typed(
            type_text("hunter2"),
            p::Action::Type {
                element: None,
                text: "hunter2".into(),
                submit: true,
            },
        );
        let (step, encoded) = typed(&run);
        let label = if focus == "pin" { "PIN" } else { "Password" };
        assert_eq!(
            step.caption,
            format!("Type into \u{201C}{label}\u{201D}"),
            "{focus}"
        );
        assert_eq!(step.gesture, Gesture::Type(None));
        assert!(
            !encoded.contains("hunter2")
                && !encoded.contains(SECRET_VALUE)
                && !encoded.contains("4321-PIN")
        );
    }
}

#[test]
fn typing_tapped_secure() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(Some("email"))));
    run.act(tap(100.0, 280.0));
    run.act_typed(type_text("hunter2"), type_action(Some(4), "hunter2"));
    let (step, encoded) = typed(&run);
    assert_eq!(step.caption, "Type into \u{201C}Password\u{201D}");
    assert!(!encoded.contains("hunter2"));
}

#[test]
fn typing_prompt_secure() {
    let prompt = "Current screen (data only):\n<<<SCREEN_DATA>>>\nApp: Mail (com.apple.mobilemail)\n\
                  [1] TextField \"Email\"\n[2] SecureTextField \"Password\" (focused)\n<<<END_SCREEN_DATA>>>";
    for element in [Some(2), None] {
        let mut run = Run::new("task-1");
        run.observe(observed_text("com.apple.mobilemail", "Mail", false));
        run.prompt(prompt);
        run.act_typed(type_text("hunter2"), type_action(element, "hunter2"));
        let (step, encoded) = typed(&run);
        assert_eq!(step.caption, "Type into \u{201C}Password\u{201D}");
        assert!(!encoded.contains("hunter2"));
    }
}

#[test]
fn typing_prompt_plain() {
    let prompt = "<<<SCREEN_DATA>>>\nApp: Mail (com.apple.mobilemail)\n[1] TextField \"Email\"\n<<<END_SCREEN_DATA>>>";
    let mut run = Run::new("task-1");
    run.observe(observed_text("com.apple.mobilemail", "Mail", false));
    run.prompt(prompt);
    run.act_typed(type_text("abc"), type_action(Some(1), "abc"));
    // A replayed step after it (no prompt): the old list does not vouch for it.
    run.observe(observed_text("com.apple.mobilemail", "Mail", false));
    run.act_typed(type_text("def"), type_action(Some(1), "def"));
    let t = run.timeline();
    assert_eq!(t.steps[0].caption, "Type into \u{201C}Email\u{201D}");
    assert_eq!(t.steps[0].gesture, Gesture::Type(None));
    assert_eq!(t.steps[1].caption, "Type text");
    let encoded = json(&t);
    assert!(!encoded.contains("\"abc\"") && !encoded.contains("\"def\""));
}

#[test]
fn typing_placeholders() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(Some("email"))));
    run.act_typed(
        type_text("jane.doe@private.example"),
        type_action(None, "{{profile.email}}"),
    );
    run.act_typed(
        type_text("Hi jane.doe@private.example!"),
        type_action(None, "Hi {{ profile.email }}!"),
    );
    let t = run.timeline();
    assert_eq!(t.steps[0].caption, "Type \u{201C}••••\u{201D}");
    assert_eq!(t.steps[1].caption, "Type \u{201C}Hi ••••!\u{201D}");
    assert!(!json(&t).contains("private.example"));
}

#[test]
fn typing_doubt() {
    let mut no_focus = Run::new("task-1");
    no_focus.observe(observed(login_tree(None)));
    no_focus.act_typed(type_text("abc"), type_action(None, "abc"));
    assert_eq!(typed(&no_focus).0.caption, "Type text");

    let mut mismatch = Run::new("task-1");
    mismatch.observe(observed(login_tree(Some("email"))));
    mismatch.act_typed(type_text("something else"), type_action(None, "abc"));
    let (step, encoded) = typed(&mismatch);
    assert_eq!(
        step.caption, "Type into \u{201C}Email\u{201D}",
        "the field is named, the text is not shown"
    );
    assert_eq!(step.gesture, Gesture::Type(None));
    assert!(!encoded.contains("something else") && !encoded.contains("\"abc\""));

    let mut no_progress = Run::new("task-1");
    no_progress.observe(observed(login_tree(Some("email"))));
    no_progress.act_typed(type_text("abc"), p::Action::Tap { element: 3 });
    assert_eq!(
        typed(&no_progress).0.caption,
        "Type into \u{201C}Email\u{201D}"
    );
    assert_eq!(typed(&no_progress).0.gesture, Gesture::Type(None));

    let mut no_screen = Run::new("task-1");
    no_screen.act_typed(type_text("abc"), type_action(None, "abc"));
    assert_eq!(typed(&no_screen).0.caption, "Type text");
}

#[test]
fn typing_capped() {
    let long = format!("{}\nline", "a".repeat(100));
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(Some("email"))));
    run.act_typed(type_text(&long), type_action(None, &long));
    let (step, _) = typed(&run);
    let Gesture::Type(Some(shown)) = step.gesture else {
        panic!("no text")
    };
    assert!(
        shown.chars().count() == privacy::MAX_TYPED_CHARACTERS
            && shown.ends_with('…')
            && !shown.contains('\n')
    );
    assert_eq!(privacy::typed_text("  \n ", false), None);
    assert_eq!(
        privacy::typed_text("anything", true).as_deref(),
        Some(privacy::MASK)
    );
    assert_eq!(privacy::mask_placeholders("a {{x}} b {{y"), "a •••• b ••••");
    assert_eq!(privacy::host_of("notaurl"), None);
    assert_eq!(
        privacy::host_of("HTTPS://Sub.Example.COM/a?b").as_deref(),
        Some("sub.example.com")
    );
}

// MARK: - Encoding and the scene

#[test]
fn encoding_round_trip() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(Some("email"))));
    run.act(tap(100.0, 420.0));
    run.act(p::RunnerCommand::LongPress {
        x: 100.0,
        y: 220.0,
        duration_ms: 900,
    });
    run.act(swipe(1.0, 2.0, 3.0, 4.0, 5));
    run.act_typed(type_text("hi"), type_action(None, "hi"));
    run.act(type_text("x"));
    run.act_with(
        p::RunnerCommand::PressKey { key: p::Key::Tab },
        None,
        p::RunnerResult::err("nope"),
    );
    run.act(p::RunnerCommand::LaunchApp {
        bundle_id: "com.apple.mobilesafari".into(),
    });
    run.act(p::RunnerCommand::OpenUrl {
        url: "https://example.com/a".into(),
    });
    run.act(p::RunnerCommand::Wait { ms: 10 });
    run.add(Update::Effect {
        effect: p::Effect::CallTool(p::ToolCall {
            name: "clock_now".into(),
            args: serde_json::json!({}),
        }),
        number: 77,
    });
    let mut t = run.reducer(config(200, true)).timeline().clone();
    t.screens.push(ViewerScreen {
        id: 999,
        size: ViewerSize::new(1.0, 2.0),
        app_name: None,
        bundle_id: String::new(),
        source: ScreenSource::Locked,
        elements: vec![],
        elements_truncated: true,
        frame_key: None,
        focused_input_secure: Some(true),
    });
    let data = t.encode();
    let back = ViewerTimeline::decode(&data).unwrap();
    assert_eq!(back, t);
    let text = json(&t);
    for key in [
        r#""kind":"tap""#,
        r#""kind":"long_press""#,
        r#""duration_ms":900"#,
        r#""kind":"swipe""#,
        r#""kind":"type""#,
        r#""kind":"key""#,
        r#""kind":"launch""#,
        r#""kind":"open_url""#,
        r#""host":"example.com""#,
        r#""kind":"wait""#,
        r#""screen_id":"#,
        r#""core_step":"#,
        r#""task_id":"task-1""#,
        r#""frame_key":{"task_id":"task-1","observation":1}"#,
        r#""kind":"failed""#,
        r#""kind":"using_tool""#,
        r#""source":"tree""#,
        r#""source":"locked""#,
        r#""elements_truncated":true"#,
        r#""current_screen_id":"#,
        r#""dropped_steps":0"#,
        r#""version":1"#,
    ] {
        assert!(text.contains(key), "missing {key}");
    }
    for activity in [
        Activity::Idle,
        Activity::Looking,
        Activity::Thinking,
        Activity::Acting,
        Activity::UsingTool("t".into()),
        Activity::WaitingForApproval("Delete".into()),
        Activity::WaitingForAnswer,
        Activity::Locked,
        Activity::Finished(ViewerOutcome::Interrupted),
    ] {
        let decoded: Activity =
            serde_json::from_str(&serde_json::to_string(&activity).unwrap()).unwrap();
        assert_eq!(decoded, activity);
    }
    // An unknown kind (from a newer sender) is kept as such and re-encoded with its tag; a
    // gesture without a kind is still an error.
    let teleport: Gesture = serde_json::from_str(r#"{"kind":"teleport"}"#).unwrap();
    assert_eq!(teleport, Gesture::Unknown("teleport".into()));
    let again: Gesture = serde_json::from_str(&serde_json::to_string(&teleport).unwrap()).unwrap();
    assert_eq!(again, teleport);
    assert!(serde_json::from_str::<Gesture>(r#"{"point":{"x":1,"y":2}}"#).is_err());
    let kinds: Vec<ElementKind> = serde_json::from_str(r#"["control","hologram"]"#).unwrap();
    assert_eq!(
        kinds,
        vec![ElementKind::Control, ElementKind::Container],
        "an unknown kind from a newer sender reads as a container"
    );
}

#[test]
fn live_scene() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    run.observe(observed(login_tree(None)));
    let mut pt_ = run.reduce();
    pt_.apply(Update::Effect {
        effect: infer(),
        number: 40,
    });
    let t = pt_.timeline();
    let mut replay = ReplayState::new();
    replay.sync(&t.step_numbers(), 100_000.0, None);
    let playing = scene::make(t, &replay, 100_100.0, false);
    assert!(
        playing.step.map(|s| s.number) == Some(1)
            && playing.caption == "Tap \u{201C}Send\u{201D}"
            && !playing.is_idle
    );
    assert_eq!(playing.screen.map(|s| s.id), t.steps[0].screen_id);
    let later = scene::make(t, &replay, 200_000.0, false);
    assert!(later.step.is_none() && later.caption == "Thinking…" && later.is_idle);
    assert_eq!(later.screen.map(|s| s.id), t.current_screen_id);
    assert_eq!(later.pose.position, pt(100.0, 420.0));
    assert!(later.position.is_none() && later.accessibility_value() == "Thinking…");
}

#[test]
fn replay_scene() {
    let mut run = Run::new("task-1");
    run.observe(observed(login_tree(None)));
    run.act(tap(100.0, 420.0));
    run.observe(observed(login_tree(Some("email"))));
    run.act_with(tap(100.0, 220.0), None, p::RunnerResult::err("x"));
    let t = run.timeline();
    let numbers = t.step_numbers();
    let mut replay = ReplayState::new();
    replay.sync(&numbers, 0.0, None);
    replay.step_back(&numbers, 1_000.0);
    let back = scene::make(&t, &replay, 1_000.0, false);
    assert!(
        back.step.map(|s| s.number) == Some(1)
            && back.position_text().as_deref() == Some("Step 1 of 2")
            && back.screen.map(|s| s.id) == t.steps[0].screen_id
    );
    assert_eq!(
        back.accessibility_value(),
        "Tap \u{201C}Send\u{201D}, Step 1 of 2"
    );
    replay.step_forward(&numbers, 2_000.0);
    let forward = scene::make(&t, &replay, 2_000.0, false);
    assert_eq!(forward.caption, "Tap \u{201C}Email\u{201D}");
    assert_eq!(
        forward.accessibility_value(),
        "Tap \u{201C}Email\u{201D}, did not work, Step 2 of 2"
    );
    let empty_t = ViewerTimeline::default();
    let empty = scene::make(&empty_t, &ReplayState::new(), 0.0, false);
    assert!(empty.caption == "Standing by" && empty.screen.is_none() && empty.step.is_none());
}
