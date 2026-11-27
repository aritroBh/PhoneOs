#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Ninth review probes (end to end through compaction and the safety gate). Each asserts
//! the SAFE behaviour.
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{Action, Policy, Role, UiNode};
use jarvis_safety::{evaluate_with, Decision, ScreenSource, TypedText};

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

fn find<'a>(view: &'a jarvis_observe::CompactView, label: &str) -> &'a jarvis_proto::Element {
    view.screen
        .elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("no {label:?} in {:#?}", view.screen.elements))
}

/// A mail screen with a full-width bottom toolbar (iOS 26) and a menu that grew out of the
/// toolbar's "Reply" button (iOS 26 menus morph out of their button and are drawn over the
/// toolbar, in a later window). The menu's "Delete" item sits over the toolbar's "Compose".
fn toolbar_with_menu(bar_role: Role) -> jarvis_proto::Screen {
    let size = IPHONE_SIZE.to_rect();
    let bar = UiNode::new(bar_role, rect(0.0, 760.0, 393.0, 92.0)).with_children(vec![
        node(Role::Button, "Reply", rect(20.0, 775.0, 44.0, 44.0)),
        node(Role::Button, "Compose", rect(330.0, 775.0, 44.0, 44.0)),
    ]);
    let app = UiNode::new(Role::Window, size).with_children(vec![
        UiNode::new(Role::ScrollView, size).with_children(vec![
            text("From: Bank", rect(16.0, 100.0, 361.0, 30.0)),
            text("Your statement is ready", rect(16.0, 140.0, 361.0, 30.0)),
        ]),
        bar,
    ]);
    // The menu: a later top-level window, items in a collection view.
    let menu = UiNode::new(Role::Window, size).with_children(vec![UiNode::new(
        Role::Other("Other".into()),
        rect(150.0, 560.0, 240.0, 270.0),
    )
    .with_children(vec![UiNode::new(
        Role::CollectionView,
        rect(150.0, 560.0, 240.0, 270.0),
    )
    .with_children(vec![
        node(Role::Button, "Reply All", rect(150.0, 560.0, 240.0, 66.0)),
        node(Role::Button, "Forward", rect(150.0, 626.0, 240.0, 66.0)),
        node(
            Role::Button,
            "Move to Junk",
            rect(150.0, 692.0, 240.0, 66.0),
        ),
        node(Role::Button, "Delete", rect(150.0, 758.0, 240.0, 72.0)),
    ])])]);
    let root = UiNode::new(Role::Application, size)
        .with_label("Mail")
        .with_children(vec![app, menu]);
    screen("com.apple.mobilemail", "Mail", root, false)
}

/// HIGH. Toolbar precedence (new in d34bacc) makes every node outside the toolbar give way
/// at points inside it, including controls drawn *later* in another window (a menu, a
/// banner, a mini player). "Compose" is classified harmless and tapped at its centre with no
/// question; the touch lands on the menu's "Delete". Before d34bacc the menu item contested
/// the point (a toolbar was not a system bar) and the tap was refused or moved.
#[test]
fn probe9_toolbar_precedence_ignores_a_menu_drawn_over_it() {
    // For comparison: the tab bar has the same precedence since the seventh review.
    for role in [Role::TabBar, Role::Other("Other".into())] {
        let s = toolbar_with_menu(role.clone());
        let view = jarvis_observe::compact_view(&s);
        let t = tree(&view);
        let c = find(&view, "Compose");
        eprintln!(
            "{role:?}: {:?}",
            jarvis_safety::tap_spot(c, &view.screen, Some(&t), None)
        );
    }
    let s = toolbar_with_menu(Role::Toolbar);
    let view = jarvis_observe::compact_view(&s);
    let t = tree(&view);
    let compose = find(&view, "Compose");
    let delete = find(&view, "Delete");
    let d = evaluate_with(
        &Policy::default(),
        &Action::Tap {
            element: compose.index,
        },
        &view.screen,
        None,
        ScreenSource::Typed,
        Some(&t),
        &[],
    );
    let spot = jarvis_safety::tap_spot(compose, &view.screen, Some(&t), None);
    eprintln!(
        "decision {d:?}; spot {spot:?}; Delete at {:?}",
        delete.frame
    );
    let lands_on_delete = matches!(spot, Ok(p) if delete.frame.contains(p));
    assert!(
        !(lands_on_delete && matches!(d, Decision::Allow)),
        "tap on Compose sent unasked at {spot:?}, inside the menu's Delete {:?}",
        delete.frame
    );
}

/// A product card (`Button "Wool socks"`) holding its title, its price and an unlabelled icon
/// button (a trash can whose only text is its image's name). Compaction's `Target::of`
/// refines the card to the single nested same-role control with a compatible (absent) label,
/// so the card's listed frame *is* the trash button, and rule v3 exempts it as the card's
/// "own refined inner control".
fn card_with_icon_button() -> jarvis_proto::Screen {
    let size = IPHONE_SIZE.to_rect();
    let card =
        node(Role::Button, "Wool socks", rect(16.0, 200.0, 361.0, 120.0)).with_children(vec![
            text("Wool socks", rect(32.0, 210.0, 200.0, 30.0)),
            text("$25.00", rect(32.0, 250.0, 100.0, 30.0)),
            UiNode::new(Role::Button, rect(320.0, 250.0, 44.0, 44.0)).with_children(vec![node(
                Role::Image,
                "trash",
                rect(330.0, 260.0, 24.0, 24.0),
            )]),
        ]);
    let root = UiNode::new(Role::Application, size)
        .with_label("Shop")
        .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::ScrollView, size).with_children(vec![card]),
        ])]);
    screen("com.example.shop", "Shop", root, false)
}

/// HIGH. "Tap the Wool socks card" is sent, unasked, at the centre of the trash button
/// nested in it: the listed element is the card's label on the trash button's frame.
#[test]
fn probe9_card_refined_to_its_unlabelled_trash_button() {
    let s = card_with_icon_button();
    let view = jarvis_observe::compact_view(&s);
    let t = tree(&view);
    let card = find(&view, "Wool socks");
    let trash = rect(320.0, 250.0, 44.0, 44.0);
    let spot = jarvis_safety::tap_spot(card, &view.screen, Some(&t), None);
    let d = evaluate_with(
        &Policy::default(),
        &Action::Tap {
            element: card.index,
        },
        &view.screen,
        None,
        ScreenSource::Typed,
        Some(&t),
        &[],
    );
    eprintln!("card frame {:?}; spot {spot:?}; decision {d:?}", card.frame);
    let on_trash = matches!(spot, Ok(p) if trash.contains(p));
    assert!(
        !(on_trash && matches!(d, Decision::Allow)),
        "the card tap is sent unasked on the nested trash button ({spot:?})"
    );
}

/// A comment form: Jarvis typed the comment, then scrolled down to the form's button. The
/// typed field is no longer listed (scrolled away) but is still in the form (`inputs`).
fn comment_form(scrolled: bool, button: &str) -> jarvis_proto::Screen {
    let size = IPHONE_SIZE.to_rect();
    let dy = if scrolled { -900.0 } else { 0.0 };
    let form = UiNode::new(
        Role::Other("Other".into()),
        rect(0.0, 100.0 + dy, 393.0, 1400.0),
    )
    .with_children(vec![
        text("Leave a review", rect(16.0, 110.0 + dy, 361.0, 30.0)),
        UiNode::new(Role::TextView, rect(16.0, 150.0 + dy, 361.0, 200.0))
            .with_label("Your review")
            .with_value("This seller scammed me, avoid at all costs"),
        text("Terms apply", rect(16.0, 1300.0 + dy, 361.0, 30.0)),
        node(
            Role::Button,
            button,
            rect(
                16.0,
                if scrolled { 1440.0 + dy } else { 400.0 },
                361.0,
                50.0,
            ),
        ),
    ]);
    let root = UiNode::new(Role::Application, size)
        .with_label("Shop")
        .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::ScrollView, size).with_children(vec![form]),
        ])]);
    screen("com.example.shop", "Shop", root, false)
}

/// HIGH/MEDIUM. The presumed-submit rule (d34bacc) holds while the typed field is listed;
/// once it scrolls away the same button is sent with no question, although `inputs` still
/// knows the form holds Jarvis's text. (`presumed_submit` reads only `form_of(..).listed`.)
#[test]
fn probe9_presumed_submit_lost_when_typed_field_scrolls_away() {
    let mut sent_unasked = Vec::new();
    for b in ["Done", "OK", "Finish", "Submit", "Confirm", "chevron.right"] {
        let before = comment_form(false, b);
        let v0 = jarvis_observe::compact_view(&before);
        let field = find(&v0, "Your review");
        let typed = [TypedText {
            field: field.fingerprint,
            text: "This seller scammed me, avoid at all costs".to_owned(),
        }];
        let after = comment_form(true, b);
        let v1 = jarvis_observe::compact_view(&after);
        let t = tree(&v1);
        let btn = find(&v1, b);
        assert!(v1.screen.elements.iter().all(|e| e.label != "Your review"));
        assert!(v1
            .inputs
            .iter()
            .any(|r| r.label.as_deref() == Some("Your review")));
        let d = evaluate_with(
            &Policy::default(),
            &Action::Tap { element: btn.index },
            &v1.screen,
            None,
            ScreenSource::Typed,
            Some(&t),
            &typed,
        );
        // While listed, the same tap is asked.
        let t0 = tree(&v0);
        let d0 = evaluate_with(
            &Policy::default(),
            &Action::Tap {
                element: find(&v0, b).index,
            },
            &v0.screen,
            None,
            ScreenSource::Typed,
            Some(&t0),
            &typed,
        );
        eprintln!("{b:?}: listed -> {d0:?}\n       scrolled -> {d:?}");
        if matches!(d, Decision::Allow) {
            sent_unasked.push(b);
        }
    }
    assert!(
        sent_unasked.is_empty(),
        "sent unasked once the typed field scrolled away: {sent_unasked:?}"
    );
}

/// MEDIUM. Digest v5: inside a node identified `SystemInputAssistantView` in the keyboard's
/// window nothing is hashed any more. An app's input accessory view (which iOS puts in the
/// keyboard window) can carry that identifier; a control it draws outside the bar's own
/// frame, over the app's "Send" button, can now appear after the owner approved the "Send"
/// and the approval is still used (v4 hashed its role and frame).
#[test]
fn probe9_fake_prediction_bar_child_appears_without_changing_the_digest() {
    let build = |overlay: bool| {
        let size = IPHONE_SIZE.to_rect();
        let mut kids = vec![node(Role::Button, "Bold", rect(0.0, 472.0, 60.0, 44.0))];
        if overlay {
            // Drawn over the app's Send button, outside the accessory's own frame.
            kids.push(node(
                Role::Button,
                "Buy coins $99.99",
                rect(16.0, 350.0, 361.0, 64.0),
            ));
        }
        let accessory = UiNode::new(Role::Other("Other".into()), rect(0.0, 472.0, 393.0, 44.0))
            .with_identifier("SystemInputAssistantView")
            .with_children(kids);
        let keyboard = UiNode::new(Role::Keyboard, rect(0.0, 516.0, 393.0, 336.0))
            .with_children(vec![node(Role::Key, "q", rect(3.0, 527.0, 37.0, 46.0))]);
        let app = UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::TextField, rect(16.0, 300.0, 361.0, 44.0))
                .with_label("Message")
                .with_value("hi")
                .focused(true),
            node(Role::Button, "Send", rect(16.0, 360.0, 361.0, 44.0)),
        ]);
        let kbwin = UiNode::new(Role::Window, size).with_children(vec![accessory, keyboard]);
        let root = UiNode::new(Role::Application, size)
            .with_label("Chat")
            .with_children(vec![app, kbwin]);
        screen("com.example.chat", "Chat", root, true)
    };
    let (a, b) = (build(false), build(true));
    let view = jarvis_observe::compact_view(&b);
    let t = tree(&view);
    let send = find(&view, "Send");
    eprintln!(
        "spot with overlay: {:?}",
        jarvis_safety::tap_spot(send, &view.screen, Some(&t), None)
    );
    assert_ne!(
        jarvis_observe::screen_digest(&a),
        jarvis_observe::screen_digest(&b),
        "a control appeared over the approved Send without changing the digest"
    );
}

/// Robustness: a form with 50k unlisted text inputs (half recipient-like) and a 2 MB value
/// goes through the summary composition in bounded time, within the bound.
#[test]
fn probe9_huge_forms_summarise_in_bounded_time() {
    let size = IPHONE_SIZE.to_rect();
    let n: u32 = std::env::var("P9N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000);
    let big: usize = std::env::var("P9B")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let mut kids: Vec<UiNode> = (0..n)
        .map(|i| {
            UiNode::new(
                Role::TextField,
                rect(0.0, 2000.0 + f64::from(i), 393.0, 44.0),
            )
            .with_label(format!("Recipient {i}"))
            .with_value(if i % 2 == 0 {
                format!("@user{i}")
            } else {
                "$1.00".into()
            })
        })
        .collect();
    kids.push(
        UiNode::new(Role::TextField, rect(0.0, 1900.0, 393.0, 44.0))
            .with_label("Note")
            .with_value("1 2 3 4 5 6 7 8 9 0 $ ".repeat(big)),
    );
    kids.push(node(
        Role::Button,
        "Pay now",
        rect(16.0, 400.0, 361.0, 50.0),
    ));
    let root = UiNode::new(Role::Application, size)
        .with_label("Shop")
        .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::Other("Other".into()), size).with_children(kids),
        ])]);
    let s = screen("com.example.shop", "Shop", root, false);
    let t0 = std::time::Instant::now();
    let view = jarvis_observe::compact_view(&s);
    let t_view = t0.elapsed();
    let t = tree(&view);
    let pay = find(&view, "Pay now");
    let t1 = std::time::Instant::now();
    let d = evaluate_with(
        &Policy::default(),
        &Action::Tap { element: pay.index },
        &view.screen,
        None,
        ScreenSource::Typed,
        Some(&t),
        &[],
    );
    let el = t0.elapsed();
    eprintln!("compact_view {t_view:?}; evaluate_with {:?}", t1.elapsed());
    if let Decision::Confirm(r) = &d {
        eprintln!("{el:?} {} chars: {}", r.summary.chars().count(), r.summary);
    } else {
        eprintln!("{el:?} {d:?}");
    }
    assert!(el.as_secs() < 20, "{el:?}");
}
