//! Shared builders: compact screens written in code, and a minimal flattener for the
//! jarvis-proto fixtures (jarvis-observe is not a dependency of this crate).

#![allow(dead_code)]

use jarvis_proto::fixtures::IPHONE_SIZE;
use jarvis_proto::{CompactScreen, Element, Rect, Role, Screen, UiNode};

/// An enabled element with `label`, stacked vertically by index.
pub fn el(index: u32, role: Role, label: &str) -> Element {
    Element {
        index,
        role,
        label: label.to_owned(),
        value: None,
        placeholder: None,
        frame: Rect::new(16.0, 40.0 * f64::from(index), 361.0, 36.0),
        enabled: true,
        focused: false,
        selected: false,
        secure: false,
        fingerprint: u64::from(index),
    }
}

/// Shorthand for a button.
pub fn button(index: u32, label: &str) -> Element {
    el(index, Role::Button, label)
}

/// Shorthand for static text.
pub fn label(index: u32, text: &str) -> Element {
    el(index, Role::StaticText, text)
}

/// A focused text input.
pub fn field(index: u32, role: Role, label: &str) -> Element {
    let mut e = el(index, role, label);
    e.focused = true;
    e
}

/// A compact screen in `bundle` named `name`.
pub fn screen(bundle: &str, name: &str, elements: Vec<Element>) -> CompactScreen {
    CompactScreen {
        app_bundle_id: bundle.to_owned(),
        app_name: Some(name.to_owned()),
        size: IPHONE_SIZE,
        keyboard_visible: elements.iter().any(|e| e.focused),
        elements,
    }
}

/// A screen in a neutral third-party app.
pub fn app_screen(elements: Vec<Element>) -> CompactScreen {
    screen("com.example.app", "Example", elements)
}

fn walk(node: &UiNode, out: &mut Vec<Element>, inside_cell: bool) {
    if !node.visible || node.frame.is_empty() {
        return;
    }
    let is_cell = node.role == Role::Cell;
    let keep = match &node.role {
        Role::Cell => true,
        Role::StaticText => !inside_cell && node.label.is_some(),
        r if r.is_container() => false,
        r => r.is_interactive(),
    };
    if keep {
        let mut label = node.label.clone().unwrap_or_default();
        if is_cell && label.is_empty() {
            label = node
                .children
                .iter()
                .filter(|c| c.role == Role::StaticText)
                .filter_map(|c| c.label.clone())
                .collect::<Vec<_>>()
                .join(" ");
        }
        let index = u32::try_from(out.len()).unwrap() + 1;
        out.push(Element {
            index,
            role: node.role.clone(),
            label,
            value: if node.secure {
                None
            } else {
                node.value.clone()
            },
            placeholder: node.placeholder.clone(),
            frame: node.frame,
            enabled: node.enabled,
            focused: node.focused,
            selected: node.selected,
            secure: node.secure,
            fingerprint: u64::from(index),
        });
    }
    for c in &node.children {
        walk(c, out, inside_cell || is_cell);
    }
}

/// Flattens a fixture screen into a compact screen (pre-order, indices from 1).
pub fn compact(s: &Screen) -> CompactScreen {
    let mut elements = Vec::new();
    walk(&s.root, &mut elements, false);
    CompactScreen {
        app_bundle_id: s.app_bundle_id.clone(),
        app_name: s.app_name.clone(),
        size: s.size,
        keyboard_visible: s.keyboard_visible,
        elements,
    }
}

/// The index of the first element with this exact label.
pub fn index_of(s: &CompactScreen, label: &str) -> u32 {
    s.elements
        .iter()
        .find(|e| e.label == label)
        .map(|e| e.index)
        .unwrap_or_else(|| panic!("no element labelled {label:?}"))
}
