#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_observe::{compact, parse_runner_text, render, ParseError};
use jarvis_proto::fixtures::screen;
use jarvis_proto::{Rect, Role, UiNode};

const SAMPLE: &str = "\
Application, label: 'Settings', {{0, 0}, {393, 852}}
  Window, {{0, 0}, {393, 852}}
    NavigationBar, id: 'Settings', {{0, 59}, {393, 81}}
      StaticText, label: 'Settings', {{16, 96}, {140, 41}}
    Table, {{0, 140}, {393, 712}}
      SearchField, label: 'Search', {{16, 148}, {361, 36}}
      Cell, {{16, 200}, {361, 44}}
        StaticText, label: 'Airplane Mode', {{60, 212}, {200, 20}}
        Switch, label: 'Airplane Mode', value: '0', id: 'airplane-mode-switch', {{312, 206}, {51, 31}}
      Cell, {{16, 244}, {361, 44}}
        StaticText, label: 'Wi-Fi', {{60, 256}, {200, 20}}
        StaticText, label: 'HomeNet', {{260, 256}, {100, 20}}
      Type47, {{0, 0}, {0, 0}}
    Keyboard, {{0, 516}, {393, 336}}
      Key, label: 'q', {{3, 527}, {37, 46}}
";

fn walk<'a>(n: &'a UiNode, out: &mut Vec<&'a UiNode>) {
    out.push(n);
    for c in &n.children {
        walk(c, out);
    }
}

fn all(n: &UiNode) -> Vec<&UiNode> {
    let mut v = Vec::new();
    walk(n, &mut v);
    v
}

#[test]
fn parses_a_realistic_multi_level_tree() {
    let parsed = parse_runner_text(SAMPLE).unwrap();
    assert_eq!(parsed.warnings, 0);
    let root = &parsed.root;
    assert_eq!(root.role, Role::Application);
    assert_eq!(root.label.as_deref(), Some("Settings"));
    assert_eq!(root.frame, Rect::new(0.0, 0.0, 393.0, 852.0));
    assert_eq!(root.count(), 15);

    let window = &root.children[0];
    assert_eq!(window.role, Role::Window);
    assert_eq!(window.children.len(), 3, "nav bar, table, keyboard");
    let nav = &window.children[0];
    assert_eq!(nav.role, Role::NavigationBar);
    assert_eq!(nav.identifier.as_deref(), Some("Settings"));
    assert_eq!(nav.label, None);
    assert_eq!(nav.children[0].label.as_deref(), Some("Settings"));

    let table = &window.children[1];
    assert_eq!(table.children.len(), 4);
    let switch = &table.children[1].children[1];
    assert_eq!(switch.role, Role::Switch);
    assert_eq!(switch.value.as_deref(), Some("0"));
    assert_eq!(switch.identifier.as_deref(), Some("airplane-mode-switch"));
    assert_eq!(switch.frame, Rect::new(312.0, 206.0, 51.0, 31.0));

    // Unmapped type name is kept as Other; keyboard children nest under the keyboard.
    assert_eq!(table.children[3].role, Role::Other("Type47".into()));
    assert_eq!(window.children[2].children[0].role, Role::Key);
}

#[test]
fn flags_default_as_documented() {
    let parsed = parse_runner_text(
        "Application, {{0, 0}, {393, 852}}\n  SecureTextField, label: 'Password', {{10, 10}, {200, 40}}\n  TextField, label: 'Email', {{10, 60}, {200, 40}}\n",
    )
    .unwrap();
    for n in all(&parsed.root) {
        assert!(n.enabled && n.visible && n.hittable, "{:?}", n.role);
        assert!(!n.focused && !n.selected, "{:?}", n.role);
    }
    let kids = &parsed.root.children;
    assert!(kids[0].secure);
    assert!(!kids[1].secure);
}

#[test]
fn awkward_labels_survive_commas_quotes_braces_and_lookalike_fields() {
    let text = "\
Application, {{0, 0}, {393, 852}}
  Button, label: 'Sign in, or create account', {{10, 10}, {200, 40}}
  Button, label: 'It's \"complicated\"', value: 'a, b', {{10, 60}, {200, 40}}
  StaticText, label: '{{curly}} {braces}', {{10, 110}, {200, 20}}
  StaticText, label: 'Total: 3 items, id: 'x'', id: 'real', {{10, 140}, {200, 20}}
  Button, label: 'Résumé 日本語 \u{1F600}', {{-5, 170}, {200.5, 40}}
  StaticText, label: '', {{10, 220}, {10, 10}}
  Button, label: 'Ends with comma,', {{10, 240}, {200, 40}}
";
    let parsed = parse_runner_text(text).unwrap();
    assert_eq!(parsed.warnings, 0);
    let k = &parsed.root.children;
    assert_eq!(k[0].label.as_deref(), Some("Sign in, or create account"));
    assert_eq!(k[1].label.as_deref(), Some("It's \"complicated\""));
    assert_eq!(k[1].value.as_deref(), Some("a, b"));
    assert_eq!(k[2].label.as_deref(), Some("{{curly}} {braces}"));
    assert_eq!(k[2].frame, Rect::new(10.0, 110.0, 200.0, 20.0));
    assert_eq!(k[3].label.as_deref(), Some("Total: 3 items, id: 'x'"));
    assert_eq!(k[3].identifier.as_deref(), Some("real"));
    assert_eq!(k[4].label.as_deref(), Some("Résumé 日本語 \u{1F600}"));
    assert_eq!(k[4].frame, Rect::new(-5.0, 170.0, 200.5, 40.0));
    assert_eq!(k[5].label, None, "empty label means absent");
    assert_eq!(k[6].label.as_deref(), Some("Ends with comma,"));
}

#[test]
fn tolerates_blank_lines_trailing_whitespace_crlf_and_other_indent_widths() {
    let text = "\r\nApplication, {{0, 0}, {393, 852}}   \r\n\r\n    Window, {{0, 0}, {393, 852}}\t\r\n        Button, label: 'OK', {{10, 10}, {80, 44}}  \n\n";
    let parsed = parse_runner_text(text).unwrap();
    assert_eq!(parsed.warnings, 0);
    assert_eq!(
        parsed.root.children[0].children[0].label.as_deref(),
        Some("OK")
    );

    // Tabs / 3-space indent / whole tree indented.
    let text = "   Application, {{0, 0}, {10, 10}}\n      Window, {{0, 0}, {10, 10}}\n         Button, label: 'Deep', {{0, 0}, {5, 5}}\n";
    let parsed = parse_runner_text(text).unwrap();
    assert_eq!(parsed.root.role, Role::Application);
    assert_eq!(
        parsed.root.children[0].children[0].label.as_deref(),
        Some("Deep")
    );
}

#[test]
fn garbage_lines_are_skipped_and_counted() {
    let text = "\
Application, {{0, 0}, {393, 852}}
  Window, {{0, 0}, {393, 852}}
===== truncated =====
    Button, label: 'OK', {{10, 10}, {80, 44}}
    Button, label: 'Broken frame', {{10, 10}, {80
    Button, label: 'Cancel', {{100, 10}, {80, 44}}
{\"json\": true}
";
    let parsed = parse_runner_text(text).unwrap();
    assert_eq!(parsed.warnings, 3);
    let labels: Vec<_> = parsed.root.children[0]
        .children
        .iter()
        .map(|n| n.label.clone().unwrap())
        .collect();
    assert_eq!(labels, vec!["OK", "Cancel"]);
}

#[test]
fn labels_with_embedded_newlines_are_rejoined() {
    let text = "\
Application, {{0, 0}, {393, 852}}
  TextView, label: 'Notes', value: 'first line
second line
third', {{10, 10}, {300, 120}}
  Button, label: 'Save', {{10, 140}, {80, 44}}
";
    let parsed = parse_runner_text(text).unwrap();
    assert_eq!(parsed.warnings, 0);
    let kids = &parsed.root.children;
    assert_eq!(kids.len(), 2);
    assert_eq!(
        kids[0].value.as_deref(),
        Some("first line\nsecond line\nthird")
    );
    assert_eq!(kids[1].label.as_deref(), Some("Save"));
}

#[test]
fn depth_jumps_and_multiple_roots_do_not_lose_nodes() {
    // Child indented three levels under a root: attaches to the nearest open parent.
    let parsed = parse_runner_text(
        "Application, {{0, 0}, {100, 100}}\n      Button, label: 'Far', {{0, 0}, {10, 10}}\n  Button, label: 'Near', {{0, 20}, {10, 10}}\n",
    )
    .unwrap();
    assert_eq!(parsed.root.children.len(), 2);

    // Two top-level nodes get a synthetic application root spanning both.
    let parsed =
        parse_runner_text("Window, {{0, 0}, {100, 50}}\nWindow, {{0, 50}, {100, 50}}\n").unwrap();
    assert_eq!(parsed.root.role, Role::Application);
    assert_eq!(parsed.root.children.len(), 2);
    assert_eq!(parsed.root.frame, Rect::new(0.0, 0.0, 100.0, 100.0));
}

#[test]
fn empty_or_unparseable_input_is_an_error() {
    assert_eq!(
        parse_runner_text(""),
        Err(ParseError::NoNodes { skipped: 0 })
    );
    assert_eq!(
        parse_runner_text("  \n\n"),
        Err(ParseError::NoNodes { skipped: 0 })
    );
    let err = parse_runner_text("hello\nworld, nothing here\n").unwrap_err();
    assert_eq!(err, ParseError::NoNodes { skipped: 2 });
    assert!(err.to_string().contains('2'));
}

#[test]
fn parsed_runner_text_flows_through_compact_and_render() {
    let parsed = parse_runner_text(SAMPLE).unwrap();
    let s = screen("com.apple.Preferences", "Settings", parsed.root, true);
    let c = compact(&s);
    let text = render(&c, 1000);
    let want = "\
App: Settings (com.apple.Preferences)
Keyboard: visible
[1] StaticText \"Settings\"
[2] SearchField \"Search\"
[3] Switch \"Airplane Mode\" value=\"0\"
[4] Cell \"Wi-Fi, HomeNet\"";
    assert_eq!(text, want);
}

#[test]
fn big_tree_parses_in_linear_time() {
    let mut text = String::from("Application, {{0, 0}, {393, 852}}\n");
    for i in 0..2000 {
        text.push_str(&format!(
            "  Button, label: 'Item {i}', {{{{10, {}}}, {{80, 44}}}}\n",
            i % 800
        ));
    }
    let started = std::time::Instant::now();
    let parsed = parse_runner_text(&text).unwrap();
    assert!(started.elapsed().as_secs() < 2);
    assert_eq!(parsed.root.children.len(), 2000);
}
