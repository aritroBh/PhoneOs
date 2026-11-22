//! The per-app deny list and trust levels.

use jarvis_proto::{
    AppInfo, AppRule, Policy, PolicyMode, TrustLevel, PASSWORDS_BUNDLE_ID, SPRINGBOARD_BUNDLE_ID,
    WALLET_BUNDLE_ID,
};

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

/// Bundle-id components that say nothing about which app it is (`com.examplebank.mobile`):
/// the name heuristic skips them and reads the component before.
const GENERIC_ID_PARTS: &[&str] = &[
    "mobile",
    "mobileapp",
    "app",
    "apps",
    "ios",
    "iosapp",
    "iphone",
    "ipad",
    "client",
    "prod",
    "production",
    "release",
    "main",
    "universal",
    "appstore",
];

/// The component of a (normalised) bundle id that a display name is matched against: the
/// last one that is not a generic word (`com.apple.Passwords` → `passwords`,
/// `com.examplebank.mobile` → `examplebank`), else the last one.
fn name_part(id: &str) -> Option<&str> {
    let mut parts = id.rsplit('.');
    let last = parts.next()?;
    if !GENERIC_ID_PARTS.contains(&last) {
        return Some(last);
    }
    // Never the reverse-DNS head (`com` of `com.mobile`).
    let rest: Vec<&str> = parts.collect();
    rest.iter()
        .take(rest.len().saturating_sub(1))
        .find(|p| !p.is_empty() && !GENERIC_ID_PARTS.contains(p))
        .copied()
        .or(Some(last))
}

/// A display name folded for equality: case and look-alike letters folded, everything but
/// letters and digits (whitespace, punctuation, invisible characters) dropped. `Example
/// Bank`, `examplebank` and `Example-Bank!` are equal; an empty result matches nothing.
fn fold_name(s: &str) -> String {
    s.chars()
        .filter(|c| !is_invisible(*c))
        .map(fold_char)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Whether the home-screen label or app name `label` (or its first comma-separated part:
/// "ExampleBank, 2 notifications") names `name`.
fn names_app(label: &str, name: &str) -> bool {
    let want = fold_name(name);
    if want.is_empty() {
        return false;
    }
    let first = label.split(',').next().unwrap_or("");
    fold_name(label) == want || fold_name(first) == want
}

/// Checks a display name (a home-screen icon or app-switcher label, an `open_app` target)
/// against the deny list, knowing the apps' names: [`check_app`] first; then a name equal
/// (see `fold_name`) to the display name of a denied app is denied. The display names are
/// the `name` of `Denied` app rules and the `known_apps` entries whose bundle id
/// [`check_app`] denies (`com.examplebank.mobile` known as "ExampleBank").
pub fn check_app_name(policy: &Policy, known_apps: &[AppInfo], name: &str) -> Verdict {
    let v = check_app(policy, name);
    if v.is_denied() {
        return v;
    }
    for app in known_apps {
        if names_app(name, &app.name) {
            if let Verdict::Deny(entry) = check_app(policy, &app.bundle_id) {
                return Verdict::Deny(entry);
            }
        }
    }
    Verdict::Allow
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
/// `policy.denied_apps` and the `Denied` entries of `policy.app_rules`.
///
/// Matching is case-insensitive. A bundle id is denied when it equals an entry or starts
/// with it (`com.examplebank` denies `com.examplebank.mobile`; a trailing `*` on an entry
/// is ignored). A display name (no `.` once a trailing `.app` and punctuation are
/// dropped) is denied when its words contain the last meaningful dot-separated component of
/// an entry (`the Passwords app` for `com.apple.Passwords`; generic trailing parts such as
/// `mobile` or `app` are skipped, so `ExampleBank` for `com.examplebank.mobile`), a known
/// alias of a denied built-in app (`Wallet & Apple Pay` for `com.apple.Passbook`), or
/// equals the display `name` of a `Denied` app rule. Both sides are folded for case,
/// look-alike letters and whitespace first. Blank entries are ignored. To also match the
/// names in `known_apps`, use [`check_app_name`].
/// Callers that resolve display names to bundle ids should check the resolved id too.
///
/// A `Denied` app rule denies a bundle id when it is the longest rule matching it (see
/// [`jarvis_proto::AppRule`]), so a more specific rule can carve an app out of a denied
/// prefix; it denies a display name the same way a `denied_apps` entry does.
pub fn check_app(policy: &Policy, app: &str) -> Verdict {
    let target = normalize(app);
    if target.is_empty() {
        return Verdict::Allow;
    }
    let is_name = !target.contains('.');
    let words = tokens(&target);
    if let Some(v) = check_listed(policy, &target, is_name, &words) {
        return v;
    }
    if is_name {
        for rule in policy
            .app_rules
            .iter()
            .filter(|r| r.level == TrustLevel::Denied)
        {
            let prefix = normalize(rule_prefix(rule));
            let by_name = rule.name.as_deref().is_some_and(|n| names_app(app, n));
            if by_name || name_part(&prefix).is_some_and(|last| has_words(&words, last)) {
                return Verdict::Deny(rule.bundle_id_prefix.clone());
            }
        }
    } else if let Some(rule) = matching_rule(policy, app) {
        if rule.level == TrustLevel::Denied {
            return Verdict::Deny(rule.bundle_id_prefix.clone());
        }
    }
    Verdict::Allow
}

/// `rule.bundle_id_prefix` without surrounding whitespace and a trailing `*` or `.`.
fn rule_prefix(rule: &AppRule) -> &str {
    rule.bundle_id_prefix
        .trim()
        .trim_end_matches('*')
        .trim_end_matches('.')
}

/// Whether `prefix` matches `bundle_id` (case-insensitive, at a `.` boundary).
fn prefix_matches(prefix: &str, bundle_id: &str) -> bool {
    let id = bundle_id.trim();
    if prefix.is_empty() {
        return false;
    }
    let head_matches = id
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix));
    head_matches
        && id
            .get(prefix.len()..)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// The app rule with the longest prefix matching `bundle_id` (the first one on ties).
pub fn matching_rule<'p>(policy: &'p Policy, bundle_id: &str) -> Option<&'p AppRule> {
    let mut best: Option<(&AppRule, usize)> = None;
    for rule in &policy.app_rules {
        let prefix = rule_prefix(rule);
        if prefix_matches(prefix, bundle_id) && best.is_none_or(|(_, len)| prefix.len() > len) {
            best = Some((rule, prefix.len()));
        }
    }
    best.map(|(r, _)| r)
}

/// The trust level for acting in `bundle_id`: `Denied` when [`check_app`] denies it, else
/// the longest matching app rule's level, else the level of `policy.mode`.
pub fn app_level(policy: &Policy, bundle_id: &str) -> TrustLevel {
    if check_app(policy, bundle_id).is_denied() {
        return TrustLevel::Denied;
    }
    match matching_rule(policy, bundle_id) {
        Some(rule) => rule.level,
        None => mode_level(policy),
    }
}

/// The trust level `policy.mode` gives apps without a rule.
pub fn mode_level(policy: &Policy) -> TrustLevel {
    match policy.mode {
        PolicyMode::Guarded => TrustLevel::Guarded,
        PolicyMode::StepByStep => TrustLevel::StepByStep,
    }
}

/// Whether `bundle_id` is SpringBoard (the home screen), which never needs first-use
/// approval.
pub fn is_springboard(bundle_id: &str) -> bool {
    bundle_id.trim().eq_ignore_ascii_case(SPRINGBOARD_BUNDLE_ID)
}

/// The `denied_apps` part of [`check_app`].
fn check_listed(policy: &Policy, target: &str, is_name: bool, words: &[String]) -> Option<Verdict> {
    for entry in &policy.denied_apps {
        let rule = normalize(entry.trim().trim_end_matches('*'));
        let rule = rule.as_str();
        if rule.is_empty() {
            continue;
        }
        let by_id = target.starts_with(rule);
        let by_name = is_name
            && (name_part(rule).is_some_and(|last| has_words(words, last))
                || NAME_ALIASES
                    .iter()
                    .any(|(name, id)| normalize(id) == rule && has_words(words, name)));
        if by_id || by_name {
            return Some(Verdict::Deny(entry.clone()));
        }
    }
    None
}
