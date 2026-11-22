//! The remote device's side: [`RemoteClient`], also sans-IO.
//!
//! One `RemoteClient` is one connection. Create it with [`RemoteClient::pair`] (first
//! contact, with the code from the phone) or [`RemoteClient::connect`] (with stored
//! [`ClientCredentials`]), feed it every byte from the phone with
//! [`RemoteClient::receive`], write out every [`ClientEvent::Send`], and once
//! [`ClientEvent::Connected`] arrives, encode commands with [`RemoteClient::send`].

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::codec::{
    encode_record, from_hex, to_hex, DeviceId, Kind, Record, RecordReader, PROTOCOL_VERSION,
};
use crate::crypto::{
    auth_key, connect_client_mac, derive_device_key, pair_client_mac, pair_key,
    verify_connect_phone_mac, verify_pair_phone_mac, Channel, PairingSecret, Secret32, Side,
    MAC_LEN, NONCE_LEN,
};
use crate::error::{ClientError, MessageError, PersistError};
use crate::message::{ClientMessage, ServerMessage};
use crate::pairing::parse_code;
use crate::store::clean_name;

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
    device_key: String,
}

impl ClientCredentials {
    /// Serializes the credentials, key included.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&CredentialsJson {
            schema_version: 1,
            device_id: self.device_id,
            device_key: to_hex(self.device_key.as_bytes()),
        })
        .unwrap_or_default()
    }

    /// Loads [`ClientCredentials::to_json`] output.
    pub fn from_json(json: &str) -> Result<Self, PersistError> {
        let doc: CredentialsJson =
            serde_json::from_str(json).map_err(|e| PersistError::Corrupt(e.to_string()))?;
        if doc.schema_version != 1 {
            return Err(PersistError::UnsupportedVersion(doc.schema_version));
        }
        let key: [u8; 32] = from_hex(&doc.device_key)
            .and_then(|b| b.try_into().ok())
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

enum State {
    AwaitHello,
    AwaitPairAccept { np: [u8; NONCE_LEN] },
    AwaitConnectAccept { np: [u8; NONCE_LEN] },
    Established(Box<Channel>),
    Failed,
}

enum Intent {
    Pair { secret: PairingSecret, name: String },
    Connect { credentials: ClientCredentials },
}

/// One connection from a remote device to the phone.
pub struct RemoteClient {
    nc: [u8; NONCE_LEN],
    intent: Intent,
    state: State,
    reader: RecordReader,
    credentials: Option<ClientCredentials>,
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
    /// `entropy` must come from a CSPRNG; it becomes this connection's nonce.
    pub fn pair(code: &str, device_name: &str, entropy: [u8; 32]) -> Result<Self, ClientError> {
        let secret = parse_code(code)?;
        let name = clean_name(device_name);
        if name.is_empty() {
            return Err(ClientError::InvalidName);
        }
        Ok(Self {
            nc: entropy,
            intent: Intent::Pair { secret, name },
            state: State::AwaitHello,
            reader: RecordReader::new(),
            credentials: None,
        })
    }

    /// Reconnect with stored credentials. `entropy` must come from a CSPRNG.
    pub fn connect(credentials: ClientCredentials, entropy: [u8; 32]) -> Self {
        Self {
            nc: entropy,
            intent: Intent::Connect { credentials },
            state: State::AwaitHello,
            reader: RecordReader::new(),
            credentials: None,
        }
    }

    /// Whether commands can be sent.
    pub fn is_connected(&self) -> bool {
        matches!(self.state, State::Established(_))
    }

    /// The credentials this connection authenticated with (after pairing or connecting).
    pub fn credentials(&self) -> Option<&ClientCredentials> {
        self.credentials.as_ref()
    }

    /// Feeds bytes from the phone. On `Err` the connection is unusable: close it.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Vec<ClientEvent>, ClientError> {
        if matches!(self.state, State::Failed) {
            return Err(ClientError::Auth);
        }
        let result = self.receive_inner(bytes);
        if result.is_err() {
            self.state = State::Failed;
        }
        result
    }

    fn receive_inner(&mut self, bytes: &[u8]) -> Result<Vec<ClientEvent>, ClientError> {
        let mut events = Vec::new();
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
                self.handle(record, &mut events)?;
            }
        }
        Ok(events)
    }

    fn handle(&mut self, record: Record, events: &mut Vec<ClientEvent>) -> Result<(), ClientError> {
        if record.version != PROTOCOL_VERSION {
            return Err(ClientError::Auth);
        }
        let kind = Kind::from_byte(record.kind).ok_or(ClientError::Auth)?;
        let state = std::mem::replace(&mut self.state, State::Failed);
        self.state = match (state, kind) {
            (State::AwaitHello, Kind::ServerHello) => {
                let np: [u8; NONCE_LEN] =
                    record.payload.try_into().map_err(|_| ClientError::Auth)?;
                self.hello(np, events)?
            }
            (State::AwaitPairAccept { np }, Kind::PairAccept) => {
                self.pair_accept(np, &record.payload, events)?
            }
            (State::AwaitConnectAccept { np }, Kind::ConnectAccept) => {
                self.connect_accept(np, &record.payload, events)?
            }
            (State::Established(mut channel), Kind::Data) => {
                let pt = channel
                    .open(&record.payload)
                    .map_err(|_| ClientError::Auth)?;
                events.push(match ServerMessage::from_bytes(&pt) {
                    Ok(m) => ClientEvent::Message(m),
                    Err(e) => ClientEvent::Undecodable(e),
                });
                State::Established(channel)
            }
            _ => return Err(ClientError::Auth),
        };
        Ok(())
    }

    fn hello(
        &mut self,
        np: [u8; NONCE_LEN],
        events: &mut Vec<ClientEvent>,
    ) -> Result<State, ClientError> {
        let nc = self.nc;
        match &self.intent {
            Intent::Pair { secret, name } => {
                let pk = pair_key(secret).map_err(|_| ClientError::Auth)?;
                let mac = pair_client_mac(&pk, &np, &nc, name).map_err(|_| ClientError::Auth)?;
                let name_len = u8::try_from(name.len()).map_err(|_| ClientError::InvalidName)?;
                let mut body = Vec::with_capacity(NONCE_LEN + 1 + name.len() + MAC_LEN);
                body.extend_from_slice(&nc);
                body.push(name_len);
                body.extend_from_slice(name.as_bytes());
                body.extend_from_slice(&mac);
                events.push(ClientEvent::Send {
                    bytes: encode_record(Kind::PairRequest, &body),
                });
                Ok(State::AwaitPairAccept { np })
            }
            Intent::Connect { credentials } => {
                let ak = auth_key(&credentials.device_key).map_err(|_| ClientError::Auth)?;
                let mac = connect_client_mac(&ak, &credentials.device_id, &np, &nc)
                    .map_err(|_| ClientError::Auth)?;
                let mut body = Vec::with_capacity(16 + NONCE_LEN + MAC_LEN);
                body.extend_from_slice(&credentials.device_id.0);
                body.extend_from_slice(&nc);
                body.extend_from_slice(&mac);
                events.push(ClientEvent::Send {
                    bytes: encode_record(Kind::ConnectRequest, &body),
                });
                Ok(State::AwaitConnectAccept { np })
            }
        }
    }

    fn pair_accept(
        &mut self,
        np: [u8; NONCE_LEN],
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
        let nc = self.nc;
        let pk = pair_key(secret).map_err(|_| ClientError::Auth)?;
        verify_pair_phone_mac(&pk, &np, &nc, name, &device_id, mac)
            .map_err(|_| ClientError::Auth)?;
        let device_key =
            derive_device_key(secret, &np, &nc, &device_id, name).map_err(|_| ClientError::Auth)?;
        let channel = Channel::new(Side::Remote, &device_key, device_id, &np, &nc)
            .map_err(|_| ClientError::Auth)?;
        let credentials = ClientCredentials {
            device_id,
            device_key,
        };
        self.credentials = Some(credentials.clone());
        events.push(ClientEvent::Paired { credentials });
        events.push(ClientEvent::Connected);
        Ok(State::Established(Box::new(channel)))
    }

    fn connect_accept(
        &mut self,
        np: [u8; NONCE_LEN],
        payload: &[u8],
        events: &mut Vec<ClientEvent>,
    ) -> Result<State, ClientError> {
        let Intent::Connect { credentials } = &self.intent else {
            return Err(ClientError::Auth);
        };
        let nc = self.nc;
        let ak = auth_key(&credentials.device_key).map_err(|_| ClientError::Auth)?;
        verify_connect_phone_mac(&ak, &credentials.device_id, &np, &nc, payload)
            .map_err(|_| ClientError::Auth)?;
        let channel = Channel::new(
            Side::Remote,
            &credentials.device_key,
            credentials.device_id,
            &np,
            &nc,
        )
        .map_err(|_| ClientError::Auth)?;
        self.credentials = Some(credentials.clone());
        events.push(ClientEvent::Connected);
        Ok(State::Established(Box::new(channel)))
    }

    /// Encodes and seals a command. Write the returned bytes to the phone.
    pub fn send(&mut self, msg: &ClientMessage) -> Result<Vec<u8>, ClientError> {
        let State::Established(channel) = &mut self.state else {
            return Err(ClientError::NotConnected);
        };
        let plaintext = msg.to_bytes()?;
        let body = channel.seal(&plaintext).map_err(|_| ClientError::Auth)?;
        Ok(encode_record(Kind::Data, &body))
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
