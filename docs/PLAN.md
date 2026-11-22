# Jarvis — on-device phone-use agent for iPhone

Plan date: 2026-11-22. Target: iPhone 17 Pro Max, iOS 27.x, personal sideload, free Apple ID, model runs on the phone.

Core logic is Rust (see `docs/ARCHITECTURE.md`); the iOS shell is Swift. Working checklist: `docs/CHECKLIST.md`.

Claims marked **[U]** are unverified (reported by READMEs/press, not tested on a device). Phase 0 exists to verify them before any real build.

---

## 1. What you asked for vs. what iOS allows

| Ask | Verdict | How |
|---|---|---|
| Tap/type/swipe in any app | Possible, personal sideload only | XCUITest runner on the phone exposing a local RPC server. Uses private APIs. Not App Store legal. |
| "Hey Jarvis" wake word | Possible, with caveats | In-app wake-word engine on a background audio session, plus Vocal Shortcuts / Action Button as fallbacks. No system-level third-party wake word exists. |
| Own app, like the Siri app | Yes | Normal SwiftUI app. |
| Connected to all apps | Partially | No API to call other apps' App Intents directly. Three tiers: native frameworks → Shortcuts bridge → UI control. |
| Keeps working when phone is "closed" | Limited | Runner keeps running with display off **[U]**, but cannot enter a passcode. Practical mode: Auto-Lock Never + minimum brightness. A truly locked phone cannot be driven. |
| Use my logins / passkeys | Limited | No app can read iCloud Keychain. Agent taps the AutoFill suggestion; you approve Face ID. Assume the agent cannot pass Face ID sheets **[U]**. |
| Local model on the phone only | Possible, lowest success rate of all options | ≤4B-parameter model. GPU inference is blocked while Jarvis is in the background, which is exactly when it is driving another app. See §4. |
| Free Apple ID | Possible, painful | 7-day re-signing, 3-app limit, likely no increased-memory entitlement **[U]**. $99/yr removes all three. |

Hard limits that no design removes:

- No jailbreak or TrollStore for A19 / iOS 26–27.
- Siri "Extensions" are closed to third parties. Side-button assistant replacement is Japan-only.
- No system MCP layer in iOS for third-party agents. MCP shipped only in Xcode 27. iOS 27's Siri AI Developer Preview does the reverse: an app's intents become tools Siri calls (`ToolIntent`, app skills bundles); Siri orchestrates, not Jarvis ([Apple](https://developer.apple.com/documentation/appintents/siri-ai-developer-preview)). Worth testing as an extra voice entry (3.3).
- Best published result on native iOS tasks (iOSWorld benchmark) is 52% overall, 37% multi-app, and that is not with a 4B phone model. Expect simple single-app tasks to work and long multi-app tasks to fail often at first.

---

## 2. Architecture

```
┌──────────────────────── iPhone ────────────────────────┐
│                                                        │
│  Jarvis.app (SwiftUI)                                  │
│  ├─ Voice: wake word → STT → TTS                       │
│  ├─ Agent loop: observe → decide → act → verify        │
│  ├─ Model runtime (on-device LLM/VLM)                  │
│  ├─ Tool router: native │ Shortcuts │ UI control       │
│  ├─ Memory: profile, task log, learned flows           │
│  ├─ Safety: confirm gates, allow/deny list, kill switch│
│  └─ App Intents: "Ask Jarvis", Action Button, Shortcuts│
│            │ JSON-RPC, 127.0.0.1                       │
│  JarvisRunner (XCUITest bundle)                        │
│  └─ tap / type / swipe / screenshot / a11y tree /      │
│     launch app, on whatever app is in front            │
│            │                                           │
│  Loopback VPN app + Developer Disk Image mount         │
│  (lets the phone start its own test session)           │
└────────────────────────────────────────────────────────┘
```

Two processes. Jarvis.app thinks, JarvisRunner acts. They talk over localhost.

### 2.1 Actuator: XCUITest runner

Do not write this from scratch. Three open projects already do it:

| Project | License | Notes |
|---|---|---|
| [LoopCtrl](https://github.com/Souitou-iop/LoopCtrl) | Apache-2.0 | Mounts the Developer Disk Image itself. On iOS 27+ pairing happens on the phone, so no computer at runtime **[U]**. One tap to remount after reboot. Runs with screen off **[U]**. |
| [iCTRL](https://github.com/OpenMinis/iCTRL) | Apache-2.0 (v1.32+) | Encrypted JSON-RPC. Start by tapping the runner icon. Sees SpringBoard and system alerts. Setup uses a computer once. |
| [rounak/PhoneAgent](https://github.com/rounak/PhoneAgent) | MIT | Reference for app + runner + wake word layout. Uses OpenAI cloud. README only documents launch from Xcode. |

Skip [Louis-iPhone-Agent](https://github.com/LouisHe511/Louis-iPhone-Agent) as a code base (GPL-3.0); read it for ideas only.

Decision: fork whichever of LoopCtrl / iCTRL passes spike S1, keep it as a separate target, wrap it behind one Swift protocol (`PhoneActuator`) so it can be swapped.

Known runner limits to design around:

- XCTest asks for the device passcode when a session starts.
- Stale UI snapshots during animations. Wait for idle, then re-snapshot.
- Keyboard input is unreliable. Verify the field value after typing.
- "Automation Running" overlay is visible.
- Breaks are likely on iOS updates. Turn off automatic iOS updates.

Fallback actuator if the runner route dies: [esp32-iphone-agent](https://github.com/onehumanbeing/esp32-iphone-agent). A $7 ESP32-S3 acts as AirPlay receiver (screenshots) and BLE mouse/keyboard (AssistiveTouch). No signing, works on any app, but 480×1040 at ≤5 fps, vision-only, stops when the phone locks.

### 2.2 Observation: tree first, pixels second

The runner returns the accessibility tree as text. Most steps need only that.

1. Compact the tree: visible, hittable elements only, each with a short numeric id, label, type, value.
2. Text-only model picks `tap(17)` / `type(4, "…")`.
3. Vision fallback only when the tree is empty or useless (games, canvases, unlabeled web views): screenshot with numbered overlays (set-of-marks), VLM picks a number.

This is the main cost lever. Text steps are small and fast on CPU. Vision steps are slow and memory-heavy.

### 2.3 Tool router: cheapest path first

| Tier | What | Examples | Reliability |
|---|---|---|---|
| 1. Native | Apple frameworks inside Jarvis | EventKit (calendar, reminders), Contacts, HomeKit, MusicKit, Photos, HealthKit, location, notifications, URL schemes | High |
| 2. Shortcuts bridge | `shortcuts://run-shortcut?name=…&input=…` with x-callback | Anything any app exposes to Shortcuts. Jarvis ships a starter library and reads the list of shortcuts you name for it. | Medium |
| 3. UI control | Runner | Everything else | Lowest |

The agent is told to try tier 1, then 2, then 3. "Connections to all my apps" in the Jarvis app is a screen that shows, per app: which tier is available, allow/deny, and whether confirmation is required.

Optional later: MCP client via the official [swift-sdk](https://github.com/modelcontextprotocol/swift-sdk) for remote services (mail, drive, etc.) that are easier to reach by API than by tapping.

### 2.4 Voice

| Piece | Choice | Reason |
|---|---|---|
| Wake word, primary | [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx) keyword spotting (Apache-2.0, iOS supported) running on a background audio session | The same audio session also keeps Jarvis alive in the background, which the agent needs anyway. It ends on a phone call or Siri, and re-activating from the background is reported to fail until the app is in front again ([forum](https://developer.apple.com/forums/thread/813278)); iOS 27 adds `AVAudioSession` lifecycle notifications for this. Jarvis must notify "tap to resume listening" when it happens. Costs battery and shows the orange mic dot permanently. |
| Wake word, fallback | Accessibility → Vocal Shortcuts → runs a Shortcut → Jarvis App Intent | Zero battery cost. Less reliable than "Hey Siri". Cannot open apps from a locked phone. |
| Hardware trigger | Action Button → Jarvis App Intent | Always works. |
| Speech-to-text | Apple `SpeechAnalyzer` (iOS 26+) | Free, on-device, no model download. [WhisperKit](https://github.com/argmaxinc/WhisperKit) if accuracy is poor. |
| Text-to-speech | `AVSpeechSynthesizer` first | Zero setup. Upgrade to Kokoro via [FluidAudio](https://github.com/FluidInference/FluidAudio) later **[U]**. |

openWakeWord has a "hey jarvis" model, but its pretrained models are CC BY-NC-SA and it has no iOS build. Use it only as a source for training data/approach if sherpa-onnx needs a custom keyword.

### 2.5 Credentials and forms

- Passwords and passkeys stay in Apple Passwords. Jarvis never stores them.
- On a login field the agent taps the AutoFill suggestion above the keyboard. Face ID approval is yours. Jarvis pauses and notifies "needs Face ID".
- Profile data for forms (name, address, phone, DOB, etc.) lives in Jarvis's own store, encrypted with Keychain-held keys, filled as plain typing.
- Payment details: not stored in v1. Use Apple Pay / AutoFill cards with your approval.

### 2.6 Safety

An agent with full control of the phone that reads untrusted screens is a real risk. Built in from Phase 1, not bolted on:

- Screen content is data, never instructions. Text on a web page or in a message cannot change the task.
- Confirmation required before: sending messages/email, payments, purchases, deleting, posting publicly, changing passwords or settings.
- Per-app deny list. Banking and Passwords apps denied by default.
- Kill switch: Action Button long-press and a Live Activity stop button.
- Full action log with screenshots, viewable in the app.
- Step cap and time cap per task.

---

### 2.7 Skills, workflows, remote control

Added after the first plan. All three sit on top of the same session loop and can only narrow what the policy allows, never widen it.

| Feature | What it is | Where it lives | Limits |
|---|---|---|---|
| **Skill** | A named capability: trigger phrases (with `{slots}`), owner-written guidance, the apps it works in, optional learned flows that replay without the model. Kinds: learned, written, built in. | `core/jarvis-skills` (store, matcher); `SkillContext` on `SessionConfig` feeds the prompt and can confine a session to the skill's apps. | A skill never grants trust or skips a confirmation. |
| **Workflow** | A trigger plus ordered steps: run skill, run task, ask owner, notify, wait, stop-unless. Triggers: manual, phrase, schedule, external key. | `core/jarvis-skills` (sans-IO `WorkflowRun`); the shell owns timers and maps iOS automations to external keys. | iOS gives no general background triggers: schedule/arrival/charger triggers arrive through Shortcuts automations calling an App Intent **[U]**. Each step is an ordinary session under the owner's policy. |
| **Remote control** | Start, watch and stop tasks from another of the owner's devices on the same network. | `core/jarvis-remote` (pairing, AEAD frames, hub state machine, client half); the shell carries bytes over a local listener. | Approvals stay on the phone unless switched on per device, and never for payment, purchase, credential, account change or delete. Needs the app alive on the phone (section 3). Protocol is unaudited. |
| **App access** | Three ways into an app, cheapest first (2.3): direct system frameworks, app actions (App Intents / Shortcuts), the screen through the runner. | Tool router + per-app trust level (Trusted / Guarded / Step by step / Denied). | Screen access to "any app" depends on the runner spike (S1, S5). |

App structure: tabs **Ask · Skills · Workflows · Apps · Control**; Activity and Remote control open from Ask and Control. Visual language follows Astryx (Meta's open design system): neutral graphite surfaces, 1px bezel, colour only for state, SF Symbols on iOS per its native icon mapping.

## 3. "Works when I close my phone"

A phone locked with a passcode cannot be driven: the runner cannot enter the passcode or act behind the lock screen. So "closed" is built as **Away mode**: the phone stays unlocked and awake but looks off.

| State | Works? |
|---|---|
| Jarvis backgrounded, another app in front, screen on | Yes. This is the normal operating state. |
| **Away mode**: dark screen, not locked (below) | Yes, expected. This is the answer to "closed". Verified by spike S7. |
| Side button pressed, display off, no passcode needed yet | Runner stays alive **[U]**; read-only work continues **[U]**. |
| Locked with passcode | No. |

### 3.1 Away mode

| Piece | Mechanism | Status |
|---|---|---|
| Never auto-locks | Settings → Auto-Lock → Never (owner sets once). While Jarvis is in front: `isIdleTimerDisabled`. | Public API / setting |
| Looks off, idle | Jarvis shows a pure black screen (OLED pixels off) at brightness 0 (`UIScreen.brightness`; the change persists until the device locks). | Public API |
| Really off, idle | Proximity monitoring (`UIDevice.isProximityMonitoringEnabled`): phone face down or covered → display off and touches ignored, device not locked. Applies only while Jarvis is in front. | Public API; behaviour with the runner **[U]** |
| Dark while working | Brightness stays 0 while other apps are in front. Darker still: Accessibility → Zoom → Low Light filter or Reduce White Point on the triple-click shortcut. | Setting; whether the runner can drive with the display off by proximity **[U]** |
| Returns to dark | After each task the runner brings Jarvis back to the front. | Runner `launch_app` |
| Still listening | Wake word on the audio session (2.4), foreground in Away mode. | S4 |
| Tells you | Live Activity / Dynamic Island, notifications for decisions and results. | Phase 4 |

Costs, stated plainly:

- **The phone is unlocked the whole time.** Anyone who picks it up has everything. Away mode is for home or a desk, not a pocket, bag or public place. The black screen can ask for Face ID before showing Jarvis, but a swipe home still reaches an unlocked phone; iOS offers no way to block that outside Guided Access, which would also block the runner.
- No pocket use: with the display on (dim) and unlocked, touches register. Face down with proximity blanking avoids this only while Jarvis is in front.
- Battery and heat: keep it on a charger; on-device inference plus charging runs warm.
- Auto-Lock Never applies to all use of the phone until switched back; Away mode should remind the owner when it ends.

### 3.2 Truly locked (side button pressed): what the evidence says

Researched from runner source (WebDriverAgent, LoopCtrl, iCTRL) and Apple documentation, 2026-11-22. Nothing here has been run on your phone yet.

| Question | Finding | Evidence |
|---|---|---|
| Can a runner enter the passcode? | No known way. Every runner's `unlock` only presses buttons; LoopCtrl's own error text says the passcode "cannot be entered automatically". Apple: "no officially supported way to automate the entry of this passcode". One forum snippet claims `springboard.secureTextFields["Passcode field"]` works; the only dated test of it failed (iOS 17.4 simulator). No real-device report for iOS 17 to 27. | WDA `XCUIDevice+FBHelpers.m`; LoopCtrl `ActionHandler.swift`; [Apple forum](https://developer.apple.com/forums/thread/693273); [Appium docs](https://github.com/appium/appium-xcuitest-driver/blob/master/docs/reference/execute-methods.md) |
| What does a locked phone give the runner? | "Black screenshots, dead taps" (iCTRL). Launching an app is refused: `Locked("Unable to launch … because the device was not, or could not be, unlocked")`. | iCTRL `SKILL.md`; [Apple forum](https://developer.apple.com/forums/thread/769872) |
| Does the runner survive the lock? | Both runners claim the server keeps running with the screen off. Unverified for a full passcode lock. | LoopCtrl and iCTRL READMEs |
| Can it tell it is locked? | Yes: the `com.apple.springboard.lockstate` notification; LoopCtrl reports `screen_locked`. | WDA source; LoopCtrl source |
| Can Jarvis open any app while locked? | No. A foreground launch always asks for unlock; `authenticationPolicy` does not change that. | [Apple DTS](https://developer.apple.com/forums/thread/779977) |
| What still runs while locked? | Audio session with microphone (wake word), CPU inference, App Intents in `.background` mode with `.alwaysAllowed`, notification actions not marked `authenticationRequired`, data in "after first unlock" classes. | [Apple](https://developer.apple.com/documentation/appintents/intentauthenticationpolicy), [Apple](https://developer.apple.com/documentation/security/restricting-keychain-item-accessibility) |
| After a reboot? | Nothing protected is readable until the first unlock. iOS reboots itself after about 72 h locked (reported). The runner needs its disk image mounted again, and iOS asks for the passcode when the runner starts. | [Apple DTS](https://developer.apple.com/forums/thread/760711); [TechCrunch](https://techcrunch.com/2024/11/14/new-apple-security-feature-reboots-iphones-after-3-days-researchers-confirm); LoopCtrl README |
| iOS 27 specifics | `pressLockButton` no longer locks (WDA switched to a HID power press); synthesized typing drops characters on real devices under Xcode 27 (FB24932647); LoopCtrl has a `keep_awake` command (a key press every 10 s), which suggests taps alone are not trusted to hold off auto-lock. | [WDA PR 1171](https://github.com/appium/WebDriverAgent/pull/1171); [Apple forum](https://developer.apple.com/forums/thread/849249); LoopCtrl source |

### 3.3 Design that follows

1. **Stay unlocked: Away mode (3.1) is the main route.** It is also what both runner authors recommend. Use the runner's `keep_awake` during tasks and Auto-Lock Never between them.
2. **If the phone locks anyway, pause and resume, never fail silently.** The core treats a lock as an interruption: no actions, no budget burned, "Unlock to continue" as a notification and Live Activity. After your Face ID glance it re-observes and carries on; nothing decided before the lock runs without a fresh look, and a confirmation given before the lock is asked again. (Being built in the core now.)
3. **Do the screen-free part while locked.** Listening, thinking on CPU, notifications, background App Intents and whatever system frameworks allow. Tasks that need another app's screen wait for step 2.
4. **Jarvis never holds your passcode.** The evidence says typing it through the runner is unlikely to work, and storing it would weaken the phone. It stays an experiment (spike S8), not a dependency. A paired Bluetooth keyboard that you type on is the only reported way to unlock without touching the screen.
5. **Survive interruptions.** Detect a dead audio session, a reboot (before-first-unlock) and an expired runner; tell you with one notification what to tap. Keep working data in "after first unlock" storage.
6. **Verify typing.** After typing, read the field back and retry once; needed anyway because of the iOS 27 dropped-character bug. (Being built in the core now.)
7. **Try Siri AI as a second voice entry.** Expose Jarvis actions as `ToolIntent`s plus an app skills bundle and test with the development build: whether a free account qualifies and what it does while locked are unknown.

**Before the first unlock after a restart, Jarvis cannot read its rules, so it refuses tasks.** Its files (your rules, what it has learned, the history, the schedule record, the remote-control audit log) are protected until the phone has been unlocked once. Until then every task, workflow and remote request is refused with "Jarvis cannot read your rules yet. Unlock the phone and try again.", and no file it could not read is ever overwritten; after the first unlock everything works again without restarting the app. A damaged rules file is set aside (never deleted), and tasks wait until you confirm you have reviewed your rules, which are back to the defaults.

`BGContinuedProcessingTask` is not the mechanism: it must be user-started, must report progress, the user can cancel it, and background GPU is unavailable on iPhone. The background audio session is the keepalive.

---

## 4. Model: the hardest part of "on-phone only"

### 4.1 The conflict

While the agent drives another app, Jarvis is in the background. On iPhone:

- Background GPU (Metal) is not available on iPhone: Apple DTS says background GPU is iPad-only on iOS 26 ([forum](https://developer.apple.com/forums/thread/802789)); no iOS 27 change found. Metal work submitted after backgrounding fails with `MTLCommandBufferError.notPermitted` ([Apple](https://developer.apple.com/documentation/metal/preparing-your-metal-app-to-run-in-the-background)); crashes are reported, not documented. Check `BGTaskScheduler.supportedResources` on the device.
- Neural Engine in the background needs `com.apple.developer.background-tasks.continued-processing.inference` on iOS 27, for Core ML, Core AI and MPS Graph alike ([release notes](https://developer.apple.com/documentation/ios-ipados-release-notes/ios-ipados-27-release-notes)). Whether a free account can have it: **[U]**. So background text steps are CPU-only until measured.
- So background inference is CPU-only, 2–5× slower than GPU.

Per-app memory: about 6 GB measured on a 17 Pro Max *with* the increased-memory-limit entitlement. Without it (free account), lower and unmeasured **[U]**. Ceiling is a 4-bit ~4B model. 8B is out.

### 4.2 Ways out, to be measured in Phase 0

| Option | How | Trade-off |
|---|---|---|
| A. CPU in background | LiteRT-LM or llama.cpp on CPU inside Jarvis | Simplest. Gemma 4 E2B measured at 25 tok/s decode, 532 tok/s prefill on CPU (iPhone 17 Pro, text). Fine for tree-first text steps. Vision steps slow. |
| B. Apple system model | Foundation Models framework. Tool calling since iOS 26; iOS 27 adds image input, a `LanguageModel` protocol and a larger context (read `contextSize` at runtime). Background calls are rate limited, mainly on battery. Runs out of process, so it does not count against Jarvis's memory. | Free and already installed. Small context. Background availability **[U]**. Grounding quality on UI **[U]**. |
| C. Foreground flip | Runner brings Jarvis to front for the GPU step, then returns to the target app | Full GPU speed. Adds app-switch time per step and can reset state in some apps. Use for vision steps only. |
| D. Inference inside the runner process | Load the model in JarvisRunner | Only if Metal works there **[U]**. Probably same restriction. |

Expected outcome: A for text steps, C for the occasional vision step, B as planner if it runs in background. Data decides.

### 4.3 Candidate models (fit a ≤4B budget)

| Model | Size | Published | Runtime | License |
|---|---|---|---|---|
| Gemma 4 E2B / E4B | 2B / 4B eff. | 56 tok/s GPU, 25 tok/s CPU (E2B, text). E4B multimodal 15.7 tok/s on Core ML. UI grounding **[U]**. | [LiteRT-LM](https://developers.google.com/edge/litert-lm/models/gemma-4), Core ML, MLX | Gemma terms |
| Qwen3-VL-4B | 4B | ScreenSpot-v2 94.0, ScreenSpot-Pro 59.5 | MLX, llama.cpp (GGUF availability **[U]**) | check card |
| [MAI-UI-2B](https://huggingface.co/Tongyi-MAI/MAI-UI-2B) | 2B | ScreenSpot-Pro 57.4. No per-size AndroidWorld number. | No MLX/GGUF build listed **[U]** | Apache-2.0 |
| Step-GUI-4B | 4B | ScreenSpot-v2 93.6, ScreenSpot-Pro 60.0 | **[U]** | **[U]** |
| Apple system model | — | Not benchmarked for UI | Foundation Models | built in |

Benchmarks are vendor-reported on Android/desktop. No per-model iOS numbers exist. The personal eval set in Phase 5 is the real measure.

MLX is Metal-only, so it is foreground-only. Anything that must run in the background goes through LiteRT-LM or llama.cpp on CPU.

### 4.4 Provider abstraction

One protocol, `ModelProvider`, with on-phone backends only by default. Evaluate [AnyLanguageModel](https://github.com/huggingface/AnyLanguageModel) (Apache-2.0; one API over Foundation Models, MLX, llama.cpp, Core ML) in Phase 0 instead of writing this.

On-phone is the default and the only thing wired up. The protocol leaves room for a home-machine or cloud backend if on-phone success rate disappoints. For scale: a small hosted model costs roughly $0.0004 per step at current list prices, about $0.04 per 100 steps.

### 4.5 Making a small model good enough

- Tree-first (§2.2): small models choose from a short list far better than they locate pixels.
- Learned flows: every successful task is saved as an action trace keyed by app + intent. Next time, replay it with verification at each step and call the model only when the screen differs. Repeat tasks become near-instant and near-free.
- Narrow tool set per step. The router picks the app first, then the model sees only that app's actions.
- Verify after every action. Retry or back out on mismatch.

---

## 5. Build and install with a free Apple ID

| Constraint | Consequence | Mitigation |
|---|---|---|
| 7-day certificates | Apps stop launching weekly | [SideStore](https://github.com/SideStore/SideStore) re-signs on the phone automatically |
| 3 sideloaded apps | SideStore + Jarvis + JarvisRunner uses all three | Prefer LoopCtrl-style in-app DDI mount so no StikDebug slot is needed. Loopback VPN app must come from the App Store **[U]**. |
| No increased-memory entitlement **[U]** | Smaller model budget | Measure in S3. May force 2B class. |
| Xcode needs a Mac | Cannot build on the phone | With a Mac: build and run from Xcode. Without: GitHub Actions macOS runner builds an unsigned `.ipa`, SideStore signs and installs it **[U for the runner bundle]**. |
| One-time computer step | SideStore install / pairing | Any Windows, Linux or Mac machine, once |
| Developer Mode | Required | Settings → Privacy & Security |

A paid account ($99/yr) removes the 7-day cycle, the 3-app limit, and the entitlement gap. It is the single cheapest reliability upgrade in this plan. Not required to start.

---

## 6. Repository layout

```
PhoneOs/
├─ core/                       Rust workspace. All decision logic. Builds and tests on Linux.
│   ├─ jarvis-proto/           shared data types
│   ├─ jarvis-observe/         a11y tree compaction, prompt rendering, fingerprints
│   ├─ jarvis-safety/          confirm gates, deny list, caps, untrusted-text hygiene
│   ├─ jarvis-memory/          learned flows, profile, task log
│   ├─ jarvis-agent/           sans-IO session state machine, prompts, parser, router
│   ├─ jarvis-sim/             simulated phone + eval CLI (not shipped)
│   └─ jarvis-ffi/             UniFFI bindings → JarvisCore.xcframework
├─ project.yml                 XcodeGen spec; no .xcodeproj in git
├─ App/Jarvis/                 SwiftUI shell: all IO
│   ├─ SessionDriver           executes core effects, feeds back events
│   ├─ Runner/                 client for the XCUITest runner
│   ├─ Model/                  LiteRT-LM / llama.cpp / FoundationModels backends
│   ├─ Tools/                  EventKit, Contacts, HomeKit, MusicKit, Shortcuts bridge
│   ├─ Voice/                  wake word, STT, TTS, audio session keepalive
│   ├─ Intents/                App Intents, Action Button
│   └─ UI/                     chat, tasks, connections, settings, Live Activity
├─ Runner/JarvisRunner/        forked XCUITest runner + RPC server
├─ .github/workflows/          Linux: Rust fmt/clippy/test. macOS: manual build.
└─ docs/                       plan, architecture, checklist, spike guides
```

Why Rust for the core: the agent loop is a state machine with many edge cases (bad model output, stale screens, declined confirmations, budgets). Rust makes those states explicit and exhaustively matched, has no data races, and the whole core is deterministic and testable on Linux against recorded screens. The core does no IO, so it is driven identically by the Swift app and by the simulator.

## 7. Phases

### Phase 0 — Spikes on the real phone (gates everything)

Each spike ends in a short result note in `docs/spikes/`. No product code until S1 and S2 pass.

| # | Question | Pass condition |
|---|---|---|
| S1 | Does a LoopCtrl / iCTRL runner install and run on iPhone 17 Pro Max, iOS 27.x, free Apple ID? | Tap, type, swipe, screenshot, tree in Safari, Settings, and one third-party app. Survives app switch. Documented restart steps after reboot. |
| S2 | Where can inference run while another app is in front? | Measured tok/s and time-per-step for options A–D in §4.2 with Gemma 4 E2B and Qwen3-VL-4B. One option under ~10 s per text step. |
| S3 | Memory budget without the paid entitlement | Largest model that loads and completes 50 steps without a jetsam kill. |
| S4 | Wake word + keepalive | "Hey Jarvis" detected from background for 8 h. Battery cost measured. Jarvis process still alive. |
| S5 | Runner vs. system UI | What happens on AutoFill bar, Face ID sheet, permission alerts, Control Center, display off. |
| S6 | Build pipeline | Unsigned `.ipa` from CI installs through SideStore, runner included. Or confirm a Mac is needed. |

If S1 fails on both runners: fall back to the ESP32 actuator or stop and reassess.

### Phase 1 — Core loop

Text command in the Jarvis app → agent loop → runner → verified actions, tree-only. Action log, stop button, step cap, confirmation gates, deny list.
Done when: "open Settings and turn on Dark Mode" and "search Safari for X and open the first result" succeed 9/10.

### Phase 2 — Voice

Wake word, STT, TTS, Action Button intent, Vocal Shortcut setup guide, spoken confirmations.
Done when: hands-free task from the Home Screen works end to end.

### Phase 3 — Connections

Native tools, Shortcuts bridge with starter library, deep links, router with tier preference, Connections screen, profile store, form filling with AutoFill handoff.
Done when: "add dinner with Sam Friday 7pm", "play my liked songs", and "fill out this form" each take the cheapest tier available.

### Phase 4 — Background and long tasks

Audio-session keepalive, Live Activity with progress and stop, notifications for human-in-the-loop, pocket mode, task queue, resume after interruption.
Done when: a 5-minute multi-step task completes with Jarvis never in the foreground.

### Phase 5 — Reliability and speed

Vision fallback with set-of-marks, learned-flow record/replay, personal eval set (30 tasks you actually want), regression runs per model, model swap based on scores.
Done when: eval pass rate is tracked per commit and learned flows cut repeat-task time by more than half.

### Phase 6 — Hardening

Prompt-injection test set, audit log export, per-app policies, recovery from runner death, iOS update checklist.

---

## 8. How the build is staffed

| Work | Model | Why |
|---|---|---|
| Planning, architecture, review of every stage | orchestrator (this session) | |
| `jarvis-proto`, `jarvis-safety`, `jarvis-agent`, `jarvis-ffi`, runner bridge, model runtime backends, background/keepalive | Opus | Hard to get right, hard to debug on device |
| `jarvis-observe`, `jarvis-memory`, `jarvis-sim`, SwiftUI screens, native tools, App Intents, voice plumbing, CI | Sonnet | Well-specified or well-trodden |
| File moves, renames, config edits, doc lookups, repo fact checks | Haiku | Mechanical |

Build order: proto → (observe, safety, memory in parallel) → agent → (sim, ffi, CI in parallel) → Swift shell. Rust is compiled and tested in this workspace on every change. Swift cannot be compiled here (no Xcode); it is verified on the Mac or by the manual macOS workflow.

## 9. Risks

| Risk | Likelihood | Impact | Response |
|---|---|---|---|
| Runner route breaks on an iOS update | High over a year | Total | Pin iOS version, disable auto-update, actuator behind a protocol, ESP32 fallback |
| 4B on-phone model too weak for multi-app tasks | High | Tasks fail | Tree-first, learned flows, narrow tool sets; provider protocol allows an upgrade path |
| No workable background inference path | Medium | Slow steps | Foreground flip (option C) always works |
| Free-account limits (3 apps, 7 days, memory) | High | Friction | SideStore auto-refresh; paid account |
| Face ID / passcode walls | Certain | Human needed | Notify-and-wait design |
| Heat and battery during long runs | Medium | Throttling | CPU thread cap, step pacing, charger for long tasks |
| Agent takes a harmful action | Low per task | High | Confirmation gates, deny list, kill switch, logs |
| Private-API runner | — | Never App Store | Accepted: personal use only |

---

## 10. Open items needing you

1. **Mac access.** Confirmed: Mac with Xcode available. Build and install from Xcode.
2. **Phase 0 needs your phone.** The spikes cannot run from here. They will come as step-by-step instructions plus a small test app.
3. **Free vs. $99 account.** Plan works on free. Revisit after S3 if the memory budget forces a 2B model.

---

## Sources

- iOS 27 / Siri / App Intents: [Apple newsroom, June 2026](https://www.apple.com/newsroom/2026/06/apple-aids-app-development-with-new-intelligence-frameworks-and-advanced-tools/), [WWDC26 iOS guide](https://developer.apple.com/wwdc26/guides/ios/), [MacRumors on Siri Extensions](https://www.macrumors.com/2026/09/14/siri-can-be-swapped-out-for-chatgpt-claude/), [MacRumors on Japan side button](https://www.macrumors.com/2025/11/17/ios-26-2-iphone-users-japan-siri/)
- Foundation Models: [Apple updates page](https://developer.apple.com/documentation/updates/foundationmodels.md)
- Background limits: [WWDC25 session 227](https://developer.apple.com/videos/play/wwdc2025/227/), [iOS 27 release notes](https://developer.apple.com/documentation/ios-ipados-release-notes/ios-ipados-27-release-notes.md), Apple forums [816774](https://developer.apple.com/forums/thread/816774), [807957](https://developer.apple.com/forums/thread/807957), [whisper.cpp #3531](https://github.com/ggml-org/whisper.cpp/issues/3531)
- Memory budget: [Apple forums 834805](https://developer.apple.com/forums/thread/834805)
- Runners: LoopCtrl, iCTRL, PhoneAgent, Louis-iPhone-Agent (linked above), [Appium preinstalled WDA](https://appium.github.io/appium-xcuitest-driver/latest/guides/run-preinstalled-wda/)
- Models: [MAI-UI](https://github.com/Tongyi-MAI/MAI-UI), [GUI-Owl-1.5](https://arxiv.org/abs/2602.16855), [Step-GUI](https://arxiv.org/pdf/2512.15431), [LiteRT-LM Gemma 4](https://developers.google.com/edge/litert-lm/models/gemma-4), [CoreML-LLM](https://github.com/john-rocky/CoreML-LLM), [iOSWorld](https://www.alphaxiv.org/abs/2606.09764v1)
- Jailbreak status: [iClarified](https://www.iclarified.com/jailbreak)
- Speech: [Argmax on SpeechAnalyzer](https://www.argmaxinc.com/blog/apple-and-argmax)
- Pricing: [Claude pricing](https://platform.claude.com/docs/en/about-claude/pricing)
