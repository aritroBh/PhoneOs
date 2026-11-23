//! Sixteenth review: confinement (Rule T) and insert-content controls (Rule P).
//!
//! **Why.** Fifteen reviews tried to decide by the shape of model-written text whether it
//! may reach a page; a steered model spells what it read with symbols, repeated owner
//! words, taps on a page-drawn keypad or Copy and Paste. Every model-chosen input is a
//! channel. What is protected is not "text" but **what the model has read elsewhere**: a
//! page can learn nothing from the model that the model does not know, and in a task that
//! has read only this page and the owner's request there is nothing to steal.
//!
//! **Rule T.** *What Jarvis read in one place does not reach another place unless the
//! owner allowed that flow on the phone for this task.* A place is a [`FlowContext`]: an
//! app (its bundle id, an OS fact), inside an app that shows web content the site (the
//! host read from the app's own chrome — a node outside every `WebView` subtree, never page
//! content; [`observed_context`]), a tool's results, or text recorded in an earlier task.
//! The session keeps a [`Confinement`]: the contexts whose content (or text derived from
//! it) entered a model prompt in this task, what the clipboard may hold, and the flows the
//! owner allowed or declined. Before the first input into a context while another context
//! read in this task has not been allowed into it, the owner is asked once, on the phone
//! ([`flow_note`] names the places).
//!
//! **Rule P.** A control that inserts content not written in the action — the edit menu's
//! Paste (and Paste and Go, Paste and Search, …), AutoFill, the Passwords / Use Password
//! items, a contact or text-scan inserter in an edit menu — enters content the question
//! cannot show. Pressing one always asks, phone only ([`insert_control`], by the edit
//! menu's structure first and the whole label second; a page's own "Paste" button asks
//! too: over-ask, never under-ask).

use std::collections::{BTreeMap, BTreeSet};

use jarvis_proto::{Action, CompactScreen, Element, RiskCategory, Role, Screen, UiNode};

use crate::actuation::{actuation, touches_screen, Actuation};
use crate::app::is_springboard;
use crate::evaluate::{ScreenSource, ScreenTree};
use crate::hit::takes_taps;

/// Where content comes from and goes to (Rule T).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlowContext {
    /// An app (lower-cased bundle id) whose screen shows no web content.
    App(String),
    /// A site shown in an app: the app (lower-cased bundle id) and the host read from the
    /// app's own chrome, outside every web view.
    Site {
        /// The app (lower-cased bundle id).
        app: String,
        /// The host (lower-cased).
        host: String,
    },
    /// Web content in an app (lower-cased bundle id) whose host could not be read from the
    /// app's chrome: one context per app (a known gap: such pages of one app are not told
    /// apart).
    SiteUnreadable(String),
    /// A tool call (its name): its arguments go there, its results come from there.
    Tool(String),
    /// Text recorded in an earlier task that a replayed step put into this task's history.
    Memory,
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
            | FlowContext::Site { app: a, .. } => Some(a),
            FlowContext::Tool(_) | FlowContext::Memory => None,
        }
    }

    /// How a question names this context: `app_name` gives an app's display name from its
    /// bundle id (the session's known apps); SpringBoard is "the Home Screen".
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
            FlowContext::Tool(name) => format!("the tool {name}"),
            FlowContext::Memory => "saved routines".to_owned(),
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

/// Rule T: the context of an observation, or `None` when it is not something Jarvis read
/// (a SpringBoard screen of icons only, see [`home_icons_only`]). The session leaves out
/// Jarvis's own app when the run does not drive it (the model gets a stub for it).
///
/// A screen holding a web view is a site: the host is read from the app's own chrome, a
/// node outside every web view subtree (Safari's address field), never from page content.
/// When no such node shows one host — none, a value that is not a host, two different
/// hosts — or the tree came from runner text (whose structure on-screen text can forge),
/// the context is the app with its site not readable.
pub fn observed_context(
    raw: &Screen,
    compact: &CompactScreen,
    source: ScreenSource,
) -> Option<FlowContext> {
    let app = raw.app_bundle_id.trim().to_lowercase();
    let chrome = scan_chrome(&raw.root);
    if is_springboard(&app) && !chrome.alert_like && home_icons_only(compact) {
        return None;
    }
    if !chrome.webview {
        return Some(FlowContext::App(app));
    }
    let host =
        (source == ScreenSource::Typed && chrome.hosts.len() == 1 && !chrome.unreadable_address)
            .then(|| chrome.hosts.into_iter().next())
            .flatten();
    Some(match host {
        Some(host) => FlowContext::Site { app, host },
        None => FlowContext::SiteUnreadable(app),
    })
}

/// Whether `action` puts model-chosen input into a context, so Rule T applies to it: every
/// action that [`touches_screen`] (taps, long presses, `tap_point`, swipes and scrolls,
/// typing with or without Return, clearing) and a tool call with any argument. `open_url`
/// with a link the owner gave and `open_app` are not (the owner chose the link; which app
/// is opened is a few bits — a known gap); a link the owner did not give is asked anyway.
pub fn flow_checked(action: &Action) -> bool {
    match action {
        Action::CallTool { args, .. } => match args {
            serde_json::Value::Null => false,
            serde_json::Value::Object(m) => !m.is_empty(),
            serde_json::Value::Array(a) => !a.is_empty(),
            serde_json::Value::String(s) => !s.is_empty(),
            _ => true,
        },
        other => touches_screen(other),
    }
}

/// The per-task record of Rule T: what was read, what the clipboard may hold, and the flows
/// the owner allowed or declined. Every operation reads small ordered sets (a task reads a
/// handful of contexts).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Confinement {
    read: BTreeSet<FlowContext>,
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
        self.read.insert(context);
    }

    /// Whether `context` was read in this task.
    pub fn has_read(&self, context: &FlowContext) -> bool {
        self.read.contains(context)
    }

    /// The contexts read in this task, in order.
    pub fn read_set(&self) -> impl Iterator<Item = &FlowContext> {
        self.read.iter()
    }

    /// Jarvis pressed Copy or Cut in `context`: the clipboard may hold its content.
    pub fn copied(&mut self, context: FlowContext) {
        self.clipboard.insert(context);
    }

    /// The contexts where Jarvis pressed Copy or Cut in this task, in order.
    pub fn clipboard(&self) -> impl Iterator<Item = &FlowContext> {
        self.clipboard.iter()
    }

    /// The contexts whose content may flow into `target` and that the owner has not allowed
    /// into it in this task: what was read (and, with `pasting`, what the clipboard may
    /// hold), but `target` itself.
    pub fn unallowed(&self, target: &FlowContext, pasting: bool) -> Vec<FlowContext> {
        let allowed = self.allowed.get(target);
        let clip = self.clipboard.iter().filter(|_| pasting);
        let mut out: Vec<FlowContext> = self
            .read
            .iter()
            .chain(clip)
            .filter(|c| *c != target && !allowed.is_some_and(|a| a.contains(*c)))
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The owner allowed `sources` to flow into `target` for the rest of the task.
    pub fn allow(&mut self, target: &FlowContext, sources: &[FlowContext]) {
        self.allowed
            .entry(target.clone())
            .or_default()
            .extend(sources.iter().cloned());
    }

    /// The owner declined `sources` flowing into `target`.
    pub fn decline(&mut self, target: &FlowContext, sources: &[FlowContext]) {
        self.declined
            .entry(target.clone())
            .or_default()
            .extend(sources.iter().cloned());
    }

    /// Whether the owner already declined every one of `sources` (non-empty) flowing into
    /// `target`: the same flow is not asked again in this task.
    pub fn is_declined(&self, target: &FlowContext, sources: &[FlowContext]) -> bool {
        !sources.is_empty()
            && self
                .declined
                .get(target)
                .is_some_and(|d| sources.iter().all(|s| d.contains(s)))
    }
}

/// The whole label's words, lower-cased and look-alike folded.
fn label_words(label: &str) -> Vec<String> {
    crate::text::tokens(label)
}

/// Whether `el` is an item of an edit or context menu, by role (XCUITest's `MenuItem`) or
/// by a raw ancestor whose role is a menu.
fn menu_item(el: &Element, tree: Option<&ScreenTree<'_>>) -> bool {
    let menu_role = |r: &Role| match r {
        Role::Other(n) => {
            let n = n.to_lowercase().replace([' ', '_'], "");
            n == "menuitem" || n == "menu" || n == "menubar"
        }
        _ => false,
    };
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
