//! Review pass 35: Unicode and ASCII look-alike tricks on English risk words.
//!
//! The tokenizer folds Cyrillic, Greek, fullwidth, styled and accented look-alikes, drops
//! invisible characters and combining marks, and sends other scripts to `Unverified`. It
//! does not fold the ASCII look-alikes every system font draws almost alike: a capital `I`
//! for a lowercase `l` (San Francisco draws them the same), the digit `0` for the letter
//! `O`, the digit `1` for `l`/`i`. A word written with them stays pure ASCII, so it is
//! neither foreign nor unreadable, and it splits into harmless tokens (`DeIete` is `de` +
//! `iete`: the camel-case split; `Rem0ve` is `rem` + `0` + `ve`: the letter/digit split).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{app_screen, button};
use jarvis_proto::{Action, Policy, RiskCategory};
use jarvis_safety::{classify, evaluate, Decision, NEVER_REMOTE_APPROVE};

fn tap(label: &str) -> RiskCategory {
    let s = app_screen(vec![button(1, label)]);
    classify(&Action::Tap { element: 1 }, &s, None)
}

fn decide(policy: &Policy, label: &str) -> Decision {
    let s = app_screen(vec![button(1, label)]);
    evaluate(policy, &Action::Tap { element: 1 }, &s, None)
}

/// F1 (high): a button drawn "Delete" but written with a capital `I` for the `l`
/// (`DeIete`) is tapped without a question under the default policy, and a policy that
/// denies deletes lets it through. Same for "DeIete Account" and "CanceI Subscription".
#[test]
fn f1_capital_i_for_l_hides_a_delete() {
    let default = Policy::default();
    let deny = Policy {
        deny_categories: vec![RiskCategory::Delete, RiskCategory::AccountChange],
        ..Policy::default()
    };
    let mut wrong = Vec::new();
    for (label, real) in [
        ("DeIete", "Delete"),
        ("DeIete Account", "Delete Account"),
        ("CanceI Subscription", "Cancel Subscription"),
    ] {
        assert_ne!(tap(real), RiskCategory::None, "{real} is a risk word");
        let got = tap(label);
        let d = decide(&default, label);
        if matches!(d, Decision::Allow) {
            wrong.push(format!(
                "{label:?} (drawn {real:?}): category {got:?}, default policy {d:?}"
            ));
        }
        let d = decide(&deny, label);
        if !matches!(d, Decision::Deny(_)) {
            wrong.push(format!(
                "{label:?} under deny Delete/AccountChange: {d:?} (the real word is refused)"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F2 (high): the digit `0` for `O` or `1` for `l`/`i` in a risk word makes the control
/// harmless: "Sign 0ut", "Log 0ut", "Rem0ve", "D0nate", "Subm1t" are tapped without a
/// question under the default policy.
#[test]
fn f2_digit_for_letter_hides_a_risk_word() {
    let p = Policy::default();
    let mut wrong = Vec::new();
    for (label, real) in [
        ("Sign 0ut", "Sign Out"),
        ("Log 0ut", "Log Out"),
        ("Rem0ve", "Remove"),
        ("Rem0ve Device", "Remove Device"),
        ("D0nate", "Donate"),
        ("Subm1t", "Submit"),
    ] {
        assert_ne!(tap(real), RiskCategory::None, "{real} is a risk word");
        let d = decide(&p, label);
        if matches!(d, Decision::Allow) {
            wrong.push(format!(
                "{label:?} (drawn {real:?}): category {:?}, decision Allow",
                tap(label)
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F3 (medium): "Send M0ney" (digit zero) is read as a message send, not a payment, so it
/// leaves the categories a paired device may never approve: a remote device holding the
/// approve permission can approve a money transfer.
#[test]
fn f3_send_m0ney_is_remotely_approvable() {
    assert_eq!(tap("Send Money"), RiskCategory::Payment);
    let got = tap("Send M0ney");
    let p = Policy::default();
    let d = decide(&p, "Send M0ney");
    let remote_ok = match &d {
        Decision::Confirm(c) => !c.phone_only && !NEVER_REMOTE_APPROVE.contains(&c.category),
        _ => true,
    };
    assert!(
        !remote_ok,
        "\"Send M0ney\" (drawn \"Send Money\"): category {got:?}, decision {d:?}, which a \
         remote device may approve"
    );
}

/// Sound (new coverage): the non-ASCII tricks of target A3 on the same words are caught:
/// Turkish dotted capital I asks as Unverified; Cyrillic, Greek, fullwidth, styled and
/// zero-width forms keep their category; combining marks are dropped.
#[test]
fn sound_unicode_tricks_on_risk_words_are_caught() {
    for (label, want) in [
        ("SUBMİT", RiskCategory::Unverified),
        ("SİGN OUT", RiskCategory::Unverified),
        ("Dele\u{0301}te", RiskCategory::Delete),
        ("Deletе", RiskCategory::Delete),
        ("DELETE AССOUNT", RiskCategory::Delete),
        ("Ｄｅｌｅｔｅ", RiskCategory::Delete),
        ("𝖯𝖺𝗒", RiskCategory::Payment),
        ("Pαy", RiskCategory::Payment),
        ("Seпd", RiskCategory::SendMessage),
        ("Se\u{3164}nd", RiskCategory::SendMessage),
        ("Send\u{2063}", RiskCategory::SendMessage),
        ("ſend", RiskCategory::Unverified),
        ("Conﬁrm", RiskCategory::Unverified),
        ("Tranѕfer", RiskCategory::Payment),
    ] {
        assert_eq!(tap(label), want, "{label:?}");
    }
}
