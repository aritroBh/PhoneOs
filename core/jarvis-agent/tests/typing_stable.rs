//! The typed-text check never corrects from a single reading: after a mismatch the session
//! waits and looks again, and Deletes plus a retype are sent only when two readings in a row
//! are equal and still wrong. Found on the iOS simulator: the field read "A" right after
//! "Ada" was typed (the other keystrokes had not reached the accessibility value yet), the
//! old check took that for a truncation, deleted one character and typed again, and the
//! field ended as "AdAda".

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod common;

use std::collections::VecDeque;

use common::*;
use jarvis_agent::{check_typed, Session, TypedVerdict, MAX_TYPING_LOOKS, TYPING_LOOK_MS};
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, ImageRef, Key, ModelOutput, Observation, OutcomeStatus, Role, RunnerCommand,
    RunnerResult, Screen, TaskSpec, TimedEvent, UiNode,
};

/// What one observation after the first `TypeText` shows.
#[derive(Clone)]
enum Reading {
    /// The search field, focused, with this value.
    Value(String),
    /// A whole screen.
    Screen(Box<Screen>),
    /// The phone is locked.
    Locked,
}

fn v(s: &str) -> Reading {
    Reading::Value(s.to_owned())
}

fn field_screen(value: &str) -> Screen {
    search_screen(true, (!value.is_empty()).then_some(value))
}

/// A phone whose field shows a script of readings after the first `TypeText` (keystrokes
/// that arrive late), and the truth after a correction.
struct Sim {
    value: String,
    script: VecDeque<Reading>,
    typed_once: bool,
    armed: bool,
    /// The first wait is refused: the phone is locked.
    lock_on_wait: bool,
    /// The retyped text loses its last character.
    drop_retype: bool,
    /// Every value an observation after typing showed.
    shown: Vec<String>,
}

#[derive(Default, Clone, Copy)]
struct Opts {
    lock_on_wait: bool,
    drop_retype: bool,
}

struct Trace {
    acts: Vec<RunnerCommand>,
    outcome: jarvis_proto::Outcome,
    prompts: String,
    last_user: String,
    questions: usize,
    shown: Vec<String>,
    final_value: String,
}

impl Trace {
    fn types(&self) -> usize {
        self.acts
            .iter()
            .filter(|c| matches!(c, RunnerCommand::TypeText { .. }))
            .count()
    }
    fn deletes(&self) -> usize {
        self.acts
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    RunnerCommand::PressKey {
                        key: Key::Delete,
                        ..
                    }
                )
            })
            .count()
    }
    fn waits(&self) -> usize {
        self.acts
            .iter()
            .filter(|c| matches!(c, RunnerCommand::Wait { .. }))
            .count()
    }
    fn failed_note(&self) -> bool {
        self.prompts.contains("did not arrive")
    }
}

fn drive_with(
    start: Screen,
    action: &str,
    prior: &str,
    script: Vec<Reading>,
    opts: Opts,
    model_tail: &[&str],
) -> Trace {
    let mut sim = Sim {
        value: prior.to_owned(),
        script: script.into(),
        typed_once: false,
        armed: false,
        lock_on_wait: opts.lock_on_wait,
        drop_retype: opts.drop_retype,
        shown: Vec::new(),
    };
    let mut model: VecDeque<String> = std::iter::once(action.to_owned())
        .chain(model_tail.iter().map(|s| (*s).to_owned()))
        .collect();
    let mut session = Session::new(
        config(),
        TaskSpec::new("t1", format!("Fill the form: {action}")),
        MemoryStore::new(),
    );
    let mut now = 1_000u64;
    let mut event = Event::Start;
    let mut acts = Vec::new();
    let mut prompts = String::new();
    let mut last_user = String::new();
    let mut questions = 0;
    let mut start = Some(start);
    for _ in 0..500 {
        let effect = session.step(TimedEvent::new(now, event));
        now += 50;
        event = match effect {
            Effect::Observe { screenshot } => {
                let mut locked = false;
                let mut scr = match start.take() {
                    Some(s) => s,
                    None => field_screen(&sim.value),
                };
                if sim.armed {
                    match sim.script.pop_front() {
                        Some(Reading::Value(x)) => {
                            sim.value = x;
                            scr = search_screen(true, Some(&sim.value));
                        }
                        Some(Reading::Screen(s)) => scr = *s,
                        Some(Reading::Locked) => locked = true,
                        None => {}
                    }
                    if !locked {
                        sim.shown.push(sim.value.clone());
                    }
                }
                Event::Observed(Observation {
                    screen: scr,
                    screenshot: screenshot.then(|| ImageRef {
                        id: "s".into(),
                        width: 1,
                        height: 1,
                    }),
                    device_locked: locked,
                })
            }
            Effect::Infer(req) => {
                prompts.push_str(&format!("{}\n{}\n=====\n", req.system, req.user));
                last_user = req.user.clone();
                match model.pop_front() {
                    Some(text) => Event::ModelOutput(ModelOutput { text }),
                    None => Event::ModelFailed {
                        error: "script exhausted".into(),
                    },
                }
            }
            Effect::Act(cmd) => {
                let cmd = cmd.without_expect();
                acts.push(cmd.clone());
                let mut reply = RunnerResult::ok();
                match &cmd {
                    RunnerCommand::TypeText { text, .. } => {
                        if sim.typed_once {
                            sim.armed = false;
                            sim.script.clear();
                            let mut t = text.clone();
                            if sim.drop_retype {
                                t.pop();
                            }
                            sim.value.push_str(&t);
                        } else {
                            sim.typed_once = true;
                            sim.armed = true;
                        }
                    }
                    RunnerCommand::PressKey {
                        key: Key::Delete, ..
                    } => {
                        sim.armed = false;
                        sim.script.clear();
                        sim.value.pop();
                    }
                    RunnerCommand::Wait { .. } if sim.lock_on_wait => {
                        sim.lock_on_wait = false;
                        reply = RunnerResult::device_locked();
                    }
                    _ => {}
                }
                Event::ActResult(reply)
            }
            Effect::CallTool(_) => Event::ToolResult(jarvis_proto::ToolResult {
                ok: true,
                output: "ok".into(),
                error: None,
            }),
            Effect::Confirm(_) => Event::phone_decision(false),
            Effect::AskUser { .. } => {
                questions += 1;
                Event::UserAnswer {
                    text: String::new(),
                }
            }
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(outcome) => {
                return Trace {
                    acts,
                    outcome,
                    prompts,
                    last_user,
                    questions,
                    shown: sim.shown,
                    final_value: sim.value,
                };
            }
        };
    }
    panic!("session did not finish");
}

fn drive(prior: &str, script: Vec<Reading>) -> Trace {
    let idx = index_of(&field_screen(prior), "Search");
    let action = format!(r#"{{"action":"type","element":{idx},"text":"Ada"}}"#);
    drive_with(
        field_screen(prior),
        &action,
        prior,
        script,
        Opts::default(),
        &["done()"],
    )
}

fn wait() -> RunnerCommand {
    RunnerCommand::Wait { ms: TYPING_LOOK_MS }
}

fn del() -> RunnerCommand {
    RunnerCommand::PressKey {
        key: Key::Delete,
        expect: None,
    }
}

fn ada() -> RunnerCommand {
    RunnerCommand::TypeText {
        text: "Ada".into(),
        expect: None,
    }
}

fn ok(t: &Trace) {
    assert_eq!(
        t.outcome.status,
        OutcomeStatus::Success,
        "{}",
        t.outcome.summary
    );
    assert!(!t.failed_note(), "{}", t.prompts);
}

#[test]
fn a_lagging_first_reading_is_not_corrected() {
    // The simulator case (run 118): "A", then "Ada".
    let t = drive("", vec![v("A"), v("Ada")]);
    ok(&t);
    assert_eq!(t.types(), 1);
    assert_eq!(t.deletes(), 0);
    assert_eq!(t.waits(), 1);
    assert_eq!(t.final_value, "Ada");
}

#[test]
fn a_growing_value_is_followed_to_the_end() {
    let t = drive("", vec![v("A"), v("Ad"), v("Ada")]);
    ok(&t);
    assert_eq!((t.types(), t.deletes(), t.waits()), (1, 0, 2));
}

#[test]
fn a_stable_truncation_is_corrected_with_the_right_deletes() {
    let t = drive("", vec![v("Ad"), v("Ad")]);
    ok(&t);
    assert_eq!(
        t.acts
            .iter()
            .filter(|c| !matches!(c, RunnerCommand::Tap { .. }))
            .cloned()
            .collect::<Vec<_>>(),
        vec![ada(), wait(), del(), del(), ada()]
    );
    assert_eq!(t.final_value, "Ada");
}

#[test]
fn a_stable_wrong_text_is_corrected_by_the_existing_rules() {
    // Autocorrect: "Adam" twice. Four characters appended, four deleted.
    let t = drive("", vec![v("Adam"), v("Adam")]);
    ok(&t);
    assert_eq!((t.types(), t.deletes(), t.waits()), (2, 4, 1));
    assert_eq!(t.final_value, "Ada");
}

#[test]
fn a_value_still_changing_at_the_look_limit_is_reported_not_corrected() {
    let t = drive(
        "",
        vec![v("A"), v("Ad"), v("Ada!"), v("Ada!!"), v("Ada!!!")],
    );
    assert_eq!(t.types(), 1, "{:?}", t.acts);
    assert_eq!(t.deletes(), 0);
    assert_eq!(t.waits() as u32, MAX_TYPING_LOOKS);
    let told = &t.last_user;
    assert!(
        told.contains("did not arrive intact") && told.contains("the field shows \"Ada!!\""),
        "{told}"
    );
    assert!(!told.contains("typed again"), "{told}");
}

#[test]
fn a_prior_value_is_kept_while_the_rest_arrives() {
    let t = drive("x", vec![v("xA"), v("xAda")]);
    ok(&t);
    assert_eq!((t.types(), t.deletes()), (1, 0));
    assert_eq!(t.final_value, "xAda");
}

#[test]
fn a_stable_truncation_after_a_prior_value_deletes_only_what_was_appended() {
    let t = drive("x", vec![v("xAd"), v("xAd")]);
    ok(&t);
    assert_eq!((t.types(), t.deletes()), (2, 2));
    assert_eq!(t.final_value, "xAda");
}

#[test]
fn a_correction_that_still_differs_is_looked_at_again_then_reported() {
    // Stable "Ad": corrected once; the retyped text loses its last letter again, twice in a
    // row, so it is reported ("typed again once") and never corrected a second time.
    let idx = index_of(&field_screen(""), "Search");
    let action = format!(r#"{{"action":"type","element":{idx},"text":"Ada"}}"#);
    let t = drive_with(
        field_screen(""),
        &action,
        "",
        vec![v("Ad"), v("Ad")],
        Opts {
            drop_retype: true,
            ..Opts::default()
        },
        &["fail()"],
    );
    assert_eq!(t.types(), 2, "{:?}", t.acts);
    // One wait before the correction, MAX_TYPING_LOOKS... until the second reading repeats.
    assert_eq!(t.waits(), 2, "{:?}", t.acts);
    assert!(
        t.last_user
            .contains("the field shows \"Ad\"; it was typed again once"),
        "{}",
        t.last_user
    );
}

#[test]
fn the_secure_field_is_handed_to_the_owner_and_no_look_is_taken() {
    let secure = |len: usize| {
        screen(
            "com.example.shop",
            "Shop",
            UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![
                node(Role::Cell, "Sign in", rect(16.0, 100.0, 361.0, 44.0)),
                UiNode::new(Role::SecureTextField, rect(16.0, 260.0, 361.0, 44.0))
                    .with_label("Password")
                    .with_value("•".repeat(len))
                    .secure(true)
                    .focused(true)
                    .hittable(true),
            ]),
            true,
        )
    };
    let s = secure(0);
    let idx = index_of(&s, "Password");
    let action = format!(r#"{{"action":"type","element":{idx},"text":"Ada"}}"#);
    let t = drive_with(
        s,
        &action,
        "",
        vec![
            Reading::Screen(Box::new(secure(1))),
            Reading::Screen(Box::new(secure(3))),
        ],
        Opts::default(),
        &["done()"],
    );
    assert_eq!(t.types(), 0);
    assert_eq!(t.waits(), 0);
    assert_eq!(t.questions, 1);
    assert!(!t.failed_note());
}

#[test]
fn a_field_that_turns_secure_during_the_look_is_not_corrected() {
    // The reading becomes hidden (a length only): nothing can be compared, so nothing is
    // retyped and the typing is not reported as failed.
    let hidden = UiNode::new(Role::SearchField, rect(16.0, 120.0, 361.0, 36.0))
        .with_label("Search")
        .with_value("\u{2022}")
        .secure(true)
        .hittable(true)
        .focused(true);
    let hidden = screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![hidden]),
        true,
    );
    let t = drive("", vec![v("A"), Reading::Screen(Box::new(hidden))]);
    assert_eq!((t.types(), t.deletes()), (1, 0), "{:?}", t.acts);
    assert!(!t.failed_note(), "{}", t.prompts);
}

#[test]
fn a_lock_while_waiting_for_the_next_look_is_not_corrected() {
    let idx = index_of(&field_screen(""), "Search");
    let action = format!(r#"{{"action":"type","element":{idx},"text":"Ada"}}"#);
    let t = drive_with(
        field_screen(""),
        &action,
        "",
        vec![v("A"), v("Ada")],
        Opts {
            lock_on_wait: true,
            ..Opts::default()
        },
        &["done()"],
    );
    assert_eq!((t.types(), t.deletes()), (1, 0));
    assert!(
        t.last_user
            .contains("the phone locked before the result could be checked"),
        "{}",
        t.last_user
    );
}

#[test]
fn a_lock_during_the_extra_look_is_not_corrected() {
    let t = drive("", vec![v("Ad"), Reading::Locked, v("Ad")]);
    assert_eq!((t.types(), t.deletes()), (1, 0), "{:?}", t.acts);
    assert!(
        t.last_user
            .contains("the phone locked before the result could be checked"),
        "{}",
        t.last_user
    );
}

fn two_fields(a_value: &str, focus_a: bool) -> Screen {
    let a = UiNode::new(Role::TextField, rect(16.0, 120.0, 361.0, 36.0))
        .with_label("Username")
        .with_value(a_value)
        .hittable(true)
        .focused(focus_a);
    let b = UiNode::new(Role::TextField, rect(16.0, 180.0, 361.0, 36.0))
        .with_label("Notes")
        .hittable(true)
        .focused(!focus_a);
    screen(
        "com.example.shop",
        "Shop",
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![
            node(Role::StaticText, "Sign up", rect(16.0, 70.0, 200.0, 30.0)),
            a,
            b,
        ]),
        true,
    )
}

#[test]
fn another_field_taking_focus_during_the_look_blocks_the_correction() {
    let start = two_fields("", true);
    let idx = index_of(&start, "Username");
    let action = format!(r#"{{"action":"type","element":{idx},"text":"Ada"}}"#);
    let t = drive_with(
        start,
        &action,
        "",
        vec![
            Reading::Screen(Box::new(two_fields("Ad", true))),
            Reading::Screen(Box::new(two_fields("Ad", false))),
            Reading::Screen(Box::new(two_fields("Ad", false))),
        ],
        Opts::default(),
        &["fail()"],
    );
    assert_eq!((t.types(), t.deletes()), (1, 0), "{:?}", t.acts);
    assert!(t.failed_note(), "{}", t.prompts);
}

// ---------------------------------------------------------------------------------------
// Property: readings that lag the keystrokes by 0-3 looks.
// ---------------------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn lagging_readings_never_duplicate_or_delete_characters() {
    let cases: u64 = std::env::var("TYPING_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    let letters: Vec<char> = "abcdefghij".chars().collect();
    for seed in 1..=cases {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let n = 1 + rng.below(6) as usize;
        let text: String = (0..n).map(|_| letters[rng.below(10) as usize]).collect();
        let prior: String = "xyz".chars().take(rng.below(4) as usize).collect();
        let lag = rng.below(4) as usize; // readings before the full one
                                         // Characters shown at each reading, non-decreasing, the last one all of them.
                                         // An empty reading of an empty field reports no value at all (the check cannot
                                         // compare it), so a partial reading of such a field shows at least one character.
        let lo = usize::from(prior.is_empty());
        let mut counts: Vec<usize> = if n > lo {
            (0..lag)
                .map(|_| lo + rng.below((n - lo) as u64) as usize)
                .collect()
        } else {
            Vec::new()
        };
        counts.sort_unstable();
        if rng.below(4) != 0 {
            counts.dedup(); // strictly growing: the usual slow keyboard
        }
        counts.push(n);
        let readings: Vec<String> = counts
            .iter()
            .map(|&c| format!("{prior}{}", text.chars().take(c).collect::<String>()))
            .collect();
        let idx = index_of(&field_screen(&prior), "Search");
        let action = format!(r#"{{"action":"type","element":{idx},"text":"{text}"}}"#);
        let t = drive_with(
            field_screen(&prior),
            &action,
            &prior,
            readings.iter().map(|r| v(r)).collect(),
            Opts::default(),
            &["done()"],
        );
        let ctx = format!(
            "seed {seed}: {prior:?}+{text:?} readings {readings:?} acts {:?}",
            t.acts
        );
        // What the session could see: readings up to the first match.
        let first_match = readings
            .iter()
            .position(|r| {
                check_typed(&text, Some(&prior), Some(r), Some("Search")) == TypedVerdict::Match
            })
            .unwrap_or_else(|| panic!("{ctx}: no match"));
        let stable_pair =
            (1..=first_match).find(|&i| i < first_match && readings[i] == readings[i - 1]);
        let want_types = if stable_pair.is_some() { 2 } else { 1 };
        assert_eq!(t.types(), want_types, "{ctx}");
        assert_eq!(
            t.outcome.status,
            OutcomeStatus::Success,
            "{ctx}: {}",
            t.outcome.summary
        );
        assert_eq!(t.final_value, format!("{prior}{text}"), "{ctx}");
        assert!(!t.failed_note(), "{ctx}");
        if let Some(i) = stable_pair {
            // Exactly what was appended in the stable reading.
            let appended = readings[i].chars().count() - prior.chars().count();
            assert_eq!(t.deletes(), appended, "{ctx}");
        } else {
            assert_eq!(t.deletes(), 0, "{ctx}");
            assert_eq!(t.shown.len(), first_match + 1, "{ctx}");
        }
        assert!(t.waits() as u32 <= MAX_TYPING_LOOKS, "{ctx}");
    }
}
