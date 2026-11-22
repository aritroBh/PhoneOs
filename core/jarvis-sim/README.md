# jarvis-sim

The desktop stand-in for the phone and the model. It runs the real `jarvis-agent` `Session`
against a deterministic simulated iPhone, with a scripted model or any local LLM served over an
OpenAI-compatible HTTP API. Dev tool only: never shipped on the phone.

```
cd core
cargo run -p jarvis-sim -- apps                       # apps, screens, alerts
cargo run -p jarvis-sim -- dump --app shop --screen cart --set 'shop.cart=[{"name":"Desk Lamp","price":35.5}]'
cargo run -p jarvis-sim -- eval builtin               # 36 built-in tasks with bundled scripts
cargo run -p jarvis-sim -- run --task "Turn off Wi-Fi" --start-app settings
```

The last command works offline because the task text matches a built-in task and its bundled
script is used. Scripts are one model output per line; `"element":"@Switch:Wi-Fi"` resolves by
role and label against the screen in the prompt (`@Label`, `@Role:Label`, `@Label#2`), so they do
not depend on element numbers.

## Commands

```
jarvis-sim run --task "..." [--start-app settings] [--mode structured|runner-text]
               [--model-url URL --model NAME] [--script file.jsonl] [--approve|--decline]
               [--context-tokens N] [--max-steps N] [--grammar] [--memory file.json]
               [--step-by-step] [--no-first-use] [--approve-app APP]...
jarvis-sim eval <dir|file|builtin> [same flags] [--filter TEXT] [--json] [--strict]
jarvis-sim apps
jarvis-sim dump --app ID --screen NAME [--mode ...] [--tokens N] [--set app.path=JSON]
```

* `--mode structured` hands the agent the `UiNode` tree as built (focus, enabled, hittable flags
  intact). `--mode runner-text` serialises it to the LoopCtrl text format and parses it back with
  `jarvis_observe::parse_runner_text`, so those flags are lost exactly as on a device. Run your
  evals in both.
* `--approve` / `--decline` answer every confirmation (default: approve, or the task's own owner
  spec in `eval`). An explicit flag wins over the task.
* First-use approval is on by default, as in `Policy::default()`: the first action inside each
  app asks `first_use` (the built-in tasks expect it). `--no-first-use` turns it off,
  `--approve-app APP` (sim app id or bundle id, repeatable) pre-approves an app, and
  `--step-by-step` confirms every state-changing action. Task files can set the same through
  `"policy": {"step_by_step", "first_use_approval", "app_rules", "approved_apps"}`, and check
  `forbid_confirm` and `expect_confirm_count` besides `expect_confirm`.
* `--memory file.json` loads the memory store if the file exists and writes it after the run, so
  running the same task twice shows learned-flow replay: the second run makes zero model calls.
* `eval` prints one row per task as it finishes (pass/fail, steps, model calls, simulated time,
  confirmation categories) plus totals. It exits non-zero on failures only with `--strict`.
  `--json` prints one JSON object per line per finished task, flushed at once (so a killed run
  keeps what finished), each with a `transcript` (system prompt, every prompt, reply and
  effect, and the `response_format` the HTTP client used), then a totals object.
* Task predicates may use `has(text, part)`, a normalised contains (case, punctuation and
  apostrophes ignored, whole words) for text a model typed or sent. `expect_status` is one
  status or a list (`["failed", "blocked"]`).
* Script lines may be conditional: `{"if_prompt": "TEXT", "then": OUTPUT, "else": OUTPUT}`
  serves `then` once the prompt contains `TEXT` and repeats `else` until then (a scripted
  model that reacts to feedback, e.g. `cart_add_once`).

## Pointing it at a local model

The HTTP client is behind the `http` feature (plain `http://`, no TLS):

```
# Ollama (default port 11434)
ollama pull qwen2.5:3b-instruct
cargo run -p jarvis-sim --features http -- run --task "Turn off Wi-Fi" --start-app settings \
  --model-url http://localhost:11434/v1 --model qwen2.5:3b-instruct --context-tokens 4096

# LM Studio (start the local server, default port 1234; use the model id it shows)
cargo run -p jarvis-sim --features http -- eval builtin \
  --model-url http://localhost:1234/v1 --model qwen2.5-3b-instruct --timeout 300 --json

# llama.cpp server: --model-url http://localhost:8080/v1
```

Requests are `POST {url}/chat/completions` with a system and a user message, `max_tokens`,
`temperature: 0` and no streaming. `--grammar` marks the model as supporting constrained output,
so each request also carries the action JSON schema as `response_format` (`json_schema`); a
server that rejects that (4xx/5xx naming the response format, as llama-cpp-python's server
does) gets the request again with `json_object` plus the schema, then with no response format,
and the client keeps the mode that worked (shown in the transcript and the `--json` output). `--api-key` sends a
bearer token. Timeouts and HTTP errors become `Event::ModelFailed`. The simulator has no
screenshots, so a vision request is sent as text only and the transcript says so.

## Simulated apps

Apps are JSON files in `apps/`, embedded with `include_str!`. The schema (screens, elements with
relative frames, `column` layout, `scroll`, `for_each`, `when`/`enabled` conditions,
`{state}` templates, effects such as `goto`, `toggle`, `append`, `alert`) is documented in
`src/app.rs`; the expression language in `src/expr.rs`. Built in: `home` (SpringBoard),
`settings` (Wi-Fi, Bluetooth, Dark Mode), `messages`, `acme` (login), `shop`, `notes` (long list,
delete alert), `passwords` (denied by default policy), `phone`, `laden` (German checkout) and
`news` (a dense web page of 60 long headlines that overflows a 4096-token prompt, so labels are
cut to fit). Scrollable views report an iOS-style `Vertical scroll bar, N pages` indicator with
the position as value, which the agent turns into a `… more below; scroll down` cue.

Eval tasks are JSON files in `tasks/` (`id`, `task`, `start_app`, `success` predicate such as
`messages.thread["Mom"].last == "running late"`, `expect_confirm`, `expect_status`, `max_steps`,
`human`, bundled `script`). Predicates see every app's state under its id plus
`phone.app/screen/alert`.

## Library

`run(session, &mut phone, &mut model, &mut human) -> RunReport` loops effect, IO, event with a hard
iteration cap. The report has the outcome, a readable transcript, counters (steps, model calls,
parse retries, confirmations), every runner command, request and confirmation, the final
`SimSnapshot` for assertions (`snapshot.check("settings.wifi == false")`) and the updated memory.
Faults: `SimPhone::fail_act_at(n)`, `ScriptedModel::fail_at(n)`, `RunOptions::cancel_at_effect`.
The simulated clock advances by `ClockCosts` per effect, so time budgets are testable.

## Performance

`jarvis-sim bench` times what the core does per observation: ingest of the tree, compaction,
the raw digest, prompt rendering, tap points (one control and the sweep of every control),
classify / evaluate / gate for a tap, a type, a swipe and a Return, and whole `Session::step`
rounds (observation to prompt, model reply to action, the look after an action, the fresh look
after an approval), on the ten real iOS 26 trees and on synthetic trees (list, deep, web;
`page` layout with off-screen nodes hidden as the runner reports them, `dense` layout with every
node on screen). It uses only `std::time::Instant`, is single threaded, and the synthetic trees
come from a fixed seed.

```
cd core
cargo run --release -p jarvis-sim -- bench                          # full matrix, tables
cargo run --release -p jarvis-sim --features ffi -- bench           # plus the UniFFI lift/convert stages
cargo run --release -p jarvis-sim -- bench --json out.json          # every number, machine readable
cargo run --release -p jarvis-sim -- bench --sizes 5000,10000,20000 --check   # budgets (CI)
cargo run --release -p jarvis-sim -- bench --quick --no-memory      # smoke run
```

Times are microseconds: min, p50, p99 and mean over an adaptive number of iterations (`--iters`
fixes it; the p99 of fewer than 100 samples is the slowest). Use a release build: debug is 10 to
50 times slower and `--check` refuses to run in it. The workspace release profile is
`opt-level = "s"`, which is what the phone gets. `--check` compares against the tables in
`src/bench/budget.rs`: absolute p99 limits and machine-independent scaling ratios (time at 20k
nodes over 10k, 50k over 10k). Session memory is read from the process's resident set (Linux
only; the workspace forbids the `unsafe` a counting allocator needs), so it is coarse.
