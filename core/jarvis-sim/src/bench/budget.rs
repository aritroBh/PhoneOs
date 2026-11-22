//! Budgets for `--check`.
//!
//! Two kinds, because they fail for different reasons.
//!
//! * **Absolute** limits on the p99 of a stage, generous (5 to 10 times what the reference
//!   machine measured, with floors) so that a slow or busy CI machine does not flake. They
//!   catch a constant factor going wrong: an accidental clone of the tree, a lock, a debug
//!   assertion.
//! * **Scaling** limits on `time(20k nodes) / time(10k nodes)` and `time(50k) / time(10k)` of
//!   the fastest run and the median of a stage (both must be over). A ratio is independent
//!   of the machine, and a quadratic stage shows as 4 and 25 where a linear one shows as 2
//!   and 5. A stage that is known to be super-linear today has its own, looser limit below
//!   (named, with the reason); tighten it when the cause is fixed.
//!
//! Re-derive the numbers with three full runs (`--json`), see the README.

use std::fmt;

use super::gen::{Shape, View};
use super::{CaseMeta, Run};

/// A budget that was not held.
#[derive(Debug, Clone)]
pub(crate) struct Violation {
    pub what: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.what)
    }
}

/// `limit(n) = floor_us + lin_ns * n / 1000 + quad_ps * n^2 / 1e6`, in microseconds for a
/// tree of `n` nodes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Curve {
    pub floor_us: f64,
    pub lin_ns: f64,
    pub quad_ps: f64,
}

impl Curve {
    const fn new(floor_us: f64, lin_ns: f64, quad_ps: f64) -> Curve {
        Curve {
            floor_us,
            lin_ns,
            quad_ps,
        }
    }

    fn at(self, nodes: usize) -> f64 {
        let n = nodes as f64;
        self.floor_us + self.lin_ns * n / 1_000.0 + self.quad_ps * n * n / 1_000_000.0
    }
}

/// The p99 limits of one stage.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Absolute {
    pub stage: &'static str,
    /// On every real iOS 26 tree.
    pub fixtures_us: f64,
    /// On synthetic trees of the `page` layout, by node count.
    pub page: Curve,
    /// On synthetic trees of the `dense` layout, by node count.
    pub dense: Curve,
}

const fn abs(stage: &'static str, fixtures_us: f64, page: Curve, dense: Curve) -> Absolute {
    Absolute {
        stage,
        fixtures_us,
        page,
        dense,
    }
}

/// The absolute limits. See the module docs; derived from three release runs.
pub(crate) const ABSOLUTE: &[Absolute] = &[
    abs(
        "ingest.runner_text",
        7500.0,
        Curve::new(10000.0, 0.0, 0.0),
        Curve::new(20000.0, 21685.0, 0.0),
    ),
    abs(
        "ingest.ffi_lift",
        750.0,
        Curve::new(1500.0, 2324.0, 0.0),
        Curve::new(1500.0, 1327.0, 0.0),
    ),
    abs(
        "ingest.ffi_convert",
        300.0,
        Curve::new(750.0, 617.0, 0.0),
        Curve::new(750.0, 359.0, 0.0),
    ),
    abs(
        "ingest.drop_tree",
        200.0,
        Curve::new(300.0, 1060.0, 0.0),
        Curve::new(500.0, 700.0, 0.7),
    ),
    abs(
        "compact",
        2000.0,
        Curve::new(2000.0, 1898.0, 0.0),
        Curve::new(20000.0, 22699.0, 0.0),
    ),
    abs(
        "digest",
        1000.0,
        Curve::new(2000.0, 4269.0, 0.0),
        Curve::new(3000.0, 4911.0, 0.0),
    ),
    abs(
        "safe_copy",
        750.0,
        Curve::new(3000.0, 280.0, 0.0),
        Curve::new(50000.0, 27518.0, 0.0),
    ),
    abs(
        "render.full",
        200.0,
        Curve::new(300.0, 5.0, 0.0),
        Curve::new(3000.0, 2636.0, 0.0),
    ),
    abs(
        "render.budget",
        300.0,
        Curve::new(300.0, 6.0, 0.0),
        Curve::new(30000.0, 9410.0, 0.0),
    ),
    abs(
        "tap.one",
        150.0,
        Curve::new(200.0, 4.0, 0.0),
        Curve::new(150.0, 38.0, 0.0),
    ),
    abs(
        "tap.sweep",
        750.0,
        Curve::new(2000.0, 0.0, 0.0),
        Curve::new(50000.0, 164104.0, 4585.8),
    ),
    abs(
        "classify.tap",
        1000.0,
        Curve::new(1500.0, 0.0, 0.0),
        Curve::new(20000.0, 14775.0, 0.0),
    ),
    abs(
        "evaluate.tap",
        1000.0,
        Curve::new(2000.0, 0.0, 0.0),
        Curve::new(20000.0, 14698.0, 0.5),
    ),
    abs(
        "classify.type",
        50.0,
        Curve::new(30.0, 0.0, 0.0),
        Curve::new(50.0, 0.0, 0.0),
    ),
    abs(
        "evaluate.type",
        300.0,
        Curve::new(150.0, 0.0, 0.0),
        Curve::new(100.0, 3.0, 0.0),
    ),
    abs(
        "classify.swipe",
        1500.0,
        Curve::new(1500.0, 76.0, 0.0),
        Curve::new(30000.0, 20294.0, 0.0),
    ),
    abs(
        "evaluate.swipe",
        3000.0,
        Curve::new(7500.0, 668.0, 0.0),
        Curve::new(75000.0, 52924.0, 0.0),
    ),
    abs(
        "classify.return",
        1000.0,
        Curve::new(150.0, 0.0, 0.0),
        Curve::new(100.0, 6.0, 0.1),
    ),
    abs(
        "evaluate.return",
        2000.0,
        Curve::new(200.0, 1.0, 0.0),
        Curve::new(200.0, 6.0, 0.0),
    ),
    abs(
        "gate",
        30.0,
        Curve::new(30.0, 0.0, 0.0),
        Curve::new(30.0, 0.0, 0.0),
    ),
    abs(
        "summary.tap",
        1500.0,
        Curve::new(5000.0, 29.0, 0.0),
        Curve::new(75000.0, 48264.0, 0.0),
    ),
    abs(
        "recheck.holds_on",
        50.0,
        Curve::new(50.0, 5.0, 0.0),
        Curve::new(750.0, 1428.0, 0.0),
    ),
    abs(
        "recheck.core",
        3000.0,
        Curve::new(30000.0, 14.0, 8.6),
        Curve::new(75000.0, 70661.0, 0.0),
    ),
    abs(
        "step.observe_infer",
        7500.0,
        Curve::new(15000.0, 6863.0, 0.0),
        Curve::new(150000.0, 115572.0, 0.0),
    ),
    abs(
        "step.reply_act",
        2000.0,
        Curve::new(1500.0, 56.0, 0.0),
        Curve::new(15000.0, 14114.0, 0.0),
    ),
    abs(
        "step.round",
        7500.0,
        Curve::new(20000.0, 2200.0, 0.0),
        Curve::new(100000.0, 144593.0, 0.0),
    ),
    abs(
        "step.verify_infer",
        7500.0,
        Curve::new(15000.0, 2407.0, 0.0),
        Curve::new(150000.0, 156084.0, 0.0),
    ),
    abs(
        "step.approved_look",
        7500.0,
        Curve::new(50000.0, 604.0, 3.6),
        Curve::new(150000.0, 148674.0, 0.0),
    ),
    abs(
        "render.zero",
        750.0,
        Curve::new(750.0, 34.0, 0.0),
        Curve::new(10000.0, 7069.0, 0.0),
    ),
];

/// A scaling limit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ratio {
    /// Stage name, or `*` for every stage without an entry of its own.
    pub stage: &'static str,
    /// Layout the entry is for.
    pub view: Option<View>,
    /// Largest `time(20k) / time(10k)`.
    pub max_20: f64,
    /// Largest `time(50k) / time(10k)`.
    pub max_50: f64,
}

/// A stage faster than this at 10k nodes is too short to take a ratio of.
const RATIO_FLOOR_US: f64 = 150.0;

const fn ratio(stage: &'static str, view: Option<View>, max_20: f64, max_50: f64) -> Ratio {
    Ratio {
        stage,
        view,
        max_20,
        max_50,
    }
}

/// The scaling limits; the first matching entry wins.
pub(crate) const RATIOS: &[Ratio] = &[
    // Dense layouts, super-linear today. Measured (fastest run, three runs): 20k/10k and
    // 50k/10k in the comments; the limits are about 1.5 times the worst.
    //
    // The sweep is quadratic: every control scans every drawn node (`hit.rs` `blocker`).
    // Measured 2.6 to 6.5 and 32 to 57.
    ratio("tap.sweep", Some(View::Dense), 10.0, 90.0),
    // `changes.rs` `element_changes` looks every element up by a linear scan. 1.8 to 4.3 and
    // 8 to 20.
    ratio("step.verify_infer", Some(View::Dense), 6.5, 32.0),
    // `classify.rs` `beside_siblings`: the window of rows within 2 pt grows with density.
    // 1.7 to 3.9 and 5 to 20.
    ratio("classify.swipe", Some(View::Dense), 5.0, 32.0),
    ratio("evaluate.swipe", Some(View::Dense), 5.0, 25.0),
    ratio("classify.return", Some(View::Dense), 3.2, 28.0),
    ratio("evaluate.return", Some(View::Dense), 3.2, 20.0),
    // One scan over every drawn node per call (`hit.rs`), plus more candidate points when
    // the first is covered. 1.3 to 2.5 and 5 to 10.5.
    ratio("tap.one", Some(View::Dense), 4.0, 16.0),
    // Allocation-bound (the tree is thousands of small blocks): cache and allocator, not the
    // algorithm. 1.8 to 4.1 and 5 to 10.
    ratio("ingest.drop_tree", None, 5.5, 16.0),
    ratio("recheck.holds_on", Some(View::Dense), 4.0, 16.0),
    // The tap point of the model's target (`tap.one`) plus the classification: 1.6 to 2.3
    // and 5.8 to 10.4.
    ratio("step.reply_act", Some(View::Dense), 4.0, 16.0),
    ratio("render.full", Some(View::Dense), 4.0, 14.0),
    // The over-budget path builds the screen text about 2 log2(n) times (`render.rs`
    // `smallest_fitting`), so it is n log n. 2.0 to 3.8 and 4.6 to 6.7.
    ratio("render.budget", Some(View::Dense), 4.6, 14.0),
    ratio("render.zero", Some(View::Dense), 4.6, 14.0),
    // Everything else should be linear: 2 and 5 are the ideal; the limits leave room for
    // cache effects and a noisy machine, and stay far below a quadratic's 4 and 25.
    ratio("*", None, 3.4, 10.0),
];

fn view_name(v: Option<View>) -> &'static str {
    v.map_or("any", View::name)
}

fn ratio_for(stage: &str, view: View) -> Option<&'static Ratio> {
    RATIOS
        .iter()
        .find(|r| r.stage == stage && r.view.is_none_or(|v| v == view))
        .or_else(|| {
            RATIOS
                .iter()
                .find(|r| r.stage == "*" && r.view.is_none_or(|v| v == view))
        })
}

fn is_synthetic(m: &CaseMeta) -> bool {
    m.shape.is_some()
}

/// Compares `run` with the tables above.
pub(crate) fn check(run: &Run) -> Vec<Violation> {
    let mut out = Vec::new();
    for (meta, case) in &run.cases {
        for row in &case.rows {
            let Some(s) = row.stats else { continue };
            let Some(b) = ABSOLUTE.iter().find(|b| b.stage == row.stage) else {
                continue;
            };
            let limit = if is_synthetic(meta) {
                match meta.view {
                    Some(View::Page) => b.page.at(case.counts.nodes),
                    _ => b.dense.at(case.counts.nodes),
                }
            } else {
                b.fixtures_us
            };
            if s.p99_us > limit {
                out.push(Violation {
                    what: format!(
                        "{} {}: p99 {:.1} us is over the budget of {:.1} us",
                        meta.id, row.stage, s.p99_us, limit
                    ),
                });
            }
        }
    }
    for shape in Shape::ALL {
        for view in View::ALL {
            let find = |size: usize| {
                run.cases.iter().find(|(m, _)| {
                    m.shape == Some(shape) && m.view == Some(view) && m.size == Some(size)
                })
            };
            let Some((_, base)) = find(10_000) else {
                continue;
            };
            for row in &base.rows {
                let Some(b) = row.stats else { continue };
                if b.min_us < RATIO_FLOOR_US {
                    continue;
                }
                let Some(limit) = ratio_for(&row.stage, view) else {
                    continue;
                };
                for (size, max) in [(20_000, limit.max_20), (50_000, limit.max_50)] {
                    let Some((_, big)) = find(size) else { continue };
                    let Some(s) = big
                        .rows
                        .iter()
                        .find(|r| r.stage == row.stage)
                        .and_then(|r| r.stats)
                    else {
                        continue;
                    };
                    // Both the fastest runs and the medians must say so: one noisy sample
                    // on a busy machine moves one of them, not both.
                    let ratio = s.min_us / b.min_us;
                    let median_ratio = s.p50_us / b.p50_us;
                    if ratio > max && median_ratio > max {
                        out.push(Violation {
                            what: format!(
                                "{}-{}: {} takes {:.2}x as long at {}k nodes as at 10k (limit {:.2}, {} entry)",
                                shape.name(),
                                view.name(),
                                row.stage,
                                ratio,
                                size / 1000,
                                max,
                                view_name(limit.view)
                            ),
                        });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_add_their_terms() {
        let c = Curve {
            floor_us: 10.0,
            lin_ns: 100.0,
            quad_ps: 2.0,
        };
        // 10 + 100 * 1000 / 1000 + 2 * 1e6 / 1e6
        assert!((c.at(1000) - 112.0).abs() < 1e-9);
        assert!((Curve::new(5.0, 0.0, 0.0).at(99999) - 5.0).abs() < 1e-9);
        assert!((Curve::new(1.0, 1000.0, 0.0).at(1000) - 1001.0).abs() < 1e-9);
    }

    #[test]
    fn every_stage_has_one_entry_at_most() {
        for (i, a) in ABSOLUTE.iter().enumerate() {
            assert!(ABSOLUTE.iter().skip(i + 1).all(|b| b.stage != a.stage));
        }
    }
}
