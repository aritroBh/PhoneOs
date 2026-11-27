#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Eighth review probes on the real iOS 26 fixtures.
mod common;

use common::ios::fixture;
use jarvis_proto::{Rect, Role, UiNode};

const FIXTURES: [&str; 10] = [
    "springboard_home",
    "playground_home",
    "toggles_off",
    "safari_start_page",
    "settings_root",
    "settings_general",
    "settings_about",
    "login_keyboard",
    "long_list_scrolled",
    "alert_dialog",
];

fn tree(view: &jarvis_observe::CompactView) -> jarvis_safety::ScreenTree<'_> {
    jarvis_safety::ScreenTree {
        sources: &view.sources,
        parents: &view.parents,
        ends: &view.ends,
        bars: &view.bars,
        nodes: &view.nodes,
        inputs: &view.inputs,
    }
}

/// Measurement for the suggested fix of the nested-control finding: on the ten real
/// fixtures, how many listed controls' tap points land on a drawn *descendant* control of
/// a different frame (which rule v2 exempts)? 0 means letting descendant controls contest
/// (except the target's own refined inner control) refuses nothing on real screens.
#[test]
fn probe_descendant_controls_under_real_tap_points() {
    let mut hits = Vec::new();
    for name in FIXTURES {
        let s = fixture(name);
        let view = jarvis_observe::compact_view(&s);
        let t = tree(&view);
        for e in view
            .screen
            .elements
            .iter()
            .filter(|e| jarvis_safety::takes_taps(&e.role))
        {
            let Ok(p) = jarvis_safety::tap_spot(e, &view.screen, Some(&t), None) else {
                continue;
            };
            let src = view.sources[(e.index - 1) as usize];
            let end = view.ends[src as usize];
            for n in &view.nodes {
                let same = (n.frame.x - e.frame.x).abs() < 1.0
                    && (n.frame.y - e.frame.y).abs() < 1.0
                    && (n.frame.width - e.frame.width).abs() < 1.0;
                if n.node > src
                    && n.node < end
                    && jarvis_safety::takes_taps(&n.role)
                    && !same
                    && n.frame.contains(p)
                {
                    hits.push(format!(
                        "{name}: [{}] {:?} at {p:?} -> {:?} {:?}",
                        e.index, e.label, n.role, n.label
                    ));
                }
            }
        }
    }
    eprintln!("{hits:#?}");
    assert!(hits.is_empty(), "{hits:#?}");
}

/// Robustness: hostile trees (very deep, very wide, NaN/inf/huge frames, huge strings) go
/// through compaction, the digest and the tap rule without a panic and in bounded time.
#[test]
fn probe_hostile_trees_do_not_panic_or_blow_up() {
    let size = Rect::new(0.0, 0.0, 393.0, 852.0);
    // Wide: 60k labelled buttons stacked on one spot, plus NaN and infinite frames.
    let mut kids = Vec::new();
    for i in 0..60_000 {
        let f = match i % 5 {
            0 => Rect::new(f64::NAN, 10.0, 50.0, 50.0),
            1 => Rect::new(10.0, 10.0, f64::INFINITY, 50.0),
            2 => Rect::new(-1e308, -1e308, 1e308, 1e308),
            _ => Rect::new(10.0, 300.0, 100.0, 44.0),
        };
        kids.push(
            UiNode::new(Role::Button, f)
                .with_label(format!("Pay {i}"))
                .hittable(true),
        );
    }
    let big = "x".repeat(2_000_000);
    kids.push(UiNode::new(Role::StaticText, Rect::new(0.0, 0.0, 300.0, 40.0)).with_label(&big));
    // Deep: 5k nested Other nodes (the shell caps depth at 200).
    let mut deep =
        UiNode::new(Role::Button, Rect::new(10.0, 500.0, 100.0, 44.0)).with_label("Deep");
    for _ in 0..5_000 {
        deep = UiNode::new(Role::Other("Other".into()), size).with_children(vec![deep]);
    }
    kids.push(deep);
    let root =
        UiNode::new(Role::Application, size)
            .with_children(vec![UiNode::new(Role::Window, size).with_children(kids)]);
    let s = jarvis_proto::fixtures::screen("com.example.x", "X", root, false);
    let t0 = std::time::Instant::now();
    let _ = jarvis_observe::screen_digest(&s);
    let view = jarvis_observe::compact_view(&s);
    let t = tree(&view);
    let n = view.screen.elements.len();
    for e in view.screen.elements.iter().take(50) {
        let _ = jarvis_safety::tap_spot(e, &view.screen, Some(&t), None);
    }
    let el = t0.elapsed();
    eprintln!("{n} elements, {} nodes, {el:?}", view.nodes.len());
    // (A 200k-deep tree overflows the stack in UiNode's recursive Drop; the shell caps depth at 200.)
    drop(s);
    assert!(el.as_secs() < 60, "{el:?}");
}

// ---- Regression tests for the eighth review's fixes ---------------------------------------

use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};

/// A chat screen with the keyboard up and `n` prediction candidates in the input
/// assistant (iOS 26: in the keyboard's own top-level window).
fn predictions(n: usize) -> jarvis_proto::Screen {
    let candidates: Vec<UiNode> = (0..n)
        .map(|i| {
            UiNode::new(
                Role::Other("Other".into()),
                rect(131.0 * i as f64, 472.0, 131.0, 44.0),
            )
            .with_label(format!("word{i}"))
        })
        .collect();
    let assistant = UiNode::new(Role::Other("Other".into()), rect(0.0, 472.0, 393.0, 44.0))
        .with_identifier("SystemInputAssistantView")
        .with_children(vec![UiNode::new(
            Role::Other("Other".into()),
            rect(0.0, 472.0, 393.0, 44.0),
        )
        .with_children(candidates)]);
    let keyboard = UiNode::new(Role::Keyboard, rect(0.0, 516.0, 393.0, 336.0)).with_children(vec![
        node(Role::Key, "q", rect(3.0, 527.0, 37.0, 46.0)),
        node(Role::Button, "send", rect(297.0, 689.0, 93.0, 46.0)).with_identifier("Return"),
    ]);
    let app = UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![
        text("To: Alice", rect(16.0, 80.0, 300.0, 40.0)),
        UiNode::new(Role::TextField, rect(16.0, 300.0, 361.0, 44.0))
            .with_label("Message")
            .with_value("On my way")
            .focused(true),
        node(Role::Button, "Send", rect(16.0, 360.0, 361.0, 44.0)),
    ]);
    let kbwin =
        UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![assistant, keyboard]);
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Chat")
        .with_children(vec![app, kbwin]);
    screen("com.example.chat", "Chat", root, true)
}

/// Only the prediction bar's own node is hashed: a different number of candidates after
/// the owner answers leaves the digest, the listing and the tap points as they were; the
/// bar moving, or a key appearing, still changes the digest; a point on the bar is still
/// refused as the keyboard.
#[test]
fn a_different_number_of_predictions_keeps_the_digest() {
    let a = predictions(3);
    for n in [0, 1, 2, 5] {
        let b = predictions(n);
        assert_eq!(
            jarvis_observe::screen_digest(&a),
            jarvis_observe::screen_digest(&b),
            "{n}"
        );
        assert_eq!(
            jarvis_observe::compact(&a),
            jarvis_observe::compact(&b),
            "{n}"
        );
    }
    let mut moved = predictions(3);
    moved.root.children[1].children[0].frame.y -= 4.0;
    assert_ne!(
        jarvis_observe::screen_digest(&a),
        jarvis_observe::screen_digest(&moved)
    );
    let mut key = predictions(3);
    key.root.children[1].children[1].children.push(node(
        Role::Key,
        "w",
        rect(42.0, 527.0, 37.0, 46.0),
    ));
    assert_ne!(
        jarvis_observe::screen_digest(&a),
        jarvis_observe::screen_digest(&key)
    );
    for n in [0, 3] {
        let s = predictions(n);
        let view = jarvis_observe::compact_view(&s);
        let t = tree(&view);
        assert_eq!(
            jarvis_safety::control_at(
                jarvis_proto::Point::new(380.0, 490.0),
                &view.screen,
                Some(&t)
            ),
            Err(jarvis_safety::PointError::Keyboard),
            "{n}"
        );
        for e in view
            .screen
            .elements
            .iter()
            .filter(|e| jarvis_safety::takes_taps(&e.role))
        {
            assert_eq!(
                jarvis_safety::tap_spot(e, &view.screen, Some(&t), None),
                Ok(e.frame.center()),
                "{n} {}",
                e.label
            );
        }
    }
}

fn toolbar_screen(in_scroll: bool) -> jarvis_proto::Screen {
    let size = IPHONE_SIZE.to_rect();
    let row = node(Role::Button, "Item 9", rect(0.0, 770.0, 393.0, 60.0));
    let bar = UiNode::new(Role::Toolbar, rect(0.0, 760.0, 393.0, 92.0)).with_children(vec![node(
        Role::Button,
        "Compose",
        rect(330.0, 775.0, 44.0, 44.0),
    )]);
    let content = if in_scroll {
        vec![UiNode::new(Role::ScrollView, size).with_children(vec![row, bar])]
    } else {
        vec![
            UiNode::new(Role::ScrollView, size).with_children(vec![row]),
            bar,
        ]
    };
    let root = UiNode::new(Role::Application, size)
        .with_label("Notes")
        .with_children(vec![UiNode::new(Role::Window, size).with_children(content)]);
    screen("com.example.notes", "Notes", root, false)
}

/// A full-width bottom toolbar outside page content takes precedence like a tab bar: a row
/// wholly under it is refused, naming the toolbar, and the toolbar's own button is tapped
/// at its centre; a toolbar inside scrolling content is page content and takes none.
#[test]
fn a_bottom_toolbar_takes_precedence_like_a_tab_bar() {
    let s = toolbar_screen(false);
    let view = jarvis_observe::compact_view(&s);
    let t = tree(&view);
    let roles: Vec<&Role> = view.bars.iter().map(|b| &b.role).collect();
    assert_eq!(roles, [&Role::Toolbar]);
    let row = view
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Item 9")
        .unwrap();
    assert_eq!(
        jarvis_safety::tap_spot(row, &view.screen, Some(&t), None),
        Err(jarvis_safety::Covered {
            target: row.index,
            by: jarvis_safety::CoveredBy::Bar(jarvis_safety::Bar::Tool),
        })
    );
    let compose = view
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Compose")
        .unwrap();
    assert_eq!(
        jarvis_safety::tap_spot(compose, &view.screen, Some(&t), None),
        Ok(compose.frame.center())
    );
    assert_eq!(jarvis_safety::Bar::Tool.name(), "the toolbar");
    // Inside scrolling content: no bar, and the row is contested only where the button is.
    let s = toolbar_screen(true);
    let view = jarvis_observe::compact_view(&s);
    assert!(view.bars.is_empty());
    let t = tree(&view);
    let row = view
        .screen
        .elements
        .iter()
        .find(|e| e.label == "Item 9")
        .unwrap();
    assert!(jarvis_safety::tap_spot(row, &view.screen, Some(&t), None).is_ok());
}

/// The raw text inputs: every one whose text is hashed, listed or not, with its value
/// (never a secure field's); scrolled-out ones are marked off screen.
#[test]
fn raw_inputs_list_scrolled_out_fields() {
    let size = IPHONE_SIZE.to_rect();
    let root = UiNode::new(Role::Application, size)
        .with_label("Mail")
        .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::ScrollView, size).with_children(vec![
                UiNode::new(Role::TextField, rect(0.0, -200.0, 393.0, 44.0))
                    .with_label("To:")
                    .with_value("ceo@evil.example"),
                UiNode::new(Role::SecureTextField, rect(0.0, -150.0, 393.0, 44.0))
                    .with_label("PIN")
                    .with_value("1234"),
                UiNode::new(Role::TextView, rect(0.0, 200.0, 393.0, 300.0))
                    .with_label("Message body")
                    .with_value("Hello"),
            ]),
        ])]);
    let s = screen("com.example.mail", "Mail", root, false);
    let view = jarvis_observe::compact_view(&s);
    let got: Vec<(Option<&str>, Option<&str>, bool, bool)> = view
        .inputs
        .iter()
        .map(|r| {
            (
                r.label.as_deref(),
                r.value.as_deref(),
                r.secure,
                r.on_screen,
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (Some("To:"), Some("ceo@evil.example"), false, false),
            (Some("PIN"), None, true, false),
            (Some("Message body"), Some("Hello"), false, true),
        ]
    );
    // Only the message body is listed.
    let labels: Vec<&str> = view
        .screen
        .elements
        .iter()
        .map(|e| e.label.as_str())
        .collect();
    assert_eq!(labels, ["Message body"]);
    // With an alert up, the fields behind it are not what its buttons send.
    let mut s = s;
    s.root.children[0].children.push(
        UiNode::new(Role::Alert, rect(40.0, 300.0, 313.0, 200.0)).with_children(vec![
            UiNode::new(Role::TextField, rect(60.0, 360.0, 273.0, 44.0)).with_label("Name"),
            node(Role::Button, "OK", rect(60.0, 420.0, 273.0, 44.0)),
        ]),
    );
    let view = jarvis_observe::compact_view(&s);
    let got: Vec<Option<&str>> = view.inputs.iter().map(|r| r.label.as_deref()).collect();
    assert_eq!(got, [Some("Name")]);
}
