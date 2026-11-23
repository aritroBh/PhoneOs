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
//! **Twelfth review.** The state is set where a `TypeText` is sent, whatever follows. The
//! simulated phone now also misbehaves on `TypeText`: it alters the text (an
//! autocorrect-like change of one letter, truncation), relabels the field after typing (a
//! counter, so its fingerprint changes), drops focus, replies with a runner error or a lock
//! (with the text typed); Return is then held ("screen changed") or the step fails. The
//! property is unchanged and is checked against the phone's own record: once **any**
//! `TypeText` reached the phone in this app, every tap on a form's button and every Return
//! the runner gets, while that form holds text, was asked. With the state set only on a
//! checked (`Match`) or uncheckable typing — the eleventh review's rule — it fails.
//!
//! Seeded (xorshift); a failure names its seed (`STATE_SEED=<seed>` runs it alone); the
//! default is 300 sessions, `STATE_SESSIONS=<n>` changes it.
mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, Key, ModelOutput, Observation, Role, RunnerCommand, RunnerResult, Screen,
    TaskSpec, TimedEvent, UiNode,
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

/// The page: form `k` at `y = 120 + 200 k` with its field and its button; `labels[k]` is
/// the field's label ("Note k", or with a counter after typing).
fn page(values: &[String], labels: &[String], focus: Option<usize>) -> Screen {
    let forms: Vec<UiNode> = values
        .iter()
        .enumerate()
        .map(|(k, v)| {
            let y = 120.0 + 200.0 * k as f64;
            let mut field = UiNode::new(Role::TextField, rect(16.0, y + 10.0, 361.0, 44.0))
                .with_label(labels[k].clone())
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

/// What the phone replies to a command.
enum Reply {
    Ok(Screen),
    Error(Screen),
    Locked(Screen),
}

/// What a session did: every effect, and the confirmations.
struct Drive {
    effects: Vec<Effect>,
    confirms: usize,
    acts: Vec<RunnerCommand>,
}

/// Drives a session (twelfth review: a phone that can reply with an error or a lock, which
/// the shared driver does not model): the phone returns a reply per command; a lock is
/// followed by an unlock at the next wait.
fn drive(
    start: Screen,
    mut phone: impl FnMut(&RunnerCommand) -> Reply,
    model: &[String],
    decisions: &[bool],
) -> Drive {
    let mut session = Session::new(
        config(),
        TaskSpec::new("t1", "Fill in the forms"),
        MemoryStore::new(),
    );
    let mut screen = start;
    let mut model: std::collections::VecDeque<String> = model.iter().cloned().collect();
    let mut decisions: std::collections::VecDeque<bool> = decisions.iter().copied().collect();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    let mut out = Drive {
        effects: Vec::new(),
        confirms: 0,
        acts: Vec::new(),
    };
    for _ in 0..MAX_ITERATIONS {
        let effect = session.step(TimedEvent::new(now, event));
        now += TICK_MS;
        out.effects.push(effect.clone());
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
                out.acts.push(cmd.clone());
                match phone(&cmd) {
                    Reply::Ok(s) => {
                        screen = s;
                        Event::ActResult(RunnerResult::ok())
                    }
                    Reply::Error(s) => {
                        screen = s;
                        Event::ActResult(RunnerResult::err("the runner could not finish typing"))
                    }
                    Reply::Locked(s) => {
                        screen = s;
                        Event::ActResult(RunnerResult::device_locked())
                    }
                }
            }
            Effect::Confirm(_) => {
                out.confirms += 1;
                Event::UserDecision {
                    approved: decisions.pop_front().unwrap_or(false),
                }
            }
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(_) => return out,
            other => panic!("unexpected effect {other:?}"),
        };
    }
    panic!("session did not finish within {MAX_ITERATIONS} iterations");
}

/// How the phone misbehaves on one `TypeText`.
#[derive(Clone, Copy)]
struct Quirks {
    /// Change one letter of the text (autocorrect).
    alter: bool,
    /// Keep only the first half (a length limit).
    truncate: bool,
    /// Relabel the field after typing (a counter: its fingerprint changes).
    relabel: bool,
    /// Drop focus after typing.
    blur: bool,
    /// Reply with an error (the text typed).
    error: bool,
    /// Reply with a lock (the text typed).
    lock: bool,
}

fn session(seed: u64) -> (usize, usize) {
    let r = &mut Rng::new(seed);
    let n = 2 + r.below(2) as usize;
    let empty: Vec<String> = vec![String::new(); n];
    let labels: Vec<String> = (0..n).map(|k| format!("Note {k}")).collect();
    let start = page(&empty, &labels, None);
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
    // Twelfth review: per `TypeText`, how the phone misbehaves (about half of them do).
    let quirks: Vec<Quirks> = (0..40)
        .map(|_| {
            let odd = r.chance(50);
            Quirks {
                alter: odd && r.chance(35),
                truncate: odd && r.chance(20),
                relabel: odd && r.chance(35),
                blur: odd && r.chance(25),
                error: odd && r.chance(15),
                lock: odd && r.chance(10),
            }
        })
        .collect();
    // The phone.
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&log);
    let unfocused = Rc::clone(&log);
    let mut values = empty;
    let mut labels = labels;
    let mut focus: Option<usize> = None;
    let mut typed = false;
    let mut submits = 0usize;
    let mut typings = 0usize;
    let mut submit = move |values: &mut Vec<String>, k: usize, typed: bool| {
        seen.borrow_mut().push((!values[k].is_empty(), typed));
        if clears.get(submits).copied().unwrap_or(false) {
            values[k].clear();
        }
        submits += 1;
    };
    let phone = move |c: &RunnerCommand| {
        let mut reply = 0u8;
        match c {
            RunnerCommand::Tap { x, y } => match hit(n, *x, *y) {
                Some((k, true)) => submit(&mut values, k, typed),
                Some((k, false)) => focus = Some(k),
                None => {}
            },
            RunnerCommand::TypeText { text } => {
                // Any text sent to the app counts, wherever it lands.
                typed = true;
                let q = quirks.get(typings).copied().unwrap_or(Quirks {
                    alter: false,
                    truncate: false,
                    relabel: false,
                    blur: false,
                    error: false,
                    lock: false,
                });
                typings += 1;
                if let Some(k) = focus {
                    let mut t: Vec<char> = text.chars().collect();
                    if q.alter {
                        if let Some(c) = t.iter_mut().find(|c| c.is_ascii_lowercase()) {
                            *c = c.to_ascii_uppercase();
                        }
                        t.push('s');
                    }
                    if q.truncate && t.len() > 2 {
                        t.truncate(t.len() / 2);
                    }
                    values[k].extend(t);
                    if q.relabel {
                        labels[k] = format!("Note {k} ({}/500)", values[k].chars().count());
                    }
                    if q.blur {
                        focus = None;
                    }
                }
                reply = if q.lock {
                    2
                } else if q.error {
                    1
                } else {
                    0
                };
            }
            RunnerCommand::PressKey { key: Key::Return } => match focus {
                Some(k) => submit(&mut values, k, typed),
                // No field has focus (it was dropped): Return submits nothing.
                None => unfocused.borrow_mut().push((false, typed)),
            },
            RunnerCommand::PressKey { key: Key::Delete } => {
                if let Some(k) = focus {
                    values[k].pop();
                }
            }
            _ => {}
        }
        let s = page(&values, &labels, focus);
        match reply {
            1 => Reply::Error(s),
            2 => Reply::Locked(s),
            _ => Reply::Ok(s),
        }
    };
    let run = drive(start, phone, &model, &decisions);
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
                             {}\nacts: {:?}",
                            run.confirms, run.acts
                        );
                    }
                }
                asked = false;
            }
            _ => {}
        }
    }
    (checked, run.confirms)
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
