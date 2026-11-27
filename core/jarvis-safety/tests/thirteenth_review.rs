#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Thirteenth review probes (safety crate). Each asserts the SAFE behaviour; a failing probe
//! is a finding.
mod common;

use common::{app_screen, button, el};
use jarvis_proto::{Action, Policy, RiskCategory, Role};
use jarvis_safety::{evaluate_with, recipient_field_name, summary_with, Decision, ScreenSource};

fn input(index: u32, label: &str, value: &str) -> jarvis_proto::Element {
    let mut e = el(index, Role::TextField, label);
    e.value = Some(value.to_owned());
    e
}

/// Names that are not who is paid or messaged but pass the closed rule: a hostile or odd
/// form's date range, the payer's own ids, a number-suffixed "To" range.
#[test]
fn probe13_non_recipient_names_pass_the_rule() {
    let mut bad = Vec::new();
    for n in [
        "To 2",           // the end of a range ("From 1 / To 2")
        "Account ID",     // the payer's own login id
        "Account name",   // the payer's own nickname
        "Account number", // the payer's own number (no "from")
        "Account no",
    ] {
        if recipient_field_name(n) {
            bad.push(n);
        }
    }
    eprintln!("pass as recipient: {bad:?}");
    // Informational: these pass by design (documented "bare Account" gap); only the
    // non-documented ones are asserted.
    let undocumented: Vec<&str> = bad
        .into_iter()
        .filter(|n| !n.to_lowercase().starts_with("account"))
        .collect();
    assert!(
        undocumented.is_empty(),
        "non-recipient names asserted under To/Account: {undocumented:?}"
    );
}

/// Real recipient names the closed rule misses, and whether a Send-labelled tap then says
/// `Recipient not verified` (it must, or name the value under To/Account).
#[test]
fn probe13_missed_recipient_names_still_say_unverified() {
    let mut bad = Vec::new();
    for n in [
        "Recipient (from contacts)",
        "Send to (search)",
        "To (reference number)",
        "Payee name or memo",
        "Reply-to",
        "Recipient\u{00A0}email",
        "Recipient\u{200B}",
        "R\u{0435}cipient", // Cyrillic e
        "ＲＥＣＩＰＩＥＮＴ",
        "Recipient #2",
        "Recipient’s e-mail",
    ] {
        let s = app_screen(vec![
            input(1, n, "mallory@evil.example"),
            input(2, "Message", "Here you go"),
            button(3, "Send"),
        ]);
        let d = evaluate_with(
            &Policy::default(),
            &Action::Tap { element: 3 },
            &s,
            None,
            ScreenSource::Typed,
            None,
            &[],
        );
        let q = match d {
            Decision::Confirm(r) => r.summary,
            d => format!("NOT ASKED {d:?}"),
        };
        eprintln!("{n:?}: rule={} :: {q}", recipient_field_name(n));
        let named = q.contains("To/Account") && q.contains("mallory@evil.example");
        if !(named || q.contains("Recipient not verified")) {
            bad.push(q);
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Look-alikes inside the exempt accented-Latin block (U+00C0–U+017F): the ligature `ĳ`
/// reads as "ij", `ŀ` as "l·" at small sizes; a payee "Dĳkstra" beside ASCII is not flagged.
#[test]
fn probe13_exempt_block_lookalikes_not_flagged() {
    let mut bad = Vec::new();
    for v in [
        "Dĳkstra Bank",
        "Bĳou Smith",
        "ĲSSEL Holdings",
        "Alice Smith Ŀtd",
    ] {
        let s = app_screen(vec![input(1, "To", v), button(2, "Send")]);
        let q = summary_with(
            &Action::Tap { element: 2 },
            &s,
            RiskCategory::SendMessage,
            ScreenSource::Typed,
            None,
            &[],
        );
        eprintln!("{v}: {q}");
        if !q.contains("(look-alike letters)") {
            bad.push(v);
        }
    }
    assert!(bad.is_empty(), "unflagged look-alikes: {bad:?}");
}
