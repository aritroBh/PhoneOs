# Jarvis build checklist

Updated as work lands. `[x]` = done and verified, `[~]` = written but not verified on a device or Mac, `[ ]` = not started.

## Planning

- [x] Research: iOS 27 platform limits, UI-control routes, on-phone models
- [x] Plan (`docs/PLAN.md`)
- [x] Architecture contract (`docs/ARCHITECTURE.md`)

## Rust core (`core/`) — verified on Linux

- [x] Workspace + `jarvis-proto` (shared types)
- [x] `jarvis-observe` (tree compaction, budgeted render, fingerprints, diff)
- [x] `jarvis-safety` (risk classification, gates, deny list, budgets, text hygiene)
- [x] `jarvis-memory` (learned flows, profile, task log, JSON snapshot)
- [x] `jarvis-agent` (session state machine, prompts, parser, schema/grammar, router)
- [x] `jarvis-sim` (simulated phone, scripted model, HTTP model, eval CLI)
- [x] `jarvis-ffi` (UniFFI surface, Swift bindings generate, iOS target check)
- [x] Structural policy: modes, per-app trust, deny categories, first-use approval
- [x] CI: fmt + clippy + test + sim eval + Swift smoke on Linux (toolchain pinned, `core/rust-toolchain.toml`)

## Swift shell (`Shell/`)

Verified on Linux (Swift 6, real Rust core linked):

- [x] `JarvisKit` package + `scripts/bootstrap.sh` (Rust lib + UniFFI bindings)
- [x] `SessionDriver` (effect → IO → event loop, cancellation, timeouts)
- [x] `LoopCtrlRunner` (XCUITest runner JSON-RPC client; wire format unverified until S1)
- [x] `OpenAICompatibleModel` (first `ModelProvider` backend)
- [x] `FileMemoryStore`, `ScreenshotStore`, `ToolRegistry`
- [x] First-use approval + policy defaults flow through `SessionConfigBuilder`
- [x] `TaskCenter` (one entry for Ask, Siri, workflows, remote), `HumanPromptBroker`, owner attention
- [x] CI: `shell.yml` builds and tests JarvisKit on Linux for every core or shell change
- [x] `scripts/host-test.sh`: on a failure reruns suites and tests alone, prints the Mac crash report

Verified on a macOS runner (Xcode 26.6, iPhone 17 Pro Max simulator, iOS 26.5):

- [x] xcframework build (device + simulator slices; Mac host tests link the host library)
- [x] XcodeGen project (`Shell/project.yml`)
- [x] App UI first wave: Ask, run view, activity, settings, owner sheets
- [x] App Intents (`AskJarvisIntent`; `RunJarvisWorkflowIntent` compiled in run 8, see below)
- [x] In-process XCUITest `PhoneRunner` (typed tree from `XCUIElementSnapshot`)
- [x] Mac host run of the package tests (found a Swift 6.3.3 abort Linux does not show: `Shell/README.md`, "On CI")

## iPhone Simulator QA (GitHub macOS runner)

- [x] `.github/workflows/ios.yml` + `Shell/scripts/sim-qa.sh` (per-test time limits, slowest-tests summary)
- [x] Tier A: playground flows (form, toggle, scroll, confirm gate, replay, secure field)
- [x] Tier B: real Settings, springboard, Safari form
- [x] Artifacts per step: compact screen, raw tree, screenshot, transcript
- [x] Green run on an iPhone 17 Pro Max simulator (run 3: 13/13; run 6: 20/21, the lock-mid-task test)
- [ ] Run 8 green: Workflows + Remote screens, lock-mid-task resume, unknown-icon question (27 tests)

## Skills, workflows, remote control

- [x] `jarvis-skills`: skill store, trigger matcher with slots, `SkillContext` in sessions, app confinement
- [x] `jarvis-skills`: workflows (triggers, steps, sans-IO run, schedule maths)
- [x] `jarvis-remote`: pairing, AEAD frames, hub + client state machines, adversarial tests
- [x] Independent adversarial review of both crates: 22 findings, 22 fixed with regression tests
- [x] FFI surface for skills, workflows, remote
- [x] Swift: skill and workflow stores, workflow runner and scheduler, remote control service (Linux-tested)
- [~] App: local-network listener (Bonjour), Keychain store, notifications, App Intent for workflows (first Mac compile in run 8)
- [~] App screens: Skills, skill detail, Workflows, Apps (ways in + trust), Control, Remote control (first Mac compile in run 8)
- [x] Design: canvas with nine screens (Astryx conventions, Lucide on web / SF Symbols on iOS)
- [ ] Human security review of the remote protocol

## Locked-phone operation

- [x] Research with sources (runner source, Apple docs): PLAN 3.2
- [x] Core: lock awareness (pause, "unlock to continue", re-observe before acting), `ObserveFailed`, typed-text verification
- [x] Swift driver: report lock state, handle `AwaitUnlock`, notification (Linux-tested; simulator run pending)
- [ ] Live Activity for runs and unlock prompts
- [~] Away mode in the app (logic Linux-tested; staying dark while another app is in front needs spike S7)
- [ ] Audio-session interruption recovery, before-first-unlock detection
- [ ] Siri AI `ToolIntent` + app skills bundle experiment

## Core follow-ups

- [x] Local-model eval on the simulated phone (`docs/MODEL-EVAL.md`)
- [x] Model-readiness fixes from that eval (state deltas, loop detection, switches `(on)/(off)`, scroll cue, icon taps checked like `open_app`)
- [x] Partial re-run after the fixes (Qwen3-4B-Instruct, its 9 failed tasks: 5 pass, 6 with one corrected expectation)
- [ ] Eval with a model of 2B or less and with constrained decoding
- [x] `jarvis-remote`: corrupt-store errors never quote the file (keys)
- [ ] First-use approval for `OpenUrl` and tools with no declared app
- [ ] Non-English safety phrases

## Phase 0 spikes — need the phone

- [ ] S1 Runner installs and drives 3 apps (LoopCtrl vs iCTRL)
- [ ] S2 Inference placement while another app is in front
- [ ] S3 Memory budget on a free account
- [ ] S4 Wake word + background keepalive for 8 h
- [ ] S5 Runner vs AutoFill / Face ID / alerts / display off
- [ ] S6 Build + install pipeline
- [ ] S7 Away mode: dark but not locked (brightness 0, proximity blanking, runner while blanked)
- [ ] S8 Locked phone: runner while locked, resume after unlock, lock-screen tree, keep_awake, reboot

## Later phases

- [ ] Voice: wake word, STT, TTS
- [ ] Native tools + Shortcuts bridge + Connections screen
- [ ] Away mode on the device (black screen, brightness, proximity, return-to-dark after each task): after S7
- [ ] Background operation, Live Activity, notifications
- [ ] Vision fallback on device, learned-flow replay on device
- [ ] Personal eval set (30 tasks)
- [ ] Hardening: injection tests, audit log, recovery
