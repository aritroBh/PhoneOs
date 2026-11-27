#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Review pass 36: the look-alike character sets (`jarvis_safety::sets`) and the bounds of
//! one decision on a hostile page.

mod common;

use common::{app_screen, button, label};
use jarvis_proto::{Action, CompactScreen, Element, Policy, Rect, RiskCategory};
use jarvis_safety::{classify, evaluate, Decision};

fn tap(l: &str) -> RiskCategory {
    classify(
        &Action::Tap { element: 1 },
        &app_screen(vec![button(1, l)]),
        None,
    )
}

/// Exceptions: an ordinary exception keeps its control harmless; a look-alike spelling of
/// one never makes a risk word harmless (it asks under the word's category).
#[test]
fn exceptions_hold_only_when_spelled_as_they_are_read() {
    for l in [
        "Don't Allow",
        "Resend Code",
        "Send Code",
        "Call History",
        "Restore Purchases",
    ] {
        assert_eq!(tap(l), RiskCategory::None, "{l:?}");
    }
    for (l, want) in [
        ("D0n't AIIow", RiskCategory::SystemSetting),
        ("Don't AIIow", RiskCategory::SystemSetting),
        ("Resend C0de", RiskCategory::SendMessage),
        ("CaII History", RiskCategory::Call),
        ("Rest0re Purchases", RiskCategory::Delete),
    ] {
        assert_eq!(tap(l), want, "{l:?}");
    }
}

/// The matcher reads every combination at once: `l`/`I` mixes, word-initial folds, case
/// steps, camel joins, non-ASCII digits, the two-way characters both ways in one word.
#[test]
fn combinations_pass_35_could_not_enumerate() {
    for (l, want) in [
        ("1nsta11 N0w", RiskCategory::SystemSetting),
        ("lnstall Now", RiskCategory::SystemSetting),
        ("UnInStAlL", RiskCategory::Delete),
        ("DeIeTeAccOunT", RiskCategory::Delete),
        ("SIGN 0UT", RiskCategory::AccountChange),
        ("\u{1D7CE}rder N\u{1D7CE}w", RiskCategory::Purchase),
        ("CIear AII", RiskCategory::Delete),
        ("BLOCK", RiskCategory::AccountChange),
        ("bl0cK", RiskCategory::AccountChange),
    ] {
        assert_eq!(tap(l), want, "{l:?}");
    }
}

fn stacked(labels: &[String], risky: Option<&str>) -> CompactScreen {
    let mut els: Vec<Element> = labels
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let mut e = button(u32::try_from(i).unwrap() + 1, l);
            e.frame = Rect::new(16.0, 300.0, 361.0, 44.0);
            e
        })
        .collect();
    if let Some(r) = risky {
        let mut e = button(9_000, r);
        e.frame = Rect::new(16.0, 300.0, 361.0, 44.0);
        els.push(e);
    }
    els.push(label(9_001, "Library"));
    app_screen(els)
}

fn timed(s: &CompactScreen, policy: &Policy) -> (f64, Decision) {
    let at = Action::TapPoint {
        x: 196.0 / 393.0,
        y: 322.0 / 852.0,
    };
    let t = std::time::Instant::now();
    let d = evaluate(policy, &at, s, None);
    (t.elapsed().as_secs_f64(), d)
}

/// F8: a `tap_point` over 3,000 stacked controls whose labels are long, look-alike-heavy
/// text (1,056 characters: past the bound, asked without being read; 484 characters: read
/// by the sets) decides in time comparable to the same page of ordinary text of the same
/// length, and the risky control under the point is still found: asked, and refused by a
/// policy that denies it (more than 32 controls ask phone only; no control under the point
/// goes unread because of a bound).
#[test]
fn a_hostile_page_decides_in_time_bounded_by_an_ordinary_one() {
    let deny = Policy {
        deny_categories: vec![RiskCategory::Payment],
        ..Policy::default()
    };
    for (name, n) in [("past the bound", 48), ("read", 22)] {
        let hostile: Vec<String> = (0..3000)
            .map(|_| "De1ete BIock 5eII |og ".repeat(n))
            .collect();
        let ordinary: Vec<String> = (0..3000)
            .map(|_| "Recently played songs ".repeat(n))
            .collect();
        assert_eq!(hostile[0].len(), ordinary[0].len());
        let (base, _) = timed(&stacked(&ordinary, Some("Pay now")), &Policy::default());
        let s = stacked(&hostile, Some("Pay now"));
        let (t, d) = timed(&s, &Policy::default());
        println!("{name}: {t:.3} s; ordinary {base:.3} s");
        assert!(
            t <= 4.0 * base + 0.05,
            "{name}: {t:.3} s against {base:.3} s"
        );
        match d {
            Decision::Confirm(r) => {
                assert_eq!(r.category, RiskCategory::Payment, "{name}");
                assert!(r.phone_only, "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }
        assert!(matches!(timed(&s, &deny).1, Decision::Deny(_)), "{name}");
        // Without the risky control, the look-alike controls themselves are asked.
        let (_, d) = timed(&stacked(&hostile, None), &Policy::default());
        assert!(
            matches!(&d, Decision::Confirm(r) if r.category != RiskCategory::None),
            "{name}: {d:?}"
        );
    }
}

/// F8: a control whose label passes the bound is asked as unverified, never read and
/// trusted; a real label (the longest in the fixtures is 383 characters) is read.
#[test]
fn a_label_past_the_bound_is_unverified() {
    let long = format!("Settings {}", "x".repeat(600));
    assert_eq!(tap(&long), RiskCategory::Unverified);
    let real = format!("Settings {}", "word ".repeat(70));
    assert_eq!(tap(&real), RiskCategory::None);
}
