# JarvisKit: the Swift shell (non-UI)

JarvisKit is a SwiftPM package that does all the IO for the Rust core. The core is a sans-IO
state machine (`JarvisSession.step(nowMs:event:) -> Effect`, see `docs/ARCHITECTURE.md`).
JarvisKit runs the loop effect → IO → event, and the iOS app target (written separately)
depends on it for the runner client, the model client, the stores and the tools.

```
Shell/
├─ Package.swift
├─ scripts/bootstrap.sh          builds the Rust library, generates the bindings
├─ Sources/
│  ├─ JarvisCoreMissing/         stand-in whose #error says "run bootstrap.sh"
│  ├─ JarvisCore/                generated JarvisCore.swift          (gitignored)
│  └─ JarvisKit/
│     ├─ Driver/                 SessionDriver, IORace, DriverUpdate, DriverConfiguration,
│     │                          HumanGate, MonotonicClock
│     ├─ Runner/                 PhoneRunner, LoopCtrlRunner, LoopCtrlAPI (every LoopCtrl
│     │                          string), LoopCtrlRPCClient, LoopCtrlError, RequestSealer
│     ├─ Model/                  ModelProvider + ModelCapabilities, OpenAICompatibleModel,
│     │                          ModelError, ScreenshotAnnotator
│     ├─ Store/                  MemoryPersisting, FileMemoryStore, ScreenshotStore
│     ├─ Tools/                  ToolHost, ToolRegistry, ToolSpecs, SampleTools
│     └─ Support/                JSONValue, HTTPTransport, SessionConfigBuilder
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
| `PhoneRunner` | `snapshot(includeScreenshot:) -> RunnerObservation`, `perform(RunnerCommand) -> RunnerResult` | `LoopCtrlRunner` |
| `ModelProvider` | `capabilities: ModelCapabilities`, `generate(ModelRequest) -> String` | `OpenAICompatibleModel` |
| `ToolHost` | `toolSpecs()`, `execute(ToolCall) -> ToolResult` (non-throwing) | `ToolRegistry` (+ `SampleTools.clock_now`, `echo`) |
| `HumanGate` | `confirm(ConfirmRequest) -> Bool`, `answer(String) -> String` | app |
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

`SessionConfigBuilder.make(capabilities:tools:knownApps:adjustPolicy:)` starts from
`defaultSessionConfig()` and mutates it. JarvisKit never calls the memberwise initialiser
of `Policy` or `SessionConfig`, so new core fields keep their defaults. `RunnerSnapshot`,
`ToolSpec` and `ToolParam` have no core defaults. Each is built in exactly one place
(`RunnerObservation.runnerSnapshot`, `ToolSpecs`), so a new field means one edit.

## SessionDriver

`SessionDriver` is an actor. `init(config:task:memoryJSON:dependencies:configuration:)`
throws when the core rejects the memory. In that case call `FileMemoryStore.quarantine()`
and retry with `""`. `run() async -> RunResult` never throws. `cancel()` stops the run, and
`nonisolated updates: AsyncStream<DriverUpdate>` reports progress.

| Effect | IO | Event on success | Event on error / timeout |
|---|---|---|---|
| `observe` | `runner.snapshot` | `observedRunnerText` (screenshot put in `ScreenshotStore`, handle passed) | retried `observeAttempts` times. Then `cancel`, `endReason = .runnerUnavailable` (the core has no "observe failed" event) |
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
15. **Tree text**: parsed in Rust (`parseRunnerText`), not here. Labels containing quotes,
    commas or braces are untested against real output.

Also untested: Apple `URLSession` behaviour (only the Linux FoundationNetworking path ran),
the `completeFileProtectionUntilFirstUserAuthentication` write option, and the renamed
xcframework as a SwiftPM binary target. Clang should find its module map in the
`Headers/JarvisCoreFFI/` subdirectory, but that is unconfirmed.

## Notes for the core

- In the generated bindings, `JarvisError` is `internal`, not `public`. Swift callers can
  only catch it as `any Error`. This is why `SessionDriver.init` documents "throws when the
  memory is rejected" without naming a type.
- There is no event for "observation failed". The driver retries, then cancels the session
  with `endReason = .runnerUnavailable`, so the core logs that run as Cancelled. A
  `ObserveFailed { error }` event would let the core count it as a failure instead.
- Set-of-marks drawing is the shell's job (`ScreenshotAnnotator`). No annotator that draws
  exists yet.
