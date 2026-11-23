#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Thirteenth review (H1), property: the "typed here" state follows from what is
//! **observed**, not from which command was sent.
//!
//! Random sessions in one app with one to three forms, each a single-line text field, a
//! "Paste k" control and a neutral "Continue k" button, some fields prefilled when the task
//! starts (text that is not Jarvis's). The scripted model never types: it taps "Paste k"
//! (text enters field k — at once, or only after the model's next `wait`, like a paste or
//! an AutoFill that lands late), taps "Continue k", or waits; the owner approves or
//! declines each question at random. Property, checked at every "Continue" the runner is
//! sent, against the phone's own record: once a value entered any field of the app after
//! a Jarvis command, a "Continue" whose form holds text was asked.
//!
//! With the observation trigger removed (the state set only by `TypeText`) it fails at the
//! first sequence that pastes.
//!
//! Seeded (xorshift); a failure names its seed (`FILL_SEED=<seed>` runs it alone); the
//! default is 300 sessions, `FILL_SESSIONS=<n>` changes it.
mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, ModelOutput, Observation, Role, RunnerCommand, RunnerResult, Screen, TaskSpec,
    TimedEvent, UiNode,
};

const DEFAULT_SESSIONS: u64 = 300;

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

/// Form `k` at `y = 120 + 220 k`: its field, "Paste k" and "Continue k".
fn page(values: &[String]) -> Screen {
    let forms: Vec<UiNode> = values
        .iter()
        .enumerate()
        .map(|(k, v)| {
            let y = 120.0 + 220.0 * k as f64;
            let mut field = UiNode::new(Role::TextField, rect(16.0, y + 10.0, 361.0, 44.0))
                .with_label(format!("Note {k}"))
                .hittable(true);
            if !v.is_empty() {
                field = field.with_value(v);
            }
            UiNode::new(Role::Other("form".into()), rect(0.0, y, 393.0, 200.0)).with_children(vec![
                field,
                node(
                    Role::Button,
                    &format!("Paste {k}"),
                    rect(16.0, y + 64.0, 170.0, 44.0),
                ),
                node(
                    Role::Button,
                    &format!("Continue {k}"),
                    rect(16.0, y + 120.0, 361.0, 44.0),
                ),
            ])
        })
        .collect();
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Forms")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(forms)
        ]);
    screen("com.example.forms", "Forms", root, false)
}

/// Which form a tap hit: (form, 0 field / 1 paste / 2 continue).
fn hit(n: usize, x: f64, y: f64) -> Option<(usize, u8)> {
    (0..n).find_map(|k| {
        let top = 120.0 + 220.0 * k as f64;
        if (16.0..=377.0).contains(&x) && (top + 10.0..=top + 54.0).contains(&y) {
            Some((k, 0))
        } else if (16.0..=186.0).contains(&x) && (top + 64.0..=top + 108.0).contains(&y) {
            Some((k, 1))
        } else if (16.0..=377.0).contains(&x) && (top + 120.0..=top + 164.0).contains(&y) {
            Some((k, 2))
        } else {
            None
        }
    })
}

/// What the phone saw at each "Continue": (its form held text, text had entered a field
/// after a Jarvis command).
type Log = Rc<RefCell<Vec<(bool, bool)>>>;

fn session(seed: u64) -> (usize, usize) {
    let r = &mut Rng::new(seed);
    let n = 1 + r.below(3) as usize;
    let values: Vec<String> = (0..n)
        .map(|k| {
            if r.chance(30) {
                format!("owner text {k}")
            } else {
                String::new()
            }
        })
        .collect();
    let start = page(&values);
    let paste: Vec<u32> = (0..n)
        .map(|k| index_of(&start, &format!("Paste {k}")))
        .collect();
    let cont: Vec<u32> = (0..n)
        .map(|k| index_of(&start, &format!("Continue {k}")))
        .collect();
    let mut model: Vec<String> = Vec::new();
    for _ in 0..3 + r.below(8) {
        let k = r.below(n as u64) as usize;
        match r.below(3) {
            0 => model.push(format!(r#"{{"action":"tap","element":{}}}"#, paste[k])),
            1 => model.push(format!(r#"{{"action":"tap","element":{}}}"#, cont[k])),
            _ => model.push(r#"{"action":"wait","ms":500}"#.to_owned()),
        }
    }
    model.push(r#"{"action":"fail","reason":"x"}"#.to_owned());
    let decisions: Vec<bool> = (0..30).map(|_| r.chance(60)).collect();
    let late: Vec<bool> = (0..30).map(|_| r.chance(40)).collect();

    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    let mut values = values;
    let mut entered = false;
    let mut pastes = 0usize;
    let mut pending: Vec<(usize, String)> = Vec::new();
    let mut phone = move |c: &RunnerCommand| {
        match c {
            RunnerCommand::Tap { x, y, .. } => match hit(n, *x, *y) {
                Some((k, 1)) => {
                    let text = format!("pasted {seed} {pastes}");
                    if late.get(pastes).copied().unwrap_or(false) {
                        pending.push((k, text));
                    } else {
                        values[k] = text;
                        entered = true;
                    }
                    pastes += 1;
                }
                Some((k, 2)) => seen.borrow_mut().push((!values[k].is_empty(), entered)),
                _ => {}
            },
            RunnerCommand::Wait { .. } => {
                for (k, text) in pending.drain(..) {
                    values[k] = text;
                    entered = true;
                }
            }
            _ => {}
        }
        page(&values)
    };

    let mut s = Session::new(
        config(),
        TaskSpec::new("t1", "Finish the forms"),
        MemoryStore::new(),
    );
    let mut screen = start;
    let mut model: std::collections::VecDeque<String> = model.into_iter().collect();
    let mut decisions: std::collections::VecDeque<bool> = decisions.into_iter().collect();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    let mut effects = Vec::new();
    let mut confirms = 0usize;
    for _ in 0..MAX_ITERATIONS {
        let effect = s.step(TimedEvent::new(now, event));
        now += TICK_MS;
        effects.push(effect.clone());
        event = match effect {
            Effect::Observe { .. } => Event::Observed(Observation {
                screen: screen.clone(),
                screenshot: None,
                device_locked: false,
            }),
            Effect::Infer(_) => match model.pop_front() {
                Some(text) => Event::ModelOutput(ModelOutput { text }),
                None => Event::ModelFailed {
                    error: "script exhausted".into(),
                },
            },
            Effect::Act(cmd) => {
                screen = phone(&cmd);
                Event::ActResult(RunnerResult::ok())
            }
            Effect::Confirm(_) => {
                confirms += 1;
                Event::UserDecision {
                    approved: decisions.pop_front().unwrap_or(false),
                }
            }
            Effect::Finished(_) => break,
            other => panic!("unexpected effect {other:?}"),
        };
    }
    let mut asked = false;
    let mut at = 0usize;
    let mut checked = 0usize;
    let log = log.borrow();
    for e in &effects {
        match e {
            Effect::Infer(_) => asked = false,
            Effect::Confirm(_) => asked = true,
            Effect::Act(RunnerCommand::Tap { x, y, .. }) => {
                if matches!(hit(n, *x, *y), Some((_, 2))) {
                    let (filled, entered) = log[at];
                    at += 1;
                    if filled && entered {
                        checked += 1;
                        assert!(
                            asked,
                            "seed {seed}: a Continue under text that entered a field after a \
                             Jarvis command ran unasked (run alone: FILL_SEED={seed})"
                        );
                    }
                }
                asked = false;
            }
            Effect::Act(_) => asked = false,
            _ => {}
        }
    }
    (checked, confirms)
}

#[test]
fn text_entering_a_field_sets_the_typed_here_state() {
    if let Some(seed) = std::env::var("FILL_SEED").ok().and_then(|v| v.parse().ok()) {
        session(seed);
        return;
    }
    let n = std::env::var("FILL_SESSIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SESSIONS);
    let (mut checked, mut asked) = (0, 0);
    for seed in 1..=n {
        let (c, a) = session(seed);
        checked += c;
        asked += a;
    }
    println!("{n} sessions: {checked} Continues under entered text, all asked; {asked} questions");
    assert!(checked > 0);
}
