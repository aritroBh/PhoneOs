//! The approval sheet's picture of a pending confirmation: which screen was evaluated
//! (named by its [`FrameKey`], so the shell can bind a real-screen frame to exactly that
//! observation), and where the control the action names is on it.
//!
//! The sheet shows the owner a picture of the screen from when Jarvis asked, with the control
//! the proposed action names outlined, beside the core's text question. The outline is
//! computed from the same tree and indices the core used; the picture may have changed since,
//! and content outside the accessibility tree (images, canvas, video) is not checked at all.
//! Like the rest of this crate it is a **display aid only**; the core alone decides what needs
//! the owner's OK and re-checks the tree digest (never pixels) before it acts.
//!
//! What is asked and where is stated by the core in the request itself (`ConfirmRequest`'s
//! `stage`, `verb` and `target`, set by the session when it decides to ask; never read from
//! the question's text or the step history): the element's on-screen frame for `tap`,
//! `long_press`, `type`, `clear` and an element `scroll`/`swipe`, looked up by the index the
//! core resolved on the compact screen (`jarvis_observe::compact`); the point for
//! `tap_point`; nothing (with a caption saying why) for Return, keyboard and app-level
//! actions, an element that cannot be found, or a frame that is not on the screen. Never a
//! box that might be wrong: a request without a target (or without a stated kind), a target
//! on another screen (its digest differs from the evaluated screen's) or an element whose
//! frame is not the one the core resolved marks nothing, with the neutral
//! [`UNMARKED_CAPTION`].
//!
//! Privacy: labels go through the core's redaction ([`LabelCache`]); on runner-text screens
//! the label comes only from the prompt the core rendered, as in the Live view. Nothing of
//! this is part of the timeline, so it is never encoded or streamed.

use jarvis_proto as p;

use crate::geometry::{ViewerPoint, ViewerRect, ViewerSize};
use crate::privacy::LabelCache;
use crate::prompt::PromptElement;
use crate::reducer::role_name;
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

/// The caption when the target cannot be marked with certainty (preview review).
pub const UNMARKED_CAPTION: &str = "Jarvis could not mark the target; read the question.";

/// The tail of every caption of a mark: the picture is not live.
const PICTURE_AGE: &str = "The picture is from when Jarvis asked and may have changed.";

/// The evaluated screen, as the reducer holds it.
pub(crate) struct Evaluated<'a> {
    pub screen: &'a p::Screen,
    /// A typed tree (else parsed runner text: labels come from the prompt only).
    pub typed: bool,
    /// The prompt's element list for this screen.
    pub sight: &'a [PromptElement],
}

/// The mark, caption and accessibility label for the confirmation `request` on `evaluated`.
///
/// What is asked comes from the request alone, as the core stated it when it asked: the part
/// of the action (`stage`), its kind (`verb`) and its target (`target`, resolved by the core
/// on the screen it evaluated). Nothing is read from the question's text or the step
/// history. Fail closed: no target, a target on another screen (its digest differs from this
/// screen's) or an element whose frame is not the one the core resolved marks nothing, with
/// [`UNMARKED_CAPTION`].
pub(crate) fn mark_for(
    evaluated: &Evaluated<'_>,
    request: &p::ConfirmRequest,
    labels: &mut LabelCache,
) -> (ApprovalMark, String, String) {
    if request.stage == p::ConfirmStage::Return {
        return nothing(
            "Jarvis will press Return on the keyboard; there is no control to outline.",
        );
    }
    let verb = match request.verb {
        p::ConfirmVerb::Tap | p::ConfirmVerb::TapPoint => "tap",
        p::ConfirmVerb::LongPress => "press and hold",
        p::ConfirmVerb::Type => "type",
        p::ConfirmVerb::Clear => "clear",
        p::ConfirmVerb::Scroll => "scroll",
        p::ConfirmVerb::Swipe => "swipe",
        p::ConfirmVerb::PressHome => {
            return nothing("Jarvis will go to the home screen; nothing on this screen is tapped.")
        }
        p::ConfirmVerb::OpenApp => {
            let name = labels
                .label(Some(&request.app))
                .unwrap_or_else(|| "an app".to_owned());
            return nothing(&format!(
                "Jarvis will open {name}; nothing on this screen is tapped."
            ));
        }
        p::ConfirmVerb::OpenUrl => {
            return nothing("Jarvis will open a link; nothing on this screen is tapped.")
        }
        p::ConfirmVerb::CallTool => {
            return nothing("Jarvis will use a tool; nothing on this screen is tapped.")
        }
        p::ConfirmVerb::Other => return nothing(UNMARKED_CAPTION),
    };
    let Some(target) = &request.target else {
        return match request.verb {
            p::ConfirmVerb::Type => nothing(
                "Jarvis will type into the focused field, which this screen does not show; nothing is outlined.",
            ),
            p::ConfirmVerb::Scroll | p::ConfirmVerb::Swipe => {
                nothing("Jarvis will swipe across the whole screen; nothing is outlined.")
            }
            _ => nothing(UNMARKED_CAPTION),
        };
    };
    // The exact screen the core asked on (its raw digest), else nothing.
    if target.digest.is_empty()
        || jarvis_observe::screen_digest(evaluated.screen).to_hex() != target.digest
    {
        return nothing(UNMARKED_CAPTION);
    }
    let size = ViewerSize::from(evaluated.screen.size);
    if request.verb == p::ConfirmVerb::TapPoint {
        return match target.point {
            Some(point) => point_mark(point, size),
            None => nothing(UNMARKED_CAPTION),
        };
    }
    match target.element {
        Some(index) => element_mark(evaluated, index, target.frame, verb, size, labels),
        None => nothing(UNMARKED_CAPTION),
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

fn element_mark(
    evaluated: &Evaluated<'_>,
    index: u32,
    resolved: Option<p::Rect>,
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
    // The element the core resolved has exactly this frame, or nothing is marked.
    if resolved != Some(element.frame) {
        return nothing(UNMARKED_CAPTION);
    }
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
    (
        ApprovalMark::Outline { frame: shown, unit },
        format!("The outline is where Jarvis will {verb}. {PICTURE_AGE}"),
        format!("Outlined: {described}"),
    )
}

/// A `tap_point`: the point the core will touch (in screen points), only. The core resolves
/// a point to a control in more than one way (the gate's classifier takes the riskiest
/// pressable element under it, the approval's pin the smallest listed control), so it states
/// the point and no element is named here (preview review).
fn point_mark(point: p::Point, size: ViewerSize) -> (ApprovalMark, String, String) {
    let Some(size) = usable(size) else {
        return nothing(
            "The screen's size is unknown, so the point Jarvis will tap is not marked.",
        );
    };
    let x = point.x / size.width;
    let y = point.y / size.height;
    let on_screen =
        x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y);
    if !on_screen {
        return nothing(
            "The point Jarvis will tap is not on the visible screen, so nothing is marked.",
        );
    }
    let across = (x * 100.0).round();
    let down = (y * 100.0).round();
    (
        ApprovalMark::Point {
            point: ViewerPoint::new(point.x, point.y),
            unit: ViewerPoint::new(x, y),
        },
        format!("The mark is where Jarvis will tap. {PICTURE_AGE}"),
        format!("Marked: a point {across:.0}% across and {down:.0}% down. Jarvis will tap at the marked point."),
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
            tree_cut: false,
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
        }
    }

    /// The request the core sends for `verb` on `target` (element 1 of `s`).
    fn asked(s: &p::Screen, verb: p::ConfirmVerb, stage: p::ConfirmStage) -> p::ConfirmRequest {
        let element = jarvis_observe::compact(s).elements.into_iter().next();
        p::ConfirmRequest {
            verb,
            stage,
            target: element
                .map(|e| p::ConfirmTarget {
                    element: Some(e.index),
                    frame: Some(e.frame),
                    digest: jarvis_observe::screen_digest(s).to_hex(),
                    ..p::ConfirmTarget::default()
                })
                .map(Box::new),
            ..p::ConfirmRequest::default()
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
        let request = asked(&s, p::ConfirmVerb::Tap, p::ConfirmStage::Whole);
        let (mark, caption, a11y) = mark_for(&eval(&s), &request, &mut labels);
        assert_eq!(
            mark,
            ApprovalMark::Outline {
                frame: ViewerRect::new(100.0, 600.0, 200.0, 50.0),
                unit: ViewerRect::new(0.25, 0.75, 0.5, 0.0625),
            }
        );
        // Preview review: the caption says where, and that the picture is not live.
        assert_eq!(
            caption,
            "The outline is where Jarvis will tap. The picture is from when Jarvis asked and may have changed."
        );
        assert_eq!(a11y, "Outlined: Button 'Pay $512.40'");
    }

    #[test]
    fn return_and_app_actions_mark_nothing() {
        let s = screen(vec![]);
        let mut labels = LabelCache::new();
        let typed = asked(&s, p::ConfirmVerb::Type, p::ConfirmStage::Return);
        let (mark, caption, _) = mark_for(&eval(&s), &typed, &mut labels);
        assert_eq!(mark, ApprovalMark::Nothing);
        assert!(caption.contains("Return"), "{caption}");
        let home = asked(&s, p::ConfirmVerb::PressHome, p::ConfirmStage::Whole);
        let (mark, caption, _) = mark_for(&eval(&s), &home, &mut labels);
        assert_eq!(mark, ApprovalMark::Nothing);
        assert!(caption.contains("home screen"), "{caption}");
    }
}
