//! Free functions exported to Swift.

use jarvis_proto as p;

use crate::error::JarvisError;
use crate::guard::guarded;
use crate::runner::snapshot_to_screen;
use crate::types::{ModelProfile, Policy, RunnerSnapshot, Screen, SessionConfig};

/// The JSON schema of the canonical action object, for runtimes with constrained decoding.
#[uniffi::export]
pub fn action_json_schema() -> Result<String, JarvisError> {
    guarded("action_json_schema", || {
        Ok(jarvis_agent::action_json_schema())
    })
}

/// The GBNF grammar of the canonical action object (llama.cpp-style runtimes).
#[uniffi::export]
pub fn action_gbnf() -> Result<String, JarvisError> {
    guarded("action_gbnf", || Ok(jarvis_agent::action_gbnf()))
}

/// The fixed system prompt (also sent in every `ModelRequest.system`), e.g. for warming a
/// model's prompt cache.
#[uniffi::export]
pub fn system_prompt() -> Result<String, JarvisError> {
    guarded("system_prompt", || Ok(jarvis_agent::system_prompt()))
}

/// The version of the Rust core (the crate version), for logs and bug reports. A
/// compile-time constant, so it is the one export that cannot fail.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Parses a runner snapshot into a typed `Screen` (what `Event.observedRunnerText` does
/// inside the session), e.g. to show or log the tree. Throws when no line is a node.
#[uniffi::export]
pub fn parse_runner_text_to_screen(snapshot: RunnerSnapshot) -> Result<Screen, JarvisError> {
    guarded("parse_runner_text_to_screen", || {
        snapshot_to_screen(snapshot).map(Into::into)
    })
}

/// The conservative default policy (deny list, confirm every risky category, budgets).
#[uniffi::export]
pub fn default_policy() -> Result<Policy, JarvisError> {
    guarded("default_policy", || Ok(p::Policy::default().into()))
}

/// The default model profile (4096-token text model).
#[uniffi::export]
pub fn default_model_profile() -> Result<ModelProfile, JarvisError> {
    guarded("default_model_profile", || {
        Ok(p::ModelProfile::default().into())
    })
}

/// A config with the default policy and model profile, no tools and no known apps.
#[uniffi::export]
pub fn default_session_config() -> Result<SessionConfig, JarvisError> {
    guarded("default_session_config", || {
        Ok(p::SessionConfig::default().into())
    })
}

/// Test hook (feature `panic-hooks`): panics inside a guarded export, which must surface in
/// Swift as a thrown `JarvisError.internal`, not a crash.
#[cfg(any(test, feature = "panic-hooks"))]
#[uniffi::export]
pub fn debug_panic(message: String) -> Result<(), JarvisError> {
    guarded("debug_panic", || {
        #[allow(clippy::panic)]
        {
            panic!("{message}");
        }
    })
}
