#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review, home screen rule (`home_view`): by role and structure, a widget and
//! anything unclassified on a SpringBoard screen is dropped (a stub with a frame and no
//! text), icons, the dock, page dots, the search pill and every alert or banner stay.

use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{Role, Screen, UiNode};
use jarvis_safety::home_view;

fn home(kids: Vec<UiNode>, app: &str) -> Screen {
    let root =
        UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![UiNode::new(
            Role::Window,
            IPHONE_SIZE.to_rect(),
        )
        .with_children(kids)]);
    screen(app, "Home", root, false)
}

fn dump(n: &UiNode, out: &mut String) {
    out.push_str(&format!("{:?}|{:?}|{:?}\n", n.label, n.value, n.identifier));
    for c in &n.children {
        dump(c, out);
    }
}

fn all_text(s: &Screen) -> String {
    let mut out = String::new();
    dump(&s.root, &mut out);
    out
}

#[test]
fn only_springboard_is_filtered() {
    let s = home(
        vec![text("Secret", rect(0.0, 0.0, 10.0, 10.0))],
        "com.example.app",
    );
    assert!(home_view(&s).is_none());
}

#[test]
fn widgets_and_unclassified_nodes_are_dropped_and_launch_targets_stay() {
    let widget = node(Role::Button, "Weather", rect(10.0, 100.0, 180.0, 180.0))
        .with_value("Widget, Stack")
        .with_children(vec![
            text("WIDGETTEXT", rect(20.0, 110.0, 100.0, 20.0)),
            node(Role::Button, "WIDGETBUTTON", rect(20.0, 150.0, 80.0, 20.0)),
        ]);
    let page_chosen = node(
        Role::Other("Mystery".into()),
        "UNCLASSIFIED",
        rect(0.0, 300.0, 50.0, 50.0),
    )
    .with_children(vec![node(
        Role::Button,
        "INNER",
        rect(0.0, 300.0, 50.0, 50.0),
    )]);
    let loose_text = text("LOOSE", rect(0.0, 400.0, 50.0, 20.0));
    let icon = node(Role::Button, "Settings", rect(20.0, 500.0, 72.0, 72.0));
    let dock = node(
        Role::Other("Other".into()),
        "Dock",
        rect(0.0, 800.0, 400.0, 100.0),
    )
    .with_children(vec![node(
        Role::Button,
        "Safari",
        rect(10.0, 810.0, 72.0, 72.0),
    )]);
    let dots = node(
        Role::Other("PageIndicator".into()),
        "",
        rect(150.0, 700.0, 80.0, 30.0),
    )
    .with_value("Page 1 of 2");
    let s = home(
        vec![widget, page_chosen, loose_text, icon, dock, dots],
        "com.apple.springboard",
    );
    let shown = home_view(&s).unwrap();
    let t = all_text(&shown);
    for gone in [
        "Weather",
        "Widget",
        "WIDGETTEXT",
        "WIDGETBUTTON",
        "UNCLASSIFIED",
        "INNER",
        "LOOSE",
    ] {
        assert!(!t.contains(gone), "{gone} in {t}");
    }
    for kept in ["Settings", "Dock", "Safari", "Page 1 of 2"] {
        assert!(t.contains(kept), "{kept} missing in {t}");
    }
    // The stub of a control is a disabled control with the widget's frame (it still covers).
    let stub = &shown.root.children[0].children[0];
    assert_eq!(stub.role, Role::Button);
    assert!(!stub.enabled);
    assert_eq!(stub.frame, s.root.children[0].children[0].frame);
    assert!(stub.children.is_empty());
}

#[test]
fn alerts_banners_and_notifications_stay_in_full() {
    let banner = node(
        Role::Other("NotificationBanner".into()),
        "",
        rect(8.0, 50.0, 377.0, 80.0),
    )
    .with_children(vec![text(
        "Mom: dinner at 7",
        rect(16.0, 60.0, 300.0, 20.0),
    )]);
    let alert =
        node(Role::Alert, "Low Battery", rect(40.0, 300.0, 313.0, 200.0)).with_children(vec![
            node(Role::Button, "OK", rect(50.0, 440.0, 290.0, 44.0)),
        ]);
    let widget = node(Role::Button, "Mail", rect(10.0, 100.0, 180.0, 180.0))
        .with_value("Widget")
        .with_children(vec![text("WIDGETTEXT", rect(20.0, 110.0, 100.0, 20.0))]);
    let s = home(vec![widget, banner, alert], "com.apple.springboard");
    let t = all_text(&home_view(&s).unwrap());
    assert!(t.contains("Mom: dinner at 7") && t.contains("Low Battery") && t.contains("OK"));
    assert!(!t.contains("WIDGETTEXT") && !t.contains("Mail"));
}
