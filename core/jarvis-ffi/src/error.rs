//! The one error type Swift sees (`JarvisError`, thrown).

use crate::runner_channel::RunnerChannelProblem;
use crate::types::EntropyProblem;

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
    /// corrupt or newer snapshot, unknown id, a flow that cannot be promoted).
    #[error("skill error: {message}")]
    InvalidSkill {
        /// What `jarvis-skills` reported.
        message: String,
    },
    // New cases go below, never above: UniFFI encodes variants by position.
    /// A workflow, a workflow-store snapshot or a workflow lookup was refused (validation
    /// error, corrupt or newer snapshot, unknown id, store full).
    #[error("workflow error: {message}")]
    InvalidWorkflow {
        /// What `jarvis-skills` reported.
        message: String,
    },
    /// `JarvisWorkflowRun.step` got an event that does not answer the outstanding effect.
    /// The run is unchanged and still waits for `expected`.
    #[error("workflow protocol error: unexpected {got} while waiting for {expected}")]
    WorkflowProtocol {
        /// The event's name (`session_finished`, `owner_answered`, ...).
        got: String,
        /// What the run waits for (`start`, `session_finished`, ..., `nothing`).
        expected: String,
    },
    /// A schedule or a local time is out of range (weekday index above 6, minute of day
    /// above 1439, hour above 23, minute above 59, no days). Never clamped.
    #[error("schedule error: {message}")]
    InvalidSchedule {
        /// What is out of range.
        message: String,
    },
    /// The entropy Swift passed was refused. Fetch fresh bytes from the OS CSPRNG
    /// (`SecRandomCopyBytes`, at least 32) and retry; never retry with the same bytes.
    #[error("entropy refused: {problem:?}")]
    RemoteEntropy {
        /// Too short or obviously not random.
        problem: EntropyProblem,
    },
    /// Pairing handshakes are locked out after repeated failed attempts until `untilMs`
    /// (hub clock, compare with `clockMs()`). No offer was opened.
    #[error("pairing is locked until {until_ms}")]
    RemotePairingLocked {
        /// End of the lockout on the hub clock.
        until_ms: u64,
    },
    /// No paired device with this id (or not a 32-hex-character device id).
    #[error("unknown device {device_id}")]
    RemoteUnknownDevice {
        /// The id as given (cut to 64 characters).
        device_id: String,
    },
    /// The persisted hub store or client credentials could not be loaded. Never contains
    /// key material. Keep the Keychain item (do not overwrite it) and report the problem.
    #[error("invalid remote store: {message}")]
    RemoteStoreInvalid {
        /// What is wrong (fixed texts, no input echoed).
        message: String,
    },
    /// A remote-control limit was reached (e.g. 8 paired devices: revoke one first).
    #[error("remote limit reached: {what} (at most {max})")]
    RemoteLimitReached {
        /// Which limit (`devices`).
        what: String,
        /// The limit.
        max: u32,
    },
    /// A device name is empty once cleaned.
    #[error("invalid device name")]
    RemoteInvalidName,
    /// A pairing code could not be parsed (the code itself is never echoed).
    #[error("invalid pairing code: {message}")]
    RemoteInvalidCode {
        /// Wrong characters or wrong length.
        message: String,
    },
    /// The remote connection failed authentication or the phone sent something out of
    /// order. Opaque on purpose; the `JarvisRemoteClient` is unusable: close the transport.
    #[error("remote authentication failed")]
    RemoteAuthFailed,
    /// `JarvisRemoteClient.send` before `connected` (or after a failure).
    #[error("remote not connected")]
    RemoteNotConnected,
    /// A remote message could not be encoded or decoded (malformed, unknown, too large).
    #[error("remote message error: {message}")]
    RemoteMessage {
        /// What `jarvis-remote` reported.
        message: String,
    },
    /// A runner channel call failed (`JarvisRunnerClientChannel`,
    /// `JarvisRunnerServerChannel`). `authentication`: the peer could not prove the key;
    /// the app treats the runner as absent.
    #[error("runner channel: {message}")]
    RunnerChannel {
        /// The kind of failure.
        problem: RunnerChannelProblem,
        /// A fixed text of `jarvis-runner` (never input or key bytes).
        message: String,
    },
}
