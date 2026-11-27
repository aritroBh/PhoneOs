//! What an owner's approval was given on, as far as the compact screen shows it.
//!
//! The agent binds an approval to four records (see `jarvis-agent`): the **exact digest of
//! the raw accessibility tree** (`jarvis_observe::screen_digest`, A1), this module's
//! **compact-screen record** (A2: every listed element in order with its role, texts and
//! states, compared **exactly**, frames only ignored, plus the target's position in the
//! list), the target's raw node (A3) and the full summary with its category (A4).
//! [`ApprovalContext::holds_on`] is the A2 check; [`ApprovalContext::change_note`] tells
//! the model and the owner what changed.
//!
//! Earlier designs tolerated digit changes in free-standing texts (so a live ETA or a
//! clock could tick). A fourth review showed that tolerance let a list shift by one card
//! (`Order #1001` → `#1002` under "Delete order"), a phone number, an account suffix, a
//! booking time or a points amount change under a standing approval. It is gone: a screen
//! with live-updating text cannot be confirmed (fail closed).

use jarvis_proto::{Action, CompactScreen, Element, Role};

use crate::app::is_springboard;
use crate::classify::{tap_point, type_target};

/// One element of the compact-screen record. Frames are not part of it (content may
/// scroll); secure values are never kept.
#[derive(Clone, PartialEq)]
struct Fact {
    index: u32,
    role: Role,
    label: String,
    value: Option<String>,
    placeholder: Option<String>,
    selected: bool,
    enabled: bool,
    secure: bool,
}

/// The compact-screen record an approval was given on, captured from the screen the
/// question was asked about.
///
/// It holds the app, the keyboard's Return-key label (the cue the classifier reads; since
/// the sixth review the keys are hashed in the raw digest too, so a changed or appearing
/// Return key voids any approval, not only a submit's), the approved action's target (its
/// position in the list) and, for every element of the compact screen in order: role, label, value (never a secure field's), placeholder,
/// selected/checked state, enabled flag and secure flag. Frames and focus are not part of
/// it, so a list that only moved keeps it.
///
/// [`ApprovalContext::holds_on`] requires the fresh record to **equal** it exactly; the
/// one exception is the home screen, where an app icon's badge (role `Icon`: the part of its
/// label after the first comma, `Mail, 3 unread`) may change on every icon but the target,
/// exactly as the digest leaves it out.
///
/// `Debug` shows counts only: the facts are screen text.
#[derive(Clone, PartialEq)]
pub struct ApprovalContext {
    app: String,
    /// The Return key's label and type (audit C1: the type by identifier is a fact too).
    return_key: (Option<String>, Option<String>),
    springboard: bool,
    facts: Vec<Fact>,
    target: Option<u32>,
}

impl std::fmt::Debug for ApprovalContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalContext")
            .field("elements", &self.facts.len())
            .field("has_target", &self.target.is_some())
            .finish_non_exhaustive()
    }
}

impl ApprovalContext {
    /// What differs between `self` (captured when the question was asked) and `action` on
    /// the fresh `screen` (empty when the record holds): "app", "return key", "element
    /// count", "element N" (its role, state or text; N is the element number) and
    /// "target". The descriptions never quote screen text.
    pub fn differences(&self, action: &Action, screen: &CompactScreen) -> Vec<String> {
        let now = record(action, screen);
        let mut out = Vec::new();
        if !self.same_app(&now) {
            out.push("app".to_owned());
        }
        if self.return_key != now.return_key {
            out.push("return key".to_owned());
        }
        if self.facts.len() != now.facts.len() {
            out.push("element count".to_owned());
        }
        for (a, b) in self.facts.iter().zip(&now.facts) {
            if !self.same_fact(a, b) {
                out.push(format!("element {}", a.index));
            }
        }
        if self.target != now.target {
            out.push("target".to_owned());
        }
        out
    }

    /// Whether `action` on the fresh `screen` acts on exactly what was approved: the whole
    /// compact record is equal (see the type docs). Linear in the number of elements.
    pub fn holds_on(&self, action: &Action, screen: &CompactScreen) -> bool {
        let now = record(action, screen);
        self.same_app(&now)
            && self.return_key == now.return_key
            && self.target == now.target
            && self.facts.len() == now.facts.len()
            && self
                .facts
                .iter()
                .zip(&now.facts)
                .all(|(a, b)| self.same_fact(a, b))
    }

    /// The target part of the record on the fresh `screen`: the same app and Return-key
    /// cue, the target at the same position in the list, with the same role, label, value,
    /// placeholder and states (an action without a target: none on either). The agent
    /// uses this beside the exact digest of the raw tree, which covers everything else.
    pub fn target_holds_on(&self, action: &Action, screen: &CompactScreen) -> bool {
        let now = record(action, screen);
        if !self.same_app(&now) || self.return_key != now.return_key || self.target != now.target {
            return false;
        }
        let Some(t) = self.target else {
            return true;
        };
        let fact = |r: &ApprovalContext| r.facts.iter().find(|f| f.index == t).cloned();
        match (fact(self), fact(&now)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// A short human-readable note of the first difference, for the model and the owner
    /// (screen text cleaned like a summary's: redacted, sanitised, quotes neutralised,
    /// bounded): `the text "Order #1001" is now "Order #1002"`, `an element appeared or
    /// disappeared (9 → 10)`, `another app is in front`. `None` when the compact records
    /// are equal (only something the compact screen does not show changed). Build both
    /// records from prompt-safe screens.
    pub fn change_note(&self, action: &Action, screen: &CompactScreen) -> Option<String> {
        let now = record(action, screen);
        if !self.same_app(&now) {
            return Some("another app is in front".to_owned());
        }
        for (a, b) in self.facts.iter().zip(&now.facts) {
            if self.same_fact(a, b) {
                continue;
            }
            let role = crate::evaluate::role_words(&a.role);
            // A field's value is quoted only where a summary could show it without knowing
            // what Jarvis typed (an amount in an amount-like field); other fields are named
            // (sixth review).
            let text = |f: &Fact| {
                let t = match &f.value {
                    Some(v)
                        if !v.trim().is_empty()
                            && crate::evaluate::value_quotable(
                                &f.role,
                                &f.label,
                                f.placeholder.as_deref(),
                                v,
                            ) =>
                    {
                        format!("{} {v}", f.label)
                    }
                    Some(v) if !v.trim().is_empty() => format!("{} (filled)", f.label),
                    _ => f.label.clone(),
                };
                format!("\"{}\"", crate::evaluate::clean_quote(&t, NOTE_TEXT_CHARS))
            };
            if a.role != b.role {
                return Some(format!(
                    "the {role} {} is now a {} {}",
                    text(a),
                    crate::evaluate::role_words(&b.role),
                    text(b)
                ));
            }
            if text(a) != text(b) {
                return Some(format!("the {role} {} is now {}", text(a), text(b)));
            }
            let state = if a.selected != b.selected {
                if b.selected {
                    "is now selected"
                } else {
                    "is no longer selected"
                }
            } else if a.enabled != b.enabled {
                if b.enabled {
                    "is now enabled"
                } else {
                    "is now disabled"
                }
            } else {
                "changed"
            };
            return Some(format!("the {role} {} {state}", text(a)));
        }
        if self.facts.len() != now.facts.len() {
            return Some(format!(
                "an element appeared or disappeared ({} → {})",
                self.facts.len(),
                now.facts.len()
            ));
        }
        if self.return_key != now.return_key {
            return Some("the keyboard's Return key changed".to_owned());
        }
        if self.target != now.target {
            return Some("the approved control is no longer where it was".to_owned());
        }
        None
    }

    fn same_app(&self, now: &ApprovalContext) -> bool {
        self.app.trim().eq_ignore_ascii_case(now.app.trim())
    }

    /// Whether `a` (asked) and `b` (fresh) are the same element: exactly, but on the home
    /// screen an icon other than the target keeps only its name (before the first comma).
    fn same_fact(&self, a: &Fact, b: &Fact) -> bool {
        if a == b {
            return true;
        }
        let badge_only = self.springboard
            && is_icon(&a.role)
            && Some(a.index) != self.target
            && a.index == b.index;
        if !badge_only {
            return false;
        }
        let name = |l: &str| l.split_once(',').map_or(l, |(n, _)| n).to_owned();
        Fact {
            label: name(&a.label),
            ..a.clone()
        } == Fact {
            label: name(&b.label),
            ..b.clone()
        }
    }
}

/// Whether `role` is a SpringBoard app icon (`Icon`, any letter case).
fn is_icon(role: &Role) -> bool {
    matches!(role, Role::Other(name) if name.trim().eq_ignore_ascii_case("icon"))
}

/// Longest screen text quoted in a [`ApprovalContext::change_note`], in characters.
const NOTE_TEXT_CHARS: usize = 60;

/// The element `action` acts on, as the classifier reads it: its element; for `type`
/// without one, the focused text input; for `tap_point`, the element that decides its
/// risk.
fn target_of<'a>(action: &Action, screen: &'a CompactScreen) -> Option<&'a Element> {
    match action {
        Action::Type { element, .. } => type_target(*element, screen),
        Action::TapPoint { x, y } => tap_point(*x, *y, screen).1,
        other => other.element().and_then(|i| screen.get(i)),
    }
}

/// The compact-screen record of `screen` for `action`.
fn record(action: &Action, screen: &CompactScreen) -> ApprovalContext {
    let target = target_of(action, screen);
    let facts = screen
        .elements
        .iter()
        .map(|e| {
            let secure = e.secure || e.role == Role::SecureTextField;
            Fact {
                index: e.index,
                role: e.role.clone(),
                label: e.label.clone(),
                value: if secure { None } else { e.value.clone() },
                placeholder: e.placeholder.clone(),
                selected: e.selected,
                enabled: e.enabled,
                secure,
            }
        })
        .collect();
    ApprovalContext {
        app: screen.app_bundle_id.clone(),
        return_key: (
            screen.cues.return_key.clone(),
            screen.cues.return_key_type.clone(),
        ),
        springboard: is_springboard(&screen.app_bundle_id),
        facts,
        target: target.map(|t| t.index),
    }
}

/// The [`ApprovalContext`] (compact-screen record) of `action` on `screen`, captured when
/// the owner is asked.
pub fn approval_context(action: &Action, screen: &CompactScreen) -> ApprovalContext {
    record(action, screen)
}
