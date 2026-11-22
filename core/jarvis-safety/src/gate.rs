//! Confirmation gates.

use jarvis_proto::{Action, CompactScreen, Direction, Policy, RiskCategory, Role, TrustLevel};

/// What to do with an action of a given risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Run it.
    Allow,
    /// Ask the user first.
    Confirm,
    /// Never run it.
    Deny,
}

/// Categories a `Trusted` app may run without asking even when they are in
/// `policy.confirm`: changing a setting inside the app and tapping a control the phrase
/// net cannot read. Everything else — `Payment`, `Purchase`, `Delete`, `Credential`,
/// `AccountChange`, `SendMessage`, `PublicPost` and `Call` — still follows `confirm`.
pub const TRUSTED_ALLOWED: &[RiskCategory] =
    &[RiskCategory::SystemSetting, RiskCategory::Unverified];

/// The gate for `category` under `policy`: `None` is allowed, categories in
/// `policy.deny_categories` are denied, categories in `policy.confirm` need confirmation,
/// others are allowed.
pub fn gate(policy: &Policy, category: RiskCategory) -> Gate {
    if category == RiskCategory::None {
        Gate::Allow
    } else if policy.denies_category(category) {
        Gate::Deny
    } else if policy.requires_confirmation(category) {
        Gate::Confirm
    } else {
        Gate::Allow
    }
}

/// [`gate`] for an action of `category` in an app of trust `level`:
///
/// * `Denied`: [`Gate::Deny`].
/// * `StepByStep`: denied categories are denied; every action that [`changes_state`]
///   needs confirmation; the rest follows [`gate`].
/// * `Trusted`: denied categories are denied; [`TRUSTED_ALLOWED`] categories are
///   allowed; the rest follows [`gate`].
/// * `Guarded`: [`gate`].
pub fn gate_in(
    policy: &Policy,
    category: RiskCategory,
    level: TrustLevel,
    action: &Action,
    screen: &CompactScreen,
) -> Gate {
    match level {
        TrustLevel::Denied => Gate::Deny,
        TrustLevel::StepByStep => match gate(policy, category) {
            Gate::Deny => Gate::Deny,
            _ if changes_state(action, screen) => Gate::Confirm,
            other => other,
        },
        TrustLevel::Trusted => match gate(policy, category) {
            Gate::Confirm if TRUSTED_ALLOWED.contains(&category) => Gate::Allow,
            other => other,
        },
        TrustLevel::Guarded => gate(policy, category),
    }
}

/// Whether `action` can change state: everything except `Wait`, `Look`, `Done`, `Fail`,
/// `AskUser` and a plain vertical `Scroll` (one whose target, if any, is not a picker,
/// slider or switch, where dragging changes the value). `PressHome`, `OpenApp`, `OpenUrl`,
/// `CallTool`, focusing taps and sideways scrolls all count.
pub fn changes_state(action: &Action, screen: &CompactScreen) -> bool {
    match action {
        Action::Wait { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. }
        | Action::AskUser { .. } => false,
        Action::Scroll { direction, element } => {
            let vertical = matches!(direction, Direction::Up | Direction::Down);
            let value_control = element
                .and_then(|i| screen.get(i))
                .is_some_and(|e| matches!(e.role, Role::Picker | Role::Slider | Role::Switch));
            !vertical || value_control
        }
        _ => true,
    }
}
