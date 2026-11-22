//! Shared plain-data types for the Jarvis core: geometry, the UI tree, model-facing actions,
//! runner commands, tools, model IO, safety policy and the session's events and effects.
//!
//! Everything here is `Serialize + Deserialize + Clone + Debug + PartialEq`. JSON field names
//! are snake_case; data-carrying enums are internally tagged with `"type"`, unit-only enums
//! are plain snake_case strings.

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

pub mod action;
pub mod compact;
pub mod geometry;
pub mod model;
pub mod policy;
pub mod runner;
pub mod session;
pub mod skill;
pub mod task;
pub mod tool;
pub mod ui;

#[cfg(any(test, feature = "test-util"))]
pub mod fixtures;

pub use action::{Action, Direction};
pub use compact::{BarNode, CompactScreen, Element, RawNode, ScreenCues};
pub use geometry::{Point, Rect, Size};
pub use model::{Mark, ModelOutput, ModelProfile, ModelRequest, RequestKind};
pub use policy::{
    AppRule, Policy, PolicyMode, RiskCategory, TrustLevel, DEFAULT_MAX_LOCKED_WAIT_MS,
    PASSWORDS_BUNDLE_ID, SETTINGS_BUNDLE_ID, SPRINGBOARD_BUNDLE_ID, WALLET_BUNDLE_ID,
};
pub use runner::{Key, RunnerCommand, RunnerFailure, RunnerResult};
pub use session::{
    ConfirmRequest, Effect, Event, Outcome, OutcomeStatus, Phase, Progress, TimedEvent,
};
pub use skill::SkillContext;
pub use task::{AppInfo, SessionConfig, TaskSpec};
pub use tool::{ParamKind, Tier, ToolCall, ToolParam, ToolResult, ToolSpec};
pub use ui::{ImageRef, Observation, Orientation, Role, Screen, UiNode};
