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

/// What a gesture or typing command expects of the phone when the runner carries it out
/// (the stale-screen guard, layer 2): the app the command was planned on is in front. The
/// core fills it from the observation the command was planned (or re-checked) on; a runner
/// that can read the front app refuses the command when another app is in front
/// ([`RunnerFailure::Stale`]). A runner that cannot read it, or ignores the field (the
/// text-format third-party runner), carries the command out as before.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Expect {
    /// Bundle id of the app that must be in front (compared ignoring ASCII case and
    /// surrounding whitespace).
    pub bundle_id: String,
}

impl Expect {
    /// Whether `front` (the runner's reading of the front app) is the expected app.
    pub fn matches(&self, front: &str) -> bool {
        self.bundle_id.trim().eq_ignore_ascii_case(front.trim())
    }
}

/// One low-level command for the runner. JSON is internally tagged with `"type"`.
///
/// Gestures and typing (`Tap`, `LongPress`, `Swipe`, `TypeText`, `PressKey`) carry an
/// optional [`Expect`]: absent in JSON written before it existed, and omitted when `None`,
/// so older runners and JSON keep working.
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
        /// What the phone must show for the tap to be carried out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<Expect>,
    },
    /// Press and hold at a point.
    LongPress {
        /// Horizontal coordinate.
        x: f64,
        /// Vertical coordinate.
        y: f64,
        /// Hold duration in milliseconds.
        duration_ms: u64,
        /// What the phone must show for the press to be carried out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<Expect>,
    },
    /// Drag from one point to another.
    Swipe {
        /// Start point.
        from: Point,
        /// End point.
        to: Point,
        /// Gesture duration in milliseconds.
        duration_ms: u64,
        /// What the phone must show for the drag to be carried out.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<Expect>,
    },
    /// Type text into the focused element.
    TypeText {
        /// Text to type.
        text: String,
        /// What the phone must show for the text to be typed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<Expect>,
    },
    /// Press a key.
    PressKey {
        /// The key.
        key: Key,
        /// What the phone must show for the key to be pressed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<Expect>,
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
        let front = |e: &Option<Expect>| e.as_ref().map(|e| e.bundle_id.clone());
        match self {
            RunnerCommand::Tap { x, y, expect } => f
                .debug_struct("Tap")
                .field("x", x)
                .field("y", y)
                .field("expect", &front(expect))
                .finish(),
            RunnerCommand::LongPress {
                x,
                y,
                duration_ms,
                expect,
            } => f
                .debug_struct("LongPress")
                .field("x", x)
                .field("y", y)
                .field("duration_ms", duration_ms)
                .field("expect", &front(expect))
                .finish(),
            RunnerCommand::Swipe {
                from,
                to,
                duration_ms,
                expect,
            } => f
                .debug_struct("Swipe")
                .field("from", from)
                .field("to", to)
                .field("duration_ms", duration_ms)
                .field("expect", &front(expect))
                .finish(),
            RunnerCommand::TypeText { text, expect } => f
                .debug_struct("TypeText")
                .field("text_chars", &text.chars().count())
                .field("expect", &front(expect))
                .finish_non_exhaustive(),
            RunnerCommand::PressKey { key, expect } => f
                .debug_struct("PressKey")
                .field("key", key)
                .field("expect", &front(expect))
                .finish(),
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

    /// What the command expects of the phone (gestures and typing; see [`Expect`]).
    pub fn expect(&self) -> Option<&Expect> {
        match self {
            RunnerCommand::Tap { expect, .. }
            | RunnerCommand::LongPress { expect, .. }
            | RunnerCommand::Swipe { expect, .. }
            | RunnerCommand::TypeText { expect, .. }
            | RunnerCommand::PressKey { expect, .. } => expect.as_ref(),
            RunnerCommand::LaunchApp { .. }
            | RunnerCommand::OpenUrl { .. }
            | RunnerCommand::Wait { .. } => None,
        }
    }

    /// The command with its expectation set to `want` (gestures and typing; the other
    /// commands, which change the front app or touch nothing, carry none and are returned
    /// as they are).
    #[must_use]
    pub fn with_expect(mut self, want: Option<Expect>) -> Self {
        match &mut self {
            RunnerCommand::Tap { expect, .. }
            | RunnerCommand::LongPress { expect, .. }
            | RunnerCommand::Swipe { expect, .. }
            | RunnerCommand::TypeText { expect, .. }
            | RunnerCommand::PressKey { expect, .. } => *expect = want,
            RunnerCommand::LaunchApp { .. }
            | RunnerCommand::OpenUrl { .. }
            | RunnerCommand::Wait { .. } => {}
        }
        self
    }

    /// The command without its expectation (what it does, for comparisons).
    #[must_use]
    pub fn without_expect(&self) -> Self {
        self.clone().with_expect(None)
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
    /// Stale-screen guard (layer 2): the command carried an [`Expect`] and the runner read
    /// another app in front, so it was refused and not carried out (the free-text error
    /// names the app in front). The core treats it like [`Self::DeviceLocked`]: nothing of
    /// the step happened, it is refunded, the session observes again and decides again.
    Stale,
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

    /// The command was refused because `front`, not the expected app, is in front
    /// ([`RunnerFailure::Stale`]); nothing was carried out.
    pub fn stale(front: &str) -> Self {
        let front = front.trim();
        Self {
            ok: false,
            error: Some(format!(
                "not carried out: {} is in front, not the app the command was planned on",
                if front.is_empty() {
                    "another app"
                } else {
                    front
                }
            )),
            failure: Some(RunnerFailure::Stale),
        }
    }

    /// Whether this is a refusal because another app is in front.
    pub fn is_stale(&self) -> bool {
        !self.ok && self.failure == Some(RunnerFailure::Stale)
    }
}
