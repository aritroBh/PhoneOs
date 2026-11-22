//! The remote device's side: [`RemoteClient`], also sans-IO.
//!
//! One `RemoteClient` is one connection. Create it with [`RemoteClient::pair`] (first
//! contact, with the code from the phone) or [`RemoteClient::connect`] (with stored
//! [`ClientCredentials`]), feed it every byte from the phone with
//! [`RemoteClient::receive`], write out every [`ClientEvent::Send`], and once
//! [`ClientEvent::Connected`] arrives, encode commands with [`RemoteClient::send`].
//!
//! The Live view stream ([`crate::viewer`]) comes out of [`RemoteClient::receive_all`]
//! as [`ViewerEvent`]s: the client checks each delta's length and position and, on a gap,
//! asks the phone for a snapshot itself (once per gap). Feed the deltas to the
//! controller's `jarvis_viewer::LiveView::apply_delta`; if that reports a resync, call
//! [`RemoteClient::request_resync`] (which also asks at most once per gap).
//!
//! After pairing the client sends `Hello` by itself: the phone keeps a new device only
//! once it has received an authenticated frame from it, so a dropped `PairAccept` cannot
//! leave an orphan device on the phone. (If that `Hello` is lost instead, the stored
//! credentials will not connect; pair again.)

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::codec::{
    encode_record, from_hex, to_hex_secret, DeviceId, Kind, Record, RecordReader, PROTOCOL_VERSION,
};
use crate::crypto::{
    auth_key, check_entropy, connect_client_mac, derive_device_key, pair_client_mac, pair_key,
    verify_connect_phone_mac, verify_pair_phone_mac, Channel, Ephemeral, PairingSecret, Secret32,
    Side, Transcript, MAC_LEN, NONCE_LEN, PUBLIC_LEN,
};
use crate::error::{ClientError, MessageError, PersistError};
use crate::message::{ClientMessage, ServerMessage};
use crate::pairing::parse_code;
use crate::store::clean_name;
use crate::viewer::{ViewerEvent, ViewerMessage, ViewerRequest};
use jarvis_viewer::delta::{peek, Admit, DeltaClass, DeltaError, StreamCursor};

/// What a remote keeps after pairing. Contains the device key: store it in the platform
/// keychain.
#[derive(Clone, PartialEq, Eq)]
pub struct ClientCredentials {
    /// Id the phone assigned.
    pub device_id: DeviceId,
    /// Long-term key shared with the phone.
    pub device_key: Secret32,
}

impl fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCredentials")
            .field("device_id", &self.device_id)
            .field("device_key", &self.device_key)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialsJson {
    schema_version: u32,
    device_id: DeviceId,
    device_key: Zeroizing<String>,
}

/// Schema version of [`ClientCredentials::to_json`].
pub const CREDENTIALS_VERSION: u32 = 2;

impl ClientCredentials {
    /// Serializes the credentials, key included. Zeroized on drop.
    pub fn to_json(&self) -> Zeroizing<String> {
        Zeroizing::new(
            serde_json::to_string(&CredentialsJson {
                schema_version: CREDENTIALS_VERSION,
                device_id: self.device_id,
                device_key: to_hex_secret(self.device_key.as_bytes()),
            })
            .unwrap_or_default(),
        )
    }

    /// Loads [`ClientCredentials::to_json`] output.
    pub fn from_json(json: &str) -> Result<Self, PersistError> {
        let doc: CredentialsJson =
            serde_json::from_str(json).map_err(|e| PersistError::corrupt(&e))?;
        if doc.schema_version != CREDENTIALS_VERSION {
            return Err(PersistError::UnsupportedVersion(doc.schema_version));
        }
        let key: [u8; 32] = from_hex(&doc.device_key)
            .and_then(|b| b.as_slice().try_into().ok())
            .ok_or_else(|| PersistError::Invalid("invalid key".into()))?;
        Ok(Self {
            device_id: doc.device_id,
            device_key: Secret32::from_bytes(key),
        })
    }
}

/// Something the remote's shell must act on.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientEvent {
    /// Write these bytes to the phone.
    Send {
        /// Bytes to write.
        bytes: Vec<u8>,
    },
    /// Pairing succeeded: persist these credentials.
    Paired {
        /// The new credentials.
        credentials: ClientCredentials,
    },
    /// The connection is authenticated in both directions; commands can be sent.
    Connected,
    /// A message from the phone.
    Message(ServerMessage),
    /// An authenticated message this client could not decode (e.g. from a newer phone).
    Undecodable(MessageError),
}

/// Everything one [`RemoteClient::receive_all`] call produced.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Received {
    /// What the shell must act on (including any resync request to send).
    pub events: Vec<ClientEvent>,
    /// The Live view stream, in order.
    pub viewer: Vec<ViewerEvent>,
}

/// The Live view stream as this client follows it.
#[derive(Debug, Default)]
struct ViewerStream {
    watching: bool,
    cursor: StreamCursor,
    /// A snapshot was asked for (or is coming after `start_watching`): no new request
    /// until a reset arrives.
    resync_pending: bool,
    incompatible: bool,
}

enum State {
    AwaitHello,
    AwaitPairAccept {
        t: Transcript,
        dh: Zeroizing<[u8; 32]>,
    },
    AwaitConnectAccept {
        t: Transcript,
        dh: Zeroizing<[u8; 32]>,
    },
    Established(Box<Channel>),
    Failed,
}

enum Intent {
    Pair { secret: PairingSecret, name: String },
    Connect { credentials: ClientCredentials },
}

/// One connection from a remote device to the phone.
pub struct RemoteClient {
    eph: Option<Ephemeral>,
    intent: Intent,
    state: State,
    reader: RecordReader,
    credentials: Option<ClientCredentials>,
    viewer: ViewerStream,
}

impl fmt::Debug for RemoteClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = match self.state {
            State::AwaitHello => "await_hello",
            State::AwaitPairAccept { .. } => "await_pair_accept",
            State::AwaitConnectAccept { .. } => "await_connect_accept",
            State::Established(_) => "established",
            State::Failed => "failed",
        };
        f.debug_struct("RemoteClient")
            .field("state", &state)
            .finish_non_exhaustive()
    }
}

impl RemoteClient {
    /// First contact: pair with the code (typed or scanned) shown on the phone.
    /// `entropy` (≥ 32 CSPRNG bytes, fresh) seeds this connection's nonce and ephemeral
    /// key.
    pub fn pair(code: &str, device_name: &str, entropy: &[u8]) -> Result<Self, ClientError> {
        let secret = parse_code(code)?;
        let name = clean_name(device_name);
        if name.is_empty() {
            return Err(ClientError::InvalidName);
        }
        Self::new(Intent::Pair { secret, name }, entropy)
    }

    /// Reconnect with stored credentials. `entropy` as for [`RemoteClient::pair`].
    pub fn connect(credentials: ClientCredentials, entropy: &[u8]) -> Result<Self, ClientError> {
        Self::new(Intent::Connect { credentials }, entropy)
    }

    fn new(intent: Intent, entropy: &[u8]) -> Result<Self, ClientError> {
        check_entropy(entropy)?;
        let eph = Ephemeral::remote(entropy).map_err(|_| ClientError::Auth)?;
        Ok(Self {
            eph: Some(eph),
            intent,
            state: State::AwaitHello,
            reader: RecordReader::new(),
            credentials: None,
            viewer: ViewerStream::default(),
        })
    }

    /// Whether commands can be sent.
    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Established(_))
    }

    /// The credentials this connection authenticated with (after pairing or connecting).
    pub fn credentials(&self) -> Option<&ClientCredentials> {
        self.credentials.as_ref()
    }

    /// Feeds bytes from the phone. On `Err` the connection is unusable: close it. Live view
    /// messages are handled (a resync request is among the events) but their
    /// [`ViewerEvent`]s are not returned: use [`RemoteClient::receive_all`] when watching.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Vec<ClientEvent>, ClientError> {
        self.receive_all(bytes).map(|r| r.events)
    }

    /// Feeds bytes from the phone: client events and the Live view stream. On `Err` the
    /// connection is unusable: close it.
    pub fn receive_all(&mut self, bytes: &[u8]) -> Result<Received, ClientError> {
        if matches!(self.state, State::Failed) {
            return Err(ClientError::Auth);
        }
        let result = self.receive_inner(bytes);
        if result.is_err() {
            self.state = State::Failed;
            self.eph = None;
        }
        result
    }

    fn receive_inner(&mut self, bytes: &[u8]) -> Result<Received, ClientError> {
        let mut out = Received::default();
        let mut rest = bytes;
        while !rest.is_empty() {
            let take = self.reader.room().min(rest.len());
            if take == 0 {
                return Err(ClientError::Auth);
            }
            let (chunk, tail) = rest.split_at_checked(take).ok_or(ClientError::Auth)?;
            self.reader.push(chunk).map_err(|_| ClientError::Auth)?;
            rest = tail;
            while let Some(record) = self.reader.next_record().map_err(|_| ClientError::Auth)? {
                self.handle(record, &mut out)?;
            }
        }
        Ok(out)
    }

    fn handle(&mut self, record: Record, out: &mut Received) -> Result<(), ClientError> {
        let events = &mut out.events;
        if record.version != PROTOCOL_VERSION {
            return Err(ClientError::Auth);
        }
        let kind = Kind::from_byte(record.kind).ok_or(ClientError::Auth)?;
        let state = std::mem::replace(&mut self.state, State::Failed);
        self.state = match (state, kind) {
            (State::AwaitHello, Kind::ServerHello) => self.hello(&record.payload, events)?,
            (State::AwaitPairAccept { t, dh }, Kind::PairAccept) => {
                self.pair_accept(&t, &dh, &record.payload, events)?
            }
            (State::AwaitConnectAccept { t, dh }, Kind::ConnectAccept) => {
                self.connect_accept(&t, &dh, &record.payload, events)?
            }
            (State::Established(mut channel), Kind::Data) => {
                let pt = channel
                    .open(&record.payload)
                    .map_err(|_| ClientError::Auth)?;
                match ServerMessage::from_bytes(&pt) {
                    Ok(m) => events.push(ClientEvent::Message(m)),
                    Err(MessageError::UnknownType(t))
                        if ViewerMessage::TYPES.contains(&t.as_str()) =>
                    {
                        match ViewerMessage::from_bytes(&pt) {
                            Ok(m) => self.on_viewer(&mut channel, m, out)?,
                            Err(e) => events.push(ClientEvent::Undecodable(e)),
                        }
                    }
                    Err(e) => events.push(ClientEvent::Undecodable(e)),
                }
                State::Established(channel)
            }
            _ => return Err(ClientError::Auth),
        };
        Ok(())
    }

    fn hello(
        &mut self,
        payload: &[u8],
        events: &mut Vec<ClientEvent>,
    ) -> Result<State, ClientError> {
        let (np, ep) = payload
            .split_at_checked(NONCE_LEN)
            .ok_or(ClientError::Auth)?;
        let np: [u8; NONCE_LEN] = np.try_into().map_err(|_| ClientError::Auth)?;
        let ep: [u8; PUBLIC_LEN] = ep.try_into().map_err(|_| ClientError::Auth)?;
        // The ephemeral is used exactly once; dropping it zeroizes the secret.
        let eph = self.eph.take().ok_or(ClientError::Auth)?;
        let t = Transcript {
            np,
            nc: eph.nonce,
            ep,
            er: eph.public,
        };
        let dh = eph.agree(&ep).map_err(|_| ClientError::Auth)?;
        drop(eph);
        match &self.intent {
            Intent::Pair { secret, name } => {
                let pk = pair_key(secret).map_err(|_| ClientError::Auth)?;
                let mac = pair_client_mac(&pk, &t, name).map_err(|_| ClientError::Auth)?;
                let name_len = u8::try_from(name.len()).map_err(|_| ClientError::InvalidName)?;
                let mut body =
                    Vec::with_capacity(NONCE_LEN + PUBLIC_LEN + 1 + name.len() + MAC_LEN);
                body.extend_from_slice(&t.nc);
                body.extend_from_slice(&t.er);
                body.push(name_len);
                body.extend_from_slice(name.as_bytes());
                body.extend_from_slice(&mac);
                events.push(ClientEvent::Send {
                    bytes: encode_record(Kind::PairRequest, &body),
                });
                Ok(State::AwaitPairAccept { t, dh })
            }
            Intent::Connect { credentials } => {
                let ak = auth_key(&credentials.device_key).map_err(|_| ClientError::Auth)?;
                let mac = connect_client_mac(&ak, &credentials.device_id, &t)
                    .map_err(|_| ClientError::Auth)?;
                let mut body = Vec::with_capacity(16 + NONCE_LEN + PUBLIC_LEN + MAC_LEN);
                body.extend_from_slice(&credentials.device_id.0);
                body.extend_from_slice(&t.nc);
                body.extend_from_slice(&t.er);
                body.extend_from_slice(&mac);
                events.push(ClientEvent::Send {
                    bytes: encode_record(Kind::ConnectRequest, &body),
                });
                Ok(State::AwaitConnectAccept { t, dh })
            }
        }
    }

    fn pair_accept(
        &mut self,
        t: &Transcript,
        dh: &[u8; 32],
        payload: &[u8],
        events: &mut Vec<ClientEvent>,
    ) -> Result<State, ClientError> {
        let Intent::Pair { secret, name } = &self.intent else {
            return Err(ClientError::Auth);
        };
        let (id, mac) = payload.split_at_checked(16).ok_or(ClientError::Auth)?;
        if mac.len() != MAC_LEN {
            return Err(ClientError::Auth);
        }
        let device_id = DeviceId(id.try_into().map_err(|_| ClientError::Auth)?);
        let pk = pair_key(secret).map_err(|_| ClientError::Auth)?;
        verify_pair_phone_mac(&pk, t, name, &device_id, mac).map_err(|_| ClientError::Auth)?;
        let device_key =
            derive_device_key(secret, dh, t, &device_id, name).map_err(|_| ClientError::Auth)?;
        let mut channel = Channel::new(Side::Remote, &device_key, dh, device_id, t)
            .map_err(|_| ClientError::Auth)?;
        let credentials = ClientCredentials {
            device_id,
            device_key,
        };
        self.credentials = Some(credentials.clone());
        events.push(ClientEvent::Paired { credentials });
        events.push(ClientEvent::Connected);
        // Confirms the pairing to the phone (see the module docs).
        let hello = ClientMessage::Hello.to_bytes()?;
        let body = channel.seal(&hello).map_err(|_| ClientError::Auth)?;
        events.push(ClientEvent::Send {
            bytes: encode_record(Kind::Data, &body),
        });
        Ok(State::Established(Box::new(channel)))
    }

    fn connect_accept(
        &mut self,
        t: &Transcript,
        dh: &[u8; 32],
        payload: &[u8],
        events: &mut Vec<ClientEvent>,
    ) -> Result<State, ClientError> {
        let Intent::Connect { credentials } = &self.intent else {
            return Err(ClientError::Auth);
        };
        let ak = auth_key(&credentials.device_key).map_err(|_| ClientError::Auth)?;
        verify_connect_phone_mac(&ak, &credentials.device_id, t, payload)
            .map_err(|_| ClientError::Auth)?;
        let channel = Channel::new(
            Side::Remote,
            &credentials.device_key,
            dh,
            credentials.device_id,
            t,
        )
        .map_err(|_| ClientError::Auth)?;
        self.credentials = Some(credentials.clone());
        events.push(ClientEvent::Connected);
        Ok(State::Established(Box::new(channel)))
    }

    /// One Live view message from the phone.
    fn on_viewer(
        &mut self,
        channel: &mut Channel,
        msg: ViewerMessage,
        out: &mut Received,
    ) -> Result<(), ClientError> {
        let (part, parts) = match &msg {
            ViewerMessage::ViewerSnapshot { part, parts, .. } => (Some(*part), *parts),
            ViewerMessage::ViewerEnded { reason } => {
                self.viewer.watching = false;
                out.viewer.push(ViewerEvent::Ended { reason: *reason });
                return Ok(());
            }
            ViewerMessage::ViewerDenied { reason } => {
                self.viewer.watching = false;
                out.viewer.push(ViewerEvent::Denied { reason: *reason });
                return Ok(());
            }
            ViewerMessage::ViewerDelta { .. } => (None, 0),
        };
        if !self.viewer.watching || self.viewer.incompatible {
            // Late frames after a stop: ignored.
            return Ok(());
        }
        let bytes = match msg.payload() {
            Some(Ok(bytes)) => bytes,
            _ => return self.viewer_gap(channel, out),
        };
        let header = match peek(&bytes) {
            Ok(h) => h,
            Err(DeltaError::Version(version)) => {
                self.viewer.incompatible = true;
                self.viewer.watching = false;
                let stop = ViewerRequest::StopWatching.to_bytes()?;
                let body = channel.seal(&stop).map_err(|_| ClientError::Auth)?;
                out.events.push(ClientEvent::Send {
                    bytes: encode_record(Kind::Data, &body),
                });
                out.viewer.push(ViewerEvent::Incompatible { version });
                return Ok(());
            }
            Err(_) => return self.viewer_gap(channel, out),
        };
        match self.viewer.cursor.admit(&header) {
            Admit::Apply => {
                if header.class == DeltaClass::Reset {
                    self.viewer.resync_pending = false;
                }
                out.viewer.push(match part {
                    Some(part) => ViewerEvent::Snapshot { bytes, part, parts },
                    None => ViewerEvent::Delta { bytes },
                });
                Ok(())
            }
            Admit::Skip => Ok(()),
            Admit::Resync(_) => self.viewer_gap(channel, out),
        }
    }

    /// The stream broke: ask for a snapshot, unless one is already on its way.
    fn viewer_gap(&mut self, channel: &mut Channel, out: &mut Received) -> Result<(), ClientError> {
        self.viewer.cursor.break_stream();
        if self.viewer.resync_pending {
            return Ok(());
        }
        self.viewer.resync_pending = true;
        let req = ViewerRequest::ViewerResync.to_bytes()?;
        let body = channel.seal(&req).map_err(|_| ClientError::Auth)?;
        out.events.push(ClientEvent::Send {
            bytes: encode_record(Kind::Data, &body),
        });
        out.viewer.push(ViewerEvent::ResyncRequested);
        Ok(())
    }

    /// Encodes and seals a Live view request, and follows it: `start_watching` expects a
    /// snapshot, `stop_watching` ignores the stream from now on.
    pub fn send_viewer(&mut self, req: &ViewerRequest) -> Result<Vec<u8>, ClientError> {
        let bytes = self.send_raw(&req.to_bytes()?)?;
        match req {
            ViewerRequest::StartWatching => {
                self.viewer = ViewerStream {
                    watching: true,
                    resync_pending: true,
                    ..ViewerStream::default()
                };
            }
            ViewerRequest::StopWatching => self.viewer.watching = false,
            ViewerRequest::ViewerResync => {
                self.viewer.cursor.break_stream();
                self.viewer.resync_pending = true;
            }
        }
        Ok(bytes)
    }

    /// Asks for a snapshot after the controller's view reported a resync
    /// (`jarvis_viewer::Applied::NeedsResync`). `None` when not watching or when one is
    /// already on its way (at most one request per gap).
    pub fn request_resync(&mut self) -> Result<Option<Vec<u8>>, ClientError> {
        if !self.viewer.watching || self.viewer.resync_pending || self.viewer.incompatible {
            return Ok(None);
        }
        self.send_viewer(&ViewerRequest::ViewerResync).map(Some)
    }

    /// Whether this client follows the Live view stream.
    pub fn is_watching(&self) -> bool {
        self.viewer.watching
    }

    /// Encodes and seals a command. Write the returned bytes to the phone.
    pub fn send(&mut self, msg: &ClientMessage) -> Result<Vec<u8>, ClientError> {
        let plaintext = msg.to_bytes()?;
        self.send_raw(&plaintext)
    }

    /// Seals arbitrary bytes as a data frame. Lets tests and diagnostics send what a
    /// [`ClientMessage`] cannot express (unknown types, malformed JSON).
    pub fn send_raw(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, ClientError> {
        let State::Established(channel) = &mut self.state else {
            return Err(ClientError::NotConnected);
        };
        let body = channel.seal(plaintext).map_err(|_| ClientError::Auth)?;
        Ok(encode_record(Kind::Data, &body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewer::ViewerEndReason;

    /// A client established on a channel, and the phone's end of it.
    fn pair_of_channels() -> (RemoteClient, Channel) {
        let key = Secret32::from_bytes([7; 32]);
        let dh = [9u8; 32];
        let id = DeviceId([3; 16]);
        let t = Transcript {
            np: [1; NONCE_LEN],
            nc: [2; NONCE_LEN],
            ep: [4; PUBLIC_LEN],
            er: [5; PUBLIC_LEN],
        };
        let phone = Channel::new(Side::Phone, &key, &dh, id, &t).unwrap();
        let remote = Channel::new(Side::Remote, &key, &dh, id, &t).unwrap();
        let client = RemoteClient {
            eph: None,
            intent: Intent::Connect {
                credentials: ClientCredentials {
                    device_id: id,
                    device_key: key,
                },
            },
            state: State::Established(Box::new(remote)),
            reader: RecordReader::new(),
            credentials: None,
            viewer: ViewerStream::default(),
        };
        (client, phone)
    }

    fn frame(phone: &mut Channel, msg: &ViewerMessage) -> Vec<u8> {
        let body = phone.seal(&msg.to_bytes().unwrap()).unwrap();
        encode_record(Kind::Data, &body)
    }

    fn delta(phone: &mut Channel, json: &str) -> Vec<u8> {
        frame(phone, &ViewerMessage::delta(json.as_bytes()).unwrap())
    }

    fn activity(seq: u64) -> String {
        format!(r#"{{"gen":1,"seq":{seq},"kind":"activity","activity":{{"kind":"thinking"}}}}"#)
    }

    const RESET: &str = r#"{"gen":1,"seq":1,"kind":"reset","task_id":"t"}"#;

    fn requests(r: &Received) -> usize {
        r.events
            .iter()
            .filter(|e| matches!(e, ClientEvent::Send { .. }))
            .count()
    }

    #[test]
    fn a_gap_asks_for_one_snapshot_and_no_more() {
        let (mut client, mut phone) = pair_of_channels();
        // Not watching: the stream is ignored.
        let r = client.receive_all(&delta(&mut phone, RESET)).unwrap();
        assert!(r.viewer.is_empty() && r.events.is_empty());
        client.send_viewer(&ViewerRequest::StartWatching).unwrap();
        // The snapshot is on its way: a gap before it asks for nothing more.
        let r = client
            .receive_all(&delta(&mut phone, &activity(5)))
            .unwrap();
        assert_eq!(requests(&r), 0);
        let r = client.receive_all(&delta(&mut phone, RESET)).unwrap();
        assert!(matches!(r.viewer[..], [ViewerEvent::Delta { .. }]));
        let r = client
            .receive_all(&delta(&mut phone, &activity(2)))
            .unwrap();
        assert_eq!(r.viewer.len(), 1);
        // A gap followed by a flood of deltas and garbage: one request.
        let mut flood = delta(&mut phone, &activity(4));
        for seq in 5..40 {
            flood.extend(delta(&mut phone, &activity(seq)));
        }
        let bad = ViewerMessage::ViewerDelta {
            data: "!!!!".into(),
        };
        flood.extend(frame(&mut phone, &bad));
        let r = client.receive_all(&flood).unwrap();
        assert_eq!(requests(&r), 1);
        assert_eq!(r.viewer, vec![ViewerEvent::ResyncRequested]);
        assert_eq!(client.request_resync().unwrap(), None);
        // The snapshot arrives: the stream flows again; a new gap asks again, once.
        let snap =
            ViewerMessage::snapshot(br#"{"gen":1,"seq":39,"kind":"reset","task_id":"t"}"#, 0, 1)
                .unwrap();
        let r = client.receive_all(&frame(&mut phone, &snap)).unwrap();
        assert!(matches!(
            r.viewer[..],
            [ViewerEvent::Snapshot {
                part: 0,
                parts: 1,
                ..
            }]
        ));
        let r = client
            .receive_all(&delta(&mut phone, &activity(40)))
            .unwrap();
        assert!(matches!(r.viewer[..], [ViewerEvent::Delta { .. }]));
        let mut flood = delta(&mut phone, &activity(42));
        flood.extend(delta(&mut phone, &activity(43)));
        let r = client.receive_all(&flood).unwrap();
        assert_eq!(requests(&r), 1);
        // A newer format: stop watching, once.
        let snap =
            ViewerMessage::snapshot(br#"{"v":9,"gen":2,"seq":1,"kind":"reset"}"#, 0, 1).unwrap();
        let r = client.receive_all(&frame(&mut phone, &snap)).unwrap();
        assert_eq!(r.viewer, vec![ViewerEvent::Incompatible { version: 9 }]);
        assert_eq!(requests(&r), 1, "the stop request");
        assert!(!client.is_watching());
        let ended = ViewerMessage::ViewerEnded {
            reason: ViewerEndReason::Stopped,
        };
        let r = client.receive_all(&frame(&mut phone, &ended)).unwrap();
        assert_eq!(
            r.viewer,
            vec![ViewerEvent::Ended {
                reason: ViewerEndReason::Stopped
            }]
        );
    }

    #[test]
    fn the_view_can_ask_for_a_resync_once() {
        let (mut client, mut phone) = pair_of_channels();
        assert_eq!(client.request_resync().unwrap(), None, "not watching");
        client.send_viewer(&ViewerRequest::StartWatching).unwrap();
        client.receive_all(&delta(&mut phone, RESET)).unwrap();
        assert!(client.request_resync().unwrap().is_some());
        assert_eq!(client.request_resync().unwrap(), None);
    }
}
