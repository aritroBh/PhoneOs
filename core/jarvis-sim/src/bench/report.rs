//! Tables and JSON.

use std::io::Write;

use serde_json::{json, Value};

use super::budget::Violation;
use super::gen::{Shape, View};
use super::stages::{CaseResult, Row};
use super::stats::Stats;
use super::{CaseMeta, Run};

/// A ratio above this for 20k nodes over 10k is called super-linear.
pub(crate) const FLAG_20_OVER_10: f64 = 2.3;
/// A ratio above this for 50k nodes over 10k is called super-linear.
pub(crate) const FLAG_50_OVER_10: f64 = 6.0;

fn fmt_us(v: f64) -> String {
    if v < 10.0 {
        format!("{v:.2}")
    } else if v < 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.0}")
    }
}

fn fmt_ratio(v: Option<f64>) -> String {
    v.map_or_else(|| "-".to_owned(), |v| format!("{v:.2}"))
}

/// Prints `rows` under `header`, the first column left aligned and the others right aligned.
fn table(out: &mut dyn Write, header: &[String], rows: &[Vec<String>]) {
    let cols = header.len();
    let mut width = vec![0usize; cols];
    for r in std::iter::once(&header.to_vec()).chain(rows.iter()) {
        for (i, c) in r.iter().enumerate() {
            if let Some(w) = width.get_mut(i) {
                *w = (*w).max(c.chars().count());
            }
        }
    }
    let line = |r: &[String]| -> String {
        r.iter()
            .enumerate()
            .map(|(i, c)| {
                let w = width.get(i).copied().unwrap_or(0);
                if i == 0 {
                    format!("{c:<w$}")
                } else {
                    format!("{c:>w$}")
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    };
    let _ = writeln!(out, "{}", line(header));
    let _ = writeln!(
        out,
        "{}",
        "-".repeat(width.iter().sum::<usize>() + 2 * cols.saturating_sub(1))
    );
    for r in rows {
        let _ = writeln!(out, "{}", line(r));
    }
    let _ = writeln!(out);
}

fn stage_order(run: &Run) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    for (_, c) in &run.cases {
        for r in &c.rows {
            if !order.contains(&r.stage) {
                order.push(r.stage.clone());
            }
        }
    }
    order
}

fn find<'a>(c: &'a CaseResult, stage: &str) -> Option<&'a Row> {
    c.rows.iter().find(|r| r.stage == stage)
}

fn stats_of<'a>(c: &'a CaseResult, stage: &str) -> Option<&'a Stats> {
    find(c, stage).and_then(|r| r.stats.as_ref())
}

/// The ratios of one stage on one shape and view.
#[derive(Debug, Clone)]
pub(crate) struct Scaling {
    pub shape: Shape,
    pub view: View,
    pub stage: String,
    /// time(20k) / time(10k) by median and by fastest run.
    pub r20_p50: Option<f64>,
    pub r20_min: Option<f64>,
    /// time(50k) / time(10k) by median and by fastest run.
    pub r50_p50: Option<f64>,
    pub r50_min: Option<f64>,
}

impl Scaling {
    /// Whether the median ratios say super-linear (see [`FLAG_20_OVER_10`]).
    pub fn flagged(&self) -> bool {
        self.r20_p50.is_some_and(|r| r > FLAG_20_OVER_10)
            || self.r50_p50.is_some_and(|r| r > FLAG_50_OVER_10)
    }
}

fn case_at(run: &Run, shape: Shape, view: View, size: usize) -> Option<&CaseResult> {
    run.cases
        .iter()
        .find(|(m, _)| m.shape == Some(shape) && m.view == Some(view) && m.size == Some(size))
        .map(|(_, c)| c)
}

/// The scaling of every stage that was measured at 10k and at 20k or 50k nodes.
pub(crate) fn scaling(run: &Run) -> Vec<Scaling> {
    let mut out = Vec::new();
    let stages = stage_order(run);
    for shape in &run.opts.shapes {
        for view in &run.opts.views {
            let Some(base) = case_at(run, *shape, *view, 10_000) else {
                continue;
            };
            let (c20, c50) = (
                case_at(run, *shape, *view, 20_000),
                case_at(run, *shape, *view, 50_000),
            );
            for stage in &stages {
                let Some(b) = stats_of(base, stage) else {
                    continue;
                };
                let ratio = |c: Option<&CaseResult>, f: fn(&Stats) -> f64| -> Option<f64> {
                    let s = stats_of(c?, stage)?;
                    (f(b) > 0.0).then(|| f(s) / f(b))
                };
                let (r20_p50, r20_min) = (ratio(c20, |s| s.p50_us), ratio(c20, |s| s.min_us));
                let (r50_p50, r50_min) = (ratio(c50, |s| s.p50_us), ratio(c50, |s| s.min_us));
                out.push(Scaling {
                    shape: *shape,
                    view: *view,
                    stage: stage.clone(),
                    r20_p50,
                    r20_min,
                    r50_p50,
                    r50_min,
                });
            }
        }
    }
    out
}

fn kb(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / 1024.0)
}

/// Prints every table.
pub(crate) fn print(run: &Run, out: &mut dyn Write) {
    let _ = writeln!(
        out,
        "jarvis-sim bench ({} build, {} stages per case at most, plan: {} ms per stage, context {} tokens, seed {:#x})",
        if cfg!(debug_assertions) { "DEBUG" } else { "release" },
        stage_order(run).len(),
        run.opts.budget_ms,
        run.opts.context_tokens,
        run.opts.seed
    );
    let _ = writeln!(
        out,
        "times are microseconds; n = timed iterations, p99 of fewer than 100 samples is the slowest\n"
    );
    print_sizes(run, out);
    print_fixtures(run, out);
    print_synthetic(run, out);
    print_scaling(run, out);
    print_prompts(run, out);
    print_memory(run, out);
    if run.opts.long {
        print_long(run, out);
    }
    print_notes(run, out);
}

fn print_sizes(run: &Run, out: &mut dyn Write) {
    let _ = writeln!(out, "== what each case is to the core ==");
    let header: Vec<String> = [
        "case",
        "nodes",
        "elements",
        "raw drawn",
        "controls",
        "text KB",
    ]
    .map(str::to_owned)
    .to_vec();
    let rows: Vec<Vec<String>> = run
        .cases
        .iter()
        .map(|(m, c)| {
            vec![
                m.id.clone(),
                c.counts.nodes.to_string(),
                c.counts.elements.to_string(),
                c.counts.raw_nodes.to_string(),
                c.counts.controls.to_string(),
                kb(c.counts.text_bytes),
            ]
        })
        .collect();
    table(out, &header, &rows);
}

fn print_fixtures(run: &Run, out: &mut dyn Write) {
    let fx: Vec<&(CaseMeta, CaseResult)> =
        run.cases.iter().filter(|(m, _)| m.is_fixture()).collect();
    if fx.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "== real iOS 26 trees: p50 per stage (last columns: worst p99 and worst p50) =="
    );
    let mut header = vec!["stage".to_owned()];
    header.extend(fx.iter().map(|(m, _)| {
        m.id.trim_start_matches("fixture:")
            .chars()
            .take(12)
            .collect::<String>()
    }));
    header.push("max p99".to_owned());
    header.push("max p50".to_owned());
    let mut rows = Vec::new();
    for stage in stage_order(run) {
        let mut row = vec![stage.clone()];
        let (mut p99, mut p50) = (None::<f64>, None::<f64>);
        for (_, c) in &fx {
            match stats_of(c, &stage) {
                Some(s) => {
                    row.push(fmt_us(s.p50_us));
                    p99 = Some(p99.map_or(s.p99_us, |v| v.max(s.p99_us)));
                    p50 = Some(p50.map_or(s.p50_us, |v| v.max(s.p50_us)));
                }
                None => row.push("-".to_owned()),
            }
        }
        row.push(p99.map_or_else(|| "-".to_owned(), fmt_us));
        row.push(p50.map_or_else(|| "-".to_owned(), fmt_us));
        if fx.iter().any(|(_, c)| stats_of(c, &stage).is_some()) {
            rows.push(row);
        }
    }
    table(out, &header, &rows);
}

fn print_synthetic(run: &Run, out: &mut dyn Write) {
    for shape in &run.opts.shapes {
        for view in &run.opts.views {
            let cases: Vec<&(CaseMeta, CaseResult)> = run
                .cases
                .iter()
                .filter(|(m, _)| m.shape == Some(*shape) && m.view == Some(*view))
                .collect();
            if cases.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                "== {} / {}: p50 per stage by node count ==",
                shape.name(),
                view.name()
            );
            let mut header = vec!["stage".to_owned()];
            header.extend(cases.iter().map(|(m, _)| m.size.unwrap_or(0).to_string()));
            let mut rows = Vec::new();
            for stage in stage_order(run) {
                let mut row = vec![stage.clone()];
                for (_, c) in &cases {
                    row.push(
                        stats_of(c, &stage).map_or_else(|| "-".to_owned(), |s| fmt_us(s.p50_us)),
                    );
                }
                rows.push(row);
            }
            table(out, &header, &rows);
        }
    }
}

fn print_scaling(run: &Run, out: &mut dyn Write) {
    let all = scaling(run);
    if all.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "== scaling by median: time(20k)/time(10k)  time(50k)/time(10k); `!` marks above {FLAG_20_OVER_10} / {FLAG_50_OVER_10} (linear is 2.0 / 5.0) =="
    );
    let mut header = vec!["stage".to_owned()];
    let mut combos = Vec::new();
    for shape in &run.opts.shapes {
        for view in &run.opts.views {
            combos.push((*shape, *view));
            header.push(format!("{}/{} 20k", shape.name(), view.name()));
            header.push("50k".to_owned());
        }
    }
    let mut rows = Vec::new();
    for stage in stage_order(run) {
        let mut row = vec![stage.clone()];
        let mut any = false;
        for (shape, view) in &combos {
            let s = all
                .iter()
                .find(|s| s.shape == *shape && s.view == *view && s.stage == stage);
            any |= s.is_some();
            let mark = |r: Option<f64>, limit: f64| -> String {
                format!(
                    "{}{}",
                    fmt_ratio(r),
                    if r.is_some_and(|r| r > limit) {
                        "!"
                    } else {
                        ""
                    }
                )
            };
            row.push(mark(s.and_then(|s| s.r20_p50), FLAG_20_OVER_10));
            row.push(mark(s.and_then(|s| s.r50_p50), FLAG_50_OVER_10));
        }
        if any {
            rows.push(row);
        }
    }
    table(out, &header, &rows);
}

fn print_prompts(run: &Run, out: &mut dyn Write) {
    let _ = writeln!(
        out,
        "== prompt size (bytes/4 is the token proxy); screen budget is {} tokens for a {} token context ==",
        run.cases.first().map_or(0, |(_, c)| c.prompt.budget_tokens),
        run.opts.context_tokens
    );
    let header: Vec<String> = [
        "case",
        "elements",
        "full KB",
        "full tok",
        "label+value KB",
        "syntax KB",
        "kept elems",
        "kept KB",
        "sent user KB",
        "sent tok",
        "top roles by bytes",
    ]
    .map(str::to_owned)
    .to_vec();
    let rows: Vec<Vec<String>> = run
        .cases
        .iter()
        .map(|(m, c)| {
            let p = &c.prompt;
            let roles = p
                .by_role
                .iter()
                .take(3)
                .map(|(r, n, b)| {
                    format!(
                        "{r} {}% ({n})",
                        (*b * 100).checked_div(p.full_bytes).unwrap_or(0)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            vec![
                m.id.clone(),
                p.full_elements.to_string(),
                kb(p.full_bytes),
                (p.full_bytes / 4).to_string(),
                kb(p.text_bytes),
                kb(p.syntax_bytes),
                p.budget_elements.to_string(),
                kb(p.budget_bytes),
                kb(p.user_bytes),
                ((p.user_bytes + p.system_bytes) / 4).to_string(),
                roles,
            ]
        })
        .collect();
    table(out, &header, &rows);
}

fn print_memory(run: &Run, out: &mut dyn Write) {
    if run.memory.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "== session memory: resident growth per session after N steps, KB (coarse: includes allocator slack) =="
    );
    let header: Vec<String> = ["screen", "1 step", "10 steps", "40 steps", "growth 1 to 40"]
        .map(str::to_owned)
        .to_vec();
    let rows: Vec<Vec<String>> = run
        .memory
        .iter()
        .map(|(name, pts)| {
            let at = |s: u32| pts.iter().find(|p| p.steps == s).map(|p| p.kb_per_session);
            let grow = match (at(1), at(40)) {
                (Some(a), Some(b)) => format!("{:.0}", b - a),
                _ => "-".to_owned(),
            };
            let f = |v: Option<f64>| v.map_or_else(|| "-".to_owned(), |v| format!("{v:.0}"));
            vec![name.clone(), f(at(1)), f(at(10)), f(at(40)), grow]
        })
        .collect();
    table(out, &header, &rows);
}

fn print_long(run: &Run, out: &mut dyn Write) {
    let _ = writeln!(out, "== every stage of every case ==");
    let header: Vec<String> = ["case", "stage", "n", "min", "p50", "p99", "mean", "note"]
        .map(str::to_owned)
        .to_vec();
    let mut rows = Vec::new();
    for (m, c) in &run.cases {
        for r in &c.rows {
            let (n, a, b, c2, d) = match &r.stats {
                Some(s) => (
                    s.n.to_string(),
                    fmt_us(s.min_us),
                    fmt_us(s.p50_us),
                    fmt_us(s.p99_us),
                    fmt_us(s.mean_us),
                ),
                None => (
                    "-".to_owned(),
                    "-".to_owned(),
                    "-".to_owned(),
                    "-".to_owned(),
                    "-".to_owned(),
                ),
            };
            rows.push(vec![
                m.id.clone(),
                r.stage.clone(),
                n,
                a,
                b,
                c2,
                d,
                r.note.clone().unwrap_or_default(),
            ]);
        }
    }
    table(out, &header, &rows);
}

fn print_notes(run: &Run, out: &mut dyn Write) {
    let notes: Vec<String> = run
        .cases
        .iter()
        .flat_map(|(_, c)| {
            c.rows.iter().filter_map(move |r| {
                r.note
                    .as_ref()
                    .map(|n| format!("{} {}: {n}", r.case, r.stage))
            })
        })
        .collect();
    if !notes.is_empty() {
        let _ = writeln!(out, "== notes ==");
        for n in notes {
            let _ = writeln!(out, "{n}");
        }
        let _ = writeln!(out);
    }
}

/// Prints the outcome of `--check`.
pub(crate) fn print_check(violations: &[Violation], out: &mut dyn Write) {
    if violations.is_empty() {
        let _ = writeln!(out, "check: ok, every budget held");
        return;
    }
    let _ = writeln!(out, "check: {} budget(s) violated", violations.len());
    for v in violations {
        let _ = writeln!(out, "  {v}");
    }
}

fn stats_json(s: &Stats) -> Value {
    json!({
        "n": s.n,
        "min_us": s.min_us,
        "p50_us": s.p50_us,
        "p99_us": s.p99_us,
        "mean_us": s.mean_us,
    })
}

/// Every number of the run as JSON.
pub(crate) fn to_json(run: &Run) -> Value {
    let cases: Vec<Value> = run
        .cases
        .iter()
        .map(|(m, c)| {
            json!({
                "id": m.id,
                "kind": if m.is_fixture() { "fixture" } else { "synthetic" },
                "shape": m.shape.map(Shape::name),
                "view": m.view.map(View::name),
                "size": m.size,
                "nodes": c.counts.nodes,
                "elements": c.counts.elements,
                "raw_drawn_nodes": c.counts.raw_nodes,
                "controls": c.counts.controls,
                "runner_text_bytes": c.counts.text_bytes,
                "prompt": {
                    "full_bytes": c.prompt.full_bytes,
                    "full_elements": c.prompt.full_elements,
                    "label_value_bytes": c.prompt.text_bytes,
                    "syntax_bytes": c.prompt.syntax_bytes,
                    "screen_budget_tokens": c.prompt.budget_tokens,
                    "budget_bytes": c.prompt.budget_bytes,
                    "budget_elements": c.prompt.budget_elements,
                    "system_bytes": c.prompt.system_bytes,
                    "user_bytes": c.prompt.user_bytes,
                    "user_elements": c.prompt.user_elements,
                    "by_role": c.prompt.by_role.iter()
                        .map(|(r, n, b)| json!({"role": r, "elements": n, "bytes": b}))
                        .collect::<Vec<_>>(),
                },
            })
        })
        .collect();
    let rows: Vec<Value> = run
        .cases
        .iter()
        .flat_map(|(m, c)| {
            c.rows.iter().map(move |r| {
                let mut v = match &r.stats {
                    Some(s) => stats_json(s),
                    None => json!({}),
                };
                if let Some(o) = v.as_object_mut() {
                    o.insert("case".to_owned(), json!(m.id));
                    o.insert("stage".to_owned(), json!(r.stage));
                    o.insert("note".to_owned(), json!(r.note));
                }
                v
            })
        })
        .collect();
    let scaling: Vec<Value> = scaling(run)
        .iter()
        .map(|s| {
            json!({
                "shape": s.shape.name(),
                "view": s.view.name(),
                "stage": s.stage,
                "r20_over_10_p50": s.r20_p50,
                "r50_over_10_p50": s.r50_p50,
                "r20_over_10_min": s.r20_min,
                "r50_over_10_min": s.r50_min,
                "flagged": s.flagged(),
            })
        })
        .collect();
    let memory: Vec<Value> = run
        .memory
        .iter()
        .map(|(name, pts)| {
            json!({
                "screen": name,
                "points": pts.iter()
                    .map(|p| json!({"steps": p.steps, "kb_per_session": p.kb_per_session}))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "tool": "jarvis-sim bench",
        "release": !cfg!(debug_assertions),
        "plan": {
            "budget_ms": run.opts.budget_ms,
            "warmup": run.opts.warmup,
            "iters": run.opts.iters,
            "context_tokens": run.opts.context_tokens,
            "sweep_cap_s": run.opts.sweep_cap_s,
            "seed": run.opts.seed,
            "sizes": run.opts.sizes,
        },
        "cases": cases,
        "rows": rows,
        "scaling": scaling,
        "memory": memory,
    })
}
