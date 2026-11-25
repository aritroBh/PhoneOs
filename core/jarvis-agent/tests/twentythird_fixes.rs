#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-third review: the fixes. R23 — every model-chosen step is seen by every place on
//! the screen when it is decided, so it is an observe-kind input into each of them (a
//! `wait`, a `look` and `press_home` have only those; every other step keeps its own
//! destinations and kind besides). F2 — a Home Screen read that showed more than icons is
//! named as such.

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    AppInfo, ConfirmRequest, ConfirmVerb, Effect, OutcomeStatus, Role, RunnerCommand, Screen,
    ToolResult, UiNode,
};
use jarvis_safety::HOME_SCREEN_READ;

const FLOW: &str = "what it read there could be passed on here";
const SEEN: &str = "which can tell when Jarvis waits, looks or acts";
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const LOOK: &str = r#"{"action":"look"}"#;
const HOME_ACT: &str = r#"{"action":"press_home"}"#;
const SCROLL: &str = r#"{"action":"scroll","direction":"down"}"#;
const MSGS: &str = "com.apple.MobileSMS";
const SHOP: &str = "com.example.shop";
const NOTES: &str = "com.apple.mobilenotes";
const SAFARI: &str = "com.apple.mobilesafari";
const MAIL: &str = "com.apple.mobilemail";
const HOME: &str = "com.apple.springboard";

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

fn wait(ms: u64) -> String {
    format!(r#"{{"action":"wait","ms":{ms}}}"#)
}

fn open(app: &str) -> String {
    format!(r#"{{"action":"open_app","app":"{app}"}}"#)
}

/// Messages with the bank's message and a link that opens Shop (a universal link: the tap
/// is an input into Messages, the only place read).
fn messages() -> Screen {
    scr(
        MSGS,
        "Messages",
        vec![
            text(
                "Bank: balance on Checking 7781 is $12,480.17",
                rect(16.0, 140.0, 361.0, 40.0),
            ),
            node(
                Role::Link,
                "shop.example/daily",
                rect(16.0, 200.0, 361.0, 44.0),
            ),
            node(Role::Button, "Info", rect(300.0, 50.0, 80.0, 44.0)),
        ],
    )
}

/// A third-party app with small controls at the top (a screen scroll there moves content
/// only: the scroll kind).
fn shop() -> Screen {
    scr(
        SHOP,
        "Shop",
        vec![
            text("Daily reward", rect(16.0, 60.0, 200.0, 30.0)),
            node(Role::Button, "Collect", rect(16.0, 100.0, 120.0, 40.0)),
            node(Role::Button, "Settings", rect(150.0, 100.0, 120.0, 40.0)),
        ],
    )
}

fn notes() -> Screen {
    scr(
        NOTES,
        "Notes",
        vec![
            text("Groceries", rect(16.0, 60.0, 200.0, 30.0)),
            node(Role::Button, "New note", rect(16.0, 100.0, 120.0, 40.0)),
            UiNode::new(Role::TextField, rect(16.0, 160.0, 361.0, 40.0))
                .with_label("Body")
                .hittable(true),
        ],
    )
}

/// A page in Safari: its web view, the address and a "Tabs" button (Safari's own chrome,
/// read with the page).
fn page(host: &str) -> Screen {
    scr(
        SAFARI,
        "Safari",
        vec![
            UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 650.0)).with_children(vec![
                text("Welcome", rect(16.0, 140.0, 361.0, 30.0)),
                node(Role::Button, "Next", rect(16.0, 300.0, 361.0, 44.0)),
            ]),
            UiNode::new(Role::Button, rect(100.0, 800.0, 200.0, 30.0))
                .with_label("Address")
                .with_value(host)
                .with_identifier("TabBarItemTitle"),
            node(Role::Button, "Tabs", rect(320.0, 800.0, 60.0, 30.0)),
        ],
    )
}

/// Mail showing one message: its native header and its body (web content).
fn mail() -> Screen {
    scr(
        MAIL,
        "Mail",
        vec![
            node(Role::Button, "Inbox", rect(8.0, 50.0, 80.0, 40.0)),
            text("From: Bank", rect(16.0, 100.0, 361.0, 30.0)),
            UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![
                text("Your statement is ready", rect(16.0, 160.0, 361.0, 30.0)),
                node(Role::Link, "View statement", rect(16.0, 300.0, 361.0, 44.0)),
            ]),
        ],
    )
}

fn home(extra: Vec<UiNode>) -> Screen {
    let mut kids = vec![
        node(Role::Button, "Shop", rect(200.0, 300.0, 64.0, 64.0)),
        node(Role::Button, "Settings", rect(20.0, 300.0, 64.0, 64.0)),
    ];
    kids.extend(extra);
    scr(HOME, " ", kids)
}

fn banner() -> UiNode {
    UiNode::new(
        Role::Other("NotificationBanner".into()),
        rect(8.0, 50.0, 377.0, 80.0),
    )
    .with_children(vec![text(
        "Messages, Bank: balance is $12,480.17",
        rect(16.0, 60.0, 361.0, 60.0),
    )])
}

fn cfg() -> jarvis_proto::SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new(NOTES, "Notes"));
    c.known_apps.push(AppInfo::new(SAFARI, "Safari"));
    c.known_apps.push(AppInfo::new(MAIL, "Mail"));
    c
}

/// Messages, whose link opens Shop; every app launches from the table.
fn phone(start: Screen) -> Phone {
    Phone::new(start, |cmd, s| {
        if taps(cmd, s, "shop.example/daily") {
            return Transition::Go(shop());
        }
        match cmd {
            RunnerCommand::LaunchApp { bundle_id } if bundle_id == NOTES => Transition::Go(notes()),
            RunnerCommand::LaunchApp { bundle_id } if bundle_id == SHOP => Transition::Go(shop()),
            RunnerCommand::OpenUrl { url } => Transition::Go(page(
                url.trim_start_matches("https://").trim_end_matches('/'),
            )),
            RunnerCommand::PressKey {
                key: jarvis_proto::Key::Home,
                ..
            } => Transition::Go(home(Vec::new())),
            _ => Transition::Stay,
        }
    })
}

fn link() -> String {
    tap(index_of(&messages(), "shop.example/daily"))
}

fn flows(r: &Run) -> Vec<&ConfirmRequest> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .collect()
}

fn verbs(r: &Run) -> Vec<ConfirmVerb> {
    flows(r).iter().map(|c| c.verb).collect()
}

/// Clause 5: no runner command, tool call or observation between a model step's proposal
/// (its `Infer`) and its first flow question, and none after a declined flow question
/// before the model is asked again.
fn assert_quiet(r: &Run) {
    let mut since_infer = 0usize;
    let mut asked = false;
    let mut declined = false;
    // The script answers one decision per question, in order; none left declines.
    let mut decisions = DECISIONS.with(|d| d.borrow().clone()).into_iter();
    for e in &r.effects {
        match e {
            Effect::Infer(_) => {
                since_infer = 0;
                asked = false;
                declined = false;
            }
            Effect::Act(_) | Effect::Observe { .. } | Effect::CallTool(_) => {
                since_infer += 1;
                assert!(!declined, "{e:?} after a declined flow question");
            }
            Effect::Confirm(c) => {
                let approved = decisions.next().unwrap_or(false);
                if c.summary.contains(FLOW) {
                    if !asked {
                        assert_eq!(since_infer, 0, "{since_infer} effects before {}", c.summary);
                    }
                    asked = true;
                    declined |= !approved;
                }
            }
            _ => {}
        }
    }
}

thread_local! {
    /// The decisions of the run being checked (the script's, one per question).
    static DECISIONS: std::cell::RefCell<Vec<bool>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// [`run`] remembering the decisions for [`assert_quiet`].
fn go(c: jarvis_proto::SessionConfig, start: Screen, steps: &[&str], decisions: &[bool]) -> Run {
    DECISIONS.with(|d| *d.borrow_mut() = decisions.to_vec());
    let r = run(
        c,
        "Read my bank message, then open the daily link at https://news.example/",
        MemoryStore::new(),
        phone(start),
        Script::model(steps).decisions(decisions),
    );
    assert_quiet(&r);
    r
}

fn waits_sent(r: &Run) -> usize {
    r.acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::Wait { ms } if *ms >= 1_000))
        .count()
}

// ---- clause 1: wait and look ask ---------------------------------------------------------

/// After reading Messages and arriving in Shop by a link (no question), a wait asks before
/// anything is sent, naming Messages and Shop as the place that sees it; declined, no wait
/// is sent and nothing is observed; the next wait would carry the declined flow: `Blocked`.
#[test]
fn a_wait_after_reading_elsewhere_asks_first_and_a_decline_sends_nothing() {
    let r = go(
        cfg(),
        messages(),
        &[&link(), &wait(1000), &wait(3000), DONE],
        &[false],
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert_eq!(
        fq[0].summary,
        format!(
            "Wait 1000 ms in Shop. Jarvis has read Messages in this task; {FLOW}, to Shop, {SEEN}"
        )
    );
    assert!(fq[0].phone_only && fq[0].flow_only);
    assert_eq!(waits_sent(&r), 0, "{:?}", r.acts);
    assert_eq!(r.outcome.status, OutcomeStatus::Blocked, "{:?}", r.outcome);
    // Approved: the wait is sent after a look (the look after the approval), once.
    let r = go(cfg(), messages(), &[&link(), &wait(1000), DONE], &[true]);
    assert_eq!(flows(&r).len(), 1);
    assert_eq!(waits_sent(&r), 1, "{:?}", r.acts);
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
}

/// A vision model's look asks the same way; declined, no screenshot is taken; approved, the
/// screenshot follows the answer.
#[test]
fn a_look_after_reading_elsewhere_asks_first_and_a_decline_takes_no_screenshot() {
    let r = go(
        vision_config(),
        messages(),
        &[&link(), LOOK, DONE],
        &[false],
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .starts_with("Look at the screen in Shop. Jarvis has read Messages"),
        "{}",
        fq[0].summary
    );
    assert!(
        fq[0].summary.ends_with(&format!(", to Shop, {SEEN}")),
        "{}",
        fq[0].summary
    );
    assert!(!r.observes.iter().any(|s| *s), "{:?}", r.observes);
    let r = go(vision_config(), messages(), &[&link(), LOOK, DONE], &[true]);
    let asked = r
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Confirm(_)))
        .unwrap();
    let shot = r
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Observe { screenshot: true }))
        .expect("the approved look took a screenshot");
    assert!(shot > asked);
}

// ---- clause 3: what an observe approval keeps ----------------------------------------------

/// An approved wait keeps observe-only grants: later waits, looks and `press_home` ask
/// nothing; a scroll still asks, and so does a tap after the scroll's approval.
#[test]
fn an_approved_wait_keeps_observe_only() {
    let r = go(
        cfg(),
        messages(),
        &[&link(), &wait(1000), &wait(3000), LOOK, HOME_ACT, DONE],
        &[true; 4],
    );
    assert_eq!(verbs(&r), vec![ConfirmVerb::Other], "{:#?}", r.confirms);
    assert_eq!(waits_sent(&r), 2);
    assert!(r.acts.iter().any(|a| matches!(
        a,
        RunnerCommand::PressKey {
            key: jarvis_proto::Key::Home,
            ..
        }
    )));
    let collect = tap(index_of(&shop(), "Collect"));
    let r = go(
        cfg(),
        messages(),
        &[&link(), &wait(1000), SCROLL, &collect, DONE],
        &[true; 4],
    );
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::Other, ConfirmVerb::Scroll, ConfirmVerb::Tap],
        "{:#?}",
        r.confirms
    );
}

/// A scroll grant covers waits and looks; a full grant covers waits, looks and scrolls.
#[test]
fn stronger_grants_cover_observing() {
    let r = go(
        cfg(),
        messages(),
        &[&link(), SCROLL, &wait(1000), LOOK, DONE],
        &[true; 4],
    );
    assert_eq!(verbs(&r), vec![ConfirmVerb::Scroll], "{:#?}", r.confirms);
    assert_eq!(waits_sent(&r), 1);
    let collect = tap(index_of(&shop(), "Collect"));
    let r = go(
        cfg(),
        messages(),
        &[&link(), &collect, &wait(1000), SCROLL, LOOK, DONE],
        &[true; 4],
    );
    assert_eq!(verbs(&r), vec![ConfirmVerb::Tap], "{:#?}", r.confirms);
    assert_eq!(waits_sent(&r), 1);
}

// ---- clause 2: leaving and other steps are seen ------------------------------------------

/// `open_app` and `open_url` leave the screen: the place left sees it, so it is named (`to
/// Shop`) beside the destination and asked.
#[test]
fn leaving_by_open_app_or_open_url_names_the_place_left() {
    let r = go(
        cfg(),
        messages(),
        &[&link(), &open("Notes"), DONE],
        &[false],
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains(&format!("{FLOW}, into Notes, and to Shop, {SEEN}")),
        "{}",
        fq[0].summary
    );
    assert!(!r
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::LaunchApp { .. })));
    let url = r#"{"action":"open_url","url":"https://news.example/"}"#;
    let r = go(cfg(), messages(), &[&link(), url, DONE], &[false]);
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains(&format!("{FLOW}, into news.example, and to Shop, {SEEN}")),
        "{}",
        fq[0].summary
    );
    assert!(!r
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::OpenUrl { .. })));
}

/// The common flow: read Messages, `open_app` Notes from it, tap and type in Notes — one
/// question (Messages saw only what it already holds), and its full grant covers the rest.
#[test]
fn read_open_tap_type_asks_once() {
    let n = notes();
    let new = tap(index_of(&n, "New note"));
    let body = index_of(&n, "Body");
    let typing = format!(r#"{{"action":"type","element":{body},"text":"Read my bank message"}}"#);
    let r = go(
        cfg(),
        messages(),
        &[&open("Notes"), &new, &typing, &wait(1000), DONE],
        &[true; 4],
    );
    assert_eq!(verbs(&r), vec![ConfirmVerb::OpenApp], "{:#?}", r.confirms);
    assert!(
        flows(&r)[0]
            .summary
            .ends_with(&format!("Jarvis has read Messages in this task; {FLOW}")),
        "{}",
        flows(&r)[0].summary
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
}

/// A tap on a page with Safari's chrome on screen: the page is the tap's own destination,
/// the chrome sees it; the question names both, keeps full for the page and observe for the
/// chrome: a later wait asks nothing, a tap on the chrome asks.
#[test]
fn a_tap_on_a_page_keeps_full_for_the_page_and_observe_for_the_chrome() {
    let p = page("news.example");
    let next = tap(index_of(&p, "Next"));
    let tabs = tap(index_of(&p, "Tabs"));
    let url = r#"{"action":"open_url","url":"https://news.example/"}"#;
    let r = go(
        cfg(),
        messages(),
        &[url, &next, &wait(1000), &next, &tabs, DONE],
        &[true; 6],
    );
    let fq = flows(&r);
    assert_eq!(
        verbs(&r),
        vec![ConfirmVerb::OpenUrl, ConfirmVerb::Tap, ConfirmVerb::Tap],
        "{:#?}",
        r.confirms
    );
    assert!(
        fq[1].summary.contains(&format!(
            "Jarvis has read Safari, news.example and Messages in this task; {FLOW}, into news.example, and to Safari, {SEEN}"
        )),
        "{}",
        fq[1].summary
    );
    assert!(
        fq[2].summary.starts_with("Tap \"Tabs\""),
        "{}",
        fq[2].summary
    );
    assert!(
        fq[2]
            .summary
            .contains("Jarvis has read news.example and Messages in this task"),
        "{}",
        fq[2].summary
    );
}

/// Partial approvals with mixed kinds finish: three places with long names, the action's own
/// destination and two that only see it; an owner who always approves finishes and no
/// question repeats.
#[test]
fn partial_approvals_with_mixed_kinds_finish() {
    const LONG_APP: &str = "com.example.longname";
    const LONG_NAME: &str = "Shop $1000.00 $2000.00 $3000.00 $4000.00";
    let three = scr(
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
    );
    let menu = tap(index_of(&three, "Menu"));
    let mut c = cfg();
    c.known_apps.push(AppInfo::new(LONG_APP, LONG_NAME));
    DECISIONS.with(|d| *d.borrow_mut() = vec![true; 30]);
    let r = run(
        c,
        "Look around",
        MemoryStore::new(),
        Phone::still(three),
        Script::model(&[&menu, &wait(1000), DONE]).decisions(&[true; 30]),
    );
    assert_quiet(&r);
    assert_eq!(r.outcome.status, OutcomeStatus::Success, "{:?}", r.outcome);
    let fq = flows(&r);
    assert!(fq.len() >= 2, "{fq:#?}");
    for (i, a) in fq.iter().enumerate() {
        for b in &fq[i + 1..] {
            assert_ne!(a.summary, b.summary);
        }
    }
    assert!(
        fq.iter().any(|q| q.summary.contains(", and to ")),
        "{fq:#?}"
    );
    assert_eq!(waits_sent(&r), 1);
}

// ---- clause 7: self-sourcing classes at observe kind ---------------------------------------

/// Mail's body is web content (a covering class, a source for itself): the first wait after
/// reading a message asks once, the second asks nothing.
#[test]
fn a_wait_on_a_mail_body_asks_once() {
    let r = go(cfg(), mail(), &[&wait(1000), &wait(1000), DONE], &[true; 3]);
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0].summary.contains(
            "Jarvis has read Mail and all web content in Mail (Jarvis cannot tell its parts apart) in this task"
        ),
        "{}",
        fq[0].summary
    );
    assert_eq!(waits_sent(&r), 2);
}

/// A single app that read nothing else asks nothing for its waits and looks (text model).
#[test]
fn waits_and_looks_in_one_app_ask_nothing() {
    let r = go(
        cfg(),
        shop(),
        &[&wait(1000), LOOK, &wait(3000), HOME_ACT, DONE],
        &[],
    );
    assert!(r.confirms.is_empty(), "{:?}", r.confirms);
    assert_eq!(waits_sent(&r), 2);
}

// ---- clause 6: tool calls ------------------------------------------------------------------

/// Every tool call is followed by a look at the screen (`Session::on_tool`), so the app in
/// front sees it: a self-contained tool after reading Messages, in Shop, names Shop.
#[test]
fn a_tool_call_is_seen_by_the_app_in_front() {
    let mut c = cfg();
    c.tools.push(jarvis_proto::ToolSpec {
        name: "clock".into(),
        description: "The time".into(),
        params: Vec::new(),
        tier: jarvis_proto::Tier::Native,
        risk: jarvis_proto::RiskCategory::None,
        app: None,
        self_contained: true,
    });
    let call = r#"{"action":"call_tool","name":"clock","args":{}}"#;
    DECISIONS.with(|d| *d.borrow_mut() = vec![false]);
    let r = run(
        c.clone(),
        "Read my bank message, then open the daily link",
        MemoryStore::new(),
        phone(messages()),
        Script::model(&[&link(), call, DONE]).decisions(&[false]),
    );
    assert_quiet(&r);
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains(&format!("{FLOW}, into the tool clock, and to Shop, {SEEN}")),
        "{}",
        fq[0].summary
    );
    assert!(r.tool_calls.is_empty());
    // Approved: the call runs, and a look follows it.
    DECISIONS.with(|d| *d.borrow_mut() = vec![true]);
    let r = run(
        c,
        "Read my bank message, then open the daily link",
        MemoryStore::new(),
        Phone::new(messages(), |cmd, s| {
            if taps(cmd, s, "shop.example/daily") {
                Transition::Go(shop())
            } else {
                Transition::Stay
            }
        }),
        Script::model(&[&link(), call, DONE])
            .decisions(&[true])
            .tool(|_| ToolResult {
                ok: true,
                output: "12:00".into(),
                error: None,
            }),
    );
    assert_quiet(&r);
    let at = r
        .effects
        .iter()
        .position(|e| matches!(e, Effect::CallTool(_)))
        .expect("the tool ran");
    assert!(matches!(r.effects[at + 1], Effect::Observe { .. }));
}

// ---- F2: the Home Screen's name -------------------------------------------------------------

/// A home screen read that showed a banner is named for what it showed; one of icons only
/// is not a read and asks nothing.
#[test]
fn the_home_screen_read_is_named_for_what_it_showed() {
    let launch = |start: Screen| {
        let icon = tap(index_of(&start, "Shop"));
        let collect = tap(index_of(&shop(), "Collect"));
        let mut c = cfg();
        c.known_apps.push(AppInfo::new(SHOP, "Shop"));
        DECISIONS.with(|d| *d.borrow_mut() = vec![true; 4]);
        let r = run(
            c,
            "Open Shop and collect",
            MemoryStore::new(),
            Phone::new(start, |cmd, s| {
                if s.app_bundle_id == HOME && taps(cmd, s, "Shop") {
                    Transition::Go(shop())
                } else {
                    Transition::Stay
                }
            }),
            Script::model(&[&icon, &collect, DONE]).decisions(&[true; 4]),
        );
        assert_quiet(&r);
        r
    };
    let r = launch(home(vec![banner()]));
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains(&format!("Jarvis has read {HOME_SCREEN_READ} in this task")),
        "{}",
        fq[0].summary
    );
    assert!(HOME_SCREEN_READ.contains("notifications"));
    assert!(!HOME_SCREEN_READ.contains(", ") && !HOME_SCREEN_READ.contains(" and "));
    let r = launch(home(Vec::new()));
    assert!(flows(&r).is_empty(), "{:?}", r.confirms);
}

/// No app's name reads as the Home Screen's: an app called "Home Screen", or one whose name
/// holds the read name's words, is named by its bundle id.
#[test]
fn an_app_name_never_reads_as_the_home_screen() {
    for (bundle, name) in [
        ("com.example.fakehome", "Home Screen"),
        (
            "com.example.fakehome",
            "Shop (notifications or search results shown on it)",
        ),
    ] {
        let s = scr(
            bundle,
            name,
            vec![
                text("Hello", rect(16.0, 60.0, 200.0, 30.0)),
                node(Role::Button, "Go", rect(16.0, 100.0, 120.0, 40.0)),
            ],
        );
        let mut c = cfg();
        c.known_apps.push(AppInfo::new(bundle, name));
        let r = go(c, s, &[&open("Notes"), DONE], &[false]);
        let fq = flows(&r);
        assert_eq!(fq.len(), 1, "{:?}", r.confirms);
        assert!(
            fq[0]
                .summary
                .contains(&format!("Jarvis has read {bundle} in this task")),
            "{}",
            fq[0].summary
        );
    }
}
