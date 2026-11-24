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
//! - `JarvisViewer(config:) throws` (the Live view of one run) and
//!   `JarvisViewer.decoded(bytes:config:) throws` (a timeline received as bytes):
//!   `feed(update:)` once per driver update (`CoreViewerUpdate` mirrors `DriverUpdate`),
//!   `scene(query:)` once per drawn frame (a flat `CoreSceneQuery` in, a flat
//!   `CoreLiveScene` out), `timeline()` (steps and screen summaries, no wireframes),
//!   `screen(id:)` (one wireframe), `encode()` (the versioned wire bytes for the remote
//!   stream), `replay(action:nowMs:reduceMotion:)`, `replayStatus()`, `generation()`;
//!   `viewerWireVersion()` ([`JarvisViewer`]; the logic is the `jarvis-viewer` crate,
//!   records carry a `Core` prefix so they never collide with the shell's `Viewer*` types).
//! - `JarvisRunnerClientChannel(key:entropy:)` (the app) and
//!   `JarvisRunnerServerChannel(key:entropy:)` (Jarvis's own runner process): the
//!   authenticated loopback runner protocol (`start`, `receive`, `request`, `reply`,
//!   `isOpen`), plus `runnerProtocolVersion()` ([`JarvisRunnerClientChannel`]; the logic is
//!   the `jarvis-runner` crate, shell duties in `runner_channel.rs`).
//! - Free functions: `actionJsonSchema()`, `actionGbnf()`, `systemPrompt()`,
//!   `coreVersion()`, `parseRunnerTextToScreen(snapshot:)`, `defaultPolicy()`,
//!   `defaultModelProfile()`, `defaultSessionConfig()`, and for the Live view
//!   `redactScreenText(text:)` and `fieldValueIsHidden(role:label:placeholder:identifier:secure:)`
//!   ([`api`]; kept for the shell's Swift Live view until it uses `JarvisViewer`).
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
mod live_remote;
mod remote;
mod runner;
mod runner_channel;
mod session;
mod skills;
pub mod types;
mod viewer;
mod workflows;

pub use error::JarvisError;
pub use live_remote::{CoreDeltaApplied, CoreResyncReason, CoreViewerFed};
pub use remote::{
    never_remote_approve, new_device_remote_permissions, remote_min_entropy_len,
    remote_protocol_version, remotely_approvable, JarvisRemoteClient, JarvisRemoteHub,
};
pub use runner_channel::{
    runner_protocol_version, JarvisRunnerClientChannel, JarvisRunnerServerChannel,
    RunnerChannelProblem, RunnerClientEvent, RunnerOutgoingRequest, RunnerServerEvent,
    RunnerWireLockState, RunnerWireReply, RunnerWireRequest, RunnerWireScreenshot,
    RunnerWireSnapshot,
};
pub use session::JarvisSession;
pub use skills::JarvisSkills;
pub use types::*;
pub use viewer::{
    viewer_wire_version, CoreApprovalMark, CoreApprovalTarget, CoreCursorPhase, CoreLiveScene,
    CoreReplayAction, CoreReplaySpeed, CoreReplayStatus, CoreSceneQuery, CoreSceneTone,
    CoreViewerActivity, CoreViewerChange, CoreViewerConfig, CoreViewerElement,
    CoreViewerElementKind, CoreViewerFrameKey, CoreViewerGesture, CoreViewerOutcome,
    CoreViewerScreen, CoreViewerScreenInfo, CoreViewerSource, CoreViewerStep, CoreViewerStepState,
    CoreViewerTimeline, CoreViewerUpdate, JarvisViewer,
};
pub use workflows::{
    default_run_limits, resolve_utterance, schedule_is_due, schedule_minutes_since_last,
    schedule_minutes_until_next, weekday_from_index, JarvisWorkflowRun, JarvisWorkflows,
};

/// Converts an FFI event the way `step` does, for tests and tools (the second value is the
/// runner-text parse note, if any).
pub fn event_to_proto(event: Event) -> (jarvis_proto::Event, Option<String>) {
    convert::event_into_proto(event)
}
