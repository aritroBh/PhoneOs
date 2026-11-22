# Jarvis build checklist

Updated as work lands. `[x]` = done and verified, `[~]` = written but not verified on a device or Mac, `[ ]` = not started.

## Planning

- [x] Research: iOS 27 platform limits, UI-control routes, on-phone models
- [x] Plan (`docs/PLAN.md`)
- [x] Architecture contract (`docs/ARCHITECTURE.md`)

## Rust core (`core/`) — verified on Linux

- [x] Workspace + `jarvis-proto` (shared types)
- [ ] `jarvis-observe` (tree compaction, budgeted render, fingerprints, diff)
- [ ] `jarvis-safety` (risk classification, gates, deny list, budgets, text hygiene)
- [ ] `jarvis-memory` (learned flows, profile, task log, JSON snapshot)
- [ ] `jarvis-agent` (session state machine, prompts, parser, schema/grammar, router)
- [ ] `jarvis-sim` (simulated phone, scripted model, HTTP model, eval CLI)
- [ ] `jarvis-ffi` (UniFFI surface, Swift bindings generate, iOS target check)
- [x] CI: fmt + clippy + test on Linux

## Swift shell — needs the Mac to verify

- [ ] XcodeGen project + xcframework build script
- [ ] `SessionDriver` (effect → IO → event loop)
- [ ] `RunnerClient` (XCUITest runner RPC)
- [ ] `ModelProvider` + first backend
- [ ] App UI: chat, task log, connections, settings
- [ ] App Intents + Action Button
- [ ] Encrypted store for memory snapshot

## Phase 0 spikes — need the phone

- [ ] S1 Runner installs and drives 3 apps (LoopCtrl vs iCTRL)
- [ ] S2 Inference placement while another app is in front
- [ ] S3 Memory budget on a free account
- [ ] S4 Wake word + background keepalive for 8 h
- [ ] S5 Runner vs AutoFill / Face ID / alerts / display off
- [ ] S6 Build + install pipeline

## Later phases

- [ ] Voice: wake word, STT, TTS
- [ ] Native tools + Shortcuts bridge + Connections screen
- [ ] Background operation, Live Activity, notifications
- [ ] Vision fallback on device, learned-flow replay on device
- [ ] Personal eval set (30 tasks)
- [ ] Hardening: injection tests, audit log, recovery
