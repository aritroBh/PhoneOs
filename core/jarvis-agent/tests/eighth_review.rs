#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Eighth review probes. Each test asserts the SAFE behaviour; a failing test is a finding.
mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::compact;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{RiskCategory, Role, RunnerCommand, Screen, UiNode};

fn scr(app: &str, children: Vec<UiNode>, kb: bool) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("App")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    screen(app, "App", root, kb)
}

fn tp(x: f64, y: f64) -> String {
    format!(
        r#"{{"action":"tap_point","x":{},"y":{}}}"#,
        x / IPHONE_SIZE.width,
        y / IPHONE_SIZE.height
    )
}

// ---- A. tap_point: the question names the riskiest element under the point, the tap
// goes to the smallest control there. ----

fn order_row() -> Screen {
    scr(
        "com.example.shop",
        vec![
            text("Your orders", rect(16.0, 100.0, 300.0, 40.0)),
            // A row button (whole row is a "Buy again" button) with a small trailing
            // "Delete" button inside it.
            node(
                Role::Button,
                "Buy again: Socks $5.00",
                rect(16.0, 300.0, 361.0, 80.0),
            )
            .with_children(vec![node(
                Role::Button,
                "Delete order",
                rect(300.0, 320.0, 60.0, 40.0),
            )]),
        ],
        false,
    )
}

#[test]
fn probe_tap_point_question_names_the_control_the_tap_lands_on() {
    let s = order_row();
    eprintln!("{:#?}", compact(&s).elements);
    let r = run(
        vision_config(),
        "Manage my orders",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[
            r#"{"action":"look"}"#,
            &tp(330.0, 340.0),
            r#"{"action":"fail","reason":"x"}"#,
        ])
        .decisions(&[true, true]),
    );
    eprintln!("confirms {:#?}\nacts {:?}", r.confirms, r.acts);
    let tapped_delete = r.acts.iter().any(|a| taps(a, &s, "Delete order"));
    let asked_delete = r
        .confirms
        .iter()
        .any(|c| c.summary.contains("Delete order"));
    assert!(
        !tapped_delete || asked_delete,
        "tap landed on \"Delete order\" after a question about something else: {:#?}",
        r.confirms
    );
}

// ---- B. tap_point on a Send button: the question does not show what is sent. ----

fn chat(draft: &str) -> Screen {
    scr(
        "com.example.chat",
        vec![
            text("Alice", rect(16.0, 80.0, 300.0, 40.0)),
            UiNode::new(Role::TextView, rect(16.0, 700.0, 300.0, 44.0))
                .with_label("Message")
                .with_value(draft)
                .hittable(true),
            node(Role::Button, "Send", rect(320.0, 700.0, 60.0, 44.0)),
        ],
        false,
    )
}

#[test]
fn probe_tap_point_send_shows_what_is_sent() {
    let s = chat("Wire the $9,000 to account 4421 today");
    let tap = r#"{"action":"tap","element":3}"#;
    let r = run(
        config(),
        "Send it",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[tap, r#"{"action":"fail","reason":"x"}"#]),
    );
    eprintln!("tap: {:#?}", r.confirms);
    let r = run(
        vision_config(),
        "Send it",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[
            r#"{"action":"look"}"#,
            &tp(350.0, 722.0),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    eprintln!("tap_point: {:#?}", r.confirms);
    assert!(
        r.confirms
            .iter()
            .any(|c| c.summary.contains("Wire the $9,000")),
        "tap_point on Send asked without the message it sends: {:#?}",
        r.confirms
    );
}

// ---- C. A send tap's "Sends:" leaves out form fields that are not listed. ----

fn mail(to_y: f64) -> Screen {
    scr(
        "com.example.mail",
        vec![
            UiNode::new(Role::NavigationBar, rect(0.0, 50.0, 393.0, 50.0)).with_children(vec![
                node(Role::Button, "Cancel", rect(10.0, 55.0, 70.0, 40.0)),
                node(Role::Button, "Send", rect(310.0, 55.0, 70.0, 40.0)),
            ]),
            UiNode::new(Role::ScrollView, rect(0.0, 100.0, 393.0, 752.0)).with_children(vec![
                UiNode::new(Role::TextField, rect(0.0, to_y, 393.0, 44.0))
                    .with_label("To:")
                    .with_value("ceo-impersonator@evil.example")
                    .hittable(true),
                UiNode::new(Role::TextField, rect(0.0, to_y + 44.0, 393.0, 44.0))
                    .with_label("Subject:")
                    .with_value("Payroll")
                    .hittable(true),
                UiNode::new(Role::TextView, rect(0.0, 200.0, 393.0, 600.0))
                    .with_label("Message body")
                    .with_value("Here are the payroll files")
                    .hittable(true),
            ]),
        ],
        false,
    )
}

#[test]
fn probe_sends_names_the_recipient_field_scrolled_out_of_view() {
    // The compose view has scrolled: the To and Subject fields are above the screen.
    let s = mail(-200.0);
    let send = index_of(&s, "Send");
    let r = run(
        config(),
        "Send the mail",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{send}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    eprintln!("{:#?}", r.confirms);
    let c = r.confirms.last().expect("asked");
    assert!(
        c.summary.contains("evil.example")
            || c.summary.contains("not shown")
            || c.summary.contains("To"),
        "Send asked without the recipient field it sends: {}",
        c.summary
    );
}

// ---- D. A control nested inside the tapped control, at its centre, takes the tap; it is
// never listed, never classified and never contests (descendants are exempt). ----

fn card(outer: Role, inner_role: Role, inner: &str) -> Screen {
    scr(
        "com.example.shop",
        vec![
            text("Recommended", rect(16.0, 100.0, 300.0, 40.0)),
            node(outer, "Wool socks", rect(16.0, 300.0, 361.0, 120.0)).with_children(vec![
                UiNode::new(Role::Image, rect(16.0, 300.0, 361.0, 120.0)),
                node(inner_role, inner, rect(146.0, 340.0, 100.0, 40.0)),
            ]),
        ],
        false,
    )
}

#[test]
fn probe_tap_on_a_card_does_not_land_on_a_nested_buy_button_unasked() {
    let mut bad = Vec::new();
    for outer in [Role::Button, Role::Link] {
        for (inner_role, inner) in [
            (Role::Button, "Buy now $25.00"),
            (Role::Button, "Delete"),
            (
                Role::Other("CheckBox".to_owned()),
                "Subscribe & save $25.00/mo",
            ),
        ] {
            let s = card(outer.clone(), inner_role.clone(), inner);
            let i = index_of(&s, "Wool socks");
            let r = run(
                config(),
                "Look at the socks",
                MemoryStore::new(),
                Phone::still(s.clone()),
                Script::model(&[
                    &format!(r#"{{"action":"tap","element":{i}}}"#),
                    r#"{"action":"fail","reason":"x"}"#,
                ]),
            );
            let on_inner = r
                .acts
                .iter()
                .any(|a| point_of(a).is_some_and(|p| rect(146.0, 340.0, 100.0, 40.0).contains(p)));
            if on_inner && r.confirms.is_empty() {
                bad.push(format!(
                    "{outer:?} > {inner_role:?} {inner:?}: {:?}",
                    r.acts
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "taps sent onto a nested control with no question: {bad:#?}"
    );
}

// ---- E. Robustness: one gated tap on a hostile 20k-element screen stays bounded. ----

#[test]
fn probe_gate_on_a_huge_screen_is_bounded() {
    let mut kids = vec![node(
        Role::Button,
        "Pay $5.00",
        rect(16.0, 60.0, 200.0, 40.0),
    )];
    for i in 0..5_000 {
        let y = 120.0 + f64::from(i % 600);
        kids.push(text(
            &format!("Item {i} $1{i}.00 to @user{i}"),
            rect(16.0, y, 300.0, 20.0),
        ));
        kids.push(
            UiNode::new(Role::TextField, rect(16.0, y, 300.0, 20.0))
                .with_label(format!("Field {i}"))
                .with_value(format!("value {i}"))
                .hittable(true),
        );
    }
    let s = scr("com.example.shop", kids, false);
    let i = index_of(&s, "Pay $5.00");
    let t0 = std::time::Instant::now();
    let r = run(
        config(),
        "Pay",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{i}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ])
        .decisions(&[true]),
    );
    let el = t0.elapsed();
    eprintln!("{el:?} confirms {}", r.confirms.len());
    assert!(el.as_secs() < 120, "{el:?}");
}

// ---- F. Return in one field of a form submits the whole form; the question shows only
// that field, so a pre-filled recipient is "(filled)". ----

fn pay_form(note: &str) -> Screen {
    scr(
        "com.example.pay",
        vec![
            text("Send money", rect(16.0, 60.0, 300.0, 40.0)),
            UiNode::new(Role::TextField, rect(16.0, 120.0, 361.0, 44.0))
                .with_label("To")
                .with_value("@mallory")
                .hittable(true),
            UiNode::new(Role::TextField, rect(16.0, 180.0, 361.0, 44.0))
                .with_label("Amount")
                .with_value("$5,000.00")
                .hittable(true),
            UiNode::new(Role::TextField, rect(16.0, 240.0, 361.0, 44.0))
                .with_label("Note")
                .with_value(note)
                .hittable(true)
                .focused(true),
            node(Role::Button, "Pay", rect(16.0, 300.0, 361.0, 50.0)),
            jarvis_proto::fixtures::keyboard("send"),
        ],
        true,
    )
}

#[test]
fn probe_return_in_a_form_field_shows_the_form_it_submits() {
    let before = pay_form("");
    let after = pay_form("thanks");
    let note = index_of(&before, "Note");
    let mut typed = false;
    let r = run(
        config(),
        "Add a thank-you note and send",
        MemoryStore::new(),
        Phone::new(before, move |c, s| {
            if matches!(c, RunnerCommand::TypeText { .. }) {
                typed = true;
            }
            if typed {
                Transition::Go(after.clone())
            } else {
                Transition::Go(s.clone())
            }
        }),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{note},"text":"thanks","submit":true}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    eprintln!("{:#?}", r.confirms);
    let c = r.confirms.last().expect("Return asked");
    assert!(
        c.summary.contains("mallory"),
        "Return submits the form to @mallory, the question does not say so: {}",
        c.summary
    );
}

// ---- Regression tests for the eighth review's fixes ---------------------------------------

fn composer(draft: &str, button: &str) -> Screen {
    scr(
        "com.example.chat",
        vec![
            text("Alice", rect(16.0, 80.0, 300.0, 40.0)),
            UiNode::new(Role::TextView, rect(16.0, 700.0, 280.0, 44.0))
                .with_label("Message")
                .with_value(draft)
                .hittable(true),
            node(Role::Button, button, rect(320.0, 700.0, 60.0, 44.0)),
        ],
        false,
    )
}

/// After Jarvis typed into a composer, a tap on an unlisted-synonym or icon button beside
/// it ("Done", an unknown symbol) asks as `SendMessage` and shows the draft it sends; the
/// same tap without typing does not ask.
#[test]
fn a_tap_beside_a_composer_jarvis_typed_into_asks_and_shows_the_draft() {
    for b in ["Done", "arrow.right.circle"] {
        let before = composer("", b);
        let after = composer("On my way", b);
        let message = index_of(&before, "Message");
        let button = index_of(&before, b);
        let mut typed = false;
        let r = run(
            config(),
            "Tell Alice I'm on my way",
            MemoryStore::new(),
            Phone::new(before, move |c, s| {
                if matches!(c, RunnerCommand::TypeText { .. }) {
                    typed = true;
                }
                if typed {
                    Transition::Go(after.clone())
                } else {
                    Transition::Go(s.clone())
                }
            }),
            Script::model(&[
                &format!(r#"{{"action":"type","element":{message},"text":"On my way"}}"#),
                &format!(r#"{{"action":"tap","element":{button}}}"#),
                r#"{"action":"fail","reason":"x"}"#,
            ])
            .decisions(&[false]),
        );
        let c = r.confirms.last().expect("asked");
        assert_eq!(c.category, RiskCategory::SendMessage, "{b}: {c:?}");
        assert!(
            c.summary.contains("Sends: Message: \"On my way\""),
            "{b}: {}",
            c.summary
        );
        // Declined: nothing tapped.
        assert!(
            !r.acts.iter().any(|a| taps(a, &composer("On my way", b), b)),
            "{b}"
        );
    }
    // Without typing first, "Done" beside a draft the owner wrote is not presumed to send.
    let s = composer("On my way", "Done");
    let button = index_of(&s, "Done");
    let r = run(
        config(),
        "Close the keyboard",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{button}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    assert!(r.confirms.is_empty(), "{:?}", r.confirms);
}

/// A long press on a send control shows what is sent, like a tap.
#[test]
fn a_long_press_on_send_shows_what_is_sent() {
    let s = chat("Wire the $9,000 to account 4421 today");
    let r = run(
        config(),
        "Send it",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[
            r#"{"action":"long_press","element":3,"duration_ms":800}"#,
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    let c = r.confirms.last().expect("asked");
    assert!(
        c.summary.contains("Sends: Message: \"Wire the $9,000"),
        "{}",
        c.summary
    );
}

/// A card whose nested button covers every candidate point is refused, naming the nested
/// control; a card with room around its nested button is tapped beside it.
#[test]
fn a_nested_control_covering_its_card_refuses_the_tap() {
    let s = scr(
        "com.example.shop",
        vec![
            node(Role::Button, "Wool socks", rect(16.0, 300.0, 361.0, 60.0)).with_children(vec![
                node(
                    Role::Button,
                    "Buy now $25.00",
                    rect(16.0, 300.0, 361.0, 60.0),
                ),
            ]),
        ],
        false,
    );
    let i = index_of(&s, "Wool socks");
    let r = run(
        config(),
        "Look at the socks",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{i}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    assert!(
        !r.acts.iter().any(|a| point_of(a).is_some()),
        "{:?}",
        r.acts
    );
    let told = r.all_prompt_text();
    assert!(
        told.contains("is covered by a control inside it labelled 'Buy now $25.00'"),
        "{told}"
    );
    // Room beside the nested button: tapped there, never on it.
    let s = card(Role::Button, Role::Button, "Buy now $25.00");
    let i = index_of(&s, "Wool socks");
    let r = run(
        config(),
        "Look at the socks",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{i}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    let points: Vec<_> = r.acts.iter().filter_map(point_of).collect();
    assert_eq!(points.len(), 1, "{:?}", r.acts);
    assert!(
        !rect(146.0, 340.0, 100.0, 40.0).contains(points[0]),
        "{points:?}"
    );
    assert!(
        rect(16.0, 300.0, 361.0, 120.0).contains(points[0]),
        "{points:?}"
    );
}

/// A SwiftUI-style toggle row (a `Switch` wrapping the real `Switch`) is still tapped on
/// its inner switch: the target's own refined inner control does not contest.
#[test]
fn a_refined_inner_control_does_not_contest() {
    let s = scr(
        "com.example.app",
        vec![UiNode::new(Role::Switch, rect(20.0, 168.0, 353.0, 58.0))
            .with_label("Show tips")
            .with_value("0")
            .hittable(true)
            .with_children(vec![UiNode::new(
                Role::Switch,
                rect(300.0, 183.0, 63.0, 28.0),
            )
            .with_value("0")
            .hittable(true)])],
        false,
    );
    let i = index_of(&s, "Show tips");
    let r = run(
        config(),
        "Turn on tips",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"tap","element":{i}}}"#),
            r#"{"action":"fail","reason":"x"}"#,
        ]),
    );
    let points: Vec<_> = r.acts.iter().filter_map(point_of).collect();
    assert_eq!(
        points,
        [rect(300.0, 183.0, 63.0, 28.0).center()],
        "{:?}",
        r.acts
    );
}
