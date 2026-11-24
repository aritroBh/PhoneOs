#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Nineteenth review (agent): the conveniences of earlier rounds removed — no one-way relation
//! (R-A), embedded classes covering from their first observation (R-B), a browser's chrome is
//! the browser (R-C), a Spotlight row is an app launch only when it is exactly that (R-E), a
//! share sheet's controls always ask (N4, the decidable part), `press_home` goes to every
//! place it leaves. The review's probes are in `nineteenth_review.rs`.

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{AppInfo, Role, RunnerCommand, Screen, SessionConfig, UiNode};
use jarvis_safety::{FlowContext, COVERS_ALL};

const FLOW: &str = "what it read there could be passed on here";
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const MAIL: &str = "com.apple.mobilemail";
const SAFARI: &str = "com.apple.mobilesafari";
const NOTES: &str = "com.apple.mobilenotes";
const HOME: &str = "com.apple.springboard";
const MAIL_WEB: &str = "all web content in Mail (Jarvis cannot tell its parts apart)";

fn cfg() -> SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new(MAIL, "Mail"));
    c.known_apps.push(AppInfo::new(SAFARI, "Safari"));
    c.known_apps.push(AppInfo::new(NOTES, "Notes"));
    c
}

fn scr(app: &str, name: &str, children: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    screen(app, name, root, false)
}

fn tap(i: u32) -> String {
    format!(r#"{{"action":"tap","element":{i}}}"#)
}

fn flows(r: &Run) -> Vec<&jarvis_proto::ConfirmRequest> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .collect()
}

fn taps_sent(r: &Run) -> usize {
    r.acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
        .count()
}

// ---- Mail: the inbox, two message bodies --------------------------------------------------

fn inbox() -> Screen {
    scr(
        MAIL,
        "Mail",
        vec![
            node(Role::Cell, "Bank, Your code", rect(0.0, 100.0, 393.0, 60.0)),
            node(Role::Cell, "Shop, Deals", rect(0.0, 170.0, 393.0, 60.0)),
        ],
    )
}

fn message(sender: &str, body: &str, link: &str) -> Screen {
    scr(
        MAIL,
        "Mail",
        vec![
            node(Role::Button, "Inbox", rect(8.0, 50.0, 80.0, 40.0)),
            text(sender, rect(16.0, 100.0, 361.0, 30.0)),
            UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![
                text(body, rect(16.0, 160.0, 361.0, 30.0)),
                node(Role::Link, link, rect(16.0, 220.0, 200.0, 40.0)),
            ]),
        ],
    )
}

fn message_a() -> Screen {
    message("Bank", "Your sign-in code is 482913", "Manage alerts")
}

fn message_b() -> Screen {
    message("Shop", "Great deals inside", "Shop now")
}

/// Read the inbox, open message A, tap inside A's body, go back, open message B, tap
/// inside B's body. By the rule: the tap inside A asks once (Mail into the web content,
/// and the web content — covering all of Mail's message bodies — into itself); going back
/// asks once (the web content into Mail: no one-way relation); opening B asks nothing (no
/// new source); the tap inside B asks nothing — B's body is part of the same covering class,
/// whose approval said it covers all of it, and every source read (Mail, the web content)
/// was approved into it.
#[test]
fn mail_bodies_ask_by_the_rule() {
    let row_a = index_of(&inbox(), "Bank, Your code");
    let row_b = index_of(&inbox(), "Shop, Deals");
    let link_a = index_of(&message_a(), "Manage alerts");
    let back = index_of(&message_a(), "Inbox");
    let link_b = index_of(&message_b(), "Shop now");
    let phone = Phone::new(inbox(), |cmd, s| {
        if taps(cmd, s, "Bank, Your code") {
            Transition::Go(message_a())
        } else if taps(cmd, s, "Shop, Deals") {
            Transition::Go(message_b())
        } else if taps(cmd, s, "Inbox") {
            Transition::Go(inbox())
        } else {
            Transition::Stay
        }
    });
    let r = run(
        cfg(),
        "Read my mail and open the links",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &tap(row_a),
            &tap(link_a),
            &tap(back),
            &tap(row_b),
            &tap(link_b),
            DONE,
        ])
        .decisions(&[true, true]),
    );
    let fq = flows(&r);
    let summaries: Vec<&String> = fq.iter().map(|c| &c.summary).collect();
    assert_eq!(fq.len(), 2, "{summaries:#?}");
    assert!(
        fq[0].summary.starts_with("Tap \"Manage alerts\"")
            && fq[0]
                .summary
                .contains(&format!("Jarvis has read Mail and {MAIL_WEB} in this task")),
        "{}",
        fq[0].summary
    );
    assert!(
        fq[1].summary.starts_with("Tap \"Inbox\"")
            && fq[1]
                .summary
                .contains(&format!("Jarvis has read {MAIL_WEB} in this task")),
        "{}",
        fq[1].summary
    );
    assert!(fq.iter().all(|c| c.phone_only));
    assert_eq!(taps_sent(&r), 5, "{:?}", r.acts);
}

/// R-A, the other way: reading a body and then tapping a row of the inbox (another party's
/// message, whose remote content loads when it opens) asks, naming the web content; a
/// decline sends nothing.
#[test]
fn a_native_tap_after_reading_a_body_asks_and_a_decline_sends_nothing() {
    let back = index_of(&message_a(), "Inbox");
    let r = run(
        cfg(),
        "Read my mail",
        MemoryStore::new(),
        Phone::still(message_a()),
        Script::model(&[&tap(back), DONE]).decisions(&[false]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(fq[0].summary.contains(MAIL_WEB), "{}", fq[0].summary);
    assert_eq!(taps_sent(&r), 0, "{:?}", r.acts);
}

// ---- Safari: the chrome is the browser ----------------------------------------------------

fn safari(host: &str, page: Vec<UiNode>) -> Screen {
    scr(
        SAFARI,
        "Safari",
        vec![
            UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 600.0)).with_children(page),
            node(Role::Button, "Back", rect(8.0, 800.0, 48.0, 40.0)),
            UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
                .with_label("Address")
                .with_value(host),
        ],
    )
}

fn page_a() -> Screen {
    safari(
        "a.example",
        vec![
            text("Your order number is 5521", rect(16.0, 120.0, 361.0, 30.0)),
            node(Role::Link, "Read more", rect(16.0, 200.0, 200.0, 40.0)),
        ],
    )
}

fn page_b() -> Screen {
    safari(
        "b.example",
        vec![
            text("Welcome to b", rect(16.0, 120.0, 361.0, 30.0)),
            node(Role::Link, "Go", rect(16.0, 200.0, 200.0, 40.0)),
        ],
    )
}

/// Read a page, tap Back (Safari's chrome: asks once, naming the page's site); tap Back again
/// (no new source: no question) — it loads another site; tap its link (asks once, naming
/// Safari, whose chrome was read, and the first site).
#[test]
fn safari_chrome_is_the_browser() {
    let back = index_of(&page_a(), "Back");
    let go = index_of(&page_b(), "Go");
    let mut backs = 0;
    let phone = Phone::new(page_a(), move |cmd, s| {
        if taps(cmd, s, "Back") {
            backs += 1;
            if backs == 2 {
                return Transition::Go(page_b());
            }
        }
        Transition::Stay
    });
    let r = run(
        cfg(),
        "Check my order",
        MemoryStore::new(),
        phone,
        Script::model(&[&tap(back), &tap(back), &tap(go), DONE]).decisions(&[true, true]),
    );
    let fq = flows(&r);
    let summaries: Vec<&String> = fq.iter().map(|c| &c.summary).collect();
    assert_eq!(fq.len(), 2, "{summaries:#?}");
    assert!(
        fq[0].summary.starts_with("Tap \"Back\"")
            && fq[0]
                .summary
                .contains("Jarvis has read a.example in this task"),
        "{}",
        fq[0].summary
    );
    assert!(
        fq[1].summary.starts_with("Tap \"Go\"")
            && fq[1]
                .summary
                .contains("Jarvis has read Safari and a.example in this task"),
        "{}",
        fq[1].summary
    );
    assert_eq!(taps_sent(&r), 3, "{:?}", r.acts);
}

/// A page whose address field is the only thing in the chrome: the chrome shows nothing of
/// its own (the address is the page's own name), so typing into the page after reading only
/// it asks nothing; tapping the address field (an input into Safari) asks, naming the site.
#[test]
fn the_address_field_alone_is_not_chrome_content_but_an_input_into_the_browser() {
    let only =
        scr(
            SAFARI,
            "Safari",
            vec![
                UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 600.0)).with_children(vec![
                    node(Role::Button, "Next", rect(16.0, 200.0, 361.0, 44.0)),
                ]),
                UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
                    .with_label("Address")
                    .with_value("a.example"),
            ],
        );
    let next = index_of(&only, "Next");
    let address = index_of(&only, "Address");
    let r = run(
        cfg(),
        "Go on",
        MemoryStore::new(),
        Phone::still(only.clone()),
        Script::model(&[&tap(next), DONE]),
    );
    assert!(flows(&r).is_empty(), "{:?}", r.confirms);
    assert_eq!(taps_sent(&r), 1);
    let r = run(
        cfg(),
        "Go on",
        MemoryStore::new(),
        Phone::still(only),
        Script::model(&[&tap(address), DONE]).decisions(&[false]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains("Jarvis has read a.example in this task"),
        "{}",
        fq[0].summary
    );
    assert_eq!(taps_sent(&r), 0);
}

/// `press_home` goes to every place on the screen it leaves: going home from a page whose
/// chrome shows content of its own (the Back button) is an input into the site and into
/// Safari, each a source for the other (was: into the site alone, no question).
#[test]
fn press_home_goes_to_every_place_it_leaves() {
    let p = page_a();
    let r = run(
        cfg(),
        "Check my order, then go home",
        MemoryStore::new(),
        Phone::new(p.clone(), |_, _| Transition::Stay),
        Script::model(&[r#"{"action":"press_home"}"#, DONE]).decisions(&[true]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    // Safari's chrome into the site and the site into the chrome: both named.
    assert!(
        fq[0]
            .summary
            .contains("Jarvis has read Safari and a.example in this task"),
        "{}",
        fq[0].summary
    );
}

// ---- Spotlight rows -------------------------------------------------------------------

fn spotlight(row: &str) -> Screen {
    let banner = UiNode::new(
        Role::Other("NotificationBanner".to_owned()),
        rect(8.0, 50.0, 377.0, 80.0),
    )
    .with_label("Messages, now, Your sign-in code is 482913");
    scr(
        HOME,
        "Home",
        vec![
            banner,
            UiNode::new(Role::SearchField, rect(16.0, 150.0, 361.0, 40.0))
                .with_label("Search")
                .focused(true)
                .hittable(true),
            UiNode::new(Role::Cell, rect(0.0, 220.0, 393.0, 60.0))
                .with_label(row)
                .hittable(true),
        ],
    )
}

/// R-E: a Spotlight row is the Settings app only when its whole label is the app's name or
/// the name and the OS's word for an app result; anything else (a website titled like the
/// app, a URL, any extra part) is a launch Jarvis cannot name, which keeps the home screen
/// (read: its banner) as a source — asked, whatever the policy relaxes.
#[test]
fn spotlight_rows_are_app_launches_only_when_exactly_that() {
    let launches = |label: &str| {
        let s = spotlight(label);
        let row = index_of(&s, label);
        let mut c = cfg();
        c.policy.confirm = Vec::new();
        run(
            c,
            "Open Settings",
            MemoryStore::new(),
            Phone::still(s),
            Script::model(&[&tap(row), DONE]).decisions(&[false, false]),
        )
    };
    for label in ["Settings", "settings", "Settings, App", "SETTINGS, app"] {
        let r = launches(label);
        assert!(flows(&r).is_empty(), "{label:?}: {:?}", r.confirms);
        assert_eq!(taps_sent(&r), 1, "{label:?}");
    }
    for label in [
        "Settings, Website, settings-help.example",
        "Settings, Website",
        "Settings, settings-help.example",
        "Settings, https://settings-help.example/",
        "Settings, App, settings-help.example",
        "Settings Help",
    ] {
        let r = launches(label);
        assert!(!flows(&r).is_empty(), "{label:?}: {:?}", r.confirms);
        assert_eq!(taps_sent(&r), 0, "{label:?}");
    }
}

// ---- Share sheets (N4, the decidable part) ------------------------------------------------

fn notes_with_share_sheet() -> Screen {
    scr(
        NOTES,
        "Notes",
        vec![
            text("Locker combination 1234", rect(16.0, 140.0, 361.0, 40.0)),
            UiNode::new(Role::Sheet, rect(0.0, 400.0, 393.0, 452.0)).with_children(vec![
                node(Role::Cell, "AirDrop", rect(16.0, 420.0, 80.0, 90.0)),
                node(
                    Role::Cell,
                    "Mallory’s iPhone",
                    rect(110.0, 420.0, 80.0, 90.0),
                ),
                node(Role::Cell, "Copy", rect(16.0, 600.0, 361.0, 44.0)),
                node(Role::Cell, "Print", rect(16.0, 650.0, 361.0, 44.0)),
                node(Role::Button, "Close", rect(340.0, 410.0, 40.0, 40.0)),
            ]),
        ],
    )
}

/// N4: a third-party action extension chooses its own title, so a press on ANY control
/// inside a share sheet (a `Sheet` holding the OS's "AirDrop" row) asks, phone only, even
/// under `confirm: []` and with only this app read — "Copy", "Print" and "Close" included.
#[test]
fn share_sheet_controls_always_ask() {
    let s = notes_with_share_sheet();
    for label in ["Copy", "Print", "Close", "Mallory’s iPhone", "AirDrop"] {
        let i = index_of(&s, label);
        let mut c = cfg();
        c.policy.confirm = Vec::new();
        let r = run(
            c,
            "Share my note",
            MemoryStore::new(),
            Phone::still(s.clone()),
            Script::model(&[&tap(i), DONE]).decisions(&[false]),
        );
        assert_eq!(r.confirms.len(), 1, "{label}: {:?}", r.confirms);
        assert!(r.confirms[0].phone_only, "{label}");
        assert_eq!(taps_sent(&r), 0, "{label}");
    }
}

/// An "AirDrop" row outside any sheet (Settings › General lists one) makes nothing a share
/// sheet: a control there is not asked for it. (While a sheet is up only its subtree is
/// listed, so no control outside a share sheet can be pressed beside it.)
#[test]
fn an_airdrop_row_outside_a_sheet_is_not_a_share_sheet() {
    let s = scr(
        "com.apple.Preferences",
        "Settings",
        vec![
            node(Role::Cell, "About", rect(0.0, 100.0, 393.0, 44.0)),
            node(Role::Cell, "AirDrop", rect(0.0, 150.0, 393.0, 44.0)),
        ],
    );
    let about = index_of(&s, "About");
    let mut c = cfg();
    c.policy.confirm = Vec::new();
    let r = run(
        c,
        "Open About",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[&tap(about), DONE]),
    );
    assert!(r.confirms.is_empty(), "{:?}", r.confirms);
    assert_eq!(taps_sent(&r), 1);
}

// ---- R-D: the host only from the browser's one address field ------------------------------

fn places_of(s: &Screen) -> jarvis_safety::ScreenPlaces {
    let v = jarvis_observe::compact_view(s);
    jarvis_safety::observed_places(
        s,
        &v.screen,
        jarvis_safety::ScreenSource::Typed,
        Some((&v.webviews, &v.ends, &v.sources)),
    )
}

fn browser_with(chrome: Vec<UiNode>) -> Screen {
    let mut kids = vec![UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 600.0))
        .with_children(vec![text("Hello", rect(16.0, 120.0, 361.0, 30.0))])];
    kids.extend(chrome);
    scr(SAFARI, "Safari", kids)
}

fn field(value: &str) -> UiNode {
    UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
        .with_label("Address")
        .with_value(value)
}

fn site(host: &str) -> FlowContext {
    FlowContext::Site {
        app: SAFARI.into(),
        host: host.into(),
    }
}

fn unreadable() -> FlowContext {
    FlowContext::SiteUnreadable(SAFARI.into())
}

/// R-D: a host is read from the address field only — `TextField "Address"` after the page
/// loaded (the real tree); the simulator's pre-load `StaticText "127.0.0.1"` with no label
/// inside an `Other`, a tab title carrying only the `TabBarItemTitle` identifier, a field
/// being edited, an IDN host and two address fields on screen are not readable, and then
/// the page is the browser's own covering class, never a site; an address node wholly off
/// the screen (not listed, never read) does not count.
#[test]
fn the_host_comes_only_from_the_one_address_field_on_screen() {
    let safari_app = FlowContext::app(SAFARI);
    // After load: the site; the field alone is not chrome content.
    let loaded = places_of(&browser_with(vec![field("\u{200E}127.0.0.1")]));
    assert_eq!(loaded.read_places(), vec![site("127.0.0.1")]);
    assert_eq!(loaded.base(), &safari_app);
    // Before load: the address as a static text in an `Other`, no label.
    let pre = places_of(&browser_with(vec![UiNode::new(
        Role::Other("Other".into()),
        rect(100.0, 800.0, 200.0, 30.0),
    )
    .with_children(vec![text("127.0.0.1", rect(110.0, 805.0, 100.0, 20.0))])]));
    assert_eq!(pre.read_places(), vec![safari_app.clone(), unreadable()]);
    assert_eq!(pre.ambiguous(), &[unreadable()]);
    let unreadable_cases: Vec<(&str, Vec<UiNode>)> = vec![
        (
            "a tab title by identifier only",
            vec![
                UiNode::new(Role::StaticText, rect(100.0, 800.0, 200.0, 30.0))
                    .with_label("bank.example")
                    .with_value("bank.example")
                    .with_identifier("TabBarItemTitle"),
            ],
        ),
        (
            "a field being edited",
            vec![field("attacker.example").focused(true)],
        ),
        ("an IDN host", vec![field("bänk.example")]),
        (
            "two address fields on screen",
            vec![
                field("bank.example"),
                UiNode::new(Role::TextField, rect(100.0, 760.0, 200.0, 30.0))
                    .with_label("Address")
                    .with_value("bank.example"),
            ],
        ),
        (
            "a static text labelled Address",
            vec![
                UiNode::new(Role::StaticText, rect(100.0, 800.0, 200.0, 30.0))
                    .with_label("Address")
                    .with_value("bank.example"),
            ],
        ),
    ];
    for (what, chrome) in unreadable_cases {
        let p = places_of(&browser_with(chrome));
        let all = p.read_places();
        assert!(all.contains(&unreadable()), "{what}: {all:?}");
        assert!(
            !all.iter().any(|c| matches!(c, FlowContext::Site { .. })),
            "{what}: {all:?}"
        );
    }
    // A second address node wholly off the screen is not shown and does not count.
    let off = places_of(&browser_with(vec![
        field("bank.example"),
        UiNode::new(Role::TextField, rect(400.0, 800.0, 200.0, 30.0))
            .with_label("Address")
            .with_value("attacker.example"),
    ]));
    assert_eq!(off.read_places(), vec![site("bank.example")]);
}

/// The covering name never holds ", " or " and " (a list of names stays readable, also to
/// the shell's own parser), and it is the name every flow note uses.
#[test]
fn the_covering_name_has_no_list_separators() {
    assert!(!MAIL_WEB.contains(", ") && !MAIL_WEB.contains(" and "));
    assert!(MAIL_WEB.ends_with(COVERS_ALL));
}
