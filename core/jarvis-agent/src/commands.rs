//! Turning one UI action into the runner commands that carry it out.
//!
//! | Action | Commands |
//! |---|---|
//! | `tap` / `long_press` | `Tap` / `LongPress` at the element's tap point (below) |
//! | `type` | `Tap` the element (at its tap point) when it is not focused, `TypeText` (skipped for empty text), `PressKey Return` when `submit` (the session carries a submit out in two gated parts and never plans one whole) |
//! | `clear` | `Tap` near the field's trailing edge (caret to the end; another uncontested point of the field when that is covered), then `PressKey Delete` once per character of the current value (1..=[`MAX_CLEAR_DELETES`]); runners whose Delete clears the whole field just see a few extra no-op presses |
//! | `scroll` / `swipe` | one `Swipe` from [`jarvis_safety::gestures::drag_gesture`]; on an element, its start point must be uncontested; element-less while the keyboard is up, above it; never starting or ending on the keyboard (fourteenth review, Rule H: refused, "dismiss the keyboard first") |
//! | `tap_point` | `Tap` at the normalized point scaled by the screen size; never on the keyboard, and on a listed control only when nothing else contests it |
//!
//! **Tap points (sixth and seventh reviews).** Every point Jarvis taps or presses lands on
//! the control it names: it is inside the target's shown frame and inside no drawn raw node
//! that contests it (another control, enabled or not; the keyboard's keys and prediction
//! bar; labelled content drawn after the target) unless that node is the target's ancestor
//! or descendant in the raw tree, with full-width navigation and tab bars of a typed tree
//! drawn over the content under them ([`jarvis_safety::tap_spot`]: the centre, else the
//! first usable point of a fixed 5 × 5 grid). When every candidate is covered the action is
//! not planned ([`PlanError::Covered`]): the session refuses it ("not done: … is covered by
//! …"). A `tap_point` on the keyboard is refused ([`PlanError::Point`],
//! [`jarvis_safety::control_at`]); one on a listed control acts as that control.
//! | `press_home` | `PressKey Home` |
//! | `open_app` | `LaunchApp` (the bundle id is resolved before safety runs) |
//! | `open_url` / `wait` | `OpenUrl` / `Wait` at [`jarvis_safety::wait_level`] of the asked duration (1 000, 3 000 or 10 000 ms; twenty-second review, R22-A) |

use std::collections::VecDeque;

use jarvis_proto::{Action, AppInfo, CompactScreen, Key, Point, RunnerCommand};

use jarvis_safety::gestures::{
    denormalize, drag_gesture, GestureError, GestureKind, KEYBOARD_SHARE,
};
use jarvis_safety::{
    control_at, point_is_clear, tap_spot, wait_level, Covered, CoveredBy, PointError, ScreenTree,
};

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
    /// Every point that would press the target is covered by another control (or a raw
    /// point lands on two unrelated controls).
    Covered(Covered),
    /// A raw point (`tap_point`) on the keyboard, or on a listed control that another node
    /// contests.
    Point(PointError),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::NoElement(i) => write!(f, "element {i} is not on this screen"),
            PlanError::Gesture(g) => write!(f, "{g}"),
            PlanError::NotUi => write!(f, "this action does not use the screen"),
            PlanError::Covered(c) => match c.by {
                CoveredBy::Control(by) => {
                    write!(f, "element {} is covered by element {by}", c.target)
                }
                CoveredBy::Bar(bar) => {
                    write!(f, "element {} is covered by {}", c.target, bar.name())
                }
                CoveredBy::Keyboard => write!(f, "element {} is covered by the keyboard", c.target),
                CoveredBy::Node(n) => {
                    write!(f, "element {} is covered by raw node {n}", c.target)
                }
                CoveredBy::Inner(n) => {
                    write!(
                        f,
                        "element {} holds a control (raw node {n}) there",
                        c.target
                    )
                }
            },
            PlanError::Point(PointError::Keyboard) => write!(f, "the point is on the keyboard"),
            PlanError::Point(PointError::Covered(c)) => {
                write!(f, "{}", PlanError::Covered(*c))
            }
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
    RunnerCommand::Tap {
        x: p.x,
        y: p.y,
        expect: None,
    }
}

/// The runner commands for `action` on `screen`. `Type` text must already have its profile
/// placeholders substituted; `OpenApp` must carry a bundle id. `tree` (the raw tree's shape,
/// from `jarvis_observe::compact_view`) tells which controls hold one another, for the tap
/// point rule (see the module docs).
pub(crate) fn plan(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
) -> Result<VecDeque<RunnerCommand>, PlanError> {
    let el = |i: u32| screen.get(i).ok_or(PlanError::NoElement(i));
    let spot = |i: u32, preferred: Option<Point>| -> Result<Point, PlanError> {
        tap_spot(el(i)?, screen, tree, preferred).map_err(PlanError::Covered)
    };
    let mut q = VecDeque::new();
    match action {
        Action::Tap { element } => q.push_back(tap(spot(*element, None)?)),
        Action::LongPress {
            element,
            duration_ms,
        } => {
            let c = spot(*element, None)?;
            q.push_back(RunnerCommand::LongPress {
                x: c.x,
                y: c.y,
                duration_ms: *duration_ms,
                expect: None,
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
                    q.push_back(tap(spot(*i, None)?));
                }
            }
            if !text.is_empty() {
                q.push_back(RunnerCommand::TypeText {
                    text: text.clone(),
                    expect: None,
                });
            }
            if *submit {
                q.push_back(RunnerCommand::PressKey {
                    key: Key::Return,
                    expect: None,
                });
            }
        }
        Action::Clear { element } => {
            let e = el(*element)?;
            let f = e.frame;
            let inset = CLEAR_TAP_INSET.min(f.width / 4.0);
            let trailing = Point::new(f.max_x() - inset, f.center().y);
            q.push_back(tap(spot(*element, Some(trailing))?));
            let n = e
                .value
                .as_deref()
                .map_or(1, |v| v.chars().count())
                .clamp(1, MAX_CLEAR_DELETES);
            for _ in 0..n {
                q.push_back(RunnerCommand::PressKey {
                    key: Key::Delete,
                    expect: None,
                });
            }
        }
        Action::Scroll { direction, element } | Action::Swipe { direction, element } => {
            let kind = if matches!(action, Action::Scroll { .. }) {
                GestureKind::Scroll
            } else {
                GestureKind::Swipe
            };
            let target = match element {
                Some(i) => Some(el(*i)?),
                None => None,
            };
            let g = drag_gesture(kind, *direction, target.map(|t| t.frame), screen)
                .map_err(PlanError::Gesture)?;
            // Fourteenth review (Rule H): no drag starts or ends on the keyboard (a key's
            // drag enters or commits text). An element-less drag is planned above it; any
            // drag that would still touch it is refused with a reason the model sees.
            if on_keyboard(g.from, screen, tree) || on_keyboard(g.to, screen, tree) {
                return Err(PlanError::Gesture(GestureError::OnKeyboard));
            }
            // A drag on an element starts on it alone (the path is what the classifier read,
            // so the start is not moved).
            if let Some(t) = target {
                point_is_clear(g.from, t, screen, tree).map_err(PlanError::Covered)?;
            }
            q.push_back(RunnerCommand::Swipe {
                from: g.from,
                to: g.to,
                duration_ms: g.duration_ms,
                expect: None,
            });
        }
        Action::TapPoint { x, y } => {
            let p = denormalize(*x, *y, screen.size);
            control_at(p, screen, tree).map_err(PlanError::Point)?;
            q.push_back(tap(p));
        }
        Action::PressHome => q.push_back(RunnerCommand::PressKey {
            key: Key::Home,
            expect: None,
        }),
        Action::OpenApp { app } => q.push_back(RunnerCommand::LaunchApp {
            bundle_id: app.clone(),
        }),
        Action::OpenUrl { url } => q.push_back(RunnerCommand::OpenUrl { url: url.clone() }),
        // Twenty-second review (R22-A): a wait is sent at one of three fixed levels, never
        // with the duration asked (the session levels the action itself first, so what it
        // shows and records is what is sent; this is the point every wait passes).
        Action::Wait { ms } => q.push_back(RunnerCommand::Wait {
            ms: wait_level(*ms),
        }),
        Action::CallTool { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => return Err(PlanError::NotUi),
    }
    Ok(q)
}

/// Whether `p` lies on the keyboard (fourteenth review, Rule H): inside a drawn keyboard
/// node of the raw tree (its keys, its prediction bar; audit C4: the `Keyboard` node is
/// drawn over the keyboard's whole area, the Emoji and Dictate row under the keys
/// included), or, while a keyboard is up and the tree shows none, in the bottom
/// [`KEYBOARD_SHARE`] of the screen.
fn on_keyboard(p: Point, screen: &CompactScreen, tree: Option<&ScreenTree<'_>>) -> bool {
    let mut drawn = tree
        .map(|t| t.nodes)
        .unwrap_or_default()
        .iter()
        .filter(|n| n.keyboard && !n.frame.is_empty())
        .peekable();
    if drawn.peek().is_some() {
        return drawn.any(|n| n.frame.contains(p));
    }
    screen.keyboard_visible && p.y >= screen.size.height * (1.0 - KEYBOARD_SHARE)
}

/// Bundle id of the home screen.
pub(crate) const SPRINGBOARD: &str = jarvis_proto::SPRINGBOARD_BUNDLE_ID;

/// What a home-screen action may launch.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HomeLaunch {
    /// An icon, widget or search result named like a known app.
    App {
        /// The app's bundle id.
        bundle_id: String,
        /// The label that was tapped.
        label: String,
        /// The element is a widget (its own label names the app it belongs to).
        widget: bool,
    },
    /// Something tappable that is not a known app (no label, or a name Jarvis does not
    /// know): it may launch anything.
    Unknown {
        /// The label that was tapped (empty when unlabeled).
        label: String,
        /// The element is a widget (its own label names the app it belongs to).
        widget: bool,
    },
    /// Return in the home screen's search field opens the top result.
    Search {
        /// The text searched for.
        text: String,
    },
    /// A `tap_point` that hits no listed element, or lands in the keyboard's area while a
    /// keyboard is up (keys are not listed; its Return key opens Spotlight's top hit): what
    /// it launches cannot be named, so it is never sent unasked.
    Unresolved {
        /// The point, normalised (0..1).
        x: f64,
        /// The point, normalised (0..1).
        y: f64,
        /// It lands in the keyboard's area.
        keyboard: bool,
    },
}

/// Share of the screen height, from the bottom, treated as the keyboard's area while a
/// keyboard is up: a tap there may press a key (Return opens Spotlight's top hit) whatever
/// listed element lies under it. Generous on purpose (the iPhone keyboard takes about 40 %
/// in portrait and more in landscape).
pub(crate) const KEYBOARD_AREA_SHARE: f64 = 0.5;

/// Whether `el` is a home-screen or Today-view widget: SpringBoard gives widgets the value
/// "Widget" ("Widget, Stack"); a label ending in "widget" counts too.
pub(crate) fn is_widget(el: &jarvis_proto::Element) -> bool {
    let value = el.value.as_deref().unwrap_or("").trim().to_lowercase();
    value == "widget"
        || value.starts_with("widget,")
        || el.label.trim().to_lowercase().ends_with(" widget")
}

/// Home-screen labels that open SpringBoard's own UI, never an app: the search pill and
/// folders (VoiceOver names them `<Name> folder`).
fn opens_springboard_ui(label: &str) -> bool {
    let l = label.trim().to_lowercase();
    matches!(l.as_str(), "search" | "spotlight search") || l.ends_with(" folder")
}

/// The known app a home-screen label names, by display name (case-insensitive, trimmed): the
/// whole label only. Nineteenth review (R-E): the first comma-separated part alone does not
/// name an app ("Settings, Website, settings-help.example" is a website row a site titled
/// "Settings"). Audit of real trees (C8): the suffixes once accepted after the name — `,
/// App` (a Spotlight app result) and an icon's badge `, <n> unread` / `, <n>
/// notification(s)` — occur in no real tree: Spotlight's app row is `Cell
/// id=Identifier:GridCell` holding `Icon "Settings"` (decided by identifiers,
/// [`spotlight_launch`]), and a real Home Screen icon keeps its label the app's name and
/// puts what it shows in its value (`Button "Calendar" value="Sunday, November 22"`). A
/// label with any suffix is an unknown launch (it asks). The denied-app checks keep their
/// own, broader name rule (`jarvis_safety::label_names_denied_app`).
fn known_app_named<'a>(label: &str, known_apps: &'a [AppInfo]) -> Option<&'a AppInfo> {
    let want = label.trim();
    if want.is_empty() {
        return None;
    }
    known_apps
        .iter()
        .find(|a| a.name.trim().eq_ignore_ascii_case(want))
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
/// A tap, long press or tap_point on an element is resolved by label (a long press on a
/// known app's icon only for the deny and decline checks — its menu offers the app's quick
/// actions — and never as its launch, [`launches`]): a known app's display name (the whole
/// label, `known_app_named`) is [`HomeLaunch::App`]; the search pill and folders open
/// SpringBoard's own UI and launch nothing (`None`); anything else, unlabeled included, is
/// [`HomeLaunch::Unknown`]. A tap_point on no listed element, or in the keyboard's area
/// while a keyboard is up, is [`HomeLaunch::Unresolved`]. Return in a search field (`type`
/// with `submit`) is [`HomeLaunch::Search`]. Text inputs only take focus (`None`). Other
/// actions launch nothing from here (`open_app` has its own checks).
pub(crate) fn home_launch(
    action: &Action,
    screen: &CompactScreen,
    known_apps: &[AppInfo],
) -> Option<HomeLaunch> {
    let target = match action {
        Action::Tap { element } | Action::LongPress { element, .. } => screen.get(*element)?,
        Action::TapPoint { x, y } => {
            let keyboard =
                screen.keyboard_visible && y.is_finite() && *y >= 1.0 - KEYBOARD_AREA_SHARE;
            match element_at(*x, *y, screen) {
                Some(e) if !keyboard => e,
                _ => {
                    return Some(HomeLaunch::Unresolved {
                        x: *x,
                        y: *y,
                        keyboard,
                    })
                }
            }
        }
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
    let widget = is_widget(target);
    Some(match known_app_named(&target.label, known_apps) {
        Some(app) => HomeLaunch::App {
            bundle_id: app.bundle_id.clone(),
            label: target.label.clone(),
            widget,
        },
        None => HomeLaunch::Unknown {
            label: target.label.clone(),
            widget,
        },
    })
}

/// Top layer (probe runs 134 and 135): what `action` on a Spotlight screen
/// (`com.apple.Spotlight`, its own process on iOS 26) may launch, decided by the OS's
/// identifiers, never by a row's label: a press inside an app result row
/// (`jarvis_safety::grid_cell`, `Identifier:GridCell`) whose `Icon` is labelled with a known
/// app's display name (whole, trimmed, case-insensitive; `spotlight_app_icon`) is a launch of
/// that app ([`HomeLaunch::App`], the home-launch exemption applies as for a Home Screen
/// icon); a press on the search field or inside it (its clear button) launches nothing
/// (`None`: an input into Spotlight); every other press — a website row (`ResultCell`), the
/// go-to-web row (`PunchOutToWebSuggestionCell`), a suggestion or shortcut row, a section's
/// button, a key, anything unknown — is [`HomeLaunch::Unknown`] (an input into an
/// undecidable destination). A `tap_point` on no listed element or in the keyboard's area is
/// [`HomeLaunch::Unresolved`]; Return in the field (`type` with `submit`) is
/// [`HomeLaunch::Search`]. The label rule of the Home Screen ([`home_launch`]:
/// `known_app_named`) is not used here.
pub(crate) fn spotlight_launch(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    known_apps: &[AppInfo],
) -> Option<HomeLaunch> {
    let target = match action {
        Action::Tap { element } | Action::LongPress { element, .. } => screen.get(*element)?,
        Action::TapPoint { x, y } => {
            let keyboard =
                screen.keyboard_visible && y.is_finite() && *y >= 1.0 - KEYBOARD_AREA_SHARE;
            match element_at(*x, *y, screen) {
                Some(e) if !keyboard => e,
                _ => {
                    return Some(HomeLaunch::Unresolved {
                        x: *x,
                        y: *y,
                        keyboard,
                    })
                }
            }
        }
        Action::Type {
            text, submit: true, ..
        } => {
            return Some(HomeLaunch::Search { text: text.clone() });
        }
        _ => return None,
    };
    if jarvis_safety::inside_text_input(target, tree) {
        return None;
    }
    let app = jarvis_safety::spotlight_app_icon(target, screen, tree).and_then(|icon| {
        let icon = icon.trim().to_owned();
        known_apps
            .iter()
            .find(|a| !icon.is_empty() && a.name.trim().eq_ignore_ascii_case(&icon))
            .map(|a| (a, icon))
    });
    Some(match app {
        Some((app, icon)) => HomeLaunch::App {
            bundle_id: app.bundle_id.clone(),
            label: icon,
            widget: false,
        },
        None => HomeLaunch::Unknown {
            label: target.label.clone(),
            widget: false,
        },
    })
}

/// Top layer: what `action` on `screen` may launch — [`home_launch`] on the Home Screen
/// (SpringBoard), [`spotlight_launch`] in Spotlight, `None` anywhere else.
pub(crate) fn launcher_launch(
    action: &Action,
    screen: &CompactScreen,
    tree: Option<&ScreenTree<'_>>,
    known_apps: &[AppInfo],
) -> Option<HomeLaunch> {
    if jarvis_safety::is_spotlight(&screen.app_bundle_id) {
        spotlight_launch(action, screen, tree, known_apps)
    } else if jarvis_safety::is_springboard(&screen.app_bundle_id) {
        home_launch(action, screen, known_apps)
    } else {
        None
    }
}

/// Twenty-sixth review (L1): whether `action`, resolved by [`home_launch`] to a known app,
/// launches it. A long press on an icon opens SpringBoard's icon menu and puts nothing into
/// the app: it is an input into the Home Screen like any other long press there (the tap on
/// one of the menu's quick actions is the launch). A long press on something Jarvis cannot
/// name keeps its undecidable destination.
pub(crate) fn launches(action: &Action) -> bool {
    !matches!(action, Action::LongPress { .. })
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
            // Sixth review: the two fields no longer share one frame (a tap on either would
            // be covered by the other).
            frame: Rect::new(20.0, 140.0 + 60.0 * f64::from(index), 300.0, 40.0),
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
        let q: Vec<_> = plan(&a, &screen(), None).unwrap().into_iter().collect();
        assert_eq!(
            q,
            vec![
                RunnerCommand::Tap {
                    x: 170.0,
                    y: 220.0,
                    expect: None
                },
                RunnerCommand::TypeText {
                    text: "hi".into(),
                    expect: None
                },
                RunnerCommand::PressKey {
                    key: Key::Return,
                    expect: None
                },
            ]
        );
        let focused = Action::Type {
            element: Some(2),
            text: "hi".into(),
            submit: false,
        };
        assert_eq!(plan(&focused, &screen(), None).unwrap().len(), 1);
    }

    #[test]
    fn clear_taps_trailing_edge_then_deletes_per_char() {
        let q: Vec<_> = plan(&Action::Clear { element: 2 }, &screen(), None)
            .unwrap()
            .into_iter()
            .collect();
        assert_eq!(q.len(), 4);
        assert_eq!(
            q[0],
            RunnerCommand::Tap {
                x: 312.0,
                y: 280.0,
                expect: None
            }
        );
        assert!(q[1..].iter().all(|c| *c
            == RunnerCommand::PressKey {
                key: Key::Delete,
                expect: None
            }));
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
