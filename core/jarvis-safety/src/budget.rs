//! Per-task step, time and failure budgets.

use jarvis_proto::Policy;

/// Result of a budget check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetVerdict {
    /// Within budget.
    Ok,
    /// `max_steps` steps have been taken.
    StepsExceeded,
    /// `max_duration_ms` has elapsed.
    TimeExceeded,
    /// `max_consecutive_failures` actions in a row failed.
    TooManyFailures,
}

/// Tracks a task's steps, elapsed time and failures against its [`Policy`].
///
/// All arithmetic saturates. Elapsed time is the sum of the *forward* steps between the
/// times seen, so a clock that jumps backwards neither panics nor buys extra time: after a
/// jump back, time counts again from the new reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    max_steps: u32,
    max_duration_ms: u64,
    max_consecutive_failures: u32,
    elapsed_ms: u64,
    last_ms: u64,
    steps: u32,
    successes: u32,
    failures: u32,
    consecutive_failures: u32,
}

impl Budget {
    /// A fresh budget starting at `now_ms`.
    pub fn new(policy: &Policy, now_ms: u64) -> Self {
        Self {
            max_steps: policy.max_steps,
            max_duration_ms: policy.max_duration_ms,
            max_consecutive_failures: policy.max_consecutive_failures,
            elapsed_ms: 0,
            last_ms: now_ms,
            steps: 0,
            successes: 0,
            failures: 0,
            consecutive_failures: 0,
        }
    }

    /// Records the time `now_ms` without counting anything.
    pub fn observe(&mut self, now_ms: u64) {
        self.elapsed_ms = self
            .elapsed_ms
            .saturating_add(now_ms.saturating_sub(self.last_ms));
        self.last_ms = now_ms;
    }

    /// Moves the clock reading to `now_ms` without counting the time since the last reading:
    /// time the session spent paused (waiting for the owner to unlock the phone) is not
    /// part of the elapsed budget.
    pub fn skip(&mut self, now_ms: u64) {
        self.last_ms = now_ms;
    }

    /// Takes back the most recent step: its action was set aside without being carried out
    /// (the phone locked first), so it will be decided again.
    pub fn refund_step(&mut self) {
        self.steps = self.steps.saturating_sub(1);
    }

    /// Counts one agent step taken at `now_ms`.
    pub fn record_step(&mut self, now_ms: u64) {
        self.observe(now_ms);
        self.steps = self.steps.saturating_add(1);
    }

    /// Records a successful action; resets the consecutive-failure count.
    pub fn record_success(&mut self) {
        self.successes = self.successes.saturating_add(1);
        self.consecutive_failures = 0;
    }

    /// Records a failed action.
    pub fn record_failure(&mut self) {
        self.failures = self.failures.saturating_add(1);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    /// Checks the budget at `now_ms`, before taking another step.
    ///
    /// Steps are checked first, then time, then failures. `StepsExceeded` once
    /// `max_steps` steps are recorded; `TimeExceeded` once `max_duration_ms` has elapsed;
    /// `TooManyFailures` once at least one and at least `max_consecutive_failures`
    /// consecutive failures are recorded.
    pub fn check(&self, now_ms: u64) -> BudgetVerdict {
        if self.steps >= self.max_steps {
            BudgetVerdict::StepsExceeded
        } else if self.elapsed_ms(now_ms) >= self.max_duration_ms {
            BudgetVerdict::TimeExceeded
        } else if self.consecutive_failures > 0
            && self.consecutive_failures >= self.max_consecutive_failures
        {
            BudgetVerdict::TooManyFailures
        } else {
            BudgetVerdict::Ok
        }
    }

    /// Milliseconds elapsed: the forward time recorded so far plus any forward time from
    /// the last reading to `now_ms` (which is not recorded).
    pub fn elapsed_ms(&self, now_ms: u64) -> u64 {
        self.elapsed_ms
            .saturating_add(now_ms.saturating_sub(self.last_ms))
    }

    /// Steps recorded so far.
    pub fn steps(&self) -> u32 {
        self.steps
    }

    /// Steps left before `StepsExceeded`.
    pub fn remaining_steps(&self) -> u32 {
        self.max_steps.saturating_sub(self.steps)
    }

    /// Successful actions recorded.
    pub fn successes(&self) -> u32 {
        self.successes
    }

    /// Failed actions recorded in total.
    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// Failed actions since the last success.
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}
