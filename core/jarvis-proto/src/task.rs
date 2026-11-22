//! Task and session configuration.

use serde::{Deserialize, Serialize};

use crate::model::ModelProfile;
use crate::policy::Policy;
use crate::tool::ToolSpec;

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Shell-assigned task id.
    pub id: String,
    /// The request in the user's words.
    pub text: String,
}

impl TaskSpec {
    /// Creates a task.
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
        }
    }
}

/// An installed app the agent may open by name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AppInfo {
    /// Bundle id.
    pub bundle_id: String,
    /// Display name.
    pub name: String,
}

impl AppInfo {
    /// Creates an app entry.
    pub fn new(bundle_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            bundle_id: bundle_id.into(),
            name: name.into(),
        }
    }
}

/// Everything a session needs besides the task and memory.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    /// Safety policy.
    pub policy: Policy,
    /// Tools on offer.
    pub tools: Vec<ToolSpec>,
    /// Model capabilities.
    pub model: ModelProfile,
    /// Installed apps for name to bundle id resolution.
    pub known_apps: Vec<AppInfo>,
}
