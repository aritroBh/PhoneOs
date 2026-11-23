#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Over-asking measurement on the ten real iOS 26 trees (thirteenth review): for every
//! enabled control, whether a tap, a sideways swipe and a vertical swipe on it ask, with
//! the "typed here" state unset and set; and one element-less scroll down per tree. Prints
//! the counts (`cargo test -p jarvis-sim --test overask_measure -- --nocapture`); asserts
//! only that the trees load. The numbers before and after a rule change are in
//! `docs/ARCHITECTURE.md` (Known gaps).

use jarvis_observe::compact_view;
use jarvis_proto::{Action, Direction, Policy, Role};
use jarvis_safety::{evaluate_owned, evaluate_with, Decision, ScreenSource, ScreenTree, TypedText};
use jarvis_sim::bench::fixtures::{default_dir, load_all};

fn asks(d: &Decision) -> bool {
    matches!(d, Decision::Confirm(_))
}

#[test]
fn over_asking_on_real_trees() {
    let fixtures = load_all(&default_dir()).unwrap();
    assert_eq!(fixtures.len(), 10);
    let state = [TypedText {
        field: 0,
        text: String::new(),
    }];
    let policy = Policy::default();
    let mut totals = [0usize; 7];
    for f in &fixtures {
        let v = compact_view(&f.screen);
        let tree = ScreenTree {
            sources: &v.sources,
            parents: &v.parents,
            ends: &v.ends,
            bars: &v.bars,
            nodes: &v.nodes,
            inputs: &v.inputs,
        };
        let gate = |a: &Action, typed: &[TypedText]| {
            evaluate_with(
                &policy,
                a,
                &v.screen,
                None,
                ScreenSource::Typed,
                Some(&tree),
                typed,
            )
        };
        // Fourteenth review: phone-only questions among the taps with the state set, the
        // same taps with "Sending a message" relaxed, and `type … submit` of the owner's own
        // words (and of other words) into every text input.
        let mut relaxed = Policy::default();
        relaxed
            .confirm
            .retain(|c| *c != jarvis_proto::RiskCategory::SendMessage);
        let owner = ["What's the weather in Paris"];
        let owned = |p: &Policy, a: &Action, typed: &[TypedText]| {
            evaluate_owned(
                p,
                a,
                &v.screen,
                None,
                ScreenSource::Typed,
                Some(&tree),
                typed,
                &owner,
            )
        };
        let mut extra = [0usize; 5];
        let mut row = [0usize; 7];
        for e in v.screen.elements.iter().filter(|e| {
            e.enabled && !matches!(e.role, Role::StaticText | Role::Key) && !e.role.is_text_input()
        }) {
            row[0] += 1;
            let tap = Action::Tap { element: e.index };
            row[1] += usize::from(asks(&gate(&tap, &[])));
            row[2] += usize::from(asks(&gate(&tap, &state)));
            extra[0] += usize::from(
                matches!(owned(&policy, &tap, &state), Decision::Confirm(ref r) if r.phone_only),
            );
            extra[1] += usize::from(asks(&owned(&relaxed, &tap, &state)));
            for (k, dir) in [(3, Direction::Right), (5, Direction::Up)] {
                let swipe = Action::Swipe {
                    direction: dir,
                    element: Some(e.index),
                };
                row[k] += usize::from(asks(&gate(&swipe, &[])));
                row[k + 1] += usize::from(asks(&gate(&swipe, &state)));
            }
        }
        // Return alone in each filled text input, the state unset (thirteenth review: what
        // Return sends is observed).
        let mut returns = (0usize, 0usize);
        for e in v.screen.elements.iter().filter(|e| {
            e.role.is_text_input() && e.value.as_deref().is_some_and(|v| !v.trim().is_empty())
        }) {
            returns.0 += 1;
            let ret = Action::Type {
                element: Some(e.index),
                text: String::new(),
                submit: true,
            };
            returns.1 += usize::from(asks(&gate(&ret, &[])));
        }
        for e in v.screen.elements.iter().filter(|e| e.role.is_text_input()) {
            extra[2] += 1;
            for (k, text) in [(3, "weather Paris"), (4, "cheap flights")] {
                let ret = Action::Type {
                    element: Some(e.index),
                    text: text.to_owned(),
                    submit: true,
                };
                extra[k] += usize::from(asks(&owned(&policy, &ret, &[])));
            }
        }
        println!(
            "{:<20} taps with state: phone only {} | asked with SendMessage relaxed {} | inputs {}: type+Return owner's words asks {}, other words asks {}",
            f.name, extra[0], extra[1], extra[2], extra[3], extra[4]
        );
        println!(
            "{:<20} filled inputs {} | bare Return asks {}",
            f.name, returns.0, returns.1
        );
        let scroll = Action::Scroll {
            direction: Direction::Down,
            element: None,
        };
        let s0 = asks(&gate(&scroll, &[]));
        let s1 = asks(&gate(&scroll, &state));
        println!(
            "{:<20} controls {:>3} | tap asks {:>3} / {:>3} with state | swipe right {:>3} / {:>3} | swipe up {:>3} / {:>3} | scroll down {s0} / {s1}",
            f.name, row[0], row[1], row[2], row[3], row[4], row[5], row[6]
        );
        for (t, r) in totals.iter_mut().zip(row) {
            *t += r;
        }
    }
    println!(
        "TOTAL controls {} | tap asks {} / {} with state | swipe right {} / {} | swipe up {} / {}",
        totals[0], totals[1], totals[2], totals[3], totals[4], totals[5], totals[6]
    );
}
