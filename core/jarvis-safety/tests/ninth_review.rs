#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Ninth review probes (jarvis-safety). Each asserts the SAFE behaviour.
mod common;
use common::{app_screen, button, el, label};
use jarvis_proto::{Action, Element, Policy, Role};
use jarvis_safety::{evaluate_from, Decision, ScreenSource};

fn tf(index: u32, name: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, name);
    e.value = Some(value.to_owned());
    e
}

/// HIGH. The priority order keeps every "protected" (recipient- or amount-like) sent field
/// in the list, and only when even those do not fit does it fall to stage (a), which drops
/// the amounts and recipients *as well* (`B_LEVELS[4] = (0, 0)` with `shape(0, 0, 0)`).
/// A checkout page that adds a dozen fields whose values look like handles or emails
/// (reference codes, "gift to" boxes) pushes the payee out of a 106-character question,
/// which then says only "Sends: N fields (0 shown)".
#[test]
fn probe9_many_recipient_like_fields_collapse_the_question() {
    for n in [8u32, 12, 16, 24, 40] {
        let mut els = vec![
            label(1, "Checkout"),
            label(2, "Pay to: @mallory"),
            label(3, "Total $5,000.00"),
        ];
        for i in 0..n {
            els.push(tf(
                4 + i,
                &format!("Gift recipient email address number {i}"),
                &format!("friend.number.{i}.with.a.long.address@example-mail-provider.com"),
            ));
        }
        els.push(button(4 + n, "Pay now"));
        let s = app_screen(els);
        let d = evaluate_from(
            &Policy::default(),
            &Action::Tap { element: 4 + n },
            &s,
            None,
            ScreenSource::Typed,
        );
        let Decision::Confirm(r) = d else {
            panic!("{d:?}")
        };
        eprintln!("{n}: {} chars: {}", r.summary.chars().count(), r.summary);
        assert!(
            r.summary.contains("5,000.00") && r.summary.contains("mallory"),
            "{n} fields: the charge or the payee is not in the question: {}",
            r.summary
        );
    }
}

/// HIGH. The same on an ordinary 7-field checkout: the contact fields hold emails and phone
/// numbers, so they are all "protected" sent values, which outrank the screen's payee: stage
/// (b) cuts `recipient_entry` to 0 ("To/Account: 1 not shown") while every contact email is
/// still quoted whole. With one more field the whole question collapses (see above).
#[test]
fn probe9_contact_emails_outrank_the_payee() {
    let fields = [
        ("Contact email", "john.appleseed.personal@icloud.com"),
        ("Mobile phone number", "+1 (415) 555-0134"),
        (
            "Billing contact email",
            "john.appleseed.billing@company-example.com",
        ),
        ("Shipping contact phone", "+1 (415) 555-0199"),
        (
            "Gift recipient email (optional)",
            "mary.appleseed.family@gmail.com",
        ),
        ("Gift recipient phone (optional)", "+1 (650) 555-0112"),
        ("Send receipt to", "receipts.archive.appleseed@icloud.com"),
    ];
    let mut els = vec![
        label(1, "Checkout"),
        label(2, "Pay to: @mallory_merch"),
        label(3, "Order total $1,249.00"),
    ];
    for (i, (n, v)) in fields.iter().enumerate() {
        els.push(tf(4 + i as u32, n, v));
    }
    let b = 4 + fields.len() as u32;
    els.push(button(b, "Place order"));
    let s = app_screen(els);
    let d = evaluate_from(
        &Policy::default(),
        &Action::Tap { element: b },
        &s,
        None,
        ScreenSource::Typed,
    );
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    eprintln!("{} chars: {}", r.summary.chars().count(), r.summary);
    assert!(
        r.summary.contains("1,249.00") && r.summary.contains("@mallory_merch"),
        "the charge or the payee is not in the question: {}",
        r.summary
    );
}
