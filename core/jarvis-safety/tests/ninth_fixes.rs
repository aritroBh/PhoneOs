#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Ninth review: regression tests for the fixes (the review's own probes are in
//! `ninth_review.rs`; the property tests in `question_property.rs` and, through compaction,
//! in `jarvis-observe/tests/{tap_landing,presumed_submit}_property.rs`).

mod common;

use common::{app_screen, button, el, label};
use jarvis_proto::{Action, BarNode, Element, Policy, RawNode, Rect, RiskCategory, Role};
use jarvis_safety::{
    classify, evaluate_with, presumed_submit, summary_with, Decision, ScreenSource, ScreenTree,
    TypedText,
};

fn tf(index: u32, name: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, name);
    e.value = Some(value.to_owned());
    e
}

fn secure(index: u32, name: &str) -> Element {
    let mut e = el(index, Role::SecureTextField, name);
    e.secure = true;
    e
}

fn at(mut e: Element, x: f64, y: f64, w: f64, h: f64) -> Element {
    e.frame = Rect::new(x, y, w, h);
    e
}

fn tap(i: u32) -> Action {
    Action::Tap { element: i }
}

fn pay_summary(els: Vec<Element>, target: u32) -> String {
    summary_with(
        &tap(target),
        &app_screen(els),
        RiskCategory::Payment,
        ScreenSource::Typed,
        None,
        &[],
    )
}

// ---- H4: the question's reserved slots ----------------------------------------------------

/// A merchant's name after "Pay to:" is the payee wherever it is on the screen (here 600 pt
/// above the button), and it comes before every field.
#[test]
fn a_merchant_payee_anywhere_on_screen_is_listed_first() {
    let s = vec![
        at(
            label(1, "Pay to: Mallory's Merch LLC"),
            16.0,
            60.0,
            361.0,
            22.0,
        ),
        at(label(2, "Order total $1,249.00"), 16.0, 100.0, 361.0, 22.0),
        at(
            tf(3, "Contact email", "john@icloud.com"),
            16.0,
            300.0,
            361.0,
            44.0,
        ),
        at(button(4, "Place order"), 16.0, 700.0, 361.0, 50.0),
    ];
    let sum = pay_summary(s, 4);
    assert!(
        sum.contains("To/Account: Pay to: Mallory's Merch LLC"),
        "{sum}"
    );
    // An email-shaped value in a field not named like a recipient is no payee.
    assert!(!sum.contains("To/Account: Contact email"), "{sum}");
    assert!(
        sum.contains("Sends: Contact email: \"john@icloud.com\""),
        "{sum}"
    );
}

/// A heading alone ("Pay to") with the name next to it is one fact.
#[test]
fn a_recipient_heading_alone_takes_the_name_beside_it() {
    let s = vec![
        at(label(1, "Pay to"), 16.0, 60.0, 80.0, 22.0),
        at(label(2, "Northwind Traders"), 100.0, 60.0, 260.0, 22.0),
        at(label(3, "Total $20.00"), 16.0, 100.0, 361.0, 22.0),
        at(button(4, "Pay"), 16.0, 700.0, 361.0, 50.0),
    ];
    let sum = pay_summary(s, 4);
    assert!(
        sum.contains("To/Account: Pay to: Northwind Traders"),
        "{sum}"
    );
}

/// Seven contact fields (emails, phones) and a payee: the payee and the total stay, the
/// fields say how many there are, and the fields named like a recipient follow the payee.
#[test]
fn contact_fields_never_push_the_payee_or_the_total_out() {
    let mut els = vec![
        label(1, "Pay to: @mallory_merch"),
        label(2, "Order total $1,249.00"),
    ];
    for i in 0..30u32 {
        els.push(tf(
            3 + i,
            &format!("Send receipt to {i}"),
            &format!("someone.with.a.long.address.{i}@example-mail-provider.com"),
        ));
    }
    els.push(button(33, "Place order"));
    let sum = pay_summary(els, 33);
    assert!(sum.chars().count() <= 480, "{sum}");
    assert!(sum.contains("$1,249.00"), "{sum}");
    // Tenth review (H2): the form's recipient-named fields ("Send receipt to N", what the
    // tap sends) come first, the heading pair right after the first of them, and the slot
    // says the screen names several recipients. Was `To/Account: Pay to: @mallory_merch`
    // (the heading first).
    assert!(
        sum.contains("To/Account (31 different): Send receipt to 0"),
        "{sum}"
    );
    assert!(sum.contains("; Pay to: @mallory_merch"), "{sum}");
    assert!(sum.contains("Sends: 30 fields ("), "{sum}");
    assert!(sum.contains(" more"), "{sum}");
}

/// Field names cannot imitate an entry or Jarvis's wording: colons and semicolons become
/// spaces, quotes become ’, and a name with anything but letters, digits and spaces is in
/// single quotes; no second marker appears.
#[test]
fn field_names_cannot_imitate_entries() {
    let els = vec![
        label(1, "Total $5.00"),
        tf(2, "Note; Amounts on screen: $1.00", "hi"),
        tf(3, "Sends: To/Account: @evil", "x"),
        tf(4, "What's \"this\"", "y"),
        button(5, "Pay"),
    ];
    let sum = pay_summary(els, 5);
    assert_eq!(sum.matches("Amounts on screen:").count(), 1, "{sum}");
    assert_eq!(sum.matches("Sends:").count(), 1, "{sum}");
    assert_eq!(sum.matches("To/Account:").count(), 1, "{sum}");
    assert!(
        sum.contains("'Note Amounts on screen $1.00': \"hi\""),
        "{sum}"
    );
    assert!(
        sum.contains("'What\u{2019}s \u{2019}this\u{2019}': \"y\""),
        "{sum}"
    );
    // A plain name stays as it is.
    let sum = pay_summary(vec![tf(1, "Gift message", "hi"), button(2, "Pay")], 2);
    assert!(sum.contains("Sends: Gift message: \"hi\""), "{sum}");
}

/// A delete (or any action that does not send) says what the form holds, not what it sends.
#[test]
fn a_delete_says_what_the_form_holds() {
    let s = app_screen(vec![tf(1, "Note", "rent"), button(2, "Delete account")]);
    let sum = summary_with(
        &tap(2),
        &s,
        RiskCategory::Delete,
        ScreenSource::Typed,
        None,
        &[],
    );
    assert_eq!(
        sum,
        "Tap \"Delete account\" in Example. The form holds: Note: \"rent\""
    );
    let sum = summary_with(
        &tap(2),
        &s,
        RiskCategory::Payment,
        ScreenSource::Typed,
        None,
        &[],
    );
    assert_eq!(
        sum,
        "Tap \"Delete account\" in Example. Sends: Note: \"rent\""
    );
}

/// What Jarvis typed comes first in `Sends:`, then the recipient- and amount-named fields,
/// then the rest.
#[test]
fn typed_fields_come_first_then_recipient_and_amount_names() {
    let els = vec![
        tf(1, "Street", "Main St"),
        tf(2, "Amount", "$40.00"),
        tf(3, "Message", "for dinner"),
        button(4, "Send"),
    ];
    let typed = [TypedText {
        field: 3,
        text: "for dinner".to_owned(),
    }];
    let sum = summary_with(
        &tap(4),
        &app_screen(els),
        RiskCategory::SendMessage,
        ScreenSource::Typed,
        None,
        &typed,
    );
    assert!(
        sum.contains("Sends: Message: \"for dinner\"; Amount: \"$40.00\"; Street: \"Main St\""),
        "{sum}"
    );
}

/// A value of 2 MB in a listed field: bounded work, the bound kept, and the characters not
/// shown counted from the whole value (cut before redaction, never before).
#[test]
fn a_huge_listed_value_is_summarised_quickly_and_counted() {
    let big = "1 2 3 4 5 6 7 8 9 0 $ ".repeat(100_000);
    let len = big.chars().count();
    let els = vec![tf(1, "Note", &big), button(2, "Pay now")];
    let t0 = std::time::Instant::now();
    let sum = pay_summary(els, 2);
    assert!(t0.elapsed().as_secs() < 5, "{:?}", t0.elapsed());
    assert!(sum.chars().count() <= 480, "{sum}");
    let shown = sum
        .split(" more characters not shown")
        .next()
        .and_then(|s| s.rsplit('(').next())
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap();
    assert!(shown > len - 200 && shown < len, "{shown} of {len}: {sum}");
}

// ---- Narrowing (a): a login form ----------------------------------------------------------

fn login(extra: Option<Element>) -> jarvis_proto::CompactScreen {
    let mut els = vec![tf(1, "Email", "me@example.com"), secure(2, "Password")];
    els.extend(extra);
    els.push(button(4, "Sign in"));
    els.push(button(5, "Forgot password?"));
    app_screen(els)
}

/// Typing the email into a login form does not make "Sign in" a presumed send; a filled
/// field that is not an account field, or no password field, keeps the presumption; "Forgot
/// password?" is never a presumed submit.
#[test]
fn a_login_form_is_not_presumed_to_send() {
    let typed = [TypedText {
        field: 1,
        text: "me@example.com".to_owned(),
    }];
    let s = login(None);
    assert!(!presumed_submit(&tap(4), &s, None, &typed));
    assert!(!presumed_submit(&tap(5), &s, None, &typed));
    // The other direction: a comment filled in the same form keeps the presumption.
    let s = login(Some(tf(3, "Comment", "hello")));
    assert!(presumed_submit(&tap(4), &s, None, &typed));
    // No password field: the presumption holds.
    let s = app_screen(vec![
        tf(1, "Email", "me@example.com"),
        button(4, "Sign in"),
        button(5, "Forgot password?"),
    ]);
    assert!(presumed_submit(&tap(4), &s, None, &typed));
    assert!(!presumed_submit(&tap(5), &s, None, &typed));
}

/// "Done" after typing always asks (not narrowed: a Done can publish), and so does an
/// unlabelled or icon button; a picker does not.
#[test]
fn done_after_typing_still_asks_and_a_picker_does_not() {
    let typed = [TypedText {
        field: 1,
        text: "great product".to_owned(),
    }];
    for b in ["Done", "", "arrow.right.circle"] {
        let s = app_screen(vec![tf(1, "Review", "great product"), button(2, b)]);
        assert!(presumed_submit(&tap(2), &s, None, &typed), "{b:?}");
        let d = evaluate_with(
            &Policy::default(),
            &tap(2),
            &s,
            None,
            ScreenSource::Typed,
            None,
            &typed,
        );
        assert!(matches!(d, Decision::Confirm(_)), "{b:?}: {d:?}");
    }
    // A button that carries a value (a picker or disclosure row): the picker role is
    // excluded; a plain Button with a value is not (it asks; the brief's "already
    // excluded" holds only for the picker and value roles).
    let mut picker = el(2, Role::Picker, "Country");
    picker.value = Some("France".to_owned());
    let s = app_screen(vec![tf(1, "Review", "great product"), picker]);
    assert!(!presumed_submit(&tap(2), &s, None, &typed));
    let mut row = button(2, "Country");
    row.value = Some("France".to_owned());
    let s = app_screen(vec![tf(1, "Review", "great product"), row]);
    assert!(presumed_submit(&tap(2), &s, None, &typed));
}

// ---- Narrowing (b): a browser's Forward ---------------------------------------------------

/// A screen with a bottom bar (raw node 1) holding Back [1] and Forward [2], and a Forward
/// [3] outside it.
fn browser(with_back: bool) -> (jarvis_proto::CompactScreen, Vec<BarNode>, Vec<RawNode>) {
    let back = at(
        button(1, if with_back { "Back" } else { "Bookmarks" }),
        16.0,
        780.0,
        44.0,
        44.0,
    );
    let fwd = at(button(2, "Forward"), 80.0, 780.0, 44.0, 44.0);
    let other = at(button(3, "Forward"), 16.0, 300.0, 361.0, 44.0);
    let s = app_screen(vec![back, fwd, other]);
    let bar = BarNode {
        node: 1,
        role: Role::Toolbar,
        frame: Rect::new(0.0, 770.0, 393.0, 82.0),
    };
    (s, vec![bar], Vec::new())
}

const B_SOURCES: [u32; 3] = [2, 3, 4];
const B_PARENTS: [u32; 5] = [u32::MAX, 0, 1, 1, 0];
const B_ENDS: [u32; 5] = [5, 4, 3, 4, 5];

#[test]
fn forward_in_a_bar_with_back_is_navigation() {
    for with_back in [true, false] {
        let (s, bars, nodes) = browser(with_back);
        let tree = ScreenTree {
            sources: &B_SOURCES,
            parents: &B_PARENTS,
            ends: &B_ENDS,
            bars: &bars,
            nodes: &nodes,
            inputs: &[],
        };
        let cat = |i: u32| match evaluate_with(
            &Policy::default(),
            &tap(i),
            &s,
            None,
            ScreenSource::Typed,
            Some(&tree),
            &[],
        ) {
            Decision::Confirm(r) => r.category,
            Decision::Allow => RiskCategory::None,
            Decision::Deny(e) => panic!("{e}"),
        };
        if with_back {
            assert_eq!(cat(2), RiskCategory::None, "Forward beside Back");
        } else {
            assert_eq!(cat(2), RiskCategory::SendMessage, "no Back in the bar");
        }
        // Outside the bar, "Forward" is a send phrase either way.
        assert_eq!(cat(3), RiskCategory::SendMessage);
    }
}

// ---- Narrowing (c): icon names match whole components --------------------------------------

#[test]
fn arrow_up_matches_only_its_own_styles() {
    let risk = |name: &str| classify(&tap(1), &app_screen(vec![button(1, name)]), None);
    for send in [
        "arrow.up",
        "arrow.up.circle",
        "arrow.up.circle.fill",
        "arrow.up.square",
    ] {
        assert_eq!(risk(send), RiskCategory::SendMessage, "{send}");
    }
    for other in ["arrow.up.right", "arrow.up.forward", "arrow.up.arrow.down"] {
        assert_ne!(risk(other), RiskCategory::SendMessage, "{other}");
    }
    // Entries that do not end in a direction keep matching any continuation.
    assert_eq!(risk("trash.fill"), RiskCategory::Delete);
    assert_eq!(risk("phone.arrow.up.right"), RiskCategory::Call);
}
