#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twentieth review fixes: R20-A (an approval kept never covers a stronger kind of input than
//! the one shown, and only the places the question named), R20-B (the share sheet from the
//! raw tree, sticky) and R20-C (names that cannot read as a list or as Jarvis's own words).

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    AppInfo, ConfirmVerb, Key, OutcomeStatus, Role, RunnerCommand, Screen, SessionConfig, UiNode,
};
use jarvis_safety::FlowContext;

const FLOW: &str = "what it read there could be passed on here";
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const SCROLL: &str = r#"{"action":"scroll","direction":"down"}"#;
const HOME_ACT: &str = r#"{"action":"press_home"}"#;
const MAIL: &str = "com.apple.mobilemail";
const SAFARI: &str = "com.apple.mobilesafari";
const NOTES: &str = "com.apple.mobilenotes";
const ACME: &str = "com.example.acme";
const SHOP: &str = "com.example.shop";

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

fn verbs(r: &Run) -> Vec<ConfirmVerb> {
    flows(r).iter().map(|c| c.verb).collect()
}

fn count(r: &Run, f: impl Fn(&RunnerCommand) -> bool) -> usize {
    r.acts.iter().filter(|a| f(a)).count()
}

fn is_tap(c: &RunnerCommand) -> bool {
    matches!(c, RunnerCommand::Tap { .. })
}

fn is_drag(c: &RunnerCommand) -> bool {
    matches!(c, RunnerCommand::Swipe { .. })
}

fn is_home(c: &RunnerCommand) -> bool {
    matches!(c, RunnerCommand::PressKey { key: Key::Home, .. })
}

// ---- Two plain apps: Acme (read first), then Shop ---------------------------------------

fn acme() -> Screen {
    scr(
        ACME,
        "Acme",
        vec![
            text("Your code is 4821", rect(16.0, 140.0, 361.0, 40.0)),
            node(Role::Button, "Open Shop", rect(16.0, 300.0, 361.0, 44.0)),
        ],
    )
}

fn shop() -> Screen {
    shop_at(0)
}

/// Shop after `n` inputs (a line at the bottom changes, so every input changes the screen).
fn shop_at(n: usize) -> Screen {
    let mut rows = vec![text("Deals", rect(16.0, 100.0, 361.0, 30.0))];
    for k in 0..6 {
        rows.push(node(
            Role::Button,
            &format!("Item {k}"),
            rect(16.0, 150.0 + 60.0 * k as f64, 361.0, 44.0),
        ));
    }
    rows.push(text(&format!("Viewed {n}"), rect(16.0, 800.0, 361.0, 20.0)));
    scr(SHOP, "Shop", rows)
}

/// Starts in Acme; "Open Shop" goes to Shop; a launch of Shop goes to Shop; every input in
/// Shop changes it; Home goes home, or (`home_stays`) changes Shop like any input.
fn two_apps(home_stays: bool) -> Phone {
    let mut n = 0usize;
    Phone::new(acme(), move |cmd, s| {
        n += 1;
        if is_home(cmd) && !home_stays {
            return Transition::Go(home_screen());
        }
        if matches!(cmd, RunnerCommand::LaunchApp { bundle_id } if bundle_id == SHOP) {
            return Transition::Go(shop_at(n));
        }
        if taps(cmd, s, "Open Shop") || s.app_bundle_id == SHOP {
            return Transition::Go(shop_at(n));
        }
        Transition::Stay
    })
}

/// R20-A rules 2 and 3: an approved scroll keeps a scroll-only grant — the second scroll is
/// silent, a tap still asks; that tap's approval keeps a full grant — later scrolls and taps
/// are silent.
#[test]
fn a_scroll_approval_covers_scrolls_only_then_a_tap_approval_covers_both() {
    let open = index_of(&acme(), "Open Shop");
    let item = index_of(&shop(), "Item 2");
    let r = run(
        cfg(),
        "Look through the shop",
        MemoryStore::new(),
        two_apps(false),
        Script::model(&[
            &tap(open),
            SCROLL,
            SCROLL,
            &tap(item),
            SCROLL,
            &tap(item),
            DONE,
        ])
        .decisions(&[true; 8]),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::Scroll, ConfirmVerb::Tap],
        "{:#?}",
        r.confirms
    );
    // One destination: the question says "here", as before.
    assert!(
        !flows(&r)[0].summary.contains(", into "),
        "{}",
        flows(&r)[0].summary
    );
    assert_eq!(count(&r, is_drag), 3);
    assert_eq!(count(&r, is_tap), 3);
}

/// R20-A rule 2 with several destinations (Mail's own place and its web content): the
/// question names both, the second scroll is silent, a native tap asks.
#[test]
fn a_scroll_with_two_destinations_names_both_and_keeps_scrolls_only() {
    let s0 = scr(
        MAIL,
        "Mail",
        vec![
            node(Role::Button, "Inbox", rect(8.0, 50.0, 80.0, 40.0)),
            text("Bank", rect(16.0, 100.0, 361.0, 30.0)),
            UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![text(
                "Your sign-in code is 482913",
                rect(16.0, 160.0, 361.0, 30.0),
            )]),
        ],
    );
    let inbox = index_of(&s0, "Inbox");
    let r = run(
        cfg(),
        "Read my bank's mail",
        MemoryStore::new(),
        Phone::still(s0),
        Script::model(&[SCROLL, SCROLL, &tap(inbox), DONE]).decisions(&[true; 4]),
    );
    let f = flows(&r);
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::Scroll, ConfirmVerb::Tap],
        "{f:#?}"
    );
    assert!(
        f[0].summary.contains(&format!(
            "{FLOW}, into Mail and all web content in Mail (Jarvis cannot tell its parts apart)"
        )) || f[0].summary.contains(&format!(
            "{FLOW}, into all web content in Mail (Jarvis cannot tell its parts apart) and Mail"
        )),
        "{}",
        f[0].summary
    );
    // The tap has one destination: "here".
    assert!(f[1].summary.ends_with(FLOW) || !f[1].summary.contains(", into "));
    assert_eq!(count(&r, is_drag), 2);
}

/// R20-A rule 6: a declined scroll sends nothing, and the same flow is blocked for the task.
#[test]
fn a_declined_scroll_blocks_the_flow() {
    let open = index_of(&acme(), "Open Shop");
    let r = run(
        cfg(),
        "Look through the shop",
        MemoryStore::new(),
        two_apps(false),
        Script::model(&[&tap(open), SCROLL, SCROLL, DONE]).decisions(&[false; 4]),
    );
    assert_eq!(flows(&r).len(), 1, "{:#?}", r.confirms);
    assert_eq!(count(&r, is_drag), 0);
    assert_eq!(r.outcome.status, OutcomeStatus::Blocked, "{:?}", r.outcome);
}

/// R20-A rule 4: a swipe asks every time and keeps nothing (a later tap asks too).
#[test]
fn a_swipe_asks_every_time_and_keeps_nothing() {
    let open = index_of(&acme(), "Open Shop");
    let item = index_of(&shop(), "Item 2");
    let swipe = r#"{"action":"swipe","direction":"up"}"#;
    let r = run(
        cfg(),
        "Look through the shop",
        MemoryStore::new(),
        two_apps(false),
        Script::model(&[&tap(open), swipe, swipe, &tap(item), DONE]).decisions(&[true; 8]),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::Swipe, ConfirmVerb::Swipe, ConfirmVerb::Tap],
        "{:#?} {:?} {}",
        r.confirms,
        r.acts,
        r.last_user()
    );
}

/// R20-A rule 4: `press_home` asked every time and kept nothing (here Home leaves the phone
/// in Shop). Twenty-third review (R23, rule changed): leaving is an observe-kind input into
/// every place it leaves, and its approval keeps an observe-only grant: the second Home is
/// not asked again (was: asked), and a tap after it still asks (an observe grant never
/// covers a tap).
#[test]
fn press_home_keeps_an_observe_grant_and_a_tap_still_asks() {
    let open = index_of(&acme(), "Open Shop");
    let item = index_of(&shop(), "Item 2");
    let r = run(
        cfg(),
        "Look through the shop",
        MemoryStore::new(),
        two_apps(true),
        Script::model(&[&tap(open), HOME_ACT, HOME_ACT, &tap(item), DONE]).decisions(&[true; 8]),
    );
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::PressHome, ConfirmVerb::Tap],
        "{:#?}",
        r.confirms
    );
    assert_eq!(count(&r, is_home), 2);
}

/// R20-A rule 4: a `tap_point` whose point lies over both an app's own place and its web
/// content asks for this action only, naming both.
#[test]
fn a_tap_point_with_two_destinations_keeps_nothing() {
    let s0 = scr(
        "com.example.reader",
        "Reader",
        vec![
            node(Role::Button, "Menu", rect(8.0, 50.0, 80.0, 40.0)),
            UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![
                text("Article", rect(16.0, 160.0, 361.0, 30.0)),
                node(Role::Button, "Read more", rect(16.0, 400.0, 361.0, 60.0)),
            ]),
        ],
    );
    let point = r#"{"action":"tap_point","x":0.5,"y":0.5}"#;
    let mut c = cfg();
    c.model.supports_vision = true;
    c.known_apps
        .push(AppInfo::new("com.example.reader", "Reader"));
    let r = run(
        c,
        "Read the article",
        MemoryStore::new(),
        Phone::still(s0),
        Script::model(&[
            r#"{"action":"look"}"#,
            point,
            r#"{"action":"look"}"#,
            point,
            DONE,
        ])
        .decisions(&[true; 4]),
    );
    // Twenty-third review (R23, rule changed): each look asks too — the first because the
    // app and its web content were each given nothing of the other, the second because of
    // the screen picture — so the flows are look, tap_point, look, tap_point (was: the two
    // tap_points only); each tap_point still asks.
    let f = flows(&r);
    assert_eq!(f.len(), 4, "{f:#?} {:?} {:?}", r.outcome, r.acts);
    let points: Vec<_> = f
        .iter()
        .filter(|q| q.verb == ConfirmVerb::TapPoint)
        .collect();
    assert_eq!(points.len(), 2, "{f:#?}");
    assert!(
        points[0].summary.contains(", into "),
        "{}",
        points[0].summary
    );
    assert_eq!(count(&r, is_tap), 2);
}

// ---- R20-A rule 5: naming every destination within the budget ----------------------------

const LONG_APP: &str = "com.example.longname";
const LONG_NAME: &str = "Shop $1000.00 $2000.00 $3000.00 $4000.00";

/// An app (not a browser) whose web content holds a nested web view: three places, each
/// with a long name.
fn three_places() -> Screen {
    scr(
        LONG_APP,
        LONG_NAME,
        vec![
            node(Role::Button, "Menu", rect(8.0, 50.0, 80.0, 40.0)),
            UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![
                text("Outer page", rect(16.0, 160.0, 361.0, 30.0)),
                UiNode::new(Role::WebView, rect(0.0, 400.0, 393.0, 200.0))
                    .with_children(vec![text("Inner frame", rect(16.0, 420.0, 361.0, 30.0))]),
            ]),
        ],
    )
}

/// The names of three destinations do not fit the budget: the question names as many as
/// fit and says it reaches more; its approval allows only the named pairs, the action is
/// decided again, and an owner who always approves finishes.
#[test]
fn destinations_that_do_not_fit_are_asked_next_and_the_task_finishes() {
    let mut c = cfg();
    c.known_apps.push(AppInfo::new(LONG_APP, LONG_NAME));
    let r = run(
        c,
        "Go home",
        MemoryStore::new(),
        Phone::new(three_places(), |cmd, _| {
            if is_home(cmd) {
                Transition::Go(home_screen())
            } else {
                Transition::Stay
            }
        }),
        Script::model(&[HOME_ACT, DONE]).decisions(&[true; 20]),
    );
    let f = flows(&r);
    assert_eq!(
        r.outcome.status,
        OutcomeStatus::Success,
        "{:?} {f:#?}",
        r.outcome
    );
    assert!(f.len() >= 2 && f.len() <= 6, "{f:#?}");
    assert!(
        f.iter().any(|q| q
            .summary
            .contains("(it reaches 1 more place; Jarvis asks about it next if needed)")),
        "{f:#?}"
    );
    // Twenty-third review (R23, wording changed): leaving is an observe-kind input, so the
    // places are named as seeing it (`, to …, which can tell when Jarvis waits, looks or
    // acts`; was `, into …`).
    for q in &f {
        assert!(q.summary.contains(&format!("{FLOW}, to ")), "{}", q.summary);
    }
    // No question repeats.
    for (i, a) in f.iter().enumerate() {
        for b in &f[i + 1..] {
            assert_ne!(a.summary, b.summary);
        }
    }
    assert_eq!(count(&r, is_home), 1);
}

// ---- R20-B: the share sheet ---------------------------------------------------------------

fn share_sheet(rows: Vec<UiNode>) -> Screen {
    scr(
        NOTES,
        "Notes",
        vec![
            text("Locker combination 1234", rect(16.0, 140.0, 361.0, 40.0)),
            UiNode::new(Role::Sheet, rect(0.0, 60.0, 393.0, 792.0)).with_children(vec![
                text("Locker combination", rect(16.0, 70.0, 300.0, 30.0)),
                node(Role::Button, "Close", rect(340.0, 70.0, 40.0, 40.0)),
                UiNode::new(Role::CollectionView, rect(0.0, 120.0, 393.0, 732.0))
                    .with_children(rows),
            ]),
        ],
    )
}

fn actions() -> Vec<UiNode> {
    vec![
        node(Role::Cell, "Copy", rect(16.0, 200.0, 361.0, 44.0)),
        node(Role::Cell, "Print", rect(16.0, 250.0, 361.0, 44.0)),
    ]
}

fn with_airdrop(frame: jarvis_proto::Rect, visible: bool) -> Screen {
    let mut rows = vec![{
        let mut n = node(Role::Cell, "AirDrop", frame);
        n.visible = visible;
        n
    }];
    rows.extend(actions());
    share_sheet(rows)
}

fn places_of(s: &Screen) -> jarvis_safety::ScreenPlaces {
    let v = jarvis_observe::compact_view(s);
    jarvis_safety::observed_places(
        s,
        &v.screen,
        jarvis_safety::ScreenSource::Typed,
        Some((&v.webviews, &v.ends, &v.sources)),
    )
}

/// R20-B (a): the raw tree decides, whether the AirDrop node is on screen, off it or hidden;
/// no `Sheet`, no share sheet.
#[test]
fn the_share_sheet_is_read_from_the_raw_tree() {
    for s in [
        with_airdrop(rect(16.0, 130.0, 80.0, 60.0), true),
        with_airdrop(rect(16.0, -120.0, 80.0, 90.0), true),
        with_airdrop(rect(16.0, 130.0, 80.0, 60.0), false),
    ] {
        let p = places_of(&s);
        assert!(p.shows_sheet() && p.share_sheet());
    }
    let p = places_of(&share_sheet(actions()));
    assert!(p.shows_sheet() && !p.share_sheet());
    let p = places_of(&scr(
        "com.apple.Preferences",
        "Settings",
        vec![node(Role::Cell, "AirDrop", rect(0.0, 150.0, 393.0, 44.0))],
    ));
    assert!(!p.shows_sheet() && !p.share_sheet());
}

/// R20-B (a): a hidden AirDrop row (scrolled away and flagged invisible) still makes it a
/// share sheet: a press asks under `confirm: []`.
#[test]
fn a_hidden_airdrop_row_still_makes_a_share_sheet() {
    let s = with_airdrop(rect(16.0, 130.0, 80.0, 60.0), false);
    let i = index_of(&s, "Print");
    let mut c = cfg();
    c.policy.confirm = Vec::new();
    let r = run(
        c,
        "Share my note",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[&tap(i), DONE]).decisions(&[false]),
    );
    assert_eq!(r.confirms.len(), 1, "{:#?}", r.confirms);
    assert!(r.confirms[0].phone_only);
    assert_eq!(count(&r, is_tap), 0);
}

/// R20-B (b): sticky — after a look that was a share sheet, the AirDrop row leaves the tree
/// (cell reuse after a scroll): a press on the sheet still asks.
#[test]
fn a_share_sheet_stays_one_after_its_airdrop_row_leaves_the_tree() {
    let first = with_airdrop(rect(16.0, 130.0, 80.0, 60.0), true);
    let copy = index_of(&share_sheet(actions()), "Copy");
    let mut c = cfg();
    c.policy.confirm = Vec::new();
    let r = run(
        c,
        "Share my note",
        MemoryStore::new(),
        Phone::new(first, |cmd, _| {
            if is_drag(cmd) {
                Transition::Go(share_sheet(actions()))
            } else {
                Transition::Stay
            }
        }),
        Script::model(&[SCROLL, &tap(copy), DONE]).decisions(&[false]),
    );
    assert_eq!(count(&r, is_drag), 1);
    assert_eq!(r.confirms.len(), 1, "{:#?}", r.confirms);
    assert!(r.confirms[0].phone_only);
    assert_eq!(count(&r, is_tap), 0);
}

/// R20-B (b): a look with no `Sheet` ends it — a later, ordinary sheet in the same app is
/// not a share sheet.
#[test]
fn a_dismissed_share_sheet_does_not_make_a_later_sheet_one() {
    let first = with_airdrop(rect(16.0, 130.0, 80.0, 60.0), true);
    let close = index_of(&first, "Close");
    let plain = || {
        scr(
            NOTES,
            "Notes",
            vec![
                text("Locker combination 1234", rect(16.0, 140.0, 361.0, 40.0)),
                node(Role::Button, "Options", rect(16.0, 300.0, 100.0, 40.0)),
            ],
        )
    };
    let options_sheet =
        || {
            scr(
                NOTES,
                "Notes",
                vec![
                    text("Locker combination 1234", rect(16.0, 140.0, 361.0, 40.0)),
                    UiNode::new(Role::Sheet, rect(0.0, 400.0, 393.0, 452.0)).with_children(vec![
                        node(Role::Button, "Pin Note", rect(16.0, 450.0, 361.0, 44.0)),
                    ]),
                ],
            )
        };
    let options = index_of(&plain(), "Options");
    let pin = index_of(&options_sheet(), "Pin Note");
    let mut c = cfg();
    c.policy.confirm = Vec::new();
    let r = run(
        c,
        "Pin my note",
        MemoryStore::new(),
        Phone::new(first, move |cmd, s| {
            if taps(cmd, s, "Close") {
                Transition::Go(plain())
            } else if taps(cmd, s, "Options") {
                Transition::Go(options_sheet())
            } else if taps(cmd, s, "Pin Note") {
                Transition::Go(plain())
            } else {
                Transition::Stay
            }
        }),
        Script::model(&[&tap(close), &tap(options), &tap(pin), DONE]).decisions(&[true; 4]),
    );
    // Only the press on the share sheet asked.
    assert_eq!(r.confirms.len(), 1, "{:#?}", r.confirms);
    assert_eq!(count(&r, is_tap), 3);
    assert_eq!(r.outcome.status, OutcomeStatus::Success);
}

// ---- R20-C: names -------------------------------------------------------------------------

/// Reads an app named `name`, then opens Acme: the flow question names the read app.
fn first_flow_naming(name: &str) -> String {
    let evil = "com.example.lists";
    let s0 = scr(
        evil,
        name,
        vec![text("hello", rect(16.0, 140.0, 361.0, 40.0))],
    );
    let phone = Phone::new(s0, |cmd, _| {
        if matches!(cmd, RunnerCommand::LaunchApp { .. }) {
            Transition::Go(acme())
        } else {
            Transition::Stay
        }
    });
    let r = run(
        cfg(),
        "Open Acme",
        MemoryStore::new(),
        phone,
        Script::model(&[r#"{"action":"open_app","app":"Acme"}"#, DONE]).decisions(&[false]),
    );
    flows(&r)[0].summary.clone()
}

/// R20-C: a name that could read as a list or as Jarvis's own words is shown as the bundle
/// id; an ordinary one (even holding "all" inside a word) is shown as it is.
#[test]
fn app_names_that_read_as_lists_or_jarvis_words_show_the_bundle_id() {
    for name in [
        "Notes, Mail and Safari",
        "Notes and Mail",
        "All web content in Mail",
        "Reader (Jarvis cannot tell its parts apart)",
        "Screen pictures",
        "Notes; Mail",
    ] {
        let s = first_flow_naming(name);
        assert!(
            s.contains("Jarvis has read com.example.lists in this task;"),
            "{name}: {s}"
        );
    }
    let s = first_flow_naming("Small Notes");
    assert!(
        s.contains("Jarvis has read Small Notes in this task;"),
        "{s}"
    );
}

/// R20-C: an address value with whitespace inside is not a host (fail closed: the page is
/// Safari's pages with no readable address), was `bank.examplelogin`.
#[test]
fn an_address_with_a_space_inside_is_not_a_host() {
    let browser = |value: &str| {
        scr(
            SAFARI,
            "Safari",
            vec![
                UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 600.0))
                    .with_children(vec![text("Hello", rect(16.0, 120.0, 361.0, 30.0))]),
                UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
                    .with_label("Address")
                    .with_value(value),
            ],
        )
    };
    let all = places_of(&browser("bank.example login")).all();
    assert!(
        all.contains(&FlowContext::SiteUnreadable(SAFARI.into())),
        "{all:?}"
    );
    assert!(!all.iter().any(|c| matches!(c, FlowContext::Site { .. })));
    // Whitespace and Safari's left-to-right mark at the ends are still dropped.
    let all = places_of(&browser("\u{200E} bank.example ")).all();
    assert!(
        all.contains(&FlowContext::Site {
            app: SAFARI.into(),
            host: "bank.example".into()
        }),
        "{all:?}"
    );
}

// ---- Over-asking, measured (an owner who always approves) ----------------------------------

/// Mail: open mail A, scroll its body twice, Back, open mail B, tap a link in B — three flow
/// questions (the scroll, Back into Mail, the link in B's body).
#[test]
fn mail_task_asks_three_flow_questions() {
    let msg = |sender: &str, link: &str| {
        scr(
            MAIL,
            "Mail",
            vec![
                node(Role::Button, "Inbox", rect(8.0, 50.0, 80.0, 40.0)),
                text(sender, rect(16.0, 100.0, 361.0, 30.0)),
                UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![
                    text("Body text", rect(16.0, 160.0, 361.0, 30.0)),
                    node(Role::Link, link, rect(16.0, 220.0, 200.0, 40.0)),
                ]),
            ],
        )
    };
    let inbox = || {
        scr(
            MAIL,
            "Mail",
            vec![
                node(Role::Cell, "Alice, Lunch", rect(0.0, 100.0, 393.0, 60.0)),
                node(Role::Cell, "Bob, Tickets", rect(0.0, 170.0, 393.0, 60.0)),
            ],
        )
    };
    let a = index_of(&inbox(), "Alice, Lunch");
    let b = index_of(&inbox(), "Bob, Tickets");
    let back = index_of(&msg("Alice", "Menu"), "Inbox");
    let link = index_of(&msg("Bob", "Seats"), "Seats");
    let phone = Phone::new(inbox(), move |cmd, s| {
        if taps(cmd, s, "Alice, Lunch") {
            Transition::Go(msg("Alice", "Menu"))
        } else if taps(cmd, s, "Bob, Tickets") {
            Transition::Go(msg("Bob", "Seats"))
        } else if taps(cmd, s, "Inbox") || taps(cmd, s, "Seats") {
            Transition::Go(inbox())
        } else {
            Transition::Stay
        }
    });
    let r = run(
        cfg(),
        "Read Alice's mail, then open Bob's tickets link",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &tap(a),
            SCROLL,
            SCROLL,
            &tap(back),
            &tap(b),
            &tap(link),
            DONE,
        ])
        .decisions(&[true; 20]),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Success);
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::Scroll, ConfirmVerb::Tap, ConfirmVerb::Tap],
        "{:#?}",
        flows(&r)
    );
}

fn safari_page(host: &str, page: Vec<UiNode>) -> Screen {
    scr(
        SAFARI,
        "Safari",
        vec![
            UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 600.0)).with_children(page),
            node(Role::Button, "Back", rect(8.0, 760.0, 40.0, 40.0)),
            UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
                .with_label("Address")
                .with_value(host),
        ],
    )
}

fn safari_start() -> Screen {
    scr(
        SAFARI,
        "Safari",
        vec![
            node(Role::Button, "Favorites", rect(16.0, 200.0, 100.0, 40.0)),
            UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
                .with_label("Address")
                .with_placeholder("Search or enter website name")
                .hittable(true),
        ],
    )
}

fn news() -> Screen {
    safari_page(
        "news.example",
        vec![
            text("Today's headlines", rect(16.0, 120.0, 361.0, 30.0)),
            node(Role::Link, "Story", rect(16.0, 200.0, 200.0, 40.0)),
        ],
    )
}

fn story() -> Screen {
    safari_page(
        "news.example",
        vec![text("The story", rect(16.0, 120.0, 361.0, 30.0))],
    )
}

/// Safari: type an address, read the page, scroll three times, tap a link on the page, tap
/// Back — three flow questions (the scroll, naming both destinations; the link into the
/// site; Back into Safari's chrome). Before the twentieth review: one (the scroll's approval
/// was kept as a full grant both ways).
#[test]
fn safari_task_asks_three_flow_questions() {
    let s0 = safari_start();
    let field = index_of(&s0, "Address");
    let link = index_of(&news(), "Story");
    let back = index_of(&story(), "Back");
    let phone = Phone::new(s0, |cmd, s| {
        if matches!(cmd, RunnerCommand::PressKey { .. }) && s.root.children[0].children.len() == 2 {
            return Transition::Go(news());
        }
        if taps(cmd, s, "Story") {
            return Transition::Go(story());
        }
        if taps(cmd, s, "Back") {
            return Transition::Go(news());
        }
        Transition::Stay
    });
    let r = run(
        cfg(),
        "Open news.example and read the story",
        MemoryStore::new(),
        phone,
        Script::model(&[
            &format!(
                r#"{{"action":"type","element":{field},"text":"news.example","submit":true}}"#
            ),
            SCROLL,
            SCROLL,
            SCROLL,
            &tap(link),
            &tap(back),
            DONE,
        ])
        .decisions(&[true; 20]),
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::Scroll, ConfirmVerb::Tap, ConfirmVerb::Tap],
        "{:#?}",
        flows(&r)
    );
}
