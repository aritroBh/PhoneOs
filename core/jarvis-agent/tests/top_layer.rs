#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! The top layer (iOS 26.5 simulator, probe runs 134 and 135): what is really on top of the
//! screen, on real trees.
//!
//! The runner now reports the top layer (a SpringBoard alert, else Spotlight in front, else
//! the app in front; `TopLayer` in the shell). These tests hold the core to what such looks
//! are:
//!
//! - **D. A SpringBoard alert** (`alert_notification_permission_springboard.txt`,
//!   `alert_sign_in_consent_springboard.txt`) is the place "a system alert", read when shown;
//!   R4 decides its buttons ("Allow"/"Continue" ask, phone only; "Don’t Allow"/"Cancel" are
//!   plain dismisses); an approval whose look finds an alert come up is stale; after the
//!   alert the next look is of the app again.
//! - **E. Spotlight** (`com.apple.Spotlight`, `spotlight_settings.txt`,
//!   `spotlight_example_com.txt` and their run-134 twins) is the place "Spotlight": the app
//!   row (`Identifier:GridCell` holding the app's `Icon`) is a launch of the app it names
//!   (the home-launch exemption, the deny and decline checks), every other row is an input
//!   into an undecidable destination; typing a search, its settle looks and the held Return
//!   apply there; it is no observer (an Apple process that draws its results itself).
//! - **C. A banner over the app** (`banner_springboard.txt` frame over the Playground's
//!   `controls.txt`): the runner's covering node (no text, disabled) makes the control under
//!   it covered and a tap there refused.

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact_view, render, CompactView};
use jarvis_proto::{
    Action, AppInfo, CompactScreen, Effect, Event, ImageRef, ModelOutput, Observation, ParamKind,
    Policy, Rect, RiskCategory, Role, RunnerCommand, RunnerResult, Screen, TaskSpec, Tier,
    TimedEvent, ToolParam, ToolSpec, UiNode,
};
use jarvis_safety::{
    evaluate_sheeted, grid_cell, home_view, observed_places, spotlight_app_icon, Decision,
    FlowContext, ScreenPlaces, ScreenSource, ScreenTree, HOME_SCREEN_READ, SPOTLIGHT, SYSTEM_ALERT,
};

/// What `ios` (the observe tests' parser) uses to print a compact screen.
fn compact_text(c: &CompactScreen) -> String {
    format!("{c:?}")
}

const ALERT_PERMISSION: &str =
    include_str!("fixtures/ios26_probes/alert_notification_permission_springboard.txt");
const ALERT_SIGN_IN: &str =
    include_str!("fixtures/ios26_probes/alert_sign_in_consent_springboard.txt");
const SPOTLIGHT_SETTINGS: &str = include_str!("fixtures/ios26_probes/spotlight_settings.txt");
const SPOTLIGHT_EXAMPLE: &str = include_str!("fixtures/ios26_probes/spotlight_example_com.txt");
const SPOTLIGHT_SETTINGS_134: &str =
    include_str!("fixtures/ios26_probes/spotlight_settings_134.txt");
const SPOTLIGHT_EXAMPLE_134: &str =
    include_str!("fixtures/ios26_probes/spotlight_example_com_134.txt");
const SPOTLIGHT_RUNNER: &str =
    include_str!("fixtures/ios26_probes/spotlight_springboard_runner.txt");
const BANNER_HOME: &str = include_str!("fixtures/ios26_probes/banner_springboard.txt");
const CONTROLS: &str = include_str!("fixtures/ios26_probes/controls.txt");

const SPOTLIGHT_ID: &str = "com.apple.spotlight";

fn parse(text: &str) -> Screen {
    ios::parse_screen(text).unwrap()
}

/// The screen as the session works on it (SpringBoard through `home_view`).
fn seen(s: &Screen) -> Screen {
    home_view(s).unwrap_or_else(|| s.clone())
}

fn tree(v: &CompactView) -> ScreenTree<'_> {
    ScreenTree {
        sources: &v.sources,
        parents: &v.parents,
        ends: &v.ends,
        bars: &v.bars,
        nodes: &v.nodes,
        inputs: &v.inputs,
    }
}

fn places_of(s: &Screen, v: &CompactView) -> ScreenPlaces {
    observed_places(
        s,
        &v.screen,
        ScreenSource::Typed,
        Some((&v.webviews, &v.ends, &v.sources)),
    )
}

/// The gate's decision for `action` on `s` (as the session sees it), nothing read.
fn decide(policy: &Policy, action: &Action, s: &Screen) -> Decision {
    let s = seen(s);
    let v = compact_view(&s);
    let share = places_of(&s, &v).share_sheet();
    evaluate_sheeted(
        policy,
        action,
        &v.screen,
        None,
        ScreenSource::Typed,
        Some(&tree(&v)),
        &[],
        &["help me with this screen"],
        false,
        false,
        Some(share),
    )
}

fn default_policy() -> Policy {
    Policy {
        first_use_approval: false,
        ..Policy::default()
    }
}

fn relaxed() -> Policy {
    Policy {
        confirm: Vec::new(),
        first_use_approval: false,
        ..Policy::default()
    }
}

fn element<'a>(c: &'a CompactScreen, label: &str) -> &'a jarvis_proto::Element {
    c.elements
        .iter()
        .find(|e| e.label == label)
        .unwrap_or_else(|| panic!("no element {label:?}: {:#?}", c.elements))
}

fn tap(c: &CompactScreen, label: &str) -> Action {
    Action::Tap {
        element: element(c, label).index,
    }
}

fn tap_json(s: &Screen, label: &str) -> String {
    let c = compact_view(&seen(s)).screen;
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        element(&c, label).index
    )
}

fn lookup() -> ToolSpec {
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

fn flows(r: &Run) -> Vec<String> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains("could be passed on"))
        .map(|c| c.summary.clone())
        .collect()
}

const LOOKUP: &str = r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#;
const DONE: &str = r#"{"action":"done","summary":"ok"}"#;

fn edit(n: &mut UiNode, f: &mut impl FnMut(&mut UiNode)) {
    f(n);
    for c in &mut n.children {
        edit(c, f);
    }
}

// ---- D. A SpringBoard alert over an app ----------------------------------------------------

/// Both real alerts are SpringBoard's: the screen is the place "a system alert", read, named
/// so as a source and as a destination (fails on the old code: the Home Screen, named
/// `HOME_SCREEN_READ`).
#[test]
fn a_springboard_alert_is_a_system_alert() {
    for text in [ALERT_PERMISSION, ALERT_SIGN_IN] {
        let s = seen(&parse(text));
        assert_eq!(s.app_bundle_id, "com.apple.springboard");
        let v = compact_view(&s);
        let p = places_of(&s, &v);
        assert_eq!(p.read_places(), vec![FlowContext::SystemAlert]);
        assert_eq!(p.base(), &FlowContext::SystemAlert);
        assert_eq!(FlowContext::SystemAlert.display(|_| None), SYSTEM_ALERT);
        assert_eq!(
            FlowContext::SystemAlert.display_read(|_| None),
            SYSTEM_ALERT
        );
        assert_ne!(SYSTEM_ALERT, HOME_SCREEN_READ);
        // The alert is kept unchanged and in full by the Home Screen rule.
        let all = render(&v.screen, 1_000_000);
        assert!(
            all.contains("Wants to Use") || all.contains("Would Like to"),
            "{all}"
        );
    }
    // A Home Screen with no alert stays the Home Screen.
    let home = seen(&parse(BANNER_HOME));
    let hv = compact_view(&home);
    assert_eq!(
        places_of(&home, &hv).read_places(),
        vec![FlowContext::app("com.apple.springboard")]
    );
}

/// R4 on the real alerts (kept as R4 states it): "Allow" and "Continue" answer a permission
/// or consent prompt — asked, phone only, at least `SystemSetting` (an unclear category: not
/// asked when the owner emptied the confirm list, as before); "Don’t Allow" (with the OS's
/// right single quotation mark) and "Cancel" are plain dismisses, not asked by R4.
#[test]
fn r4_decides_the_real_alerts_buttons() {
    for (text, yes, no) in [
        (ALERT_PERMISSION, "Allow", "Don\u{2019}t Allow"),
        (ALERT_SIGN_IN, "Continue", "Cancel"),
    ] {
        let s = parse(text);
        let c = compact_view(&seen(&s)).screen;
        match decide(&default_policy(), &tap(&c, yes), &s) {
            Decision::Confirm(r) => {
                assert!(r.phone_only, "{yes}: {r:?}");
                assert!(
                    jarvis_safety::max_risk(r.category, RiskCategory::SystemSetting) == r.category,
                    "{yes}: {:?}",
                    r.category
                );
                // The alert's title (the app and the site it names) is quoted as on-screen
                // text, never as Jarvis's words.
                assert!(r.summary.contains("Jarvis"), "{}", r.summary);
            }
            other => panic!("{yes}: {other:?}"),
        }
        assert_eq!(
            decide(&default_policy(), &tap(&c, no), &s),
            Decision::Allow,
            "{no}"
        );
        assert_eq!(
            decide(&relaxed(), &tap(&c, no), &s),
            Decision::Allow,
            "{no}"
        );
    }
}

/// A question after the alert was read names it "a system alert" (fails on the old code:
/// the Home Screen with notifications or search results).
#[test]
fn a_question_after_the_alert_names_a_system_alert() {
    for text in [ALERT_PERMISSION, ALERT_SIGN_IN] {
        let mut c = config();
        c.tools.push(lookup());
        let r = run(
            c,
            "look up hello",
            MemoryStore::new(),
            Phone::still(parse(text)),
            Script::model(&[LOOKUP, DONE]).decisions(&[false; 4]),
        );
        let fq = flows(&r);
        assert!(fq.iter().any(|q| q.contains(SYSTEM_ALERT)), "{fq:#?}");
        assert!(fq.iter().all(|q| !q.contains(HOME_SCREEN_READ)), "{fq:#?}");
    }
}

/// The app below the alert was read; pressing "Cancel" on the alert is an input into it
/// (asks under Rule T), its "Continue" asks as a consent prompt, phone only, in one
/// question; the commands on the alert are planned on SpringBoard, and once the alert is
/// gone the next command is planned on the app again (an ordinary arrival).
#[test]
fn an_alert_over_the_app_and_back() {
    let app = parse(CONTROLS);
    let alert = parse(ALERT_SIGN_IN);
    let back = app.clone();
    let alert2 = alert.clone();
    let mut c = config();
    c.tools.push(lookup());
    let r = run(
        c,
        "look up hello, then sign in",
        MemoryStore::new(),
        Phone::new(app.clone(), move |cmd, s| {
            if s.app_bundle_id == "dev.jarvis.app" && point_of(cmd).is_some() {
                jarvis_safety_alert_up(&alert2)
            } else if s.app_bundle_id == "com.apple.springboard" && point_of(cmd).is_some() {
                Transition::Go(back.clone())
            } else {
                Transition::Stay
            }
        }),
        Script::model(&[
            &tap_json(&app, "Fact probes"),
            &tap_json(&alert, "Continue"),
            &tap_json(&app, "Fact probes"),
            DONE,
        ])
        .decisions(&[true; 8]),
    );
    let expects: Vec<String> = r
        .effects
        .iter()
        .filter_map(|e| match e {
            Effect::Act(cmd) if point_of(cmd).is_some() => {
                cmd.expect().map(|x| x.bundle_id.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        expects,
        vec![
            "dev.jarvis.app".to_owned(),
            "com.apple.springboard".to_owned(),
            "dev.jarvis.app".to_owned()
        ],
        "{:#?}",
        r.confirms
    );
    // "Continue" asked as a consent prompt, phone only.
    let cont = r
        .confirms
        .iter()
        .find(|q| q.summary.starts_with("Tap \"Continue\""))
        .unwrap_or_else(|| panic!("{:#?}", r.confirms));
    assert!(cont.phone_only);
    assert_ne!(cont.category, RiskCategory::None);
    // No app was brought forward behind the alert.
    assert!(
        !r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::LaunchApp { .. })),
        "{:#?}",
        r.acts
    );
}

fn jarvis_safety_alert_up(alert: &Screen) -> Transition {
    Transition::Go(alert.clone())
}

/// An approval whose look finds a system alert come up is stale: the step is decided again
/// on the alert, and the app is not brought forward behind it (fails on the old code: a
/// SpringBoard look counted as the Home Screen passed through, and the app was launched).
#[test]
fn an_approval_whose_look_finds_an_alert_is_stale() {
    let app = parse(CONTROLS);
    let alert = parse(ALERT_SIGN_IN);
    let mut c = config();
    c.tools.push(lookup());
    let mut session = Session::new(c, TaskSpec::new("t1", "look up hello"), MemoryStore::new());
    let mut model = std::collections::VecDeque::from(vec![
        LOOKUP.to_owned(),
        tap_json(&app, "Fact probes"),
        DONE.to_owned(),
        DONE.to_owned(),
    ]);
    let mut screen = app.clone();
    let mut confirmed = false;
    let mut swapped = false;
    let mut acts: Vec<RunnerCommand> = Vec::new();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for _ in 0..500 {
        let effect = session.step(TimedEvent::new(now, event));
        now += 50;
        event = match effect {
            Effect::Observe { .. } => {
                if confirmed && !swapped {
                    swapped = true;
                    screen = alert.clone();
                }
                Event::Observed(Observation {
                    screen: screen.clone(),
                    screenshot: None::<ImageRef>,
                    device_locked: false,
                })
            }
            Effect::Infer(_) => Event::ModelOutput(ModelOutput {
                text: model.pop_front().unwrap_or_else(|| DONE.to_owned()),
            }),
            Effect::Act(cmd) => {
                acts.push(cmd.without_expect());
                Event::ActResult(RunnerResult::ok())
            }
            Effect::CallTool(_) => Event::ToolResult(jarvis_proto::ToolResult {
                ok: true,
                output: "hello".into(),
                error: None,
            }),
            Effect::Confirm(req) => {
                confirmed |= req.summary.starts_with("Tap \"Fact probes\"");
                Event::UserDecision { approved: true }
            }
            Effect::AskUser { .. } => Event::UserAnswer {
                text: String::new(),
            },
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(_) => break,
        };
    }
    assert!(confirmed, "the tap asked nothing");
    assert!(swapped);
    assert!(
        !acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::LaunchApp { .. })),
        "{acts:#?}"
    );
    assert!(acts.iter().all(|a| point_of(a).is_none()), "{acts:#?}");
}

/// An app whose display name reads as Jarvis's words for the alert or for Spotlight is
/// shown by its bundle id in a question (fails on the old code: shown by its name).
#[test]
fn app_names_like_the_new_words_are_shown_by_bundle_id() {
    for name in ["System Alert Center", "Spotlight Notes"] {
        let root = UiNode::new(Role::Application, Rect::new(0.0, 0.0, 393.0, 852.0))
            .with_label(name)
            .with_children(vec![UiNode::new(
                Role::StaticText,
                Rect::new(20.0, 100.0, 300.0, 40.0),
            )
            .with_label("A note")]);
        let s = jarvis_proto::fixtures::screen("com.example.words", name, root, false);
        let mut c = config();
        c.known_apps.push(AppInfo::new("com.example.words", name));
        c.tools.push(lookup());
        let r = run(
            c,
            "look up hello",
            MemoryStore::new(),
            Phone::still(s),
            Script::model(&[LOOKUP, DONE]).decisions(&[false; 4]),
        );
        let fq = flows(&r);
        assert!(!fq.is_empty());
        assert!(fq.iter().all(|q| !q.contains(name)), "{fq:#?}");
        assert!(
            fq.iter().any(|q| q.contains("com.example.words")),
            "{fq:#?}"
        );
    }
}

// ---- E. Spotlight is a place of its own ------------------------------------------------------

/// A look at `com.apple.Spotlight` is a read of the place "Spotlight" (fails on the old code:
/// named by its bundle id). What the runner read before the top-layer rule (SpringBoard,
/// the status bar only) was no read at all: an icons-only Home Screen, which is why that
/// look must no longer be reachable while Spotlight is up (the shell's `TopLayer`).
#[test]
fn spotlight_is_a_place_of_its_own() {
    for text in [
        SPOTLIGHT_SETTINGS,
        SPOTLIGHT_EXAMPLE,
        SPOTLIGHT_SETTINGS_134,
        SPOTLIGHT_EXAMPLE_134,
    ] {
        let s = seen(&parse(text));
        assert_eq!(s.app_bundle_id, "com.apple.Spotlight");
        let v = compact_view(&s);
        let place = FlowContext::app(SPOTLIGHT_ID);
        assert_eq!(places_of(&s, &v).read_places(), vec![place.clone()]);
        assert_eq!(place.display(|_| None), SPOTLIGHT);
        assert_eq!(place.display_read(|_| None), SPOTLIGHT);
        // The field and the result rows are listed.
        assert!(v.screen.elements.iter().any(|e| e.role.is_text_input()));
        assert!(v.screen.elements.iter().any(|e| e.role == Role::Cell));
    }
    let old = seen(&parse(SPOTLIGHT_RUNNER));
    let ov = compact_view(&old);
    assert_eq!(old.app_bundle_id, "com.apple.springboard");
    assert!(ov.screen.elements.iter().all(|e| e.role != Role::Cell));
    assert!(places_of(&old, &ov).read_places().is_empty());
}

/// The app row is decided by the OS's identifiers: the real `GridCell` holding the `Icon`
/// "Settings" (both runs); no other row of the real trees is one.
#[test]
fn the_real_app_row_is_the_grid_cell_with_its_icon() {
    for text in [SPOTLIGHT_SETTINGS, SPOTLIGHT_SETTINGS_134] {
        let s = parse(text);
        let v = compact_view(&s);
        let c = &v.screen;
        let icon = element(c, "Settings");
        assert_eq!(icon.role, Role::Button);
        assert_eq!(
            spotlight_app_icon(icon, c, Some(&tree(&v))).as_deref(),
            Some("Settings")
        );
        assert!(v.nodes.iter().filter(|n| n.grid_cell).count() >= 1);
        for e in c.elements.iter().filter(|e| e.role == Role::Cell) {
            assert_eq!(
                spotlight_app_icon(e, c, Some(&tree(&v))),
                None,
                "{}",
                e.label
            );
        }
    }
    for text in [SPOTLIGHT_EXAMPLE, SPOTLIGHT_EXAMPLE_134] {
        let s = parse(text);
        let v = compact_view(&s);
        assert!(v.nodes.iter().all(|n| !n.grid_cell));
        for e in &v.screen.elements {
            assert_eq!(spotlight_app_icon(e, &v.screen, Some(&tree(&v))), None);
        }
    }
    // The identifier alone decides a raw node.
    let mut cell = UiNode::new(Role::Cell, Rect::new(0.0, 0.0, 10.0, 10.0));
    cell.identifier = Some("Identifier:GridCell,Section:0,Row:0".into());
    assert!(grid_cell(&cell));
    cell.identifier = Some("Identifier:ResultCell,Section:0,Row:0".into());
    assert!(!grid_cell(&cell));
}

/// A tap on the real app row is a launch of Settings: the deny list refuses it (fails on the
/// old code: an input into Spotlight, sent).
#[test]
fn the_app_row_is_a_launch_of_its_app() {
    let s = parse(SPOTLIGHT_SETTINGS);
    let mut c = config();
    c.policy.denied_apps = vec!["com.apple.Preferences".into()];
    let r = run(
        c,
        "open settings",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[&tap_json(&s, "Settings"), DONE]).decisions(&[true; 4]),
    );
    assert!(
        !r.acts.iter().any(|a| taps(a, &s, "Settings")),
        "{:#?}",
        r.acts
    );
}

/// After something else was read, a tap on the app row asks about what was read only, not
/// about Spotlight's own content (the home-launch exemption, as for a Home Screen icon); a
/// tap on any other row — the go-to-web row, a website row, an app result that is no app
/// row, a row relabelled with a known app's name — is an input into an undecidable
/// destination: Spotlight's content is a source for it (fails on the old code: every row an
/// input into Spotlight itself, which is no source for itself).
#[test]
fn every_other_row_is_an_undecidable_destination() {
    let settings = parse(SPOTLIGHT_SETTINGS);
    let example = parse(SPOTLIGHT_EXAMPLE);
    // A row whose label is a known app's display name but which is no app row.
    let mut renamed = settings.clone();
    edit(&mut renamed.root, &mut |n| {
        if n.label.as_deref() == Some("Watch") {
            n.label = Some("Messages".into());
        }
    });
    // An app row whose icon carries more than the name ("Settings, App"): no app named so.
    let mut suffixed = settings.clone();
    edit(&mut suffixed.root, &mut |n| {
        if n.role == Role::Button && n.label.as_deref() == Some("Settings") {
            n.label = Some("Settings, App".into());
        }
    });
    let cases: [(&Screen, &str, bool); 7] = [
        (&settings, "Settings", false),
        (&settings, "settings", true),
        (&settings, "Watch", true),
        (
            &settings,
            "Windows Settings, Configuration interface of Microsoft Windows",
            true,
        ),
        (&example, "example.com", true),
        (&renamed, "Messages", true),
        (&suffixed, "Settings, App", true),
    ];
    for (screen, label, undecidable) in cases {
        let mut c = config();
        c.tools.push(lookup());
        let r = run(
            c,
            "look up hello and open it",
            MemoryStore::new(),
            Phone::still(screen.clone()),
            Script::model(&[LOOKUP, &tap_json(screen, label), DONE]).decisions(&[true; 6]),
        );
        let q = flows(&r)
            .into_iter()
            .find(|q| q.starts_with(&format!("Tap \"{}", &label[..label.len().min(16)])))
            .unwrap_or_else(|| panic!("{label}: {:#?}", r.confirms));
        // The places read the question names (the question names the app in front, Spotlight,
        // in its action line in every case).
        let read = q.split("Jarvis has read").nth(1).unwrap_or_default();
        assert_eq!(read.contains(SPOTLIGHT), undecidable, "{label}: {q}");
        assert!(q.contains("the tool lookup"), "{label}: {q}");
    }
}

/// Typing a denied app's name into Spotlight is refused before it is typed (fails on the old
/// code: typed).
#[test]
fn typing_a_denied_apps_name_into_spotlight_is_refused() {
    let s = parse(SPOTLIGHT_SETTINGS);
    let field = element(&compact_view(&s).screen, "Settings").index;
    let field = compact_view(&s)
        .screen
        .elements
        .iter()
        .find(|e| e.role.is_text_input())
        .map_or(field, |e| e.index);
    let mut c = config();
    c.policy.denied_apps = vec!["com.apple.Preferences".into()];
    let r = run(
        c,
        "search for Settings",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{field},"text":"Settings"}}"#),
            DONE,
        ])
        .decisions(&[true; 4]),
    );
    assert!(
        !r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::TypeText { .. })),
        "{:#?}",
        r.acts
    );
}

/// Return in Spotlight opens the top hit: it waits for the results to settle (one more look)
/// and is not pressed while a denied app's row is among them (fails on the old code: no
/// settle look, Return pressed).
#[test]
fn spotlights_return_is_held_while_a_denied_app_is_among_the_results() {
    let s = parse(SPOTLIGHT_SETTINGS);
    let field = compact_view(&s)
        .screen
        .elements
        .iter()
        .find(|e| e.role.is_text_input())
        .unwrap()
        .index;
    let mut c = config();
    c.known_apps.push(AppInfo::new("com.apple.Bridge", "Watch"));
    c.policy.denied_apps = vec!["com.apple.Bridge".into()];
    let r = run(
        c,
        "search for Settings and open it",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{field},"text":"Settings","submit":true}}"#),
            DONE,
        ])
        .decisions(&[true; 6]),
    );
    assert!(
        r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::TypeText { .. })),
        "{:#?}",
        r.acts
    );
    assert!(
        !r.acts.iter().any(|a| matches!(
            a,
            RunnerCommand::PressKey {
                key: jarvis_proto::Key::Return,
                ..
            }
        )),
        "{:#?}",
        r.acts
    );
    assert!(
        r.all_prompt_text().contains("Return was not pressed"),
        "{}",
        r.all_prompt_text()
    );
}

/// Spotlight is no observer: a wait there after a tool result was read asks nothing (fails
/// on the old code: an observe-kind input into an app); typing there is a full input into
/// it and asks.
#[test]
fn spotlight_sees_no_wait_but_typing_into_it_asks() {
    let s = parse(SPOTLIGHT_EXAMPLE);
    let field = compact_view(&s)
        .screen
        .elements
        .iter()
        .find(|e| e.role.is_text_input())
        .unwrap()
        .index;
    let mut c = config();
    c.tools.push(lookup());
    let r = run(
        c.clone(),
        "look up hello and wait",
        MemoryStore::new(),
        Phone::still(s.clone()),
        Script::model(&[LOOKUP, r#"{"action":"wait","ms":500}"#, DONE]).decisions(&[true; 4]),
    );
    // The tool call itself asks (Spotlight was read); the wait does not.
    assert!(
        r.confirms.iter().all(|q| !q.summary.starts_with("Wait")),
        "{:#?}",
        r.confirms
    );
    assert!(
        r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::Wait { .. })),
        "{:#?}",
        r.acts
    );
    let r = run(
        c,
        "look up hello and search it",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            LOOKUP,
            &format!(r#"{{"action":"type","element":{field},"text":"hello"}}"#),
            DONE,
        ])
        .decisions(&[true; 4]),
    );
    let fq = flows(&r);
    assert!(
        fq.iter()
            .any(|q| q.starts_with("Type") && q.contains("the tool lookup")),
        "{fq:#?}"
    );
}

/// First use: Spotlight is an app Jarvis acts in (its typing goes to a search service, its
/// rows are other parties'), so with first-use approval on the first input there asks to
/// act in Spotlight — unlike SpringBoard, which never does.
#[test]
fn spotlight_has_a_first_use_question() {
    let s = parse(SPOTLIGHT_EXAMPLE);
    let field = compact_view(&s)
        .screen
        .elements
        .iter()
        .find(|e| e.role.is_text_input())
        .unwrap()
        .index;
    let r = run(
        first_use_config(),
        "search hello",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{field},"text":"hello"}}"#),
            DONE,
        ])
        .decisions(&[false; 4]),
    );
    assert!(
        r.confirms
            .iter()
            .any(|q| q.category == RiskCategory::FirstUse
                && q.summary.contains("com.apple.Spotlight")),
        "{:#?}",
        r.confirms
    );
}

// ---- C. A banner over the app ------------------------------------------------------------------

/// The real banner's frame (`banner_springboard.txt`) in screen points.
const BANNER: (f64, f64, f64, f64) = (8.0, 58.7, 424.0, 66.0);

/// The runner's covering node for a banner: a control role, no text, disabled, not
/// hittable (the shell's `BannerCover.node`).
fn covering_node() -> UiNode {
    let (x, y, w, h) = BANNER;
    UiNode::new(Role::Button, Rect::new(x, y, w, h)).enabled(false)
}

/// The Playground's Controls screen (Jarvis's own app, run 134) with the real banner's
/// covering node appended last, as the runner adds it.
fn controls_under_banner() -> Screen {
    let mut s = parse(CONTROLS);
    s.root.children.push(covering_node());
    s
}

/// The banner's frame in the real SpringBoard tree is the one the covering node takes.
#[test]
fn the_real_banner_frame() {
    let s = parse(BANNER_HOME);
    let mut found = None;
    let mut stack = vec![&s.root];
    while let Some(n) = stack.pop() {
        if jarvis_safety::notification_banner(n) {
            found = Some(n.frame);
        }
        stack.extend(n.children.iter());
    }
    let (x, y, w, h) = BANNER;
    let f = found.unwrap();
    assert!((f.x - x).abs() < 0.5 && (f.y - y).abs() < 0.5);
    assert!((f.width - w).abs() < 0.5 && (f.height - h).abs() < 0.5);
}

/// The covering node marks the control under it covered (the Back button "Fact probes" at
/// `{20, 62, 44, 44}`), adds no text to what the model reads, and a tap there is refused;
/// without it the same tap is sent.
#[test]
fn a_banner_covers_the_control_under_it() {
    let plain = parse(CONTROLS);
    let covered = controls_under_banner();
    let before = render(&compact_view(&plain).screen, 1_000_000);
    let after = render(&compact_view(&covered).screen, 1_000_000);
    let line = |all: &str| {
        all.lines()
            .find(|l| l.contains("\"Fact probes\""))
            .unwrap_or_default()
            .to_owned()
    };
    assert!(!line(&before).contains("(covered)"), "{before}");
    assert!(line(&after).contains("(covered)"), "{after}");
    // Nothing of the banner's text reaches the model: the covering node is listed as an
    // unlabelled, disabled control (as the Home Screen's stubs are), and the rest is the
    // same screen with the marker.
    let added: Vec<&str> = after
        .lines()
        .filter(|l| l.contains("Button (disabled)"))
        .collect();
    assert_eq!(added.len(), 1, "{after}");
    assert!(!added[0].contains('"'), "{}", added[0]);
    assert_eq!(
        before.lines().count() + 1,
        after.lines().count(),
        "{before}\n---\n{after}"
    );
    for (screen, sent) in [(plain, true), (covered, false)] {
        let r = run(
            config(),
            "go back",
            MemoryStore::new(),
            Phone::still(screen.clone()),
            Script::model(&[&tap_json(&screen, "Fact probes"), DONE]).decisions(&[true; 4]),
        );
        let tapped = r.acts.iter().any(|a| {
            point_of(a).is_some_and(|p| {
                let (x, y, w, h) = BANNER;
                p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
            })
        });
        assert_eq!(tapped, sent, "{:#?}", r.acts);
    }
}
