//! Risk classification: what could go wrong if an action ran unattended.

use jarvis_proto::{Action, CompactScreen, Direction, Element, Point, Role, RiskCategory, ToolSpec};

use crate::phrases::{
    action_tables, match_label, match_tokens, max_risk, Phrase, COMPOSE_FIELD_PHRASES,
    CONTEXT_PHRASES, CREDENTIAL_FIELD_PHRASES, SETTINGS_ACTION_PHRASES, SETTINGS_BUNDLE_ID,
    SETTINGS_CONTEXT_PHRASES,
};
use crate::redact::contains_secret;
use crate::text::{has_money, tokens};
use crate::validate::url_scheme;

/// Classifies `action` on `screen`.
///
/// * `CallTool` uses the tool's declared risk, raised if the tool's *name* itself reads as
///   risky (`delete_event`). Without a spec only the name is used; `evaluate` denies
///   unknown tools outright.
/// * Taps and long presses read the target's label (and value, for non-inputs) through the
///   phrase tables. Generic confirmations ("OK", "Continue") and unlabeled buttons take on
///   the riskiest signal elsewhere on the screen (prices, "Delete 3 photos?", a Send
///   button). A price in a button label is at least a `Purchase`.
/// * `Type` is `Credential` for secure fields, credential-labelled fields or secret-looking
///   text; with `submit` it also takes the compose field's and the screen's risk, so Return
///   in a message field next to a Send button is `SendMessage`.
/// * `TapPoint` takes the riskiest element under the point, or the screen context if none.
/// * Swiping a cell sideways is `Delete` (full swipes trigger the destructive action).
/// * `OpenUrl` to `tel:`/`facetime:` is `SendMessage` (it starts a call).
/// * Everything else is `None`. An element index that does not exist falls back to the
///   screen context (and is rejected by `validate_action` anyway).
pub fn classify(action: &Action, screen: &CompactScreen, tool: Option<&ToolSpec>) -> RiskCategory {
    match action {
        Action::Tap { element } | Action::LongPress { element, .. } => match screen.get(*element)
        {
            Some(el) => element_risk(el, screen),
            None => context_risk(screen, None),
        },
        Action::Type {
            element,
            text,
            submit,
        } => type_risk(*element, text, *submit, screen),
        Action::Swipe {
            direction,
            element: Some(i),
        } => match screen.get(*i) {
            Some(el) => swipe_risk(el, *direction, screen),
            None => context_risk(screen, None),
        },
        Action::TapPoint { x, y } => tap_point_risk(*x, *y, screen),
        Action::OpenUrl { url } => match url_scheme(url).as_deref() {
            Some("tel" | "telprompt" | "facetime" | "facetime-audio") => RiskCategory::SendMessage,
            _ => RiskCategory::None,
        },
        Action::CallTool { name, .. } => {
            let by_name = match_label(name, &[crate::phrases::ACTION_PHRASES]).category;
            match tool {
                Some(spec) => max_risk(spec.risk, by_name),
                None => by_name,
            }
        }
        Action::Clear { .. }
        | Action::Scroll { .. }
        | Action::Swipe { element: None, .. }
        | Action::PressHome
        | Action::OpenApp { .. }
        | Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => RiskCategory::None,
    }
}

/// The text an element shows: its label, plus its value unless it is a text input (whose
/// value is user content, not a description of what the element does).
fn element_text(el: &Element) -> String {
    match (&el.value, el.role.is_text_input()) {
        (Some(v), false) if !v.is_empty() => format!("{} {}", el.label, v),
        _ => el.label.clone(),
    }
}

/// Whether the element is something that is pressed rather than read or typed into.
fn is_pressable(role: &Role) -> bool {
    !role.is_text_input() && !matches!(role, Role::StaticText | Role::Image)
}

/// Risk of tapping (or long-pressing) `el`.
fn element_risk(el: &Element, screen: &CompactScreen) -> RiskCategory {
    if el.role.is_text_input() {
        // Tapping a field only focuses it.
        return RiskCategory::None;
    }
    let text = element_text(el);
    let toks = tokens(&text);
    let tables = action_tables(&screen.app_bundle_id);
    let lr = match_tokens(&toks, &tables);
    let mut risk = lr.category;
    if el.role == Role::Key && risk == RiskCategory::Delete {
        // The keyboard's backspace key is labelled "delete".
        risk = RiskCategory::None;
    }
    if has_money(&text) {
        risk = match risk {
            RiskCategory::SendMessage | RiskCategory::Payment => RiskCategory::Payment,
            other => max_risk(other, RiskCategory::Purchase),
        };
    }
    let in_settings_app = screen.app_bundle_id.eq_ignore_ascii_case(SETTINGS_BUNDLE_ID);
    if el.role == Role::Switch && in_settings_app {
        risk = max_risk(risk, RiskCategory::SystemSetting);
    }
    if match_tokens(&toks, &[SETTINGS_ACTION_PHRASES]).category != RiskCategory::None
        && is_settings_screen(screen)
    {
        risk = max_risk(risk, RiskCategory::AccountChange);
    }
    let unlabeled = toks.is_empty() && is_pressable(&el.role);
    if lr.generic || unlabeled {
        risk = max_risk(risk, context_risk(screen, Some(el.index)));
    }
    risk
}

/// The riskiest signal among the screen's elements other than `exclude`: risky labels,
/// explanatory text ("This cannot be undone") and prices (which count as `Payment`).
/// Keyboard keys are ignored.
pub(crate) fn context_risk(screen: &CompactScreen, exclude: Option<u32>) -> RiskCategory {
    let mut tables: Vec<&[Phrase]> = action_tables(&screen.app_bundle_id);
    tables.push(CONTEXT_PHRASES);
    let mut risk = RiskCategory::None;
    for el in &screen.elements {
        if Some(el.index) == exclude || el.role == Role::Key {
            continue;
        }
        let text = element_text(el);
        risk = max_risk(risk, match_label(&text, &tables).category);
        if has_money(&text) {
            risk = max_risk(risk, RiskCategory::Payment);
        }
    }
    risk
}

/// Whether the screen is the Settings app or reads like an account/settings page.
fn is_settings_screen(screen: &CompactScreen) -> bool {
    screen.app_bundle_id.eq_ignore_ascii_case(SETTINGS_BUNDLE_ID)
        || screen.elements.iter().any(|e| {
            match_label(&e.label, &[SETTINGS_CONTEXT_PHRASES]).category != RiskCategory::None
        })
}

/// The element `Type` lands in: the given one, or the focused text input.
pub(crate) fn type_target(element: Option<u32>, screen: &CompactScreen) -> Option<&Element> {
    match element {
        Some(i) => screen.get(i),
        None => screen
            .elements
            .iter()
            .find(|e| e.focused && e.role.is_text_input()),
    }
}

/// Risk of typing `text` (and pressing Return when `submit`).
fn type_risk(element: Option<u32>, text: &str, submit: bool, screen: &CompactScreen) -> RiskCategory {
    let target = type_target(element, screen);
    let mut risk = RiskCategory::None;
    if let Some(t) = target {
        let field = format!("{} {}", t.label, t.placeholder.as_deref().unwrap_or(""));
        if t.secure
            || t.role == Role::SecureTextField
            || match_label(&field, &[CREDENTIAL_FIELD_PHRASES]).category != RiskCategory::None
        {
            risk = RiskCategory::Credential;
        }
    }
    if contains_secret(text) {
        risk = RiskCategory::Credential;
    }
    if submit {
        risk = max_risk(risk, submit_risk(target, screen));
    }
    risk
}

/// Risk of pressing Return in `target`: the compose field's own meaning plus the screen's
/// context (a Send button, a price). Search fields only search.
fn submit_risk(target: Option<&Element>, screen: &CompactScreen) -> RiskCategory {
    match target {
        Some(t) if t.role == Role::SearchField => RiskCategory::None,
        Some(t) => {
            let field = format!("{} {}", t.label, t.placeholder.as_deref().unwrap_or(""));
            let compose = match_label(&field, &[COMPOSE_FIELD_PHRASES]).category;
            max_risk(compose, context_risk(screen, Some(t.index)))
        }
        None => context_risk(screen, None),
    }
}

/// Risk of swiping on `el`.
fn swipe_risk(el: &Element, direction: Direction, screen: &CompactScreen) -> RiskCategory {
    let direct = match_label(&element_text(el), &action_tables(&screen.app_bundle_id)).category;
    let sideways = matches!(direction, Direction::Left | Direction::Right);
    if sideways && el.role == Role::Cell {
        max_risk(direct, RiskCategory::Delete)
    } else {
        direct
    }
}

/// Risk of tapping a normalized point: the riskiest pressable element under it; when only
/// text or nothing is under it, the screen context too.
fn tap_point_risk(x: f64, y: f64, screen: &CompactScreen) -> RiskCategory {
    if !(x.is_finite() && y.is_finite()) {
        return context_risk(screen, None);
    }
    let p = Point::new(x * screen.size.width, y * screen.size.height);
    let mut pressable_hit = false;
    let mut risk = RiskCategory::None;
    for el in screen.elements.iter().filter(|e| e.frame.contains(p)) {
        pressable_hit |= is_pressable(&el.role);
        let r = if el.role.is_text_input() {
            RiskCategory::None
        } else {
            element_risk(el, screen)
        };
        risk = max_risk(risk, r);
    }
    if pressable_hit {
        risk
    } else {
        // Only text (or nothing) under the point: whatever is really there is unknown.
        max_risk(risk, context_risk(screen, None))
    }
}
