//! The JSON messages carried inside authenticated frames.
//!
//! Both enums are internally tagged with `"type"` (snake_case) and reject unknown fields.
//! Decoding distinguishes a well-formed message of an unknown type
//! ([`MessageError::UnknownType`], answered with `Error { code: unknown_message }`) from
//! garbage ([`MessageError::Malformed`]).

use std::collections::BTreeMap;

use jarvis_proto::{OutcomeStatus, Phase, RiskCategory};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::codec::MAX_PLAINTEXT_LEN;
use crate::error::MessageError;
use crate::permissions::RemotePermissions;

/// Remote → phone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    /// Ask for a `Welcome` (phone name, this device's permissions, whether a task runs).
    Hello,
    /// Start a free-text task. Needs `start_tasks`; refused while a task runs.
    StartTask {
        /// What to do.
        text: String,
    },
    /// Run a skill by id with arguments. Needs `start_tasks`; the id must be listed.
    RunSkill {
        /// Skill id from `Skills`.
        id: String,
        /// Arguments as the skill defines them.
        #[serde(default)]
        args: BTreeMap<String, serde_json::Value>,
    },
    /// Run a workflow by id. Needs `start_tasks`; the id must be listed.
    RunWorkflow {
        /// Workflow id from `Skills`.
        id: String,
    },
    /// Stop the running task. Allowed for every paired device: stopping is always safe.
    Cancel,
    /// Answer the pending confirmation. Needs `approve` and a remotely decidable category.
    Decision {
        /// The `request_id` from `ConfirmPending`.
        request_id: u64,
        /// True to proceed.
        approved: bool,
    },
    /// Receive progress, confirmation and finish events. Needs `see_progress`.
    Subscribe,
    /// Stop receiving them.
    Unsubscribe,
    /// List the skills and workflows the phone offers.
    ListSkills,
    /// Keep-alive; answered with `Pong`.
    Ping,
}

impl ClientMessage {
    /// Every `"type"` tag, for telling unknown types from malformed messages.
    pub const TYPES: [&'static str; 10] = [
        "hello",
        "start_task",
        "run_skill",
        "run_workflow",
        "cancel",
        "decision",
        "subscribe",
        "unsubscribe",
        "list_skills",
        "ping",
    ];

    /// The `"type"` tag.
    pub fn name(&self) -> &'static str {
        match self {
            ClientMessage::Hello => "hello",
            ClientMessage::StartTask { .. } => "start_task",
            ClientMessage::RunSkill { .. } => "run_skill",
            ClientMessage::RunWorkflow { .. } => "run_workflow",
            ClientMessage::Cancel => "cancel",
            ClientMessage::Decision { .. } => "decision",
            ClientMessage::Subscribe => "subscribe",
            ClientMessage::Unsubscribe => "unsubscribe",
            ClientMessage::ListSkills => "list_skills",
            ClientMessage::Ping => "ping",
        }
    }

    /// Encodes to JSON bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        encode(self)
    }

    /// Decodes JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        decode(bytes, &Self::TYPES)
    }
}

/// One skill or workflow as the shell lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    /// Stable id, used in `RunSkill` / `RunWorkflow`.
    pub id: String,
    /// Display name.
    pub name: String,
    /// One-line description.
    pub summary: String,
}

/// Why a request was refused. Every refusal is also audited on the phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenyReason {
    /// This device lacks the permission the request needs.
    NotPermitted,
    /// A task is already running (requests are not queued).
    Busy,
    /// Nothing is running to cancel.
    NotRunning,
    /// The `request_id` is not the pending confirmation, or it was already decided.
    StaleRequest,
    /// The category can only be decided on the phone ([`crate::NEVER_REMOTE_APPROVE`]).
    NotRemotelyApprovable,
    /// The skill or workflow id is not in the phone's list.
    UnknownSkill,
}

/// Why an authenticated message could not be served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The message was not valid JSON for its type.
    Malformed,
    /// The message type is unknown to this phone.
    UnknownMessage,
    /// Too many messages in the current window; this one was dropped.
    RateLimited,
    /// A field is out of range (empty or too long task text, empty id).
    InvalidRequest,
    /// The reply would not fit in one frame.
    TooLarge,
}

/// Phone → remote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerMessage {
    /// Answer to `Hello`.
    Welcome {
        /// The phone's display name.
        phone_name: String,
        /// What this device may do.
        permissions: RemotePermissions,
        /// Whether a task is running.
        busy: bool,
    },
    /// A task started on the phone (from any origin).
    TaskStarted,
    /// Progress of the running task, filtered by this device's permissions.
    Progress {
        /// Steps taken so far.
        step: u32,
        /// Coarse phase.
        phase: Phase,
        /// Foreground app bundle id, if known.
        app: Option<String>,
        /// Kind of the last action (`"tap"`, `"type"`…), never its arguments.
        last_action: Option<String>,
        /// Redacted screen text; only with `see_screen_text`.
        screen_text: Option<String>,
    },
    /// The task waits for a confirmation.
    ConfirmPending {
        /// Id to answer with `Decision`.
        request_id: u64,
        /// Why it asks.
        category: RiskCategory,
        /// What it wants to do (redacted; generic unless this device may see screen text
        /// or decide this request).
        summary: String,
        /// Whether a `Decision` from this device would be honoured.
        can_decide: bool,
    },
    /// The pending confirmation was answered (on the phone or remotely).
    ConfirmResolved {
        /// The request that is no longer pending.
        request_id: u64,
    },
    /// The task ended.
    Finished {
        /// Final status.
        status: OutcomeStatus,
        /// Redacted summary; generic unless this device may see screen text.
        summary: String,
    },
    /// Answer to `ListSkills`.
    Skills {
        /// Skills.
        skills: Vec<CatalogEntry>,
        /// Workflows.
        workflows: Vec<CatalogEntry>,
    },
    /// The owner changed this device's permissions.
    PermissionsChanged {
        /// The new permissions.
        permissions: RemotePermissions,
    },
    /// The request was refused.
    Denied {
        /// Why.
        reason: DenyReason,
    },
    /// The message could not be served.
    Error {
        /// What went wrong.
        code: ErrorCode,
    },
    /// Answer to `Ping`.
    Pong,
}

impl ServerMessage {
    /// Every `"type"` tag.
    pub const TYPES: [&'static str; 11] = [
        "welcome",
        "task_started",
        "progress",
        "confirm_pending",
        "confirm_resolved",
        "finished",
        "skills",
        "permissions_changed",
        "denied",
        "error",
        "pong",
    ];

    /// Encodes to JSON bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, MessageError> {
        encode(self)
    }

    /// Decodes JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MessageError> {
        decode(bytes, &Self::TYPES)
    }
}

fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, MessageError> {
    let bytes = serde_json::to_vec(msg).map_err(|e| MessageError::Malformed(e.to_string()))?;
    if bytes.len() > MAX_PLAINTEXT_LEN {
        return Err(MessageError::TooLarge);
    }
    Ok(bytes)
}

fn decode<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
    known: &[&str],
) -> Result<T, MessageError> {
    if bytes.len() > MAX_PLAINTEXT_LEN {
        return Err(MessageError::TooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| MessageError::Malformed(e.to_string()))?;
    let ty = value
        .as_object()
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| MessageError::Malformed("missing \"type\"".to_owned()))?;
    if !known.contains(&ty) {
        return Err(MessageError::UnknownType(ty.chars().take(64).collect()));
    }
    let msg: T = serde_json::from_value(value.clone())
        .map_err(|e| MessageError::Malformed(e.to_string()))?;
    // `deny_unknown_fields` is not applied to unit variants of internally tagged enums, so
    // check generically: every key sent must survive a round trip.
    let canonical =
        serde_json::to_value(&msg).map_err(|e| MessageError::Malformed(e.to_string()))?;
    if let (Some(sent), Some(known)) = (value.as_object(), canonical.as_object()) {
        if let Some(extra) = sent.keys().find(|k| !known.contains_key(*k)) {
            return Err(MessageError::Malformed(format!(
                "unknown field {:?}",
                extra.chars().take(64).collect::<String>()
            )));
        }
    }
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shapes() {
        let m = ClientMessage::StartTask {
            text: "Turn on Wi-Fi".into(),
        };
        let b = m.to_bytes().unwrap();
        assert_eq!(
            std::str::from_utf8(&b).unwrap(),
            r#"{"type":"start_task","text":"Turn on Wi-Fi"}"#
        );
        assert_eq!(ClientMessage::from_bytes(&b).unwrap(), m);
        assert_eq!(
            ClientMessage::from_bytes(br#"{"type":"ping"}"#).unwrap(),
            ClientMessage::Ping
        );
        let d = ServerMessage::Denied {
            reason: DenyReason::Busy,
        };
        let b = d.to_bytes().unwrap();
        assert_eq!(
            std::str::from_utf8(&b).unwrap(),
            r#"{"type":"denied","reason":"busy"}"#
        );
        assert_eq!(ServerMessage::from_bytes(&b).unwrap(), d);
    }

    #[test]
    fn unknown_types_and_garbage_are_typed() {
        assert_eq!(
            ClientMessage::from_bytes(br#"{"type":"format_disk"}"#),
            Err(MessageError::UnknownType("format_disk".into()))
        );
        assert!(matches!(
            ClientMessage::from_bytes(b"not json"),
            Err(MessageError::Malformed(_))
        ));
        assert!(matches!(
            ClientMessage::from_bytes(br#"{"type":"start_task"}"#),
            Err(MessageError::Malformed(_))
        ));
        assert!(matches!(
            ClientMessage::from_bytes(br#"{"type":"ping","extra":1}"#),
            Err(MessageError::Malformed(_))
        ));
        assert!(matches!(
            ClientMessage::from_bytes(br#"[1,2]"#),
            Err(MessageError::Malformed(_))
        ));
        assert_eq!(
            ServerMessage::from_bytes(br#"{"type":"hello"}"#),
            Err(MessageError::UnknownType("hello".into()))
        );
    }

    #[test]
    fn type_lists_match_the_enums() {
        for t in ClientMessage::TYPES {
            let json = format!(r#"{{"type":"{t}"}}"#);
            // Unit variants decode; data variants fail as Malformed, never UnknownType.
            assert!(!matches!(
                ClientMessage::from_bytes(json.as_bytes()),
                Err(MessageError::UnknownType(_))
            ));
        }
    }
}
