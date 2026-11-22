//! Laptop-side simulator for the Jarvis core: a JSON-defined `SimPhone`, scripted and HTTP
//! models, the driver that runs a `Session` against them, and the eval harness. Dev tool
//! only: the only crate allowed to do IO, never shipped on the phone.
//!
//! * [`phone::SimPhone`] runs apps defined in JSON (see [`app`] for the schema) and reports
//!   the screen as a structured tree or as the runner's text format.
//! * [`model::Model`] is implemented by [`model::ScriptedModel`] and, with the `http`
//!   feature, `http::HttpModel`.
//! * [`driver::run`] loops effect, IO, event and returns a [`driver::RunReport`].
//! * [`eval`] and [`cli`] build the `jarvis-sim` command on top.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

pub mod app;
pub mod builtin;
pub mod cli;
pub mod driver;
pub mod engine;
pub mod error;
pub mod eval;
pub mod expr;
pub mod harness;
#[cfg(feature = "http")]
pub mod http;
pub mod human;
pub mod model;
pub mod phone;
pub mod runner_text;

pub use driver::{run, run_with, RunCounts, RunOptions, RunReport, TranscriptStep};
pub use error::SimError;
pub use harness::{run_task, RunSetup};
pub use human::Human;
pub use model::{Model, ModelError, ScriptedModel};
pub use phone::{ClockCosts, ObserveMode, SimPhone, SimSnapshot};
