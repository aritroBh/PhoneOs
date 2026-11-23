//! Sixteenth review: confinement (Rule T) and insert-content controls (Rule P); seventeenth
//! review: what a place is, which outputs are input, and controls that hand content on.
//!
//! **Why.** Fifteen reviews tried to decide by the shape of model-written text whether it
//! may reach a page; a steered model spells what it read with symbols, repeated owner
//! words, taps on a page-drawn keypad or Copy and Paste. Every model-chosen input is a
//! channel. What is protected is not "text" but **what the model has read elsewhere**: a
//! page can learn nothing from the model that the model does not know, and in a task that
//! has read only this page and the owner's request there is nothing to steal.
//!
//! **Rule T.** *What Jarvis read in one place does not reach another place unless the
//! owner allowed that flow on the phone for this task.* A place is a [`FlowContext`]:
//!
//! - an **app** (its bundle id, an OS fact);
//! - a **site**: in an app whose own chrome shows the host (a node outside every
//!   `WebView` subtree, never page content; [`observed_context`]), the top-level host —
//!   the browser's chrome belongs to the site's place;
//! - **embedded web content** (seventeenth review): a web view nested inside another web
//!   view, or a web view on a screen of an app whose chrome shows no host while the screen
//!   also lists content outside it, is a place of its own ([`ScreenPlaces`]). Between a
//!   native app and the web view it hosts the relation is one-way: what was read in the app
//!   is a source for input into its web content, what was read in the web content is not a
//!   source for input into the app (a host app can read what its own web view shows:
//!   [`FlowContext::surrounds`]); between a page and a frame nested in it, two-way;
//! - a **tool** (`tool:<name>`): a tool that is not self-contained (the default) is a
//!   source for its own later calls;
//! - **memory**: text recorded in an earlier task.
//!
//! Every model-chosen action is an input into its destination place ([`flow_checked`]:
//! all but `wait`, `look` and the text of `done`, `fail` and `ask_user` shown on the
//! phone). The session keeps a [`Confinement`]: the places whose content (or text derived
//! from it) entered a model prompt in this task, most recent first, what the clipboard may
//! hold, and the flows the owner allowed or declined. Before an input into a place while
//! another place read in this task has not been allowed into it, the owner is asked on the
//! phone ([`flow_note_fit`] names every place the approval allows).
//!
//! **Rule P.** A control that inserts content not written in the action — the edit menu's
//! Paste (and Paste and Go, Paste and Search, …), AutoFill, the Passwords / Use Password
//! items, a contact or text-scan inserter in an edit menu — enters content the question
//! cannot show. Pressing one always asks, phone only ([`insert_control`], by the edit
//! menu's structure first and the whole label second; a page's own "Paste" button asks
//! too: over-ask, never under-ask). Seventeenth review: every other item of an edit menu
//! that is not a local action ([`handoff_control`]: Search Web, Translate, Look Up,
//! Share…, anything unknown) hands the selection to another app or service and asks too.

use std::collections::{BTreeMap, BTreeSet};

use jarvis_proto::{Action, CompactScreen, Element, Point, RiskCategory, Role, Screen, UiNode};

use crate::actuation::{actuation, Actuation};
use crate::app::is_springboard;
use crate::classify::type_target;
use crate::evaluate::{ScreenSource, ScreenTree};
use crate::gestures::{drag_gesture, GestureKind};
use crate::hit::takes_taps;

/// The bundle id a web link opens in when nothing else says (the system browser): the app
/// part of an `open_url` destination ([`url_destination`]). A link that opens elsewhere
/// (another browser, an app's universal link) is observed there as another place, and the
/// next input there asks again.
pub const SYSTEM_BROWSER: &str = "com.apple.mobilesafari";

/// Where content comes from and goes to (Rule T).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlowContext {
    /// An app (lower-cased bundle id): its own content, outside web views that are places
    /// of their own.
    App(String),
    /// A site shown in an app: the app (lower-cased bundle id) and the top-level host read
    /// from the app's own chrome, outside every web view. The chrome is the site's.
    Site {
        /// The app (lower-cased bundle id).
        app: String,
        /// The host (lower-cased).
        host: String,
    },
    /// A screen of an app that is web content with nothing listed outside it and whose host
    /// could not be read: one context per app ("site not readable").
    SiteUnreadable(String),
    /// Seventeenth review: embedded web content — a web view nested inside another web view
    /// (`within`: that page's host, `None` when it is not readable), or a web view beside
    /// listed content in an app whose chrome shows no host (`within: None`). One place per
    /// app (and per site); frames the accessibility tree does not mark as a nested web
    /// view are part of their page.
    Embedded {
        /// The app (lower-cased bundle id).
        app: String,
        /// The site it is embedded in, if the page's host was read.
        within: Option<String>,
    },
    /// A tool call (its name): its arguments go there, its results come from there.
    Tool(String),
    /// Text recorded in an earlier task that a replayed step put into this task's history.
    Memory,
    /// Seventeenth review: a destination that cannot be decided — a link with no web host,
    /// an app the home screen cannot name. Every place read is a source for it, and no
    /// approval is kept for it (the next such input asks again).
    Unknown,
}

impl FlowContext {
    /// The context of an app with no web content.
    pub fn app(bundle_id: &str) -> FlowContext {
        FlowContext::App(bundle_id.trim().to_lowercase())
    }

    /// The app (lower-cased bundle id) of a screen context, if it is one.
    pub fn bundle(&self) -> Option<&str> {
        match self {
            FlowContext::App(a)
            | FlowContext::SiteUnreadable(a)
            | FlowContext::Site { app: a, .. }
            | FlowContext::Embedded { app: a, .. } => Some(a),
            FlowContext::Tool(_) | FlowContext::Memory | FlowContext::Unknown => None,
        }
    }

    /// Seventeenth review (the one-way relation): whether `inner` is web content that the
    /// native app `self` hosts — a web view beside the app's own listed content, in an app
    /// whose chrome shows no host. What was read in `inner` is not a source for input into
    /// `self` (a host app can read what its own web view shows); what was read in `self` is
    /// a source for input into `inner`.
    ///
    /// A frame in a page (a web view nested in a web view, under a site or under a page
    /// whose host is not readable) is **two-way**: the page does not have what a
    /// cross-origin frame shows, so each is a source for input into the other, and each
    /// direction is its own flow (approving one does not allow the other).
    pub fn surrounds(&self, inner: &FlowContext) -> bool {
        match (self, inner) {
            (
                FlowContext::App(app),
                FlowContext::Embedded {
                    app: a,
                    within: None,
                },
            ) => app == a,
            _ => false,
        }
    }

    /// A stable key of this place: what a learned flow's step records as the place it ran
    /// in (`app:<bundle>`, `site:<bundle> <host>`, …).
    pub fn key(&self) -> String {
        match self {
            FlowContext::App(a) => format!("app:{a}"),
            FlowContext::Site { app, host } => format!("site:{app} {host}"),
            FlowContext::SiteUnreadable(a) => format!("unreadable:{a}"),
            FlowContext::Embedded { app, within } => {
                format!("embedded:{app} {}", within.as_deref().unwrap_or_default())
            }
            FlowContext::Tool(t) => format!("tool:{t}"),
            FlowContext::Memory => "memory".to_owned(),
            FlowContext::Unknown => "unknown".to_owned(),
        }
    }

    /// How a question names this context: `app_name` gives an app's display name from its
    /// bundle id (the session's known apps); SpringBoard is "the Home Screen". A site is
    /// named by its host, a tool by its name, embedded content by where it is embedded.
    pub fn display(&self, app_name: impl Fn(&str) -> Option<String>) -> String {
        let app = |b: &str| {
            if is_springboard(b) {
                "the Home Screen".to_owned()
            } else {
                app_name(b).unwrap_or_else(|| b.to_owned())
            }
        };
        match self {
            FlowContext::App(b) => app(b),
            FlowContext::Site { host, .. } => host.clone(),
            FlowContext::SiteUnreadable(b) => format!("{} (site not readable)", app(b)),
            FlowContext::Embedded {
                within: Some(h), ..
            } => format!("embedded content in {h}"),
            FlowContext::Embedded {
                app: b,
                within: None,
            } => format!("web content in {}", app(b)),
            FlowContext::Tool(name) => format!("the tool {name}"),
            FlowContext::Memory => "saved routines".to_owned(),
            FlowContext::Unknown => "a place Jarvis cannot name".to_owned(),
        }
    }
}

/// What the gate's walk over a raw tree found outside every web view.
#[derive(Default)]
struct Chrome {
    webview: bool,
    alert_like: bool,
    hosts: BTreeSet<String>,
    unreadable_address: bool,
    /// Content outside every web view: a node other than the application and its windows
    /// with a label or a value (the screen lists something besides its web content).
    outside: bool,
}

/// Whether a raw node is an address bar of the app's chrome: a node labelled "Address" or
/// "URL" (Safari, in-app browsers) or with one of the identifiers browsers give it.
fn address_node(n: &UiNode) -> bool {
    let label = n
        .label
        .as_deref()
        .map(|l| l.trim().to_lowercase())
        .unwrap_or_default();
    let id = n.identifier.as_deref().unwrap_or_default().trim();
    matches!(
        label.as_str(),
        "address" | "url" | "address bar" | "website address"
    ) || matches!(
        id,
        "URL" | "url" | "TabBarItemTitle" | "Address" | "AddressBar"
    )
}

/// The host an address bar's value shows (`‎example.com`, `https://a.b/c`), lower-cased;
/// `None` when the value is not a host (a title, a search, empty).
pub(crate) fn host_of(value: &str) -> Option<String> {
    let v: String = value
        .chars()
        .filter(|c| !c.is_whitespace() && !crate::text::is_invisible(*c))
        .collect();
    let v = v.trim();
    let v = v.split_once("://").map_or(v, |(_, rest)| rest);
    let v = v.split(['/', '?', '#']).next().unwrap_or_default();
    let v = v.rsplit_once('@').map_or(v, |(_, h)| h);
    let v = v.trim_end_matches('.').to_lowercase();
    let shaped = !v.is_empty()
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
        && (v.contains('.') || v.contains(':') || v == "localhost")
        && v.chars().any(|c| c.is_ascii_alphanumeric());
    shaped.then_some(v)
}

/// Seventeenth review: where `open_url` sends its input — the site of the link's host in
/// the system browser ([`SYSTEM_BROWSER`]) for an `http(s)` link (whoever gave the link),
/// else [`FlowContext::Unknown`] (an app scheme, a link without a host: the destination
/// cannot be decided).
pub fn url_destination(url: &str) -> FlowContext {
    let u = url.trim();
    let scheme = u.split_once("://").map(|(s, _)| s.to_ascii_lowercase());
    let web = matches!(scheme.as_deref(), Some("http" | "https"));
    match host_of(u).filter(|_| web) {
        Some(host) => FlowContext::Site {
            app: SYSTEM_BROWSER.to_owned(),
            host,
        },
        None => FlowContext::Unknown,
    }
}

/// Whether a raw role is an alert, a sheet or a notification banner or list (content a
/// SpringBoard screen shows besides icons).
fn alert_like_role(role: &Role) -> bool {
    match role {
        Role::Alert | Role::Sheet => true,
        Role::Other(name) => {
            let n = name.to_lowercase();
            n.contains("banner") || n.contains("notification")
        }
        _ => false,
    }
}

/// One walk over the raw tree that never descends into a web view (page content is never
/// read for the context; its cost is the app's chrome only).
fn scan_chrome(root: &UiNode) -> Chrome {
    let mut out = Chrome::default();
    let mut stack: Vec<&UiNode> = vec![root];
    while let Some(n) = stack.pop() {
        if n.role == Role::WebView {
            out.webview = true;
            continue;
        }
        if alert_like_role(&n.role) {
            out.alert_like = true;
        }
        if !matches!(n.role, Role::Application | Role::Window)
            && (n.label.as_deref().is_some_and(|l| !l.trim().is_empty())
                || n.value.as_deref().is_some_and(|v| !v.trim().is_empty()))
        {
            out.outside = true;
        }
        if address_node(n) {
            match n.value.as_deref().and_then(host_of) {
                Some(h) => {
                    out.hosts.insert(h);
                }
                None => out.unreadable_address = true,
            }
        }
        stack.extend(n.children.iter());
    }
    out
}

/// Whether a SpringBoard screen lists only app icons, folders, the dock, the page dots and
/// the search pill (by role and structure): such a screen is not something Jarvis read. A
/// widget (SpringBoard gives it the value "Widget"), a cell (a notification list, a search
/// result), a text input (Spotlight), static text other than the pill's "Search", or any
/// alert, sheet or banner in the raw tree makes it a read.
fn home_icons_only(screen: &CompactScreen) -> bool {
    screen.elements.iter().all(|e| {
        let label = e.label.trim().to_lowercase();
        let value = e.value.as_deref().unwrap_or_default().to_lowercase();
        match &e.role {
            Role::Button => !value.contains("widget"),
            Role::StaticText | Role::Image => label.is_empty() || label == "search",
            Role::Other(name) => {
                let n = name.to_lowercase();
                n.contains("pageindicator")
                    || label == "dock"
                    || label == "search"
                    || (label.is_empty() && value.starts_with("page "))
            }
            _ => false,
        }
    })
}

/// Sixteenth review, home screen rule: what a widget shows is not shown to the model.
///
/// On a SpringBoard screen (bundle id `com.apple.springboard`, an OS fact) this returns the
/// tree the model gets: launch targets and the OS chrome needed to act are kept — app icons
/// and folders (buttons that are not widgets), the dock, the page dots, the search pill, the
/// status bar, the keyboard, `Icon` and app-switcher `card` nodes — and every alert, sheet, banner or notification subtree is
/// kept **unchanged and in full** (the model must see and dismiss those; they count as a
/// read, [`observed_context`]). Spotlight's field and result rows (cells, tables) are kept
/// too: the owner's typing chooses them, the denied-app check reads them, and a screen that
/// shows them is a read as before.
///
/// A widget is a button SpringBoard gives the value "Widget" (or "Widget, Stack"; see the
/// real `springboard_home` fixture) and the Today view's scroll view (identifier
/// `left-of-home…`). Everything else on the screen that is not classified above fails
/// closed: it is treated as widget content. A dropped node is replaced by a **stub**: the
/// same frame and drawing flags, no label, value, identifier, placeholder or children, role
/// `Other`. The stub is not listed (it has no label) and cannot be tapped, but it still
/// covers what lies under it, so an icon under a widget is refused as covered as before.
/// There is no marker line: nothing from a widget, not even its name, reaches the prompt.
///
/// `None` for any other app. The digest of the screen is taken from the raw tree, not from
/// this one.
pub fn home_view(screen: &Screen) -> Option<Screen> {
    if !is_springboard(&screen.app_bundle_id) {
        return None;
    }
    let mut out = screen.clone();
    out.root = home_node(&screen.root);
    Some(out)
}

/// A dropped node: its frame and drawing flags only.
fn home_stub(n: &UiNode) -> UiNode {
    UiNode {
        // A control stays a (disabled, unlabelled) control, so it still takes touches from
        // what lies under it as a cover; anything else is a plain node.
        role: if takes_taps(&n.role) {
            Role::Button
        } else {
            Role::Other("Other".to_owned())
        },
        label: None,
        value: None,
        identifier: None,
        placeholder: None,
        frame: n.frame,
        enabled: false,
        visible: n.visible,
        hittable: n.hittable,
        focused: false,
        selected: false,
        secure: false,
        children: Vec::new(),
    }
}

/// Whether a SpringBoard node is a widget: the value SpringBoard gives it, or the Today
/// view's scroll view.
fn home_widget(n: &UiNode) -> bool {
    let value = n.value.as_deref().unwrap_or_default().trim().to_lowercase();
    let label = n.label.as_deref().unwrap_or_default().trim().to_lowercase();
    value == "widget"
        || value.starts_with("widget,")
        || label.ends_with(" widget")
        || n.identifier
            .as_deref()
            .is_some_and(|i| i.to_lowercase().starts_with("left-of-home"))
}

/// Whether a SpringBoard node (not a widget, not alert-like) is kept.
fn home_kept(n: &UiNode) -> bool {
    let label = n.label.as_deref().unwrap_or_default().trim().to_lowercase();
    let value = n.value.as_deref().unwrap_or_default().trim().to_lowercase();
    match &n.role {
        Role::Application
        | Role::Window
        | Role::Button
        | Role::TextField
        | Role::SecureTextField
        | Role::TextView
        | Role::SearchField
        | Role::Cell
        | Role::Table
        | Role::CollectionView
        | Role::Keyboard
        | Role::Key
        | Role::ScrollView => true,
        Role::StaticText | Role::Image => {
            (label.is_empty() || label == "search") && value.is_empty()
        }
        Role::Other(name) => {
            let name = name.to_lowercase();
            name.contains("pageindicator")
                || name.contains("statusbar")
                || name == "icon"
                || name == "card"
                || label == "dock"
                || label == "search"
                || label.contains("scroll bar")
                || (label.is_empty() && value.is_empty())
                || (label.is_empty() && value.starts_with("page "))
        }
        _ => false,
    }
}

fn home_node(n: &UiNode) -> UiNode {
    if alert_like_role(&n.role) {
        return n.clone();
    }
    if home_widget(n) || !home_kept(n) {
        return home_stub(n);
    }
    // The status bar is OS chrome whose labels (time, signal, battery) the model may read.
    if matches!(&n.role, Role::Other(name) if name.to_lowercase().contains("statusbar")) {
        return n.clone();
    }
    let mut out = n.clone();
    out.children = n.children.iter().map(home_node).collect();
    out
}

/// Rule T: the place of an observation's own content (outside the web views that are
/// places of their own, [`ScreenPlaces`]), or `None` when it is not something Jarvis read
/// (a SpringBoard screen of icons only, see [`home_icons_only`]). The session leaves out
/// Jarvis's own app when the run does not drive it (the model gets a stub for it).
///
/// A screen holding a web view whose host the app's own chrome shows (a node outside every
/// web view subtree: Safari's address field; never page content) is that site. With no
/// single host — none, a value that is not a host, two different hosts — the screen is the
/// app when it lists content outside its web views (the web views are then "web content in
/// <App>", [`ScreenPlaces`]), else the app with its site not readable. A tree from runner
/// text (whose structure on-screen text can forge) is one place: the app, or with a web view
/// the app with its site not readable.
pub fn observed_context(
    raw: &Screen,
    compact: &CompactScreen,
    source: ScreenSource,
) -> Option<FlowContext> {
    let places = observed_places(raw, compact, source, None);
    places.read.then_some(places.base)
}

/// Seventeenth review: the places one observation shows, decided once per observation
/// (one walk of the chrome, O(web views) over the raw indices), and the place of an action
/// on it in O(depth) ([`ScreenPlaces::destinations`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenPlaces {
    /// The place of the screen's own content: the app, the site (its chrome included) or
    /// the app with its site not readable.
    base: FlowContext,
    /// Embedded web content on this screen, if any ([`FlowContext::Embedded`]).
    embedded: Option<FlowContext>,
    /// Top-level web views are embedded content too (an app without a readable host whose
    /// screen lists content outside them); else only nested web views are.
    top_embedded: bool,
    /// Every web view's raw index, ascending (`jarvis_observe::CompactView::webviews`).
    webviews: Vec<u32>,
    /// The screen is something Jarvis read (not an icons-only home screen).
    read: bool,
}

impl Default for ScreenPlaces {
    fn default() -> Self {
        ScreenPlaces {
            base: FlowContext::App(String::new()),
            embedded: None,
            top_embedded: false,
            webviews: Vec::new(),
            read: false,
        }
    }
}

impl ScreenPlaces {
    /// The place of the screen's own content (also for an icons-only home screen, which is
    /// not a read).
    pub fn base(&self) -> &FlowContext {
        &self.base
    }

    /// The embedded web content on this screen, if any.
    pub fn embedded(&self) -> Option<&FlowContext> {
        self.embedded.as_ref()
    }

    /// The places reading this screen reads: every place on it (none for an icons-only
    /// home screen).
    pub fn read_places(&self) -> Vec<FlowContext> {
        if !self.read {
            return Vec::new();
        }
        let mut out = vec![self.base.clone()];
        out.extend(self.embedded.clone());
        out
    }

    /// Every place on this screen.
    pub fn all(&self) -> Vec<FlowContext> {
        let mut out = vec![self.base.clone()];
        out.extend(self.embedded.clone());
        out
    }

    /// The place of the content of raw node `n`: how many web views hold it (itself
    /// included), walking its ancestors — O(depth · log web views).
    fn of_node(&self, n: u32, tree: &ScreenTree<'_>) -> &FlowContext {
        let Some(embedded) = self.embedded.as_ref() else {
            return &self.base;
        };
        let mut depth = 0usize;
        let mut at = Some(n);
        let mut steps = 0usize;
        while let Some(i) = at {
            if self.webviews.binary_search(&i).is_ok() {
                depth += 1;
                if depth >= 2 {
                    break;
                }
            }
            steps += 1;
            if steps > tree.parents.len() {
                break;
            }
            at = usize::try_from(i)
                .ok()
                .and_then(|k| tree.parents.get(k))
                .copied()
                .filter(|p| *p != u32::MAX);
        }
        match depth {
            0 => &self.base,
            1 if !self.top_embedded => &self.base,
            _ => embedded,
        }
    }

    /// The place of a listed element; every place on the screen when its raw node is not
    /// known (fail closed).
    fn of_element(&self, el: &Element, tree: Option<&ScreenTree<'_>>, out: &mut Vec<FlowContext>) {
        if self.embedded.is_none() {
            out.push(self.base.clone());
            return;
        }
        match tree.and_then(|t| t.source(el).map(|s| (t, s))) {
            Some((t, s)) => out.push(self.of_node(s, t).clone()),
            None => out.extend(self.all()),
        }
    }

    /// The places under point `p`: the screen's own place and the place inside every drawn
    /// web view whose frame holds it (what is under a point can be any of them: fail
    /// closed). Without the drawn nodes, every place on the screen.
    fn of_point(&self, p: Point, tree: Option<&ScreenTree<'_>>, out: &mut Vec<FlowContext>) {
        out.push(self.base.clone());
        let Some(embedded) = self.embedded.as_ref() else {
            return;
        };
        let Some(t) = tree.filter(|t| !t.nodes.is_empty()) else {
            out.push(embedded.clone());
            return;
        };
        for &w in &self.webviews {
            if crate::hit::raw_node(t, w).is_some_and(|n| n.frame.contains(p)) {
                out.push(self.of_node(w, t).clone());
            }
        }
    }

    /// Seventeenth review: the places an input `action` on `screen` goes to — the place of
    /// the element it targets (for `type` without an element, the focused field's), for
    /// `tap_point` the places under the point, for a swipe or scroll the place of its
    /// element and the places under its start point. Deduplicated; never empty. Actions
    /// that do not touch the screen go to the screen's own place (the session decides their
    /// destinations itself).
    pub fn destinations(
        &self,
        action: &Action,
        screen: &CompactScreen,
        tree: Option<&ScreenTree<'_>>,
    ) -> Vec<FlowContext> {
        let mut out = Vec::new();
        let element = |e: Option<&Element>, out: &mut Vec<FlowContext>| match e {
            Some(e) => self.of_element(e, tree, out),
            None => out.extend(self.all()),
        };
        match action {
            Action::Tap { element: i }
            | Action::LongPress { element: i, .. }
            | Action::Clear { element: i } => element(screen.get(*i), &mut out),
            Action::Type { element: i, .. } => element(type_target(*i, screen), &mut out),
            Action::TapPoint { x, y } => {
                if x.is_finite() && y.is_finite() {
                    let p = Point::new(x * screen.size.width, y * screen.size.height);
                    self.of_point(p, tree, &mut out);
                } else {
                    out.extend(self.all());
                }
            }
            Action::Scroll {
                direction,
                element: i,
            }
            | Action::Swipe {
                direction,
                element: i,
            } => {
                let kind = if matches!(action, Action::Scroll { .. }) {
                    GestureKind::Scroll
                } else {
                    GestureKind::Swipe
                };
                let target = i.and_then(|i| screen.get(i));
                if target.is_some() {
                    element(target, &mut out);
                }
                match drag_gesture(kind, *direction, target.map(|e| e.frame), screen) {
                    Ok(g) => self.of_point(g.from, tree, &mut out),
                    Err(_) => out.extend(self.all()),
                }
            }
            Action::PressHome
            | Action::OpenApp { .. }
            | Action::OpenUrl { .. }
            | Action::Wait { .. }
            | Action::CallTool { .. }
            | Action::AskUser { .. }
            | Action::Look
            | Action::Done { .. }
            | Action::Fail { .. } => out.push(self.base.clone()),
        }
        let mut seen = BTreeSet::new();
        out.retain(|c| seen.insert(c.clone()));
        out
    }
}

/// Seventeenth review: the places of one observation ([`ScreenPlaces`]). `view` is the raw
/// tree's shape from `jarvis_observe::CompactView`: every web view's raw index
/// (ascending), every raw node's subtree end, and every listed element's raw node. Without
/// it (`None`) nested web views are not told apart, and content outside the web views is
/// any labelled node of the chrome (the screen's own place is decided the same way).
pub fn observed_places(
    raw: &Screen,
    compact: &CompactScreen,
    source: ScreenSource,
    view: Option<(&[u32], &[u32], &[u32])>,
) -> ScreenPlaces {
    let app = raw.app_bundle_id.trim().to_lowercase();
    let chrome = scan_chrome(&raw.root);
    if is_springboard(&app) && !chrome.alert_like && home_icons_only(compact) {
        return ScreenPlaces {
            base: FlowContext::App(app),
            ..ScreenPlaces::default()
        };
    }
    let single = |base| ScreenPlaces {
        base,
        read: true,
        ..ScreenPlaces::default()
    };
    if !chrome.webview {
        return single(FlowContext::App(app));
    }
    // Runner text: its structure is forgeable, so its web views are not told apart.
    if source == ScreenSource::RunnerText {
        return single(FlowContext::SiteUnreadable(app));
    }
    let host = (chrome.hosts.len() == 1 && !chrome.unreadable_address)
        .then(|| chrome.hosts.iter().next().cloned())
        .flatten();
    // Top-level web views (their raw subtree ranges, disjoint and ascending) and whether a
    // web view lies inside another: one pass over the web views.
    let (webviews, ends, sources) = view.unwrap_or((&[], &[], &[]));
    let end = |w: u32| {
        usize::try_from(w)
            .ok()
            .and_then(|i| ends.get(i))
            .copied()
            .unwrap_or(0)
    };
    let mut tops: Vec<(u32, u32)> = Vec::new();
    let mut nested = false;
    for &w in webviews {
        if tops.last().is_some_and(|&(a, e)| w > a && w < e) {
            nested = true;
        } else {
            tops.push((w, end(w).max(w.saturating_add(1))));
        }
    }
    // Content outside the web views: a listed element whose raw node no top-level web view
    // holds (by the listed elements when the tree is known, else by the chrome walk).
    let outside = if view.is_some() && !tops.is_empty() {
        sources.iter().any(|&s| {
            let k = tops.partition_point(|&(a, _)| a <= s);
            !(k > 0 && tops.get(k - 1).is_some_and(|&(a, e)| s >= a && s < e))
        })
    } else {
        chrome.outside
    };
    let (base, top_embedded, within) = match host {
        Some(h) => (
            FlowContext::Site {
                app: app.clone(),
                host: h.clone(),
            },
            false,
            Some(h),
        ),
        None if outside => (FlowContext::App(app.clone()), true, None),
        None => (FlowContext::SiteUnreadable(app.clone()), false, None),
    };
    let embedded = (top_embedded || nested).then_some(FlowContext::Embedded { app, within });
    ScreenPlaces {
        base,
        top_embedded,
        webviews: if embedded.is_some() {
            webviews.to_vec()
        } else {
            Vec::new()
        },
        embedded,
        read: true,
    }
}

/// Seventeenth review: whether `action` is an input Rule T checks — every model-chosen
/// action except `wait` (its timing is observable only by places that receive some other
/// input, each of which is checked), `look` (it reads), and the text of `done`, `fail`
/// and `ask_user` (shown on the phone). Taps, long presses, `tap_point`, swipes and
/// scrolls, typing, clearing, `press_home` (into the place it leaves), `open_app` and a
/// home-screen launch (into the app), `open_url` (into the link's site, whoever gave it)
/// and every tool call, with or without arguments.
pub fn flow_checked(action: &Action) -> bool {
    match action {
        Action::Tap { .. }
        | Action::LongPress { .. }
        | Action::Type { .. }
        | Action::Clear { .. }
        | Action::Scroll { .. }
        | Action::Swipe { .. }
        | Action::TapPoint { .. }
        | Action::PressHome
        | Action::OpenApp { .. }
        | Action::OpenUrl { .. }
        | Action::CallTool { .. } => true,
        Action::Wait { .. }
        | Action::Look
        | Action::AskUser { .. }
        | Action::Done { .. }
        | Action::Fail { .. } => false,
    }
}

/// The per-task record of Rule T: what was read (with when it was last read), what the
/// clipboard may hold, and the flows the owner allowed or declined. Every operation reads
/// small ordered sets (a task reads a handful of contexts).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Confinement {
    read: BTreeMap<FlowContext, u64>,
    seq: u64,
    clipboard: BTreeSet<FlowContext>,
    allowed: BTreeMap<FlowContext, BTreeSet<FlowContext>>,
    declined: BTreeMap<FlowContext, BTreeSet<FlowContext>>,
}

impl Confinement {
    /// An empty record (a new task).
    pub fn new() -> Self {
        Self::default()
    }

    /// Content of `context` (or text derived from it) entered a model prompt.
    pub fn read(&mut self, context: FlowContext) {
        self.seq = self.seq.saturating_add(1);
        self.read.insert(context, self.seq);
    }

    /// Whether `context` was read in this task.
    pub fn has_read(&self, context: &FlowContext) -> bool {
        self.read.contains_key(context)
    }

    /// The contexts read in this task, most recently read first.
    pub fn read_set(&self) -> impl Iterator<Item = &FlowContext> {
        let mut all: Vec<(&FlowContext, u64)> = self.read.iter().map(|(c, s)| (c, *s)).collect();
        all.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        all.into_iter().map(|(c, _)| c)
    }

    /// Jarvis pressed Copy or Cut in `context`: the clipboard may hold its content.
    pub fn copied(&mut self, context: FlowContext) {
        self.clipboard.insert(context);
    }

    /// The contexts where Jarvis pressed Copy or Cut in this task, in order.
    pub fn clipboard(&self) -> impl Iterator<Item = &FlowContext> {
        self.clipboard.iter()
    }

    /// Whether the owner allowed `source` into `target`. An approval into an app covers
    /// its "site not readable" page (the launch asks once, the first touch does not ask
    /// again for the same flow).
    fn allowed_into(&self, target: &FlowContext, source: &FlowContext) -> bool {
        let has = |t: &FlowContext| self.allowed.get(t).is_some_and(|a| a.contains(source));
        has(target)
            || matches!(target, FlowContext::SiteUnreadable(a) if has(&FlowContext::App(a.clone())))
    }

    /// The contexts whose content may flow into `target` and that the owner has not allowed
    /// into it in this task: what was read (and, with `pasting`, what the clipboard may
    /// hold), but `target` itself and the embedded content it surrounds. Most recently
    /// read first.
    pub fn unallowed(&self, target: &FlowContext, pasting: bool) -> Vec<FlowContext> {
        self.unallowed_into(std::slice::from_ref(target), pasting, false)
    }

    /// [`Self::unallowed`] into any of `targets` (an input whose place is not one place:
    /// every place it may reach). `self_source`: the target's own earlier content is a
    /// source too (a tool that is not self-contained reaches many places).
    pub fn unallowed_into(
        &self,
        targets: &[FlowContext],
        pasting: bool,
        self_source: bool,
    ) -> Vec<FlowContext> {
        let clip = self.clipboard.iter().filter(|_| pasting);
        let mut out: Vec<FlowContext> = Vec::new();
        for target in targets {
            for c in self.read.keys().chain(clip.clone()) {
                if (c == target && !self_source)
                    || target.surrounds(c)
                    || self.allowed_into(target, c)
                    || out.contains(c)
                {
                    continue;
                }
                out.push(c.clone());
            }
        }
        let when = |c: &FlowContext| self.read.get(c).copied().unwrap_or(0);
        out.sort_by(|a, b| when(b).cmp(&when(a)).then(a.cmp(b)));
        out
    }

    /// The owner allowed `sources` to flow into `target` for the rest of the task.
    pub fn allow(&mut self, target: &FlowContext, sources: &[FlowContext]) {
        self.allow_into(std::slice::from_ref(target), sources);
    }

    /// [`Self::allow`] into each of `targets`; nothing is kept for
    /// [`FlowContext::Unknown`] (each such input asks again).
    pub fn allow_into(&mut self, targets: &[FlowContext], sources: &[FlowContext]) {
        for t in targets.iter().filter(|t| **t != FlowContext::Unknown) {
            self.allowed
                .entry(t.clone())
                .or_default()
                .extend(sources.iter().cloned());
        }
    }

    /// The owner declined `sources` flowing into `target`.
    pub fn decline(&mut self, target: &FlowContext, sources: &[FlowContext]) {
        self.decline_into(std::slice::from_ref(target), sources);
    }

    /// [`Self::decline`] into each of `targets`.
    pub fn decline_into(&mut self, targets: &[FlowContext], sources: &[FlowContext]) {
        for t in targets {
            self.declined
                .entry(t.clone())
                .or_default()
                .extend(sources.iter().cloned());
        }
    }

    /// Whether the owner already declined one of `sources` flowing into `target`: an input
    /// that would carry it is not asked again in this task (it is refused).
    pub fn is_declined(&self, target: &FlowContext, sources: &[FlowContext]) -> bool {
        self.is_declined_into(std::slice::from_ref(target), sources)
    }

    /// [`Self::is_declined`] into any of `targets`.
    pub fn is_declined_into(&self, targets: &[FlowContext], sources: &[FlowContext]) -> bool {
        sources.iter().any(|s| {
            targets
                .iter()
                .any(|t| self.declined.get(t).is_some_and(|d| d.contains(s)))
        })
    }
}

/// The whole label's words, lower-cased and look-alike folded.
fn label_words(label: &str) -> Vec<String> {
    crate::text::tokens(label)
}

/// Whether a raw or listed role is a menu's (XCUITest's `MenuItem`, `Menu`, `MenuBar`, a
/// context or edit menu, a callout bar) — not a button that opens one (`MenuButton`).
fn menu_role(r: &Role) -> bool {
    match r {
        Role::Other(n) => {
            let n = n.to_lowercase().replace([' ', '_', '-'], "");
            (n.contains("menu") && n != "menubutton") || n.contains("callout")
        }
        _ => false,
    }
}

/// Whether `el` is an item of an edit or context menu, by role (XCUITest's `MenuItem`) or
/// by a raw ancestor whose role is a menu.
fn menu_item(el: &Element, tree: Option<&ScreenTree<'_>>) -> bool {
    if menu_role(&el.role) {
        return true;
    }
    tree.filter(|t| !t.nodes.is_empty()).is_some_and(|t| {
        t.ancestors(el)
            .into_iter()
            .any(|a| crate::hit::raw_node(t, a).is_some_and(|n| menu_role(&n.role)))
    })
}

/// Rule P: the category of an insert-content control's label — the edit menu's Paste
/// family and AutoFill, password items anywhere; contact and text-scan inserters inside a
/// menu — or `None`.
fn insert_label(label: &str, menu: bool) -> Option<RiskCategory> {
    let w = label_words(label);
    let words: Vec<&str> = w.iter().map(String::as_str).collect();
    let first = words.first().copied().unwrap_or_default();
    let last = words.last().copied().unwrap_or_default();
    let joined = words.join(" ");
    if first == "paste" {
        return Some(RiskCategory::SendMessage);
    }
    // AutoFill: the whole label anywhere ("AutoFill"); inside a menu any AutoFill item. A
    // Settings row "AutoFill & Passwords" is a page that opens, not an insert.
    let autofill = first == "autofill" || joined.starts_with("auto fill");
    if (autofill && (menu || matches!(words.as_slice(), ["autofill"] | ["auto", "fill"])))
        || words == ["passwords"]
        || (first == "use" && matches!(last, "password" | "passkey"))
        || joined.contains("suggested password")
        || joined.contains("strong password")
    {
        return Some(RiskCategory::Credential);
    }
    if menu
        && (matches!(
            words.as_slice(),
            ["contact"] | ["contacts"] | ["scan", "text"]
        ) || first == "insert")
    {
        return Some(RiskCategory::SendMessage);
    }
    None
}

/// Rule P: the category of pressing an insert-content control with `action` (a tap, long
/// press or `tap_point` on it), or `None` when `action` presses no such control.
pub fn insert_control(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Option<RiskCategory> {
    let Actuation::Press(el) = actuation(action, screen) else {
        return None;
    };
    insert_label(&el.label, menu_item(el, tree))
}

/// Seventeenth review (Rule P, items that hand content on): the edit-menu items that act
/// on the selection locally, by whole label — Copy, Cut, Select, Select All and the plain
/// formatting items (Format, Bold, Italic, Underline, Strikethrough). Every other item of
/// an edit or context menu — Search Web, Translate, Look Up, Share…, Open in…, anything
/// this list does not name — may send the selection to another app or service.
pub const LOCAL_MENU_ITEMS: &[&str] = &[
    "copy",
    "cut",
    "select",
    "select all",
    "format",
    "bold",
    "italic",
    "underline",
    "strikethrough",
];

/// Seventeenth review: whether `action` presses an item of an edit or context menu (by its
/// role or a raw ancestor's) that is neither a local action ([`LOCAL_MENU_ITEMS`], by whole
/// label) nor an insert (Rule P's own): it may hand the selected content to another app or
/// service, so it always asks, phone only, naming the place the content was read in.
/// Unknown items ask (fail closed).
pub fn handoff_control(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> bool {
    let Actuation::Press(el) = actuation(action, screen) else {
        return false;
    };
    if !menu_item(el, tree) || insert_label(&el.label, true).is_some() {
        return false;
    }
    let joined = label_words(&el.label).join(" ");
    !LOCAL_MENU_ITEMS.contains(&joined.as_str())
}

/// Whether `action` presses a Copy or Cut control (whole label starting with "copy" or
/// "cut": "Copy", "Copy Code", "Copy Link", "Cut"): the clipboard may then hold content
/// of the context in front ([`Confinement::copied`]). Copy itself is not asked: nothing
/// leaves the context until something is pasted, and a Paste always asks (Rule P).
pub fn copy_control(action: &Action, screen: &CompactScreen) -> bool {
    let Actuation::Press(el) = actuation(action, screen) else {
        return false;
    };
    let w = label_words(&el.label);
    matches!(w.first().map(String::as_str), Some("copy" | "cut"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_read_from_address_values() {
        assert_eq!(host_of("\u{200E}127.0.0.1").as_deref(), Some("127.0.0.1"));
        assert_eq!(
            host_of("https://Bank.Example/login?x=1").as_deref(),
            Some("bank.example")
        );
        assert_eq!(host_of("example.com.").as_deref(), Some("example.com"));
        assert_eq!(host_of("Search or enter website name"), None);
        assert_eq!(host_of(""), None);
        assert_eq!(host_of("Apple"), None);
    }

    #[test]
    fn insert_labels() {
        for l in [
            "Paste",
            "Paste and Go",
            "Paste and Search",
            "AutoFill",
            "Use Strong Password",
        ] {
            assert!(insert_label(l, false).is_some(), "{l}");
        }
        for l in [
            "Copy",
            "Cut",
            "Select All",
            "Contact",
            "Password reset",
            "Pastel",
        ] {
            assert!(insert_label(l, false).is_none(), "{l}");
        }
        assert!(insert_label("Contact", true).is_some());
        assert!(insert_label("Scan Text", true).is_some());
    }

    #[test]
    fn confinement_asks_per_target_and_source_set() {
        let mut c = Confinement::new();
        let msgs = FlowContext::app("com.apple.MobileSMS");
        let page = FlowContext::Site {
            app: "com.apple.mobilesafari".into(),
            host: "evil.example".into(),
        };
        c.read(page.clone());
        assert!(c.unallowed(&page, false).is_empty());
        c.read(msgs.clone());
        assert_eq!(c.unallowed(&page, false), vec![msgs.clone()]);
        c.allow(&page, std::slice::from_ref(&msgs));
        assert!(c.unallowed(&page, false).is_empty());
        let tool = FlowContext::Tool("lookup".into());
        c.read(tool.clone());
        assert_eq!(c.unallowed(&page, false), vec![tool.clone()]);
        assert!(!c.is_declined(&page, std::slice::from_ref(&tool)));
        c.decline(&page, std::slice::from_ref(&tool));
        assert!(c.is_declined(&page, std::slice::from_ref(&tool)));
        assert_eq!(c.unallowed(&msgs, false).len(), 2);
    }
}
