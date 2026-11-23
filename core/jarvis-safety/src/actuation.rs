//! Thirteenth review: what an action actuates — **one** classification of every action,
//! a `match` over [`Action`] with no wildcard arm, so a new variant does not compile until
//! it is decided here.
//!
//! The presumption that an action after typing submits a form
//! (`evaluate::presumed_submit`) and what the question says is sent
//! (`evaluate::what_is_sent`) both take their answer from [`actuation`]: a gesture that
//! can actuate a control is presumed and shows `Sends:` exactly like a tap. Before this,
//! each of them listed the action kinds it knew (taps, long presses, tap points), and a
//! "slide to continue" swipe after typing ran unasked.

use jarvis_proto::{Action, CompactScreen, Element};

use crate::classify::{drag_target, tap_point};
use crate::gestures::GestureKind;

/// What an action does to the page's controls.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Actuation<'a> {
    /// Presses this control: a tap, a long press, a `tap_point` resolved to an element.
    Press(&'a Element),
    /// Drags this control: a swipe or scroll whose element reacts to the drag or is
    /// button-like, or that crosses a control reacting to it, or that starts on a
    /// button-like control (`classify::drag_target`).
    Drag(&'a Element),
    /// Presses Return in this field (`type … submit`, or Return alone).
    Return(Option<u32>),
    /// Moves the page only: a drag over page content that starts on no actionable control
    /// and crosses none.
    Navigation,
    /// Actuates no control of the page: typing without Return, clearing a field, leaving
    /// the app, opening a link or an app, waiting, reading, asking, finishing, a tool call
    /// (each of these is classified on its own by the gate), or a target that does not
    /// exist (refused by validation).
    NoEffect,
}

/// The one classification of `action` on `screen` (see the module docs).
pub(crate) fn actuation<'a>(action: &Action, screen: &'a CompactScreen) -> Actuation<'a> {
    let press = |e: Option<&'a Element>| e.map_or(Actuation::NoEffect, Actuation::Press);
    let drag = |kind, direction, element: &Option<u32>| match drag_target(
        kind, direction, *element, screen,
    ) {
        Some(e) => Actuation::Drag(e),
        None => Actuation::Navigation,
    };
    match action {
        Action::Tap { element } | Action::LongPress { element, .. } => press(screen.get(*element)),
        Action::TapPoint { x, y } => press(tap_point(*x, *y, screen).1),
        Action::Swipe { direction, element } => drag(GestureKind::Swipe, *direction, element),
        Action::Scroll { direction, element } => drag(GestureKind::Scroll, *direction, element),
        Action::Type {
            element,
            submit: true,
            ..
        } => Actuation::Return(*element),
        Action::Type { submit: false, .. }
        | Action::Clear { .. }
        | Action::PressHome
        | Action::OpenApp { .. }
        | Action::OpenUrl { .. }
        | Action::CallTool { .. }
        | Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => Actuation::NoEffect,
    }
}
