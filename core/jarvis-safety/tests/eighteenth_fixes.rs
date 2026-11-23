#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Eighteenth review (safety): places are merged, or a source exempted, only on an OS fact —
//! the one-way relation for Apple's apps only (E2), a host read only from a listed browser's
//! chrome (E3), ambiguous embedded classes (E5), the screen picture as a source (E1), and
//! flow notes that keep Jarvis's own words whole.

use jarvis_proto::fixtures::{node, rect, IPHONE_SIZE};
use jarvis_proto::{CompactScreen, Role, Screen, UiNode};
use jarvis_safety::{
    flow_note_fit, is_apple_app, is_browser, observed_context, Confinement, FlowContext,
    ScreenSource, BROWSERS, COVERS_ALL, SCREEN_PICTURE,
};

fn web(app: &str, within: Option<&str>) -> FlowContext {
    FlowContext::Embedded {
        app: app.into(),
        within: within.map(str::to_owned),
    }
}

#[test]
fn the_one_way_relation_is_for_apples_apps_only() {
    let news = FlowContext::app("com.apple.news");
    let social = FlowContext::app("com.example.social");
    assert!(news.surrounds(&web("com.apple.news", None)));
    assert!(!social.surrounds(&web("com.example.social", None)));
    // Not by a look-alike prefix.
    assert!(!is_apple_app("com.applesauce.app"));
    assert!(!is_apple_app("org.com.apple.x"));
    assert!(is_apple_app("com.apple.mobilemail"));
    // Two-way: each is a source for the other, each direction its own flow.
    let mut c = Confinement::new();
    c.read(social.clone());
    c.read(web("com.example.social", None));
    assert_eq!(
        c.unallowed(&social, false),
        vec![web("com.example.social", None)]
    );
    assert_eq!(
        c.unallowed(&web("com.example.social", None), false),
        vec![social.clone()]
    );
    c.allow(&social, &[web("com.example.social", None)]);
    assert!(c.unallowed(&social, false).is_empty());
    assert_eq!(
        c.unallowed(&web("com.example.social", None), false),
        vec![social]
    );
}

#[test]
fn nesting_levels_are_places_of_their_own() {
    let top = web("com.example.social", None);
    let nested = FlowContext::NestedWeb("com.example.social".into());
    assert_ne!(top, nested);
    assert_ne!(top.key(), nested.key());
    let app = FlowContext::app("com.example.social");
    assert!(!app.surrounds(&nested));
    let mut c = Confinement::new();
    c.read(top.clone());
    c.read(nested.clone());
    assert_eq!(c.unallowed(&top, false), vec![nested.clone()]);
    assert_eq!(c.unallowed(&nested, false), vec![top]);
    assert_eq!(
        nested.display(|_| Some("Social".into())),
        "nested web content in Social"
    );
}

#[test]
fn an_ambiguous_class_is_a_source_for_itself_and_named_as_all_of_it() {
    let site = FlowContext::Site {
        app: "com.apple.mobilesafari".into(),
        host: "attacker.example".into(),
    };
    let frames = web("com.apple.mobilesafari", Some("attacker.example"));
    let mut c = Confinement::new();
    c.read(site.clone());
    c.read(frames.clone());
    // One document: as before.
    assert_eq!(c.unallowed(&frames, false), vec![site.clone()]);
    c.allow(&site, std::slice::from_ref(&frames));
    assert!(c.unallowed(&site, false).is_empty());
    // Two documents seen: a source for itself, and an approval given before is dropped.
    c.mark_ambiguous(frames.clone());
    assert!(c.is_ambiguous(&frames));
    let into_frames = c.unallowed(&frames, false);
    assert!(into_frames.contains(&frames) && into_frames.contains(&site));
    assert_eq!(c.unallowed(&site, false), vec![frames.clone()]);
    // Its approval into itself is kept.
    c.allow(&frames, &[frames.clone(), site.clone()]);
    assert!(c.unallowed(&frames, false).is_empty());
    // Marking again drops nothing more.
    c.mark_ambiguous(frames.clone());
    assert!(c.unallowed(&frames, false).is_empty());
    let name = frames.display_covering(|_| None);
    assert_eq!(
        name,
        format!("embedded content in attacker.example{COVERS_ALL}")
    );
}

#[test]
fn the_screen_picture_is_a_source_for_every_place() {
    let mut c = Confinement::new();
    c.read(FlowContext::ScreenPicture);
    for target in [
        FlowContext::app("com.example.game"),
        FlowContext::Site {
            app: "com.apple.mobilesafari".into(),
            host: "a.example".into(),
        },
        FlowContext::Tool("lookup".into()),
        FlowContext::Unknown,
    ] {
        assert_eq!(
            c.unallowed(&target, false),
            vec![FlowContext::ScreenPicture],
            "{target:?}"
        );
    }
    assert_eq!(FlowContext::ScreenPicture.display(|_| None), SCREEN_PICTURE);
    assert_eq!(FlowContext::ScreenPicture.bundle(), None);
}

/// Jarvis's own words in a flow note are never shortened in the middle; the place's own
/// name before the covering suffix is.
#[test]
fn flow_notes_keep_jarvis_own_words_whole() {
    let long_host = format!("{}.example", "a".repeat(80));
    let names = vec![
        SCREEN_PICTURE.to_owned(),
        format!("embedded content in {long_host}{COVERS_ALL}"),
    ];
    let (row, n) = flow_note_fit(&names);
    assert_eq!(n, 2, "{}", row.text);
    assert!(row.text.contains(SCREEN_PICTURE), "{}", row.text);
    assert!(row.text.contains(COVERS_ALL), "{}", row.text);
    assert!(!row.text.contains(&long_host), "{}", row.text);
}

#[test]
fn the_browser_list_is_closed_and_names_third_party_sites_with_the_browser() {
    assert_eq!(BROWSERS.len(), 7);
    for b in [
        "com.apple.mobilesafari",
        "com.google.chrome.ios",
        "org.mozilla.ios.Firefox",
        "com.microsoft.msedge",
        "com.brave.ios.browser",
        "com.duckduckgo.mobile.ios",
        "com.opera.OperaTouch",
    ] {
        assert!(is_browser(b), "{b}");
    }
    assert!(!is_browser("com.example.evil"));
    assert!(!is_browser("com.google.chrome.ios.evil"));
    let chrome = FlowContext::Site {
        app: "com.google.chrome.ios".into(),
        host: "bank.example".into(),
    };
    assert_eq!(
        chrome.display(|_| Some("Chrome".into())),
        "bank.example in Chrome"
    );
}

fn raw(app: &str, kids: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("App")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(kids)
        ]);
    jarvis_proto::fixtures::screen(app, "App", root, false)
}

fn empty(app: &str) -> CompactScreen {
    CompactScreen {
        app_bundle_id: app.into(),
        app_name: None,
        size: IPHONE_SIZE,
        keyboard_visible: false,
        elements: Vec::new(),
        cues: Default::default(),
    }
}

/// A host is read only from a listed browser's chrome: another app's "Address" node names
/// nothing, its screen is the app and its web view "web content in <App>".
#[test]
fn a_host_is_read_only_from_a_browsers_chrome() {
    let kids =
        || {
            vec![
                UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 600.0)).with_children(vec![
                    node(Role::Button, "Continue", rect(16.0, 300.0, 361.0, 44.0)),
                ]),
                UiNode::new(Role::Button, rect(100.0, 800.0, 200.0, 30.0))
                    .with_label("Address")
                    .with_value("bank.example")
                    .with_identifier("TabBarItemTitle"),
            ]
        };
    for (app, site) in [
        ("com.apple.mobilesafari", true),
        ("com.google.chrome.ios", true),
        ("org.mozilla.ios.Firefox", true),
        ("com.example.evil", false),
    ] {
        let s = raw(app, kids());
        let ctx = observed_context(&s, &empty(app), ScreenSource::Typed).unwrap();
        if site {
            assert_eq!(
                ctx,
                FlowContext::Site {
                    app: app.to_lowercase(),
                    host: "bank.example".into()
                },
                "{app}"
            );
        } else {
            assert_eq!(ctx, FlowContext::app(app), "{app}");
        }
    }
}
