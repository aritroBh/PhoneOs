#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! An app in front that the runner cannot identify (audit of the twelve-app list, iOS 26.5).
//!
//! The XCUITest runner used to read the front app from a fixed list of twelve bundle ids; any
//! other app in front (Mail, Wallet, App Store, every third-party app) was read as SpringBoard,
//! whose tree then holds no icons — an empty Home Screen, on which an element-less swipe was
//! harmless and the front-app guard passed. The shell now reports such a layer as the
//! synthetic id `unknown.front.app` (`jarvis_proto::UNIDENTIFIED_APP_BUNDLE_ID`) with a tree
//! of the application node only. These tests hold the core to that screen: it is a screen
//! Jarvis cannot read (every input asks, naming everything read, nothing is kept), named
//! "an app Jarvis cannot identify", never SpringBoard, Jarvis's own app, a known app or
//! something to launch; and a gesture planned on the Home Screen is never sent once the layer
//! is the synthetic id (it carries SpringBoard as its expectation, which the runner refuses).

mod common;

#[path = "../../jarvis-observe/tests/common/ios.rs"]
mod ios;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_observe::compact_view;
use jarvis_proto::{
    AppInfo, CompactScreen, Effect, Event, ImageRef, ModelOutput, Observation, Rect, RiskCategory,
    Role, RunnerCommand, RunnerResult, Screen, TaskSpec, TimedEvent, UiNode,
    UNIDENTIFIED_APP_BUNDLE_ID,
};
use jarvis_safety::{
    is_launcher, is_springboard, is_unidentified_app, observed_places, thin_tree, FlowContext,
    ScreenSource, UNIDENTIFIED_APP, UNREADABLE_SCREEN_PREFIX, UNREADABLE_SCREEN_SUFFIX,
};

/// What `ios` (the observe tests' parser) uses to print a compact screen.
fn compact_text(c: &CompactScreen) -> String {
    format!("{c:?}")
}

const HOME: &str = include_str!("../../jarvis-observe/tests/fixtures/ios26/springboard_home.txt");

/// The observation the runner sends for an app it cannot identify: the application node
/// only (the real screen's size, 440 x 956 pt), no name.
fn unidentified() -> Screen {
    let root = UiNode::new(Role::Application, Rect::new(0.0, 0.0, 440.0, 956.0));
    let mut s = jarvis_proto::fixtures::screen(UNIDENTIFIED_APP_BUNDLE_ID, "", root, false);
    s.app_name = None;
    s.size = jarvis_proto::Size::new(440.0, 956.0);
    s
}

fn home() -> Screen {
    ios::parse_screen(HOME).unwrap()
}

/// The screen Jarvis cannot read there, as a question names it.
fn unreadable_name() -> String {
    format!("{UNREADABLE_SCREEN_PREFIX}{UNIDENTIFIED_APP}{UNREADABLE_SCREEN_SUFFIX}")
}

fn flows(r: &Run) -> Vec<String> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains("could be passed on"))
        .map(|c| c.summary.clone())
        .collect()
}

/// A config that knows an app claiming the synthetic id (the shell never sends one; the
/// session must ignore it).
fn config_with_impostor() -> jarvis_proto::SessionConfig {
    let mut c = config();
    c.known_apps
        .push(AppInfo::new(UNIDENTIFIED_APP_BUNDLE_ID, "Messages"));
    c.agent_bundle_ids = vec![UNIDENTIFIED_APP_BUNDLE_ID.to_owned()];
    c
}

const DONE: &str = r#"{"action":"done","summary":"ok"}"#;

/// The name and the places: an unreadable screen, named "an app Jarvis cannot identify"
/// whatever name a known app gives the id; not SpringBoard, not a launcher.
#[test]
fn the_synthetic_id_is_a_screen_jarvis_cannot_read() {
    let s = unidentified();
    assert!(is_unidentified_app(&s.app_bundle_id));
    assert!(is_unidentified_app(" UNKNOWN.front.APP "));
    assert!(!is_springboard(&s.app_bundle_id));
    assert!(!is_launcher(&s.app_bundle_id));
    let v = compact_view(&s);
    assert!(v.screen.elements.is_empty(), "{:#?}", v.screen.elements);
    assert!(thin_tree(&v.screen));
    let p = observed_places(
        &s,
        &v.screen,
        ScreenSource::Typed,
        Some((&v.webviews, &v.ends, &v.sources)),
    );
    let u = FlowContext::UnreadableScreen(UNIDENTIFIED_APP_BUNDLE_ID.to_lowercase());
    assert_eq!(p.unreadable(), Some(&u));
    assert!(u.undecidable());
    // A known app's name never replaces Jarvis's words.
    let named = |_: &str| Some("Messages".to_owned());
    assert_eq!(u.display(named), unreadable_name());
    assert_eq!(
        FlowContext::app(UNIDENTIFIED_APP_BUNDLE_ID).display(named),
        UNIDENTIFIED_APP
    );
    assert_eq!(
        FlowContext::app(UNIDENTIFIED_APP_BUNDLE_ID).display_read(named),
        UNIDENTIFIED_APP
    );
    assert!(!UNIDENTIFIED_APP.contains(", ") && !UNIDENTIFIED_APP.contains(" and "));
}

/// Every input on that screen asks, naming the screen and everything read, and nothing is
/// kept: two element-less swipes, a scroll and two `tap_point`s each ask (vision model, so
/// the screen pictures were read), the looks between them ask, and once approved they run.
#[test]
fn every_input_there_asks_and_nothing_is_kept() {
    // Two runs, so that the still screen's "nothing changed" failures stay under the limit.
    let scripts: [&[&str]; 2] = [
        &[
            r#"{"action":"swipe","direction":"left"}"#,
            r#"{"action":"swipe","direction":"left"}"#,
            r#"{"action":"scroll","direction":"down"}"#,
            DONE,
        ],
        &[
            r#"{"action":"tap_point","x":0.5,"y":0.3}"#,
            r#"{"action":"look"}"#,
            r#"{"action":"tap_point","x":0.5,"y":0.4}"#,
            DONE,
        ],
    ];
    let name = unreadable_name();
    for (script, verbs) in scripts
        .iter()
        .zip([&[("Swipe", 2), ("Scroll", 1)][..], &[("Tap", 2)][..]])
    {
        let r = run(
            vision_config(),
            "Clear my inbox",
            MemoryStore::new(),
            Phone::still(unidentified()),
            Script::model(script).decisions(&[true; 16]),
        );
        let fq = flows(&r);
        for (verb, want) in verbs {
            let asked: Vec<&String> = fq.iter().filter(|q| q.starts_with(verb)).collect();
            assert_eq!(asked.len(), *want, "{verb}: {fq:#?}");
            for q in asked {
                assert!(q.contains(&name), "{q}");
                assert!(q.contains("Jarvis has read"), "{q}");
                assert!(q.contains("screen pictures"), "{q}");
                assert!(q.contains(&format!(" in {UNIDENTIFIED_APP}.")), "{q}");
                assert!(!q.contains("Home Screen"), "{q}");
            }
        }
        if verbs[0].0 == "Tap" {
            assert!(
                fq.iter()
                    .any(|q| q.starts_with("Look") && q.contains(&name)),
                "{fq:#?}"
            );
        }
        // Approved, they ran, each against the synthetic id (what the runner checks).
        let sent: Vec<&RunnerCommand> = r
            .effects
            .iter()
            .filter_map(|e| match e {
                Effect::Act(c) if c.expect().is_some() => Some(c),
                _ => None,
            })
            .collect();
        let total: usize = verbs.iter().map(|(_, n)| n).sum();
        assert_eq!(sent.len(), total, "{sent:#?}");
        for c in sent {
            assert_eq!(c.expect().unwrap().bundle_id, UNIDENTIFIED_APP_BUNDLE_ID);
        }
    }
}

/// Declined, none of them is sent.
#[test]
fn declined_nothing_is_sent() {
    let r = run(
        vision_config(),
        "Clear my inbox",
        MemoryStore::new(),
        Phone::still(unidentified()),
        Script::model(&[
            r#"{"action":"swipe","direction":"left"}"#,
            r#"{"action":"tap_point","x":0.5,"y":0.3}"#,
            DONE,
        ])
        .decisions(&[false; 8]),
    );
    assert!(
        !r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::Swipe { .. } | RunnerCommand::Tap { .. })),
        "{:#?}",
        r.acts
    );
}

/// A text model (no pictures): an element-less swipe still asks on that screen (the app's
/// place counts as read), `tap_point` is refused (no picture to aim with), and typing with
/// nothing to type into is refused; nothing is sent.
#[test]
fn without_pictures_a_swipe_asks_and_tap_point_and_typing_are_refused() {
    let r = run(
        config(),
        "Clear my inbox",
        MemoryStore::new(),
        Phone::still(unidentified()),
        Script::model(&[
            r#"{"action":"swipe","direction":"left"}"#,
            r#"{"action":"tap_point","x":0.5,"y":0.3}"#,
            r#"{"action":"type","text":"hello"}"#,
            DONE,
        ])
        .decisions(&[false; 8]),
    );
    let fq = flows(&r);
    assert!(
        fq.iter()
            .any(|q| q.starts_with("Swipe") && q.contains(&unreadable_name())),
        "{fq:#?}\n{:#?}",
        r.confirms
    );
    assert!(
        !r.acts.iter().any(|a| matches!(
            a,
            RunnerCommand::Swipe { .. }
                | RunnerCommand::Tap { .. }
                | RunnerCommand::TypeText { .. }
        )),
        "{:#?}",
        r.acts
    );
}

/// A wait there after a tool result was read asks (the screen may be another party's, which
/// could see it); once approved it is not kept (the second wait asks again).
#[test]
fn a_wait_there_asks_each_time() {
    let mut c = config();
    c.tools.push(jarvis_proto::ToolSpec {
        name: "lookup".into(),
        description: "Looks something up".into(),
        params: vec![jarvis_proto::ToolParam {
            name: "q".into(),
            kind: jarvis_proto::ParamKind::String,
            required: true,
            description: "query".into(),
        }],
        tier: jarvis_proto::Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: false,
    });
    let r = run(
        c,
        "look up hello and wait",
        MemoryStore::new(),
        Phone::still(unidentified()),
        Script::model(&[
            r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#,
            r#"{"action":"wait","ms":500}"#,
            r#"{"action":"wait","ms":500}"#,
            DONE,
        ])
        .decisions(&[true; 8]),
    );
    let waits: Vec<&String> = r
        .confirms
        .iter()
        .map(|c| &c.summary)
        .filter(|q| q.starts_with("Wait"))
        .collect();
    assert_eq!(waits.len(), 2, "{:#?}", r.confirms);
    for q in waits {
        assert!(q.contains(&unreadable_name()), "{q}");
    }
}

/// `press_home` from that screen goes home (leaving is possible); `open_app` with the
/// synthetic id is refused (it is no app) and never launched, even when a known app claims
/// the id or the shell lists it as Jarvis's own.
#[test]
fn press_home_works_and_the_synthetic_id_is_never_launched() {
    let r = run(
        config_with_impostor(),
        "go home",
        MemoryStore::new(),
        Phone::still(unidentified()),
        Script::model(&[
            &format!(r#"{{"action":"open_app","app":"{UNIDENTIFIED_APP_BUNDLE_ID}"}}"#),
            r#"{"action":"press_home"}"#,
            DONE,
        ])
        .decisions(&[true; 8]),
    );
    assert!(
        !r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::LaunchApp { .. })),
        "{:#?}",
        r.acts
    );
    assert!(
        r.acts.iter().any(|a| matches!(
            a,
            RunnerCommand::PressKey {
                key: jarvis_proto::Key::Home,
                ..
            }
        )),
        "{:#?}",
        r.acts
    );
    assert!(
        r.all_prompt_text().contains("is not an app"),
        "{}",
        r.all_prompt_text()
    );
}

/// Neither Jarvis's own app (an exempt stub when not driven) nor a known app's name: with
/// the shell (wrongly) listing the id as its own and as "Messages", a swipe there still asks
/// and names "an app Jarvis cannot identify", never "Messages".
#[test]
fn the_synthetic_id_is_never_own_or_known() {
    let r = run(
        {
            let mut c = config_with_impostor();
            c.model.supports_vision = true;
            c
        },
        "Clear my inbox",
        MemoryStore::new(),
        Phone::still(unidentified()),
        Script::model(&[r#"{"action":"swipe","direction":"left"}"#, DONE]).decisions(&[false; 4]),
    );
    let fq = flows(&r);
    assert!(
        fq.iter()
            .any(|q| q.starts_with("Swipe") && q.contains(&unreadable_name())),
        "{fq:#?}\n{:#?}",
        r.confirms
    );
    assert!(r.confirms.iter().all(|q| !q.summary.contains("Messages")));
}

/// First use: asked as "an app Jarvis cannot identify", and an approval is never kept — not
/// for the next input in the task, not in memory (the next unidentified app may be any
/// other).
#[test]
fn first_use_is_asked_and_never_kept() {
    let r = run(
        {
            let mut c = first_use_config();
            c.model.supports_vision = true;
            c
        },
        "Clear my inbox",
        MemoryStore::new(),
        Phone::still(unidentified()),
        Script::model(&[
            r#"{"action":"tap_point","x":0.5,"y":0.3}"#,
            r#"{"action":"look"}"#,
            r#"{"action":"tap_point","x":0.5,"y":0.4}"#,
            DONE,
        ])
        .decisions(&[true; 16]),
    );
    let first: Vec<&String> = r
        .confirms
        .iter()
        .filter(|q| q.category == RiskCategory::FirstUse)
        .map(|q| &q.summary)
        .collect();
    assert_eq!(
        first.len(),
        2,
        "{:#?}\n{:#?}\n{}",
        r.confirms.iter().map(|c| &c.summary).collect::<Vec<_>>(),
        r.acts,
        r.all_prompt_text()
    );
    for q in first {
        assert!(q.contains(UNIDENTIFIED_APP), "{q}");
    }
    assert!(!r.memory.is_app_approved(UNIDENTIFIED_APP_BUNDLE_ID));
}

/// The stale path: a swipe planned while the core believed it was on the Home Screen (the
/// real iOS 26.5 Home Screen, `springboard_home.txt`) carries SpringBoard as its
/// expectation. An app the runner cannot identify comes to the front before the gesture:
/// the runner's layer is now the synthetic id, so it refuses the command as stale
/// (`RunnerResult::stale`) and nothing is sent. The session looks again, sees the
/// unidentified app, and the model's next swipe asks before anything is sent (declined:
/// no swipe is ever carried out).
#[test]
fn a_swipe_planned_on_the_home_screen_is_not_sent_once_the_layer_changed() {
    let home = home();
    let mut session = Session::new(
        vision_config(),
        TaskSpec::new("t1", "show the next page"),
        MemoryStore::new(),
    );
    let mut model = std::collections::VecDeque::from(vec![
        r#"{"action":"swipe","direction":"left"}"#.to_owned(),
        r#"{"action":"swipe","direction":"left"}"#.to_owned(),
        DONE.to_owned(),
    ]);
    let mut screen = home.clone();
    // What the runner's layer is when the next command arrives.
    let mut layer = "com.apple.springboard".to_owned();
    let mut observed_after_switch = false;
    let mut refused = 0;
    let mut carried_out: Vec<RunnerCommand> = Vec::new();
    let mut expects: Vec<String> = Vec::new();
    let mut confirms: Vec<String> = Vec::new();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for _ in 0..500 {
        let effect = session.step(TimedEvent::new(now, event));
        now += 50;
        event = match effect {
            Effect::Observe { screenshot } => {
                if layer == UNIDENTIFIED_APP_BUNDLE_ID {
                    screen = unidentified();
                    observed_after_switch = true;
                }
                Event::Observed(Observation {
                    screen: screen.clone(),
                    screenshot: screenshot.then(|| ImageRef {
                        id: "shot".into(),
                        width: 1320,
                        height: 2868,
                    }),
                    device_locked: false,
                })
            }
            Effect::Infer(_) => Event::ModelOutput(ModelOutput {
                text: model.pop_front().unwrap_or_else(|| DONE.to_owned()),
            }),
            Effect::Act(cmd) => {
                if let Some(x) = cmd.expect() {
                    expects.push(x.bundle_id.clone());
                }
                // Between the look and this gesture an unidentified app came to the front.
                if matches!(cmd, RunnerCommand::Swipe { .. }) && refused == 0 {
                    layer = UNIDENTIFIED_APP_BUNDLE_ID.to_owned();
                }
                // The runner's check next to the gesture (`FrontAppGuard`).
                match cmd.expect() {
                    Some(x) if !x.matches(&layer) => {
                        refused += 1;
                        Event::ActResult(RunnerResult::stale(&layer))
                    }
                    _ => {
                        carried_out.push(cmd.without_expect());
                        Event::ActResult(RunnerResult::ok())
                    }
                }
            }
            Effect::CallTool(_) => Event::ToolResult(jarvis_proto::ToolResult {
                ok: true,
                output: String::new(),
                error: None,
            }),
            Effect::Confirm(req) => {
                confirms.push(req.summary.clone());
                Event::UserDecision { approved: false }
            }
            Effect::AskUser { .. } => Event::UserAnswer {
                text: String::new(),
            },
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(_) => break,
        };
    }
    // The swipe planned on the Home Screen was planned on SpringBoard, and refused.
    assert_eq!(
        expects.first().map(String::as_str),
        Some("com.apple.springboard"),
        "{expects:?}"
    );
    assert_eq!(refused, 1, "{expects:?}");
    assert!(observed_after_switch, "the session did not look again");
    // On the unidentified app the next swipe asked, and declined, nothing was carried out.
    assert!(
        confirms
            .iter()
            .any(|q| q.starts_with("Swipe") && q.contains(&unreadable_name())),
        "{confirms:#?}"
    );
    assert!(
        !carried_out
            .iter()
            .any(|c| matches!(c, RunnerCommand::Swipe { .. })),
        "{carried_out:#?}"
    );
}

/// An app whose display name reads as Jarvis's words for the unidentified app is shown by
/// its bundle id in a question (fails without the guard: shown by its name).
#[test]
fn an_app_named_like_the_unidentified_app_is_shown_by_bundle_id() {
    let name = "Notes Jarvis Cannot Identify";
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
    c.tools.push(jarvis_proto::ToolSpec {
        name: "lookup".into(),
        description: "Looks something up".into(),
        params: vec![jarvis_proto::ToolParam {
            name: "q".into(),
            kind: jarvis_proto::ParamKind::String,
            required: true,
            description: "query".into(),
        }],
        tier: jarvis_proto::Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: false,
    });
    let r = run(
        c,
        "look up hello",
        MemoryStore::new(),
        Phone::still(s),
        Script::model(&[
            r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#,
            DONE,
        ])
        .decisions(&[false; 4]),
    );
    let fq = flows(&r);
    assert!(!fq.is_empty(), "{:#?}", r.confirms);
    assert!(fq.iter().all(|q| !q.contains(name)), "{fq:#?}");
    assert!(
        fq.iter().any(|q| q.contains("com.example.words")),
        "{fq:#?}"
    );
}
