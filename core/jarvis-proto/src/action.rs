//! Model-facing actions: what the model is asked to emit.

use serde::{Deserialize, Serialize};

/// A swipe or scroll direction.
///
/// For [`Action::Scroll`] the direction is where the *content* should move into view
/// (`Down` reveals what is below); for [`Action::Swipe`] it is the finger's direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Towards the top of the screen.
    Up,
    /// Towards the bottom of the screen.
    Down,
    /// Towards the left edge.
    Left,
    /// Towards the right edge.
    Right,
}

/// One action chosen by the model.
///
/// JSON is internally tagged with `"type"` (`{"type":"tap","element":17}`). This is the
/// canonical wire form; the agent's tolerant parser additionally accepts the model's own
/// output shapes. `element` fields are 1-based indices assigned by `jarvis-observe` for the
/// current screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Tap an element.
    Tap {
        /// Element index.
        element: u32,
    },
    /// Press and hold an element.
    LongPress {
        /// Element index.
        element: u32,
        /// Hold duration in milliseconds.
        duration_ms: u64,
    },
    /// Type text, into `element` if given (tapping it first if unfocused) or the focused field.
    Type {
        /// Target text input, or `None` for the focused one.
        #[serde(default)]
        element: Option<u32>,
        /// Text to type.
        text: String,
        /// Press Return afterwards.
        #[serde(default)]
        submit: bool,
    },
    /// Clear a text input.
    Clear {
        /// Element index.
        element: u32,
    },
    /// Scroll content, within `element` or the whole screen.
    Scroll {
        /// Direction to scroll.
        direction: Direction,
        /// Scroll container, or `None` for the screen.
        #[serde(default)]
        element: Option<u32>,
    },
    /// Swipe gesture, on `element` or across the screen.
    Swipe {
        /// Finger direction.
        direction: Direction,
        /// Element to swipe on, or `None` for the screen.
        #[serde(default)]
        element: Option<u32>,
    },
    /// Tap a normalized point (0..1 on each axis); vision fallback only.
    TapPoint {
        /// Normalized horizontal coordinate.
        x: f64,
        /// Normalized vertical coordinate.
        y: f64,
    },
    /// Go to the home screen.
    PressHome,
    /// Launch or foreground an app by bundle id or display name.
    OpenApp {
        /// Bundle id or display name.
        app: String,
    },
    /// Open a URL (web or app URL scheme).
    OpenUrl {
        /// The URL.
        url: String,
    },
    /// Wait for the UI to settle.
    Wait {
        /// Milliseconds to wait.
        ms: u64,
    },
    /// Call a native or Shortcuts tool.
    CallTool {
        /// Tool name as declared in its `ToolSpec`.
        name: String,
        /// Tool arguments as a JSON object.
        #[serde(default)]
        args: serde_json::Value,
    },
    /// Ask the user a question and wait for the answer.
    AskUser {
        /// The question.
        question: String,
    },
    /// Request a screenshot (vision) step.
    Look,
    /// The task is complete.
    Done {
        /// What was accomplished.
        summary: String,
    },
    /// The task cannot be completed.
    Fail {
        /// Why.
        reason: String,
    },
}

impl Action {
    /// The snake_case action name, identical to the JSON `"type"` tag.
    pub fn name(&self) -> &'static str {
        match self {
            Action::Tap { .. } => "tap",
            Action::LongPress { .. } => "long_press",
            Action::Type { .. } => "type",
            Action::Clear { .. } => "clear",
            Action::Scroll { .. } => "scroll",
            Action::Swipe { .. } => "swipe",
            Action::TapPoint { .. } => "tap_point",
            Action::PressHome => "press_home",
            Action::OpenApp { .. } => "open_app",
            Action::OpenUrl { .. } => "open_url",
            Action::Wait { .. } => "wait",
            Action::CallTool { .. } => "call_tool",
            Action::AskUser { .. } => "ask_user",
            Action::Look => "look",
            Action::Done { .. } => "done",
            Action::Fail { .. } => "fail",
        }
    }

    /// The element index this action targets, if any.
    pub fn element(&self) -> Option<u32> {
        match self {
            Action::Tap { element }
            | Action::LongPress { element, .. }
            | Action::Clear { element } => Some(*element),
            Action::Type { element, .. }
            | Action::Scroll { element, .. }
            | Action::Swipe { element, .. } => *element,
            _ => None,
        }
    }

    /// True for actions that end the task (`Done`, `Fail`).
    pub fn is_terminal(&self) -> bool {
        matches!(self, Action::Done { .. } | Action::Fail { .. })
    }

    /// True for actions carried out through the runner on the UI.
    pub fn is_ui(&self) -> bool {
        !matches!(
            self,
            Action::CallTool { .. }
                | Action::AskUser { .. }
                | Action::Look
                | Action::Done { .. }
                | Action::Fail { .. }
        )
    }
}
