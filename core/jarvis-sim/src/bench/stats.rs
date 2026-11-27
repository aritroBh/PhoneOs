//! Timing: warm-up, adaptive iteration count, min / p50 / p99 / mean.

use std::hint::black_box;
use std::time::{Duration, Instant};

/// Summary of one measurement, in microseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    /// Number of timed iterations.
    pub n: usize,
    /// Fastest iteration.
    pub min_us: f64,
    /// Median (nearest rank).
    pub p50_us: f64,
    /// 99th percentile (nearest rank; with fewer than 100 samples this is the slowest).
    pub p99_us: f64,
    /// Arithmetic mean.
    pub mean_us: f64,
}

impl Stats {
    /// Summarises nanosecond samples; `None` when there are none.
    pub fn from_ns(samples: &mut [u64]) -> Option<Stats> {
        if samples.is_empty() {
            return None;
        }
        samples.sort_unstable();
        let n = samples.len();
        let at = |p: f64| -> f64 {
            // Nearest rank: ceil(p * n), 1-based.
            let rank = ((p * n as f64).ceil() as usize).clamp(1, n);
            samples.get(rank - 1).copied().unwrap_or(0) as f64 / 1000.0
        };
        let sum: u128 = samples.iter().map(|&s| u128::from(s)).sum();
        Some(Stats {
            n,
            min_us: samples.first().copied().unwrap_or(0) as f64 / 1000.0,
            p50_us: at(0.50),
            p99_us: at(0.99),
            mean_us: (sum as f64 / n as f64) / 1000.0,
        })
    }

    /// The same measurement scaled by `k` (sampled work extrapolated to the whole).
    pub fn scaled(self, k: f64) -> Stats {
        Stats {
            n: self.n,
            min_us: self.min_us * k,
            p50_us: self.p50_us * k,
            p99_us: self.p99_us * k,
            mean_us: self.mean_us * k,
        }
    }
}

/// How many iterations to run.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Warm-up runs (not timed into the result) before measuring, at most; a single slow run
    /// is its own warm-up.
    pub warmup: usize,
    /// Fewest timed iterations for a fast stage.
    pub min_iters: usize,
    /// Most timed iterations.
    pub max_iters: usize,
    /// Target total timed time per stage.
    pub budget: Duration,
    /// Exact iteration count, overriding the adaptive choice.
    pub fixed_iters: Option<usize>,
}

impl Plan {
    /// The plan `jarvis-sim bench` uses unless told otherwise.
    pub fn standard() -> Plan {
        Plan {
            warmup: 3,
            min_iters: 15,
            max_iters: 2000,
            budget: Duration::from_millis(150),
            fixed_iters: None,
        }
    }
}

/// Runs `run` on a fresh input from `setup` each time. Only `run` is timed; its result is
/// kept alive through [`black_box`] and dropped after the clock stops.
pub fn measure<S, R>(
    plan: &Plan,
    mut setup: impl FnMut() -> S,
    mut run: impl FnMut(S) -> R,
) -> Option<Stats> {
    let mut one = || -> Duration {
        let input = setup();
        let start = Instant::now();
        let result = run(black_box(input));
        let took = start.elapsed();
        drop(black_box(result));
        took
    };
    let mut estimate = one();
    // A run that already takes long is its own warm-up.
    let warm = if estimate > Duration::from_millis(100) {
        0
    } else {
        plan.warmup.saturating_sub(1)
    };
    for _ in 0..warm {
        estimate = one();
    }
    let n = match plan.fixed_iters {
        Some(n) => n.max(1),
        None => {
            let min = if estimate > Duration::from_secs(1) {
                3
            } else if estimate > Duration::from_millis(100) {
                5
            } else {
                plan.min_iters
            };
            let est_ns = estimate.as_nanos().max(1);
            let fit = usize::try_from(plan.budget.as_nanos() / est_ns).unwrap_or(usize::MAX);
            fit.clamp(min, plan.max_iters.max(min))
        }
    };
    let mut samples: Vec<u64> = Vec::with_capacity(n);
    for _ in 0..n {
        let t = one();
        samples.push(u64::try_from(t.as_nanos()).unwrap_or(u64::MAX));
    }
    Stats::from_ns(&mut samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut s: Vec<u64> = (1..=100).map(|v| v * 1000).collect();
        let st = Stats::from_ns(&mut s).unwrap();
        assert_eq!(st.n, 100);
        assert_eq!(st.min_us, 1.0);
        assert_eq!(st.p50_us, 50.0);
        assert_eq!(st.p99_us, 99.0);
        assert!((st.mean_us - 50.5).abs() < 1e-9);
        let mut one = vec![5000];
        let st = Stats::from_ns(&mut one).unwrap();
        assert_eq!((st.min_us, st.p50_us, st.p99_us), (5.0, 5.0, 5.0));
        assert!(Stats::from_ns(&mut []).is_none());
    }

    #[test]
    fn measure_runs_the_requested_iterations() {
        let plan = Plan {
            fixed_iters: Some(7),
            ..Plan::standard()
        };
        let mut setups = 0;
        let st = measure(
            &plan,
            || {
                setups += 1;
                setups
            },
            |x| x + 1,
        )
        .unwrap();
        assert_eq!(st.n, 7);
        assert!(setups >= 7);
    }
}
