//! Review pass 35, fixes: risk words written with ASCII look-alikes keep their category.
//! A control is classified by its own words and by every look-alike reading of them, and
//! carries every category any reading gives (a denial of any refuses).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{app_screen, button, el, field};
use jarvis_proto::{Action, Policy, RiskCategory, Role};
use jarvis_safety::{classify, evaluate, Decision};

fn tap(label: &str) -> RiskCategory {
    let s = app_screen(vec![button(1, label)]);
    classify(&Action::Tap { element: 1 }, &s, None)
}

fn decide(policy: &Policy, label: &str) -> Decision {
    let s = app_screen(vec![button(1, label)]);
    evaluate(policy, &Action::Tap { element: 1 }, &s, None)
}

/// Every look-alike form reads as its word, whatever the case around it.
#[test]
fn lookalike_forms_keep_the_category_of_the_word() {
    let mut wrong = Vec::new();
    for (label, real) in [
        // Capital I for l, lowercase l for I.
        ("DeIete", "Delete"),
        ("CanceI Subscription", "Cancel Subscription"),
        ("SUBMlT", "SUBMIT"),
        ("SlGN OUT", "SIGN OUT"),
        // Digits.
        ("Sign 0ut", "Sign Out"),
        ("SIGN 0UT", "SIGN OUT"),
        ("Rem0ve Device", "Remove Device"),
        ("D0NATE", "DONATE"),
        ("Subm1t", "Submit"),
        ("SUBM1T", "SUBMIT"),
        ("De1ete", "Delete"),
        ("D3lete", "Delete"),
        ("P4y", "Pay"),
        ("5end", "Send"),
        ("Pos7", "Post"),
        ("8uy Now", "Buy Now"),
        ("Transf3r", "Transfer"),
        // Symbols.
        ("P@y", "Pay"),
        ("$end", "Send"),
        ("Cance| Subscription", "Cancel Subscription"),
        ("De|ete", "Delete"),
        ("De!ete", "Delete"),
        // Several at once, and alternating case.
        ("D3L3TE ACC0UNT", "DELETE ACCOUNT"),
        ("DeLeTe", "Delete"),
        ("ReMoVe", "Remove"),
        // Invisible characters do not separate a look-alike from its word.
        ("Rem\u{200B}0ve", "Remove"),
    ] {
        let want = tap(real);
        assert_ne!(want, RiskCategory::None, "{real} is a risk word");
        let got = tap(label);
        if got != want {
            wrong.push(format!("{label:?}: {got:?}, the word {real:?} is {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every reading's category is carried: "Send M0ney" asks as a payment, and an owner who
/// denies message sends is refused it too (its own words read "send").
#[test]
fn every_reading_is_carried() {
    assert_eq!(tap("Send M0ney"), RiskCategory::Payment);
    let deny_send = Policy {
        deny_categories: vec![RiskCategory::SendMessage],
        ..Policy::default()
    };
    assert!(
        matches!(decide(&deny_send, "Send M0ney"), Decision::Deny(_)),
        "{:?}",
        decide(&deny_send, "Send M0ney")
    );
    let deny_delete = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    for label in ["DeIete", "Rem0ve", "D3LETE", "DeLeTe"] {
        assert!(
            matches!(decide(&deny_delete, label), Decision::Deny(_)),
            "{label}: {:?}",
            decide(&deny_delete, label)
        );
    }
    // A long press reads the same.
    let s = app_screen(vec![button(1, "DeIete")]);
    let lp = Action::LongPress {
        element: 1,
        duration_ms: 800,
    };
    assert!(matches!(
        evaluate(&deny_delete, &lp, &s, None),
        Decision::Deny(_)
    ));
}

/// A value and a field's own name are read the same way: a switch whose value reads
/// "Rem0ve", a password field named "Passw0rd".
#[test]
fn values_and_field_names_are_read_too() {
    let mut cell = el(1, Role::Cell, "Device");
    cell.value = Some("Rem0ve".to_owned());
    let s = app_screen(vec![cell]);
    assert_eq!(
        classify(&Action::Tap { element: 1 }, &s, None),
        classify(
            &Action::Tap { element: 1 },
            &app_screen(vec![{
                let mut c = el(1, Role::Cell, "Device");
                c.value = Some("Remove".to_owned());
                c
            }]),
            None
        )
    );
    let typed = |name: &str| {
        let s = app_screen(vec![field(1, Role::TextField, name)]);
        classify(
            &Action::Type {
                element: Some(1),
                text: "hunter2hunter2".to_owned(),
                submit: false,
            },
            &s,
            None,
        )
    };
    assert_eq!(typed("Password"), RiskCategory::Credential);
    assert_eq!(typed("Passw0rd"), RiskCategory::Credential);
}

/// Numbers, versions and prices are not words: nothing about them changes.
#[test]
fn numbers_stay_numbers() {
    for (label, want) in [
        ("Top 10", RiskCategory::None),
        ("iOS 26", RiskCategory::None),
        ("5G", RiskCategory::None),
        ("Go!", RiskCategory::None),
        ("Pay42", tap("Pay 42")),
    ] {
        assert_eq!(tap(label), want, "{label}");
    }
}

/// Review pass 35 (add-on): the Cf characters the invisible list missed (prepended
/// concatenation marks, Egyptian format controls) hide no risk word.
#[test]
fn formerly_missed_format_characters_hide_no_risk_word() {
    let missed = ('\u{0600}'..='\u{0605}')
        .chain(['\u{06DD}', '\u{070F}', '\u{0890}', '\u{0891}', '\u{08E2}'])
        .chain(['\u{110BD}', '\u{110CD}'])
        .chain('\u{13430}'..='\u{1343F}');
    let policy = Policy {
        deny_categories: vec![RiskCategory::Delete],
        ..Policy::default()
    };
    for c in missed {
        for (label, real) in [
            (format!("De{c}lete"), "Delete"),
            (format!("Se{c}nd"), "Send"),
            (format!("P{c}ay"), "Pay"),
        ] {
            assert_eq!(tap(&label), tap(real), "U+{:04X} in {real}", u32::from(c));
        }
        assert!(
            matches!(decide(&policy, &format!("De{c}lete")), Decision::Deny(_)),
            "U+{:04X}",
            u32::from(c)
        );
    }
}
