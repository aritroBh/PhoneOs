#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Twenty-fourth review: the fixes. R24 — a look is something the place looked at can see:
//! while a place on the current screen still has a source not allowed into it at the observe
//! kind, the session looks again only after a command it sent, an owner's answer or approval,
//! or an unlock. F1 — the stale-screen guard decides a step on the screen the model planned
//! on first, and looks again only for a step that would be sent unasked (at most
//! `MAX_STALE_LOOKS` per stretch without a sent command). F3 — the vision fallback is such a
//! look. F4 — the Home Screen (SpringBoard) is no observe-kind destination.

mod common;

use std::collections::VecDeque;

use common::*;
use jarvis_agent::{Session, MAX_STALE_LOOKS};
use jarvis_memory::MemoryStore;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    Effect, Event, ImageRef, ModelOutput, Observation, OutcomeStatus, ParamKind, RiskCategory,
    Role, RunnerCommand, RunnerResult, Screen, SessionConfig, TaskSpec, Tier, TimedEvent,
    ToolParam, ToolResult, ToolSpec, UiNode,
};

const FLOW: &str = "what it read there could be passed on here";
const MSGS: &str = "com.apple.MobileSMS";
const SHOP: &str = "com.example.shop";
const HOME: &str = "com.apple.springboard";

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
        MSGS,
        "Messages",
        vec![
            text(
                "Bank: your code is 482913. Balance $12,480.17",
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

/// Shop with a live countdown on its button: the label changes at every look.
fn shop(tick: u32) -> Screen {
    scr(
        SHOP,
        "Shop",
        vec![
            text("Daily reward", rect(16.0, 60.0, 200.0, 30.0)),
            node(
                Role::Button,
                &format!("Collect in 0:{:02}", 59 - (tick % 60)),
                rect(16.0, 100.0, 200.0, 40.0),
            ),
            node(Role::Button, "Settings", rect(250.0, 100.0, 120.0, 40.0)),
        ],
    )
}

/// Shop with a single control (a thin tree: the vision fallback wants a screenshot) and a
/// live countdown in its text.
fn thin_shop(tick: u32) -> Screen {
    scr(
        SHOP,
        "Shop",
        vec![
            text(
                &format!("Offer ends in 0:{:02}", 59 - (tick % 60)),
                rect(16.0, 60.0, 300.0, 30.0),
            ),
            node(
                Role::Button,
                "Delete account",
                rect(16.0, 100.0, 200.0, 40.0),
            ),
        ],
    )
}

/// What the phone shows: Messages (its link opens Shop), then Shop, whose screen advances
/// at every look; or a single app from the start.
struct Phone2 {
    screen: Screen,
    shop: fn(u32) -> Screen,
    looks: u32,
}

impl Phone2 {
    fn observe(&mut self) -> Screen {
        if self.screen.app_bundle_id == SHOP {
            self.looks += 1;
            self.screen = (self.shop)(self.looks);
        }
        self.screen.clone()
    }
    fn act(&mut self, cmd: &RunnerCommand) {
        if self.screen.app_bundle_id == MSGS && taps(cmd, &self.screen, "shop.example/daily") {
            self.screen = (self.shop)(0);
        }
    }
}

/// The previous effect of an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Before {
    /// Nothing (the first look of the task).
    Start,
    /// A runner command or a tool call the session sent.
    Sent,
    /// An owner's approval.
    Approved,
    /// An owner's answer to `ask_user`.
    Answered,
    /// Anything else: a model reply, a decline, another observation.
    Other,
}

struct Trace {
    effects: Vec<Effect>,
    /// Every observation: where it was taken, what came directly before it, whether a
    /// screenshot was asked.
    looks: Vec<(String, Before, bool)>,
    confirms: Vec<String>,
    status: OutcomeStatus,
    fresh_looks: u32,
}

impl Trace {
    /// Observations in Shop the model's timing chose: not the first, not directly after a
    /// sent command, an approval or an answer.
    fn model_timed_in_shop(&self) -> usize {
        self.looks
            .iter()
            .filter(|(app, before, _)| app == SHOP && *before == Before::Other)
            .count()
    }
    fn answered_in_shop(&self) -> usize {
        self.looks
            .iter()
            .filter(|(app, before, _)| app == SHOP && *before == Before::Answered)
            .count()
    }
}

fn drive(
    config: SessionConfig,
    start: Screen,
    shop: fn(u32) -> Screen,
    mut model: impl FnMut(usize) -> (u64, String),
    mut decisions: VecDeque<bool>,
) -> Trace {
    let mut s = Session::new(
        config,
        TaskSpec::new("t", "Read my bank message, then open the daily link"),
        MemoryStore::new(),
    );
    let mut phone = Phone2 {
        screen: start,
        shop,
        looks: 0,
    };
    let mut now = 1_000u64;
    let mut event = Event::Start;
    let mut before = Before::Start;
    let mut t = Trace {
        effects: Vec::new(),
        looks: Vec::new(),
        confirms: Vec::new(),
        status: OutcomeStatus::Failed,
        fresh_looks: 0,
    };
    let mut infers = 0usize;
    for _ in 0..3_000 {
        let effect = s.step(TimedEvent::new(now, event));
        t.effects.push(effect.clone());
        let prev = std::mem::replace(&mut before, Before::Other);
        event = match effect {
            Effect::Observe { screenshot } => {
                now += 50;
                t.looks
                    .push((phone.screen.app_bundle_id.clone(), prev, screenshot));
                let sc = phone.observe();
                Event::Observed(Observation {
                    screen: sc,
                    screenshot: screenshot.then(|| ImageRef {
                        id: "s".into(),
                        width: 1179,
                        height: 2556,
                    }),
                    device_locked: false,
                })
            }
            Effect::Infer(_) => {
                let (lat, text) = model(infers);
                infers += 1;
                now += lat;
                Event::ModelOutput(ModelOutput { text })
            }
            Effect::Act(cmd) => {
                now += 50;
                phone.act(&cmd);
                before = Before::Sent;
                Event::ActResult(RunnerResult::ok())
            }
            Effect::Confirm(c) => {
                now += 2_000;
                t.confirms.push(c.summary.clone());
                let approved = decisions.pop_front().unwrap_or(false);
                if approved {
                    before = Before::Approved;
                }
                Event::phone_decision(approved)
            }
            Effect::CallTool(_) => {
                now += 50;
                before = Before::Sent;
                Event::ToolResult(ToolResult {
                    ok: true,
                    output: "ok".into(),
                    error: None,
                })
            }
            Effect::AskUser { .. } => {
                now += 2_000;
                before = Before::Answered;
                Event::UserAnswer {
                    text: "fine".into(),
                }
            }
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(o) => {
                t.status = o.status;
                t.fresh_looks = s.progress().fresh_looks;
                return t;
            }
        };
    }
    panic!("did not finish");
}

fn link_tap() -> String {
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&messages(), "shop.example/daily")
    )
}

fn collect_tap() -> String {
    format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&shop(0), "Collect in 0:59")
    )
}

fn lookup_config(vision: bool) -> SessionConfig {
    let mut c = if vision { vision_config() } else { config() };
    c.tools.push(ToolSpec {
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
        self_contained: true,
    });
    c
}

/// The slow model's proposals in Shop.
fn proposal(kind: &str, i: usize) -> String {
    match kind {
        "tap" => collect_tap(),
        "scroll" => r#"{"action":"scroll","direction":"down"}"#.to_owned(),
        "wait" => r#"{"action":"wait","ms":1000}"#.to_owned(),
        "look" => r#"{"action":"look"}"#.to_owned(),
        "tool" => r#"{"action":"call_tool","name":"lookup","args":{"q":"x"}}"#.to_owned(),
        "invalid" => "I think I will tap the button".to_owned(),
        "ask" => r#"{"action":"ask_user","question":"Which colour?"}"#.to_owned(),
        // Taps on the changing control with an `ask_user` every fourth reply.
        "mixed" if i.is_multiple_of(4) => {
            r#"{"action":"ask_user","question":"Which colour?"}"#.to_owned()
        }
        "mixed" => collect_tap(),
        _ => unreachable!(),
    }
}

/// R24: after reading Messages and arriving in Shop through its link, a slow model (2.5 s or
/// 6 s per reply) causes no look in Shop beyond the arrival look, whatever it proposes, with
/// every question declined — with a declined wait first and without; an `ask_user`'s answer
/// is the owner's moment (one look per answer, nothing else).
#[test]
fn a_slow_model_in_the_restricted_state_causes_no_look_beyond_the_arrival_look() {
    for kind in [
        "tap", "scroll", "wait", "look", "tool", "invalid", "ask", "mixed",
    ] {
        for prior_decline in [false, true] {
            let link = link_tap();
            let t = drive(
                lookup_config(kind == "look"),
                messages(),
                shop,
                |i| match i {
                    0 => (200, link.clone()),
                    1 if prior_decline => (200, r#"{"action":"wait","ms":1000}"#.to_owned()),
                    _ => (
                        if i.is_multiple_of(2) { 2_500 } else { 6_000 },
                        proposal(kind, i),
                    ),
                },
                VecDeque::new(),
            );
            let case = format!("{kind}, prior decline {prior_decline}");
            assert_eq!(
                t.model_timed_in_shop(),
                0,
                "{case}: looks {:?}; confirms {:?}; status {:?}",
                t.looks,
                t.confirms,
                t.status
            );
            // The arrival look follows the link's tap; nothing else in Shop but the looks
            // after the owner's answers.
            let in_shop = t.looks.iter().filter(|(a, _, _)| a == SHOP).count();
            assert_eq!(in_shop, 1 + t.answered_in_shop(), "{case}: {:?}", t.looks);
            assert!(
                !t.looks.iter().any(|(a, _, shot)| a == SHOP && *shot),
                "{case}: a screenshot in Shop"
            );
            assert_eq!(t.fresh_looks, 0, "{case}: a stale-screen guard look");
            if kind != "ask" && kind != "invalid" {
                assert!(
                    t.confirms.iter().any(|c| c.contains(FLOW)),
                    "{case}: {:?}",
                    t.confirms
                );
            }
        }
    }
}

/// R24 (fix 2): in an app that read nothing else (nothing is restricted), a slow model's taps
/// on a control whose label changes at every look get at most `MAX_STALE_LOOKS` fresh looks
/// in the stretch without a sent command; the steps after them fail with no look, and the
/// task ends at the failure budget — an `ask_user` between them resets neither bound.
#[test]
fn stale_looks_for_unasked_steps_are_bounded_per_stretch_without_a_command() {
    for with_questions in [false, true] {
        let collect = collect_tap();
        let t = drive(
            config(),
            shop(0),
            shop,
            |i| {
                if with_questions && i % 3 == 2 {
                    return (
                        300,
                        r#"{"action":"ask_user","question":"Which colour?"}"#.to_owned(),
                    );
                }
                (2_500, collect.clone())
            },
            VecDeque::new(),
        );
        let stale_guard = t
            .looks
            .iter()
            .filter(|(_, before, _)| *before == Before::Other)
            .count();
        assert_eq!(
            stale_guard, MAX_STALE_LOOKS as usize,
            "questions {with_questions}: {:?}",
            t.looks
        );
        assert_eq!(t.fresh_looks, MAX_STALE_LOOKS);
        assert!(
            !t.effects.iter().any(|e| matches!(e, Effect::Act(_))),
            "nothing is sent on a screen that keeps changing"
        );
        assert_eq!(
            t.status,
            OutcomeStatus::Failed,
            "questions {with_questions}"
        );
        let summary = match t.effects.last() {
            Some(Effect::Finished(o)) => o.summary.clone(),
            other => panic!("{other:?}"),
        };
        assert!(summary.contains("kept changing"), "{summary}");
    }
}

/// A sent command starts a new stretch: a stale tap that holds is sent after its fresh look,
/// and the gate decided it on the fresh screen (unchanged expectation of `stale_screen.rs`).
#[test]
fn an_unasked_stale_step_still_gets_its_fresh_look_and_is_sent() {
    let settings = format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&shop(0), "Settings")
    );
    let mut n = 0;
    let t = drive(
        config(),
        shop(0),
        shop,
        move |_| {
            n += 1;
            match n {
                1 => (2_500, settings.clone()),
                _ => (200, r#"{"action":"done","summary":"ok"}"#.to_owned()),
            }
        },
        VecDeque::new(),
    );
    assert_eq!(t.fresh_looks, 1);
    let first_act = t
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Act(_)))
        .expect("the tap was sent");
    // Look, infer, the fresh look, the tap.
    assert!(matches!(t.effects[first_act - 1], Effect::Observe { .. }));
    assert_eq!(t.status, OutcomeStatus::Success);
}

/// F3: the vision fallback's screenshot is a look the session chooses. After reading
/// Messages, Shop's thin tree (one control) gets no screenshot: the model is asked on the
/// element list.
#[test]
fn the_vision_fallback_is_not_taken_in_the_restricted_state() {
    let link = link_tap();
    let t = drive(
        vision_config(),
        messages(),
        thin_shop,
        |i| match i {
            0 => (200, link.clone()),
            _ => (200, r#"{"action":"fail","reason":"x"}"#.to_owned()),
        },
        VecDeque::new(),
    );
    assert!(
        !t.looks.iter().any(|(a, _, shot)| a == SHOP && *shot),
        "{:?}",
        t.looks
    );
    // In an app that read nothing else the fallback is taken as before.
    let t = drive(
        vision_config(),
        thin_shop(0),
        thin_shop,
        |_| (200, r#"{"action":"fail","reason":"x"}"#.to_owned()),
        VecDeque::new(),
    );
    assert!(t.looks.iter().any(|(_, _, shot)| *shot), "{:?}", t.looks);
}

/// F3: after a decline the session decides on the screen in hand: no screenshot, even of a
/// screen that changed since the last one (was: the stale-screen guard's fresh look before
/// the question gave a new screen, and the decline's decision took its screenshot).
#[test]
fn the_vision_fallback_is_not_taken_after_a_decline() {
    let delete = format!(
        r#"{{"action":"tap","element":{}}}"#,
        index_of(&thin_shop(0), "Delete account")
    );
    let t = drive(
        vision_config(),
        thin_shop(0),
        thin_shop,
        |i| match i {
            0 => (200, r#"{"action":"wait","ms":1000}"#.to_owned()),
            1 => (2_500, delete.clone()),
            _ => (200, r#"{"action":"fail","reason":"x"}"#.to_owned()),
        },
        VecDeque::new(),
    );
    assert_eq!(t.confirms.len(), 1, "{:?}", t.confirms);
    let asked = t
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Confirm(_)))
        .unwrap();
    let next_infer = t.effects[asked..]
        .iter()
        .position(|e| matches!(e, Effect::Infer(_)))
        .unwrap()
        + asked;
    assert!(
        !t.effects[asked..next_infer]
            .iter()
            .any(|e| matches!(e, Effect::Observe { .. })),
        "{:?}",
        &t.effects[asked..next_infer]
    );
    // No look between the slow reply and its question either.
    let reply = t.effects[..asked]
        .iter()
        .rposition(|e| matches!(e, Effect::Infer(_)))
        .unwrap();
    assert!(!t.effects[reply..asked]
        .iter()
        .any(|e| matches!(e, Effect::Observe { .. })));
}

fn home_with_alert() -> Screen {
    scr(
        HOME,
        " ",
        vec![
            node(Role::Button, "Settings", rect(20.0, 100.0, 64.0, 64.0)),
            UiNode::new(Role::Alert, rect(40.0, 300.0, 313.0, 200.0)).with_children(vec![
                text("Software Update", rect(60.0, 320.0, 273.0, 30.0)),
                node(Role::Button, "Later", rect(60.0, 430.0, 273.0, 44.0)),
            ]),
        ],
    )
}

/// F4: after reading Messages, a wait and a look on the Home Screen ask nothing (SpringBoard
/// is the OS); a wait in a third-party app still asks; a tap on a Home Screen alert (a full
/// input into it) keeps its rule and asks, naming the Home Screen.
#[test]
fn the_home_screen_is_no_observe_kind_destination() {
    for step in [
        r#"{"action":"wait","ms":1000}"#,
        r#"{"action":"look"}"#,
        r#"{"action":"press_home"}"#,
    ] {
        let r = run(
            vision_config(),
            "Read my bank message",
            MemoryStore::new(),
            Phone::new(messages(), |cmd, _| match cmd {
                RunnerCommand::PressKey {
                    key: jarvis_proto::Key::Home,
                    ..
                } => Transition::Go(home_screen()),
                _ => Transition::Stay,
            }),
            Script::model(&[
                r#"{"action":"press_home"}"#,
                step,
                r#"{"action":"done","summary":"ok"}"#,
            ]),
        );
        let fq: Vec<&String> = r
            .confirms
            .iter()
            .map(|c| &c.summary)
            .filter(|s| s.contains(FLOW))
            .collect();
        assert!(fq.is_empty(), "{step}: {fq:?}");
        assert_eq!(r.outcome.status, OutcomeStatus::Success, "{step}");
    }

    // A wait in Shop after reading Messages asks, naming Shop.
    let link = link_tap();
    let t = drive(
        config(),
        messages(),
        shop,
        |i| match i {
            0 => (200, link.clone()),
            1 => (200, r#"{"action":"wait","ms":1000}"#.to_owned()),
            _ => (200, r#"{"action":"done","summary":"ok"}"#.to_owned()),
        },
        VecDeque::from(vec![true]),
    );
    assert_eq!(t.confirms.len(), 1, "{:?}", t.confirms);
    assert!(
        t.confirms[0].contains(FLOW) && t.confirms[0].contains("Shop"),
        "{}",
        t.confirms[0]
    );

    // A tap on a Home Screen alert after reading Messages: a full input into SpringBoard,
    // asked as before.
    let later = index_of(&home_with_alert(), "Later");
    let r = run(
        config(),
        "Read my bank message",
        MemoryStore::new(),
        Phone::new(messages(), |cmd, _| match cmd {
            RunnerCommand::PressKey {
                key: jarvis_proto::Key::Home,
                ..
            } => Transition::Go(home_with_alert()),
            _ => Transition::Stay,
        }),
        Script::model(&[
            r#"{"action":"press_home"}"#,
            &format!(r#"{{"action":"tap","element":{later}}}"#),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true, true]),
    );
    let fq: Vec<&String> = r
        .confirms
        .iter()
        .map(|c| &c.summary)
        .filter(|s| s.contains(FLOW))
        .collect();
    assert_eq!(fq.len(), 1, "{fq:?}");
    assert!(fq[0].contains("Messages"), "{}", fq[0]);
}
