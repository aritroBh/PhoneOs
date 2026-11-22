#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! The exact digest of the raw tree (fourth review): what an approval is checked against.
//! Everything the runner reports counts (also what compaction drops), but for the documented
//! exclusions: frames, focus, secure values, the status bar, the keyboard and its
//! prediction bar, SpringBoard badges and scroll-indicator positions.

mod common;

use std::time::{Duration, Instant};

use common::ios::fixture;
use common::*;
use jarvis_observe::{compact, parse_runner_text, screen_digest, screen_digest_input};
use jarvis_proto::fixtures::{node, rect, screen, text};
use jarvis_proto::{Rect, Role, Screen, UiNode};

fn same(a: &Screen, b: &Screen) -> bool {
    screen_digest(a) == screen_digest(b)
}

/// Every node of `s` (pre-order, mutable), for editing fixtures.
fn edit(s: &mut Screen, mut f: impl FnMut(&mut UiNode)) {
    let mut stack = vec![&mut s.root];
    while let Some(n) = stack.pop() {
        f(n);
        stack.extend(n.children.iter_mut());
    }
}

fn row(parts: usize, last: &str) -> Screen {
    let mut kids: Vec<UiNode> = (0..parts)
        .map(|k| {
            text(
                &format!("Line {k} detail"),
                rect(16.0, 100.0 + 20.0 * k as f64, 300.0, 18.0),
            )
        })
        .collect();
    kids.push(text(
        last,
        rect(16.0, 100.0 + 20.0 * parts as f64, 300.0, 18.0),
    ));
    app_screen(vec![
        UiNode::new(Role::Cell, rect(0.0, 90.0, 393.0, 300.0)).with_children(kids),
        button("Place order", 16.0, 700.0, 361.0, 50.0),
    ])
}

#[test]
fn equal_trees_with_different_frames_are_equal() {
    let a = fixture("long_list_scrolled");
    let mut b = a.clone();
    edit(&mut b, |n| {
        n.frame.y -= 137.0;
        n.frame.x += 3.0;
        n.visible = !n.visible;
        n.hittable = !n.hittable;
        n.focused = !n.focused;
    });
    assert!(same(&a, &b));
}

/// What compaction drops still counts: the 13th text of a row (past `MAX_MERGED_PARTS`),
/// an unlabeled image's identifier, a placeholder, an identifier, a hidden node.
#[test]
fn what_compaction_drops_is_in_the_digest() {
    let a = row(12, "Ship to Alex Home");
    let b = row(12, "Ship to Mallory Depot");
    assert_eq!(
        compact(&a),
        compact(&b),
        "compaction cannot tell them apart"
    );
    assert!(!same(&a, &b));

    let img = |id: &str| {
        app_screen(vec![
            UiNode::new(Role::Image, rect(16.0, 100.0, 40.0, 40.0)).with_identifier(id),
            button("Pay", 16.0, 700.0, 361.0, 50.0),
        ])
    };
    assert_eq!(compact(&img("visa")), compact(&img("amex")));
    assert!(!same(&img("visa"), &img("amex")));

    let field = |placeholder: &str| {
        app_screen(vec![
            UiNode::new(Role::TextField, rect(16.0, 100.0, 361.0, 44.0))
                .with_label("Amount")
                .with_placeholder(placeholder),
            button("Pay", 16.0, 700.0, 361.0, 50.0),
        ])
    };
    assert!(!same(&field("USD"), &field("EUR")));

    let hidden = |label: &str| {
        app_screen(vec![
            text(label, rect(16.0, 2000.0, 300.0, 18.0)).visible(false),
            button("Pay", 16.0, 700.0, 361.0, 50.0),
        ])
    };
    assert!(!same(&hidden("Fee $1"), &hidden("Fee $9")));
    // States and structure count too.
    let mut b = row(2, "x");
    edit(&mut b, |n| {
        if n.label.as_deref() == Some("Place order") {
            n.enabled = false;
        }
    });
    assert!(!same(&row(2, "x"), &b));
    let mut b = row(2, "x");
    edit(&mut b, |n| {
        if n.label.as_deref() == Some("Place order") {
            n.selected = true;
        }
    });
    assert!(!same(&row(2, "x"), &b));
    let mut b = row(2, "x");
    b.app_bundle_id = "com.example.other".to_owned();
    assert!(!same(&row(2, "x"), &b));
}

/// The same texts in another nesting differ (the tree's shape is part of the input).
#[test]
fn the_tree_shape_counts() {
    let flat = app_screen(vec![label("A", 0.0, 100.0), label("B", 0.0, 130.0)]);
    let nested = app_screen(vec![
        label("A", 0.0, 100.0).with_children(vec![label("B", 0.0, 130.0)])
    ]);
    assert!(!same(&flat, &nested));
}

/// A secure field's value never reaches the digest input; its presence, role and label do.
#[test]
fn a_secure_value_is_never_hashed() {
    let login = |pw: &str| {
        app_screen(vec![
            UiNode::new(Role::SecureTextField, rect(16.0, 100.0, 361.0, 44.0))
                .with_label("Password")
                .with_value(pw),
            button("Sign in", 16.0, 700.0, 361.0, 50.0),
        ])
    };
    let a = login("hunter2-secret");
    assert!(same(&a, &login("other-secret")));
    let input = screen_digest_input(&a);
    let needle = b"hunter2-secret";
    assert!(!input.windows(needle.len()).any(|w| w == needle));
    assert!(input.windows(8).any(|w| w == b"Password"));
    // A field flagged secure by its flag only is treated the same.
    let flagged = |pw: &str| {
        app_screen(vec![UiNode::new(
            Role::TextField,
            rect(16.0, 100.0, 361.0, 44.0),
        )
        .with_label("PIN")
        .with_value(pw)
        .secure(true)])
    };
    assert!(same(&flagged("1234"), &flagged("9999")));
    // The field appearing or disappearing counts.
    assert!(!same(
        &a,
        &app_screen(vec![button("Sign in", 16.0, 700.0, 361.0, 50.0)])
    ));
}

/// The real status bar (by role) ticks freely; the same text elsewhere does not.
#[test]
fn the_status_bar_is_left_out_by_role() {
    let a = fixture("springboard_home");
    let mut b = a.clone();
    edit(&mut b, |n| {
        if n.label.as_deref() == Some("7:53 AM") {
            n.label = Some("7:54 AM".to_owned());
        }
        if n.label.as_deref() == Some("100% battery power") {
            n.label = Some("99% battery power".to_owned());
        }
    });
    assert_ne!(compact(&a), compact(&b));
    assert!(same(&a, &b));
    // A clock that is an ordinary text in the app (no status-bar role) counts.
    let clock = |t: &str| {
        app_screen(vec![
            label(t, 16.0, 14.0),
            button("Pay", 16.0, 700.0, 361.0, 50.0),
        ])
    };
    assert!(!same(&clock("9:41"), &clock("9:42")));
}

/// On SpringBoard an icon's badge (after the first comma) is left out; its name is not.
/// Elsewhere the same label counts whole.
#[test]
fn springboard_badges_are_left_out_but_names_are_not() {
    let home =
        |mail: &str| {
            let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![
                UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![
                    node(Role::Button, mail, rect(20.0, 100.0, 70.0, 90.0)),
                    node(Role::Button, "Settings", rect(110.0, 100.0, 70.0, 90.0)),
                ]),
            ]);
            screen("com.apple.springboard", "Home", root, false)
        };
    assert!(same(&home("Mail, 3 unread"), &home("Mail, 4 unread")));
    assert!(same(&home("Mail"), &home("Mail, 1 unread")));
    assert!(!same(&home("Mail, 3 unread"), &home("Mall, 3 unread")));
    let app = |l: &str| app_screen(vec![button(l, 20.0, 100.0, 70.0, 90.0)]);
    assert!(!same(&app("Mail, 3 unread"), &app("Mail, 4 unread")));
}

/// Keys and typing predictions change on their own and are left out; the Return key's
/// label (what Return does) is not.
#[test]
fn the_keyboard_and_its_prediction_bar_are_left_out_but_not_the_return_key() {
    let a = fixture("login_keyboard");
    let mut b = a.clone();
    edit(&mut b, |n| {
        if n.label.as_deref() == Some("Passwords") {
            n.label = Some("Hello".to_owned());
        }
        if n.label.as_deref() == Some("q") {
            n.label = Some("Q".to_owned());
        }
    });
    assert!(same(&a, &b));
    let mut c = a.clone();
    edit(&mut c, |n| {
        if n.identifier.as_deref() == Some("Return") {
            n.label = Some("send".to_owned());
        }
    });
    assert!(!same(&a, &c));
    // Without a keyboard in the tree the identifier hides nothing.
    let bar = |l: &str| {
        app_screen(vec![UiNode::new(
            Role::Other("Other".into()),
            rect(0.0, 600.0, 393.0, 44.0),
        )
        .with_identifier("SystemInputAssistantView")
        .with_children(vec![button(l, 0.0, 600.0, 393.0, 44.0)])])
    };
    assert!(!same(&bar("Pay $5"), &bar("Pay $500")));
}

/// A scroll indicator's position is left out (its label and page count are not); a node
/// that only looks a bit like one is not.
#[test]
fn scroll_indicator_positions_are_left_out() {
    let a = fixture("long_list_scrolled");
    let mut b = a.clone();
    edit(&mut b, |n| {
        if n.label.as_deref() == Some("Vertical scroll bar, 6 pages") {
            n.value = Some("47%".to_owned());
        }
    });
    assert!(same(&a, &b));
    let mut c = a.clone();
    edit(&mut c, |n| {
        if n.label.as_deref() == Some("Vertical scroll bar, 6 pages") {
            n.label = Some("Vertical scroll bar, 7 pages".to_owned());
        }
    });
    assert!(!same(&a, &c));
    let fake = |v: &str| {
        app_screen(vec![UiNode::new(
            Role::Other("Other".into()),
            rect(0.0, 100.0, 30.0, 300.0),
        )
        .with_label("Vertical scroll bar, 2 pages")
        .with_value(v)])
    };
    assert!(same(&fake("10%"), &fake("90%")));
    assert!(!same(&fake("$10"), &fake("$90")));
}

/// Runner-text observations: the digest is taken of whatever tree the text parses to.
#[test]
fn runner_text_trees_are_digested_like_typed_trees() {
    let text_tree = |price: &str| {
        format!(
            "Application, label: 'Shop', {{{{0, 0}}, {{393, 852}}}}\n  Window, {{{{0, 0}}, {{393, 852}}}}\n    StaticText, label: 'Total {price}', {{{{16, 200}}, {{300, 20}}}}\n    Image, id: 'card-visa', {{{{16, 240}}, {{40, 20}}}}\n    Button, label: 'Pay', {{{{16, 700}}, {{361, 50}}}}\n"
        )
    };
    let parse = |t: &str| {
        let root = parse_runner_text(t).unwrap().root;
        screen("com.example.shop", "Shop", root, false)
    };
    let a = parse(&text_tree("$5.00"));
    assert!(same(&a, &parse(&text_tree("$5.00"))));
    assert!(!same(&a, &parse(&text_tree("$500.00"))));
    let swapped = text_tree("$5.00").replace("card-visa", "card-amex");
    assert!(!same(&a, &parse(&swapped)));
}

/// Linear and cheap: a 5,000-node tree is digested well within a generous bound in a debug
/// build.
#[test]
fn the_digest_of_a_huge_tree_is_cheap() {
    let kids: Vec<UiNode> = (0..5000u32)
        .map(|i| {
            UiNode::new(Role::Cell, Rect::new(0.0, f64::from(i) * 44.0, 393.0, 44.0))
                .with_label(format!("Row {i} with a description, $1{i}.00"))
                .with_value(format!("value {i}"))
                .with_identifier(format!("row.{i}"))
        })
        .collect();
    let s = app_screen(kids);
    assert!(s.root.count() > 5000);
    let t = Instant::now();
    let d = screen_digest(&s);
    let took = t.elapsed();
    println!("5,000 nodes: digest {took:?}");
    assert_eq!(d, screen_digest(&s));
    assert!(took < Duration::from_millis(1000), "{took:?}");
}
