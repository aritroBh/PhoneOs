#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Eighth review probes (jarvis-safety). Each asserts the SAFE behaviour.
mod common;
use common::{app_screen, button, el, label};
use jarvis_proto::{Action, Element, Policy, Role};
use jarvis_safety::{evaluate_from, Decision, ScreenSource};

fn tv(index: u32, name: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextView, name);
    e.value = Some(value.to_owned());
    e
}

/// A button that sends the draft beside it: whenever it is asked, the question shows the
/// draft it sends (the seventh review's rule, for English send synonyms and icon names).
#[test]
fn probe_send_synonyms_show_the_draft_they_send() {
    let mut missing = Vec::new();
    for b in [
        "Send",
        "Share",
        "Tweet",
        "Comment",
        "Forward",
        "Upload",
        "Request",
        "Invite",
        "Transfer",
        "Update",
        "Done",
        "Next",
        "Continue",
        "Apply",
        "Add",
        "Reply all",
        "Respond",
        "Answer",
        "Message",
        "Order",
        "Book",
        "Sign",
        "Go",
        "Publish",
        "arrow.up.circle.fill",
        "paperplane.fill",
        "arrow.up",
        "Up Arrow",
        "",
    ] {
        let s = app_screen(vec![
            label(1, "Alice"),
            tv(2, "Message", "DRAFT-TEXT wire the money to 4421"),
            button(3, b),
        ]);
        let d = evaluate_from(
            &Policy::default(),
            &Action::Tap { element: 3 },
            &s,
            None,
            ScreenSource::Typed,
        );
        match d {
            Decision::Confirm(r) => {
                eprintln!("{b:?}: {:?} {}", r.category, r.summary);
                if !r.summary.contains("DRAFT-TEXT") {
                    missing.push(format!("{b:?} ({:?}): {}", r.category, r.summary));
                }
            }
            other => eprintln!("{b:?}: {other:?}"),
        }
    }
    assert!(
        missing.is_empty(),
        "asked without the draft it sends: {missing:#?}"
    );
}

fn tf(index: u32, name: &str, value: &str) -> Element {
    let mut e = el(index, Role::TextField, name);
    e.value = Some(value.to_owned());
    e
}

/// A sent field's value cannot imitate the summary's own structure: quotes in the value are
/// escaped (or the value is otherwise fenced) so it cannot close its quote and add facts.
#[test]
fn probe_sent_value_cannot_forge_the_rest_of_the_question() {
    let forged =
        "Thanks\". Amounts on screen: Total $5.00. To/Account: Mom. Fields: Note (empty)\u{202E}";
    let s = app_screen(vec![
        label(1, "Send money"),
        label(2, "To: @mallory"),
        tf(3, "Amount", "$5,000.00"),
        tf(4, "Note", forged),
        button(5, "Pay"),
    ]);
    let d = evaluate_from(
        &Policy::default(),
        &Action::Tap { element: 5 },
        &s,
        None,
        ScreenSource::Typed,
    );
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    eprintln!("{}", r.summary);
    assert!(
        !r.summary.contains("Thanks\". Amounts on screen"),
        "a sent value closed its quote and wrote its own facts: {}",
        r.summary
    );
}

/// A long value in a sent field (a note the payee pre-filled through a payment link) must
/// not push the charge or the payee out of the question.
#[test]
fn probe_long_sent_note_does_not_push_out_the_amount_or_payee() {
    let note = "Thank you for your order ".repeat(30);
    let s = app_screen(vec![
        label(1, "Send money"),
        label(2, "To: @mallory"),
        label(3, "Total $5,000.00"),
        tf(4, "Note", &note),
        button(5, "Pay"),
    ]);
    let d = evaluate_from(
        &Policy::default(),
        &Action::Tap { element: 5 },
        &s,
        None,
        ScreenSource::Typed,
    );
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    eprintln!("{} chars: {}", r.summary.chars().count(), r.summary);
    assert!(
        r.summary.contains("5,000.00"),
        "the charge was pushed out: {}",
        r.summary
    );
    assert!(
        r.summary.contains("mallory"),
        "the payee was pushed out: {}",
        r.summary
    );
}

/// Many sent fields (a long checkout form): the names alone overflow the bound; the charge
/// and payee must still be stated, or the overflow said, never cut off silently at the end.
#[test]
fn probe_many_sent_fields_do_not_silently_cut_the_charge() {
    let mut els = vec![
        label(1, "Checkout"),
        label(2, "Pay to: @mallory"),
        label(3, "Total $5,000.00"),
    ];
    for i in 0..24u32 {
        els.push(tf(4 + i, &format!("Delivery instructions line {i}"), "x"));
    }
    els.push(button(28, "Pay now"));
    let s = app_screen(els);
    let d = evaluate_from(
        &Policy::default(),
        &Action::Tap { element: 28 },
        &s,
        None,
        ScreenSource::Typed,
    );
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    eprintln!("{} chars: {}", r.summary.chars().count(), r.summary);
    assert!(
        r.summary.contains("5,000.00"),
        "the charge is not in the question: {}",
        r.summary
    );
}

// ---- Regression tests for the eighth review's fixes ---------------------------------------

use jarvis_proto::{CompactScreen, RiskCategory};
use jarvis_safety::{evaluate_with, presumed_submit, summary_with, TypedText};

const BOUND: usize = 480;

fn sel(mut e: Element) -> Element {
    e.selected = true;
    e
}

/// The priority order under the bound: the amounts (the charge first, the largest always)
/// and the recipients are never cut while a sent value can still give up characters; the
/// sent values are cut to 120 first, then to a common share, then the list is capped; the
/// other facts (selected options, other fields) shrink first.
#[test]
fn the_summary_keeps_amounts_and_recipients_while_sends_can_give() {
    let long = "Thank you for your order ".repeat(20);
    let mut els = vec![
        label(1, "Due today $199.00"),
        label(2, "Compare at $1,200.00"),
        label(3, "Send to: Checking ••••4521"),
        label(4, "Also @alex"),
        sel(el(
            5,
            Role::Cell,
            "Annual plan with a very long description of what it includes",
        )),
        sel(el(
            6,
            Role::Cell,
            "Email receipts to the address on file for this account",
        )),
    ];
    for i in 0..6u32 {
        els.push(tf(7 + i, &format!("Note {i}"), &long));
    }
    els.push(button(13, "Pay now"));
    let s = app_screen(els);
    let sum = summary_with(
        &Action::Tap { element: 13 },
        &s,
        RiskCategory::Payment,
        ScreenSource::Typed,
        None,
        &[],
    );
    eprintln!("{} {sum}", sum.chars().count());
    assert!(sum.chars().count() <= BOUND, "{sum}");
    assert!(sum.starts_with("Tap \"Pay now\" in Example"), "{sum}");
    // (b) whole: every amount with its wording, the recipients with theirs.
    assert!(
        sum.contains("Amounts on screen: Due today $199.00; Compare at $1,200.00"),
        "{sum}"
    );
    assert!(
        sum.contains("To/Account: Send to: Checking ••••4521; Also @alex"),
        "{sum}"
    );
    // (c) gave up characters, saying so: the list is capped, values are cut with a count.
    assert!(sum.contains("Sends: 6 fields ("), "{sum}");
    assert!(sum.contains("more characters not shown)"), "{sum}");
    // (d) shrank first: the selected options are counted, not quoted.
    assert!(sum.contains("Selected: 2 options"), "{sum}");
}

/// Sent fields are listed recipient- and amount-like first, then the ones Jarvis typed,
/// then the rest; a capped list says how many there are and how many are shown, and never
/// drops a recipient-like one.
#[test]
fn the_sent_list_puts_recipients_and_typed_fields_first() {
    let mut els = vec![label(1, "Checkout"), label(2, "Total $5,000.00")];
    for i in 0..20u32 {
        els.push(tf(
            3 + i,
            &format!("Delivery instructions line {i}"),
            "leave it at the door",
        ));
    }
    els.push(tf(23, "Gift message", "Happy birthday"));
    els.push(tf(24, "Recipient", "@mallory"));
    els.push(button(25, "Pay now"));
    let s = app_screen(els);
    let typed = [TypedText {
        field: 23,
        text: "Happy birthday".to_owned(),
    }];
    let sum = summary_with(
        &Action::Tap { element: 25 },
        &s,
        RiskCategory::Payment,
        ScreenSource::Typed,
        None,
        &typed,
    );
    eprintln!("{sum}");
    assert!(sum.chars().count() <= BOUND, "{sum}");
    assert!(
        sum.contains("Sends: 22 fields (")
            && sum.contains(" shown): Recipient: \"@mallory\"; Gift message: \"Happy birthday\""),
        "{sum}"
    );
    assert!(sum.contains("5,000.00"), "{sum}");
}

/// Whatever is on the screen, the composed summary fits the bound before the final
/// sanitising cut (which `finish_summary` checks in debug builds, so this test also fails
/// on a fact being cut there).
#[test]
fn composed_summaries_fit_the_bound_before_the_final_cut() {
    for fields in [0u32, 1, 3, 12, 40] {
        for len in [0usize, 30, 200, 700] {
            for label_len in [4usize, 200] {
                for source in [ScreenSource::Typed, ScreenSource::RunnerText] {
                    let mut els: Vec<Element> = (0..8u32)
                        .map(|i| label(1 + i, &format!("Line {i} total ${i}9{i}.00 to @user{i}")))
                        .collect();
                    for i in 0..fields {
                        els.push(tf(9 + i, &format!("Field {i}"), &"v".repeat(len)));
                    }
                    let b = 9 + fields;
                    els.push(button(b, &format!("Pay {}", "x".repeat(label_len))));
                    let s = app_screen(els);
                    for return_only in [false, true] {
                        let sum = if return_only && fields > 0 {
                            jarvis_safety::return_summary(
                                9,
                                "typed text",
                                &s,
                                RiskCategory::SendMessage,
                                source,
                                None,
                                &[],
                            )
                        } else {
                            summary_with(
                                &Action::Tap { element: b },
                                &s,
                                RiskCategory::Payment,
                                source,
                                None,
                                &[],
                            )
                        };
                        assert!(sum.chars().count() <= BOUND, "{sum}");
                        assert!(sum.contains("$797.00"), "the largest amount: {sum}");
                    }
                }
            }
        }
    }
}

fn composer(value: &str, button_label: &str) -> CompactScreen {
    app_screen(vec![
        label(1, "Alice"),
        tv(2, "Message", value),
        button(3, button_label),
    ])
}

fn typed_message(text: &str) -> [TypedText; 1] {
    [TypedText {
        field: 2,
        text: text.to_owned(),
    }]
}

/// A control beside a composer Jarvis typed into is presumed to submit it: asked as
/// `SendMessage` (or stronger), with what is sent; clearly other controls, other fields and
/// search fields keep their category; without typing nothing is presumed.
#[test]
fn a_tap_after_typing_in_a_composer_is_presumed_to_send() {
    let decide = |s: &CompactScreen, typed: &[TypedText]| {
        evaluate_with(
            &Policy::default(),
            &Action::Tap { element: 3 },
            s,
            None,
            ScreenSource::Typed,
            None,
            typed,
        )
    };
    let typed = typed_message("On my way");
    for b in ["Done", "arrow.right", "Go", ""] {
        let s = composer("On my way", b);
        assert!(
            presumed_submit(&Action::Tap { element: 3 }, &s, None, &typed),
            "{b:?}"
        );
        match decide(&s, &typed) {
            Decision::Confirm(r) => {
                assert_eq!(r.category, RiskCategory::SendMessage, "{b:?}");
                assert!(
                    r.summary.contains("Sends: Message: \"On my way\""),
                    "{b:?}: {}",
                    r.summary
                );
            }
            other => panic!("{b:?}: {other:?}"),
        }
        // Not typed by Jarvis: nothing presumed ("Done" is not asked).
        assert!(
            !presumed_submit(&Action::Tap { element: 3 }, &s, None, &[]),
            "{b:?}"
        );
    }
    // A stronger category stays.
    let s = composer("On my way", "Delete conversation");
    match decide(&s, &typed) {
        Decision::Confirm(r) => assert_eq!(r.category, RiskCategory::Delete),
        other => panic!("{other:?}"),
    }
    // Clearly not a submit.
    for b in [
        "Cancel",
        "Close",
        "Back",
        "Clear text",
        "Emoji",
        "Dictate",
        "Attach",
        "Add photo",
        "camera.fill",
        "Next keyboard",
        "Bold",
        "Expand",
        "Show password",
        "xmark.circle.fill",
    ] {
        let s = composer("On my way", b);
        assert!(
            !presumed_submit(&Action::Tap { element: 3 }, &s, None, &typed),
            "{b:?}"
        );
    }
    // Another field, a switch: not a submit.
    let mut s = composer("On my way", "Done");
    s.elements[2].role = Role::Switch;
    assert!(!presumed_submit(
        &Action::Tap { element: 3 },
        &s,
        None,
        &typed
    ));
    // A search field keeps its leniency.
    let mut s = composer("socks", "Done");
    s.elements[1].role = Role::SearchField;
    s.elements[1].label = "Search".to_owned();
    assert!(!presumed_submit(
        &Action::Tap { element: 3 },
        &s,
        None,
        &typed_message("socks")
    ));
    // An emptied field sends nothing.
    let s = composer("", "Done");
    assert!(!presumed_submit(
        &Action::Tap { element: 3 },
        &s,
        None,
        &typed
    ));
}

/// `tap_point` and `long_press` on a send control show what is sent, like a tap.
#[test]
fn tap_point_and_long_press_show_what_is_sent() {
    let mut s = composer("Wire the $9,000 today", "Send");
    s.elements[2].frame = jarvis_proto::Rect::new(300.0, 700.0, 60.0, 44.0);
    for action in [
        Action::LongPress {
            element: 3,
            duration_ms: 800,
        },
        Action::TapPoint {
            x: 330.0 / s.size.width,
            y: 722.0 / s.size.height,
        },
    ] {
        let sum = summary_with(
            &action,
            &s,
            RiskCategory::SendMessage,
            ScreenSource::Typed,
            None,
            &[],
        );
        assert!(
            sum.contains("Sends: Message: \"Wire the $9,000 today\""),
            "{sum}"
        );
    }
}

/// Return in a single-line field of a form submits the form: the question says so and
/// shows the form's fields; a field alone keeps "the field's whole content"; a multi-line
/// text view with a plain Return key stays as it was.
#[test]
fn return_in_a_form_shows_the_form() {
    let form = |role: Role| {
        let mut s = app_screen(vec![
            label(1, "Send money"),
            tf(2, "To", "@mallory"),
            tf(3, "Amount", "$5,000.00"),
            {
                let mut e = el(4, role, "Note");
                e.value = Some("thanks".to_owned());
                e.focused = true;
                e
            },
        ]);
        s.keyboard_visible = true;
        s
    };
    let ret = |s: &CompactScreen| {
        jarvis_safety::return_summary(
            4,
            "thanks",
            s,
            RiskCategory::SendMessage,
            ScreenSource::Typed,
            None,
            &[],
        )
    };
    let mut s = form(Role::TextField);
    s.cues.return_key = Some("send".to_owned());
    assert_eq!(
        ret(&s),
        "Press Return (\"send\") in \"Note\" in Example, which submits the form. Sends: Note: \"thanks\"; To: \"@mallory\"; Amount: \"$5,000.00\". Typed: \"thanks\". Amounts on screen: Amount: $5,000.00"
    );
    // Alone.
    let mut alone = app_screen(vec![label(1, "Chat"), {
        let mut e = tf(2, "Message", "On my way");
        e.focused = true;
        e
    }]);
    alone.cues.return_key = Some("send".to_owned());
    let sum = jarvis_safety::return_summary(
        2,
        "On my way",
        &alone,
        RiskCategory::SendMessage,
        ScreenSource::Typed,
        None,
        &[],
    );
    assert!(
        sum.starts_with(
            "Press Return (\"send\") in \"Message\" in Example, which sends the field's whole content: \"On my way\""
        ),
        "{sum}"
    );
    // A multi-line text view with a plain Return key: as before (its own field only).
    let mut s = form(Role::TextView);
    s.cues.return_key = Some("return".to_owned());
    let sum = ret(&s);
    assert!(
        sum.contains("which sends the field's whole content: \"thanks\""),
        "{sum}"
    );
    // With a submit key it submits the form.
    s.cues.return_key = Some("send".to_owned());
    assert!(
        ret(&s).contains("which submits the form. Sends: Note"),
        "{}",
        ret(&s)
    );
}
