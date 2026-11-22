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
///
/// `Debug` never shows the text of `TypeText` (it may carry substituted profile values),
/// only its length.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
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

impl std::fmt::Debug for RunnerCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerCommand::Tap { x, y } => {
                f.debug_struct("Tap").field("x", x).field("y", y).finish()
            }
            RunnerCommand::LongPress { x, y, duration_ms } => f
                .debug_struct("LongPress")
                .field("x", x)
                .field("y", y)
                .field("duration_ms", duration_ms)
                .finish(),
            RunnerCommand::Swipe {
                from,
                to,
                duration_ms,
            } => f
                .debug_struct("Swipe")
                .field("from", from)
                .field("to", to)
                .field("duration_ms", duration_ms)
                .finish(),
            RunnerCommand::TypeText { text } => f
                .debug_struct("TypeText")
                .field("text_chars", &text.chars().count())
                .finish_non_exhaustive(),
            RunnerCommand::PressKey { key } => {
                f.debug_struct("PressKey").field("key", key).finish()
            }
            RunnerCommand::LaunchApp { bundle_id } => f
                .debug_struct("LaunchApp")
                .field("bundle_id", bundle_id)
                .finish(),
            RunnerCommand::OpenUrl { url } => f.debug_struct("OpenUrl").field("url", url).finish(),
            RunnerCommand::Wait { ms } => f.debug_struct("Wait").field("ms", ms).finish(),
        }
    }
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

/// Why the runner did not carry out a command, when the reason is one the core acts on.
/// The free-text `RunnerResult::error` still carries the details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerFailure {
    /// The device is locked (`screen_locked`): the command was refused and not carried out.
    /// The session waits for the owner to unlock instead of counting a failure.
    DeviceLocked,
    /// Any other typed reason (also what unknown reasons from newer shells read as).
    #[serde(other)]
    Other,
}

/// The runner's reply to a command.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunnerResult {
    /// Whether the command was carried out.
    pub ok: bool,
    /// Error text when `ok` is false.
    #[serde(default)]
    pub error: Option<String>,
    /// Typed reason when `ok` is false and the shell knows it. Absent in JSON written
    /// before typed reasons existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<RunnerFailure>,
}

impl RunnerResult {
    /// A successful result.
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
            failure: None,
        }
    }

    /// A failed result with an error message.
    pub fn err(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(error.into()),
            failure: None,
        }
    }

    /// The command was refused because the device is locked; nothing was carried out.
    pub fn device_locked() -> Self {
        Self {
            ok: false,
            error: Some("the device is locked".to_owned()),
            failure: Some(RunnerFailure::DeviceLocked),
        }
    }

    /// Whether this is a refusal because the device is locked.
    pub fn is_device_locked(&self) -> bool {
        !self.ok && self.failure == Some(RunnerFailure::DeviceLocked)
    }
}
