#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Ninth review: regression tests for the fixes, through compaction (the review's probes
//! are in `ninth_review.rs`, the property tests in `tap_landing_property.rs` and
//! `presumed_submit_property.rs`).

use jarvis_observe::{compact_view, screen_digest, CompactView};
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{Action, Policy, RiskCategory, Role, Screen, UiNode};
use jarvis_safety::{
    evaluate_with, tap_spot, CoveredBy, Decision, ScreenSource, ScreenTree, TypedText,
};

fn tree(view: &CompactView) -> ScreenTree<'_> {
    ScreenTree {
        sources: &view.sources,
        parents: &view.parents,
        ends: &view.ends,
        bars: &view.bars,
        nodes: &view.nodes,
        inputs: &view.inputs,
    }
}

fn find<'a>(view: &'a CompactView, label: &str) -> &'a jarvis_proto::Element {
    view.screen
        .elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("no {label:?} in {:#?}", view.screen.elements))
}

fn app(windows: Vec<UiNode>) -> Screen {
    let root = UiNode::new(jarvis_proto::Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Mail")
        .with_children(windows);
    screen("com.example.mail", "Mail", root, false)
}

/// A page (a scroller of `page_role`, `page` frame) with a row under the bottom bar's
/// "Compose", and the bar after (or before) it.
fn page_and_bar(
    page_role: Role,
    page: jarvis_proto::Rect,
    bar_first: bool,
    bar_role: Role,
) -> Screen {
    let size = IPHONE_SIZE.to_rect();
    let rows = UiNode::new(page_role, page).with_children(vec![node(
        Role::Button,
        "Row under the bar",
        rect(0.0, 760.0, 393.0, 92.0),
    )]);
    let bar = UiNode::new(bar_role, rect(0.0, 766.0, 393.0, 86.0)).with_children(vec![node(
        Role::Button,
        "Compose",
        rect(330.0, 775.0, 44.0, 44.0),
    )]);
    let kids = if bar_first {
        vec![bar, rows]
    } else {
        vec![rows, bar]
    };
    app(vec![UiNode::new(Role::Window, size).with_children(kids)])
}

/// H1: the bar's control wins only over page content scrolled under it: a page-sized
/// scroller before the bar. A scroller after a bottom bar, a narrow one, a short one, or a
/// plain container all contest; so does a modal container (a menu) in the same window.
#[test]
fn a_bottom_bar_wins_only_over_page_content_scrolled_under_it() {
    let full = IPHONE_SIZE.to_rect();
    let cases = [
        (Role::ScrollView, full, false, true),
        (Role::CollectionView, full, false, true),
        (Role::ScrollView, full, true, false),
        (Role::ScrollView, rect(0.0, 0.0, 240.0, 852.0), false, false),
        (
            Role::ScrollView,
            rect(0.0, 700.0, 393.0, 152.0),
            false,
            false,
        ),
        (Role::Other("Other".into()), full, false, false),
        (Role::Other("Menu".into()), full, false, false),
    ];
    for (role, frame, bar_first, wins) in cases {
        for bar_role in [Role::TabBar, Role::Toolbar] {
            let s = page_and_bar(role.clone(), frame, bar_first, bar_role.clone());
            let view = compact_view(&s);
            let t = tree(&view);
            let compose = find(&view, "Compose");
            let got = tap_spot(compose, &view.screen, Some(&t), None);
            if wins {
                assert_eq!(got, Ok(compose.frame.center()), "{role:?} {bar_role:?}");
            } else {
                assert!(
                    matches!(got, Err(c) if matches!(c.by, CoveredBy::Control(_))),
                    "{role:?} {frame:?} first {bar_first} {bar_role:?}: {got:?}"
                );
            }
        }
    }
}

/// H1: a navigation bar at the top accepts page content before or after it in the tree
/// (SwiftUI lists the bar first).
#[test]
fn a_top_bar_wins_over_page_content_in_either_order() {
    let size = IPHONE_SIZE.to_rect();
    for bar_first in [true, false] {
        let rows = UiNode::new(Role::CollectionView, size).with_children(vec![node(
            Role::Button,
            "Row under the bar",
            rect(0.0, 40.0, 393.0, 80.0),
        )]);
        let bar =
            UiNode::new(Role::NavigationBar, rect(0.0, 59.0, 393.0, 44.0)).with_children(vec![
                node(Role::Button, "Back", rect(16.0, 59.0, 44.0, 44.0)),
            ]);
        let kids = if bar_first {
            vec![bar, rows]
        } else {
            vec![rows, bar]
        };
        let s = app(vec![UiNode::new(Role::Window, size).with_children(kids)]);
        let view = compact_view(&s);
        let t = tree(&view);
        let back = find(&view, "Back");
        assert_eq!(
            tap_spot(back, &view.screen, Some(&t), None),
            Ok(back.frame.center()),
            "bar first: {bar_first}"
        );
    }
}

/// H1: a bar must be a bar: a full-screen `Toolbar` takes no precedence (its controls are
/// contested like any, and the content under it is not "covered by the toolbar"); a bar
/// in the middle of the screen neither.
#[test]
fn a_full_screen_or_floating_toolbar_is_no_system_bar() {
    let size = IPHONE_SIZE.to_rect();
    for bar_frame in [size, rect(0.0, 400.0, 393.0, 60.0)] {
        let page = UiNode::new(Role::ScrollView, size).with_children(vec![node(
            Role::Button,
            "Delete everything",
            rect(0.0, 400.0, 393.0, 80.0),
        )]);
        let bar = UiNode::new(Role::Toolbar, bar_frame).with_children(vec![node(
            Role::Button,
            "Compose",
            rect(16.0, 410.0, 44.0, 44.0),
        )]);
        let s = app(vec![
            UiNode::new(Role::Window, size).with_children(vec![page, bar])
        ]);
        let view = compact_view(&s);
        let t = tree(&view);
        let compose = find(&view, "Compose");
        assert!(
            tap_spot(compose, &view.screen, Some(&t), None).is_err(),
            "{bar_frame:?}"
        );
        let row = find(&view, "Delete everything");
        let got = tap_spot(row, &view.screen, Some(&t), None);
        assert!(
            !matches!(got, Err(c) if matches!(c.by, CoveredBy::Bar(_))),
            "{bar_frame:?}: {got:?}"
        );
    }
}

/// H2: a card is refined to an inner control only when it shows nothing outside it, or the
/// inner control covers most of it; then the inner control's icon name is part of the
/// element's label (and of its classification).
#[test]
fn a_card_is_refined_only_to_what_covers_it() {
    let size = IPHONE_SIZE.to_rect();
    let card = |inner: jarvis_proto::Rect, title: bool| {
        let mut kids = vec![UiNode::new(Role::Button, inner).with_children(vec![node(
            Role::Image,
            "trash",
            rect(inner.x + 4.0, inner.y + 4.0, 24.0, 24.0),
        )])];
        if title {
            kids.insert(0, text("Wool socks", rect(32.0, 210.0, 200.0, 30.0)));
        }
        let c =
            node(Role::Button, "Wool socks", rect(16.0, 200.0, 361.0, 120.0)).with_children(kids);
        app(vec![UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::ScrollView, size).with_children(vec![c]),
        ])])
    };
    // Title outside a small inner button: the card stays the card.
    let view = compact_view(&card(rect(320.0, 250.0, 44.0, 44.0), true));
    let e = find(&view, "Wool socks");
    assert_eq!(e.frame, rect(16.0, 200.0, 361.0, 120.0));
    // Nothing outside the small inner button: refined (the SwiftUI toggle case), and the
    // element reads the icon: a Delete.
    let view = compact_view(&card(rect(320.0, 250.0, 44.0, 44.0), false));
    let e = find(&view, "Wool socks, trash");
    assert_eq!(e.frame, rect(320.0, 250.0, 44.0, 44.0));
    let d = evaluate_with(
        &Policy::default(),
        &Action::Tap { element: e.index },
        &view.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(&view)),
        &[],
    );
    assert!(
        matches!(&d, Decision::Confirm(r) if r.category == RiskCategory::Delete && r.summary.contains("trash")),
        "{d:?}"
    );
    // A title outside, but the inner button covers 90 % of the card: refined.
    let view = compact_view(&card(rect(16.0, 200.0, 361.0, 108.0), true));
    let e = find(&view, "Wool socks, trash");
    assert_eq!(e.frame, rect(16.0, 200.0, 361.0, 108.0));
}

/// M5: a node of the prediction bar's subtree reaching outside the bar's frame is ordinary
/// content: listed, hashed (its label changing changes the digest) and contesting; the
/// candidates inside the bar stay out of the digest and the list.
#[test]
fn a_prediction_bar_child_outside_the_bar_is_ordinary_content() {
    let build = |offer: &str, word: &str| {
        let size = IPHONE_SIZE.to_rect();
        let accessory = UiNode::new(Role::Other("Other".into()), rect(0.0, 472.0, 393.0, 44.0))
            .with_identifier("SystemInputAssistantView")
            .with_children(vec![
                node(
                    Role::Other("Other".into()),
                    word,
                    rect(0.0, 472.0, 131.0, 44.0),
                ),
                node(Role::Button, offer, rect(16.0, 350.0, 361.0, 64.0)),
            ]);
        let keyboard = UiNode::new(Role::Keyboard, rect(0.0, 516.0, 393.0, 336.0))
            .with_children(vec![node(Role::Key, "q", rect(3.0, 527.0, 37.0, 46.0))]);
        let app_win = UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::TextField, rect(16.0, 300.0, 361.0, 44.0))
                .with_label("Message")
                .focused(true),
            node(Role::Button, "Send", rect(16.0, 360.0, 361.0, 44.0)),
        ]);
        let kb = UiNode::new(Role::Window, size).with_children(vec![accessory, keyboard]);
        let root = UiNode::new(Role::Application, size)
            .with_label("Chat")
            .with_children(vec![app_win, kb]);
        screen("com.example.chat", "Chat", root, true)
    };
    let a = build("Buy coins $99.99", "hello");
    let view = compact_view(&a);
    let offer = find(&view, "Buy coins $99.99");
    assert!(!view
        .nodes
        .iter()
        .any(|n| n.keyboard && n.label.as_deref() == Some("Buy coins $99.99")));
    let send = find(&view, "Send");
    assert!(matches!(
        tap_spot(send, &view.screen, Some(&tree(&view)), None),
        Err(c) if c.by == CoveredBy::Control(offer.index)
    ));
    assert!(view.screen.elements.iter().all(|e| e.label != "hello"));
    assert_ne!(
        screen_digest(&a),
        screen_digest(&build("Buy coins $9.99", "hello"))
    );
    assert_eq!(
        screen_digest(&a),
        screen_digest(&build("Buy coins $99.99", "world"))
    );
}

/// H3: the field Jarvis typed in, scrolled out of the list, is still what the form sends:
/// first in `Sends:`, marked `(not on screen)`.
#[test]
fn the_typed_field_off_the_list_is_sent_first() {
    let size = IPHONE_SIZE.to_rect();
    let form = |dy: f64| {
        let f = UiNode::new(
            Role::Other("Other".into()),
            rect(0.0, 100.0 + dy, 393.0, 1400.0),
        )
        .with_children(vec![
            UiNode::new(Role::TextField, rect(16.0, 120.0 + dy, 361.0, 44.0))
                .with_label("Subject")
                .with_value("Hello"),
            UiNode::new(Role::TextView, rect(16.0, 180.0 + dy, 361.0, 200.0))
                .with_label("Your review")
                .with_value("Great seller"),
            node(Role::Button, "Done", rect(16.0, 1440.0 + dy, 361.0, 50.0)),
        ]);
        app(vec![UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::ScrollView, size).with_children(vec![f]),
        ])])
    };
    let before = compact_view(&form(0.0));
    let typed = [TypedText {
        field: find(&before, "Your review").fingerprint,
        text: "Great seller".to_owned(),
    }];
    let after = compact_view(&form(-900.0));
    let done = find(&after, "Done");
    let d = evaluate_with(
        &Policy::default(),
        &Action::Tap {
            element: done.index,
        },
        &after.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(&after)),
        &typed,
    );
    let Decision::Confirm(r) = d else {
        panic!("{d:?}")
    };
    assert!(
        r.summary.contains(
            "Sends: Your review: \"Great seller\" (not on screen); Subject: \"Hello\" (not on screen)"
        ),
        "{}",
        r.summary
    );
}
