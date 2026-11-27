//! Voice for the Jarvis core: "Hey Jarvis, <request>" starts a task hands-free, Jarvis says
//! short status lines aloud, and "Jarvis, stop" stops it.
//!
//! Like the rest of the core this crate is **sans-IO**: [`VoiceSession`] is a state machine
//! the shell drives. Every input is a [`VoiceEvent`] with the shell's monotonic `now_ms`;
//! every output is a list of [`VoiceEffect`]s the shell carries out in order (start the
//! recogniser, speak a line, start a task). No clock, no IO, no threads, no randomness.
//! Every public entry point is total: no input panics.
//!
//! The adversary is what is on the screen **and what the phone or the room plays as
//! audio**: a video, a call on speaker, a web page read aloud, another person. The
//! microphone cannot tell the owner's voice from these. So:
//!
//! 1. A task started by voice without a touch is [`jarvis_proto::TaskOrigin::Voice`]: not
//!    the owner's words for any gate (as strict as a remote device's request). A transcript
//!    the owner confirmed by touch is `Phone`.
//! 2. A spoken word never approves anything. While a question is pending, voice can only
//!    stop the task or is ignored; there is no spoken "yes".
//! 3. "Stop", "cancel", "never mind", "abort" always work after the wake phrase, and
//!    without it while Jarvis speaks or a voice exchange is open (barge-in).
//! 4. Jarvis does not hear itself: while it speaks and for [`SPEAKING_TAIL_MS`] after,
//!    transcripts are ignored except the stop words, and a transcript contained in what it
//!    is saying or just said is ignored. No line Jarvis says contains the wake phrase.
//! 5. What is spoken is an output to the room: fixed phrases ([`phrases`]); the task's
//!    answer only when [`SpeakAnswers::WhenUnlocked`] allows it and the phone is unlocked,
//!    cleaned and capped ([`answer_line`]).
//! 6. Nothing starts from a partial transcript: only a final one, the endpoint timeout over
//!    the last partial (marked `endpointed`), or a transcript confirmed by touch.
//! 7. The wake phrase is off by default; push to talk works without it and, by default,
//!    shows the transcript for a touch on Start before anything runs.
//! 8. While a task started elsewhere runs, a voice request is answered "I am busy." and
//!    nothing starts; the stop words still stop it.
//!
//! See `docs/ARCHITECTURE.md`, section *Voice*, for the state machine and the residuals.

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

pub mod parse;
pub mod phrases;
mod session;
mod types;

pub use parse::{contains_wake_word, parse_utterance, Utterance, STOP_PHRASES, WAKE_PHRASES};
pub use phrases::{answer_line, clean_spoken, MAX_SPOKEN_ANSWER_CHARS};
pub use session::{
    VoiceSession, ECHO_WINDOW_MS, ENDPOINT_SILENCE_MS, LISTEN_NO_SPEECH_MS, MAX_REQUEST_CHARS,
    MAX_SPEAKING_MS, MAX_UTTERANCE_MS, SPEAKING_TAIL_MS, START_TIMEOUT_MS, WAKE_RETRY_MS,
};
pub use types::{
    CueKind, SpeakAnswers, TaskEvent, UtteranceKind, VoiceEffect, VoiceEvent, VoicePhase,
    VoiceSettings, VoiceStatus,
};
