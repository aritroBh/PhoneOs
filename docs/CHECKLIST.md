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
- [x] App with Workflows + Remote screens builds; lock-mid-task resumes and finishes after the unlock (run 10: 25 of 27)
- [ ] All 27 green: the two run-10 failures were in the tests and are changed; rerun blocked (Actions jobs are not being started for this private repository, see the PR)

Waiting for a Mac build (app code changed after `18bc128`, the last commit compiled on a Mac; the JarvisKit side of each change is tested on Linux):

- [~] `AppModel.swift`: rules gate and banner, re-read on activation and after the first unlock, intent time limit, notification permission in the foreground, remote control following the saved setting
- [~] `Platform/RemoteAdapters.swift`: listener rebuild with backoff, peer address rule, byte budgets, ready deadline, connection cap
- [~] `Platform/WorkflowNotifications.swift`, `UnlockNotifier.swift`, `Views/ControlView.swift`, `Views/AskView.swift`, `Views/WorkflowsView.swift`
- [ ] Check first on the Mac: `NWConnection.currentPath` is set at `.ready` (else global IPv6 peers are refused); `NWPath.usesInterfaceType(.cellular)`; the `.contentProcessed` and `asyncAfter` closures under strict concurrency; `UNUserNotificationCenter.notificationSettings()`; `XCUIElement` answering `hasKeyboardFocus` in the UI tests

## Skills, workflows, remote control

- [x] `jarvis-skills`: skill store, trigger matcher with slots, `SkillContext` in sessions, app confinement
- [x] `jarvis-skills`: workflows (triggers, steps, sans-IO run, schedule maths)
- [x] `jarvis-remote`: pairing, AEAD frames, hub + client state machines, adversarial tests
- [x] Independent adversarial review of both crates: 22 findings, 22 fixed with regression tests
- [x] FFI surface for skills, workflows, remote
- [x] Swift: skill and workflow stores, workflow runner and scheduler, remote control service (Linux-tested)
- [~] App: local-network listener (Bonjour), Keychain store, notifications, App Intent for workflows (compiled; listener not exercised between two devices)
- [x] App screens: Skills, skill detail, Workflows, Apps (ways in + trust), Control, Remote control (compiled and driven by UI tests on the simulator)
- [x] Independent adversarial review of the Swift layer: 4 proven defects + 4 gaps, all fixed with regression tests
- [x] Design: canvas with nine screens (Astryx conventions, Lucide on web / SF Symbols on iOS)
- [ ] Human security review of the remote protocol

## Locked-phone operation

- [x] Research with sources (runner source, Apple docs): PLAN 3.2
- [x] Core: lock awareness (pause, "unlock to continue", re-observe before acting), `ObserveFailed`, typed-text verification
- [x] Swift driver: report lock state, handle `AwaitUnlock`, notification (Linux-tested; simulator run pending)
- [ ] Live Activity for runs and unlock prompts
- [~] Away mode in the app (logic Linux-tested; staying dark while another app is in front needs spike S7)
- [x] Before the first unlock: stores never overwrite an unreadable file; tasks are refused until the rules can be read
- [x] Lock state read before every command; nothing is sent to a phone that reads locked
- [ ] Audio-session interruption recovery (with voice)
- [ ] Siri AI `ToolIntent` + app skills bundle experiment

## Live view (cursor)

Watch Jarvis act: a phone-shaped viewer in the run screen shows the screen Jarvis acts on, with a "Jarvis" cursor that glides to each target and shows the tap, long press or swipe, and a caption ("Tap “Send”"). iOS does not let Jarvis (or any app) draw a cursor over other apps, so the viewer lives inside Jarvis; it is not an overlay on the app being driven.

- [x] JarvisKit `Viewer/` (Linux-tested): `PointerTimeline` reducer over the run's events (one step per runner command, the screen it acted on, the element under the pointer as a display aid only, caption, running/done/failed; capped at 200 steps), `CursorPath` (glide, press, drag; clamping, NaN, landscape), `ViewerMapping` (aspect fit, letterbox), `ReplayState` (step, play 1×/2×, scrub, Live without being yanked forward), `LiveViewScene` (what the view draws at a moment). All `Sendable`; the timeline is `Codable` with stable keys for the remote stream
- [x] Backdrop: wireframe from the typed tree (shown elements, labels and placeholders, never values, at most 400); real screenshots only with "Show real screen in Live view" (off by default): `DriverConfiguration.viewerScreenshots` asks the runner for pixels on every observation and keeps them in `ViewerFrameStore` (memory only, capped by count and bytes, emptied per run and when the option is turned off); the core still gets an `ImageRef` only when it asked; nothing is kept for a locked observation
- [x] Privacy: typed text comes from the model's (or flow's) action, never the runner command; `{{profile.…}}` values and anything typed into a secure field or a field that cannot be shown to be plain show as `••••` or not at all; URLs show the host only; runner error texts are not kept. Tests assert on the encoded JSON
- [~] App: `Views/LiveScreenView.swift` (viewer, cursor, ripple, long-press ring, swipe trail, target outline, caption, replay controls; Reduce Motion, VoiceOver, Dynamic Type), Screen / Elements switch on the run screen, the setting in Control. **Not compiled yet**: needs the Mac CI
- [~] UI test `testLiveViewFollowsAndReplaysDemoTask` (demo run: viewer present, caption changes, step back/forward/Live). **Unverified** until the Mac CI runs it
- [ ] Check on the Mac: `backward.frame.fill` / `forward.frame.fill` symbols render; Canvas text in small rectangles; the viewer's height on small phones; the workflow run view still shows the element list only
- [ ] Phase 2: stream the timeline and (when the option is on) frames to the paired remote controller over the existing encrypted channel, off by default, with its own permission
- [ ] Phase 3 (device spike): Picture-in-Picture floating mini-viewer over other apps (check it never covers tap targets and never appears in the tree the runner reads); Live Activity caption for the current step

## Core follow-ups

- [x] Local-model eval on the simulated phone (`docs/MODEL-EVAL.md`)
- [x] Model-readiness fixes from that eval (state deltas, loop detection, switches `(on)/(off)`, scroll cue, icon taps checked like `open_app`)
- [x] Partial re-run after the fixes (Qwen3-4B-Instruct, its 9 failed tasks: 5 pass, 6 with one corrected expectation)
- [ ] Eval with a model of 2B or less and with constrained decoding
- [x] `jarvis-remote`: corrupt-store errors never quote the file (keys)
- [x] Independent adversarial review of the safety gates (agent, safety, observe, memory): 6 proven defects (3 critical: unclassified scroll and element-less swipe over a pay slider, Return-key risk dead after compaction) + 3 more, all fixed with regression tests
- [x] After an approval the screen is read again; the action runs only if its target is still the same control
- [x] Second review, of that fix itself: 8 more proven defects (4 critical), fixed. An approval is tied to the target's state, its row, the amounts on screen and what the classifier read; summaries name the neighbouring item, the total, on/off
- [x] Spotlight, widgets and suggestions cannot open a denied app; classifier scans cannot be padded past a risky control
- [x] Third, fourth and fifth reviews of the approval path: each broke the version before it. The binding is now the raw digest (with what is shown), the compact content in order, the raw node the target comes from, and the category with the full summary; exclusions apply to digest and compaction alike
- [ ] Sixth review of that binding: in progress. Until a review passes without a critical finding, treat the approval path as not yet proven
- [x] Screens read through the third-party runner's text format always ask (the format can be forged by on-screen text)
- [ ] Typed runner for the device: Jarvis's own XCUITest runner serving typed trees to the app (today it exists only inside the UI test bundle); needed before hands-off use, because the text path now asks before every action
- [x] Denied apps are refused by display name (home-screen icon, switcher card, `open_app`)
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
