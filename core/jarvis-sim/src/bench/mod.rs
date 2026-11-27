//! `jarvis-sim bench`: timings of the core's per-observation work.
//!
//! See the README's Performance section for the command lines. The harness times, one
//! stage at a time and end to end, what the core does between "the shell hands over a tree"
//! and "the core answers with one effect", on the ten real iOS 26 trees and on synthetic
//! trees of chosen sizes and shapes. Everything is single threaded and deterministic in what
//! it measures; only the clock varies between runs.

mod budget;
pub mod fixtures;
mod gen;
mod memory;
mod report;
mod stages;
mod stats;

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use gen::{generate, GenSpec, Shape, View, DEFAULT_SEED};
use stages::{run_case, CaseResult, Cfg};
use stats::Plan;

const USAGE: &str = "\
jarvis-sim bench: time the core's per-observation work

USAGE
  jarvis-sim bench [--sizes N,N,..] [--shapes list,deep,web] [--views page,dense]
                   [--fixtures DIR | --no-fixtures] [--no-synthetic] [--no-memory]
                   [--stages PREFIX,..] [--iters N] [--budget-ms N] [--warmup N]
                   [--context-tokens N] [--sweep-cap-s S] [--seed N]
                   [--json PATH] [--check] [--long] [--quick] [--allow-debug]

Run it in a release build (`cargo run --release -p jarvis-sim -- bench`); a debug build is
10 to 50 times slower and `--check` refuses to run in one. Add `--features ffi` to include
the stages that cross the UniFFI bridge.

  --sizes         synthetic node counts (default 1000,5000,10000,20000,50000)
  --shapes        list (table of cells), deep (200 nested wrappers), web (web view of texts)
  --views         page (tall page, off-screen nodes hidden) and dense (all nodes on screen)
  --stages        only stages whose name starts with one of these (e.g. compact,tap)
  --iters         exactly this many timed iterations per stage (default: adaptive)
  --budget-ms     target timed time per stage (default 150)
  --context-tokens  the model's context window for prompt sizes (default 4096)
  --sweep-cap-s   sample a tap sweep estimated to take longer than this (default 4)
  --json PATH     write every number as JSON
  --check         compare against the budgets in the source; exit 1 on a violation
  --long          also print one line per stage and case
  --quick         sizes 1000,5000 and a shorter run (smoke test)
";

/// Options of one run.
#[derive(Debug, Clone)]
pub(crate) struct Opts {
    sizes: Vec<usize>,
    shapes: Vec<Shape>,
    views: Vec<View>,
    fixtures: Option<PathBuf>,
    use_fixtures: bool,
    synthetic: bool,
    memory: bool,
    stages: Vec<String>,
    iters: Option<usize>,
    budget_ms: u64,
    warmup: usize,
    context_tokens: u32,
    sweep_cap_s: f64,
    seed: u64,
    json: Option<String>,
    check: bool,
    long: bool,
    allow_debug: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            sizes: vec![1_000, 5_000, 10_000, 20_000, 50_000],
            shapes: Shape::ALL.to_vec(),
            views: View::ALL.to_vec(),
            fixtures: None,
            use_fixtures: true,
            synthetic: true,
            memory: true,
            stages: Vec::new(),
            iters: None,
            budget_ms: 150,
            warmup: 3,
            context_tokens: 4096,
            sweep_cap_s: 4.0,
            seed: DEFAULT_SEED,
            json: None,
            check: false,
            long: false,
            allow_debug: false,
        }
    }
}

fn list<T>(v: &str, one: impl Fn(&str) -> Option<T>, what: &str) -> Result<Vec<T>, String> {
    v.split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| one(s.trim()).ok_or_else(|| format!("unknown {what} `{s}`")))
        .collect()
}

fn parse(args: &[String]) -> Result<Option<Opts>, String> {
    let mut o = Opts::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let Some(name) = a.strip_prefix("--") else {
            return Err(format!("unexpected argument `{a}`"));
        };
        let (name, inline) = match name.split_once('=') {
            Some((n, v)) => (n, Some(v.to_owned())),
            None => (name, None),
        };
        match name {
            "help" => return Ok(None),
            "no-fixtures" => o.use_fixtures = false,
            "no-synthetic" => o.synthetic = false,
            "no-memory" => o.memory = false,
            "check" => o.check = true,
            "long" => o.long = true,
            "allow-debug" => o.allow_debug = true,
            "quick" => {
                o.sizes = vec![1_000, 5_000];
                o.budget_ms = 40;
                o.warmup = 1;
            }
            _ => {
                let v = match inline {
                    Some(v) => v,
                    None => it
                        .next()
                        .cloned()
                        .ok_or_else(|| format!("--{name} needs a value"))?,
                };
                let num = |v: &str| -> Result<u64, String> {
                    v.parse()
                        .map_err(|_| format!("--{name} expects a number, got `{v}`"))
                };
                match name {
                    "sizes" => {
                        o.sizes = list(&v, |s| s.parse().ok().filter(|n| *n >= 100), "size")?;
                    }
                    "shapes" => o.shapes = list(&v, Shape::parse, "shape")?,
                    "views" => o.views = list(&v, View::parse, "view")?,
                    "fixtures" => o.fixtures = Some(PathBuf::from(v)),
                    "stages" => o.stages = v.split(',').map(str::to_owned).collect(),
                    "iters" => o.iters = Some(usize::try_from(num(&v)?).unwrap_or(1)),
                    "budget-ms" => o.budget_ms = num(&v)?,
                    "warmup" => o.warmup = usize::try_from(num(&v)?).unwrap_or(1),
                    "context-tokens" => {
                        o.context_tokens = u32::try_from(num(&v)?).unwrap_or(4096);
                    }
                    "sweep-cap-s" => {
                        o.sweep_cap_s = v
                            .parse()
                            .map_err(|_| format!("--sweep-cap-s expects a number, got `{v}`"))?;
                    }
                    "seed" => o.seed = num(&v)?,
                    "json" => o.json = Some(v),
                    other => return Err(format!("unknown option --{other}")),
                }
            }
        }
    }
    o.sizes.sort_unstable();
    o.sizes.dedup();
    Ok(Some(o))
}

/// What kind of case a result is.
#[derive(Debug, Clone)]
pub(crate) struct CaseMeta {
    pub id: String,
    pub shape: Option<Shape>,
    pub view: Option<View>,
    pub size: Option<usize>,
}

impl CaseMeta {
    pub fn is_fixture(&self) -> bool {
        self.shape.is_none()
    }
}

/// Everything one run measured.
pub(crate) struct Run {
    pub cases: Vec<(CaseMeta, CaseResult)>,
    pub memory: Vec<(String, Vec<memory::MemoryPoint>)>,
    pub opts: Opts,
}

/// Entry point of `jarvis-sim bench`; `args` are the words after `bench`.
pub fn main_with(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let opts = match parse(args) {
        Ok(Some(o)) => o,
        Ok(None) => {
            let _ = write!(out, "{USAGE}");
            return 0;
        }
        Err(e) => {
            let _ = writeln!(err, "error: {e}\n\n{USAGE}");
            return 2;
        }
    };
    match run(&opts, out, err) {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            2
        }
    }
}

fn run(opts: &Opts, out: &mut dyn Write, err: &mut dyn Write) -> Result<i32, String> {
    if cfg!(debug_assertions) {
        let _ = writeln!(
            err,
            "warning: this is a debug build; the numbers say nothing about the shipped core"
        );
        if opts.check && !opts.allow_debug {
            return Err("--check needs a release build (or --allow-debug)".to_owned());
        }
    }
    let plan = Plan {
        warmup: opts.warmup,
        fixed_iters: opts.iters,
        budget: Duration::from_millis(opts.budget_ms),
        min_iters: if opts.budget_ms < 100 { 5 } else { 15 },
        ..Plan::standard()
    };
    let cfg = Cfg {
        plan,
        context_tokens: opts.context_tokens,
        sweep_cap_s: opts.sweep_cap_s,
        only: opts.stages.clone(),
    };

    let mut memory_rows = Vec::new();
    if opts.memory && opts.synthetic {
        for view in &opts.views {
            let base = generate(&GenSpec {
                shape: Shape::List,
                view: *view,
                nodes: 5_000,
                seed: opts.seed,
            });
            let targets = memory::targets_of(&base);
            let _ = writeln!(err, "[memory] list-5000-{}", view.name());
            if let Some(points) =
                memory::growth(&base, &targets, opts.context_tokens, 6, &[1, 10, 40])
            {
                memory_rows.push((format!("list-5000-{}", view.name()), points));
            }
        }
    }

    let mut plan_cases: Vec<(CaseMeta, Option<fixtures::Fixture>)> = Vec::new();
    if opts.use_fixtures {
        let dir = opts.fixtures.clone().unwrap_or_else(fixtures::default_dir);
        for f in fixtures::load_all(&dir)? {
            plan_cases.push((
                CaseMeta {
                    id: format!("fixture:{}", f.name),
                    shape: None,
                    view: None,
                    size: None,
                },
                Some(f),
            ));
        }
    }
    if opts.synthetic {
        for shape in &opts.shapes {
            for view in &opts.views {
                for size in &opts.sizes {
                    plan_cases.push((
                        CaseMeta {
                            id: format!("{}-{}-{}", shape.name(), size, view.name()),
                            shape: Some(*shape),
                            view: Some(*view),
                            size: Some(*size),
                        },
                        None,
                    ));
                }
            }
        }
    }

    let total = plan_cases.len();
    let mut cases = Vec::new();
    for (i, (meta, fixture)) in plan_cases.into_iter().enumerate() {
        let screen = match (&fixture, meta.shape, meta.view, meta.size) {
            (Some(f), ..) => f.screen.clone(),
            (None, Some(shape), Some(view), Some(nodes)) => generate(&GenSpec {
                shape,
                view,
                nodes,
                seed: opts.seed,
            }),
            _ => continue,
        };
        let _ = writeln!(err, "[{}/{total}] {}", i + 1, meta.id);
        let result = run_case(&meta.id, &screen, &cfg);
        cases.push((meta, result));
    }

    let run = Run {
        cases,
        memory: memory_rows,
        opts: opts.clone(),
    };
    report::print(&run, out);
    if let Some(path) = &opts.json {
        let json = report::to_json(&run);
        let text = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?;
        let _ = writeln!(err, "wrote {path}");
    }
    if opts.check {
        let violations = budget::check(&run);
        report::print_check(&violations, out);
        if !violations.is_empty() {
            return Ok(1);
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn options_parse_and_reject_the_unknown() {
        let o = parse(&args(&["--sizes", "300,100", "--shapes=list", "--check"]))
            .unwrap()
            .unwrap();
        assert_eq!(o.sizes, vec![100, 300]);
        assert_eq!(o.shapes, vec![Shape::List]);
        assert!(o.check);
        assert!(parse(&args(&["--nope", "1"])).is_err());
        assert!(parse(&args(&["--shapes", "tree"])).is_err());
        assert!(parse(&args(&["--help"])).unwrap().is_none());
    }
}
