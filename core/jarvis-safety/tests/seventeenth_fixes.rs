//! Seventeenth review (safety): the place definition, the link destination, the forced
//! gate under `confirm: []`, the flow note's names, and edit-menu items that hand the
//! selection on.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{el, screen};
use jarvis_proto::fixtures::{node, rect, text, IPHONE_SIZE};
use jarvis_proto::{
    Action, CompactScreen, Direction, Policy, RiskCategory, Role, Screen, TrustLevel, UiNode,
};
use jarvis_safety::{
    evaluate_flowing, flow_note_fit, gate_owned, handoff_control, observed_context,
    url_destination, Confinement, Decision, FlowContext, Gate, ScreenSource,
};

fn decide(
    policy: &Policy,
    action: &Action,
    s: &CompactScreen,
    owner: &[&str],
    tainted: bool,
) -> Decision {
    evaluate_flowing(
        policy,
        action,
        s,
        None,
        ScreenSource::Typed,
        None,
        &[],
        owner,
        false,
        tainted,
    )
}

#[test]
fn a_link_goes_to_its_hosts_site_or_to_an_unknown_place() {
    assert_eq!(
        url_destination("https://Evil.Example/c?v=1"),
        FlowContext::Site {
            app: "com.apple.mobilesafari".into(),
            host: "evil.example".into()
        }
    );
    assert_eq!(
        url_destination("https://bank.example@evil.example/"),
        FlowContext::Site {
            app: "com.apple.mobilesafari".into(),
            host: "evil.example".into()
        }
    );
    for u in [
        "tel:123",
        "maps://?q=x",
        "shortcuts://run",
        "evil.example",
        "",
    ] {
        assert_eq!(url_destination(u), FlowContext::Unknown, "{u}");
    }
}

#[test]
fn app_web_content_is_one_way_a_frame_two_way() {
    let app = FlowContext::app("com.example.acme");
    let web = FlowContext::Embedded {
        app: "com.example.acme".into(),
        within: None,
    };
    let site = FlowContext::Site {
        app: "com.apple.mobilesafari".into(),
        host: "bank.example".into(),
    };
    let frame = FlowContext::Embedded {
        app: "com.apple.mobilesafari".into(),
        within: Some("bank.example".into()),
    };
    assert!(app.surrounds(&web) && !web.surrounds(&app));
    // A frame in a page is two-way: neither surrounds the other.
    assert!(!site.surrounds(&frame) && !frame.surrounds(&site));
    assert!(!site.surrounds(&web));
    let unreadable = FlowContext::SiteUnreadable("com.example.acme".into());
    assert!(!unreadable.surrounds(&web));
    let mut f = Confinement::new();
    f.read(site.clone());
    f.read(frame.clone());
    assert_eq!(f.unallowed(&site, false), vec![frame.clone()]);
    assert_eq!(f.unallowed(&frame, false), vec![site.clone()]);
    // Each direction is its own flow.
    f.allow(&frame, std::slice::from_ref(&site));
    assert!(f.unallowed(&frame, false).is_empty());
    assert_eq!(f.unallowed(&site, false), vec![frame.clone()]);
    let mut c = Confinement::new();
    c.read(app.clone());
    c.read(web.clone());
    // Into the app: the web content it shows is not a source; into the web content, the
    // app's own content is.
    assert!(c.unallowed(&app, false).is_empty());
    assert_eq!(c.unallowed(&web, false), vec![app.clone()]);
    assert_eq!(web.display(|_| Some("Acme".into())), "web content in Acme");
    assert_eq!(frame.display(|_| None), "embedded content in bank.example");
}

#[test]
fn a_self_sourced_tool_and_recency() {
    let tool = FlowContext::Tool("fetch".into());
    let msgs = FlowContext::app("com.apple.MobileSMS");
    let mut c = Confinement::new();
    c.read(tool.clone());
    c.read(msgs.clone());
    // A tool that is not self-contained is a source for itself; most recent first.
    assert_eq!(
        c.unallowed_into(std::slice::from_ref(&tool), false, true),
        vec![msgs.clone(), tool.clone()]
    );
    assert_eq!(
        c.unallowed_into(std::slice::from_ref(&tool), false, false),
        vec![msgs.clone()]
    );
    c.read(tool.clone());
    assert_eq!(
        c.unallowed_into(std::slice::from_ref(&tool), false, true),
        vec![tool.clone(), msgs.clone()]
    );
    // Nothing is kept for an unknown destination.
    c.allow(&FlowContext::Unknown, std::slice::from_ref(&msgs));
    assert_eq!(c.unallowed(&FlowContext::Unknown, false).len(), 2);
    // A declined source blocks every flow that would carry it.
    c.decline(&tool, std::slice::from_ref(&msgs));
    assert!(c.is_declined(&tool, &[tool.clone(), msgs]));
}

fn raw(app: &str, kids: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("App")
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(kids)
        ]);
    jarvis_proto::fixtures::screen(app, "App", root, false)
}

/// The screen's own place: a web view beside listed content in an app without a readable
/// host is embedded (the screen is the app); one web view alone stays "site not readable".
#[test]
fn the_screens_own_place() {
    let web = || {
        UiNode::new(Role::WebView, rect(0.0, 400.0, 393.0, 300.0)).with_children(vec![node(
            Role::Button,
            "Ad",
            rect(16.0, 420.0, 300.0, 40.0),
        )])
    };
    let mixed = raw(
        "com.example.acme",
        vec![text("Balance", rect(16.0, 140.0, 300.0, 30.0)), web()],
    );
    let alone = raw("com.example.acme", vec![web()]);
    let ctx =
        |s: &Screen| observed_context(s, &jarvis_observe_free_compact(s), ScreenSource::Typed);
    assert_eq!(ctx(&mixed), Some(FlowContext::app("com.example.acme")));
    assert_eq!(
        ctx(&alone),
        Some(FlowContext::SiteUnreadable("com.example.acme".into()))
    );
}

/// `observed_context` reads the raw tree; the compact screen matters only on SpringBoard.
fn jarvis_observe_free_compact(s: &Screen) -> CompactScreen {
    screen(&s.app_bundle_id, "App", Vec::new())
}

/// Seventeenth review (gate.rs): an egress Rule T leaves to another rule is not switched off
/// by an empty confirm list — a link the owner did not give, once anything was read; an
/// insert or hand-on control of an edit menu, always.
#[test]
fn forced_requests_ask_whatever_the_policy_relaxes() {
    let nothing = Policy {
        confirm: Vec::new(),
        ..Policy::default()
    };
    let s = screen(
        "com.apple.mobilesafari",
        "Safari",
        vec![el(1, Role::Other("MenuItem".into()), "Translate")],
    );
    let open = Action::OpenUrl {
        url: "https://evil.example/?c=4821".into(),
    };
    let owner = ["Read my code"];
    // Nothing read: the owner who confirms nothing keeps that meaning.
    assert_eq!(decide(&nothing, &open, &s, &owner, false), Decision::Allow);
    // Something read: asked, phone only.
    match decide(&nothing, &open, &s, &owner, true) {
        Decision::Confirm(r) => assert!(r.phone_only),
        other => panic!("{other:?}"),
    }
    match decide(&nothing, &Action::Tap { element: 1 }, &s, &owner, false) {
        Decision::Confirm(r) => assert!(r.phone_only),
        other => panic!("{other:?}"),
    }
    // The gate itself.
    let gate = |forced, unclear| {
        gate_owned(
            &nothing,
            RiskCategory::SendMessage,
            TrustLevel::Guarded,
            &open,
            &s,
            false,
            unclear,
            forced,
        )
    };
    assert_eq!(gate(false, true), Gate::Allow);
    assert_eq!(gate(true, true), Gate::Confirm);
    assert_eq!(gate(true, false), Gate::Confirm);
}

/// No "and N more": as many names as fit, and the count of the rest asked next.
#[test]
fn the_flow_note_names_what_the_approval_allows() {
    let few: Vec<String> = [
        "Messages",
        "Mail",
        "a.example",
        "b.example",
        "Notes",
        "Photos",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let (row, n) = flow_note_fit(&few);
    assert_eq!(n, 6);
    assert!(!row.text.contains("more"), "{}", row.text);
    let many: Vec<String> = (0..12).map(|i| format!("{i} {}", "x".repeat(30))).collect();
    let (row, n) = flow_note_fit(&many);
    assert!((1..12).contains(&n), "{n}");
    assert!(
        row.text
            .contains(&format!("it read {} more places in this task", 12 - n)),
        "{}",
        row.text
    );
    for (i, name) in many.iter().enumerate() {
        let shown = row.text.contains(&format!("{i} x"));
        assert_eq!(shown, i < n, "{name}: {}", row.text);
    }
}

fn menu(labels: &[&str]) -> CompactScreen {
    let mut els = vec![el(1, Role::StaticText, "Locker combination 1234")];
    for (k, l) in labels.iter().enumerate() {
        els.push(el(k as u32 + 2, Role::Other("MenuItem".into()), l));
    }
    screen("com.apple.mobilenotes", "Notes", els)
}

/// Edit-menu items: only local actions by whole label are unasked; everything else that may
/// hand the selection on asks, phone only, naming the place; unknown items ask too.
#[test]
fn edit_menu_items_that_hand_the_selection_on_ask() {
    let local = [
        "Copy",
        "Cut",
        "Select",
        "Select All",
        "Bold",
        "Italic",
        "Underline",
        "Format",
    ];
    let away = [
        "Search Web",
        "Translate",
        "Look Up",
        "Share…",
        "Open in Safari",
        "Speak",
        "New Quick Note",
        "Frobnicate",
    ];
    let mut labels: Vec<&str> = local.to_vec();
    labels.extend(away);
    let s = menu(&labels);
    for (k, l) in labels.iter().enumerate() {
        let tap = Action::Tap {
            element: k as u32 + 2,
        };
        let hands = handoff_control(&tap, &s, None);
        assert_eq!(hands, away.contains(l), "{l}");
        let d = decide(&Policy::default(), &tap, &s, &["Open my note"], false);
        if away.contains(l) {
            match d {
                Decision::Confirm(r) => {
                    assert!(r.phone_only, "{l}");
                    assert!(
                        r.summary
                            .contains("may send the selected content to another app or service; it was read in Notes"),
                        "{l}: {}",
                        r.summary
                    );
                }
                other => panic!("{l}: {other:?}"),
            }
        } else if *l != "Cut" {
            assert_eq!(d, Decision::Allow, "{l}");
        }
    }
    // Outside a menu a "Translate" button is an ordinary control; a button that opens a
    // menu is not a menu item.
    let s = screen(
        "com.apple.mobilenotes",
        "Notes",
        vec![
            el(1, Role::Button, "Translate"),
            el(2, Role::Other("MenuButton".into()), "More"),
        ],
    );
    assert!(!handoff_control(&Action::Tap { element: 1 }, &s, None));
    assert!(!handoff_control(&Action::Tap { element: 2 }, &s, None));
    // A scroll presses nothing.
    let s = menu(&["Search Web"]);
    assert!(!handoff_control(
        &Action::Scroll {
            direction: Direction::Down,
            element: None
        },
        &s,
        None
    ));
}

/// The share sheet's rows already ask (a person row sends on one tap).
#[test]
fn share_sheet_rows_ask() {
    let s = screen(
        "com.apple.mobilenotes",
        "Notes",
        vec![
            el(1, Role::StaticText, "Locker combination 1234"),
            el(2, Role::Cell, "AirDrop"),
            el(3, Role::Cell, "Mallory’s iPhone"),
            el(4, Role::Cell, "Messages"),
        ],
    );
    for i in [3, 4] {
        assert!(
            matches!(
                decide(
                    &Policy::default(),
                    &Action::Tap { element: i },
                    &s,
                    &["Open my note"],
                    false
                ),
                Decision::Confirm(_)
            ),
            "{i}"
        );
    }
}
