#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-second review probes (safety crate). Each asserts the SAFE behaviour.

mod common;

use std::time::Instant;

use common::*;
use jarvis_proto::{Action, CompactScreen, Direction, Element, Rect, Role};
use jarvis_safety::{classify, InputKind};

/// `n` enabled full-width buttons, all with the same tall frame (a page that stacks
/// absolutely positioned full-width buttons on top of each other; each is a `<button>`
/// with its own text, so compaction lists each one).
fn stacked(n: u32) -> CompactScreen {
    let els: Vec<Element> = (0..n)
        .map(|i| {
            let mut e = button(i, &format!("Card {i}"));
            e.frame = Rect::new(0.0, 300.0, 393.0, 300.0);
            e
        })
        .collect();
    app_screen(els)
}

fn time<F: FnMut()>(mut f: F) -> f64 {
    let t = Instant::now();
    f();
    t.elapsed().as_secs_f64()
}

/// G: the kind decision of an element-less sideways scroll on a screen whose horizontal
/// path crosses many tall full-width buttons. `reacts_to_drag` → `is_list_row` →
/// `is_page` scans every element for each crossed one: O(n²). Safe: the kind decision
/// (and the gate's classification of the same scroll) grows about linearly, and a
/// 20 000-element screen is decided in well under a second.
#[test]
fn g1_kind_of_a_sideways_scroll_is_not_quadratic() {
    let scroll = Action::Scroll {
        direction: Direction::Right,
        element: None,
    };
    let small = stacked(2_000);
    let big = stacked(8_000);
    let mut k1 = InputKind::Scroll;
    let mut k2 = InputKind::Scroll;
    let t_small = time(|| k1 = InputKind::of(&scroll, &small));
    let t_big = time(|| k2 = InputKind::of(&scroll, &big));
    let t_classify = time(|| {
        let _ = classify(&scroll, &big, None);
    });
    eprintln!(
        "InputKind::of 2k: {t_small:.3}s ({k1:?}), 8k: {t_big:.3}s ({k2:?}), ratio {:.1}; classify 8k: {t_classify:.3}s",
        t_big / t_small.max(1e-6)
    );
    let huge = stacked(20_000);
    let t_huge = time(|| {
        let _ = InputKind::of(&scroll, &huge);
    });
    eprintln!("InputKind::of 20k: {t_huge:.3}s");
    assert!(
        t_huge < 1.0,
        "the kind decision took {t_huge:.2}s on 20k stacked buttons (8k/2k ratio {:.1})",
        t_big / t_small.max(1e-6)
    );
}

/// A (refutation): the element-less scroll's path does not depend on the screen's content:
/// two different screens of one size give the same drag for one direction (only the
/// keyboard changes it, by a flag the model does not set without an input).
#[test]
fn a1_screen_scroll_path_is_fixed() {
    use jarvis_safety::gestures::{drag_gesture, GestureKind};
    let a = app_screen(vec![button(0, "One")]);
    let b = stacked(40);
    for d in [
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
    ] {
        let ga = drag_gesture(GestureKind::Scroll, d, None, &a).unwrap();
        let gb = drag_gesture(GestureKind::Scroll, d, None, &b).unwrap();
        assert_eq!(ga, gb);
    }
}

/// B (refutation): no XCTest element type name other than the four container types
/// normalises to a scroller name; an `Other` named e.g. "ScrollBar", "TableRow",
/// "TableColumn", "Group" is never a scroller, so a scroll naming one is full.
#[test]
fn b1_other_type_names_are_not_scrollers() {
    for name in [
        "ScrollBar",
        "TableRow",
        "TableColumn",
        "Group",
        "Other",
        "Map",
        "Type99",
        "LayoutArea",
    ] {
        let mut e = button(0, "x");
        e.role = Role::Other(name.to_owned());
        e.frame = Rect::new(0.0, 200.0, 393.0, 400.0);
        let s = app_screen(vec![e]);
        let k = InputKind::of(
            &Action::Scroll {
                direction: Direction::Down,
                element: Some(0),
            },
            &s,
        );
        assert_eq!(k, InputKind::Full, "{name}");
    }
}
