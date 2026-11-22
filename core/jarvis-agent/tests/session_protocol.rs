//! The state machine's protocol: cancel everywhere, unexpected events, sticky finish,
//! progress, and a deterministic fuzz over arbitrary event sequences and model output.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{observation, settings_screen};
use jarvis_proto::{
    Effect, Event, ModelOutput, OutcomeStatus, ParamKind, Phase, RiskCategory, RunnerResult,
    TaskSpec, Tier, TimedEvent, ToolParam, ToolResult, ToolSpec,
};

fn session() -> Session {
    let mut c = config();
    c.tools = vec![ToolSpec {
        name: "note".into(),
        description: "Write a note".into(),
        params: vec![ToolParam {
            name: "text".into(),
            description: String::new(),
            kind: ParamKind::String,
            required: true,
        }],
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
    }];
    Session::new(c, TaskSpec::new("t", "Turn off Wi-Fi"), MemoryStore::new())
}

fn ev(now: u64, e: Event) -> TimedEvent {
    TimedEvent::new(now, e)
}

fn observed() -> Event {
    Event::Observed(observation(settings_screen()))
}

fn model(text: &str) -> Event {
    Event::ModelOutput(ModelOutput { text: text.into() })
}

/// Steps `s` through `events`, returning the last effect.
fn drive(s: &mut Session, events: Vec<Event>) -> Effect {
    let mut last = None;
    for (i, e) in events.into_iter().enumerate() {
        last = Some(s.step(ev(10 + i as u64, e)));
    }
    last.unwrap()
}

/// Event prefixes that leave the session in each non-final state.
fn states() -> Vec<(&'static str, Vec<Event>, Phase)> {
    let tap = r#"{"action":"tap","element":4}"#;
    vec![
        ("idle", vec![], Phase::Idle),
        ("observing", vec![Event::Start], Phase::Observing),
        ("thinking", vec![Event::Start, observed()], Phase::Thinking),
        (
            "acting",
            vec![Event::Start, observed(), model(tap)],
            Phase::Acting,
        ),
        (
            "verifying",
            vec![
                Event::Start,
                observed(),
                model(tap),
                Event::ActResult(RunnerResult::ok()),
            ],
            Phase::Verifying,
        ),
        (
            "tool",
            vec![
                Event::Start,
                observed(),
                model(r#"{"action":"call_tool","name":"note","args":{"text":"hi"}}"#),
            ],
            Phase::CallingTool,
        ),
        (
            "confirm",
            vec![
                Event::Start,
                observed(),
                model(r#"{"action":"tap","element":3}"#),
            ],
            Phase::WaitingForUser,
        ),
        (
            "answer",
            vec![
                Event::Start,
                observed(),
                model(r#"{"action":"ask_user","question":"Which?"}"#),
            ],
            Phase::WaitingForUser,
        ),
    ]
}

#[test]
fn each_prefix_reaches_its_state() {
    for (name, events, phase) in states() {
        let mut s = session();
        for (i, e) in events.into_iter().enumerate() {
            let eff = s.step(ev(10 + i as u64, e));
            assert!(!eff.is_finished(), "{name}: finished early with {eff:?}");
        }
        assert_eq!(s.progress().phase, phase, "{name}");
    }
}

#[test]
fn cancel_is_honoured_from_every_state() {
    for (name, events, _) in states() {
        let mut s = session();
        let n = events.len() as u64;
        drive_all(&mut s, events);
        let eff = s.step(ev(100 + n, Event::Cancel));
        match &eff {
            Effect::Finished(o) => assert_eq!(o.status, OutcomeStatus::Cancelled, "{name}"),
            other => panic!("{name}: {other:?}"),
        }
        assert_eq!(
            s.step(ev(200, Event::Start)),
            eff,
            "{name}: finish is sticky"
        );
        assert_eq!(s.step(ev(201, Event::Cancel)), eff, "{name}");
        assert_eq!(s.progress().phase, Phase::Finished);
    }
}

fn drive_all(s: &mut Session, events: Vec<Event>) {
    for (i, e) in events.into_iter().enumerate() {
        s.step(ev(10 + i as u64, e));
    }
}

#[test]
fn unexpected_events_are_protocol_errors_in_every_state() {
    let wrong: Vec<(&str, Event)> = vec![
        ("idle", observed()),
        ("observing", model("{}")),
        ("thinking", Event::ActResult(RunnerResult::ok())),
        ("acting", observed()),
        ("verifying", Event::UserDecision { approved: true }),
        ("tool", Event::ActResult(RunnerResult::ok())),
        ("confirm", Event::UserAnswer { text: "yes".into() }),
        ("answer", Event::Start),
    ];
    for ((name, events, _), (wname, bad)) in states().into_iter().zip(wrong) {
        assert_eq!(name, wname);
        let mut s = session();
        drive_all(&mut s, events);
        match s.step(ev(500, bad)) {
            Effect::Finished(o) => {
                assert_eq!(o.status, OutcomeStatus::Failed, "{name}");
                assert!(
                    o.summary.starts_with("protocol error: unexpected"),
                    "{name}: {}",
                    o.summary
                );
            }
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn start_twice_is_a_protocol_error() {
    let mut s = session();
    s.step(ev(1, Event::Start));
    let Effect::Finished(o) = s.step(ev(2, Event::Start)) else {
        panic!()
    };
    assert_eq!(
        o.summary,
        "protocol error: unexpected start while waiting for an observation"
    );
}

#[test]
fn model_failure_is_retried_twice_then_fails_the_session() {
    let mut s = session();
    let first = drive(&mut s, vec![Event::Start, observed()]);
    let Effect::Infer(request) = first else {
        panic!("{first:?}")
    };
    let fail = || Event::ModelFailed {
        error: "out of memory".into(),
    };
    // Two retries of exactly the same request...
    for attempt in 1..=2u32 {
        let eff = s.step(ev(100 + u64::from(attempt), fail()));
        assert_eq!(eff, Effect::Infer(request.clone()), "retry {attempt}");
        assert_eq!(s.progress().phase, Phase::Thinking);
        assert_eq!(s.progress().model_calls, 1 + attempt);
    }
    // ...then the third failure ends the session.
    let eff = s.step(ev(200, fail()));
    let Effect::Finished(o) = eff else {
        panic!("{eff:?}")
    };
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert_eq!(o.summary, "Model error: out of memory");
    let mem = s.into_memory();
    assert_eq!(mem.log.len(), 1);
}

#[test]
fn model_recovers_after_a_transient_failure_and_counts_calls() {
    let mut s = session();
    let eff = drive(
        &mut s,
        vec![
            Event::Start,
            observed(),
            Event::ModelFailed {
                error: "model still loading".into(),
            },
            model("not json at all"),
            model(r#"{"action":"tap","element":4}"#),
        ],
    );
    assert!(matches!(eff, Effect::Act(_)), "{eff:?}");
    let p = s.progress();
    // First request, its retry after the failure, and the re-inference after the
    // unparsable reply.
    assert_eq!(p.model_calls, 3);
    assert_eq!(p.parse_retries, 1);
    // The retry budget is per request: two more failures are still retried.
    let mut s = session();
    drive(&mut s, vec![Event::Start, observed()]);
    for i in 0..2 {
        let eff = s.step(ev(50 + i, Event::ModelFailed { error: "x".into() }));
        assert!(matches!(eff, Effect::Infer(_)), "{eff:?}");
    }
    let eff = s.step(ev(60, model(r#"{"action":"wait","ms":100}"#)));
    assert!(matches!(eff, Effect::Act(_)), "{eff:?}");
}

#[test]
fn model_failures_count_against_the_failure_budget() {
    let mut c = config();
    c.policy.max_consecutive_failures = 2;
    let mut s = Session::new(c, TaskSpec::new("t", "Turn off Wi-Fi"), MemoryStore::new());
    drive(&mut s, vec![Event::Start, observed()]);
    let eff = s.step(ev(50, Event::ModelFailed { error: "x".into() }));
    assert!(matches!(eff, Effect::Infer(_)), "{eff:?}");
    // The second failure reaches the consecutive-failure budget before the retry cap.
    let eff = s.step(ev(51, Event::ModelFailed { error: "x".into() }));
    let Effect::Finished(o) = eff else {
        panic!("{eff:?}")
    };
    assert_eq!(o.status, OutcomeStatus::Failed);
    assert_eq!(o.summary, "Model error: x");
}

#[test]
fn progress_reports_step_action_and_app() {
    let mut s = session();
    assert_eq!(s.progress().app, None);
    drive_all(
        &mut s,
        vec![
            Event::Start,
            observed(),
            model(r#"{"action":"tap","element":4}"#),
        ],
    );
    let p = s.progress();
    assert_eq!(p.step, 1);
    assert_eq!(p.phase, Phase::Acting);
    assert_eq!(p.app.as_deref(), Some("com.apple.Preferences"));
    assert_eq!(
        p.last_action,
        Some(jarvis_proto::Action::Tap { element: 4 })
    );
}

#[test]
fn identical_inputs_give_identical_effects() {
    let a = run(
        config(),
        "Open General",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[r#"{"action":"tap","element":7}"#, "done()"]),
    );
    let b = run(
        config(),
        "Open General",
        MemoryStore::new(),
        Phone::still(settings_screen()),
        Script::model(&[r#"{"action":"tap","element":7}"#, "done()"]),
    );
    assert_eq!(a.effects, b.effects);
    assert_eq!(a.memory, b.memory);
}

// ---------------------------------------------------------------------------------------
// Fuzz: a hand-written deterministic PRNG (xorshift64*).
// ---------------------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

const FRAGMENTS: &[&str] = &[
    r#"{"action":"tap","element":4}"#,
    r#"{"action":"tap","element":3}"#,
    r#"{"action":"type","element":2,"text":"hi","submit":true}"#,
    r#"{"action":"type","text":"{{profile.email}}"}"#,
    r#"{"action":"scroll","direction":"down"}"#,
    r#"{"action":"swipe","direction":"left","element":5}"#,
    r#"{"action":"tap_point","x":0.4,"y":0.9}"#,
    r#"{"action":"open_app","app":"Passwords"}"#,
    r#"{"action":"open_app","app":"Settings"}"#,
    r#"{"action":"open_url","url":"javascript:alert(1)"}"#,
    r#"{"action":"call_tool","name":"note","args":{"text":"x"}}"#,
    r#"{"action":"call_tool","name":"nope"}"#,
    r#"{"action":"ask_user","question":"?"}"#,
    r#"{"action":"look"}"#,
    r#"{"action":"wait","ms":99999999}"#,
    r#"{"action":"done"}"#,
    "tap(7)",
    "type(4, \"x\")",
    "{'action': 'clear', 'element': '2',}",
    "```json\n{\"action\":\"long_press\",\"element\":1}\n```",
    "{\"thought\":\"",
    "}}}{{{",
    "\u{0}\u{1}\u{202e}\u{feff}",
    "[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[",
    "tap(",
    "done(\"ok\") fail(\"no\")",
    "{\"action\":null}",
    "{\"action\":\"tap\",\"element\":-1}",
    "{\"action\":\"tap\",\"element\":1e300}",
];

fn random_text(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..rng.below(4) {
        match rng.below(3) {
            0 => s.push_str(rng.pick(FRAGMENTS)),
            1 => {
                for _ in 0..rng.below(40) {
                    let c = char::from_u32(rng.below(0x2FFF) as u32).unwrap_or('?');
                    s.push(c);
                }
            }
            _ => s.push_str(" so I will "),
        }
    }
    s
}

fn random_event(rng: &mut Rng) -> Event {
    let screens = [
        settings_screen(),
        wifi_screen(true),
        canvas_screen(),
        passwords_screen(),
        product_screen(),
    ];
    match rng.below(10) {
        0 => Event::Start,
        1 | 2 => Event::Observed(observation(rng.pick(&screens).clone())),
        3 | 4 => model(&random_text(rng)),
        5 => Event::ModelFailed {
            error: random_text(rng),
        },
        6 => Event::ActResult(if rng.below(3) == 0 {
            RunnerResult::err("x")
        } else {
            RunnerResult::ok()
        }),
        7 => Event::ToolResult(ToolResult {
            ok: rng.below(2) == 0,
            output: random_text(rng),
            error: None,
        }),
        8 => Event::UserDecision {
            approved: rng.below(2) == 0,
        },
        _ => {
            if rng.below(20) == 0 {
                Event::Cancel
            } else {
                Event::UserAnswer {
                    text: random_text(rng),
                }
            }
        }
    }
}

#[test]
fn fuzz_parser_never_panics() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..20_000 {
        let t = random_text(&mut rng);
        let _ = jarvis_agent::parse_model_output(&t);
    }
}

#[test]
fn fuzz_random_event_sequences_terminate_without_panicking() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..1_500 {
        let mut s = session();
        let mut finished = None;
        for i in 0..60u64 {
            let e = if i == 0 && rng.below(4) != 0 {
                Event::Start
            } else {
                random_event(&mut rng)
            };
            let eff = s.step(ev(i * 10, e));
            if let Effect::Finished(o) = &eff {
                finished = Some(o.clone());
                break;
            }
        }
        let _ = finished;
        let _ = s.into_memory();
    }
}

/// Well-formed protocol, arbitrary model output and phone: must finish within the budget.
#[test]
fn fuzz_well_formed_protocol_with_arbitrary_model_terminates_within_budget() {
    let mut rng = Rng(42);
    let policy = jarvis_proto::Policy::default();
    // Upper bound on effects: each step is at most (parse retries + 1) inferences plus a
    // handful of observe/act/confirm round trips; Clear can queue up to 65 commands.
    let cap = (policy.max_steps as usize + 1) * ((policy.max_parse_retries as usize + 1) + 80);
    for round in 0..400 {
        let outputs: Vec<String> = (0..200).map(|_| random_text(&mut rng)).collect();
        let refs: Vec<&str> = outputs.iter().map(String::as_str).collect();
        let mut decisions = Vec::new();
        for _ in 0..50 {
            decisions.push(rng.below(2) == 0);
        }
        let screens = vec![
            settings_screen(),
            wifi_screen(true),
            product_screen(),
            canvas_screen(),
            search_screen(false, None),
        ];
        let mut prng = Rng(round + 1);
        let phone = Phone::new(settings_screen(), move |_, _| match prng.below(5) {
            0 => Transition::Fail("boom".into()),
            1 => Transition::Stay,
            _ => Transition::Go(prng.pick(&screens).clone()),
        });
        let mut c = if round % 2 == 0 {
            config()
        } else {
            vision_config()
        };
        c.model.supports_grammar = round % 3 == 0;
        let r = run(
            c,
            "Do something",
            MemoryStore::new(),
            phone,
            Script::model(&refs).decisions(&decisions).answers(&["ok"]),
        );
        assert!(
            r.effects.len() <= cap,
            "round {round}: {} effects",
            r.effects.len()
        );
        assert!(r.outcome.steps <= policy.max_steps);
        for req in &r.requests {
            let total = jarvis_observe::estimate_tokens(&req.system)
                + jarvis_observe::estimate_tokens(&req.user);
            assert!(total + req.max_tokens <= 4096, "round {round}: {total}");
            assert_eq!(
                req.user.matches(jarvis_safety::SCREEN_BLOCK_START).count(),
                1
            );
            assert_eq!(req.user.matches(jarvis_safety::SCREEN_BLOCK_END).count(), 1);
        }
    }
}
