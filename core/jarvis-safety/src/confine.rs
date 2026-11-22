//! Skill confinement: a session running a skill with `confine_to_apps` stays inside the
//! skill's apps.
//!
//! Confinement only narrows. It is checked in addition to every other gate (deny list,
//! trust levels, first-use approval, confirmations, budgets) and never replaces one: an
//! action it allows still goes through [`crate::evaluate`] and first-use approval.

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

/// Whether the agent may be in (or go to) `bundle_id` under the skill: one of its apps, or
/// SpringBoard (the home screen), which stays reachable for navigation.
fn allowed_place(skill: &SkillContext, bundle_id: &str) -> bool {
    is_springboard(bundle_id) || in_skill_apps(skill, bundle_id)
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
///   scroll, swipe, tap point, look) while the foreground app is neither one of the
///   skill's apps nor SpringBoard;
/// - `OpenApp` into an app outside the skill's apps (SpringBoard allowed);
/// - `OpenUrl`, which can land in any app;
/// - `CallTool` whose spec acts in an app outside the skill's apps (tools that declare no
///   app do not act inside an app and are not confined; they still pass every other gate).
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
            if allowed_place(skill, &screen.app_bundle_id) {
                Verdict::Allow
            } else {
                Verdict::Deny(format!(
                    "the skill \"{name}\" works only in its own apps, and {} is not one of them; \
                     open one of its apps or press home",
                    quote(&screen.app_bundle_id)
                ))
            }
        }
        Action::OpenApp { app } => {
            if allowed_place(skill, app) {
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
        } => {
            match tool
                .filter(|t| t.name == *tool_name)
                .and_then(|t| t.app.as_deref())
            {
                Some(app) if !in_skill_apps(skill, app) => Verdict::Deny(format!(
                    "the skill \"{name}\" works only in its own apps, and tool {} acts in {}",
                    quote(tool_name),
                    quote(app)
                )),
                _ => Verdict::Allow,
            }
        }
        Action::PressHome
        | Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Done { .. }
        | Action::Fail { .. } => Verdict::Allow,
    }
}
