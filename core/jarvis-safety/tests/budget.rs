//! Budget edge cases.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_proto::Policy;
use jarvis_safety::{Budget, BudgetVerdict};

fn policy(steps: u32, ms: u64, failures: u32) -> Policy {
    Policy {
        max_steps: steps,
        max_duration_ms: ms,
        max_consecutive_failures: failures,
        ..Policy::default()
    }
}

#[test]
fn steps_run_out_after_max_steps() {
    let mut b = Budget::new(&policy(3, 60_000, 4), 1_000);
    for i in 0..3 {
        assert_eq!(b.check(1_000 + i), BudgetVerdict::Ok, "before step {i}");
        b.record_step(1_000 + i);
    }
    assert_eq!(b.steps(), 3);
    assert_eq!(b.remaining_steps(), 0);
    assert_eq!(b.check(1_010), BudgetVerdict::StepsExceeded);
}

#[test]
fn time_runs_out_at_max_duration() {
    let b = Budget::new(&policy(40, 600_000, 4), 5_000);
    assert_eq!(b.check(5_000), BudgetVerdict::Ok);
    assert_eq!(b.check(604_999), BudgetVerdict::Ok);
    assert_eq!(b.check(605_000), BudgetVerdict::TimeExceeded);
    assert_eq!(b.check(u64::MAX), BudgetVerdict::TimeExceeded);
}

#[test]
fn consecutive_failures_reset_on_success() {
    let mut b = Budget::new(&policy(40, 600_000, 3), 0);
    b.record_failure();
    b.record_failure();
    assert_eq!(b.check(1), BudgetVerdict::Ok);
    b.record_success();
    assert_eq!(b.consecutive_failures(), 0);
    b.record_failure();
    b.record_failure();
    assert_eq!(b.check(1), BudgetVerdict::Ok);
    b.record_failure();
    assert_eq!(b.check(1), BudgetVerdict::TooManyFailures);
    assert_eq!(b.failures(), 5);
    assert_eq!(b.successes(), 1);
}

#[test]
fn clock_going_backwards_neither_panics_nor_buys_time() {
    let mut b = Budget::new(&policy(40, 10_000, 4), 50_000);
    // Before the start: elapsed saturates at zero.
    assert_eq!(b.elapsed_ms(0), 0);
    assert_eq!(b.check(0), BudgetVerdict::Ok);
    b.record_step(59_000);
    // The clock jumps back; the latest time seen still counts.
    assert_eq!(b.elapsed_ms(1_000), 9_000);
    b.observe(60_500);
    assert_eq!(b.check(2_000), BudgetVerdict::TimeExceeded);
}

#[test]
fn zero_limits_and_saturation() {
    let b = Budget::new(&policy(0, 600_000, 4), 0);
    assert_eq!(b.check(0), BudgetVerdict::StepsExceeded);
    let b = Budget::new(&policy(10, 0, 4), 0);
    assert_eq!(b.check(0), BudgetVerdict::TimeExceeded);
    // Zero failures allowed: the first failure ends it, but no failure is fine.
    let mut b = Budget::new(&policy(10, 1_000, 0), 0);
    assert_eq!(b.check(0), BudgetVerdict::Ok);
    b.record_failure();
    assert_eq!(b.check(0), BudgetVerdict::TooManyFailures);

    let mut b = Budget::new(&policy(u32::MAX, u64::MAX, u32::MAX), u64::MAX);
    for _ in 0..3 {
        b.record_step(u64::MAX);
        b.record_failure();
    }
    assert_eq!(b.elapsed_ms(u64::MAX), 0);
    assert_eq!(b.check(u64::MAX), BudgetVerdict::Ok);
}

#[test]
fn steps_are_checked_before_time_and_failures() {
    let mut b = Budget::new(&policy(1, 10, 1), 0);
    b.record_step(100);
    b.record_failure();
    assert_eq!(b.check(100), BudgetVerdict::StepsExceeded);
    let mut b = Budget::new(&policy(5, 10, 1), 0);
    b.record_failure();
    assert_eq!(b.check(100), BudgetVerdict::TimeExceeded);
}
