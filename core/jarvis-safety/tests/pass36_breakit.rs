//! Review pass 36: break-it tests on the look-alike readings of commit `de7ff70`
//! (`jarvis_safety::text::lookalike_readings`).
//!
//! The claim: a label is also read with ASCII look-alikes folded (a capital `I` for an `l`
//! in mixed-case words, a lowercase `l` for an `I`, inside a word `0` o, `1` l/i, `3` e,
//! `4` a, `5` s, `7` t, `8` b, `@` a, `$` s, `|` l/i, `!` l/i), and the tap carries every
//! category any reading gives. Each `f…` test below names a label a system font draws as
//! an English risk word that still classifies as nothing, so the tap runs unasked under the
//! default policy and passes a policy that denies the category. None of them uses a pair
//! the commit states it does not fold (`rn`/`m`, `vv`/`w`, `cl`/`d`, a word needing both
//! readings of `1`, other elements' texts).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{app_screen, button};
use jarvis_proto::{Action, Element, Policy, Rect, RiskCategory};
use jarvis_safety::{classify, evaluate, Decision};

fn tap(label: &str) -> RiskCategory {
    let s = app_screen(vec![button(1, label)]);
    classify(&Action::Tap { element: 1 }, &s, None)
}

fn decide(policy: &Policy, label: &str) -> Decision {
    let s = app_screen(vec![button(1, label)]);
    evaluate(policy, &Action::Tap { element: 1 }, &s, None)
}

/// For each `(label, drawn as)` pair: the real word must be a risk word (checked, so a
/// test cannot pass vacuously), and the look-alike must not be tapped unasked under the
/// default policy, and must be refused by a policy that denies the real word's category.
fn misses(pairs: &[(&str, &str)]) -> Vec<String> {
    let mut wrong = Vec::new();
    for &(label, real) in pairs {
        let want = tap(real);
        assert_ne!(want, RiskCategory::None, "{real} is a risk word");
        let got = tap(label);
        let d = decide(&Policy::default(), label);
        if matches!(d, Decision::Allow) {
            wrong.push(format!(
                "{label:?} (drawn {real:?}, {want:?}): category {got:?}, default policy Allow"
            ));
        }
        let deny = Policy {
            deny_categories: vec![want],
            ..Policy::default()
        };
        let d = decide(&deny, label);
        if !matches!(d, Decision::Deny(_)) {
            wrong.push(format!(
                "{label:?} under a policy denying {want:?}: {} (the real word is refused)",
                short(&d)
            ));
        }
    }
    wrong
}

fn short(d: &Decision) -> String {
    match d {
        Decision::Allow => "Allow".to_owned(),
        Decision::Deny(_) => "Deny".to_owned(),
        Decision::Confirm(c) => format!("Confirm({:?})", c.category),
    }
}

/// F1 (high): a capital `I` for `l` is folded only right after a lowercase letter. Title
/// case puts a capital before the `l` in the commonest risk words ("Block", "Clear
/// History", "Close Account", "Allow"), and a double `ll` written `II` has a capital
/// before its second `I`: San Francisco draws `BIock`, `CIear`, `SeII`, `AIIow` as
/// `Block`, `Clear`, `Sell`, `Allow`. Each is tapped unasked (a sale, a block, a delete, a
/// permission, a call) and passes a policy that denies the category.
#[test]
fn f1_capital_i_after_a_capital_or_another_i_is_not_read_as_l() {
    let wrong = misses(&[
        ("BIock", "Block"),
        ("BIock Contact", "Block Contact"),
        ("CIear History", "Clear History"),
        ("CIear Data", "Clear Data"),
        ("CIear AII", "Clear All"),
        ("CIose Account", "Close Account"),
        ("SeII", "Sell"),
        ("SeII Now", "Sell Now"),
        ("CaII", "Call"),
        ("AIIow", "Allow"),
        ("AIways AIIow", "Always Allow"),
        ("UninstaII", "Uninstall"),
        ("InstaII Now", "Install Now"),
        ("UnfoIIow", "Unfollow"),
    ]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F2 (high): a word-initial capital `I` for `l` is not folded (nothing precedes it):
/// "Iog Out", "Iogout", "Ieave Group" draw as "log Out", "logout", "leave Group" and are
/// tapped unasked.
#[test]
fn f2_word_initial_capital_i_for_l_is_not_folded() {
    let wrong = misses(&[
        ("Iog Out", "Log Out"),
        ("Iogout", "Logout"),
        ("Iog Off", "Log Off"),
        ("Ieave Group", "Leave Group"),
        ("Ieave", "Leave"),
    ]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F3 (high): the lowercase-`l`-to-`I` fold rewrites a genuine `l`. A real `l` right after
/// a capital, followed by a capital or a look-alike digit, is read as `I` in the only
/// reading, so the digit trick it sits next to is lost ("Bl0ck" reads "BIock", "Cl3ar
/// Data" reads "CIear Data"); and `Il`/`lI` mixes of a real `l` and a capital `I` ("SeIl",
/// "CaIl", "AIlow") read with the `l` turned into `I`. Every label here has only a real
/// letter or a folded look-alike where the risk word has its letter.
#[test]
fn f3_the_l_to_capital_i_fold_breaks_a_genuine_l() {
    let wrong = misses(&[
        ("Bl0ck", "Block"),
        ("Cl0se Account", "Close Account"),
        ("Cl3ar Data", "Clear Data"),
        ("Cl3ar History", "Clear History"),
        ("SeIl", "Sell"),
        ("CaIl", "Call"),
        ("AIlow", "Allow"),
    ]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F4 (high): a risk word written with one lowercase-to-capital step inside it splits at
/// the step ("DeLete" is `de` + `lete`), and only a word with two steps or more is also
/// read whole. "DeLete", "ReMove", "CanCel Subscription", "deLETE", "dELETE" read as the
/// risk word to anyone and are tapped unasked.
#[test]
fn f4_a_single_case_step_inside_a_risk_word_hides_it() {
    let wrong = misses(&[
        ("DeLete", "Delete"),
        ("DeLETE", "Delete"),
        ("deLETE", "Delete"),
        ("dELETE", "Delete"),
        ("dELETE ACCOUNT", "Delete Account"),
        ("DEleTE", "Delete"),
        ("ReMove", "Remove"),
        ("CanCel Subscription", "Cancel Subscription"),
    ]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F5 (high): the whole-word reading of a word with two case steps lowercases it, which
/// also drops the camel-case split the folded reading needed: "DeIeteAccount" (capital `I`
/// for `l`, the identifier style) reads `deleteaccount`, no phrase, where "DeleteAccount"
/// and "DeIete Account" are both `Delete`.
#[test]
fn f5_the_whole_word_reading_loses_the_camel_split() {
    assert_eq!(tap("DeleteAccount"), RiskCategory::Delete);
    assert_eq!(tap("DeIete Account"), RiskCategory::Delete);
    let wrong = misses(&[("DeIeteAccount", "DeleteAccount")]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// F6 (high): the look-alike digits are ASCII only. A fullwidth zero (U+FF10) or a
/// mathematical digit zero (U+1D7CE, U+1D7EC) is folded to an ASCII `0` by the tokenizer
/// and then split from the letters as a digit, but is never read as the letter `o`:
/// "Rem０ve", "Rem𝟎ve", "D𝟎nate", "Sign ０ut" are tapped unasked where "Rem0ve" asks.
#[test]
fn f6_non_ascii_digits_are_not_read_as_letters() {
    assert_eq!(
        tap("Rem0ve"),
        RiskCategory::Delete,
        "the ASCII form is caught"
    );
    let wrong = misses(&[
        ("Rem\u{FF10}ve", "Remove"),
        ("Rem\u{1D7CE}ve", "Remove"),
        ("D\u{1D7EC}nate", "Donate"),
        ("Sign \u{FF10}ut", "Sign Out"),
        ("De\u{FF11}ete", "Delete"),
    ]);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Sound (new coverage): the folds combine with the earlier defences — a zero-width space
/// beside a folded digit, Cyrillic letters around a digit, a combining mark on a digit, the
/// two-way characters, digits glued to a risk word, a non-breaking hyphen, underscores.
#[test]
fn sound_folds_combine_with_the_earlier_defences() {
    for (label, want) in [
        ("De\u{200B}1ete", RiskCategory::Delete),
        ("R\u{0435}m0v\u{0435}", RiskCategory::Delete),
        ("Rem0\u{0301}ve", RiskCategory::Delete),
        ("De|ete", RiskCategory::Delete),
        ("De!ete", RiskCategory::Delete),
        ("1og out", RiskCategory::AccountChange),
        ("|og out", RiskCategory::AccountChange),
        ("Pay2day", RiskCategory::Payment),
        ("2Pay", RiskCategory::Payment),
        ("Buy1Get1", RiskCategory::Purchase),
        ("DELETE_ACC0UNT", RiskCategory::Delete),
        ("Delete\u{2011}Account", RiskCategory::Delete),
        ("D3LETE", RiskCategory::Delete),
        ("$end M0ney", RiskCategory::Payment),
        ("Tr@nsfer", RiskCategory::Payment),
        ("Sh0w Passw0rd", RiskCategory::Credential),
        ("Insta11 Now", RiskCategory::SystemSetting),
        ("B1ock", RiskCategory::AccountChange),
    ] {
        assert_eq!(tap(label), want, "{label:?}");
    }
}

/// F8 (medium, cost): a `tap_point` over 3,000 stacked buttons whose 1,024-character
/// labels are all look-alike candidates with two-way characters (`a1a1…`, two readings
/// each) against the same page with as many tokens and no candidate (`a-a-…`). Every
/// pressable control under a point is classified (review pass 31), each once per reading.
/// Measured (release build, this machine): 8.7 s without readings, 45.5 s with them
/// (debug: 13.6 s for `abab…`, 213 s for `a1a1…`). The gate decides before anything is
/// sent, so a hostile page stalls the session that long per look; the readings should
/// cost no more than twice the words they are read beside.
#[test]
fn f8_lookalike_readings_multiply_the_cost_of_a_tap_point() {
    fn page(label: &str) -> jarvis_proto::CompactScreen {
        let elements: Vec<Element> = (1..=3000u32)
            .map(|i| {
                let mut e = button(i, label);
                e.frame = Rect::new(16.0, 300.0, 361.0, 44.0);
                e
            })
            .collect();
        app_screen(elements)
    }
    // The same number of tokens without a look-alike (`a-a-…`), and with one (`a1a1…`).
    let plain: String = "a-".repeat(512);
    let foldable: String = "a1".repeat(512);
    let at = Action::TapPoint {
        x: 196.0 / 393.0,
        y: 322.0 / 852.0,
    };
    let mut times = Vec::new();
    for label in [&plain, &foldable] {
        let s = page(label);
        let t = std::time::Instant::now();
        let d = evaluate(&Policy::default(), &at, &s, None);
        let dt = t.elapsed();
        times.push(dt.as_secs_f64());
        println!(
            "tap_point over 3000 controls, label {:?}…: {dt:?} ({})",
            &label[..8],
            short(&d)
        );
    }
    // The readings may cost, but not more than the words they are read beside.
    assert!(
        times[1] <= 2.0 * times[0] + 0.05,
        "plain {:.3} s, with readings {:.3} s",
        times[0],
        times[1]
    );
}
