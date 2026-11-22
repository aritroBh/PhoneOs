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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
}

impl ScreenCues {
    /// True when there is nothing to say.
    pub fn is_empty(&self) -> bool {
        !self.more_above && !self.more_below && self.page.is_none()
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
