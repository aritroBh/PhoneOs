//! Tools the agent can call instead of driving the UI.

use serde::{Deserialize, Serialize};

use crate::policy::RiskCategory;

/// How a tool is reached, cheapest and most reliable first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Apple frameworks inside Jarvis (EventKit, Contacts...).
    Native,
    /// A Shortcuts shortcut run via URL scheme.
    Shortcut,
    /// UI control through the runner.
    Ui,
}

/// JSON type of a tool parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    /// A string.
    #[default]
    String,
    /// An integer.
    Integer,
    /// Any number.
    Number,
    /// A boolean.
    Boolean,
}

/// One parameter of a tool.
///
/// The spec names `ToolParam` without fields; this is the minimal shape needed to describe a
/// parameter to the model and to build a JSON schema for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolParam {
    /// Parameter name (JSON key in `args`).
    pub name: String,
    /// What the parameter means, shown to the model.
    #[serde(default)]
    pub description: String,
    /// JSON type.
    #[serde(default)]
    pub kind: ParamKind,
    /// Whether the parameter must be present.
    #[serde(default)]
    pub required: bool,
}

/// A tool the shell offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Unique name the model uses in `call_tool`.
    pub name: String,
    /// What the tool does, shown to the model.
    pub description: String,
    /// Parameters.
    #[serde(default)]
    pub params: Vec<ToolParam>,
    /// How the tool is reached.
    pub tier: Tier,
    /// Declared risk; drives confirmation.
    #[serde(default)]
    pub risk: RiskCategory,
    /// Bundle id of the app the tool acts on, if any.
    #[serde(default)]
    pub app: Option<String>,
}

/// A request for the shell to run a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name.
    pub name: String,
    /// Arguments as a JSON object.
    #[serde(default)]
    pub args: serde_json::Value,
}

/// The shell's reply to a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool succeeded.
    pub ok: bool,
    /// Text output shown to the model.
    #[serde(default)]
    pub output: String,
    /// Error text when `ok` is false.
    #[serde(default)]
    pub error: Option<String>,
}
