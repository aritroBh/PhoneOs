#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Summary facts must not leak and must not silently drop (fifth review). Summaries travel
//! to the owner's sheet, remote devices, the transcript and the history. The first three
//! tests are the fifth review's probes, kept as regressions.

mod common;

use common::{app_screen, button, el, label};
use jarvis_proto::{Action, Element, Rect, RiskCategory, Role};
use jarvis_safety::{
    evaluate_from, redact_screen, summarize, summary_for, Decision, ScreenSource, ScreenTree,
    UNVERIFIED_SCREEN_WARNING,
};

fn at(mut e: Element, x: f64, y: f64, w: f64, h: f64) -> Element {
    e.frame = Rect::new(x, y, w, h);
    e
}

fn tf(index: u32, name: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, name);
    e.value = Some(value.to_owned());
    e
}

fn tap(i: u32) -> Action {
    Action::Tap { element: i }
}

// --- Credentials ------------------------------------------------------------------------

/// Fifth review probe: field values the prompt masks were shown verbatim in the summary.
#[test]
fn credential_field_values_never_reach_the_summary() {
    let s = app_screen(vec![
        label(1, "Verify your identity"),
        tf(2, "SSN", "123-45-6789"),
        tf(3, "Expiration date", "12/29"),
        tf(4, "Recovery phrase", "orbit"),
        label(5, "Fee $25.00"),
        button(6, "Pay now"),
    ]);
    let summary = summarize(&tap(6), &s, None);
    for secret in ["123-45-6789", "12/29", "orbit"] {
        assert!(!summary.contains(secret), "{secret}: {summary}");
    }
    // The prompt masks them the same way.
    let safe = redact_screen(&s);
    for e in &safe.elements[1..4] {
        assert_ne!(e.value, s.elements[e.index as usize - 1].value);
    }
}

/// Every protected kind of field is listed by name only, "(hidden)", whether it is named by
/// its label or by its placeholder; ordinary fields keep their value.
#[test]
fn protected_fields_are_listed_by_name_only() {
    let mut by_placeholder = el(9, Role::TextField, "");
    by_placeholder.placeholder = Some("Routing number".to_owned());
    by_placeholder.value = Some("021000021".to_owned());
    let fields = [
        ("Password", "hunter2hunter2"),
        ("Passcode", "246810"),
        ("PIN", "8642"),
        ("Card number", "4111 1111 1111 1111"),
        ("CVC", "737"),
        ("Security code", "9182"),
        ("Expiry", "11/31"),
        ("Seed phrase", "apple banana cherry"),
        ("One-time code", "482913"),
        ("Verification code", "771122"),
        ("IBAN", "DE89 3704 0044 0532 0130 00"),
        ("Account number", "000123456789"),
        ("Social security number", "078-05-1120"),
    ];
    for (name, value) in fields {
        let s = app_screen(vec![
            tf(1, name, value),
            tf(2, "Note", "dinner"),
            button(3, "Continue"),
        ]);
        let sum = summarize(&tap(3), &s, None);
        assert!(!sum.contains(value), "{name}: {sum}");
        assert!(sum.contains(&format!("{name} (hidden)")), "{name}: {sum}");
        // Sixth review: an ordinary field Jarvis did not type is named, not quoted (was
        // "Note: dinner"; deliberate change).
        assert!(sum.contains("Note (filled)"), "{sum}");
    }
    let s = app_screen(vec![by_placeholder, button(10, "Send")]);
    let sum = summarize(&tap(10), &s, None);
    assert!(!sum.contains("021000021"), "{sum}");
    assert!(sum.contains("Routing number (hidden)"), "{sum}");
}

/// Recipient and amount facts never quote a protected field either.
#[test]
fn protected_fields_feed_no_other_fact() {
    let s = app_screen(vec![
        tf(1, "Account number", "••••4521 $500.00"),
        tf(2, "Amount", "$20.00"),
        label(3, "To account ••••9999"),
        button(4, "Send money"),
    ]);
    let sum = summarize(&tap(4), &s, None);
    assert!(!sum.contains("4521"), "{sum}");
    assert!(!sum.contains("$500.00"), "{sum}");
    assert!(sum.contains("Amount: $20.00"), "{sum}");
    assert!(sum.contains("9999"), "{sum}");
}

// --- Amounts ----------------------------------------------------------------------------

/// Fifth review probe: five $0.00 decoys with charge wording pushed the real yearly charge
/// into "and N more". "Renews" is charge wording now, and the largest amount is always
/// listed.
#[test]
fn charge_worded_decoys_cannot_hide_the_real_price() {
    let s = app_screen(vec![
        label(1, "Total savings $0.00"),
        label(2, "Pay later $0.00"),
        label(3, "Due at signup $0.00"),
        label(4, "Billing fee $0.00"),
        label(5, "Subtotal today $0.00"),
        label(6, "Renews yearly at $499.00"),
        button(7, "Start free trial"),
    ]);
    let summary = summarize(&tap(7), &s, None);
    assert!(summary.contains("$499.00"), "{summary}");
    assert!(summary.contains("and 1 more"), "{summary}");
}

/// The largest amount is listed whatever its wording, and recurring wording ("per month",
/// "/mo", "then", "after trial") counts as a charge.
#[test]
fn the_largest_amount_is_always_listed() {
    let decoys: Vec<Element> = (1..=6)
        .map(|i| label(i, &format!("Total item {i} $1.0{i}")))
        .collect();
    let mut els = decoys;
    els.push(label(7, "Membership $899.00"));
    els.push(button(8, "Join"));
    let sum = summarize(&tap(8), &app_screen(els), None);
    assert!(sum.contains("$899.00"), "{sum}");
    assert!(sum.contains("and 2 more"), "{sum}");
    // Recurring wording counts as a charge: it ranks before larger amounts without it.
    let plan = |wording: &str| {
        let mut els: Vec<Element> = (1..=5)
            .map(|i| label(i, &format!("Compare at $6{i}.00")))
            .collect();
        els.push(label(6, &format!("Plus {wording}")));
        els.push(button(7, "Subscribe"));
        summarize(&tap(7), &app_screen(els), None)
    };
    for wording in [
        "$9.99 per month",
        "$9.99/mo",
        "then $9.99",
        "$9.99 after trial",
        "renews at $9.99",
    ] {
        let sum = plan(wording);
        assert!(sum.contains("$9.99"), "{wording}: {sum}");
    }
    let sum = plan("$9.99");
    assert!(
        !sum.contains("$9.99") && sum.contains("and 1 more"),
        "{sum}"
    );
}

// --- To/Account ---------------------------------------------------------------------------

/// Fifth review probe: a help line and a reference number above the payee pushed the
/// account out silently. Recipient wording ranks first, reference/help numbers last, three
/// are listed and the rest counted.
#[test]
fn recipient_decoys_cannot_drop_the_payee() {
    let s = app_screen(vec![
        label(1, "Help: +1 (800) 555-0100"),
        label(2, "Reference 20261122"),
        label(3, "Send $500.00"),
        label(4, "To account ••••9999"),
        button(5, "Send"),
    ]);
    let summary = summarize(&tap(5), &s, None);
    assert!(
        summary.contains("To/Account: To account ••••9999;"),
        "{summary}"
    );
    let s = app_screen(vec![
        label(1, "Order 1234567890"),
        label(2, "Support 18005550100"),
        label(3, "alex@example.com"),
        label(4, "@sam"),
        label(5, "Pay to ••••4521"),
        label(6, "$5.00"),
        button(7, "Pay"),
    ]);
    let summary = summarize(&tap(7), &s, None);
    assert!(summary.contains("Pay to ••••4521"), "{summary}");
    assert!(summary.contains("and 2 more"), "{summary}");
    assert!(
        !summary.contains("1234567890"),
        "references rank last: {summary}"
    );
}

/// With no room left a fact list says how many entries it has instead of vanishing.
#[test]
fn a_fact_list_without_room_counts_its_entries() {
    let long = "x".repeat(60);
    let mut els: Vec<Element> = (1..=5)
        .map(|i| label(i, &format!("{long} {i} $1{i}.00")))
        .collect();
    els.push(tf(6, &format!("Note {long}"), &format!("first {long}")));
    els.push(tf(7, &format!("Memo {long}"), &format!("second {long}")));
    els.push(label(8, "To account ••••4521"));
    els.push(button(9, &format!("Pay {long}")));
    // Sixth review: field values show only when Jarvis typed them, so the long values are
    // typed here (was: shown unconditionally).
    let typed = [
        jarvis_safety::TypedText {
            field: 6,
            text: format!("first {long}"),
        },
        jarvis_safety::TypedText {
            field: 7,
            text: format!("second {long}"),
        },
    ];
    let sum = jarvis_safety::summary_with(
        &tap(9),
        &app_screen(els),
        RiskCategory::Payment,
        ScreenSource::Typed,
        None,
        &typed,
    );
    assert!(sum.contains("Fields: 2 not shown"), "{sum}");
    assert!(sum.chars().count() <= 320, "{sum}");
}

// --- Under "…" ----------------------------------------------------------------------------

/// With the raw tree known, `under "…"` prefers texts in the target's own container over
/// texts that are merely closer above it.
#[test]
fn under_prefers_the_targets_own_container() {
    let s = app_screen(vec![
        at(label(1, "Order #1001"), 16.0, 100.0, 361.0, 20.0),
        at(label(2, "Order #1002"), 16.0, 130.0, 361.0, 20.0),
        at(button(3, "Delete order"), 16.0, 160.0, 361.0, 40.0),
    ]);
    // Raw tree: 0 root, 1 window, 2 card A {3 text #1001, 4 button}, 5 card B {6 #1002}.
    let parents = [u32::MAX, 0, 1, 2, 2, 1, 5];
    let ends = [7, 7, 5, 4, 5, 7, 7];
    let sources = [3, 6, 4];
    let tree = ScreenTree {
        sources: &sources,
        parents: &parents,
        ends: &ends,
        bars: &[],
    };
    let plain = summary_for(&tap(3), &s, RiskCategory::Delete, ScreenSource::Typed, None);
    assert!(
        plain.contains("under \"Order #1001, Order #1002\""),
        "{plain}"
    );
    let sum = summary_for(
        &tap(3),
        &s,
        RiskCategory::Delete,
        ScreenSource::Typed,
        Some(&tree),
    );
    assert!(sum.contains("under \"Order #1001\""), "{sum}");
}

// --- Runner text ----------------------------------------------------------------------

/// On a runner-text screen a harmless tap is `Unverified` and the summary starts with the
/// warning; a wait is not asked.
#[test]
fn runner_text_screens_raise_state_changes_to_unverified() {
    let s = app_screen(vec![label(1, "Hello"), button(2, "Next")]);
    let policy = jarvis_proto::Policy::default();
    assert_eq!(
        evaluate_from(&policy, &tap(2), &s, None, ScreenSource::Typed),
        Decision::Allow
    );
    let Decision::Confirm(req) =
        evaluate_from(&policy, &tap(2), &s, None, ScreenSource::RunnerText)
    else {
        panic!()
    };
    assert_eq!(req.category, RiskCategory::Unverified);
    assert!(
        req.summary.starts_with(UNVERIFIED_SCREEN_WARNING),
        "{}",
        req.summary
    );
    assert!(req.summary.chars().count() <= 320 + UNVERIFIED_SCREEN_WARNING.len());
    assert_eq!(
        evaluate_from(
            &policy,
            &Action::Wait { ms: 500 },
            &s,
            None,
            ScreenSource::RunnerText
        ),
        Decision::Allow
    );
    // Denied apps still refuse.
    let mut wallet = s.clone();
    wallet.app_bundle_id = jarvis_proto::WALLET_BUNDLE_ID.to_owned();
    assert!(matches!(
        evaluate_from(&policy, &tap(2), &wallet, None, ScreenSource::RunnerText),
        Decision::Deny(_)
    ));
}
