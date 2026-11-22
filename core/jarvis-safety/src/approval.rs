//! What an owner's approval was given on: the content of the whole screen, so the agent can
//! tell, on a fresh look, whether the screen still shows what the owner approved.
//!
//! An earlier design compared a heuristic *neighbourhood* of the target (its row, the texts
//! just above it, the amounts and the classifier's signals). Three reviews in a row found
//! screens where what mattered sat outside it (a selected plan, a recipient header far
//! above "Pay", amounts that swapped roles) and screens where a harmless live text inside
//! it (an ETA) voided every approval. The rule is now the whole screen, compared exactly
//! but for one normalisation of digits in texts that carry no amount.

use jarvis_proto::{Action, CompactScreen, Element, Rect, Role};

use crate::app::is_springboard;
use crate::classify::{tap_point, type_target};
use crate::evaluate::same_row;
use crate::text::{fold_fullwidth, has_money};

/// Elements whose frame ends at or above this many points from the top of the screen are
/// status-bar-like (a clock, a battery percentage): their digits may change.
pub const STATUS_BAR_HEIGHT: f64 = 54.0;

/// One element of the screen content record. Frames are not part of it (content may
/// scroll); secure values are never kept.
#[derive(Clone, PartialEq)]
struct Fact {
    index: u32,
    role: Role,
    label: String,
    value: Option<String>,
    selected: bool,
    enabled: bool,
    secure: bool,
    /// Whether its texts compare exactly even without an amount (see [`compares_exactly`]).
    exact: bool,
}

/// The **screen content record** an approval was given on, captured from the screen the
/// question was asked about and compared with the same record of the fresh screen before
/// the approved action is sent.
///
/// It holds the app, the keyboard's Return-key label (the screen cue the classifier reads)
/// and, for every element of the compact screen in order: role, label, value (never a
/// secure field's), selected/checked state, enabled flag and secure flag. Frames are not
/// part of it, so a list that only scrolled keeps the approval (the agent re-plans the
/// coordinates on the fresh screen); the target is located on the fresh screen by its
/// position in the record, so a twin cannot take its place.
///
/// The fresh record must **equal** this one after a single normalisation: in a text that
/// contains **no money amount**, every run of digits compares equal to any other ("Pickup
/// in 3 min" = "Pickup in 2 min", "9:58" = "9:41", "45%" = "46%"). That tolerance applies
/// only to free-standing static texts and status-bar-like elements (and, on the home
/// screen, to the badge part after an icon's first comma: "Mail, 3 unread"); everything
/// else compares exactly, digits included: the target itself, every element in the
/// target's row band, selected or focused elements, text inputs and adjustable controls
/// (a stepper's or slider's value), and any text that contains an amount. Element count
/// and order, roles, states and every non-digit character compare exactly. (On the home
/// screen a row of icons is not one item: only the target itself compares exactly there.)
///
/// Trade-off: a non-money quantity in a free-standing static text away from the target
/// ("2 tickets" → "9 tickets" in a summary line above the button) may change without
/// voiding the approval. A quantity shown in the target's row, in a stepper or a text
/// field, or next to an amount, does void it.
///
/// `Debug` shows counts only: the facts are screen text.
#[derive(Clone, PartialEq)]
pub struct ApprovalContext {
    app: String,
    return_key: Option<String>,
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
    /// the fresh `screen` (empty when the approval still holds): "app", "return key",
    /// "element count", "element N" (its role, state or text; N is the element number) and
    /// "target". The descriptions never quote screen text.
    pub fn differences(&self, action: &Action, screen: &CompactScreen) -> Vec<String> {
        let now = record(action, screen);
        let mut out = Vec::new();
        if !self.app.trim().eq_ignore_ascii_case(now.app.trim()) {
            out.push("app".to_owned());
        }
        if self.return_key != now.return_key {
            out.push("return key".to_owned());
        }
        if self.facts.len() != now.facts.len() {
            out.push("element count".to_owned());
        }
        for (a, b) in self.facts.iter().zip(&now.facts) {
            if !same_fact(a, b, self.springboard) {
                out.push(format!("element {}", a.index));
            }
        }
        if self.target != now.target {
            out.push("target".to_owned());
        }
        out
    }

    /// Whether `action` on the fresh `screen` still acts on exactly what was approved.
    /// Linear in the number of elements.
    pub fn holds_on(&self, action: &Action, screen: &CompactScreen) -> bool {
        let now = record(action, screen);
        self.app.trim().eq_ignore_ascii_case(now.app.trim())
            && self.return_key == now.return_key
            && self.target == now.target
            && self.facts.len() == now.facts.len()
            && self
                .facts
                .iter()
                .zip(&now.facts)
                .all(|(a, b)| same_fact(a, b, self.springboard))
    }
}

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

/// Whether `role` is a control whose value is set by the owner: text inputs and adjustable
/// controls (sliders, switches, pickers, steppers).
fn holds_a_setting(role: &Role) -> bool {
    match role {
        Role::Slider | Role::Switch | Role::Picker => true,
        Role::Other(name) => {
            let n = name.to_lowercase();
            [
                "stepper",
                "slider",
                "switch",
                "toggle",
                "picker",
                "segmented",
            ]
            .iter()
            .any(|k| n.contains(k))
        }
        r => r.is_text_input(),
    }
}

/// Whether `el`'s texts compare exactly (digits included) even when they show no amount:
/// it is the target or in the target's row band, selected or focused, a text input or an
/// adjustable control, or anything but a free-standing static text or a status-bar-like
/// element.
fn compares_exactly(el: &Element, target: Option<&Rect>, springboard: bool) -> bool {
    if el.selected || el.focused || holds_a_setting(&el.role) {
        return true;
    }
    // The target's row band (on the home screen a row of icons is independent apps, so
    // only the target itself, compared exactly by `record`).
    if let Some(t) = target.filter(|_| !springboard) {
        if (*t == el.frame || same_row(t, &el.frame)) && !el.frame.is_empty() {
            return true;
        }
    }
    let status_bar = !el.frame.is_empty() && el.frame.max_y() <= STATUS_BAR_HEIGHT;
    // Home-screen icons: the badge after the first comma is status (see `same_text`).
    !(el.role == Role::StaticText || status_bar || springboard)
}

/// The screen content record of `screen` for `action`.
fn record(action: &Action, screen: &CompactScreen) -> ApprovalContext {
    let target = target_of(action, screen);
    let target_frame = target.map(|t| t.frame);
    let springboard = is_springboard(&screen.app_bundle_id);
    let facts = screen
        .elements
        .iter()
        .map(|e| Fact {
            index: e.index,
            role: e.role.clone(),
            label: e.label.clone(),
            value: if e.secure { None } else { e.value.clone() },
            selected: e.selected,
            enabled: e.enabled,
            secure: e.secure,
            exact: target.is_some_and(|t| t.index == e.index)
                || compares_exactly(e, target_frame.as_ref(), springboard),
        })
        .collect();
    ApprovalContext {
        app: screen.app_bundle_id.clone(),
        return_key: screen.cues.return_key.clone(),
        springboard,
        facts,
        target: target.map(|t| t.index),
    }
}

/// `text` with every run of digits replaced by one `#`.
fn digits_folded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_digits = false;
    for c in text.chars() {
        let f = fold_fullwidth(c);
        if f.is_numeric() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out
}

/// Whether two texts are the same under the record's normalisation: exactly, or, when
/// `tolerant` and neither carries a money amount, with digit runs folded. On the home
/// screen a tolerant text keeps its part before the first comma (the app's name) exact.
fn same_text(a: &str, b: &str, tolerant: bool, springboard_icon: bool) -> bool {
    if a == b {
        return true;
    }
    if !tolerant || has_money(a) || has_money(b) {
        return false;
    }
    if springboard_icon {
        let (ha, ta) = a.split_once(',').unwrap_or((a, ""));
        let (hb, tb) = b.split_once(',').unwrap_or((b, ""));
        return ha == hb && digits_folded(ta) == digits_folded(tb);
    }
    digits_folded(a) == digits_folded(b)
}

/// Whether `a` (asked) and `b` (fresh) are the same element of the record.
fn same_fact(a: &Fact, b: &Fact, springboard: bool) -> bool {
    if a.role != b.role
        || a.selected != b.selected
        || a.enabled != b.enabled
        || a.secure != b.secure
        || a.value.is_some() != b.value.is_some()
    {
        return false;
    }
    if a.label == b.label && a.value == b.value {
        return true;
    }
    let tolerant = !a.exact && !b.exact;
    let icon = springboard && a.role != Role::StaticText;
    // An amount anywhere in the element (label and value read together) makes both exact.
    let joined = |f: &Fact| match &f.value {
        Some(v) => format!("{} {v}", f.label),
        None => f.label.clone(),
    };
    let tolerant = tolerant && !has_money(&joined(a)) && !has_money(&joined(b));
    same_text(&a.label, &b.label, tolerant, icon)
        && match (&a.value, &b.value) {
            (Some(x), Some(y)) => same_text(x, y, tolerant, false),
            _ => true,
        }
}

/// The [`ApprovalContext`] (screen content record) of `action` on `screen`, captured when
/// the owner is asked.
pub fn approval_context(action: &Action, screen: &CompactScreen) -> ApprovalContext {
    record(action, screen)
}
