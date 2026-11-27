//! Property-style tests over seeded random inputs (a small xorshift generator, no extra
//! dependency): cursor continuity and monotone progress, encode → decode → encode
//! stability, redaction of a secret corpus embedded in random labels, and a reducer that
//! never panics nor breaks its bounds on arbitrary update orders (out of order, duplicated,
//! interleaved runs).

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
use jarvis_viewer::{
    scene, Gesture, LiveView, ReplayAction, ReplayState, ScreenSource, StepState, Update,
    ViewerPoint, ViewerScreen, ViewerSize, ViewerStep, ViewerTimeline, MAX_ELEMENTS, MAX_SCREENS,
    MAX_STEPS,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn f(&mut self, lo: f64, hi: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        lo + (hi - lo) * unit
    }
    fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[usize::try_from(self.below(items.len() as u64)).unwrap()]
    }
}

fn random_point(rng: &mut Rng) -> ViewerPoint {
    match rng.below(10) {
        0 => ViewerPoint::new(f64::NAN, rng.f(0.0, 852.0)),
        1 => ViewerPoint::new(rng.f(-2_000.0, 2_000.0), f64::INFINITY),
        _ => ViewerPoint::new(rng.f(-100.0, 500.0), rng.f(-100.0, 950.0)),
    }
}

fn random_gesture(rng: &mut Rng) -> Gesture {
    match rng.below(8) {
        0 | 1 => Gesture::Tap(random_point(rng)),
        2 => Gesture::LongPress {
            point: random_point(rng),
            duration_ms: rng.below(5_000),
        },
        3 | 4 => Gesture::Swipe {
            from: random_point(rng),
            to: random_point(rng),
            duration_ms: rng.below(3_000),
        },
        5 => Gesture::Type(None),
        6 => Gesture::Wait(rng.below(3_000)),
        _ => Gesture::Key("Return".into()),
    }
}

#[test]
fn cursor_continuity_and_monotone_progress_over_random_steps() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..200 {
        let sizes = [
            ViewerSize::new(393.0, 852.0),
            ViewerSize::new(852.0, 393.0),
            ViewerSize::new(0.0, 0.0),
        ];
        let screens: Vec<ViewerScreen> = sizes
            .iter()
            .enumerate()
            .map(|(i, s)| ViewerScreen {
                id: u32::try_from(i).unwrap() + 1,
                size: *s,
                app_name: None,
                bundle_id: "b".into(),
                source: ScreenSource::Tree,
                elements: vec![],
                elements_truncated: false,
                frame_key: None,
                focused_input_secure: None,
            })
            .collect();
        // Steps on one screen per round, so the previous rest point is on the same screen.
        let screen_id = u32::try_from(round % 3).unwrap() + 1;
        let size = sizes[usize::try_from(screen_id - 1).unwrap()];
        let steps: Vec<ViewerStep> = (1..=20)
            .map(|n| ViewerStep {
                number: n,
                core_step: None,
                screen_id: Some(screen_id),
                gesture: random_gesture(&mut rng),
                target: None,
                caption: String::new(),
                state: StepState::Done,
            })
            .collect();
        let t = ViewerTimeline {
            task_id: "t".into(),
            steps,
            screens,
            current_screen_id: Some(screen_id),
            ..ViewerTimeline::default()
        };
        let usable = size.usable();
        for reduce_motion in [false, true] {
            for w in t.steps.windows(2) {
                let end = cursor::pose_of_step(w[0].number, &t, 1e12, None, reduce_motion).unwrap();
                let start =
                    cursor::pose_of_step(w[1].number, &t, 0.0, None, reduce_motion).unwrap();
                if !reduce_motion || !w[1].gesture.is_pointer() {
                    assert!(
                        end.position.distance(start.position) < 1e-9,
                        "round {round}: jump between {} and {}",
                        w[0].number,
                        w[1].number
                    );
                }
            }
            for step in &t.steps {
                let rest = cursor::resting_point_before(step.number, &t);
                let timing = cursor::timing(&step.gesture, rest, size, reduce_motion);
                let total = cursor::play_ms_of_step(step.number, &t, None, reduce_motion);
                assert!(total.is_finite() && total >= cursor::SETTLE_MS);
                let press = step.gesture.press_point().map(|p| size.clamp(p));
                let mut last_distance = f64::INFINITY;
                let mut last_phase_rank = 0;
                let mut last_progress = -1.0;
                let mut last_phase = None;
                for i in 0..=120 {
                    let at = timing.total_ms() * 1.1 * f64::from(i) / 120.0;
                    let pose = cursor::pose(&step.gesture, rest, size, at, reduce_motion);
                    let p = pose.position;
                    assert!(p.is_finite());
                    assert!(
                        p.x >= 0.0 && p.x <= usable.width && p.y >= 0.0 && p.y <= usable.height
                    );
                    assert!((0.0..=1.0).contains(&pose.progress) || pose.phase == Phase::Gliding);
                    let rank = match pose.phase {
                        Phase::Gliding => 0,
                        Phase::Pressing | Phase::Swiping => 1,
                        Phase::Resting => 2,
                        Phase::Parked => 1,
                    };
                    assert!(rank >= last_phase_rank, "phases never go back");
                    if last_phase == Some(pose.phase) {
                        assert!(
                            pose.progress >= last_progress - 1e-12,
                            "progress is monotone"
                        );
                    }
                    if pose.phase == Phase::Gliding {
                        let d = p.distance(press.unwrap());
                        assert!(d <= last_distance + 1e-9, "the glide approaches its target");
                        last_distance = d;
                    }
                    last_phase_rank = rank;
                    last_progress = pose.progress;
                    last_phase = Some(pose.phase);
                }
            }
        }
    }
}

/// `root` written in the runner's text format (`Type, label: '…', value: '…', id: '…',
/// {{x, y}, {w, h}}`, two spaces per depth).
fn runner_text(root: &p::UiNode) -> String {
    fn line(n: &p::UiNode, depth: usize, out: &mut String) {
        let role = jarvis_viewer::reducer::role_name(&n.role);
        let mut s = format!("{}{role}", "  ".repeat(depth));
        for (key, v) in [
            ("label", n.label.as_deref()),
            ("value", n.value.as_deref()),
            ("id", n.identifier.as_deref()),
        ] {
            if let Some(v) = v.filter(|v| !v.is_empty()) {
                s.push_str(&format!(", {key}: '{v}'"));
            }
        }
        let f = n.frame;
        s.push_str(&format!(
            ", {{{{{}, {}}}, {{{}, {}}}}}\n",
            f.x, f.y, f.width, f.height
        ));
        out.push_str(&s);
        for c in &n.children {
            line(c, depth + 1, out);
        }
    }
    let mut out = String::new();
    line(root, 0, &mut out);
    out
}

/// An observation of `root` as runner text, parsed exactly as the session parses it.
fn observed_parsed_text(root: &p::UiNode, app: &str, locked: bool) -> p::Event {
    let text = runner_text(root);
    let parsed = jarvis_observe::parse_runner_text(&text).expect("parses");
    p::Event::ObservedRunnerText(p::Observation {
        screen: p::Screen {
            tree_cut: false,
            app_bundle_id: "com.example".to_owned(),
            app_name: Some(app.to_owned()),
            size: PHONE,
            root: parsed.root,
            keyboard_visible: false,
            orientation: p::Orientation::Portrait,
        },
        screenshot: None,
        device_locked: locked,
    })
}

/// A random run's updates, in the driver's order.
fn random_run(rng: &mut Rng, secrets: &[&str]) -> Run {
    let mut run = Run::new(&format!("task-{}", rng.below(1_000)));
    let words = [
        "Send", "Wi-Fi", "Name", "Message", "Total", "Pay", "Code", "Account", "Hi",
    ];
    let label = |rng: &mut Rng| -> String {
        let mut s = String::new();
        for _ in 0..rng.below(4) {
            s.push_str(rng.pick(&words));
            s.push(' ');
        }
        if !secrets.is_empty() && rng.below(3) == 0 {
            s.push_str(rng.pick(secrets));
            s.push(' ');
        }
        for _ in 0..rng.below(3) {
            s.push_str(rng.pick(&words));
            s.push(' ');
        }
        s.trim().to_owned()
    };
    for _ in 0..rng.below(25) + 1 {
        if rng.below(3) == 0 {
            let mut children = Vec::new();
            for i in 0..rng.below(12) {
                let role = rng
                    .pick(&[
                        p::Role::Button,
                        p::Role::TextField,
                        p::Role::SecureTextField,
                        p::Role::StaticText,
                        p::Role::Cell,
                        p::Role::Other("Pager".into()),
                    ])
                    .clone();
                let mut n = node(
                    role,
                    Some(&label(rng)),
                    rect(
                        rng.f(-50.0, 380.0),
                        40.0 * i as f64,
                        rng.f(-5.0, 300.0),
                        40.0,
                    ),
                )
                .value(&label(rng))
                .focused(rng.below(4) == 0);
                if rng.coin() {
                    n = n.placeholder(&label(rng));
                }
                children.push(n);
            }
            let app = label(rng);
            let root = node(
                p::Role::Application,
                Some(&app),
                rect(0.0, 0.0, 393.0, 852.0),
            )
            .children(children)
            .build();
            let locked = rng.below(8) == 0;
            if rng.below(8) == 0 {
                run.observe(observed_text("com.example", &app, locked));
            } else if rng.below(4) == 0 {
                // Preview review: runner text now gets a wireframe; the same tree written as
                // the runner's text and parsed with the session's parser (labels, values and
                // ids carry the secrets too).
                run.observe(observed_parsed_text(&root, &app, locked));
            } else {
                run.observe(observed_full(
                    root,
                    PHONE,
                    locked,
                    Some(&app),
                    "com.example",
                ));
            }
        }
        if rng.below(4) == 0 {
            let mut prompt = String::from("<<<SCREEN_DATA>>>\nApp: X (com.example)\n");
            for i in 1..=rng.below(5) {
                let role = rng.pick(&["TextField", "SecureTextField", "Button", "TextView"]);
                let focused = if rng.coin() { " (focused)" } else { "" };
                prompt.push_str(&format!("[{i}] {role} \"{}\"{focused}\n", label(rng)));
            }
            prompt.push_str("<<<END_SCREEN_DATA>>>");
            run.prompt(&prompt);
        }
        let text = label(rng);
        let command = match rng.below(7) {
            0 | 1 => tap(rng.f(0.0, 393.0), rng.f(0.0, 400.0)),
            2 => p::RunnerCommand::Swipe {
                from: p::Point::new(rng.f(0.0, 393.0), rng.f(0.0, 852.0)),
                to: p::Point::new(rng.f(0.0, 393.0), rng.f(0.0, 852.0)),
                duration_ms: rng.below(2_000),
                expect: None,
            },
            3 | 4 => type_text(&text),
            5 => p::RunnerCommand::OpenUrl {
                url: format!(
                    "https://example.com/{}?q={}",
                    label(rng).replace(' ', "-"),
                    label(rng).replace(' ', "+")
                ),
            },
            _ => p::RunnerCommand::Wait {
                ms: rng.below(3_000),
            },
        };
        let action = if rng.below(5) == 0 {
            None
        } else {
            Some(type_action(
                if rng.coin() {
                    Some(u32::try_from(rng.below(5)).unwrap())
                } else {
                    None
                },
                &text,
            ))
        };
        let result = if rng.below(6) == 0 {
            p::RunnerResult::err(format!("could not: {}", label(rng)))
        } else {
            p::RunnerResult::ok()
        };
        run.act_with(command, action, result);
    }
    if rng.coin() {
        run.add(Update::Finished(*rng.pick(&[
            p::OutcomeStatus::Success,
            p::OutcomeStatus::Cancelled,
            p::OutcomeStatus::Failed,
        ])));
    }
    run
}

#[test]
fn encode_decode_encode_is_stable() {
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for _ in 0..150 {
        let run = random_run(&mut rng, &[]);
        let t = run
            .reducer(config(usize::try_from(rng.below(250)).unwrap(), rng.coin()))
            .timeline()
            .clone();
        let bytes = t.encode();
        let back = ViewerTimeline::decode(&bytes).unwrap();
        assert_eq!(back.encode(), bytes);
        let again = ViewerTimeline::decode(&back.encode()).unwrap();
        assert_eq!(again.encode(), bytes);
    }
}

/// Secrets the core's redaction always masks, whatever surrounds them (separated by spaces).
const SECRETS: [&str; 5] = [
    "4111 1111 1111 1111",
    "5555 5555 5555 4444",
    "GB82 WEST 1234 5698 7654 32",
    "sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N",
    "ghp_16C7e42F292c6912E7710c838347Ae178B4a",
];

#[test]
fn secrets_in_random_labels_never_reach_the_encoding() {
    for secret in SECRETS {
        assert_ne!(
            jarvis_safety::redact(&format!("Total {secret} Hi")),
            format!("Total {secret} Hi")
        );
    }
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for round in 0..300 {
        let run = random_run(&mut rng, &SECRETS);
        let t = run.reducer(config(200, rng.coin())).timeline().clone();
        let text = String::from_utf8(t.encode()).unwrap();
        for secret in SECRETS {
            assert!(!text.contains(secret), "round {round}: {secret} in {text}");
            // Nor a 4-character piece of a card number that only a cut could leave.
            assert!(!text.contains("1111 1111"), "round {round}");
        }
    }
}

fn random_update(rng: &mut Rng) -> Update {
    let effects = [
        p::Effect::Observe { screenshot: false },
        infer(),
        p::Effect::Act(tap(rng.f(0.0, 393.0), rng.f(0.0, 852.0))),
        p::Effect::Act(type_text("abc")),
        p::Effect::AskUser {
            question: "?".into(),
        },
        p::Effect::AwaitUnlock {
            reason: "locked".into(),
            waited_ms: 0,
        },
        p::Effect::Finished(p::Outcome {
            status: p::OutcomeStatus::Success,
            summary: String::new(),
            steps: 0,
            duration_ms: 0,
        }),
    ];
    let events = [
        observed(login_tree(Some("email"))),
        observed_full(login_tree(None), PHONE, true, None, "x"),
        observed_text("com.x", "X", rng.coin()),
        p::Event::ActResult(p::RunnerResult::ok()),
        p::Event::ActResult(p::RunnerResult::device_locked()),
        p::Event::Cancel,
        p::Event::Start,
    ];
    match rng.below(12) {
        0 => Update::Started {
            task_id: format!("t{}", rng.below(3)),
        },
        1 => Update::Prompt(
            "<<<SCREEN_DATA>>>\n[1] SecureTextField \"P\" (focused)\n[2] TextField \"E\"\n<<<END_SCREEN_DATA>>>".into(),
        ),
        2 | 3 => Update::Effect {
            effect: rng.pick(&effects).clone(),
            number: rng.below(50),
        },
        4 => Update::Progress(progress(
            u32::try_from(rng.below(40)).unwrap(),
            Some(type_action(Some(u32::try_from(rng.below(3)).unwrap()), "abc")),
        )),
        5 => Update::ConfirmationPending(p::ConfirmRequest {
            category: p::RiskCategory::Payment,
            summary: String::new(),
            app: String::new(),
            phone_only: false,
            ..Default::default()
        }),
        6 => Update::QuestionPending,
        7 => Update::AwaitingUnlock,
        8..=10 => Update::EffectCompleted {
            effect: rng.pick(&effects).clone(),
            event: rng.pick(&events).clone(),
        },
        _ => Update::Finished(*rng.pick(&[p::OutcomeStatus::Success, p::OutcomeStatus::Cancelled])),
    }
}

fn check_invariants(t: &ViewerTimeline) {
    assert!(t.steps.len() <= MAX_STEPS);
    assert!(t.screens.len() <= MAX_SCREENS);
    for w in t.steps.windows(2) {
        assert_eq!(w[1].number, w[0].number + 1, "numbers are consecutive");
    }
    for w in t.screens.windows(2) {
        assert!(w[0].id < w[1].id, "screens in id order");
    }
    for s in &t.screens {
        assert!(s.elements.len() <= MAX_ELEMENTS);
    }
    for step in &t.steps {
        if let Some(id) = step.screen_id {
            assert!(
                t.screens.iter().any(|s| s.id == id),
                "a kept step's screen is kept"
            );
        }
    }
    if let Some(first) = t.steps.first() {
        assert_eq!(u64::from(first.number), u64::from(t.dropped_steps) + 1);
    }
}

#[test]
fn reducer_never_panics_on_arbitrary_orders() {
    let mut rng = Rng(0x5151_2323_7777_0001);
    for round in 0..300 {
        let mut live = LiveView::new(config(usize::try_from(rng.below(30)).unwrap(), rng.coin()));
        let mut now = rng.f(-10.0, 1e6);
        let mut last: Option<Update> = None;
        for _ in 0..rng.below(300) {
            // Duplicates: replay the previous update now and then.
            let update = match (&last, rng.below(5)) {
                (Some(prev), 0) => prev.clone(),
                _ => random_update(&mut rng),
            };
            last = Some(update.clone());
            live.apply(update);
            check_invariants(live.timeline());
            now += rng.f(-50.0, 500.0);
            if rng.below(4) == 0 {
                let to = u32::try_from(rng.below(60)).unwrap();
                let fraction = rng.f(-1.0, 2.0);
                let action = *rng.pick(&[
                    ReplayAction::StepBack,
                    ReplayAction::StepForward,
                    ReplayAction::TogglePlay,
                    ReplayAction::ScrubTo(to),
                    ReplayAction::ScrubFraction(fraction),
                    ReplayAction::NextSpeed,
                    ReplayAction::GoLive,
                    ReplayAction::Reset,
                ]);
                live.control(action, now, rng.coin());
            }
            let rm = rng.coin();
            let frame = live.frame(if rng.below(20) == 0 { f64::NAN } else { now }, rm);
            assert!(frame.scene.pose.position.is_finite(), "round {round}");
            let geometry = frame.in_view(ViewerSize::new(rng.f(-10.0, 500.0), rng.f(-10.0, 900.0)));
            assert!(geometry.cursor.is_finite());
        }
        let t = live.timeline().clone();
        let back = ViewerTimeline::decode(&t.encode()).unwrap();
        assert_eq!(back.encode(), t.encode());
        // The pure scene functions on any replay state.
        let numbers = t.step_numbers();
        let mut r = ReplayState::new();
        r.sync(&numbers, now, None);
        r.step_back(&numbers, now);
        let _ = scene::make(&t, &r, now + 10.0, false);
        let _ = scene::needs_frames(&t, &r, now + 10.0, true);
    }
}
