#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Run 122 (iOS 26.5 simulator, `SystemAppAgentTests.testSafariFillsAndSubmitsAForm`): while
//! the keyboard is up Safari minimizes its bar, and the address node it reads the host from
//! becomes a plain `Other "Address" value="‎127.0.0.1" id=TabBarItemTitle`. The page's place
//! flipped from the site to Safari's pages with no readable address in the middle of a form,
//! and the second typing asked a flow question naming the host. The address field
//! (`jarvis_safety::flow::address_field`) now also is an `Other` with exactly that identifier
//! and an address label of its own; every other condition is unchanged.
//!
//! The trees are the runner's real typed trees from that run: the keyboard-up one is the
//! observe crate's fixture `ios26_extra/safari_form_keyboard`, the others are under
//! `tests/fixtures/safari_form/` (observation numbers of the run).

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::{
    AppInfo, CompactScreen, ConfirmRequest, OutcomeStatus, RiskCategory, Role, RunnerCommand,
    Screen, SessionConfig, UiNode,
};
use jarvis_safety::{FlowContext, ScreenPlaces};

/// What `ios` (the observe tests' parser) uses to print a compact screen.
fn compact_text(c: &CompactScreen) -> String {
    format!("{c:?}")
}

const SAFARI: &str = "com.apple.mobilesafari";
const HOST: &str = "127.0.0.1";
const FLOW: &str = "what it read there could be passed on here";
const UNREADABLE: &str =
    "all pages in Safari with no readable address (Jarvis cannot tell its parts apart)";

const KEYBOARD: &str =
    include_str!("../../jarvis-observe/tests/fixtures/ios26_extra/safari_form_keyboard.txt");
const OBS01_HOME: &str = include_str!("fixtures/safari_form/01.txt");
const OBS02_PRELOAD: &str = include_str!("fixtures/safari_form/02.txt");
const OBS03_PRELOAD: &str = include_str!("fixtures/safari_form/03.txt");
const OBS04_TIP: &str = include_str!("fixtures/safari_form/04.txt");
const OBS06_FORM: &str = include_str!("fixtures/safari_form/06.txt");

fn parse(text: &str) -> Screen {
    ios::parse_screen(text).unwrap()
}

/// Observation 9: the form loaded, "Full name" typed, keyboard up, the bar minimized.
fn keyboard_up() -> Screen {
    parse(KEYBOARD)
}

fn places(s: &Screen) -> ScreenPlaces {
    let v = jarvis_observe::compact_view(s);
    jarvis_safety::observed_places(
        s,
        &v.screen,
        jarvis_safety::ScreenSource::Typed,
        Some((&v.webviews, &v.ends, &v.sources)),
    )
}

fn safari() -> FlowContext {
    FlowContext::app(SAFARI)
}

fn site() -> FlowContext {
    FlowContext::Site {
        app: SAFARI.into(),
        host: HOST.into(),
    }
}

fn unreadable() -> FlowContext {
    FlowContext::SiteUnreadable(SAFARI.into())
}

/// Applies `f` to every node of the tree.
fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

/// Removes every node (with its subtree) for which `drop` holds.
fn remove(n: &mut UiNode, drop: &impl Fn(&UiNode) -> bool) {
    n.children.retain(|c| !drop(c));
    for c in &mut n.children {
        remove(c, drop);
    }
}

/// The first node for which `p` holds.
fn find_mut<'a>(n: &'a mut UiNode, p: &impl Fn(&UiNode) -> bool) -> Option<&'a mut UiNode> {
    if p(n) {
        return Some(n);
    }
    n.children.iter_mut().find_map(|c| find_mut(c, p))
}

fn is_bar_address(n: &UiNode) -> bool {
    n.identifier.as_deref() == Some("TabBarItemTitle")
}

/// The page's own form (inside the innermost web view).
fn is_page_form(n: &UiNode) -> bool {
    matches!(n.role, Role::Other(_)) && n.label.as_deref() == Some("form")
}

fn assert_unreadable(what: &str, p: &ScreenPlaces) {
    let all = p.read_places();
    assert!(all.contains(&unreadable()), "{what}: {all:?}");
    assert!(
        !all.iter().any(|c| matches!(c, FlowContext::Site { .. })),
        "{what}: {all:?}"
    );
}

// ---- places of the real trees ---------------------------------------------------------------

/// The real keyboard-up tree reads Safari (its keyboard toolbar's Previous / Next / Done are
/// chrome content) and the site; the minimized address node is the page's name.
#[test]
fn the_keyboard_up_tree_is_safari_and_the_site() {
    let s = keyboard_up();
    let p = places(&s);
    assert_eq!(p.read_places(), vec![safari(), site()]);
    assert_eq!(p.base(), &safari());
    let c = jarvis_observe::compact(&s);
    assert_eq!(
        jarvis_safety::observed_context(&s, &c, jarvis_safety::ScreenSource::Typed),
        Some(site())
    );
    // The same page with the keyboard hidden (observation 6: `TextField "Address"`) is the
    // same site: the place does not flip when the keyboard comes up.
    let hidden = places(&parse(OBS06_FORM));
    assert_eq!(hidden.read_places(), vec![safari(), site()]);
}

/// With nothing else in the chrome than the minimized address node, the chrome shows no
/// content of its own: the node is the page's name, as the address text field is.
#[test]
fn the_minimized_address_alone_does_not_make_the_chrome_a_read() {
    let mut s = keyboard_up();
    remove(&mut s.root, &|n| n.identifier.as_deref() == Some("Toolbar"));
    let c = jarvis_observe::compact(&s);
    assert!(
        c.elements
            .iter()
            .all(|e| !matches!(e.label.as_str(), "Previous" | "Next" | "Done")),
        "{:?}",
        c.elements
    );
    assert_eq!(places(&s).read_places(), vec![site()]);
}

/// An `Other "Address"` without the identifier is not the address field.
#[test]
fn an_other_labelled_address_without_the_identifier_is_not_read() {
    let mut s = keyboard_up();
    let n = find_mut(&mut s.root, &is_bar_address).unwrap();
    n.identifier = None;
    assert_unreadable("no identifier", &places(&s));
    // Nor with another identifier browsers use.
    let mut s = keyboard_up();
    find_mut(&mut s.root, &is_bar_address).unwrap().identifier = Some("URL".into());
    assert_unreadable("identifier URL", &places(&s));
}

/// Before the page loads, `Other id=TabBarItemTitle` has no label and a child
/// `Other "‎127.0.0.1"` > `StaticText "‎127.0.0.1"` (observations 2 and 3): unreadable, on the
/// real trees and built the same way in the keyboard-up tree.
#[test]
fn the_unlabelled_pre_load_node_stays_unreadable() {
    for (what, text) in [
        ("observation 2", OBS02_PRELOAD),
        ("observation 3", OBS03_PRELOAD),
    ] {
        let p = places(&parse(text));
        assert_eq!(p.read_places(), vec![safari(), unreadable()], "{what}");
    }
    let mut s = keyboard_up();
    let n = find_mut(&mut s.root, &is_bar_address).unwrap();
    let frame = n.frame;
    let host = "\u{200E}127.0.0.1";
    *n = UiNode::new(Role::Other("Other".into()), frame)
        .with_identifier("TabBarItemTitle")
        .with_children(vec![UiNode::new(Role::Other("Other".into()), frame)
            .with_label(host)
            .with_children(vec![
                UiNode::new(Role::StaticText, frame).with_label(host)
            ])]);
    assert_unreadable("pre-load node in the minimized bar", &places(&s));
}

/// A page-drawn `Other "Address" id=TabBarItemTitle` inside the web view is never read and
/// never counted: alone it leaves the page unreadable; beside the bar's node it changes
/// nothing.
#[test]
fn a_page_drawn_address_node_inside_the_web_view_is_ignored() {
    let drawn = |host: &str| {
        UiNode::new(
            Role::Other("Other".into()),
            jarvis_proto::Rect::new(195.3, 400.0, 49.3, 14.3),
        )
        .with_label("Address")
        .with_value(host)
        .with_identifier("TabBarItemTitle")
    };
    // Alone (the bar's node gone): unreadable, not the page's claimed host.
    let mut s = keyboard_up();
    remove(&mut s.root, &is_bar_address);
    find_mut(&mut s.root, &is_page_form)
        .unwrap()
        .children
        .push(drawn("bank.example"));
    let p = places(&s);
    assert_unreadable("page-drawn only", &p);
    assert!(
        !p.read_places()
            .iter()
            .any(|c| format!("{c:?}").contains("bank")),
        "{:?}",
        p.read_places()
    );
    // Beside the bar's node: still the bar's host.
    let mut s = keyboard_up();
    find_mut(&mut s.root, &is_page_form)
        .unwrap()
        .children
        .push(drawn("bank.example"));
    assert_eq!(places(&s).read_places(), vec![safari(), site()]);
}

/// Two address-like nodes on screen leave the page unreadable, whichever the second is.
#[test]
fn two_address_like_nodes_are_still_unreadable() {
    let second = |id: Option<&str>| {
        let mut n = UiNode::new(
            Role::Other("Other".into()),
            jarvis_proto::Rect::new(195.3, 500.0, 49.3, 14.3),
        )
        .with_label("Address")
        .with_value("\u{200E}127.0.0.1");
        if let Some(id) = id {
            n = n.with_identifier(id);
        }
        n
    };
    for (what, id) in [
        ("a second minimized address", Some("TabBarItemTitle")),
        ("an Other labelled Address", None),
    ] {
        let mut s = keyboard_up();
        let bar = find_mut(&mut s.root, &|n| {
            n.identifier
                .as_deref()
                .is_some_and(|i| i.starts_with("CapsuleNavigationBar"))
        })
        .unwrap();
        bar.children.push(second(id));
        assert_unreadable(what, &places(&s));
    }
}

// ---- the run, replayed ----------------------------------------------------------------------

/// The places a flow question names as read (as `SystemAppAgentTests.flowSources`).
fn flow_sources(c: &ConfirmRequest) -> Vec<String> {
    let s = &c.summary;
    let start = s.find("Jarvis has read ").unwrap() + "Jarvis has read ".len();
    let end = start + s[start..].find(" in this task;").unwrap();
    s[start..end]
        .replace(" and ", ", ")
        .split(", ")
        .map(str::to_owned)
        .collect()
}

/// `SystemAppAgentTests.isSafariTipDismissFlow`.
fn is_tip_flow(c: &ConfirmRequest) -> bool {
    c.summary.starts_with("Tap \"Close\"")
        && flow_sources(c).iter().all(|n| n == HOST || n == UNREADABLE)
}

/// `SystemAppAgentTests.isSafariLoadingFlow`.
fn is_loading_flow(c: &ConfirmRequest) -> bool {
    !c.summary.starts_with("Tap \"Close\"")
        && flow_sources(c)
            .iter()
            .all(|n| n == "Safari" || n == UNREADABLE)
}

fn cfg() -> SessionConfig {
    let mut c = first_use_config();
    c.known_apps.push(AppInfo::new(SAFARI, "Safari"));
    c
}

/// Sets the value and focus of the form's two text fields.
fn form(mut s: Screen, name: Option<&str>, email: Option<&str>, focus_email: bool) -> Screen {
    edit(&mut s.root, &mut |n| {
        if n.role == Role::TextField {
            match n.label.as_deref() {
                Some("Full name") => {
                    n.value = name.map(str::to_owned);
                    n.focused = !focus_email;
                }
                Some("Email address") => {
                    n.value = email.map(str::to_owned);
                    n.focused = focus_email;
                }
                _ => {}
            }
        }
    });
    s
}

/// The test's real sequence, from the captured trees: the home screen (1), `open_url`, the
/// page before it loads (2, 3: two waits), the first-launch tip (4: tap "Close"), the form
/// with the keyboard hidden (6: type the name — the tap brings the keyboard up, 8: observation
/// 9 without the typed name, the only difference in the capture), the keyboard-up look (9:
/// type the email), done. An owner who approves everything sees exactly the two flow
/// questions the UI test allows: one for the tip's "Close" and one at the first typing; the
/// second typing asks nothing (it asked naming the host before the fix).
#[test]
fn the_form_fill_asks_only_the_tip_and_the_first_typing() {
    let home = parse(OBS01_HOME);
    let s02 = parse(OBS02_PRELOAD);
    let s03 = parse(OBS03_PRELOAD);
    let s04 = parse(OBS04_TIP);
    let s06 = parse(OBS06_FORM);
    let s08 = form(keyboard_up(), None, None, false);
    let s09 = keyboard_up();
    let s09_email = form(keyboard_up(), Some("Ada Lovelace"), None, true);
    let s10 = form(
        keyboard_up(),
        Some("Ada Lovelace"),
        Some("ada@example.com"),
        true,
    );
    let close = index_of(&s04, "Close");
    // The elements the run's script typed into: [3] "Full name" on observation 6 and [5]
    // "Email address" on observation 9, both text fields.
    for (s, i, label) in [(&s06, 3u32, "Full name"), (&s09, 5, "Email address")] {
        let c = jarvis_observe::compact(s);
        let e = &c.elements[usize::try_from(i - 1).unwrap()];
        assert_eq!((&e.role, e.label.as_str()), (&Role::TextField, label));
    }
    let mut waits = 0;
    let phone = Phone::new(home, move |cmd, s| match cmd {
        RunnerCommand::OpenUrl { .. } => Transition::Go(s02.clone()),
        RunnerCommand::Wait { .. } => {
            waits += 1;
            Transition::Go(if waits == 1 { s03.clone() } else { s04.clone() })
        }
        RunnerCommand::TypeText { text, .. } if text == "Ada Lovelace" => {
            Transition::Go(s09.clone())
        }
        RunnerCommand::TypeText { text, .. } if text == "ada@example.com" => {
            Transition::Go(s10.clone())
        }
        _ if taps(cmd, s, "Close") => Transition::Go(s06.clone()),
        _ if !s.keyboard_visible && taps(cmd, s, "Full name") => Transition::Go(s08.clone()),
        _ if taps(cmd, s, "Email address") => Transition::Go(s09_email.clone()),
        _ => Transition::Stay,
    });
    let url = "http://127.0.0.1:59162/";
    let r = run(
        cfg(),
        &format!(
            "Open {url} in Safari and fill in the contact form with the name Ada Lovelace and \
             the email ada@example.com"
        ),
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(r#"{{"action":"open_url","url":"{url}"}}"#),
            r#"{"action":"wait","ms":1000}"#,
            r#"{"action":"wait","ms":1000}"#,
            &format!(r#"{{"action":"tap","element":{close}}}"#),
            r#"{"action":"type","element":3,"text":"Ada Lovelace","submit":false}"#,
            r#"{"action":"type","element":5,"text":"ada@example.com","submit":false}"#,
            r#"{"action":"done","summary":"Form filled"}"#,
        ])
        .decisions(&[true; 20]),
    );
    let asked: Vec<&str> = r.confirms.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{asked:#?}");
    let typed: Vec<&str> = r
        .acts
        .iter()
        .filter_map(|c| match c {
            RunnerCommand::TypeText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(typed, vec!["Ada Lovelace", "ada@example.com"], "{asked:#?}");
    // Every question is Safari's first use or a flow question.
    let flows: Vec<&ConfirmRequest> = r
        .confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .collect();
    for c in &r.confirms {
        assert!(
            c.category == RiskCategory::FirstUse || c.summary.contains(FLOW),
            "{asked:#?}"
        );
        assert_eq!(c.app, SAFARI, "{asked:#?}");
    }
    // Exactly two flow questions: the tip's "Close", then the first typing.
    assert_eq!(flows.len(), 2, "{asked:#?}");
    assert!(is_tip_flow(flows[0]), "{asked:#?}");
    assert!(is_loading_flow(flows[1]), "{asked:#?}");
    assert!(
        flows[1].summary.starts_with("Type \"Ada Lovelace\""),
        "{asked:#?}"
    );
    assert_eq!(flow_sources(flows[0]), vec![UNREADABLE], "{asked:#?}");
    assert_eq!(
        flow_sources(flows[1]),
        vec!["Safari", UNREADABLE],
        "{asked:#?}"
    );
    // The second typing asks nothing, and no question names the host as read.
    assert!(
        !r.confirms
            .iter()
            .any(|c| c.summary.contains("ada@example.com")),
        "{asked:#?}"
    );
    assert!(
        flows
            .iter()
            .all(|c| !flow_sources(c).iter().any(|n| n == HOST)),
        "{asked:#?}"
    );
}
