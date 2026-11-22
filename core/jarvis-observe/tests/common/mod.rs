#![allow(dead_code)]
//! Shared builders for the observe tests.

use jarvis_observe::compact;
use jarvis_proto::fixtures::{keyboard, node, rect, screen, text};
use jarvis_proto::{CompactScreen, Element, Role, Screen, UiNode};

pub use jarvis_proto::fixtures::{rect as r, IPHONE_SIZE};

/// Wraps content nodes in Application > Window and builds a Settings-like screen.
pub fn app_screen(children: Vec<UiNode>) -> Screen {
    app_screen_with(children, false)
}

pub fn app_screen_with(children: Vec<UiNode>, keyboard_visible: bool) -> Screen {
    let root =
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Window,
            IPHONE_SIZE.to_rect(),
        )
        .with_children(children)]);
    screen("com.example.app", "Example", root, keyboard_visible)
}

pub fn button(label: &str, x: f64, y: f64, w: f64, h: f64) -> UiNode {
    node(Role::Button, label, rect(x, y, w, h))
}

pub fn label(label: &str, x: f64, y: f64) -> UiNode {
    text(label, rect(x, y, 120.0, 20.0))
}

pub fn other(children: Vec<UiNode>) -> UiNode {
    UiNode::new(Role::Other("other".into()), IPHONE_SIZE.to_rect()).with_children(children)
}

pub fn kb() -> UiNode {
    keyboard("return")
}

pub fn compacted(children: Vec<UiNode>) -> CompactScreen {
    compact(&app_screen(children))
}

pub fn labels(c: &CompactScreen) -> Vec<&str> {
    c.elements.iter().map(|e| e.label.as_str()).collect()
}

pub fn by_label<'a>(c: &'a CompactScreen, label: &str) -> &'a Element {
    c.elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("no element labelled {label:?} in {:?}", labels(c)))
}

pub fn count_label(c: &CompactScreen, label: &str) -> usize {
    c.elements.iter().filter(|e| e.label == label).count()
}
