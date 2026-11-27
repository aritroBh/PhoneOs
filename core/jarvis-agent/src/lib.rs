//! The Jarvis agent: a sans-IO session state machine that turns screens into actions.
//!
//! The shell drives a [`Session`] with [`Session::step`]: every input is a
//! [`jarvis_proto::TimedEvent`], every output is exactly one [`jarvis_proto::Effect`]. The
//! loop per step is observe → decide (learned flow or model) → gate (jarvis-safety) → act
//! (one runner command at a time) → verify.
//!
//! Besides the session the crate exports the tolerant model-output parser
//! ([`parse_model_output`]) and the JSON schema / GBNF grammar of the canonical action form
//! ([`action_json_schema`], [`action_gbnf`]) so runtimes with constrained decoding can
//! enforce exactly what the parser's canonical form accepts.

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

mod act;
mod changes;
mod commands;
mod history;
mod lenient;
mod parser;
mod profile;
mod prompt;
mod schema;
mod session;
mod state;
mod tools;
pub mod typing;

/// Gesture geometry (where scrolls and swipes start and end); it lives in `jarvis-safety`
/// so the classifier reads the very path the command will take.
pub use jarvis_safety::gestures;
pub use parser::{parse_model_output, ParseError, Parsed};
pub use prompt::{system_prompt, ACTION_MAX_TOKENS, NO_SCREENSHOT_NOTE};
pub use schema::{action_examples, action_gbnf, action_json_schema, action_names};
pub use session::{
    Session, APPROVAL_SETTLE_MS, MAX_MODEL_RETRIES, MAX_REPLANS, MAX_STALE_APPROVALS,
    MAX_STALE_LOOKS, MAX_TYPING_LOOKS, TYPING_LOOK_MS, VERIFY_SETTLE_MS,
};
pub use typing::{check_typed, correction_deletes, MismatchKind, TypedVerdict};
