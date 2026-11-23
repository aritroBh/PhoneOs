#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Eighteenth review (agent): the screen picture is a source (E1), embedded classes that
//! cannot be told apart (E5), home-screen launches exempt only for known apps (E6, with
//! `confirm: []`), a partially approved Return decided again alone (E7). The review's
//! probes are in `eighteenth_review.rs`.

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_observe::compact_view;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{AppInfo, Key, Role, RunnerCommand, Screen, SessionConfig, UiNode};
use jarvis_safety::{observed_places, FlowContext, ScreenSource, COVERS_ALL, SCREEN_PICTURE};

const FLOW: &str = "what it read there could be passed on here";
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;
const LOOK: &str = r#"{"action":"look"}"#;
const WAIT: &str = r#"{"action":"wait","ms":500}"#;
const SAFARI: &str = "com.apple.mobilesafari";
const SOCIAL: &str = "com.example.social";
const HOME: &str = "com.apple.springboard";

fn cfg() -> SessionConfig {
    let mut c = config();
    c.known_apps.push(AppInfo::new(SAFARI, "Safari"));
    c.known_apps.push(AppInfo::new(SOCIAL, "Social"));
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

// ---- E1: the screen picture ----------------------------------------------------------------

fn acme() -> Screen {
    scr(
        "com.example.acme",
        "Acme",
        vec![
            text("Acme home", rect(16.0, 140.0, 361.0, 30.0)),
            node(Role::Button, "OK", rect(16.0, 300.0, 100.0, 40.0)),
            node(Role::Button, "Later", rect(200.0, 300.0, 100.0, 40.0)),
        ],
    )
}

/// Without pixels nothing changes: a task in one app asks no flow question, with a vision
/// model too.
#[test]
fn a_task_that_never_receives_pixels_is_unaffected() {
    let s = acme();
    let ok = index_of(&s, "OK");
    for c in [cfg(), {
        let mut c = cfg();
        c.model.supports_vision = true;
        c
    }] {
        let r = run(
            c,
            "Open Acme",
            MemoryStore::new(),
            Phone::still(s.clone()),
            Script::model(&[&tap(ok), &tap(ok), DONE]),
        );
        assert!(r.confirms.is_empty(), "{:?}", r.confirms);
        assert!(r.requests.iter().all(|q| q.image.is_none()));
        assert_eq!(taps_sent(&r), 2);
    }
}

/// After a `look` the first input into the front app asks, naming the screen picture; the
/// approval covers that destination (a second screenshot later does not ask again for it),
/// a new destination asks.
#[test]
fn the_screen_picture_is_a_source_once_per_destination() {
    let s = acme();
    let ok = index_of(&s, "OK");
    let mut c = cfg();
    c.model.supports_vision = true;
    c.known_apps.push(AppInfo::new("com.example.acme", "Acme"));
    let r = run(
        c,
        "Open Acme",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            LOOK,
            &tap(ok),
            LOOK,
            &tap(ok),
            r#"{"action":"open_app","app":"Settings"}"#,
            DONE,
        ])
        .decisions(&[true, false]),
    );
    assert_eq!(
        r.observes.iter().filter(|v| **v).count(),
        2,
        "{:?}",
        r.observes
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 2, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains(&format!("Jarvis has read {SCREEN_PICTURE} in")),
        "{}",
        fq[0].summary
    );
    assert!(fq[0].phone_only);
    // The second one is the launch of another app.
    assert!(fq[1].summary.contains(SCREEN_PICTURE), "{}", fq[1].summary);
    assert!(fq[1].summary.contains("Settings"), "{}", fq[1].summary);
    assert_eq!(taps_sent(&r), 2);
    assert!(!r
        .acts
        .iter()
        .any(|a| matches!(a, RunnerCommand::LaunchApp { .. })));
}

// ---- E5: places of embedded content, by observation -----------------------------------------

fn places(s: &Screen) -> jarvis_safety::ScreenPlaces {
    let v = compact_view(s);
    observed_places(
        s,
        &v.screen,
        ScreenSource::Typed,
        Some((&v.webviews, &v.ends, &v.sources)),
    )
}

/// An in-app browser: its page and a frame nested in it are two places (nesting level is
/// part of the place); one frame is not ambiguous, two are.
#[test]
fn nesting_levels_and_ambiguous_classes_by_observation() {
    let frame = |y: f64, label: &str| {
        UiNode::new(Role::WebView, rect(0.0, y, 393.0, 100.0))
            .with_children(vec![text(label, rect(16.0, y + 10.0, 361.0, 20.0))])
    };
    let in_app = |frames: Vec<UiNode>| {
        let mut page = vec![text("Win a prize", rect(16.0, 120.0, 361.0, 30.0))];
        page.extend(frames);
        scr(
            SOCIAL,
            "Social",
            vec![
                node(Role::Button, "Done", rect(16.0, 50.0, 80.0, 40.0)),
                UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 650.0)).with_children(page),
            ],
        )
    };
    let one = places(&in_app(vec![frame(200.0, "Continue as Jane")]));
    assert_eq!(one.base(), &FlowContext::app(SOCIAL));
    assert_eq!(
        one.embedded_places(),
        vec![
            FlowContext::Embedded {
                app: SOCIAL.into(),
                within: None
            },
            FlowContext::NestedWeb(SOCIAL.into()),
        ]
    );
    assert!(one.ambiguous().is_empty());
    let two = places(&in_app(vec![
        frame(200.0, "Continue as Jane"),
        frame(400.0, "Next"),
    ]));
    assert_eq!(two.ambiguous(), &[FlowContext::NestedWeb(SOCIAL.into())]);
    // Two top-level web views beside the app's content: that class is ambiguous.
    let tops = places(&scr(
        SOCIAL,
        "Social",
        vec![
            node(Role::Button, "Done", rect(16.0, 50.0, 80.0, 40.0)),
            frame(200.0, "Sign in"),
            frame(400.0, "Ad"),
        ],
    ));
    assert_eq!(
        tops.ambiguous(),
        &[FlowContext::Embedded {
            app: SOCIAL.into(),
            within: None
        }]
    );
}

fn two_frames(host: &str) -> Screen {
    scr(
        SAFARI,
        "Safari",
        vec![
            UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 650.0)).with_children(vec![
                text("Welcome", rect(16.0, 110.0, 361.0, 20.0)),
                UiNode::new(Role::WebView, rect(0.0, 150.0, 393.0, 150.0)).with_children(vec![
                    text("jane.doe@mail.example", rect(16.0, 190.0, 361.0, 20.0)),
                ]),
                UiNode::new(Role::WebView, rect(0.0, 400.0, 393.0, 150.0)).with_children(vec![
                    node(Role::Button, "Next", rect(16.0, 420.0, 361.0, 44.0)),
                ]),
            ]),
            UiNode::new(Role::Button, rect(100.0, 800.0, 200.0, 30.0))
                .with_label("Address")
                .with_value(host)
                .with_identifier("TabBarItemTitle"),
        ],
    )
}

/// An ambiguous class is a source for input into itself even when only it was read (here
/// the page is allowed first), and the question names it as covering all of it.
#[test]
fn an_ambiguous_class_asks_for_input_into_itself() {
    let s = two_frames("attacker.example");
    let next = index_of(&s, "Next");
    let r = run(
        cfg(),
        "Open attacker.example",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[&tap(next), &tap(next), DONE]).decisions(&[true]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 1, "{:?}", r.confirms);
    assert!(
        fq[0]
            .summary
            .contains(&format!("embedded content in attacker.example{COVERS_ALL}")),
        "{}",
        fq[0].summary
    );
    // The approval covers the class into itself: the second tap is not asked.
    assert_eq!(taps_sent(&r), 2);
}

// ---- E6: the launch exemption, `confirm: []` -------------------------------------------------

fn springboard(children: Vec<UiNode>) -> Screen {
    scr(HOME, "Home", children)
}

fn banner() -> UiNode {
    UiNode::new(
        Role::Other("NotificationBanner".to_owned()),
        rect(8.0, 50.0, 377.0, 80.0),
    )
    .with_label("Messages, now, Your sign-in code is 482913")
}

fn spotlight() -> Screen {
    springboard(vec![
        banner(),
        UiNode::new(Role::SearchField, rect(16.0, 150.0, 361.0, 40.0))
            .with_label("Search")
            .focused(true)
            .hittable(true),
        UiNode::new(Role::Cell, rect(0.0, 220.0, 393.0, 60.0))
            .with_label("evil.example/482913, Website")
            .hittable(true),
        UiNode::new(Role::Cell, rect(0.0, 290.0, 393.0, 60.0))
            .with_label("Settings")
            .hittable(true),
    ])
}

/// Under `confirm: []`, after a banner was read on the home screen: a row that is a website,
/// a search's top hit and a tap Jarvis cannot resolve keep the Home Screen as a source and
/// ask (or are refused); only a known installed app's launch is exempt.
#[test]
fn home_rows_that_are_not_known_apps_ask_under_confirm_nothing() {
    let s = spotlight();
    let row = index_of(&s, "evil.example/482913, Website");
    let settings = index_of(&s, "Settings");
    let field = index_of(&s, "Search");
    let go = |steps: &[&str]| {
        let mut c = cfg();
        c.policy.confirm = Vec::new();
        c.model.supports_vision = true;
        let mut script: Vec<&str> = steps.to_vec();
        script.push(DONE);
        run(
            c,
            "Find my code",
            MemoryStore::new(),
            Phone::still(spotlight()),
            Script::model(&script),
        )
    };
    let typ =
        format!(r#"{{"action":"type","element":{field},"text":"Find my code","submit":true}}"#);
    for (kind, steps) in [
        ("website row", vec![tap(row)]),
        ("search top hit", vec![typ.clone()]),
        (
            "unresolved point",
            vec![r#"{"action":"tap_point","x":0.5,"y":0.9}"#.to_owned()],
        ),
    ] {
        let refs: Vec<&str> = steps.iter().map(String::as_str).collect();
        let r = go(&refs);
        // Declined (or refused): nothing of it was sent.
        assert!(
            !r.acts.iter().any(|a| matches!(
                a,
                RunnerCommand::Tap { .. }
                    | RunnerCommand::PressKey {
                        key: Key::Return,
                        ..
                    }
            )),
            "{kind}: {:?}",
            r.acts
        );
        if kind != "unresolved point" {
            let fq = flows(&r);
            assert!(!fq.is_empty(), "{kind}: {:?}", r.confirms);
            assert!(
                fq[0].summary.contains("the Home Screen"),
                "{kind}: {}",
                fq[0].summary
            );
        }
    }
    // A known installed app: the launch itself is not asked for the home screen's content.
    let r = go(&[&tap(settings)]);
    assert!(flows(&r).is_empty(), "{:?}", r.confirms);
    assert_eq!(taps_sent(&r), 1);
}

// ---- E7: a partially approved Return ----------------------------------------------------------

fn long_app(k: usize) -> (String, Screen) {
    let bundle = format!("com.example.app{k}-with-a-rather-long-bundle-identifier");
    let s = scr(
        &bundle,
        &bundle,
        vec![
            text(&format!("secret {k}"), rect(16.0, 140.0, 361.0, 30.0)),
            node(Role::Button, "OK", rect(16.0, 300.0, 100.0, 40.0)),
            node(Role::Button, "Later", rect(200.0, 300.0, 100.0, 40.0)),
        ],
    );
    (bundle, s)
}

/// Five places with long names were read, then Spotlight: the typing of the owner's words
/// goes into the search field (asked, partly, twice), its Return opens a search Jarvis
/// cannot name (asked again, partly, twice). Approving the Return's first, partial question
/// decides the Return again alone: the text is typed exactly once.
#[test]
fn a_partially_approved_return_does_not_type_again() {
    let mut screens: Vec<Screen> = (1..5).map(|k| long_app(k).1).collect();
    screens.push(springboard(vec![UiNode::new(
        Role::SearchField,
        rect(16.0, 150.0, 361.0, 40.0),
    )
    .with_label("Search")
    .focused(true)
    .hittable(true)]));
    let mut shown = screens.into_iter();
    let phone = Phone::new(long_app(0).1, move |c, _| match c {
        RunnerCommand::Wait { .. } => shown.next().map_or(Transition::Stay, Transition::Go),
        _ => Transition::Stay,
    });
    let field = index_of(
        &springboard(vec![UiNode::new(
            Role::SearchField,
            rect(16.0, 150.0, 361.0, 40.0),
        )
        .with_label("Search")]),
        "Search",
    );
    let typ = format!(r#"{{"action":"type","element":{field},"text":"weather","submit":true}}"#);
    let r = run(
        cfg(),
        "Search for weather",
        MemoryStore::new(),
        phone,
        Script::model(&[WAIT, WAIT, WAIT, WAIT, WAIT, &typ, DONE]).decisions(&[true; 12]),
    );
    let partial: Vec<&String> = flows(&r)
        .iter()
        .map(|c| &c.summary)
        .filter(|s| s.contains("Jarvis asks about them next"))
        .collect();
    assert!(partial.len() >= 2, "{:?}", r.confirms);
    let typed = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::TypeText { .. }))
        .count();
    assert_eq!(typed, 1, "{:?}", r.acts);
    assert_eq!(
        r.acts
            .iter()
            .filter(|a| matches!(
                a,
                RunnerCommand::PressKey {
                    key: Key::Return,
                    ..
                }
            ))
            .count(),
        1,
        "{:?}",
        r.acts
    );
}
