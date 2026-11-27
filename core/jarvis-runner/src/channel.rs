//! The two state machines: [`RunnerClientChannel`] (the Jarvis app) and
//! [`RunnerServerChannel`] (the runner process). One channel per connection; a failed
//! channel stays failed (every later call returns [`RunnerError::Closed`]) and the shell
//! drops the connection.
//!
//! ```text
//! client                                   server
//!   start()        -- ClientHello(Nc) -->
//!                  <-- ServerHello(Ns, mac_s) --   receive(): Send
//!   receive(): verify mac_s, Send ClientFinish, Authenticated
//!                  -- ClientFinish(mac_c) -->      receive(): verify mac_c, Authenticated
//!   request()      -- Data(c2s, 0) -->             receive(): Request
//!                  <-- Data(s2c, 0) --             reply()
//!   receive(): Reply
//! ```
//!
//! The client sends no request before it has verified the server's proof, so a squatting
//! server learns nothing of what Jarvis would do and can serve nothing Jarvis accepts. The
//! server accepts no request before it has verified the client's proof. Exactly one
//! request is outstanding at a time; a reply must carry the outstanding request's id and
//! answer its kind (or be an error).
//!
//! Every request carries `not_after_ms`, set by the client from its own deadline for that
//! call. The server channel is given the shell's `now_ms` with every `receive` and refuses
//! a request whose `not_after_ms` has passed ([`RunnerError::Expired`], the channel
//! closes): a request held back by anything between the two processes and delivered late
//! is never surfaced. The server keeps the latest `now_ms` it saw, so a clock reading that
//! goes backwards cannot make an expired request current again.

use crate::crypto::{
    nonce_from_entropy, Psk, SessionKeys, DIR_C2S, DIR_S2C, KIND_DATA, MAC_LEN, MAGIC, NONCE_LEN,
    TAG_LEN,
};
use crate::error::RunnerError;
use crate::frame::{frame, Deframer};
use crate::message::{
    decode_reply, decode_request, encode_reply, encode_request, Reply, ReplyKind, Request,
};
use crate::{MAX_CLIENT_FRAME, MAX_SERVER_FRAME, PROTOCOL_VERSION};

const T_CLIENT_HELLO: u8 = 0x01;
const T_SERVER_HELLO: u8 = 0x02;
const T_CLIENT_FINISH: u8 = 0x03;

/// Body length of a ClientHello.
pub const CLIENT_HELLO_LEN: usize = 1 + MAGIC.len() + 1 + NONCE_LEN;
/// Body length of a ServerHello.
pub const SERVER_HELLO_LEN: usize = 1 + MAGIC.len() + 1 + NONCE_LEN + MAC_LEN;
/// Body length of a ClientFinish.
pub const CLIENT_FINISH_LEN: usize = 1 + MAC_LEN;
/// Bytes a data frame body adds to its plaintext: kind, counter, tag.
pub const DATA_OVERHEAD: usize = 1 + 8 + TAG_LEN;

const CLIENT_LABEL: &[u8] = b"client-nonce";
const SERVER_LABEL: &[u8] = b"server-nonce";

/// Seals outgoing and opens incoming data frames, with strict counters.
struct Sealer {
    keys: SessionKeys,
    send_dir: u8,
    recv_dir: u8,
    send_counter: u64,
    recv_counter: u64,
}

impl Sealer {
    fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, RunnerError> {
        let counter = self.send_counter;
        let next = counter.checked_add(1).ok_or(RunnerError::Closed)?;
        let sealed = self.keys.seal(self.send_dir, counter, plaintext)?;
        let mut body = Vec::with_capacity(1 + 8 + sealed.len());
        body.push(KIND_DATA);
        body.extend_from_slice(&counter.to_be_bytes());
        body.extend_from_slice(&sealed);
        let out = frame(&body)?;
        self.send_counter = next;
        Ok(out)
    }

    fn open(&mut self, body: &[u8]) -> Result<Vec<u8>, RunnerError> {
        let Some((&kind, rest)) = body.split_first() else {
            return Err(RunnerError::Frame("empty frame"));
        };
        if kind != KIND_DATA {
            return Err(RunnerError::Frame("unexpected frame"));
        }
        if rest.len() < 8 + TAG_LEN {
            return Err(RunnerError::Frame("short data frame"));
        }
        let (counter_bytes, sealed) = rest.split_at(8);
        let mut counter = [0u8; 8];
        counter.copy_from_slice(counter_bytes);
        let counter = u64::from_be_bytes(counter);
        // Strict order: a replayed, reordered or skipped frame fails here.
        if counter != self.recv_counter {
            return Err(RunnerError::Auth);
        }
        let plaintext = self.keys.open(self.recv_dir, counter, sealed)?;
        self.recv_counter = counter.checked_add(1).ok_or(RunnerError::Closed)?;
        Ok(plaintext)
    }
}

/// Reads `MAGIC || version` from a hello body after its kind byte.
fn check_magic_version(rest: &[u8]) -> Result<&[u8], RunnerError> {
    let (magic, rest) = rest
        .split_at_checked(MAGIC.len())
        .ok_or(RunnerError::Frame("short hello"))?;
    if magic != MAGIC {
        return Err(RunnerError::Frame("not a Jarvis runner hello"));
    }
    let (&version, rest) = rest
        .split_first()
        .ok_or(RunnerError::Frame("short hello"))?;
    if version != PROTOCOL_VERSION {
        return Err(RunnerError::Frame("unsupported protocol version"));
    }
    Ok(rest)
}

fn array32(bytes: &[u8]) -> Result<[u8; 32], RunnerError> {
    bytes
        .try_into()
        .map_err(|_| RunnerError::Frame("short hello"))
}

// ---------------------------------------------------------------------------------------
// Client.
// ---------------------------------------------------------------------------------------

/// What the client must do or know after [`RunnerClientChannel::receive`].
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    /// Write these bytes to the connection (the ClientFinish).
    Send(Vec<u8>),
    /// The server proved the key; requests may be sent now.
    Authenticated,
    /// The answer to the outstanding request.
    Reply {
        /// The request's id.
        id: u64,
        /// The answer.
        reply: Reply,
    },
}

enum ClientState {
    New { psk: Psk, nc: [u8; NONCE_LEN] },
    AwaitServerHello { psk: Psk, nc: [u8; NONCE_LEN] },
    Open(Sealer),
    Closed,
}

/// The Jarvis app's side of one connection.
pub struct RunnerClientChannel {
    state: ClientState,
    deframer: Deframer,
    next_id: u64,
    pending: Option<(u64, ReplyKind)>,
}

impl std::fmt::Debug for RunnerClientChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerClientChannel")
            .field("open", &self.is_open())
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

impl RunnerClientChannel {
    /// A channel for one new connection. `key` is the 32-byte pre-shared key; `entropy`
    /// at least 32 fresh CSPRNG bytes (never reused across connections).
    pub fn new(key: &[u8], entropy: &[u8]) -> Result<Self, RunnerError> {
        let psk = Psk::new(key)?;
        let nc = nonce_from_entropy(entropy, CLIENT_LABEL)?;
        Ok(Self {
            state: ClientState::New { psk, nc },
            // A server says nothing larger than its hello before it is authenticated.
            deframer: Deframer::new(SERVER_HELLO_LEN),
            next_id: 1,
            pending: None,
        })
    }

    /// The ClientHello to write first. Valid once.
    pub fn start(&mut self) -> Result<Vec<u8>, RunnerError> {
        match std::mem::replace(&mut self.state, ClientState::Closed) {
            ClientState::New { psk, nc } => {
                let mut body = Vec::with_capacity(CLIENT_HELLO_LEN);
                body.push(T_CLIENT_HELLO);
                body.extend_from_slice(MAGIC);
                body.push(PROTOCOL_VERSION);
                body.extend_from_slice(&nc);
                self.state = ClientState::AwaitServerHello { psk, nc };
                frame(&body)
            }
            ClientState::Closed => Err(RunnerError::Closed),
            other => {
                self.state = other;
                Err(RunnerError::State("already started"))
            }
        }
    }

    /// Whether the server proved the key and the channel has not failed.
    pub fn is_open(&self) -> bool {
        matches!(self.state, ClientState::Open(_))
    }

    /// Whether a request awaits its reply.
    pub fn has_pending_request(&self) -> bool {
        self.pending.is_some()
    }

    /// Feeds bytes read from the connection. Any failure closes the channel.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Vec<ClientEvent>, RunnerError> {
        if matches!(self.state, ClientState::Closed) {
            return Err(RunnerError::Closed);
        }
        let mut events = Vec::new();
        let mut input = bytes;
        let result = (|| {
            while let Some(body) = self.deframer.next_frame(&mut input)? {
                self.handle(&body, &mut events)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(events),
            Err(e) => {
                self.close();
                Err(e)
            }
        }
    }

    fn close(&mut self) {
        self.state = ClientState::Closed;
        self.pending = None;
    }

    fn handle(&mut self, body: &[u8], events: &mut Vec<ClientEvent>) -> Result<(), RunnerError> {
        match std::mem::replace(&mut self.state, ClientState::Closed) {
            ClientState::AwaitServerHello { psk, nc } => {
                if body.len() != SERVER_HELLO_LEN || body.first() != Some(&T_SERVER_HELLO) {
                    return Err(RunnerError::Frame("expected a server hello"));
                }
                let rest = check_magic_version(body.get(1..).unwrap_or_default())?;
                let (ns, mac) = rest
                    .split_at_checked(NONCE_LEN)
                    .ok_or(RunnerError::Frame("short hello"))?;
                let ns = array32(ns)?;
                let keys = SessionKeys::derive(&psk, &nc, &ns)?;
                keys.verify_server(mac)?;
                let mut finish = Vec::with_capacity(CLIENT_FINISH_LEN);
                finish.push(T_CLIENT_FINISH);
                finish.extend_from_slice(&keys.client_mac()?);
                events.push(ClientEvent::Send(frame(&finish)?));
                events.push(ClientEvent::Authenticated);
                self.deframer.set_limit(MAX_SERVER_FRAME);
                self.state = ClientState::Open(Sealer {
                    keys,
                    send_dir: DIR_C2S,
                    recv_dir: DIR_S2C,
                    send_counter: 0,
                    recv_counter: 0,
                });
                Ok(())
            }
            ClientState::Open(mut sealer) => {
                let plaintext = sealer.open(body)?;
                self.state = ClientState::Open(sealer);
                let (id, reply) = decode_reply(&plaintext)
                    .map_err(|_| RunnerError::Protocol("undecodable reply"))?;
                let Some((pending_id, kind)) = self.pending else {
                    return Err(RunnerError::Protocol("a reply nobody asked for"));
                };
                if id != pending_id {
                    return Err(RunnerError::Protocol("a reply to another request"));
                }
                if reply.kind().is_some_and(|k| k != kind) {
                    return Err(RunnerError::Protocol("a reply of the wrong kind"));
                }
                self.pending = None;
                events.push(ClientEvent::Reply { id, reply });
                Ok(())
            }
            ClientState::New { .. } | ClientState::Closed => {
                Err(RunnerError::Frame("the server spoke first"))
            }
        }
    }

    /// Seals a request that the server may act on until `not_after_ms` of the shell's
    /// shared uptime clock (set it from this call's own deadline). Returns its id and the
    /// bytes to write. Refused (nothing sent, the channel stays open) before
    /// authentication, while a request is outstanding, or when the request is too large.
    pub fn request(
        &mut self,
        request: &Request,
        not_after_ms: u64,
    ) -> Result<(u64, Vec<u8>), RunnerError> {
        let sealer = match &mut self.state {
            ClientState::Open(sealer) => sealer,
            ClientState::Closed => return Err(RunnerError::Closed),
            _ => return Err(RunnerError::State("not authenticated")),
        };
        if self.pending.is_some() {
            return Err(RunnerError::State("a request is outstanding"));
        }
        let id = self.next_id;
        let plaintext = encode_request(id, not_after_ms, request).map_err(RunnerError::TooLarge)?;
        if plaintext.len() + DATA_OVERHEAD > MAX_CLIENT_FRAME {
            return Err(RunnerError::TooLarge(crate::MessageError::Limit("request")));
        }
        let bytes = sealer.seal(&plaintext)?;
        self.next_id = id.checked_add(1).ok_or(RunnerError::Closed)?;
        self.pending = Some((id, request.reply_kind()));
        Ok((id, bytes))
    }
}

// ---------------------------------------------------------------------------------------
// Server.
// ---------------------------------------------------------------------------------------

/// What the server must do or know after [`RunnerServerChannel::receive`].
#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    /// Write these bytes to the connection (the ServerHello).
    Send(Vec<u8>),
    /// The client proved the key; requests follow.
    Authenticated,
    /// A request to carry out and answer with [`RunnerServerChannel::reply`].
    Request {
        /// Its id.
        id: u64,
        /// The client's deadline for it (already checked against `now_ms`).
        not_after_ms: u64,
        /// The request.
        request: Request,
    },
}

enum ServerState {
    AwaitClientHello { psk: Psk, ns: [u8; NONCE_LEN] },
    AwaitClientFinish(SessionKeys),
    Open(Sealer),
    Closed,
}

/// The runner's side of one connection.
pub struct RunnerServerChannel {
    state: ServerState,
    deframer: Deframer,
    last_id: u64,
    pending: Option<(u64, ReplyKind)>,
    /// The latest `now_ms` given to `receive` (never decreases).
    now_ms: u64,
}

impl std::fmt::Debug for RunnerServerChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunnerServerChannel")
            .field("open", &self.is_open())
            .field("pending", &self.pending)
            .finish_non_exhaustive()
    }
}

impl RunnerServerChannel {
    /// A channel for one accepted connection. `key` is the 32-byte pre-shared key;
    /// `entropy` at least 32 fresh CSPRNG bytes (never reused across connections).
    pub fn new(key: &[u8], entropy: &[u8]) -> Result<Self, RunnerError> {
        let psk = Psk::new(key)?;
        let ns = nonce_from_entropy(entropy, SERVER_LABEL)?;
        Ok(Self {
            state: ServerState::AwaitClientHello { psk, ns },
            // Before authentication only the two handshake frames are accepted.
            deframer: Deframer::new(CLIENT_HELLO_LEN.max(CLIENT_FINISH_LEN)),
            last_id: 0,
            pending: None,
            now_ms: 0,
        })
    }

    /// Whether the client proved the key and the channel has not failed.
    pub fn is_open(&self) -> bool {
        matches!(self.state, ServerState::Open(_))
    }

    /// Whether a request awaits its reply.
    pub fn has_pending_request(&self) -> bool {
        self.pending.is_some()
    }

    /// Feeds bytes read from the connection, at `now_ms` of the shell's shared uptime clock
    /// (requests received past their `not_after_ms` are refused). Any failure closes the
    /// channel.
    pub fn receive(&mut self, bytes: &[u8], now_ms: u64) -> Result<Vec<ServerEvent>, RunnerError> {
        if matches!(self.state, ServerState::Closed) {
            return Err(RunnerError::Closed);
        }
        // A clock reading that goes backwards is ignored: expiry only ever moves forward.
        self.now_ms = self.now_ms.max(now_ms);
        let mut events = Vec::new();
        let mut input = bytes;
        let result = (|| {
            while let Some(body) = self.deframer.next_frame(&mut input)? {
                self.handle(&body, &mut events)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => Ok(events),
            Err(e) => {
                self.state = ServerState::Closed;
                self.pending = None;
                Err(e)
            }
        }
    }

    fn handle(&mut self, body: &[u8], events: &mut Vec<ServerEvent>) -> Result<(), RunnerError> {
        match std::mem::replace(&mut self.state, ServerState::Closed) {
            ServerState::AwaitClientHello { psk, ns } => {
                if body.len() != CLIENT_HELLO_LEN || body.first() != Some(&T_CLIENT_HELLO) {
                    return Err(RunnerError::Frame("expected a client hello"));
                }
                let rest = check_magic_version(body.get(1..).unwrap_or_default())?;
                let nc = array32(rest)?;
                let keys = SessionKeys::derive(&psk, &nc, &ns)?;
                let mut hello = Vec::with_capacity(SERVER_HELLO_LEN);
                hello.push(T_SERVER_HELLO);
                hello.extend_from_slice(MAGIC);
                hello.push(PROTOCOL_VERSION);
                hello.extend_from_slice(&ns);
                hello.extend_from_slice(&keys.server_mac()?);
                events.push(ServerEvent::Send(frame(&hello)?));
                self.state = ServerState::AwaitClientFinish(keys);
                Ok(())
            }
            ServerState::AwaitClientFinish(keys) => {
                if body.len() != CLIENT_FINISH_LEN || body.first() != Some(&T_CLIENT_FINISH) {
                    return Err(RunnerError::Frame("expected a client finish"));
                }
                keys.verify_client(body.get(1..).unwrap_or_default())?;
                events.push(ServerEvent::Authenticated);
                self.deframer.set_limit(MAX_CLIENT_FRAME);
                self.state = ServerState::Open(Sealer {
                    keys,
                    send_dir: DIR_S2C,
                    recv_dir: DIR_C2S,
                    send_counter: 0,
                    recv_counter: 0,
                });
                Ok(())
            }
            ServerState::Open(mut sealer) => {
                let plaintext = sealer.open(body)?;
                self.state = ServerState::Open(sealer);
                let (id, not_after_ms, request) = decode_request(&plaintext)
                    .map_err(|_| RunnerError::Protocol("undecodable request"))?;
                if self.pending.is_some() {
                    return Err(RunnerError::Protocol("a request while one is outstanding"));
                }
                if id <= self.last_id {
                    return Err(RunnerError::Protocol("request ids must increase"));
                }
                // Checked before anything is surfaced: an expired request is never acted on.
                if self.now_ms > not_after_ms {
                    return Err(RunnerError::Expired);
                }
                self.last_id = id;
                self.pending = Some((id, request.reply_kind()));
                events.push(ServerEvent::Request {
                    id,
                    not_after_ms,
                    request,
                });
                Ok(())
            }
            ServerState::Closed => Err(RunnerError::Closed),
        }
    }

    /// Seals the reply to the outstanding request `id`. A reply that cannot be encoded or
    /// is too large is replaced by [`Reply::Error`] saying so, so the client always hears
    /// an answer. Refused (nothing sent) when `id` is not the outstanding request or the
    /// reply answers another kind.
    pub fn reply(&mut self, id: u64, reply: &Reply) -> Result<Vec<u8>, RunnerError> {
        let sealer = match &mut self.state {
            ServerState::Open(sealer) => sealer,
            ServerState::Closed => return Err(RunnerError::Closed),
            _ => return Err(RunnerError::State("not authenticated")),
        };
        let Some((pending_id, kind)) = self.pending else {
            return Err(RunnerError::State("no request is outstanding"));
        };
        if pending_id != id {
            return Err(RunnerError::State("not the outstanding request"));
        }
        if reply.kind().is_some_and(|k| k != kind) {
            return Err(RunnerError::State("the reply answers another request"));
        }
        let plaintext = match encode_reply(id, reply) {
            Ok(p) if p.len() + DATA_OVERHEAD <= MAX_SERVER_FRAME => p,
            Ok(_) => error_reply(id, "the reply is too large for the runner protocol")?,
            Err(e) => error_reply(id, &format!("the reply could not be encoded: {e}"))?,
        };
        let bytes = sealer.seal(&plaintext)?;
        self.pending = None;
        Ok(bytes)
    }
}

fn error_reply(id: u64, message: &str) -> Result<Vec<u8>, RunnerError> {
    encode_reply(
        id,
        &Reply::Error {
            message: message.to_owned(),
        },
    )
    .map_err(RunnerError::TooLarge)
}
