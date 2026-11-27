//! Jarvis's own runner protocol for Swift: `JarvisRunnerClientChannel` (the app) and
//! `JarvisRunnerServerChannel` (the runner process), wrapping the sans-IO state machines
//! of `jarvis-runner`.
//!
//! Swift moves the bytes over a loopback TCP connection, supplies fresh entropy
//! (`SecRandomCopyBytes`, at least 32 bytes, one channel per connection) and writes every
//! `send` event's bytes in order. The pre-shared key crosses the boundary once per
//! channel, into a zeroizing buffer; it is never returned, logged or put in an error.
//! Swift also passes the time: `not_after_ms` with each request (the client) and `now_ms`
//! with each `receive` (the server), both in milliseconds of the uptime clock the two
//! processes on the phone share (`RunnerUptimeClock`); Rust reads no clock.
//!
//! Errors are `JarvisError.runnerChannel(problem:message:)`. `authentication` means the
//! peer could not prove the key (a stranger, a squatter, tampering, replay): the client
//! must treat the runner as absent, never fall back to trusting it. Every problem except
//! `tooLarge` and `notNow` closes the channel.
//!
//! Every call runs inside the panic guard; a panic poisons the object (later calls throw
//! `JarvisError.poisoned`) and the shell drops the connection.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard};

use jarvis_proto as p;
use jarvis_runner as r;
use zeroize::Zeroizing;

use crate::error::JarvisError;
use crate::guard::{guarded, panic_message};
use crate::types::{CommandExpect, Orientation, RunnerCommand, RunnerResult, Size, UiNode};

/// The phone's lock state as the runner reads it (the shell's `DeviceLockState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum RunnerWireLockState {
    Locked,
    Unlocked,
    Unknown,
}

/// An encoded screenshot. Its `Debug` shows the byte count, never the bytes.
#[derive(Clone, PartialEq, uniffi::Record)]
pub struct RunnerWireScreenshot {
    pub data: Vec<u8>,
    pub mime_type: String,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub scale: Option<f64>,
}

impl std::fmt::Debug for RunnerWireScreenshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerWireScreenshot")
            .field("bytes", &self.data.len())
            .field("mime_type", &self.mime_type)
            .field("pixel_width", &self.pixel_width)
            .field("pixel_height", &self.pixel_height)
            .field("scale", &self.scale)
            .finish()
    }
}

/// One observation with a typed tree (the shell's `RunnerObservation`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunnerWireSnapshot {
    pub root: UiNode,
    pub app_bundle_id: String,
    pub app_name: Option<String>,
    pub screen_size: Size,
    pub keyboard_visible: bool,
    pub orientation: Orientation,
    pub device_locked: bool,
    pub lock_state: RunnerWireLockState,
    pub notes: Vec<String>,
    pub screenshot: Option<RunnerWireScreenshot>,
    /// Review pass 31 (L1, protocol version 3): the tree was cut (by the runner's own copy, or
    /// by the encoder at `jarvis_runner::MAX_TREE_DEPTH` levels or `MAX_TREE_NODES` nodes).
    #[uniffi(default = false)]
    pub tree_cut: bool,
}

/// App → runner: exactly the `PhoneRunner` surface, plus a hello.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum RunnerWireRequest {
    Hello {
        protocol_version: u8,
    },
    Snapshot {
        include_screenshot: bool,
    },
    /// `expect` is carried to the runner, which checks it next to the gesture and answers
    /// `stale` (nothing done) when another app is in front or the front app cannot be
    /// read. The app's driver also checks it (`FrontAppGuard`) before sending.
    Perform {
        command: RunnerCommand,
        expect: Option<CommandExpect>,
    },
    LockState,
    ViewerScreenshot,
    FrontBundleId,
}

/// Runner → app.
// A UniFFI enum cannot box its variants; the snapshot is moved, not copied, so the size
// difference costs nothing measurable.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum RunnerWireReply {
    Hello {
        protocol_version: u8,
        runner: String,
    },
    Snapshot {
        snapshot: RunnerWireSnapshot,
    },
    Performed {
        result: RunnerResult,
    },
    LockState {
        state: RunnerWireLockState,
    },
    ViewerScreenshot {
        screenshot: Option<RunnerWireScreenshot>,
    },
    FrontBundleId {
        bundle_id: Option<String>,
    },
    /// The runner could not answer (its own call threw).
    Error {
        message: String,
    },
}

/// What the app must do or know after `receive`.
// A UniFFI enum cannot box its variants; the snapshot is moved, not copied, so the size
// difference costs nothing measurable.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum RunnerClientEvent {
    /// Write these bytes to the connection.
    Send { bytes: Vec<u8> },
    /// The runner proved the key; requests may be sent.
    Authenticated,
    /// The answer to the outstanding request.
    Reply {
        request_id: u64,
        reply: RunnerWireReply,
    },
}

/// What the runner must do or know after `receive`.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum RunnerServerEvent {
    /// Write these bytes to the connection.
    Send { bytes: Vec<u8> },
    /// The app proved the key.
    Authenticated,
    /// Carry this out and answer with `reply(requestId:reply:)`, no later than
    /// `not_after_ms` (the client's deadline; already checked against `now_ms`).
    Request {
        request_id: u64,
        not_after_ms: u64,
        request: RunnerWireRequest,
    },
}

/// A sealed request: its id and the bytes to write.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunnerOutgoingRequest {
    pub request_id: u64,
    pub bytes: Vec<u8>,
}

/// Why a runner channel call failed (`JarvisError.runnerChannel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum RunnerChannelProblem {
    /// The entropy was refused (nothing sent).
    Entropy,
    /// The key is not 32 bytes (nothing sent).
    Key,
    /// The peer could not prove the key, or a frame failed authentication. Closed.
    Authentication,
    /// A malformed frame or handshake message. Closed.
    Frame,
    /// An undecodable message or one out of order. Closed. (Not `Protocol`: a Swift
    /// keyword.)
    Message,
    /// The outgoing message is too large or not encodable (nothing sent; still open).
    TooLarge,
    /// Not valid now (not authenticated, a request outstanding; nothing sent; still open).
    NotNow,
    /// A request arrived after its `not_after_ms`; nothing was carried out. Closed.
    Expired,
    /// The channel failed earlier.
    Closed,
}

fn channel_error(e: r::RunnerError) -> JarvisError {
    let problem = match e {
        r::RunnerError::Entropy => RunnerChannelProblem::Entropy,
        r::RunnerError::Key => RunnerChannelProblem::Key,
        r::RunnerError::Auth => RunnerChannelProblem::Authentication,
        r::RunnerError::Frame(_) => RunnerChannelProblem::Frame,
        r::RunnerError::Protocol(_) => RunnerChannelProblem::Message,
        r::RunnerError::TooLarge(_) => RunnerChannelProblem::TooLarge,
        r::RunnerError::State(_) => RunnerChannelProblem::NotNow,
        r::RunnerError::Expired => RunnerChannelProblem::Expired,
        r::RunnerError::Closed => RunnerChannelProblem::Closed,
    };
    // The texts are fixed strings of `jarvis-runner`; none quotes input or key bytes.
    JarvisError::RunnerChannel {
        problem,
        message: e.to_string(),
    }
}

// ---------------------------------------------------------------------------------------
// Conversions.
// ---------------------------------------------------------------------------------------

impl From<r::LockState> for RunnerWireLockState {
    fn from(v: r::LockState) -> Self {
        match v {
            r::LockState::Locked => RunnerWireLockState::Locked,
            r::LockState::Unlocked => RunnerWireLockState::Unlocked,
            r::LockState::Unknown => RunnerWireLockState::Unknown,
        }
    }
}

impl From<RunnerWireLockState> for r::LockState {
    fn from(v: RunnerWireLockState) -> Self {
        match v {
            RunnerWireLockState::Locked => r::LockState::Locked,
            RunnerWireLockState::Unlocked => r::LockState::Unlocked,
            RunnerWireLockState::Unknown => r::LockState::Unknown,
        }
    }
}

impl From<r::Screenshot> for RunnerWireScreenshot {
    fn from(v: r::Screenshot) -> Self {
        let r::Screenshot {
            data,
            mime_type,
            pixel_width,
            pixel_height,
            scale,
        } = v;
        Self {
            data,
            mime_type,
            pixel_width,
            pixel_height,
            scale,
        }
    }
}

impl From<RunnerWireScreenshot> for r::Screenshot {
    fn from(v: RunnerWireScreenshot) -> Self {
        let RunnerWireScreenshot {
            data,
            mime_type,
            pixel_width,
            pixel_height,
            scale,
        } = v;
        Self {
            data,
            mime_type,
            pixel_width,
            pixel_height,
            scale,
        }
    }
}

impl From<r::Snapshot> for RunnerWireSnapshot {
    fn from(v: r::Snapshot) -> Self {
        let r::Snapshot {
            root,
            app_bundle_id,
            app_name,
            screen_size,
            keyboard_visible,
            orientation,
            device_locked,
            lock_state,
            notes,
            screenshot,
            tree_cut,
        } = v;
        Self {
            root: root.into(),
            app_bundle_id,
            app_name,
            screen_size: screen_size.into(),
            keyboard_visible,
            orientation: orientation.into(),
            device_locked,
            lock_state: lock_state.into(),
            notes,
            screenshot: screenshot.map(Into::into),
            tree_cut,
        }
    }
}

impl From<RunnerWireSnapshot> for r::Snapshot {
    fn from(v: RunnerWireSnapshot) -> Self {
        let RunnerWireSnapshot {
            root,
            app_bundle_id,
            app_name,
            screen_size,
            keyboard_visible,
            orientation,
            device_locked,
            lock_state,
            notes,
            screenshot,
            tree_cut,
        } = v;
        Self {
            root: root.into(),
            app_bundle_id,
            app_name,
            screen_size: screen_size.into(),
            keyboard_visible,
            orientation: orientation.into(),
            device_locked,
            lock_state: lock_state.into(),
            notes,
            screenshot: screenshot.map(Into::into),
            tree_cut,
        }
    }
}

/// The proto command with its `expect` set (the FFI command carries it separately).
fn with_expect(command: RunnerCommand, expect: Option<CommandExpect>) -> p::RunnerCommand {
    let expect = expect.map(|e| p::Expect {
        bundle_id: e.bundle_id,
    });
    match p::RunnerCommand::from(command) {
        p::RunnerCommand::Tap { x, y, .. } => p::RunnerCommand::Tap { x, y, expect },
        p::RunnerCommand::LongPress {
            x, y, duration_ms, ..
        } => p::RunnerCommand::LongPress {
            x,
            y,
            duration_ms,
            expect,
        },
        p::RunnerCommand::Swipe {
            from,
            to,
            duration_ms,
            ..
        } => p::RunnerCommand::Swipe {
            from,
            to,
            duration_ms,
            expect,
        },
        p::RunnerCommand::TypeText { text, .. } => p::RunnerCommand::TypeText { text, expect },
        p::RunnerCommand::PressKey { key, .. } => p::RunnerCommand::PressKey { key, expect },
        other @ (p::RunnerCommand::LaunchApp { .. }
        | p::RunnerCommand::OpenUrl { .. }
        | p::RunnerCommand::Wait { .. }) => other,
    }
}

impl From<RunnerWireRequest> for r::Request {
    fn from(v: RunnerWireRequest) -> Self {
        match v {
            RunnerWireRequest::Hello { protocol_version } => r::Request::Hello { protocol_version },
            RunnerWireRequest::Snapshot { include_screenshot } => {
                r::Request::Snapshot { include_screenshot }
            }
            RunnerWireRequest::Perform { command, expect } => r::Request::Perform {
                command: with_expect(command, expect),
            },
            RunnerWireRequest::LockState => r::Request::LockState,
            RunnerWireRequest::ViewerScreenshot => r::Request::ViewerScreenshot,
            RunnerWireRequest::FrontBundleId => r::Request::FrontBundleId,
        }
    }
}

impl From<r::Request> for RunnerWireRequest {
    fn from(v: r::Request) -> Self {
        match v {
            r::Request::Hello { protocol_version } => RunnerWireRequest::Hello { protocol_version },
            r::Request::Snapshot { include_screenshot } => {
                RunnerWireRequest::Snapshot { include_screenshot }
            }
            r::Request::Perform { command } => RunnerWireRequest::Perform {
                expect: command.expect().map(|e| CommandExpect {
                    bundle_id: e.bundle_id.clone(),
                }),
                command: command.into(),
            },
            r::Request::LockState => RunnerWireRequest::LockState,
            r::Request::ViewerScreenshot => RunnerWireRequest::ViewerScreenshot,
            r::Request::FrontBundleId => RunnerWireRequest::FrontBundleId,
        }
    }
}

impl From<RunnerWireReply> for r::Reply {
    fn from(v: RunnerWireReply) -> Self {
        match v {
            RunnerWireReply::Hello {
                protocol_version,
                runner,
            } => r::Reply::Hello {
                protocol_version,
                runner,
            },
            RunnerWireReply::Snapshot { snapshot } => r::Reply::Snapshot(Box::new(snapshot.into())),
            RunnerWireReply::Performed { result } => r::Reply::Performed(result.into()),
            RunnerWireReply::LockState { state } => r::Reply::LockState(state.into()),
            RunnerWireReply::ViewerScreenshot { screenshot } => {
                r::Reply::ViewerScreenshot(screenshot.map(Into::into))
            }
            RunnerWireReply::FrontBundleId { bundle_id } => r::Reply::FrontBundleId(bundle_id),
            RunnerWireReply::Error { message } => r::Reply::Error { message },
        }
    }
}

impl From<r::Reply> for RunnerWireReply {
    fn from(v: r::Reply) -> Self {
        match v {
            r::Reply::Hello {
                protocol_version,
                runner,
            } => RunnerWireReply::Hello {
                protocol_version,
                runner,
            },
            r::Reply::Snapshot(snapshot) => RunnerWireReply::Snapshot {
                snapshot: (*snapshot).into(),
            },
            r::Reply::Performed(result) => RunnerWireReply::Performed {
                result: result.into(),
            },
            r::Reply::LockState(state) => RunnerWireReply::LockState {
                state: state.into(),
            },
            r::Reply::ViewerScreenshot(screenshot) => RunnerWireReply::ViewerScreenshot {
                screenshot: screenshot.map(Into::into),
            },
            r::Reply::FrontBundleId(bundle_id) => RunnerWireReply::FrontBundleId { bundle_id },
            r::Reply::Error { message } => RunnerWireReply::Error { message },
        }
    }
}

// ---------------------------------------------------------------------------------------
// Objects.
// ---------------------------------------------------------------------------------------

const POISONED: &str = "runner channel unusable after an internal error; drop the connection";

/// Runs `f` on the channel in `slot`, poisoning it (setting `None`) if `f` panics.
fn with_slot<S, T>(
    slot: &Mutex<Option<S>>,
    what: &str,
    f: impl FnOnce(&mut S) -> Result<T, JarvisError>,
) -> Result<T, JarvisError> {
    guarded(what, || {
        let mut guard: MutexGuard<'_, Option<S>> = match slot.lock() {
            Ok(g) => g,
            Err(poison) => {
                slot.clear_poison();
                let mut g = poison.into_inner();
                *g = None;
                g
            }
        };
        let Some(inner) = guard.as_mut() else {
            return Err(JarvisError::Poisoned {
                message: POISONED.to_owned(),
            });
        };
        match catch_unwind(AssertUnwindSafe(|| f(inner))) {
            Ok(r) => r,
            Err(payload) => {
                *guard = None;
                Err(JarvisError::Internal {
                    message: format!("{what}: {}", panic_message(payload.as_ref())),
                })
            }
        }
    })
}

/// The Jarvis app's side of one runner connection. Swift: `JarvisRunnerClientChannel`.
#[derive(uniffi::Object)]
pub struct JarvisRunnerClientChannel {
    inner: Mutex<Option<r::RunnerClientChannel>>,
}

#[uniffi::export]
impl JarvisRunnerClientChannel {
    /// A channel for one new connection: the 32-byte key and at least 32 fresh CSPRNG
    /// bytes. Both buffers are zeroized here.
    #[uniffi::constructor]
    pub fn new(key: Vec<u8>, entropy: Vec<u8>) -> Result<Arc<Self>, JarvisError> {
        let key = Zeroizing::new(key);
        let entropy = Zeroizing::new(entropy);
        guarded("runner client", || {
            let channel = r::RunnerClientChannel::new(&key, &entropy).map_err(channel_error)?;
            Ok(Arc::new(Self {
                inner: Mutex::new(Some(channel)),
            }))
        })
    }

    /// The ClientHello: write it first. Valid once.
    pub fn start(&self) -> Result<Vec<u8>, JarvisError> {
        with_slot(&self.inner, "runner client start", |c| {
            c.start().map_err(channel_error)
        })
    }

    /// Feeds bytes read from the connection; carry out the events in order.
    pub fn receive(&self, bytes: Vec<u8>) -> Result<Vec<RunnerClientEvent>, JarvisError> {
        with_slot(&self.inner, "runner client receive", |c| {
            let events = c.receive(&bytes).map_err(channel_error)?;
            Ok(events
                .into_iter()
                .map(|e| match e {
                    r::ClientEvent::Send(bytes) => RunnerClientEvent::Send { bytes },
                    r::ClientEvent::Authenticated => RunnerClientEvent::Authenticated,
                    r::ClientEvent::Reply { id, reply } => RunnerClientEvent::Reply {
                        request_id: id,
                        reply: reply.into(),
                    },
                })
                .collect())
        })
    }

    /// Seals a request (only after `authenticated`, one at a time) that the runner may act
    /// on until `not_after_ms` of the shared uptime clock (this call's own deadline).
    pub fn request(
        &self,
        request: RunnerWireRequest,
        not_after_ms: u64,
    ) -> Result<RunnerOutgoingRequest, JarvisError> {
        with_slot(&self.inner, "runner client request", |c| {
            let (request_id, bytes) = c
                .request(&request.into(), not_after_ms)
                .map_err(channel_error)?;
            Ok(RunnerOutgoingRequest { request_id, bytes })
        })
    }

    /// Whether the runner proved the key and the channel has not failed.
    pub fn is_open(&self) -> Result<bool, JarvisError> {
        with_slot(&self.inner, "runner client is_open", |c| Ok(c.is_open()))
    }
}

/// The runner's side of one connection. Swift: `JarvisRunnerServerChannel`.
#[derive(uniffi::Object)]
pub struct JarvisRunnerServerChannel {
    inner: Mutex<Option<r::RunnerServerChannel>>,
}

#[uniffi::export]
impl JarvisRunnerServerChannel {
    /// A channel for one accepted connection: the 32-byte key and at least 32 fresh
    /// CSPRNG bytes. Both buffers are zeroized here.
    #[uniffi::constructor]
    pub fn new(key: Vec<u8>, entropy: Vec<u8>) -> Result<Arc<Self>, JarvisError> {
        let key = Zeroizing::new(key);
        let entropy = Zeroizing::new(entropy);
        guarded("runner server", || {
            let channel = r::RunnerServerChannel::new(&key, &entropy).map_err(channel_error)?;
            Ok(Arc::new(Self {
                inner: Mutex::new(Some(channel)),
            }))
        })
    }

    /// Feeds bytes read from the connection at `now_ms` of the shared uptime clock (a
    /// request received past its `not_after_ms` fails with `expired`); carry out the
    /// events in order.
    pub fn receive(
        &self,
        bytes: Vec<u8>,
        now_ms: u64,
    ) -> Result<Vec<RunnerServerEvent>, JarvisError> {
        with_slot(&self.inner, "runner server receive", |s| {
            let events = s.receive(&bytes, now_ms).map_err(channel_error)?;
            Ok(events
                .into_iter()
                .map(|e| match e {
                    r::ServerEvent::Send(bytes) => RunnerServerEvent::Send { bytes },
                    r::ServerEvent::Authenticated => RunnerServerEvent::Authenticated,
                    r::ServerEvent::Request {
                        id,
                        not_after_ms,
                        request,
                    } => RunnerServerEvent::Request {
                        request_id: id,
                        not_after_ms,
                        request: request.into(),
                    },
                })
                .collect())
        })
    }

    /// Seals the reply to the outstanding request. A reply too large for the protocol is
    /// replaced by an error reply saying so.
    pub fn reply(&self, request_id: u64, reply: RunnerWireReply) -> Result<Vec<u8>, JarvisError> {
        with_slot(&self.inner, "runner server reply", |s| {
            s.reply(request_id, &reply.into()).map_err(channel_error)
        })
    }

    /// Whether the app proved the key and the channel has not failed.
    pub fn is_open(&self) -> Result<bool, JarvisError> {
        with_slot(&self.inner, "runner server is_open", |s| Ok(s.is_open()))
    }
}

/// The runner protocol version this core speaks.
#[uniffi::export]
pub fn runner_protocol_version() -> u8 {
    r::PROTOCOL_VERSION
}
