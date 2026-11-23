//! A row under the tab bar reaches the model marked, and unmarked once a scroll cleared it
//! (the Mac simulator's `testScrollThenTap`: a scripted model tapped such a row and the
//! core refused the tap). Also: the system prompt explains the marker.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use jarvis_agent::system_prompt;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, screen};
use jarvis_proto::{Rect, Role, RunnerCommand, Screen, UiNode};

/// A list scrolled by `offset` points under a full-width tab bar.
fn list(offset: f64) -> Screen {
    let rows: Vec<UiNode> = (0..12)
        .map(|i| {
            node(
                Role::Cell,
                &format!("Item {i}"),
                Rect::new(0.0, 100.0 + 60.0 * f64::from(i) - offset, 393.0, 56.0),
            )
        })
        .collect();
    let page = UiNode::new(Role::ScrollView, Rect::new(0.0, 0.0, 393.0, 852.0)).with_children(rows);
    let tabs = UiNode::new(Role::TabBar, Rect::new(0.0, 769.0, 393.0, 83.0)).with_children(vec![
        node(Role::Tab, "Home", Rect::new(10.0, 773.0, 100.0, 49.0)),
        node(Role::Tab, "Search", Rect::new(150.0, 773.0, 100.0, 49.0)),
    ]);
    let window = UiNode::new(Role::Window, Rect::new(0.0, 0.0, 393.0, 852.0))
        .with_children(vec![page, tabs]);
    let root = UiNode::new(Role::Application, Rect::new(0.0, 0.0, 393.0, 852.0))
        .with_children(vec![window]);
    screen("com.example.app", "Example", root, false)
}

fn line<'a>(user: &'a str, label: &str) -> &'a str {
    user.lines()
        .find(|l| l.starts_with('[') && l.contains(&format!("\"{label}\"")))
        .unwrap_or_else(|| panic!("no line for {label} in\n{user}"))
}

#[test]
fn the_model_sees_the_cover_and_then_the_clear_row() {
    let phone = Phone::new(list(0.0), |cmd, _| match cmd {
        RunnerCommand::Swipe { .. } => Transition::Go(list(200.0)),
        _ => Transition::Stay,
    });
    let model = Script::model(&[
        r#"{"thought":"bring it clear","action":"scroll","direction":"down"}"#,
        r#"{"thought":"seen","action":"done","summary":"seen"}"#,
    ]);
    let run = run(config(), "Open Item 11", MemoryStore::new(), phone, model);
    assert!(run.requests.len() >= 2, "{}", run.requests.len());
    let first = line(&run.requests[0].user, "Item 11");
    assert!(first.ends_with(" (under the tab bar: scroll)"), "{first}");
    // Rows clear of the bar and the tab bar's own tabs carry nothing.
    for label in ["Item 0", "Home"] {
        let l = line(&run.requests[0].user, label);
        assert!(!l.contains("(under") && !l.contains("(covered)"), "{l}");
    }
    let second = line(&run.requests[1].user, "Item 11");
    assert!(
        !second.contains("(under") && !second.contains("(covered)"),
        "{second}"
    );
}

#[test]
fn the_system_prompt_explains_the_marker() {
    let s = system_prompt();
    assert!(
        s.contains("(under the tab bar: scroll)") && s.contains("(covered)"),
        "{s}"
    );
    assert!(
        s.contains("scroll to bring it clear before tapping it"),
        "{s}"
    );
}
