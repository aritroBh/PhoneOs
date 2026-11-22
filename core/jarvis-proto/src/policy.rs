//! Safety policy data.

use serde::{Deserialize, Serialize};

/// What could go wrong if an action ran unattended.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskCategory {
    /// Harmless.
    #[default]
    None,
    /// Sending a message or email.
    SendMessage,
    /// Moving money.
    Payment,
    /// Buying something.
    Purchase,
    /// Deleting data.
    Delete,
    /// Posting publicly.
    PublicPost,
    /// Changing account details (password, email, sign-out).
    AccountChange,
    /// Changing device settings.
    SystemSetting,
    /// Entering or revealing credentials.
    Credential,
}

impl RiskCategory {
    /// Every category, in declaration order.
    pub const ALL: [RiskCategory; 9] = [
        RiskCategory::None,
        RiskCategory::SendMessage,
        RiskCategory::Payment,
        RiskCategory::Purchase,
        RiskCategory::Delete,
        RiskCategory::PublicPost,
        RiskCategory::AccountChange,
        RiskCategory::SystemSetting,
        RiskCategory::Credential,
    ];
}

/// Bundle id of Apple's Passwords app (iOS 18+). Confirmed against Apple's
/// "Bundle IDs for native iPhone and iPad apps" deployment guide.
pub const PASSWORDS_BUNDLE_ID: &str = "com.apple.Passwords";
/// Bundle id of Apple Wallet (historically Passbook). Confirmed against the same guide.
pub const WALLET_BUNDLE_ID: &str = "com.apple.Passbook";

/// User-adjustable limits and gates for a session.
///
/// `denied_apps` entries are matched by `jarvis-safety` exactly or as a prefix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    /// Bundle ids the agent must never operate.
    pub denied_apps: Vec<String>,
    /// Categories that need user confirmation before running.
    pub confirm: Vec<RiskCategory>,
    /// Maximum agent steps per task.
    pub max_steps: u32,
    /// Maximum wall time per task, in milliseconds.
    pub max_duration_ms: u64,
    /// Consecutive failed actions before giving up.
    pub max_consecutive_failures: u32,
    /// Re-inference attempts after unparsable model output.
    pub max_parse_retries: u32,
}

impl Default for Policy {
    /// Conservative: confirm every risky category, deny Passwords and Wallet, 40 steps,
    /// 10 minutes, 4 consecutive failures, 2 parse retries.
    ///
    /// Banking apps are not listed because their bundle ids vary per bank; the shell adds
    /// them from the user's own app list.
    fn default() -> Self {
        Self {
            denied_apps: vec![PASSWORDS_BUNDLE_ID.to_owned(), WALLET_BUNDLE_ID.to_owned()],
            confirm: RiskCategory::ALL
                .iter()
                .copied()
                .filter(|c| *c != RiskCategory::None)
                .collect(),
            max_steps: 40,
            max_duration_ms: 600_000,
            max_consecutive_failures: 4,
            max_parse_retries: 2,
        }
    }
}

impl Policy {
    /// Whether `category` is in the confirm list.
    pub fn requires_confirmation(&self, category: RiskCategory) -> bool {
        self.confirm.contains(&category)
    }
}
