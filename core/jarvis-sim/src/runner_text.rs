//! Serialising a `UiNode` tree to the indented text a LoopCtrl-style runner returns.
//!
//! One line per node, two spaces of indent per depth:
//! `Type, label: '...', value: '...', id: '...', {{x, y}, {w, h}}`. Label, value and id appear
//! only when non-empty and the frame is truncated to integers. Flags (enabled, focused,
//! selected, hittable) and placeholders are not part of the format, which is exactly what the
//! agent loses on a real device. `jarvis_observe::parse_runner_text` reads it back.

use jarvis_proto::{Rect, Role, UiNode};

fn type_name(role: &Role) -> String {
    match role {
        Role::Button => "Button",
        Role::Link => "Link",
        Role::TextField => "TextField",
        Role::SecureTextField => "SecureTextField",
        Role::TextView => "TextView",
        Role::SearchField => "SearchField",
        Role::StaticText => "StaticText",
        Role::Image => "Image",
        Role::Cell => "Cell",
        Role::Switch => "Switch",
        Role::Slider => "Slider",
        Role::Picker => "Picker",
        Role::Tab => "Tab",
        Role::NavigationBar => "NavigationBar",
        Role::TabBar => "TabBar",
        Role::Toolbar => "Toolbar",
        Role::Alert => "Alert",
        Role::Sheet => "Sheet",
        Role::Keyboard => "Keyboard",
        Role::Key => "Key",
        Role::ScrollView => "ScrollView",
        Role::Table => "Table",
        Role::CollectionView => "CollectionView",
        Role::WebView => "WebView",
        Role::Window => "Window",
        Role::Application => "Application",
        Role::Other(name) => name,
    }
    .to_owned()
}

fn frame(r: &Rect) -> String {
    // The runner truncates each component to an integer.
    #[allow(clippy::cast_possible_truncation)]
    let i = |v: f64| v.trunc() as i64;
    format!(
        "{{{{{}, {}}}, {{{}, {}}}}}",
        i(r.x),
        i(r.y),
        i(r.width),
        i(r.height)
    )
}

fn line(node: &UiNode, depth: usize, out: &mut String) {
    out.push_str(&"  ".repeat(depth));
    out.push_str(&type_name(&node.role));
    let mut field = |key: &str, v: &Option<String>| {
        if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
            out.push_str(&format!(", {key}: '{v}'"));
        }
    };
    field("label", &node.label);
    field("value", &node.value);
    field("id", &node.identifier);
    out.push_str(", ");
    out.push_str(&frame(&node.frame));
    out.push('\n');
    for c in node.children.iter().filter(|c| c.visible) {
        line(c, depth + 1, out);
    }
}

/// The runner's text rendering of `root`.
pub fn to_runner_text(root: &UiNode) -> String {
    let mut out = String::new();
    line(root, 0, &mut out);
    out
}
