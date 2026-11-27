//! Model inference requests and responses.

use serde::{Deserialize, Serialize};

use crate::geometry::Rect;
use crate::ui::ImageRef;

/// Whether a request is text-only or includes a screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    /// Text-only prompt.
    #[default]
    Text,
    /// Prompt with a screenshot and set-of-marks.
    Vision,
}

/// Where the shell draws a numbered set-of-marks label on the screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    /// Element index printed in the label.
    pub index: u32,
    /// Element frame in screen points.
    pub frame: Rect,
}

/// One inference request for the shell's model runtime.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ModelRequest {
    /// System prompt.
    pub system: String,
    /// User prompt.
    pub user: String,
    /// Screenshot handle for vision requests.
    #[serde(default)]
    pub image: Option<ImageRef>,
    /// Set-of-marks labels to draw on `image`.
    #[serde(default)]
    pub marks: Vec<Mark>,
    /// Generation cap.
    pub max_tokens: u32,
    /// GBNF grammar for constrained decoding, if supported.
    #[serde(default)]
    pub grammar: Option<String>,
    /// JSON schema for constrained decoding, if supported.
    #[serde(default)]
    pub json_schema: Option<String>,
    /// Text or vision.
    #[serde(default)]
    pub kind: RequestKind,
}

/// Raw text the model produced.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModelOutput {
    /// Generated text.
    pub text: String,
}

/// Capabilities of the model the shell runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelProfile {
    /// Context window in tokens.
    pub context_tokens: u32,
    /// Whether image input is supported.
    #[serde(default)]
    pub supports_vision: bool,
    /// Whether grammar/JSON-schema constrained decoding is supported.
    #[serde(default)]
    pub supports_grammar: bool,
}

impl Default for ModelProfile {
    /// A small text-only on-device model with a 4096-token window.
    fn default() -> Self {
        Self {
            context_tokens: 4096,
            supports_vision: false,
            supports_grammar: false,
        }
    }
}
