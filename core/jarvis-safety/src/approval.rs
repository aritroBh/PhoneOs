//! The facts an owner's approval was given on, so the agent can tell, on a fresh look,
//! whether the action is still the one approved.

use jarvis_proto::{Action, CompactScreen, Element, Role};

use crate::classify::{signal_texts, tap_point, type_target};
use crate::evaluate::{amount_ranges, same_row};
use crate::text::fold_fullwidth;

/// Static texts before the target (in screen order, outside its row) that are read as its
/// neighbourhood.
pub const NEIGHBOURHOOD_TEXTS_BEFORE: usize = 2;
/// How far above the target's top (in points) a text before it may sit and still count as
/// its neighbourhood. Farther texts (a title, a clock, a "last updated" line) are not part
/// of what distinguishes the control, so their changing does not void an approval.
pub const NEIGHBOURHOOD_REACH: f64 = 150.0;
/// Elements taller than this share of the screen height (scroll and web views, side
/// bars, scroll indicators) overlap every row and are not part of a row's neighbourhood.
const MAX_ROW_ITEM_HEIGHT: f64 = 0.5;

/// The state of the control an action acts on.
#[derive(Clone, PartialEq)]
struct TargetFacts {
    fingerprint: u64,
    role: Role,
    label: String,
    value: Option<String>,
    selected: bool,
    enabled: bool,
}

/// What an approval was given on: captured from the screen the question was asked about
/// and compared, as a whole, with the same facts read from the fresh screen before the
/// approved action is sent. Any difference means the screen no longer shows what the
/// owner approved.
///
/// * the app;
/// * the target control: fingerprint, role, label, value, selected (on/off) state and
///   enabled flag;
/// * its neighbourhood: the text of every other element in its row band (vertical extent
///   overlapping the target's; this includes the cells and groups that contain it, but
///   not screen-high containers), and the nearest [`NEIGHBOURHOOD_TEXTS_BEFORE`] static
///   texts before it in screen order within [`NEIGHBOURHOOD_REACH`] points above it: what
///   tells one "Buy" from another;
/// * every money amount on the screen, as a sorted list of normalised tokens;
/// * the texts the classifier read to decide the category (every element that carries a
///   context signal: risky wording, prices, payment or settings words), taken from the
///   classifier's own context reading so the two cannot drift.
///
/// Elements far from the target that carry no signal (a clock, a spinner, a status line)
/// are not part of it, so their changing does not void an approval.
///
/// `Debug` shows counts only: the facts are screen text.
#[derive(Clone, PartialEq)]
pub struct ApprovalContext {
    app: String,
    target: Option<TargetFacts>,
    neighbourhood: Vec<String>,
    before: Vec<String>,
    amounts: Vec<String>,
    signals: Vec<String>,
}

impl std::fmt::Debug for ApprovalContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalContext")
            .field("has_target", &self.target.is_some())
            .field("neighbourhood", &self.neighbourhood.len())
            .field("before", &self.before.len())
            .field("amounts", &self.amounts.len())
            .field("signals", &self.signals.len())
            .finish_non_exhaustive()
    }
}

impl ApprovalContext {
    /// Names of the facts that differ between `self` (captured when the question was
    /// asked) and `action` on the fresh `screen` (empty when the approval still holds):
    /// "app", "target", "neighbourhood", "amounts", "context". The texts before the
    /// target are compared by position, not distance: the fresh screen must show the same
    /// texts as the nearest ones before the target (as many as were captured), wherever
    /// they moved, so a control that only shifted keeps its approval.
    pub fn differences(&self, action: &Action, screen: &CompactScreen) -> Vec<&'static str> {
        let now = build(action, screen, Some(self.before.len()));
        let mut out = Vec::new();
        if !self.app.trim().eq_ignore_ascii_case(now.app.trim()) {
            out.push("app");
        }
        if self.target != now.target {
            out.push("target");
        }
        if self.neighbourhood != now.neighbourhood || self.before != now.before {
            out.push("neighbourhood");
        }
        if self.amounts != now.amounts {
            out.push("amounts");
        }
        if self.signals != now.signals {
            out.push("context");
        }
        out
    }

    /// Whether `action` on the fresh `screen` is still exactly what was approved.
    pub fn holds_on(&self, action: &Action, screen: &CompactScreen) -> bool {
        self.differences(action, screen).is_empty()
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

/// An element's text for comparison: label and value (text inputs included, since what a
/// field holds is part of what was approved; secure fields have none).
fn fact_text(el: &Element) -> String {
    let value = if el.secure { None } else { el.value.as_deref() };
    match value {
        Some(v) if !v.is_empty() => format!("{} = {}", el.label.trim(), v.trim()),
        _ => el.label.trim().to_owned(),
    }
}

/// The row texts of `el` (see [`ApprovalContext`]).
fn row_band(el: &Element, screen: &CompactScreen) -> Vec<String> {
    let tall = screen.size.height * MAX_ROW_ITEM_HEIGHT;
    screen
        .elements
        .iter()
        .filter(|e| e.index != el.index && e.role != Role::Key && !e.frame.is_empty())
        // Screen-high containers, side bars and scroll indicators overlap every row and
        // say nothing about this one.
        .filter(|e| !(tall.is_finite() && tall > 0.0 && e.frame.height > tall))
        .filter(|e| same_row(&e.frame, &el.frame))
        .map(fact_text)
        .collect()
}

/// The static texts before `el` (screen order, above its row): the nearest `count` of
/// them, or, when `count` is `None`, the nearest [`NEIGHBOURHOOD_TEXTS_BEFORE`] within
/// [`NEIGHBOURHOOD_REACH`] points above it.
fn texts_before(el: &Element, screen: &CompactScreen, count: Option<usize>) -> Vec<String> {
    let before: Vec<String> = screen
        .elements
        .iter()
        .take_while(|e| e.index != el.index)
        .filter(|e| e.role == Role::StaticText && !e.frame.is_empty())
        .filter(|e| e.frame.max_y() <= el.frame.y)
        .filter(|e| count.is_some() || el.frame.y - e.frame.max_y() <= NEIGHBOURHOOD_REACH)
        .map(fact_text)
        .collect();
    let keep = count.unwrap_or(NEIGHBOURHOOD_TEXTS_BEFORE);
    let skip = before.len().saturating_sub(keep);
    before.into_iter().skip(skip).collect()
}

/// Every money amount on `screen`, normalised (look-alike digits folded, whitespace
/// dropped, lowercase) and sorted.
fn amounts(screen: &CompactScreen) -> Vec<String> {
    let mut out = Vec::new();
    for e in screen.elements.iter().filter(|e| e.role != Role::Key) {
        let text = if e.secure {
            e.label.clone()
        } else {
            match e.value.as_deref() {
                Some(v) if !v.is_empty() => format!("{} {v}", e.label),
                _ => e.label.clone(),
            }
        };
        let chars: Vec<char> = text.chars().collect();
        for (a, b) in amount_ranges(&chars) {
            let token: String = chars
                .get(a..b)
                .unwrap_or_default()
                .iter()
                .map(|c| fold_fullwidth(*c))
                .filter(|c| !c.is_whitespace())
                .flat_map(char::to_lowercase)
                .collect();
            out.push(token);
        }
    }
    out.sort();
    out
}

/// The [`ApprovalContext`] of `action` on `screen`, captured when the owner is asked.
pub fn approval_context(action: &Action, screen: &CompactScreen) -> ApprovalContext {
    build(action, screen, None)
}

fn build(action: &Action, screen: &CompactScreen, before_count: Option<usize>) -> ApprovalContext {
    let target = target_of(action, screen);
    let mut signals: Vec<String> = signal_texts(screen);
    signals.sort();
    ApprovalContext {
        app: screen.app_bundle_id.clone(),
        target: target.map(|t| TargetFacts {
            fingerprint: t.fingerprint,
            role: t.role.clone(),
            label: t.label.clone(),
            value: if t.secure { None } else { t.value.clone() },
            selected: t.selected,
            enabled: t.enabled,
        }),
        neighbourhood: target.map_or_else(Vec::new, |t| row_band(t, screen)),
        before: target.map_or_else(Vec::new, |t| texts_before(t, screen, before_count)),
        amounts: amounts(screen),
        signals,
    }
}
