//! UniFFI bridge exposing the Jarvis core session, memory and proto types to the Swift
//! shell.
//!
//! Swift-visible surface (UniFFI namespace `jarvis_core`, Swift module `JarvisCore`):
//!
//! - `JarvisSession(config:task:memoryJson:) throws`, `step(nowMs:event:) -> Effect`,
//!   `progress() throws -> SessionProgress`, `finish() throws -> String`,
//!   `lastObservationIssue() throws -> String?` ([`JarvisSession`]).
//! - `JarvisSkills(json:) throws`: the skill store and matcher (`list`, `get`, `upsert`,
//!   `remove`, `recordRun`, `matchTask`, `context`, `contextForMatch`, `taskText`,
//!   `promoteFlow`, `toJson`, `loadIssues`) ([`JarvisSkills`]). A session runs a skill
//!   through `SessionConfig.skill`.
//! - Free functions: `actionJsonSchema()`, `actionGbnf()`, `systemPrompt()`,
//!   `coreVersion()`, `parseRunnerTextToScreen(snapshot:)`, `defaultPolicy()`,
//!   `defaultModelProfile()`, `defaultSessionConfig()` ([`api`]).
//! - Mirrors of every type in the `Event`/`Effect` graph as records and enums
//!   ([`types`]), converted to and from `jarvis-proto` in `convert.rs` (`From`/`TryFrom`).
//! - One error enum, [`JarvisError`].
//!
//! Panic containment is described in `guard.rs`; the parse-failure behaviour of the
//! runner-text event in `runner.rs`.
//!
//! `unsafe_code` is forbidden here like in every other crate: the UniFFI 0.32 macro
//! expansions do contain `unsafe`, but rustc does not apply the lint to code spanned by an
//! external proc macro, so no allowance is needed (checked; see `Cargo.toml`).

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

uniffi::setup_scaffolding!("jarvis_core");

pub mod api;
mod convert;
mod error;
mod guard;
mod runner;
mod session;
mod skills;
pub mod types;

pub use error::JarvisError;
pub use session::JarvisSession;
pub use skills::JarvisSkills;
pub use types::*;

/// Converts an FFI event the way `step` does, for tests and tools (the second value is the
/// runner-text parse note, if any).
pub fn event_to_proto(event: Event) -> (jarvis_proto::Event, Option<String>) {
    convert::event_into_proto(event)
}
