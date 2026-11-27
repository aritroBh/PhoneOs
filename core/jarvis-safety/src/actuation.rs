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
    /// Fourteenth review (Rule E): presses something the screen does not list — a
    /// `tap_point` that resolves to no element (a web canvas, a control without an
    /// accessibility node). What it actuates is unknown: after typing, the whole screen is
    /// its form.
    PressUnknown,
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
        Action::TapPoint { x, y } => tap_point(*x, *y, screen)
            .1
            .map_or(Actuation::PressUnknown, Actuation::Press),
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

/// Stale-screen guard: whether `action` touches the page in front, so that a plan for it
/// made on an observation older than `Policy::max_plan_age_ms` is checked against a fresh
/// look before it runs. Every [`Actuation`] but [`Actuation::NoEffect`] (presses, point
/// presses, drags, page moves, Return), plus typing and clearing, which reach whatever
/// field has focus: the same `match` over [`Action`] as [`actuation`], no wildcard arm.
/// Leaving the app, opening a link or an app, waiting, reading, asking, finishing and tool
/// calls do not touch the page.
pub fn touches_screen(action: &Action) -> bool {
    match action {
        Action::Tap { .. }
        | Action::LongPress { .. }
        | Action::TapPoint { .. }
        | Action::Swipe { .. }
        | Action::Scroll { .. }
        | Action::Type { .. }
        | Action::Clear { .. } => true,
        Action::PressHome
        | Action::OpenApp { .. }
        | Action::OpenUrl { .. }
        | Action::CallTool { .. }
        | Action::Wait { .. }
        | Action::AskUser { .. }
        | Action::Look
        | Action::Done { .. }
        | Action::Fail { .. } => false,
    }
}
