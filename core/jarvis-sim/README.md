# jarvis-sim

The desktop stand-in for the phone and the model. It runs the real `jarvis-agent` `Session`
against a deterministic simulated iPhone, with a scripted model or any local LLM served over an
OpenAI-compatible HTTP API. Dev tool only: never shipped on the phone.

```
cd core
cargo run -p jarvis-sim -- apps                       # apps, screens, alerts
cargo run -p jarvis-sim -- dump --app shop --screen cart --set 'shop.cart=[{"name":"Desk Lamp","price":35.5}]'
cargo run -p jarvis-sim -- eval builtin               # 16 built-in tasks with bundled scripts
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
* `--memory file.json` loads the memory store if the file exists and writes it after the run, so
  running the same task twice shows learned-flow replay: the second run makes zero model calls.
* `eval` prints one row per task (pass/fail, steps, model calls, simulated time, confirmation
  categories) plus totals. It exits non-zero on failures only with `--strict`.

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
so each request also carries the action JSON schema as `response_format`. `--api-key` sends a
bearer token. Timeouts and HTTP errors become `Event::ModelFailed`. The simulator has no
screenshots, so a vision request is sent as text only and the transcript says so.

## Simulated apps

Apps are JSON files in `apps/`, embedded with `include_str!`. The schema (screens, elements with
relative frames, `column` layout, `scroll`, `for_each`, `when`/`enabled` conditions,
`{state}` templates, effects such as `goto`, `toggle`, `append`, `alert`) is documented in
`src/app.rs`; the expression language in `src/expr.rs`. Built in: `home` (SpringBoard),
`settings` (Wi-Fi, Bluetooth, Dark Mode), `messages`, `acme` (login), `shop`, `notes` (long list,
delete alert), `passwords` (denied by default policy).

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
