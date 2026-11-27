#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Eighteenth review (E4): a web view is the same document as the web view holding it only
//! when it is a structural wrapper — the same frame, and the outer web view holds no content
//! of its own beside it.

mod common;

use common::ios::fixture;
use jarvis_observe::compact_view;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{Role, Screen, UiNode};

fn app(kids: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Safari")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(kids)
        ]);
    screen("com.apple.mobilesafari", "Safari", root, false)
}

/// Safari's real tree: three web views of one frame, the innermost beside WebKit's scroll
/// indicators only. One document.
#[test]
fn safaris_three_wrappers_are_one_document() {
    let s = fixture("safari_start_page");
    let v = compact_view(&s);
    assert_eq!(v.webviews.len(), 1, "{:?}", v.webviews);
}

/// A same-frame web view beside the outer page's own content (a full-viewport
/// cross-origin interstitial) is a document of its own; without that content it is a
/// wrapper.
#[test]
fn a_same_frame_web_view_beside_the_pages_content_is_a_document() {
    let page = rect(0.0, 100.0, 393.0, 650.0);
    let inner = || {
        UiNode::new(Role::WebView, page).with_children(vec![node(
            Role::Button,
            "4",
            rect(16.0, 300.0, 100.0, 60.0),
        )])
    };
    let with_content = app(vec![UiNode::new(Role::WebView, page).with_children(vec![
        text("Balance $4,210.17", rect(16.0, 140.0, 361.0, 30.0)),
        inner(),
    ])]);
    assert_eq!(compact_view(&with_content).webviews.len(), 2);
    // Content after the child counts too.
    let after = app(vec![UiNode::new(Role::WebView, page).with_children(vec![
        inner(),
        node(Role::Button, "Menu", rect(16.0, 700.0, 100.0, 40.0)),
    ])]);
    assert_eq!(compact_view(&after).webviews.len(), 2);
    // Two same-frame children: the second is beside the first.
    let two = app(vec![
        UiNode::new(Role::WebView, page).with_children(vec![inner(), inner()])
    ]);
    assert_eq!(compact_view(&two).webviews.len(), 3);
    // Only unlabelled containers between: a wrapper chain, one document.
    let wrapped = app(vec![UiNode::new(Role::WebView, page).with_children(vec![
        UiNode::new(Role::Other("Other".into()), page).with_children(vec![UiNode::new(
            Role::WebView,
            page,
        )
        .with_children(vec![inner()])]),
    ])]);
    assert_eq!(compact_view(&wrapped).webviews.len(), 1);
    // An empty same-frame web view beside the page's content has nothing to tell apart.
    let empty = app(vec![UiNode::new(Role::WebView, page).with_children(vec![
        text("Balance $4,210.17", rect(16.0, 140.0, 361.0, 30.0)),
        UiNode::new(Role::WebView, page),
    ])]);
    assert_eq!(compact_view(&empty).webviews.len(), 1);
    // A different frame is a document whatever surrounds it (unchanged).
    let small = app(vec![UiNode::new(Role::WebView, page).with_children(vec![
        UiNode::new(Role::WebView, rect(0.0, 200.0, 393.0, 100.0)),
    ])]);
    assert_eq!(compact_view(&small).webviews.len(), 2);
}
