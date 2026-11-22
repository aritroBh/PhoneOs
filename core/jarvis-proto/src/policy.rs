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
    /// The target cannot be classified: its label is unreadable (a script or language the
    /// phrase tables do not cover), an unknown symbol name, or missing; or a tool call has no
    /// declared spec. What it does is unknown, so it is treated as possibly consequential.
    Unverified,
    /// Placing a phone or FaceTime call.
    Call,
    /// The first time the agent acts inside an app in this session (see
    /// [`Policy::first_use_approval`]). Never produced by classification.
    FirstUse,
}

impl RiskCategory {
    /// Every category, in declaration order.
    pub const ALL: [RiskCategory; 12] = [
        RiskCategory::None,
        RiskCategory::SendMessage,
        RiskCategory::Payment,
        RiskCategory::Purchase,
        RiskCategory::Delete,
        RiskCategory::PublicPost,
        RiskCategory::AccountChange,
        RiskCategory::SystemSetting,
        RiskCategory::Credential,
        RiskCategory::Unverified,
        RiskCategory::Call,
        RiskCategory::FirstUse,
    ];
}

/// Bundle id of Apple's Passwords app (iOS 18+). Confirmed against Apple's
/// "Bundle IDs for native iPhone and iPad apps" deployment guide.
pub const PASSWORDS_BUNDLE_ID: &str = "com.apple.Passwords";
/// Bundle id of Apple Wallet (historically Passbook). Confirmed against the same guide.
pub const WALLET_BUNDLE_ID: &str = "com.apple.Passbook";

/// Bundle id of the Settings app.
pub const SETTINGS_BUNDLE_ID: &str = "com.apple.Preferences";
/// Bundle id of SpringBoard, the home screen (and lock screen, Control Center host).
pub const SPRINGBOARD_BUNDLE_ID: &str = "com.apple.springboard";

/// How closely the owner supervises actions in apps without their own [`AppRule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyMode {
    /// Confirm risky categories (`confirm`) only.
    #[default]
    Guarded,
    /// Confirm every action that can change state (everything except waiting, looking,
    /// asking, finishing and plain vertical scrolling).
    StepByStep,
}

/// How much an app is trusted, set per app with an [`AppRule`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// No first-use approval; lower-stakes categories run without asking. Payment,
    /// Purchase, Delete, Credential and AccountChange still follow `confirm`.
    Trusted,
    /// Like [`PolicyMode::Guarded`].
    Guarded,
    /// Like [`PolicyMode::StepByStep`].
    StepByStep,
    /// Never operated, exactly like an entry of `denied_apps`.
    Denied,
}

/// A per-app trust level. The rule with the longest matching `bundle_id_prefix` wins.
///
/// A prefix matches a bundle id case-insensitively when the id equals it or continues it
/// at a `.` (`com.example` matches `com.example.shop`, not `com.examples`); a trailing `.`
/// or `*` on the prefix is ignored.
///
/// `name` is the display name the owner sees for the app (its home-screen icon,
/// app-switcher card, Settings row). Optional and absent from older JSON. With a name, a
/// `Denied` rule also refuses a home-screen icon, app-switcher card or `open_app` whose
/// name equals it (case, whitespace and punctuation folded); for other levels the name lets
/// the agent recognise the app's icon when the rule's prefix is a full bundle id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    /// Bundle id or bundle id prefix.
    pub bundle_id_prefix: String,
    /// Trust level for matching apps.
    pub level: TrustLevel,
    /// The app's display name, when the shell knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl AppRule {
    /// Creates a rule without a display name.
    pub fn new(bundle_id_prefix: impl Into<String>, level: TrustLevel) -> Self {
        Self {
            bundle_id_prefix: bundle_id_prefix.into(),
            level,
            name: None,
        }
    }

    /// The rule with the app's display name.
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// User-adjustable limits and gates for a session.
///
/// `denied_apps` entries are matched by `jarvis-safety` exactly or as a prefix. They always
/// win over `app_rules`; a `Denied` rule behaves like a `denied_apps` entry. Apps without a
/// rule use `mode`.
///
/// JSON written before `mode`/`app_rules`/`first_use_approval` existed (detected by a
/// missing `first_use_approval` key) is migrated so it behaves as it did: an explicit
/// `confirm` list containing `payment` gains `unverified` (unreadable targets used to be
/// classified `payment`), one containing `send_message` gains `call` (calls used to be
/// classified `send_message`), and `first_use` is added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PolicyWire")]
pub struct Policy {
    /// Bundle ids the agent must never operate.
    pub denied_apps: Vec<String>,
    /// Categories that need user confirmation before running.
    pub confirm: Vec<RiskCategory>,
    /// Categories that are never executed. `None` is ignored here; `FirstUse` here means
    /// the agent may only act in apps approved earlier (or trusted).
    pub deny_categories: Vec<RiskCategory>,
    /// Supervision for apps without an `AppRule`.
    pub mode: PolicyMode,
    /// Per-app trust levels (longest prefix wins).
    pub app_rules: Vec<AppRule>,
    /// Ask before the first state-changing action inside an app that is not yet approved
    /// (remembered in memory) and not `Trusted`. SpringBoard and the agent's own app are
    /// exempt.
    pub first_use_approval: bool,
    /// Maximum agent steps per task.
    pub max_steps: u32,
    /// Maximum wall time per task, in milliseconds.
    pub max_duration_ms: u64,
    /// Consecutive failed actions before giving up.
    pub max_consecutive_failures: u32,
    /// Re-inference attempts after unparsable model output.
    pub max_parse_retries: u32,
    /// Longest the session waits for the owner to unlock a locked phone, in milliseconds,
    /// before it finishes `Interrupted`. Time spent waiting does not count against
    /// `max_duration_ms`. Absent in older JSON, which reads as the default (30 minutes).
    pub max_locked_wait_ms: u64,
}

/// Default for [`Policy::max_locked_wait_ms`]: 30 minutes.
pub const DEFAULT_MAX_LOCKED_WAIT_MS: u64 = 30 * 60 * 1000;

impl Default for Policy {
    /// Conservative: confirm every risky category (the three new ones included), deny
    /// Passwords and Wallet, no denied categories, `Guarded` mode, no app rules, first-use
    /// approval on, 40 steps, 10 minutes, 4 consecutive failures, 2 parse retries, and at
    /// most 30 minutes waiting for the owner to unlock the phone.
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
            deny_categories: Vec::new(),
            mode: PolicyMode::Guarded,
            app_rules: Vec::new(),
            first_use_approval: true,
            max_steps: 40,
            max_duration_ms: 600_000,
            max_consecutive_failures: 4,
            max_parse_retries: 2,
            max_locked_wait_ms: DEFAULT_MAX_LOCKED_WAIT_MS,
        }
    }
}

impl Policy {
    /// Whether `category` is in the confirm list.
    pub fn requires_confirmation(&self, category: RiskCategory) -> bool {
        self.confirm.contains(&category)
    }

    /// Whether `category` is in the deny list (`None` never is).
    pub fn denies_category(&self, category: RiskCategory) -> bool {
        category != RiskCategory::None && self.deny_categories.contains(&category)
    }
}

/// The JSON shape of [`Policy`], with the migration hook (`first_use_approval` optional).
#[derive(Deserialize)]
#[serde(default)]
struct PolicyWire {
    denied_apps: Vec<String>,
    confirm: Option<Vec<RiskCategory>>,
    deny_categories: Vec<RiskCategory>,
    mode: PolicyMode,
    app_rules: Vec<AppRule>,
    first_use_approval: Option<bool>,
    max_steps: u32,
    max_duration_ms: u64,
    max_consecutive_failures: u32,
    max_parse_retries: u32,
    max_locked_wait_ms: u64,
}

impl Default for PolicyWire {
    fn default() -> Self {
        let p = Policy::default();
        Self {
            denied_apps: p.denied_apps,
            confirm: None,
            deny_categories: p.deny_categories,
            mode: p.mode,
            app_rules: p.app_rules,
            first_use_approval: None,
            max_steps: p.max_steps,
            max_duration_ms: p.max_duration_ms,
            max_consecutive_failures: p.max_consecutive_failures,
            max_parse_retries: p.max_parse_retries,
            max_locked_wait_ms: p.max_locked_wait_ms,
        }
    }
}

impl From<PolicyWire> for Policy {
    fn from(w: PolicyWire) -> Self {
        let legacy = w.first_use_approval.is_none();
        let confirm = match w.confirm {
            None => Policy::default().confirm,
            Some(mut c) => {
                if legacy {
                    let adds = [
                        (RiskCategory::Unverified, c.contains(&RiskCategory::Payment)),
                        (RiskCategory::Call, c.contains(&RiskCategory::SendMessage)),
                        (RiskCategory::FirstUse, true),
                    ];
                    for (cat, when) in adds {
                        if when && !c.contains(&cat) {
                            c.push(cat);
                        }
                    }
                }
                c
            }
        };
        Policy {
            denied_apps: w.denied_apps,
            confirm,
            deny_categories: w.deny_categories,
            mode: w.mode,
            app_rules: w.app_rules,
            first_use_approval: w.first_use_approval.unwrap_or(true),
            max_steps: w.max_steps,
            max_duration_ms: w.max_duration_ms,
            max_consecutive_failures: w.max_consecutive_failures,
            max_parse_retries: w.max_parse_retries,
            max_locked_wait_ms: w.max_locked_wait_ms,
        }
    }
}
