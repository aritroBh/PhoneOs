#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review (Rule T), property test, extended by the seventeenth: random tasks over
//! three apps, an app whose screen holds an embedded web view (an ad: one-way), three sites
//! in Safari (two the owner named, one not; the second holds a frame: two-way), a tool with arguments that is not self-contained
//! and one without arguments that is, `press_home` and `wait`, with random owner decisions
//! and, in half the cases, `policy.confirm: []`. An independent oracle implements the
//! seventeenth review's sections 1–3 (what a place is, which outputs are input, what an
//! approval allows). Invariant, checked on every effect:
//! - no runner command and no tool call reaches a destination place while the read set holds
//!   a source for it the owner has not allowed into it in this task (sources: every place
//!   read but the destination itself — a tool that is not self-contained is a source for
//!   itself — and but the embedded content the destination surrounds);
//! - a flow question is asked only when such a source exists, names every unallowed source
//!   (no "and N more"), and its approval allows exactly the places it named;
//! - after a declined flow question nothing is sent before the model is asked again;
//! - `confirm: []` changes nothing about it.
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
    AppInfo, Effect, Event, Key, ModelOutput, Observation, ParamKind, Point, Rect, RiskCategory,
    Role, RunnerCommand, RunnerResult, Screen, TaskSpec, Tier, TimedEvent, ToolParam, ToolResult,
    ToolSpec, UiNode,
};

const FLOW: &str = "what it read there could be passed on here";
const APPS: [&str; 4] = [
    "com.example.a",
    "com.example.b",
    "com.example.c",
    "com.example.d",
];
/// The app whose screen holds an embedded web view.
const MIXED: usize = 3;
const SAFARI: &str = "com.apple.mobilesafari";
const HOSTS: [&str; 3] = ["one.example", "two.example", "three.example"];
const TASK: &str = "Do it with hello on https://one.example/ and https://two.example/";
const HOME: &str = "com.apple.springboard";

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

/// The oracle's name of a place.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Ctx {
    App(String),
    Site(String),
    /// Web content embedded in an app (the mixed app's ad).
    Web(String),
    /// A frame nested in a site's page (by host).
    Frame(String),
    Tool(String),
}

impl Ctx {
    /// How a question names it (apps are known under their bundle ids).
    fn name(&self) -> String {
        match self {
            Ctx::App(a) => a.clone(),
            Ctx::Site(h) => h.clone(),
            Ctx::Web(a) => format!("web content in {a}"),
            Ctx::Frame(h) => format!("embedded content in {h}"),
            Ctx::Tool(t) => format!("the tool {t}"),
        }
    }
}

/// The embedded web view's frame on the mixed app's screen.
fn ad_frame() -> Rect {
    rect(0.0, 600.0, 393.0, 150.0)
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
    let mut kids = body(&format!("secret {i}"));
    if i == MIXED {
        kids.push(
            UiNode::new(Role::WebView, ad_frame()).with_children(vec![node(
                Role::Button,
                "Ad",
                rect(16.0, 620.0, 361.0, 40.0),
            )]),
        );
    }
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(APPS[i])
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(kids)
        ]);
    screen(APPS[i], APPS[i], root, false)
}

/// The frame nested in site 1's page.
fn frame_rect() -> Rect {
    rect(0.0, 480.0, 393.0, 60.0)
}

/// The site with a frame.
const FRAMED: usize = 1;

fn site_screen(h: usize) -> Screen {
    let mut page = body(&format!("page {h}"));
    if h == FRAMED {
        page.push(
            UiNode::new(Role::WebView, frame_rect()).with_children(vec![node(
                Role::Button,
                "Frame",
                rect(16.0, 490.0, 361.0, 40.0),
            )]),
        );
    }
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label("Safari")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![
                UiNode::new(Role::WebView, rect(0.0, 100.0, 393.0, 450.0)).with_children(page),
                UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
                    .with_label("Address")
                    .with_value(HOSTS[h])
                    .with_identifier("TabBarItemTitle"),
            ])]);
    screen(SAFARI, "Safari", root, false)
}

fn home_screen() -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(" ")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![node(
                Role::Button,
                "Mail",
                rect(20.0, 120.0, 72.0, 72.0),
            )
            .with_identifier("Mail")])]);
    screen(HOME, " ", root, false)
}

/// The places a screen shows (what reading it reads), the base place first; none for the
/// icons-only home screen.
fn places_of(s: &Screen) -> Vec<Ctx> {
    if s.app_bundle_id == HOME {
        return Vec::new();
    }
    if s.app_bundle_id == SAFARI {
        let c = compact(s);
        let h = (0..3)
            .find(|h| c.elements.iter().any(|e| e.label == format!("page {h}")))
            .unwrap();
        let mut out = vec![Ctx::Site(HOSTS[h].to_owned())];
        if h == FRAMED {
            out.push(Ctx::Frame(HOSTS[h].to_owned()));
        }
        return out;
    }
    let mut out = vec![Ctx::App(s.app_bundle_id.clone())];
    if s.app_bundle_id == APPS[MIXED] {
        out.push(Ctx::Web(s.app_bundle_id.clone()));
    }
    out
}

/// The place under a point of `s` (the oracle's own geometry: the mixed app's ad frame).
fn place_at(s: &Screen, p: Point) -> Ctx {
    let places = places_of(s);
    if s.app_bundle_id == APPS[MIXED] && ad_frame().contains(p) {
        return places[1].clone();
    }
    if places.len() == 2 && s.app_bundle_id == SAFARI && frame_rect().contains(p) {
        return places[1].clone();
    }
    places
        .first()
        .cloned()
        .unwrap_or_else(|| Ctx::App(s.app_bundle_id.clone()))
}

fn host_of(url: &str) -> usize {
    HOSTS.iter().position(|h| url.contains(h)).unwrap()
}

/// The destination of a runner command or tool call (sections 1 and 2), or `None` for one
/// that is no input (a wait).
fn dest_of(cmd: &RunnerCommand, current: &Screen) -> Option<Ctx> {
    match cmd {
        RunnerCommand::Tap { x, y, .. } | RunnerCommand::LongPress { x, y, .. } => {
            Some(place_at(current, Point::new(*x, *y)))
        }
        RunnerCommand::Swipe { from, .. } => Some(place_at(current, *from)),
        // The only field is native; Return and Home go to the place in front.
        RunnerCommand::TypeText { .. } | RunnerCommand::PressKey { .. } => Some(
            places_of(current)
                .first()
                .cloned()
                .unwrap_or_else(|| Ctx::App(current.app_bundle_id.clone())),
        ),
        RunnerCommand::LaunchApp { bundle_id } => Some(Ctx::App(bundle_id.clone())),
        RunnerCommand::OpenUrl { url } => Some(Ctx::Site(HOSTS[host_of(url)].to_owned())),
        RunnerCommand::Wait { .. } => None,
    }
}

/// The destinations of the model's action `json` on `current` (for the question it causes):
/// one place, but for a scroll the screen's own place and the place under its start point
/// (what is under a point may be either: the session asks for both, fail closed).
fn dest_of_action(json: &str, current: &Screen, next: u32, ad: u32) -> Vec<Ctx> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    if v["action"].as_str() == Some("scroll") {
        let base = places_of(current)
            .first()
            .cloned()
            .unwrap_or_else(|| Ctx::App(current.app_bundle_id.clone()));
        let g = jarvis_safety::gestures::drag_gesture(
            jarvis_safety::gestures::GestureKind::Scroll,
            jarvis_proto::Direction::Down,
            None,
            &compact(current),
        )
        .unwrap();
        let mut out = vec![base];
        let under = place_at(current, g.from);
        if !out.contains(&under) {
            out.push(under);
        }
        return out;
    }
    dest_one(json, current, next, ad).into_iter().collect()
}

/// [`dest_of_action`] for an action with one destination.
fn dest_one(json: &str, current: &Screen, next: u32, ad: u32) -> Option<Ctx> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    let base = || {
        places_of(current)
            .first()
            .cloned()
            .unwrap_or_else(|| Ctx::App(current.app_bundle_id.clone()))
    };
    match v["action"].as_str().unwrap() {
        "open_app" => Some(Ctx::App(v["app"].as_str().unwrap().to_owned())),
        "open_url" => Some(Ctx::Site(
            HOSTS[host_of(v["url"].as_str().unwrap())].to_owned(),
        )),
        "call_tool" => Some(Ctx::Tool(v["name"].as_str().unwrap().to_owned())),
        "tap" => {
            let i = v["element"].as_u64().unwrap() as u32;
            let el = compact(current)
                .elements
                .into_iter()
                .find(|e| e.index == i)?;
            if i == ad || i == next || current.app_bundle_id != APPS[MIXED] {
                Some(place_at(current, el.frame.center()))
            } else {
                Some(base())
            }
        }
        "wait" | "done" => None,
        _ => Some(base()),
    }
}

/// The places a flow note names, in order.
fn named(summary: &str) -> Vec<String> {
    let from = summary.find("Jarvis has read ").unwrap() + "Jarvis has read ".len();
    let to = summary[from..].find(" in this task;").unwrap() + from;
    let list = &summary[from..to];
    let (head, last) = match list.rsplit_once(" and ") {
        Some((h, l)) => (h, Some(l)),
        None => (list, None),
    };
    let mut out: Vec<String> = head.split(", ").map(str::to_owned).collect();
    out.extend(last.map(str::to_owned));
    out
}

/// The oracle's record of one task.
#[derive(Default)]
struct Oracle {
    read: BTreeSet<Ctx>,
    allowed: BTreeMap<Ctx, BTreeSet<Ctx>>,
}

impl Oracle {
    /// The sources that may not reach `dest` (section 1: a place is not a source for itself,
    /// except a tool that is not self-contained; the embedded content an app surrounds is
    /// not a source for it).
    fn unallowed(&self, dest: &Ctx) -> BTreeSet<Ctx> {
        self.read
            .iter()
            .filter(|r| {
                let itself = *r == dest && !matches!(dest, Ctx::Tool(t) if t == "lookup");
                // One-way between an app and the web content it hosts only; a frame and its
                // page are sources for each other.
                let surrounded = matches!((dest, r), (Ctx::App(a), Ctx::Web(w)) if a == w);
                !itself && !surrounded && !self.allowed.get(dest).is_some_and(|a| a.contains(*r))
            })
            .cloned()
            .collect()
    }
}

fn one_case(seed: u64) -> (u32, u32, u32) {
    let (mut asked, mut declined, mut sent) = (0u32, 0u32, 0u32);
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let index = |s: &Screen, l: &str| {
        compact(s)
            .elements
            .iter()
            .find(|e| e.label == l)
            .unwrap()
            .index
    };
    let next = index(&app_screen(0), "Next");
    let field = index(&app_screen(0), "Note");
    let ad = index(&app_screen(MIXED), "Ad");
    // "tap the ad" taps the frame on the framed site.
    assert_eq!(index(&site_screen(FRAMED), "Frame"), ad);
    for s in [
        app_screen(1),
        app_screen(MIXED),
        site_screen(0),
        site_screen(2),
    ] {
        assert_eq!(index(&s, "Next"), next);
        assert_eq!(index(&s, "Note"), field);
    }
    let steps = 3 + rng.below(12);
    let mut script: Vec<String> = Vec::new();
    for _ in 0..steps {
        script.push(match rng.below(12) {
            0 | 1 => format!(
                r#"{{"action":"open_app","app":"{}"}}"#,
                APPS[rng.below(4) as usize]
            ),
            2 => format!(
                r#"{{"action":"open_url","url":"https://{}/"}}"#,
                HOSTS[rng.below(3) as usize]
            ),
            3 | 4 => format!(r#"{{"action":"tap","element":{next}}}"#),
            5 => format!(r#"{{"action":"tap","element":{ad}}}"#),
            6 => r#"{"action":"scroll","direction":"down"}"#.to_owned(),
            7 => format!(r#"{{"action":"type","element":{field},"text":"hello","submit":false}}"#),
            8 => r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#.to_owned(),
            9 => r#"{"action":"call_tool","name":"clock","args":{}}"#.to_owned(),
            10 => r#"{"action":"wait","ms":300}"#.to_owned(),
            _ => r#"{"action":"press_home"}"#.to_owned(),
        });
    }
    script.push(r#"{"action":"done","summary":"ok"}"#.to_owned());
    let mut config = config();
    config.policy.max_consecutive_failures = 100;
    config.policy.max_steps = 100;
    let confirm_nothing = rng.below(2) == 0;
    if confirm_nothing {
        config.policy.confirm = Vec::new();
    }
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
        self_contained: false,
    });
    config.tools.push(ToolSpec {
        name: "clock".into(),
        description: "The time".into(),
        params: Vec::new(),
        tier: Tier::Native,
        risk: RiskCategory::None,
        app: None,
        self_contained: true,
    });
    let start = rng.below(7) as usize;
    let mut current = if start < 4 {
        app_screen(start)
    } else {
        site_screen(start - 4)
    };
    let mut session = Session::new(config, TaskSpec::new("p", TASK), MemoryStore::new());
    let mut model: std::collections::VecDeque<String> = script.into_iter().collect();
    let mut oracle = Oracle::default();
    let mut observed: BTreeSet<Ctx> = BTreeSet::new();
    let mut declined_since_infer = false;
    let mut last_action = String::new();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for _ in 0..4_000 {
        let effect = session.step(TimedEvent::new(now, event));
        now += 50;
        event = match effect {
            Effect::Observe { .. } => {
                observed.extend(places_of(&current));
                Event::Observed(Observation {
                    screen: current.clone(),
                    screenshot: None,
                    device_locked: false,
                })
            }
            Effect::Infer(_) => {
                oracle.read.append(&mut observed);
                declined_since_infer = false;
                match model.pop_front() {
                    Some(text) => {
                        last_action = text.clone();
                        Event::ModelOutput(ModelOutput { text })
                    }
                    None => Event::ModelFailed {
                        error: "done".into(),
                    },
                }
            }
            Effect::Act(cmd) => {
                if let Some(dest) = dest_of(&cmd, &current) {
                    sent += 1;
                    assert!(
                        !declined_since_infer,
                        "seed {seed}: {cmd:?} sent after a declined flow question"
                    );
                    let bad = oracle.unallowed(&dest);
                    assert!(
                        bad.is_empty(),
                        "seed {seed} (confirm empty: {confirm_nothing}): {cmd:?} into {dest:?} while {bad:?} not allowed (read {:?}, allowed {:?})",
                        oracle.read,
                        oracle.allowed
                    );
                }
                match &cmd {
                    RunnerCommand::LaunchApp { bundle_id } => {
                        let i = APPS.iter().position(|a| a == bundle_id).unwrap();
                        current = app_screen(i);
                    }
                    RunnerCommand::OpenUrl { url } => current = site_screen(host_of(url)),
                    RunnerCommand::PressKey { key: Key::Home, .. } => current = home_screen(),
                    _ => {}
                }
                Event::ActResult(RunnerResult::ok())
            }
            Effect::CallTool(call) => {
                let dest = Ctx::Tool(call.name.clone());
                assert!(
                    !declined_since_infer,
                    "seed {seed}: tool call after a decline"
                );
                let bad = oracle.unallowed(&dest);
                assert!(
                    bad.is_empty(),
                    "seed {seed}: tool call {} while {bad:?} not allowed",
                    call.name
                );
                observed.insert(dest);
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
                    let dests = dest_of_action(&last_action, &current, next, ad);
                    assert!(
                        !dests.is_empty(),
                        "seed {seed}: a flow question for {last_action}"
                    );
                    let bad: BTreeSet<Ctx> =
                        dests.iter().flat_map(|d| oracle.unallowed(d)).collect();
                    let dest = &dests;
                    // Asked only when a source may not reach it; every one named, no "more".
                    assert!(
                        !bad.is_empty(),
                        "seed {seed}: needless ask {} ({last_action} into {dest:?})",
                        req.summary
                    );
                    assert!(!req.summary.contains(" more "), "{}", req.summary);
                    let names: BTreeSet<String> = named(&req.summary).into_iter().collect();
                    let expected: BTreeSet<String> = bad.iter().map(Ctx::name).collect();
                    assert_eq!(
                        names, expected,
                        "seed {seed}: {} ({last_action} into {dest:?})",
                        req.summary
                    );
                    asked += 1;
                    declined += u32::from(!approve);
                    if approve {
                        // Exactly the places named.
                        let srcs: Vec<Ctx> = bad
                            .into_iter()
                            .filter(|c| names.contains(&c.name()))
                            .collect();
                        for d in dests.iter().cloned() {
                            oracle
                                .allowed
                                .entry(d)
                                .or_default()
                                .extend(srcs.iter().cloned());
                        }
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
            Effect::Finished(_) => return (asked, declined, sent),
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
    println!("{cases} tasks: {asked} flow questions ({declined} declined), {sent} inputs sent");
    // The generator exercises the rule: questions are asked, declined and approved, and
    // inputs go out.
    if cases >= 100 {
        assert!(
            asked > cases as u32 / 4 && declined > 0 && sent > cases as u32,
            "{asked} {declined} {sent}"
        );
    }
}
