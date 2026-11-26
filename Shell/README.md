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
│     ├─ Store/                  MemoryPersisting, FileMemoryStore, ScreenshotStore, StoredFile
│     │                          (StoredFileRead: missing / unreadable / damaged)
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
│     └─ App/                    JarvisSettings (+ Codable mirrors), SettingsStore, SettingsGate, TaskHistory,
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

`.github/workflows/shell.yml` runs the bootstrap and then `scripts/host-test.sh` (build and
test) on a Linux runner for every change under `core/` or `Shell/`, so a core change the
Swift side has not followed fails there in a few minutes. The app and UI tests under `App/`
are compiled only by `.github/workflows/ios.yml` on a macOS runner, which runs the same
script on the Mac host first.

`scripts/host-test.sh`, when the tests fail or the test process dies, reruns every suite
alone, then every test of a failing suite alone, and on a Mac prints the newest crash report
with the code before the crashing call. It also refuses one source pattern before building:

- **No `async` closure as a default argument.** With Swift 6.3.3 (Xcode 26.6) such a closure
  on a public function, called from another module, aborts the process when it returns
  (`freed pointer was not the last allocation`). Linux (Swift 6.4) does not show it. Take an
  optional and do the default in the body, as `WorkflowRunner`'s `sleep` does.

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

`SessionConfigBuilder.make(capabilities:tools:knownApps:agentBundleIds:skill:adjustPolicy:)` starts from
`defaultSessionConfig()` and mutates it. JarvisKit never calls the memberwise initialiser
of `Policy` or `SessionConfig`, so new core fields keep their defaults. `RunnerSnapshot`,
`ToolSpec` and `ToolParam` have no core defaults. Each is built in exactly one place
(`RunnerObservation.runnerSnapshot`, `ToolSpecs`), so a new field means one edit.

### Prompt cache

The system prompt is 70-85% of each prompt and identical at every step, so
`OpenAICompatibleModel` always sends it first (byte-for-byte as the core gave it) and adds
`"cache_prompt": true` (llama.cpp's server keeps the prefix in its KV cache) according to
`Configuration.promptCaching` / `JarvisSettings.modelPromptCaching`: `.automatic` (default)
sends it and, when the server answers 400/422 with a body naming the field (`cache_prompt`,
"unrecognized", "unknown field", "extra fields", "additional properties"), retries that one
request without it and omits it from then on for that client; any other error is returned
unchanged and not retried. `.on` always sends it, `.off` never. The retry is one more HTTP
call inside the same `generate`, so the caller's deadline still covers both. At task start
the coordinator calls `startPromptWarmUp(for:system:)`: a detached, never-awaited
`PromptWarming.warmUp` (`max_tokens: 1`, user "ok", 5 s own timeout, errors ignored, one in
flight, once per system prompt per client unless forced). Real requests are not queued behind
it. Prompt text is never logged.

## Stored files (JarvisKit side, Linux-tested)

Every JSON store keeps three cases apart (`StoredFileRead`): **missing** (defaults or empty,
normal), **unreadable** (the file exists but reading failed: on iOS every read before the
first unlock after a restart, or an I/O error) and **damaged** (read, but not valid for the
schema). A file that could not be read is never overwritten: writes are refused (or kept in
memory) and the next call reads it again, so the store recovers after the first unlock
without a relaunch. A damaged file is renamed to `<name>.corrupt-<ms>` next to it (never
deleted or overwritten, as `SkillLibrary` does) and the caller is told.

- **`SettingsStore`**: `read() -> SettingsRead` (`.loaded`, `.missing`, `.unreadable(reason:)`,
  `.damaged(reason:setAside:)`). A missing key takes its default (old files keep loading); a
  safety field that is present but cannot be decoded (`policy_mode`, `first_use_approval`,
  `relaxed_categories`, `deny_categories`, `app_trust`, `remote_control_enabled`, `max_steps`,
  `max_minutes`, `max_locked_wait_minutes`: wrong type, `null`, or a value this build does not
  know, also one entry of a list) makes the whole file damaged, so it is set aside and the owner
  must review; the model/runner addresses, context size, prompt caching (`model_prompt_caching`:
  `automatic` / `on` / `off`), Away toggles and the two real-screen
  options stay lenient; `save` throws `StoredFileError` while the file is unreadable;
  `load()` (defaults for anything but `.loaded`) must not feed a task. **`SettingsGate`** (main
  actor) holds the settings the app runs with and the `problem`; `refresh()` re-reads while
  unreadable, `acknowledgeReview()` ends the damaged state. `TaskCenter(…, settingsProblem:)`
  refuses `start` and `beginWorkflow` with `Refusal.settingsUnavailable(text)` while it
  returns a text (`WorkflowRunner.StartRefusal.settingsUnavailable`; the scheduler retries
  inside its window; a remote start is answered as a failed task).
- **`TaskHistoryStore`**: `read()`; `append` throws while unreadable; a damaged file is set
  aside and the list starts again.
- **`RemoteAuditLog`**: while unreadable new entries stay in memory and the file is not
  touched (`clear()` empties only memory); once readable both are merged, newest first,
  bounded. `loadIssue` says what happened.
- **`WorkflowScheduler`**: while its state is unreadable nothing fires (what fired is
  unknown) and nothing is written; a minute marked meanwhile is merged in later. A damaged
  state is set aside and minutes up to that moment count as fired (no double run). Old state
  files without local slots still load.
- **`FileMemoryStore`**: `read()` reports the case; `JarvisCoordinator.run` does not start
  (`.notStarted(JarvisCoordinator.memoryUnreadable)`) when the memory cannot be read or a
  rejected memory cannot be set aside, because the run's save would replace the learned
  flows and first-use approvals. Only the core's `InvalidMemory` error sets the memory aside;
  any other driver init error ends the run as not started and leaves the file alone. Stores
  of the same path share one serial queue, so saves land in the order they were asked for.
- **`SkillLibrary` / `WorkflowLibrary`**: a file unreadable at launch leaves the library empty
  and saving blocked; `reloadIfBlocked()` (called with `AppModel.reloadSettings()`) reads it
  again, so the owner's skills and workflows appear after the first unlock without a
  relaunch. A library that loaded is never re-read.

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
  `workflow-schedule.json`, so a relaunch catches up once); per workflow the latest fired UTC
  minute and the latest fired local slot (local date + minute, `LocalTime.localDay`) are kept,
  never pruned by age (only workflows that no longer exist are dropped): a fire whose local
  slot already fired is refused (01:30 on the night the clocks go back, flying west), and a
  clock that ran ahead and was set back cannot re-fire a minute. The window is real time: when
  the clocks go forward inside it, it is widened on the wall clock by the skipped minutes, so a
  02:30 schedule on the spring-forward night fires once at the first check after the gap
  (03:00…03:14), never twice; minutes that ended before the
  workflow was saved are skipped; while something runs it waits for the next check. iOS does
  not wake the app: schedules run while Jarvis is open (the UI says so); exact times come from
  a Shortcuts automation calling `RunJarvisWorkflowIntent`.
- **`RemoteControlService`** (main actor) over `JarvisRemoteHub`: every hub call goes through
  one FIFO inbox and its effects run in order; `storeChanged` writes `toJson()` to the
  `SecretStore` before the next effect (a failed write sends nothing more and closes every
  connection); phone events caused by effects are queued behind them, so frames leave in the
  hub's counter order. Remote `startTask`/`runSkill`/`runWorkflow` go through
  `TaskCenter`/`WorkflowRunner` (skills re-checked with `SkillLibrary.context`); `cancelTask`
  cancels only what its id names; `decide` is bound to a prompt id: when the hub's request is
  created, the broker's prompt id for it is recorded (`ConfirmationDesk.confirmations(after:)`),
  and a decision resolves only that exact prompt while it is still pending (category and
  request re-checked; approval also needs `remotelyApprovable`); anything else (a second
  device, an answer already given on the phone, an equal prompt asked again after a lock or in
  step-by-step mode) is refused and audited as "Ignored a stale decision".
  Progress is forwarded with the typed `Screen` from the last observation. `catalogChanged`
  is sent when enabled skills/workflows change. Off by default; on loads the hub from the
  secret store (a refused store is kept and reported), starts the transport and a 1 s tick;
  a poisoned hub is rebuilt from the store; `ensureListening()` restarts only the transport
  (the hub and its devices stay, open connections are closed and the hub told) when the
  service is on but its listener failed. The consumer acknowledges every `.received` to
  its transport (`consumed`); `InMemoryRemoteTransport` counts them (`unconsumedBytes`) and
  can fail its listener (`failListener`) for tests. `RemotePairingOffer`/`Redacted` never print.
  `RemoteAuditLog` is bounded (300) and on disk (no secrets). Ports: `SecretStore`
  (`InMemorySecretStore`), `RandomSource` (`GeneratorRandomSource` for tests only),
  `RemoteTransport` (`InMemoryRemoteTransport`; `InProcessRemote` drives the real
  `JarvisRemoteClient` over it).
- **Live view over remote control** (Linux-tested, `RemoteLiveViewTests`). Phone:
  `RemoteControlService.feedLiveView(viewer, event)` is `AppModel.feedViewer`'s one FFI call
  per driver update while remote control is on (`LiveViewModel.apply(_:streamingTo:nowMs:)` →
  `JarvisViewer.feedAndStream`: feeds the Live view and gives the hub its timeline by
  reference); the hub's frames join the ordered effect path ahead of hub calls not made yet
  (`ready` before the inbox). Every `send` carries a write completion
  (`RemoteTransport.send(_:to:written:)`, default: confirmed at once; the in-memory and
  Network.framework transports confirm on delivery / `.contentProcessed`), reported to the hub
  (`written`, coalesced per connection), so a device that stops reading gets at most the hub's
  128 KiB window plus one frame or snapshot and never slows the run (`holdWrites` in tests).
  `tick` flushes screens held by the hub's 250 ms rate limit. Controller: `RemoteLiveView`
  (start/stop bytes, `receive(_:client:)` applies `receiveAll`'s viewer events to its own
  `LiveViewModel` via `applyDelta`, writes a resync request at most once per gap; state
  `idle`/`starting`/`watching`/`denied`/`ended(reason)`/`incompatible`), driven in tests by
  `InProcessRemote.startWatching`.

## Live view (JarvisKit side, Linux-tested)

`Sources/JarvisKit/Viewer/`: the Live view's logic is in the core (`core/jarvis-viewer`, the
FFI's `JarvisViewer`: reducer, privacy, wire format, cursor path, replay, scene). `LiveViewModel`
is the shell's thin adapter: it maps each `RunEvent` (the same stream `LiveRun` reads) to a
`CoreViewerUpdate` and feeds it (one FFI call per update), republishes the light timeline only
when the core says something changed, caches a few wireframes by screen id (bounded, dropped when
the timeline no longer refers to them), and passes on the per-frame `scene(query:)` and the replay
controls; FFI errors leave it usable (the view draws nothing rather than crash). The SwiftUI view
(`App/Jarvis/Views/LiveScreenView.swift`) draws the scene's view-point values only. The backdrop is a
wireframe of the typed tree, or, with "Show real screen in Live view" on
(`DriverConfiguration.viewerScreenshots`, off by default), screenshots kept in a
`ViewerFrameStore` (memory only, capped by count and bytes). The option never changes the run:
the core's observation is taken exactly as with it off; while it is on and the store is enabled
(checked on every observation), a separate best-effort `PhoneRunner.viewerScreenshot()` follows
an observation the core asked no pixels for (its own deadline, `viewerScreenshotTimeout`; errors
ignored with at most one note per run; never retried; outside every failure count, deadline and
effect budget), and pixels the core asked for are reused. Fail closed on the lock: frames only
for an observation the runner reported unlocked (`RunnerObservation.lockState`), still unlocked
after the capture, never in an unlock wait, dropped when the core answers with an unlock wait; a
runner that cannot report its lock state gets none. Frames of a run that ends with no Live view on
screen are dropped. One privacy rule, decided by the core: every label, placeholder and caption
label goes through the core's redaction (`redactScreenText`) before it is kept; typed text is
shown only for a field resolved from a typed tree that is not secure, not credential-like
(`fieldValueIsHidden`), when the core's redaction leaves the text unchanged and it is what was
typed (else "Type into “<field>”" or "Type text"); never on runner-text screens; links show the
host of http(s) URLs and only `<scheme>:` otherwise. iOS does not let an app draw over other
apps, so the cursor is shown inside Jarvis, not over the app being driven.

### Approval preview

The confirmation sheet shows a screenshot taken when Jarvis asked (or the layout of that
screen) with the control the action names outlined (docs/ARCHITECTURE.md, *Approval preview*,
which lists exactly what is checked: the outline comes from the tree the core used; pixels are
not checked by the core, and content outside the accessibility tree can differ). The core's
viewer computes the picture (`LiveViewModel.approvalTarget`: the evaluated observation's
`ViewerFrameKey`, its screen id, the target in unit coordinates, caption, VoiceOver label).
With "Show the real screen when Jarvis asks" on (`JarvisSettings.approvalScreenshots`, default
on → `DriverConfiguration.approvalScreenshots`, driver default off), `SessionDriver` makes one
best-effort capture of that observation before it asks (`viewerScreenshotTimeout`; the core's
own pixels, taken with the tree, are reused as is; the Live view's frame of it or a fresh
capture is kept only when a tree read after it shows the very screen evaluated, phone
unlocked), whichever app is in front, into an `ApprovalFrameStore` (one frame, memory only),
and removes it as soon as the question is resolved or the phone locks.
`ApprovalPreview.make(prompt:target:frames:appActive:)` binds the picture to the pending
confirmation and the open question, and shows the screenshot ("Screen when Jarvis asked") only
under exactly the evaluated key, with the screen's aspect ratio (±2%) and the app active; else
the wireframe ("Layout from the app's data" / "Layout from the runner's text") or "No picture
of this screen". Nothing of it reaches the encoded timeline, the history or a paired device
(tested in `ApprovalPreviewTests`).

## SessionDriver

`SessionDriver` is an actor. `init(config:task:memoryJSON:dependencies:configuration:)`
throws when the core rejects the memory (`JarvisError.InvalidMemory`). Only in that case call
`FileMemoryStore.quarantine()` and retry with `""` (only if the quarantine succeeded:
otherwise the run's save would overwrite the file); any other error means the run does not
start and the memory file stays as it is (`JarvisCoordinator.startDriver`). `run() async -> RunResult` never throws. `cancel()` stops the run, and
`nonisolated updates: AsyncStream<DriverUpdate>` reports progress.

| Effect | IO | Event on success | Event on error / timeout |
|---|---|---|---|
| `observe` | `runner.snapshot` | `observedRunnerText` for runner text, `observed` for a typed tree (screenshot put in `ScreenshotStore`, handle passed; `deviceLocked` passed through) | retried `observeAttempts` times, then `observeFailed(reason:)`; the core finishes Failed after `max_consecutive_failures` |
| `awaitUnlock` | `OwnerAttention.awaitUnlock` once per wait, then every `unlockPollInterval` `runner.lockState()` | `tick` (locked), `deviceUnlocked`, or a fresh observation when the runner cannot tell | a failed poll is an `ioFailed` update and the observation answers |
| `infer`, `callTool`, `confirm`, `askUser` while the runner reports a lock (`lockWatchInterval`) | the wait is abandoned | `deviceLocked` (`.lockDetected` update) | (none) |
| `infer` | `model.generate` | `modelOutput` | `modelFailed(error:)` |
| `act` | `runner.perform` | `actResult` | `actResult(ok: false, error:)` |
| `callTool` | `tools.execute` | `toolResult` | `toolResult(ok: false, …)` (timeout) |
| `confirm` | `human.decide` (review pass 35: `confirm` plus where it was answered; a gate that only implements `confirm` is not the phone) | `userDecision(approved:byPhone:)` | `userDecision(approved: false, byPhone: false)` |
| `askUser` | `human.answer` | `userAnswer(text:)` | `cancel`, `endReason = .noAnswer` |
| any, on `cancel()` / task cancellation | the IO in flight is abandoned at once | `cancel` | (none) |

Further guarantees:

- Every IO has its own deadline (`DriverConfiguration`), and `maxEffects` is a backstop.
- IO runs in `IORace`. The waiter resumes on the first of completion, deadline or
  cancellation, even if the IO ignores cancellation. Abandoned IO keeps running detached
  until it ends, and its result is dropped.
- A prompt of `HumanPromptBroker` whose asking task is cancelled (the driver abandoning a
  confirmation or question because the phone locked, a timeout) is withdrawn by its own id at
  once; a newer prompt is never touched (`withdraw(_:)` does the same for a caller with an id).
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
  under it. Apps: ways in + trust. Control: Remote control, Activity, Away mode rows, the Live
  view section with its two settings, then the rules (identifiers `rules.*` kept). The
  settings beyond the rules: "Show real screen in Live view" (`liveViewScreenshots`,
  `live_view_screenshots`, off by default) and "Show the real screen when Jarvis asks"
  (`approvalScreenshots`, `approval_screenshots`, on by default; see *Approval preview*). Workflows: list (trigger, steps, on/off, next run,
  load-issue banner, schedule note), detail as a node chain, editor (trigger kinds, ordered
  steps with add/remove/reorder, on-failure, the core's messages inline, skill warnings), Run
  now with the workflow run view (step states, the session inside, Stop, outcome). Ask runs a
  workflow by its phrase (`resolveUtterance`); the chooser offers workflows too. Remote
  control: master switch, status, Pair a device (code, QR from `CIQRCodeGenerator`, countdown,
  Cancel; the code is read from the model each second and gone when the offer ends), devices
  with per-device permissions (Start and stop tasks, See what Jarvis sees, Approve for you,
  and "Watch the screen layout": off by default, stored with the device in the hub's
  Keychain JSON, footnote "Shows this device what Jarvis is doing: outlines of the screen,
  where it taps, and step captions. Never pictures of the screen, passwords or the lock
  screen.", a "Watching now" line while it watches), the core's never-remote list, rename,
  remove, lockout, audit log (watch start/stop/refusal and the permission's changes in
  words). App Intents: `AskJarvisIntent`, `RunJarvisWorkflowIntent` (`openAppWhenRun = false`);
  both wait up to 25 s for the outcome with `value(of:within:)` (JarvisKit, tested: returns
  `nil` at the limit without cancelling the task) and otherwise answer that it is still
  running; an automation is answered with the rules problem or "cannot read its workflows
  until the phone has been unlocked once" (`WorkflowLibrary.externalUnavailable`) rather than
  "there is no workflow". Adapters in `Platform/`: `KeychainSecretStore`
  (`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, not synchronizable),
  `SecureRandomSource` (`SecRandomCopyBytes`), `NetworkRemoteTransport` (`NWListener`, TCP,
  Bonjour `_jarvis-remote._tcp`, no cellular interface; Info.plist
  `NSLocalNetworkUsageDescription` + `NSBonjourServices`). Its rules live in JarvisKit
  (`Remote/RemoteTransportPolicy.swift`, tested): a peer is judged when its connection is
  ready by `PeerAddressPolicy` (loopback, link-local, private IPv4, unique-local IPv6, global
  IPv6 only on a path that is not cellular; IPv4-mapped follows IPv4; nothing on a cellular
  path); a connection not ready within 10 s is cancelled, a failed one is cancelled;
  received-but-unconsumed bytes (`RemoteTransport.consumed`) and sent-but-unprocessed bytes
  are each bounded per connection by a `ByteBudget` (`RemoteTransportLimits`, from the
  core's `MAX_BUFFERED_BYTES`) and the connection is closed past it; a failed listener is
  cancelled and rebuilt after `ListenerRestartPolicy`'s delay (1 s doubling to 60 s, reset
  after 30 s of health), and `RemoteControlService.ensureListening()` (called when the app
  becomes active) restarts a transport whose status is failed. Notifications
  (`UnlockNotifier`, `UserNotificationWorkflowNotifier`) never ask for permission: `AppModel`
  asks in the foreground when the owner turns on remote control, saves a workflow with a
  Notify step or starts a task, reads the state when the app becomes active
  (`notificationAccess`), and Control shows one line when notifications are denied. In playground mode the
  Playground tab replaces Workflows (a sixth tab would go behind "More"); Workflows is then a
  row in Control. Also: the run view (what Jarvis sees, step timeline, Stop, "Using skill"),
  confirmation and question sheets (`HumanPromptBroker`; the confirmation sheet shows the
  approval preview above the question), Away mode (explanation, then a black
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
  `device_info`, which also reads the lock state *before* each command (not waits): model
  inference sits between the observation and the command, so when the phone locked meanwhile
  the command is not sent and the result is the runner's locked refusal
  (`RunnerResult.deviceLocked`); the core waits for the unlock, observes and decides again.
  `.unknown` sends the command as before. `ScriptedModel` decides from the prompt's "Steps so far" whether a tap/type
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
