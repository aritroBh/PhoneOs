//! Skills and workflows: reusable capabilities and automations built on the agent core.
//!
//! - A **skill** ([`Skill`]) is a named, reusable capability: what the owner calls it, the
//!   phrases that trigger it (with `{param}` slots), the owner's guidance in plain
//!   language, the apps it works in (optionally confined to them) and the learned flows
//!   that replay it. [`SkillStore`] keeps them validated, versioned as JSON and matchable
//!   ([`SkillStore::match_task`]). A session runs a skill through
//!   `SessionConfig::skill` ([`Skill::context`]).
//! - A **workflow** ([`Workflow`]) is an automation: a [`Trigger`] (manual, phrase, weekly
//!   schedule, external key from an iOS automation) plus ordered [`Step`]s.
//!   [`WorkflowRun`] executes one run as a sans-IO state machine; [`WorkflowStore`] keeps
//!   them; [`schedule`] does the weekly arithmetic.
//!
//! A workflow never widens policy: every step that touches the phone is an ordinary agent
//! session under the owner's `Policy`, and a skill can only narrow what a session may do.
//!
//! Like the rest of the core: no IO, no clock (time arrives as `now_ms` or local weekday
//! and minute), no randomness, no panics; errors are [`SkillError`].

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

mod error;
pub mod matcher;
pub mod run;
pub mod schedule;
pub mod skill;
pub mod store;
mod text;
pub mod workflow;
pub mod workflow_store;

pub use error::SkillError;
pub use jarvis_proto::SkillContext;
pub use matcher::{match_skills, Ambiguity, SkillMatch, AMBIGUITY_MARGIN, MAX_TASK_CHARS};
pub use run::{Awaiting, RunError, RunLimits, WorkflowEffect, WorkflowEvent, WorkflowRun};
pub use schedule::{is_due, minutes_since_last, minutes_until_next};
pub use skill::{is_valid_id, is_valid_param_name, Skill, SkillKind, SkillParam};
pub use store::{SkillStore, Upsert, MAX_SKILLS, SKILLS_SCHEMA_VERSION};
pub use workflow::{
    Condition, OnFailure, Schedule, Step, Trigger, Weekday, Workflow, WorkflowOutcome,
    WorkflowStatus,
};
pub use workflow_store::{WorkflowStore, MAX_WORKFLOWS, WORKFLOWS_SCHEMA_VERSION};
