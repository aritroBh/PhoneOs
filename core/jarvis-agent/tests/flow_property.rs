#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
//! Sixteenth review (Rule T), property test, extended by the seventeenth and eighteenth:
//! random tasks over plain apps; a third-party app whose screen holds an embedded web view
//! (an ad: two-way since the eighteenth review) and an Apple app holding one (one-way); a
//! third-party app that draws a fake address bar (not a browser: no site, its web view is
//! "web content in <App>"); a third-party browser (Firefox: its chrome names the site, a
//! place apart from the same host in Safari, named `<host> in Firefox`); three sites in
//! Safari (two the owner named, one not; the second holds a frame: two-way; the third two
//! sibling frames: an ambiguous class, a source for itself, named as covering all of it); a
//! tool with arguments that is not self-contained and one without arguments that is;
//! `look` with a vision model in half the cases (the screen picture is a source for every
//! place); `press_home` onto an icons-only home screen or, in half the cases, onto a
//! Spotlight screen with a banner and a "Website" row (a launch Jarvis cannot name: every
//! place read, the Home Screen included, is a source; nothing is kept beyond the action);
//! `wait`; random owner decisions and, in half the cases, `policy.confirm: []`. An
//! independent oracle implements the threat model's sections 1–3 (what a place is, which
//! outputs are input, what an approval allows).
//!
//! Nineteenth review: no one-way relation (the Apple app and its web content are two-way
//! like any other); an app's web content covers all of itself from its first observation (a
//! source for itself, named `all … (Jarvis cannot tell its parts apart)`); a browser's chrome
//! (its address field, a "Tabs" button) is the browser app's place, not the site's — the
//! "Tabs" button makes it a read, and "tap the ad" on the first site taps the address field
//! (an input into the browser); `press_home` goes to every place on the screen it leaves; the
//! Spotlight screen also lists a website row titled like a known app (`<app>, Website,
//! help.example`: not that app, an undecidable launch).
//!
//! Twentieth review (R20-A): the oracle knows the kinds of input. A question shown for a
//! scroll keeps, when approved, a scroll-only grant for each pair it named (a later scroll
//! does not ask, any other input does); a full input with one decidable destination keeps a
//! full grant; `press_home`, an undecidable destination and any other input with more than
//! one destination keep nothing beyond the action. A question for an input with more than
//! one destination names every destination (`, into A and B`).
//!
//! Twenty-first review (R21-A): the kind is decided on the screen the action acts on. The
//! oracle's own model of it: a scroll is of the scroll kind only when it names no element
//! (the generator's elements are never scroll containers), its path can be computed, it does
//! not start on an enabled button, link, slider, switch or picker, and it crosses no picker or
//! vertical slider (the generator's `com.example.c` holds one on the screen scroll's path; a
//! mixed app's "Ad" lies under the screen scroll's start point). Everything else — a scroll
//! on "Next" included — is full: only a full grant covers it, and its approval keeps what any
//! full input keeps.
//!
//! Twenty-third review (R23): every model-chosen step is seen by every place on the screen
//! when it is decided. The oracle's kinds are observe < scroll < full: `wait`, `look` and
//! `press_home` are observe-kind inputs into every place on the screen; every other step
//! keeps its own destinations and kind and is also an observe-kind input into every other
//! place on the screen. Grants are kept per (destination, source) pair with their kind (an
//! observe question's approval keeps an observe-only grant; a grant covers its kind and the
//! weaker ones, never a stronger one). A question names the action's own destinations as
//! before, and the places that only see it when they need approval (`, to …, which can tell
//! when Jarvis waits, looks or acts`).
//!
//! Invariant, checked on every effect:
//! - no runner command and no tool call reaches a destination place while the read set holds
//!   a source for it the owner has not allowed into it in this task (sources: every place
//!   read but the destination itself — a tool that is not self-contained and a covering
//!   embedded class are sources for themselves);
//! - a flow question is asked only when such a source exists, names every unallowed source
//!   (no "and N more"; an ambiguous class by its covering name), and its approval allows
//!   exactly the places it named (into an undecidable destination: for that action only);
//! - after a declined flow question nothing is sent before the model is asked again;
//! - (twenty-first review, F2b) no runner command directly follows an approved question:
//!   a look (an observation) always comes in between, on every path (partial approvals and
//!   a Return decided again included);
//! - `confirm: []` changes nothing about it;
//! - (twenty-third review) no runner command, tool call or observation comes between a
//!   model-chosen step and its flow question, and none after a declined one before the model
//!   is asked again; a look's screenshot and a model wait are checked like any input (observe
//!   kind, into every place on the screen).
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
    AppInfo, Effect, Event, ImageRef, Key, ModelOutput, Observation, ParamKind, Point, Rect,
    RiskCategory, Role, RunnerCommand, RunnerResult, Screen, TaskSpec, Tier, TimedEvent, ToolParam,
    ToolResult, ToolSpec, UiNode,
};
use jarvis_safety::{COVERS_ALL, COVERS_ALL_PREFIX, HOME_SCREEN_READ, SCREEN_PICTURE};

const FLOW: &str = "what it read there could be passed on here";
const APPS: [&str; 7] = [
    "com.example.a",
    "com.example.b",
    "com.example.c",
    "com.example.d",
    "com.apple.news",
    "com.example.evil",
    "org.mozilla.ios.Firefox",
];
/// The third-party app whose screen holds an embedded web view (two-way).
const MIXED: usize = 3;
/// The Apple app whose screen holds an embedded web view (two-way since the nineteenth
/// review).
const APPLE_MIXED: usize = 4;
/// Twenty-first review (R21-A): the plain app whose screen holds a vertical slider on the
/// screen scroll's path (a screen scroll there works the slider: a full input).
const SLIDER: usize = 2;
/// The third-party app that draws a fake address bar beside its web view.
const EVIL: usize = 5;
/// A listed third-party browser.
const FIREFOX: usize = 6;
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
    /// A site in Firefox (by host).
    FxSite(String),
    /// Web content embedded in an app (an ad).
    Web(String),
    /// Content nested in a site's page (by host).
    Frame(String),
    Tool(String),
    /// The screen picture given to the model.
    Pixels,
    /// A destination Jarvis cannot name (a Spotlight row).
    Unknown,
}

impl Ctx {
    /// How a question names it (apps are known under their bundle ids).
    fn name(&self, ambiguous: &BTreeSet<Ctx>) -> String {
        let base = match self {
            Ctx::App(a) if a == HOME => "the Home Screen".to_owned(),
            Ctx::App(a) if a == SAFARI => "Safari".to_owned(),
            Ctx::App(a) => a.clone(),
            Ctx::Site(h) => h.clone(),
            Ctx::FxSite(h) => format!("{h} in {}", APPS[FIREFOX]),
            Ctx::Web(a) => format!("web content in {a}"),
            Ctx::Frame(h) => format!("embedded content in {h}"),
            Ctx::Tool(t) => format!("the tool {t}"),
            Ctx::Pixels => SCREEN_PICTURE.to_owned(),
            Ctx::Unknown => "a place Jarvis cannot name".to_owned(),
        };
        if ambiguous.contains(self) || matches!(self, Ctx::Web(_)) {
            format!("{COVERS_ALL_PREFIX}{base}{COVERS_ALL}")
        } else {
            base
        }
    }

    /// Whether it is a source for input into itself (section 1): a tool that is not
    /// self-contained, an app's web content (from its first observation), a site's frames
    /// once they held two documents.
    fn self_source(&self, ambiguous: &BTreeSet<Ctx>) -> bool {
        matches!(self, Ctx::Tool(t) if t == "lookup")
            || matches!(self, Ctx::Web(_))
            || ambiguous.contains(self)
    }
}

/// The embedded web view's frame on the mixed apps' screens.
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

fn address(host: &str) -> UiNode {
    UiNode::new(Role::TextField, rect(100.0, 800.0, 200.0, 30.0))
        .with_label("Address")
        .with_value(host)
        .with_identifier("TabBarItemTitle")
}

fn app_screen(i: usize) -> Screen {
    if i == FIREFOX {
        return site_screen_in(APPS[FIREFOX], 0);
    }
    let mut kids = body(&format!("secret {i}"));
    if matches!(i, MIXED | APPLE_MIXED | EVIL) {
        kids.push(
            UiNode::new(Role::WebView, ad_frame()).with_children(vec![node(
                Role::Button,
                "Ad",
                rect(16.0, 620.0, 361.0, 40.0),
            )]),
        );
    }
    if i == SLIDER {
        // Below "Next" in reading order (element numbers unchanged), across the screen
        // scroll's path.
        kids.push(
            UiNode::new(Role::Slider, rect(180.0, 380.0, 33.0, 200.0))
                .with_label("Level")
                .with_value("50%")
                .hittable(true),
        );
    }
    if i == EVIL {
        // Not a browser: the "host" it draws names nothing.
        kids.push(address("one.example"));
    }
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(APPS[i])
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(kids)
        ]);
    screen(APPS[i], APPS[i], root, false)
}

/// The frame nested in sites 1 and 2.
fn frame_rect() -> Rect {
    rect(0.0, 480.0, 393.0, 60.0)
}

/// The second frame of site 2.
fn frame_b_rect() -> Rect {
    rect(0.0, 545.0, 393.0, 50.0)
}

/// The site with a frame.
const FRAMED: usize = 1;
/// The site with two sibling frames (an ambiguous class).
const TWO_FRAMES: usize = 2;

fn site_screen(h: usize) -> Screen {
    site_screen_in(SAFARI, h)
}

fn site_screen_in(app: &str, h: usize) -> Screen {
    let mut page = body(&format!("page {h}"));
    if h == FRAMED || h == TWO_FRAMES {
        page.push(
            UiNode::new(Role::WebView, frame_rect()).with_children(vec![node(
                Role::Button,
                "Frame",
                rect(16.0, 490.0, 361.0, 40.0),
            )]),
        );
    }
    if h == TWO_FRAMES {
        page.push(
            UiNode::new(Role::WebView, frame_b_rect()).with_children(vec![text(
                "Continue as Jane",
                rect(16.0, 550.0, 361.0, 30.0),
            )]),
        );
    }
    let name = if app == SAFARI { "Safari" } else { app };
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![
                UiNode::new(Role::WebView, page_rect()).with_children(page),
                address(HOSTS[h]),
                // Nineteenth review: the browser's own chrome content (the browser's place).
                node(Role::Button, "Tabs", rect(320.0, 800.0, 60.0, 30.0)),
            ])]);
    screen(app, name, root, false)
}

/// The page's web view on the browsers' screens.
fn page_rect() -> Rect {
    rect(0.0, 100.0, 393.0, 500.0)
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

/// The Spotlight row that opens a website.
fn row_rect() -> Rect {
    rect(0.0, 290.0, 393.0, 60.0)
}

/// SpringBoard with a banner, Spotlight's field and a "Website" row: a read.
fn spotlight_screen() -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(" ")
        .with_children(vec![UiNode::new(Role::Window, IPHONE_SIZE.to_rect())
            .with_children(vec![
                UiNode::new(
                    Role::Other("NotificationBanner".to_owned()),
                    rect(8.0, 50.0, 377.0, 80.0),
                )
                .with_label("Messages, now, Your code is 482913"),
                UiNode::new(Role::SearchField, rect(16.0, 200.0, 361.0, 40.0))
                    .with_label("Search")
                    .hittable(true),
                UiNode::new(Role::Cell, row_rect())
                    .with_label("evil.example/482913, Website")
                    .hittable(true),
                // Nineteenth review (R-E): a website row titled like a known app.
                UiNode::new(Role::Cell, row2_rect())
                    .with_label(format!("{}, Website, help.example", APPS[0]))
                    .hittable(true),
            ])]);
    screen(HOME, " ", root, false)
}

/// The Spotlight row a site titled like a known app.
fn row2_rect() -> Rect {
    rect(0.0, 360.0, 393.0, 60.0)
}

fn is_spotlight(s: &Screen) -> bool {
    s.app_bundle_id == HOME && compact(s).elements.iter().any(|e| e.label == "Search")
}

/// The places a screen shows (what reading it reads), the base place first; none for the
/// icons-only home screen.
fn places_of(s: &Screen) -> Vec<Ctx> {
    if s.app_bundle_id == HOME {
        return if is_spotlight(s) {
            vec![Ctx::App(HOME.to_owned())]
        } else {
            Vec::new()
        };
    }
    let page = || {
        let c = compact(s);
        (0..3)
            .find(|h| c.elements.iter().any(|e| e.label == format!("page {h}")))
            .unwrap()
    };
    if is_browser_screen(s) {
        let h = page();
        // The chrome lists "Tabs" (content of the browser's own place), then the page.
        let mut out = vec![base_of(s), site_ctx(s, h)];
        if s.app_bundle_id == SAFARI && (h == FRAMED || h == TWO_FRAMES) {
            out.push(Ctx::Frame(HOSTS[h].to_owned()));
        }
        return out;
    }
    let mut out = vec![Ctx::App(s.app_bundle_id.clone())];
    if is_mixed(s) {
        out.push(Ctx::Web(s.app_bundle_id.clone()));
    }
    out
}

/// Every place on a screen, shown content or not (what leaving it leaves).
fn every_of(s: &Screen) -> Vec<Ctx> {
    let mut out = places_of(s);
    if out.is_empty() {
        out.push(base_of(s));
    }
    out
}

fn is_browser_screen(s: &Screen) -> bool {
    s.app_bundle_id == SAFARI || s.app_bundle_id == APPS[FIREFOX]
}

fn is_mixed(s: &Screen) -> bool {
    [APPS[MIXED], APPS[APPLE_MIXED], APPS[EVIL]].contains(&s.app_bundle_id.as_str())
}

/// The site of page `h` in the browser of `s`.
fn site_ctx(s: &Screen, h: usize) -> Ctx {
    if s.app_bundle_id == SAFARI {
        Ctx::Site(HOSTS[h].to_owned())
    } else {
        Ctx::FxSite(HOSTS[h].to_owned())
    }
}

/// The place under a point of `s` (the oracle's own geometry): in a browser the page's web
/// view is the site (its frames their class), everything else the browser.
fn place_at(s: &Screen, p: Point) -> Ctx {
    let places = places_of(s);
    if is_spotlight(s) && (row_rect().contains(p) || row2_rect().contains(p)) {
        return Ctx::Unknown;
    }
    if is_mixed(s) && ad_frame().contains(p) {
        return places[1].clone();
    }
    if is_browser_screen(s) && page_rect().contains(p) {
        if let Some(f) = places.get(2) {
            if frame_rect().contains(p)
                || (*f == Ctx::Frame(HOSTS[TWO_FRAMES].to_owned()) && frame_b_rect().contains(p))
            {
                return f.clone();
            }
        }
        return places[1].clone();
    }
    base_of(s)
}

/// The place of everything outside every web view: the app (in a browser, its chrome).
fn base_of(s: &Screen) -> Ctx {
    Ctx::App(s.app_bundle_id.clone())
}

/// The place of the "Note" field (the only text field the generator types into).
fn field_place(s: &Screen) -> Ctx {
    place_at(s, Point::new(196.5, 220.0))
}

/// The action name of the model's step `json`.
fn kind_of_last(json: &str) -> Option<&'static str> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    match v["action"].as_str()? {
        "look" => Some("look"),
        "wait" => Some("wait"),
        _ => None,
    }
}

fn host_of(url: &str) -> usize {
    HOSTS.iter().position(|h| url.contains(h)).unwrap()
}

/// The destinations of a runner command (sections 1 and 2); none for one that is no input
/// (a wait).
fn dest_of(cmd: &RunnerCommand, current: &Screen) -> Vec<Ctx> {
    match cmd {
        RunnerCommand::Tap { x, y, .. } | RunnerCommand::LongPress { x, y, .. } => {
            vec![place_at(current, Point::new(*x, *y))]
        }
        // What is under the start point, and the screen's own place (fail closed).
        RunnerCommand::Swipe { from, .. } => {
            dedup(vec![base_of(current), place_at(current, *from)])
        }
        // Home goes to every place of the screen it leaves (nineteenth review).
        RunnerCommand::PressKey { key: Key::Home, .. } => every_of(current),
        // The only field is "Note"; Return goes where the typing went.
        RunnerCommand::TypeText { .. } | RunnerCommand::PressKey { .. } => {
            vec![field_place(current)]
        }
        RunnerCommand::LaunchApp { bundle_id } => vec![Ctx::App(bundle_id.clone())],
        RunnerCommand::OpenUrl { url } => vec![Ctx::Site(HOSTS[host_of(url)].to_owned())],
        RunnerCommand::Wait { .. } => Vec::new(),
    }
}

fn dedup(v: Vec<Ctx>) -> Vec<Ctx> {
    let mut out: Vec<Ctx> = Vec::new();
    for c in v {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The destinations of the model's action `json` on `current` (for the question it causes):
/// one place, but for a scroll the screen's own place and the place under its start point
/// (what is under a point may be either: the session asks for both, fail closed).
fn dest_of_action(json: &str, current: &Screen) -> Vec<Ctx> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    if v["action"].as_str() == Some("scroll") {
        let g = scroll_path(&v, current).unwrap();
        return dedup(vec![base_of(current), place_at(current, g.from)]);
    }
    if matches!(v["action"].as_str(), Some("press_home" | "wait" | "look")) {
        return every_of(current);
    }
    dest_one(json, current).into_iter().collect()
}

/// The drag a generated scroll (`down`, on its element if it names one) makes on `current`.
fn scroll_path(
    v: &serde_json::Value,
    current: &Screen,
) -> Option<jarvis_safety::gestures::Gesture> {
    let c = compact(current);
    let frame = v["element"].as_u64().map(|i| {
        c.elements
            .iter()
            .find(|e| u64::from(e.index) == i)
            .unwrap()
            .frame
    });
    jarvis_safety::gestures::drag_gesture(
        jarvis_safety::gestures::GestureKind::Scroll,
        jarvis_proto::Direction::Down,
        frame,
        &c,
    )
    .ok()
}

/// Twenty-first review (R21-A), the oracle's own model: whether the action `json` is of the
/// scroll kind on `current` — a scroll naming no element, with a path, that starts on no
/// enabled button, link, slider, switch or picker and crosses no picker or vertical slider
/// (the generator's scroll goes down: the finger moves up).
fn scroll_kind(json: &str, current: &Screen) -> bool {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    if v["action"].as_str() != Some("scroll") || !v["element"].is_null() {
        return false;
    }
    let Some(g) = scroll_path(&v, current) else {
        return false;
    };
    let (y0, y1) = (g.from.y.min(g.to.y), g.from.y.max(g.to.y));
    !compact(current).elements.iter().any(|e| {
        let f = e.frame;
        let control = matches!(
            e.role,
            Role::Button | Role::Link | Role::Slider | Role::Switch | Role::Picker
        );
        let reacts = e.role == Role::Picker || (e.role == Role::Slider && f.height > f.width);
        let crosses = f.x <= g.from.x && f.max_x() >= g.from.x && f.y <= y1 && f.max_y() >= y0;
        e.enabled && ((control && f.contains(g.from)) || (reacts && crosses))
    })
}

/// [`dest_of_action`] for an action with one destination.
fn dest_one(json: &str, current: &Screen) -> Option<Ctx> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
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
            Some(place_at(current, el.frame.center()))
        }
        "type" => Some(field_place(current)),
        "wait" | "done" | "look" => None,
        _ => Some(base_of(current)),
    }
}

/// The places a flow note names, in order (the names Jarvis composes itself, which hold
/// `, ` and ` and `, are read whole).
fn named(summary: &str) -> Vec<String> {
    let from = summary.find("Jarvis has read ").unwrap() + "Jarvis has read ".len();
    let to = summary[from..].find(" in this task;").unwrap() + from;
    let list = summary[from..to]
        .replace(SCREEN_PICTURE, "\u{1}")
        .replace(COVERS_ALL, "\u{2}");
    let (head, last) = match list.rsplit_once(" and ") {
        Some((h, l)) => (h.to_owned(), Some(l.to_owned())),
        None => (list.clone(), None),
    };
    let mut out: Vec<String> = head.split(", ").map(str::to_owned).collect();
    out.extend(last);
    out.into_iter()
        .map(|n| {
            n.replace('\u{1}', SCREEN_PICTURE)
                .replace('\u{2}', COVERS_ALL)
        })
        .collect()
}

/// The oracle's kinds of input, weakest first (twentieth, twenty-first and twenty-third
/// reviews).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Observe,
    Scroll,
    Full,
}

/// The oracle's record of one task.
#[derive(Default)]
struct Oracle {
    read: BTreeSet<Ctx>,
    /// Per (destination, source): the strongest kind allowed for the task.
    granted: BTreeMap<(Ctx, Ctx), Kind>,
    /// (destination, source) pairs allowed for the action being carried out only.
    for_action: BTreeSet<(Ctx, Ctx)>,
    /// Embedded classes that held more than one document.
    ambiguous: BTreeSet<Ctx>,
}

impl Oracle {
    /// The sources that may not reach `dest` with an input of `kind` (section 1: a place is
    /// not a source for itself, except a tool that is not self-contained and a covering
    /// embedded class; nineteenth review: no one-way relation, every other place read is a
    /// source).
    fn unallowed(&self, dest: &Ctx, kind: Kind) -> BTreeSet<Ctx> {
        self.read
            .iter()
            .filter(|r| {
                let itself = *r == dest && !dest.self_source(&self.ambiguous);
                let pair = (dest.clone(), (*r).clone());
                let for_action = self.for_action.contains(&pair);
                let granted = self.granted.get(&pair).is_some_and(|g| *g >= kind);
                !itself && !for_action && !granted
            })
            .cloned()
            .collect()
    }

    /// Keeps `kind` for (dest, source) for the task (never lowers a grant).
    fn grant(&mut self, dest: &Ctx, source: &Ctx, kind: Kind) {
        if *dest == Ctx::Unknown {
            return;
        }
        let g = self
            .granted
            .entry((dest.clone(), source.clone()))
            .or_insert(kind);
        *g = (*g).max(kind);
    }

    /// The (destination, kind) pairs that must all allow every source before the step is
    /// sent: its own destinations at its own kind, and every other place on the screen at
    /// the observe kind (twenty-third review).
    fn check(&self, dests: &[(Ctx, Kind)]) -> BTreeSet<(Ctx, Ctx)> {
        dests
            .iter()
            .flat_map(|(d, k)| {
                self.unallowed(d, *k)
                    .into_iter()
                    .map(move |c| (d.clone(), c))
            })
            .collect()
    }
}

/// Twenty-third review: the step's own destinations at `kind`, and every other place on
/// `current` at the observe kind.
fn with_observers(own: Vec<Ctx>, kind: Kind, current: &Screen) -> Vec<(Ctx, Kind)> {
    let mut out: Vec<(Ctx, Kind)> = own.iter().map(|d| (d.clone(), kind)).collect();
    for p in every_of(current) {
        if !own.contains(&p) {
            out.push((p, Kind::Observe));
        }
    }
    out
}

/// The oracle's kind of the model's action `json` on `current`.
fn kind_of(json: &str, current: &Screen) -> Kind {
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    match v["action"].as_str().unwrap_or_default() {
        "wait" | "look" | "press_home" => Kind::Observe,
        _ if scroll_kind(json, current) => Kind::Scroll,
        _ => Kind::Full,
    }
}

/// The destinations a flow note names after `FLOW`: those the action's own input goes
/// into (`, into …`), and those that only see it (`, to …, which can tell …`).
fn named_dests(note: &str) -> (Vec<String>, Vec<String>) {
    const SEEN: &str = ", which can tell when Jarvis waits, looks or acts";
    let tail = &note[note.find(FLOW).unwrap() + FLOW.len()..];
    let tail = tail.split(" (it ").next().unwrap_or_default();
    let list = |l: &str| -> Vec<String> {
        if l.is_empty() {
            return Vec::new();
        }
        let l = l.replace(COVERS_ALL, "\u{2}");
        let (head, last) = match l.rsplit_once(" and ") {
            Some((h, x)) => (h.to_owned(), Some(x.to_owned())),
            None => (l.clone(), None),
        };
        let mut out: Vec<String> = head.split(", ").map(str::to_owned).collect();
        out.extend(last);
        out.into_iter()
            .map(|n| n.replace('\u{2}', COVERS_ALL))
            .collect()
    };
    let (into, seen) = if let Some(rest) = tail.strip_prefix(", into ") {
        match rest.split_once(", and to ") {
            Some((i, s)) => (i, s.strip_suffix(SEEN).unwrap_or(s)),
            None => (rest, ""),
        }
    } else if let Some(rest) = tail.strip_prefix(", to ") {
        ("", rest.strip_suffix(SEEN).unwrap_or(rest))
    } else {
        ("", "")
    };
    (list(into), list(seen))
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
    // "tap the ad" taps the frame on the framed sites; on Spotlight "tap next" taps the
    // website row and "type" types into the search field.
    assert_eq!(index(&site_screen(FRAMED), "Frame"), ad);
    assert_eq!(index(&site_screen(TWO_FRAMES), "Frame"), ad);
    assert_eq!(index(&app_screen(EVIL), "Ad"), ad);
    assert_eq!(index(&app_screen(APPLE_MIXED), "Ad"), ad);
    assert_eq!(
        index(&spotlight_screen(), "evil.example/482913, Website"),
        next
    );
    assert_eq!(index(&spotlight_screen(), "Search"), field);
    // Nineteenth review: "tap the ad" taps the website row titled like a known app on
    // Spotlight, and the address field (the browser's chrome) on the first site.
    assert_eq!(
        index(
            &spotlight_screen(),
            &format!("{}, Website, help.example", APPS[0])
        ),
        ad
    );
    assert_eq!(index(&site_screen(0), "Address"), ad);
    assert_eq!(index(&app_screen(FIREFOX), "Address"), ad);
    for s in [
        app_screen(1),
        app_screen(MIXED),
        app_screen(EVIL),
        app_screen(FIREFOX),
        site_screen(0),
        site_screen(2),
    ] {
        assert_eq!(index(&s, "Next"), next);
        assert_eq!(index(&s, "Note"), field);
    }
    let steps = 3 + rng.below(12);
    let mut script: Vec<String> = Vec::new();
    for _ in 0..steps {
        script.push(match rng.below(13) {
            0 | 1 => format!(
                r#"{{"action":"open_app","app":"{}"}}"#,
                APPS[rng.below(APPS.len() as u64) as usize]
            ),
            2 => format!(
                r#"{{"action":"open_url","url":"https://{}/"}}"#,
                HOSTS[rng.below(3) as usize]
            ),
            3 | 4 => format!(r#"{{"action":"tap","element":{next}}}"#),
            5 => format!(r#"{{"action":"tap","element":{ad}}}"#),
            // Twenty-first review: a third of the scrolls start on "Next" (a full input).
            6 if rng.below(3) == 0 => {
                format!(r#"{{"action":"scroll","direction":"down","element":{next}}}"#)
            }
            6 => r#"{"action":"scroll","direction":"down"}"#.to_owned(),
            7 => format!(r#"{{"action":"type","element":{field},"text":"hello","submit":false}}"#),
            8 => r#"{"action":"call_tool","name":"lookup","args":{"q":"hello"}}"#.to_owned(),
            9 => r#"{"action":"call_tool","name":"clock","args":{}}"#.to_owned(),
            10 => r#"{"action":"wait","ms":300}"#.to_owned(),
            11 => r#"{"action":"look"}"#.to_owned(),
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
    config.model.supports_vision = rng.below(2) == 0;
    let spotlight_home = rng.below(2) == 0;
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
    let start = rng.below(APPS.len() as u64 + 3) as usize;
    let mut current = if start < APPS.len() {
        app_screen(start)
    } else {
        site_screen(start - APPS.len())
    };
    let mut session = Session::new(config, TaskSpec::new("p", TASK), MemoryStore::new());
    let mut model: std::collections::VecDeque<String> = script.into_iter().collect();
    let mut oracle = Oracle::default();
    let mut observed: BTreeSet<Ctx> = BTreeSet::new();
    let mut declined_since_infer = false;
    // Twenty-first review (F2b): a question was approved and nothing was observed since.
    let mut approved_unlooked = false;
    // Twenty-third review: effects (commands, tool calls, observations) since the model's
    // last step was proposed, and whether its first question has been answered.
    let mut since_proposal = 0u32;
    let mut answered = false;
    let mut last_action = String::new();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    for _ in 0..4_000 {
        let effect = session.step(TimedEvent::new(now, event));
        now += 50;
        if matches!(
            effect,
            Effect::Observe { .. } | Effect::Act(_) | Effect::CallTool(_)
        ) {
            since_proposal += 1;
            assert!(
                !declined_since_infer,
                "seed {seed}: {effect:?} after a declined flow question"
            );
        }
        event = match effect {
            Effect::Observe { screenshot } => {
                // Twenty-third review: a look's screenshot is an observe-kind input into
                // every place on the screen.
                if screenshot && kind_of_last(&last_action) == Some("look") {
                    let bad =
                        oracle.check(&with_observers(every_of(&current), Kind::Observe, &current));
                    assert!(
                        bad.is_empty(),
                        "seed {seed}: a look's screenshot while {bad:?} not allowed"
                    );
                }
                approved_unlooked = false;
                observed.extend(places_of(&current));
                let f = Ctx::Frame(HOSTS[TWO_FRAMES].to_owned());
                if places_of(&current).contains(&f) {
                    oracle.ambiguous.insert(f);
                }
                Event::Observed(Observation {
                    screen: current.clone(),
                    screenshot: screenshot.then(|| ImageRef {
                        id: "shot".into(),
                        width: 1179,
                        height: 2556,
                    }),
                    device_locked: false,
                })
            }
            Effect::Infer(req) => {
                oracle.read.append(&mut observed);
                // The screen picture given to the model is read.
                if req.image.is_some() {
                    oracle.read.insert(Ctx::Pixels);
                }
                oracle.for_action.clear();
                declined_since_infer = false;
                // A new action is proposed: the approved one was carried out or dropped.
                approved_unlooked = false;
                since_proposal = 0;
                answered = false;
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
                assert!(
                    !approved_unlooked,
                    "seed {seed}: {cmd:?} sent right after an approval, with no look"
                );
                let dests = dest_of(&cmd, &current);
                // Twenty-third review: a model wait (sent at a level, 1 000 ms or more; the
                // core's own waits are shorter) and Home are observe-kind inputs into every
                // place on the screen; every other command also reaches every other place on
                // it at the observe kind.
                let model_wait = matches!(cmd, RunnerCommand::Wait { ms } if ms >= 1_000);
                if !dests.is_empty() || model_wait {
                    sent += 1;
                }
                // A scroll is the only action the generator sends as a drag; its kind is
                // decided on the screen it acts on (R21-A).
                let kind = match &cmd {
                    RunnerCommand::Swipe { .. } if scroll_kind(&last_action, &current) => {
                        Kind::Scroll
                    }
                    RunnerCommand::PressKey { key: Key::Home, .. } => Kind::Observe,
                    RunnerCommand::Wait { .. } => Kind::Observe,
                    _ => Kind::Full,
                };
                let checked = if model_wait {
                    every_of(&current)
                        .into_iter()
                        .map(|d| (d, Kind::Observe))
                        .collect()
                } else if dests.is_empty() {
                    Vec::new()
                } else {
                    with_observers(dests, kind, &current)
                };
                let bad = oracle.check(&checked);
                assert!(
                    bad.is_empty(),
                    "seed {seed} (confirm empty: {confirm_nothing}): {cmd:?} into {checked:?} while {bad:?} not allowed (read {:?}, granted {:?})",
                    oracle.read,
                    oracle.granted
                );
                match &cmd {
                    RunnerCommand::LaunchApp { bundle_id } => {
                        let i = APPS.iter().position(|a| a == bundle_id).unwrap();
                        current = app_screen(i);
                    }
                    RunnerCommand::OpenUrl { url } => current = site_screen(host_of(url)),
                    RunnerCommand::PressKey { key: Key::Home, .. } => {
                        current = if spotlight_home {
                            spotlight_screen()
                        } else {
                            home_screen()
                        }
                    }
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
                let bad = oracle.check(&with_observers(vec![dest.clone()], Kind::Full, &current));
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
                    // Twenty-third review: nothing was sent or observed for the step before its
                    // first question (later questions of the same step follow approvals).
                    if !answered {
                        assert_eq!(
                            since_proposal, 0,
                            "seed {seed}: {since_proposal} effects between {last_action} and its question"
                        );
                    }
                    let own = dest_of_action(&last_action, &current);
                    assert!(
                        !own.is_empty(),
                        "seed {seed}: a flow question for {last_action}"
                    );
                    let v: serde_json::Value = serde_json::from_str(&last_action).unwrap();
                    let action = v["action"].as_str().unwrap_or_default().to_owned();
                    let kind = kind_of(&last_action, &current);
                    let all = with_observers(own.clone(), kind, &current);
                    let observers: Vec<Ctx> = all
                        .iter()
                        .filter(|(d, _)| !own.contains(d))
                        .map(|(d, _)| d.clone())
                        .collect();
                    let unallowed_of = |d: &Ctx| -> BTreeSet<Ctx> {
                        let k = all.iter().find(|(x, _)| x == d).map(|(_, k)| *k).unwrap();
                        oracle.unallowed(d, k)
                    };
                    let bad: BTreeSet<Ctx> =
                        all.iter().flat_map(|(d, _)| unallowed_of(d)).collect();
                    let note = req
                        .rows
                        .iter()
                        .find(|r| r.kind == jarvis_proto::QuestionRowKind::Flow)
                        .map(|r| r.text.clone())
                        .unwrap();
                    let (into, seen) = named_dests(&note);
                    let reaches_more = note.contains("(it reaches ");
                    let name = |c: &Ctx| c.name(&oracle.ambiguous);
                    // Twenty-third review (F2): the Home Screen read is named as such.
                    let source_name = |c: &Ctx| match c {
                        Ctx::App(a) if a == HOME => HOME_SCREEN_READ.to_owned(),
                        other => other.name(&oracle.ambiguous),
                    };
                    // The places that only see the step are named when they need approval,
                    // after `to`; the action's own after `into` (every one of them when it has
                    // several), or "here" with one own destination and no other named.
                    for d in &observers {
                        let named = seen.contains(&name(d));
                        assert!(
                            named || unallowed_of(d).is_empty() || reaches_more,
                            "seed {seed}: {d:?} not named: {note}"
                        );
                        assert!(!into.contains(&name(d)), "seed {seed}: {note}");
                    }
                    if kind == Kind::Observe {
                        assert!(into.is_empty(), "seed {seed}: {note}");
                        for d in &own {
                            assert!(
                                seen.contains(&name(d))
                                    || unallowed_of(d).is_empty()
                                    || reaches_more,
                                "seed {seed}: {d:?} not named: {note}"
                            );
                        }
                    } else if own.len() > 1 {
                        for d in &own {
                            assert!(
                                into.contains(&name(d)) || reaches_more,
                                "seed {seed}: {d:?} not named: {note}"
                            );
                        }
                    } else if seen.is_empty() {
                        assert!(
                            !note.contains(&format!("{FLOW}, into ")),
                            "seed {seed}: {note}"
                        );
                    }
                    let dest = &all;
                    // Asked only when a source may not reach it; every one named, no "more".
                    assert!(
                        !bad.is_empty(),
                        "seed {seed}: needless ask {} ({last_action} into {dest:?})",
                        req.summary
                    );
                    assert!(!req.summary.contains("and 1 more"), "{}", req.summary);
                    let names: BTreeSet<String> = named(&req.summary).into_iter().collect();
                    // The destinations the question names: all of them unless some do not fit.
                    let named_dests: Vec<(Ctx, Kind)> = if into.is_empty() && seen.is_empty() {
                        all.clone()
                    } else {
                        all.iter()
                            .filter(|(d, _)| into.contains(&name(d)) || seen.contains(&name(d)))
                            .cloned()
                            .collect()
                    };
                    let expected: BTreeSet<String> = named_dests
                        .iter()
                        .flat_map(|(d, _)| unallowed_of(d))
                        .map(|c| source_name(&c))
                        .collect();
                    // When the names do not fit, the question names as many as fit and says
                    // how many more it asks about next.
                    let rest = expected.len().saturating_sub(names.len());
                    if rest > 0 {
                        assert!(names.is_subset(&expected), "seed {seed}: {}", req.summary);
                        let what = if rest == 1 { "place" } else { "places" };
                        assert!(
                            req.summary.contains(&format!(
                                "it read {rest} more {what} in this task; Jarvis asks about them next"
                            )),
                            "seed {seed}: {}",
                            req.summary
                        );
                    } else {
                        assert_eq!(
                            names, expected,
                            "seed {seed}: {} ({last_action} into {dest:?})",
                            req.summary
                        );
                    }
                    asked += 1;
                    declined += u32::from(!approve);
                    answered = true;
                    if approve {
                        // Exactly the places named, into the destinations named; what each
                        // pair keeps (twentieth review, R20-A; twenty-third, R23).
                        // (destination, source, kept kind; `None`: this action only)
                        let mut keep: Vec<(Ctx, Ctx, Option<Kind>)> = Vec::new();
                        for (d, k) in &named_dests {
                            for c in unallowed_of(d) {
                                if !names.contains(&source_name(&c)) {
                                    continue;
                                }
                                let own_dest = own.contains(d);
                                let kept = if !own_dest || *k == Kind::Observe {
                                    Some(Kind::Observe)
                                } else if own.contains(&Ctx::Unknown)
                                    || (*k == Kind::Full && own.len() > 1)
                                    || action == "swipe"
                                {
                                    None
                                } else {
                                    Some(*k)
                                };
                                keep.push((d.clone(), c, kept));
                            }
                        }
                        for (d, c, kept) in keep {
                            match kept {
                                Some(k) => oracle.grant(&d, &c, k),
                                None => {
                                    oracle.for_action.insert((d, c));
                                }
                            }
                        }
                    } else {
                        declined_since_infer = true;
                    }
                }
                approved_unlooked |= approve;
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

/// The generator reaches every kind of place the oracle knows: each is read in some case
/// and, for the destinations, receives an input in some case.
#[test]
fn the_generator_covers_every_kind_of_place() {
    // Screens: what each one is read as.
    assert_eq!(places_of(&app_screen(EVIL)).len(), 2);
    // Nineteenth review: the browser's chrome is its own place (was: the site only).
    assert_eq!(
        places_of(&app_screen(FIREFOX)),
        vec![
            Ctx::App(APPS[FIREFOX].to_owned()),
            Ctx::FxSite(HOSTS[0].to_owned())
        ]
    );
    assert_eq!(places_of(&spotlight_screen()), vec![Ctx::App(HOME.into())]);
    assert!(places_of(&home_screen()).is_empty());
    // The session agrees on the places of each screen (named in one flow question each).
    let mut c = config();
    for a in APPS {
        c.known_apps.push(AppInfo::new(a, a));
    }
    c.model.supports_vision = true;
    let r = run(
        c,
        "Look at it",
        MemoryStore::new(),
        Phone::still(site_screen(TWO_FRAMES)),
        Script::model(&[
            r#"{"action":"look"}"#,
            &format!(r#"{{"action":"tap","element":{}}}"#, {
                compact(&site_screen(TWO_FRAMES))
                    .elements
                    .iter()
                    .find(|e| e.label == "Frame")
                    .unwrap()
                    .index
            }),
            r#"{"action":"done","summary":"ok"}"#,
        ])
        .decisions(&[true, false]),
    );
    let q: Vec<&String> = r
        .confirms
        .iter()
        .map(|c| &c.summary)
        .filter(|s| s.contains(FLOW))
        .collect();
    // Twenty-third review (R23): the look asks first (every place on the screen sees it and
    // none was given the others); approved, then the tap's question names every place read.
    assert_eq!(q.len(), 2, "{q:?}");
    assert!(q[0].starts_with("Look at the screen"), "{}", q[0]);
    let names: BTreeSet<String> = named(q[1]).into_iter().collect();
    let amb: BTreeSet<Ctx> = [Ctx::Frame(HOSTS[TWO_FRAMES].into())].into();
    let expected: BTreeSet<String> = [
        // Nineteenth review: Safari's chrome ("Tabs") is a place of its own, read.
        Ctx::App(SAFARI.into()),
        Ctx::Site(HOSTS[TWO_FRAMES].into()),
        Ctx::Frame(HOSTS[TWO_FRAMES].into()),
        Ctx::Pixels,
    ]
    .iter()
    .map(|c| c.name(&amb))
    .collect();
    assert_eq!(names, expected, "{}", q[1]);
}
