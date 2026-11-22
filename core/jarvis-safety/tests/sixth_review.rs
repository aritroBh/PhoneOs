#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixth review: the source-level rule for runner-text screens, the summary field rule and
//! where taps land (`jarvis_safety::tap_spot`). The review's probes are kept here as named
//! regression tests.

mod common;

use common::{app_screen, button, el, label};
use jarvis_proto::{
    Action, AppRule, CompactScreen, Element, Point, Policy, Rect, RiskCategory, Role, TrustLevel,
};
use jarvis_safety::{
    control_at, evaluate_from, summarize, summary_with, tap_spot, Covered, Decision, ScreenSource,
    ScreenTree, TypedText,
};

fn tf(index: u32, name: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, name);
    e.value = Some(value.to_owned());
    e
}

fn tap(i: u32) -> Action {
    Action::Tap { element: i }
}

fn at(mut e: Element, x: f64, y: f64, w: f64, h: f64) -> Element {
    e.frame = Rect::new(x, y, w, h);
    e
}

fn trusted() -> Policy {
    let mut policy = Policy::default();
    policy
        .app_rules
        .push(AppRule::new("com.example.app", TrustLevel::Trusted));
    policy
}

// --- 4. Runner-text screens always confirm -----------------------------------------------

/// Probe: a runner-text screen in an app the owner marked Trusted. `Unverified` is in
/// `TRUSTED_ALLOWED`, so riding on the category sent the tap unasked.
#[test]
fn runner_text_in_a_trusted_app_asks() {
    let s = app_screen(vec![label(1, "Profile"), button(2, "Next")]);
    let d = evaluate_from(&trusted(), &tap(2), &s, None, ScreenSource::RunnerText);
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
    // A category Trusted allows (a setting) asks too, under its own category.
    let s = app_screen(vec![label(1, "Settings"), button(2, "Dark mode")]);
    let d = evaluate_from(&trusted(), &tap(2), &s, None, ScreenSource::RunnerText);
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
}

/// The same screen read as a typed tree in the same Trusted app does not ask.
#[test]
fn a_typed_screen_in_the_trusted_app_does_not_ask() {
    let s = app_screen(vec![label(1, "Profile"), button(2, "Next")]);
    let d = evaluate_from(&trusted(), &tap(2), &s, None, ScreenSource::Typed);
    assert_eq!(d, Decision::Allow);
}

/// The owner relaxed every category (nothing in `confirm`): a runner-text screen still asks;
/// a denied category still refuses first; waits and plain scrolls change nothing and pass.
#[test]
fn runner_text_asks_with_relaxed_categories_and_denials_come_first() {
    let mut relaxed = Policy::default();
    relaxed.confirm.clear();
    let s = app_screen(vec![
        label(1, "Profile"),
        button(2, "Next"),
        button(3, "Pay"),
    ]);
    let d = evaluate_from(&relaxed, &tap(2), &s, None, ScreenSource::RunnerText);
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
    assert_eq!(
        evaluate_from(&relaxed, &tap(2), &s, None, ScreenSource::Typed),
        Decision::Allow
    );
    let mut deny = relaxed.clone();
    deny.deny_categories.push(RiskCategory::Payment);
    let d = evaluate_from(&deny, &tap(3), &s, None, ScreenSource::RunnerText);
    assert!(matches!(d, Decision::Deny(_)), "{d:?}");
    let wait = Action::Wait { ms: 500 };
    assert_eq!(
        evaluate_from(&relaxed, &wait, &s, None, ScreenSource::RunnerText),
        Decision::Allow
    );
}

// --- 5. Summaries name fields; values only when safe -------------------------------------

/// Probe: field values the old name lists did not catch were quoted in summaries.
#[test]
fn field_values_are_not_quoted_in_summaries() {
    let fields = [
        ("CVV2", "737"),
        ("Security number", "4417"),
        ("Card no.", "4111 1111 1111 1111"),
        ("Card #", "5500 0000 0000 0004"),
        ("Answer", "Fluffy"),
        ("Mother's maiden name", "Smithers"),
        ("Date of birth", "04/07/1981"),
        ("Tax ID", "12-3456789"),
        ("Driver license", "D1234567"),
        ("Passport number", "X12345678"),
        ("Field 1", "4111111111111111"),
        ("Field 2", "482913"),
        ("Memorable word", "zebra42"),
        ("Access code", "73519"),
        ("CSC", "123"),
    ];
    let mut leaks = Vec::new();
    for (name, value) in fields {
        let s = app_screen(vec![
            tf(1, name, value),
            label(2, "Fee $25.00"),
            button(3, "Pay now"),
        ]);
        let sum = summarize(&tap(3), &s, None);
        if sum.contains(value) {
            leaks.push(format!("{name}={value}: {sum}"));
        }
        assert!(sum.contains(name.trim_end_matches('.')), "{sum}");
    }
    // A static text (not a field) with a code / full card number.
    let s = app_screen(vec![
        label(1, "Your code is 482913"),
        label(2, "Card 4111 1111 1111 1111 Total $25.00"),
        button(3, "Pay now"),
    ]);
    let sum = summarize(&tap(3), &s, None);
    for v in ["482913", "4111 1111 1111 1111"] {
        if sum.contains(v) {
            leaks.push(format!("static {v}: {sum}"));
        }
    }
    assert!(leaks.is_empty(), "{}", leaks.join("\n"));
}

/// The new names are masked in prompts as well.
#[test]
fn the_new_names_are_credential_like_fields() {
    for name in [
        "CSC",
        "CVV2",
        "Security number",
        "Memorable word",
        "Answer",
        "Mother's maiden name",
        "Date of birth",
        "Tax ID",
        "Driver license",
        "Passport number",
        "Access code",
    ] {
        assert!(
            jarvis_safety::credential_like_field(&tf(1, name, "x")),
            "{name}"
        );
    }
    assert!(!jarvis_safety::credential_like_field(&tf(1, "Note", "x")));
}

/// A message Jarvis typed is shown before Send; one it did not type is "(filled)"; an empty
/// field is "(empty)"; an amount field shows its amount; a note with an amount in it does
/// not.
#[test]
fn typed_text_amounts_and_other_fields() {
    let s = app_screen(vec![
        at(tf(1, "Message", "On my way"), 16.0, 200.0, 361.0, 44.0),
        at(el(2, Role::TextField, "Subject"), 16.0, 260.0, 361.0, 44.0),
        at(button(3, "Send"), 16.0, 700.0, 361.0, 50.0),
    ]);
    let send = |typed: &[TypedText]| {
        summary_with(
            &tap(3),
            &s,
            RiskCategory::SendMessage,
            ScreenSource::Typed,
            None,
            typed,
        )
    };
    let typed = [TypedText {
        field: 1,
        text: "On my way".to_owned(),
    }];
    assert_eq!(
        send(&typed),
        "Tap \"Send\" in Example. Fields: Message: On my way; Subject (empty)"
    );
    assert_eq!(
        send(&[]),
        "Tap \"Send\" in Example. Fields: Message (filled); Subject (empty)"
    );
    // Typed into another field, or other text: not shown.
    let elsewhere = [TypedText {
        field: 2,
        text: "On my way".to_owned(),
    }];
    assert!(send(&elsewhere).contains("Message (filled)"));
    let amounts = app_screen(vec![
        at(tf(1, "Amount", "$4,000.00"), 16.0, 200.0, 361.0, 44.0),
        at(tf(2, "Tip", "15"), 16.0, 260.0, 361.0, 44.0),
        at(tf(3, "Note", "rent $40"), 16.0, 320.0, 361.0, 44.0),
        at(tf(4, "Quantity", "two"), 16.0, 380.0, 361.0, 44.0),
        at(button(5, "Pay"), 16.0, 700.0, 361.0, 50.0),
    ]);
    let sum = summarize(&tap(5), &amounts, None);
    assert!(sum.contains("Amount: $4,000.00"), "{sum}");
    assert!(sum.contains("Tip: 15"), "{sum}");
    assert!(sum.contains("Note (filled)"), "{sum}");
    assert!(sum.contains("Quantity (filled)"), "{sum}");
    assert!(!sum.contains("rent"), "{sum}");
}

/// Recipient and selected facts never quote a field's value either.
#[test]
fn recipient_facts_never_quote_a_field_value() {
    let s = app_screen(vec![
        at(tf(1, "To", "mallory@example.com"), 16.0, 200.0, 361.0, 44.0),
        at(label(2, "Total $5.00"), 16.0, 260.0, 361.0, 22.0),
        at(button(3, "Pay"), 16.0, 700.0, 361.0, 50.0),
    ]);
    let sum = summarize(&tap(3), &s, None);
    assert!(!sum.contains("mallory"), "{sum}");
    assert!(sum.contains("To (filled)"), "{sum}");
}

// --- 1. Where taps land -------------------------------------------------------------------

fn screen_of(elements: Vec<Element>) -> CompactScreen {
    app_screen(elements)
}

/// Two buttons overlapping completely: every candidate is contested, whichever is first in
/// document order.
#[test]
fn a_fully_covered_button_has_no_point() {
    let pay = at(button(1, "Pay $40.00"), 16.0, 300.0, 361.0, 50.0);
    let mallory = at(button(2, "Pay $4,000.00"), 16.0, 302.0, 361.0, 50.0);
    let s = screen_of(vec![pay.clone(), mallory.clone()]);
    assert_eq!(
        tap_spot(&pay, &s, None, None),
        Err(Covered { target: 1, by: 2 })
    );
    assert_eq!(
        tap_spot(&mallory, &s, None, None),
        Err(Covered { target: 2, by: 1 })
    );
}

/// A partial overlap over the centre: another point inside the target is used, and it is
/// uncontested. Deterministic.
#[test]
fn a_partial_overlap_moves_the_point_inside_the_target() {
    let pay = at(button(1, "Pay"), 16.0, 300.0, 361.0, 50.0);
    let badge = at(button(2, "Info"), 150.0, 290.0, 100.0, 40.0);
    let s = screen_of(vec![pay.clone(), badge.clone()]);
    let p = tap_spot(&pay, &s, None, None).unwrap();
    assert!(pay.frame.contains(p) && !badge.frame.contains(p), "{p:?}");
    assert_eq!(tap_spot(&pay, &s, None, None).unwrap(), p);
    // Static texts, images and containers never cover.
    let mut overlay = at(
        el(3, Role::Other("Other".into()), "Overlay"),
        0.0,
        0.0,
        393.0,
        852.0,
    );
    overlay.enabled = true;
    let text = at(label(4, "Pay now"), 16.0, 300.0, 361.0, 50.0);
    let s = screen_of(vec![overlay, text, pay.clone()]);
    assert_eq!(tap_spot(&pay, &s, None, None), Ok(pay.frame.center()));
    // A disabled control does not take the touch away.
    let mut off = at(button(5, "Other"), 16.0, 300.0, 361.0, 50.0);
    off.enabled = false;
    let s = screen_of(vec![pay.clone(), off]);
    assert_eq!(tap_spot(&pay, &s, None, None), Ok(pay.frame.center()));
}

/// A switch inside a full-width cell (related in the raw tree): the switch's own point when
/// the switch is the target, the cell's when the cell is. The same frames without the
/// ancestry are a conflict for the switch (its every point is inside the cell).
#[test]
fn a_switch_inside_a_cell_and_the_cell() {
    let cell = at(el(1, Role::Cell, "Wi-Fi"), 0.0, 200.0, 393.0, 52.0);
    let switch = at(el(2, Role::Switch, "Wi-Fi"), 320.0, 210.0, 51.0, 31.0);
    let s = screen_of(vec![cell.clone(), switch.clone()]);
    // Raw nodes: 0 root, 1 cell, 2 switch (inside the cell).
    let sources = [1, 2];
    let parents = [u32::MAX, 0, 1];
    let ends = [3, 3, 3];
    let tree = ScreenTree {
        sources: &sources,
        parents: &parents,
        ends: &ends,
    };
    let p = tap_spot(&switch, &s, Some(&tree), None).unwrap();
    assert!(switch.frame.contains(p));
    let c = tap_spot(&cell, &s, Some(&tree), None).unwrap();
    assert_eq!(c, cell.frame.center());
    assert!(!switch.frame.contains(c));
    assert!(tap_spot(&switch, &s, None, None).is_err());
}

/// A raw point resolves to the smallest control containing it; two unrelated controls
/// containing it are ambiguous.
#[test]
fn a_raw_point_resolves_to_one_control_or_is_ambiguous() {
    let pay = at(button(1, "Pay"), 16.0, 300.0, 361.0, 50.0);
    let other = at(button(2, "Other"), 16.0, 320.0, 361.0, 50.0);
    let s = screen_of(vec![pay.clone(), other]);
    let only_pay = Point::new(100.0, 305.0);
    assert_eq!(control_at(only_pay, &s, None).unwrap().unwrap().index, 1);
    assert!(control_at(Point::new(100.0, 330.0), &s, None).is_err());
    assert!(control_at(Point::new(100.0, 600.0), &s, None)
        .unwrap()
        .is_none());
}
