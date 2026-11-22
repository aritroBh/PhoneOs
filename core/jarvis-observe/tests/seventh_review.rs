#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Seventh review, the observation side: the layout is in the digest (2); inside the
//! keyboard and its input assistant only structure, shown bits and frames are hashed (and
//! the Return key's label), and what is not hashed is never listed (3); the raw nodes a
//! touch could land on, and the bars, for the tap-point rule (1).

mod common;

use common::ios::fixture;
use common::*;
use jarvis_observe::{compact, compact_view, screen_digest};
use jarvis_proto::fixtures::{node, rect, screen, text};
use jarvis_proto::{Role, Screen, UiNode};

fn same(a: &Screen, b: &Screen) -> bool {
    screen_digest(a) == screen_digest(b)
}

/// What a keyboard's window looks like on iOS 26, in small: the input assistant with three
/// candidates above a keyboard holding keys, shift and dictation, beside the app's own
/// accessory bar ("Send $40.00 to Alice").
struct Kb<'a> {
    candidates: &'a [(&'a str, f64)],
    shift: &'a str,
    dictation: &'a str,
    q_at: f64,
    accessory: &'a str,
    extra_key: bool,
}

impl Default for Kb<'_> {
    fn default() -> Self {
        Kb {
            candidates: &[("\u{201c}Ada\u{201d}", 0.0), ("I'm", 131.0), ("The", 262.0)],
            shift: "shift",
            dictation: "Dictate",
            q_at: 3.0,
            accessory: "Send $40.00 to Alice",
            extra_key: false,
        }
    }
}

fn kb_screen(k: &Kb<'_>) -> Screen {
    let candidates: Vec<UiNode> = k
        .candidates
        .iter()
        .map(|(l, x)| {
            UiNode::new(Role::Other("Other".into()), rect(*x, 472.0, 131.0, 44.0)).with_label(*l)
        })
        .collect();
    let assistant = UiNode::new(Role::Other("Other".into()), rect(0.0, 472.0, 393.0, 44.0))
        .with_identifier("SystemInputAssistantView")
        .with_children(vec![UiNode::new(
            Role::Other("Other".into()),
            rect(0.0, 472.0, 393.0, 44.0),
        )
        .with_label("Typing Predictions")
        .with_children(candidates)]);
    let mut keys = vec![
        node(Role::Key, "q", rect(k.q_at, 527.0, 37.0, 46.0)),
        node(Role::Button, k.shift, rect(3.0, 635.0, 46.0, 46.0)).with_identifier("shift"),
        node(Role::Button, k.dictation, rect(50.0, 689.0, 46.0, 46.0)).with_identifier("dictation"),
        node(Role::Button, "send", rect(297.0, 689.0, 93.0, 46.0)).with_identifier("Return"),
    ];
    if k.extra_key {
        keys.push(node(Role::Key, "w", rect(42.0, 527.0, 37.0, 46.0)));
    }
    let keyboard = UiNode::new(Role::Keyboard, rect(0.0, 516.0, 393.0, 336.0)).with_children(keys);
    let accessory = UiNode::new(Role::Other("Other".into()), rect(0.0, 420.0, 393.0, 50.0))
        .with_children(vec![node(
            Role::Button,
            k.accessory,
            rect(16.0, 423.0, 361.0, 44.0),
        )]);
    let app = UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![
        text("To: Alice", rect(16.0, 80.0, 300.0, 40.0)),
        UiNode::new(Role::TextField, rect(16.0, 300.0, 361.0, 44.0))
            .with_label("Message")
            .focused(true),
    ]);
    let kbwin = UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
        .with_children(vec![accessory, assistant, keyboard]);
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Chat")
        .with_children(vec![app, kbwin]);
    screen("com.example.chat", "Chat", root, true)
}

/// Prediction text, the shift key's label and the dictation key's label change on their
/// own: the digest stays equal.
#[test]
fn keyboard_text_changing_keeps_the_digest() {
    let a = kb_screen(&Kb::default());
    for b in [
        Kb {
            candidates: &[
                ("\u{201c}Ad\u{201d}", 0.0),
                ("Adam", 131.0),
                ("Ada's", 262.0),
            ],
            ..Kb::default()
        },
        Kb {
            shift: "Shift",
            ..Kb::default()
        },
        Kb {
            dictation: "Stop dictation",
            ..Kb::default()
        },
    ] {
        assert!(same(&a, &kb_screen(&b)));
    }
}

/// A key or a candidate that appears, vanishes or moves changes the digest; so does the
/// Return key's label (a cue).
#[test]
fn keyboard_structure_and_geometry_change_the_digest() {
    let a = kb_screen(&Kb::default());
    let changed = [
        Kb {
            extra_key: true,
            ..Kb::default()
        },
        Kb {
            q_at: 10.0,
            ..Kb::default()
        },
        Kb {
            candidates: &[("\u{201c}Ada\u{201d}", 0.0), ("I'm", 131.0)],
            ..Kb::default()
        },
        Kb {
            candidates: &[("\u{201c}Ada\u{201d}", 0.0), ("I'm", 140.0), ("The", 262.0)],
            ..Kb::default()
        },
    ];
    for b in changed {
        assert!(!same(&a, &kb_screen(&b)));
    }
    let mut go = kb_screen(&Kb::default());
    fn relabel(n: &mut UiNode) {
        if n.identifier.as_deref() == Some("Return") {
            n.label = Some("go".to_owned());
        }
        n.children.iter_mut().for_each(relabel);
    }
    relabel(&mut go.root);
    assert!(!same(&a, &go));
    assert_eq!(compact(&go).cues.return_key.as_deref(), Some("go"));
}

/// The app's own accessory bar next to the keyboard is ordinary content: fully hashed and
/// listed. The keys and the candidates are not listed.
#[test]
fn the_accessory_bar_is_content_and_the_keyboard_text_is_not_listed() {
    let a = kb_screen(&Kb::default());
    let b = kb_screen(&Kb {
        accessory: "Send $4,000.00 to Mallory",
        ..Kb::default()
    });
    assert!(!same(&a, &b));
    let c = compact(&a);
    let labels = labels(&c);
    assert!(labels.contains(&"Send $40.00 to Alice"), "{labels:?}");
    for gone in [
        "\u{201c}Ada\u{201d}",
        "I'm",
        "The",
        "Typing Predictions",
        "q",
        "shift",
        "Dictate",
        "send",
    ] {
        assert!(!labels.contains(&gone), "{gone} listed: {labels:?}");
    }
    // The raw nodes there carry no label, and are part of the keyboard.
    let view = compact_view(&a);
    let kb_nodes: Vec<_> = view.nodes.iter().filter(|n| n.keyboard).collect();
    assert!(kb_nodes.iter().any(|n| n.role == Role::Key));
    assert!(view
        .nodes
        .iter()
        .filter(|n| n.role == Role::Key || n.label.as_deref() == Some("I'm"))
        .all(|n| n.keyboard && n.label.is_none()));
}

/// The identifier has an effect only in a keyboard's top-level window and outside a web
/// view: elsewhere its subtree is hashed and listed whole.
#[test]
fn the_assistant_identifier_elsewhere_is_fully_hashed() {
    let wrapped = |amount: &str, in_web: bool, with_keyboard: bool| {
        let inner = UiNode::new(Role::Other("Other".into()), rect(0.0, 400.0, 393.0, 60.0))
            .with_identifier("SystemInputAssistantView")
            .with_children(vec![node(
                Role::Button,
                &format!("Send {amount} to Alice"),
                rect(16.0, 405.0, 361.0, 44.0),
            )]);
        let content = if in_web {
            UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 700.0)).with_children(vec![inner])
        } else {
            inner
        };
        let mut windows =
            vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![content])];
        if with_keyboard {
            windows.push(
                UiNode::new(Role::Window, rect(0.0, 516.0, 393.0, 336.0)).with_children(vec![kb()]),
            );
        }
        let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(windows);
        screen("com.example.app", "App", root, with_keyboard)
    };
    for (in_web, kb) in [(false, false), (false, true), (true, false)] {
        let a = wrapped("$40.00", in_web, kb);
        assert!(
            !same(&a, &wrapped("$4,000.00", in_web, kb)),
            "web {in_web} kb {kb}"
        );
        assert!(labels(&compact(&a)).contains(&"Send $40.00 to Alice"));
    }
    // In the keyboard's own window and outside a web view it applies (in a web view there,
    // it does not).
    let in_kb_window =
        |amount: &str, in_web: bool| {
            let inner = UiNode::new(Role::Other("Other".into()), rect(0.0, 470.0, 393.0, 44.0))
                .with_identifier("SystemInputAssistantView")
                .with_children(vec![node(
                    Role::Button,
                    &format!("Send {amount} to Alice"),
                    rect(16.0, 470.0, 361.0, 44.0),
                )]);
            let content = if in_web {
                UiNode::new(Role::WebView, rect(0.0, 400.0, 393.0, 116.0))
                    .with_children(vec![inner])
            } else {
                inner
            };
            let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
                .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
                    .with_children(vec![content, kb()])]);
            screen("com.example.app", "App", root, true)
        };
    assert!(same(
        &in_kb_window("$40.00", false),
        &in_kb_window("$4,000.00", false)
    ));
    assert!(!labels(&compact(&in_kb_window("$40.00", false))).contains(&"Send $40.00 to Alice"));
    assert!(!same(
        &in_kb_window("$40.00", true),
        &in_kb_window("$4,000.00", true)
    ));
    assert!(labels(&compact(&in_kb_window("$40.00", true))).contains(&"Send $40.00 to Alice"));
}

/// The iOS 26 keyboard fixture: the prediction bar's "Passwords" button and the keys are
/// not listed; the Emoji and Dictate buttons sit in the keyboard's window but outside the
/// `Keyboard` node and the assistant, so they are hashed whole (a change of their label
/// changes the digest) and not listed (they are below the keyboard's top, so not shown).
#[test]
fn the_ios26_keyboard_fixture_lists_and_hashes() {
    let a = fixture("login_keyboard");
    let c = compact(&a);
    assert_eq!(
        labels(&c),
        [
            "Playground",
            "Login form",
            "Username",
            "Password",
            "Sign In",
            "Signed out"
        ]
    );
    let mut b = a.clone();
    fn relabel(n: &mut UiNode, from: &str, to: &str) {
        if n.label.as_deref() == Some(from) {
            n.label = Some(to.to_owned());
        }
        n.children.iter_mut().for_each(|c| relabel(c, from, to));
    }
    relabel(&mut b.root, "Dictate", "Stop");
    assert!(!same(&a, &b));
    let mut p = a.clone();
    relabel(&mut p.root, "Passwords", "Ada");
    relabel(&mut p.root, "shift", "Shift");
    assert!(same(&a, &p));
}

/// A tab bar inside web content (WebKit's ARIA `tablist`) is not a bar for precedence;
/// the app's own tab bar is.
#[test]
fn a_tab_bar_inside_a_web_view_is_not_a_bar() {
    let s = app_screen(vec![
        UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 700.0)).with_children(vec![
            UiNode::new(Role::TabBar, rect(0.0, 400.0, 393.0, 60.0))
                .with_children(vec![button("Overview", 0.0, 400.0, 196.0, 60.0)]),
        ]),
        UiNode::new(Role::TabBar, rect(0.0, 803.0, 393.0, 49.0))
            .with_children(vec![button("Home", 0.0, 803.0, 196.0, 49.0)]),
    ]);
    let v = compact_view(&s);
    assert_eq!(v.bars.len(), 1);
    assert_eq!(v.bars[0].frame.y, 803.0);
    for role in [Role::ScrollView, Role::Table, Role::CollectionView] {
        let s = app_screen(vec![UiNode::new(role, rect(0.0, 0.0, 393.0, 852.0))
            .with_children(vec![UiNode::new(
                Role::NavigationBar,
                rect(0.0, 50.0, 393.0, 44.0),
            )])]);
        assert!(compact_view(&s).bars.is_empty());
    }
}

/// Every drawn raw node with an on-screen part is listed for the tap rule, in node order,
/// clipped to the screen; pruned subtrees are not.
#[test]
fn drawn_nodes_are_listed_with_their_on_screen_frames() {
    let mut hidden = button("Hidden", 16.0, 200.0, 100.0, 44.0);
    hidden.visible = false;
    let s = app_screen(vec![
        UiNode::new(Role::Image, rect(-50.0, 100.0, 200.0, 100.0)),
        hidden,
        button("Pay", 16.0, 300.0, 361.0, 50.0),
    ]);
    let v = compact_view(&s);
    let roles: Vec<&str> = v.nodes.iter().map(|n| n.role.as_str()).collect();
    assert_eq!(roles, ["application", "window", "image", "button"]);
    assert_eq!(v.nodes[2].frame, rect(0.0, 100.0, 150.0, 100.0));
    assert!(v.nodes.windows(2).all(|w| w[0].node < w[1].node));
    assert_eq!(v.nodes[3].label.as_deref(), Some("Pay"));
}
