#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! List rows and pages (fifth review): height alone never makes a page. A full-width tall
//! card is a page only when nothing of its family is stacked above or below it at a
//! similar width; a stack of tall rows is a list, whose sideways swipe asks as Delete.
//! The first two tests are the fifth review's probes, kept as regressions.

mod common;

use common::{el, screen};
use jarvis_proto::{Action, Direction, Element, Rect, RiskCategory, Role, Size};
use jarvis_safety::classify;

fn at(mut e: Element, x: f64, y: f64, w: f64, h: f64) -> Element {
    e.frame = Rect::new(x, y, w, h);
    e
}

fn swipe_left(i: u32) -> Action {
    Action::Swipe {
        direction: Direction::Left,
        element: Some(i),
    }
}

/// Fifth review probe: landscape (852 x 393 pt) Mail rows with a 3-line preview are 110 pt,
/// more than a quarter of the height.
#[test]
fn mail_rows_in_landscape_still_ask_delete() {
    let mut s = screen(
        "com.apple.mobilemail",
        "Mail",
        vec![
            at(
                el(1, Role::Cell, "Alex, Contract, Please sign by Friday, 9:41"),
                0.0,
                60.0,
                852.0,
                110.0,
            ),
            at(
                el(2, Role::Cell, "Bank, Statement ready, 9:12"),
                0.0,
                170.0,
                852.0,
                110.0,
            ),
            at(
                el(3, Role::Cell, "Mom, Dinner?, 8:03"),
                0.0,
                280.0,
                852.0,
                110.0,
            ),
        ],
    );
    s.size = Size::new(852.0, 393.0);
    assert_eq!(classify(&swipe_left(1), &s, None), RiskCategory::Delete);
}

/// Fifth review probe: at an accessibility text size a Mail row is 240 pt on an 852 pt
/// screen.
#[test]
fn large_text_mail_rows_in_portrait_still_ask_delete() {
    let mut s = screen(
        "com.apple.mobilemail",
        "Mail",
        vec![
            at(
                el(1, Role::Cell, "Alex, Contract, Please sign by Friday, 9:41"),
                0.0,
                120.0,
                393.0,
                240.0,
            ),
            at(
                el(2, Role::Cell, "Bank, Statement ready, 9:12"),
                0.0,
                360.0,
                393.0,
                240.0,
            ),
        ],
    );
    s.size = Size::new(393.0, 852.0);
    assert_eq!(classify(&swipe_left(1), &s, None), RiskCategory::Delete);
}

/// Three 240 pt rows stacked are rows; the same card alone is a page.
#[test]
fn a_stack_of_tall_rows_is_a_list_and_a_lone_tall_card_is_a_page() {
    let rows = |n: u32| {
        let els = (1..=n)
            .map(|i| {
                at(
                    el(i, Role::Cell, &format!("Note {i}, edited today")),
                    0.0,
                    100.0 + 240.0 * f64::from(i - 1),
                    393.0,
                    240.0,
                )
            })
            .collect();
        let mut s = screen("com.apple.mobilenotes", "Notes", els);
        s.size = Size::new(393.0, 852.0);
        s
    };
    for i in 1..=3 {
        assert_eq!(
            classify(&swipe_left(i), &rows(3), None),
            RiskCategory::Delete,
            "row {i}"
        );
    }
    assert_ne!(
        classify(&swipe_left(1), &rows(1), None),
        RiskCategory::Delete
    );
    // A narrower element below (a "Continue" button) does not make the card a row.
    let mut s = rows(1);
    s.elements.push(at(
        el(2, Role::Button, "Continue"),
        100.0,
        700.0,
        193.0,
        50.0,
    ));
    assert_ne!(classify(&swipe_left(1), &s, None), RiskCategory::Delete);
}
