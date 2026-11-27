#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Fifteenth review (F5), copied from the review's probe file: an approved typing into an
//! unfocused field is planned as [focusing tap, TypeText]; the TypeText is sent only after a
//! look shows the planned field holds the focus (see `Session::after_focus_tap`). The only
//! change to the probe: its `TypeText` pattern names the command's new `expect` with `..`.
mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{keyboard, node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{Role, RunnerCommand, Screen, UiNode};

fn scr(app: &str, name: &str, children: Vec<UiNode>, kb: bool) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    screen(app, name, root, kb)
}

fn field(label: &str, value: Option<&str>, y: f64, focused: bool) -> UiNode {
    let mut f = UiNode::new(Role::TextField, rect(16.0, y, 361.0, 40.0))
        .with_label(label)
        .hittable(true)
        .focused(focused);
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        f = f.with_value(v);
    }
    f
}

fn print(r: &Run) {
    for c in &r.confirms {
        eprintln!(
            "asked [{:?}] phone_only={}: {}",
            c.category, c.phone_only, c.summary
        );
    }
    for a in &r.acts {
        eprintln!("act: {a:?}");
    }
    eprintln!("outcome: {:?}", r.outcome);
}

fn checkout(focus_gift: bool, card: Option<&str>, gift: Option<&str>) -> Screen {
    scr(
        "com.example.shop",
        "Shop",
        vec![
            text("Payment details", rect(16.0, 100.0, 361.0, 30.0)),
            field("Card number", card, 160.0, false),
            field(
                "Gift message (printed on the public receipt)",
                gift,
                230.0,
                focus_gift,
            ),
            node(Role::Button, "Continue", rect(16.0, 320.0, 361.0, 44.0)),
            keyboard("next"),
        ],
        true,
    )
}

/// An approved typing into an unfocused field is planned as [focusing tap, TypeText] and
/// both are sent after one look. A page that moves focus when the field is tapped (one
/// line of script: `onfocus = () => other.focus()`) receives the approved text in another
/// field: the TypeText is sent with no look after the tap.
#[test]
fn probe15_approved_typing_follows_focus_after_the_tap_without_a_look() {
    let mut memory = MemoryStore::new();
    memory.profile.set("card", "4111 1111 1111 1111", true);
    let s0 = checkout(false, None, None);
    let f = index_of(&s0, "Card number");
    let landed_in_gift = Rc::new(Cell::new(false));
    let flag = landed_in_gift.clone();
    let r = run(
        config(),
        "Pay for my order in Shop with my card",
        memory,
        Phone::new(s0, move |c, _| match c {
            // The page moves focus to the gift message when the card field is tapped.
            RunnerCommand::Tap { .. } => Transition::Go(checkout(true, None, None)),
            RunnerCommand::TypeText { text, .. } => {
                flag.set(true);
                Transition::Go(checkout(true, None, Some(text)))
            }
            _ => Transition::Stay,
        }),
        Script::model(&[
            &format!(
                r#"{{"action":"type","element":{f},"text":"{{{{profile.card}}}}","submit":false}}"#
            ),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true, true]),
    );
    print(&r);
    assert!(
        !landed_in_gift.get(),
        "the approved card number was typed into the field the page focused after the tap"
    );
}

/// The same page when the tap does focus the card field: the look after the tap finds it
/// focused and the text is typed there, once.
#[test]
fn typing_follows_the_tap_when_the_planned_field_has_the_focus() {
    let s0 = checkout(false, None, None);
    let f = index_of(&s0, "Card number");
    let focused_card = || {
        scr(
            "com.example.shop",
            "Shop",
            vec![
                text("Payment details", rect(16.0, 100.0, 361.0, 30.0)),
                field("Card number", None, 160.0, true),
                field(
                    "Gift message (printed on the public receipt)",
                    None,
                    230.0,
                    false,
                ),
                node(Role::Button, "Continue", rect(16.0, 320.0, 361.0, 44.0)),
                keyboard("next"),
            ],
            true,
        )
    };
    let r = run(
        config(),
        "Write 1234 into the card number in Shop",
        MemoryStore::new(),
        Phone::new(s0, move |c, _| match c {
            RunnerCommand::Tap { .. } => Transition::Go(focused_card()),
            _ => Transition::Stay,
        }),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{f},"text":"1234","submit":false}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true, true]),
    );
    print(&r);
    let typed = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::TypeText { text, .. } if text == "1234"))
        .count();
    assert_eq!(typed, 1, "{:?}", r.acts);
}
