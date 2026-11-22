//! The `jarvis-sim` command line: `run`, `eval`, `apps`, `dump`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

use jarvis_memory::MemoryStore;
use jarvis_observe::{compact, render};
use serde_json::Value;

use crate::builtin::{builtin_apps, builtin_tasks};
use crate::driver::RunReport;
use crate::error::SimError;
use crate::eval::{format_table, human_for, run_one, TaskDef, TaskResult};
use crate::harness::{run_task, RunSetup};
use crate::human::Human;
use crate::model::{Model, ScriptedModel};
use crate::phone::{ObserveMode, SimPhone};

const USAGE: &str = "\
jarvis-sim: run the Jarvis agent against a simulated phone

USAGE
  jarvis-sim run --task \"...\" [--start-app home|settings|...] [--mode structured|runner-text]
                 [--model-url URL --model NAME [--api-key KEY] [--timeout SECS]]
                 [--script FILE.jsonl] [--approve | --decline] [--context-tokens N]
                 [--max-steps N] [--grammar] [--memory FILE.json]
                 [--step-by-step] [--no-first-use] [--approve-app APP]...
  jarvis-sim eval <DIR | FILE | builtin> [same model, mode and owner flags]
                 [--filter TEXT] [--json] [--strict]
  jarvis-sim apps
  jarvis-sim dump --app ID --screen NAME [--mode structured|runner-text] [--tokens N]
                 [--set app.path=JSON]...

MODELS
  --model-url/--model   an OpenAI-compatible server (build with --features http)
  --script FILE         one model output per line; `\"element\":\"@Label\"` resolves by label
  (neither)             a built-in task's bundled script, matched by its task text

POLICY
  --step-by-step        confirm every state-changing action (StepByStep mode)
  --no-first-use        do not ask before the first action inside each app
  --approve-app APP     treat APP (sim app id or bundle id) as already approved; repeatable

--approve/--decline answer every confirmation (default: approve). `eval` exits non-zero on
failed tasks only with --strict.
";

#[derive(Debug, Default)]
struct Args {
    positional: Vec<String>,
    opts: BTreeMap<String, Vec<String>>,
    flags: BTreeSet<String>,
}

const BOOL_FLAGS: [&str; 8] = [
    "approve",
    "decline",
    "json",
    "strict",
    "grammar",
    "help",
    "step-by-step",
    "no-first-use",
];

/// Options every command that runs a session accepts, besides its own.
const POLICY_OPTS: [&str; 3] = ["step-by-step", "no-first-use", "approve-app"];

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let Some(name) = a.strip_prefix("--") else {
            out.positional.push(a.clone());
            continue;
        };
        let (name, inline) = match name.split_once('=') {
            Some((n, v)) => (n, Some(v.to_owned())),
            None => (name, None),
        };
        if BOOL_FLAGS.contains(&name) {
            out.flags.insert(name.to_owned());
            continue;
        }
        let value = match inline {
            Some(v) => v,
            None => it
                .next()
                .cloned()
                .ok_or_else(|| format!("--{name} needs a value"))?,
        };
        out.opts.entry(name.to_owned()).or_default().push(value);
    }
    Ok(out)
}

impl Args {
    fn get(&self, name: &str) -> Option<&str> {
        self.opts
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    fn all(&self, name: &str) -> &[String] {
        self.opts.get(name).map_or(&[], Vec::as_slice)
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    fn number<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, String> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => v
                .parse()
                .map(Some)
                .map_err(|_| format!("--{name} expects a number, got `{v}`")),
        }
    }

    fn check_known(&self, known: &[&str]) -> Result<(), String> {
        for k in self.opts.keys().chain(self.flags.iter()) {
            if !known.contains(&k.as_str()) {
                return Err(format!("unknown option --{k}"));
            }
        }
        Ok(())
    }
}

fn setup_from(args: &Args) -> Result<RunSetup, String> {
    let mut setup = RunSetup::default();
    if let Some(m) = args.get("mode") {
        setup.mode = ObserveMode::parse(m)
            .ok_or_else(|| format!("--mode must be structured or runner-text, got `{m}`"))?;
    }
    if let Some(n) = args.number::<u32>("context-tokens")? {
        setup.context_tokens = n;
    }
    if let Some(n) = args.number::<u32>("max-steps")? {
        setup.max_steps = Some(n);
    }
    setup.grammar = args.flag("grammar");
    setup.step_by_step = args.flag("step-by-step");
    setup.first_use_approval = !args.flag("no-first-use");
    if !args.all("approve-app").is_empty() {
        let apps = builtin_apps().map_err(|e| e.to_string())?;
        for a in args.all("approve-app") {
            let id = apps
                .iter()
                .find(|d| d.id == *a)
                .map_or_else(|| a.clone(), |d| d.bundle_id.clone());
            setup.approved_apps.push(id);
        }
    }
    Ok(setup)
}

fn approve_choice(args: &Args) -> Result<Option<bool>, String> {
    match (args.flag("approve"), args.flag("decline")) {
        (true, true) => Err("--approve and --decline are mutually exclusive".to_owned()),
        (true, false) => Ok(Some(true)),
        (false, true) => Ok(Some(false)),
        (false, false) => Ok(None),
    }
}

#[cfg(feature = "http")]
fn http_model(args: &Args) -> Result<Option<Box<dyn Model>>, String> {
    let Some(url) = args.get("model-url") else {
        return Ok(None);
    };
    let name = args.get("model").ok_or("--model-url needs --model NAME")?;
    let mut m = crate::http::HttpModel::new(url, name);
    if let Some(secs) = args.number::<u64>("timeout")? {
        m = m.with_timeout(std::time::Duration::from_secs(secs));
    }
    if let Some(k) = args.get("api-key") {
        m = m.with_api_key(k);
    }
    Ok(Some(Box::new(m)))
}

#[cfg(not(feature = "http"))]
fn http_model(args: &Args) -> Result<Option<Box<dyn Model>>, String> {
    if args.get("model-url").is_some() {
        return Err(
            "this build has no HTTP model client; rebuild with `--features http`".to_owned(),
        );
    }
    Ok(None)
}

fn read(path: &str) -> Result<String, SimError> {
    std::fs::read_to_string(path).map_err(|source| SimError::Io {
        path: path.to_owned(),
        source,
    })
}

fn load_tasks(spec: &str) -> Result<Vec<TaskDef>, SimError> {
    if spec == "builtin" {
        return builtin_tasks();
    }
    let path = Path::new(spec);
    let mut files = Vec::new();
    if path.is_dir() {
        let entries = std::fs::read_dir(path).map_err(|source| SimError::Io {
            path: spec.to_owned(),
            source,
        })?;
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "json") {
                files.push(p.to_string_lossy().into_owned());
            }
        }
        files.sort();
    } else {
        files.push(spec.to_owned());
    }
    let mut tasks = Vec::new();
    for f in files {
        let text = read(&f)?;
        let value: Value =
            serde_json::from_str(&text).map_err(|e| SimError::Input(format!("{f}: {e}")))?;
        let items = match value {
            Value::Array(a) => a,
            one => vec![one],
        };
        for item in items {
            tasks.push(
                serde_json::from_value(item).map_err(|e| SimError::Input(format!("{f}: {e}")))?,
            );
        }
    }
    Ok(tasks)
}

fn print_report(out: &mut dyn Write, report: &RunReport) {
    let _ = write!(out, "{}", report.transcript());
    let c = &report.counts;
    let _ = writeln!(
        out,
        "counts: {} steps, {} model calls ({} failed, {} parse retries), {} runner commands ({} failed), {} confirmations ({} approved), {} questions",
        report.outcome.steps,
        c.model_calls,
        c.model_failures,
        c.parse_retries,
        c.acts,
        c.act_failures,
        c.confirmations,
        c.approvals,
        c.questions
    );
    let f = &report.final_state;
    let _ = writeln!(
        out,
        "final: {}/{}{}",
        f.app,
        f.screen,
        f.alert
            .as_deref()
            .map_or(String::new(), |a| format!(" (alert {a})"))
    );
}

fn cmd_run(args: &Args, out: &mut dyn Write) -> Result<i32, String> {
    args.check_known(&[
        "task",
        "start-app",
        "mode",
        "model-url",
        "model",
        "api-key",
        "timeout",
        "script",
        "approve",
        "decline",
        "context-tokens",
        "max-steps",
        "grammar",
        "memory",
        POLICY_OPTS[0],
        POLICY_OPTS[1],
        POLICY_OPTS[2],
    ])?;
    let task = args.get("task").ok_or("run needs --task \"...\"")?;
    let setup = setup_from(args)?;
    let start = args.get("start-app").unwrap_or("home");
    let approve = approve_choice(args)?;
    let builtin = builtin_tasks().map_err(|e| e.to_string())?;
    let matched = builtin.iter().find(|t| t.task == task);
    let mut model: Box<dyn Model> = if let Some(m) = http_model(args)? {
        m
    } else if let Some(file) = args.get("script") {
        Box::new(ScriptedModel::from_jsonl(
            &read(file).map_err(|e| e.to_string())?,
        ))
    } else if let Some(t) = matched {
        Box::new(ScriptedModel::new(t.script.clone()))
    } else {
        return Err(
            "no model: pass --model-url and --model, --script FILE, or use a built-in task text"
                .to_owned(),
        );
    };
    let mut human = match matched {
        Some(t) => human_for(t, approve),
        None if approve == Some(false) => Human::decline_all(),
        None => Human::approve_all(),
    };
    let memory = match args.get("memory") {
        Some(p) if Path::new(p).exists() => {
            MemoryStore::from_json(&read(p).map_err(|e| e.to_string())?)
                .map_err(|e| format!("{p}: {e}"))?
        }
        _ => MemoryStore::new(),
    };
    let flows_before = memory.flows.len();
    let _ = writeln!(out, "task: {task}");
    let _ = writeln!(
        out,
        "start app: {start}, mode: {}, model: {}, owner {}, memory flows: {flows_before}",
        setup.mode.name(),
        model.name(),
        if approve == Some(false) {
            "declines"
        } else {
            "approves"
        },
    );
    let _ = writeln!(
        out,
        "policy: {}, first-use approval {}{}",
        if setup.step_by_step {
            "step-by-step"
        } else {
            "guarded"
        },
        if setup.first_use_approval {
            "on"
        } else {
            "off"
        },
        if setup.approved_apps.is_empty() {
            String::new()
        } else {
            format!(", pre-approved: {}", setup.approved_apps.join(", "))
        },
    );
    let report = run_task(
        &setup,
        start,
        "cli",
        task,
        model.as_mut(),
        &mut human,
        memory,
    )
    .map_err(|e| e.to_string())?;
    print_report(out, &report);
    if let Some(p) = args.get("memory") {
        let json = report.memory.to_json().map_err(|e| e.to_string())?;
        std::fs::write(p, json).map_err(|e| format!("{p}: {e}"))?;
        let _ = writeln!(
            out,
            "memory: {} flows saved to {p} (was {flows_before})",
            report.memory.flows.len()
        );
    }
    Ok(0)
}

fn cmd_eval(args: &Args, out: &mut dyn Write) -> Result<i32, String> {
    args.check_known(&[
        "mode",
        "model-url",
        "model",
        "api-key",
        "timeout",
        "approve",
        "decline",
        "context-tokens",
        "max-steps",
        "grammar",
        "filter",
        "json",
        "strict",
        POLICY_OPTS[0],
        POLICY_OPTS[1],
        POLICY_OPTS[2],
    ])?;
    let spec = args
        .positional
        .get(1)
        .ok_or("eval needs a task directory, a task file or `builtin`")?;
    let setup = setup_from(args)?;
    let approve = approve_choice(args)?;
    let mut tasks = load_tasks(spec).map_err(|e| e.to_string())?;
    if let Some(f) = args.get("filter") {
        tasks.retain(|t| t.id.contains(f));
    }
    if tasks.is_empty() {
        return Err("no tasks to run".to_owned());
    }
    let mut results: Vec<TaskResult> = Vec::new();
    for t in &tasks {
        let model: Option<Box<dyn Model>> = match http_model(args)? {
            Some(m) => Some(m),
            None if t.script.is_empty() => None,
            None => Some(Box::new(ScriptedModel::new(t.script.clone()))),
        };
        let r = match model {
            Some(mut m) => run_one(t, &setup, m.as_mut(), approve).map_err(|e| e.to_string())?,
            None => TaskResult {
                id: t.id.clone(),
                passed: false,
                status: "no model".to_owned(),
                steps: 0,
                model_calls: 0,
                sim_ms: 0,
                real_model_ms: 0,
                confirmations: Vec::new(),
                reasons: vec![
                    "no model: the task has no script and --model-url was not given".to_owned(),
                ],
            },
        };
        results.push(r);
    }
    if args.flag("json") {
        let json = serde_json::to_string_pretty(&results).map_err(|e| e.to_string())?;
        let _ = writeln!(out, "{json}");
    } else {
        let _ = write!(out, "{}", format_table(&results));
    }
    let failed = results.iter().any(|r| !r.passed);
    Ok(i32::from(failed && args.flag("strict")))
}

fn cmd_apps(out: &mut dyn Write) -> Result<i32, String> {
    for def in builtin_apps().map_err(|e| e.to_string())? {
        let _ = writeln!(
            out,
            "{:<10} {:<26} {} (start: {})",
            def.id, def.bundle_id, def.name, def.start
        );
        for (name, s) in &def.screens {
            let _ = writeln!(
                out,
                "    screen {name:<14} {}",
                s.title.as_deref().unwrap_or("(no navigation bar)")
            );
        }
        for name in def.alerts.keys() {
            let _ = writeln!(out, "    alert  {name}");
        }
    }
    Ok(0)
}

fn cmd_dump(args: &Args, out: &mut dyn Write) -> Result<i32, String> {
    args.check_known(&["app", "screen", "mode", "tokens", "set"])?;
    let app = args.get("app").ok_or("dump needs --app ID")?;
    let screen = args.get("screen").ok_or("dump needs --screen NAME")?;
    let setup = setup_from(args)?;
    let tokens = args.number::<u32>("tokens")?.unwrap_or(1_500);
    let mut phone: SimPhone = setup.new_phone("home").map_err(|e| e.to_string())?;
    for s in args.all("set") {
        let (path, json) = s.split_once('=').ok_or("--set expects app.path=JSON")?;
        let (a, p) = path.split_once('.').ok_or("--set expects app.path=JSON")?;
        let v = serde_json::from_str(json).unwrap_or_else(|_| Value::String(json.to_owned()));
        phone.set_state(a, p, v).map_err(|e| e.to_string())?;
    }
    phone.show_screen(app, screen).map_err(|e| e.to_string())?;
    let obs = phone.observe().map_err(|e| e.to_string())?;
    let compacted = compact(&obs.screen);
    let _ = writeln!(
        out,
        "# {app}/{screen} as the model sees it ({} mode)",
        setup.mode.name()
    );
    let _ = writeln!(out, "{}", render(&compacted, tokens));
    Ok(0)
}

/// Runs the command line and returns the process exit code.
pub fn main_with(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(err, "error: {e}\n\n{USAGE}");
            return 2;
        }
    };
    if parsed.flag("help") || parsed.positional.first().is_none_or(|c| c == "help") {
        let _ = write!(out, "{USAGE}");
        return i32::from(parsed.positional.is_empty() && !parsed.flag("help")) * 2;
    }
    let result = match parsed.positional.first().map(String::as_str) {
        Some("run") => cmd_run(&parsed, out),
        Some("eval") => cmd_eval(&parsed, out),
        Some("apps") => cmd_apps(out),
        Some("dump") => cmd_dump(&parsed, out),
        Some(other) => Err(format!("unknown command `{other}`")),
        None => Ok(0),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(err, "error: {e}");
            2
        }
    }
}
