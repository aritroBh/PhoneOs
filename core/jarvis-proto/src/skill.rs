//! What a session knows about the skill it runs under (see `SessionConfig::skill`).
//!
//! Defined here rather than in `jarvis-skills` so that `jarvis-agent` and `jarvis-safety`
//! can read it without depending on the skill store. `jarvis-skills` builds it with
//! `Skill::context`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The skill a session runs under: the owner's own guidance for the task, the apps it works
/// in and the arguments captured from the request.
///
/// A skill can only narrow what a session may do: with `confine_to_apps` the session refuses
/// actions outside `apps` (SpringBoard stays reachable for navigation). It never grants
/// trust, skips a confirmation or suppresses first-use approval; every gate of the
/// session's `Policy` still applies.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillContext {
    /// Skill id (`[a-z0-9-]`).
    pub id: String,
    /// What the owner calls the skill.
    pub name: String,
    /// Owner-written guidance in plain language. Trusted as intent, but sanitised before it
    /// enters a prompt so it cannot pose as prompt structure.
    pub instructions: String,
    /// Bundle ids the skill works in.
    pub apps: Vec<String>,
    /// Refuse actions that would act outside `apps`.
    pub confine_to_apps: bool,
    /// Parameter values captured from the request (`{contact}` -> `mom`).
    pub args: BTreeMap<String, String>,
}
