# JarvisKit: the Swift shell (non-UI)

JarvisKit is a SwiftPM package that does all the IO for the Rust core. The core is a sans-IO
state machine (`JarvisSession.step(nowMs:event:) -> Effect`, see `docs/ARCHITECTURE.md`).
JarvisKit runs the loop effect → IO → event, and the iOS app target (written separately)
depends on it for the runner client, the model client, the stores and the tools.

```
Shell/
├─ Package.swift
├─ project.yml                   XcodeGen: app Jarvis + UI tests JarvisUITests, scheme Jarvis
├─ scripts/bootstrap.sh          builds the Rust library, generates the bindings
├─ Sources/
│  ├─ JarvisCoreMissing/         stand-in whose #error says "run bootstrap.sh"
│  ├─ JarvisCore/                generated JarvisCore.swift          (gitignored)
│  └─ JarvisKit/
│     ├─ Driver/                 SessionDriver, IORace, DriverUpdate, DriverConfiguration,
│     │                          HumanGate, MonotonicClock
│     ├─ Runner/                 PhoneRunner (+ ObservedUI), LoopCtrlRunner, LoopCtrlAPI,
│     │                          LoopCtrlRPCClient, LoopCtrlError, RequestSealer
│     │  └─ UITree/              RawElement, XCUIElementTypeCode, UiTreeBuilder (snapshot → UiNode),
│     │                          LockScreenSignature, LockSignals (lock decision rule)
│     ├─ Model/                  ModelProvider + ModelCapabilities, OpenAICompatibleModel,
│     │                          ModelError, ScreenshotAnnotator
│     ├─ Store/                  MemoryPersisting, FileMemoryStore, ScreenshotStore
│     ├─ Tools/                  ToolHost, ToolRegistry, ToolSpecs, SampleTools
│     ├─ Support/                JSONValue, HTTPTransport, SessionConfigBuilder
│     ├─ QA/                     PromptScreen, ScriptedModel (+ ElementQuery, ScriptStep),
│     │                          Transcript + RecordingModel, FormCaptureServer, DemoPhone
│     ├─ Skills/                 SkillLibrary (+ SkillDraft, SkillRoute, SkillCandidate, SkillSeeds),
│     │                          CoreErrorMessage
│     ├─ Workflows/              WorkflowLibrary (+ AskRoute, LocalTime, WorkflowSeeds), WorkflowDraft
│     │                          (TriggerDraft, StepDraft, WorkflowField), WorkflowRunner,
│     │                          WorkflowScheduler
│     ├─ Remote/                 RemoteControlService, RemotePorts (SecretStore, RandomSource,
│     │                          RemoteTransport, Redacted), InMemoryRemoteTransport + InProcessRemote,
│     │                          RemoteAuditLog
│     └─ App/                    JarvisSettings (+ Codable mirrors), SettingsStore, TaskHistory,
│                                MemorySnapshot, HumanPromptBroker, LiveRun, JarvisCoordinator,
│                                TaskCenter, ModelBackendFactory, ConnectionProbe, AwayMode, AppAccess
├─ App/                          Apple-only, outside Sources/ (never in the Linux build)
│  ├─ Jarvis/                    SwiftUI app: Design/, Views/, Playground/, Intents/, Platform/
│  │                             (Keychain, SecRandomCopyBytes, NWListener, notifications)
│  └─ JarvisUITests/             Runner/ (XCUITestPhoneRunner), Support/ (QA harness), tests
├─ Tests/JarvisKitTests/         swift-testing; drives the real Rust core
├─ Generated/                    C module + host static library      (gitignored)
└─ Frameworks/                   JarvisCoreFFI.xcframework (macOS)   (gitignored)
```

Targets: `JarvisCoreFFI` (C header and module map from the UniFFI generator), `JarvisCore`
(generated Swift bindings), `JarvisKit`, `JarvisKitTests`. All of them use Swift 6 language
mode with complete strict concurrency, and the build has no warnings.

## Bootstrap and test on Linux

Requirements: Rust (rustup), Swift 6 for Linux (here `/opt/swift/usr/bin`).

```sh
export PATH=/opt/swift/usr/bin:$PATH
cd Shell
scripts/bootstrap.sh                 # core defaults to ../core; or: scripts/bootstrap.sh /path/to/core
swift build
swift test
```

- The core path comes from the first argument, then `$JARVIS_CORE_DIR`, then `<repo>/core`.
- Cargo honours `CARGO_TARGET_DIR`, so the core tree can stay read-only.
- `JARVIS_FFI_PROFILE=debug` builds a debug static library. The default is release, with LTO
  and without the bindgen CLI, the same way `build-xcframework.sh` builds the device library.
- Run bootstrap again whenever `core/` changes. The bindings are generated from the exact
  library that gets linked, so they always match it.

How linking works on Linux: `Generated/JarvisCoreFFI/` holds `JarvisCoreFFI.h` and
`module.modulemap`, and the package declares it as a `systemLibrary` target. The `JarvisCore`
target links `Generated/lib/libjarvis_core_ffi.a` with `-L<abs path>` (an `unsafeFlags`
setting) plus `linkedLibrary("jarvis_core_ffi")`. The `-L` flag is unsafe, so this path is for
development and testing only. Only the `.a` file is in that directory, so the link is static.

If the generated files are missing, the manifest compiles `Sources/JarvisCoreMissing`
instead. `swift build` then fails with:
`error: JarvisCore bindings are missing: run Shell/scripts/bootstrap.sh …`.

### On CI

`.github/workflows/shell.yml` runs the three commands above on a Linux runner for every
change under `core/` or `Shell/`, so a core change the Swift side has not followed fails
there in a few minutes. The app and UI tests under `App/` are compiled only by
`.github/workflows/ios.yml` on a macOS runner.

## Bootstrap and test on a Mac (untested: written on Linux)

```sh
cd Shell
scripts/bootstrap.sh                 # host static lib + core/jarvis-ffi/scripts/build-xcframework.sh
JARVIS_LINK=host swift test          # tests on macOS link the host static library
```

- `bootstrap.sh` on macOS also runs `core/jarvis-ffi/scripts/build-xcframework.sh`. That
  script needs Xcode and the Rust targets `aarch64-apple-ios` and `aarch64-apple-ios-sim`.
  Bootstrap installs the result as **`Shell/Frameworks/JarvisCoreFFI.xcframework`**. It is
  renamed from `JarvisCore.xcframework` so the binary target is named after the C module it
  provides and does not clash with the Swift target `JarvisCore`. The Swift bindings are
  taken from the same build.
- On Apple hosts, `Package.swift` uses `.binaryTarget(name: "JarvisCoreFFI", path:
  "Frameworks/JarvisCoreFFI.xcframework")` by default, with no unsafe flags. An app target
  that adds `Shell` as a local package gets the iOS device and simulator slices from it.
- The xcframework has iOS slices only. `swift build` / `swift test` for macOS therefore
  need `JARVIS_LINK=host`, which selects the Linux-style host static library path.
- Platforms: iOS 26, macOS 15. `Synchronization.Mutex` needs iOS 18 / macOS 15.

## Protocols (the injection points)

| Protocol | Method(s) | Implementations here |
|---|---|---|
| `PhoneRunner` | `snapshot(includeScreenshot:) -> RunnerObservation` (runner text or a typed `UiNode` tree, `ObservedUI`), `perform(RunnerCommand) -> RunnerResult` | `LoopCtrlRunner` (text), `DemoPhone` (typed), `XCUITestPhoneRunner` (typed, UI-test target) |
| `ModelProvider` | `capabilities: ModelCapabilities`, `generate(ModelRequest) -> String` | `OpenAICompatibleModel`; `ScriptedModel` (QA/demo); wrappers `RecordingModel`, `TappedModel` |
| `ToolHost` | `toolSpecs()`, `execute(ToolCall) -> ToolResult` (non-throwing) | `ToolRegistry` (+ `SampleTools.clock_now`, `echo`) |
| `HumanGate` | `confirm(ConfirmRequest) -> Bool`, `answer(String) -> String` | `HumanPromptBroker` (the app's sheets), `QAOwner` (UI tests) |
| `MemoryPersisting` | `save(String)` | `FileMemoryStore` |
| `MonotonicClock` | `nowMs() -> UInt64` | `SystemMonotonicClock` (named so it does not shadow `Swift.Clock`) |
| `RequestSealer` | `seal(body:into:)`, `unseal(body:response:)` | `PlaintextSealer` (pass-through) |
| `HTTPTransport` | `send(URLRequest) -> (Data, HTTPURLResponse)` | `URLSessionTransport` (FoundationNetworking on Linux) |
| `ScreenshotAnnotator` | `annotate(_:marks:)` (set-of-marks drawing) | `NoAnnotation`; the CoreGraphics version belongs in the app |

`ModelCapabilities` (context tokens, vision, `constrainedDecoding: .none / .gbnfGrammar /
.jsonSchema`) fills the core's `ModelProfile` through `modelProfile()`. When constrained
decoding is on, the core puts both the GBNF grammar and the JSON schema in each
`ModelRequest`, and a backend enforces whichever one its runtime supports. On-device
runtimes (llama.cpp: GBNF, Foundation Models / LiteRT-LM: schema) implement the same
protocol. They resolve the image from the shared `ScreenshotStore` by `ImageRef.id`.

`SessionConfigBuilder.make(capabilities:tools:knownApps:agentBundleId:skill:adjustPolicy:)` starts from
`defaultSessionConfig()` and mutates it. JarvisKit never calls the memberwise initialiser
of `Policy` or `SessionConfig`, so new core fields keep their defaults. `RunnerSnapshot`,
`ToolSpec` and `ToolParam` have no core defaults. Each is built in exactly one place
(`RunnerObservation.runnerSnapshot`, `ToolSpecs`), so a new field means one edit.

## Skills, Away mode, Apps (JarvisKit side, Linux-tested)

- **`SkillLibrary`** (`Skills/SkillLibrary.swift`): the owner's skills in `skills.json`
  (`JarvisPaths.skills`), backed by the core's `JarvisSkills`. Synchronous and serialised
  (a `Mutex`; the file is small). Writes are atomic with the same file protection as
  `FileMemoryStore`. Load issues are kept and shown: the core writes damaged entries back
  unchanged, and a file it refuses as a whole (not JSON, newer schema) is renamed to
  `skills.json.corrupt-<ms>` and reported, never deleted; if it cannot be renamed, saving is
  blocked. API: `skills`, `skill(id:)`, `loadIssues`, `save(SkillDraft)` (new drafts become
  Written skills with an id from the name; `{slot}`s in the phrases become parameters),
  `setEnabled`, `remove`, `recordRun`, `promote(flowId:memoryJSON:name:)`, `route(text)`,
  `context(for:values:)`, `context(skillId:values:)`, `taskText(skillId:values:)`, and
  `learnedFlowID(forTask:memoryJSON:flowIDs:)` (which flow a finished task would replay, decided
  by the core: each flow is promoted into a scratch store and the task is matched against it).
- **Errors**: `CoreErrorMessage` matches the generated `JarvisError` (it is `public`; the
  generator prints `public` and `enum JarvisError` on separate lines) and gives the case name
  and the core's words; remote cases get fixed texts that echo no input.
  `SkillLibraryError.invalidSkill(message)` carries the core's words verbatim and `field` says
  which form field they are about (routing only: every rule is the core's).
- **`SkillRoute`** from `route(text)`: `.plain` (no match), `.run(candidate, context)` (one
  exact, unambiguous, complete match), `.askArguments(candidate)` (required values missing),
  `.choose(candidates)` (the core's `needsOwnerChoice`). `JarvisCoordinator.run(task:settings:
  skill:human:…)` puts the context in `SessionConfig.skill`; the task record keeps
  `skillId`/`skillName`, and `LiveRun.skillName` drives "Using skill: …".
- **`AwayModeController`** (`App/AwayMode.swift`, main actor) over `AwayDisplay` (brightness,
  idle timer, proximity): explanation until acknowledged (remembered in
  `JarvisSettings.awayAcknowledged`), dims to 0 after saving the brightness and the idle-timer
  setting, restores them on exit, on leaving the foreground and on terminate, re-dims on return;
  double exits never write 0. `AwayModeCopy` holds the owner-facing costs (PLAN §3.1).
  No wake word is built and nothing claims one.
- **`AppAccessCatalog.make`** (`App/AppAccess.swift`): one row per app from the trust list, the
  known apps and the tools that declare an app. Screen = not denied (by the owner or the core's
  `deniedApps`); app actions = registered `.shortcut` tools for the app; direct = registered
  `.native` tools for the app. With today's registry (`clock_now` only, no app) every app is
  screen-only.

## TaskCenter, workflows, remote control (JarvisKit side, Linux-tested)

- **`TaskCenter`** (`App/TaskCenter.swift`, main actor): the one entry every task takes —
  Ask, Siri, Run now, a workflow step, a paired device — `start(text, skill:, origin:)` →
  `JarvisCoordinator.run` with the owner's settings, the one `HumanPromptBroker` and the same
  `OwnerAttention`, so nothing skips first use, a confirmation or the lock handling. One
  session at a time; while a workflow holds the slot only its own steps start. Observers get
  `sessionStarted / session(RunEvent) / sessionFinished / workflowStarted / workflowFinished`
  in order on the main actor (the run view and remote control both listen). `TaskOrigin`
  (`owner`, `remote(taskId, deviceId, name)`, `workflow(tag)`) tags history records.
- **`WorkflowLibrary`** (`workflows.json`): the core's `JarvisWorkflows`, same file rules as
  `SkillLibrary` (atomic, protected, damaged entries kept, a refused file set aside), sharing
  the live `JarvisSkills` for warnings and runs (lock order: workflows, then skills).
  `save(WorkflowDraft)`, `setEnabled`, `remove`, `recordRun`, `warnings(for:)`,
  `forExternal(key:)`, `find(key)` (key, id, then name), `due(at:windowMinutes:)`,
  `minutesUntilNextRun(of:at:)`, `catalog`, and `resolve(text) -> AskRoute` over
  `resolveUtterance` (Ask no longer calls `matchTask` directly). `WorkflowField` routes a core
  message (`name`, `trigger…`, `steps[i]…`) to its place on the form.
- **`WorkflowRunner`** (main actor): drives one `JarvisWorkflowRun` — `startSession` →
  `TaskCenter.run` (origin `.workflow`), `askOwner` → the broker's yes/no prompt
  (`HumanPrompt.yesNo`), `notify` → `WorkflowNotifier`, `wait` → a cancellable sleep on the
  monotonic clock, `finished` → `recordRun` plus an Activity entry (`TaskRecord.isWorkflow`,
  `sessionIds`; sessions carry `workflowRunId`). A second start is refused with the running
  workflow's name; `cancel()` stops the session, withdraws a question, ends a wait.
  `Progress` (step states, activity, notes, outcome) drives the workflow run view.
- **`WorkflowScheduler`**: each check asks the core what fired within the catch-up window (15
  min) and starts it, at most once per scheduled minute (fired minutes kept in
  `workflow-schedule.json`, so a relaunch catches up once); minutes that ended before the
  workflow was saved are skipped; while something runs it waits for the next check. iOS does
  not wake the app: schedules run while Jarvis is open (the UI says so); exact times come from
  a Shortcuts automation calling `RunJarvisWorkflowIntent`.
- **`RemoteControlService`** (main actor) over `JarvisRemoteHub`: every hub call goes through
  one FIFO inbox and its effects run in order; `storeChanged` writes `toJson()` to the
  `SecretStore` before the next effect (a failed write sends nothing more and closes every
  connection); phone events caused by effects are queued behind them, so frames leave in the
  hub's counter order. Remote `startTask`/`runSkill`/`runWorkflow` go through
  `TaskCenter`/`WorkflowRunner` (skills re-checked with `SkillLibrary.context`); `cancelTask`
  cancels only what its id names; `decide` applies only to the confirmation on screen (same
  hub request id, category and broker request; approval also needs `remotelyApprovable`).
  Progress is forwarded with the typed `Screen` from the last observation. `catalogChanged`
  is sent when enabled skills/workflows change. Off by default; on loads the hub from the
  secret store (a refused store is kept and reported), starts the transport and a 1 s tick;
  a poisoned hub is rebuilt from the store. `RemotePairingOffer`/`Redacted` never print.
  `RemoteAuditLog` is bounded (300) and on disk (no secrets). Ports: `SecretStore`
  (`InMemorySecretStore`), `RandomSource` (`GeneratorRandomSource` for tests only),
  `RemoteTransport` (`InMemoryRemoteTransport`; `InProcessRemote` drives the real
  `JarvisRemoteClient` over it).

## SessionDriver

`SessionDriver` is an actor. `init(config:task:memoryJSON:dependencies:configuration:)`
throws when the core rejects the memory. In that case call `FileMemoryStore.quarantine()`
and retry with `""`. `run() async -> RunResult` never throws. `cancel()` stops the run, and
`nonisolated updates: AsyncStream<DriverUpdate>` reports progress.

| Effect | IO | Event on success | Event on error / timeout |
|---|---|---|---|
| `observe` | `runner.snapshot` | `observedRunnerText` for runner text, `observed` for a typed tree (screenshot put in `ScreenshotStore`, handle passed; `deviceLocked` passed through) | retried `observeAttempts` times, then `observeFailed(reason:)`; the core finishes Failed after `max_consecutive_failures` |
| `awaitUnlock` | `OwnerAttention.awaitUnlock` once per wait, then every `unlockPollInterval` `runner.lockState()` | `tick` (locked), `deviceUnlocked`, or a fresh observation when the runner cannot tell | a failed poll is an `ioFailed` update and the observation answers |
| `infer`, `callTool`, `confirm`, `askUser` while the runner reports a lock (`lockWatchInterval`) | the wait is abandoned | `deviceLocked` (`.lockDetected` update) | (none) |
| `infer` | `model.generate` | `modelOutput` | `modelFailed(error:)` |
| `act` | `runner.perform` | `actResult` | `actResult(ok: false, error:)` |
| `callTool` | `tools.execute` | `toolResult` | `toolResult(ok: false, …)` (timeout) |
| `confirm` | `human.confirm` | `userDecision(approved:)` | `userDecision(approved: false)` |
| `askUser` | `human.answer` | `userAnswer(text:)` | `cancel`, `endReason = .noAnswer` |
| any, on `cancel()` / task cancellation | the IO in flight is abandoned at once | `cancel` | (none) |

Further guarantees:

- Every IO has its own deadline (`DriverConfiguration`), and `maxEffects` is a backstop.
- IO runs in `IORace`. The waiter resumes on the first of completion, deadline or
  cancellation, even if the IO ignores cancellation. Abandoned IO keeps running detached
  until it ends, and its result is dropped.
- `finish()` is called once and its JSON goes to `MemoryPersisting.save` exactly once, on
  every exit path. The save runs in a detached task with its own deadline, so a cancelled run
  still saves. If `finish()` throws (the core is poisoned), nothing is saved and
  `memory == .unavailable`.
- Concurrent or repeated `run()` calls share the single run and its result.
- `step` is synchronous and fast, so the driver calls it directly. File IO runs on a private
  dispatch queue, never on the cooperative pool.
- The update order is documented on `DriverUpdate` and checked by a test.

## iOS app and simulator QA (written on Linux; first compiled by CI on macOS)

```sh
cd Shell
scripts/bootstrap.sh                 # on a Mac: also builds Frameworks/JarvisCoreFFI.xcframework
xcodegen generate                    # Jarvis.xcodeproj (gitignored)
xcodebuild test -project Jarvis.xcodeproj -scheme Jarvis \
  -destination 'platform=iOS Simulator,name=iPhone 17 Pro' CODE_SIGNING_ALLOWED=NO
```

- `project.yml`: app `Jarvis` (`dev.jarvis.app`, `App/Jarvis/`), UI tests `JarvisUITests`
  (`dev.jarvis.app.uitests`, `App/JarvisUITests/`), shared scheme `Jarvis`, local package `.`
  (products `JarvisKit`, `JarvisCore`). Both targets use **Swift 5 language mode** with complete
  concurrency checking as warnings (JarvisKit stays Swift 6): XCTest UI APIs are main-actor
  bound and this code was never compiled before CI. For a device, fill in `DEVELOPMENT_TEAM`
  (marked in `project.yml`); a free Apple ID works.
- The app: tabs **Ask · Skills · Workflows · Apps · Control** over JarvisKit (`AppModel` →
  `JarvisCoordinator`). Ask routes each request through `WorkflowLibrary.resolve` (chooser sheet
  with "Just do what I said", or a values sheet; the run starts after the sheet is gone), and
  has History (Activity) and Away mode buttons. Skills: search, kind filter, load-issue
  banner, detail (phrases, guidance, guardrails, on/off, Run now, Delete), editor for Written
  skills with the core's messages inline. Activity (from Ask and Control) offers "Save as
  skill" for finished tasks with a learned flow, and lists a workflow run with its sessions
  under it. Apps: ways in + trust. Control: Remote control, Activity, Away mode rows, then the
  rules (identifiers `rules.*` kept). Workflows: list (trigger, steps, on/off, next run,
  load-issue banner, schedule note), detail as a node chain, editor (trigger kinds, ordered
  steps with add/remove/reorder, on-failure, the core's messages inline, skill warnings), Run
  now with the workflow run view (step states, the session inside, Stop, outcome). Ask runs a
  workflow by its phrase (`resolveUtterance`); the chooser offers workflows too. Remote
  control: master switch, status, Pair a device (code, QR from `CIQRCodeGenerator`, countdown,
  Cancel; the code is read from the model each second and gone when the offer ends), devices
  with per-device permissions, the core's never-remote list, rename, remove, lockout, audit
  log. App Intents: `AskJarvisIntent`, `RunJarvisWorkflowIntent` (`openAppWhenRun = false`,
  waits up to 25 s for the outcome). Adapters in `Platform/`: `KeychainSecretStore`
  (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synchronizable),
  `SecureRandomSource` (`SecRandomCopyBytes`), `NetworkRemoteTransport` (`NWListener`, TCP,
  Bonjour `_jarvis-remote._tcp`, local addresses only, no cellular; Info.plist
  `NSLocalNetworkUsageDescription` + `NSBonjourServices`), `UserNotificationWorkflowNotifier`. In playground mode the
  Playground tab replaces Workflows (a sixth tab would go behind "More"); Workflows is then a
  row in Control. Also: the run view (what Jarvis sees, step timeline, Stop, "Using skill"),
  confirmation and question sheets (`HumanPromptBroker`), Away mode (explanation, then a black
  full-screen cover; leave by a 2 s touch and hold; `UIKitAwayDisplay` is the thin adapter),
  App Intent `AskJarvisIntent` + `JarvisShortcuts`. Tokens in
  `App/Jarvis/Design/Theme.swift`, icons in `JarvisIcon`. Launch arguments:
  `-jarvis-playground` (Playground fixture tab + demo backend: `DemoPhone` and a scripted
  model, separate data folder), `-jarvis-reset` (fresh settings, memory, history, skills),
  `-jarvis-seed-damaged-skills` / `-jarvis-seed-ambiguous-skills` (write `SkillSeeds.damaged`
  / `.ambiguous` as the skills file, for the banner and chooser tests),
  `-jarvis-seed-scheduled-workflow` (saves `WorkflowSeeds.scheduled`),
  `-jarvis-seed-remote-device` (playground: remote control on, "Test Mac" paired in-process).
  In playground mode remote control uses the in-memory transport and secret store: UI tests
  never touch the network or the Keychain.
- The model backend is chosen in one place: `ModelBackendKind` / `ModelBackendFactory`.
- `XCUITestPhoneRunner` (UI-test target) is a `PhoneRunner` over public XCUITest API: it copies
  `XCUIElementSnapshot` into `RawElement`, and `UiTreeBuilder` (JarvisKit, Linux-tested) makes
  the typed tree. The QA tests run the real core + `SessionDriver` + `ScriptedModel`, which
  picks elements by reading the prompt's screen block. Tier A drives the Playground
  fixtures; Tier B drives Settings, SpringBoard and Safari (form served by
  `FormCaptureServer` on the shared loopback); `LockScreenProbeTests` gathers evidence about
  the lock screen and `LockedPhoneAgentTests` locks mid-task. The XCUITest runner reads the
  lock from Darwin notify `com.apple.springboard.lockstate` (looked up with `dlsym`;
  confirmed working on the iOS 26.5 simulator in run 6: it flips at the lock, while the
  "front app" reading lags it by about 0.3 s) and from SpringBoard showing the lock screen
  (`LockScreenSignature`, from the run-16 probe trees). `LockSignals` (JarvisKit, tested)
  combines them: commands are refused when either says locked; `lockState()` (unlock polls,
  lock watch) and observations say unlocked as soon as an app or the home screen is in front,
  so a stale notify state cannot hold a run in its unlock wait. Every change of the readings
  goes into the notes and `lockSignalHistory` (every reading with
  `noteLockSignalsEveryObservation`, on in the lock test). After every command the runner
  reads the notify state again (cheap, no snapshot): a command that "succeeded" while the
  phone locked under it is reported as `RunnerFailure.deviceLocked`
  (`RunnerResult.checkedAfterCommand`), so the core does not trust it and looks again after
  the unlock; live runs wrap `LoopCtrlRunner` in `LockCheckingRunner` for the same check over
  `device_info`. `ScriptedModel` decides from the prompt's "Steps so far" whether a tap/type
  took effect (`ok`, whatever follows it (`ok; changed: …`, `ok; now in …`, `ok; screen
  unchanged`) is not read; or `done, but …` while its precondition no longer holds) and
  otherwise retries it, at most `Options.maxRetries` times. A switch is rendered `(on)` /
  `(off)` by the core, so `ElementQuery(isOn:)` asks for the state and `value:` stays for
  real values; `StepOutcome` classifies a history result, `PromptScreen.cues` reads the scroll
  and page cue lines, and `ScreenContractTests` pins all of it against the real core's prompt.
  A confirmation for an icon the core cannot name (`FirstUse`, app `com.apple.springboard`)
  is worded by `ConfirmationWording` ("Open "Weather" from the Home Screen?").
- Time limits in the UI tests: the harness sets `maxLockedWaitMs` 20 s (3 s in the lock
  probe), unlock polls 0.5 s, lock watch 1 s, every driver deadline ≤ 30 s and a run budget of
  150 s (under the CI per-test limit); playground mode in the app does the same
  (`JarvisCoordinator(maxLockedWaitOverrideMs:)`, 20 s). No UI-test wait is longer than 30 s. Every observation attaches `<run>.NN-screen.txt`, `<run>.NN-tree.txt`, `<run>.NN.png`;
  every run attaches `<run>.transcript.json` and `<run>.result.txt` (`.keepAlways`). Names
  use `.`, not `/`: the result-bundle export drops `/`.

## Not verified on a device

Nothing in `LoopCtrlRunner` has talked to a real runner, `OpenAICompatibleModel` has not been
used against a real model server, and the Apple-platform build (binary target, xcframework
path, file protection) has not been compiled. Every LoopCtrl name the adapter uses is in
`Sources/JarvisKit/Runner/LoopCtrlAPI.swift` with its evidence tag from `docs/RUNNER.md`. The
first device test (spike S1) must confirm the following assumptions:

1. **Envelope**: the JSON-RPC 2.0 field names `jsonrpc`, `id`, `method`, `params` and
   `result` [unverified]. Only the type names and the `error{code,message,data}` shape are
   [source].
2. **Auth**: requests are sent in plain text through `PlaintextSealer`. LoopCtrl's encrypted
   envelope (`v, tok, ts, nonce, ct, tag`) is not implemented. A real sealer for the fork
   plugs in through `RequestSealer`. Error code `-32099` is mapped to "pair again" [source].
3. **Endpoint**: `http://127.0.0.1:8300/rpc` [readme].
4. **Observation**: `page_source{visible_only: true, max_children: 50}` [readme] returns
   `source` (text tree) and `format: "text"` [source]. The keyboard comes from
   `keyboard.visible` [readme]. When that is missing, the adapter looks for a `Keyboard` line
   in the tree [unverified] and adds a note.
5. **Foreground app**: from `device_info` → `system.foreground_app` [source: result lists].
   Whether that value is a string or an object `{bundle_id, name}` is [unverified], so both
   are accepted. Anything else is a typed error, because the deny list needs the bundle id.
6. **Orientation**: `device_info.orientation` [readme]. The value strings
   (`portrait`, `landscape_left`, …) are [unverified]. Unknown or missing values become
   portrait, with a note.
7. **Screen size**: learned once from a screenshot's `point_width/point_height` [source,
   iCTRL lineage], or from pixels ÷ `scale` [readme]. It can also be configured. The
   `device_info` size keys were never seen.
8. **Screenshot**: base64 `data`, `format`, `pixel_width/pixel_height` [source]. The MIME
   type is taken from the bytes when `format` is absent.
9. **Units**: `duration` of `long_press`, `drag` and `wait` is assumed to be seconds
   [unverified].
10. **Swipe**: sent as `drag{from_x, from_y, to_x, to_y, duration}` [source names], because
    `swipe` takes no from/to.
11. **Keys**: `home` is a method [source]. Return is sent as `type{text: "", submit: true}`
    [unverified]. Delete, Tab, Space and Escape are typed as XCUIKeyboardKey characters
    (`\u{8}`, `\t`, ` `, `\u{1B}`) [unverified].
12. **Typing**: `type{text}` without x,y is assumed to go to the focused field [unverified].
13. **Launch**: `launch_app{bundle_id}` [source name, body unseen]. `activate_app` is
    available through `LaunchStrategy`.
14. **Success / failure**: any `result` counts as success. A JSON-RPC `error` becomes
    `RunnerResult(ok: false)`, and business errors come with HTTP 200 [source].
15. **Lock state** [unverified]: `device_info.system.screen_locked` (boolean). `true` makes
    the snapshot a locked observation (page_source is not read) and `lockState()` report
    `.locked`; absent means `.unknown` (the driver then answers waits with observations).
    A business error whose message contains "locked", "lock screen" or "passcode" becomes
    `RunnerFailure.deviceLocked`. `device_info` is now read before `page_source`.
16. **Tree text**: parsed in Rust (`parseRunnerText`), not here. Labels containing quotes,
    commas or braces are untested against real output.

Also untested: Apple `URLSession` behaviour (only the Linux FoundationNetworking path ran),
the `completeFileProtectionUntilFirstUserAuthentication` write option, and the renamed
xcframework as a SwiftPM binary target. Clang should find its module map in the
`Headers/JarvisCoreFFI/` subdirectory, but that is unconfirmed.

## Notes for the core

- `JarvisError` is `public` (an earlier note here said `internal`: the generator prints
  `public` on its own line); `CoreErrorMessage` now matches its cases.
- `JarvisWorkflowRun` reads skills from the live `JarvisSkills`, but `JarvisWorkflows.due` and
  `nextScheduled` take local time only, so "fired minutes" are the shell's to remember; a
  `due` that returned the scheduled minute would save a second call per workflow.
- There is no FFI for `intent_key`; `SkillLibrary.learnedFlowID` gets the same answer by
  promoting flows into a scratch store and matching. An `intentKey(text)` export would be
  simpler.
- Set-of-marks drawing is the shell's job (`ScreenshotAnnotator`). No annotator that draws
  exists yet.
