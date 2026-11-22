//! UniFFI bridge exposing the Jarvis core session, memory and proto types to the Swift
//! shell.
//!
//! Swift-visible surface (UniFFI namespace `jarvis_core`, Swift module `JarvisCore`):
//!
//! - `JarvisSession(config:task:memoryJson:) throws`, `step(nowMs:event:) -> Effect`,
//!   `progress() throws -> SessionProgress`, `finish() throws -> String`,
//!   `lastObservationIssue() throws -> String?` ([`JarvisSession`]).
//! - `JarvisSkills(json:) throws`: the skill store and matcher (`list`, `get`, `upsert`,
//!   `remove`, `setKind`, `recordRun`, `matchTask`, `context`, `contextForMatch`,
//!   `taskText`, `promoteFlow`, `toJson`, `loadIssues`, `hasLoadIssues`) ([`JarvisSkills`]). A session runs a skill
//!   through `SessionConfig.skill`.
//! - `JarvisWorkflows(json:) throws`: the workflow store (`list`, `get`, `upsert`,
//!   `remove`, `setEnabled`, `warnings`, `matchPhrase`, `forExternal`, `due`,
//!   `nextScheduled`, `recordRun`, `toJson`, `loadIssues`, `hasLoadIssues`, `damagedIds`)
//!   and `JarvisWorkflowRun(workflow:limits:)` (`step(nowMs:event:skills:)`, `finish`,
//!   `awaiting`, `pending`, `outcome`, ...) ([`JarvisWorkflows`], [`JarvisWorkflowRun`]);
//!   `resolveUtterance(skills:workflows:text:)`, `defaultRunLimits()`,
//!   `weekdayFromIndex(index:)` and the `schedule*` functions.
//! - `JarvisRemoteHub(storeJson:phoneName:) throws` (the phone's remote-control hub) and
//!   `JarvisRemoteClient.pair/connect` (a remote device), plus `neverRemoteApprove()`,
//!   `remotelyApprovable(category:)`, `newDeviceRemotePermissions()`
//!   ([`JarvisRemoteHub`], [`JarvisRemoteClient`]; secrets and shell duties in `remote.rs`).
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
mod remote;
mod runner;
mod session;
mod skills;
pub mod types;
mod workflows;

pub use error::JarvisError;
pub use remote::{
    never_remote_approve, new_device_remote_permissions, remote_min_entropy_len,
    remote_protocol_version, remotely_approvable, JarvisRemoteClient, JarvisRemoteHub,
};
pub use session::JarvisSession;
pub use skills::JarvisSkills;
pub use types::*;
pub use workflows::{
    default_run_limits, resolve_utterance, schedule_is_due, schedule_minutes_since_last,
    schedule_minutes_until_next, weekday_from_index, JarvisWorkflowRun, JarvisWorkflows,
};

/// Converts an FFI event the way `step` does, for tests and tools (the second value is the
/// runner-text parse note, if any).
pub fn event_to_proto(event: Event) -> (jarvis_proto::Event, Option<String>) {
    convert::event_into_proto(event)
}
