//! Task and session configuration.

use serde::{Deserialize, Serialize};

use crate::model::ModelProfile;
use crate::policy::Policy;
use crate::skill::SkillContext;
use crate::tool::ToolSpec;

/// Where a task's request came from (fifteenth review, R2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOrigin {
    /// Typed or spoken on the phone itself: the owner's own words.
    #[default]
    Phone,
    /// Sent by a paired remote device: not the owner's words for the typing gate (text the
    /// model types for it asks on the phone).
    Remote,
}

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Shell-assigned task id.
    pub id: String,
    /// The request in the user's words.
    pub text: String,
    /// Where the request came from (absent from JSON written before the fifteenth review:
    /// the phone).
    #[serde(default)]
    pub origin: TaskOrigin,
}

impl TaskSpec {
    /// Creates a task started on the phone.
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            origin: TaskOrigin::Phone,
        }
    }

    /// This task, started from `origin`.
    #[must_use]
    pub fn from_origin(mut self, origin: TaskOrigin) -> Self {
        self.origin = origin;
        self
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
    /// Bundle ids of Jarvis's own bundles (the app itself first, then its own test runner
    /// apps), which never need first-use approval. Twenty-fifth review: an exact list,
    /// matched exactly and case-sensitively — no prefix, suffix or wildcard rule, so an
    /// app whose id merely extends Jarvis's (`dev.jarvis.app.evil`) is just another app.
    /// Empty (the default): no app is Jarvis's own. The shell builds it from its own
    /// bundle id as the OS reports it (`JarvisOwnBundles.ids(main:)`).
    pub agent_bundle_ids: Vec<String>,
    /// Sixteenth review: whether this run drives Jarvis's own app (`agent_bundle_ids`). Off
    /// (the default, also when absent from JSON): while Jarvis's own app is in front the
    /// model gets a fixed one-line stub instead of its screen (no labels, values or
    /// screenshot), nothing there can be acted on, and that screen is not something Jarvis
    /// read (confinement, Rule T). The simulator UI tests turn it on to drive the
    /// Playground screens; then the own app is a context like any other.
    pub drive_own_app: bool,
    /// The skill this session runs under, if any: adds the owner's guidance to the prompt
    /// and, with `confine_to_apps`, keeps the session inside the skill's apps. Absent from
    /// JSON written before skills existed (and omitted when `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<SkillContext>,
}
