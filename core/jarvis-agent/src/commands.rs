//! Turning one UI action into the runner commands that carry it out.
//!
//! | Action | Commands |
//! |---|---|
//! | `tap` / `long_press` | `Tap` / `LongPress` at the element centre |
//! | `type` | `Tap` the element when it is not focused, `TypeText` (skipped for empty text), `PressKey Return` when `submit` |
//! | `clear` | `Tap` near the field's trailing edge (caret to the end), then `PressKey Delete` once per character of the current value (1..=[`MAX_CLEAR_DELETES`]); runners whose Delete clears the whole field just see a few extra no-op presses |
//! | `scroll` / `swipe` | one `Swipe` from [`crate::gestures`] |
//! | `tap_point` | `Tap` at the normalized point scaled by the screen size |
//! | `press_home` | `PressKey Home` |
//! | `open_app` | `LaunchApp` (the bundle id is resolved before safety runs) |
//! | `open_url` / `wait` | `OpenUrl` / `Wait` |

use std::collections::VecDeque;

use jarvis_proto::{Action, AppInfo, CompactScreen, Key, Point, RunnerCommand};

use crate::gestures::{denormalize, gesture, GestureError, GestureKind};

/// Most Delete presses a `clear` sends.
pub const MAX_CLEAR_DELETES: usize = 64;
/// Points in from the trailing edge where `clear` taps to put the caret at the end.
const CLEAR_TAP_INSET: f64 = 8.0;

/// Why an action could not be turned into commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlanError {
    /// The element is not on the screen.
    NoElement(u32),
    /// A gesture could not be placed.
    Gesture(GestureError),
    /// The action is not a runner action.
    NotUi,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::NoElement(i) => write!(f, "element {i} is not on this screen"),
            PlanError::Gesture(g) => write!(f, "{g}"),
            PlanError::NotUi => write!(f, "this action does not use the screen"),
        }
    }
}

/// How `open_app`'s target was understood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AppTarget {
    /// A known or bundle-id-shaped app.
    Bundle(String),
    /// Neither a known display name nor a bundle id.
    Unknown,
}

/// Whether `s` looks like a reverse-DNS bundle id (`com.example.app`).
fn looks_like_bundle_id(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// Resolves `app` (display name or bundle id) through `known_apps`, case-insensitively;
/// an unknown name that is shaped like a bundle id is accepted as one.
pub(crate) fn resolve_app(app: &str, known_apps: &[AppInfo]) -> AppTarget {
    let want = app.trim();
    if want.is_empty() {
        return AppTarget::Unknown;
    }
    if let Some(a) = known_apps
        .iter()
        .find(|a| a.bundle_id.eq_ignore_ascii_case(want))
        .or_else(|| {
            known_apps
                .iter()
                .find(|a| a.name.trim().eq_ignore_ascii_case(want))
        })
        .or_else(|| {
            known_apps
                .iter()
                .find(|a| a.name.trim().to_lowercase() == want.to_lowercase())
        })
    {
        return AppTarget::Bundle(a.bundle_id.clone());
    }
    if looks_like_bundle_id(want) {
        AppTarget::Bundle(want.to_owned())
    } else {
        AppTarget::Unknown
    }
}

fn tap(p: Point) -> RunnerCommand {
    RunnerCommand::Tap { x: p.x, y: p.y }
}

/// The runner commands for `action` on `screen`. `Type` text must already have its profile
/// placeholders substituted; `OpenApp` must carry a bundle id.
pub(crate) fn plan(
    action: &Action,
    screen: &CompactScreen,
) -> Result<VecDeque<RunnerCommand>, PlanError> {
    let el = |i: u32| screen.get(i).ok_or(PlanError::NoElement(i));
    let mut q = VecDeque::new();
    match action {
        Action::Tap { element } => q.push_back(tap(el(*element)?.frame.center())),
        Action::LongPress {
            element,
            duration_ms,
        } => {
            let c = el(*element)?.frame.center();
            q.push_back(RunnerCommand::LongPress {
                x: c.x,
                y: c.y,
                duration_ms: *duration_ms,
            });
        }
        Action::Type {
            element,
            text,
            submit,
        } => {
            if let Some(i) = element {
                let e = el(*i)?;
                if !e.focused {
                    q.push_back(tap(e.frame.center()));
                }
            }
            if !text.is_empty() {
                q.push_back(RunnerCommand::TypeText { text: text.clone() });
            }
            if *submit {
                q.push_back(RunnerCommand::PressKey { key: Key::Return });
            }
        }
        Action::Clear { element } => {
            let e = el(*element)?;
            let f = e.frame;
            let inset = CLEAR_TAP_INSET.min(f.width / 4.0);
            q.push_back(tap(Point::new(f.max_x() - inset, f.center().y)));
            let n = e
                .value
                .as_deref()
                .map_or(1, |v| v.chars().count())
                .clamp(1, MAX_CLEAR_DELETES);
            for _ in 0..n {
                q.push_back(RunnerCommand::PressKey { key: Key::Delete });
            }
        }
        Action::Scroll { direction, element } | Action::Swipe { direction, element } => {
            let kind = if matches!(action, Action::Scroll { .. }) {
                GestureKind::Scroll
            } else {
                GestureKind::Swipe
            };
            let frame = match element {
                Some(i) => Some(el(*i)?.frame),
                None => None,
            };
            let g = gesture(kind, *direction, frame, screen.size).map_err(PlanError::Gesture)?;
            q.push_back(RunnerCommand::Swipe {
                from: g.from,
                to: g.to,
                duration_ms: g.duration_ms,
            });
        }
        Action::TapPoint { x, y } => q.push_back(tap(denormalize(*x, *y, screen.size))),
        Action::PressHome => q.push_back(RunnerCommand::PressKey { key: Key::Home }),
        Action::OpenApp { app } => q.push_back(RunnerCommand::LaunchApp {
            bundle_id: app.clone(),
        }),
        Action::OpenUrl { url } => q.push_back(RunnerCommand::OpenUrl { url: url.clone() }),
        Action::Wait { ms } => q.push_back(RunnerCommand::Wait { ms: *ms }),
        Action::CallTool { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => return Err(PlanError::NotUi),
    }
    Ok(q)
}

/// Bundle id of the home screen.
pub(crate) const SPRINGBOARD: &str = jarvis_proto::SPRINGBOARD_BUNDLE_ID;

/// What a home-screen action may launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HomeLaunch {
    /// An icon, widget or search result named like a known app.
    App {
        /// The app's bundle id.
        bundle_id: String,
        /// The label that was tapped.
        label: String,
    },
    /// Something tappable that is not a known app (no label, or a name Jarvis does not
    /// know): it may launch anything.
    Unknown {
        /// The label that was tapped (empty when unlabeled).
        label: String,
    },
    /// Return in the home screen's search field opens the top result.
    Search {
        /// The text searched for.
        text: String,
    },
}

/// Home-screen labels that open SpringBoard's own UI, never an app: the search pill and
/// folders (VoiceOver names them `<Name> folder`).
fn opens_springboard_ui(label: &str) -> bool {
    let l = label.trim().to_lowercase();
    matches!(l.as_str(), "search" | "spotlight search") || l.ends_with(" folder")
}

/// The known app a home-screen label names: the whole label, else its first
/// comma-separated part ("Messages, 2 unread"), by display name (case-insensitive).
fn known_app_named<'a>(label: &str, known_apps: &'a [AppInfo]) -> Option<&'a AppInfo> {
    let first = label.split(',').next().unwrap_or("");
    [label, first].into_iter().find_map(|want| {
        let want = want.trim();
        (!want.is_empty()).then(|| {
            known_apps
                .iter()
                .find(|a| a.name.trim().eq_ignore_ascii_case(want))
        })?
    })
}

/// The element a normalized point lands on: the smallest one containing it.
fn element_at(x: f64, y: f64, screen: &CompactScreen) -> Option<&jarvis_proto::Element> {
    let p = denormalize(x, y, screen.size);
    screen
        .elements
        .iter()
        .filter(|e| e.frame.contains(p))
        .min_by(|a, b| a.frame.area().total_cmp(&b.frame.area()))
}

/// What `action` on the home screen `screen` may launch, so the agent can give it the
/// checks `open_app` gets before anything is sent.
///
/// A tap, long press (its menu offers the app's quick actions) or tap_point on an element
/// is resolved by label: a known app's display name (the label, or its first
/// comma-separated part) is [`HomeLaunch::App`]; the search pill and folders open
/// SpringBoard's own UI and launch nothing (`None`); anything else, unlabeled included, is
/// [`HomeLaunch::Unknown`]. Return in a search field (`type` with `submit`) is
/// [`HomeLaunch::Search`]. Text inputs only take focus (`None`). Other actions launch
/// nothing from here (`open_app` has its own checks).
pub(crate) fn home_launch(
    action: &Action,
    screen: &CompactScreen,
    known_apps: &[AppInfo],
) -> Option<HomeLaunch> {
    let target = match action {
        Action::Tap { element } | Action::LongPress { element, .. } => screen.get(*element)?,
        Action::TapPoint { x, y } => element_at(*x, *y, screen)?,
        Action::Type {
            text, submit: true, ..
        } => {
            return Some(HomeLaunch::Search { text: text.clone() });
        }
        _ => return None,
    };
    if target.role.is_text_input() || opens_springboard_ui(&target.label) {
        return None;
    }
    Some(match known_app_named(&target.label, known_apps) {
        Some(app) => HomeLaunch::App {
            bundle_id: app.bundle_id.clone(),
            label: target.label.clone(),
        },
        None => HomeLaunch::Unknown {
            label: target.label.clone(),
        },
    })
}

/// Whether executing `action` on `screen` should visibly change something; when it should
/// and verification sees no change, the step counts as a failure.
///
/// * `wait` never has to change anything (the UI may already be settled);
/// * `open_app` of the app already in front, and `press_home` on the home screen, are no-ops;
/// * `clear` of an already empty field is a no-op;
/// * everything else that touches the screen must change it (a scroll that changes nothing
///   has reached the end, which the model should be told).
pub(crate) fn should_change(action: &Action, screen: &CompactScreen) -> bool {
    match action {
        Action::Wait { .. } => false,
        Action::OpenApp { app } => !app.eq_ignore_ascii_case(&screen.app_bundle_id),
        Action::PressHome => screen.app_bundle_id != SPRINGBOARD,
        Action::Clear { element } => screen
            .get(*element)
            .is_some_and(|e| e.value.as_deref().is_some_and(|v| !v.is_empty())),
        a => a.is_ui(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvis_proto::{Element, Rect, Role, Size};

    fn screen() -> CompactScreen {
        let field = |index, focused, value: Option<&str>| Element {
            index,
            role: Role::TextField,
            label: "Email".into(),
            value: value.map(str::to_owned),
            placeholder: None,
            frame: Rect::new(20.0, 200.0, 300.0, 40.0),
            enabled: true,
            focused,
            selected: false,
            secure: false,
            fingerprint: u64::from(index),
        };
        CompactScreen {
            app_bundle_id: "com.example".into(),
            app_name: None,
            size: Size::new(400.0, 800.0),
            keyboard_visible: false,
            elements: vec![field(1, false, None), field(2, true, Some("abc"))],
            cues: Default::default(),
        }
    }

    #[test]
    fn type_into_unfocused_field_taps_first_and_submits() {
        let a = Action::Type {
            element: Some(1),
            text: "hi".into(),
            submit: true,
        };
        let q: Vec<_> = plan(&a, &screen()).unwrap().into_iter().collect();
        assert_eq!(
            q,
            vec![
                RunnerCommand::Tap { x: 170.0, y: 220.0 },
                RunnerCommand::TypeText { text: "hi".into() },
                RunnerCommand::PressKey { key: Key::Return },
            ]
        );
        let focused = Action::Type {
            element: Some(2),
            text: "hi".into(),
            submit: false,
        };
        assert_eq!(plan(&focused, &screen()).unwrap().len(), 1);
    }

    #[test]
    fn clear_taps_trailing_edge_then_deletes_per_char() {
        let q: Vec<_> = plan(&Action::Clear { element: 2 }, &screen())
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(q.len(), 4);
        assert_eq!(q[0], RunnerCommand::Tap { x: 312.0, y: 220.0 });
        assert!(q[1..]
            .iter()
            .all(|c| *c == RunnerCommand::PressKey { key: Key::Delete }));
        assert!(!should_change(&Action::Clear { element: 1 }, &screen()));
        assert!(should_change(&Action::Clear { element: 2 }, &screen()));
    }

    #[test]
    fn resolves_apps() {
        let apps = vec![AppInfo::new("com.apple.Preferences", "Settings")];
        assert_eq!(
            resolve_app(" settings ", &apps),
            AppTarget::Bundle("com.apple.Preferences".into())
        );
        assert_eq!(
            resolve_app("COM.APPLE.PREFERENCES", &apps),
            AppTarget::Bundle("com.apple.Preferences".into())
        );
        assert_eq!(
            resolve_app("com.example.other", &apps),
            AppTarget::Bundle("com.example.other".into())
        );
        assert_eq!(resolve_app("Mail", &apps), AppTarget::Unknown);
        assert_eq!(resolve_app("", &apps), AppTarget::Unknown);
    }
}
