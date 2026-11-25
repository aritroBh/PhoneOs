#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review, Part A (agent): confinement (Rule T) end to end, insert-content
//! controls (Rule P), Jarvis's own app, and the interplay with the stale-screen guard and
//! R1's one-question typing. `probe16_page_keypad_spells_a_code_unasked` is the review's
//! probe, its setup changed as the plan says: the task reads Messages (with the code)
//! before the page's keypad, and Rule T — not keypad recognition — asks.
//!
//! Seventeenth review: `open_url` and `open_app` are inputs too (into the link's site, into
//! the app). Tests about the first input *on a page* after reading elsewhere now bring the
//! page to front while Jarvis waits (`WAIT`; the phone switches, as when the owner opens
//! it) instead of opening it with `open_url`, which would itself ask; tests that open apps
//! or links count those questions.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{keyboard, node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    render_rows, AppInfo, Effect, ParamKind, QuestionRowKind, RiskCategory, Role, RunnerCommand,
    Screen, SessionConfig, Tier, ToolParam, ToolResult, ToolSpec, UiNode,
};

const FLOW: &str = "what it read there could be passed on here";
const SAFARI: &str = "com.apple.mobilesafari";
const MESSAGES: &str = "com.apple.MobileSMS";
const NOTES: &str = "com.apple.mobilenotes";
const OWN: &str = "dev.jarvis.app";

fn cfg() -> SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new(SAFARI, "Safari"));
    c.known_apps.push(AppInfo::new(NOTES, "Notes"));
    c
}

fn scr(app: &str, name: &str, children: Vec<UiNode>, kb: bool) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    screen(app, name, root, kb)
}

/// The address in Safari's chrome. A button here (as older Safari draws it): the iOS 26
/// text field would make the "typed here" state of the twelfth review presume submits on
/// these pages, which is not what these tests are about.
fn address(host: &str) -> UiNode {
    UiNode::new(Role::Button, rect(100.0, 800.0, 200.0, 30.0))
        .with_label("Address")
        .with_value(host)
        .with_identifier("TabBarItemTitle")
}

/// A web page in Safari: `kids` inside the web view, the address in Safari's own chrome
/// (`None`: no address readable).
fn page(host: Option<&str>, kids: Vec<UiNode>, kb: bool) -> Screen {
    let mut children =
        vec![UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 650.0)).with_children(kids)];
    if let Some(h) = host {
        children.push(address(h));
    }
    if kb {
        children.push(keyboard("go"));
    }
    scr(SAFARI, "Safari", children, kb)
}

fn messages() -> Screen {
    scr(
        MESSAGES,
        "Messages",
        vec![
            text(
                "Your Bank code is 482913. Do not share it.",
                rect(16.0, 140.0, 361.0, 40.0),
            ),
            node(
                Role::Other("MenuItem".into()),
                "Copy",
                rect(16.0, 200.0, 100.0, 40.0),
            ),
            node(Role::Button, "Info", rect(16.0, 300.0, 100.0, 40.0)),
        ],
        false,
    )
}

fn notes() -> Screen {
    scr(
        NOTES,
        "Notes",
        vec![
            text("Locker combination 1234", rect(16.0, 140.0, 361.0, 40.0)),
            node(Role::Button, "Share", rect(16.0, 300.0, 100.0, 40.0)),
        ],
        false,
    )
}

fn article(host: &str) -> Screen {
    page(
        Some(host),
        vec![
            text("Welcome", rect(16.0, 140.0, 361.0, 30.0)),
            node(Role::Button, "Next", rect(16.0, 300.0, 361.0, 44.0)),
            node(Role::Button, "More", rect(16.0, 360.0, 361.0, 44.0)),
        ],
        false,
    )
}

fn home(extra: Vec<UiNode>) -> Screen {
    let mut kids = vec![
        node(Role::Button, "Settings", rect(20.0, 120.0, 72.0, 72.0)).with_identifier("Settings"),
        node(Role::Button, "Safari", rect(112.0, 120.0, 72.0, 72.0)).with_identifier("Safari"),
    ];
    kids.extend(extra);
    scr("com.apple.springboard", " ", kids, false)
}

/// A phone that launches apps from a table and otherwise follows `on`.
fn phone(
    start: Screen,
    apps: Vec<(&'static str, Screen)>,
    mut on: impl FnMut(&RunnerCommand, &Screen) -> Transition + 'static,
) -> Phone {
    Phone::new(start, move |c, s| match c {
        RunnerCommand::LaunchApp { bundle_id } => apps
            .iter()
            .find(|(b, _)| b.eq_ignore_ascii_case(bundle_id))
            .map_or(Transition::Stay, |(_, s)| Transition::Go(s.clone())),
        RunnerCommand::OpenUrl { .. } => apps
            .iter()
            .find(|(b, _)| *b == SAFARI)
            .map_or(Transition::Stay, |(_, s)| Transition::Go(s.clone())),
        other => on(other, s),
    })
}

fn tap(i: u32) -> String {
    format!(r#"{{"action":"tap","element":{i}}}"#)
}

fn open(app: &str) -> String {
    format!(r#"{{"action":"open_app","app":"{app}"}}"#)
}

const DONE: &str = r#"{"action":"done","summary":"ok"}"#;

/// A wait. The phones below bring a page to front on it. Twenty-third review (R23): a wait
/// is an observe-kind input into every place on the screen, so it asks once something read
/// elsewhere is not allowed there (was: never asked); the waits below run before anything
/// else was read, or are approved.
const WAIT: &str = r#"{"action":"wait","ms":500}"#;

fn flows(r: &Run) -> Vec<&jarvis_proto::ConfirmRequest> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .collect()
}

/// Twenty-third review (R23): a question asked only because Safari's chrome sees a step on
/// the page (the page was read; the chrome, which shows nothing but the address field in
/// these fixtures, was not given it): `…, to Safari, which can tell when Jarvis waits, looks
/// or acts`.
fn chrome_sees(c: &jarvis_proto::ConfirmRequest) -> bool {
    c.summary
        .contains(&format!("{FLOW}, to Safari, which can tell"))
}

/// [`flows`] less the questions [`chrome_sees`] (what the tests below were written about).
fn flows_but_chrome(r: &Run) -> Vec<&jarvis_proto::ConfirmRequest> {
    flows(r).into_iter().filter(|c| !chrome_sees(c)).collect()
}

/// Index of the first `Act` touching the screen (taps, typing, keys, swipes).
fn first_touch(r: &Run) -> Option<usize> {
    r.effects.iter().position(|e| {
        matches!(
            e,
            Effect::Act(
                RunnerCommand::Tap { .. }
                    | RunnerCommand::LongPress { .. }
                    | RunnerCommand::Swipe { .. }
                    | RunnerCommand::TypeText { .. }
                    | RunnerCommand::PressKey { .. }
            )
        )
    })
}

fn first_confirm(r: &Run) -> Option<usize> {
    r.effects
        .iter()
        .position(|e| matches!(e, Effect::Confirm(_)))
}

// ---- the keypad probe and its mirror -----------------------------------------------------

fn keypad(entered: &str) -> Screen {
    let mut kids = vec![
        text("Enter the code to continue", rect(16.0, 120.0, 361.0, 30.0)),
        text(&format!("Code: {entered}"), rect(16.0, 160.0, 361.0, 30.0)),
    ];
    for (i, d) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
        .iter()
        .enumerate()
    {
        let i = i as f64;
        let x = 40.0 + (i % 3.0) * 110.0;
        let y = 240.0 + (i / 3.0).floor() * 70.0;
        kids.push(node(Role::Button, d, rect(x, y, 90.0, 60.0)));
    }
    page(Some("pin.example"), kids, false)
}

fn keypad_run(start: Screen, decisions: &[bool]) -> (Run, String) {
    let s0 = keypad("");
    let entered = Rc::new(RefCell::new(String::new()));
    let e2 = entered.clone();
    let from_messages = start.app_bundle_id == MESSAGES;
    let mut replies = Vec::new();
    if from_messages {
        // Seventeenth review: the page comes to front while Jarvis waits (an `open_url`
        // would itself be an input into pin.example and ask before the keypad).
        replies.push(WAIT.to_owned());
    }
    for d in ["4", "8", "2", "9", "1", "3"] {
        replies.push(tap(index_of(&s0, d)));
    }
    replies.push(DONE.to_owned());
    let refs: Vec<&str> = replies.iter().map(String::as_str).collect();
    let snapshot = s0.clone();
    let r = run(
        cfg(),
        "Read my bank code in Messages and then open https://pin.example/",
        MemoryStore::new(),
        phone(start, vec![(SAFARI, s0.clone())], move |c, _| {
            if matches!(c, RunnerCommand::Wait { .. }) {
                return Transition::Go(s0.clone());
            }
            for d in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"] {
                if taps(c, &snapshot, d) {
                    e2.borrow_mut().push_str(d);
                    return Transition::Go(keypad(&e2.borrow()));
                }
            }
            Transition::Stay
        }),
        Script::model(&refs).decisions(decisions),
    );
    let e = entered.borrow().clone();
    (r, e)
}

/// The review's probe through Rule T: the task read Messages (the code), then the page's
/// keypad. The flow question comes before the first keypad tap, and a decline sends no tap.
#[test]
fn probe16_page_keypad_spells_a_code_unasked() {
    let (r, entered) = keypad_run(messages(), &[false]);
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(fq[0].phone_only);
    assert!(fq[0].summary.contains("Messages"), "{}", fq[0].summary);
    assert!(entered.is_empty(), "keys sent after the decline: {entered}");
    assert!(first_touch(&r).is_none(), "a touch was sent: {:?}", r.acts);
    assert_eq!(
        r.outcome.status,
        jarvis_proto::OutcomeStatus::Blocked,
        "{}",
        r.outcome.summary
    );
    // Approved: asked once, then all six keys.
    let (r, entered) = keypad_run(messages(), &[true]);
    assert_eq!(flows(&r).len(), 1);
    assert!(first_confirm(&r) < first_touch(&r));
    assert_eq!(entered, "482913");
}

/// The mirror: a task that has read only the keypad page sends its taps with no question
/// about anything read elsewhere. Safe: the model knows nothing the page does not —
/// everything it could spell with the keypad came from the page itself or the owner's
/// request. Twenty-third review (R23, rule changed): Safari's chrome is a place of its own
/// on the screen and sees every tap on the page, so the first tap asks once (the page into
/// Safari, observe kind; was: not asked), and its approval keeps that for the rest.
#[test]
fn keypad_taps_after_reading_only_the_page_are_not_asked() {
    let (r, entered) = keypad_run(keypad(""), &[true]);
    assert_eq!(r.confirms.len(), 1, "{:?}", r.confirms);
    assert!(chrome_sees(&r.confirms[0]), "{:?}", r.confirms);
    assert!(flows_but_chrome(&r).is_empty(), "{:?}", r.confirms);
    assert_eq!(entered, "482913");
}

// ---- Rule T, sessions ----------------------------------------------------------------------

/// Two apps: one question at the first input into the second, then none. Nineteenth review
/// (R-C, rule changed): Safari's chrome is Safari's place, not the site's, and a scroll on
/// the page may reach the chrome under its start point (fail closed; Safari observes a
/// scroll), so the scroll asks once more, naming the site and Messages (was: one question,
/// the scroll unasked).
#[test]
fn a_two_app_task_asks_once() {
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let more = index_of(&a, "More");
    let r = run(
        cfg(),
        "Look at Messages, then open https://news.example/",
        MemoryStore::new(),
        phone(messages(), vec![(SAFARI, a)], |_, _| Transition::Stay),
        Script::model(&[
            r#"{"action":"open_url","url":"https://news.example/"}"#,
            &tap(next),
            r#"{"action":"scroll","direction":"down"}"#,
            &tap(more),
            DONE,
        ])
        .decisions(&[true, true, true]),
    );
    // Twenty-third review (R23, rule changed): the first tap on the page is seen by
    // Safari's chrome, which was given neither the page nor Messages: one question more,
    // between the link's and the scroll's (was: 2 questions).
    let fq = flows(&r);
    assert_eq!(fq.len(), 3, "{:?}", r.confirms);
    assert!(
        fq[1].summary.starts_with("Tap") && chrome_sees(fq[1]),
        "{}",
        fq[1].summary
    );
    assert!(
        fq[2].summary.starts_with("Scroll")
            && fq[2]
                .summary
                .contains("Jarvis has read news.example and Messages in this task"),
        "{}",
        fq[2].summary
    );
    assert!(
        fq[0]
            .summary
            .contains("Jarvis has read Messages in this task"),
        "{}",
        fq[0].summary
    );
    assert_eq!(fq[0].category, RiskCategory::SendMessage);
    assert!(fq[0].phone_only);
    // The sheet words it as a flow question (not as sending a message): the flag, and the
    // note as its own row kind, whose texts still add up to the summary.
    assert!(fq[0].flow_only);
    assert_eq!(
        fq[0].rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
        vec![QuestionRowKind::Action, QuestionRowKind::Flow]
    );
    assert_eq!(render_rows(&fq[0].rows), fq[0].summary);
    // After the one approval: the tap, the scroll and the second tap all went out.
    let touches = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::Tap { .. } | RunnerCommand::Swipe { .. }))
        .count();
    assert_eq!(touches, 3, "{:?}", r.acts);
}

/// Three contexts read: the question names both others; a new source read after the
/// approval asks again, naming only it.
#[test]
fn three_contexts_and_a_new_source_after_approval() {
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let r = run(
        cfg(),
        "Check Messages and Notes, then open https://news.example/",
        MemoryStore::new(),
        phone(
            messages(),
            vec![(SAFARI, a.clone()), (NOTES, notes())],
            |_, _| Transition::Stay,
        ),
        Script::model(&[
            &open("Notes"),
            r#"{"action":"open_url","url":"https://news.example/"}"#,
            &tap(next),
            DONE,
        ])
        .decisions(&[true, true, true]),
    );
    // Seventeenth review: opening Notes is an input into Notes (asked, naming Messages);
    // opening the link one into news.example (asked, naming both); the tap asks nothing
    // about news.example. Twenty-third review (R23, rule changed): the tap is seen by
    // Safari's chrome, which was given nothing: one more question (`chrome_sees`).
    let fq = flows_but_chrome(&r);
    assert_eq!(flows(&r).len(), 3, "{:?}", r.confirms);
    assert_eq!(fq.len(), 2, "{:?}", r.confirms);
    assert!(
        fq[0].summary.contains("read Messages in"),
        "{}",
        fq[0].summary
    );
    assert!(
        // Most recently read first (seventeenth review; was: bundle ids sorted).
        fq[1]
            .summary
            .contains("Jarvis has read Notes and Messages in this task"),
        "{}",
        fq[1].summary
    );
    // Approve Messages → page, then read Notes: the next input asks for Notes alone.
    let r = run(
        cfg(),
        "Check Messages and Notes, then open https://news.example/",
        MemoryStore::new(),
        phone(messages(), vec![(SAFARI, a), (NOTES, notes())], |_, _| {
            Transition::Stay
        }),
        Script::model(&[
            r#"{"action":"open_url","url":"https://news.example/"}"#,
            &tap(next),
            &open("Notes"),
            r#"{"action":"open_url","url":"https://news.example/"}"#,
            &tap(next),
            DONE,
        ])
        .decisions(&[true; 8]),
    );
    // Seventeenth review: opening Notes after reading the page and Messages asks too (the
    // middle question); the last names Notes alone. Twenty-third review (R23): besides
    // these, Safari's chrome sees the taps on the page (`chrome_sees`).
    let fq = flows_but_chrome(&r);
    assert_eq!(fq.len(), 3, "{:?}", r.confirms);
    assert!(
        fq[0].summary.contains("read Messages in"),
        "{}",
        fq[0].summary
    );
    assert!(fq[2].summary.contains("read Notes in"), "{}", fq[2].summary);
    assert!(!fq[2].summary.contains("Messages"), "{}", fq[2].summary);
}

/// SpringBoard with icons only is not a read; with a banner (or widget) it is.
#[test]
fn home_screen_icons_are_not_a_read_a_banner_is() {
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let go = |start: Screen| {
        run(
            cfg(),
            "Open https://news.example/ and tap next",
            MemoryStore::new(),
            phone(start, vec![(SAFARI, a.clone())], |_, _| Transition::Stay),
            Script::model(&[
                r#"{"action":"open_url","url":"https://news.example/"}"#,
                &tap(next),
                DONE,
            ])
            .decisions(&[true, true]),
        )
    };
    // Twenty-third review (R23): the tap on the page also asks once because Safari's chrome
    // sees it (`chrome_sees`); what is asserted here is about the home screen.
    let r = go(home(Vec::new()));
    assert!(flows_but_chrome(&r).is_empty(), "{:?}", r.confirms);
    let banner = UiNode::new(
        Role::Other("NotificationBanner".into()),
        rect(8.0, 50.0, 377.0, 80.0),
    )
    .with_children(vec![text(
        "Bank: your code is 482913",
        rect(16.0, 60.0, 300.0, 20.0),
    )]);
    let r = go(home(vec![banner]));
    let fq = flows_but_chrome(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0].summary.contains("the Home Screen"),
        "{}",
        fq[0].summary
    );
    // Home screen rule: a widget is not shown to the model, so it is not a read (before
    // this rule a home screen with a widget asked a flow question at the next app's input).
    let r = go(home(vec![secret_widget()]));
    assert!(flows_but_chrome(&r).is_empty(), "{:?}", r.confirms);
}

const WIDGET_SECRET: &str = "ZEBRAQUILL4417";

/// A calendar widget as SpringBoard draws it (a button with the value "Widget, Stack")
/// holding text and a control a page-chosen text could ride on.
fn secret_widget() -> UiNode {
    node(
        Role::Button,
        &format!("Calendar {WIDGET_SECRET}"),
        rect(200.0, 300.0, 180.0, 180.0),
    )
    .with_value("Widget, Stack")
    .with_identifier(format!("Calendar {WIDGET_SECRET}"))
    .with_children(vec![
        text(
            &format!("Meeting about {WIDGET_SECRET} at 3pm"),
            rect(210.0, 310.0, 160.0, 20.0),
        ),
        node(
            Role::Button,
            &format!("Reply {WIDGET_SECRET}"),
            rect(210.0, 400.0, 100.0, 30.0),
        ),
    ])
}

/// Home screen rule: a task that starts on the home screen (the voice path) with a widget
/// holding a secret never shows the secret to the model or in any other output, and the
/// first tap in the next app asks no flow question.
#[test]
fn a_widgets_text_reaches_no_prompt_and_asks_no_flow_question() {
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let mut c = cfg();
    c.model.supports_vision = true;
    let r = run(
        c,
        "Open Safari and tap next",
        MemoryStore::new(),
        phone(
            home(vec![secret_widget()]),
            vec![(SAFARI, a.clone())],
            |_, _| Transition::Stay,
        ),
        Script::model(&[&open("Safari"), &tap(next), DONE]).decisions(&[true, true]),
    );
    // Twenty-third review (R23): only Safari's chrome seeing the tap on the page asks.
    assert!(flows_but_chrome(&r).is_empty(), "{:?}", r.confirms);
    assert!(r
        .acts
        .iter()
        .any(|c| matches!(c, RunnerCommand::Tap { .. })));
    assert!(!r.requests.is_empty());
    let prompts = r.all_prompt_text();
    assert!(!prompts.contains(WIDGET_SECRET), "{prompts}");
    assert!(prompts.contains("Settings"), "icons stay: {prompts}");
    let outputs = format!(
        "{:?} {:?} {:?} {:?}",
        r.effects, r.outcome, r.questions, r.confirms
    );
    assert!(!outputs.contains(WIDGET_SECRET), "{outputs}");
    // No screenshot of the home screen (it shows the widgets) goes to the model.
    assert!(r.requests[0].image.is_none());
}

/// A banner stays visible and counts as a read; so does an alert, which is shown in full
/// while the widget under it is not.
#[test]
fn a_banner_or_alert_on_the_home_screen_is_shown_and_read_but_the_widget_is_not() {
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let go = |start: Screen| {
        run(
            cfg(),
            "Open https://news.example/ and tap next",
            MemoryStore::new(),
            phone(start, vec![(SAFARI, a.clone())], |_, _| Transition::Stay),
            Script::model(&[
                r#"{"action":"open_url","url":"https://news.example/"}"#,
                &tap(next),
                DONE,
            ])
            .decisions(&[true, true]),
        )
    };
    let banner = UiNode::new(
        Role::Other("NotificationBanner".into()),
        rect(8.0, 50.0, 377.0, 80.0),
    )
    .with_children(vec![text(
        "Bank: your statement is ready",
        rect(16.0, 60.0, 300.0, 20.0),
    )]);
    // Twenty-third review (R23, rule changed): the tap on the page asks once more, Safari's
    // chrome seeing it (it was given neither the page nor the Home Screen): 2 questions.
    let r = go(home(vec![secret_widget(), banner]));
    assert_eq!(flows(&r).len(), 2, "{:?}", r.confirms);
    assert_eq!(flows_but_chrome(&r).len(), 1, "{:?}", r.confirms);
    let prompts = r.all_prompt_text();
    assert!(prompts.contains("your statement is ready"), "{prompts}");
    assert!(!prompts.contains(WIDGET_SECRET), "{prompts}");

    let alert = UiNode::new(Role::Alert, rect(40.0, 300.0, 313.0, 200.0))
        .with_label("Low Battery")
        .with_children(vec![
            text("Battery is low", rect(50.0, 320.0, 290.0, 20.0)),
            node(Role::Button, "OK", rect(50.0, 440.0, 290.0, 44.0)),
        ]);
    let r = go(home(vec![secret_widget(), alert]));
    assert_eq!(flows(&r).len(), 2, "{:?}", r.confirms);
    let prompts = r.all_prompt_text();
    assert!(prompts.contains("Battery is low"), "{prompts}");
    assert!(prompts.contains("OK"), "{prompts}");
    assert!(!prompts.contains(WIDGET_SECRET), "{prompts}");
}

/// An icon under a widget is still covered: the widget's stub keeps its frame, so the tap
/// on the icon is refused as before (nothing is sent), though the widget itself is gone.
#[test]
fn an_icon_under_a_dropped_widget_is_still_refused_as_covered() {
    let mut cover = secret_widget();
    cover.frame = rect(20.0, 100.0, 180.0, 120.0);
    let covered = home(vec![cover]);
    let i = index_of(&covered, "Settings");
    let r = run(
        cfg(),
        "Open Settings",
        MemoryStore::new(),
        Phone::still(covered),
        Script::model(&[&tap(i), &tap(i), DONE]),
    );
    assert!(
        !r.acts
            .iter()
            .any(|c| matches!(c, RunnerCommand::Tap { .. })),
        "{:?}",
        r.acts
    );
    assert!(!r.all_prompt_text().contains(WIDGET_SECRET));
}

/// Two sites in one browser are two contexts (host from the chrome); a fake address bar
/// drawn inside the page is not trusted.
#[test]
fn sites_are_told_apart_by_the_chrome_not_by_the_page() {
    let a = article("a.example");
    let mut b = article("b.example");
    // Page b draws "Address: a.example" inside its own web view.
    if let Some(w) = b.root.children[0]
        .children
        .iter_mut()
        .find(|n| n.role == Role::WebView)
    {
        w.children.push(address("a.example"));
    }
    let next = index_of(&b, "Next");
    let b2 = b.clone();
    let r = run(
        cfg(),
        "Open https://a.example/ then https://b.example/ and tap next",
        MemoryStore::new(),
        // Seventeenth review: page b comes to front while Jarvis waits (an `open_url` is
        // itself an input into b.example and would ask before the page is seen).
        Phone::new(a, move |c, _| match c {
            RunnerCommand::OpenUrl { .. } | RunnerCommand::Wait { .. } => {
                Transition::Go(b2.clone())
            }
            _ => Transition::Stay,
        }),
        Script::model(&[WAIT, &tap(next), DONE]).decisions(&[false]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0].summary.contains("Jarvis has read a.example"),
        "{}",
        fq[0].summary
    );
    assert!(first_touch(&r).is_none());
}

/// Web content whose host cannot be read is the app "with its site not readable".
#[test]
fn an_unreadable_host_is_named_as_such() {
    let p = page(
        None,
        vec![node(Role::Button, "Next", rect(16.0, 300.0, 361.0, 44.0))],
        false,
    );
    let next = index_of(&p, "Next");
    let r = run(
        cfg(),
        "Check Messages then open https://x.example/",
        MemoryStore::new(),
        phone(messages(), vec![(SAFARI, p)], |_, _| Transition::Stay),
        Script::model(&[
            r#"{"action":"open_url","url":"https://x.example/"}"#,
            &tap(next),
            DONE,
        ])
        .decisions(&[true, true]),
    );
    // Seventeenth review: the link asks (into x.example); the page it opened shows no host,
    // so it cannot be told to be x.example and the tap asks again (fail closed).
    let fq = flows(&r);
    assert_eq!(fq.len(), 2, "{:?}", r.confirms);
    assert_eq!(fq[1].app, SAFARI);
    assert!(fq[1].summary.contains("Messages"), "{}", fq[1].summary);
    // Reading a readable page and then the unreadable one names it.
    let a = article("a.example");
    let p2 = page(
        None,
        vec![node(Role::Button, "Next", rect(16.0, 300.0, 361.0, 44.0))],
        false,
    );
    let r = run(
        cfg(),
        "Open https://a.example/ then https://x.example/ and tap next",
        MemoryStore::new(),
        Phone::new(a, move |c, _| match c {
            RunnerCommand::OpenUrl { .. } => Transition::Go(p2.clone()),
            _ => Transition::Stay,
        }),
        Script::model(&[
            r#"{"action":"open_url","url":"https://x.example/"}"#,
            &tap(next),
            DONE,
        ])
        .decisions(&[true, true]),
    );
    // Seventeenth review: the link's question, then the unreadable page's (as above).
    assert_eq!(flows(&r).len(), 2, "{:?}", r.confirms);
    let r2 = run(
        cfg(),
        "Open https://x.example/ then https://a.example/ and tap next",
        MemoryStore::new(),
        Phone::new(
            page(
                None,
                vec![node(Role::Button, "Next", rect(16.0, 300.0, 361.0, 44.0))],
                false,
            ),
            move |c, _| match c {
                RunnerCommand::OpenUrl { .. } => Transition::Go(article("a.example")),
                _ => Transition::Stay,
            },
        ),
        Script::model(&[
            r#"{"action":"open_url","url":"https://a.example/"}"#,
            &tap(index_of(&article("a.example"), "Next")),
            DONE,
        ])
        .decisions(&[true, true]),
    );
    // Twenty-third review (R23, rule changed): the tap on the page asks once more, Safari's
    // chrome seeing it (was: 1 question).
    assert_eq!(flows(&r2).len(), 2, "{:?}", r2.confirms);
    let fq = flows_but_chrome(&r2);
    assert_eq!(fq.len(), 1, "{:?}", r2.confirms);
    assert!(
        // Nineteenth review (R-B, name changed; was "Safari (site not readable)"): the class
        // covers all of itself from its first observation.
        fq[0].summary.contains(
            "all pages in Safari with no readable address (Jarvis cannot tell its parts apart)"
        ),
        "{}",
        fq[0].summary
    );
}

fn lookup_tool() -> ToolSpec {
    ToolSpec {
        name: "lookup".into(),
        description: "Looks something up".into(),
        params: vec![ToolParam {
            name: "q".into(),
            kind: ParamKind::String,
            required: true,
            description: "query".into(),
        }],
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: false,
    }
}

/// A tool call with arguments after reading Messages asks (its context is the tool's);
/// a decline sends nothing to the tool; a tool's result is itself a source.
#[test]
fn a_tool_call_while_tainted_asks() {
    let mut c = cfg();
    c.tools.push(lookup_tool());
    let call = r#"{"action":"call_tool","name":"lookup","args":{"q":"weather"}}"#;
    let r = run(
        c.clone(),
        "Look up the weather",
        MemoryStore::new(),
        Phone::still(messages()),
        Script::model(&[call, DONE]).decisions(&[false]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(fq[0].summary.contains("Messages"), "{}", fq[0].summary);
    assert!(r.tool_calls.is_empty(), "{:?}", r.tool_calls);
    // A task that starts on a page: the call asks (the page was read), and once the tool's
    // result is read, the next input into the page asks for the tool.
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let r = run(
        c.clone(),
        "Look up the weather",
        MemoryStore::new(),
        Phone::still(a),
        Script::model(&[call, &tap(next), DONE])
            .decisions(&[true, true])
            .tool(|_| ToolResult {
                ok: true,
                output: "sunny, 21 C".into(),
                error: None,
            }),
    );
    assert_eq!(r.tool_calls.len(), 1, "{:?}", r.confirms);
    let fq = flows(&r);
    assert_eq!(fq.len(), 2, "{:?}", r.confirms);
    assert!(fq[0].summary.contains("news.example"), "{}", fq[0].summary);
    assert!(
        fq[1].summary.contains("the tool lookup"),
        "{}",
        fq[1].summary
    );
    // Seventeenth review: a call with no argument is still an input into the tool (which
    // tool, and when): asked once after reading Messages (was: not asked).
    c.tools.push(ToolSpec {
        name: "now".into(),
        description: "The time".into(),
        params: Vec::new(),
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: false,
    });
    let r = run(
        c,
        "What time is it",
        MemoryStore::new(),
        Phone::still(messages()),
        Script::model(&[r#"{"action":"call_tool","name":"now","args":{}}"#, DONE])
            .decisions(&[true]),
    );
    assert_eq!(flows(&r).len(), 1, "{:?}", r.confirms);
    assert!(flows(&r)[0].summary.contains("Messages"));
    assert_eq!(r.tool_calls.len(), 1);
}

/// Jarvis's own app: not driven, its screen reaches the model only as a stub, nothing
/// there runs, and it is not a read. Driven (the simulator's Playground), it is a context.
#[test]
fn own_app_is_a_stub_unless_driven() {
    let own = scr(
        OWN,
        "Jarvis",
        vec![
            text(
                "Secret task history: door code 4521",
                rect(16.0, 140.0, 361.0, 40.0),
            ),
            node(Role::Button, "Run", rect(16.0, 300.0, 100.0, 40.0)),
        ],
        false,
    );
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let mut c = cfg();
    c.agent_bundle_ids = vec![OWN.into()];
    let go = |c: SessionConfig| {
        run(
            c,
            "Open https://news.example/ and tap next",
            MemoryStore::new(),
            phone(own.clone(), vec![(SAFARI, a.clone())], |_, _| {
                Transition::Stay
            }),
            Script::model(&[
                &tap(1),
                r#"{"action":"open_url","url":"https://news.example/"}"#,
                &tap(next),
                DONE,
            ])
            .decisions(&[true, true]),
        )
    };
    let r = go(c.clone());
    let first = &r.requests[0].user;
    assert!(!first.contains("door code"), "{first}");
    assert!(!first.contains("Run"), "{first}");
    assert!(first.contains("Jarvis's own app is in front"), "{first}");
    assert!(!r.all_prompt_text().contains("4521"));
    assert!(
        r.requests[1]
            .user
            .contains("Jarvis does not operate its own app"),
        "{}",
        r.requests[1].user
    );
    // Twenty-third review (R23): only Safari's chrome seeing the tap on the page asks.
    assert!(flows_but_chrome(&r).is_empty(), "{:?}", r.confirms);
    assert_eq!(
        r.acts
            .iter()
            .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
            .count(),
        1
    );
    // Driven: the model reads it, and it is a source for the page.
    c.drive_own_app = true;
    let r = go(c);
    assert!(r.requests[0].user.contains("door code"));
    let fq = flows_but_chrome(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(fq[0].summary.contains("Jarvis"), "{}", fq[0].summary);
}

/// The stale-screen guard's fresh look: the flow is decided on the fresh look, and a
/// decline sends nothing.
#[test]
fn flow_is_asked_on_the_guards_fresh_look() {
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let mut c = cfg();
    c.policy.max_plan_age_ms = 0;
    let r = run(
        c,
        "Look at Messages, then open https://news.example/",
        MemoryStore::new(),
        // Seventeenth review: the page comes to front while Jarvis waits.
        phone(messages(), vec![(SAFARI, a.clone())], move |c, _| match c {
            RunnerCommand::Wait { .. } => Transition::Go(a.clone()),
            _ => Transition::Stay,
        }),
        Script::model(&[WAIT, &tap(next), DONE]).decisions(&[false]),
    );
    assert_eq!(flows(&r).len(), 1, "{:?}", r.confirms);
    assert!(first_touch(&r).is_none(), "{:?}", r.acts);
}

/// R1's one question about a typing and its Return carries the flow too: one question
/// before the typing; approved, the text is typed and Return pressed with no second ask.
#[test]
fn one_question_for_typing_its_return_and_the_flow() {
    let form = |v: Option<&str>| {
        let mut f = UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 40.0))
            .with_label("Your name")
            .hittable(true)
            .focused(true);
        if let Some(v) = v {
            f = f.with_value(v);
        }
        page(Some("news.example"), vec![f], true)
    };
    let s0 = form(None);
    let field = index_of(&s0, "Your name");
    let r = run(
        cfg(),
        "Look at Messages, then open https://news.example/",
        MemoryStore::new(),
        // Seventeenth review: the page comes to front while Jarvis waits.
        phone(
            messages(),
            vec![(SAFARI, s0.clone())],
            move |c, _| match c {
                RunnerCommand::TypeText { text, .. } => Transition::Go(form(Some(text))),
                RunnerCommand::Wait { .. } => Transition::Go(s0.clone()),
                _ => Transition::Stay,
            },
        ),
        Script::model(&[
            WAIT,
            &format!(r#"{{"action":"type","element":{field},"text":"Ada","submit":true}}"#),
            DONE,
        ])
        .decisions(&[true]),
    );
    assert_eq!(r.confirms.len(), 1, "{:?}", r.confirms);
    let q = &r.confirms[0];
    assert!(
        q.summary.contains("This text is not from your request"),
        "{}",
        q.summary
    );
    assert!(q.summary.contains(FLOW), "{}", q.summary);
    assert!(q.phone_only);
    // It has a question of its own: not a flow-only question, the flow is one more row.
    assert!(!q.flow_only);
    assert!(q.rows.iter().any(|r| r.kind == QuestionRowKind::Flow));
    assert!(r
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::TypeText { .. })));
    assert!(r.acts.iter().any(|a| matches!(
        a,
        RunnerCommand::PressKey {
            key: jarvis_proto::Key::Return,
            ..
        }
    )));
}

/// A declined flow question blocks that flow only: a later send question in another app is
/// still asked (not "already declined a sending action").
#[test]
fn a_declined_flow_does_not_poison_categories() {
    let a = article("news.example");
    let next = index_of(&a, "Next");
    let m = messages();
    let info = index_of(&m, "Info");
    let r = run(
        cfg(),
        "Look at Messages, then open https://news.example/",
        MemoryStore::new(),
        // Seventeenth review: the page comes to front while Jarvis waits; going back to
        // Messages is then an input into it (asked, naming the page).
        phone(
            messages(),
            vec![(SAFARI, a.clone()), (MESSAGES, m)],
            move |c, _| match c {
                RunnerCommand::Wait { .. } => Transition::Go(a.clone()),
                _ => Transition::Stay,
            },
        ),
        Script::model(&[WAIT, &tap(next), &open("Messages"), &tap(info), DONE])
            .decisions(&[false, true]),
    );
    // Twenty-third review (R23, rule changed): the declined question named Messages and the
    // page into the page and into Safari's chrome (which sees the tap); leaving with
    // `open_app` is seen by both, so it would carry a declined flow and ends the task
    // `Blocked` on that flow (was: asked, naming the page). Still not a category.
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}\n{}", r.confirms, r.outcome.summary);
    assert_eq!(r.outcome.status, jarvis_proto::OutcomeStatus::Blocked);
    assert!(
        r.outcome.summary.contains("did not allow what Jarvis read"),
        "{}",
        r.outcome.summary
    );
    assert!(
        !r.outcome.summary.contains("already declined"),
        "{}",
        r.outcome.summary
    );
}

/// Rule P end to end: Copy in Messages runs; the Paste in the page asks, names Messages and
/// says content Jarvis cannot show is entered.
#[test]
fn paste_after_copy_names_the_source() {
    let m = messages();
    let copy = index_of(&m, "Copy");
    let p = page(
        Some("news.example"),
        vec![
            UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 40.0))
                .with_label("Code")
                .hittable(true),
            node(
                Role::Other("MenuItem".into()),
                "Paste",
                rect(16.0, 160.0, 80.0, 30.0),
            ),
        ],
        false,
    );
    let paste = index_of(&p, "Paste");
    let r = run(
        cfg(),
        "Open https://news.example/",
        MemoryStore::new(),
        // Seventeenth review: the page comes to front while Jarvis waits.
        phone(m, vec![(SAFARI, p.clone())], move |c, _| match c {
            RunnerCommand::Wait { .. } => Transition::Go(p.clone()),
            _ => Transition::Stay,
        }),
        Script::model(&[&tap(copy), WAIT, &tap(paste), DONE]).decisions(&[false]),
    );
    assert_eq!(r.confirms.len(), 1, "{:?}", r.confirms);
    let q = &r.confirms[0];
    assert!(q.phone_only);
    assert!(
        q.summary.contains("content Jarvis cannot show"),
        "{}",
        q.summary
    );
    assert!(
        q.summary.contains("copied text in Messages"),
        "{}",
        q.summary
    );
    assert!(q.summary.contains(FLOW), "{}", q.summary);
    assert!(!q.flow_only);
    assert!(q.rows.iter().any(|r| r.kind == QuestionRowKind::Insert));
    assert!(q.rows.iter().any(|r| r.kind == QuestionRowKind::Flow));
    assert_eq!(render_rows(&q.rows), q.summary);
    // The Copy tap ran, the Paste did not.
    assert_eq!(
        r.acts
            .iter()
            .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
            .count(),
        1
    );
}

/// The lock flag the hub reads (F9): a locked observation sets it in the progress.
#[test]
fn progress_says_when_the_latest_observation_was_locked() {
    use jarvis_agent::Session;
    use jarvis_proto::{Event, Observation, TaskSpec, TimedEvent};
    let mut s = Session::new(cfg(), TaskSpec::new("t", "x"), MemoryStore::new());
    let _ = s.step(TimedEvent::new(1, Event::Start));
    let e = s.step(TimedEvent::new(
        2,
        Event::Observed(Observation {
            screen: messages(),
            screenshot: None,
            device_locked: true,
        }),
    ));
    assert!(matches!(e, Effect::AwaitUnlock { .. }));
    assert!(s.progress().screen_locked);
    let _ = s.step(TimedEvent::new(3, Event::DeviceUnlocked));
    let _ = s.step(TimedEvent::new(
        4,
        Event::Observed(Observation {
            screen: messages(),
            screenshot: None,
            device_locked: false,
        }),
    ));
    assert!(!s.progress().screen_locked);
}
