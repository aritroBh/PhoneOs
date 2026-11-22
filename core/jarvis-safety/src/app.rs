//! The per-app deny list.

use jarvis_proto::{Policy, PASSWORDS_BUNDLE_ID, WALLET_BUNDLE_ID};

use crate::text::{fold_char, is_invisible, tokens};

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

/// Case- and look-alike-folded (`Passwοrds` with a Greek omicron reads `passwords`),
/// invisible characters removed, any whitespace (NBSP included) collapsed to one space,
/// trimmed, and trailing punctuation and `.app` suffixes removed.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().filter(|c| !is_invisible(*c)) {
        let c = fold_char(c);
        if c.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    loop {
        let before = out.len();
        let trimmed = out
            .trim_end_matches(|c: char| c.is_whitespace() || (!c.is_alphanumeric() && c != '*'))
            .to_owned();
        out = trimmed
            .strip_suffix(".app")
            .map_or(trimmed.clone(), str::to_owned);
        if out.len() == before {
            break;
        }
    }
    out
}

/// Whether the words of `needle` appear contiguously among the words of `hay`, or `hay`'s
/// words run together equal `needle` (`ExampleBank` for `examplebank`).
fn has_words(hay: &[String], needle: &str) -> bool {
    let n = tokens(needle);
    if n.is_empty() {
        return false;
    }
    hay.windows(n.len()).any(|w| w == n.as_slice()) || hay.concat() == n.concat()
}

/// Checks `app` (a bundle id, or a display name from `OpenApp`) against
/// `policy.denied_apps`.
///
/// Matching is case-insensitive. A bundle id is denied when it equals an entry or starts
/// with it (`com.examplebank` denies `com.examplebank.mobile`; a trailing `*` on an entry
/// is ignored). A display name (no `.` once a trailing `.app` and punctuation are
/// dropped) is denied when its words contain the last dot-separated component of an entry
/// (`the Passwords app` for `com.apple.Passwords`) or a known alias of a denied built-in
/// app (`Wallet & Apple Pay` for `com.apple.Passbook`). Both sides are folded for case,
/// look-alike letters and whitespace first. Blank entries are ignored.
/// Callers that resolve display names to bundle ids should check the resolved id too.
pub fn check_app(policy: &Policy, app: &str) -> Verdict {
    let target = normalize(app);
    if target.is_empty() {
        return Verdict::Allow;
    }
    let is_name = !target.contains('.');
    let words = tokens(&target);
    for entry in &policy.denied_apps {
        let rule = normalize(entry.trim().trim_end_matches('*'));
        let rule = rule.as_str();
        if rule.is_empty() {
            continue;
        }
        let by_id = target.starts_with(rule);
        let by_name = is_name
            && (rule
                .rsplit('.')
                .next()
                .is_some_and(|last| has_words(&words, last))
                || NAME_ALIASES
                    .iter()
                    .any(|(name, id)| normalize(id) == rule && has_words(&words, name)));
        if by_id || by_name {
            return Verdict::Deny(entry.clone());
        }
    }
    Verdict::Allow
}
