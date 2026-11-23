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
//!
//! Fifteenth review: also taps on controls inside an alert (by the raw tree's roles), and a
//! plain `type` (no Return) of the owner's words and of other words into every text input
//! (R1: other words ask before they are typed).

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
    // Fifteenth review: alert controls, alert taps that ask; inputs, owner's words typed that
    // ask, other words typed that ask.
    let mut fifteen = [0usize; 5];
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
        let in_alert = |e: &jarvis_proto::Element| {
            let mut at = v
                .sources
                .get(e.index as usize - 1)
                .and_then(|s| v.parents.get(*s as usize))
                .copied();
            while let Some(n) = at.filter(|n| *n != u32::MAX) {
                if v.nodes.iter().any(|r| r.node == n && r.role == Role::Alert) {
                    return true;
                }
                at = v.parents.get(n as usize).copied();
            }
            false
        };
        let mut extra = [0usize; 5];
        let mut row = [0usize; 7];
        for e in v.screen.elements.iter().filter(|e| {
            e.enabled && !matches!(e.role, Role::StaticText | Role::Key) && !e.role.is_text_input()
        }) {
            row[0] += 1;
            let tap = Action::Tap { element: e.index };
            if in_alert(e) {
                fifteen[0] += 1;
                fifteen[1] += usize::from(asks(&owned(&policy, &tap, &state)));
            }
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
            fifteen[2] += 1;
            for (k, text) in [(3, "weather Paris"), (4, "cheap flights")] {
                let typing = Action::Type {
                    element: Some(e.index),
                    text: text.to_owned(),
                    submit: false,
                };
                fifteen[k] += usize::from(asks(&owned(&policy, &typing, &[])));
            }
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
    println!(
        "TOTAL alert controls {} (taps that ask {}) | text inputs {}: type owner's words asks {}, other words asks {}",
        fifteen[0], fifteen[1], fifteen[2], fifteen[3], fifteen[4]
    );
}

/// Sixteenth review: the context of each real tree (Rule T: whether it counts as a read and
/// how its place is named), the enabled controls a tap on which is Rule P's insert-content
/// control (each asks whatever the policy), and the enabled controls in all (each input
/// asks once per place after another place was read, then once per new source).
#[test]
fn flow_contexts_on_real_trees() {
    let fixtures = load_all(&default_dir()).unwrap();
    let mut inserts_total = 0usize;
    let mut inputs_total = 0usize;
    let mut embedded_total = 0usize;
    let mut hands_on_total = 0usize;
    for f in &fixtures {
        // Home screen rule: the model is shown a SpringBoard screen without its widgets.
        let shown = jarvis_safety::home_view(&f.screen);
        let raw = shown.as_ref().unwrap_or(&f.screen);
        let v = compact_view(raw);
        let tree = ScreenTree {
            sources: &v.sources,
            parents: &v.parents,
            ends: &v.ends,
            bars: &v.bars,
            nodes: &v.nodes,
            inputs: &v.inputs,
        };
        let ctx = jarvis_safety::observed_context(raw, &v.screen, ScreenSource::Typed);
        if f.name == "springboard_home" {
            // Was a read (its Maps and Calendar widgets); now it shows icons only.
            assert!(ctx.is_none(), "{ctx:?}");
        }
        // Seventeenth review: every place on the screen, and how many controls send their
        // input into embedded web content (asked once per task after the app's own content
        // was read) rather than into the screen's own place.
        let places = jarvis_safety::observed_places(
            raw,
            &v.screen,
            ScreenSource::Typed,
            Some((&v.webviews, &v.ends, &v.sources)),
        );
        let mut inserts = 0usize;
        let mut inputs = 0usize;
        let mut embedded = 0usize;
        let mut hands_on = 0usize;
        for e in v.screen.elements.iter().filter(|e| e.enabled) {
            let a = Action::Tap { element: e.index };
            if jarvis_safety::flow_checked(&a) {
                inputs += 1;
            }
            if places
                .destinations(&a, &v.screen, Some(&tree))
                .iter()
                .any(|d| Some(d) == places.embedded())
            {
                embedded += 1;
            }
            if jarvis_safety::insert_control(&a, &v.screen, Some(&tree)).is_some() {
                inserts += 1;
                println!("  {}: insert control {:?} {:?}", f.name, e.role, e.label);
            }
            if jarvis_safety::handoff_control(&a, &v.screen, Some(&tree)) {
                hands_on += 1;
                println!("  {}: hand-on control {:?} {:?}", f.name, e.role, e.label);
            }
        }
        inserts_total += inserts;
        inputs_total += inputs;
        embedded_total += embedded;
        hands_on_total += hands_on;
        println!(
            "{}: context {:?}; places read {:?}; web views {}; enabled controls {inputs}; into embedded content {embedded}; Rule P controls {inserts}; hand-on controls {hands_on}",
            f.name,
            ctx,
            places.read_places(),
            v.webviews.len()
        );
    }
    println!(
        "TOTAL enabled controls {inputs_total}; into embedded content {embedded_total}; Rule P controls {inserts_total}; hand-on controls {hands_on_total}"
    );
}
