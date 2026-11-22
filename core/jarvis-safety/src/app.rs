//! The per-app deny list.

use jarvis_proto::{Policy, PASSWORDS_BUNDLE_ID, WALLET_BUNDLE_ID};

use crate::text::{fold_fullwidth, is_invisible};

/// Whether the agent may operate an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The app is not on the deny list.
    Allow,
    /// The app matched this deny-list entry.
    Deny(String),
}

impl Verdict {
    /// True for [`Verdict::Deny`].
    pub fn is_denied(&self) -> bool {
        matches!(self, Verdict::Deny(_))
    }
}

/// Display names that resolve to a built-in denied bundle id, for `OpenApp` targets given
/// by name.
const NAME_ALIASES: &[(&str, &str)] = &[
    ("wallet", WALLET_BUNDLE_ID),
    ("apple wallet", WALLET_BUNDLE_ID),
    ("passbook", WALLET_BUNDLE_ID),
    ("passwords", PASSWORDS_BUNDLE_ID),
    ("apple passwords", PASSWORDS_BUNDLE_ID),
];

/// Lowercase, fullwidth-folded, with invisible characters removed and whitespace trimmed.
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !is_invisible(*c))
        .map(fold_fullwidth)
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Checks `app` (a bundle id, or a display name from `OpenApp`) against
/// `policy.denied_apps`.
///
/// Matching is case-insensitive. A bundle id is denied when it equals an entry or starts
/// with it (`com.examplebank` denies `com.examplebank.mobile`; a trailing `*` on an entry
/// is ignored). A display name (no `.`) is denied when it equals the last dot-separated
/// component of an entry (`Passwords` for `com.apple.Passwords`) or is a known alias of a
/// denied built-in app (`Wallet` for `com.apple.Passbook`). Blank entries are ignored.
/// Callers that resolve display names to bundle ids should check the resolved id too.
pub fn check_app(policy: &Policy, app: &str) -> Verdict {
    let target = normalize(app);
    if target.is_empty() {
        return Verdict::Allow;
    }
    let is_name = !target.contains('.');
    for entry in &policy.denied_apps {
        let rule = normalize(entry);
        let rule = rule.trim_end_matches('*');
        if rule.is_empty() {
            continue;
        }
        let by_id = target.starts_with(rule);
        let by_name = is_name
            && (rule.rsplit('.').next() == Some(target.as_str())
                || NAME_ALIASES
                    .iter()
                    .any(|(name, id)| *name == target && id.eq_ignore_ascii_case(rule)));
        if by_id || by_name {
            return Verdict::Deny(entry.clone());
        }
    }
    Verdict::Allow
}
