//! Replaying a learned flow with no model calls, one verified step at a time.
//!
//! Safety rules, in order, for every step: the expected app must be in front (for steps that
//! act on app content); the target control must be found by fingerprint (with the same role),
//! or, when no element carries that fingerprint, by exactly one role + label match; ambiguity,
//! absence or a disabled control hand control back. Typing into a secure field is never
//! replayed. Emitted actions use the indices of the screen passed to [`Replayer::next`].

use std::fmt;

use jarvis_proto::{Action, CompactScreen, Element};

use crate::flow::{Bindings, Flow, FlowAction, FlowStep, FlowText};
use crate::profile::profile_placeholder;

/// Why a replay stopped and handed control back to the model. Never an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Divergence {
    /// The foreground app is not the one the step was recorded in.
    AppMismatch {
        /// App the step expects.
        expected: String,
        /// App actually in front.
        found: String,
    },
    /// No matching control on screen.
    ControlMissing {
        /// Label of the expected control.
        label: String,
    },
    /// More than one control matches, so the right one cannot be told.
    Ambiguous {
        /// Label of the expected control.
        label: String,
        /// How many controls matched.
        matches: usize,
    },
    /// The control exists but is disabled.
    ControlDisabled {
        /// Label of the control.
        label: String,
    },
    /// The step would type into a secure field.
    SecureField,
    /// The step types into the focused field, but nothing is focused.
    NoFocusedField,
    /// The step needs a slot value the task did not provide.
    UnboundSlot {
        /// The slot number.
        slot: u32,
    },
    /// The recording stopped here (secure or sensitive input); the model takes over.
    HandBack {
        /// Reason recorded.
        reason: String,
    },
    /// The stored step is inconsistent (for example a tap without a target).
    Malformed,
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Divergence::AppMismatch { expected, found } => {
                write!(f, "expected app {expected}, found {found}")
            }
            Divergence::ControlMissing { label } => write!(f, "control \"{label}\" not on screen"),
            Divergence::Ambiguous { label, matches } => {
                write!(f, "{matches} controls match \"{label}\"")
            }
            Divergence::ControlDisabled { label } => write!(f, "control \"{label}\" is disabled"),
            Divergence::SecureField => write!(f, "step would type into a secure field"),
            Divergence::NoFocusedField => write!(f, "no focused field to type into"),
            Divergence::UnboundSlot { slot } => write!(f, "no value for slot {{{slot}}}"),
            Divergence::HandBack { reason } => write!(f, "hand back: {reason}"),
            Divergence::Malformed => write!(f, "stored step is malformed"),
        }
    }
}

/// What to do next.
#[derive(Debug, Clone, PartialEq)]
pub enum Next {
    /// Execute this action (indices refer to the screen given to `next`).
    Action(Action),
    /// Stop replaying; the model continues from the current screen.
    Diverged {
        /// Why.
        reason: Divergence,
    },
    /// Every step has been replayed.
    Complete,
}

/// Walks a flow's steps. `next` is pure; call [`Replayer::advance`] once the step is confirmed.
#[derive(Debug, Clone)]
pub struct Replayer {
    flow: Flow,
    bindings: Bindings,
    pos: usize,
}

impl Replayer {
    /// Starts at the first step.
    pub fn new(flow: Flow, bindings: Bindings) -> Self {
        Self {
            flow,
            bindings,
            pos: 0,
        }
    }

    /// The current step, if any (seventeenth review: the session compares the place it was
    /// recorded in with the place it would run in).
    pub fn current_step(&self) -> Option<&FlowStep> {
        self.flow.steps.get(self.pos)
    }

    /// Index of the current step.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Number of steps in the flow.
    pub fn total_steps(&self) -> usize {
        self.flow.steps.len()
    }

    /// Id of the flow being replayed.
    pub fn flow_id(&self) -> &str {
        &self.flow.id
    }

    /// Moves to the next step; call after the current step's action was confirmed.
    pub fn advance(&mut self) {
        self.pos = self.pos.saturating_add(1).min(self.flow.steps.len());
    }

    /// The action for the current step on `screen`, or why replay must stop.
    pub fn next(&self, screen: &CompactScreen) -> Next {
        let Some(step) = self.flow.steps.get(self.pos) else {
            return Next::Complete;
        };
        match self.resolve_step(step, screen) {
            Ok(action) => Next::Action(action),
            Err(reason) => Next::Diverged { reason },
        }
    }

    fn resolve_step(&self, step: &FlowStep, screen: &CompactScreen) -> Result<Action, Divergence> {
        if let FlowAction::HandBack { reason } = &step.action {
            return Err(Divergence::HandBack {
                reason: reason.clone(),
            });
        }
        if step.action.acts_on_screen() && screen.app_bundle_id != step.app_bundle_id {
            return Err(Divergence::AppMismatch {
                expected: step.app_bundle_id.clone(),
                found: screen.app_bundle_id.clone(),
            });
        }
        let has_target = step.fingerprint.is_some() || step.role.is_some();
        let target = if has_target {
            Some(resolve_target(step, screen)?)
        } else if step.action.needs_target() {
            return Err(Divergence::Malformed);
        } else {
            None
        };
        if let Some(e) = target {
            let interactive = !matches!(
                step.action,
                FlowAction::Scroll { .. } | FlowAction::Swipe { .. }
            );
            if interactive && !e.enabled {
                return Err(Divergence::ControlDisabled {
                    label: e.label.clone(),
                });
            }
        }
        let element = target.map(|e| e.index);
        match &step.action {
            FlowAction::Tap => element
                .map(|element| Action::Tap { element })
                .ok_or(Divergence::Malformed),
            FlowAction::LongPress { duration_ms } => element
                .map(|element| Action::LongPress {
                    element,
                    duration_ms: *duration_ms,
                })
                .ok_or(Divergence::Malformed),
            FlowAction::Clear => element
                .map(|element| Action::Clear { element })
                .ok_or(Divergence::Malformed),
            FlowAction::Type { text, submit } => {
                let secure = match target {
                    Some(e) => e.secure,
                    None => screen.elements.iter().any(|e| e.focused && e.secure),
                };
                if secure {
                    return Err(Divergence::SecureField);
                }
                if target.is_none()
                    && !screen.keyboard_visible
                    && !screen.elements.iter().any(|e| e.focused)
                {
                    return Err(Divergence::NoFocusedField);
                }
                let text = match text {
                    FlowText::Literal { text } => text.clone(),
                    FlowText::Slot { slot } => self
                        .bindings
                        .get(slot)
                        .cloned()
                        .ok_or(Divergence::UnboundSlot { slot: *slot })?,
                    FlowText::Profile { key } => profile_placeholder(key),
                };
                Ok(Action::Type {
                    element,
                    text,
                    submit: *submit,
                })
            }
            FlowAction::Scroll { direction } => Ok(Action::Scroll {
                direction: *direction,
                element,
            }),
            FlowAction::Swipe { direction } => Ok(Action::Swipe {
                direction: *direction,
                element,
            }),
            FlowAction::PressHome => Ok(Action::PressHome),
            FlowAction::OpenApp { app } => Ok(Action::OpenApp { app: app.clone() }),
            FlowAction::OpenUrl { url } => Ok(Action::OpenUrl { url: url.clone() }),
            FlowAction::Wait { ms } => Ok(Action::Wait { ms: *ms }),
            FlowAction::HandBack { reason } => Err(Divergence::HandBack {
                reason: reason.clone(),
            }),
        }
    }
}

/// Normalised label for comparison: trimmed, lowercase, single spaces.
fn norm_label(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .flat_map(char::to_lowercase)
        .collect()
}

/// Labels too generic to identify a control without its fingerprint.
const GENERIC_LABELS: &[&str] = &[
    "ok", "done", "continue", "yes", "no", "allow", "delete", "send", "pay", "buy", "confirm",
    "cancel", "next", "back", "close", "save", "submit", "remove", "accept", "agree",
];

/// Finds the step's control, preferring divergence over any guess.
///
/// A fingerprint hit must be unique on screen AND agree on role and normalised label. Without
/// any fingerprint hit, a unique role + label match is used, but only for a specific label
/// (at least 2 characters, not a generic word such as "OK" or "Delete").
fn resolve_target<'s>(
    step: &FlowStep,
    screen: &'s CompactScreen,
) -> Result<&'s Element, Divergence> {
    let want_label = norm_label(&step.label);
    if let Some(fp) = step.fingerprint {
        let hits: Vec<&Element> = screen
            .elements
            .iter()
            .filter(|e| e.fingerprint == fp)
            .collect();
        match hits.as_slice() {
            [] => {}
            [one] => {
                let role_ok = step.role.as_ref().is_none_or(|r| *r == one.role);
                if role_ok && norm_label(&one.label) == want_label {
                    return Ok(one);
                }
                return Err(Divergence::ControlMissing {
                    label: step.label.clone(),
                });
            }
            many => {
                return Err(Divergence::Ambiguous {
                    label: step.label.clone(),
                    matches: many.len(),
                })
            }
        }
    }
    let Some(role) = step.role.as_ref() else {
        return Err(Divergence::ControlMissing {
            label: step.label.clone(),
        });
    };
    if want_label.chars().count() < 2 || GENERIC_LABELS.contains(&want_label.as_str()) {
        return Err(Divergence::ControlMissing {
            label: step.label.clone(),
        });
    }
    let hits: Vec<&Element> = screen
        .elements
        .iter()
        .filter(|e| e.role == *role && norm_label(&e.label) == want_label)
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(Divergence::ControlMissing {
            label: step.label.clone(),
        }),
        many => Err(Divergence::Ambiguous {
            label: step.label.clone(),
            matches: many.len(),
        }),
    }
}
