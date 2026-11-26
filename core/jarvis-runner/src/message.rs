//! The messages inside data frames: exactly the `PhoneRunner` surface of the shell
//! (`snapshot`, `perform`, `lockState`, `viewerScreenshot`, `frontBundleId`) plus a hello,
//! over the `jarvis-proto` types the driver already hands to the core.
//!
//! ```text
//! request = kind (u8) || request id (u64 BE) || not_after_ms (u64 BE) || body
//! reply   = kind (u8) || request id (u64 BE) || body
//! ```
//!
//! `not_after_ms` is the last instant (milliseconds of the uptime clock both processes on
//! the phone share; see `docs/JARVIS-RUNNER.md`) at which the server may still act on the
//! request. It is inside the AEAD like everything else, so only the key holder sets it; the
//! server channel refuses a request it receives later ([`crate::RunnerError::Expired`]).
//!
//! A compact binary encoding, not JSON: screenshots are raw bytes, and trees are written
//! and read iteratively (pre-order, each node followed by its child count), so no tree
//! depth can exhaust a stack and every limit ([`crate::MAX_TREE_NODES`],
//! [`crate::MAX_TREE_DEPTH`], [`crate::MAX_SCREENSHOT_BYTES`], [`crate::MAX_NOTES`]) is
//! checked on both sides. Decoding is total: any input is a value or a [`MessageError`].
//! Numbers must be finite; strings must be UTF-8; nothing may follow the message.

use jarvis_proto::{
    Expect, Key, Orientation, Point, Rect, Role, RunnerCommand, RunnerFailure, RunnerResult, Size,
    UiNode,
};

use crate::error::MessageError;
use crate::wire::{Reader, Writer};
use crate::{MAX_NOTES, MAX_SCREENSHOT_BYTES, MAX_TREE_DEPTH, MAX_TREE_NODES};

/// The phone's lock state as the runner reads it (the shell's `DeviceLockState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockState {
    /// Locked.
    Locked,
    /// Unlocked (the runner positively read it).
    Unlocked,
    /// The runner cannot tell.
    Unknown,
}

/// An encoded screenshot as the runner took it (the shell's `CapturedScreenshot`).
#[derive(Clone, PartialEq)]
pub struct Screenshot {
    /// The encoded image (PNG or JPEG).
    pub data: Vec<u8>,
    /// MIME type of `data`.
    pub mime_type: String,
    /// Width in pixels.
    pub pixel_width: u32,
    /// Height in pixels.
    pub pixel_height: u32,
    /// Pixels per point, when known.
    pub scale: Option<f64>,
}

impl std::fmt::Debug for Screenshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Screenshot")
            .field("bytes", &self.data.len())
            .field("mime_type", &self.mime_type)
            .field("pixel_width", &self.pixel_width)
            .field("pixel_height", &self.pixel_height)
            .field("scale", &self.scale)
            .finish()
    }
}

/// One observation (the shell's `RunnerObservation` with a typed tree: this runner never
/// sends the text format).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The typed tree (root normally the `Application` node).
    pub root: UiNode,
    /// Bundle id of the foreground app.
    pub app_bundle_id: String,
    /// Display name of the foreground app, when known.
    pub app_name: Option<String>,
    /// Screen size in points.
    pub screen_size: Size,
    /// Whether the software keyboard is up.
    pub keyboard_visible: bool,
    /// Interface orientation.
    pub orientation: Orientation,
    /// The runner reads the phone locked.
    pub device_locked: bool,
    /// The lock state read for this observation.
    pub lock_state: LockState,
    /// What the runner had to assume.
    pub notes: Vec<String>,
    /// The screenshot, when asked for and taken.
    pub screenshot: Option<Screenshot>,
    /// Review pass 31 (L1, protocol version 3): the tree was cut — by the runner's own copy
    /// (`UiTreeBuilder`'s `treeCut`) or by the encoder, at [`crate::MAX_TREE_DEPTH`] levels or
    /// [`crate::MAX_TREE_NODES`] nodes. The client's screen carries it
    /// (`jarvis_proto::Screen::tree_cut`): the core reads an incomplete screen.
    pub tree_cut: bool,
}

/// Client → server.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// First request of a connection: the client's protocol version.
    Hello {
        /// [`crate::PROTOCOL_VERSION`] of the client.
        protocol_version: u8,
    },
    /// `snapshot(includeScreenshot:)`.
    Snapshot {
        /// Whether to take a screenshot too.
        include_screenshot: bool,
    },
    /// `perform(_:)`.
    Perform {
        /// The command (its `expect`, if any, is carried too).
        command: RunnerCommand,
    },
    /// `lockState()`.
    LockState,
    /// `viewerScreenshot()`.
    ViewerScreenshot,
    /// `frontBundleId()`.
    FrontBundleId,
}

/// Which reply answers a request (an [`Reply::Error`] answers any).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplyKind {
    /// [`Reply::Hello`].
    Hello,
    /// [`Reply::Snapshot`].
    Snapshot,
    /// [`Reply::Performed`].
    Performed,
    /// [`Reply::LockState`].
    LockState,
    /// [`Reply::ViewerScreenshot`].
    ViewerScreenshot,
    /// [`Reply::FrontBundleId`].
    FrontBundleId,
}

impl Request {
    /// The reply kind that answers this request.
    pub fn reply_kind(&self) -> ReplyKind {
        match self {
            Request::Hello { .. } => ReplyKind::Hello,
            Request::Snapshot { .. } => ReplyKind::Snapshot,
            Request::Perform { .. } => ReplyKind::Performed,
            Request::LockState => ReplyKind::LockState,
            Request::ViewerScreenshot => ReplyKind::ViewerScreenshot,
            Request::FrontBundleId => ReplyKind::FrontBundleId,
        }
    }
}

/// Server → client.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// Answer to [`Request::Hello`].
    Hello {
        /// [`crate::PROTOCOL_VERSION`] of the server.
        protocol_version: u8,
        /// A short name of the runner build.
        runner: String,
    },
    /// Answer to [`Request::Snapshot`].
    Snapshot(Box<Snapshot>),
    /// Answer to [`Request::Perform`]: carried out, or refused with a reason.
    Performed(RunnerResult),
    /// Answer to [`Request::LockState`].
    LockState(LockState),
    /// Answer to [`Request::ViewerScreenshot`].
    ViewerScreenshot(Option<Screenshot>),
    /// Answer to [`Request::FrontBundleId`].
    FrontBundleId(Option<String>),
    /// The runner could not answer (its own call threw).
    Error {
        /// What went wrong.
        message: String,
    },
}

impl Reply {
    /// The kind of this reply; `None` for [`Reply::Error`].
    pub fn kind(&self) -> Option<ReplyKind> {
        match self {
            Reply::Hello { .. } => Some(ReplyKind::Hello),
            Reply::Snapshot(_) => Some(ReplyKind::Snapshot),
            Reply::Performed(_) => Some(ReplyKind::Performed),
            Reply::LockState(_) => Some(ReplyKind::LockState),
            Reply::ViewerScreenshot(_) => Some(ReplyKind::ViewerScreenshot),
            Reply::FrontBundleId(_) => Some(ReplyKind::FrontBundleId),
            Reply::Error { .. } => None,
        }
    }
}

// Kind bytes.
const REQ_HELLO: u8 = 0x01;
const REQ_SNAPSHOT: u8 = 0x02;
const REQ_PERFORM: u8 = 0x03;
const REQ_LOCK: u8 = 0x04;
const REQ_VIEWER_SHOT: u8 = 0x05;
const REQ_FRONT: u8 = 0x06;
const REP_HELLO: u8 = 0x81;
const REP_SNAPSHOT: u8 = 0x82;
const REP_PERFORMED: u8 = 0x83;
const REP_LOCK: u8 = 0x84;
const REP_VIEWER_SHOT: u8 = 0x85;
const REP_FRONT: u8 = 0x86;
const REP_ERROR: u8 = 0xEF;

/// Encodes a request with its id and expiry (`not_after_ms`, see the module docs).
pub fn encode_request(
    id: u64,
    not_after_ms: u64,
    request: &Request,
) -> Result<Vec<u8>, MessageError> {
    let mut w = Writer::default();
    let kind = match request {
        Request::Hello { .. } => REQ_HELLO,
        Request::Snapshot { .. } => REQ_SNAPSHOT,
        Request::Perform { .. } => REQ_PERFORM,
        Request::LockState => REQ_LOCK,
        Request::ViewerScreenshot => REQ_VIEWER_SHOT,
        Request::FrontBundleId => REQ_FRONT,
    };
    w.u8(kind);
    w.u64(id);
    w.u64(not_after_ms);
    match request {
        Request::Hello { protocol_version } => w.u8(*protocol_version),
        Request::Snapshot { include_screenshot } => w.bool(*include_screenshot),
        Request::Perform { command } => write_command(&mut w, command)?,
        Request::LockState | Request::ViewerScreenshot | Request::FrontBundleId => {}
    }
    Ok(w.buf)
}

/// Decodes a request: its id, its expiry (`not_after_ms`) and the request.
pub fn decode_request(bytes: &[u8]) -> Result<(u64, u64, Request), MessageError> {
    let mut r = Reader::new(bytes);
    let kind = r.u8()?;
    let id = r.u64()?;
    let not_after_ms = r.u64()?;
    let request = match kind {
        REQ_HELLO => Request::Hello {
            protocol_version: r.u8()?,
        },
        REQ_SNAPSHOT => Request::Snapshot {
            include_screenshot: r.bool()?,
        },
        REQ_PERFORM => Request::Perform {
            command: read_command(&mut r)?,
        },
        REQ_LOCK => Request::LockState,
        REQ_VIEWER_SHOT => Request::ViewerScreenshot,
        REQ_FRONT => Request::FrontBundleId,
        _ => return Err(MessageError::Unknown("request kind")),
    };
    r.finish()?;
    Ok((id, not_after_ms, request))
}

/// Encodes a reply with the id of the request it answers.
pub fn encode_reply(id: u64, reply: &Reply) -> Result<Vec<u8>, MessageError> {
    let mut w = Writer::default();
    match reply {
        Reply::Hello {
            protocol_version,
            runner,
        } => {
            w.u8(REP_HELLO);
            w.u64(id);
            w.u8(*protocol_version);
            w.str(runner)?;
        }
        Reply::Snapshot(snapshot) => {
            w.u8(REP_SNAPSHOT);
            w.u64(id);
            write_snapshot(&mut w, snapshot)?;
        }
        Reply::Performed(result) => {
            w.u8(REP_PERFORMED);
            w.u64(id);
            write_result(&mut w, result)?;
        }
        Reply::LockState(state) => {
            w.u8(REP_LOCK);
            w.u64(id);
            w.u8(lock_byte(*state));
        }
        Reply::ViewerScreenshot(shot) => {
            w.u8(REP_VIEWER_SHOT);
            w.u64(id);
            write_opt_screenshot(&mut w, shot.as_ref())?;
        }
        Reply::FrontBundleId(front) => {
            w.u8(REP_FRONT);
            w.u64(id);
            w.opt_str(front.as_deref())?;
        }
        Reply::Error { message } => {
            w.u8(REP_ERROR);
            w.u64(id);
            w.str(message)?;
        }
    }
    Ok(w.buf)
}

/// Decodes a reply and the id of the request it answers.
pub fn decode_reply(bytes: &[u8]) -> Result<(u64, Reply), MessageError> {
    let mut r = Reader::new(bytes);
    let kind = r.u8()?;
    let id = r.u64()?;
    let reply = match kind {
        REP_HELLO => Reply::Hello {
            protocol_version: r.u8()?,
            runner: r.string()?,
        },
        REP_SNAPSHOT => Reply::Snapshot(Box::new(read_snapshot(&mut r)?)),
        REP_PERFORMED => Reply::Performed(read_result(&mut r)?),
        REP_LOCK => Reply::LockState(read_lock(&mut r)?),
        REP_VIEWER_SHOT => Reply::ViewerScreenshot(read_opt_screenshot(&mut r)?),
        REP_FRONT => Reply::FrontBundleId(r.opt_string()?),
        REP_ERROR => Reply::Error {
            message: r.string()?,
        },
        _ => return Err(MessageError::Unknown("reply kind")),
    };
    r.finish()?;
    Ok((id, reply))
}

// ---------------------------------------------------------------------------------------
// Commands and results.
// ---------------------------------------------------------------------------------------

const CMD_TAP: u8 = 1;
const CMD_LONG_PRESS: u8 = 2;
const CMD_SWIPE: u8 = 3;
const CMD_TYPE: u8 = 4;
const CMD_KEY: u8 = 5;
const CMD_LAUNCH: u8 = 6;
const CMD_OPEN_URL: u8 = 7;
const CMD_WAIT: u8 = 8;

fn write_expect(w: &mut Writer, expect: Option<&Expect>) -> Result<(), MessageError> {
    w.opt_str(expect.map(|e| e.bundle_id.as_str()))
}

fn read_expect(r: &mut Reader<'_>) -> Result<Option<Expect>, MessageError> {
    Ok(r.opt_string()?.map(|bundle_id| Expect { bundle_id }))
}

fn write_command(w: &mut Writer, command: &RunnerCommand) -> Result<(), MessageError> {
    match command {
        RunnerCommand::Tap { x, y, expect } => {
            w.u8(CMD_TAP);
            w.f64(*x)?;
            w.f64(*y)?;
            write_expect(w, expect.as_ref())
        }
        RunnerCommand::LongPress {
            x,
            y,
            duration_ms,
            expect,
        } => {
            w.u8(CMD_LONG_PRESS);
            w.f64(*x)?;
            w.f64(*y)?;
            w.u64(*duration_ms);
            write_expect(w, expect.as_ref())
        }
        RunnerCommand::Swipe {
            from,
            to,
            duration_ms,
            expect,
        } => {
            w.u8(CMD_SWIPE);
            w.f64(from.x)?;
            w.f64(from.y)?;
            w.f64(to.x)?;
            w.f64(to.y)?;
            w.u64(*duration_ms);
            write_expect(w, expect.as_ref())
        }
        RunnerCommand::TypeText { text, expect } => {
            w.u8(CMD_TYPE);
            w.str(text)?;
            write_expect(w, expect.as_ref())
        }
        RunnerCommand::PressKey { key, expect } => {
            w.u8(CMD_KEY);
            w.u8(key_byte(*key));
            write_expect(w, expect.as_ref())
        }
        RunnerCommand::LaunchApp { bundle_id } => {
            w.u8(CMD_LAUNCH);
            w.str(bundle_id)
        }
        RunnerCommand::OpenUrl { url } => {
            w.u8(CMD_OPEN_URL);
            w.str(url)
        }
        RunnerCommand::Wait { ms } => {
            w.u8(CMD_WAIT);
            w.u64(*ms);
            Ok(())
        }
    }
}

fn read_command(r: &mut Reader<'_>) -> Result<RunnerCommand, MessageError> {
    Ok(match r.u8()? {
        CMD_TAP => RunnerCommand::Tap {
            x: r.f64()?,
            y: r.f64()?,
            expect: read_expect(r)?,
        },
        CMD_LONG_PRESS => RunnerCommand::LongPress {
            x: r.f64()?,
            y: r.f64()?,
            duration_ms: r.u64()?,
            expect: read_expect(r)?,
        },
        CMD_SWIPE => RunnerCommand::Swipe {
            from: Point {
                x: r.f64()?,
                y: r.f64()?,
            },
            to: Point {
                x: r.f64()?,
                y: r.f64()?,
            },
            duration_ms: r.u64()?,
            expect: read_expect(r)?,
        },
        CMD_TYPE => RunnerCommand::TypeText {
            text: r.string()?,
            expect: read_expect(r)?,
        },
        CMD_KEY => RunnerCommand::PressKey {
            key: read_key(r)?,
            expect: read_expect(r)?,
        },
        CMD_LAUNCH => RunnerCommand::LaunchApp {
            bundle_id: r.string()?,
        },
        CMD_OPEN_URL => RunnerCommand::OpenUrl { url: r.string()? },
        CMD_WAIT => RunnerCommand::Wait { ms: r.u64()? },
        _ => return Err(MessageError::Unknown("command")),
    })
}

fn key_byte(key: Key) -> u8 {
    match key {
        Key::Return => 0,
        Key::Delete => 1,
        Key::Tab => 2,
        Key::Space => 3,
        Key::Escape => 4,
        Key::Home => 5,
    }
}

fn read_key(r: &mut Reader<'_>) -> Result<Key, MessageError> {
    Ok(match r.u8()? {
        0 => Key::Return,
        1 => Key::Delete,
        2 => Key::Tab,
        3 => Key::Space,
        4 => Key::Escape,
        5 => Key::Home,
        _ => return Err(MessageError::Unknown("key")),
    })
}

fn write_result(w: &mut Writer, result: &RunnerResult) -> Result<(), MessageError> {
    w.bool(result.ok);
    w.opt_str(result.error.as_deref())?;
    w.u8(match result.failure {
        None => 0,
        Some(RunnerFailure::DeviceLocked) => 1,
        Some(RunnerFailure::Stale) => 2,
        Some(RunnerFailure::Other) => 3,
    });
    Ok(())
}

fn read_result(r: &mut Reader<'_>) -> Result<RunnerResult, MessageError> {
    let ok = r.bool()?;
    let error = r.opt_string()?;
    let failure = match r.u8()? {
        0 => None,
        1 => Some(RunnerFailure::DeviceLocked),
        2 => Some(RunnerFailure::Stale),
        3 => Some(RunnerFailure::Other),
        _ => return Err(MessageError::Unknown("failure")),
    };
    Ok(RunnerResult { ok, error, failure })
}

fn lock_byte(state: LockState) -> u8 {
    match state {
        LockState::Locked => 0,
        LockState::Unlocked => 1,
        LockState::Unknown => 2,
    }
}

fn read_lock(r: &mut Reader<'_>) -> Result<LockState, MessageError> {
    Ok(match r.u8()? {
        0 => LockState::Locked,
        1 => LockState::Unlocked,
        2 => LockState::Unknown,
        _ => return Err(MessageError::Unknown("lock state")),
    })
}

// ---------------------------------------------------------------------------------------
// Snapshots and screenshots.
// ---------------------------------------------------------------------------------------

/// The bytes [`write_opt_screenshot`] writes for a snapshot's screenshot.
fn screenshot_len(s: &Snapshot) -> usize {
    s.screenshot.as_ref().map_or(1, |shot| {
        1 + 4 + shot.mime_type.len() + 4 + 4 + shot.scale.map_or(1, |_| 9) + 4 + shot.data.len()
    })
}

fn write_opt_screenshot(w: &mut Writer, shot: Option<&Screenshot>) -> Result<(), MessageError> {
    let Some(shot) = shot else {
        w.u8(0);
        return Ok(());
    };
    if shot.data.len() > MAX_SCREENSHOT_BYTES {
        return Err(MessageError::Limit("screenshot"));
    }
    w.u8(1);
    w.str(&shot.mime_type)?;
    w.u32(shot.pixel_width);
    w.u32(shot.pixel_height);
    match shot.scale {
        None => w.u8(0),
        Some(scale) => {
            w.u8(1);
            w.f64(scale)?;
        }
    }
    w.bytes(&shot.data, "screenshot")
}

fn read_opt_screenshot(r: &mut Reader<'_>) -> Result<Option<Screenshot>, MessageError> {
    match r.u8()? {
        0 => return Ok(None),
        1 => {}
        _ => return Err(MessageError::Unknown("option tag")),
    }
    let mime_type = r.string()?;
    let pixel_width = r.u32()?;
    let pixel_height = r.u32()?;
    let scale = match r.u8()? {
        0 => None,
        1 => Some(r.f64()?),
        _ => return Err(MessageError::Unknown("option tag")),
    };
    let data = r.bytes()?;
    if data.len() > MAX_SCREENSHOT_BYTES {
        return Err(MessageError::Limit("screenshot"));
    }
    Ok(Some(Screenshot {
        data: data.to_vec(),
        mime_type,
        pixel_width,
        pixel_height,
        scale,
    }))
}

fn orientation_byte(o: Orientation) -> u8 {
    match o {
        Orientation::Portrait => 0,
        Orientation::PortraitUpsideDown => 1,
        Orientation::LandscapeLeft => 2,
        Orientation::LandscapeRight => 3,
    }
}

fn read_orientation(r: &mut Reader<'_>) -> Result<Orientation, MessageError> {
    Ok(match r.u8()? {
        0 => Orientation::Portrait,
        1 => Orientation::PortraitUpsideDown,
        2 => Orientation::LandscapeLeft,
        3 => Orientation::LandscapeRight,
        _ => return Err(MessageError::Unknown("orientation")),
    })
}

fn write_snapshot(w: &mut Writer, s: &Snapshot) -> Result<(), MessageError> {
    w.str(&s.app_bundle_id)?;
    w.opt_str(s.app_name.as_deref())?;
    w.f64(s.screen_size.width)?;
    w.f64(s.screen_size.height)?;
    w.bool(s.keyboard_visible);
    w.u8(orientation_byte(s.orientation));
    w.bool(s.device_locked);
    w.u8(lock_byte(s.lock_state));
    if s.notes.len() > MAX_NOTES {
        return Err(MessageError::Limit("notes"));
    }
    // Review pass 32 (F2): what the tree may take of the frame — the frame's body less the
    // seal, the headroom, what is written so far (and the cut flag) and what the notes and
    // the screenshot take after it.
    let rest = 1 + 4 + s.notes.iter().map(|n| 4 + n.len()).sum::<usize>() + screenshot_len(s);
    let budget = crate::MAX_SERVER_FRAME
        .saturating_sub(crate::channel::DATA_OVERHEAD + WIRE_HEADROOM + w.buf.len() + rest);
    // Review pass 31 (L1): the cut, decided before anything of the tree is written.
    let plan = plan_tree(&s.root, budget);
    w.bool(s.tree_cut || plan.cut);
    w.length(s.notes.len(), "notes")?;
    for note in &s.notes {
        w.str(note)?;
    }
    write_opt_screenshot(w, s.screenshot.as_ref())?;
    write_planned(w, &plan)
}

fn read_snapshot(r: &mut Reader<'_>) -> Result<Snapshot, MessageError> {
    let app_bundle_id = r.string()?;
    let app_name = r.opt_string()?;
    let screen_size = Size {
        width: r.f64()?,
        height: r.f64()?,
    };
    let keyboard_visible = r.bool()?;
    let orientation = read_orientation(r)?;
    let device_locked = r.bool()?;
    let lock_state = read_lock(r)?;
    let tree_cut = r.bool()?;
    // Each note is at least its 4-byte length.
    let count = r.count(4)?;
    if count > MAX_NOTES {
        return Err(MessageError::Limit("notes"));
    }
    let mut notes = Vec::with_capacity(count);
    for _ in 0..count {
        notes.push(r.string()?);
    }
    let screenshot = read_opt_screenshot(r)?;
    let root = read_tree(r)?;
    Ok(Snapshot {
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
    })
}

// ---------------------------------------------------------------------------------------
// Trees: pre-order, each node followed by its child count; no recursion either way.
// ---------------------------------------------------------------------------------------

/// Smallest encoded node: role (1), four option tags (4), frame (32), flags (1), child
/// count (4).
const MIN_NODE_LEN: usize = 42;

const ROLE_OTHER: u8 = 0xFF;

fn role_byte(role: &Role) -> u8 {
    match role {
        Role::Button => 0,
        Role::Link => 1,
        Role::TextField => 2,
        Role::SecureTextField => 3,
        Role::TextView => 4,
        Role::SearchField => 5,
        Role::StaticText => 6,
        Role::Image => 7,
        Role::Cell => 8,
        Role::Switch => 9,
        Role::Slider => 10,
        Role::Picker => 11,
        Role::Tab => 12,
        Role::NavigationBar => 13,
        Role::TabBar => 14,
        Role::Toolbar => 15,
        Role::Alert => 16,
        Role::Sheet => 17,
        Role::Keyboard => 18,
        Role::Key => 19,
        Role::ScrollView => 20,
        Role::Table => 21,
        Role::CollectionView => 22,
        Role::WebView => 23,
        Role::Window => 24,
        Role::Application => 25,
        Role::Other(_) => ROLE_OTHER,
    }
}

fn read_role(r: &mut Reader<'_>) -> Result<Role, MessageError> {
    Ok(match r.u8()? {
        0 => Role::Button,
        1 => Role::Link,
        2 => Role::TextField,
        3 => Role::SecureTextField,
        4 => Role::TextView,
        5 => Role::SearchField,
        6 => Role::StaticText,
        7 => Role::Image,
        8 => Role::Cell,
        9 => Role::Switch,
        10 => Role::Slider,
        11 => Role::Picker,
        12 => Role::Tab,
        13 => Role::NavigationBar,
        14 => Role::TabBar,
        15 => Role::Toolbar,
        16 => Role::Alert,
        17 => Role::Sheet,
        18 => Role::Keyboard,
        19 => Role::Key,
        20 => Role::ScrollView,
        21 => Role::Table,
        22 => Role::CollectionView,
        23 => Role::WebView,
        24 => Role::Window,
        25 => Role::Application,
        ROLE_OTHER => Role::Other(r.string()?),
        _ => return Err(MessageError::Unknown("role")),
    })
}

const FLAG_ENABLED: u8 = 1 << 0;
const FLAG_VISIBLE: u8 = 1 << 1;
const FLAG_HITTABLE: u8 = 1 << 2;
const FLAG_FOCUSED: u8 = 1 << 3;
const FLAG_SELECTED: u8 = 1 << 4;
const FLAG_SECURE: u8 = 1 << 5;
const FLAG_ALL: u8 =
    FLAG_ENABLED | FLAG_VISIBLE | FLAG_HITTABLE | FLAG_FOCUSED | FLAG_SELECTED | FLAG_SECURE;

/// Writes one node without its children, followed by its child count.
#[cfg(test)]
fn write_node(w: &mut Writer, n: &UiNode) -> Result<(), MessageError> {
    write_node_with(w, n, n.children.len())
}

/// Writes one node without its children, followed by `children`, the number of its
/// children written after it.
fn write_node_with(w: &mut Writer, n: &UiNode, children: usize) -> Result<(), MessageError> {
    w.u8(role_byte(&n.role));
    if let Role::Other(name) = &n.role {
        w.str(capped(name).0)?;
    }
    for s in node_strings(n) {
        w.opt_str(s.map(|s| capped(s).0))?;
    }
    w.f64(n.frame.x)?;
    w.f64(n.frame.y)?;
    w.f64(n.frame.width)?;
    w.f64(n.frame.height)?;
    let mut flags = 0u8;
    for (on, bit) in [
        (n.enabled, FLAG_ENABLED),
        (n.visible, FLAG_VISIBLE),
        (n.hittable, FLAG_HITTABLE),
        (n.focused, FLAG_FOCUSED),
        (n.selected, FLAG_SELECTED),
        (n.secure, FLAG_SECURE),
    ] {
        if on {
            flags |= bit;
        }
    }
    w.u8(flags);
    w.length(children, "children")
}

/// Reads one node (children still empty) and its child count.
fn read_node(r: &mut Reader<'_>) -> Result<(UiNode, usize), MessageError> {
    let role = read_role(r)?;
    let label = r.opt_string()?;
    let value = r.opt_string()?;
    let identifier = r.opt_string()?;
    let placeholder = r.opt_string()?;
    let frame = Rect {
        x: r.f64()?,
        y: r.f64()?,
        width: r.f64()?,
        height: r.f64()?,
    };
    let flags = r.u8()?;
    if flags & !FLAG_ALL != 0 {
        return Err(MessageError::Unknown("node flags"));
    }
    // Every child takes at least MIN_NODE_LEN more bytes.
    let children = r.count(MIN_NODE_LEN)?;
    Ok((
        UiNode {
            role,
            label,
            value,
            identifier,
            placeholder,
            frame,
            enabled: flags & FLAG_ENABLED != 0,
            visible: flags & FLAG_VISIBLE != 0,
            hittable: flags & FLAG_HITTABLE != 0,
            focused: flags & FLAG_FOCUSED != 0,
            selected: flags & FLAG_SELECTED != 0,
            secure: flags & FLAG_SECURE != 0,
            // Not reserved: nested claims would multiply; children grow as they are read.
            children: Vec::new(),
        },
        children,
    ))
}

/// Review pass 32 (F2): the longest text of one node the wire carries (its label, value,
/// identifier, placeholder and `Other` role name, each), in bytes. A longer one is cut at a
/// character boundary at or below this and the snapshot is flagged cut (`tree_cut`): the
/// rules read label text, so a shortened text is never passed on as the whole screen.
pub const MAX_NODE_STRING_BYTES: usize = 64 * 1024;

/// Review pass 32 (F2): bytes of the server frame kept free below the tree's budget.
const WIRE_HEADROOM: usize = 4 * 1024;

/// `s` as the wire carries it: at most [`MAX_NODE_STRING_BYTES`] (cut at a character
/// boundary), and whether it was cut.
fn capped(s: &str) -> (&str, bool) {
    if s.len() <= MAX_NODE_STRING_BYTES {
        return (s, false);
    }
    let mut end = MAX_NODE_STRING_BYTES;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s.get(..end).unwrap_or(""), true)
}

/// A node's optional texts in wire order.
fn node_strings(n: &UiNode) -> [Option<&str>; 4] {
    [
        n.label.as_deref(),
        n.value.as_deref(),
        n.identifier.as_deref(),
        n.placeholder.as_deref(),
    ]
}

/// The bytes [`write_node_with`] writes for `n`, and whether one of its texts is cut.
fn node_len(n: &UiNode) -> (usize, bool) {
    let mut cut = false;
    let mut text = |s: &str| {
        let (s, c) = capped(s);
        cut |= c;
        4 + s.len()
    };
    let mut len = 1 + 32 + 1 + 4;
    if let Role::Other(name) = &n.role {
        len += text(name);
    }
    for s in node_strings(n) {
        len += 1 + s.map_or(0, &mut text);
    }
    (len, cut)
}

/// What of a tree the encoder writes (review pass 31, L1): the nodes in pre-order, each with
/// the number of its children written, and whether anything was left out.
struct TreePlan<'a> {
    nodes: Vec<(&'a UiNode, usize)>,
    cut: bool,
}

/// The tree as written: no node deeper than [`MAX_TREE_DEPTH`] levels (the children of a
/// node at the bound are left out) and the first [`MAX_TREE_NODES`] nodes in pre-order
/// (a prefix, so every written node's written children are a prefix of its children). A
/// tree past either bound is cut there and the snapshot says so (`tree_cut`); it was an
/// error ("the reply could not be encoded"), so a screen deeper than the bound could not be
/// read at all and the session could not leave it. Iterative.
///
/// Review pass 32 (F2): and no more than `budget` bytes of nodes (the root is always
/// written): the pre-order prefix stops before the first node that would not fit, so the
/// snapshot always fits the server frame (it was cut by node count only: 20,000 nodes of
/// 1,000-byte labels encoded past the frame and the server answered "the reply is too
/// large", so every look failed). Each text is at most [`MAX_NODE_STRING_BYTES`]; a cut text
/// is a cut too. Linear in the nodes written.
fn plan_tree(root: &UiNode, budget: usize) -> TreePlan<'_> {
    let mut nodes: Vec<(&UiNode, usize)> = Vec::new();
    let mut cut = false;
    let mut used = 0usize;
    // (node, depth, index of its parent in `nodes`)
    let mut stack: Vec<(&UiNode, usize, Option<usize>)> = vec![(root, 1, None)];
    while let Some((node, depth, parent)) = stack.pop() {
        if nodes.len() >= MAX_TREE_NODES {
            cut = true;
            break;
        }
        let (len, text_cut) = node_len(node);
        if !nodes.is_empty() && used.saturating_add(len) > budget {
            cut = true;
            break;
        }
        used = used.saturating_add(len);
        cut |= text_cut;
        let here = nodes.len();
        nodes.push((node, 0));
        if let Some((_, written)) = parent.and_then(|p| nodes.get_mut(p)) {
            *written += 1;
        }
        if depth >= MAX_TREE_DEPTH {
            cut |= !node.children.is_empty();
            continue;
        }
        for child in node.children.iter().rev() {
            stack.push((child, depth + 1, Some(here)));
        }
    }
    TreePlan { nodes, cut }
}

fn write_planned(w: &mut Writer, plan: &TreePlan<'_>) -> Result<(), MessageError> {
    for (node, written) in &plan.nodes {
        write_node_with(w, node, *written)?;
    }
    Ok(())
}

#[cfg(test)]
fn write_tree(w: &mut Writer, root: &UiNode) -> Result<bool, MessageError> {
    let plan = plan_tree(root, usize::MAX);
    write_planned(w, &plan)?;
    Ok(plan.cut)
}

fn read_tree(r: &mut Reader<'_>) -> Result<UiNode, MessageError> {
    // Open nodes from the root down, each with the number of children still to read.
    let mut open: Vec<(UiNode, usize)> = Vec::new();
    let mut count = 0usize;
    loop {
        let (node, children) = read_node(r)?;
        count += 1;
        if count > MAX_TREE_NODES {
            return Err(MessageError::Limit("tree nodes"));
        }
        // This node's depth is open.len() + 1.
        if open.len() + 1 > MAX_TREE_DEPTH {
            return Err(MessageError::Limit("tree depth"));
        }
        if children > 0 {
            open.push((node, children));
            continue;
        }
        // A complete node: attach it, closing every parent it completes.
        let mut done = node;
        loop {
            let Some((parent, left)) = open.last_mut() else {
                return Ok(done);
            };
            parent.children.push(done);
            *left -= 1;
            if *left > 0 {
                break;
            }
            match open.pop() {
                Some((closed, _)) => done = closed,
                None => return Err(MessageError::Truncated),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(label: &str) -> UiNode {
        UiNode::new(Role::Button, Rect::new(1.0, 2.0, 3.0, 4.0)).with_label(label)
    }

    #[test]
    fn a_nested_tree_round_trips_in_order() {
        let mut root = UiNode::new(Role::Application, Rect::new(0.0, 0.0, 10.0, 10.0));
        let mut mid = UiNode::new(Role::Other("Foo".into()), Rect::new(0.0, 0.0, 5.0, 5.0));
        mid.children = vec![leaf("a"), leaf("b")];
        root.children = vec![mid, leaf("c")];
        let mut w = Writer::default();
        write_tree(&mut w, &root).unwrap();
        let mut r = Reader::new(&w.buf);
        assert_eq!(read_tree(&mut r).unwrap(), root);
        r.finish().unwrap();
    }

    #[test]
    fn depth_is_limited_both_ways() {
        let mut node = leaf("x");
        for _ in 0..MAX_TREE_DEPTH {
            let mut parent = leaf("p");
            parent.children = vec![node];
            node = parent;
        }
        // Review pass 31 (L1): the writer cuts a chain one level too deep at the bound and
        // says so (it refused it); what it wrote reads back at exactly the bound.
        let mut w = Writer::default();
        assert_eq!(write_tree(&mut w, &node), Ok(true));
        let mut r = Reader::new(&w.buf);
        let back = read_tree(&mut r).unwrap();
        r.finish().unwrap();
        let mut depth = 0usize;
        let mut n = &back;
        loop {
            depth += 1;
            match n.children.first() {
                Some(c) => n = c,
                None => break,
            }
        }
        assert_eq!(depth, MAX_TREE_DEPTH);
        jarvis_proto::drop_iteratively(vec![back, node]);
        // Hand-built: a chain one level too deep is refused by the reader.
        let mut w = Writer::default();
        for i in 0..=MAX_TREE_DEPTH {
            let mut n = leaf("p");
            n.children = if i < MAX_TREE_DEPTH {
                vec![leaf("c")]
            } else {
                vec![]
            };
            let only = UiNode {
                children: vec![],
                ..n.clone()
            };
            write_node(&mut w, &only).unwrap();
            // Overwrite the child count just written.
            let len = w.buf.len();
            let c = u32::from(i < MAX_TREE_DEPTH);
            w.buf[len - 4..].copy_from_slice(&c.to_be_bytes());
        }
        let mut r = Reader::new(&w.buf);
        assert_eq!(read_tree(&mut r), Err(MessageError::Limit("tree depth")));
    }
}
