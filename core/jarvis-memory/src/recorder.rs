//! Recording a flow while a session runs.
//!
//! A flow is recordable only if every step can be replayed without a human or a tool:
//! * UI actions with a resolvable target element (the caller passes the element the action's
//!   index points at), or element-less ones (`PressHome`, `OpenApp`, `OpenUrl`, `Wait`,
//!   scroll/swipe without an element) are recorded;
//! * `AskUser`, `CallTool` (side effects cannot be replayed) and `TapPoint` (pixel guess) make
//!   the whole flow non-recordable, as does an action whose element is missing or has a
//!   different index than the action names;
//! * `Look`, `Done` and `Fail` are not UI steps and are ignored;
//! * typing into a secure field (or typing a value registered as sensitive) is never stored:
//!   the step becomes [`FlowAction::HandBack`] and the flow ends there;
//! * typing exactly one `{{profile.KEY}}` placeholder is stored as
//!   [`FlowText::Profile`] (the key, never the value), sensitive fields included, and
//!   replays as the placeholder for the agent to substitute.

use jarvis_proto::{Action, CompactScreen, Element};

use crate::flow::{flow_id, screen_signature, Flow, FlowAction, FlowStep, FlowText, MAX_STEPS};
use crate::intent::{
    find_span, intent_key, is_slotted, literal_count, match_slotted, slotted_key, MAX_SLOTS,
};
use crate::profile::placeholder_key;

/// One successful step handed to the [`Recorder`].
#[derive(Debug, Clone)]
pub struct RecordedStep<'a> {
    /// The action that was executed.
    pub action: Action,
    /// The element the action's index referred to, if it has one.
    pub element: Option<&'a Element>,
    /// The screen the action was chosen on (before executing it).
    pub screen: &'a CompactScreen,
}

/// Collects steps of a running session and produces a [`Flow`] on success.
#[derive(Debug, Clone)]
pub struct Recorder {
    task_text: String,
    steps: Vec<FlowStep>,
    non_recordable: Option<&'static str>,
    handed_back: bool,
    sensitive_values: Vec<String>,
    target_app: Option<String>,
    last_target_secure: bool,
}

impl Recorder {
    /// Starts recording for a task.
    pub fn new(task_text: impl Into<String>) -> Self {
        Self {
            task_text: task_text.into(),
            steps: Vec::new(),
            non_recordable: None,
            handed_back: false,
            sensitive_values: Vec::new(),
            target_app: None,
            last_target_secure: false,
        }
    }

    /// Registers values (e.g. sensitive profile fields) that must never be stored: typing one
    /// becomes a hand-back step.
    pub fn set_sensitive_values(&mut self, values: Vec<String>) {
        self.sensitive_values = values
            .into_iter()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
            .collect();
    }

    /// Why the session cannot become a flow, if it already cannot.
    pub fn non_recordable_reason(&self) -> Option<&'static str> {
        self.non_recordable
    }

    /// Number of steps recorded so far.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// True when nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    fn is_sensitive(&self, typed: &str) -> bool {
        let t = alnum(typed);
        if t.is_empty() {
            return false;
        }
        self.sensitive_values.iter().any(|v| {
            let v = alnum(v);
            if v.len() >= 6 {
                t.contains(&v) || (t.len() >= 6 && v.contains(&t))
            } else {
                !v.is_empty() && v == t
            }
        })
    }

    /// Records one successful step.
    pub fn record(&mut self, step: RecordedStep<'_>) {
        if self.non_recordable.is_some() || self.handed_back {
            return;
        }
        let RecordedStep {
            action,
            element,
            screen,
        } = step;
        match &action {
            Action::Done { .. } | Action::Fail { .. } | Action::Look => return,
            Action::CallTool { .. } => {
                self.non_recordable = Some("tool call with side effects");
                return;
            }
            Action::AskUser { .. } => {
                self.non_recordable = Some("needed the user");
                return;
            }
            Action::TapPoint { .. } => {
                self.non_recordable = Some("coordinate tap");
                return;
            }
            _ => {}
        }
        let mut target = match action.element() {
            Some(index) => match element {
                Some(e) if e.index == index => Some(e),
                _ => {
                    self.non_recordable = Some("action target not resolvable");
                    return;
                }
            },
            None => None,
        };
        // A `Type` without an element goes to the focused field; pin it to that field.
        if target.is_none() && matches!(action, Action::Type { .. }) {
            target = screen.elements.iter().find(|e| e.focused);
        }
        // No resolvable target: the field is unknown. Treat as secure when the last control
        // used was a secure field, or the screen has a secure field and nothing is focused.
        let unknown_secure = target.is_none()
            && matches!(action, Action::Type { .. })
            && (self.last_target_secure
                || (screen.elements.iter().any(|e| e.secure)
                    && !screen.elements.iter().any(|e| e.focused)));
        let flow_action = match action {
            Action::Tap { .. } => FlowAction::Tap,
            Action::LongPress { duration_ms, .. } => FlowAction::LongPress { duration_ms },
            Action::Clear { .. } => FlowAction::Clear,
            Action::Scroll { direction, .. } => FlowAction::Scroll { direction },
            Action::Swipe { direction, .. } => FlowAction::Swipe { direction },
            Action::PressHome => FlowAction::PressHome,
            Action::OpenApp { app } => FlowAction::OpenApp { app },
            Action::OpenUrl { url } => FlowAction::OpenUrl { url },
            Action::Wait { ms } => FlowAction::Wait { ms },
            Action::Type { text, submit, .. } => {
                let secure = unknown_secure || target.is_some_and(|e| e.secure);
                let profile_key = placeholder_key(&text);
                let sensitive = profile_key.is_none() && self.is_sensitive(&text);
                if secure || sensitive {
                    self.handed_back = true;
                    FlowAction::HandBack {
                        reason: if secure {
                            "secure field".to_owned()
                        } else {
                            "sensitive value".to_owned()
                        },
                    }
                } else if let Some(key) = profile_key {
                    FlowAction::Type {
                        text: FlowText::Profile { key },
                        submit,
                    }
                } else {
                    FlowAction::Type {
                        text: FlowText::Literal { text },
                        submit,
                    }
                }
            }
            Action::CallTool { .. }
            | Action::AskUser { .. }
            | Action::TapPoint { .. }
            | Action::Look
            | Action::Done { .. }
            | Action::Fail { .. } => return,
        };
        if let Some(e) = target {
            self.last_target_secure = e.secure;
        }
        if target.is_some() {
            self.target_app = Some(screen.app_bundle_id.clone());
        }
        self.steps.push(FlowStep {
            fingerprint: target.map(|e| e.fingerprint),
            role: target.map(|e| e.role.clone()),
            label: target.map(|e| e.label.clone()).unwrap_or_default(),
            app_bundle_id: screen.app_bundle_id.clone(),
            action: flow_action,
            screen_hash_before: screen_signature(screen),
        });
    }

    /// Ends recording. Returns a flow only when the session succeeded, stayed recordable and
    /// holds at least one replayable step (a flow starting with a hand-back, or made only of
    /// waits, is useless and dropped).
    ///
    /// The flow's app is the foreground app of the last step that had a target control, else of
    /// the last step. Typed text that appears once, word-bounded and case-insensitively, in the
    /// task text becomes a slot (`{1}`, `{2}`, ...) in both the steps and the intent key; if the
    /// slotted key would not round-trip through [`match_slotted`] on the original task, or
    /// would have no literal word, the flow is stored unslotted instead.
    pub fn finish(self, success: bool, now_ms: u64) -> Option<Flow> {
        if !success || self.non_recordable.is_some() {
            return None;
        }
        let Self {
            task_text,
            mut steps,
            target_app,
            ..
        } = self;
        let first = steps.first()?;
        if matches!(first.action, FlowAction::HandBack { .. })
            || steps.len() > MAX_STEPS
            || steps
                .iter()
                .all(|s| matches!(s.action, FlowAction::Wait { .. }))
        {
            return None;
        }
        let app = target_app.or_else(|| steps.last().map(|s| s.app_bundle_id.clone()))?;
        let mut key = intent_key(&task_text);
        if let Some((slot_key, slots)) = extract_slots(&task_text, &steps) {
            key = slot_key;
            for step in &mut steps {
                if let FlowAction::Type { text, .. } = &mut step.action {
                    if let FlowText::Literal { text: t } = text {
                        if let Some((id, _)) = slots.iter().find(|(_, v)| v == t) {
                            *text = FlowText::Slot { slot: *id };
                        }
                    }
                }
            }
        }
        if key.is_empty() {
            return None;
        }
        Some(Flow {
            id: flow_id(&key, &app),
            app_bundle_id: app,
            intent_key: key,
            steps,
            successes: 1,
            failures: 0,
            last_used_ms: now_ms,
            retired: false,
        })
    }
}

/// Finds typed texts inside the task and returns the slotted key and `(slot, text)` pairs, or
/// `None` when slotting is not possible or not safe.
fn extract_slots(task_text: &str, steps: &[FlowStep]) -> Option<(String, Vec<(u32, String)>)> {
    let mut slots: Vec<(u32, String)> = Vec::new();
    let mut spans: Vec<(usize, usize, u32)> = Vec::new();
    for step in steps {
        let FlowAction::Type {
            text: FlowText::Literal { text },
            ..
        } = &step.action
        else {
            continue;
        };
        if slots.iter().any(|(_, t)| t == text) || slots.len() >= MAX_SLOTS {
            continue;
        }
        let Some((start, end)) = find_span(task_text, text) else {
            continue;
        };
        if spans.iter().any(|(s, e, _)| start < *e && *s < end) {
            continue;
        }
        let id = u32::try_from(slots.len() + 1).ok()?;
        slots.push((id, text.clone()));
        spans.push((start, end, id));
    }
    if slots.is_empty() {
        return None;
    }
    let key = slotted_key(task_text, &spans)?;
    if !is_slotted(&key) || literal_count(&key) == 0 {
        return None;
    }
    let bound = match_slotted(&key, task_text)?;
    let outer = |s: &str| s.trim_matches(|c: char| !c.is_alphanumeric()).to_owned();
    for (id, text) in &slots {
        if outer(bound.get(id)?) != outer(text) {
            return None;
        }
    }
    if bound.len() != slots.len() {
        return None;
    }
    Some((key, slots))
}

/// Lowercase alphanumerics only, for comparing sensitive values.
fn alnum(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
