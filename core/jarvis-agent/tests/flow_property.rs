#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review (Rule T), property test: random tasks over three apps, two sites in
//! Safari and a tool, with random owner decisions. Invariant, checked on every effect:
//! - no runner command that touches the screen in context Y (and no tool call, context
//!   `tool:lookup`) is emitted while the contexts the model has read in this task hold one,
//!   other than Y, that the owner has not allowed into Y in this task;
//! - an approval allows exactly the contexts read so far into that one place (per task, per
//!   source set: a context read later asks again);
//! - after a declined flow question nothing touches the screen or calls a tool before the
//!   model is asked again (or the task ends).
//!
//! `FLOW_CASES` sets the number of cases (default 400), `FLOW_SEED` runs one seed.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_observe::compact;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    AppInfo, Effect, Event, ModelOutput, Observation, ParamKind, RiskCategory, Role, RunnerCommand,
    RunnerResult, Screen, TaskSpec, Tier, TimedEvent, ToolParam, ToolResult, ToolSpec, UiNode,
};

const FLOW: &str = "what it read there could be passed on here";
const APPS: [&str; 3] = ["com.example.a", "com.example.b", "com.example.c"];
const SAFARI: &str = "com.apple.mobilesafari";
const HOSTS: [&str; 2] = ["one.example", "two.example"];
const TASK: &str = "Do it with hello on https://one.example/ and https://two.example/";

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The oracle's name of a context.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Ctx {
    App(String),
    Site(String),
    Tool,
}

fn body(kids_top: &str) -> Vec<UiNode> {
    vec![
        text(kids_top, rect(16.0, 140.0, 361.0, 30.0)),
        UiNode::new(Role::TextField, rect(16.0, 200.0, 361.0, 40.0))
            .with_label("Note")
            .hittable(true),
        node(Role::Button, "Next", rect(16.0, 300.0, 361.0, 44.0)),
    ]
}

fn app_screen(i: usize) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(APPS[i])
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(body(&format!("secret {i}")))]);
    screen(APPS[i], APPS[i], root, false)
}

fn site_screen(h: usize) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Safari")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![
                UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 650.0))
                    .with_children(body(&format!("page {h}"))),
                UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
                    .with_label("Address")
                    .with_value(HOSTS[h])
                    .with_identifier("TabBarItemTitle"),
            ])]);
    screen(SAFARI, "Safari", root, false)
}

fn ctx_of(s: &Screen) -> Ctx {
    if s.app_bundle_id == SAFARI {
        let h = if compact(s).elements.iter().any(|e| e.label == "page 0") {
            HOSTS[0]
        } else {
            HOSTS[1]
        };
        Ctx::Site(h.to_owned())
    } else {
        Ctx::App(s.app_bundle_id.clone())
    }
}

fn touches(c: &RunnerCommand) -> bool {
    matches!(
        c,
        RunnerCommand::Tap { .. }
            | RunnerCommand::LongPress { .. }
            | RunnerCommand::Swipe { .. }
            | RunnerCommand::TypeText { .. }
            | RunnerCommand::PressKey { .. }
    )
}

fn one_case(seed: u64) -> (u32, u32, u32) {
    let (mut asked, mut declined, mut sent) = (0u32, 0u32, 0u32);
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let next = compact(&app_screen(0))
        .elements
        .iter()
        .find(|e| e.label == "Next")
        .unwrap()
        .index;
    let field = compact(&app_screen(0))
        .elements
        .iter()
        .find(|e| e.label == "Note")
        .unwrap()
        .index;
    for s in [app_screen(1), site_screen(0), site_screen(1)] {
        let c = compact(&s);
        assert_eq!(
            c.elements.iter().find(|e| e.label == "Next").unwrap().index,
            next
        );
    }
    let steps = 3 + rng.below(10);
    let mut script: Vec<String> = Vec::new();
    for _ in 0..steps {
        script.push(match rng.below(7) {
            0 => format!(
                r#"{{"action":"open_app","app":"{}"}}"#,
                APPS[rng.below(3) as usize]
            ),
            1 => format!(
                r#"{{"action":"open_url","url":"https://{}/"}}"#,
                HOSTS[rng.below(2) as usize]
            ),
            2 | 3 => format!(r#"{{"action":"tap","element":{next}}}"#),
            4 => r#"{"action":"scroll","direction":"down"}"#.to_owned(),
            5 => format!(r#"{{"action":"type","element":{field},"text":"hello","submit":false}}"#),
            _ => r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#.to_owned(),
        });
    }
    script.push(r#"{"action":"done","summary":"ok"}"#.to_owned());
    let mut config = config();
    config.policy.max_consecutive_failures = 100;
    for a in APPS {
        config.known_apps.push(AppInfo::new(a, a));
    }
    config.known_apps.push(AppInfo::new(SAFARI, "Safari"));
    config.tools.push(ToolSpec {
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
    });
    let start = rng.below(5) as usize;
    let mut current = if start < 3 {
        app_screen(start)
    } else {
        site_screen(start - 3)
    };
    let mut session = Session::new(config, TaskSpec::new("p", TASK), MemoryStore::new());
    let mut model: std::collections::VecDeque<String> = script.into_iter().collect();
    // The oracle.
    let mut observed: BTreeSet<Ctx> = BTreeSet::new();
    let mut tool_results = false;
    let mut read: BTreeSet<Ctx> = BTreeSet::new();
    let mut allowed: BTreeMap<Ctx, BTreeSet<Ctx>> = BTreeMap::new();
    let mut declined_since_infer = false;
    let mut asked_target: Option<Ctx> = None;
    let mut now = 1_000u64;
    let mut event = Event::Start;
    let ok = |c: &Ctx, read: &BTreeSet<Ctx>, allowed: &BTreeMap<Ctx, BTreeSet<Ctx>>| {
        read.iter()
            .filter(|r| *r != c)
            .all(|r| allowed.get(c).is_some_and(|a| a.contains(r)))
    };
    for _ in 0..2_000 {
        let effect = session.step(TimedEvent::new(now, event));
        now += 50;
        event = match effect {
            Effect::Observe { .. } => {
                observed.insert(ctx_of(&current));
                Event::Observed(Observation {
                    screen: current.clone(),
                    screenshot: None,
                    device_locked: false,
                })
            }
            Effect::Infer(_) => {
                read.extend(observed.iter().cloned());
                if tool_results {
                    read.insert(Ctx::Tool);
                }
                declined_since_infer = false;
                match model.pop_front() {
                    Some(text) => Event::ModelOutput(ModelOutput { text }),
                    None => Event::ModelFailed {
                        error: "done".into(),
                    },
                }
            }
            Effect::Act(cmd) => {
                if touches(&cmd) {
                    sent += 1;
                    let y = ctx_of(&current);
                    assert!(
                        !declined_since_infer,
                        "seed {seed}: {cmd:?} sent after a declined flow question"
                    );
                    assert!(
                        ok(&y, &read, &allowed),
                        "seed {seed}: {cmd:?} into {y:?} while read {read:?}, allowed {allowed:?}"
                    );
                }
                match &cmd {
                    RunnerCommand::LaunchApp { bundle_id } => {
                        let i = APPS.iter().position(|a| a == bundle_id).unwrap();
                        current = app_screen(i);
                    }
                    RunnerCommand::OpenUrl { url } => {
                        current = site_screen(usize::from(url.contains("two")));
                    }
                    _ => {}
                }
                Event::ActResult(RunnerResult::ok())
            }
            Effect::CallTool(_) => {
                assert!(
                    !declined_since_infer,
                    "seed {seed}: tool call after a decline"
                );
                assert!(
                    ok(&Ctx::Tool, &read, &allowed),
                    "seed {seed}: tool call while read {read:?}, allowed {allowed:?}"
                );
                tool_results = true;
                Event::ToolResult(ToolResult {
                    ok: true,
                    output: "result".into(),
                    error: None,
                })
            }
            Effect::Confirm(req) => {
                let approve = rng.below(3) != 0;
                if req.summary.contains(FLOW) {
                    assert!(req.phone_only, "seed {seed}: {}", req.summary);
                    let y = if req.verb == jarvis_proto::ConfirmVerb::CallTool {
                        Ctx::Tool
                    } else {
                        ctx_of(&current)
                    };
                    // It is asked only when something read is not allowed in.
                    assert!(
                        !ok(&y, &read, &allowed),
                        "seed {seed}: needless ask {}",
                        req.summary
                    );
                    asked_target = Some(y.clone());
                    asked += 1;
                    declined += u32::from(!approve);
                    if approve {
                        let srcs: Vec<Ctx> = read.iter().filter(|r| **r != y).cloned().collect();
                        allowed.entry(y).or_default().extend(srcs);
                    } else {
                        declined_since_infer = true;
                    }
                }
                Event::UserDecision { approved: approve }
            }
            Effect::AskUser { .. } => Event::UserAnswer {
                text: String::new(),
            },
            Effect::AwaitUnlock { .. } => Event::DeviceUnlocked,
            Effect::Finished(_) => {
                let _ = asked_target;
                return (asked, declined, sent);
            }
        };
    }
    panic!("seed {seed}: did not finish");
}

#[test]
fn no_input_reaches_a_place_while_something_read_elsewhere_is_not_allowed_there() {
    if let Some(seed) = std::env::var("FLOW_SEED").ok().and_then(|s| s.parse().ok()) {
        one_case(seed);
        return;
    }
    let cases: u64 = std::env::var("FLOW_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(400);
    let (mut asked, mut declined, mut sent) = (0u32, 0u32, 0u32);
    for seed in 1..=cases {
        let (a, d, s) = one_case(seed);
        asked += a;
        declined += d;
        sent += s;
    }
    println!("{cases} tasks: {asked} flow questions ({declined} declined), {sent} touches sent");
    // The generator exercises the rule: questions are asked, declined and approved, and
    // touches go out.
    if cases >= 100 {
        assert!(
            asked > cases as u32 / 4 && declined > 0 && sent > cases as u32,
            "{asked} {declined} {sent}"
        );
    }
}
