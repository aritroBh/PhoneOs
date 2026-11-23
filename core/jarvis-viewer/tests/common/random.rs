//! The random generators of `properties.rs`, shared with the delta stream tests (copied
//! verbatim so the original test file stays as it is).

#![allow(dead_code, clippy::cast_precision_loss)]

use super::*;
use jarvis_proto as p;
use jarvis_viewer::{Update, ViewerTimeline, MAX_ELEMENTS, MAX_SCREENS, MAX_STEPS};

pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    pub fn f(&mut self, lo: f64, hi: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        lo + (hi - lo) * unit
    }
    pub fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[usize::try_from(self.below(items.len() as u64)).unwrap()]
    }
}

/// A random run's updates, in the driver's order.
pub fn random_run(rng: &mut Rng, secrets: &[&str]) -> Run {
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
            if rng.below(4) == 0 {
                run.observe(observed_text("com.example", &app, locked));
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

/// Secrets the core's redaction always masks, whatever surrounds them (separated by spaces).
pub const SECRETS: [&str; 5] = [
    "4111 1111 1111 1111",
    "5555 5555 5555 4444",
    "GB82 WEST 1234 5698 7654 32",
    "sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N",
    "ghp_16C7e42F292c6912E7710c838347Ae178B4a",
];

pub fn random_update(rng: &mut Rng) -> Update {
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

pub fn check_invariants(t: &ViewerTimeline) {
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
