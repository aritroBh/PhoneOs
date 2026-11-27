#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Thirteenth review fixes in the gate: M1 (money by value; presumed submits phone only),
//! M2 (strong and weak recipient heads), M3 (`open_url` asks unless the owner gave the
//! link), L2 (look-alikes inside U+00C0–U+017F), L3 (a bare number after a one-word head).
mod common;

use common::{app_screen, button, el};
use jarvis_proto::{Action, Element, Policy, RiskCategory, Role};
use jarvis_safety::{
    evaluate_owned, evaluate_with, owner_gave_url, recipient_field_name,
    strong_recipient_field_name, summary_with, Decision, ScreenSource, TypedText,
    RECIPIENT_FIELD_HEADS, RECIPIENT_FIELD_STRONG_HEADS, RECIPIENT_FIELD_WEAK_HEADS,
};

fn input(index: u32, label: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, label);
    e.value = Some(value.to_owned());
    e
}

fn state() -> [TypedText; 1] {
    [TypedText {
        field: 0,
        text: String::new(),
    }]
}

fn gate(s: &jarvis_proto::CompactScreen, a: &Action, typed: &[TypedText]) -> Decision {
    evaluate_with(
        &Policy::default(),
        a,
        s,
        None,
        ScreenSource::Typed,
        None,
        typed,
    )
}

// ---------------------------------------------------------------------------------------
// L2: the exempt block, exhaustively.
// ---------------------------------------------------------------------------------------

/// The base ASCII letter of each code point of U+00C0–U+017F that is an ASCII letter plus
/// diacritics (its canonical decomposition), `-` for the rest — written out from the
/// Unicode data, independent of the crate's tables.
const BASES: [&str; 12] = [
    "AAAAAA-CEEEEIIII", // U+00C0
    "-NOOOOO--UUUUY--", // U+00D0
    "aaaaaa-ceeeeiiii", // U+00E0
    "-nooooo--uuuuy-y", // U+00F0
    "AaAaAaCcCcCcCcDd", // U+0100
    "--EeEeEeEeEeGgGg", // U+0110
    "GgGgHh--IiIiIiIi", // U+0120
    "I---JjKk-LlLlLl-", // U+0130
    "---NnNnNn---OoOo", // U+0140
    "Oo--RrRrRrSsSsSs", // U+0150
    "SsTtTt--UuUuUuUu", // U+0160
    "UuUuWwYyYZzZzZz-", // U+0170
];

/// Stand-alone letters real names use (exempt although not decomposable).
const NAME_LETTERS: &str = "ßæÆøØđĐłŁþÞðÐœŒŋŊħĦŧŦ";

fn flagged(value: &str) -> bool {
    let s = app_screen(vec![input(1, "To", value), button(2, "Send")]);
    summary_with(
        &Action::Tap { element: 2 },
        &s,
        RiskCategory::SendMessage,
        ScreenSource::Typed,
        None,
        &[],
    )
    .contains("(look-alike letters)")
}

#[test]
fn the_accented_latin_block_is_partitioned_exhaustively() {
    let mut wrong = Vec::new();
    for (row, bases) in BASES.iter().enumerate() {
        for (col, base) in bases.chars().enumerate() {
            let cp = 0xC0 + 16 * row as u32 + col as u32;
            let c = char::from_u32(cp).unwrap();
            if !c.is_alphabetic() {
                // × and ÷ are signs, not letters: never part of a word's letters.
                continue;
            }
            let exempt = base != '-' || NAME_LETTERS.contains(c);
            // Inside a word beside ASCII letters ("Dĳkstra"-like).
            let value = format!("Mar{c}a Smith");
            if flagged(&value) == exempt {
                wrong.push(format!("U+{cp:04X} {c}: exempt {exempt}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    for v in [
        "Dĳkstra Bank",
        "Alice Smith Ŀtd",
        "ſmith",
        "Kıra Jones",
        "Mĸ Ltd",
    ] {
        assert!(flagged(v), "{v}");
    }
    for v in [
        "José Smith",
        "Łukasz Nowak",
        "Søren Kierkegaard",
        "Þórr",
        "Œuvre Ltd",
    ] {
        assert!(!flagged(v), "{v}");
    }
}

// ---------------------------------------------------------------------------------------
// M2 and L3(a): the recipient heads.
// ---------------------------------------------------------------------------------------

#[test]
fn strong_and_weak_heads_partition_the_heads() {
    let mut all: Vec<&str> = RECIPIENT_FIELD_STRONG_HEADS
        .iter()
        .chain(RECIPIENT_FIELD_WEAK_HEADS)
        .copied()
        .collect();
    all.sort_unstable();
    let mut heads = RECIPIENT_FIELD_HEADS.to_vec();
    heads.sort_unstable();
    assert_eq!(all, heads);
    for h in RECIPIENT_FIELD_WEAK_HEADS {
        assert!(recipient_field_name(h), "{h}");
        assert!(!strong_recipient_field_name(h), "{h}");
    }
    for h in RECIPIENT_FIELD_STRONG_HEADS {
        assert!(strong_recipient_field_name(h), "{h}");
    }
}

#[test]
fn a_bare_number_qualifies_only_longer_heads() {
    for n in ["To 2", "To 10", "Recipient 2", "Payee 3"] {
        assert!(!recipient_field_name(n), "{n}");
    }
    for n in [
        "Send to 2",
        "Recipient email 2",
        "Pay to 1",
        "To account 4",
        "To phone",
    ] {
        assert!(recipient_field_name(n), "{n}");
    }
}

/// The payer's own "Account" (a weak head) is shown with its name, but does not stand for
/// a verified recipient; a strong head does, and its question is unchanged.
#[test]
fn a_weak_head_alone_still_says_recipient_not_verified() {
    let s = app_screen(vec![
        input(1, "Account", "Everyday Checking 4521"),
        input(2, "Who are you paying?", "mallory@evil.example"),
        input(3, "Amount", "$900.00"),
        button(4, "Send money"),
    ]);
    let Decision::Confirm(q) = gate(&s, &Action::Tap { element: 4 }, &[]) else {
        panic!()
    };
    assert!(
        q.summary
            .contains("To/Account: Account: \"Everyday Checking 4521\". Recipient not verified"),
        "{}",
        q.summary
    );
    let s = app_screen(vec![
        input(1, "To", "alice@example.com"),
        input(2, "Account", "Everyday Checking 4521"),
        button(3, "Send money"),
    ]);
    let Decision::Confirm(q) = gate(&s, &Action::Tap { element: 3 }, &[]) else {
        panic!()
    };
    assert!(
        !q.summary.contains("Recipient not verified"),
        "{}",
        q.summary
    );
    assert!(q.summary.contains("To/Account"), "{}", q.summary);
}

// ---------------------------------------------------------------------------------------
// M1: money by value; presumed submits are phone only.
// ---------------------------------------------------------------------------------------

#[test]
fn a_presumed_submit_of_a_money_form_is_payment_and_lists_the_amount() {
    for (to, amount_name, amount) in [
        ("Recipient", "How much?", "900"),
        ("To", "Memo", "$900"),
        ("Who is it for", "€", "900"),
        ("Send to", "Value", "900.00"),
    ] {
        let s = app_screen(vec![
            input(1, to, "mallory@evil.example"),
            input(2, amount_name, amount),
            button(3, "Continue"),
        ]);
        let Decision::Confirm(q) = gate(&s, &Action::Tap { element: 3 }, &state()) else {
            panic!("{to}/{amount_name}")
        };
        assert_eq!(q.category, RiskCategory::Payment, "{}", q.summary);
        assert!(q.summary.contains("Amounts on screen:"), "{}", q.summary);
        assert!(q.summary.contains(amount), "{}", q.summary);
    }
    // Who plus a phone-like run is not money: a presumed send, phone only.
    let s = app_screen(vec![
        input(1, "To", "Alice"),
        input(2, "Phone", "5551234567"),
        button(3, "Continue"),
    ]);
    let Decision::Confirm(q) = gate(&s, &Action::Tap { element: 3 }, &state()) else {
        panic!()
    };
    assert_eq!(q.category, RiskCategory::SendMessage, "{}", q.summary);
    assert!(q.phone_only);
}

#[test]
fn presumed_submits_are_phone_only_and_own_sends_are_not() {
    let s = app_screen(vec![
        input(1, "Your note", "hello"),
        button(2, "Continue"),
        button(3, "Send"),
    ]);
    let Decision::Confirm(q) = gate(&s, &Action::Tap { element: 2 }, &state()) else {
        panic!()
    };
    assert_eq!(q.category, RiskCategory::SendMessage);
    assert!(
        q.phone_only,
        "a presumed submit is approved on the phone only"
    );
    let Decision::Confirm(q) = gate(&s, &Action::Tap { element: 3 }, &state()) else {
        panic!()
    };
    assert!(
        !q.phone_only,
        "a Send button reads as a send by its own label"
    );
}

// ---------------------------------------------------------------------------------------
// M3: open_url.
// ---------------------------------------------------------------------------------------

#[test]
fn owner_given_links_are_whole_links_in_the_owners_text() {
    let task = ["Open https://bank.example/pay?to=me. Then https://a.com and (https://b.org/x)"];
    for (url, given) in [
        ("https://bank.example/pay?to=me", true),
        (" https://bank.example/pay?to=me ", true),
        ("https://bank.example/pay", false),
        ("https://a.co", false),
        ("https://a.com", true),
        ("https://b.org/x", true),
        ("https://b.org", false),
        ("https://collector.example/c?v=482913", false),
        ("", false),
    ] {
        assert_eq!(owner_gave_url(url, &task), given, "{url:?}");
    }
}

fn open(url: &str, owner: &[&str]) -> Decision {
    let s = app_screen(vec![el(1, Role::StaticText, "Your code is 482913")]);
    evaluate_owned(
        &Policy::default(),
        &Action::OpenUrl {
            url: url.to_owned(),
        },
        &s,
        None,
        ScreenSource::Typed,
        None,
        &[],
        owner,
    )
}

#[test]
fn open_url_asks_unless_the_owner_gave_it() {
    assert_eq!(
        open(
            "https://example.com/news",
            &["read https://example.com/news"]
        ),
        Decision::Allow
    );
    let Decision::Confirm(q) = open("https://collector.example/c?v=482913", &["check my code"])
    else {
        panic!()
    };
    assert_eq!(q.category, RiskCategory::SendMessage);
    assert!(q.phone_only);
    assert!(
        q.summary.starts_with(
            "Open a link that was not in your request: https://collector.example, path and query \""
        ),
        "{}",
        q.summary
    );
    // A long host: its first 12 and last 24 characters; a punycode or non-ASCII host is
    // marked.
    let long = "https://login.mybank.example.verify-account-security-check.evil.example/";
    let Decision::Confirm(q) = open(long, &[]) else {
        panic!()
    };
    assert!(
        q.summary
            .contains("https://login.mybank…urity-check.evil.example"),
        "{}",
        q.summary
    );
    for host in ["https://xn--pple-43d.com/", "https://аpple.com/"] {
        let Decision::Confirm(q) = open(host, &[]) else {
            panic!()
        };
        assert!(q.summary.contains("(look-alike letters)"), "{}", q.summary);
    }
    // A user part does not hide the host (validation refuses such a link outright).
    match open("https://bank.example@evil.example/x", &[]) {
        Decision::Confirm(q) => {
            assert!(q.summary.contains("https://evil.example"), "{}", q.summary)
        }
        Decision::Deny(_) => {}
        Decision::Allow => panic!("a link with a user part ran unasked"),
    }
    // sms: and mailto: show who and what.
    let Decision::Confirm(q) = open("sms:+15551234567&body=Code%20482913", &[]) else {
        panic!()
    };
    assert!(
        q.summary.contains("to \"+15551234567\". Sends: body: \""),
        "{}",
        q.summary
    );
    let Decision::Confirm(q) = open("mailto:x@evil.example?subject=hi&body=yo", &[]) else {
        panic!()
    };
    assert!(q.summary.contains("to \"x@evil.example\""), "{}", q.summary);
    assert!(
        q.summary.contains("Sends: subject: \"hi\"; body: \"yo\""),
        "{}",
        q.summary
    );
    // A call link stays a call (asked), phone only when the owner did not give it.
    let Decision::Confirm(q) = open("tel:+15551234567", &[]) else {
        panic!()
    };
    assert_eq!(q.category, RiskCategory::Call);
    assert!(q.phone_only);
    let Decision::Confirm(q) = open("tel:+15551234567", &["call tel:+15551234567"]) else {
        panic!()
    };
    assert!(!q.phone_only);
}
