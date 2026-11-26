//! The JSON schema of a simulated app, as Rust types.
//!
//! An app file is one JSON object:
//!
//! ```json
//! {
//!   "id": "settings",                      // short name used by the CLI and predicates
//!   "bundle_id": "com.apple.Preferences",
//!   "name": "Settings",
//!   "start": "root",                       // screen shown on first launch
//!   "state": { "wifi": true },             // initial state variables (any JSON)
//!   "url_prefixes": ["prefs:"],            // open_url targets that launch this app
//!   "screens": { "root": { "title": "Settings", "elements": [ ... ] } },
//!   "alerts":  { "confirm": { "title": "...", "message": "...", "buttons": [ ... ] } }
//! }
//! ```
//!
//! # Screens and elements
//!
//! A screen with a `title` gets a navigation bar (large title, a back button when the
//! navigation stack is deeper than one screen, plus its `nav_buttons`). Content starts at
//! y = 150 on a 393 x 852 point screen.
//!
//! Every element's `frame` is `[x, y, w, h]` **relative to its parent's origin**
//! (top-level elements: the screen). A parent with `"layout": "column"` stacks children
//! that have no `frame`: full parent width, height `h` (default 44), `gap` points apart.
//! `"scroll": true` makes a column scrollable: only rows intersecting the viewport exist in
//! the tree, as in a UIKit table. `"for_each": "<expr>"` repeats the element for every item
//! of a list, binding the item to `"item"` (default name `item`) and its position to
//! `index`.
//!
//! Fields of an element: `id`, `kind` (`"row"`: a table cell with `label`, `detail`,
//! `switch`), `role` (any runner type name, default `other`), `label`, `value`,
//! `placeholder`, `frame`, `h`, `when` (visible if the expression is true), `enabled`,
//! `selected`, `on_tap`, `on_submit`, `bind` (state variable edited by typing; makes the
//! element a text input that takes focus when tapped), `secure`, `layout`, `gap`, `scroll`,
//! `for_each`, `item`, `switch` (state variable shown as an on/off switch inside a row),
//! `opaque` (swallows taps that reach it), `reveal`, `actions` and `on_full_swipe` (a row's
//! swipe actions, see [`ElementDef::reveal`]) and `children`.
//!
//! All string fields are templates, see [`crate::expr`]: `{wifi ? '1' : '0'}`, `{total|money}`.
//!
//! # Effects
//!
//! `on_tap` and `on_submit` hold lists of effects, each an object with a `"do"` tag:
//!
//! | `do` | fields | meaning |
//! |---|---|---|
//! | `goto` | `screen`, `replace`, `reset` | push, replace or reset to a screen |
//! | `back` | | pop the navigation stack |
//! | `set` | `var`, `value` | assign a state path (`"thread.{current}"`) |
//! | `toggle` | `var` | flip a boolean |
//! | `add` | `var`, `by` | add a number |
//! | `append` | `to`, `value` | push onto a list |
//! | `remove` | `from`, `index` | remove a list item by position |
//! | `alert` | `id` | show an alert from `alerts` |
//! | `dismiss` | | close the alert |
//! | `launch` | `app` | bring another app to the front |
//! | `if` | `cond`, `then`, `else` | conditional effects |
//!
//! Effect values are templates evaluated when the tap happens, against the state and the
//! element's `for_each` scope.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One simulated app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppDef {
    /// Short name (`settings`).
    pub id: String,
    /// Bundle id reported to the agent.
    pub bundle_id: String,
    /// Display name.
    pub name: String,
    /// Screen shown first.
    pub start: String,
    /// Initial state variables (a JSON object).
    #[serde(default)]
    pub state: Value,
    /// URL prefixes that `open_url` routes to this app.
    #[serde(default)]
    pub url_prefixes: Vec<String>,
    /// Screens by name.
    pub screens: BTreeMap<String, ScreenDef>,
    /// Alerts by name.
    #[serde(default)]
    pub alerts: BTreeMap<String, AlertDef>,
}

/// One screen of an app.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenDef {
    /// Navigation bar title; no title means no navigation bar.
    #[serde(default)]
    pub title: Option<String>,
    /// Buttons on the right of the navigation bar (frames relative to the bar).
    #[serde(default)]
    pub nav_buttons: Vec<ElementDef>,
    /// Content, top to bottom in paint order.
    #[serde(default)]
    pub elements: Vec<ElementDef>,
}

/// One element of a screen.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ElementDef {
    /// Accessibility identifier and focus key.
    pub id: Option<String>,
    /// Preset: `"row"` builds a table cell from `label`, `detail` and `switch`.
    pub kind: Option<String>,
    /// Runner type name (`button`, `static_text`, `cell`, ...).
    pub role: Option<String>,
    /// Accessibility label (template).
    pub label: Option<String>,
    /// Trailing text of a row (template).
    pub detail: Option<String>,
    /// Value (template).
    pub value: Option<String>,
    /// Placeholder of a text input (template).
    pub placeholder: Option<String>,
    /// `[x, y, w, h]` relative to the parent.
    pub frame: Option<[f64; 4]>,
    /// Height when stacked in a column.
    pub h: Option<f64>,
    /// Visible when this expression is true.
    pub when: Option<String>,
    /// Enabled when this expression is true (default true).
    pub enabled: Option<String>,
    /// Selected when this expression is true.
    pub selected: Option<String>,
    /// Effects of a tap.
    pub on_tap: Vec<Effect>,
    /// Effects of the Return key while this input is focused.
    pub on_submit: Vec<Effect>,
    /// State variable edited by typing.
    pub bind: Option<String>,
    /// A secure (password) input.
    pub secure: bool,
    /// `"column"` stacks frameless children.
    pub layout: Option<String>,
    /// Points between stacked children.
    pub gap: f64,
    /// A scrollable column.
    pub scroll: bool,
    /// Repeat for every item of this list expression.
    pub for_each: Option<String>,
    /// Name the item is bound to (default `item`).
    pub item: Option<String>,
    /// Row only: a state variable shown as an on/off switch.
    pub switch: Option<String>,
    /// Swallows taps that reach it even without effects.
    pub opaque: bool,
    /// Row only (Mac run 161, a SwiftUI row with `.swipeActions`): a state variable
    /// (template) holding whether the row's trailing swipe actions are showing. A
    /// horizontal swipe starting on the row reveals, closes or full-swipes it
    /// (`SimPhone`'s swipe: a leftward drag of 40 pt up to under 0.60 of the row reveals,
    /// 0.60 or more is a full swipe; once revealed, 0.45 or more — 0.30 with two actions — is
    /// a full swipe, a rightward drag closes). While revealed the row's cell is drawn shifted
    /// left by 107 pt (one action; 205 with two) and the actions lie in a sibling `Other`
    /// at the row's rest frame, after the cell, as on iOS 26.5; a tap on the shifted row
    /// closes it.
    pub reveal: Option<String>,
    /// Row with `reveal` only: the trailing swipe actions shown while revealed (rightmost
    /// last); each is built like any element (its own `on_tap`).
    pub actions: Vec<ElementDef>,
    /// Row with `reveal` only: the effects of a full swipe (the row's state variable is
    /// reset first).
    pub on_full_swipe: Vec<Effect>,
    /// Child elements.
    pub children: Vec<ElementDef>,
}

/// A modal alert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertDef {
    /// Title (template).
    pub title: String,
    /// Body text (template).
    #[serde(default)]
    pub message: String,
    /// Buttons, top to bottom.
    pub buttons: Vec<AlertButton>,
}

/// One alert button; tapping it closes the alert, then runs `then`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertButton {
    /// Button label.
    pub label: String,
    /// Effects after the alert closes.
    #[serde(default)]
    pub then: Vec<Effect>,
}

/// A state change or navigation caused by a tap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    /// Push (or replace) a screen.
    Goto {
        /// Target screen name.
        screen: String,
        /// Replace the current screen instead of pushing.
        #[serde(default)]
        replace: bool,
        /// Make this the only screen on the stack (no back button).
        #[serde(default)]
        reset: bool,
    },
    /// Pop the navigation stack.
    Back,
    /// Assign a state path.
    Set {
        /// Path template (`name` or `thread.{current}`).
        var: String,
        /// Value template.
        value: Value,
    },
    /// Flip a boolean.
    Toggle {
        /// Path template.
        var: String,
    },
    /// Add a number.
    Add {
        /// Path template.
        var: String,
        /// Amount template.
        by: Value,
    },
    /// Push onto a list.
    Append {
        /// Path template of the list.
        to: String,
        /// Value template.
        value: Value,
    },
    /// Remove a list item by position.
    Remove {
        /// Path template of the list.
        from: String,
        /// Position template.
        index: Value,
    },
    /// Show an alert.
    Alert {
        /// Alert name.
        id: String,
    },
    /// Close the alert.
    Dismiss,
    /// Bring another app to the front.
    Launch {
        /// App id.
        app: String,
    },
    /// Conditional effects.
    If {
        /// Condition expression.
        cond: String,
        /// Effects when true.
        then: Vec<Effect>,
        /// Effects when false.
        #[serde(default, rename = "else")]
        otherwise: Vec<Effect>,
    },
}
