//! Free functions exported to Swift.

use jarvis_proto as p;

use crate::error::JarvisError;
use crate::guard::guarded;
use crate::runner::snapshot_to_screen;
use crate::types::{ModelProfile, Policy, Role, RunnerSnapshot, Screen, SessionConfig};

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

/// The exact digest of `screen` as lower-case hex: what `ConfirmTarget.digest` holds for the
/// screen a confirmation was asked on (`jarvis_observe::screen_digest`), so a shell can tell
/// whether a target belongs to a screen it holds.
#[uniffi::export]
pub fn screen_digest_hex(screen: Screen) -> Result<String, JarvisError> {
    guarded("screen_digest_hex", || {
        Ok(jarvis_observe::screen_digest(&p::Screen::from(screen)).to_hex())
    })
}

/// The display-safe form of a piece of on-screen text: the same redaction the core applies
/// before screen text reaches a prompt, a log or the owner's summary (card numbers, IBANs,
/// one-time codes, API keys and tokens, labelled secrets and seed phrases masked; invisible
/// characters dropped). Text with nothing to mask comes back unchanged.
#[uniffi::export]
pub fn redact_screen_text(text: String) -> Result<String, JarvisError> {
    guarded("redact_screen_text", || Ok(jarvis_safety::redact(&text)))
}

/// Whether the core hides the value of a field with this role, label, placeholder,
/// identifier and secure flag: a secure field (flag or `SecureTextField` role), or an entry
/// field named as a credential or a bank or card detail ("Password", "Verification code",
/// "Card number", "CVV", "IBAN"...). An identifier counts as the label, as compaction uses
/// it when the field has no label; it is checked whenever one is given, so the answer is at
/// least as strict as the core's own.
#[uniffi::export]
pub fn field_value_is_hidden(
    role: Role,
    label: Option<String>,
    placeholder: Option<String>,
    identifier: Option<String>,
    secure: bool,
) -> Result<bool, JarvisError> {
    guarded("field_value_is_hidden", || {
        // One implementation, shared with the Live view's reducer (`JarvisViewer`).
        Ok(jarvis_viewer::privacy::field_value_hidden(
            &p::Role::from(role),
            label.as_deref(),
            placeholder.as_deref(),
            identifier.as_deref(),
            secure,
        ))
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
