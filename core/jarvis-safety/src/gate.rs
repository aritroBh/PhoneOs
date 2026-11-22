//! Confirmation gates.

use jarvis_proto::{Policy, RiskCategory};

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

/// The gate for `category` under `policy`: `None` is allowed, categories in
/// `policy.confirm` need confirmation, others are allowed.
///
/// `Policy` has no per-category deny list, so this never returns [`Gate::Deny`] today;
/// hard denials (deny-listed apps, invalid actions, unknown tools) come from `evaluate`.
pub fn gate(policy: &Policy, category: RiskCategory) -> Gate {
    if category == RiskCategory::None {
        Gate::Allow
    } else if policy.requires_confirmation(category) {
        Gate::Confirm
    } else {
        Gate::Allow
    }
}
