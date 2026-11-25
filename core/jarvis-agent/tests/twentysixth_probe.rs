#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-sixth review: minimal reproductions of what the independent oracle
//! (`twentysixth_oracle.rs`) found. Each asserts the behaviour the document states.

mod common;

use common::*;
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    ParamKind, RiskCategory, Role, RunnerCommand, Screen, Tier, ToolParam, ToolSpec, UiNode,
};

const FLOW: &str = "what it read there could be passed on here";

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
        "com.apple.MobileSMS",
        "Messages",
        vec![
            text(
                "Bank: your code is 482913. Balance $12,480.17",
                rect(16.0, 140.0, 361.0, 40.0),
            ),
            node(
                Role::Link,
                "shop.example/daily",
                rect(16.0, 200.0, 300.0, 44.0),
            ),
            node(
                Role::Link,
                "bank.example/login",
                rect(16.0, 260.0, 260.0, 44.0),
            ),
            node(Role::Button, "Info", rect(300.0, 60.0, 80.0, 44.0)),
            UiNode::new(Role::TextField, rect(16.0, 330.0, 280.0, 40.0))
                .with_label("Message")
                .hittable(true),
            node(Role::Button, "Send", rect(305.0, 330.0, 70.0, 36.0)),
        ],
    )
}

fn flows(r: &Run) -> Vec<String> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .map(|c| c.summary.clone())
        .collect()
}

fn own_app() -> Screen {
    scr(
        "dev.jarvis.app",
        "Jarvis",
        vec![
            text("Recent tasks", rect(16.0, 100.0, 300.0, 30.0)),
            node(Role::Button, "History", rect(16.0, 200.0, 120.0, 44.0)),
        ],
    )
}

fn home() -> Screen {
    scr(
        "com.apple.springboard",
        " ",
        vec![
            node(Role::Button, "Safari", rect(20.0, 300.0, 64.0, 64.0)),
            node(Role::Button, "Messages", rect(110.0, 300.0, 64.0, 64.0)),
        ],
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

/// Probe a (LOW, over-asking; threat model (2), R23: "Jarvis's own app ... is never such an
/// observer"): with Jarvis's own app in front (not driven) and a tool result read, leaving
/// it by `press_home` — or a `wait` there — asks a flow question naming "Jarvis" as a place
/// "which can tell when Jarvis waits, looks or acts". Expected: no flow question (its own
/// app is no party; the Home Screen is the OS).
#[test]
fn leaving_jarvis_own_app_asks_no_flow_question() {
    for step in [
        r#"{"action":"press_home"}"#,
        r#"{"action":"wait","ms":1000}"#,
    ] {
        let mut c = config();
        c.agent_bundle_ids = vec!["dev.jarvis.app".into()];
        c.tools.push(lookup());
        let r = run(
            c,
            "look up hello world",
            MemoryStore::new(),
            Phone::new(own_app(), |cmd, _| match cmd {
                RunnerCommand::PressKey {
                    key: jarvis_proto::Key::Home,
                    ..
                } => Transition::Go(home()),
                _ => Transition::Stay,
            }),
            Script::model(&[
                r#"{"action":"call_tool","name":"lookup","args":{"q":"hello world"}}"#,
                step,
                r#"{"action":"done","summary":"ok"}"#,
            ]),
        );
        let fq = flows(&r);
        assert!(fq.is_empty(), "{step}: {fq:#?}");
    }
}

/// Probe c (LOW; threat model (1)-(2), E6: the exemption is for a home-screen *launch*): a
/// long press on a known app's icon is resolved as a launch of that app (`home_launch`
/// treats `LongPress` like `Tap`), so its approval keeps a full grant "what was read → the
/// app" although the long press opens SpringBoard's icon menu and puts nothing into the
/// app; the next `open_app` of that app then asks nothing about it. Expected: the input of
/// a long press goes to the place of the element it targets (SpringBoard); opening the app
/// later asks naming what was read.
#[test]
fn a_long_press_on_an_icon_keeps_no_grant_into_the_app() {
    let safari = index_of(&home(), "Safari");
    // Landing note: the shared test config does not list Safari as a known app, so as
    // written the icon was an unknown one and `open_app` failed ("unknown app"): the probe
    // failed for that reason before and after any fix. Listing Safari makes it the known
    // app's icon the probe describes (setup only; the assertion is unchanged).
    let mut c = config();
    c.known_apps.push(jarvis_proto::AppInfo::new(
        "com.apple.mobilesafari",
        "Safari",
    ));
    let r = run(
        c,
        "Check my messages",
        MemoryStore::new(),
        Phone::new(messages(), |cmd, _| match cmd {
            RunnerCommand::PressKey {
                key: jarvis_proto::Key::Home,
                ..
            } => Transition::Go(home()),
            _ => Transition::Stay,
        }),
        Script::model(&[
            r#"{"action":"press_home"}"#,
            &format!(r#"{{"action":"long_press","element":{safari},"duration_ms":800}}"#),
            r#"{"action":"open_app","app":"Safari"}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true, true, true, true]),
    );
    let fq = flows(&r);
    assert!(
        fq.iter()
            .any(|q| q.starts_with("Open Safari") && q.contains("Messages")),
        "opening Safari after the long press asks naming Messages: {fq:#?}; acts {:?}",
        r.acts
    );
}

/// Safari showing a page whose address is not readable (no address field on screen), with a
/// search field on the page.
fn safari_unreadable(search_value: &str) -> Screen {
    let mut f = UiNode::new(Role::TextField, rect(16.0, 640.0, 300.0, 40.0))
        .with_label("Search site")
        .hittable(true);
    if !search_value.is_empty() {
        f = f.with_value(search_value);
    }
    scr(
        "com.apple.mobilesafari",
        "Safari",
        vec![
            UiNode::new(Role::WebView, rect(0.0, 60.0, 393.0, 720.0)).with_children(vec![
                text("Welcome to ", rect(16.0, 120.0, 361.0, 30.0)),
                node(Role::Link, "Next page", rect(16.0, 300.0, 200.0, 44.0)),
                node(Role::Link, "Other site", rect(16.0, 360.0, 250.0, 44.0)),
                f,
                node(Role::Button, "Subscribe", rect(16.0, 700.0, 180.0, 44.0)),
            ]),
            node(Role::Button, "Back", rect(10.0, 790.0, 60.0, 34.0)),
            node(Role::Button, "Tabs", rect(320.0, 790.0, 64.0, 30.0)),
        ],
    )
}

/// Probe b (check 6, no panic): `type … submit` into a page's search field whose question
/// carries a flow note must compose without tripping the summary's slot assertion (debug
/// builds panic in `evaluate::fit_slot`; release builds hard-cut the slot).
#[test]
fn a_typing_question_with_a_flow_note_composes_without_a_panic() {
    let field = index_of(&safari_unreadable(""), "Search site");
    for value in ["", "hello world"] {
        let r = std::panic::catch_unwind(|| {
            run(
                config(),
                "Check my messages, the Shop and Mail, browse bank.example and news.example, type hello world",
                MemoryStore::new(),
                Phone::still(safari_unreadable(value)),
                Script::model(&[
                    &format!(
                        r#"{{"action":"type","element":{field},"text":"balance 12480","submit":true}}"#
                    ),
                    r#"{"action":"done","summary":"ok"}"#,
                ])
                .decisions(&[true, true, true, true]),
            )
        });
        assert!(
            r.is_ok(),
            "value {value:?}: the question's composition panicked"
        );
    }
}
