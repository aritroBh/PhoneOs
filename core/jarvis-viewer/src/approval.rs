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
//! The target comes from the action the core proposed (`Progress::last_action`) and the
//! evaluated screen, indexed exactly as the core indexed it (`jarvis_observe::compact`): the
//! element's on-screen frame for `tap`, `long_press`, `type`, `clear` and an element
//! `scroll`/`swipe`; a point for `tap_point`; nothing (with a caption saying why) for
//! Return, keyboard and app-level actions, an element that cannot be found, or a frame that
//! is not on the screen. Never a box that might be wrong: where the part of a `type … submit`
//! that is asked cannot be told for certain (`TypeStage::Unknown`), nothing is marked and
//! the caption is the neutral [`UNMARKED_CAPTION`].
//!
//! Privacy: labels go through the core's redaction ([`LabelCache`]); on runner-text screens
//! the label comes only from the prompt the core rendered, as in the Live view. Nothing of
//! this is part of the timeline, so it is never encoded or streamed.

use jarvis_proto as p;

use crate::geometry::{ViewerPoint, ViewerRect, ViewerSize};
use crate::privacy::{self, LabelCache};
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

/// Which part of a `type … submit` a confirmation asks about. The core gates such an action
/// in two parts (the typing, then Return). Until the core sends the gated stage with the
/// confirmation, the viewer tells them apart from the core's own summary only (preview
/// review; the step history is not used: a typing refused by a lock refunds its step number,
/// so "a typing in the same core step" does not mean the typing ran): `Type …` is the
/// typing, `Press Return …` the Return part, anything else (a first-use question, …) is
/// `Unknown` and nothing is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeStage {
    /// Not a `type … submit` (the action is asked as a whole).
    Whole,
    /// The typing part: the field is outlined.
    Typing,
    /// The Return part (the typing already ran): nothing to outline.
    Return,
    /// Cannot be told for certain: nothing is marked, the caption is neutral.
    Unknown,
}

impl TypeStage {
    /// The stage of `action` that `request` asks about (see [`TypeStage`]).
    pub(crate) fn of(action: Option<&p::Action>, request: &p::ConfirmRequest) -> TypeStage {
        if !matches!(action, Some(p::Action::Type { submit: true, .. })) {
            return TypeStage::Whole;
        }
        // The summary's head is written by the core (`jarvis_safety::summary_with`: "Type …
        // into …"; `return_summary`: "Press Return … in …"), behind the fixed warning on a
        // runner-text screen; on-screen text only ever follows the head.
        let summary = request.summary.as_str();
        let head = summary
            .strip_prefix(jarvis_safety::UNVERIFIED_SCREEN_WARNING)
            .unwrap_or(summary);
        if head.starts_with("Press Return") {
            TypeStage::Return
        } else if head.starts_with("Type ") {
            TypeStage::Typing
        } else {
            TypeStage::Unknown
        }
    }
}

/// The evaluated screen, as the reducer holds it.
pub(crate) struct Evaluated<'a> {
    pub screen: &'a p::Screen,
    /// A typed tree (else parsed runner text: labels come from the prompt only).
    pub typed: bool,
    /// The prompt's element list for this screen.
    pub sight: &'a [PromptElement],
    /// Which part of a `type … submit` is asked.
    pub stage: TypeStage,
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
    match evaluated.stage {
        TypeStage::Whole | TypeStage::Typing => {}
        TypeStage::Return => {
            return nothing(
                "Jarvis will press Return on the keyboard; there is no control to outline.",
            )
        }
        TypeStage::Unknown => return nothing(UNMARKED_CAPTION),
    }
    let size = ViewerSize::from(evaluated.screen.size);
    match action {
        p::Action::Tap { element } => element_mark(evaluated, *element, "tap", size, labels),
        p::Action::LongPress { element, .. } => {
            element_mark(evaluated, *element, "press and hold", size, labels)
        }
        p::Action::Clear { element } => element_mark(evaluated, *element, "clear", size, labels),
        p::Action::Type { element, .. } => match element.or_else(|| focused_index(evaluated)) {
            Some(index) => element_mark(evaluated, index, "type", size, labels),
            None => nothing(
                "Jarvis will type into the focused field, which this screen does not show; nothing is outlined.",
            ),
        },
        p::Action::Scroll {
            element: Some(index),
            ..
        } => element_mark(evaluated, *index, "scroll", size, labels),
        p::Action::Swipe {
            element: Some(index),
            ..
        } => element_mark(evaluated, *index, "swipe", size, labels),
        p::Action::Scroll { element: None, .. } | p::Action::Swipe { element: None, .. } => {
            nothing("Jarvis will swipe across the whole screen; nothing is outlined.")
        }
        p::Action::TapPoint { x, y } => point_mark(*x, *y, size),
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

/// The focused text input of the compact screen: what `type` without an element types into.
/// The same rule as the session's (`jarvis-agent` `session.rs`, `prepare` / `type_check` /
/// `action_target`: the first listed element that is `focused && role.is_text_input()`), so a
/// focused button never takes the outline from the field the text goes into (preview review).
/// No such field: nothing is outlined.
fn focused_index(evaluated: &Evaluated<'_>) -> Option<u32> {
    jarvis_observe::compact(evaluated.screen)
        .elements
        .iter()
        .find(|e| e.focused && e.role.is_text_input())
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
    (
        ApprovalMark::Outline { frame: shown, unit },
        format!("The outline is where Jarvis will {verb}. {PICTURE_AGE}"),
        format!("Outlined: {described}"),
    )
}

/// A `tap_point`: the point only. The core resolves a point to a control in more than one
/// way (the gate's classifier takes the riskiest pressable element under it, the approval's
/// pin the smallest listed control, `jarvis_safety::control_at`, on the session's own raw
/// tree), so no element is named here: never one the core might not resolve to (preview
/// review).
fn point_mark(x: f64, y: f64, size: ViewerSize) -> (ApprovalMark, String, String) {
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
    let across = (x * 100.0).round();
    let down = (y * 100.0).round();
    (
        ApprovalMark::Point {
            point,
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
            stage: TypeStage::Whole,
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
        let mut e = eval(&s);
        e.stage = TypeStage::Return;
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
