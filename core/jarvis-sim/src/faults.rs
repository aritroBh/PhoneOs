//! Deterministic fault injection for the simulated phone: locks, unreadable screens and
//! dropped keystrokes.
//!
//! Everything is keyed to counters or the simulated clock, never to randomness, so a run
//! with the same faults replays exactly.

use serde::{Deserialize, Serialize};

/// One time the phone locks.
///
/// Exactly one trigger should be set; when several are, the first to fire wins (and the
/// others are ignored for this lock).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LockSpec {
    /// Lock just before the n-th runner command (0-based, counting every command the
    /// phone receives, refused ones included): the runner refuses it as `device_locked`.
    pub before_act: Option<usize>,
    /// Lock once the simulated clock reaches this time; the agent finds out from its next
    /// observation or command.
    pub at_ms: Option<u64>,
    /// Lock instead of answering the n-th effect (0-based): the shell reports
    /// `Event::DeviceLocked` (the lock notification arrived while the effect was pending).
    pub at_effect: Option<usize>,
    /// The owner unlocks this long after the lock; `None` leaves the phone locked.
    pub unlock_after_ms: Option<u64>,
}

/// Observations that fail (`Event::ObserveFailed`), counted over unlocked observations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ObserveFault {
    /// Observations that succeed before the failures start.
    pub after: usize,
    /// Failures in a row.
    pub times: usize,
}

/// Typing that loses characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DropChars {
    /// Drop every n-th character (1-based positions n, 2n, ...); values below 2 drop every
    /// second character.
    pub every: usize,
    /// How many `TypeText` commands are affected (the first ones).
    pub times: usize,
}

impl DropChars {
    /// `text` with every `every`-th character removed.
    pub fn apply(&self, text: &str) -> String {
        let n = self.every.max(2);
        text.chars()
            .enumerate()
            .filter(|(i, _)| (i + 1) % n != 0)
            .map(|(_, c)| c)
            .collect()
    }
}

/// All faults of one run.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Faults {
    /// Locks, in any order.
    pub locks: Vec<LockSpec>,
    /// Failing observations.
    pub observe_fail: Option<ObserveFault>,
    /// Dropped characters.
    pub drop_chars: Option<DropChars>,
    /// How often the simulated shell's timer fires while waiting for an unlock
    /// (milliseconds; 0 means 60 s).
    pub unlock_poll_ms: u64,
}

impl Faults {
    /// Whether nothing is injected.
    pub fn is_empty(&self) -> bool {
        self.locks.is_empty() && self.observe_fail.is_none() && self.drop_chars.is_none()
    }

    /// The timer interval while waiting for an unlock.
    pub fn poll_ms(&self) -> u64 {
        if self.unlock_poll_ms == 0 {
            60_000
        } else {
            self.unlock_poll_ms
        }
    }
}
