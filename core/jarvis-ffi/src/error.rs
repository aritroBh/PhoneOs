//! The one error type Swift sees (`JarvisError`, thrown).

/// Every error a Jarvis FFI call can throw.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum JarvisError {
    /// The memory bytes given to `JarvisSession(…)` could not be loaded.
    #[error("invalid memory JSON: {message}")]
    InvalidMemory {
        /// What `MemoryStore::from_json` reported.
        message: String,
    },
    /// A JSON string field (tool arguments) is not valid JSON.
    #[error("invalid JSON in {field}: {message}")]
    InvalidJson {
        /// Which field.
        field: String,
        /// The parser's message.
        message: String,
    },
    /// `parseRunnerTextToScreen` found no node in the runner's text.
    #[error("runner text could not be parsed: {message}")]
    RunnerText {
        /// The parser's message.
        message: String,
    },
    /// The session hit an internal error earlier and no longer runs; its memory was not
    /// updated, so the shell must keep the memory bytes it had before this session.
    #[error("session unusable after an internal error: {message}")]
    Poisoned {
        /// The original internal error.
        message: String,
    },
    /// A Rust panic was caught at the FFI boundary.
    #[error("internal error: {message}")]
    Internal {
        /// The panic message.
        message: String,
    },
    /// A skill, a skill-store snapshot or a skill lookup was refused (validation error,
    /// corrupt or newer snapshot, unknown id, a flow that cannot be promoted). Kept last:
    /// UniFFI encodes variants by position.
    #[error("skill error: {message}")]
    InvalidSkill {
        /// What `jarvis-skills` reported.
        message: String,
    },
}
