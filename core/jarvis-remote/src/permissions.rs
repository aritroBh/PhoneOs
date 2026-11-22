//! Per-device permissions and the categories no remote can ever approve.

use jarvis_proto::RiskCategory;
use serde::{Deserialize, Serialize};

/// Risk categories a remote device can never approve, whatever its `approve` toggle says.
///
/// These are the confirmations whose mistakes are expensive or hard to undo (money,
/// credentials, account takeover, data loss). They must be answered on the phone itself,
/// where the owner sees the screen and the OS's own prompts. A remote can still decline
/// them indirectly by sending `Cancel`.
pub const NEVER_REMOTE_APPROVE: [RiskCategory; 5] = [
    RiskCategory::Payment,
    RiskCategory::Purchase,
    RiskCategory::Credential,
    RiskCategory::AccountChange,
    RiskCategory::Delete,
];

/// True when `category` may be decided remotely at all (it is not in
/// [`NEVER_REMOTE_APPROVE`]).
pub fn remotely_decidable(category: RiskCategory) -> bool {
    !NEVER_REMOTE_APPROVE.contains(&category)
}

/// What one paired device may do. Checked on every request, so a change applies to the
/// next message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemotePermissions {
    /// Start tasks, skills and workflows.
    pub start_tasks: bool,
    /// Subscribe to progress, confirmation and finish events.
    pub see_progress: bool,
    /// Receive free text from the phone: the screen text in progress events, the full
    /// confirmation summary and the finish summary. Always passed through redaction.
    pub see_screen_text: bool,
    /// Answer confirmation prompts whose category is not in [`NEVER_REMOTE_APPROVE`].
    pub approve: bool,
}

impl Default for RemotePermissions {
    fn default() -> Self {
        Self {
            start_tasks: true,
            see_progress: true,
            see_screen_text: false,
            approve: false,
        }
    }
}

impl RemotePermissions {
    /// True when this device may decide a confirmation of `category`.
    pub fn can_decide(&self, category: RiskCategory) -> bool {
        self.approve && remotely_decidable(category)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cannot_approve_anything() {
        let p = RemotePermissions::default();
        assert!(RiskCategory::ALL.iter().all(|c| !p.can_decide(*c)));
    }

    #[test]
    fn approve_toggle_never_reaches_forbidden_categories() {
        let p = RemotePermissions {
            approve: true,
            ..RemotePermissions::default()
        };
        for c in RiskCategory::ALL {
            assert_eq!(p.can_decide(c), !NEVER_REMOTE_APPROVE.contains(&c), "{c:?}");
        }
        assert!(p.can_decide(RiskCategory::SendMessage));
        assert!(!p.can_decide(RiskCategory::Payment));
    }
}
