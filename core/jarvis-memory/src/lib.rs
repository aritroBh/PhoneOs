//! Memory for the Jarvis core: learned flows (record and replay), user profile, task log and
//! versioned JSON persistence of the [`MemoryStore`].
//!
//! The crate does no IO and reads no clock: timestamps are passed in as `now_ms`, and the
//! shell persists the JSON bytes produced by [`MemoryStore::to_json`].

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

pub mod flow;
pub mod intent;
pub mod log;
pub mod profile;
pub mod recorder;
pub mod replayer;
pub mod store;

pub use flow::{flow_id, fnv1a64, Bindings, Flow, FlowAction, FlowStep, FlowText};
pub use intent::{intent_key, match_slotted};
pub use log::{LogEntry, LogStep, TaskLog};
pub use profile::{Profile, ProfileField};
pub use recorder::{RecordedStep, Recorder};
pub use replayer::{Divergence, Next, Replayer};
pub use store::{FlowStore, MemoryError, MemoryStore, UpsertOutcome, SCHEMA_VERSION};
