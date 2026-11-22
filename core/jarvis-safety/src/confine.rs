//! Skill confinement: a session running a skill with `confine_to_apps` stays inside the
//! skill's apps.
//!
//! Confinement only narrows. It is checked in addition to every other gate (deny list,
//! trust levels, first-use approval, confirmations, budgets) and never replaces one: an
//! action it allows still goes through [`crate::evaluate`] and first-use approval.
//!
//! The home screen (SpringBoard) is a launcher: its icons, Spotlight, widgets and Control
//! Center reach every app and many system actions, and acting there needs no first-use
//! approval. So a confined skill does not act on it at all; from there it may only go home,
//! wait, or `open_app` one of its own apps.

use jarvis_proto::{Action, CompactScreen, SkillContext, ToolSpec};

use crate::app::{is_springboard, Verdict};
use crate::sanitize::sanitize_screen_text_with_limit;

/// Longest app id quoted in a confinement reason.
const MAX_QUOTE_CHARS: usize = 80;

/// Whether `bundle_id` is one of the skill's apps (case-insensitive, surrounding whitespace
/// ignored, exact id: `com.example` does not cover `com.example.shop`).
fn in_skill_apps(skill: &SkillContext, bundle_id: &str) -> bool {
    let id = bundle_id.trim();
    !id.is_empty() && skill.apps.iter().any(|a| a.trim().eq_ignore_ascii_case(id))
}

fn quote(s: &str) -> String {
    sanitize_screen_text_with_limit(s, MAX_QUOTE_CHARS)
}

/// Checks `action` against the confinement of `skill`.
///
/// Allows everything when there is no skill or `confine_to_apps` is off. Otherwise denies,
/// with a reason safe to show the model and the owner:
///
/// - any action that touches or reads the foreground app (tap, long press, type, clear,
///   scroll, swipe, tap point, look) unless the foreground app is one of the skill's apps.
///   This includes SpringBoard: home-screen icons, Spotlight and Control Center launch
///   other apps without first-use approval;
/// - `OpenApp` into an app outside the skill's apps (SpringBoard, the same as going home,
///   is allowed);
/// - `OpenUrl`, which can land in any app;
/// - `CallTool` when the tool's spec is unknown, when it acts in an app outside the skill's
///   apps, or when it declares no app and is not named in `skill.tools` (a tool without an
///   app can open links or act anywhere).
///
/// `PressHome`, `Wait`, `AskUser`, `Done` and `Fail` are always allowed. `OpenApp` must
/// carry a bundle id here (the agent resolves display names first); an unresolved name
/// simply does not match any app and is denied.
pub fn check_confinement(
    skill: Option<&SkillContext>,
    action: &Action,
    screen: &CompactScreen,
    tool: Option<&ToolSpec>,
) -> Verdict {
    let Some(skill) = skill.filter(|s| s.confine_to_apps) else {
        return Verdict::Allow;
    };
    let name = quote(&skill.name);
    match action {
        Action::Tap { .. }
        | Action::LongPress { .. }
        | Action::Type { .. }
        | Action::Clear { .. }
        | Action::Scroll { .. }
        | Action::Swipe { .. }
        | Action::TapPoint { .. }
        | Action::Look => {
            if in_skill_apps(skill, &screen.app_bundle_id) {
                Verdict::Allow
            } else if is_springboard(&screen.app_bundle_id) {
                Verdict::Deny(format!(
                    "the skill \"{name}\" does not act on the home screen; use open_app with \
                     one of its apps"
                ))
            } else {
                Verdict::Deny(format!(
                    "the skill \"{name}\" works only in its own apps, and {} is not one of them; \
                     open one of its apps or press home",
                    quote(&screen.app_bundle_id)
                ))
            }
        }
        Action::OpenApp { app } => {
            if in_skill_apps(skill, app) || is_springboard(app) {
                Verdict::Allow
            } else {
                Verdict::Deny(format!(
                    "the skill \"{name}\" works only in its own apps, and {} is not one of them",
                    quote(app)
                ))
            }
        }
        Action::OpenUrl { .. } => Verdict::Deny(format!(
            "the skill \"{name}\" works only in its own apps, and a link could open another app"
        )),
        Action::CallTool {
            name: tool_name, ..
        } => match tool.filter(|t| t.name == *tool_name) {
            None => Verdict::Deny(format!(
                "the skill \"{name}\" only calls tools it knows, and {} is unknown",
                quote(tool_name)
            )),
            Some(spec) => match spec.app.as_deref() {
                Some(app) if in_skill_apps(skill, app) => Verdict::Allow,
                Some(app) => Verdict::Deny(format!(
                    "the skill \"{name}\" works only in its own apps, and tool {} acts in {}",
                    quote(tool_name),
                    quote(app)
                )),
                None if skill.tools.iter().any(|t| t == tool_name) => Verdict::Allow,
                None => Verdict::Deny(format!(
                    "the skill \"{name}\" works only in its own apps, and tool {} is not tied \
                     to one of them nor allowed by the skill",
                    quote(tool_name)
                )),
            },
        },
        Action::PressHome
        | Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Done { .. }
        | Action::Fail { .. } => Verdict::Allow,
    }
}
