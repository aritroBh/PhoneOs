#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Eleventh review, the session's side of property test 3 (presumed submit): the "Jarvis
//! typed here" state lasts for the task.
//!
//! Random sessions in one app with two to three forms, each a single-line text field and a
//! neutral button ("Continue"). The scripted model types into fields (with and without
//! `submit`) and taps buttons in random order; the owner approves or declines each question
//! at random; a submit (a tap on a form's button, or Return in its field) either empties
//! that form's field (a chat composer) or leaves it (a form that stays). Property, checked
//! at every tap on a button and every Return the runner is sent: once Jarvis typed in this
//! app, when the form of that button or field holds text, the action was asked (the owner
//! approved it) — whatever was approved or declined before, in that form or another.
//!
//! The tenth review's clearing rule (an approved submit cleared the state for the whole
//! app) fails this within the first sequences: an approved "Continue" in one form, then a
//! "Continue" under text Jarvis typed in another form, ran unasked.
//!
//! Seeded (xorshift); a failure names its seed (`STATE_SEED=<seed>` runs it alone); the
//! default is 300 sessions, `STATE_SESSIONS=<n>` changes it.
mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{Effect, Key, Role, RunnerCommand, Screen, UiNode};

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

/// The page: form `k` at `y = 120 + 200 k` with its field and its button.
fn page(values: &[String], focus: Option<usize>) -> Screen {
    let forms: Vec<UiNode> = values
        .iter()
        .enumerate()
        .map(|(k, v)| {
            let y = 120.0 + 200.0 * k as f64;
            let mut field = UiNode::new(Role::TextField, rect(16.0, y + 10.0, 361.0, 44.0))
                .with_label(format!("Note {k}"))
                .hittable(true);
            if !v.is_empty() {
                field = field.with_value(v);
            }
            if focus == Some(k) {
                field = field.focused(true);
            }
            UiNode::new(Role::Other("form".into()), rect(0.0, y, 393.0, 180.0)).with_children(vec![
                field,
                node(
                    Role::Button,
                    &format!("Continue {k}"),
                    rect(16.0, y + 70.0, 361.0, 44.0),
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

/// Which form a point is in, and whether it hit the button.
fn hit(n: usize, x: f64, y: f64) -> Option<(usize, bool)> {
    if !(16.0..=377.0).contains(&x) {
        return None;
    }
    (0..n).find_map(|k| {
        let top = 120.0 + 200.0 * k as f64;
        if (top + 10.0..=top + 54.0).contains(&y) {
            Some((k, false))
        } else if (top + 70.0..=top + 114.0).contains(&y) {
            Some((k, true))
        } else {
            None
        }
    })
}

/// What the phone saw at each submit-like command: (the form held text, Jarvis had typed
/// in the app before).
type Log = Rc<RefCell<Vec<(bool, bool)>>>;

fn session(seed: u64) -> (usize, usize) {
    let r = &mut Rng::new(seed);
    let n = 2 + r.below(2) as usize;
    let empty: Vec<String> = vec![String::new(); n];
    let start = page(&empty, None);
    let fields: Vec<u32> = (0..n)
        .map(|k| index_of(&start, &format!("Note {k}")))
        .collect();
    let buttons: Vec<u32> = (0..n)
        .map(|k| index_of(&start, &format!("Continue {k}")))
        .collect();
    // The model's actions.
    let mut model: Vec<String> = Vec::new();
    for step in 0..3 + r.below(8) {
        let k = r.below(n as u64) as usize;
        if r.chance(50) {
            let submit = r.chance(40);
            model.push(format!(
                r#"{{"action":"type","element":{},"text":"note {seed} {step}","submit":{submit}}}"#,
                fields[k]
            ));
        } else {
            model.push(format!(r#"{{"action":"tap","element":{}}}"#, buttons[k]));
        }
    }
    model.push(r#"{"action":"fail","reason":"x"}"#.to_owned());
    let decisions: Vec<bool> = (0..20).map(|_| r.chance(60)).collect();
    let clears: Vec<bool> = (0..20).map(|_| r.chance(50)).collect();
    // The phone.
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    let mut values = empty;
    let mut focus: Option<usize> = None;
    let mut typed = false;
    let mut submits = 0usize;
    let mut submit = move |values: &mut Vec<String>, k: usize, typed: bool| {
        seen.borrow_mut().push((!values[k].is_empty(), typed));
        if clears.get(submits).copied().unwrap_or(false) {
            values[k].clear();
        }
        submits += 1;
    };
    let phone = Phone::new(start, move |c, _| {
        match c {
            RunnerCommand::Tap { x, y } => match hit(n, *x, *y) {
                Some((k, true)) => submit(&mut values, k, typed),
                Some((k, false)) => focus = Some(k),
                None => {}
            },
            RunnerCommand::TypeText { text } => {
                if let Some(k) = focus {
                    values[k].push_str(text);
                    typed = true;
                }
            }
            RunnerCommand::PressKey { key: Key::Return } => {
                if let Some(k) = focus {
                    submit(&mut values, k, typed);
                }
            }
            RunnerCommand::PressKey { key: Key::Delete } => {
                if let Some(k) = focus {
                    values[k].pop();
                }
            }
            _ => {}
        }
        Transition::Go(page(&values, focus))
    });
    let models: Vec<&str> = model.iter().map(String::as_str).collect();
    let run = run(
        config(),
        "Fill in the forms",
        MemoryStore::new(),
        phone,
        Script::model(&models).decisions(&decisions),
    );
    // Walk the effects: every submit-like command the runner got was asked when the state
    // was set and its form held text.
    let mut asked = false;
    let mut at = 0usize;
    let mut checked = 0usize;
    let log = log.borrow();
    for e in &run.effects {
        match e {
            Effect::Infer(_) => asked = false,
            Effect::Confirm(_) => asked = true,
            Effect::Act(cmd) => {
                let submit_like = match cmd {
                    RunnerCommand::Tap { x, y } => matches!(hit(n, *x, *y), Some((_, true))),
                    RunnerCommand::PressKey { key: Key::Return } => true,
                    _ => false,
                };
                if submit_like {
                    let Some(&(filled, typed)) = log.get(at) else {
                        panic!("seed {seed}: the phone did not log a submit");
                    };
                    at += 1;
                    if filled && typed {
                        checked += 1;
                        assert!(
                            asked,
                            "seed {seed}: a submit of a form holding text ran unasked after \
                             Jarvis typed in the app (run alone: STATE_SEED={seed})\nconfirms: \
                             {:#?}\nacts: {:?}",
                            run.confirms, run.acts
                        );
                    }
                }
                asked = false;
            }
            _ => {}
        }
    }
    (checked, run.confirms.len())
}

#[test]
fn the_typed_here_state_lasts_for_the_task() {
    if let Some(seed) = std::env::var("STATE_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        session(seed);
        return;
    }
    let n = std::env::var("STATE_SESSIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SESSIONS);
    let (mut checked, mut asked) = (0, 0);
    for seed in 1..=n {
        let (c, a) = session(seed);
        checked += c;
        asked += a;
    }
    println!("{n} sessions: {checked} submits of a filled form after typing, all asked; {asked} questions");
    assert!(checked > 0);
}
