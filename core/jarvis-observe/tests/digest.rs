#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! The exact digest of the raw tree (fourth review): what an approval is checked against.
//! Everything the runner reports counts (also what compaction drops), frames included
//! (seventh review, rounded to whole points), but for the documented exclusions: focus,
//! `hittable`, secure values, SpringBoard's status bar, SpringBoard badges, scroll-indicator
//! positions, and the text inside the keyboard and its input assistant (seventh review: their
//! structure and geometry are hashed, and the Return key's label).

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

/// `hittable` and `focus` are not hashed; what is shown is (fifth review: the digest covers
/// the effective visibility compaction uses, so a scroll that moves a row out of the shown
/// set, or a hidden panel that becomes shown, changes it), and so are frames, rounded to
/// whole points (seventh review: was "equal trees with different frames are equal"; a frame
/// that moves now changes the digest, deliberately).
#[test]
fn frames_are_hashed_to_whole_points_hittable_and_focus_are_not() {
    let a = fixture("long_list_scrolled");
    let mut b = a.clone();
    edit(&mut b, |n| {
        n.hittable = !n.hittable;
        n.focused = !n.focused;
    });
    assert!(same(&a, &b));
    // Less than the rounding moves nothing; a point moves the digest.
    let nudge = |dx: f64| {
        let mut s = a.clone();
        edit(&mut s, |n| {
            if n.label.as_deref() == Some("Item 9") {
                n.frame.x += dx;
            }
        });
        s
    };
    assert!(same(&a, &nudge(0.2)));
    assert!(!same(&a, &nudge(1.0)));
    let mut f = a.clone();
    edit(&mut f, |n| n.frame.x += 1.0);
    assert!(!same(&a, &f));
    // A big scroll moves rows out of (and into) the shown set.
    let mut c = a.clone();
    edit(&mut c, |n| n.frame.y -= 137.0);
    assert!(!same(&a, &c));
    // So does flipping the runner's visibility flag of shown nodes.
    let mut d = a.clone();
    edit(&mut d, |n| n.visible = !n.visible);
    assert!(!same(&a, &d));
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
    // Excluded everywhere: not hashed and not listed either (fifth review).
    assert_eq!(compact(&a), compact(&b));
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

/// On SpringBoard an app icon's badge (after the first comma) is left out; its name is
/// not. Only the icon role gets the cut (fifth review): any other node, also on
/// SpringBoard, and the same label in an app count whole.
#[test]
fn springboard_badges_are_left_out_but_names_are_not() {
    let home_with =
        |role: Role, mail: &str| {
            let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![
                UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(vec![
                    node(role.clone(), mail, rect(20.0, 100.0, 70.0, 90.0)),
                    node(role, "Settings", rect(110.0, 100.0, 70.0, 90.0)),
                ]),
            ]);
            screen("com.apple.springboard", "Home", root, false)
        };
    let home = |mail: &str| home_with(Role::Other("Icon".into()), mail);
    assert!(same(&home("Mail, 3 unread"), &home("Mail, 4 unread")));
    assert!(same(&home("Mail"), &home("Mail, 1 unread")));
    assert!(!same(&home("Mail, 3 unread"), &home("Mall, 3 unread")));
    let buttons = |mail: &str| home_with(Role::Button, mail);
    assert!(!same(
        &buttons("Mail, 3 unread"),
        &buttons("Mail, 4 unread")
    ));
    let app = |l: &str| app_screen(vec![button(l, 20.0, 100.0, 70.0, 90.0)]);
    assert!(!same(&app("Mail, 3 unread"), &app("Mail, 4 unread")));
}

/// Seventh review: inside the keyboard and inside the input assistant next to it only
/// structure, shown bits and frames are hashed (and the Return key's label), so a changing
/// prediction or shift label keeps an approval; nothing can appear, vanish or move there
/// unnoticed. Was (sixth review): their text hashed too (deliberate change). Anywhere else
/// the identifier changes nothing.
#[test]
fn keyboard_text_is_left_out_but_its_return_key_and_geometry_are_not() {
    let a = fixture("login_keyboard");
    let mut b = a.clone();
    edit(&mut b, |n| {
        if n.label.as_deref() == Some("Passwords") {
            n.label = Some("Hello".to_owned());
        }
    });
    assert!(same(&a, &b), "the prediction bar's text is left out");
    let mut k = a.clone();
    edit(&mut k, |n| {
        if n.label.as_deref() == Some("q") {
            n.label = Some("Q".to_owned());
        }
    });
    assert!(same(&a, &k), "the keys' labels are left out");
    let mut m = a.clone();
    edit(&mut m, |n| {
        if n.label.as_deref() == Some("q") {
            n.frame.y += 4.0;
        }
    });
    assert!(!same(&a, &m), "a key that moves is not");
    let mut c = a.clone();
    edit(&mut c, |n| {
        if n.identifier.as_deref() == Some("Return") {
            n.label = Some("send".to_owned());
        }
    });
    assert!(!same(&a, &c), "the Return key's label is hashed");
    assert_eq!(compact(&c).cues.return_key.as_deref(), Some("send"));
    // The identifier hides nothing, keyboard or not.
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

/// Fifth review probe: a pathological 200,000-deep typed tree (an FFI `Observed` is not
/// capped) is digested and compacted on a 2 MiB stack.
#[test]
fn a_200k_deep_tree_does_not_overflow() {
    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let mut n = UiNode::new(Role::Button, Rect::new(0.0, 0.0, 10.0, 10.0)).with_label("x");
            for _ in 0..200_000 {
                n = UiNode::new(Role::Other("Other".into()), Rect::new(0.0, 0.0, 10.0, 10.0))
                    .with_children(vec![n]);
            }
            let s = screen("com.example.app", "App", n, false);
            let _ = screen_digest(&s);
            let _ = compact(&s);
            let _ = jarvis_observe::compact_view(&s);
            std::mem::forget(s); // dropping is a separate question
        })
        .unwrap();
    assert!(handle.join().is_ok());
}

/// Fifth review: a `StatusBar` container deep inside an app (with a button and an amount)
/// is ordinary content: hashed and listed, and a change in it changes the digest. Only on
/// SpringBoard, directly under a top-level window, is it excluded.
#[test]
fn a_status_bar_role_inside_an_app_is_hashed_and_listed() {
    let bar = |amount: &str| {
        UiNode::new(
            Role::Other("StatusBar".into()),
            rect(0.0, 150.0, 393.0, 400.0),
        )
        .with_children(vec![
            text(&format!("Send {amount}"), rect(16.0, 200.0, 361.0, 30.0)),
            button("Confirm", 16.0, 400.0, 361.0, 50.0),
        ])
    };
    let app = |amount: &str| app_screen(vec![other(vec![bar(amount)])]);
    assert!(!same(&app("$5.00"), &app("$5,000.00")));
    let c = compact(&app("$5.00"));
    assert!(labels(&c).contains(&"Send $5.00"), "{:?}", labels(&c));
    assert!(labels(&c).contains(&"Confirm"));
    // As a direct child of a top-level window: still content outside SpringBoard...
    let top =
        |bundle: &str, amount: &str| {
            let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
                .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
                    .with_children(vec![bar(amount)])]);
            screen(bundle, "X", root, false)
        };
    assert!(!same(
        &top("com.example.app", "$5.00"),
        &top("com.example.app", "$5,000.00")
    ));
    // ...and excluded (not hashed, not listed) on SpringBoard.
    let sb = "com.apple.springboard";
    assert!(same(&top(sb, "$5.00"), &top(sb, "$5,000.00")));
    assert!(compact(&top(sb, "$5.00")).elements.is_empty());
}

/// The iOS 26 keyboard lives in a top-level window of its own, with the prediction /
/// autofill bar (`SystemInputAssistantView`) above it and the emoji and dictation buttons
/// below it. The whole window is indexed and its structure hashed; seventh review: the
/// assistant's text is not hashed, so its "Passwords" button is no longer listed (was:
/// listed, sixth review; deliberate change), the keys are not listed, the emoji and
/// dictation buttons (below the keyboard's top) are not shown, and the Return key is
/// still a cue.
#[test]
fn the_ios26_keyboard_window_is_hashed_and_its_bar_listed() {
    let s = fixture("login_keyboard");
    let view = jarvis_observe::compact_view(&s);
    let keyboard_window = s.root.children.last().unwrap();
    assert!(keyboard_window.iter().any(|n| n.role == Role::Keyboard));
    assert_eq!(view.parents.len(), s.root.count(), "every node is indexed");
    assert!(!labels(&view.screen).contains(&"Passwords"));
    assert_eq!(
        labels(&view.screen),
        [
            "Playground",
            "Login form",
            "Username",
            "Password",
            "Sign In",
            "Signed out"
        ],
        "the tab bar is under the keyboard; the assistant's button is not listed"
    );
    for gone in [
        "Emoji", "Dictate", "q", "shift", "delete", "return", "numbers",
    ] {
        assert!(!labels(&view.screen).contains(&gone), "{gone} listed");
    }
    assert!(view.screen.elements.iter().all(|e| e.role != Role::Key));
    assert_eq!(view.screen.cues.return_key.as_deref(), Some("return"));
    assert!(view.screen.keyboard_visible);
    // Each listed element names a raw node, in range.
    assert_eq!(view.sources.len(), view.screen.elements.len());
    assert!(view
        .sources
        .iter()
        .all(|i| (*i as usize) < view.parents.len()));
}

// ---------------------------------------------------------------------------------------
// Digest v6: order-free runs (CI run of df4a416). Within one parent, a maximal run of
// consecutive sibling subtrees in which every node is hidden and area-less (and none is a
// text input, a secure node, a labelled text or image, a keyboard or a key, or in the
// keyboard or its input assistant) is hashed as a sorted multiset; everything else keeps
// its order.
// ---------------------------------------------------------------------------------------

/// A hidden, area-less node.
fn ghost(role: Role, label: &str) -> UiNode {
    UiNode::new(role, Rect::default())
        .with_label(label)
        .with_identifier(label)
        .visible(false)
}

fn gb(label: &str) -> UiNode {
    ghost(Role::Button, label)
}

/// An app screen whose window holds a shown button and a hidden, area-less wrapper with
/// `kids`.
fn wrapped(kids: Vec<UiNode>) -> Screen {
    app_screen(vec![
        button("Pay", 16.0, 700.0, 361.0, 50.0),
        UiNode::new(Role::Button, Rect::default())
            .visible(false)
            .with_children(kids),
    ])
}

/// The simulator's case: a hidden, area-less folder whose two hidden, area-less children
/// come back in the other order. Nested reorders inside an order-free subtree are equal
/// too, at any depth.
#[test]
fn a_reorder_inside_an_order_free_run_keeps_the_digest() {
    let folder = |kids: Vec<UiNode>| gb("Utilities folder").with_children(kids);
    let a = wrapped(vec![
        gb("Fitness"),
        folder(vec![gb("Passwords"), gb("Shortcuts")]),
        gb("Jarvis"),
    ]);
    let b = wrapped(vec![
        gb("Fitness"),
        folder(vec![gb("Shortcuts"), gb("Passwords")]),
        gb("Jarvis"),
    ]);
    assert!(same(&a, &b), "the folder's children swapped");
    let c = wrapped(vec![
        gb("Jarvis"),
        gb("Fitness"),
        folder(vec![gb("Shortcuts"), gb("Passwords")]),
    ]);
    assert!(same(&a, &c), "the run's members in any order");
    // Deeper: grandchildren of a member.
    let deep = |swap: bool| {
        let mut g = vec![gb("x"), gb("y"), ghost(Role::Other("Other".into()), "")];
        if swap {
            g.rotate_left(1);
        }
        wrapped(vec![gb("p").with_children(vec![gb("q").with_children(g)])])
    };
    assert!(same(&deep(false), &deep(true)));
    // Unlabelled texts and images are order-free (they lend no words).
    let plain = |swap: bool| {
        let mut k = vec![
            UiNode::new(Role::StaticText, Rect::default()).visible(false),
            UiNode::new(Role::Image, Rect::default())
                .with_identifier("chevron")
                .visible(false),
        ];
        if swap {
            k.reverse();
        }
        wrapped(k)
    };
    assert!(same(&plain(false), &plain(true)));
    // The input never holds a member's text verbatim, only its subtree hash.
    let input = screen_digest_input(&a);
    assert!(!input.windows(9).any(|w| w == b"Passwords"));
}

/// Any hashed field of any node in a run (the member or a descendant) changing changes the
/// digest.
#[test]
fn a_content_change_inside_a_run_changes_the_digest() {
    let base = || {
        wrapped(vec![
            gb("Fitness"),
            gb("Utilities folder").with_children(vec![gb("Passwords"), gb("Shortcuts")]),
        ])
    };
    let a = base();
    type Change = (&'static str, fn(&mut UiNode));
    let changes: [Change; 7] = [
        ("label", |n| n.label = Some("Pass".into())),
        ("value", |n| n.value = Some("1".into())),
        ("identifier", |n| n.identifier = Some("P".into())),
        ("placeholder", |n| n.placeholder = Some("P".into())),
        ("role", |n| n.role = Role::Link),
        ("enabled", |n| n.enabled = !n.enabled),
        ("selected", |n| n.selected = !n.selected),
    ];
    for target in ["Fitness", "Passwords"] {
        for (what, f) in &changes {
            let mut b = base();
            edit(&mut b, |n| {
                if n.label.as_deref() == Some(target) {
                    f(n);
                }
            });
            assert!(!same(&a, &b), "{what} of {target}");
        }
    }
    // Ignored fields stay ignored inside a run too.
    let mut b = base();
    edit(&mut b, |n| {
        n.hittable = !n.hittable;
        n.focused = !n.focused;
    });
    assert!(same(&a, &b));
}

/// Adding, removing or duplicating a member, or a member's child, changes the digest (a
/// multiset, not a set).
#[test]
fn structure_inside_a_run_counts_duplicates_included() {
    let s = |names: &[&str]| wrapped(names.iter().map(|n| gb(n)).collect());
    assert!(same(&s(&["A", "B"]), &s(&["B", "A"])));
    assert!(!same(&s(&["A", "B"]), &s(&["A", "B", "C"])), "added");
    assert!(!same(&s(&["A", "B"]), &s(&["A"])), "removed");
    assert!(!same(&s(&["A", "B"]), &s(&["A", "A", "B"])), "duplicated");
    assert!(
        !same(&s(&["A", "A", "B"]), &s(&["A", "B", "B"])),
        "multiset"
    );
    assert!(same(&s(&["A", "A", "B"]), &s(&["A", "B", "A"])));
    // A member gaining a child, or a child moving to another member.
    let t = |a: Vec<UiNode>, b: Vec<UiNode>| {
        wrapped(vec![gb("A").with_children(a), gb("B").with_children(b)])
    };
    assert!(!same(&t(vec![], vec![]), &t(vec![gb("x")], vec![])));
    assert!(!same(&t(vec![gb("x")], vec![]), &t(vec![], vec![gb("x")])));
    assert!(same(
        &t(vec![gb("x")], vec![]),
        &wrapped(vec![gb("B"), gb("A").with_children(vec![gb("x")])])
    ));
}

/// Runs are maximal and consecutive: a member moving across a sibling that is not
/// order-free (shown, framed, hidden but framed) changes the digest, and so does moving
/// to another parent or depth.
#[test]
fn a_run_never_spans_a_sibling_that_keeps_its_order() {
    let shown = || button("Pay", 16.0, 600.0, 361.0, 50.0);
    let framed_hidden =
        || UiNode::new(Role::Other("Other".into()), rect(500.0, 0.0, 100.0, 100.0)).visible(false);
    let visible_arealess = || UiNode::new(Role::Button, Rect::default()).with_label("v");
    for (what, sep) in [
        ("shown", shown as fn() -> UiNode),
        ("hidden with a frame", framed_hidden),
        ("visible without area", visible_arealess),
    ] {
        let a = wrapped(vec![gb("A"), sep(), gb("B")]);
        let b = wrapped(vec![gb("B"), sep(), gb("A")]);
        assert!(!same(&a, &b), "across a {what} sibling");
        let c = wrapped(vec![sep(), gb("A"), gb("B")]);
        assert!(!same(&a, &c), "moved across a {what} sibling");
        // Inside each side of it, order is free.
        let d = wrapped(vec![gb("A"), gb("C"), sep(), gb("B")]);
        let e = wrapped(vec![gb("C"), gb("A"), sep(), gb("B")]);
        assert!(same(&d, &e), "{what}");
    }
    // Another parent, another depth.
    let a = wrapped(vec![gb("A"), gb("B").with_children(vec![gb("C")])]);
    let b = wrapped(vec![gb("A").with_children(vec![gb("C")]), gb("B")]);
    assert!(!same(&a, &b), "another parent");
    let c = wrapped(vec![gb("A"), gb("B"), gb("C")]);
    assert!(!same(&a, &c), "another depth");
    // Shown siblings keep their order even with identical frames (the compact screen
    // cannot tell them apart).
    let twins = |swap: bool| {
        let mut k = vec![
            button("Pay", 16.0, 600.0, 361.0, 50.0),
            button("Cancel", 16.0, 600.0, 361.0, 50.0),
        ];
        if swap {
            k.reverse();
        }
        app_screen(k)
    };
    assert!(!same(&twins(false), &twins(true)));
}

/// A node anywhere in a member's subtree becoming visible or getting an area takes that
/// subtree out of the rule: its order is hashed again.
#[test]
fn a_visible_or_framed_descendant_takes_the_subtree_out_of_the_rule() {
    let pair = |x: UiNode, swap: bool| {
        let mut k = vec![gb("A").with_children(vec![x]), gb("B")];
        if swap {
            k.reverse();
        }
        wrapped(k)
    };
    let ghost_child = || gb("c");
    assert!(same(
        &pair(ghost_child(), false),
        &pair(ghost_child(), true)
    ));
    let visible_child = || UiNode::new(Role::Button, Rect::default()).with_label("c");
    assert!(!same(
        &pair(visible_child(), false),
        &pair(visible_child(), true)
    ));
    let framed_child = || {
        UiNode::new(Role::Button, rect(-500.0, 0.0, 10.0, 10.0))
            .with_label("c")
            .visible(false)
    };
    assert!(!same(
        &pair(framed_child(), false),
        &pair(framed_child(), true)
    ));
    // The member itself.
    let s = |a: UiNode, swap: bool| {
        let mut k = vec![a, gb("B")];
        if swap {
            k.reverse();
        }
        wrapped(k)
    };
    let shown_member = || button("A", 16.0, 100.0, 100.0, 40.0).visible(false);
    assert!(!same(&s(shown_member(), false), &s(shown_member(), true)));
    // A non-finite origin with nothing on screen is area-less; a NaN origin with a size is
    // clamped onto the screen (drawn), so it keeps its order.
    let at =
        |x: f64| move || UiNode::new(Role::Button, Rect::new(x, 0.0, 10.0, 10.0)).visible(false);
    let inf = at(f64::INFINITY);
    assert!(same(&s(inf(), false), &s(inf(), true)));
    let nan = at(f64::NAN);
    assert!(!same(&s(nan(), false), &s(nan(), true)));
}

/// What compaction reads in document order is never order-free: text inputs (the fields a
/// submit sends), secure nodes, labelled texts and images (their words make an enclosing
/// control's label), keyboards and keys.
#[test]
fn what_compaction_reads_in_order_keeps_its_order() {
    let swap = |make: &dyn Fn(&str) -> UiNode| {
        let a = wrapped(vec![make("A"), make("B")]);
        let b = wrapped(vec![make("B"), make("A")]);
        same(&a, &b)
    };
    for role in [
        Role::TextField,
        Role::SecureTextField,
        Role::TextView,
        Role::SearchField,
        Role::Key,
    ] {
        assert!(!swap(&|l| ghost(role.clone(), l)), "{role:?}");
    }
    // A keyboard's label is not hashed (seventh review); its structure is, in order.
    assert!(!swap(&|l| {
        let kids = if l == "A" { vec![gb("k")] } else { vec![] };
        ghost(Role::Keyboard, l).with_children(kids)
    }));
    assert!(!swap(&|l| gb(l).secure(true)), "secure flag");
    assert!(!swap(&|l| ghost(Role::StaticText, l)), "labelled text");
    assert!(!swap(&|l| ghost(Role::Image, l)), "labelled image");
    assert!(
        !swap(&|l| UiNode::new(Role::StaticText, Rect::default())
            .with_value(l)
            .visible(false)),
        "text with a value"
    );
    // Nested anywhere below a member, too.
    assert!(!swap(
        &|l| gb(l).with_children(vec![ghost(Role::TextField, "f")])
    ));
    assert!(!swap(
        &|l| gb(l).with_children(vec![ghost(Role::StaticText, "t")])
    ));
    // The compact screen proves why: those hidden texts are an unlabelled button's label.
    let pay = |a: &str, b: &str| {
        app_screen(vec![UiNode::new(
            Role::Button,
            rect(16.0, 600.0, 361.0, 50.0),
        )
        .with_children(vec![
            ghost(Role::StaticText, a),
            ghost(Role::StaticText, b),
        ])])
    };
    assert_ne!(compact(&pay("Pay", "$5")), compact(&pay("$5", "Pay")));
}

/// Nodes in the keyboard or its input assistant keep their order (their text is not hashed
/// in the first place; their structure is, in order).
#[test]
fn keyboard_nodes_keep_their_order() {
    let s = |swap: bool| {
        let mut keys = vec![
            UiNode::new(Role::Other("Other".into()), Rect::default()).visible(false),
            UiNode::new(Role::Button, Rect::default())
                .with_identifier("x")
                .visible(false),
        ];
        if swap {
            keys.reverse();
        }
        app_screen_with(
            vec![UiNode::new(Role::Keyboard, rect(0.0, 516.0, 393.0, 336.0)).with_children(keys)],
            true,
        )
    };
    assert!(!same(&s(false), &s(true)));
}

/// The SpringBoard status bar stays excluded: a run around it is as if it were absent.
#[test]
fn an_excluded_status_bar_does_not_split_a_run() {
    let home =
        |swap: bool, bar: Option<&str>| {
            let mut kids = vec![
                UiNode::new(Role::Other("Other".into()), Rect::default()).visible(false),
                UiNode::new(Role::Button, Rect::default())
                    .with_identifier("x")
                    .visible(false),
            ];
            if swap {
                kids.reverse();
            }
            if let Some(t) = bar {
                kids.insert(
                    1,
                    UiNode::new(Role::Other("StatusBar".into()), rect(0.0, 0.0, 393.0, 50.0))
                        .with_children(vec![text(t, rect(20.0, 10.0, 60.0, 20.0))]),
                );
            }
            let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect()).with_children(vec![
                UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(kids),
            ]);
            screen("com.apple.springboard", "Home", root, false)
        };
    assert!(same(&home(false, Some("9:41")), &home(true, Some("9:42"))));
    assert!(same(&home(false, None), &home(true, Some("9:41"))));
}

/// The ten real iOS 26 fixtures: how common the rule is (runs of two or more members,
/// their members, the nodes in them), and that the census matches the digest (a reorder of
/// every counted run keeps it).
#[test]
fn ios26_fixtures_order_free_census() {
    let names = [
        "alert_dialog",
        "login_keyboard",
        "long_list_scrolled",
        "playground_home",
        "safari_start_page",
        "settings_about",
        "settings_general",
        "settings_root",
        "springboard_home",
        "toggles_off",
    ];
    for name in names {
        let s = fixture(name);
        let (runs, members, nodes) = jarvis_observe::order_free_census(&s);
        println!(
            "{name}: {} nodes, {runs} runs, {members} members, {nodes} nodes under the rule",
            s.root.count()
        );
        // Reversing every hidden, area-less leaf group keeps the digest exactly when the
        // census says those leaves are in runs.
        let mut r = s.clone();
        edit(&mut r, |n| {
            if n.children.len() > 1
                && n.children
                    .iter()
                    .all(|c| !c.visible && c.frame.is_empty() && c.children.is_empty())
                && n.children.iter().all(|c| {
                    !c.role.is_text_input()
                        && !c.secure
                        && !(matches!(c.role, Role::StaticText | Role::Image)
                            && (c.label.is_some() || c.value.is_some()))
                        && !matches!(c.role, Role::Keyboard | Role::Key)
                })
            {
                n.children.reverse();
            }
        });
        if r != s && runs > 0 {
            assert!(same(&s, &r), "{name}");
        }
    }
}

/// A `Keyboard` node beside app content: its keys' geometry is hashed, their labels are not
/// (seventh review; sixth review: hashed whole), and the content next to it is hashed whole.
#[test]
fn a_keyboard_beside_app_content_is_hashed_with_its_keys() {
    let s = |amount: &str, key: &str| {
        app_screen_with(
            vec![
                label(&format!("Pay {amount}"), 16.0, 300.0),
                UiNode::new(Role::Keyboard, rect(0.0, 516.0, 393.0, 336.0))
                    .with_children(vec![node(Role::Key, key, rect(3.0, 527.0, 37.0, 46.0))]),
            ],
            true,
        )
    };
    assert!(same(&s("$5", "q"), &s("$5", "Q")));
    assert!(!same(&s("$5", "q"), &s("$500", "q")));
    assert!(compact(&s("$5", "q"))
        .elements
        .iter()
        .all(|e| e.label != "q"));
}
