#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-sixth review, landing: the fixes of the review's three low findings beyond its
//! probes (`twentysixth_probe.rs`), and tests pinning the four rulings on its document gaps.
//!
//! - L1 follow-up: a tap on a quick action in an icon's menu launches the app with an action
//!   the model chose; it is an input into an undecidable destination, never SpringBoard
//!   only, whatever its label says.
//! - D1: a decline answers the question that was shown; it does not revoke an approval the
//!   owner gave to an earlier question.
//! - D2: a place renamed during a step (Safari's address field focused): the rest of the
//!   approved step is sent; the next step is decided against the renamed place.
//! - D3: a scroll whose drag starts on a button, a link or an adjustable control is a full
//!   input; one starting on a text field or a plain cell only scrolls.
//! - D4: a `wait` in Jarvis's own app is sent (at its level) and asks no flow question.

mod common;

use common::*;
use jarvis_agent::gestures::{drag_gesture, GestureKind};
use jarvis_memory::MemoryStore;
use jarvis_observe::compact;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    Action, Direction, OutcomeStatus, ParamKind, RiskCategory, Role, RunnerCommand, Screen, Tier,
    ToolParam, ToolSpec, UiNode,
};
use jarvis_safety::InputKind;

const FLOW: &str = "what it read there could be passed on here";

fn scr(app: &str, name: &str, keyboard: bool, children: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    screen(app, name, root, keyboard)
}

fn flows(r: &Run) -> Vec<String> {
    r.confirms
        .iter()
        .filter(|c| c.summary.contains(FLOW))
        .map(|c| c.summary.clone())
        .collect()
}

fn messages() -> Screen {
    scr(
        "com.apple.MobileSMS",
        "Messages",
        false,
        vec![
            text(
                "Bank: your code is 482913. Balance $12,480.17",
                rect(16.0, 140.0, 361.0, 40.0),
            ),
            node(Role::Button, "Info", rect(300.0, 60.0, 80.0, 44.0)),
        ],
    )
}

fn home() -> Screen {
    scr(
        "com.apple.springboard",
        " ",
        false,
        vec![
            node(Role::Button, "Shop", rect(20.0, 300.0, 64.0, 64.0)),
            node(Role::Button, "Messages", rect(110.0, 300.0, 64.0, 64.0)),
        ],
    )
}

/// The Home Screen with the icon menu a long press on Shop opens: its quick actions are
/// menu items (XCUITest's `MenuItem`), one labelled like another known app and one like
/// SpringBoard's own search. `in_sheet`: the menu is presented in a sheet (kept whole by the
/// home screen rule); otherwise it is outside every alert and sheet (stubbed for the model).
fn icon_menu(in_sheet: bool) -> Screen {
    let item = |label: &str, y: f64| {
        node(
            Role::Other("MenuItem".into()),
            label,
            rect(20.0, y, 250.0, 44.0),
        )
    };
    let menu = UiNode::new(Role::Other("Menu".into()), rect(20.0, 380.0, 250.0, 150.0))
        .with_children(vec![
            item("New Order", 380.0),
            item("Messages", 430.0),
            item("Search", 480.0),
        ]);
    let menu = if in_sheet {
        UiNode::new(Role::Sheet, rect(20.0, 380.0, 250.0, 150.0)).with_children(vec![menu])
    } else {
        menu
    };
    scr(
        "com.apple.springboard",
        " ",
        false,
        vec![
            node(Role::Button, "Shop", rect(20.0, 300.0, 64.0, 64.0)),
            node(Role::Button, "Messages", rect(110.0, 300.0, 64.0, 64.0)),
            menu,
        ],
    )
}

fn shop() -> Screen {
    scr(
        "com.example.shop",
        "Shop",
        false,
        vec![
            text("Today's deals", rect(16.0, 100.0, 361.0, 30.0)),
            node(Role::Button, "Deals", rect(16.0, 200.0, 120.0, 44.0)),
            node(Role::Button, "Cart", rect(200.0, 200.0, 120.0, 44.0)),
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

/// L1 follow-up: after a long press on Shop's icon (approved: a full grant "Messages → the
/// Home Screen"), a tap on one of the menu's quick actions launches Shop with an action the
/// model chose. Presented in a sheet (the items are shown to the model), the tap asks again,
/// naming Messages, whatever the item's label (one labelled like a known app or like
/// SpringBoard's search included): an undecidable destination, not SpringBoard only.
/// Outside an alert or sheet the home screen rule stubs the menu: its items are unlabelled
/// and disabled, so a tap on one is refused and nothing is sent.
#[test]
fn a_tap_on_a_quick_action_is_an_input_into_an_undecidable_destination() {
    let shop_icon = index_of(&home(), "Shop");
    let tap_item = |in_sheet: bool, it: u32| {
        run(
            config(),
            "Check my messages",
            MemoryStore::new(),
            Phone::new(messages(), move |cmd, _| match cmd {
                RunnerCommand::PressKey {
                    key: jarvis_proto::Key::Home,
                    ..
                } => Transition::Go(home()),
                RunnerCommand::LongPress { .. } => Transition::Go(icon_menu(in_sheet)),
                _ => Transition::Stay,
            }),
            Script::model(&[
                r#"{"action":"press_home"}"#,
                &format!(r#"{{"action":"long_press","element":{shop_icon},"duration_ms":800}}"#),
                &format!(r#"{{"action":"tap","element":{it}}}"#),
                r#"{"action":"done","summary":"ok"}"#,
            ])
            .decisions(&[true, true, true, true, true]),
        )
    };
    for item in ["New Order", "Messages", "Search"] {
        let r = tap_item(true, index_of(&icon_menu(true), item));
        let fq = flows(&r);
        assert!(
            fq.first()
                .is_some_and(|q| q.starts_with("Long-press") && q.contains("Messages")),
            "{item}: the long press asks as an input into the Home Screen: {fq:#?}"
        );
        // Into the Home Screen only, Messages would be allowed by the long press's full
        // grant and the tap would not name it: naming it shows the undecidable destination
        // (every place read is a source for it).
        assert!(
            fq.iter()
                .any(|q| q.starts_with(&format!("Tap \"{item}\"")) && q.contains("Messages")),
            "{item}: the quick action asks naming Messages: {fq:#?}"
        );
        assert!(
            r.acts
                .iter()
                .any(|a| matches!(a, RunnerCommand::Tap { .. })),
            "{item}: approved, the tap is sent: {:?}",
            r.acts
        );
    }
    // Outside a sheet: the stubbed item cannot be tapped.
    let stubbed = icon_menu(false);
    let it = index_of(&stubbed, "New Order");
    let r = tap_item(false, it);
    let taps = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
        .count();
    assert_eq!(taps, 0, "{:?}", r.acts);
    assert!(
        r.all_prompt_text().contains("is disabled"),
        "{}",
        r.all_prompt_text()
    );
}

/// D1: approve(Messages → Shop), then a question naming only the tool's result → Shop is
/// declined. The decline does not revoke the earlier approval: a later input that carries
/// only allowed pairs (a wait in Shop: Messages full, the tool observe) goes unasked; a
/// full input into Shop, which carries the declined source, ends the task `Blocked`.
#[test]
fn a_decline_does_not_revoke_an_earlier_approval() {
    let mut c = config();
    c.tools.push(lookup());
    let deals = index_of(&shop(), "Deals");
    let r = run(
        c,
        "Check my messages, then look up deals in the Shop",
        MemoryStore::new(),
        Phone::new(messages(), |cmd, _| match cmd {
            RunnerCommand::LaunchApp { .. } => Transition::Go(shop()),
            _ => Transition::Stay,
        }),
        Script::model(&[
            r#"{"action":"open_app","app":"Shop"}"#,
            r#"{"action":"call_tool","name":"lookup","args":{"q":"deals"}}"#,
            r#"{"action":"wait","ms":1000}"#,
            &format!(r#"{{"action":"tap","element":{deals}}}"#),
            r#"{"action":"wait","ms":1000}"#,
            &format!(r#"{{"action":"tap","element":{deals}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        // open_app (Messages → Shop), the tool call, the first wait (the tool → Shop,
        // observe), the tap (the tool → Shop, full): declined.
        .decisions(&[true, true, true, false]),
    );
    let fq = flows(&r);
    assert_eq!(fq.len(), 4, "{fq:#?}");
    assert!(
        fq[0].starts_with("Open Shop") && fq[0].contains("Messages"),
        "{fq:#?}"
    );
    let declined = &fq[3];
    assert!(
        declined.starts_with("Tap") && declined.contains("the tool lookup"),
        "{declined}"
    );
    assert!(
        !declined.contains("Messages"),
        "the declined question names only the tool (Messages was allowed): {declined}"
    );
    // The second wait went unasked and was sent; nothing was tapped in Shop.
    let waits = r
        .acts
        .iter()
        .filter(|a| matches!(a, RunnerCommand::Wait { .. }))
        .count();
    assert_eq!(waits, 2, "{:?}", r.acts);
    assert!(
        !r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::Tap { .. })),
        "{:?}",
        r.acts
    );
    assert_eq!(r.outcome.status, OutcomeStatus::Blocked, "{:?}", r.outcome);
}

/// Safari on `bank.example`, its address field focused or not (a focused address field
/// shows what is being typed, so the host is unreadable: the page is "a page with no
/// readable address").
fn safari(focused: bool, typed: &str) -> Screen {
    let value = if focused {
        typed.to_owned()
    } else {
        "bank.example".to_owned()
    };
    scr(
        "com.apple.mobilesafari",
        "Safari",
        focused,
        vec![
            UiNode::new(Role::WebView, rect(0.0, 60.0, 393.0, 720.0)).with_children(vec![
                text("Welcome to bank.example", rect(16.0, 120.0, 361.0, 30.0)),
                node(Role::Link, "Next page", rect(16.0, 300.0, 200.0, 44.0)),
            ]),
            UiNode::new(Role::TextField, rect(90.0, 790.0, 210.0, 34.0))
                .with_label("Address")
                .with_value(value)
                .hittable(true)
                .focused(focused),
        ],
    )
}

/// D2: typing into Safari's address field focuses it, so the page shown is renamed "pages in
/// Safari with no readable address" during the step. The rest of the approved typing is sent
/// unasked (the documented residual); the next step is decided against the renamed place
/// and asks, naming it.
#[test]
fn the_step_after_a_rename_is_decided_against_the_renamed_place() {
    let address = index_of(&safari(false, ""), "Address");
    let link = index_of(&safari(true, "news"), "Next page");
    let r = run(
        config(),
        "Browse bank.example, then go to news",
        MemoryStore::new(),
        Phone::new(safari(false, ""), |cmd, _| match cmd {
            RunnerCommand::Tap { .. } => Transition::Go(safari(true, "")),
            RunnerCommand::TypeText { .. } => Transition::Go(safari(true, "news")),
            _ => Transition::Stay,
        }),
        Script::model(&[
            &format!(r#"{{"action":"type","element":{address},"text":"news"}}"#),
            &format!(r#"{{"action":"tap","element":{link}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true, true, true]),
    );
    let fq = flows(&r);
    assert!(
        r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::TypeText { .. })),
        "the approved typing is sent: {:?}; {fq:#?}",
        r.acts
    );
    let tap = fq
        .iter()
        .find(|q| q.starts_with("Tap \"Next page\""))
        .unwrap_or_else(|| panic!("the tap after the rename asks: {fq:#?}"));
    assert!(
        tap.contains("pages in Safari with no readable address"),
        "decided against the renamed place: {tap}"
    );
}

/// The kind of an element-less downward scroll on a plain screen whose drag starts on
/// `start` (a node of `role` with `label`, placed under the drag's touch-down point).
fn scroll_kind_starting_on(role: Role, label: &str, value: Option<&str>) -> InputKind {
    let base = scr("com.example.shop", "Shop", false, Vec::new());
    let from = drag_gesture(GestureKind::Scroll, Direction::Down, None, &compact(&base))
        .unwrap()
        .from;
    let mut start = UiNode::new(role, rect(0.0, from.y - 30.0, 393.0, 60.0))
        .with_label(label)
        .hittable(true);
    if let Some(v) = value {
        start = start.with_value(v);
    }
    let s = scr("com.example.shop", "Shop", false, vec![start]);
    let c = compact(&s);
    assert!(
        c.elements.iter().any(|e| e.frame.contains(from)),
        "the drag starts on the element: {:?} {from:?}",
        c.elements
    );
    InputKind::of(
        &Action::Scroll {
            direction: Direction::Down,
            element: None,
        },
        &c,
    )
}

/// D3: which controls make a scroll full when its drag starts on them: a button, a link or
/// an adjustable control (enabled); a text field or a plain cell does not.
#[test]
fn a_scroll_is_full_when_its_drag_starts_on_a_button_link_or_adjustable() {
    for (role, label) in [(Role::TextField, "Search"), (Role::Cell, "Order 1234")] {
        assert_eq!(
            scroll_kind_starting_on(role.clone(), label, None),
            InputKind::Scroll,
            "{role:?}"
        );
    }
    for (role, label, value) in [
        (Role::Button, "Buy now", None),
        (Role::Link, "Details", None),
        (Role::Slider, "Volume", Some("50%")),
    ] {
        assert_eq!(
            scroll_kind_starting_on(role.clone(), label, value),
            InputKind::Full,
            "{role:?}"
        );
    }
}

/// D4: with Jarvis's own app in front (not driven) and a tool result read, a `wait` is
/// sent at its level and asks no flow question (Jarvis's own app is no party; a wait does
/// not use the foreground app, so it is not refused there).
#[test]
fn a_wait_in_jarvis_own_app_is_sent_and_asks_nothing() {
    let own = scr(
        "dev.jarvis.app",
        "Jarvis",
        false,
        vec![text("Recent tasks", rect(16.0, 100.0, 300.0, 30.0))],
    );
    let mut c = config();
    c.agent_bundle_ids = vec!["dev.jarvis.app".into()];
    c.tools.push(lookup());
    let r = run(
        c,
        "look up hello world",
        MemoryStore::new(),
        Phone::still(own),
        Script::model(&[
            r#"{"action":"call_tool","name":"lookup","args":{"q":"hello world"}}"#,
            r#"{"action":"wait","ms":1000}"#,
            r#"{"action":"done","summary":"ok"}"#,
        ]),
    );
    assert!(r.confirms.is_empty(), "{:#?}", r.confirms);
    assert!(
        r.acts
            .iter()
            .any(|a| matches!(a, RunnerCommand::Wait { ms: 1000 })),
        "{:?}",
        r.acts
    );
    assert!(
        !r.all_prompt_text()
            .contains("not done: Jarvis does not operate its own app"),
        "the wait is not refused"
    );
}
