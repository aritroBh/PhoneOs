//! `JarvisSession`: the session object Swift drives.
//!
//! One `jarvis_agent::Session` behind a `Mutex`, so Swift may call from any thread (each
//! call is atomic; concurrent `step`s are serialized, never interleaved). Lifecycle:
//!
//! - **Live**: `step` forwards to the agent session.
//! - **Poisoned**: a panic was caught inside `step` (or `finish`). From then on `step`
//!   returns `Finished(Outcome { status: .failed, summary: "internal error: …" })`, and
//!   `finish` throws `JarvisError.poisoned`: the session's memory may be half-updated, so it
//!   is never handed out; the shell keeps the bytes it had.
//! - **Closed**: `finish` was called. It returned the memory JSON; later `finish` calls
//!   return the same string (or the same error). `step` after `finish` returns the session's
//!   final `Finished` effect, or a `Failed` protocol error when it had not finished.
//!
//! A poisoned `Mutex` (a panic that escaped while the lock was held, which the inner
//! `catch_unwind` should make impossible) is recovered with `into_inner` and the session is
//! treated as poisoned.

use std::mem;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard};

use jarvis_agent::Session;
use jarvis_memory::MemoryStore;
use jarvis_proto as p;

use crate::convert::event_into_proto;
use crate::error::JarvisError;
use crate::guard::{guarded, panic_message};
use crate::types::{Effect, Event, SessionConfig, SessionProgress, TaskSpec};

enum State {
    Live {
        session: Box<Session>,
        /// The last `Finished` outcome `step` returned, if any.
        finished: Option<p::Outcome>,
    },
    Poisoned {
        outcome: p::Outcome,
        progress: p::Progress,
    },
    Closed {
        memory: Result<String, JarvisError>,
        finished: Option<p::Outcome>,
        progress: p::Progress,
    },
}

struct Inner {
    state: State,
    /// Note about the last `ObservedRunnerText` event, if it did not parse cleanly.
    last_issue: Option<String>,
    /// Test hook: the next `step` panics inside the guarded region.
    #[cfg(any(test, feature = "panic-hooks"))]
    panic_next_step: bool,
}

/// A running agent session for one task. Swift: `JarvisSession`.
#[derive(uniffi::Object)]
pub struct JarvisSession {
    inner: Mutex<Inner>,
}

/// Loads memory bytes; empty (or whitespace-only) means fresh memory.
fn load_memory(memory_json: &str) -> Result<MemoryStore, JarvisError> {
    if memory_json.trim().is_empty() {
        return Ok(MemoryStore::new());
    }
    MemoryStore::from_json(memory_json).map_err(|e| JarvisError::InvalidMemory {
        message: e.to_string(),
    })
}

/// The outcome of a session poisoned by an internal error.
fn poisoned_outcome(message: &str, steps: u32) -> p::Outcome {
    p::Outcome {
        status: p::OutcomeStatus::Failed,
        summary: format!("internal error: {message}"),
        steps,
        duration_ms: 0,
    }
}

/// Progress of a session that is no longer live.
fn final_progress(mut progress: p::Progress) -> p::Progress {
    progress.phase = p::Phase::Finished;
    progress
}

impl JarvisSession {
    /// Builds a session from proto values (shared by the FFI constructor and Rust tests).
    pub fn from_proto(
        config: p::SessionConfig,
        task: p::TaskSpec,
        memory_json: &str,
    ) -> Result<Self, JarvisError> {
        let memory = load_memory(memory_json)?;
        Ok(JarvisSession {
            inner: Mutex::new(Inner {
                state: State::Live {
                    session: Box::new(Session::new(config, task, memory)),
                    finished: None,
                },
                last_issue: None,
                #[cfg(any(test, feature = "panic-hooks"))]
                panic_next_step: false,
            }),
        })
    }

    /// Locks the state, recovering (and poisoning the session) if the mutex is poisoned.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poison) => {
                let mut guard = poison.into_inner();
                if let State::Live { .. } = guard.state {
                    guard.state = State::Poisoned {
                        outcome: poisoned_outcome("a previous call panicked", 0),
                        progress: final_progress(p::Progress::default()),
                    };
                }
                self.inner.clear_poison();
                guard
            }
        }
    }

    /// One step with proto types in and out (the FFI `step` is a thin wrapper).
    pub fn step_proto(&self, now_ms: u64, event: Event) -> p::Effect {
        let mut inner = self.lock();
        let inner = &mut *inner;
        match &mut inner.state {
            State::Live { session, finished } => {
                #[cfg(any(test, feature = "panic-hooks"))]
                let panic_now = mem::take(&mut inner.panic_next_step);
                let result = catch_unwind(AssertUnwindSafe(|| {
                    #[cfg(any(test, feature = "panic-hooks"))]
                    if panic_now {
                        #[allow(clippy::panic)]
                        {
                            panic!("deliberate panic from the panic-hooks test hook");
                        }
                    }
                    let (event, issue) = event_into_proto(event);
                    let effect = session.step(p::TimedEvent::new(now_ms, event));
                    (effect, issue)
                }));
                match result {
                    Ok((effect, issue)) => {
                        if issue.is_some() {
                            inner.last_issue = issue;
                        }
                        if let p::Effect::Finished(outcome) = &effect {
                            *finished = Some(outcome.clone());
                        }
                        effect
                    }
                    Err(payload) => {
                        let message = panic_message(payload.as_ref());
                        // The session may be half-updated; read what is safe and drop it.
                        let progress = catch_unwind(AssertUnwindSafe(|| session.progress()))
                            .unwrap_or_default();
                        let outcome = poisoned_outcome(&message, progress.step);
                        inner.state = State::Poisoned {
                            outcome: outcome.clone(),
                            progress: final_progress(progress),
                        };
                        p::Effect::Finished(outcome)
                    }
                }
            }
            State::Poisoned { outcome, .. } => p::Effect::Finished(outcome.clone()),
            State::Closed {
                finished, progress, ..
            } => p::Effect::Finished(finished.clone().unwrap_or_else(|| p::Outcome {
                status: p::OutcomeStatus::Failed,
                summary: "protocol error: step after finish() on an unfinished session".to_owned(),
                steps: progress.step,
                duration_ms: 0,
            })),
        }
    }

    /// Progress with proto types.
    pub fn progress_proto(&self) -> Result<p::Progress, JarvisError> {
        let inner = self.lock();
        match &inner.state {
            State::Live { session, .. } => guarded("progress", || Ok(session.progress())),
            State::Poisoned { progress, .. } | State::Closed { progress, .. } => {
                Ok(progress.clone())
            }
        }
    }

    /// `finish` with the memory JSON as a Rust string.
    pub fn finish_json(&self) -> Result<String, JarvisError> {
        let mut inner = self.lock();
        match &inner.state {
            State::Poisoned { outcome, .. } => {
                return Err(JarvisError::Poisoned {
                    message: outcome.summary.clone(),
                })
            }
            State::Closed { memory, .. } => return memory.clone(),
            State::Live { .. } => {}
        }
        let placeholder = State::Poisoned {
            outcome: poisoned_outcome("finish() did not complete", 0),
            progress: final_progress(p::Progress::default()),
        };
        let State::Live { session, finished } = mem::replace(&mut inner.state, placeholder) else {
            // Checked above; keep the placeholder (poisoned) rather than panic.
            return Err(JarvisError::Internal {
                message: "finish: session state changed while locked".to_owned(),
            });
        };
        let result = catch_unwind(AssertUnwindSafe(move || {
            let progress = session.progress();
            let memory = session
                .into_memory()
                .to_json()
                .map_err(|e| JarvisError::Internal {
                    message: format!("memory could not be serialized: {e}"),
                });
            (memory, progress)
        }));
        match result {
            Ok((memory, progress)) => {
                let progress = if finished.is_some() {
                    final_progress(progress)
                } else {
                    progress
                };
                inner.state = State::Closed {
                    memory: memory.clone(),
                    finished,
                    progress,
                };
                memory
            }
            Err(payload) => {
                let message = panic_message(payload.as_ref());
                inner.state = State::Poisoned {
                    outcome: poisoned_outcome(&message, 0),
                    progress: final_progress(p::Progress::default()),
                };
                Err(JarvisError::Internal {
                    message: format!("finish: {message}"),
                })
            }
        }
    }

    /// Test hook: make the next `step` panic inside the guarded region.
    #[cfg(any(test, feature = "panic-hooks"))]
    pub fn arm_panic_on_next_step(&self) {
        self.lock().panic_next_step = true;
    }
}

#[uniffi::export]
impl JarvisSession {
    /// Creates a session. `memory_json` is the bytes `finish()` returned last time (empty
    /// for fresh memory); invalid JSON throws `JarvisError.invalidMemory`.
    #[uniffi::constructor]
    pub fn new(
        config: SessionConfig,
        task: TaskSpec,
        memory_json: String,
    ) -> Result<Arc<Self>, JarvisError> {
        guarded("JarvisSession.init", || {
            Self::from_proto(config.into(), task.into(), &memory_json).map(Arc::new)
        })
    }

    /// Feeds one event (with the shell's monotonic clock) and returns the one effect the
    /// shell must perform next. Never throws: an internal error finishes the session.
    pub fn step(&self, now_ms: u64, event: Event) -> Effect {
        // `step_proto` contains every panic itself; this outer catch only covers the
        // conversion of the effect, which is plain data moving.
        match catch_unwind(AssertUnwindSafe(|| self.step_proto(now_ms, event).into())) {
            Ok(effect) => effect,
            Err(payload) => Effect::Finished {
                outcome: poisoned_outcome(&panic_message(payload.as_ref()), 0).into(),
            },
        }
    }

    /// Progress for the UI.
    pub fn progress(&self) -> Result<SessionProgress, JarvisError> {
        guarded("progress", || self.progress_proto().map(Into::into))
    }

    /// Ends the session and returns the updated memory JSON for the shell to persist
    /// (learned flow, flow statistics and the task log entry, added when the session had
    /// finished). Call once; later calls return the same string.
    pub fn finish(&self) -> Result<String, JarvisError> {
        guarded("finish", || self.finish_json())
    }

    /// A note about the last runner-text observation that did not parse cleanly (lines
    /// skipped, or nothing parseable so an empty screen was observed), for the shell's log.
    pub fn last_observation_issue(&self) -> Result<Option<String>, JarvisError> {
        guarded("last_observation_issue", || {
            Ok(self.lock().last_issue.clone())
        })
    }
}

/// Exported test hooks (feature `panic-hooks`): prove from Swift that panics are contained.
#[cfg(feature = "panic-hooks")]
#[uniffi::export]
impl JarvisSession {
    /// Makes the next `step` panic inside Rust (test builds only).
    pub fn debug_panic_on_next_step(&self) {
        self.arm_panic_on_next_step();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AppInfo, OutcomeStatus, Phase};

    fn session() -> Arc<JarvisSession> {
        let mut config: SessionConfig = p::SessionConfig::default().into();
        config.known_apps.push(AppInfo {
            bundle_id: "com.apple.Preferences".to_owned(),
            name: "Settings".to_owned(),
        });
        let task = TaskSpec {
            id: "t".to_owned(),
            text: "Turn off Wi-Fi".to_owned(),
            remote: false,
            voice: false,
        };
        JarvisSession::new(config, task, String::new()).unwrap()
    }

    #[test]
    fn a_panic_in_step_poisons_the_session_into_finished_failed() {
        let s = session();
        assert!(matches!(s.step(1, Event::Start), Effect::Observe { .. }));
        s.arm_panic_on_next_step();
        let Effect::Finished { outcome } = s.step(2, Event::Cancel) else {
            panic!("a caught panic must finish the session")
        };
        assert_eq!(outcome.status, OutcomeStatus::Failed);
        assert!(
            outcome
                .summary
                .starts_with("internal error: deliberate panic"),
            "{}",
            outcome.summary
        );
        // Permanently: every later step returns the same outcome, whatever the event.
        for (now, ev) in [(3, Event::Start), (4, Event::Cancel)] {
            assert_eq!(
                s.step(now, ev),
                Effect::Finished {
                    outcome: outcome.clone()
                }
            );
        }
        assert_eq!(s.progress().unwrap().phase, Phase::Finished);
        // The half-updated memory is never handed out.
        for _ in 0..2 {
            let err = s.finish().unwrap_err();
            assert!(matches!(err, JarvisError::Poisoned { .. }), "{err:?}");
        }
        // And the mutex is not poisoned: other calls still work.
        assert!(s.last_observation_issue().unwrap().is_none());
    }

    #[test]
    fn a_panic_in_another_export_is_a_thrown_error() {
        let err = crate::api::debug_panic("boom".to_owned()).unwrap_err();
        assert_eq!(
            err,
            JarvisError::Internal {
                message: "debug_panic: boom".to_owned()
            }
        );
        // The process is fine afterwards.
        assert!(crate::api::action_gbnf().is_ok());
    }

    #[test]
    fn a_poisoned_mutex_is_recovered_and_poisons_the_session() {
        let s = session();
        s.step(1, Event::Start);
        let s2 = Arc::clone(&s);
        // Poison the mutex by panicking while holding the lock (outside our catch).
        let _ = std::thread::spawn(move || {
            let _guard = s2.inner.lock().unwrap();
            panic!("poison the lock");
        })
        .join();
        assert!(s.inner.is_poisoned());
        let Effect::Finished { outcome } = s.step(2, Event::Cancel) else {
            panic!()
        };
        assert_eq!(outcome.status, OutcomeStatus::Failed);
        assert!(outcome.summary.starts_with("internal error"));
        assert!(!s.inner.is_poisoned());
        assert!(matches!(
            s.finish().unwrap_err(),
            JarvisError::Poisoned { .. }
        ));
    }

    #[test]
    fn panic_payload_messages() {
        let r = std::panic::catch_unwind(|| panic!("formatted {}", 1)).unwrap_err();
        assert_eq!(panic_message(r.as_ref()), "formatted 1");
        let r = std::panic::catch_unwind(|| std::panic::panic_any(7u8)).unwrap_err();
        assert_eq!(panic_message(r.as_ref()), "unknown panic");
    }
}
