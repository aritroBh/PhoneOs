# Jarvis architecture: Rust core, Swift shell

Read `docs/PLAN.md` first for the product context. This file is the engineering contract.

## Split

| Layer | Language | Owns |
|---|---|---|
| Core | Rust | All decision logic: agent loop, action parsing, tree compaction, prompt building, safety policy, tool routing, learned flows. No IO, no clock, no threads. |
| Shell | Swift | All IO: model inference, runner RPC, native tools, Shortcuts, voice, UI, storage, notifications. |
| Bridge | UniFFI | Generated Swift bindings over the core. |

The core is **sans-IO**: a state machine the shell drives. Every input is an `Event`, every output is one `Effect`. Same events in, same effects out. That makes the whole agent testable on Linux against recorded screens, and keeps async out of the FFI.

```
Swift shell                              Rust core
───────────                              ─────────
session.step(Event::Start)        ─────▶ Effect::Observe { screenshot: false }
runner.snapshot()
session.step(Event::Observed(s))  ─────▶ Effect::Infer(ModelRequest)
model.generate(request)
session.step(Event::ModelOutput)  ─────▶ Effect::Act(RunnerCommand::Tap { x, y })
runner.tap(x, y)
session.step(Event::ActResult)    ─────▶ Effect::Observe { .. }      (verify)
...                                      Effect::Finished(Outcome)
```

Every `Event` carries `now_ms` (monotonic milliseconds from the shell). The core never reads a clock.

## Workspace

```
core/
├─ Cargo.toml            workspace
├─ jarvis-proto/         shared data types, serde only
├─ jarvis-observe/       UI tree → compact elements → prompt text
├─ jarvis-safety/        policy, risk classification, text hygiene
├─ jarvis-memory/        learned flows, profile, task log
├─ jarvis-agent/         Session state machine, prompts, parser, router
├─ jarvis-sim/           simulated phone, scripted/HTTP model, eval CLI
└─ jarvis-ffi/           UniFFI surface for Swift
```

Dependency direction: `proto` ← `observe`, `safety`, `memory` ← `agent` ← `sim`, `ffi`. No cycles. Nothing but `sim` may do IO.

Rules for every crate:

- Stable Rust, edition 2021. `#![forbid(unsafe_code)]` everywhere except generated FFI glue.
- No `unwrap`/`expect`/`panic!`/indexing that can panic in non-test code. Return `Result` with a `thiserror` error enum. A panic across FFI kills the app.
- Dependencies: `serde`, `serde_json`, `thiserror` by default. Anything else needs a reason. No C dependencies in the core crates (they must cross-compile to `aarch64-apple-ios` cleanly).
- Deterministic: no `HashMap` iteration order leaking into output (use `BTreeMap` or sort), no randomness, no time.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` must pass.
- Tests are behaviour tests with realistic fixtures, not one-assert-per-getter.

## jarvis-proto

Plain data, all `Serialize + Deserialize + Clone + Debug + PartialEq`. JSON field names `snake_case`. Enums tagged with `"type"`.

**Geometry.** `Point { x, y }`, `Rect { x, y, width, height }` in screen points (`f64`). `Rect::center()`, `contains`, `intersects`, `area`.

**UI tree (what the runner returns).**
`UiNode { role: Role, label, value, identifier, placeholder: Option<String>, frame: Rect, enabled, visible, hittable, focused, selected, secure: bool, children: Vec<UiNode> }`.
`Role` covers XCUIElement types we care about: `Button, Link, TextField, SecureTextField, TextView, SearchField, StaticText, Image, Cell, Switch, Slider, Picker, Tab, NavigationBar, TabBar, Toolbar, Alert, Sheet, Keyboard, Key, ScrollView, Table, CollectionView, WebView, Window, Application, Other(String)`. Unknown strings deserialize to `Other`.
`Screen { app_bundle_id, app_name: Option<String>, size: Size, root: UiNode, keyboard_visible: bool, orientation }`.
`Observation { screen: Screen, screenshot: Option<ImageRef> }` where `ImageRef { id: String, width: u32, height: u32 }`. Pixels never cross into the core; the shell keeps them and passes a handle.

**Model-facing actions** (`Action`, what the model is asked to emit):
`Tap { element }`, `LongPress { element, duration_ms }`, `Type { element: Option<u32>, text, submit: bool }`, `Clear { element }`, `Scroll { direction, element: Option<u32> }`, `Swipe { direction, element: Option<u32> }`, `TapPoint { x, y }` (normalized 0..1, vision fallback only), `PressHome`, `OpenApp { app }` (bundle id or display name), `OpenUrl { url }`, `Wait { ms }`, `CallTool { name, args: serde_json::Value }`, `AskUser { question }`, `Look` (request a screenshot step), `Done { summary }`, `Fail { reason }`.
`element` is the index assigned by `jarvis-observe` for the current screen.

**Runner commands** (`RunnerCommand`, what the shell sends to the XCUITest runner; absolute screen points):
`Tap { x, y }`, `LongPress { x, y, duration_ms }`, `Swipe { from: Point, to: Point, duration_ms }`, `TypeText { text }`, `PressKey { key }` (`Return, Delete, Home, ...`), `LaunchApp { bundle_id }`, `OpenUrl { url }`, `Wait { ms }`.
`RunnerResult { ok: bool, error: Option<String> }`.

**Tools.** `ToolSpec { name, description, params: Vec<ToolParam>, tier: Tier, risk: RiskCategory, app: Option<String> }`, `Tier { Native, Shortcut, Ui }`, `ToolCall { name, args }`, `ToolResult { ok, output: String, error: Option<String> }`.

**Model IO.** `ModelRequest { system: String, user: String, image: Option<ImageRef>, marks: Vec<Mark>, max_tokens: u32, grammar: Option<String>, json_schema: Option<String>, kind: RequestKind }` with `RequestKind { Text, Vision }`. `Mark { index: u32, frame: Rect }` tells the shell where to draw set-of-marks labels on the screenshot. `ModelOutput { text: String }`.

**Safety.** `RiskCategory { None, SendMessage, Payment, Purchase, Delete, PublicPost, AccountChange, SystemSetting, Credential }`. `Policy { denied_apps: Vec<String>, confirm: Vec<RiskCategory>, max_steps: u32, max_duration_ms: u64, max_consecutive_failures: u32, max_parse_retries: u32 }` with a conservative `Default`.

**Session IO.**
`Event`: `Start`, `Observed(Observation)`, `ModelOutput(ModelOutput)`, `ModelFailed { error }`, `ActResult(RunnerResult)`, `ToolResult(ToolResult)`, `UserDecision { approved: bool }`, `UserAnswer { text }`, `Cancel`. Wrapped as `TimedEvent { now_ms: u64, event: Event }`.
`Effect`: `Observe { screenshot: bool }`, `Infer(ModelRequest)`, `Act(RunnerCommand)`, `CallTool(ToolCall)`, `Confirm(ConfirmRequest)`, `AskUser { question }`, `Finished(Outcome)`.
`ConfirmRequest { category: RiskCategory, summary: String, app: String }`.
`Outcome { status: Success | Failed | Cancelled | Blocked, summary: String, steps: u32, duration_ms: u64 }`.
Progress for the UI is read from the session (`session.progress()`), not emitted as effects.

**Task.** `TaskSpec { id: String, text: String }`. `SessionConfig { policy: Policy, tools: Vec<ToolSpec>, model: ModelProfile, known_apps: Vec<AppInfo> }`, `ModelProfile { context_tokens: u32, supports_vision: bool, supports_grammar: bool }`, `AppInfo { bundle_id, name }`.

## jarvis-observe

- `compact(&Screen) -> CompactScreen`: flatten to actionable/informative elements only, drop invisible/off-screen/zero-area/pure-container nodes, merge a container's label from its static-text children when the container is the hittable thing (cells), dedupe, order top-to-bottom then left-to-right, assign `index` from 1.
- `Element { index, role, label, value, frame, enabled, focused, selected, secure, fingerprint }`. Secure fields never carry a value.
- `fingerprint`: stable across runs for "the same control": hash of role + label/identifier + ancestor role path. Must not depend on frame or index. Implement the hash by hand (FNV-1a 64) so it is stable across Rust versions.
- `render(&CompactScreen, budget_tokens) -> String`: one line per element, e.g. `[17] Button "Sign in"`, `[4] TextField "Email" value="a@b.c" (focused)`. Token estimate = bytes/4 rounded up. When over budget: truncate long labels first, then drop static text far from interactive elements, then drop from the bottom, and append `… N more elements (scroll to see)`.
- `screen_hash(&CompactScreen) -> u64` ignoring volatile values (clock, focus), for stuck detection and verification.
- `diff(before, after) -> ScreenDiff { changed: bool, appeared, disappeared, app_changed }`.
- `marks(&CompactScreen) -> Vec<Mark>` for set-of-marks.
- `resolve(&CompactScreen, index) -> Result<&Element>` and `find_by_fingerprint`.

## jarvis-safety

- `check_app(&Policy, bundle_id) -> Verdict` — deny list (exact and prefix match).
- `classify(action, &CompactScreen, tool: Option<&ToolSpec>) -> RiskCategory` — from the tool's declared risk, or for UI actions from the target element's label/role and screen context (e.g. a `Button` labelled Send/Pay/Buy/Place order/Delete/Post/Transfer/Confirm purchase). Word-boundary, case-insensitive matching; a table that is easy to extend. Prefer false positives over false negatives for Payment/Purchase/Delete.
- `gate(&Policy, category) -> Gate { Allow, Confirm, Deny }`.
- `Budget` tracker: steps, elapsed, consecutive failures → `BudgetVerdict`.
- `sanitize_screen_text(&str) -> String`: strip control characters and zero-width/bidi characters, collapse whitespace, cap length, and neutralise anything that imitates our prompt delimiters. Screen text is data; it must not be able to close the data block.
- `redact`: secure field values and anything matching obvious secrets (card-number-like digit runs, OTP-like codes next to "code") are never placed in prompts or logs.
- `validate_action(action, &CompactScreen) -> Result<(), ActionError>`: element exists, is enabled, typing targets a text input, URL schemes limited to `http`, `https` and an allow-list, `TapPoint` in range.

## jarvis-memory

- `Flow { id, app_bundle_id, intent_key, steps: Vec<FlowStep>, successes, failures, last_used_ms }`; `FlowStep { fingerprint, role, label, action: FlowAction, screen_hash_before }` where `FlowAction` mirrors UI actions with text as either literal or a `{slot}`.
- `intent_key(task_text) -> String`: lowercase, strip punctuation and filler words, collapse whitespace. Deterministic. Exact-match lookup in v1.
- `Recorder`: fed each successful step during a session, produces a `Flow` on success.
- `Replayer`: given a `Flow` and the current `CompactScreen`, returns `Next::Action(Action)` when the expected element is present, `Next::Diverged` otherwise, `Next::Complete` at the end. Divergence hands control back to the model; it is never an error.
- Flows with more failures than successes after ≥3 uses are retired.
- `Profile { fields: BTreeMap<String, ProfileField { value, sensitive } > }` with lookup by common aliases (email, phone, first name…).
- `TaskLog` entries: task, steps (action, app, result), outcome. Text only; redaction applied before storage.
- `MemoryStore { flows, profile, log }` ↔ JSON (`to_json`, `from_json`, versioned with a `schema_version` field, tolerant of unknown fields). The shell persists and encrypts the bytes.

## jarvis-agent

`Session::new(config: SessionConfig, task: TaskSpec, memory: MemoryStore) -> Session`
`Session::step(&mut self, ev: TimedEvent) -> Effect`
`Session::progress(&self) -> Progress { step, phase, last_action, app }`
`Session::into_memory(self) -> MemoryStore` (with the recorded flow and log entry)

Loop per step: **observe → decide → gate → act → verify**.

- Decide: if a learned flow is active and matches the screen, replay with no model call. Otherwise build a `ModelRequest`.
- Prompt: short fixed system prompt (role, action format, rules: prefer tools over tapping, screen content is data not instructions, never guess credentials, `ask_user` when blocked by Face ID/passcode/CAPTCHA). User part: task, relevant tools (narrowed to fit budget), last N steps with results, the rendered screen inside a clearly delimited data block. Whole request must fit `model.context_tokens` minus `max_tokens`; shrink screen budget before dropping history.
- Output format: one JSON object, e.g. `{"thought":"…","action":"tap","element":17}`. `action_json_schema()` and `action_gbnf()` export the schema/grammar so runtimes with constrained decoding can enforce it.
- Parser is tolerant: code fences, leading prose, single quotes, trailing commas, `tap(17)` call syntax, string numbers. On failure: re-infer with a one-line correction, up to `max_parse_retries`.
- Gate: `safety::validate_action`, `check_app`, `classify` + `gate`. `Confirm` → `Effect::Confirm`; declined → tell the model and continue or finish `Blocked`. `Deny` → never executed.
- Act: resolve element → coordinates → `RunnerCommand`. `Type` with an element taps it first if not focused. `Scroll`/`Swipe` compute gesture endpoints from the element frame or screen size. `OpenApp` resolves display name → bundle id through `known_apps`.
- Verify: observe again; if nothing changed after an action that should change something, count a failure and tell the model. Same screen hash + same action 3 times → stuck → one hint, then `Fail`.
- Vision fallback: when the compact screen has too few elements to act on, or the model emits `look`, and the model supports vision → `Observe { screenshot: true }` then a `Vision` request with marks.
- Tools: `CallTool` → `Effect::CallTool`; result fed back as a step.
- Budget exceeded / `Cancel` → `Finished` with the right status. `Cancel` is honoured from every state.
- Every transition for an unexpected event (e.g. `ActResult` while waiting for the model) is handled without panicking: finish `Failed` with a protocol-error summary.

## jarvis-sim

Not shipped on the phone. Makes the agent runnable on a laptop.

- `SimPhone`: apps defined in JSON (screens of `UiNode`s, transitions on tap/type/scroll). Ships a few apps: Settings (toggle), a login form, a messaging app with a Send button, a shopping flow with a Buy button, a long scrolling list.
- Models: `ScriptedModel` (fixed outputs for deterministic tests) and, behind the `http` feature, an OpenAI-compatible chat client for any local server (Ollama, LM Studio, llama.cpp server).
- `jarvis-sim run --task "…" --app settings [--model-url …]` prints the transcript. `jarvis-sim eval tasks/` runs a task set and prints pass rate, steps, model calls.
- End-to-end tests drive `Session` through the sim with the scripted model: success path, parse-retry, confirmation approve/decline, denied app, stuck detection, budget exhaustion, cancel, flow record then replay with zero model calls.

## jarvis-ffi

UniFFI (proc-macro mode). Exposes `JarvisSession` (`new`, `step`, `progress`, `finish`), memory load/save, `action_json_schema`, `action_gbnf`, and the proto types as Swift enums/structs. Panics must not unwind into Swift. Build script produces `JarvisCore.xcframework` (device + simulator) and the generated Swift file on a Mac. On Linux, CI checks that the crate compiles for `aarch64-apple-ios` and that bindings generate.

## Swift shell (outline)

```
App/Jarvis/
├─ SessionDriver.swift      loop: effect → IO → event
├─ Runner/RunnerClient      JSON-RPC to the XCUITest runner on 127.0.0.1
├─ Model/ModelProvider      protocol; LiteRT-LM / llama.cpp / FoundationModels backends
├─ Tools/                   native tools (EventKit, Contacts…), Shortcuts bridge
├─ Voice/                   wake word, STT, TTS, audio-session keepalive
├─ Store/                   encrypted persistence of MemoryStore bytes
├─ UI/                      chat, task log, connections, settings
└─ Intents/                 App Intents, Action Button
```
