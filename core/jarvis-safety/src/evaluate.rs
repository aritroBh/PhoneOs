//! The single entry point the agent calls before executing an action.

use jarvis_proto::{
    Action, CompactScreen, ConfirmRequest, Element, Policy, RiskCategory, ToolSpec, TrustLevel,
};

use crate::app::{app_level, check_app, is_springboard, mode_level, Verdict};
use crate::classify::{classify, type_target};
use crate::gate::{changes_state, gate_in, Gate};
use crate::redact::{contains_secret, redact};
use crate::sanitize::sanitize_screen_text_with_limit;
use crate::validate::{looks_like_url, validate_action, validate_url};

/// What the agent should do with a proposed action.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Execute it.
    Allow,
    /// Ask the user with this request; execute only if approved.
    Confirm(ConfirmRequest),
    /// Never execute it; the reason is safe to show the model and the user.
    Deny(String),
}

/// Longest quoted fragment (label, typed text, argument) in a summary, in characters.
const MAX_QUOTE_CHARS: usize = 60;
/// Longest summary, in characters.
const MAX_SUMMARY_CHARS: usize = 200;

/// Whether the action touches or reads the foreground app (and so is blocked inside a
/// denied app). Leaving the app (`PressHome`, `OpenApp`...) stays possible.
fn uses_foreground_app(action: &Action) -> bool {
    matches!(
        action,
        Action::Tap { .. }
            | Action::LongPress { .. }
            | Action::Type { .. }
            | Action::Clear { .. }
            | Action::Scroll { .. }
            | Action::Swipe { .. }
            | Action::TapPoint { .. }
            | Action::Look
    )
}

/// Runs `validate_action`, `check_app`, `classify` and `gate_in` in that order.
///
/// Denies: invalid actions; any action that touches or reads a denied foreground app
/// (`denied_apps` or a `Denied` app rule); `OpenApp` into a denied app; `CallTool` without
/// a matching `tool` spec, with a URL argument whose scheme is not allowed, or whose tool
/// acts on a denied app; any action whose category is in `policy.deny_categories`.
///
/// Otherwise the gate depends on the trust level ([`crate::app_level`]) of the app the
/// action affects (for `CallTool` the tool's app, or `policy.mode` when it has none; else
/// the foreground app): `StepByStep`
/// confirms every action that [`changes_state`] (with category `None` when nothing riskier
/// was found), `Trusted` lets [`crate::TRUSTED_ALLOWED`] categories through, and in every
/// case a category in `policy.confirm` asks, with a one-line sanitised summary such as
/// `Tap "Pay $42.10" in Amazon`.
///
/// First-use approval is not part of this decision; see [`first_use_app`].
pub fn evaluate(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
) -> Decision {
    if let Err(e) = validate_action(action, screen) {
        return Decision::Deny(clean(&e.to_string(), MAX_SUMMARY_CHARS));
    }
    if uses_foreground_app(action) {
        if let Verdict::Deny(_) = check_app(policy, &screen.app_bundle_id) {
            return Decision::Deny(format!(
                "{} is on the deny list",
                clean(&screen.app_bundle_id, MAX_QUOTE_CHARS)
            ));
        }
    }
    let mut affected_app = screen.app_bundle_id.clone();
    let mut level = app_level(policy, &screen.app_bundle_id);
    match action {
        Action::OpenApp { app } => {
            if check_app(policy, app).is_denied() {
                return Decision::Deny(format!(
                    "{} is on the deny list",
                    clean(app, MAX_QUOTE_CHARS)
                ));
            }
            affected_app = app.clone();
        }
        Action::CallTool { name, .. } => {
            let mut strings = Vec::new();
            if let Action::CallTool { args, .. } = action {
                crate::classify::tool_strings(args, &mut strings);
            }
            for s in strings.iter().filter(|s| looks_like_url(s)) {
                if let Err(e) = validate_url(s.trim()) {
                    return Decision::Deny(format!(
                        "tool argument: {}",
                        clean(&e.to_string(), MAX_SUMMARY_CHARS)
                    ));
                }
            }
            let Some(spec) = tool.filter(|t| t.name == *name) else {
                return Decision::Deny(format!(
                    "unknown tool \"{}\"",
                    clean(name, MAX_QUOTE_CHARS)
                ));
            };
            match &spec.app {
                Some(app) => {
                    if check_app(policy, app).is_denied() {
                        return Decision::Deny(format!(
                            "tool acts on {}, which is on the deny list",
                            clean(app, MAX_QUOTE_CHARS)
                        ));
                    }
                    affected_app = app.clone();
                    level = app_level(policy, app);
                }
                // A tool that acts in no app is not covered by the foreground app's rule.
                None => level = mode_level(policy),
            }
        }
        _ => {}
    }
    if level == TrustLevel::Denied && !uses_foreground_app(action) {
        // Leaving a denied app stays possible; how it is gated does not depend on it.
        level = app_level(&without_app_denials(policy), &affected_app);
    }
    let category = classify(action, screen, tool);
    match gate_in(policy, category, level, action, screen) {
        Gate::Allow => Decision::Allow,
        Gate::Deny => Decision::Deny(format!("{category:?} actions are not allowed")),
        Gate::Confirm => Decision::Confirm(ConfirmRequest {
            category,
            summary: summarize_with(action, screen, category),
            app: affected_app,
        }),
    }
}

/// `policy` with no app denied, for gating the way out of a denied app.
fn without_app_denials(policy: &Policy) -> Policy {
    let mut p = policy.clone();
    p.denied_apps.clear();
    p.app_rules.retain(|r| r.level != TrustLevel::Denied);
    p
}

/// The app that `action` would act inside and that therefore needs first-use approval
/// under `policy`, before the agent's own bookkeeping (already-approved apps, its own
/// app) is applied.
///
/// `None` when `policy.first_use_approval` is off, when the action does not act inside an
/// app (`PressHome`, `OpenApp`, `OpenUrl`, `Wait`, `Look`, asking, finishing, plain
/// vertical scrolls — see [`changes_state`]), when the app is SpringBoard, and when its
/// trust level is `Trusted` (or `Denied`, which `evaluate` refuses anyway). UI actions act
/// inside the foreground app; a `CallTool` acts inside its spec's `app`, if any.
pub fn first_use_app(
    policy: &Policy,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
) -> Option<String> {
    if !policy.first_use_approval || !changes_state(action, screen) {
        return None;
    }
    let app = match action {
        Action::CallTool { name, .. } => tool.filter(|t| t.name == *name)?.app.clone()?,
        Action::Tap { .. }
        | Action::LongPress { .. }
        | Action::Type { .. }
        | Action::Clear { .. }
        | Action::Scroll { .. }
        | Action::Swipe { .. }
        | Action::TapPoint { .. } => screen.app_bundle_id.clone(),
        _ => return None,
    };
    if app.trim().is_empty() || is_springboard(&app) {
        return None;
    }
    match app_level(policy, &app) {
        TrustLevel::Trusted | TrustLevel::Denied => None,
        TrustLevel::Guarded | TrustLevel::StepByStep => Some(app),
    }
}

/// The `FirstUse` confirmation for acting in `bundle_id`, e.g.
/// `Let Jarvis act in Shop (com.example.shop)?` (sanitised; the name is left out when
/// unknown or blank).
pub fn first_use_request(bundle_id: &str, app_name: Option<&str>) -> ConfirmRequest {
    let id = clean(bundle_id, MAX_QUOTE_CHARS);
    let summary = match app_name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => format!("Let Jarvis act in {} ({id})?", clean(name, MAX_QUOTE_CHARS)),
        None => format!("Let Jarvis act in {id}?"),
    };
    ConfirmRequest {
        category: RiskCategory::FirstUse,
        summary: sanitize_screen_text_with_limit(&summary, MAX_SUMMARY_CHARS),
        app: bundle_id.to_owned(),
    }
}

/// Redacts, sanitises and caps `s`, and turns `"` into `'` so it can sit inside quotes.
fn clean(s: &str, max: usize) -> String {
    sanitize_screen_text_with_limit(&redact(s), max).replace('"', "'")
}

/// `"label"`, or the role and index when the element has no text.
fn describe(el: &Element) -> String {
    let text = [Some(el.label.as_str()), el.placeholder.as_deref()]
        .into_iter()
        .flatten()
        .find(|t| !t.trim().is_empty());
    match text {
        Some(t) => format!("\"{}\"", clean(t, MAX_QUOTE_CHARS)),
        None => format!("{} #{}", el.role.as_str(), el.index),
    }
}

/// The element description, or `element N` when it is not on the screen.
fn describe_index(screen: &CompactScreen, index: u32) -> String {
    screen
        .get(index)
        .map_or_else(|| format!("element {index}"), describe)
}

/// A one-line, human-readable, sanitised description of `action` for a confirmation
/// prompt, e.g. `Tap "Pay $42.10" in Amazon`. Typed secrets are never shown.
pub fn summarize(action: &Action, screen: &CompactScreen, tool: Option<&ToolSpec>) -> String {
    summarize_with(action, screen, classify(action, screen, tool))
}

fn summarize_with(action: &Action, screen: &CompactScreen, category: RiskCategory) -> String {
    let app_name = screen
        .app_name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(&screen.app_bundle_id);
    let in_app = format!(" in {}", clean(app_name, MAX_QUOTE_CHARS));
    let line = match action {
        Action::Tap { element } => format!("Tap {}{in_app}", describe_index(screen, *element)),
        Action::LongPress { element, .. } => {
            format!("Long-press {}{in_app}", describe_index(screen, *element))
        }
        Action::Type {
            element,
            text,
            submit,
        } => {
            let target = type_target(*element, screen);
            let secret = category == RiskCategory::Credential
                || contains_secret(text)
                || target.is_some_and(|t| t.secure);
            let what = if secret {
                format!("a secret ({} characters)", text.chars().count())
            } else {
                format!("\"{}\"", clean(text, MAX_QUOTE_CHARS))
            };
            let into = target.map_or_else(|| "the focused field".to_owned(), describe);
            let ret = if *submit { " and press Return" } else { "" };
            format!("Type {what} into {into}{ret}{in_app}")
        }
        Action::Clear { element } => format!("Clear {}{in_app}", describe_index(screen, *element)),
        Action::Scroll { direction, element } => match element {
            Some(i) => format!(
                "Scroll {direction:?} in {}{in_app}",
                describe_index(screen, *i)
            ),
            None => format!("Scroll {direction:?}{in_app}"),
        },
        Action::Swipe { direction, element } => match element {
            Some(i) => format!(
                "Swipe {direction:?} on {}{in_app}",
                describe_index(screen, *i)
            ),
            None => format!("Swipe {direction:?}{in_app}"),
        },
        Action::TapPoint { x, y } => {
            // The element that decided the risk: the riskiest pressable one under the
            // point, never a decorative text inside it.
            let on = crate::classify::tap_point(*x, *y, screen)
                .1
                .map(|e| format!(" on {}", describe(e)))
                .unwrap_or_default();
            format!("Tap at ({:.0}%, {:.0}%){on}{in_app}", x * 100.0, y * 100.0)
        }
        Action::PressHome => "Go to the Home Screen".to_owned(),
        Action::OpenApp { app } => format!("Open {}", clean(app, MAX_QUOTE_CHARS)),
        Action::OpenUrl { url } => format!("Open {}", clean(url, 120)),
        Action::Wait { ms } => format!("Wait {ms} ms"),
        Action::CallTool { name, args } => {
            let mut parts = Vec::new();
            if let Some(map) = args.as_object() {
                for (k, v) in map {
                    let v = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    parts.push(format!("{}: {}", clean(k, 30), clean(&v, 40)));
                }
            }
            if parts.is_empty() {
                format!("Run {}", clean(name, MAX_QUOTE_CHARS))
            } else {
                format!(
                    "Run {} ({})",
                    clean(name, MAX_QUOTE_CHARS),
                    parts.join(", ")
                )
            }
        }
        Action::AskUser { question } => format!("Ask: {}", clean(question, 120)),
        Action::Look => format!("Take a screenshot{in_app}"),
        Action::Done { summary } => format!("Finish: {}", clean(summary, 120)),
        Action::Fail { reason } => format!("Give up: {}", clean(reason, 120)),
    };
    sanitize_screen_text_with_limit(&line, MAX_SUMMARY_CHARS)
}
