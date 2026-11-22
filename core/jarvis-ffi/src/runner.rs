//! The device path: the runner's raw text tree, parsed in Rust.
//!
//! Behaviour when the text cannot be parsed at all (`parse_runner_text` found no node):
//! the event still becomes an `Observed` for the session, of an *empty screen* of the
//! reported app (an `Application` root covering the screen, labelled with the app name, no
//! children). That is the least surprising option given how `Session` treats observations:
//!
//! - The session keeps its normal path: the deny-list check still runs on the bundle id,
//!   time and step budgets still apply, and a vision-capable model gets a screenshot step
//!   (an empty tree is below the "too few actionable elements" threshold), which is exactly
//!   the fallback for a tree that is useless.
//! - A text-only model sees a screen with no elements and can `wait`, `look` or `fail`;
//!   stuck detection and the failure budget bound any loop.
//! - The alternatives are worse: finishing the session would turn one bad read into a
//!   failed task, re-emitting `Observe` without stepping the session would bypass its
//!   budgets, and reusing the previous screen would let the agent act on stale data.
//!
//! The parse problem is not lost: `JarvisSession.lastObservationIssue()` reports it (also
//! for partially parsed trees with skipped lines), so the shell can log it.

use jarvis_observe::{parse_runner_text, ParseError, ParsedTree};
use jarvis_proto as p;

use crate::error::JarvisError;
use crate::types::RunnerSnapshot;

/// The screen around a parsed (or empty) root, plus the parse result.
fn parse(snapshot: RunnerSnapshot) -> (p::Screen, Option<p::ImageRef>, Result<usize, ParseError>) {
    let RunnerSnapshot {
        source,
        app_bundle_id,
        app_name,
        size,
        keyboard_visible,
        orientation,
        screenshot,
    } = snapshot;
    let size: p::Size = size.into();
    let (root, parsed) = match parse_runner_text(&source) {
        Ok(ParsedTree { root, warnings }) => (root, Ok(warnings)),
        Err(e) => (empty_root(size, app_name.as_deref()), Err(e)),
    };
    let screen = p::Screen {
        app_bundle_id,
        app_name,
        size,
        root,
        keyboard_visible,
        orientation: orientation.into(),
    };
    (screen, screenshot.map(Into::into), parsed)
}

/// The root of an empty screen of the app.
fn empty_root(size: p::Size, app_name: Option<&str>) -> p::UiNode {
    let root = p::UiNode::new(p::Role::Application, size.to_rect());
    match app_name {
        Some(name) => root.with_label(name),
        None => root,
    }
}

/// The observation the session gets for a snapshot, and a note when parsing was not clean.
pub(crate) fn snapshot_to_observation(
    snapshot: RunnerSnapshot,
) -> (p::Observation, Option<String>) {
    let (screen, screenshot, parsed) = parse(snapshot);
    let issue = match parsed {
        Ok(0) => None,
        Ok(skipped) => Some(format!(
            "runner text: {skipped} unparseable line(s) skipped"
        )),
        Err(e) => Some(format!("runner text: {e}; observed as an empty screen")),
    };
    (p::Observation { screen, screenshot }, issue)
}

/// Strict variant for `parseRunnerTextToScreen`: no node at all is an error.
pub(crate) fn snapshot_to_screen(snapshot: RunnerSnapshot) -> Result<p::Screen, JarvisError> {
    let (screen, _, parsed) = parse(snapshot);
    match parsed {
        Ok(_) => Ok(screen),
        Err(e) => Err(JarvisError::RunnerText {
            message: e.to_string(),
        }),
    }
}
