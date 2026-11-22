//! The compacted view of a screen: the flat, indexed element list the model chooses from.
//!
//! These are data types only. `jarvis-observe` builds them from a [`crate::Screen`];
//! `jarvis-safety`, `jarvis-memory` and `jarvis-agent` consume them.

use serde::{Deserialize, Serialize};

use crate::{Rect, Role, Size};

/// One actionable or informative element of the current screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    /// Index the model refers to. Starts at 1, valid for this screen only.
    pub index: u32,
    /// Element type.
    pub role: Role,
    /// Human-readable label (may be merged from child text; empty when the element has none).
    pub label: String,
    /// Current value. Always `None` for secure fields.
    #[serde(default)]
    pub value: Option<String>,
    /// Placeholder text of an empty text input.
    #[serde(default)]
    pub placeholder: Option<String>,
    /// Frame in screen points.
    pub frame: Rect,
    /// Whether the element accepts interaction.
    pub enabled: bool,
    /// Whether it has keyboard focus.
    #[serde(default)]
    pub focused: bool,
    /// Whether it is selected.
    #[serde(default)]
    pub selected: bool,
    /// Whether it is a secure (password) field.
    #[serde(default)]
    pub secure: bool,
    /// Identity of "the same control" across runs. Independent of frame and index.
    pub fingerprint: u64,
}

/// A screen reduced to its indexed elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactScreen {
    /// Bundle id of the foreground app.
    pub app_bundle_id: String,
    /// Display name of the foreground app, when known.
    #[serde(default)]
    pub app_name: Option<String>,
    /// Screen size in points.
    pub size: Size,
    /// Whether the software keyboard is up.
    #[serde(default)]
    pub keyboard_visible: bool,
    /// Elements ordered top-to-bottom then left-to-right, indices ascending from 1.
    pub elements: Vec<Element>,
    /// What the tree says about content that is not listed (more to scroll to, the
    /// home-screen page). Numbers and flags only, never screen text. Omitted from JSON when
    /// empty; absent in older JSON.
    #[serde(default, skip_serializing_if = "ScreenCues::is_empty")]
    pub cues: ScreenCues,
}

/// Hints derived from the tree about content beyond what is listed.
///
/// `jarvis-observe` fills these only where the tree supports the inference (see
/// `jarvis_observe::compact`); a screen without the signals has none.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScreenCues {
    /// A scrollable list continues above what is visible.
    #[serde(default)]
    pub more_above: bool,
    /// A scrollable list continues below what is visible.
    #[serde(default)]
    pub more_below: bool,
    /// A page indicator: `(current, count)`, 1-based, e.g. the home screen's "Page 1 of 2".
    #[serde(default)]
    pub page: Option<(u32, u32)>,
    /// The label of the on-screen keyboard's Return key (`"send"`, `"go"`, `"search"`,
    /// `"return"`), when a keyboard is up. A fact about the screen, not an element: it has no
    /// index and is never tapped. Screen text: bounded by `jarvis-observe`, sanitised before
    /// it reaches a prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_key: Option<String>,
}

impl ScreenCues {
    /// True when there is nothing to say.
    pub fn is_empty(&self) -> bool {
        !self.more_above && !self.more_below && self.page.is_none() && self.return_key.is_none()
    }
}

/// A raw navigation, tab or tool bar node of the tree a compact screen was built from: bars
/// are containers, never listed, but they are drawn above scrolling content, which decides
/// where a tap lands (`jarvis_safety::tap_spot`). `jarvis-observe` lists every shown
/// `NavigationBar`/`TabBar`/`Toolbar` node outside an alert and outside page content; `jarvis-safety` decides which of them
/// count (full width, typed tree only). Kept out of the FFI, beside the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarNode {
    /// The bar's raw node, in the digest's node order (`jarvis_observe::CompactView`).
    pub node: u32,
    /// Its role (`NavigationBar`, `TabBar` or `Toolbar`).
    pub role: Role,
    /// Its shown frame (clipped to the screen, as an element's frame is).
    pub frame: Rect,
}

/// A drawn raw node of the tree a compact screen was built from, for the tap-point rule
/// (`jarvis_safety::tap_spot`): any node whose frame contains a point may take a touch
/// there, whatever its role. `jarvis-observe` lists every drawn node with any part of its
/// frame on screen. Kept out of the FFI, beside the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawNode {
    /// The node, in the digest's node order (`jarvis_observe::CompactView`).
    pub node: u32,
    /// Its role.
    pub role: Role,
    /// The part of its frame on screen.
    pub frame: Rect,
    /// Its label, when it has one and its text is hashed (never inside the keyboard or the
    /// input assistant): how a refusal names it.
    pub label: Option<String>,
    /// It shows text: a non-empty label, or a value that is not a secure field's (always
    /// false inside the keyboard and the input assistant).
    pub text: bool,
    /// It is part of the keyboard: the `Keyboard` node or inside it, or inside the input
    /// assistant (prediction bar) next to it (see `jarvis_observe`'s layout docs). The
    /// keyboard window's other nodes are not.
    pub keyboard: bool,
}

/// A text input of the raw tree a compact screen was built from, listed or not (eighth
/// review): what a submit sends includes the form's fields that scrolled out of view
/// (`jarvis_safety`'s summaries). `jarvis-observe` lists every text input whose text is
/// hashed by the digest (not excluded, not inside the keyboard or the input assistant),
/// shown or not, so nothing here can change under an approval unnoticed. Kept out of the
/// FFI, beside the screen.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct RawInput {
    /// The node, in the digest's node order (`jarvis_observe::CompactView`).
    pub node: u32,
    /// Its role (a text input).
    pub role: Role,
    /// Its label (whitespace collapsed; `None` when blank).
    pub label: Option<String>,
    /// Its identifier.
    pub identifier: Option<String>,
    /// Its placeholder.
    pub placeholder: Option<String>,
    /// Its value (whitespace collapsed); never a secure field's, nor one equal to the
    /// placeholder.
    pub value: Option<String>,
    /// It is a secure field.
    pub secure: bool,
    /// Some part of it is drawn on screen (no pruned ancestor, frame on screen).
    pub on_screen: bool,
}

impl std::fmt::Debug for RawInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawInput")
            .field("node", &self.node)
            .field("role", &self.role)
            .field(
                "value_chars",
                &self.value.as_ref().map(|v| v.chars().count()),
            )
            .field("secure", &self.secure)
            .field("on_screen", &self.on_screen)
            .finish()
    }
}

impl CompactScreen {
    /// The element with this index, if present.
    pub fn get(&self, index: u32) -> Option<&Element> {
        self.elements.iter().find(|e| e.index == index)
    }

    /// The first element with this fingerprint, if present.
    pub fn find_by_fingerprint(&self, fingerprint: u64) -> Option<&Element> {
        self.elements.iter().find(|e| e.fingerprint == fingerprint)
    }
}
