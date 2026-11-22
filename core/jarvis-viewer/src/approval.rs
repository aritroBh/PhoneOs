//! The approval sheet's picture of a pending confirmation: which screen was evaluated
//! (named by its [`FrameKey`], so the shell can bind a real-screen frame to exactly that
//! observation), and where the control the action names is on it.
//!
//! Ground truth beats heuristics: the sheet shows the owner the screen Jarvis looked at with
//! the exact control it will act on outlined, beside the core's text question. Like the rest
//! of this crate it is a **display aid only**; the core alone decides what needs the owner's
//! OK and re-checks the screen after an approval.
//!
//! The target comes from the action the core proposed (`Progress::last_action`) and the
//! evaluated screen, indexed exactly as the core indexed it (`jarvis_observe::compact`): the
//! element's on-screen frame for `tap`, `long_press`, `type`, `clear` and an element
//! `scroll`/`swipe`; a point for `tap_point`; nothing (with a caption saying why) for
//! Return, keyboard and app-level actions, an element that cannot be found, or a frame that
//! is not on the screen. Never a box that might be wrong.
//!
//! Privacy: labels go through the core's redaction ([`LabelCache`]); on runner-text screens
//! the label comes only from the prompt the core rendered, as in the Live view. Nothing of
//! this is part of the timeline, so it is never encoded or streamed.

use jarvis_proto as p;

use crate::geometry::{ViewerPoint, ViewerRect, ViewerSize};
use crate::privacy::{self, LabelCache};
use crate::prompt::PromptElement;
use crate::reducer::{role_name, target_node};
use crate::wire::FrameKey;

/// What the approval sheet marks on the evaluated screen.
#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalMark {
    /// The target's frame, clipped to the screen: in screen points and in unit coordinates
    /// (0…1 of the screen's width and height).
    Outline {
        /// Screen points.
        frame: ViewerRect,
        /// Unit coordinates.
        unit: ViewerRect,
    },
    /// A `tap_point`: the point in screen points and in unit coordinates.
    Point {
        /// Screen points.
        point: ViewerPoint,
        /// Unit coordinates.
        unit: ViewerPoint,
    },
    /// Nothing to mark (the caption says why).
    Nothing,
}

/// The approval sheet's picture of one pending confirmation.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalTarget {
    /// The observation the confirmation was computed from (task id and the number of the
    /// effect that observed): a real-screen frame is shown only under exactly this key.
    pub frame_key: FrameKey,
    /// That screen in the timeline (its wireframe is the fallback backdrop).
    pub screen_id: u32,
    /// Its size in points, as reported.
    pub size: ViewerSize,
    /// The confirmation this picture belongs to (the sheet shows it only for this one).
    pub request: p::ConfirmRequest,
    /// What is marked.
    pub mark: ApprovalMark,
    /// One short line under the picture ("Jarvis will tap the outlined control").
    pub caption: String,
    /// For VoiceOver ("Outlined: Button 'Pay $512.40'").
    pub accessibility_label: String,
}

/// The evaluated screen, as the reducer holds it.
pub(crate) struct Evaluated<'a> {
    pub screen: &'a p::Screen,
    /// A typed tree (else parsed runner text: labels come from the prompt only).
    pub typed: bool,
    /// The prompt's element list for this screen.
    pub sight: &'a [PromptElement],
    /// The Return part of a `type … submit` is what is asked (the typing already ran).
    pub return_pending: bool,
}

/// The mark, caption and accessibility label for `action` on `evaluated`.
pub(crate) fn mark_for(
    evaluated: &Evaluated<'_>,
    action: Option<&p::Action>,
    labels: &mut LabelCache,
) -> (ApprovalMark, String, String) {
    let Some(action) = action else {
        return nothing("Jarvis named no control on this screen, so nothing is outlined.");
    };
    if evaluated.return_pending {
        return nothing(
            "Jarvis will press Return on the keyboard; there is no control to outline.",
        );
    }
    let size = ViewerSize::from(evaluated.screen.size);
    match action {
        p::Action::Tap { element } => element_mark(evaluated, *element, "tap", size, labels),
        p::Action::LongPress { element, .. } => {
            element_mark(evaluated, *element, "press and hold", size, labels)
        }
        p::Action::Clear { element } => element_mark(evaluated, *element, "clear", size, labels),
        p::Action::Type { element, .. } => match element.or_else(|| focused_index(evaluated)) {
            Some(index) => element_mark(evaluated, index, "type into", size, labels),
            None => nothing(
                "Jarvis will type into the focused field, which this screen does not show; nothing is outlined.",
            ),
        },
        p::Action::Scroll {
            element: Some(index),
            ..
        } => element_mark(evaluated, *index, "scroll in", size, labels),
        p::Action::Swipe {
            element: Some(index),
            ..
        } => element_mark(evaluated, *index, "swipe on", size, labels),
        p::Action::Scroll { element: None, .. } | p::Action::Swipe { element: None, .. } => {
            nothing("Jarvis will swipe across the whole screen; nothing is outlined.")
        }
        p::Action::TapPoint { x, y } => point_mark(evaluated, *x, *y, size, labels),
        p::Action::PressHome => {
            nothing("Jarvis will go to the home screen; nothing on this screen is tapped.")
        }
        p::Action::OpenApp { app } => {
            let name = labels.label(Some(app)).unwrap_or_else(|| "an app".to_owned());
            nothing(&format!(
                "Jarvis will open {name}; nothing on this screen is tapped."
            ))
        }
        p::Action::OpenUrl { url } => {
            let what = privacy::link_of(url).map_or_else(|| "a link".to_owned(), |l| l.to_string());
            nothing(&format!(
                "Jarvis will open {what}; nothing on this screen is tapped."
            ))
        }
        p::Action::CallTool { name, .. } => {
            let name = labels.label(Some(name)).unwrap_or_else(|| "a tool".to_owned());
            nothing(&format!(
                "Jarvis will use {name}; nothing on this screen is tapped."
            ))
        }
        _ => nothing("Jarvis named no control on this screen, so nothing is outlined."),
    }
}

fn nothing(caption: &str) -> (ApprovalMark, String, String) {
    (
        ApprovalMark::Nothing,
        caption.to_owned(),
        format!("Nothing outlined. {caption}"),
    )
}

/// The screen's usable size, or `None` when the reported one cannot place anything.
fn usable(size: ViewerSize) -> Option<ViewerSize> {
    size.is_usable().then_some(size)
}

/// The focused entry field of the compact screen (what `type` without an element types into).
fn focused_index(evaluated: &Evaluated<'_>) -> Option<u32> {
    jarvis_observe::compact(evaluated.screen)
        .elements
        .iter()
        .find(|e| e.focused)
        .map(|e| e.index)
}

fn element_mark(
    evaluated: &Evaluated<'_>,
    index: u32,
    verb: &str,
    size: ViewerSize,
    labels: &mut LabelCache,
) -> (ApprovalMark, String, String) {
    let compact = jarvis_observe::compact(evaluated.screen);
    let Some(element) = compact.elements.iter().find(|e| e.index == index) else {
        return nothing(&format!(
            "Jarvis could not find the control it will {verb} on this screen, so nothing is outlined."
        ));
    };
    let role = role_name(&element.role);
    let label = if evaluated.typed {
        labels
            .label(Some(&element.label))
            .or_else(|| labels.label(element.placeholder.as_deref()))
    } else {
        // Runner text: only what the core rendered in the prompt, as the Live view does.
        evaluated
            .sight
            .iter()
            .find(|e| e.index == index && e.role == role)
            .and_then(|e| labels.label(Some(&e.label)))
    };
    let described = match &label {
        Some(l) => format!("{role} '{l}'"),
        None => role.clone(),
    };
    let Some(size) = usable(size) else {
        return nothing(&format!(
            "The screen's size is unknown, so the {described} Jarvis will {verb} is not outlined."
        ));
    };
    let bounds = ViewerRect::new(0.0, 0.0, size.width, size.height);
    let frame = ViewerRect::from(element.frame);
    let Some(shown) = frame
        .intersection(&bounds)
        .filter(|r| r.is_finite() && r.has_area())
    else {
        return nothing(&format!(
            "The {described} Jarvis will {verb} is not on the visible screen, so nothing is outlined."
        ));
    };
    let unit = ViewerRect::new(
        shown.x / size.width,
        shown.y / size.height,
        shown.width / size.width,
        shown.height / size.height,
    );
    let what = if verb == "type into" || verb == "clear" {
        "field"
    } else if verb == "scroll in" || verb == "swipe on" {
        "area"
    } else {
        "control"
    };
    (
        ApprovalMark::Outline { frame: shown, unit },
        format!("Jarvis will {verb} the outlined {what}."),
        format!("Outlined: {described}"),
    )
}

fn point_mark(
    evaluated: &Evaluated<'_>,
    x: f64,
    y: f64,
    size: ViewerSize,
    labels: &mut LabelCache,
) -> (ApprovalMark, String, String) {
    let on_screen =
        x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y);
    if !on_screen {
        return nothing(
            "The point Jarvis will tap is not on the visible screen, so nothing is marked.",
        );
    }
    let Some(size) = usable(size) else {
        return nothing(
            "The screen's size is unknown, so the point Jarvis will tap is not marked.",
        );
    };
    let point = ViewerPoint::new(x * size.width, y * size.height);
    let under = if evaluated.typed {
        target_node(point, &evaluated.screen.root, size, labels).map(|(e, _)| match e.label {
            Some(l) => format!(" on {} '{l}'", e.role),
            None => format!(" on {}", e.role),
        })
    } else {
        None
    };
    let across = (x * 100.0).round();
    let down = (y * 100.0).round();
    (
        ApprovalMark::Point {
            point,
            unit: ViewerPoint::new(x, y),
        },
        "Jarvis will tap the marked point.".to_owned(),
        format!(
            "Marked: a point {across:.0}% across and {down:.0}% down{}",
            under.unwrap_or_default()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(role: p::Role, label: &str, frame: p::Rect) -> p::UiNode {
        let mut n = p::UiNode::new(role, frame);
        n.label = Some(label.to_owned());
        n
    }

    fn screen(children: Vec<p::UiNode>) -> p::Screen {
        let size = p::Size {
            width: 400.0,
            height: 800.0,
        };
        let mut root = p::UiNode::new(p::Role::Application, size.to_rect());
        root.children = children;
        p::Screen {
            app_bundle_id: "com.example.shop".to_owned(),
            app_name: Some("Shop".to_owned()),
            size,
            root,
            keyboard_visible: false,
            orientation: p::Orientation::Portrait,
        }
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> p::Rect {
        p::Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    fn eval(s: &p::Screen) -> Evaluated<'_> {
        Evaluated {
            screen: s,
            typed: true,
            sight: &[],
            return_pending: false,
        }
    }

    #[test]
    fn tap_outlines_the_element_in_unit_coordinates() {
        let s = screen(vec![node(
            p::Role::Button,
            "Pay $512.40",
            rect(100.0, 600.0, 200.0, 50.0),
        )]);
        let mut labels = LabelCache::new();
        let (mark, caption, a11y) =
            mark_for(&eval(&s), Some(&p::Action::Tap { element: 1 }), &mut labels);
        assert_eq!(
            mark,
            ApprovalMark::Outline {
                frame: ViewerRect::new(100.0, 600.0, 200.0, 50.0),
                unit: ViewerRect::new(0.25, 0.75, 0.5, 0.0625),
            }
        );
        assert_eq!(caption, "Jarvis will tap the outlined control.");
        assert_eq!(a11y, "Outlined: Button 'Pay $512.40'");
    }

    #[test]
    fn return_and_app_actions_mark_nothing() {
        let s = screen(vec![]);
        let mut labels = LabelCache::new();
        let mut e = eval(&s);
        e.return_pending = true;
        let typed = p::Action::Type {
            element: None,
            text: "x".to_owned(),
            submit: true,
        };
        let (mark, caption, _) = mark_for(&e, Some(&typed), &mut labels);
        assert_eq!(mark, ApprovalMark::Nothing);
        assert!(caption.contains("Return"), "{caption}");
        let (mark, caption, _) = mark_for(&eval(&s), Some(&p::Action::PressHome), &mut labels);
        assert_eq!(mark, ApprovalMark::Nothing);
        assert!(caption.contains("home screen"), "{caption}");
    }
}
