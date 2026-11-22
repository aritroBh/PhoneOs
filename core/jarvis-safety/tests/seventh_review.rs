#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Seventh review: amount-like field values in locale formats (6), and Return classified
//! with the field's name and the typed text (5). The review's probes are kept here as
//! regressions, with the additions the fix called for.
mod common;
use common::{app_screen, button, el, label};
use jarvis_proto::{Action, Element, Policy, Role};
use jarvis_safety::{evaluate_from, Decision, ScreenSource};

fn tf(index: u32, name: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, name);
    e.value = Some(value.to_owned());
    e
}

#[test]
fn amount_like_field_values_in_grouped_and_locale_formats() {
    for (name, v) in [
        ("Total", "4111111111111111"),
        ("Total", "4111 1111 1111 1111"),
        ("Amount", "1e9"),
        ("Amount", "-50"),
        ("Tip %", "5000"),
        ("Quantity", "123456"),
        ("Amount", "000123456789"),
        ("Amount", "5000,00"),
        ("Amount", " 50 000 "),
        ("Amount", "4.000,00 €"),
        ("Amount", "40.00 USD / month"),
        ("Amount", "4000"),
    ] {
        let s = app_screen(vec![
            label(1, "Checkout"),
            tf(2, name, v),
            button(3, "Pay now"),
        ]);
        let d = evaluate_from(
            &Policy::default(),
            &Action::Tap { element: 3 },
            &s,
            None,
            ScreenSource::Typed,
        );
        let Decision::Confirm(r) = d else {
            panic!("{d:?}")
        };
        eprintln!("{name}={v:?} -> {}", r.summary);
        // "Pay now" sends the form, so every value shows whole (section 4) after the
        // pattern redaction; the amounts list is checked below with a tap that sends
        // nothing. (Adapted: the probe asserted the amount for "000123456789" too, which
        // section 6 deliberately reads as an account number, not an amount.)
        if name == "Amount" && v.contains('4') {
            assert!(
                r.summary.contains(v.trim()),
                "the amount being paid is not shown: {}",
                r.summary
            );
        }
    }
}

#[test]
fn return_in_a_type_delete_to_confirm_field_asks() {
    for key in ["return", "done", "go"] {
        for heading in [
            "Delete your account? This cannot be undone.",
            "Close account",
            "Danger zone",
        ] {
            let mut s = app_screen(vec![
                label(1, heading),
                tf(2, "Type DELETE to confirm", "DELETE"),
            ]);
            s.keyboard_visible = true;
            s.cues.return_key = Some(key.to_owned());
            let a = Action::Type {
                element: Some(2),
                text: "DELETE".into(),
                submit: true,
            };
            let d = evaluate_from(&Policy::default(), &a, &s, None, ScreenSource::Typed);
            assert!(
                matches!(d, Decision::Confirm(_)),
                "{key} / {heading}: typing DELETE into 'Type DELETE to confirm' and Return: {d:?}"
            );
        }
    }
}

/// An amount-like field shows its value among the amounts in grouped and locale formats; a
/// plain run of 9 or more digits (an account number) and card-number groups are not
/// amounts. Eighth review: the asked tap shows the form it is in (`Sends: Amount: "…"`;
/// was "(filled)" for the non-amounts; deliberate change), after the pattern redaction
/// (card groups masked).
#[test]
fn amounts_list_grouped_formats_and_not_long_digit_runs() {
    for (v, shown) in [
        ("4.000,00 €", true),
        ("50 000", true),
        ("-50", true),
        ("+1,250.50", true),
        ("40.00 USD / month", true),
        ("$9.99/mo", true),
        ("4000", true),
        ("12345678", true),
        ("000123456789", false),
        ("123456789", false),
        ("4111 1111 1111 1111", false),
        ("1e9", false),
        ("forty", false),
    ] {
        let s = app_screen(vec![
            label(1, "Checkout"),
            tf(2, "Amount", v),
            button(3, "Remove card"),
        ]);
        let sum = jarvis_safety::summary_for(
            &Action::Tap { element: 3 },
            &s,
            jarvis_proto::RiskCategory::Delete,
            ScreenSource::Typed,
            None,
        );
        assert!(sum.contains("Sends: Amount: \""), "{v}: {sum}");
        assert!(!sum.contains("4111 1111"), "{v}: {sum}");
        if shown {
            assert!(
                sum.contains(&format!("Amounts on screen: Amount: {v}")),
                "{v}: {sum}"
            );
            assert!(!sum.contains("Amount (filled)"), "{v}: {sum}");
        } else {
            assert!(!sum.contains("Amounts on screen"), "{v}: {sum}");
        }
    }
}

fn return_in(field: Element, typed: &str, key: &str, heading: &str) -> Decision {
    let mut s = app_screen(vec![label(1, heading), field]);
    s.keyboard_visible = true;
    s.cues.return_key = Some(key.to_owned());
    let a = Action::Type {
        element: Some(2),
        text: typed.into(),
        submit: true,
    };
    evaluate_from(&Policy::default(), &a, &s, None, ScreenSource::Typed)
}

/// The typed text is read too: "DELETE" typed into a neutrally named confirmation field asks;
/// "delete" typed into a search field named "Search" is a query and does not ask (the
/// search field's leniency covers the typed text only); "Type CONFIRM to transfer" asks.
#[test]
fn return_reads_the_field_name_and_the_typed_text() {
    let d = return_in(tf(2, "Confirmation", ""), "DELETE", "return", "Danger zone");
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
    let search = el(2, Role::SearchField, "Search");
    let d = return_in(search, "delete", "search", "Photos");
    assert_eq!(d, Decision::Allow);
    let d = return_in(
        tf(2, "Type CONFIRM to transfer", ""),
        "CONFIRM",
        "return",
        "Move money",
    );
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
    // A search field whose own label is risky still asks (no leniency for the label).
    let mut risky = el(2, Role::SearchField, "Search");
    risky.label = "Type DELETE to erase all".to_owned();
    let d = return_in(risky, "DELETE", "search", "Photos");
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
    // Plain text into a plain field with a plain Return key still does not ask.
    let d = return_in(tf(2, "Notes", ""), "groceries", "return", "My notes");
    assert_eq!(d, Decision::Allow);
}

/// Return in a multi-line text view with a plain "return" key inserts a line break: the
/// typed text is not read as a command ("buy milk" is not a purchase). A single-line field,
/// or a submit-labelled Return key, keeps the strict reading.
#[test]
fn a_line_break_in_a_text_view_does_not_read_the_typed_text() {
    let note = || el(2, Role::TextView, "Note");
    let a = Action::Type {
        element: Some(2),
        text: "buy milk".into(),
        submit: true,
    };
    let category = |field: Element, key: &str| {
        let mut s = app_screen(vec![label(1, "Groceries"), field]);
        s.keyboard_visible = true;
        s.cues.return_key = Some(key.to_owned());
        jarvis_safety::classify(&a, &s, None)
    };
    // The text view's own presumption (a submit there may send) still applies; the typed
    // words add nothing.
    assert_eq!(
        category(note(), "return"),
        jarvis_proto::RiskCategory::SendMessage
    );
    assert_ne!(
        category(tf(2, "Item", ""), "return"),
        jarvis_proto::RiskCategory::None
    );
    assert_eq!(
        category(tf(2, "Item", ""), "return"),
        jarvis_proto::RiskCategory::Purchase
    );
    assert_eq!(
        category(note(), "done"),
        jarvis_proto::RiskCategory::Purchase
    );
    // "Type DELETE to confirm" + Return still asks.
    let d = return_in(
        tf(2, "Type DELETE to confirm", ""),
        "DELETE",
        "return",
        "Danger zone",
    );
    assert!(matches!(d, Decision::Confirm(_)), "{d:?}");
}
