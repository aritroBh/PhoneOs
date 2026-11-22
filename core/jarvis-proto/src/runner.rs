//! Commands for the XCUITest runner, in absolute screen points.

use serde::{Deserialize, Serialize};

use crate::geometry::Point;

/// A hardware or keyboard key the runner can press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    /// Return / Enter / Go / Send on the software keyboard.
    Return,
    /// Backspace.
    Delete,
    /// Tab.
    Tab,
    /// Space bar.
    Space,
    /// Escape.
    Escape,
    /// The Home button (home gesture).
    Home,
}

/// One low-level command for the runner. JSON is internally tagged with `"type"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunnerCommand {
    /// Tap at a point.
    Tap {
        /// Horizontal coordinate.
        x: f64,
        /// Vertical coordinate.
        y: f64,
    },
    /// Press and hold at a point.
    LongPress {
        /// Horizontal coordinate.
        x: f64,
        /// Vertical coordinate.
        y: f64,
        /// Hold duration in milliseconds.
        duration_ms: u64,
    },
    /// Drag from one point to another.
    Swipe {
        /// Start point.
        from: Point,
        /// End point.
        to: Point,
        /// Gesture duration in milliseconds.
        duration_ms: u64,
    },
    /// Type text into the focused element.
    TypeText {
        /// Text to type.
        text: String,
    },
    /// Press a key.
    PressKey {
        /// The key.
        key: Key,
    },
    /// Launch or foreground an app.
    LaunchApp {
        /// Bundle id.
        bundle_id: String,
    },
    /// Open a URL.
    OpenUrl {
        /// The URL.
        url: String,
    },
    /// Wait.
    Wait {
        /// Milliseconds to wait.
        ms: u64,
    },
}

impl RunnerCommand {
    /// The snake_case command name, identical to the JSON `"type"` tag.
    pub fn name(&self) -> &'static str {
        match self {
            RunnerCommand::Tap { .. } => "tap",
            RunnerCommand::LongPress { .. } => "long_press",
            RunnerCommand::Swipe { .. } => "swipe",
            RunnerCommand::TypeText { .. } => "type_text",
            RunnerCommand::PressKey { .. } => "press_key",
            RunnerCommand::LaunchApp { .. } => "launch_app",
            RunnerCommand::OpenUrl { .. } => "open_url",
            RunnerCommand::Wait { .. } => "wait",
        }
    }
}

/// The runner's reply to a command.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunnerResult {
    /// Whether the command was carried out.
    pub ok: bool,
    /// Error text when `ok` is false.
    #[serde(default)]
    pub error: Option<String>,
}

impl RunnerResult {
    /// A successful result.
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    /// A failed result with an error message.
    pub fn err(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(error.into()),
        }
    }
}
