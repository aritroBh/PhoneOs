//! The Live view's pure logic: a viewer of the phone screen with an animated cursor showing
//! each tap, swipe and keystroke of the agent, live and as a replay.
//!
//! Sans-IO and deterministic like the rest of the core: no clocks (time comes in as
//! milliseconds on any monotonic clock), no threads, no pixels (real-screen frames stay in
//! the shell, named by a [`FrameKey`]).
//!
//! - [`reducer`]: the driver's updates (core types: `Effect`, `Event`, `Progress`, the
//!   prompt text, run start and end) → [`ViewerTimeline`], with the privacy rules.
//! - [`privacy`]: what may be shown of labels, typed text and links; calls
//!   `jarvis_safety` directly.
//! - [`wire`]: the timeline types and their versioned JSON wire format, streamed to a
//!   paired remote controller.
//! - [`cursor`]: where the cursor is at each moment of a step.
//! - [`replay`]: the scrubber (live, step, scrub, play, speed).
//! - [`scene`]: what the view draws at one moment, and whether it must keep redrawing.
//! - [`geometry`]: points, sizes, rectangles and the screen → view mapping.
//! - [`approval`]: the approval sheet's picture of a pending confirmation (the evaluated
//!   observation's key and the target's frame), never part of the timeline.
//! - [`delta`]: the timeline as a bounded stream of deltas for a paired remote
//!   controller ([`DeltaEncoder`] on the phone, [`DeltaDecoder`] on the controller).
//! - [`live`]: all of the above behind one value ([`LiveView`]): one call per driver
//!   update, one per drawn frame.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod approval;
pub mod cursor;
pub mod delta;
pub mod geometry;
pub mod live;
pub mod privacy;
pub mod prompt;
pub mod reducer;
pub mod replay;
pub mod scene;
pub mod wire;

pub use approval::{ApprovalMark, ApprovalTarget};
pub use cursor::{Phase, Pose, Timing};
pub use delta::{
    Applied, DeltaDecoder, DeltaEncoder, DeltaError, DeltaHeader, ResyncReason, StreamCursor,
    ViewerDelta, MAX_DELTA_BYTES, MAX_SNAPSHOT_BYTES, SNAPSHOT_STEPS,
};
pub use geometry::{ViewerMapping, ViewerPoint, ViewerRect, ViewerSize};
pub use live::{Frame, LiveView, ReplayAction, ViewGeometry};
pub use reducer::{Change, Reducer, ReducerConfig, Update};
pub use replay::{ReplayState, Speed};
pub use scene::{Scene, Tone};
pub use wire::{
    Activity, ElementKind, FrameKey, Gesture, ScreenSource, StepState, ViewerElement,
    ViewerOutcome, ViewerScreen, ViewerStep, ViewerTimeline, WireError, MAX_ELEMENTS, MAX_SCREENS,
    MAX_STEPS, MAX_WIRE_TEXT, WIRE_VERSION,
};
