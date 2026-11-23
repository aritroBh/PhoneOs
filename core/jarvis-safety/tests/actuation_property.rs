#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Thirteenth review (H2), property: every command that can actuate a control is presumed
//! to submit like a tap.
//!
//! Random forms: a filled text field and one neutral control (a button or link with a
//! neutral or no label, or a slider labelled like "slide to continue" or not at all), at a
//! random place and size, with the "typed here" state set. For every action variant that
//! actuates the control — tap, long press, `tap_point` on it, a swipe or scroll on it in
//! any direction, and, for a slider across the middle of the screen, a sideways swipe or
//! scroll with no element — the gate asks and the question says `Sends:` with the field.
//! Fourteenth review (Rule E): a `tap_point` on a spot no listed element covers is presumed
//! too (the form is the screen).
//! A tap on a slider sets its value (not presumed, as before). `classify_variant` below is a
//! `match` over every `Action` variant with no wildcard arm, so a new variant does not
//! compile until this test decides it.
//!
//! Seeded (xorshift); `ACT_SEED=<seed>` runs one; the default is 2,000 forms
//! (`ACT_FORMS=<n>`). With `presumed_submit` limited to taps, long presses and tap points
//! again (the twelfth review's rule) it fails at the first form.
mod common;

use common::{app_screen, el};
use jarvis_proto::{Action, Direction, Element, Policy, Rect, Role};
use jarvis_safety::{evaluate_with, Decision, ScreenSource, TypedText};

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
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
}

/// What the test expects of an action on the form's control: it actuates the control and
/// must be presumed (`true`), or it is not this test's business (`false`). Exhaustive over
/// `Action`.
/// `control` is what the action is on; `None` for a `tap_point` that resolves to no listed
/// element (fourteenth review, Rule E: a press on something unknown is presumed too).
fn classify_variant(a: &Action, control: Option<&Element>) -> bool {
    let slider = control.is_some_and(|c| c.role == Role::Slider);
    match a {
        Action::Tap { .. } | Action::LongPress { .. } => control.is_some() && !slider,
        Action::TapPoint { .. } => !slider,
        Action::Swipe { .. } | Action::Scroll { .. } => true,
        // Return in a field has its own property test (jarvis-observe's presumed submit).
        Action::Type { .. } => false,
        Action::Clear { .. }
        | Action::PressHome
        | Action::OpenApp { .. }
        | Action::OpenUrl { .. }
        | Action::CallTool { .. }
        | Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => false,
    }
}

const DIRS: [Direction; 4] = [
    Direction::Up,
    Direction::Down,
    Direction::Left,
    Direction::Right,
];

fn form(seed: u64) -> usize {
    let r = &mut Rng::new(seed);
    let mut note = el(1, Role::TextField, "Your note");
    note.frame = Rect::new(16.0, 120.0, 361.0, 44.0);
    note.value = Some(format!("Meet me at gate {seed}"));
    let slider = r.below(3) == 0;
    let mut control = if slider {
        let label = *r.pick(&["Slide to continue", "", "Swipe to proceed", "Unlock"]);
        el(2, Role::Slider, label)
    } else {
        let role = if r.below(4) == 0 {
            Role::Link
        } else {
            Role::Button
        };
        let label = *r.pick(&[
            "Continue",
            "OK",
            "Next",
            "Done",
            "Proceed",
            "",
            "arrow.right",
        ]);
        el(2, role, label)
    };
    // A slider across the middle of the screen (where an element-less sideways drag runs),
    // anything else anywhere below the field.
    control.frame = if slider {
        Rect::new(16.0, 395.0, 361.0, 50.0)
    } else {
        let h = 36.0 + r.below(40) as f64;
        let w = 80.0 + r.below(281) as f64;
        Rect::new(16.0, 200.0 + r.below(500) as f64, w, h)
    };
    let s = app_screen(vec![note, control.clone()]);
    let typed = [TypedText {
        field: 0,
        text: String::new(),
    }];
    let c = control.frame.center();
    let mut actions = vec![
        Action::Tap { element: 2 },
        Action::LongPress {
            element: 2,
            duration_ms: 800,
        },
        Action::TapPoint {
            x: c.x / s.size.width,
            y: c.y / s.size.height,
        },
    ];
    for d in DIRS {
        actions.push(Action::Swipe {
            direction: d,
            element: Some(2),
        });
        actions.push(Action::Scroll {
            direction: d,
            element: Some(2),
        });
    }
    if slider {
        for d in [Direction::Left, Direction::Right] {
            actions.push(Action::Swipe {
                direction: d,
                element: None,
            });
            actions.push(Action::Scroll {
                direction: d,
                element: None,
            });
        }
    }
    // Fourteenth review (Rule E): a `tap_point` on a spot no listed element covers (a web
    // canvas, an unlisted control).
    let spot = jarvis_proto::Point::new(8.0 + r.below(377) as f64, 180.0 + r.below(600) as f64);
    let unlisted = !s.elements.iter().any(|e| e.frame.contains(spot));
    let mut checked = 0;
    let on: Vec<(Action, Option<&Element>)> = actions
        .into_iter()
        .map(|a| (a, Some(&control)))
        .chain(unlisted.then(|| {
            (
                Action::TapPoint {
                    x: spot.x / s.size.width,
                    y: spot.y / s.size.height,
                },
                None,
            )
        }))
        .collect();
    for (a, target) in &on {
        if !classify_variant(a, *target) {
            continue;
        }
        checked += 1;
        let d = evaluate_with(
            &Policy::default(),
            a,
            &s,
            None,
            ScreenSource::Typed,
            None,
            &typed,
        );
        let Decision::Confirm(q) = d else {
            panic!("seed {seed}: {a:?} on {control:?} ran unasked: {d:?} (ACT_SEED={seed})")
        };
        assert!(
            q.summary.contains("Sends:") && q.summary.contains("Your note"),
            "seed {seed}: {a:?}: {}",
            q.summary
        );
    }
    checked
}

#[test]
fn every_actuating_command_is_presumed_like_a_tap() {
    if let Some(seed) = std::env::var("ACT_SEED").ok().and_then(|v| v.parse().ok()) {
        form(seed);
        return;
    }
    let n = std::env::var("ACT_FORMS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_000u64);
    let checked: usize = (1..=n).map(form).sum();
    println!("{n} forms: {checked} actuating commands, all asked with Sends:");
    assert!(checked > 0);
}

/// An element-less vertical scroll over page content (no control under the finger, none
/// crossed) only moves the page: never presumed, whatever the state.
#[test]
fn a_scroll_over_page_content_is_navigation() {
    let mut note = el(1, Role::TextField, "Your note");
    note.frame = Rect::new(16.0, 100.0, 361.0, 44.0);
    note.value = Some("hello".to_owned());
    let mut text = el(
        2,
        Role::StaticText,
        "Terms and conditions apply to this form",
    );
    text.frame = Rect::new(16.0, 300.0, 361.0, 300.0);
    let mut cont = el(3, Role::Button, "Continue");
    cont.frame = Rect::new(16.0, 760.0, 361.0, 44.0);
    let s = app_screen(vec![note, text, cont]);
    let typed = [TypedText {
        field: 0,
        text: String::new(),
    }];
    for d in [Direction::Up, Direction::Down] {
        let a = Action::Scroll {
            direction: d,
            element: None,
        };
        assert_eq!(
            evaluate_with(
                &Policy::default(),
                &a,
                &s,
                None,
                ScreenSource::Typed,
                None,
                &typed
            ),
            Decision::Allow,
            "{a:?}"
        );
    }
}
