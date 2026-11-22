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
- [x] All UI tests green on the Mac: run 82 (`e849cc0`), 28 of 28 UI tests, 278 package tests, no skips, no retries. Runs 76 and 77 had 25 of 28; the three causes (scripted model skipping a step a Safari tip sheet covered; the simulator not locking in the lock test; the workflow editor replacing the text field under the tap) were fixed in `3c5b5fa` and passed in runs 79 and 82
- [~] After run 82 the UI tests are 29 (one more for the approval picture). Three later runs each had one failure, each a different real cause, each fixed, none yet confirmed on the Mac: run 83, the lock test (the simulator took 7 s to lock and the unlock's single home press landed while it was still locking; the helper repeats the press, `df4a416`); the run on `df4a416`, the same first-use question asked twice (the screen fingerprint depended on the order of two hidden zero-size home-screen nodes that the simulator returned in a different order; digest v6); the run on `c0cff3e`, the lock test again (the rule for commands said locked while the rule for resuming said unlocked, so the driver spun through four refused taps in 3 s; one verdict for both, `3570e3b`). A Mac-only build error in the model client's warm-up (Swift 6.3 on the Mac, 6.4 on Linux) is fixed in `a75de3b`

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

- [x] Logic in Rust: crate `jarvis-viewer` (reducer over the run's effects and events, one step per runner command with the screen it acted on, the element under the pointer as a display aid only, caption and state; at most 200 steps; cursor path, replay, per-frame scene; versioned wire format). JarvisKit keeps a thin adapter (`LiveViewModel`: one FFI call per update, one per drawn frame) and the in-memory frame store
- [x] Backdrop: wireframe from the typed tree (shown elements, labels and placeholders, never values, at most 400); real screenshots only with "Show real screen in Live view" (off by default): the core's observation is unchanged (pixels only when the core asks); `DriverConfiguration.viewerScreenshots` adds a separate best-effort `PhoneRunner.viewerScreenshot()` with its own deadline (errors ignored, one note per run, no retry, no effect on failure counts or budgets), reuses pixels the core asked for, and checks the store is enabled on every observation; frames only for an observation the runner reported unlocked and still unlocked after the capture, never in an unlock wait, dropped when the core answers with an unlock wait (a runner that cannot report its lock state gets none); kept in `ViewerFrameStore` (memory only, capped by count and bytes, emptied per run, when the option is turned off, and when a run ends with no Live view on screen)
- [x] Privacy, decided in the core: every label, placeholder and caption is redacted in Rust before it is kept or encoded; typed text is shown only for a field from a typed tree that the core does not hide and whose text redaction leaves unchanged, never on text-format screens; links show the host for http(s), otherwise only the scheme. Property test: a corpus of secrets in random labels never reaches the encoded bytes
- [x] Forward-compatible decoding: unknown gestures, step states, activities and screen sources decode as `.unknown(<tag>)` and re-encode unchanged; a step stopped mid-command reads "Stopped"
- [x] App view: `TimelineView` paused unless the scene needs frames, the real screen hidden while the app is not active, screenshots aspect-fit, a live step arriving mid-animation continues from the cursor. Compiled and passing on the Mac after the switch to Rust (run 82, `testLiveViewFollowsAndReplaysDemoTask`)
- [~] App: `Views/LiveScreenView.swift` (viewer, cursor, ripple, long-press ring, swipe trail, target outline, caption, replay controls; Reduce Motion, VoiceOver, Dynamic Type), Screen / Elements switch on the run screen, the setting in Control. Same status as the line above
- [x] UI test `testLiveViewFollowsAndReplaysDemoTask` passed on the simulator (runs 76, 77); to be rerun after the switch to Rust
- [ ] Check on the Mac: `backward.frame.fill` / `forward.frame.fill` symbols render; Canvas text in small rectangles; the viewer's height on small phones; the workflow run view still shows the element list only
- [~] Phase 2: stream the timeline to the paired remote controller. Rust half done (`b0489e6`): bounded deltas (48,000 bytes each), resync by a snapshot of at most 256 KiB, per-device `watch_screen` permission off by default, backpressure and a screen rate limit in the hub, never pixels, never the approval picture; a 200-step run on sign-in screens streams as 206 KB. Not done: FFI and app wiring, the permission's toggle, a controller screen; pixels (their own permission) not designed yet
- [x] Rust crate jarvis-viewer (reducer, privacy, wire format, cursor path, replay, scene) with FFI; Swift switched over to it; compiled and green on the Mac (run 82)
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
- [x] Sixth and seventh reviews: the binding held under a 200,000-tree property test; the defects were around it (taps landing on a covering control, Return not re-checked, keyboard keys tapped unasked, questions hiding the text sent). Fixed: tap points refuse covered targets by a rule measured on real iOS 26 trees (1 refusal in 108 controls), digest v4 includes layout, questions show what is sent
- [x] Eighth review: 2 high, 4 medium, 1 low (a tap on a card landing on a control nested in it; form fields pushing the amount and payee out of the question; Return showing only its own field; a scrolled-out recipient; `tap_point` on Send hiding the text; icon-only send buttons). Fixed in `d34bacc`
- [x] Ninth review: 4 high (a menu drawn over a toolbar not seen as a cover; a card narrowed to an unlabelled icon button inside it; the tap-after-typing rule lost when the typed field scrolls away; contact fields pushing the payee out of the question), 1 medium, 1 low. Fixed by construction in `c7c85b2`: the question built from reserved slots, bar precedence only for page content scrolling under a bar, and property tests (random screens against an independent tap oracle, random forms against the question invariants, random type/scroll/tap sequences)
- [x] Tenth review: where a tap lands and whether the screen is the approved one held for the first time (bars, covers, nested controls, keyboard, digest, card narrowing). What the question says did not: 2 high (the tap-after-typing rule lost when the typed field's label changes; chat bubbles and quoted mail outranking the real recipient), 3 medium (decoy totals, look-alike slot markers, unbounded time on huge pages). Fixed: typed-here state per app, recipients from the form's own fields then proper headings, totals ranked by exact phrase and nearness, look-alikes folded, bounded work; property-test generators extended with each case; the extended tap generator found one more defect (two overlapping system bars), fixed
- [x] Eleventh review: tap landing and screen binding held a second time. 4 high: Return after typing submitted unasked (the tenth-pass rule covered taps only); one approved send cleared the typed-here state for the whole app; a chat bubble reading "To: Mom" shown as the recipient; a cut address hiding `@pay-` before `bigbank.com`. 5 medium (unfolded look-alike headings, decoy totals nearer the button, a payee beyond the 250-element cap, a homoglyph domain not flagged, quadratic time over thousands of inputs). Fixed in `ceee908` under a new rule: the question shows evidence with its provenance and never asserts what the gate cannot verify (`To/Account:` only for the form's own recipient fields, page text under `Screen says:`, the navigation bar's title in the action line, `Recipient not verified` otherwise); Return after typing asks; the typed-here state lasts for the task; a cut recipient keeps `@` and the host; a colon after any non-ASCII word is dropped instead of a look-alike table. Property tests extended per finding and mutation-checked
- [x] Digest v6: the order of adjacent siblings that are entirely hidden and have no area no longer voids an approval (the double question on the simulator). Everything shown, framed, or read by the gate (hidden text inputs, hidden labelled texts, keyboard nodes) keeps its order; audit of every consumer of node order in the module docs; 200,000-tree oracle test
- [ ] Twelfth review. Until a review passes without a high finding, treat the approval path as not yet proven
- [ ] Approval question as structured rows drawn by the app (page text cannot forge a heading by construction); after the eleventh-pass fixes
- [x] Approval sheet shows the screen from when Jarvis asked, with the target outlined (`9926ec1`, `f0d966d`; compiled on the Mac, its UI test passes). Its own adversarial review: no high finding, five medium, fixed in `af62a18` and `4699524`: the screenshot is taken while the target app is in front (before, almost never), shown only inside the sheet, dropped when the question resolves or the phone locks; reused pixels are tree-checked; a frame of another shape is not drawn; the outline is only drawn when the target is certain; labels say what is checked ("Screen when Jarvis asked", "Layout from the app's data"). The fixes' app code is not yet compiled on a Mac
- [x] Wireframes from the runner's text: on the device (third-party runner) the Live view and the approval picture were empty; they are now drawn from the parsed text with the same redaction (`af62a18`)
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

## Performance (Rust core)

- [x] Benchmark harness (`jarvis-sim bench`, `234e830`): 24 stages of a step on the ten real iOS 26 trees and on 1k–50k-node synthetic trees; prompt size per screen. Real screens: screen read to prompt 0.3 ms p50, under 1.2 ms p99 (shared VM); the fixed system prompt is 70–85% of every prompt
- [x] Budgets and scaling limits are in the source, hold, and run in CI (`core.yml` "Performance budgets": `bench --sizes 1000,5000,10000,20000 --check`, about 2.5 min plus a 30 s release build). The looser scaling limits whose causes were fixed are gone (change detection, carousel-neighbour search, budgeted render); the step and render budgets are tightened
- [x] Hotspot fixes, output unchanged (differential tests against the old code, eval transcripts byte-identical at four context sizes): the screen at budget 0 rendered once per prompt and only the returned rendering built; tap-point candidates after the first checked only against the drawn nodes that reach them, the cover named once; change detection by fingerprint map; the neighbour search stops at the first sibling; redaction reads words once and clones no element twice. Real screens, p50 median of the ten: screen read to prompt 0.33 → 0.14 ms, a whole round 0.39 → 0.20 ms. Dense 20k nodes: prompt 144 → 53 ms, look after an action 149 → 50 ms, sweep of every control 330 → 148 ms (fastest of three, same session)
- [ ] Still super-linear by construction: the sweep of every control (one call per control, each reading every drawn node once). Still linear but heavy on dense screens: redaction, compaction, the classifier's screen tally and the question's facts (each summary redacts the whole screen; two per approved look)
- [x] The fixed system prompt stays in a local model server's cache between steps (`cache_prompt`, automatic fallback for servers that reject it) and is warmed when a task starts (`94bf40f`, `a75de3b`). Not yet measured against a real local server; an in-process on-phone runtime must keep the same prefix evaluated

## Checks

- [x] `core.yml` and `shell.yml` steps run locally on a clean checkout of the pushed commit before and after every push (GitHub Actions also runs them again now that the repository is public)
- [x] Core changes are committed only from a clean snapshot that passed fmt, clippy, tests, the iOS check, the scripted eval and both Swift smoke runs
