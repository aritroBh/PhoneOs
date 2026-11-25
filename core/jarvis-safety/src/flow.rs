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
//! Eighteenth review, the governing rule: a place may be merged with another, or exempted
//! as a source, only on an OS fact. Otherwise every distinct document and every input given
//! to the model (tree, pixels, tool output) is its own source; where two things cannot be
//! told apart they are not silently merged — the question says so and its approval covers
//! all of them.
//!
//! Nineteenth review: the conveniences kept in earlier rounds are removed — there is no
//! one-way relation anywhere, every embedded class Jarvis cannot name by an OS fact covers
//! all of itself from its first observation, and a browser's chrome is the browser, not the
//! site.
//!
//! - an **app** (its bundle id, an OS fact): its own content outside every web view. In a
//!   listed browser ([`BROWSERS`]) that is the browser's chrome (the address bar, tab
//!   cards, history and suggestion lists, the start page), whatever site is shown;
//! - a **site**: in a listed browser, the content inside the page's top-level web views,
//!   when the browser's one address field on screen shows a host ([`observed_places`]). Any
//!   other app has no site;
//! - **embedded web content** with no OS-given name (seventeenth to nineteenth reviews) —
//!   the web views of any app that is not a listed browser (`web content in <App>`), what
//!   is nested in them or in a page whose host is not readable (`nested web content in
//!   <App>`), a listed browser's page whose host is not readable (`pages in <App> with no
//!   readable address`), a runner-text screen with a web view — is **covering** from its
//!   first observation ([`FlowContext::always_covering`]): a source for input into itself,
//!   named `all … (Jarvis cannot tell its parts apart)`. Frames nested in a site's page
//!   (`embedded content in <site>`) are one class per site that becomes covering once it
//!   held two documents ([`Confinement::mark_ambiguous`]). A same-frame web view is part of
//!   the one holding it only when it is a structural wrapper
//!   (`jarvis_observe::CompactView::webviews`). Every two places are **two-way**: each is a
//!   source for input into the other;
//! - the **screen picture** ([`FlowContext::ScreenPicture`]): every screenshot given to the
//!   model, a source for every place;
//! - a **tool** (`tool:<name>`): a tool that is not self-contained (the default) is a
//!   source for its own later calls;
//! - **memory**: text recorded in an earlier task.
//!
//! Every model-chosen action is an input into its destination place ([`flow_checked`]:
//! all but the text of `done`, `fail` and `ask_user` shown on the phone). Twenty-third
//! review (R23): every model-chosen step is also seen by every place on the screen when it
//! is decided — an input of the weakest kind ([`InputKind::Observe`]) into each of them
//! (`wait`, `look` and `press_home` have no other destinations; [`ScreenPlaces::observers`]).
//! The session keeps a [`Confinement`]: the places whose content (or text derived
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

/// Eighteenth review (E3): the browsers whose own chrome names the site in front (lower-case
/// bundle ids, a closed list): Safari and the widely used third-party browsers. A host is
/// read from an address bar outside every web view only in one of them; in any other app
/// there is no site, and its web content is `web content in <App>` (an app can draw a node
/// labelled "Address" with any host it likes). A third-party browser's chrome is that
/// browser's word, which the owner chose to install; its sites are named with it
/// (`bank.example in Chrome`).
pub const BROWSERS: &[&str] = &[
    "com.apple.mobilesafari",
    "com.google.chrome.ios",
    "org.mozilla.ios.firefox",
    "com.microsoft.msedge",
    "com.brave.ios.browser",
    "com.duckduckgo.mobile.ios",
    "com.opera.operatouch",
];

/// Whether `bundle_id` is one of [`BROWSERS`] (compared lower-cased).
pub fn is_browser(bundle_id: &str) -> bool {
    let b = bundle_id.trim().to_lowercase();
    BROWSERS.contains(&b.as_str())
}

/// Eighteenth review (E1): how a question names [`FlowContext::ScreenPicture`]. Nineteenth
/// review: one place for every picture of the task, so the name says that an approval
/// covers the later pictures too.
pub const SCREEN_PICTURE: &str =
    "screen pictures (they can show notifications and other apps' banners)";

/// Twenty-third review (F2): how a flow question names the Home Screen as a place Jarvis
/// read. SpringBoard is a read only when it showed more than icons (a banner, a notification
/// list, an alert, Spotlight's field or results: [`home_icons_only`] and the alert-like roles
/// of the chrome walk), so its name as a source says so; as a destination it stays "the Home
/// Screen". Jarvis's own words: never shortened, and holding neither `, ` nor ` and `.
pub const HOME_SCREEN_READ: &str = "the Home Screen (notifications or search results shown on it)";

/// Nineteenth review: what the name of a covering embedded class
/// ([`Confinement::is_ambiguous`]) starts with (`all web content in Mail (…)`).
pub const COVERS_ALL_PREFIX: &str = "all ";

/// Eighteenth review (E5): what the name of a covering embedded class
/// ([`Confinement::is_ambiguous`]) ends with: its approval covers every part of it.
/// Nineteenth review: the whole name is `all <name> (Jarvis cannot tell its parts apart)`
/// ([`COVERS_ALL_PREFIX`]); neither part holds `, ` or ` and ` (a list of names stays
/// readable).
pub const COVERS_ALL: &str = " (Jarvis cannot tell its parts apart)";

/// Where content comes from and goes to (Rule T).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlowContext {
    /// An app (lower-cased bundle id): its own content, outside every web view. In a listed
    /// browser, its chrome (nineteenth review: never the site's).
    App(String),
    /// A site shown in a listed browser: the browser (lower-cased bundle id) and the host
    /// read from its one address field on screen. The content inside the page's top-level
    /// web views only.
    Site {
        /// The app (lower-cased bundle id).
        app: String,
        /// The host (lower-cased).
        host: String,
    },
    /// The top-level web content of a listed browser whose host could not be read (no
    /// address field, two, one being edited, a value that is not an ASCII host), or a
    /// runner-text screen holding a web view (any app: its structure is forgeable, so the
    /// whole screen is this one place). One class per app, covering from its first
    /// observation ([`FlowContext::always_covering`]).
    SiteUnreadable(String),
    /// Seventeenth review: embedded web content — with `within: Some(host)`, a web view
    /// nested (at any depth) inside a site's page (one class per site, covering once it held
    /// two documents, [`Confinement::mark_ambiguous`]); with `within: None`, the top-level
    /// web views of an app that is not a listed browser (covering from its first
    /// observation; what is nested inside is [`FlowContext::NestedWeb`]). Frames the
    /// accessibility tree does not mark as a web view are part of their page.
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
    /// Eighteenth review (E5a): content nested inside the web content of an app with no
    /// readable host (a frame in an in-app browser's page; every deeper level too). The top
    /// level of that web content is [`FlowContext::Embedded`] with `within: None` (or a
    /// browser's [`FlowContext::SiteUnreadable`]): two places, two-way. Covering from its
    /// first observation.
    NestedWeb(String),
    /// Eighteenth review (E1): the screen picture given to the model — the whole display,
    /// notification banners, the Dynamic Island and Live Activities drawn over the app
    /// included. Never a destination; a source for input into every place.
    ScreenPicture,
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
            | FlowContext::Embedded { app: a, .. }
            | FlowContext::NestedWeb(a) => Some(a),
            FlowContext::Tool(_)
            | FlowContext::Memory
            | FlowContext::Unknown
            | FlowContext::ScreenPicture => None,
        }
    }

    /// Nineteenth review (R-B): whether this place is an embedded class Jarvis cannot tell
    /// apart into its documents across looks from its first observation — every embedded
    /// class with no OS-given name: an app's web content ([`FlowContext::Embedded`] with
    /// `within: None`), nested web content ([`FlowContext::NestedWeb`]), a browser page whose
    /// host is not readable or a runner-text screen with a web view
    /// ([`FlowContext::SiteUnreadable`]). Such a class is a source for input into itself and
    /// is named as covering all of it ([`FlowContext::display_covering`]). A site's frames
    /// ([`FlowContext::Embedded`] within a host) become covering only once they held two
    /// documents ([`Confinement::mark_ambiguous`]).
    ///
    /// (The seventeenth review's one-way relation between an app and its web content, kept
    /// for Apple's apps in the eighteenth, is gone: a native control can open another
    /// party's document — an inbox row, a history entry — and that party observes the open,
    /// so what was read in web content is a source for the app's own controls too.)
    pub fn always_covering(&self) -> bool {
        matches!(
            self,
            FlowContext::Embedded { within: None, .. }
                | FlowContext::NestedWeb(_)
                | FlowContext::SiteUnreadable(_)
        )
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
            FlowContext::NestedWeb(a) => format!("nested:{a}"),
            FlowContext::ScreenPicture => "screen-picture".to_owned(),
        }
    }

    /// How a question names this context: `app_name` gives an app's display name from its
    /// bundle id (the session's known apps); SpringBoard is "the Home Screen". A site is
    /// named by its host — in Safari by its host alone, in any other app (a third-party
    /// browser) as `<host> in <App>` (eighteenth review, E3: two places are never named
    /// alike) — a tool by its name, embedded content by where it is embedded.
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
            FlowContext::Site { app: b, host } if b == SYSTEM_BROWSER => host.clone(),
            FlowContext::Site { app: b, host } => format!("{host} in {}", app(b)),
            FlowContext::SiteUnreadable(b) => {
                format!("pages in {} with no readable address", app(b))
            }
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
            FlowContext::NestedWeb(b) => format!("nested web content in {}", app(b)),
            FlowContext::ScreenPicture => SCREEN_PICTURE.to_owned(),
        }
    }

    /// Twenty-third review (F2): how a flow note names this context as a place Jarvis read
    /// (a source): [`Self::display`], except that SpringBoard — a read only when it showed
    /// more than icons — is [`HOME_SCREEN_READ`].
    pub fn display_read(&self, app_name: impl Fn(&str) -> Option<String>) -> String {
        match self {
            FlowContext::App(b) if is_springboard(b) => HOME_SCREEN_READ.to_owned(),
            other => other.display(app_name),
        }
    }

    /// Eighteenth review (E5b): how a flow note names an ambiguous embedded class
    /// ([`Confinement::is_ambiguous`]): [`COVERS_ALL_PREFIX`], its [`Self::display`] and
    /// [`COVERS_ALL`] (`all web content in Mail (Jarvis cannot tell its parts apart)`), so
    /// an approval states that it covers every part.
    pub fn display_covering(&self, app_name: impl Fn(&str) -> Option<String>) -> String {
        format!("{COVERS_ALL_PREFIX}{}{COVERS_ALL}", self.display(app_name))
    }
}

/// What the gate's walk over a raw tree found outside every web view.
#[derive(Default)]
struct Chrome {
    webview: bool,
    alert_like: bool,
    /// Every address-like node on screen ([`address_like`]): the host its value shows when
    /// it is the address field ([`address_field`]) and that value is a host, else `None`.
    addresses: Vec<Option<String>>,
    /// Content outside every web view: a node other than the application and its windows
    /// with a label or a value (the screen lists something besides its web content).
    outside: bool,
    /// [`Chrome::outside`] not counting the address-like nodes.
    outside_not_address: bool,
    /// Twentieth review (R20-B): a raw `Sheet` outside every web view, shown or not.
    sheet: bool,
    /// Twentieth review (R20-B): a raw `Sheet` outside every web view whose subtree holds the
    /// OS's "AirDrop" node — shown, hidden, scrolled away or wholly off the screen.
    share_sheet: bool,
    /// Run 122: the frame of each address field on screen that is Safari's minimized bar
    /// (an `Other`, [`minimized_address`]), to tell its listed element (which carries no
    /// identifier) from any other `Other` labelled as the address.
    minimized: Vec<jarvis_proto::Rect>,
}

/// Twentieth review (R20-B): whether a raw node is the share sheet's "AirDrop" row (whole
/// label, trimmed, ASCII case ignored; "Air Drop" too).
fn airdrop_label(l: &str) -> bool {
    let l = l.trim();
    l.eq_ignore_ascii_case("airdrop") || l.eq_ignore_ascii_case("air drop")
}

/// The labels a browser gives its address field (lower-cased, whole label).
const ADDRESS_LABELS: &[&str] = &["address", "url", "address bar", "website address"];

/// Whether a raw node may be an address bar of the app's chrome, by its label or by one of
/// the identifiers browsers give it — broad on purpose: only to count them (two on screen
/// make the host unreadable, fail closed).
fn address_like(n: &UiNode) -> bool {
    let label = n
        .label
        .as_deref()
        .map(|l| l.trim().to_lowercase())
        .unwrap_or_default();
    let id = n.identifier.as_deref().unwrap_or_default().trim();
    ADDRESS_LABELS.contains(&label.as_str())
        || matches!(
            id,
            "URL" | "url" | "TabBarItemTitle" | "Address" | "AddressBar"
        )
}

/// Nineteenth review (R-D): whether a raw node is the browser's address **field**, the only
/// node a host is read from: the browser's own control labelled as the address — a text
/// field (iOS 26: the real `safari_start_page` tree's `TextField "Address"
/// value="‎127.0.0.1"`) or a button (the unfocused address control of older Safari) — not
/// being edited (a focused field shows what is typed, not the page's host). The label is
/// what decides: an identifier alone (`TabBarItemTitle`, on some layouts a tab's title,
/// page-controlled text) or a static text (Safari's address before the page loads:
/// `StaticText "127.0.0.1"` with no label) is not one.
///
/// Run 122 (iOS 26.5): while the keyboard is up Safari minimizes its bar and the same node
/// becomes a plain `Other "Address" value="‎127.0.0.1" id=TabBarItemTitle` (the real
/// `safari_form_keyboard` tree), so an `Other` is the address field too, but only with that
/// exact identifier ([`minimized_address`]) **and** an address label of its own; the
/// unlabelled pre-load `Other id=TabBarItemTitle` stays unreadable.
fn address_field(n: &UiNode) -> bool {
    let label = n
        .label
        .as_deref()
        .map(|l| l.trim().to_lowercase())
        .unwrap_or_default();
    (matches!(n.role, Role::TextField | Role::Button) || minimized_address(n))
        && !n.focused
        && ADDRESS_LABELS.contains(&label.as_str())
}

/// Run 122: whether a raw node has the role and identifier of Safari's minimized address
/// bar — raw XCUITest type `Other` and the identifier exactly `TabBarItemTitle` (its label
/// is checked by [`address_field`]).
fn minimized_address(n: &UiNode) -> bool {
    matches!(&n.role, Role::Other(name) if name == "Other")
        && n.identifier.as_deref() == Some("TabBarItemTitle")
}

/// Whether a raw node is positively hidden (flagged invisible with a real frame):
/// compaction prunes it with its whole subtree (`jarvis_observe::compact`).
fn pruned(n: &UiNode) -> bool {
    !n.visible && !n.frame.is_empty()
}

/// Whether a raw node's own frame lies wholly off `bounds` (compaction never lists it, so
/// the model never reads it; its children are still looked at, fail closed).
fn off_screen(n: &UiNode, bounds: &jarvis_proto::Rect) -> bool {
    !n.frame.is_empty() && !bounds.is_empty() && !n.frame.intersects(bounds)
}

/// The host an address bar's value shows (`‎example.com`, `https://a.b/c`), lower-cased;
/// `None` when the value is not a host (a title, a search, empty). Twentieth review: only
/// whitespace and invisible characters at either end are dropped (Safari's leading
/// left-to-right mark); a value with one inside (`bank.example login`) is not read as a
/// host (fail closed: was stripped to `bank.examplelogin`).
pub(crate) fn host_of(value: &str) -> Option<String> {
    let edge = |c: char| c.is_whitespace() || crate::text::is_invisible(c);
    let v = value.trim_matches(edge);
    if v.chars().any(edge) {
        return None;
    }
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
/// read for the context; its cost is the app's chrome only). Hidden subtrees and nodes
/// wholly off `bounds` are not shown to the model and are not counted.
fn scan_chrome(root: &UiNode, bounds: &jarvis_proto::Rect) -> Chrome {
    let mut out = Chrome::default();
    // Each node with whether no ancestor (nor itself) is hidden, and whether a `Sheet` holds
    // it (itself included).
    let mut stack: Vec<(&UiNode, bool, bool)> = vec![(root, true, false)];
    while let Some((n, shown, in_sheet)) = stack.pop() {
        if n.role == Role::WebView {
            out.webview = true;
            continue;
        }
        // Twentieth review (R20-B): the share sheet is decided from the raw tree whether or
        // not its "AirDrop" row is on screen, hidden or scrolled away.
        let in_sheet = in_sheet || n.role == Role::Sheet;
        out.sheet |= in_sheet;
        if in_sheet && n.label.as_deref().is_some_and(airdrop_label) {
            out.share_sheet = true;
        }
        // An alert, sheet or banner anywhere makes a home screen a read (unchanged, fail
        // closed whether or not it is shown).
        if alert_like_role(&n.role) {
            out.alert_like = true;
        }
        let shown = shown && !pruned(n);
        stack.extend(n.children.iter().map(|c| (c, shown, in_sheet)));
        if !shown || off_screen(n, bounds) {
            continue;
        }
        let address = address_like(n);
        if !matches!(n.role, Role::Application | Role::Window)
            && (n.label.as_deref().is_some_and(|l| !l.trim().is_empty())
                || n.value.as_deref().is_some_and(|v| !v.trim().is_empty()))
        {
            out.outside = true;
            out.outside_not_address |= !address;
        }
        if address {
            let field = address_field(n);
            if field && minimized_address(n) {
                out.minimized.push(n.frame);
            }
            out.addresses.push(
                field
                    .then(|| n.value.as_deref().and_then(host_of))
                    .flatten(),
            );
        }
    }
    out
}

impl Chrome {
    /// Nineteenth review (R-D): the host of the page in front — only when exactly one
    /// address-like node is on screen and it is the address field showing a host.
    fn host(&self) -> Option<&str> {
        match self.addresses.as_slice() {
            [Some(h)] => Some(h),
            _ => None,
        }
    }
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

/// Rule T: the main place of an observation, or `None` when it is not something Jarvis
/// read (a SpringBoard screen of icons only, see [`home_icons_only`]). The session leaves
/// out Jarvis's own app when the run does not drive it (the model gets a stub for it).
///
/// The main place is the page's in a listed browser ([`BROWSERS`]): the site when its one
/// address field on screen shows a host, else its pages with no readable address; in any
/// other app it is the app, or its web content when the screen lists nothing outside its
/// web views. A tree from runner text (whose structure on-screen text can forge) is one
/// place: the app, or with a web view [`FlowContext::SiteUnreadable`]. Every place of the
/// observation is [`observed_places`].
pub fn observed_context(
    raw: &Screen,
    compact: &CompactScreen,
    source: ScreenSource,
) -> Option<FlowContext> {
    let places = observed_places(raw, compact, source, None);
    if !places.read {
        return None;
    }
    let main = match &places.top {
        Some(t) if places.browser || !places.base_read => t.clone(),
        _ => places.base,
    };
    Some(main)
}

/// Seventeenth review: the places one observation shows, decided once per observation
/// (one walk of the chrome, O(web views) over the raw indices), and the place of an action
/// on it in O(depth) ([`ScreenPlaces::destinations`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenPlaces {
    /// The place of everything outside every web view (the destination of an input there):
    /// the app — in a listed browser, its chrome (nineteenth review) — or, for runner text
    /// with a web view, [`FlowContext::SiteUnreadable`].
    base: FlowContext,
    /// Whether the screen shows content of `base` (a screen with no web view always does;
    /// with web views, a listed element outside them — in a listed browser, other than its
    /// one address field showing the host).
    base_read: bool,
    /// The screen is a listed browser's.
    browser: bool,
    /// Nesting level 1: the place of a top-level web view's content — a listed browser's
    /// site or its pages with no readable address, any other app's web content
    /// ([`FlowContext::Embedded`], `within: None`).
    top: Option<FlowContext>,
    /// Nesting level 2 and deeper: the place of a web view nested in another web view
    /// ([`FlowContext::Embedded`] within the site, or [`FlowContext::NestedWeb`]).
    nested: Option<FlowContext>,
    /// The embedded classes on this screen that cover all of themselves: those that do from
    /// their first observation ([`FlowContext::always_covering`]) and a site's frames when
    /// this screen holds two of them.
    ambiguous: Vec<FlowContext>,
    /// Every document web view's raw index, ascending (`jarvis_observe::CompactView::webviews`).
    webviews: Vec<u32>,
    /// The screen is something Jarvis read (not an icons-only home screen).
    read: bool,
    /// Twentieth review (R20-B): the raw tree holds a `Sheet` outside every web view.
    sheet: bool,
    /// Twentieth review (R20-B): the raw tree holds a `Sheet` outside every web view whose
    /// subtree holds the OS's "AirDrop" node, on screen or not.
    share_sheet: bool,
}

impl Default for ScreenPlaces {
    fn default() -> Self {
        ScreenPlaces {
            base: FlowContext::App(String::new()),
            base_read: true,
            browser: false,
            top: None,
            nested: None,
            ambiguous: Vec::new(),
            webviews: Vec::new(),
            read: false,
            sheet: false,
            share_sheet: false,
        }
    }
}

impl ScreenPlaces {
    /// Twentieth review (R20-B): the raw tree holds a `Sheet` (outside every web view).
    pub fn shows_sheet(&self) -> bool {
        self.sheet
    }

    /// Twentieth review (R20-B): the raw tree holds a share sheet — a `Sheet` (outside every
    /// web view) whose subtree holds the OS's "AirDrop" node, whether that node is on
    /// screen, hidden or scrolled away. The session keeps it sticky
    /// ([`share_sheet_control`]).
    pub fn share_sheet(&self) -> bool {
        self.share_sheet
    }

    /// The place of everything outside every web view (also for an icons-only home screen,
    /// which is not a read): the app, in a listed browser its chrome.
    pub fn base(&self) -> &FlowContext {
        &self.base
    }

    /// The first embedded web content on this screen (the top level, else the nested), if
    /// any.
    pub fn embedded(&self) -> Option<&FlowContext> {
        self.top.as_ref().or(self.nested.as_ref())
    }

    /// Every embedded web content place on this screen (top level, then nested).
    pub fn embedded_places(&self) -> Vec<FlowContext> {
        self.top.iter().chain(self.nested.iter()).cloned().collect()
    }

    /// The embedded classes on this screen that cover all of themselves (nineteenth review:
    /// every one with no OS-given name, and a site's frames when the screen holds two;
    /// [`Confinement::mark_ambiguous`]).
    pub fn ambiguous(&self) -> &[FlowContext] {
        &self.ambiguous
    }

    /// The places reading this screen reads: every place whose content it shows (none for
    /// an icons-only home screen), the base first.
    pub fn read_places(&self) -> Vec<FlowContext> {
        if !self.read {
            return Vec::new();
        }
        self.all()
    }

    /// Every place whose content this screen shows, the base first (when it shows any of
    /// it; nineteenth review: a browser's chrome showing nothing but the address field is
    /// not shown content).
    pub fn all(&self) -> Vec<FlowContext> {
        let mut out = Vec::new();
        if self.base_read || (self.top.is_none() && self.nested.is_none()) {
            out.push(self.base.clone());
        }
        out.extend(self.embedded_places());
        out
    }

    /// Every place on this screen, shown content or not: [`Self::all`] and the base (what an
    /// input whose place is not known may reach, and what leaving the screen leaves).
    pub fn every(&self) -> Vec<FlowContext> {
        let mut out = vec![self.base.clone()];
        out.extend(self.embedded_places());
        out
    }

    /// Twenty-third review (R23): the places on this screen that see a model-chosen step
    /// whose own destinations are `own` without receiving its input — every place on the
    /// screen ([`Self::every`]) that is not one of them. Each is an observe-kind destination
    /// of the step ([`InputKind::Observe`]).
    pub fn observers(&self, own: &[FlowContext]) -> Vec<FlowContext> {
        self.every()
            .into_iter()
            .filter(|p| !own.contains(p))
            .collect()
    }

    /// The place of the content of raw node `n`: how many web views hold it (itself
    /// included), walking its ancestors — O(depth · log web views).
    fn of_node(&self, n: u32, tree: &ScreenTree<'_>) -> &FlowContext {
        if self.top.is_none() && self.nested.is_none() {
            return &self.base;
        }
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
            1 => self.top.as_ref().unwrap_or(&self.base),
            // A nested web view exists whenever depth 2 is reached; the fallback is never
            // taken.
            _ => self
                .nested
                .as_ref()
                .or(self.top.as_ref())
                .unwrap_or(&self.base),
        }
    }

    /// The place of a listed element; every place on the screen when its raw node is not
    /// known (fail closed).
    fn of_element(&self, el: &Element, tree: Option<&ScreenTree<'_>>, out: &mut Vec<FlowContext>) {
        if self.top.is_none() && self.nested.is_none() {
            out.push(self.base.clone());
            return;
        }
        match tree
            .filter(|_| !self.webviews.is_empty())
            .and_then(|t| t.source(el).map(|s| (t, s)))
        {
            Some((t, s)) => out.push(self.of_node(s, t).clone()),
            None => out.extend(self.every()),
        }
    }

    /// The places under point `p`: the screen's own place and the place inside every drawn
    /// web view whose frame holds it (what is under a point can be any of them: fail
    /// closed). Without the drawn nodes, every place on the screen.
    fn of_point(&self, p: Point, tree: Option<&ScreenTree<'_>>, out: &mut Vec<FlowContext>) {
        out.push(self.base.clone());
        if self.top.is_none() && self.nested.is_none() {
            return;
        }
        let Some(t) = tree.filter(|t| !t.nodes.is_empty() && !self.webviews.is_empty()) else {
            out.extend(self.embedded_places());
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
            None => out.extend(self.every()),
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
                    out.extend(self.every());
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
                    Err(_) => out.extend(self.every()),
                }
            }
            // Twenty-third review (R23): a wait, a look and leaving are seen by every place
            // on the screen.
            Action::Wait { .. } | Action::Look | Action::PressHome => out.extend(self.every()),
            Action::OpenApp { .. }
            | Action::OpenUrl { .. }
            | Action::CallTool { .. }
            | Action::AskUser { .. }
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
    let chrome = scan_chrome(&raw.root, &raw.bounds());
    let mut places = places_of(raw, compact, source, view, &chrome);
    places.sheet = chrome.sheet;
    places.share_sheet = chrome.share_sheet;
    places
}

/// [`observed_places`] from the chrome walk.
fn places_of(
    raw: &Screen,
    compact: &CompactScreen,
    source: ScreenSource,
    view: Option<(&[u32], &[u32], &[u32])>,
    chrome: &Chrome,
) -> ScreenPlaces {
    let app = raw.app_bundle_id.trim().to_lowercase();
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
    // Runner text: its structure is forgeable, so its web views are not told apart from the
    // app or from each other: one place, covering from its first observation.
    if source == ScreenSource::RunnerText {
        let s = FlowContext::SiteUnreadable(app);
        return ScreenPlaces {
            ambiguous: vec![s.clone()],
            ..single(s)
        };
    }
    // Eighteenth review (E3): a host is read from the chrome of a listed browser only.
    // Nineteenth review (R-D): from its address field, the only address-like node on screen.
    let browser = is_browser(&app);
    let host = chrome.host().filter(|_| browser).map(str::to_owned);
    // Top-level web views (their raw subtree ranges, disjoint and ascending) and how many
    // web views lie inside another: one pass over the web views.
    let (webviews, ends, sources) = view.unwrap_or((&[], &[], &[]));
    let end = |w: u32| {
        usize::try_from(w)
            .ok()
            .and_then(|i| ends.get(i))
            .copied()
            .unwrap_or(0)
    };
    let mut tops: Vec<(u32, u32)> = Vec::new();
    let mut nested_docs = 0usize;
    for &w in webviews {
        if tops.last().is_some_and(|&(a, e)| w > a && w < e) {
            nested_docs += 1;
        } else {
            tops.push((w, end(w).max(w.saturating_add(1))));
        }
    }
    // Content outside the web views: a listed element whose raw node no top-level web view
    // holds (by the listed elements when the tree is known, else by the chrome walk). In a
    // listed browser whose host is read, its one address field showing that host is the
    // page's own name, not content of the chrome (nineteenth review, R-C).
    // Run 122: Safari's minimized bar lists the same field as an `Other` (a listed element
    // carries no identifier): it is the page's name only when the one address field read is
    // that `Other` node (the same frame, up to compaction's float rounding); any other
    // `Other` labelled as the address is content.
    let same_frame = |a: &jarvis_proto::Rect, b: &jarvis_proto::Rect| {
        let near = |p: f64, q: f64| (p - q).abs() <= 0.5;
        near(a.x, b.x) && near(a.y, b.y) && near(a.width, b.width) && near(a.height, b.height)
    };
    let is_address = |e: &Element| {
        let role = match &e.role {
            Role::TextField | Role::Button => true,
            Role::Other(name) => {
                name == "Other" && chrome.minimized.iter().any(|f| same_frame(f, &e.frame))
            }
            _ => false,
        };
        host.is_some() && role && ADDRESS_LABELS.contains(&e.label.trim().to_lowercase().as_str())
    };
    let base_read = if view.is_some() && !tops.is_empty() {
        sources.iter().zip(&compact.elements).any(|(&s, e)| {
            let k = tops.partition_point(|&(a, _)| a <= s);
            let inside = k > 0 && tops.get(k - 1).is_some_and(|&(a, e)| s >= a && s < e);
            !inside && !is_address(e)
        })
    } else if host.is_some() {
        chrome.outside_not_address
    } else {
        chrome.outside
    };
    // Nineteenth review (R-C): everything outside every web view is the app's own place —
    // in a listed browser its chrome, whatever site is shown. The page inside the top-level
    // web views is the site (its host read, R-D), else a listed browser's pages with no
    // readable address, else (any other app) its web content; what is nested in a web view
    // is a place of its own (eighteenth review, E5a), deeper levels sharing one.
    let base = FlowContext::App(app.clone());
    let top = Some(match (&host, browser) {
        (Some(h), _) => FlowContext::Site {
            app: app.clone(),
            host: h.clone(),
        },
        (None, true) => FlowContext::SiteUnreadable(app.clone()),
        (None, false) => FlowContext::Embedded {
            app: app.clone(),
            within: None,
        },
    });
    let nested = (nested_docs > 0).then(|| match host {
        Some(h) => FlowContext::Embedded {
            app: app.clone(),
            within: Some(h),
        },
        None => FlowContext::NestedWeb(app.clone()),
    });
    // Nineteenth review (R-B): every class with no OS-given name covers all of itself from
    // its first observation; a site's frames once this screen holds two of them (eighteenth
    // review, E5b).
    let ambiguous: Vec<FlowContext> = top
        .iter()
        .chain(nested.iter())
        .filter(|c| {
            c.always_covering() || (matches!(c, FlowContext::Embedded { .. }) && nested_docs > 1)
        })
        .cloned()
        .collect();
    ScreenPlaces {
        base,
        base_read,
        browser,
        top,
        nested,
        ambiguous,
        webviews: webviews.to_vec(),
        read: true,
        sheet: false,
        share_sheet: false,
    }
}

/// Seventeenth review: whether `action` is an input Rule T checks — every model-chosen
/// action except the text of `done`, `fail` and `ask_user` (shown on the phone; a task must
/// be able to end and to ask). Taps, long presses, `tap_point`, swipes and scrolls, typing,
/// clearing, `press_home` (into every place it leaves), `open_app` and a home-screen launch
/// (into the app), `open_url` (into the link's site, whoever gave it) and every tool call,
/// with or without arguments.
///
/// Twenty-third review (R23): `wait` and `look` too. Every observation is an accessibility
/// snapshot served by the app in front (a page's content is read through its web content
/// process), so a model-chosen wait or look is seen, at a moment the model chose, by every
/// place on the screen: an input of the weakest kind ([`InputKind::Observe`]) into each of
/// them. Every other model-chosen step is seen the same way by every place on the screen
/// besides its own destinations ([`ScreenPlaces::observers`]).
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
        | Action::CallTool { .. }
        | Action::Wait { .. }
        | Action::Look => true,
        Action::AskUser { .. } | Action::Done { .. } | Action::Fail { .. } => false,
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
    /// Per destination, per source: the strongest kind of input the owner allowed (twentieth
    /// review, R20-A: a scroll's approval keeps a scroll-only grant; twenty-third, R23: an
    /// observe question's approval an observe-only one). A grant covers its kind and every
    /// weaker kind, never a stronger one.
    granted: BTreeMap<FlowContext, BTreeMap<FlowContext, InputKind>>,
    declined: BTreeMap<FlowContext, BTreeSet<FlowContext>>,
    ambiguous: BTreeSet<FlowContext>,
}

/// Twentieth review (R20-A): the kind of input a flow question is shown for, weakest first.
/// An approval kept for a kind never covers a stronger one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InputKind {
    /// Twenty-third review (R23): being seen. `wait`, `look` and `press_home` are this kind
    /// into every place on the screen; every other model-chosen step is this kind into every
    /// place on the screen that is not one of its own destinations
    /// ([`ScreenPlaces::observers`]). Leaving is the same strength as being looked at.
    Observe,
    /// A scroll that only moves content (twenty-first review, R21-A: see [`InputKind::of`]).
    Scroll,
    /// Everything else: a tap, long press, typing, clear, Return, a tool call, `open_app`,
    /// `open_url`, a home-screen launch, a swipe — and a scroll that names a control or works
    /// one.
    Full,
}

impl InputKind {
    /// The kind of `action` on `screen` (the screen it will act on). Twenty-third review
    /// (R23): `wait`, `look` and `press_home` are of the *observe* kind. Twenty-first review
    /// (R21-A): an action is of the *scroll* kind only when all of these hold:
    ///
    /// 1. it is `Action::Scroll`;
    /// 2. it names no element, or its element is a scroll container of the screen — a role
    ///    the tree gives (scroll view, table, collection view, web view: the set the hit
    ///    test calls scrollable, `hit::is_scroller`), never decided by a label;
    /// 3. the drag it will produce can be computed (`gestures::drag_gesture`, the path the
    ///    runner is sent) and works no control: the one drag classification
    ///    (`actuation` → `classify::drag_target`) finds no control it starts on, crosses
    ///    and reacts to, or is named (`Actuation::Navigation`).
    ///
    /// Everything else is *full*: a scroll on a picker wheel, slider, switch, a row dragged
    /// sideways, a button or any other non-container element (the drag's touch-down lands
    /// on the element the model chose), or one whose path crosses a control that reacts to
    /// it, or whose path cannot be computed. Only a full grant covers a full input. The
    /// session derives both the grants a scroll may use and what its approval keeps from
    /// this one function. O(elements) for a scroll; nothing for any other action.
    pub fn of(action: &Action, screen: &CompactScreen) -> InputKind {
        if matches!(
            action,
            Action::Wait { .. } | Action::Look | Action::PressHome
        ) {
            return InputKind::Observe;
        }
        let Action::Scroll { direction, element } = action else {
            return InputKind::Full;
        };
        let frame = match element {
            None => None,
            Some(i) => match screen.get(*i) {
                Some(e) if crate::hit::is_scroller(&e.role) => Some(e.frame),
                _ => return InputKind::Full,
            },
        };
        if drag_gesture(GestureKind::Scroll, *direction, frame, screen).is_err() {
            return InputKind::Full;
        }
        match actuation(action, screen) {
            Actuation::Navigation => InputKind::Scroll,
            _ => InputKind::Full,
        }
    }
}

impl Confinement {
    /// An empty record (a new task).
    pub fn new() -> Self {
        Self::default()
    }

    /// Eighteenth review (E5b): `context` (an embedded class) held more than one document in
    /// an observation of this task, so Jarvis cannot tell its parts apart across looks. From
    /// now on it is a source for input into itself (like a tool's own results), and its name
    /// in a flow note says it covers all of it ([`FlowContext::display_covering`]). An
    /// approval given before it was known to be ambiguous named only "embedded content":
    /// it is dropped (fail closed), so the next input asks with the covering name.
    ///
    /// Nineteenth review: a class that covers all of itself from its first observation
    /// ([`FlowContext::always_covering`]) was never named otherwise; marking it changes
    /// nothing. Only a site's frames still become ambiguous here.
    pub fn mark_ambiguous(&mut self, context: FlowContext) {
        if context.always_covering() || !self.ambiguous.insert(context.clone()) {
            return;
        }
        self.granted.remove(&context);
        for sources in self.granted.values_mut() {
            sources.remove(&context);
        }
    }

    /// Whether `context` covers all of itself in this task: from its first observation
    /// ([`FlowContext::always_covering`]) or since it was marked ([`Self::mark_ambiguous`]).
    pub fn is_ambiguous(&self, context: &FlowContext) -> bool {
        context.always_covering() || self.ambiguous.contains(context)
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

    /// Whether `source` may reach `target` with an input of `kind` (nineteenth review:
    /// exactly that pair): a grant covers its own kind and every weaker one — a full grant
    /// every kind, a scroll grant scrolls and observing (twentieth review, R20-A), an observe
    /// grant observing only (twenty-third review, R23). A grant never upgrades.
    fn allowed_kind(&self, target: &FlowContext, source: &FlowContext, kind: InputKind) -> bool {
        self.granted(target, source).is_some_and(|g| g >= kind)
    }

    /// The strongest kind of input the owner allowed `source` into `target` with, if any.
    pub fn granted(&self, target: &FlowContext, source: &FlowContext) -> Option<InputKind> {
        self.granted
            .get(target)
            .and_then(|a| a.get(source))
            .copied()
    }

    /// The order a source was last read in (0: never read, e.g. only on the clipboard);
    /// higher is more recent.
    pub fn recency(&self, context: &FlowContext) -> u64 {
        self.read.get(context).copied().unwrap_or(0)
    }

    /// The contexts whose content may flow into `target` and that the owner has not allowed
    /// into it in this task: what was read (and, with `pasting`, what the clipboard may
    /// hold), but `target` itself unless it covers all of itself ([`Self::is_ambiguous`]).
    /// Every other place is a source (nineteenth review: no one-way relation). Most
    /// recently read first.
    pub fn unallowed(&self, target: &FlowContext, pasting: bool) -> Vec<FlowContext> {
        self.unallowed_into(std::slice::from_ref(target), pasting, false)
    }

    /// [`Self::unallowed`] into any of `targets` (an input whose place is not one place:
    /// every place it may reach). `self_source`: the target's own earlier content is a
    /// source too (a tool that is not self-contained reaches many places). An ambiguous
    /// embedded class ([`Self::mark_ambiguous`]) is always a source for itself.
    pub fn unallowed_into(
        &self,
        targets: &[FlowContext],
        pasting: bool,
        self_source: bool,
    ) -> Vec<FlowContext> {
        self.unallowed_kind(targets, pasting, self_source, InputKind::Full)
    }

    /// [`Self::unallowed_into`] for an input of `kind` (twentieth review: a scroll is allowed
    /// by a scroll grant too).
    pub fn unallowed_kind(
        &self,
        targets: &[FlowContext],
        pasting: bool,
        self_source: bool,
        kind: InputKind,
    ) -> Vec<FlowContext> {
        let clip = self.clipboard.iter().filter(|_| pasting);
        let mut out: Vec<FlowContext> = Vec::new();
        for target in targets {
            for c in self.read.keys().chain(clip.clone()) {
                if (c == target && !self_source && !self.is_ambiguous(c))
                    || self.allowed_kind(target, c, kind)
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
        self.allow_kind_into(targets, sources, InputKind::Full);
    }

    /// Twentieth review (R20-A): the owner approved a question shown for a scroll: `sources`
    /// may reach each of `targets` by later scrolls for the rest of the task, not by any
    /// other input. Nothing is kept for [`FlowContext::Unknown`].
    pub fn allow_scroll_into(&mut self, targets: &[FlowContext], sources: &[FlowContext]) {
        self.allow_kind_into(targets, sources, InputKind::Scroll);
    }

    /// Twenty-third review (R23): `sources` may reach each of `targets` by later inputs of
    /// `kind` or a weaker kind for the rest of the task (an observe question's approval: by
    /// waits, looks and leaving only). A grant never lowers one already kept. Nothing is kept
    /// for [`FlowContext::Unknown`].
    pub fn allow_kind_into(
        &mut self,
        targets: &[FlowContext],
        sources: &[FlowContext],
        kind: InputKind,
    ) {
        for t in targets.iter().filter(|t| **t != FlowContext::Unknown) {
            let entry = self.granted.entry(t.clone()).or_default();
            for s in sources {
                let g = entry.entry(s.clone()).or_insert(kind);
                *g = (*g).max(kind);
            }
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

/// Nineteenth review (N4): whether `action` presses something on a share (activity) sheet.
/// Every row and button of one may run another party's code with the shared content — a
/// person or device (AirDrop), an app's share extension, a third-party action extension,
/// whose title the extension chooses ("Copy", "Close") — so a press there always asks,
/// phone only, whatever its label (the hand-on items' rule, [`handoff_control`]).
///
/// What a share sheet is rests on the roles the code knows, nothing else: a raw `Sheet`
/// whose subtree holds a node whose whole label is "AirDrop" (the OS's row; the repository
/// holds no real share-sheet tree, so this is an open device spike).
///
/// Twentieth review (R20-B): `share` is that fact decided from the raw tree by whoever holds
/// it ([`ScreenPlaces::share_sheet`]: the AirDrop node counts whether it is on screen,
/// hidden or scrolled away; the session keeps it sticky while a `Sheet` stays up in the same
/// app, for cell reuse that drops the row from the tree). With `Some(true)`, every press
/// asks (on the sheet, or beside it, which the sheet covers or which dismisses it: an
/// over-ask, never an under-ask); with `Some(false)`, no press is a share-sheet press. With
/// `None` (no raw tree), the listed elements decide: a press on a screen that lists an
/// "AirDrop" element asks, under a `Sheet` when the tree's shape is known.
pub fn share_sheet_control(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    share: Option<bool>,
) -> bool {
    // Only a press can be one (typing, Return, drags: nothing to scan).
    let el = match actuation(action, screen) {
        Actuation::Press(el) => Some(el),
        Actuation::PressUnknown => None,
        _ => return false,
    };
    // From the raw tree: while a share sheet is up, every press asks (on the sheet or
    // beside it: what lies beside it is covered or dismisses it; never fewer questions).
    if let Some(share) = share {
        return share;
    }
    // Without the raw tree: the OS's whole label "AirDrop" (or "Air Drop") among the listed
    // elements, one pass.
    let airdrop: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| airdrop_label(&e.label))
        .collect();
    if airdrop.is_empty() {
        return false;
    }
    let Some(el) = el else {
        return true;
    };
    let Some(t) = tree.filter(|t| !t.nodes.is_empty() && t.source(el).is_some()) else {
        return true;
    };
    t.ancestors(el).into_iter().any(|a| {
        crate::hit::raw_node(t, a).is_some_and(|n| n.role == Role::Sheet)
            && airdrop.iter().any(|d| t.inside(d, a))
    })
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
        // Twentieth review: whitespace or an invisible character inside fails closed.
        assert_eq!(host_of("bank.example login"), None);
        assert_eq!(host_of("bank.\u{200B}example"), None);
        assert_eq!(
            host_of(" \u{200E}bank.example ").as_deref(),
            Some("bank.example")
        );
    }

    /// Run 122: Safari's minimized bar is an `Other "Address" id=TabBarItemTitle`; an
    /// `Other` is the address field only with that exact identifier and a label of its own.
    #[test]
    fn the_minimized_bar_is_an_address_field_only_with_its_identifier_and_label() {
        let frame = jarvis_proto::Rect::new(195.3, 547.0, 49.3, 14.3);
        let other = || UiNode::new(Role::Other("Other".into()), frame);
        let minimized = other()
            .with_label("Address")
            .with_value("\u{200E}127.0.0.1")
            .with_identifier("TabBarItemTitle");
        assert!(address_field(&minimized));
        // Not without the identifier, with another identifier, or with another role name.
        assert!(!address_field(
            &other()
                .with_label("Address")
                .with_value("\u{200E}127.0.0.1")
        ));
        assert!(!address_field(
            &other()
                .with_label("Address")
                .with_value("bank.example")
                .with_identifier("URL")
        ));
        assert!(!address_field(
            &UiNode::new(Role::Other("Group".into()), frame)
                .with_label("Address")
                .with_value("bank.example")
                .with_identifier("TabBarItemTitle")
        ));
        // Not the pre-load node (no label of its own), nor a tab title, nor while focused.
        assert!(!address_field(&other().with_identifier("TabBarItemTitle")));
        assert!(!address_field(
            &other()
                .with_label("bank.example")
                .with_value("bank.example")
                .with_identifier("TabBarItemTitle")
        ));
        assert!(!address_field(&minimized.clone().focused(true)));
        // The existing carriers are unchanged.
        assert!(address_field(
            &UiNode::new(Role::TextField, frame).with_label("Address")
        ));
        assert!(!address_field(
            &UiNode::new(Role::StaticText, frame).with_label("Address")
        ));
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
