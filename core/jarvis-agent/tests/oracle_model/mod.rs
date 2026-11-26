//! The independent oracle of Rule T (twenty-sixth review) as a shared model: places, steps,
//! the phone, the generator, the driver and the checks. Its reading of the specification is
//! stated at the top of `twentysixth_oracle.rs` (rules 1-19, A1-A13, D3); the extensions
//! (pickers, incomplete trees, the unidentified layer, system alerts, focus failures, place
//! changes inside a step) are stated at the top of `oracle_extended.rs` (E1-E6, X1-X7) and
//! are all behind `Features`, which is all zero for `twentysixth_oracle.rs`: its tasks, its
//! random draws and its checks are what they were.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

use std::collections::{BTreeMap, BTreeSet};

use jarvis_agent::gestures::{drag_gesture, GestureKind};
use jarvis_agent::{Session, VERIFY_SETTLE_MS};
use jarvis_memory::MemoryStore;
use jarvis_observe::compact;
use jarvis_proto::fixtures::{node, rect, screen, text, IPHONE_SIZE};
use jarvis_proto::{
    AppInfo, CompactScreen, ConfirmRequest, Direction, Effect, Event, ImageRef, Key, ModelOutput,
    ModelProfile, Observation, OutcomeStatus, ParamKind, Point, Policy, Rect, RiskCategory, Role,
    RunnerCommand, RunnerResult, Screen, SessionConfig, TaskSpec, Tier, TimedEvent, ToolParam,
    ToolResult, ToolSpec, UiNode, MAX_TREE_DEPTH, UNIDENTIFIED_APP_BUNDLE_ID,
};

#[path = "../../../jarvis-observe/tests/common/ios.rs"]
mod ios;

/// For the shared fixture parser (`ios.rs`).
fn compact_text(c: &CompactScreen) -> String {
    format!("{c:?}")
}

pub const FLOW: &str = "what it read there could be passed on here";
pub const MSGS: &str = "com.apple.mobilesms";
pub const SHOP: &str = "com.example.shop";
pub const SAFARI: &str = "com.apple.mobilesafari";
pub const MAIL: &str = "com.apple.mobilemail";
pub const HOME: &str = "com.apple.springboard";
pub const OWN: &str = "dev.jarvis.app";
pub const OWN_SECRET: &str = "OWNSECRET7731";
pub const TASK: &str =
    "Check my messages, the Shop and Mail, browse bank.example and news.example, type hello world";
const BROWSERS: &[&str] = &[
    "com.apple.mobilesafari",
    "com.google.chrome.ios",
    "org.mozilla.ios.firefox",
    "com.microsoft.msedge",
    "com.brave.ios.browser",
    "com.duckduckgo.mobile.ios",
    "com.opera.operatouch",
];

// ---------------------------------------------------------------------------------------
// Deterministic randomness.
// ---------------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }
    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() % 10_000) as f64 / 10_000.0 < p
    }
    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len() as u64) as usize]
    }
}

// ---------------------------------------------------------------------------------------
// The oracle: places.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum P {
    App(String),
    Site(String),
    Unreadable(String),
    Web(String),
    Nested(String),
    Pic,
    Home,
    Tool(String),
    Unknown,
    // --- Extensions (oracle_extended.rs, E1-E4); never produced with `Features` all zero.
    /// E1: `<App> (showing the <kind> picker)`: (bundle, kind).
    Picker(String, String),
    /// E4: a system alert (SpringBoard's tree holding a raw `Alert`).
    Alert,
    /// E3: the unidentified layer (a system surface, a SpringBoard mode).
    Unident,
}

impl P {
    pub fn covering(&self) -> bool {
        mutation() != 3
            && matches!(
                self,
                P::Unreadable(_) | P::Web(_) | P::Nested(_) | P::Unident
            )
    }
}

pub fn app_name(bundle: &str) -> String {
    match bundle {
        MSGS => "Messages".into(),
        SHOP => "Shop".into(),
        SAFARI => "Safari".into(),
        MAIL => "Mail".into(),
        "com.apple.preferences" => "Settings".into(),
        OWN => "Jarvis".into(),
        UNIDENTIFIED_APP_BUNDLE_ID => "an app Jarvis cannot identify".into(),
        other => other.into(),
    }
}

/// Every name a place may be given in a question (source and destination forms).
pub fn names_of(p: &P) -> Vec<String> {
    const C: &str = "(Jarvis cannot tell its parts apart)";
    match p {
        P::App(b) => vec![app_name(b)],
        P::Site(h) => vec![h.clone()],
        P::Unreadable(b) => vec![format!(
            "all pages in {} with no readable address {C}",
            app_name(b)
        )],
        P::Web(b) => vec![format!("all web content in {} {C}", app_name(b))],
        P::Nested(b) => vec![format!("all nested web content in {} {C}", app_name(b))],
        P::Pic => {
            vec!["screen pictures (they can show notifications and other apps' banners)".into()]
        }
        P::Home => vec![
            "the Home Screen (notifications or search results shown on it)".into(),
            "the Home Screen".into(),
        ],
        P::Tool(n) => vec![format!("the tool {n}"), n.clone()],
        P::Unknown => vec![],
        P::Picker(b, k) => vec![format!("{} (showing the {k} picker)", app_name(b))],
        P::Alert => vec!["a system alert".into()],
        P::Unident => vec![
            "a screen in an app Jarvis cannot identify that Jarvis cannot read (it may show another party's page)".into(),
        ],
    }
}

#[derive(Clone, Debug, Default)]
pub struct Info {
    pub bundle: String,
    pub on_screen: BTreeSet<P>,
    pub read: BTreeSet<P>,
    /// Web-view regions, deepest last.
    pub regions: Vec<(Rect, P, usize)>,
    pub home: bool,
    pub own: bool,
    /// E1: the picker place, when the screen shows a system picker.
    pub picker: Option<P>,
    /// E4: SpringBoard's tree holds an alert.
    pub alert: bool,
    /// E3: the unidentified layer.
    pub unident: bool,
    /// E2: the tree was cut, or holds a shown node past the depth bound.
    pub incomplete: bool,
}

fn contains(r: &Rect, p: Point) -> bool {
    p.x >= r.x && p.x < r.x + r.width && p.y >= r.y && p.y < r.y + r.height
}

fn is_control(role: &Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::Link
            | Role::TextField
            | Role::SecureTextField
            | Role::TextView
            | Role::SearchField
            | Role::Cell
            | Role::Switch
            | Role::Slider
            | Role::Picker
            | Role::Tab
    )
}

const ADDRESS_LABELS: &[&str] = &["address", "url", "address bar", "website address"];
const ADDRESS_IDS: &[&str] = &["URL", "Address", "AddressBar", "TabBarItemTitle"];

fn ascii_host(v: &str) -> Option<String> {
    let v = v.trim_matches(|c: char| c.is_whitespace() || c == '\u{200e}' || c == '\u{200f}');
    if v.is_empty()
        || !v.contains('.')
        || !v.chars().any(|c| c.is_ascii_alphabetic())
        || !v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
    {
        return None;
    }
    Some(v.to_ascii_lowercase())
}

/// Places of a screen, decided from its raw tree by the document's rules.
pub fn places_of(s: &Screen, own_driven: bool) -> Info {
    let bundle = s.app_bundle_id.to_ascii_lowercase();
    let mut info = Info {
        bundle: bundle.clone(),
        incomplete: s.tree_cut || shown_depth(&s.root) > MAX_TREE_DEPTH,
        ..Info::default()
    };
    // E4: a SpringBoard alert; E3: a SpringBoard mode or the runner's unidentified layer.
    if bundle == HOME && has_shown(&s.root, &|n| n.role == Role::Alert) {
        info.alert = true;
        info.on_screen.insert(P::Alert);
        if reads_content(&s.root) {
            info.read.insert(P::Alert);
        }
        return info;
    }
    if bundle == UNIDENTIFIED_APP_BUNDLE_ID || (bundle == HOME && springboard_mode(&s.root)) {
        // E3 (triage, oracle fix): two places, as for any screen Jarvis cannot read: the
        // app's own place (here the unidentified app), on screen and read (the document:
        // "what is read there is unchanged (the app's place and the screen picture)"), and
        // the screen Jarvis cannot read, where every input goes.
        info.unident = true;
        info.bundle = UNIDENTIFIED_APP_BUNDLE_ID.into();
        let app = P::App(UNIDENTIFIED_APP_BUNDLE_ID.into());
        info.on_screen.insert(app.clone());
        info.on_screen.insert(P::Unident);
        info.read.insert(app);
        return info;
    }
    if bundle == HOME {
        info.home = true;
        info.on_screen.insert(P::Home);
        let mut more = false;
        fn walk(n: &UiNode, more: &mut bool) {
            let banner = match &n.role {
                Role::Alert | Role::Sheet | Role::SearchField => true,
                Role::Other(name) => {
                    let l = name.to_ascii_lowercase();
                    l.contains("banner") || l.contains("notification")
                }
                _ => false,
            };
            if banner && n.visible {
                *more = true;
            }
            for c in &n.children {
                walk(c, more);
            }
        }
        walk(&s.root, &mut more);
        if more {
            info.read.insert(P::Home);
        }
        return info;
    }
    if (bundle == OWN || bundle.starts_with(&format!("{OWN}."))) && !own_driven {
        info.own = true;
        return info;
    }
    let browser = BROWSERS.contains(&bundle.as_str());
    // Pass 1: address-like nodes outside every web view.
    let mut addr_like = 0usize;
    let mut candidate: Option<(String, bool)> = None; // (value, qualifies)
    fn pass1(n: &UiNode, in_web: bool, addr_like: &mut usize, cand: &mut Option<(String, bool)>) {
        if !n.visible {
            return;
        }
        let in_web_here = in_web || n.role == Role::WebView;
        if !in_web && n.role != Role::WebView {
            let label = n.label.clone().unwrap_or_default();
            let ll = label.trim().to_ascii_lowercase();
            let id = n.identifier.clone().unwrap_or_default();
            let labelled = ADDRESS_LABELS.contains(&ll.as_str());
            if labelled || ADDRESS_IDS.contains(&id.as_str()) {
                *addr_like += 1;
                let role_ok = matches!(n.role, Role::TextField | Role::Button)
                    || (matches!(&n.role, Role::Other(_)) && id == "TabBarItemTitle");
                let q = labelled && role_ok && !n.focused;
                *cand = Some((n.value.clone().unwrap_or_default(), q));
            }
        }
        for c in &n.children {
            pass1(c, in_web_here, addr_like, cand);
        }
    }
    pass1(&s.root, false, &mut addr_like, &mut candidate);
    let host = if browser && addr_like == 1 {
        candidate.and_then(|(v, q)| if q { ascii_host(&v) } else { None })
    } else {
        None
    };
    let top = if browser {
        match &host {
            Some(h) => P::Site(h.clone()),
            None => P::Unreadable(bundle.clone()),
        }
    } else {
        P::Web(bundle.clone())
    };
    let app = P::App(bundle.clone());
    info.on_screen.insert(app.clone());
    // Pass 2: content and regions.
    struct W<'a> {
        info: &'a mut Info,
        top: P,
        bundle: String,
        native: bool,
        host: Option<String>,
    }
    fn pass2(w: &mut W, n: &UiNode, depth: usize) {
        if !n.visible {
            return;
        }
        let mut d = depth;
        if n.role == Role::WebView {
            d += 1;
            let p = if d == 1 {
                w.top.clone()
            } else {
                P::Nested(w.bundle.clone())
            };
            w.info.regions.push((n.frame, p.clone(), d));
            w.info.on_screen.insert(p.clone());
            w.info.read.insert(p);
        } else if depth == 0 && !matches!(n.role, Role::Application | Role::Window) {
            let label = n.label.clone().unwrap_or_default();
            let value = n.value.clone().unwrap_or_default();
            let is_addr = w.host.is_some()
                && ADDRESS_LABELS.contains(&label.trim().to_ascii_lowercase().as_str());
            if !is_addr
                && (!label.trim().is_empty() || !value.trim().is_empty() || is_control(&n.role))
            {
                w.native = true;
            }
        }
        for c in &n.children {
            pass2(w, c, d);
        }
    }
    let mut w = W {
        info: &mut info,
        top,
        bundle: bundle.clone(),
        native: false,
        host,
    };
    pass2(&mut w, &s.root, 0);
    if w.native {
        info.read.insert(app.clone());
    }
    info.regions.sort_by_key(|r| r.2);
    // E1: a system picker in the host's tree: a place of its own, read with the app.
    // (Self-test `ORACLE26_MUTATE=4`: no picker place.)
    if let Some(kind) = picker_kind(&s.root, false, s.size).filter(|_| mutation() != 4) {
        let pk = P::Picker(bundle.clone(), kind.into());
        info.on_screen.insert(pk.clone());
        info.read.insert(pk.clone());
        info.read.insert(app);
        info.picker = Some(pk);
    }
    info
}

/// The deepest level (the root is 1) of a shown node.
fn shown_depth(n: &UiNode) -> usize {
    let mut best = 0;
    let mut stack = vec![(n, 1usize)];
    while let Some((x, d)) = stack.pop() {
        if !x.visible {
            continue;
        }
        best = best.max(d);
        for c in &x.children {
            stack.push((c, d + 1));
        }
    }
    best
}

fn has_shown(n: &UiNode, f: &dyn Fn(&UiNode) -> bool) -> bool {
    let mut stack = vec![n];
    while let Some(x) = stack.pop() {
        if !x.visible {
            continue;
        }
        if f(x) {
            return true;
        }
        stack.extend(x.children.iter());
    }
    false
}

/// Rule 8 of the base model on a whole tree: a node other than the application and window
/// nodes with a label, a value or a control role.
fn reads_content(n: &UiNode) -> bool {
    has_shown(n, &|x| {
        !matches!(x.role, Role::Application | Role::Window)
            && (x.label.as_deref().is_some_and(|l| !l.trim().is_empty())
                || x.value.as_deref().is_some_and(|v| !v.trim().is_empty())
                || is_control(&x.role))
    })
}

/// E3: SpringBoard's edit mode, App Library or an open folder, by the identifiers the
/// document names.
fn springboard_mode(n: &UiNode) -> bool {
    has_shown(n, &|x| {
        x.identifier.as_deref().is_some_and(|i| {
            i == "DeleteButton" || i == "dewey-pod-view" || i.ends_with(", Folder name")
        })
    })
}

/// E1, X1: the picker kind a shown, on-screen node outside every web view names.
fn picker_kind(n: &UiNode, in_web: bool, size: jarvis_proto::Size) -> Option<&'static str> {
    if !n.visible {
        return None;
    }
    let in_web = in_web || n.role == Role::WebView;
    if !in_web {
        let f = n.frame;
        let on = f.width > 0.0
            && f.height > 0.0
            && f.x < size.width
            && f.y < size.height
            && f.x + f.width > 0.0
            && f.y + f.height > 0.0;
        if on {
            match n.identifier.as_deref().unwrap_or("") {
                "ContactsListView" => return Some("contact"),
                "photosView_content_scroll_view" | "PickerOnboardingHeaderViewIcon" => {
                    return Some("photo")
                }
                "FullDocumentManagerViewControllerNavigationBar" | "DOC.browsingModeTabBar" => {
                    return Some("file")
                }
                i if i.starts_with("DOC.browsingRoot") => return Some("file"),
                _ => {}
            }
        }
    }
    n.children.iter().find_map(|c| picker_kind(c, in_web, size))
}

impl Info {
    /// Every place under a point: the screen's own place and every web view holding it.
    pub fn places_under(&self, p: Point) -> Vec<P> {
        if self.home {
            return vec![P::Home];
        }
        if self.alert {
            return vec![P::Alert];
        }
        if self.unident {
            return vec![P::Unident];
        }
        let mut v = vec![P::App(self.bundle.clone())];
        for (r, pl, _) in &self.regions {
            if contains(r, p) {
                v.push(pl.clone());
            }
        }
        v
    }

    pub fn place_at(&self, p: Point) -> P {
        if self.home {
            return P::Home;
        }
        if self.alert {
            return P::Alert;
        }
        if self.unident {
            return P::Unident;
        }
        let mut best = P::App(self.bundle.clone());
        for (r, pl, _) in &self.regions {
            if contains(r, p) {
                best = pl.clone();
            }
        }
        best
    }
}

// ---------------------------------------------------------------------------------------
// Model steps.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum A {
    Tap(u32),
    LongPress(u32),
    Type {
        el: Option<u32>,
        text: String,
        submit: bool,
    },
    Clear(u32),
    Scroll {
        dir: Direction,
        el: Option<u32>,
    },
    Swipe {
        dir: Direction,
        el: Option<u32>,
    },
    TapPoint(f64, f64),
    Home,
    OpenApp(String),
    OpenUrl(String),
    Tool(String),
    Wait(u64),
    Look,
    Ask,
    Done,
    Fail,
    Invalid,
}

fn dir_s(d: Direction) -> &'static str {
    match d {
        Direction::Up => "up",
        Direction::Down => "down",
        Direction::Left => "left",
        Direction::Right => "right",
    }
}

impl A {
    pub fn json(&self) -> String {
        match self {
            A::Tap(i) => format!(r#"{{"action":"tap","element":{i}}}"#),
            A::LongPress(i) => {
                format!(r#"{{"action":"long_press","element":{i},"duration_ms":800}}"#)
            }
            A::Type { el, text, submit } => match el {
                Some(i) => format!(
                    r#"{{"action":"type","element":{i},"text":"{text}","submit":{submit}}}"#
                ),
                None => format!(r#"{{"action":"type","text":"{text}","submit":{submit}}}"#),
            },
            A::Clear(i) => format!(r#"{{"action":"clear","element":{i}}}"#),
            A::Scroll { dir, el } => match el {
                Some(i) => format!(
                    r#"{{"action":"scroll","direction":"{}","element":{i}}}"#,
                    dir_s(*dir)
                ),
                None => format!(r#"{{"action":"scroll","direction":"{}"}}"#, dir_s(*dir)),
            },
            A::Swipe { dir, el } => match el {
                Some(i) => format!(
                    r#"{{"action":"swipe","direction":"{}","element":{i}}}"#,
                    dir_s(*dir)
                ),
                None => format!(r#"{{"action":"swipe","direction":"{}"}}"#, dir_s(*dir)),
            },
            A::TapPoint(x, y) => format!(r#"{{"action":"tap_point","x":{x:.3},"y":{y:.3}}}"#),
            A::Home => r#"{"action":"press_home"}"#.into(),
            A::OpenApp(a) => format!(r#"{{"action":"open_app","app":"{a}"}}"#),
            A::OpenUrl(u) => format!(r#"{{"action":"open_url","url":"{u}"}}"#),
            A::Tool(n) if n == "lookup" => {
                r#"{"action":"call_tool","name":"lookup","args":{"q":"hello world"}}"#.into()
            }
            A::Tool(n) => format!(r#"{{"action":"call_tool","name":"{n}","args":{{}}}}"#),
            A::Wait(ms) => format!(r#"{{"action":"wait","ms":{ms}}}"#),
            A::Look => r#"{"action":"look"}"#.into(),
            A::Ask => r#"{"action":"ask_user","question":"Which one?"}"#.into(),
            A::Done => r#"{"action":"done","summary":"ok"}"#.into(),
            A::Fail => r#"{"action":"fail","reason":"cannot"}"#.into(),
            A::Invalid => "I will now tap something nice".into(),
        }
    }
    fn kind_name(&self) -> &'static str {
        match self {
            A::Tap(_) => "tap",
            A::LongPress(_) => "long_press",
            A::Type { .. } => "type",
            A::Clear(_) => "clear",
            A::Scroll { .. } => "scroll",
            A::Swipe { .. } => "swipe",
            A::TapPoint(..) => "tap_point",
            A::Home => "press_home",
            A::OpenApp(_) => "open_app",
            A::OpenUrl(_) => "open_url",
            A::Tool(_) => "tool",
            A::Wait(_) => "wait",
            A::Look => "look",
            A::Ask => "ask_user",
            A::Done => "done",
            A::Fail => "fail",
            A::Invalid => "invalid",
        }
    }
    fn gated(&self) -> bool {
        !matches!(self, A::Ask | A::Done | A::Fail | A::Invalid)
    }
    /// An action that touches the screen in front (E1b, E2a).
    pub fn touches(&self) -> bool {
        matches!(
            self,
            A::Tap(_)
                | A::LongPress(_)
                | A::Type { .. }
                | A::Clear(_)
                | A::Scroll { .. }
                | A::Swipe { .. }
                | A::TapPoint(..)
        )
    }
}

/// Kinds: 1 observe, 2 scroll, 3 full.
pub type Kind = u8;
pub const OBS: Kind = 1;
pub const SCR: Kind = 2;
pub const FULL: Kind = 3;
/// Kept for this action only.
const ACTION: Kind = 0;

fn el(c: &CompactScreen, i: u32) -> Option<&jarvis_proto::Element> {
    c.elements.iter().find(|e| e.index == i)
}

fn is_scroller(r: &Role) -> bool {
    matches!(
        r,
        Role::ScrollView | Role::Table | Role::CollectionView | Role::WebView
    )
}

fn seg_hits(a: Point, b: Point, r: &Rect) -> bool {
    // Sample the segment (the paths are straight and short).
    (0..=40).any(|k| {
        let t = f64::from(k) / 40.0;
        contains(r, Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t))
    })
}

pub fn kind_of(a: &A, c: &CompactScreen) -> Kind {
    match a {
        A::Wait(_) | A::Look | A::Home => OBS,
        A::Scroll { dir, el: e } => {
            let frame = match e {
                None => None,
                Some(i) => match el(c, *i) {
                    Some(x) if is_scroller(&x.role) => Some(x.frame),
                    _ => return FULL,
                },
            };
            let Ok(g) = drag_gesture(GestureKind::Scroll, *dir, frame, c) else {
                return FULL;
            };
            let sideways = matches!(dir, Direction::Left | Direction::Right);
            let home = c.app_bundle_id.eq_ignore_ascii_case(HOME);
            // R21-A: a drag that starts on a control works it (the touch-down lands there).
            // Triage (document gap D3): "a control" is a button, a link or an adjustable
            // control; a cell or a text field a vertical drag starts on is not one.
            if c.elements.iter().any(|x| {
                x.enabled
                    && matches!(
                        x.role,
                        Role::Button | Role::Link | Role::Switch | Role::Slider | Role::Picker
                    )
                    && contains(&x.frame, g.from)
            }) {
                return FULL;
            }
            for x in &c.elements {
                if !x.enabled || !seg_hits(g.from, g.to, &x.frame) {
                    continue;
                }
                let reacts = match x.role {
                    Role::Picker | Role::Slider => true,
                    Role::Switch => sideways,
                    Role::Button | Role::Cell => sideways && !home,
                    _ => false,
                };
                if reacts {
                    return FULL;
                }
            }
            SCR
        }
        _ => FULL,
    }
}

/// X2: whether a scroll on an incomplete screen is of the full kind.
pub fn x2_full() -> bool {
    std::env::var("ORACLE_EXT_X2").is_ok_and(|v| v == "full")
}

/// X6: the most a node may be tall to be a list row (the document says "of row height").
pub const ROW_MAX_HEIGHT: f64 = 200.0;

/// Probe round 3 (a), M2, M3: a sideways scroll's drag touches a raw `Cell` (or a node a
/// drawn cell holds), or a drawn node of row height spanning at least 0.8 of the width of
/// its nearest scrollable ancestor.
fn crosses_row(a: &A, s: &Screen, c: &CompactScreen) -> bool {
    let A::Scroll { dir, el: e } = a else {
        return false;
    };
    if !matches!(dir, Direction::Left | Direction::Right) {
        return false;
    }
    let frame = match e {
        None => None,
        Some(i) => match el(c, *i) {
            Some(x) if is_scroller(&x.role) => Some(x.frame),
            _ => return false,
        },
    };
    let Ok(g) = drag_gesture(GestureKind::Scroll, *dir, frame, c) else {
        return false;
    };
    fn walk(n: &UiNode, scroller: Option<Rect>, in_cell: bool, a: Point, b: Point) -> bool {
        if !n.visible {
            return false;
        }
        let drawn = n.frame.width > 0.0 && n.frame.height > 0.0;
        let cell = in_cell || (drawn && n.role == Role::Cell);
        if drawn && seg_hits(a, b, &n.frame) {
            if cell {
                return true;
            }
            if let Some(sf) = scroller {
                if !is_scroller(&n.role)
                    && n.frame.width >= 0.8 * sf.width
                    && n.frame.height <= ROW_MAX_HEIGHT
                {
                    return true;
                }
            }
        }
        let next = if is_scroller(&n.role) && drawn {
            Some(n.frame)
        } else {
            scroller
        };
        n.children.iter().any(|ch| walk(ch, next, cell, a, b))
    }
    walk(&s.root, None, false, g.from, g.to)
}

fn known_app_label(label: &str) -> Option<String> {
    let l = label.trim().to_ascii_lowercase();
    let apps = [
        (MSGS, "messages"),
        (SHOP, "shop"),
        (SAFARI, "safari"),
        (MAIL, "mail"),
        ("com.apple.preferences", "settings"),
    ];
    // Audit of real trees (C8): only the whole label names an app; the `, App` and badge
    // suffixes occur in no real tree (a real icon keeps its name as the label), so such a
    // label is a launch Jarvis cannot name (stricter than the oracle's first model).
    apps.into_iter()
        .find(|(_, n)| l == *n)
        .map(|(b, _)| b.into())
}

fn url_host(u: &str) -> Option<String> {
    let rest = u.split("://").nth(1)?;
    let h = rest.split(['/', '?', '#']).next()?;
    ascii_host(h)
}

/// The own destinations of a step on a screen; `point` is the command's own point when known.
pub fn own_dests(
    a: &A,
    s: &Screen,
    info: &Info,
    c: &CompactScreen,
    point: Option<Point>,
) -> (BTreeSet<P>, bool) {
    let mut out = BTreeSet::new();
    let mut home_launch = false;
    let at = |i: u32| el(c, i).map(|e| e.frame.center());
    match a {
        A::Tap(i) | A::LongPress(i) | A::Clear(i) => {
            if info.home {
                let label = el(c, *i).map(|e| e.label.clone()).unwrap_or_default();
                let in_banner = el(c, *i).is_some_and(|e| e.frame.y < 140.0);
                if in_banner {
                    out.insert(P::Home);
                } else if let Some(b) = known_app_label(&label) {
                    if matches!(a, A::Tap(_)) {
                        // L1 (fixed when this review landed): the oracle followed the
                        // implementation, which resolved a long press on a known app's icon
                        // as a launch; it now takes the strict reading it stated, an input
                        // into SpringBoard (below).
                        home_launch = true;
                        out.insert(P::App(b));
                    } else {
                        out.insert(P::Home);
                    }
                } else {
                    out.insert(P::Unknown);
                }
            } else if let Some(p) = point.or_else(|| at(*i)) {
                out.insert(info.place_at(p));
            }
        }
        A::Type { el: e, .. } => {
            if let Some(i) = e {
                if let Some(p) = at(*i) {
                    out.insert(info.place_at(p));
                }
            } else if let Some(f) = c.elements.iter().find(|x| x.focused) {
                out.insert(info.place_at(f.frame.center()));
            } else {
                out.extend(info.on_screen.iter().cloned());
            }
            if info.home && out.is_empty() {
                out.insert(P::Home);
            }
        }
        A::Scroll { dir, el: e } | A::Swipe { dir, el: e } => {
            let gk = if matches!(a, A::Scroll { .. }) {
                GestureKind::Scroll
            } else {
                GestureKind::Swipe
            };
            let frame = e.and_then(|i| el(c, i).map(|x| x.frame));
            if let Some(f) = frame {
                out.insert(info.place_at(f.center()));
            }
            let from = point.or_else(|| drag_gesture(gk, *dir, frame, c).ok().map(|g| g.from));
            if let Some(p) = from {
                // A12: the places under a point: the screen's own place and every web view
                // holding the point.
                out.extend(info.places_under(p));
            }
        }
        A::TapPoint(x, y) => {
            let p = point.unwrap_or(Point::new(x * s.size.width, y * s.size.height));
            if info.home {
                out.insert(P::Unknown);
            } else {
                out.extend(info.places_under(p));
            }
        }
        A::OpenApp(n) => {
            if let Some(b) = known_app_label(n) {
                out.insert(P::App(b));
            }
        }
        A::OpenUrl(u) => match url_host(u) {
            Some(h) => {
                out.insert(P::Site(h));
            }
            None => {
                out.insert(P::Unknown);
            }
        },
        A::Tool(n) => {
            out.insert(P::Tool(n.clone()));
        }
        _ => {}
    }
    if info.own {
        out.retain(|p| !matches!(p, P::App(b) if b == OWN));
    }
    (out, home_launch)
}

pub fn self_contained(tool: &str) -> bool {
    tool == "clock"
}

#[derive(Clone, Debug, Default)]
pub struct Need {
    /// (dest, source, kind)
    pub pairs: Vec<(P, P, Kind)>,
    pub own: BTreeSet<P>,
    pub observers: BTreeSet<P>,
    pub kind: Kind,
}

#[derive(Clone, Debug, Default)]
pub struct Oracle {
    pub read: BTreeSet<P>,
    pub grants: BTreeMap<(P, P), Kind>,
    pub blocked: BTreeSet<(P, P)>,
    pub scope: BTreeSet<(P, P)>,
    pub own_driven: bool,
}

impl Oracle {
    pub fn sources(&self, d: &P, home_launch: bool) -> Vec<P> {
        self.read
            .iter()
            .filter(|s| {
                if *s == d {
                    return d.covering() || matches!(d, P::Tool(n) if !self_contained(n));
                }
                !(home_launch && **s == P::Home)
            })
            .cloned()
            .collect()
    }

    /// Every (dest, source, kind) the step needs on this screen.
    pub fn need(&self, a: &A, s: &Screen, point: Option<Point>) -> Need {
        let info = places_of(s, self.own_driven);
        let c = compact(s);
        let (mut own, home_launch) = own_dests(a, s, &info, &c, point);
        let mut kind = kind_of(a, &c);
        // Triage (oracle fix, probe round 3 (a) and its review's M3): a sideways scroll whose
        // drag touches a list row is a full input for Rule T.
        if kind == SCR && crosses_row(a, s, &c) {
            kind = FULL;
        }
        // E2, X2: on an incomplete screen a scroll is of the full kind (fail-closed reading,
        // `ORACLE_EXT_X2=full`); by default the session's reading (see oracle_extended.rs).
        if info.incomplete && matches!(a, A::Scroll { .. }) && x2_full() {
            kind = FULL;
        }
        // E1b: every input on a picker screen goes into the picker and the app.
        if let Some(pk) = &info.picker {
            if kind != OBS && a.touches() {
                own.insert(pk.clone());
                own.insert(P::App(info.bundle.clone()));
            }
        }
        let mut observers: BTreeSet<P> = info
            .on_screen
            .iter()
            .filter(|p| **p != P::Home && **p != P::Alert && !own.contains(*p))
            .cloned()
            .collect();
        if info.own || mutation() == 2 {
            observers.clear();
        }
        let mut pairs = Vec::new();
        for d in &own {
            for src in self.sources(d, home_launch) {
                pairs.push((d.clone(), src, kind));
            }
        }
        for d in &observers {
            for src in self.sources(d, false) {
                pairs.push((d.clone(), src, OBS));
            }
        }
        Need {
            pairs,
            own,
            observers,
            kind,
        }
    }

    pub fn granted(&self, d: &P, s: &P) -> Kind {
        self.grants
            .get(&(d.clone(), s.clone()))
            .copied()
            .unwrap_or(0)
    }

    pub fn unsatisfied(&self, n: &Need) -> Vec<(P, P, Kind)> {
        n.pairs
            .iter()
            .filter(|(d, s, k)| {
                self.granted(d, s) < *k && !self.scope.contains(&(d.clone(), s.clone()))
            })
            .cloned()
            .collect()
    }

    pub fn blocked_in(&self, n: &Need) -> Vec<(P, P)> {
        n.pairs
            .iter()
            .filter(|(d, s, k)| {
                self.blocked.contains(&(d.clone(), s.clone())) && self.granted(d, s) < *k
            })
            .map(|(d, s, _)| (d.clone(), s.clone()))
            .collect()
    }

    /// R24: some observer-eligible place on the screen lacks an observe grant from a source.
    pub fn restricted(&self, s: &Screen) -> bool {
        let info = places_of(s, self.own_driven);
        if info.own {
            return false;
        }
        info.on_screen
            .iter()
            .filter(|p| **p != P::Home && **p != P::Alert)
            .any(|d| {
                self.sources(d, false)
                    .iter()
                    .any(|src| self.granted(d, src) < OBS)
            })
    }

    /// X7: `restricted`, counting the pairs the current action's own approval allowed
    /// ("for this action only") as allowed.
    pub fn restricted_in_scope(&self, s: &Screen) -> bool {
        let info = places_of(s, self.own_driven);
        if info.own {
            return false;
        }
        info.on_screen
            .iter()
            .filter(|p| **p != P::Home && **p != P::Alert)
            .any(|d| {
                self.sources(d, false).iter().any(|src| {
                    self.granted(d, src) < OBS && !self.scope.contains(&(d.clone(), src.clone()))
                })
            })
    }

    pub fn keep_for(n: &Need, d: &P, a: &A) -> Kind {
        if mutation() == 1 {
            return ACTION;
        }
        // E3b: nothing is kept into the unidentified layer beyond the action. (Self-test
        // `ORACLE26_MUTATE=5`: kept like any place.)
        if *d == P::Unident && mutation() != 5 {
            return ACTION;
        }
        if n.observers.contains(d) {
            return OBS;
        }
        match n.kind {
            OBS => OBS,
            SCR => SCR,
            _ => {
                if n.own.len() == 1 && *d != P::Unknown && !matches!(a, A::Swipe { .. }) {
                    FULL
                } else {
                    ACTION
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Parsing a flow question.
// ---------------------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct Parsed {
    pub sources: Vec<String>,
    pub here: bool,
    pub into: Vec<String>,
    pub to: Vec<String>,
    pub partial: bool,
}

fn split_names(s: &str, known: &[String]) -> Vec<String> {
    let mut s = s.to_owned();
    let mut found = Vec::new();
    let mut sorted: Vec<&String> = known.iter().collect();
    sorted.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for n in sorted {
        while let Some(pos) = s.find(n.as_str()) {
            found.push((pos, n.clone()));
            let blank = "\u{1}".repeat(n.len());
            s.replace_range(pos..pos + n.len(), &blank);
        }
    }
    for part in s.split(", ") {
        for p in part.split(" and ") {
            let t = p.trim().trim_matches('\u{1}').trim();
            let t = t.trim_start_matches("and ").trim();
            if !t.is_empty() && !t.chars().all(|c| c == '\u{1}') {
                found.push((usize::MAX, t.to_owned()));
            }
        }
    }
    found.sort();
    found.into_iter().map(|(_, n)| n).collect()
}

pub fn parse_flow(summary: &str, known: &[String]) -> Option<Parsed> {
    let i = summary.find("Jarvis has read ")?;
    let rest = &summary[i + "Jarvis has read ".len()..];
    let j = rest.find(" in this task")?;
    let mut p = Parsed {
        sources: split_names(&rest[..j], known),
        partial: summary.contains("Jarvis asks about them next")
            || summary.contains("Jarvis asks about it next"),
        ..Parsed::default()
    };
    let k = summary.find(FLOW)?;
    let mut tail = summary[k + FLOW.len()..].to_owned();
    for marker in [" (it reaches", " (it read"] {
        while let Some(x) = tail.find(marker) {
            let end = tail[x..].find(')').map_or(tail.len(), |e| x + e + 1);
            tail.replace_range(x..end, "");
        }
    }
    if let Some(x) = tail.find(". ") {
        tail.truncate(x);
    }
    let tail = tail.trim_end_matches('.').to_owned();
    let end_obs = tail.find(", which can tell");
    let (into_part, to_part) = if let Some(rest) = tail.strip_prefix(", into ") {
        let cut = rest
            .find(", and to ")
            .map(|c| (c, ", and to ".len()))
            .or_else(|| rest.find(", to ").map(|c| (c, ", to ".len())));
        match cut {
            Some((c, l)) => {
                let obs = &rest[c + l..];
                let oe = obs.find(", which can tell").unwrap_or(obs.len());
                (Some(rest[..c].to_owned()), Some(obs[..oe].to_owned()))
            }
            None => {
                let e = rest.find(';').unwrap_or(rest.len());
                (Some(rest[..e].to_owned()), None)
            }
        }
    } else if let Some(rest) = tail.strip_prefix(", to ") {
        let oe = rest.find(", which can tell").unwrap_or(rest.len());
        (None, Some(rest[..oe].to_owned()))
    } else {
        let _ = end_obs;
        (None, None)
    };
    match into_part {
        Some(x) => p.into = split_names(&x, known),
        None => p.here = true,
    }
    if let Some(x) = to_part {
        p.to = split_names(&x, known);
    }
    Some(p)
}

// ---------------------------------------------------------------------------------------
// The phone.
// ---------------------------------------------------------------------------------------

fn scr(app: &str, name: &str, kb: bool, children: Vec<UiNode>) -> Screen {
    let root = UiNode::new(Role::Application, IPHONE_SIZE.to_rect())
        .with_label(name)
        .with_children(vec![
            UiNode::new(Role::Window, IPHONE_SIZE.to_rect()).with_children(children)
        ]);
    let mut s = screen(app, name, root, kb);
    // Bundle ids as the OS reports them (mixed case for Messages).
    if app == MSGS {
        s.app_bundle_id = "com.apple.MobileSMS".into();
    }
    s
}

fn field(label: &str, frame: Rect, value: &str, focused: bool) -> UiNode {
    let mut n = UiNode::new(Role::TextField, frame)
        .with_label(label)
        .hittable(true)
        .focused(focused);
    if !value.is_empty() {
        n = n.with_value(value);
    }
    n
}

#[derive(Clone, Debug, PartialEq)]
pub enum Front {
    Msgs,
    Shop,
    Safari {
        host: Option<String>,
        focused: bool,
    },
    Inbox,
    Message(u8),
    Home,
    Own,
    // --- Extensions (behind `Features`).
    /// E1: a system picker over its host (0 contact picker in the Shop; 1 photo and 2 file
    /// picker in Mail), with the screen it returns to.
    Picker(u8, Box<Front>),
    /// E3: a system surface over an app or the Home Screen (0 Control Centre, 1
    /// Notification Centre, 2 App Switcher): the runner reports the unidentified layer.
    Surface(u8, Box<Front>),
    /// E3: a SpringBoard mode (0 edit mode, 1 App Library, 2 open folder; real trees).
    SbMode(u8),
    /// E4: a system alert over an app (real tree).
    Alert(Box<Front>),
}

impl Front {
    /// An app's own screen (not the Home Screen, Jarvis, a layer over an app).
    pub fn is_app(&self) -> bool {
        matches!(
            self,
            Front::Msgs
                | Front::Shop
                | Front::Safari { .. }
                | Front::Inbox
                | Front::Message(_)
                | Front::Picker(..)
        )
    }
}

/// The generator's extensions (`oracle_extended.rs`); all zero for `twentysixth_oracle.rs`,
/// where none of them draws a random number or changes a screen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Features {
    /// E1: the Shop and Mail offer system pickers (a button each).
    pub picker: f64,
    /// E2: chance that a screen an app opens is incomplete.
    pub cut: f64,
    /// E3: chance per look that a system surface or a SpringBoard mode comes up.
    pub system: f64,
    /// E4: chance per look that a system alert comes up over an app.
    pub alert: f64,
    /// E5: chance that a focusing tap on a text field misses.
    pub focus_fail: f64,
    /// E6: chance of a place change after a focusing tap, and at a verify-settle look.
    pub place_change: f64,
}

impl Features {
    pub fn any(&self) -> bool {
        self.picker > 0.0
            || self.cut > 0.0
            || self.system > 0.0
            || self.alert > 0.0
            || self.focus_fail > 0.0
            || self.place_change > 0.0
    }
}

/// What the extended phone did (counts per task).
#[derive(Clone, Debug, Default)]
pub struct ExtEvents {
    pub focus_fail: usize,
    pub change_after_focus: usize,
    pub change_at_settle: usize,
}

fn parse_fixture(text: &str) -> Screen {
    let typed = text
        .split("\n== raw XCUITest snapshot")
        .next()
        .unwrap_or(text);
    ios::parse_screen(typed).unwrap()
}

fn relabel(n: &mut UiNode) {
    if let Some(l) = &mut n.label {
        *l = l.replace("Jarvis", "Shop");
    }
    for c in &mut n.children {
        relabel(c);
    }
}

/// The real trees: the App Library, an open folder, the edit mode (run 146, P15, the
/// runner's own observations of SpringBoard) and SpringBoard's notification-permission
/// alert (run 134), the app's name in it changed to the Shop's.
fn real_trees() -> &'static [Screen; 4] {
    static T: std::sync::OnceLock<[Screen; 4]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut alert = parse_fixture(include_str!(
            "../fixtures/ios26_probes/alert_notification_permission_springboard.txt"
        ));
        relabel(&mut alert.root);
        [
            parse_fixture(include_str!(
                "../fixtures/ios26_probes3/p15-springboard-10-jiggle-runner-tree.txt"
            )),
            parse_fixture(include_str!(
                "../fixtures/ios26_probes3/p15-springboard-06-app-library-runner-tree.txt"
            )),
            parse_fixture(include_str!(
                "../fixtures/ios26_probes3/p15-springboard-08-folder-runner-tree.txt"
            )),
            alert,
        ]
    })
}

fn expect_of(cmd: &RunnerCommand) -> Option<&jarvis_proto::Expect> {
    match cmd {
        RunnerCommand::Tap { expect, .. }
        | RunnerCommand::LongPress { expect, .. }
        | RunnerCommand::Swipe { expect, .. }
        | RunnerCommand::TypeText { expect, .. }
        | RunnerCommand::PressKey { expect, .. } => expect.as_ref(),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct Phone {
    pub front: Front,
    pub ad: bool,
    pub nested: bool,
    pub banner: bool,
    pub tick: u32,
    pub values: BTreeMap<String, String>,
    pub focus: Option<String>,
    pub back: Vec<Option<String>>,
    // --- Extensions (behind `Features`).
    pub feat: Features,
    /// E2: the app's screen is incomplete: flagged cut, or (`deep`) holding a shown node
    /// past the depth bound.
    pub cut: bool,
    pub deep: bool,
    /// E6a: the next look shows a place change.
    pub change_next: bool,
    pub ev: ExtEvents,
}

impl Phone {
    pub fn new(front: Front, nested: bool, banner: bool, feat: Features) -> Phone {
        Phone {
            front,
            ad: true,
            nested,
            banner,
            tick: 0,
            values: BTreeMap::new(),
            focus: None,
            back: Vec::new(),
            feat,
            cut: false,
            deep: false,
            change_next: false,
            ev: ExtEvents::default(),
        }
    }

    pub fn screen(&self) -> Screen {
        let mut s = self.screen_base();
        if self.cut && self.front.is_app() {
            if self.deep {
                // A shown node past the bound (a hand-built tree), not flagged.
                let mut n = text("Older posts", rect(16.0, 846.0, 200.0, 4.0));
                for _ in 0..MAX_TREE_DEPTH + 4 {
                    n = UiNode::new(Role::Other("Other".into()), rect(0.0, 845.0, 393.0, 6.0))
                        .with_children(vec![n]);
                }
                s.root.children[0].children.push(n);
            } else {
                s.tree_cut = true;
            }
        }
        s
    }

    fn screen_base(&self) -> Screen {
        let v = |k: &str| self.values.get(k).cloned().unwrap_or_default();
        let f = |k: &str| self.focus.as_deref() == Some(k);
        let kb = self.focus.is_some();
        match &self.front {
            Front::Msgs => scr(
                MSGS,
                "Messages",
                kb,
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
                    field(
                        "Message",
                        rect(16.0, 330.0, 280.0, 40.0),
                        &v("Message"),
                        f("Message"),
                    ),
                    node(Role::Button, "Send", rect(305.0, 330.0, 70.0, 36.0)),
                ],
            ),
            Front::Shop => {
                let mut kids = vec![
                    text("Daily reward", rect(16.0, 60.0, 200.0, 30.0)),
                    node(
                        Role::Button,
                        &format!("Collect in 0:{:02}", 59 - (self.tick % 60)),
                        rect(16.0, 100.0, 200.0, 40.0),
                    ),
                    node(Role::Button, "Settings", rect(250.0, 100.0, 120.0, 30.0)),
                    field(
                        "Promo code",
                        rect(16.0, 160.0, 300.0, 40.0),
                        &v("Promo code"),
                        f("Promo code"),
                    ),
                ];
                if self.feat.picker > 0.0 {
                    kids.push(node(
                        Role::Button,
                        "Share with a contact",
                        rect(16.0, 220.0, 250.0, 40.0),
                    ));
                }
                if self.ad {
                    let mut ad = vec![
                        text("Win a prize today", rect(16.0, 440.0, 300.0, 30.0)),
                        node(Role::Button, "Claim", rect(16.0, 480.0, 150.0, 44.0)),
                    ];
                    if self.nested {
                        ad.push(
                            UiNode::new(Role::WebView, rect(200.0, 620.0, 180.0, 90.0))
                                .with_children(vec![node(
                                    Role::Button,
                                    "Spin",
                                    rect(210.0, 640.0, 100.0, 40.0),
                                )]),
                        );
                    }
                    kids.push(
                        UiNode::new(Role::WebView, rect(0.0, 420.0, 393.0, 300.0))
                            .with_children(ad),
                    );
                }
                scr(SHOP, "Shop", kb, kids)
            }
            Front::Safari { host, focused } => {
                let h = host.clone().unwrap_or_default();
                let mut kids = vec![UiNode::new(Role::WebView, rect(0.0, 60.0, 393.0, 720.0))
                    .with_children(vec![
                        text(&format!("Welcome to {h}"), rect(16.0, 120.0, 361.0, 30.0)),
                        node(Role::Link, "Next page", rect(16.0, 300.0, 200.0, 44.0)),
                        node(Role::Link, "Other site", rect(16.0, 360.0, 250.0, 44.0)),
                        field(
                            "Search site",
                            rect(16.0, 640.0, 300.0, 40.0),
                            &v("Search site"),
                            f("Search site"),
                        ),
                        node(Role::Button, "Subscribe", rect(16.0, 700.0, 180.0, 44.0)),
                    ])];
                kids.push(node(Role::Button, "Back", rect(10.0, 790.0, 60.0, 34.0)));
                if host.is_some() {
                    kids.push(
                        UiNode::new(Role::TextField, rect(90.0, 790.0, 210.0, 34.0))
                            .with_label("Address")
                            .with_value(format!("\u{200e}{h}"))
                            .hittable(true)
                            .focused(*focused),
                    );
                }
                kids.push(node(Role::Button, "Tabs", rect(320.0, 790.0, 64.0, 30.0)));
                scr(SAFARI, "Safari", kb || *focused, kids)
            }
            Front::Inbox => scr(
                MAIL,
                "Mail",
                kb,
                vec![
                    node(Role::Button, "Mailboxes", rect(8.0, 50.0, 110.0, 40.0)),
                    node(Role::Cell, "Bank statement", rect(0.0, 120.0, 393.0, 80.0)),
                    node(Role::Cell, "Promo from Shop", rect(0.0, 200.0, 393.0, 80.0)),
                ],
            ),
            Front::Message(which) => {
                let (from, body) = if *which == 0 {
                    ("From: Bank", "Your statement is ready, balance $12,480.17")
                } else {
                    ("From: Shop", "Click here to win, reply with your code")
                };
                let mut top = vec![
                    node(Role::Button, "Inbox", rect(8.0, 50.0, 80.0, 40.0)),
                    text(from, rect(16.0, 100.0, 361.0, 30.0)),
                ];
                if self.feat.picker > 0.0 {
                    top.push(node(Role::Button, "Attach", rect(300.0, 50.0, 80.0, 40.0)));
                }
                top.push(
                    UiNode::new(Role::WebView, rect(0.0, 140.0, 393.0, 600.0)).with_children(vec![
                        text(body, rect(16.0, 160.0, 361.0, 30.0)),
                        node(Role::Link, "View statement", rect(16.0, 300.0, 250.0, 44.0)),
                        node(Role::Link, "Unsubscribe", rect(16.0, 400.0, 200.0, 44.0)),
                    ]),
                );
                scr(MAIL, "Mail", kb, top)
            }
            Front::Home => {
                let mut kids = vec![
                    node(Role::Button, "Shop", rect(20.0, 300.0, 64.0, 64.0)),
                    node(Role::Button, "Messages", rect(110.0, 300.0, 64.0, 64.0)),
                    node(
                        Role::Button,
                        "Mail, 3 unread",
                        rect(200.0, 300.0, 64.0, 64.0),
                    ),
                    node(Role::Button, "Safari", rect(290.0, 300.0, 64.0, 64.0)),
                    node(Role::Button, "Mystery", rect(20.0, 400.0, 64.0, 64.0)),
                ];
                if self.banner {
                    kids.push(
                        UiNode::new(
                            Role::Other("NotificationBanner".into()),
                            rect(8.0, 50.0, 377.0, 80.0),
                        )
                        .with_children(vec![
                            text("Messages, Bank: code 482913", rect(16.0, 60.0, 280.0, 40.0)),
                            node(Role::Button, "Dismiss", rect(300.0, 60.0, 80.0, 40.0)),
                        ]),
                    );
                }
                scr(HOME, " ", false, kids)
            }
            Front::Own => scr(
                OWN,
                "Jarvis",
                false,
                vec![
                    text(OWN_SECRET, rect(16.0, 100.0, 300.0, 30.0)),
                    node(Role::Button, "History", rect(16.0, 200.0, 120.0, 44.0)),
                ],
            ),
            Front::Picker(k, _) => {
                let (app, name, id, title, rows): (&str, &str, &str, &str, [&str; 2]) = match k {
                    0 => (
                        SHOP,
                        "Shop",
                        "ContactsListView",
                        "Contacts",
                        ["John Appleseed", "Kate Bell"],
                    ),
                    1 => (
                        MAIL,
                        "Mail",
                        "photosView_content_scroll_view",
                        "Photos",
                        ["Photo, 3 March", "Photo, 4 March"],
                    ),
                    _ => (
                        MAIL,
                        "Mail",
                        "FullDocumentManagerViewControllerNavigationBar",
                        "Recents",
                        ["Report.pdf", "Statement.pdf"],
                    ),
                };
                let mut kids = vec![
                    // The sheet presentation's backdrop and the picker's own identifier.
                    UiNode::new(
                        Role::Other("Other".into()),
                        rect(-393.0, -852.0, 1179.0, 2556.0),
                    ),
                    UiNode::new(Role::Other("Other".into()), rect(0.0, 0.0, 393.0, 852.0))
                        .with_identifier(id),
                    text(title, rect(150.0, 60.0, 120.0, 30.0)),
                    node(Role::Button, "Cancel", rect(16.0, 60.0, 80.0, 40.0)),
                    field(
                        "Search",
                        rect(16.0, 110.0, 361.0, 36.0),
                        &v("Search"),
                        f("Search"),
                    ),
                ];
                for (i, r) in rows.iter().enumerate() {
                    kids.push(node(
                        Role::Cell,
                        r,
                        rect(0.0, 170.0 + 60.0 * i as f64, 393.0, 56.0),
                    ));
                }
                scr(app, name, kb, kids)
            }
            Front::Surface(..) => {
                // The runner's reading: the application node only, nothing of the surface.
                let mut s = screen(
                    UNIDENTIFIED_APP_BUNDLE_ID,
                    "",
                    UiNode::new(Role::Application, IPHONE_SIZE.to_rect()),
                    false,
                );
                s.app_name = None;
                s
            }
            Front::SbMode(k) => real_trees()[usize::from(*k).min(2)].clone(),
            Front::Alert(_) => real_trees()[3].clone(),
        }
    }

    /// E6: the places change on their own (a redirect, another app in front, an alert, the
    /// picker gone). Returns whether anything changed. Focus stays where it is.
    pub fn force_change(&mut self, rng: &mut Rng) -> bool {
        match self.front.clone() {
            Front::Safari {
                host: Some(h),
                focused,
            } => {
                let next = if h == "bank.example" {
                    "news.example"
                } else {
                    "bank.example"
                };
                self.front = Front::Safari {
                    host: Some(next.into()),
                    focused,
                };
                true
            }
            Front::Shop => {
                if rng.chance(0.5) {
                    self.ad = !self.ad;
                    self.nested = false;
                } else {
                    self.front = Front::Msgs;
                }
                true
            }
            Front::Msgs => {
                self.front = Front::Shop;
                true
            }
            Front::Picker(_, back) => {
                self.front = *back;
                true
            }
            f if f.is_app() => {
                self.front = Front::Alert(Box::new(f));
                true
            }
            _ => false,
        }
    }

    /// E3, E4: system layers come and go on their own between looks.
    pub fn ext_drift(&mut self, rng: &mut Rng) {
        if !self.feat.any() {
            return;
        }
        match self.front.clone() {
            Front::Surface(_, under) if rng.chance(0.15) => self.front = *under,
            Front::SbMode(_) if rng.chance(0.1) => self.front = Front::Home,
            f => {
                if self.feat.system > 0.0 && rng.chance(self.feat.system) {
                    if f.is_app() {
                        self.front = Front::Surface(rng.below(3) as u8, Box::new(f));
                    } else if f == Front::Home {
                        self.front = if rng.chance(0.5) {
                            Front::Surface(rng.below(3) as u8, Box::new(f))
                        } else {
                            Front::SbMode(rng.below(3) as u8)
                        };
                    }
                } else if self.feat.alert > 0.0 && rng.chance(self.feat.alert) && f.is_app() {
                    self.front = Front::Alert(Box::new(f));
                }
            }
        }
    }

    fn label_at(&self, p: Point) -> Option<(String, Role)> {
        let s = self.screen();
        let c = compact(&s);
        c.elements
            .iter()
            .filter(|e| contains(&e.frame, p))
            .min_by(|a, b| a.frame.area().partial_cmp(&b.frame.area()).unwrap())
            .map(|e| (e.label.clone(), e.role.clone()))
    }

    fn go(&mut self, f: Front, rng: &mut Rng) {
        self.focus = None;
        if f == Front::Shop {
            self.ad = rng.chance(0.6);
            self.nested = self.ad && rng.chance(0.4);
        }
        if self.feat.cut > 0.0 {
            self.cut = f.is_app() && rng.chance(self.feat.cut);
            self.deep = self.cut && rng.chance(0.5);
        }
        self.front = f;
    }

    fn launch(&mut self, b: &str, rng: &mut Rng) {
        let f = match b.to_ascii_lowercase().as_str() {
            MSGS => Front::Msgs,
            SHOP => Front::Shop,
            SAFARI => Front::Safari {
                host: Some("news.example".into()),
                focused: false,
            },
            MAIL => Front::Inbox,
            OWN => Front::Own,
            _ => return,
        };
        self.go(f, rng);
    }

    pub fn act(&mut self, cmd: &RunnerCommand, rng: &mut Rng) -> RunnerResult {
        if self.feat.any() {
            // The runner's check next to the gesture (Home is a device button).
            let home = matches!(cmd, RunnerCommand::PressKey { key: Key::Home, .. });
            if let Some(e) = expect_of(cmd) {
                let front = self.screen().app_bundle_id;
                if !home && !e.matches(&front) {
                    return RunnerResult::stale(&front);
                }
            }
            let unident = matches!(self.front, Front::Surface(..) | Front::SbMode(_));
            if unident
                && (matches!(cmd, RunnerCommand::TypeText { .. })
                    || (matches!(cmd, RunnerCommand::PressKey { .. }) && !home))
            {
                return RunnerResult::err("typing is refused on an app Jarvis cannot identify");
            }
            match (self.front.clone(), cmd) {
                (
                    Front::Surface(_, under),
                    RunnerCommand::Tap { .. } | RunnerCommand::Swipe { .. },
                ) => {
                    if rng.chance(0.5) {
                        self.front = *under;
                    }
                    return RunnerResult::ok();
                }
                (Front::SbMode(_), RunnerCommand::Tap { .. }) if rng.chance(0.3) => {
                    self.front = Front::Home;
                    return RunnerResult::ok();
                }
                (Front::Home, RunnerCommand::Swipe { from, to, .. })
                    if self.feat.system > 0.0 && to.x < from.x - 40.0 && rng.chance(0.5) =>
                {
                    self.front = Front::SbMode(1);
                    return RunnerResult::ok();
                }
                (Front::SbMode(1), RunnerCommand::Swipe { from, to, .. })
                    if to.x > from.x + 40.0 =>
                {
                    self.front = Front::Home;
                    return RunnerResult::ok();
                }
                _ => {}
            }
        }
        self.act_base(cmd, rng);
        RunnerResult::ok()
    }

    fn act_base(&mut self, cmd: &RunnerCommand, rng: &mut Rng) {
        match cmd {
            RunnerCommand::Tap { x, y, .. } | RunnerCommand::LongPress { x, y, .. } => {
                let long = matches!(cmd, RunnerCommand::LongPress { .. });
                let Some((label, role)) = self.label_at(Point::new(*x, *y)) else {
                    return;
                };
                if matches!(role, Role::TextField) && self.feat.any() {
                    if self.feat.place_change > 0.0 && rng.chance(self.feat.place_change) {
                        self.change_next = true;
                    }
                    if self.feat.focus_fail > 0.0 && rng.chance(self.feat.focus_fail) {
                        self.ev.focus_fail += 1;
                        return;
                    }
                }
                if matches!(role, Role::TextField) {
                    self.focus = Some(label.clone());
                    if label == "Address" {
                        if let Front::Safari { focused, .. } = &mut self.front {
                            *focused = true;
                        }
                    }
                    return;
                }
                if long {
                    if self.feat.system > 0.0 && self.front == Front::Home && rng.chance(0.5) {
                        self.front = Front::SbMode(0);
                    }
                    return;
                }
                let cur_host = match &self.front {
                    Front::Safari { host, .. } => host.clone(),
                    _ => None,
                };
                match (self.front.clone(), label.as_str()) {
                    (Front::Msgs, "shop.example/daily") => self.go(Front::Shop, rng),
                    (Front::Msgs, "bank.example/login") => self.go(
                        Front::Safari {
                            host: Some("bank.example".into()),
                            focused: false,
                        },
                        rng,
                    ),
                    (Front::Msgs, "Send") => {
                        self.values.remove("Message");
                    }
                    (Front::Shop, "Claim") => self.go(
                        Front::Safari {
                            host: Some("news.example".into()),
                            focused: false,
                        },
                        rng,
                    ),
                    (Front::Safari { .. }, "Other site") => {
                        self.back.push(cur_host.clone());
                        let next = if cur_host.as_deref() == Some("bank.example") {
                            "news.example"
                        } else {
                            "bank.example"
                        };
                        self.go(
                            Front::Safari {
                                host: Some(next.into()),
                                focused: false,
                            },
                            rng,
                        );
                    }
                    (Front::Safari { .. }, "Back") => {
                        let h = self.back.pop().unwrap_or(Some("news.example".into()));
                        self.go(
                            Front::Safari {
                                host: h,
                                focused: false,
                            },
                            rng,
                        );
                    }
                    (Front::Safari { .. }, "Tabs") => {
                        // A tab switcher shows a page whose address is not readable.
                        self.go(
                            Front::Safari {
                                host: None,
                                focused: false,
                            },
                            rng,
                        );
                    }
                    (Front::Inbox, "Bank statement") => self.go(Front::Message(0), rng),
                    (Front::Inbox, "Promo from Shop") => self.go(Front::Message(1), rng),
                    (Front::Message(_), "Inbox") => self.go(Front::Inbox, rng),
                    (Front::Message(_), "View statement") => self.go(
                        Front::Safari {
                            host: Some("bank.example".into()),
                            focused: false,
                        },
                        rng,
                    ),
                    (Front::Shop, "Share with a contact") => {
                        self.go(Front::Picker(0, Box::new(Front::Shop)), rng);
                    }
                    (Front::Message(w), "Attach") => {
                        let k = 1 + rng.below(2) as u8;
                        self.go(Front::Picker(k, Box::new(Front::Message(w))), rng);
                    }
                    (Front::Picker(k, back), l) if l != "Search" => {
                        self.focus = None;
                        if k == 0 && l != "Cancel" {
                            self.values.insert("Promo code".into(), l.to_owned());
                        }
                        self.front = *back;
                    }
                    (Front::Alert(under), "Allow" | "Don\u{2019}t Allow") => {
                        self.front = *under;
                    }
                    (Front::Home, "Dismiss") => self.banner = false,
                    (Front::Home, "Mystery") => self.go(Front::Shop, rng),
                    (Front::Home, l) => {
                        if let Some(b) = known_app_label(l) {
                            self.launch(&b, rng);
                        }
                    }
                    _ => {
                        if let Front::Safari { focused, .. } = &mut self.front {
                            *focused = false;
                        }
                        self.focus = None;
                    }
                }
            }
            RunnerCommand::TypeText { text, .. } => {
                if let Some(f) = &self.focus {
                    let v = self.values.entry(f.clone()).or_default();
                    v.push_str(text);
                }
            }
            RunnerCommand::PressKey { key, .. } => match key {
                Key::Home => {
                    self.banner = rng.chance(0.4);
                    self.go(Front::Home, rng);
                }
                Key::Delete => {
                    if let Some(f) = &self.focus {
                        if let Some(v) = self.values.get_mut(f) {
                            v.pop();
                        }
                    }
                }
                Key::Return => {
                    if let (Front::Safari { .. }, Some("Address")) =
                        (&self.front, self.focus.as_deref())
                    {
                        let t = self.values.remove("Address").unwrap_or_default();
                        let h = ascii_host(&t);
                        self.go(
                            Front::Safari {
                                host: h,
                                focused: false,
                            },
                            rng,
                        );
                    } else {
                        self.focus = None;
                    }
                }
                _ => {}
            },
            RunnerCommand::LaunchApp { bundle_id } => self.launch(bundle_id, rng),
            RunnerCommand::OpenUrl { url } => {
                let h = url_host(url);
                self.go(
                    Front::Safari {
                        host: h,
                        focused: false,
                    },
                    rng,
                );
            }
            RunnerCommand::Swipe { .. } | RunnerCommand::Wait { .. } => {}
        }
    }

    /// The phone changes on its own between looks (a countdown; an ad that loads or goes; a
    /// page that redirects).
    pub fn drift(&mut self, rng: &mut Rng, p: f64) {
        if !rng.chance(p) {
            return;
        }
        // The countdown advances; sometimes a place comes or goes as well.
        self.tick += 1;
        if rng.chance(0.5) {
            return;
        }
        match &mut self.front {
            Front::Shop => {
                self.ad = !self.ad;
                self.nested = self.ad && rng.chance(0.3);
            }
            Front::Safari { host, .. } => {
                *host = match host.as_deref() {
                    Some("bank.example") => Some("news.example".into()),
                    Some(_) => Some("bank.example".into()),
                    None => None,
                };
            }
            Front::Home => self.banner = !self.banner,
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------------------
// Config and the model.
// ---------------------------------------------------------------------------------------

pub fn cfg(vision: bool, confirm_nothing: bool) -> SessionConfig {
    let mut policy = Policy {
        first_use_approval: false,
        max_steps: 45,
        max_consecutive_failures: 10,
        ..Policy::default()
    };
    if confirm_nothing {
        policy.confirm = Vec::new();
    }
    SessionConfig {
        policy,
        tools: vec![
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
            },
            ToolSpec {
                name: "clock".into(),
                description: "The time".into(),
                params: Vec::new(),
                tier: Tier::Native,
                risk: RiskCategory::None,
                app: None,
                self_contained: true,
            },
        ],
        model: ModelProfile {
            supports_vision: vision,
            ..ModelProfile::default()
        },
        known_apps: vec![
            AppInfo::new("com.apple.MobileSMS", "Messages"),
            AppInfo::new(SHOP, "Shop"),
            AppInfo::new(SAFARI, "Safari"),
            AppInfo::new(MAIL, "Mail"),
            AppInfo::new("com.apple.Preferences", "Settings"),
        ],
        agent_bundle_ids: vec![OWN.into()],
        drive_own_app: false,
        skill: None,
    }
}

pub fn gen_step(rng: &mut Rng, s: &Screen, vision: bool) -> A {
    let c = compact(s);
    let els: Vec<u32> = c.elements.iter().map(|e| e.index).collect();
    let fields: Vec<u32> = c
        .elements
        .iter()
        .filter(|e| matches!(e.role, Role::TextField))
        .map(|e| e.index)
        .collect();
    let any = |rng: &mut Rng| -> Option<u32> {
        if els.is_empty() {
            None
        } else {
            Some(*rng.pick(&els))
        }
    };
    let dirs = [
        Direction::Up,
        Direction::Down,
        Direction::Left,
        Direction::Right,
    ];
    let r = rng.below(100);
    match r {
        0..=21 => any(rng).map_or(A::Wait(1000), A::Tap),
        22..=24 => any(rng).map_or(A::Look, A::LongPress),
        25..=33 => {
            let text = if rng.chance(0.6) {
                "hello world".to_owned()
            } else {
                rng.pick(&["482913", "balance 12480", "bank.example"])
                    .to_string()
            };
            let submit = rng.chance(0.3);
            if fields.is_empty() || rng.chance(0.15) {
                A::Type {
                    el: None,
                    text,
                    submit,
                }
            } else {
                A::Type {
                    el: Some(*rng.pick(&fields)),
                    text,
                    submit,
                }
            }
        }
        34..=35 => {
            if fields.is_empty() {
                A::Wait(1000)
            } else {
                A::Clear(*rng.pick(&fields))
            }
        }
        36..=47 => A::Scroll {
            dir: *rng.pick(&[
                Direction::Down,
                Direction::Down,
                Direction::Up,
                Direction::Left,
            ]),
            el: if rng.chance(0.25) { any(rng) } else { None },
        },
        48..=51 => A::Swipe {
            dir: *rng.pick(&dirs),
            el: if rng.chance(0.5) { any(rng) } else { None },
        },
        52..=55 => {
            if vision {
                A::TapPoint(
                    0.05 + rng.below(90) as f64 / 100.0,
                    0.08 + rng.below(80) as f64 / 100.0,
                )
            } else {
                any(rng).map_or(A::Wait(1000), A::Tap)
            }
        }
        56..=59 => A::Home,
        60..=66 => A::OpenApp(
            rng.pick(&["Shop", "Messages", "Safari", "Mail"])
                .to_string(),
        ),
        67..=71 => A::OpenUrl(
            rng.pick(&[
                "https://bank.example",
                "https://news.example",
                "https://evil.example/c?482913",
            ])
            .to_string(),
        ),
        72..=76 => A::Tool(rng.pick(&["lookup", "lookup", "clock"]).to_string()),
        77..=83 => A::Wait(*rng.pick(&[500, 1000, 2500, 5000])),
        84..=88 => A::Look,
        89..=90 => A::Ask,
        91..=93 => A::Invalid,
        94 if rng.chance(0.25) => A::Done,
        95 if rng.chance(0.25) => A::Fail,
        94 | 95 => A::Look,
        _ => any(rng).map_or(A::Look, A::Tap),
    }
}

// ---------------------------------------------------------------------------------------
// The driver and the checks.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    ApproveAll,
    DeclineAll,
    Random,
    ApproveThenDecline(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prev {
    Start,
    Sent,
    Tool,
    Approved,
    Declined,
    Answered,
    Unlocked,
    Model,
    Other,
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    pub tasks: usize,
    pub steps: usize,
    pub sent: usize,
    pub flow_questions: usize,
    pub declined: usize,
    /// check id → messages
    pub mismatches: BTreeMap<String, Vec<String>>,
    /// flow questions per approve-all task, with its shape
    pub approve_all_load: Vec<(usize, String)>,
    pub over_asks: usize,
    pub statuses: BTreeMap<String, usize>,
    /// Observations taken while the oracle's restricted state held.
    pub restricted_looks: usize,
    /// Sends checked against the oracle (commands and tool calls of model steps).
    pub sends_checked: usize,
    /// Flow questions whose names were checked.
    pub questions_checked: usize,
    /// Model steps the oracle said must ask.
    pub must_ask: usize,
    /// Per owner policy: (tasks, steps, tasks with at least 30 steps).
    pub by_owner: BTreeMap<String, (usize, usize, usize)>,
    /// Every flagged event per check (the examples above are one per task).
    pub totals: BTreeMap<String, usize>,
    /// Approve-all tasks: per step kind, (steps, flow questions asked for them).
    pub by_kind: BTreeMap<String, (usize, usize)>,
    /// Extended checker: tasks in which each feature occurred.
    pub feature_tasks: BTreeMap<String, usize>,
    /// Approve-all tasks: (flow questions, all questions, some feature occurred).
    pub ext_loads: Vec<(usize, usize, bool)>,
    /// Extended counters.
    pub ext_counts: BTreeMap<String, usize>,
}

impl Report {
    pub fn flag(&mut self, check: &str, msg: String) {
        *self.totals.entry(check.into()).or_default() += 1;
        let v = self.mismatches.entry(check.into()).or_default();
        let seed = msg.split(' ').nth(1).unwrap_or("").to_owned();
        if v.iter().any(|m| m.split(' ').nth(1) == Some(seed.as_str())) {
            return;
        }
        if v.len() < 12 {
            v.push(msg);
        } else if v.len() == 12 {
            v.push("…".into());
        }
    }
    pub fn count(&self, check: &str) -> usize {
        self.mismatches.get(check).map_or(0, Vec::len)
    }
}

pub struct Task {
    pub seed: u64,
    pub start: Front,
    pub vision: bool,
    pub confirm_nothing: bool,
    pub owner: Owner,
    pub slow: f64,
    pub drift: f64,
    pub lock: f64,
    pub feat: Features,
}

impl Task {
    pub fn random(seed: u64) -> Task {
        let mut r = Rng::new(seed ^ 0xA5A5);
        let start = match r.below(7) {
            0 => Front::Msgs,
            1 => Front::Home,
            2 => Front::Shop,
            3 => Front::Safari {
                host: Some("bank.example".into()),
                focused: false,
            },
            4 => Front::Inbox,
            5 => Front::Message(0),
            _ => Front::Own,
        };
        let owner = match r.below(4) {
            0 => Owner::ApproveAll,
            1 => Owner::DeclineAll,
            2 => Owner::Random,
            _ => Owner::ApproveThenDecline(2 + r.below(14) as u32),
        };
        Task {
            seed,
            start,
            vision: r.chance(0.5),
            confirm_nothing: r.chance(0.5),
            owner,
            slow: *r.pick(&[0.0, 0.3, 1.0]),
            drift: *r.pick(&[0.0, 0.05, 0.2]),
            lock: *r.pick(&[0.0, 0.0, 0.02]),
            feat: Features::default(),
        }
    }

    /// A task of the extended checker: the base task of the seed, its features drawn from
    /// a separate stream (each on in about half the tasks), and sometimes a start on a new
    /// kind of screen.
    pub fn random_ext(seed: u64) -> Task {
        let mut t = Task::random(seed);
        let mut r = Rng::new(seed ^ 0x0E47_E4DE);
        // A quarter of the tasks are the base generator's, as the control group.
        if r.chance(0.25) {
            return t;
        }
        let on = |r: &mut Rng, v: f64| if r.chance(0.5) { v } else { 0.0 };
        t.feat = Features {
            picker: on(&mut r, 1.0),
            cut: on(&mut r, 0.3),
            system: on(&mut r, 0.03),
            alert: on(&mut r, 0.03),
            focus_fail: on(&mut r, 0.5),
            place_change: on(&mut r, 0.35),
        };
        if !t.feat.any() {
            t.feat.focus_fail = 0.5;
        }
        if r.chance(0.2) {
            let mut opts = Vec::new();
            if t.feat.picker > 0.0 {
                opts.push(Front::Picker(0, Box::new(Front::Shop)));
                opts.push(Front::Picker(1, Box::new(Front::Message(0))));
            }
            if t.feat.system > 0.0 {
                opts.push(Front::SbMode(r.below(3) as u8));
                opts.push(Front::Surface(r.below(3) as u8, Box::new(Front::Inbox)));
            }
            if t.feat.alert > 0.0 {
                opts.push(Front::Alert(Box::new(Front::Shop)));
            }
            if !opts.is_empty() {
                t.start = r.pick(&opts).clone();
            }
        }
        t
    }
}

fn short(p: &BTreeSet<P>) -> String {
    p.iter()
        .map(|x| format!("{x:?}"))
        .collect::<Vec<_>>()
        .join("|")
}

struct Log {
    v: Vec<String>,
    live: bool,
}
impl Log {
    fn push(&mut self, l: String) {
        if self.live {
            eprintln!("  | {}", l.chars().take(900).collect::<String>());
        }
        self.v.push(l);
    }
    fn iter(&self) -> std::slice::Iter<'_, String> {
        self.v.iter()
    }
}

/// Runs one task; returns its log on a mismatch.
pub fn run_task(t: &Task, rep: &mut Report, verbose: bool) {
    let mut rng = Rng::new(t.seed);
    let nested = rng.chance(0.4);
    let banner = rng.chance(0.5);
    let mut phone = Phone::new(t.start.clone(), nested, banner, t.feat);
    let config = cfg(t.vision, t.confirm_nothing);
    let mut s = Session::new(config, TaskSpec::new("t", TASK), MemoryStore::new());
    let mut o = Oracle::default();
    let mut now = 1_000u64;
    let mut event = Event::Start;
    let mut prev = Prev::Start;
    let mut log = Log {
        v: Vec::new(),
        live: verbose,
    };
    let mut last_obs: Option<Screen> = None;
    let mut since_infer: Vec<Screen> = Vec::new();
    let mut tool_results: Vec<String> = Vec::new();
    let mut cur: Option<A> = None;
    let mut step_screen: Option<Screen> = None;
    let mut window_ask = false;
    let mut flows = 0usize;
    let mut decisions = 0u32;
    let mut asked_keys: BTreeMap<String, usize> = BTreeMap::new();
    let mut shape: Vec<&'static str> = Vec::new();
    let mut pending_screenshot = false;
    let mut known: Vec<String> = Vec::new();
    known.extend(names_of(&P::App(OWN.into())));
    for b in [MSGS, SHOP, SAFARI, MAIL] {
        for p in [
            P::App(b.into()),
            P::Web(b.into()),
            P::Nested(b.into()),
            P::Unreadable(b.into()),
        ] {
            known.extend(names_of(&p));
        }
    }
    for p in [
        P::Site("bank.example".into()),
        P::Site("news.example".into()),
        P::Site("evil.example".into()),
        P::Pic,
        P::Home,
        P::Tool("lookup".into()),
        P::Tool("clock".into()),
    ] {
        known.extend(names_of(&p));
    }
    if t.feat.any() {
        for p in ext_places() {
            known.extend(names_of(&p));
        }
    }
    let name_to_place = |n: &str| -> Option<P> {
        let mut cands = vec![
            P::Site("bank.example".into()),
            P::Site("news.example".into()),
            P::Site("evil.example".into()),
            P::Pic,
            P::Home,
            P::Tool("lookup".into()),
            P::Tool("clock".into()),
        ];
        cands.push(P::App(OWN.into()));
        if t.feat.any() {
            cands.extend(ext_places());
        }
        for b in [MSGS, SHOP, SAFARI, MAIL] {
            cands.extend([
                P::App(b.into()),
                P::Web(b.into()),
                P::Nested(b.into()),
                P::Unreadable(b.into()),
            ]);
        }
        cands
            .into_iter()
            .find(|p| names_of(p).iter().any(|x| x == n))
    };
    let tag = format!("seed {} ({:?}, owner {:?})", t.seed, t.start, t.owner);
    rep.tasks += 1;
    let mut finished = false;
    let mut x = ExtState::default();
    // Triage (oracle fix): an approved model `look` takes the look after the approval and
    // then the look itself ("a model look the owner approved is taken").
    let mut approved_look = false;
    for _ in 0..4_000 {
        let effect = s.step(TimedEvent::new(now, event));
        let before = prev;
        prev = Prev::Other;
        // --- checks that depend on the effect, before replying ---
        let restricted = last_obs.as_ref().is_some_and(|sc| o.restricted(sc));
        // X7: still restricted when the current action's own approvals count.
        let restricted_scoped = last_obs
            .as_ref()
            .is_some_and(|sc| o.restricted_in_scope(sc));
        let is_send = matches!(
            effect,
            Effect::Act(_) | Effect::CallTool(_) | Effect::Observe { .. }
        );
        if window_ask && is_send {
            rep.flag(
                "2b-quiet-before-question",
                format!("{tag}: {:?} between a gated step's proposal and its question; step {:?}; log {:?}", effect.name(), cur, log.iter().rev().take(8).collect::<Vec<_>>()),
            );
        }
        if matches!(
            effect,
            Effect::Confirm(_)
                | Effect::Infer(_)
                | Effect::Finished(_)
                | Effect::AskUser { .. }
                | Effect::AwaitUnlock { .. }
        ) {
            window_ask = false;
        }
        if before == Prev::Declined && is_send {
            rep.flag(
                "2c-quiet-after-decline",
                format!("{tag}: {} after a decline; step {:?}", effect.name(), cur),
            );
        }
        if let Effect::Observe { screenshot } = &effect {
            if t.feat.any() && x.after_settle && restricted {
                // E6b (and X5 for a typing's focus retry): the settle look is the session's
                // own and goes through the R24 gate.
                let check = if !restricted_scoped {
                    "X7-settle-look-inside-an-approved-action"
                } else if matches!(cur, Some(A::Type { .. } | A::Clear(_))) {
                    "S-focus-settle-look-while-restricted"
                } else {
                    "S-settle-look-while-restricted"
                };
                rep.flag(
                    check,
                    format!(
                        "{tag}: oracle: no settle look while a place on {} may not see a look (read {}); session: looked; step {:?}; trace {:?}",
                        last_obs.as_ref().map_or(String::new(), |s| s.app_bundle_id.clone()),
                        short(&o.read),
                        cur,
                        log.iter().rev().take(8).collect::<Vec<_>>()
                    ),
                );
            }
            let second_look = approved_look && before == Prev::Other;
            if second_look {
                approved_look = false;
            }
            if restricted
                && !second_look
                && !matches!(
                    before,
                    Prev::Start
                        | Prev::Sent
                        | Prev::Tool
                        | Prev::Approved
                        | Prev::Answered
                        | Prev::Unlocked
                )
            {
                rep.flag(
                    if restricted_scoped {
                        "2a-look-while-restricted"
                    } else {
                        "X7-look-inside-an-approved-action"
                    },
                    format!("{tag}: look (screenshot {screenshot}) after {before:?} on {} while restricted; step {:?}; read {}; log {:?}",
                        last_obs.as_ref().map_or(String::new(), |s| s.app_bundle_id.clone()), cur, short(&o.read), log.iter().rev().take(6).collect::<Vec<_>>()),
                );
            }
            pending_screenshot = *screenshot;
            if restricted {
                rep.restricted_looks += 1;
            }
        }
        let check_send = |o: &Oracle,
                          rep: &mut Report,
                          a: &A,
                          point: Option<Point>,
                          what: String,
                          sc: &Screen| {
            rep.sends_checked += 1;
            let need = o.need(a, sc, point);
            let uns = o.unsatisfied(&need);
            let blk = o.blocked_in(&need);
            if !uns.is_empty() {
                rep.flag(
                    "1-unasked-flow",
                    format!(
                        "{tag}: {what} for {a:?} on {} with unsatisfied {:?}; grants {:?}",
                        sc.app_bundle_id, uns, o.grants
                    ),
                );
            }
            if !blk.is_empty() {
                rep.flag(
                    "1-blocked-flow-sent",
                    format!("{tag}: {what} for {a:?} with blocked {blk:?}"),
                );
            }
            let over: Vec<_> = need
                .pairs
                .iter()
                .filter(|(d, s, _)| o.blocked.contains(&(d.clone(), s.clone())))
                .collect();
            if !over.is_empty() {
                rep.flag(
                    "G2-decline-over-earlier-grant",
                    format!(
                        "{tag}: {what} for {a:?}: declined earlier but granted before: {over:?}"
                    ),
                );
            }
        };
        match &effect {
            Effect::Act(cmd) => {
                rep.sent += 1;
                log.push(format!("act {cmd:?}"));
                if matches!(cmd, RunnerCommand::PressKey { key: Key::Home, .. }) {
                    x.home_sent = true;
                }
                let core_wait =
                    matches!(cmd, RunnerCommand::Wait { ms } if *ms == 400 || *ms == 500);
                let activation = matches!(cmd, RunnerCommand::LaunchApp { .. })
                    && !matches!(cur, Some(A::OpenApp(_)))
                    && !matches!(cur, Some(A::Tap(_)) if last_obs.as_ref().is_some_and(|s| s.app_bundle_id == HOME));
                if let (Some(a), Some(sc)) = (&cur, &last_obs) {
                    if !core_wait && !activation {
                        let point = match cmd {
                            RunnerCommand::Tap { x, y, .. }
                            | RunnerCommand::LongPress { x, y, .. } => Some(Point::new(*x, *y)),
                            RunnerCommand::Swipe { from, .. } => Some(*from),
                            _ => None,
                        };
                        let ret = matches!(
                            cmd,
                            RunnerCommand::PressKey {
                                key: Key::Return,
                                ..
                            }
                        );
                        let decided = match (&step_screen, ret) {
                            (Some(d), false) => d.clone(),
                            _ => {
                                step_screen = Some(sc.clone());
                                sc.clone()
                            }
                        };
                        check_send(&o, rep, a, point, format!("{cmd:?}"), &decided);
                        if t.feat.any() {
                            ext_on_act(t, rep, &tag, a, cmd, &decided, sc, &mut x, &log);
                        }
                        if decided != *sc {
                            let late = o.unsatisfied(&o.need(a, sc, point));
                            let early = o.unsatisfied(&o.need(a, &decided, point));
                            if late.iter().any(|x| !early.contains(x)) {
                                rep.flag("gap-mid-step-new-place", format!("{tag}: {cmd:?} for {a:?}: a place came on screen during the step: {late:?}"));
                            }
                        }
                        let info = places_of(sc, false);
                        if info.own
                            && !matches!(
                                cmd,
                                RunnerCommand::LaunchApp { .. }
                                    | RunnerCommand::OpenUrl { .. }
                                    | RunnerCommand::PressKey { key: Key::Home, .. }
                                    | RunnerCommand::Wait { .. }
                            )
                        {
                            rep.flag(
                                "4-own-app-touched",
                                format!("{tag}: {cmd:?} in Jarvis's own app"),
                            );
                        }
                    }
                } else if !core_wait && !activation {
                    rep.flag(
                        "1-command-without-step",
                        format!("{tag}: {cmd:?} with no model step"),
                    );
                }
            }
            Effect::CallTool(call) => {
                rep.sent += 1;
                x.after_settle = false;
                log.push(format!("tool {}", call.name));
                if let (Some(a), Some(sc)) = (&cur, &last_obs) {
                    check_send(&o, rep, a, None, format!("tool {}", call.name), sc);
                }
            }
            Effect::Infer(req) => {
                // The read set grows by what this prompt carries.
                for sc in since_infer.drain(..) {
                    let info = places_of(&sc, false);
                    o.read.extend(info.read.iter().cloned());
                }
                for tname in tool_results.drain(..) {
                    o.read.insert(P::Tool(tname));
                }
                if req.image.is_some() {
                    o.read.insert(P::Pic);
                    if last_obs.as_ref().is_some_and(|s| places_of(s, false).home) {
                        rep.flag(
                            "4-home-screenshot",
                            format!("{tag}: a screenshot of the Home Screen given to the model"),
                        );
                    }
                }
                if req.user.contains(OWN_SECRET) {
                    rep.flag(
                        "4-own-app-read",
                        format!("{tag}: Jarvis's own app's text in a prompt"),
                    );
                }
            }
            Effect::Confirm(c) if c.summary.contains(FLOW) => {
                flows += 1;
                rep.flow_questions += 1;
                if t.owner == Owner::ApproveAll {
                    if let Some(a) = &cur {
                        rep.by_kind.entry(a.kind_name().into()).or_default().1 += 1;
                    }
                }
            }
            _ => {}
        }
        if let Effect::Confirm(c) = &effect {
            x.confirms += 1;
            if t.feat.any() {
                ext_on_confirm(rep, &tag, c, &cur, &mut x, &log);
            }
        }
        if !matches!(effect, Effect::Observe { .. }) {
            x.after_settle = matches!(
                &effect,
                Effect::Act(RunnerCommand::Wait { ms }) if *ms == VERIFY_SETTLE_MS
            );
        }
        // --- reply ---
        let inject_lock = t.lock > 0.0
            && !matches!(effect, Effect::Finished(_) | Effect::AwaitUnlock { .. })
            && rng.chance(t.lock);
        if inject_lock {
            x.locked = true;
        }
        event = match effect {
            Effect::Observe { screenshot } => {
                now += 60;
                if t.feat.any() {
                    if phone.change_next {
                        phone.change_next = false;
                        if phone.force_change(&mut rng) {
                            phone.ev.change_after_focus += 1;
                        }
                    } else if x.after_settle
                        && phone.feat.place_change > 0.0
                        && rng.chance(0.5)
                        && phone.force_change(&mut rng)
                    {
                        phone.ev.change_at_settle += 1;
                    }
                    phone.ext_drift(&mut rng);
                }
                x.after_settle = false;
                phone.drift(&mut rng, t.drift);
                let sc = phone.screen();
                if t.feat.any() && !inject_lock {
                    let i = places_of(&sc, false);
                    if i.picker.is_some() {
                        x.seen.insert("pickers");
                    }
                    if i.incomplete {
                        x.seen.insert("incomplete trees");
                    }
                    if i.unident {
                        x.seen.insert("SpringBoard modes and system surfaces");
                    }
                    if i.alert {
                        x.seen.insert("system alerts");
                    }
                    if x.decided.as_ref().is_some_and(|d| *d != sc) {
                        x.changed = true;
                    }
                }
                if inject_lock {
                    Event::DeviceLocked
                } else {
                    last_obs = Some(sc.clone());
                    since_infer.push(sc.clone());
                    log.push(format!("observe {} shot={screenshot}", sc.app_bundle_id));
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
            }
            Effect::Infer(_) => {
                let sc = last_obs.clone().unwrap_or_else(|| phone.screen());
                let a = gen_step(&mut rng, &sc, t.vision);
                let lat = if rng.chance(t.slow) { 2_500 } else { 200 };
                now += lat;
                rep.steps += 1;
                shape.push(a.kind_name());
                if t.owner == Owner::ApproveAll {
                    rep.by_kind.entry(a.kind_name().into()).or_default().0 += 1;
                }
                log.push(format!(
                    "model {} ({lat} ms) on {}",
                    a.json(),
                    sc.app_bundle_id
                ));
                if inject_lock {
                    Event::DeviceLocked
                } else {
                    // Decide what the oracle expects of this step on the screen in hand.
                    if a.gated() {
                        let need = o.need(&a, &sc, None);
                        let uns = o.unsatisfied(&need);
                        let blk = o.blocked_in(&need);
                        window_ask = !uns.is_empty() || !blk.is_empty();
                        if window_ask {
                            rep.must_ask += 1;
                        }
                    } else {
                        window_ask = false;
                    }
                    o.scope.clear();
                    approved_look = false;
                    if t.feat.any() {
                        ext_end_step(rep, &tag, &mut x, &log);
                        x.begin_step(&a, &sc, &o);
                    }
                    step_screen = None;
                    cur = if a.gated() { Some(a.clone()) } else { None };
                    prev = Prev::Model;
                    let _ = pending_screenshot;
                    Event::ModelOutput(ModelOutput { text: a.json() })
                }
            }
            Effect::Act(cmd) => {
                now += 60;
                let res = phone.act(&cmd, &mut rng);
                if !res.ok {
                    x.stale += 1;
                }
                prev = Prev::Sent;
                if inject_lock {
                    Event::DeviceLocked
                } else {
                    Event::ActResult(res)
                }
            }
            Effect::CallTool(call) => {
                now += 60;
                prev = Prev::Tool;
                tool_results.push(call.name.clone());
                Event::ToolResult(ToolResult {
                    ok: true,
                    output: "result 42".into(),
                    error: None,
                })
            }
            Effect::Confirm(c) => {
                now += 1_500;
                if inject_lock {
                    Event::DeviceLocked
                } else {
                    decisions += 1;
                    let approved = match t.owner {
                        Owner::ApproveAll => true,
                        Owner::DeclineAll => false,
                        Owner::Random => rng.chance(0.8),
                        Owner::ApproveThenDecline(k) => decisions <= k,
                    };
                    log.push(format!(
                        "confirm {:?} approved={approved}: {}",
                        c.category, c.summary
                    ));
                    on_question(
                        &c,
                        approved,
                        &mut o,
                        &cur,
                        last_obs.as_ref(),
                        rep,
                        &tag,
                        &known,
                        &name_to_place,
                        &mut asked_keys,
                        t.owner == Owner::ApproveAll,
                    );
                    step_screen = None;
                    approved_look = approved && matches!(cur, Some(A::Look));
                    prev = if approved {
                        Prev::Approved
                    } else {
                        Prev::Declined
                    };
                    if !approved {
                        rep.declined += 1;
                    }
                    Event::phone_decision(approved)
                }
            }
            Effect::AskUser { .. } => {
                now += 1_500;
                prev = Prev::Answered;
                Event::UserAnswer { text: "yes".into() }
            }
            Effect::AwaitUnlock { .. } => {
                now += 1_000;
                prev = Prev::Unlocked;
                Event::DeviceUnlocked
            }
            Effect::Finished(out) => {
                *rep.statuses.entry(format!("{:?}", out.status)).or_default() += 1;
                if t.owner == Owner::ApproveAll {
                    *rep.statuses
                        .entry(format!("approve-all {:?}", out.status))
                        .or_default() += 1;
                    if verbose || std::env::var("ORACLE26_ENDS").is_ok() {
                        eprintln!("end {tag}: {:?} {}", out.status, out.summary);
                    }
                }
                finished = true;
                if t.feat.any() {
                    ext_end_step(rep, &tag, &mut x, &log);
                }
                if verbose {
                    eprintln!("{tag}: {:?} {}", out.status, out.summary);
                }
                break;
            }
        };
    }
    if !finished {
        rep.flag(
            "5-liveness",
            format!("{tag}: did not finish in 4000 effects"),
        );
    }
    {
        let key = match t.owner {
            Owner::ApproveAll => "approve all",
            Owner::DeclineAll => "decline all",
            Owner::Random => "random",
            Owner::ApproveThenDecline(_) => "approve then decline",
        };
        let e = rep.by_owner.entry(key.into()).or_default();
        e.0 += 1;
        e.1 += shape.len();
        if shape.len() >= 30 {
            e.2 += 1;
        }
    }
    if phone.ev.focus_fail > 0 {
        x.seen.insert("focus failures");
    }
    if phone.ev.change_after_focus + phone.ev.change_at_settle > 0 {
        x.seen.insert("place changes");
    }
    for f in &x.seen {
        *rep.feature_tasks.entry((*f).into()).or_default() += 1;
    }
    for (k, v) in [
        ("focus failures", phone.ev.focus_fail),
        (
            "place changes after a focusing tap",
            phone.ev.change_after_focus,
        ),
        ("place changes at a settle look", phone.ev.change_at_settle),
        ("commands the runner refused", x.stale),
        ("questions (all)", x.confirms),
    ] {
        *rep.ext_counts.entry(k.into()).or_default() += v;
    }
    if t.owner == Owner::ApproveAll {
        rep.ext_loads.push((flows, x.confirms, !x.seen.is_empty()));
    }
    if t.owner == Owner::ApproveAll {
        let mut sh: Vec<&str> = shape.clone();
        sh.dedup();
        rep.approve_all_load.push((
            flows,
            format!("{tag}: start {:?}, steps {}", t.start, shape.len()),
        ));
    }
    let _ = OutcomeStatus::Success;
}

#[allow(clippy::too_many_arguments)]
fn on_question(
    c: &ConfirmRequest,
    approved: bool,
    o: &mut Oracle,
    cur: &Option<A>,
    sc: Option<&Screen>,
    rep: &mut Report,
    tag: &str,
    known: &[String],
    name_to_place: &dyn Fn(&str) -> Option<P>,
    asked: &mut BTreeMap<String, usize>,
    approve_all: bool,
) {
    let (Some(a), Some(sc)) = (cur, sc) else {
        if c.summary.contains(FLOW) {
            rep.flag(
                "3-flow-question-without-step",
                format!("{tag}: {}", c.summary),
            );
        }
        return;
    };
    let need = o.need(a, sc, None);
    let uns = o.unsatisfied(&need);
    if !c.summary.contains(FLOW) {
        return;
    }
    if !c.phone_only {
        rep.flag("3-flow-not-phone-only", format!("{tag}: {}", c.summary));
    }
    let Some(p) = parse_flow(&c.summary, known) else {
        rep.flag("3-unparsed", format!("{tag}: {}", c.summary));
        return;
    };
    rep.questions_checked += 1;
    let mut srcs = BTreeSet::new();
    for n in &p.sources {
        match name_to_place(n) {
            Some(pl) => {
                srcs.insert(pl);
            }
            None => rep.flag(
                "3-unknown-source-name",
                format!("{tag}: {n:?} in {}", c.summary),
            ),
        }
    }
    // L1 (fixed when this review landed): the recorder of every question about a long press
    // on a known app's icon is gone; the strict reading in `own_dests` checks the case (a
    // long press resolved as a launch again shows as `1-unasked-flow` at the launch after).
    let mut own_named = BTreeSet::new();
    let mut obs_named = BTreeSet::new();
    if p.here {
        if need.own.len() == 1 {
            own_named.extend(need.own.iter().cloned());
        } else if need.own.len() > 1 {
            rep.flag(
                "3-here-with-several-destinations",
                format!("{tag}: {a:?} own {:?}: {}", need.own, c.summary),
            );
            own_named.extend(need.own.iter().cloned());
        }
    }
    for n in &p.into {
        match name_to_place(n) {
            Some(pl) => {
                own_named.insert(pl);
            }
            None if need.own.contains(&P::Unknown) => {
                own_named.insert(P::Unknown);
            }
            None => rep.flag(
                "3-unknown-dest-name",
                format!("{tag}: {n:?} in {}", c.summary),
            ),
        }
    }
    for n in &p.to {
        match name_to_place(n) {
            Some(pl) => {
                obs_named.insert(pl);
            }
            None => rep.flag(
                "3-unknown-dest-name",
                format!("{tag}: {n:?} in {}", c.summary),
            ),
        }
    }
    if own_named.remove(&P::App(OWN.into())) | obs_named.remove(&P::App(OWN.into())) {
        rep.flag(
            "L2-own-app-named-as-destination",
            format!("{tag}: {a:?}: {}", c.summary),
        );
        if own_named.is_empty() && obs_named.is_empty() {
            return;
        }
    }
    // H2: every named destination is involved, with the right role.
    for d in &own_named {
        if !need.own.contains(d) {
            if need.observers.contains(d) {
                rep.flag(
                    "3-observer-named-as-destination",
                    format!("{tag}: {d:?} for {a:?}: {}", c.summary),
                );
            } else {
                rep.flag(
                    "3-uninvolved-destination",
                    format!(
                        "{tag}: {d:?} for {a:?} (own {:?}, observers {:?}): {}",
                        need.own, need.observers, c.summary
                    ),
                );
            }
        }
    }
    for d in &obs_named {
        if !need.observers.contains(d) {
            if need.own.contains(d) && need.kind != OBS {
                rep.flag(
                    "3-destination-named-as-observer",
                    format!("{tag}: {d:?} for {a:?}: {}", c.summary),
                );
            } else if !need.own.contains(d) {
                rep.flag(
                    "3-uninvolved-observer",
                    format!(
                        "{tag}: {d:?} for {a:?} (own {:?}, observers {:?}): {}",
                        need.own, need.observers, c.summary
                    ),
                );
            }
        }
        if *d == P::Alert {
            rep.flag("4-alert-as-observer", format!("{tag}: {}", c.summary));
        }
        if *d == P::Home {
            rep.flag("4-home-screen-as-observer", format!("{tag}: {}", c.summary));
        }
    }
    let dests: BTreeSet<P> = own_named.union(&obs_named).cloned().collect();
    // H1: every named source is read and unallowed into some named destination.
    for s in &srcs {
        if !o.read.contains(s) {
            rep.flag(
                "3-source-not-read",
                format!("{tag}: {s:?} (read {}): {}", short(&o.read), c.summary),
            );
        } else if !uns.iter().any(|(d, x, _)| x == s && dests.contains(d)) {
            rep.flag(
                "3-source-not-needed",
                format!("{tag}: {s:?} for {a:?}; unsatisfied {uns:?}: {}", c.summary),
            );
        }
    }
    // H3: completeness.
    if !p.partial {
        for (d, s, k) in &uns {
            if !(dests.contains(d) && srcs.contains(s)) {
                rep.flag(
                    "3-pair-not-named",
                    format!(
                        "{tag}: ({d:?} <- {s:?}, kind {k}) for {a:?} on {}: {}",
                        sc.app_bundle_id, c.summary
                    ),
                );
            }
        }
    }
    if uns.is_empty() {
        rep.over_asks += 1;
    }
    let key = format!("{}|{}|{}", a.kind_name(), short(&srcs), short(&dests));
    let n = asked.entry(key.clone()).or_default();
    *n += 1;
    if approve_all && *n > 1 && uns.is_empty() {
        rep.flag(
            "5-repeated-question",
            format!("{tag}: asked {n} times: {key}: {}", c.summary),
        );
    }
    if approved {
        for d in &dests {
            let keep = Oracle::keep_for(&need, d, a);
            for s in &srcs {
                if keep > 0 {
                    let e = o.grants.entry((d.clone(), s.clone())).or_default();
                    *e = (*e).max(keep);
                }
                o.scope.insert((d.clone(), s.clone()));
            }
        }
    } else {
        let all: BTreeSet<P> = need.own.union(&need.observers).cloned().collect();
        for d in &all {
            for s in &srcs {
                o.blocked.insert((d.clone(), s.clone()));
            }
        }
    }
}

/// Oracle self-test: `ORACLE26_MUTATE=1` keeps no approval beyond its action, `2` drops
/// the observers (R23), `3` makes covering classes no source for themselves; each must make
/// the fuzzer report mismatches.
pub fn mutation() -> u8 {
    std::env::var("ORACLE26_MUTATE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

pub fn cases() -> u64 {
    std::env::var("ORACLE26_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120)
}

pub fn seed0() -> u64 {
    std::env::var("ORACLE26_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(26_000)
}

// ---------------------------------------------------------------------------------------
// Extensions: the checks of `oracle_extended.rs` (E1-E6).
// ---------------------------------------------------------------------------------------

/// Places the extended generator can produce, for naming.
pub fn ext_places() -> Vec<P> {
    vec![
        P::App(UNIDENTIFIED_APP_BUNDLE_ID.into()),
        P::Picker(SHOP.into(), "contact".into()),
        P::Picker(MAIL.into(), "photo".into()),
        P::Picker(MAIL.into(), "file".into()),
        P::Alert,
        P::Unident,
    ]
}

pub const INCOMPLETE_WORDS: &str = "could not be read completely";

/// Per-task state of the extended checks.
#[derive(Default)]
pub struct ExtState {
    /// The screen the step's model output was decided on.
    pub decided: Option<Screen>,
    /// The screen of the step's first command.
    pub first: Option<Screen>,
    pub any_confirm: bool,
    pub incomplete_asked: bool,
    pub focus_taps: u32,
    pub typed: bool,
    pub settles: u32,
    /// The last effect was the verify-settle wait.
    pub after_settle: bool,
    /// The step is a press_home that Rule T lets through on an incomplete or unidentified
    /// screen (E2b, E3d).
    pub home_step: bool,
    pub home_sent: bool,
    pub locked: bool,
    /// A look of the step showed another screen than the one it was decided on.
    pub changed: bool,
    pub seen: BTreeSet<&'static str>,
    pub confirms: usize,
    pub stale: usize,
}

impl ExtState {
    pub fn begin_step(&mut self, a: &A, sc: &Screen, o: &Oracle) {
        let i = places_of(sc, o.own_driven);
        let need = o.need(a, sc, None);
        *self = ExtState {
            decided: Some(sc.clone()),
            home_step: matches!(a, A::Home)
                && (i.incomplete || i.unident)
                && o.unsatisfied(&need).is_empty()
                && o.blocked_in(&need).is_empty(),
            seen: std::mem::take(&mut self.seen),
            confirms: self.confirms,
            stale: self.stale,
            ..ExtState::default()
        };
    }
}

fn trace(log: &Log) -> Vec<&String> {
    log.iter().rev().take(10).collect::<Vec<_>>()
}

/// The label of the field a typing or a clear targets on a screen.
fn target_label(a: &A, s: &Screen) -> Option<String> {
    let c = compact(s);
    match a {
        A::Type { el: Some(i), .. } | A::Clear(i) => el(&c, *i).map(|e| e.label.clone()),
        A::Type { el: None, .. } => c
            .elements
            .iter()
            .find(|e| e.focused)
            .map(|e| e.label.clone()),
        _ => None,
    }
    .filter(|l| !l.is_empty())
}

/// E6a: the places of a field (by its label) on a screen: the app in front, the places
/// under it, the picker, the top layer.
/// (app in front, places under the field, picker, alert up, unidentified layer).
type FieldPlaces = (String, Vec<P>, Option<P>, bool, bool);

fn field_places(s: &Screen, label: &str) -> Option<FieldPlaces> {
    let c = compact(s);
    let e = c.elements.iter().find(|e| e.label == label)?;
    let i = places_of(s, false);
    Some((
        i.bundle.clone(),
        i.places_under(e.frame.center()),
        i.picker.clone(),
        i.alert,
        i.unident,
    ))
}

fn field_focused(s: &Screen, label: &str) -> bool {
    has_shown(&s.root, &|n| n.focused && n.label.as_deref() == Some(label))
}

#[allow(clippy::too_many_arguments)]
fn ext_on_act(
    t: &Task,
    rep: &mut Report,
    tag: &str,
    a: &A,
    cmd: &RunnerCommand,
    decided: &Screen,
    last: &Screen,
    x: &mut ExtState,
    log: &Log,
) {
    let first = x.first.get_or_insert_with(|| last.clone()).clone();
    let di = places_of(decided, false);
    let li = places_of(last, false);
    let settle = matches!(cmd, RunnerCommand::Wait { ms } if *ms == VERIFY_SETTLE_MS);
    let cmd_s = format!("{cmd:?}");
    if settle {
        x.settles += 1;
        if matches!(a, A::Scroll { .. } | A::Swipe { .. }) {
            rep.flag(
                "S-settle-for-a-drag",
                format!("{tag}: oracle: no settle wait for a scroll or swipe; session: {cmd_s} for {a:?}; trace {:?}", trace(log)),
            );
        }
        if !matches!(a, A::Type { .. } | A::Clear(_)) && x.settles > 1 {
            rep.flag(
                "S-two-settles",
                format!(
                    "{tag}: oracle: at most one settle per step; session: {} for {a:?}; trace {:?}",
                    x.settles,
                    trace(log)
                ),
            );
        }
    }
    // E2a: on an incomplete screen every touching action and open_app/open_url asks first.
    let touching = a.touches() || matches!(a, A::OpenApp(_) | A::OpenUrl(_));
    if di.incomplete && touching && !settle {
        *rep.ext_counts
            .entry("checked: commands of touching steps on incomplete screens".into())
            .or_default() += 1;
    }
    if di.unident {
        *rep.ext_counts
            .entry("checked: commands on the unidentified layer".into())
            .or_default() += 1;
    }
    if settle {
        *rep.ext_counts
            .entry("checked: settle waits".into())
            .or_default() += 1;
    }
    if di.incomplete && touching && !settle && !x.incomplete_asked {
        rep.flag(
            "I-touch-unasked",
            format!("{tag}: oracle: \"{INCOMPLETE_WORDS}\" asked first; session: sent {cmd_s} for {a:?} on an incomplete {} with no such question; trace {:?}", decided.app_bundle_id, trace(log)),
        );
    }
    // E3c: no typing and no key but Home on the unidentified layer.
    let key_not_home = matches!(cmd, RunnerCommand::PressKey { key, .. } if *key != Key::Home);
    if di.unident && (matches!(cmd, RunnerCommand::TypeText { .. }) || key_not_home) {
        rep.flag(
            "U-typed-into-unidentified",
            format!(
                "{tag}: oracle: refused; session: sent {cmd_s} for {a:?}; trace {:?}",
                trace(log)
            ),
        );
    }
    // E4b: "Allow" on a system alert asks.
    if di.alert && !t.confirm_nothing {
        if let A::Tap(i) = a {
            if el(&compact(decided), *i).is_some_and(|e| e.label == "Allow") {
                *rep.ext_counts
                    .entry("checked: taps on Allow".into())
                    .or_default() += 1;
            }
            if el(&compact(decided), *i).is_some_and(|e| e.label == "Allow") && !x.any_confirm {
                rep.flag(
                    "AL-allow-unasked",
                    format!(
                        "{tag}: oracle: asked (R4); session: sent {cmd_s} unasked; trace {:?}",
                        trace(log)
                    ),
                );
            }
        }
    }
    // E5, E6a: a typing or a clear.
    if matches!(a, A::Type { .. } | A::Clear(_)) {
        if matches!(cmd, RunnerCommand::Tap { .. }) && !x.typed {
            x.focus_taps += 1;
            if x.focus_taps > 2 {
                rep.flag(
                    "F-too-many-focus-taps",
                    format!("{tag}: oracle: at most two focusing taps; session: {} for {a:?}; trace {:?}", x.focus_taps, trace(log)),
                );
            }
            if x.focus_taps == 2 {
                *rep.ext_counts
                    .entry("checked: focus retaps".into())
                    .or_default() += 1;
            }
            if x.focus_taps == 2
                && (!li
                    .bundle
                    .eq_ignore_ascii_case(&places_of(&first, false).bundle)
                    || li.alert
                    || li.unident)
            {
                rep.flag(
                    "F-retap-elsewhere",
                    format!("{tag}: oracle: the focus retap only in the same app, no alert up; session: retapped on {}; trace {:?}", last.app_bundle_id, trace(log)),
                );
            }
        }
        let text = matches!(cmd, RunnerCommand::TypeText { .. });
        // Triage (oracle fix): the Return of a `type … submit` is gated again on the fresh
        // screen as its own action (review pass 30, M3), so it is not compared with the
        // step's first screen; the base checks decide it on the screen it is sent from.
        let del = matches!(
            cmd,
            RunnerCommand::PressKey {
                key: Key::Delete,
                ..
            }
        );
        if text || del {
            *rep.ext_counts
                .entry("checked: texts and Deletes of a step".into())
                .or_default() += 1;
            if let Some(lbl) = target_label(a, &first) {
                if text && !field_focused(last, &lbl) {
                    rep.flag(
                        "F-typed-unfocused",
                        format!("{tag}: oracle: text only into {lbl:?} shown focused; session: {cmd_s} after a look on {} where it is not; trace {:?}", last.app_bundle_id, trace(log)),
                    );
                }
                let want = field_places(&first, &lbl);
                let got = field_places(last, &lbl);
                if want != got {
                    rep.flag(
                        "PC-sent-after-place-change",
                        format!("{tag}: oracle: nothing more of the step after a place change ({want:?} -> {got:?}); session: {cmd_s} for {a:?}; trace {:?}", trace(log)),
                    );
                }
            }
            if text {
                x.typed = true;
            }
        }
    }
}

fn ext_on_confirm(
    rep: &mut Report,
    tag: &str,
    c: &ConfirmRequest,
    cur: &Option<A>,
    x: &mut ExtState,
    log: &Log,
) {
    x.any_confirm = true;
    if c.summary.contains(INCOMPLETE_WORDS) {
        x.incomplete_asked = true;
        if !c.phone_only {
            rep.flag(
                "I-not-phone-only",
                format!(
                    "{tag}: oracle: phone only; session: remotely approvable: {}",
                    c.summary
                ),
            );
        }
        // Triage (oracle fix): a flow question for Home, a wait or a look carries the
        // incomplete screen's warning as a note; only a question asked for that alone is a
        // raise by the incomplete-screen rules.
        if matches!(cur, Some(A::Home | A::Wait(_) | A::Look)) && !c.summary.contains(FLOW) {
            rep.flag(
                "I-asked-for-home-wait-or-look",
                format!("{tag}: oracle: never raised by the incomplete-screen rules; session: asked for {cur:?}: {}", c.summary),
            );
        }
    }
    if c.summary.contains(FLOW)
        && matches!(cur, Some(A::Type { .. }))
        && x.focus_taps > 0
        && !x.typed
    {
        rep.flag(
            "F-question-inside-focus-retry",
            format!("{tag}: oracle: no new question inside a step's focus retry; session: {}; trace {:?}", c.summary, trace(log)),
        );
    }
}

fn ext_end_step(rep: &mut Report, tag: &str, x: &mut ExtState, log: &Log) {
    if x.home_step {
        *rep.ext_counts
            .entry(
                "checked: press_home steps Rule T allowed on incomplete or unidentified screens"
                    .into(),
            )
            .or_default() += 1;
    }
    if x.home_step && !x.home_sent && !x.any_confirm && !x.locked && !x.changed {
        rep.flag(
            "H-home-not-sent",
            format!("{tag}: oracle: Home is the way out (Rule T allowed it); session: no Home sent and nothing asked; trace {:?}", trace(log)),
        );
    }
    x.home_step = false;
}
