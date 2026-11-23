//! Ported from `Shell/Tests/JarvisKitTests/LiveViewReviewTests.swift` (the adversarial
//! review's regression tests): privacy against the core's redaction, forward-compatible
//! decoding, the pure pieces the view reads, stopped steps. The driver's capture tests and
//! the frame store stay in Swift.

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
use jarvis_viewer::cursor::{self, Phase};
use jarvis_viewer::privacy;
use jarvis_viewer::scene;
use jarvis_viewer::{
    Activity, Gesture, ReplayState, ScreenSource, StepState, Update, ViewerMapping, ViewerPoint,
    ViewerRect, ViewerScreen, ViewerSize, ViewerStep, ViewerTimeline,
};

fn pobserved(root: p::UiNode) -> p::Event {
    observed_full(root, PHONE, false, Some("Messages"), "com.apple.MobileSMS")
}

fn full() -> p::Rect {
    rect(0.0, 0.0, 393.0, 852.0)
}

fn screen_of(children: Vec<N>, app: &str) -> p::UiNode {
    node(p::Role::Application, Some(app), full())
        .children(vec![node(p::Role::Window, None, full()).children(children)])
        .build()
}

// MARK: - Live view privacy vs the core's redaction

#[test]
fn screen_text_is_redacted() {
    let tree = screen_of(
        vec![
            node(
                p::Role::StaticText,
                Some("Your Apple ID code is 482913. Do not share it."),
                rect(16.0, 300.0, 361.0, 60.0),
            ),
            node(
                p::Role::Cell,
                Some("Visa 4111 1111 1111 1111"),
                rect(16.0, 400.0, 361.0, 44.0),
            ),
        ],
        "Messages",
    );
    let mut run = Run::new("probe");
    run.observe(pobserved(tree));
    run.act(tap(100.0, 320.0));
    let t = run.timeline();
    let text = json(&t);
    let caption = &t.steps.last().unwrap().caption;
    assert!(!caption.contains("482913"), "caption: {caption}");
    assert!(
        !text.contains("482913"),
        "the OTP is in the encoded timeline"
    );
    assert!(
        !text.contains("4111 1111 1111 1111"),
        "the card number is in the encoded timeline"
    );
}

#[test]
fn typed_otp_hidden() {
    let tree = screen_of(
        vec![node(
            p::Role::TextField,
            Some("Verification code"),
            rect(16.0, 200.0, 361.0, 44.0),
        )
        .focused(true)],
        "Bank",
    );
    let mut run = Run::new("probe");
    run.observe(pobserved(tree));
    run.act_typed(type_text("482913"), type_action(None, "482913"));
    let t = run.timeline();
    assert!(
        !json(&t).contains("482913"),
        "caption: {}",
        t.steps.last().unwrap().caption
    );
}

#[test]
fn typed_card_hidden() {
    let tree = screen_of(
        vec![node(
            p::Role::TextField,
            Some("Card number"),
            rect(16.0, 200.0, 361.0, 44.0),
        )
        .focused(true)],
        "Shop",
    );
    let mut run = Run::new("probe");
    run.observe(pobserved(tree));
    run.act_typed(
        type_text("4111 1111 1111 1111"),
        type_action(None, "4111 1111 1111 1111"),
    );
    let t = run.timeline();
    assert!(
        !json(&t).contains("4111"),
        "caption: {}",
        t.steps.last().unwrap().caption
    );
}

// MARK: - Secure-flagged field on a runner-text screen

#[test]
fn secure_flagged_text_field() {
    let prompt = "<<<SCREEN_DATA>>>\nApp: Bank (com.example.bank)\n[1] TextField \"Passcode\" (focused)\n<<<END_SCREEN_DATA>>>";
    let mut run = Run::new("probe");
    run.observe(observed_text("com.example.bank", "Bank", false));
    run.prompt(prompt);
    run.act_typed(type_text("731904"), type_action(Some(1), "731904"));
    let t = run.timeline();
    assert!(
        !json(&t).contains("731904"),
        "caption: {}",
        t.steps.last().unwrap().caption
    );
}

// MARK: - Forward-compatible decoding

fn sample_json() -> String {
    let tree = screen_of(
        vec![node(
            p::Role::Button,
            Some("Send"),
            rect(16.0, 400.0, 361.0, 44.0),
        )],
        "Mail",
    );
    let mut run = Run::new("probe");
    run.observe(pobserved(tree));
    run.act(tap(100.0, 420.0));
    run.act(p::RunnerCommand::Swipe {
        from: p::Point::new(200.0, 700.0),
        to: p::Point::new(200.0, 200.0),
        duration_ms: 300,
        expect: None,
    });
    json(&run.timeline())
}

#[test]
fn unknown_gesture() {
    let text = sample_json().replace("\"kind\":\"swipe\"", "\"kind\":\"pinch\"");
    assert!(text.contains("pinch"));
    assert!(
        ViewerTimeline::decode(text.as_bytes()).is_ok(),
        "one unknown gesture made the whole timeline undecodable"
    );
}

#[test]
fn unknown_others() {
    let base = sample_json();
    for (old, new) in [
        ("\"source\":\"tree\"", "\"source\":\"mirror\""),
        ("\"kind\":\"done\"", "\"kind\":\"skipped\""),
        ("\"kind\":\"idle\"", "\"kind\":\"paused\""),
        ("\"kind\":\"acting\"", "\"kind\":\"paused\""),
    ] {
        if !base.contains(old) {
            continue;
        }
        let text = base.replace(old, new);
        assert!(
            ViewerTimeline::decode(text.as_bytes()).is_ok(),
            "{new} made the whole timeline undecodable"
        );
    }
}

// MARK: - One privacy rule, decided by the core

const OTP: (&str, &str) = ("Your verification code is 482913.", "482913");
const CARD: (&str, &str) = ("Card: 5555 5555 5555 4444 exp 12/29", "5555 5555 5555 4444");
const IBAN: (&str, &str) = (
    "IBAN: GB82 WEST 1234 5698 7654 32",
    "GB82 WEST 1234 5698 7654 32",
);
const KEY: (&str, &str) = (
    "key sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N here",
    "sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N",
);

fn typed_field(
    label: Option<&str>,
    placeholder: Option<&str>,
    identifier: Option<&str>,
    secure: bool,
) -> p::UiNode {
    let mut field = node(p::Role::TextField, label, rect(16.0, 200.0, 361.0, 44.0))
        .focused(true)
        .secure(secure);
    if let Some(ph) = placeholder {
        field = field.placeholder(ph);
    }
    if let Some(id) = identifier {
        field = field.identifier(id);
    }
    screen_of(vec![field], "Shop")
}

fn type_step(root: p::UiNode, text: &str) -> (ViewerStep, String) {
    let mut run = Run::new("probe");
    run.observe(pobserved(root));
    run.act_typed(type_text(text), type_action(None, text));
    let t = run.timeline();
    (t.steps.last().unwrap().clone(), json(&t))
}

#[test]
fn labels_and_captions() {
    let samples = [OTP, CARD, IBAN, KEY];
    let mut children = Vec::new();
    for (i, sample) in samples.iter().enumerate() {
        children.push(node(
            p::Role::Button,
            Some(sample.0),
            rect(16.0, 100.0 + 60.0 * i as f64, 361.0, 44.0),
        ));
    }
    children
        .push(node(p::Role::TextField, None, rect(16.0, 500.0, 361.0, 44.0)).placeholder(CARD.0));
    let tree = node(p::Role::Application, Some("App"), full())
        .children(vec![node(p::Role::Window, None, full()).children(children)])
        .build();
    let mut run = Run::new("probe");
    run.add(Update::Effect {
        effect: p::Effect::Observe { screenshot: false },
        number: 1,
    });
    run.add(Update::EffectCompleted {
        effect: p::Effect::Observe { screenshot: false },
        event: observed_full(
            tree,
            PHONE,
            false,
            Some(&format!("Codes {}", OTP.0)),
            "com.example",
        ),
    });
    run.number = 1;
    for i in 0..samples.len() {
        run.act(tap(100.0, 120.0 + 60.0 * i as f64));
    }
    run.act(tap(100.0, 520.0));
    let t = run.timeline();
    let text = json(&t);
    let screen = t.current_screen().unwrap();
    for sample in samples {
        assert!(
            !text.contains(sample.1),
            "{} in the encoded timeline",
            sample.1
        );
        assert!(!screen.elements.iter().any(|e| e
            .label
            .as_deref()
            .unwrap_or_default()
            .contains(sample.1)));
        assert!(!t.steps.iter().any(|s| s.caption.contains(sample.1)));
    }
    // What is shown is the core's redaction of the label, capped.
    for (i, sample) in samples.iter().enumerate() {
        let expected = privacy::label(Some(sample.0)).unwrap();
        assert_eq!(
            expected,
            privacy::capped(
                &jarvis_safety::redact(sample.0),
                privacy::MAX_LABEL_CHARACTERS
            )
        );
        assert_eq!(
            t.steps[i].caption,
            format!("Tap \u{201C}{expected}\u{201D}")
        );
        assert!(expected.contains("[redacted"));
    }
    assert!(
        screen
            .elements
            .last()
            .unwrap()
            .label
            .as_deref()
            .unwrap()
            .contains("[redacted card]"),
        "the placeholder is redacted too"
    );
    assert!(!screen.app_name.as_deref().unwrap().contains("482913"));
}

#[test]
fn redact_before_cap() {
    let long = format!("{} card 4111 1111 1111 1111", "x".repeat(30));
    let label = privacy::label(Some(&long)).unwrap();
    assert!(!label.contains("4111"), "{label}");
    assert_eq!(privacy::label(Some("Send")).as_deref(), Some("Send"));
    assert!(privacy::label(Some("")).is_none() && privacy::label(None).is_none());
}

#[test]
fn links() {
    assert_eq!(
        privacy::link_of("https://user:pw@Example.com:8443/path?q=1").as_deref(),
        Some("example.com")
    );
    assert_eq!(
        privacy::link_of("http://example.org").as_deref(),
        Some("example.org")
    );
    assert_eq!(
        privacy::link_of("myapp://SECRETTOKEN123/reset?code=1").as_deref(),
        Some("myapp:")
    );
    assert_eq!(
        privacy::host_of("myapp://SECRETTOKEN123/reset"),
        None,
        "a custom scheme's host can be a token"
    );
    assert_eq!(
        privacy::link_of("tel:+15551234567").as_deref(),
        Some("tel:")
    );
    assert_eq!(privacy::link_of("notaurl"), None);
    assert_eq!(privacy::link_of("1bad:thing"), None);
    let mut run = Run::new("probe");
    run.act(p::RunnerCommand::OpenUrl {
        url: "myapp://SECRETTOKEN123/reset".into(),
    });
    run.act(p::RunnerCommand::OpenUrl {
        url: "https://Example.com/a".into(),
    });
    let t = run.timeline();
    assert_eq!(
        t.steps
            .iter()
            .map(|s| s.caption.as_str())
            .collect::<Vec<_>>(),
        vec!["Open a myapp: link", "Open example.com"]
    );
    assert_eq!(t.steps[0].gesture, Gesture::OpenUrl("myapp:".into()));
    assert!(!json(&t).contains("SECRETTOKEN"));
}

#[test]
fn credential_like_fields() {
    let cases = [
        (typed_field(Some("CVV"), None, None, false), "CVV"),
        (
            typed_field(Some("Card number"), None, None, false),
            "Card number",
        ),
        (typed_field(Some("IBAN"), None, None, false), "IBAN"),
        (
            typed_field(Some("One-time code"), None, None, false),
            "One-time code",
        ),
        (
            typed_field(None, Some("Verification code"), None, false),
            "Verification code",
        ),
        (
            typed_field(None, Some("Card number"), None, false),
            "Card number",
        ),
        (typed_field(Some("Name"), None, None, true), "Name"),
    ];
    for (tree, label) in cases {
        let (step, text) = type_step(tree, "731904");
        assert_eq!(
            step.caption,
            format!("Type into \u{201C}{label}\u{201D}"),
            "{label}"
        );
        assert_eq!(step.gesture, Gesture::Type(None));
        assert!(!text.contains("731904"), "{label}");
    }
    // An identifier the core would use as the label.
    let (step, text) = type_step(
        typed_field(None, None, Some("Verification code"), false),
        "731904",
    );
    assert!(step.caption == "Type text" && !text.contains("731904"));
    // The core's answer is what decides.
    assert!(privacy::field_value_hidden(
        &p::Role::TextField,
        Some("Card number"),
        None,
        None,
        false
    ));
    assert!(privacy::field_value_hidden(
        &p::Role::TextField,
        None,
        Some("CVV"),
        None,
        false
    ));
    assert!(!privacy::field_value_hidden(
        &p::Role::TextField,
        Some("Email"),
        None,
        None,
        false
    ));
}

#[test]
fn redacted_text() {
    for sample in [OTP, CARD, IBAN, KEY] {
        let (step, text) = type_step(typed_field(Some("Message"), None, None, false), sample.0);
        assert_eq!(
            step.caption, "Type into \u{201C}Message\u{201D}",
            "{}",
            sample.0
        );
        assert!(!text.contains(sample.1));
    }
    let (plain, _) = type_step(
        typed_field(Some("Message"), None, None, false),
        "See you at 7",
    );
    assert_eq!(
        plain.caption, "Type \u{201C}See you at 7\u{201D}",
        "plain text in a plain field is still shown"
    );
}

#[test]
fn runner_text_screens() {
    let prompt = "<<<SCREEN_DATA>>>\nApp: Notes (com.apple.mobilenotes)\n[1] TextField \"Title\"\n\
                  [2] TextView \"Code 482913 is your login code\" (focused)\n<<<END_SCREEN_DATA>>>";
    for element in [Some(1), Some(2), None] {
        let mut run = Run::new("probe");
        run.observe(observed_text("com.apple.mobilenotes", "Notes", false));
        run.prompt(prompt);
        run.act_typed(type_text("Groceries"), type_action(element, "Groceries"));
        let t = run.timeline();
        let step = t.steps.last().unwrap();
        let text = json(&t);
        assert_eq!(step.gesture, Gesture::Type(None));
        assert!(
            !text.contains("Groceries") && !text.contains("482913"),
            "{element:?}"
        );
        if element == Some(1) {
            assert_eq!(step.caption, "Type into \u{201C}Title\u{201D}");
        }
    }
}

// MARK: - Decoding from a newer sender

#[test]
fn unknowns_round_trip() {
    let screen = ViewerScreen {
        id: 1,
        size: ViewerSize::new(393.0, 852.0),
        app_name: None,
        bundle_id: "b".into(),
        source: ScreenSource::Unknown("mirror".into()),
        elements: vec![],
        elements_truncated: false,
        frame_key: None,
        focused_input_secure: None,
    };
    let step = ViewerStep {
        number: 1,
        core_step: None,
        screen_id: Some(1),
        gesture: Gesture::Unknown("pinch".into()),
        target: None,
        caption: "Pinch".into(),
        state: StepState::Unknown("skipped".into()),
    };
    let stopped = ViewerStep {
        number: 2,
        core_step: None,
        screen_id: Some(1),
        gesture: Gesture::Wait(5),
        target: None,
        caption: "Wait".into(),
        state: StepState::Stopped,
    };
    let t = ViewerTimeline {
        task_id: "x".into(),
        steps: vec![step, stopped],
        screens: vec![screen],
        current_screen_id: Some(1),
        activity: Activity::Unknown("paused".into()),
        ..ViewerTimeline::default()
    };
    let text = json(&t);
    for tag in [
        r#""kind":"pinch""#,
        r#""kind":"skipped""#,
        r#""kind":"paused""#,
        r#""source":"mirror""#,
        r#""kind":"stopped""#,
    ] {
        assert!(text.contains(tag), "{tag}");
    }
    let back = ViewerTimeline::decode(text.as_bytes()).unwrap();
    assert_eq!(back, t);
    assert_eq!(back.encode(), t.encode(), "encoding is stable");
    // Known values keep their tags.
    for source in [
        ScreenSource::Tree,
        ScreenSource::RunnerText,
        ScreenSource::Locked,
    ] {
        let again: ScreenSource =
            serde_json::from_str(&serde_json::to_string(&source).unwrap()).unwrap();
        assert_eq!(again, source);
    }
    assert_eq!(ScreenSource::RunnerText.tag(), "runner_text");
    // An outcome this reader does not know.
    let finished: Activity =
        serde_json::from_str(r#"{"kind":"finished","outcome":"exploded"}"#).unwrap();
    assert_eq!(finished, Activity::Unknown("finished".into()));
}

#[test]
fn unknowns_rendering() {
    let g = Gesture::Unknown("pinch".into());
    assert!(!g.is_pointer() && g.kind() == "pinch");
    let rest = ViewerPoint::new(10.0, 20.0);
    let pose = cursor::pose(&g, rest, ViewerSize::FALLBACK_PHONE, 100.0, false);
    assert!(pose.position == rest && pose.phase == Phase::Parked);
    assert!(
        !StepState::Unknown("x".into()).is_failed()
            && StepState::Unknown("x".into()).title().is_none()
    );
    assert!(
        Activity::Unknown("paused".into()).caption() == "Working…"
            && Activity::Unknown("paused".into()).is_idle()
    );
}

// MARK: - Pure pieces the view reads

fn taps_timeline(points: &[(f64, f64)], finished: bool) -> ViewerTimeline {
    let mut run = Run::new("probe");
    run.observe(pobserved(
        node(p::Role::Application, Some("App"), full()).build(),
    ));
    for &(x, y) in points {
        run.act(tap(x, y));
    }
    run.add(Update::Effect {
        effect: infer(),
        number: 99,
    });
    if finished {
        run.add(Update::Finished(p::OutcomeStatus::Success));
    }
    run.timeline()
}

#[test]
fn needs_frames() {
    let running = taps_timeline(&[(100.0, 100.0), (300.0, 700.0)], false);
    let mut live = ReplayState::new();
    live.sync(&running.step_numbers(), 0.0, None);
    // The newest step just started: in flight.
    assert!(scene::needs_frames(&running, &live, 10.0, false));
    // Played out, Jarvis thinking: the idle pulse needs frames, but not under Reduce Motion.
    assert!(scene::needs_frames(&running, &live, 100_000.0, false));
    assert!(!scene::needs_frames(&running, &live, 100_000.0, true));

    let finished = taps_timeline(&[(100.0, 100.0), (300.0, 700.0)], true);
    let numbers = finished.step_numbers();
    let mut replay = ReplayState::new();
    replay.sync(&numbers, 0.0, None);
    assert!(
        !scene::needs_frames(&finished, &replay, 100_000.0, false),
        "a finished run, following, settled: static"
    );
    replay.step_back(&numbers, 200_000.0);
    assert!(
        scene::needs_frames(&finished, &replay, 200_050.0, false),
        "the scrubbed step animates"
    );
    assert!(
        !scene::needs_frames(&finished, &replay, 300_000.0, false),
        "paused replay of a finished run: static"
    );
    replay.toggle_play(&numbers, 300_000.0);
    assert!(
        scene::needs_frames(&finished, &replay, 10_000_000.0, false),
        "playing"
    );
    assert!(
        !scene::needs_frames(
            &ViewerTimeline::default(),
            &ReplayState::new(),
            5_000.0,
            false
        ),
        "no run: static"
    );
}

#[test]
fn aspect_fit() {
    let r = ViewerRect::new(10.0, 20.0, 200.0, 400.0);
    assert_eq!(
        r.aspect_fit(ViewerSize::new(393.0, 786.0)),
        ViewerRect::new(10.0, 20.0, 200.0, 400.0)
    );
    assert_eq!(
        r.aspect_fit(ViewerSize::new(400.0, 200.0)),
        ViewerRect::new(10.0, 170.0, 200.0, 100.0)
    );
    assert_eq!(
        r.aspect_fit(ViewerSize::new(100.0, 400.0)),
        ViewerRect::new(60.0, 20.0, 100.0, 400.0)
    );
    assert_eq!(r.aspect_fit(ViewerSize::new(0.0, 10.0)), r);
    assert_eq!(r.aspect_fit(ViewerSize::new(f64::NAN, 10.0)), r);
    assert_eq!(
        ViewerRect::ZERO.aspect_fit(ViewerSize::new(1.0, 1.0)),
        ViewerRect::ZERO
    );
    let m = ViewerMapping::new(ViewerSize::new(393.0, 852.0), ViewerSize::new(300.0, 600.0));
    let fitted = m.content_rect().aspect_fit(ViewerSize::new(786.0, 1704.0));
    assert!((fitted.width - m.content_rect().width).abs() < 1e-9);
    assert!((fitted.height - m.content_rect().height).abs() < 1e-9);
}

#[test]
fn live_follow_continuity() {
    let one = taps_timeline(&[(20.0, 20.0)], false);
    let mut replay = scene::synced(&ReplayState::new(), &one, &one.step_numbers(), 0.0, false);
    // 100 ms into the first glide (it starts at the centre).
    let mid = scene::make(&one, &replay, 100.0, false).pose.position;
    assert!(mid != ViewerPoint::new(20.0, 20.0) && mid != ViewerSize::FALLBACK_PHONE.center());
    let two = taps_timeline(&[(20.0, 20.0), (380.0, 800.0)], false);
    replay = scene::synced(&replay, &two, &two.step_numbers(), 100.0, false);
    assert_eq!(replay.current(), Some(2));
    let start = scene::make(&two, &replay, 100.0, false).pose.position;
    assert!(
        start.distance(mid) < 1e-6,
        "the new step starts where the cursor was ({mid:?}), not at {start:?}"
    );
    let end = scene::make(
        &two,
        &replay,
        100.0 + cursor::play_ms_of_step(2, &two, replay.origin(), false),
        false,
    );
    assert!(end.pose.position == ViewerPoint::new(380.0, 800.0) || end.step.is_none());
    // A step arriving after the previous one played out starts from its rest point.
    let mut settled = scene::synced(&ReplayState::new(), &one, &one.step_numbers(), 0.0, false);
    settled = scene::synced(&settled, &two, &two.step_numbers(), 50_000.0, false);
    assert_eq!(settled.origin(), Some(ViewerPoint::new(20.0, 20.0)));
    let first = scene::make(&two, &settled, 50_000.0, false).pose.position;
    assert_eq!(first, ViewerPoint::new(20.0, 20.0));
    // Replay never carries an origin.
    settled.step_back(&two.step_numbers(), 60_000.0);
    assert_eq!(settled.origin(), None);
}

// MARK: - Stopped steps

#[test]
fn stopped() {
    let mut run = Run::new("probe");
    run.observe(pobserved(
        node(p::Role::Application, Some("App"), full()).build(),
    ));
    run.number += 1;
    run.add(Update::Effect {
        effect: p::Effect::Act(tap(1.0, 1.0)),
        number: run.number,
    });
    run.add(Update::EffectCompleted {
        effect: p::Effect::Act(tap(1.0, 1.0)),
        event: p::Event::Cancel,
    });
    run.number += 1;
    run.add(Update::Effect {
        effect: p::Effect::Act(tap(2.0, 2.0)),
        number: run.number,
    });
    run.add(Update::Finished(p::OutcomeStatus::Cancelled));
    let t = run.timeline();
    assert_eq!(
        t.steps.iter().map(|s| s.state.clone()).collect::<Vec<_>>(),
        vec![StepState::Stopped, StepState::Stopped]
    );
    assert!(t
        .steps
        .iter()
        .all(|s| !s.state.is_failed() && s.state.title() == Some("Stopped")));
    let numbers = t.step_numbers();
    let mut replay = ReplayState::new();
    replay.sync(&numbers, 0.0, None);
    replay.step_back(&numbers, 0.0);
    assert!(scene::make(&t, &replay, 0.0, false)
        .accessibility_value()
        .contains("stopped"));
}
