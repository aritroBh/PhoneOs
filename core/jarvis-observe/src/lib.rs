//! UI tree compaction for the Jarvis core: `Screen` to indexed `Element`s, prompt rendering,
//! fingerprints, screen hashes, diffs and set-of-marks.
//!
//! Everything here is pure and deterministic. The pipeline is
//! [`compact`] (tree to flat element list), [`render`] (element list to prompt text within a
//! token budget), plus [`screen_hash`] / [`diff`] for stuck detection and verification, and
//! [`screen_digest`] for the exact record an owner's approval is checked against, and
//! [`parse_runner_text`] to read the LoopCtrl-style text tree into a [`jarvis_proto::UiNode`].

#![forbid(unsafe_code)]

mod compact;
mod diff;
mod digest;
mod fingerprint;
mod lookup;
mod render;
mod runner_text;

pub use compact::compact;
pub use diff::{diff, screen_hash, ScreenDiff};
pub use digest::{screen_digest, screen_digest_input, ScreenDigest};
pub use fingerprint::{fingerprint, fnv1a64, identity_key};
pub use lookup::{find_by_fingerprint, marks, resolve, ResolveError};
pub use render::{estimate_tokens, render, role_name, switch_state, MORE_ABOVE, MORE_BELOW};
pub use runner_text::{parse_runner_text, ParseError, ParsedTree};
