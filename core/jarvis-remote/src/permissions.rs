//! Per-device permissions and the categories no remote can ever approve.

use jarvis_proto::RiskCategory;
use serde::{Deserialize, Serialize};

/// Risk categories a remote device can never approve, whatever its `approve` toggle says.
///
/// These are the confirmations whose mistakes are expensive or hard to undo (money,
/// credentials, account takeover, data loss), plus `Unverified`: a control the classifier
/// could not read may be a "Pay" in another language. They must be approved on the phone
/// itself, where the owner sees the screen and the OS's own prompts. A remote that was
/// shown the prompt can still decline it, and any remote can `Cancel`.
///
/// Thirteenth review: a request flagged `ConfirmRequest::phone_only` (a presumed submit,
/// whose control the gate could not read; a link the owner did not give) is not remotely
/// approvable either, whatever its category (the hub checks the flag beside this list).
///
/// Review pass 29 (M1): defined in the gate (`jarvis_safety::NEVER_REMOTE_APPROVE`), which
/// flags a request phone only when it carries one of them beside the category it shows.
pub const NEVER_REMOTE_APPROVE: [RiskCategory; 6] = jarvis_safety::NEVER_REMOTE_APPROVE;

/// True when `category` may be approved remotely at all (it is not in
/// [`NEVER_REMOTE_APPROVE`]).
pub fn remotely_approvable(category: RiskCategory) -> bool {
    !NEVER_REMOTE_APPROVE.contains(&category)
}

/// What one paired device may do. Checked on every request, so a change applies to the
/// next message.
///
/// Deserialisation is strict: unknown fields are an error and a missing field is `false`,
/// so a typo or an older writer can only ever grant *less*. [`Default`] is likewise
/// everything off. A freshly paired device gets [`RemotePermissions::NEW_DEVICE`] in code
/// (pairing is an explicit act by the owner on the phone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePermissions {
    /// Start tasks, skills and workflows, and list skills.
    #[serde(default)]
    pub start_tasks: bool,
    /// Subscribe to progress, confirmation and finish events.
    #[serde(default)]
    pub see_progress: bool,
    /// Receive free text from the phone: the screen text in progress events, the full
    /// confirmation summary and the finish summary. Always passed through redaction.
    #[serde(default)]
    pub see_screen_text: bool,
    /// Approve confirmation prompts whose category is not in [`NEVER_REMOTE_APPROVE`].
    #[serde(default)]
    pub approve: bool,
}

impl RemotePermissions {
    /// Everything off.
    pub const NONE: RemotePermissions = RemotePermissions {
        start_tasks: false,
        see_progress: false,
        see_screen_text: false,
        approve: false,
    };

    /// What a newly paired device gets: start tasks and see progress; no screen text, no
    /// approvals.
    pub const NEW_DEVICE: RemotePermissions = RemotePermissions {
        start_tasks: true,
        see_progress: true,
        see_screen_text: false,
        approve: false,
    };

    /// True when this device may approve a confirmation of `category`.
    pub fn can_approve(&self, category: RiskCategory) -> bool {
        self.approve && remotely_approvable(category)
    }

    /// True when every permission is off.
    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_devices_cannot_approve_anything() {
        let p = RemotePermissions::NEW_DEVICE;
        assert!(RiskCategory::ALL.iter().all(|c| !p.can_approve(*c)));
    }

    #[test]
    fn approve_toggle_never_reaches_forbidden_categories() {
        let p = RemotePermissions {
            approve: true,
            ..RemotePermissions::NEW_DEVICE
        };
        for c in RiskCategory::ALL {
            assert_eq!(
                p.can_approve(c),
                !NEVER_REMOTE_APPROVE.contains(&c),
                "{c:?}"
            );
        }
        assert!(p.can_approve(RiskCategory::SendMessage));
        assert!(p.can_approve(RiskCategory::FirstUse));
        assert!(!p.can_approve(RiskCategory::Payment));
        assert!(!p.can_approve(RiskCategory::Unverified));
    }

    #[test]
    fn serde_is_strict_and_fails_closed() {
        let p: RemotePermissions = serde_json::from_str("{}").unwrap();
        assert!(p.is_none());
        let p: RemotePermissions = serde_json::from_str(r#"{"start_tasks":true}"#).unwrap();
        assert_eq!(
            p,
            RemotePermissions {
                start_tasks: true,
                ..RemotePermissions::NONE
            }
        );
        assert!(serde_json::from_str::<RemotePermissions>(r#"{"startTasks":false}"#).is_err());
        assert_eq!(RemotePermissions::default(), RemotePermissions::NONE);
    }
}
