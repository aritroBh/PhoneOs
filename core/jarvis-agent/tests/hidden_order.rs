#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Digest v6 (CI run of df4a416, `SystemAppAgentTests.testSpringboardUnknownIconAsksOnce`):
//! XCUITest reported the two hidden, area-less children of SpringBoard's hidden
//! `Button "Utilities folder" {{0, 0}, {0, 0}}` in the other order in the snapshot taken
//! right after the owner approved; the v5 digest hashed raw order, so the approval was
//! voided and the owner was asked the same first-use question twice. The order of a run of
//! order-free siblings (every node hidden and area-less, see `jarvis_observe`'s digest
//! docs) is no longer hashed; any other reorder still voids the approval.

mod common;

use std::collections::VecDeque;

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_observe::{compact, screen_digest};
use jarvis_proto::fixtures::{node, rect, screen};
use jarvis_proto::{
    ConfirmRequest, Effect, Event, ModelOutput, Observation, Rect, Role, RunnerCommand,
    RunnerResult, Screen, TaskSpec, TimedEvent, UiNode,
};

const HOME: &str = "com.apple.springboard";
const WEATHER: &str = "com.apple.weather";

fn hidden(role: Role, label: &str, id: &str) -> UiNode {
    UiNode::new(role, Rect::default())
        .with_label(label)
        .with_identifier(id)
        .visible(false)
}

fn icon(label: &str, x: f64, y: f64) -> UiNode {
    node(Role::Button, label, rect(x, y, 72.0, 94.0)).with_identifier(label)
}

/// The home screen as the core received it in that run, reduced: the page with its icons
/// inside SpringBoard's hidden, area-less wrapper `Button`, the next page (hidden, framed),
/// the apps not on this page in a second hidden, area-less `Button` (the `Utilities` folder
/// holding `Passwords` and `Shortcuts`, both hidden and area-less, no children), and the
/// search pill. `swapped`: the folder's two children in the other order (the second
/// snapshot). `shown_swap`: two shown icons in the other order in the tree (same frames).
fn home(swapped: bool, shown_swap: bool) -> Screen {
    let folder_kids = if swapped {
        vec![
            hidden(Role::Button, "Shortcuts", "Shortcuts"),
            hidden(Role::Button, "Passwords", "Passwords"),
        ]
    } else {
        vec![
            hidden(Role::Button, "Passwords", "Passwords"),
            hidden(Role::Button, "Shortcuts", "Shortcuts"),
        ]
    };
    let mut page = vec![
        icon("Health", 20.0, 300.0),
        icon("Weather", 110.0, 300.0),
        icon("Maps", 200.0, 300.0),
    ];
    if shown_swap {
        page.swap(0, 1);
    }
    let size = jarvis_proto::fixtures::IPHONE_SIZE.to_rect();
    let root = UiNode::new(Role::Application, size)
        .with_label(" ")
        .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
            UiNode::new(Role::Other("Other".into()), size)
                .with_identifier("Home screen icons")
                .with_children(vec![
                    UiNode::new(Role::Other("Other".into()), rect(880.0, 0.0, 393.0, 852.0))
                        .visible(false),
                    UiNode::new(Role::Button, Rect::default())
                        .visible(false)
                        .with_children(page),
                    UiNode::new(Role::Button, Rect::default())
                        .visible(false)
                        .with_children(vec![
                            hidden(Role::Button, "Fitness", "Fitness"),
                            hidden(Role::Button, "Files", "Files"),
                            hidden(Role::Button, "Utilities folder", "Utilities")
                                .with_children(folder_kids),
                            hidden(Role::Button, "Jarvis", "Jarvis"),
                        ]),
                    UiNode::new(Role::Other("Other".into()), rect(150.0, 700.0, 93.0, 30.0))
                        .with_label("Search")
                        .with_value("Page 1 of 2")
                        .with_identifier("spotlight-pill"),
                ]),
        ])]);
    screen(HOME, "Home", root, false)
}

fn weather() -> Screen {
    let size = jarvis_proto::fixtures::IPHONE_SIZE.to_rect();
    let root = UiNode::new(Role::Application, size)
        .with_label("Weather")
        .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
            node(Role::StaticText, "Weather", rect(16.0, 100.0, 200.0, 40.0)),
            node(Role::Button, "Add card", rect(300.0, 60.0, 80.0, 40.0)),
        ])]);
    screen(WEATHER, "Weather", root, false)
}

/// What one scripted run did.
struct Ran {
    /// Every question asked, in order.
    confirms: Vec<ConfirmRequest>,
    /// Every runner command, in order.
    acts: Vec<RunnerCommand>,
    /// The session finished within the iteration cap.
    finished: bool,
}

/// Drives a first-use session: observations come from `looks` in order (the last one
/// repeats) until a tap, then the phone shows `after`; every confirmation is approved.
fn drive(looks: Vec<Screen>, after: Screen, model: &[&str]) -> Ran {
    let mut s = Session::new(
        first_use_config(),
        TaskSpec::new("t", "Open Weather from the home screen"),
        MemoryStore::new(),
    );
    let mut looks: VecDeque<Screen> = looks.into();
    let mut tapped = false;
    let mut model: VecDeque<String> = model.iter().map(|m| (*m).to_owned()).collect();
    let mut out = Ran {
        confirms: Vec::new(),
        acts: Vec::new(),
        finished: false,
    };
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for _ in 0..200 {
        now += 50;
        event = match s.step(TimedEvent::new(now, event)) {
            Effect::Observe { .. } => {
                let shown = if tapped {
                    after.clone()
                } else if looks.len() > 1 {
                    looks.pop_front().unwrap()
                } else {
                    looks.front().unwrap().clone()
                };
                Event::Observed(Observation::new(shown))
            }
            Effect::Infer(_) => match model.pop_front() {
                Some(text) => Event::ModelOutput(ModelOutput { text }),
                None => Event::ModelFailed {
                    error: "script exhausted".into(),
                },
            },
            Effect::Act(cmd) => {
                tapped |= matches!(cmd, RunnerCommand::Tap { .. });
                out.acts.push(cmd);
                Event::ActResult(RunnerResult::ok())
            }
            Effect::Confirm(req) => {
                out.confirms.push(req);
                Event::UserDecision { approved: true }
            }
            Effect::AskUser { .. } => Event::UserAnswer {
                text: String::new(),
            },
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::CallTool(_) => panic!("no tools here"),
            Effect::Finished(_) => {
                out.finished = true;
                return out;
            }
        };
    }
    panic!("the session did not finish");
}

fn tap_weather(s: &Screen) -> String {
    let i = compact(s)
        .elements
        .iter()
        .find(|e| e.label == "Weather")
        .unwrap()
        .index;
    format!(r#"{{"action":"tap","element":{i}}}"#)
}

const DONE: &str = r#"{"action":"done","summary":"Weather is open"}"#;

/// The fixture is the run's case: the swap is inside a run of order-free siblings, the
/// compact screen is identical, and only the digest version told them apart.
#[test]
fn the_reduced_simulator_trees_differ_only_in_hidden_order() {
    let (a, b) = (home(false, false), home(true, false));
    assert_ne!(a, b);
    assert_eq!(compact(&a), compact(&b));
    assert_eq!(
        jarvis_observe::compact_view(&a).sources,
        jarvis_observe::compact_view(&b).sources
    );
    assert_eq!(screen_digest(&a), screen_digest(&b), "digest v6");
}

/// The regression: tap proposed → first-use question → approved → the next observation
/// has the folder's children swapped → the tap IS carried out, and the owner is asked once.
#[test]
fn a_hidden_order_swap_after_the_approval_is_acted_on_and_asked_once() {
    let asked = home(false, false);
    let r = drive(
        vec![asked.clone(), home(true, false)],
        weather(),
        &[&tap_weather(&asked), DONE],
    );
    assert!(r.finished);
    assert_eq!(r.confirms.len(), 1, "asked once: {:?}", r.confirms);
    assert_eq!(r.confirms[0].category, jarvis_proto::RiskCategory::FirstUse);
    // The approved tap ran on the swapped screen, with no settle wait before it.
    assert!(
        matches!(r.acts.first(), Some(RunnerCommand::Tap { .. })),
        "{:?}",
        r.acts
    );
    let weather_icon = rect(110.0, 300.0, 72.0, 94.0);
    let RunnerCommand::Tap { x, y } = r.acts[0] else {
        unreachable!()
    };
    assert!(weather_icon.contains(jarvis_proto::Point::new(x, y)));
}

/// The inverse: two SHOWN icons in the other order in the tree (frames unchanged, so the
/// compact screen is identical) still void the approval: not carried out on that screen,
/// the owner is asked again.
#[test]
fn a_shown_order_swap_after_the_approval_is_asked_again() {
    let asked = home(false, false);
    let moved = home(false, true);
    assert_eq!(
        compact(&asked),
        compact(&moved),
        "only the tree order differs"
    );
    assert_ne!(screen_digest(&asked), screen_digest(&moved));
    let r = drive(
        vec![asked.clone(), moved.clone()],
        weather(),
        &[&tap_weather(&asked), &tap_weather(&moved), DONE],
    );
    assert!(r.finished);
    assert_eq!(r.confirms.len(), 2, "asked again: {:?}", r.confirms);
    assert_eq!(r.confirms[0].summary, r.confirms[1].summary);
    // The first approval's tap never ran: a settle wait, then the model was told.
    assert!(
        matches!(r.acts.first(), Some(RunnerCommand::Wait { .. })),
        "{:?}",
        r.acts
    );
    assert_eq!(
        r.acts
            .iter()
            .filter(|a| matches!(a, RunnerCommand::Tap { .. }))
            .count(),
        1
    );
}

/// The inverse for hidden nodes the audit found observable: two hidden, area-less texts
/// inside a shown, unlabelled button lend it its label in document order. Swapping them
/// changes what the question names, so they are not order-free: the approval is void.
#[test]
fn a_swap_of_hidden_texts_a_label_is_made_of_is_asked_again() {
    let pay = |swapped: bool| {
        let mut words = vec![
            UiNode::new(Role::StaticText, Rect::default())
                .with_label("Pay Alice")
                .visible(false),
            UiNode::new(Role::StaticText, Rect::default())
                .with_label("$40.00")
                .visible(false),
        ];
        if swapped {
            words.reverse();
        }
        let size = jarvis_proto::fixtures::IPHONE_SIZE.to_rect();
        let root = UiNode::new(Role::Application, size)
            .with_label("Bank")
            .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
                UiNode::new(Role::Button, rect(16.0, 600.0, 361.0, 50.0))
                    .hittable(true)
                    .with_children(words),
            ])]);
        screen("com.example.bank", "Bank", root, false)
    };
    let (a, b) = (pay(false), pay(true));
    assert_eq!(compact(&a).elements[0].label, "Pay Alice, $40.00");
    assert_eq!(compact(&b).elements[0].label, "$40.00, Pay Alice");
    assert_ne!(screen_digest(&a), screen_digest(&b));
    let r = drive(
        vec![a.clone(), b.clone()],
        weather(),
        &[
            r#"{"action":"tap","element":1}"#,
            r#"{"action":"tap","element":1}"#,
            DONE,
        ],
    );
    // Asked on `a` (first use, then the payment naming "Pay Alice, $40.00"); void on `b`:
    // a settle wait and no tap; asked again, naming what `b` shows; only then tapped.
    let summaries: Vec<&str> = r.confirms.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries.len(), 3, "{summaries:?}");
    assert!(summaries[1].contains("Pay Alice, $40.00"), "{summaries:?}");
    assert!(summaries[2].contains("$40.00, Pay Alice"), "{summaries:?}");
    assert!(
        matches!(r.acts.first(), Some(RunnerCommand::Wait { .. })),
        "{:?}",
        r.acts
    );
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

/// A hidden, area-less text field is never order-free (the fields a submit sends are read
/// in document order): two of them swapping changes the digest.
#[test]
fn hidden_text_fields_keep_their_order() {
    let form = |swapped: bool| {
        let mut fields = vec![
            UiNode::new(Role::TextField, Rect::default())
                .with_label("To")
                .with_value("alice@example.com")
                .visible(false),
            UiNode::new(Role::TextField, Rect::default())
                .with_label("Cc")
                .with_value("mallory@example.com")
                .visible(false),
        ];
        if swapped {
            fields.reverse();
        }
        let size = jarvis_proto::fixtures::IPHONE_SIZE.to_rect();
        let root = UiNode::new(Role::Application, size)
            .with_label("Mail")
            .with_children(vec![UiNode::new(Role::Window, size).with_children(vec![
                UiNode::new(Role::Other("Other".into()), Rect::default())
                    .visible(false)
                    .with_children(fields),
                node(Role::Button, "Send", rect(300.0, 60.0, 80.0, 40.0)),
            ])]);
        screen("com.example.mail", "Mail", root, false)
    };
    let (a, b) = (form(false), form(true));
    assert_eq!(compact(&a), compact(&b));
    assert_ne!(
        jarvis_observe::compact_view(&a).inputs,
        jarvis_observe::compact_view(&b).inputs,
        "the unlisted inputs come in document order"
    );
    assert_ne!(screen_digest(&a), screen_digest(&b));
}
